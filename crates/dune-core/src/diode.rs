//! Junction diode, the 3f5 model (`dev/dio/dioload.c`, `diotemp.c`,
//! `diosetup.c`, `dioconv.c`, `diotrunc.c`, `devsup.c`).
//!
//! DC current (`dioload.c`, with `vte = N·k·T/q`):
//!
//! - `vd >= -3·vte`:        `cd = csat·(exp(vd/vte) - 1) + gmin·vd`
//! - no/pre-breakdown:      `cd = -csat·(1 + (3·vte/(vd·e))³) + gmin·vd`
//! - below breakdown:       `cd = -csat·exp(-(bv+vd)/vte) + gmin·vd`
//!
//! Depletion + diffusion charge (transient/AC):
//!
//! - `vd < FC·VJt`: `qd = TT·cd + VJ·cz·(1-arg·sarg)/(1-M)`,
//!   `capd = TT·gd + cz·sarg`, `sarg = (1-vd/VJ)^-M`
//! - else the `f2`/`f3` linearized form of `dioload.c`.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::matrix::{AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};

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
    /// `TT`: transit time, default 0.
    pub tt: f64,
    /// `CJO`: zero-bias junction capacitance, default 0.
    pub cjo: f64,
    /// `VJ`: junction potential, default 1 V.
    pub vj: f64,
    /// `M`: grading coefficient, default 0.5 (clamped to 0.9).
    pub m: f64,
    /// `EG`: activation energy, default 1.11 eV (clamped to ≥ 0.1).
    pub eg: f64,
    /// `XTI`: saturation-current temperature exponent, default 3.
    pub xti: f64,
    /// `KF`/`AF`: flicker noise (noise analysis only).
    pub kf: f64,
    pub af: f64,
    /// `FC`: forward-bias depletion coefficient, default 0.5
    /// (clamped to 0.95).
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

/// Temperature-adjusted per-instance values (`DIOtemperature`), plus
/// the model-level `f2`/`f3` of `diosetup.c`.
pub(crate) struct DiodeTemp {
    pub csat: f64,
    pub gspr: f64,
    pub vte: f64,
    pub vcrit: f64,
    pub brkdwn: Option<f64>,
    /// `DIOtJctCap·area` is applied at load; store per-area value.
    pub t_jct_cap: f64,
    pub t_dep_cap: f64,
    pub t_f1: f64,
    pub f2: f64,
    pub f3: f64,
    /// Model values after the setup clamps.
    pub m: f64,
    pub vj: f64,
    pub tt: f64,
    /// Messages for the caller to surface as warnings.
    pub notes: Vec<String>,
}

/// `egfet(T)` as `diotemp.c` computes it.
fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

impl DiodeTemp {
    /// `DIOtemperature` (`diotemp.c`) + the `diosetup.c` clamps.
    pub fn new(
        name: &str,
        model: &DiodeModel,
        area: f64,
        temp: f64,
        default_tnom: f64,
        reltol: f64,
    ) -> Self {
        let mut notes = Vec::new();
        let mut m = model.m;
        if m > 0.9 {
            notes.push(format!(
                "{name}: grading coefficient too large, limited to 0.9"
            ));
            m = 0.9;
        }
        let mut eg = model.eg;
        if eg < 0.1 {
            notes.push(format!(
                "{name}: activation energy too small, limited to 0.1"
            ));
            eg = 0.1;
        }
        let mut fc = model.fc;
        if fc > 0.95 {
            notes.push(format!("{name}: coefficient Fc too large, limited to 0.95"));
            fc = 0.95;
        }
        let xfc = f64::ln(1.0 - fc);
        let f2 = f64::exp((1.0 + m) * xfc);
        let f3 = 1.0 - fc * (1.0 + m);

        let tnom = model.tnom.unwrap_or(default_tnom);
        let vt = KOVERQ * temp;
        let vtnom = KOVERQ * tnom;

        // Junction potential and capacitance temperature adjustment
        // ("this part gets really ugly").
        let fact2 = temp / REFTEMP;
        let eg_t = egfet(temp);
        let arg = -eg_t / (2.0 * BOLTZ * temp) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);
        let eg_nom = egfet(tnom);
        let arg1 = -eg_nom / (BOLTZ * 2.0 * tnom) + 1.1150877 / (2.0 * BOLTZ * REFTEMP);
        let fact1 = tnom / REFTEMP;
        let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);
        let pbo = (model.vj - pbfact1) / fact1;
        let gmaold = (model.vj - pbo) / pbo;
        let mut t_jct_cap = model.cjo / (1.0 + m * (400e-6 * (tnom - REFTEMP) - gmaold));
        let t_jct_pot = pbfact + fact2 * pbo;
        let gmanew = (t_jct_pot - pbo) / pbo;
        t_jct_cap *= 1.0 + m * (400e-6 * (temp - REFTEMP) - gmanew);

        let t_sat_cur = model.is
            * f64::exp(
                ((temp / tnom) - 1.0) * eg / (model.n * vt)
                    + model.xti / model.n * f64::ln(temp / tnom),
            );
        let t_f1 = t_jct_pot * (1.0 - f64::exp((1.0 - m) * xfc)) / (1.0 - m);
        let t_dep_cap = fc * t_jct_pot;
        let vte = model.n * vt;
        let vcrit = vte * f64::ln(vte / (ROOT2 * t_sat_cur));
        let gspr = if model.rs == 0.0 { 0.0 } else { 1.0 / model.rs } * area;

        // Match the exponential and linear breakdown regions.
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
            t_jct_cap,
            t_dep_cap,
            t_f1,
            f2,
            f3,
            m,
            vj: model.vj,
            tt: model.tt,
            notes,
        }
    }
}

pub(crate) struct DiodeI {
    pos: Var,
    neg: Var,
    /// Internal node past RS; equals `pos` when RS is zero.
    pos_prime: Var,
    t: DiodeTemp,
    area: f64,
    off: bool,
    /// `IC=` initial junction voltage, for UIC.
    init_cond: f64,
    /// State offsets: voltage, current, conductance, capCharge,
    /// capCurrent (`DIOvoltage` …, `diosetup.c`).
    st: usize,
    slots: [Slot; 7],
}

const VOLTAGE: usize = 0;
const CURRENT: usize = 1;
const CONDUCT: usize = 2;
const CAP_CHARGE: usize = 3;

impl DiodeI {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pos: Var,
        neg: Var,
        pos_prime: Var,
        t: DiodeTemp,
        area: f64,
        off: bool,
        init_cond: f64,
        st: usize,
    ) -> Self {
        DiodeI {
            pos,
            neg,
            pos_prime,
            t,
            area,
            off,
            init_cond,
            st,
            slots: [TRASH; 7],
        }
    }
}

impl crate::devices::Device for DiodeI {
    /// `DIOconvTest`: predicted vs computed junction current,
    /// evaluated at the previous solution (`CKTrhsOld`).
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let vd = ctx.sol[self.pos_prime] - ctx.sol[self.neg];
        let delvd = vd - ctx.states.get(0, self.st + VOLTAGE);
        let cd = ctx.states.get(0, self.st + CURRENT);
        let cdhat = cd + ctx.states.get(0, self.st + CONDUCT) * delvd;
        let tol = ctx.reltol * f64::max(cdhat.abs(), cd.abs()) + ctx.abstol;
        (cdhat - cd).abs() <= tol
    }

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

    fn load(&mut self, ctx: &mut LoadCtx) {
        let vte = self.t.vte;
        let csat = self.t.csat;
        let st = self.st;
        let mut check = true;

        let vd = match ctx.mode {
            InitMode::Smsig => ctx.states.get(0, st + VOLTAGE),
            InitMode::Tran => ctx.states.get(1, st + VOLTAGE),
            InitMode::Jct if ctx.kind == AnKind::TranOp && ctx.uic => self.init_cond,
            InitMode::Jct if self.off => 0.0,
            InitMode::Jct => self.t.vcrit,
            InitMode::Fix if self.off => 0.0,
            InitMode::Pred => ctx.pred(st + VOLTAGE),
            _ => {
                let mut vd = ctx.sol[self.pos_prime] - ctx.sol[self.neg];
                // Per-iteration junction limiting (`DEVpnjlim`, with
                // the separate breakdown-region branch).
                let vd_old = ctx.states.get(0, st + VOLTAGE);
                match self.t.brkdwn {
                    Some(bv) if vd < f64::min(0.0, -bv + 10.0 * vte) => {
                        let vdtemp =
                            pnjlim(-(vd + bv), -(vd_old + bv), vte, self.t.vcrit, &mut check);
                        vd = -(vdtemp + bv);
                    }
                    _ => {
                        vd = pnjlim(vd, vd_old, vte, self.t.vcrit, &mut check);
                    }
                }
                vd
            }
        };

        // DC current and conductance by region.
        let (cd0, gd0);
        if vd >= -3.0 * vte {
            let evd = f64::exp(vd / vte);
            cd0 = csat * (evd - 1.0) + ctx.gmin * vd;
            gd0 = csat * evd / vte + ctx.gmin;
        } else if self.t.brkdwn.is_none() || vd >= -self.t.brkdwn.unwrap_or(0.0) {
            let arg = 3.0 * vte / (vd * crate::consts::E);
            let arg = arg * arg * arg;
            cd0 = -csat * (1.0 + arg) + ctx.gmin * vd;
            gd0 = csat * 3.0 * arg / vd + ctx.gmin;
        } else {
            let bv = self.t.brkdwn.unwrap_or(0.0);
            let evrev = f64::exp(-(bv + vd) / vte);
            cd0 = -csat * evrev + ctx.gmin * vd;
            gd0 = csat * evrev / vte + ctx.gmin;
        }
        let mut cd = cd0;
        let mut gd = gd0;

        // Charge storage (transient, UIC transient op, and the
        // small-signal pass before `.ac`).
        if ctx.kind == AnKind::Tran
            || (ctx.kind == AnKind::TranOp && ctx.uic)
            || ctx.mode == InitMode::Smsig
        {
            let czero = self.t.t_jct_cap * self.area;
            #[allow(clippy::needless_late_init)] // mirrors dioload.c
            let capd;
            if vd < self.t.t_dep_cap {
                let arg = 1.0 - vd / self.t.vj;
                let sarg = f64::exp(-self.t.m * f64::ln(arg));
                ctx.states.set(
                    0,
                    st + CAP_CHARGE,
                    self.t.tt * cd + self.t.vj * czero * (1.0 - arg * sarg) / (1.0 - self.t.m),
                );
                capd = self.t.tt * gd + czero * sarg;
            } else {
                let czof2 = czero / self.t.f2;
                ctx.states.set(
                    0,
                    st + CAP_CHARGE,
                    self.t.tt * cd
                        + czero * self.t.t_f1
                        + czof2
                            * (self.t.f3 * (vd - self.t.t_dep_cap)
                                + (self.t.m / (self.t.vj + self.t.vj))
                                    * (vd * vd - self.t.t_dep_cap * self.t.t_dep_cap)),
                );
                capd = self.t.tt * gd + czof2 * (self.t.f3 + self.t.m * vd / self.t.vj);
            }

            // `MODEINITSMSIG`: store the small-signal capacitance
            // into the capCurrent slot and stop — no stamps
            // (`dioload.c` does `continue` here; `DIOacLoad` reads
            // it back as `xceq = capd·ω`).
            if ctx.mode == InitMode::Smsig {
                ctx.states.set(0, st + CAP_CHARGE + 1, capd);
                return;
            }
            if ctx.kind == AnKind::Tran {
                if ctx.mode == InitMode::Tran {
                    let q0 = ctx.states.get(0, st + CAP_CHARGE);
                    ctx.states.set(1, st + CAP_CHARGE, q0);
                }
                let (geq, ceq_int) = ctx.integrate(capd, st + CAP_CHARGE);
                let _ = ceq_int;
                gd += geq;
                cd += ctx.states.get(0, st + CAP_CHARGE + 1);
                if ctx.mode == InitMode::Tran {
                    let c0 = ctx.states.get(0, st + CAP_CHARGE + 1);
                    ctx.states.set(1, st + CAP_CHARGE + 1, c0);
                }
            }
        }

        // Convergence: limiting counts as non-convergence (the
        // current-agreement test lives in `DIOconvTest` under
        // NEWCONV and only fires with device bypass, which Dune does
        // not implement — see docs/dune-core.md).
        if !(ctx.mode == InitMode::Fix && self.off) && check {
            ctx.noncon = true;
        }

        ctx.states.set(0, st + VOLTAGE, vd);
        ctx.states.set(0, st + CURRENT, cd);
        ctx.states.set(0, st + CONDUCT, gd);

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

    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        // `DIOtrunc`: `CKTterr` on the junction charge.
        crate::integ::terr(self.st + CAP_CHARGE, t, time_step);
    }

    /// `DIOacLoad`: `gd` from the operating point plus `jω·capd`.
    fn load_ac(&mut self, ctx: &mut crate::matrix::AcCtx) {
        let gspr = self.t.gspr;
        let geq = ctx.states.get(0, self.st + CONDUCT);
        let xceq = ctx.states.get(0, self.st + CAP_CHARGE + 1) * ctx.omega;
        ctx.add_g(self.slots[0], gspr);
        ctx.add_g(self.slots[1], geq);
        ctx.add_b(self.slots[1], xceq);
        ctx.add_g(self.slots[2], geq + gspr);
        ctx.add_b(self.slots[2], xceq);
        ctx.add_g(self.slots[3], -gspr);
        ctx.add_g(self.slots[4], -geq);
        ctx.add_b(self.slots[4], -xceq);
        ctx.add_g(self.slots[5], -gspr);
        ctx.add_g(self.slots[6], -geq);
        ctx.add_b(self.slots[6], -xceq);
    }
}

/// `DEVpnjlim` (`dev/devsup.c`).
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
        let v = pnjlim(0.5, 0.49, vt, vcrit, &mut check);
        assert_eq!(v, 0.5);
        assert!(!check);
        let v = pnjlim(5.0, 0.6, vt, vcrit, &mut check);
        assert!(check);
        assert!((v - (0.6 + vt * (1.0f64 + (5.0 - 0.6) / vt).ln())).abs() < 1e-15);
        let v = pnjlim(5.0, 0.0, vt, vcrit, &mut check);
        assert!(check);
        assert!((v - vt * (5.0f64 / vt).ln()).abs() < 1e-15);
    }

    #[test]
    fn temp_adjust_is_identity_at_reftemp() {
        let m = DiodeModel::default();
        let t = DiodeTemp::new("d1", &m, 1.0, 300.15, 300.15, 1e-3);
        assert_eq!(t.csat, m.is);
        let vte = m.n * KOVERQ * 300.15;
        assert!((t.vcrit - vte * (vte / (ROOT2 * m.is)).ln()).abs() < 1e-15);
        // At T == Tnom == REFTEMP the junction-potential adjustment
        // collapses to the model values.
        assert!((t.t_jct_cap - m.cjo).abs() < 1e-12);
        assert!((t.t_dep_cap - m.fc * m.vj).abs() < 1e-12);
        assert!(t.notes.is_empty());
    }

    #[test]
    fn setup_clamps_warn() {
        let m = DiodeModel {
            m: 0.95,
            eg: 0.05,
            fc: 0.99,
            ..DiodeModel::default()
        };
        let t = DiodeTemp::new("dx", &m, 1.0, 300.15, 300.15, 1e-3);
        assert_eq!(t.notes.len(), 3);
        assert_eq!(t.m, 0.9);
        assert!((t.f3 - (1.0 - 0.95 * 1.9)).abs() < 1e-12);
    }
}
