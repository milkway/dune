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
use crate::disto::{
    DistoCtx, DistoKernels, DistoMode, DistoSetupCtx, d1i2f1, d1i2f12, d1i3f1, d1if12, d1n2f1,
    d1n2f12, d1n3f1, d1nf12,
};
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
    /// `vt = CONSTKoverQ·T` (`DIOdSetup` uses it in breakdown).
    pub vt: f64,
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
    /// `area` is the instance AREA and `mult` the ngspice multiplier
    /// `m` (1 on cards): both scale the saturation current and the
    /// series conductance, but only `area` enters the breakdown match
    /// (`IBV` is absolute, `M·IBV` in ngspice).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        model: &DiodeModel,
        area: f64,
        mult: f64,
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
        let gspr = if model.rs == 0.0 { 0.0 } else { 1.0 / model.rs } * area * mult;

        // Match the exponential and linear breakdown regions: find
        // `xbv` with `IS·(exp((BV−xbv)/(N·vt)) − 1 + xbv/vt) = IBV`, so
        // that the breakdown branch of the load,
        // `cd = −csat·exp(−(xbv+vd)/(N·vt))`, carries IBV at `vd = −BV`.
        // Two corrections of `diotemp.c` that ngspice-47 carries
        // (corpus/KNOWN_DIFFS.md, gates in `regress/diobv_*`):
        // - the emission coefficient: the 3f5 matches with plain `vt`
        //   while its load uses `N·vt`, so for `N ≠ 1` the current at
        //   −BV is `IS·(IBV/IS)^(1/N)`, not IBV; ngspice's `NBV·vt`
        //   (`NBV` defaults to `N`, `diosetup.c`) is used here.
        // - the area: the 3f5 matches the per-area `DIOtSatCur` against
        //   the absolute IBV, so the current at −BV is `AREA·IBV`
        //   (`spice.txt` scales only IS/RS/CJO); ngspice matches
        //   `IS·AREA·M` against `M·IBV`.
        let brkdwn = model.bv.map(|bv| {
            let mut cbv = model.ibv;
            let sat = t_sat_cur * area;
            if cbv < sat * bv / vt {
                cbv = sat * bv / vt;
                notes.push(format!(
                    "{name}: breakdown current increased to {cbv:.3e} to resolve \
                     incompatibility with specified saturation current"
                ));
                bv
            } else {
                let tol = reltol * cbv;
                let vtebrk = model.n * vt;
                let mut xbv = bv - vtebrk * f64::ln(1.0 + cbv / sat);
                let mut matched = false;
                for _ in 0..25 {
                    xbv = bv - vtebrk * f64::ln(cbv / sat + 1.0 - xbv / vt);
                    let xcbv = sat * (f64::exp((bv - xbv) / vtebrk) - 1.0 + xbv / vt);
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
            csat: t_sat_cur * area * mult,
            gspr,
            vt,
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
    /// `DIOdCoeffs` (`diodefs.h`): `id_x2, id_x3, cdif_x2, cdif_x3,
    /// cjnc_x2, cjnc_x3`, set by `DIOdSetup`.
    d: DioDistoCoeffs,
    /// What `DIOtemp` is recomputed from when `.sens` perturbs a
    /// parameter: the model, the instance temperature (K), the
    /// ngspice multiplier `m` (1 on cards; it scales like the area
    /// except in the breakdown match, where `M·IBV` cancels it)
    /// and the circuit `TNOM`/`RELTOL`.
    sens: DiodeSensBase,
}

/// The inputs of [`DiodeTemp::new`] a sensitivity perturbation can
/// move.
pub(crate) struct DiodeSensBase {
    pub name: String,
    pub model: DiodeModel,
    pub temp: f64,
    pub m: f64,
    pub default_tnom: f64,
    pub reltol: f64,
}

/// The diode parameters `.sens` walks: ngspice-47's `DIOmPTable`
/// order restricted to the entries the 3f5 model has (the AC-only
/// `tt`/`cjo` only in an AC walk), then the whole `DIOpTable` walk.
/// ngspice's own model extensions (`jsw`, `ikf`, `isr`, `jtun`, …)
/// are not listed: several only switch on once *given*, so their
/// ngspice sensitivities are artifacts of the perturbation itself
/// (`corpus/KNOWN_DIFFS.md`). Of the instance entries only `temp`,
/// `area` and `m` act on Dune's equations; `dtemp` is listed inert
/// because it is inert in the C too — `temp` comes first in the
/// table and perturbing it marks it given, after which `DIOtemp`
/// ignores `dtemp` — and `pj`/`w`/`l`/`lm`/`lp`/`wm`/`wp` act only
/// through the sidewall/level-3 terms the 3f5 model lacks (zero at
/// their defaults).
const DIO_SENS_DC: &[&str] = &[
    ":is", ":rs", ":n", ":vj", ":m", ":eg", ":xti", ":kf", ":af", ":fc", "_temp", "_dtemp",
    "_area", "_pj", "_w", "_l", "_m", "_lm", "_lp", "_wm", "_wp",
];
const DIO_SENS_AC: &[&str] = &[
    ":is", ":rs", ":n", ":tt", ":cjo", ":vj", ":m", ":eg", ":xti", ":kf", ":af", ":fc", "_temp",
    "_dtemp", "_ic", "_area", "_pj", "_w", "_l", "_m", "_lm", "_lp", "_wm", "_wp",
];

/// Taylor coefficients of the diode nonlinearities at the operating
/// point (`diodset.c`): conductance (`id_x*`), diffusion (`cdif_x*`)
/// and junction (`cjnc_x*`) capacitance, 2nd and 3rd order.
#[derive(Clone, Copy, Debug, Default)]
struct DioDistoCoeffs {
    id_x2: f64,
    id_x3: f64,
    cdif_x2: f64,
    cdif_x3: f64,
    cjnc_x2: f64,
    cjnc_x3: f64,
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
        sens: DiodeSensBase,
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
            d: DioDistoCoeffs::default(),
            sens,
        }
    }

    /// The current value of a sensitivity parameter.
    fn sens_value(&self, suffix: &str) -> f64 {
        let m = &self.sens.model;
        match suffix {
            ":is" => m.is,
            ":rs" => m.rs,
            ":n" => m.n,
            ":tt" => m.tt,
            ":cjo" => m.cjo,
            ":vj" => m.vj,
            ":m" => m.m,
            ":eg" => m.eg,
            ":xti" => m.xti,
            ":kf" => m.kf,
            ":af" => m.af,
            ":fc" => m.fc,
            // `DIOask(DIO_TEMP)` reports °C.
            "_temp" => self.sens.temp - crate::consts::CTOK,
            "_area" => self.area,
            "_m" => self.sens.m,
            "_ic" => self.init_cond,
            _ => 0.0,
        }
    }
}

impl crate::devices::Device for DiodeI {
    fn par_load(&self) -> bool {
        true
    }

    /// `DIOgetic` (`diogetic.c`): `if(!DIOinitCondGiven) DIOinitCond
    /// = rhs[pos] − rhs[neg]`. `DIO_IC` in `dioparam.c` stores the
    /// value without ever raising `DIOinitCondGiven` (3f5 and
    /// ngspice-47 alike), so under UIC the card's `IC=` is always
    /// replaced by the node values (0 without a `.ic`/`.nodeset`).
    fn set_ic(&mut self, rhs: &[f64]) {
        self.init_cond = rhs[self.pos] - rhs[self.neg];
    }

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
            _ => {
                let mut vd = if ctx.mode == InitMode::Pred {
                    // `dioload.c` (`#ifndef PREDICTOR`): carry the
                    // last timepoint's operating point into state0
                    // and extrapolate — and the extrapolation is then
                    // *limited* like any other iterate, against that
                    // carried voltage.
                    for o in [VOLTAGE, CURRENT, CONDUCT] {
                        let v = ctx.states.get(1, st + o);
                        ctx.states.set(0, st + o, v);
                    }
                    ctx.pred(st + VOLTAGE)
                } else {
                    ctx.sol[self.pos_prime] - ctx.sol[self.neg]
                };
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
        ctx.add_rhs(self.neg, cdeq);
        ctx.add_rhs(self.pos_prime, -cdeq);

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

    fn sens_params(&self, ac: bool) -> Option<Vec<crate::devices::SensParam>> {
        let list = if ac { DIO_SENS_AC } else { DIO_SENS_DC };
        Some(
            list.iter()
                .map(|&suffix| crate::devices::SensParam {
                    suffix,
                    value: self.sens_value(suffix),
                })
                .collect(),
        )
    }

    /// `DIOmParam`/`DIOparam` + `DIOtemp`. The junction capacitance
    /// and the stored small-signal values are not recomputed: the C
    /// sets the zero-bias capacitance in `DIOsetup` and the AC load
    /// reads `gd`/`capd` back from the state vector, so neither moves
    /// under a perturbation there either.
    fn sens_set(&mut self, suffix: &str, value: f64) {
        let b = &mut self.sens;
        match suffix {
            ":is" => b.model.is = value,
            // `DIOconductance = 1/RS` is set in `DIOsetup`, which the
            // walk runs *before* the perturbation: moving RS changes
            // nothing in the C, so it is inert here too.
            ":n" => b.model.n = value,
            ":tt" => b.model.tt = value,
            ":cjo" => b.model.cjo = value,
            ":vj" => b.model.vj = value,
            ":m" => b.model.m = value,
            ":eg" => b.model.eg = value,
            ":xti" => b.model.xti = value,
            ":kf" => b.model.kf = value,
            ":af" => b.model.af = value,
            ":fc" => b.model.fc = value,
            "_temp" => b.temp = value + crate::consts::CTOK,
            "_area" => self.area = value,
            "_m" => b.m = value,
            _ => return,
        }
        let b = &self.sens;
        let t = DiodeTemp::new(
            &b.name,
            &b.model,
            self.area,
            b.m,
            b.temp,
            b.default_tnom,
            b.reltol,
        );
        // Keep the setup-time capacitance/charge parameters.
        self.t.csat = t.csat;
        self.t.gspr = t.gspr;
        self.t.vt = t.vt;
        self.t.vte = t.vte;
        self.t.vcrit = t.vcrit;
        self.t.brkdwn = t.brkdwn;
    }

    /// `DIOnoise` (ngspice): thermal on RS, shot on the junction
    /// current, flicker `KF·|Id/m|^AF/f·m`, then ngspice's three
    /// sidewall generators — identically zero here (no `RSW` model,
    /// the vectors exist in the golden all the same) — and the total.
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(&["_rs", "_id", "_1overf", "_rsw", "_idsw", "_1overfsw", ""])
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        const N_MINLOG: f64 = 1e-38;
        let cd = ctx.states.get(0, self.st + CURRENT);
        let m = self.sens.m;
        // `dionoise.c` (ngspice-47): RS at the instance temperature.
        let rs = ctx.thermal_at(self.pos_prime, self.pos, self.t.gspr, self.sens.temp);
        let id = ctx.shot(self.pos_prime, self.neg, cd);
        let model = &self.sens.model;
        let flicker = ctx.flicker(
            self.pos_prime,
            self.neg,
            model.kf * f64::exp(model.af * f64::ln(f64::max((cd / m).abs(), N_MINLOG))) * m,
            1.0,
        );
        vec![rs, id, flicker, 0.0, 0.0, 0.0, rs + id + flicker]
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

    /// `DIOdSetup` (`dev/dio/diodset.c`): the 2nd/3rd-order Taylor
    /// coefficients at the operating point in `CKTrhsOld`.
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        let csat = self.t.csat;
        let vt = self.t.vt;
        let vte = self.t.vte;
        let gmin = ctx.gmin;
        let vd = ctx.sol[self.pos_prime] - ctx.sol[self.neg];

        // Derivatives of the junction current.
        let (g2, g3, cdiff2, cdiff3);
        if vd >= -5.0 * vte {
            let evd = f64::exp(vd / vte);
            let gd = csat * evd / vte + gmin;
            g2 = 0.5 * (gd - gmin) / vte;
            cdiff2 = g2 * self.t.tt;
            g3 = g2 / 3.0 / vte;
            cdiff3 = g3 * self.t.tt;
        } else if self.t.brkdwn.is_none_or(|bv| bv == 0.0 || vd >= -bv) {
            // off
            g2 = 0.0;
            g3 = 0.0;
            cdiff2 = 0.0;
            cdiff3 = 0.0;
        } else {
            // Reverse breakdown ("why using csat instead of
            // breakdowncurrent?" — the C's own question; kept).
            let bv = self.t.brkdwn.unwrap_or(0.0);
            let evrev = f64::exp(-(bv + vd) / vt);
            let gd = csat * evrev / vt;
            g2 = -gd / 2.0 / vt;
            g3 = -g2 / 3.0 / vt;
            cdiff2 = 0.0;
            cdiff3 = 0.0;
        }

        // Junction (depletion) charge storage.
        let czero = self.t.t_jct_cap * self.area;
        let (cjunc2, cjunc3);
        if czero != 0.0 {
            let (m, vj) = (self.t.m, self.t.vj);
            if vd < self.t.t_dep_cap {
                let arg = 1.0 - vd / vj;
                let sarg = f64::exp(-m * f64::ln(arg));
                let cjunc1 = czero * sarg;
                cjunc2 = cjunc1 / 2.0 / vj * m / arg;
                cjunc3 = cjunc2 / 3.0 / vj / arg * (m + 1.0);
            } else {
                let czof2 = czero / self.t.f2;
                cjunc2 = czof2 / 2.0 / vj * m;
                cjunc3 = 0.0;
            }
        } else {
            cjunc2 = 0.0;
            cjunc3 = 0.0;
        }

        self.d = DioDistoCoeffs {
            id_x2: g2,
            id_x3: g3,
            cdif_x2: cdiff2,
            cdif_x3: cdiff3,
            cjnc_x2: cjunc2,
            cjnc_x3: cjunc3,
        };
        Ok(())
    }

    /// `DIOdisto` (`dev/dio/diodisto.c`), the load modes: the
    /// memoryless current term plus `jω×` the charge term, both
    /// contracted with the lower-order kernels of `v(pos') − v(neg)`.
    fn disto_load(&self, ctx: &mut DistoCtx) {
        let d = &self.d;
        let (pp, nn) = (self.pos_prime, self.neg);
        let h = &ctx.h;
        let omega = ctx.omega;
        let g2 = d.id_x2;
        let g3 = d.id_x3;
        let c2 = d.cdif_x2 + d.cjnc_x2;
        let c3 = d.cdif_x3 + d.cjnc_x3;
        let (r1h1x, i1h1x) = DistoKernels::branch(h.r1h1, h.i1h1, pp, nn);

        let (temp, itemp) = match ctx.mode {
            DistoMode::TwoF1 => {
                let mut temp = d1n2f1(g2, r1h1x, i1h1x);
                let mut itemp = d1i2f1(g2, r1h1x, i1h1x);
                if c2 != 0.0 {
                    temp += -omega * d1i2f1(c2, r1h1x, i1h1x);
                    itemp += omega * d1n2f1(c2, r1h1x, i1h1x);
                }
                (temp, itemp)
            }
            DistoMode::ThrF1 => {
                let (r2h11x, i2h11x) = DistoKernels::branch(h.r2h11, h.i2h11, pp, nn);
                let mut temp = d1n3f1(g2, g3, r1h1x, i1h1x, r2h11x, i2h11x);
                let mut itemp = d1i3f1(g2, g3, r1h1x, i1h1x, r2h11x, i2h11x);
                if c2 != 0.0 {
                    temp += -omega * d1i3f1(c2, c3, r1h1x, i1h1x, r2h11x, i2h11x);
                    itemp += omega * d1n3f1(c2, c3, r1h1x, i1h1x, r2h11x, i2h11x);
                }
                (temp, itemp)
            }
            DistoMode::F1pF2 | DistoMode::F1mF2 => {
                let (r1h2x, mut i1h2x) = DistoKernels::branch(h.r1h2, h.i1h2, pp, nn);
                if ctx.mode == DistoMode::F1mF2 {
                    // H1(−f2) = conj(H1(f2)).
                    i1h2x = -i1h2x;
                }
                let mut temp = d1nf12(g2, r1h1x, i1h1x, r1h2x, i1h2x);
                let mut itemp = d1if12(g2, r1h1x, i1h1x, r1h2x, i1h2x);
                if c2 != 0.0 {
                    temp += -omega * d1if12(c2, r1h1x, i1h1x, r1h2x, i1h2x);
                    itemp += omega * d1nf12(c2, r1h1x, i1h1x, r1h2x, i1h2x);
                }
                (temp, itemp)
            }
            DistoMode::TwoF1mF2 => {
                let (r1h2x, i1h2x) = DistoKernels::branch(h.r1h2, h.i1h2, pp, nn);
                let i1hm2x = -i1h2x;
                let (r2h11x, i2h11x) = DistoKernels::branch(h.r2h11, h.i2h11, pp, nn);
                let (r2h1m2x, i2h1m2x) = DistoKernels::branch(h.r2h1m2, h.i2h1m2, pp, nn);
                let mut temp = d1n2f12(
                    g2, g3, r1h1x, i1h1x, r1h2x, i1hm2x, r2h11x, i2h11x, r2h1m2x, i2h1m2x,
                );
                let mut itemp = d1i2f12(
                    g2, g3, r1h1x, i1h1x, r1h2x, i1hm2x, r2h11x, i2h11x, r2h1m2x, i2h1m2x,
                );
                if c2 != 0.0 {
                    temp += -omega
                        * d1i2f12(
                            c2, c3, r1h1x, i1h1x, r1h2x, i1hm2x, r2h11x, i2h11x, r2h1m2x, i2h1m2x,
                        );
                    itemp += omega
                        * d1n2f12(
                            c2, c3, r1h1x, i1h1x, r1h2x, i1hm2x, r2h11x, i2h11x, r2h1m2x, i2h1m2x,
                        );
                }
                (temp, itemp)
            }
        };

        ctx.rhs_re[pp] -= temp;
        ctx.rhs_im[pp] -= itemp;
        ctx.rhs_re[nn] += temp;
        ctx.rhs_im[nn] += itemp;
    }

    /// `DIOpzLoad`: the AC stamps at a general complex frequency.
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        let gspr = self.t.gspr;
        let geq = ctx.states.get(0, self.st + CONDUCT);
        let capd = ctx.states.get(0, self.st + CAP_CHARGE + 1);
        ctx.add_g(self.slots[0], gspr);
        ctx.add_g(self.slots[1], geq);
        ctx.add_cap(self.slots[1], capd);
        ctx.add_g(self.slots[2], geq + gspr);
        ctx.add_cap(self.slots[2], capd);
        ctx.add_g(self.slots[3], -gspr);
        ctx.add_g(self.slots[4], -geq);
        ctx.add_cap(self.slots[4], -capd);
        ctx.add_g(self.slots[5], -gspr);
        ctx.add_g(self.slots[6], -geq);
        ctx.add_cap(self.slots[6], -capd);
        Ok(())
    }
}

/// `DEVpnjlim` (`dev/devsup.c`), in its ngspice form: the 3f5 only
/// limits forward steps above `vcrit`; ngspice also clamps negative
/// iterates at `−vold − 1` (coming from forward bias) or `2·vold − 1`
/// (already reversed), setting `check` likewise. Measured against
/// ngspice-47: it is what keeps the ltra_1/ltra_3 BJT driver and
/// diode load converging at TMAX steps, and it tightens mosmem,
/// rca3040 and bjtnoise.
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
    } else if vnew < 0.0 {
        // ngspice's `DEVpnjlim` (not the 3f5's): reverse swings are
        // limited too — to `−vold − 1` from forward bias, to
        // `2·vold − 1` from reverse (corpus/KNOWN_DIFFS.md).
        let arg = if vold > 0.0 {
            -vold - 1.0
        } else {
            2.0 * vold - 1.0
        };
        if vnew < arg {
            *check = true;
            arg
        } else {
            *check = false;
            vnew
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
        // ngspice's reverse limits.
        let v = pnjlim(-3.0, 0.5, vt, vcrit, &mut check);
        assert!(check);
        assert_eq!(v, -1.5);
        let v = pnjlim(-3.0, -0.5, vt, vcrit, &mut check);
        assert!(check);
        assert_eq!(v, -2.0);
        let v = pnjlim(-1.0, -0.5, vt, vcrit, &mut check);
        assert!(!check);
        assert_eq!(v, -1.0);
    }

    #[test]
    fn temp_adjust_is_identity_at_reftemp() {
        let m = DiodeModel::default();
        let t = DiodeTemp::new("d1", &m, 1.0, 1.0, 300.15, 300.15, 1e-3);
        assert_eq!(t.csat, m.is);
        let vte = m.n * KOVERQ * 300.15;
        assert!((t.vcrit - vte * (vte / (ROOT2 * m.is)).ln()).abs() < 1e-15);
        // At T == Tnom == REFTEMP the junction-potential adjustment
        // collapses to the model values.
        assert!((t.t_jct_cap - m.cjo).abs() < 1e-12);
        assert!((t.t_dep_cap - m.fc * m.vj).abs() < 1e-12);
        assert!(t.notes.is_empty());
    }

    /// The breakdown match (`diotemp.c`) with ngspice-47's two
    /// corrections: the knee uses `N·vt` like the load, and the
    /// absolute IBV is matched against `IS·AREA`. The load's
    /// breakdown branch then carries IBV at `vd = −BV`; the 3f5 forms
    /// put the knee 0.397 V (N = 1.47) and `vt·ln 10` (AREA = 10) off
    /// (`regress/diobv_dc`: FAIL 495 / 197 with them, PASS 0.476).
    #[test]
    fn breakdown_carries_ibv_at_bv() {
        let vt = KOVERQ * 300.15;
        let cases = [
            // (is, n, ibv, area, xbv of the 3f5 form)
            (6.79e-18, 1.47, 1e-3, 1.0, 49.156_224_229_991_27),
            (1e-14, 1.0, 1e-4, 10.0, 50.0 - vt * f64::ln(1e10)),
        ];
        for (is, n, ibv, area, xbv_3f5) in cases {
            let m = DiodeModel {
                is,
                n,
                bv: Some(50.0),
                ibv,
                ..DiodeModel::default()
            };
            let t = DiodeTemp::new("dz", &m, area, 1.0, 300.15, 300.15, 1e-3);
            let xbv = t.brkdwn.expect("bv given");
            let cd = t.csat * f64::exp(-(xbv - 50.0) / t.vte);
            assert!((cd - ibv).abs() <= 1e-3 * ibv, "cd(-BV) = {cd:e}");
            assert!(t.notes.is_empty());
            let shift = xbv_3f5 - xbv;
            let want = (n - 1.0) * vt * f64::ln(ibv / is) - vt * f64::ln(area);
            assert!((shift - want).abs() < 1e-3, "shift {shift} vs {want}");
        }
        // The symptom deck (`i 1.6 A`, is=6.79e-18 n=1.47 bv=50):
        // 50.2805 V against the 3f5's 50.6771 V.
        let m = DiodeModel {
            is: 6.79e-18,
            n: 1.47,
            bv: Some(50.0),
            ..DiodeModel::default()
        };
        let t = DiodeTemp::new("dz", &m, 1.0, 1.0, 300.15, 300.15, 1e-3);
        let v = t.brkdwn.unwrap() + t.vte * f64::ln(1.6 / t.csat);
        assert!((v - 50.2805).abs() < 1e-4, "{v}");
    }

    #[test]
    fn setup_clamps_warn() {
        let m = DiodeModel {
            m: 0.95,
            eg: 0.05,
            fc: 0.99,
            ..DiodeModel::default()
        };
        let t = DiodeTemp::new("dx", &m, 1.0, 1.0, 300.15, 300.15, 1e-3);
        assert_eq!(t.notes.len(), 3);
        assert_eq!(t.m, 0.9);
        assert!((t.f3 - (1.0 - 0.95 * 1.9)).abs() < 1e-12);
    }
}
