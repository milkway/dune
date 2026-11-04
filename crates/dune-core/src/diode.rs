//! Junction diode, the 3f5 model in full DC fidelity
//! (`dev/dio/dioload.c`, `diotemp.c`, `devsup.c`). Capacitance and
//! noise come with the transient/AC phases.
//!
//! DC current (`dioload.c`, with `vte = N·k·T/q`):
//!
//! - `vd >= -3·vte`:        `cd = csat·(exp(vd/vte) - 1) + gmin·vd`
//! - no/pre-breakdown:      `cd = -csat·(1 + (3·vte/(vd·e))³) + gmin·vd`
//! - below breakdown:       `cd = -csat·exp(-(bv+vd)/vte) + gmin·vd`

use crate::consts::{KOVERQ, ROOT2};
use crate::matrix::{InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};

/// `.model … d(…)` parameters, 3f5 names and defaults
/// (`spice.txt` §Junction Diodes; `diompar.c`).
#[derive(Clone, Debug)]
pub struct DiodeModel {
    /// `IS`: saturation current, default 1e-14 A.
    pub is: f64,
    /// `RS`: ohmic resistance, default 0.
    pub rs: f64,
    /// `N`: emission coefficient, default 1.
    pub n: f64,
    /// `TT`: transit time, default 0 (transient only).
    pub tt: f64,
    /// `CJO`: zero-bias junction capacitance, default 0.
    pub cjo: f64,
    /// `VJ`: junction potential, default 1 V.
    pub vj: f64,
    /// `M`: grading coefficient, default 0.5.
    pub m: f64,
    /// `EG`: activation energy, default 1.11 eV.
    pub eg: f64,
    /// `XTI`: saturation-current temperature exponent, default 3.
    pub xti: f64,
    /// `KF`/`AF`: flicker noise (noise analysis only).
    pub kf: f64,
    pub af: f64,
    /// `FC`: forward-bias depletion coefficient, default 0.5.
    pub fc: f64,
    /// `BV`: reverse breakdown voltage (positive number), unset by
    /// default.
    pub bv: Option<f64>,
    /// `IBV`: current at breakdown voltage, default 1e-3 A.
    pub ibv: f64,
    /// `TNOM` override in kelvin (card value is °C).
    pub tnom: Option<f64>,
}

impl Default for DiodeModel {
    fn default() -> Self {
        DiodeModel {
            is: 1e-14,
            rs: 0.0,
            n: 1.0,
            tt: 0.0,
            cjo: 0.0,
            vj: 1.0,
            m: 0.5,
            eg: 1.11,
            xti: 3.0,
            kf: 0.0,
            af: 1.0,
            fc: 0.5,
            bv: None,
            ibv: 1e-3,
            tnom: None,
        }
    }
}

/// Temperature-adjusted per-instance values (`DIOtemp`).
pub(crate) struct DiodeTemp {
    pub csat: f64,
    pub gspr: f64,
    pub vte: f64,
    pub vcrit: f64,
    pub brkdwn: Option<f64>,
    /// Messages for the caller to surface as warnings.
    pub notes: Vec<String>,
}

impl DiodeTemp {
    /// `DIOtemperature` (`diotemp.c`), DC subset: saturation-current
    /// scaling, Vcrit and the matched breakdown voltage.
    pub fn new(
        name: &str,
        model: &DiodeModel,
        area: f64,
        temp: f64,
        default_tnom: f64,
        reltol: f64,
    ) -> Self {
        let mut notes = Vec::new();
        let tnom = model.tnom.unwrap_or(default_tnom);
        let vt = KOVERQ * temp;
        // tSatCur = IS * exp(((T/Tnom)-1)*EG/(N*vt) + XTI/N*ln(T/Tnom))
        let t_sat_cur = model.is
            * f64::exp(
                ((temp / tnom) - 1.0) * model.eg / (model.n * vt)
                    + model.xti / model.n * f64::ln(temp / tnom),
            );
        let vte = model.n * vt;
        let vcrit = vte * f64::ln(vte / (ROOT2 * t_sat_cur));
        let gspr = if model.rs == 0.0 { 0.0 } else { 1.0 / model.rs } * area;

        // Match the exponential and linear breakdown regions
        // (`diotemp.c`): find xbv with
        // csat*(exp((BV-xbv)/vt) - 1 + xbv/vt) == ibv.
        let brkdwn = model.bv.map(|bv| {
            let mut cbv = model.ibv;
            if cbv < t_sat_cur * bv / vt {
                cbv = t_sat_cur * bv / vt;
                notes.push(format!(
                    "{name}: breakdown current increased to {cbv:.3e} to resolve \
                     incompatibility with specified saturation current"
                ));
                bv
            } else {
                let tol = reltol * cbv;
                let mut xbv = bv - vt * f64::ln(1.0 + cbv / t_sat_cur);
                let mut matched = false;
                for _ in 0..25 {
                    xbv = bv - vt * f64::ln(cbv / t_sat_cur + 1.0 - xbv / vt);
                    let xcbv = t_sat_cur * (f64::exp((bv - xbv) / vt) - 1.0 + xbv / vt);
                    if (xcbv - cbv).abs() <= tol {
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    notes.push(format!(
                        "{name}: unable to match forward and reverse diode regions: \
                         bv = {xbv:.5e}, ibv = {cbv:.3e}"
                    ));
                }
                xbv
            }
        });

        DiodeTemp {
            csat: t_sat_cur * area,
            gspr,
            vte,
            vcrit,
            brkdwn,
            notes,
        }
    }
}

pub(crate) struct DiodeI {
    pos: Var,
    neg: Var,
    /// Internal node past RS; equals `pos` when RS is zero
    /// (`DIOposPrimeNode`).
    pos_prime: Var,
    t: DiodeTemp,
    off: bool,
    // `CKTstate0`: voltage, current, conductance of the previous
    // iteration.
    vd: f64,
    cd: f64,
    gd: f64,
    slots: [Slot; 7],
}

impl DiodeI {
    pub fn new(pos: Var, neg: Var, pos_prime: Var, t: DiodeTemp, off: bool) -> Self {
        DiodeI {
            pos,
            neg,
            pos_prime,
            t,
            off,
            vd: 0.0,
            cd: 0.0,
            gd: 0.0,
            slots: [TRASH; 7],
        }
    }
}

impl crate::devices::Device for DiodeI {
    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.pos),
            mb.slot(self.neg, self.neg),
            mb.slot(self.pos_prime, self.pos_prime),
            mb.slot(self.pos, self.pos_prime),
            mb.slot(self.neg, self.pos_prime),
            mb.slot(self.pos_prime, self.pos),
            mb.slot(self.pos_prime, self.neg),
        ];
    }

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        let vte = self.t.vte;
        let csat = self.t.csat;
        let mut check = true;
        let mut cdhat = None;

        let vd = match ctx.mode {
            InitMode::Jct if self.off => 0.0,
            InitMode::Jct => self.t.vcrit,
            InitMode::Fix if self.off => 0.0,
            _ => {
                let mut vd = ctx.sol[self.pos_prime] - ctx.sol[self.neg];
                let delvd = vd - self.vd;
                cdhat = Some(self.cd + self.gd * delvd);
                // Per-iteration junction limiting (`DEVpnjlim`, with
                // the separate breakdown-region limiting of
                // `dioload.c`).
                match self.t.brkdwn {
                    Some(bv) if vd < f64::min(0.0, -bv + 10.0 * vte) => {
                        let vdtemp =
                            pnjlim(-(vd + bv), -(self.vd + bv), vte, self.t.vcrit, &mut check);
                        vd = -(vdtemp + bv);
                    }
                    _ => {
                        vd = pnjlim(vd, self.vd, vte, self.t.vcrit, &mut check);
                    }
                }
                vd
            }
        };

        // DC current and conductance by region.
        let (cd, gd);
        if vd >= -3.0 * vte {
            let evd = f64::exp(vd / vte);
            cd = csat * (evd - 1.0) + ctx.gmin * vd;
            gd = csat * evd / vte + ctx.gmin;
        } else if self.t.brkdwn.is_none() || vd >= -self.t.brkdwn.unwrap_or(0.0) {
            let arg = 3.0 * vte / (vd * crate::consts::E);
            let arg = arg * arg * arg;
            cd = -csat * (1.0 + arg) + ctx.gmin * vd;
            gd = csat * 3.0 * arg / vd + ctx.gmin;
        } else {
            let bv = self.t.brkdwn.unwrap_or(0.0);
            let evrev = f64::exp(-(bv + vd) / vte);
            cd = -csat * evrev + ctx.gmin * vd;
            gd = csat * evrev / vte + ctx.gmin;
        }

        // Convergence check (`dioload.c`: limiting counts as
        // non-convergence; otherwise predicted vs actual current).
        if !(ctx.mode == InitMode::Fix && self.off) {
            if check {
                ctx.noncon = true;
            } else if let Some(cdhat) = cdhat {
                let tol = ctx.reltol * f64::max(cdhat.abs(), cd.abs()) + ctx.abstol;
                if (cdhat - cd).abs() > tol {
                    ctx.noncon = true;
                }
            }
        }

        self.vd = vd;
        self.cd = cd;
        self.gd = gd;

        let cdeq = cd - gd * vd;
        ctx.rhs[self.neg] += cdeq;
        ctx.rhs[self.pos_prime] -= cdeq;

        let gspr = self.t.gspr;
        ctx.add(self.slots[0], gspr);
        ctx.add(self.slots[1], gd);
        ctx.add(self.slots[2], gd + gspr);
        ctx.add(self.slots[3], -gspr);
        ctx.add(self.slots[4], -gd);
        ctx.add(self.slots[5], -gspr);
        ctx.add(self.slots[6], -gd);
    }
}

/// `DEVpnjlim` (`dev/devsup.c`): limit the per-iteration change of a
/// PN junction voltage; sets `check` when it limited.
pub(crate) fn pnjlim(vnew: f64, vold: f64, vt: f64, vcrit: f64, check: &mut bool) -> f64 {
    if vnew > vcrit && (vnew - vold).abs() > vt + vt {
        *check = true;
        if vold > 0.0 {
            let arg = 1.0 + (vnew - vold) / vt;
            if arg > 0.0 {
                vold + vt * f64::ln(arg)
            } else {
                vcrit
            }
        } else {
            vt * f64::ln(vnew / vt)
        }
    } else {
        *check = false;
        vnew
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pnjlim_matches_the_c() {
        let vt = 0.025852;
        let vcrit = 0.6145;
        let mut check = false;
        // Small steps pass through untouched.
        let v = pnjlim(0.5, 0.49, vt, vcrit, &mut check);
        assert_eq!(v, 0.5);
        assert!(!check);
        // A huge jump from a positive voltage is log-limited.
        let v = pnjlim(5.0, 0.6, vt, vcrit, &mut check);
        assert!(check);
        assert!((v - (0.6 + vt * (1.0f64 + (5.0 - 0.6) / vt).ln())).abs() < 1e-15);
        // From a non-positive voltage: vt*ln(vnew/vt).
        let v = pnjlim(5.0, 0.0, vt, vcrit, &mut check);
        assert!(check);
        assert!((v - vt * (5.0f64 / vt).ln()).abs() < 1e-15);
    }

    #[test]
    fn temp_adjust_is_identity_at_tnom() {
        let m = DiodeModel::default();
        let t = DiodeTemp::new("d1", &m, 1.0, 300.15, 300.15, 1e-3);
        assert_eq!(t.csat, m.is);
        let vte = m.n * KOVERQ * 300.15;
        assert!((t.vcrit - vte * (vte / (ROOT2 * m.is)).ln()).abs() < 1e-15);
        assert!(t.notes.is_empty());
    }
}
