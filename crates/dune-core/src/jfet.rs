//! JFET — the Shichman–Hodges FET with the Sydney University doping-
//! tail modification (`B`), from the 3f5 (`dev/jfet/jfetload.c`,
//! `jfettemp.c`, `jfetset.c`, `jfetacld.c`, `jfettrun.c`). Two gate
//! junction diodes with depletion charge (−1/2 power, linearized past
//! `FC·PB`), drain current by region in normal and inverse mode, and
//! series drain/source resistance via prime nodes.
//!
//! The three-terminal FET scaffolding — the bias-resolution ladder
//! with `pnjlim`/`fetlim`, the gate diodes, the 13-slot state layout,
//! the stamps and the AC load — is shared with the MESFET
//! (`mesload.c` is line for line the same outside the drain current
//! and the gate charge), so it lives here as `pub(crate)` helpers.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::diode::pnjlim;
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::fetlim;

/// `.model … njf(…)/pjf(…)` parameters, 3f5 names and defaults
/// (`jfet.c` keyword table; defaults from `jfetset.c`).
#[derive(Clone, Debug)]
pub struct JfetModel {
    /// `NJF` (true) or `PJF`.
    pub njf: bool,
    /// `VTO`/`VT0`: threshold (pinch-off) voltage, default −2.
    pub vto: f64,
    /// `BETA`: transconductance parameter, A/V², default 1e-4.
    pub beta: f64,
    /// `LAMBDA`: channel-length modulation, 1/V, default 0.
    pub lambda: f64,
    /// `RD`: drain ohmic resistance, default 0.
    pub rd: f64,
    /// `RS`: source ohmic resistance, default 0.
    pub rs: f64,
    /// `CGS`: zero-bias G-S junction capacitance, default 0.
    pub cgs: f64,
    /// `CGD`: zero-bias G-D junction capacitance, default 0.
    pub cgd: f64,
    /// `PB`: gate junction potential, default 1.
    pub pb: f64,
    /// `IS`: gate junction saturation current, default 1e-14.
    pub is: f64,
    /// `FC`: forward-bias depletion capacitance coefficient,
    /// default 0.5 (limited to 0.95 by `JFETtemp`).
    pub fc: f64,
    /// `B`: doping tail parameter (Sydney University), default 1 —
    /// which reduces the drain current to plain Shichman–Hodges.
    pub b: f64,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `TNOM` in kelvin (the card gives °C).
    pub tnom: Option<f64>,
}

impl JfetModel {
    pub fn new(njf: bool) -> Self {
        JfetModel {
            njf,
            vto: -2.0,
            beta: 1e-4,
            lambda: 0.0,
            rd: 0.0,
            rs: 0.0,
            cgs: 0.0,
            cgd: 0.0,
            pb: 1.0,
            is: 1e-14,
            fc: 0.5,
            b: 1.0,
            kf: 0.0,
            af: 1.0,
            tnom: None,
        }
    }
}

impl Default for JfetModel {
    fn default() -> Self {
        JfetModel::new(true)
    }
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

/// Model- and temperature-derived values (`JFETtemp`, the model
/// block and the per-instance block).
#[derive(Clone, Debug)]
pub(crate) struct JfetTemp {
    pub type_: f64,
    pub vt: f64,
    pub vto: f64,
    pub beta: f64,
    pub lambda: f64,
    pub b: f64,
    /// `JFETbFac = (1 − B)/(PB − VTO)`.
    pub b_fac: f64,
    pub t_sat_cur: f64,
    pub t_cgs: f64,
    pub t_cgd: f64,
    pub t_gate_pot: f64,
    pub cor_dep_cap: f64,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
    pub vcrit: f64,
    pub drain_conduct: f64,
    pub source_conduct: f64,
    /// `KF`/`AF`: flicker-noise coefficient and exponent.
    pub kf: f64,
    pub af: f64,
    pub notes: Vec<String>,
}

impl JfetTemp {
    pub fn new(name: &str, m: &JfetModel, temp: f64, default_tnom: f64) -> Self {
        let mut notes = Vec::new();
        let tnom = m.tnom.unwrap_or(default_tnom);
        let vtnom = KOVERQ * tnom;
        let fact1 = tnom / REFTEMP;
        let kt1 = BOLTZ * tnom;
        let egfet1 = egfet(tnom);
        let arg1 = -egfet1 / (kt1 + kt1) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);
        let pbo = (m.pb - pbfact1) / fact1;
        let gmaold = (m.pb - pbo) / pbo;
        let cjfact = 1.0 / (1.0 + 0.5 * (4e-4 * (tnom - REFTEMP) - gmaold));

        let conduct = |r: f64| if r != 0.0 { 1.0 / r } else { 0.0 };
        let mut fc = m.fc;
        if fc > 0.95 {
            notes.push(format!(
                "{name}: Depletion cap. coefficient too large, limited to .95"
            ));
            fc = 0.95;
        }
        let xfc = f64::ln(1.0 - fc);
        let f2 = f64::exp((1.0 + 0.5) * xfc);
        let f3 = 1.0 - fc * (1.0 + 0.5);
        let b_fac = (1.0 - m.b) / (m.pb - m.vto);

        // Per-instance block.
        let vt = temp * KOVERQ;
        let fact2 = temp / REFTEMP;
        let ratio1 = temp / tnom - 1.0;
        let t_sat_cur = m.is * f64::exp(ratio1 * 1.11 / vt);
        let mut t_cgs = m.cgs * cjfact;
        let mut t_cgd = m.cgd * cjfact;
        let kt = BOLTZ * temp;
        let egfet_t = egfet(temp);
        let arg = -egfet_t / (kt + kt) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);
        let t_gate_pot = fact2 * pbo + pbfact;
        let gmanew = (t_gate_pot - pbo) / pbo;
        let cjfact1 = 1.0 + 0.5 * (4e-4 * (temp - REFTEMP) - gmanew);
        t_cgs *= cjfact1;
        t_cgd *= cjfact1;
        let cor_dep_cap = fc * t_gate_pot;
        let f1 = t_gate_pot * (1.0 - f64::exp((1.0 - 0.5) * xfc)) / (1.0 - 0.5);
        let vcrit = vt * f64::ln(vt / (ROOT2 * t_sat_cur));

        JfetTemp {
            type_: if m.njf { 1.0 } else { -1.0 },
            vt,
            vto: m.vto,
            beta: m.beta,
            lambda: m.lambda,
            b: m.b,
            b_fac,
            t_sat_cur,
            t_cgs,
            t_cgd,
            t_gate_pot,
            cor_dep_cap,
            f1,
            f2,
            f3,
            vcrit,
            drain_conduct: conduct(m.rd),
            source_conduct: conduct(m.rs),
            kf: m.kf,
            af: m.af,
            notes,
        }
    }
}

/// Drain current and derivatives (`jfetload.c`, the Sydney
/// University section): `(cdrain, gm, gds)` for normal (`vds ≥ 0`)
/// and inverse mode, cutoff/linear/saturation each.
#[allow(clippy::too_many_arguments)]
pub(crate) fn jfet_ids(
    vgs: f64,
    vgd: f64,
    vto: f64,
    beta: f64,
    lambda: f64,
    b: f64,
    b_fac: f64,
) -> (f64, f64, f64) {
    let vds = vgs - vgd;
    if vds >= 0.0 {
        let vgst = vgs - vto;
        if vgst <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let betap = beta * (1.0 + lambda * vds);
        if vgst >= vds {
            // Normal mode, linear region.
            let apart = 2.0 * b + 3.0 * b_fac * (vgst - vds);
            let cpart = vds * (vds * (b_fac * vds - b) + vgst * apart);
            let cdrain = betap * cpart;
            let gm = betap * vds * (apart + 3.0 * b_fac * vgst);
            let gds = betap * (vgst - vds) * apart + beta * lambda * cpart;
            (cdrain, gm, gds)
        } else {
            // Normal mode, saturation region.
            let bf = vgst * b_fac;
            let gm = betap * vgst * (2.0 * b + 3.0 * bf);
            let cpart = vgst * vgst * (b + bf);
            (betap * cpart, gm, lambda * beta * cpart)
        }
    } else {
        let vgdt = vgd - vto;
        if vgdt <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let betap = beta * (1.0 - lambda * vds);
        if vgdt + vds >= 0.0 {
            // Inverse mode, linear region.
            let apart = 2.0 * b + 3.0 * b_fac * (vgdt + vds);
            let cpart = vds * (-vds * (-b_fac * vds - b) + vgdt * apart);
            let cdrain = betap * cpart;
            let gm = betap * vds * (apart + 3.0 * b_fac * vgdt);
            let gds = betap * (vgdt + vds) * apart - beta * lambda * cpart - gm;
            (cdrain, gm, gds)
        } else {
            // Inverse mode, saturation region.
            let bf = vgdt * b_fac;
            let gm = -betap * vgdt * (2.0 * b + 3.0 * bf);
            let cpart = vgdt * vgdt * (b + bf);
            (-betap * cpart, gm, lambda * beta * cpart - gm)
        }
    }
}

// State offsets (`jfetdefs.h`; `mesdefs.h` is identical).
pub(crate) const VGS: usize = 0;
pub(crate) const VGD: usize = 1;
pub(crate) const CG: usize = 2;
pub(crate) const CD: usize = 3;
pub(crate) const CGD: usize = 4;
pub(crate) const GM: usize = 5;
pub(crate) const GDS: usize = 6;
pub(crate) const GGS: usize = 7;
pub(crate) const GGD: usize = 8;
pub(crate) const QGS: usize = 9;
pub(crate) const CQGS: usize = 10;
pub(crate) const QGD: usize = 11;
pub(crate) const CQGD: usize = 12;
/// `*states += 13` (`jfetset.c`; `MESnumStates`).
pub(crate) const FET3_NUM_STATES: usize = 13;

// Slot order (the TSTALLOC list of `jfetset.c` / `messetup.c`).
const D_DP: usize = 0;
const G_DP: usize = 1;
const G_SP: usize = 2;
const S_SP: usize = 3;
const DP_D: usize = 4;
const DP_G: usize = 5;
const DP_SP: usize = 6;
const SP_G: usize = 7;
const SP_S: usize = 8;
const SP_DP: usize = 9;
const D_D: usize = 10;
const G_G: usize = 11;
const S_S: usize = 12;
const DP_DP: usize = 13;
const SP_SP: usize = 14;

/// Terminals, prime nodes and matrix slots of a three-terminal FET
/// (JFET or MESFET).
pub(crate) struct Fet3Nodes {
    pub drain: Var,
    pub gate: Var,
    pub source: Var,
    pub d_p: Var,
    pub s_p: Var,
    s: [Slot; 15],
}

impl Fet3Nodes {
    /// `nodes` = drain, gate, source; `primes` = drain', source'.
    pub fn new(nodes: [Var; 3], primes: [Var; 2]) -> Self {
        Fet3Nodes {
            drain: nodes[0],
            gate: nodes[1],
            source: nodes[2],
            d_p: primes[0],
            s_p: primes[1],
            s: [TRASH; 15],
        }
    }

    pub fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (d, g, s, dp, sp) = (self.drain, self.gate, self.source, self.d_p, self.s_p);
        self.s = [
            mb.slot(d, dp),
            mb.slot(g, dp),
            mb.slot(g, sp),
            mb.slot(s, sp),
            mb.slot(dp, d),
            mb.slot(dp, g),
            mb.slot(dp, sp),
            mb.slot(sp, g),
            mb.slot(sp, s),
            mb.slot(sp, dp),
            mb.slot(d, d),
            mb.slot(g, g),
            mb.slot(s, s),
            mb.slot(dp, dp),
            mb.slot(sp, sp),
        ];
    }
}

/// Where a load's branch voltages came from: the initialization
/// ladder of `jfetload.c`/`mesload.c`. `hat` carries the predicted
/// gate and drain currents (`cghat`, `cdhat`) when the voltages came
/// from the solution (or the predictor) — only then does the C's
/// current-agreement convergence test mean anything.
pub(crate) struct Fet3Bias {
    pub vgs: f64,
    pub vgd: f64,
    pub check: bool,
    pub hat: Option<(f64, f64)>,
}

/// The bias-resolution ladder shared by `JFETload` and `MESload`.
/// `vt`/`vcrit` feed `DEVpnjlim` (JFET: instance temperature; MES:
/// `CONSTvt0`), `vto` feeds `DEVfetlim`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fet3_bias(
    ctx: &mut LoadCtx,
    n: &Fet3Nodes,
    st: usize,
    typ: f64,
    off: bool,
    ic: (f64, f64),
    vt: f64,
    vcrit: f64,
    vto: f64,
) -> Fet3Bias {
    let s0 = |ctx: &LoadCtx, o: usize| ctx.states.get(0, st + o);
    let fixed = |vgs, vgd| Fet3Bias {
        vgs,
        vgd,
        check: true,
        hat: None,
    };
    match ctx.mode {
        InitMode::Smsig => return fixed(s0(ctx, VGS), s0(ctx, VGD)),
        InitMode::Tran => {
            return fixed(ctx.states.get(1, st + VGS), ctx.states.get(1, st + VGD));
        }
        InitMode::Jct if ctx.kind == AnKind::TranOp && ctx.uic => {
            let vds = typ * ic.0;
            let vgs = typ * ic.1;
            return fixed(vgs, vgs - vds);
        }
        InitMode::Jct if !off => return fixed(-1.0, -1.0),
        InitMode::Jct => return fixed(0.0, 0.0),
        InitMode::Fix if off => return fixed(0.0, 0.0),
        _ => {}
    }
    let (mut vgs, mut vgd);
    if ctx.mode == InitMode::Pred {
        for o in [VGS, VGD, CG, CD, CGD, GM, GDS, GGS, GGD] {
            let v = ctx.states.get(1, st + o);
            ctx.states.set(0, st + o, v);
        }
        vgs = ctx.pred(st + VGS);
        vgd = ctx.pred(st + VGD);
    } else {
        vgs = typ * (ctx.sol[n.gate] - ctx.sol[n.s_p]);
        vgd = typ * (ctx.sol[n.gate] - ctx.sol[n.d_p]);
    }
    let delvgs = vgs - s0(ctx, VGS);
    let delvgd = vgd - s0(ctx, VGD);
    let delvds = delvgs - delvgd;
    let cghat = s0(ctx, CG) + s0(ctx, GGD) * delvgd + s0(ctx, GGS) * delvgs;
    let cdhat = s0(ctx, CD) + s0(ctx, GM) * delvgs + s0(ctx, GDS) * delvds - s0(ctx, GGD) * delvgd;

    // Limit the nonlinear branch voltages (no device bypass in Dune).
    let mut check = true;
    let mut ichk1 = true;
    vgs = pnjlim(vgs, s0(ctx, VGS), vt, vcrit, &mut check);
    vgd = pnjlim(vgd, s0(ctx, VGD), vt, vcrit, &mut ichk1);
    if ichk1 {
        check = true;
    }
    vgs = fetlim(vgs, s0(ctx, VGS), vto);
    vgd = fetlim(vgd, s0(ctx, VGD), vto);
    Fet3Bias {
        vgs,
        vgd,
        check,
        hat: Some((cghat, cdhat)),
    }
}

/// One gate junction: `(conductance, current)` with gmin, the
/// `−5·vt` reverse form of `jfetload.c`/`mesload.c`.
pub(crate) fn gate_diode(v: f64, csat: f64, vt: f64, gmin: f64) -> (f64, f64) {
    if v <= -5.0 * vt {
        let g = -csat / v + gmin;
        (g, g * v)
    } else {
        let e = f64::exp(v / vt);
        (csat * e / vt + gmin, csat * (e - 1.0) + gmin * v)
    }
}

/// Small-signal and companion values a FET load hands to the stamp.
pub(crate) struct Fet3Op {
    pub vgs: f64,
    pub vgd: f64,
    pub cg: f64,
    pub cd: f64,
    pub cgd: f64,
    pub gm: f64,
    pub gds: f64,
    pub ggs: f64,
    pub ggd: f64,
}

/// Integrate the two gate charges (already in `state0` QGS/QGD) and
/// fold the companions into `op` — the common tail of the charge
/// block. Returns `true` for the `MODEINITSMSIG` pass, which stores
/// the capacitances and stops (`continue` in the C).
pub(crate) fn fet3_charge(
    ctx: &mut LoadCtx,
    st: usize,
    capgs: f64,
    capgd: f64,
    op: &mut Fet3Op,
) -> bool {
    if ctx.kind == AnKind::TranOp && ctx.uic {
        return false;
    }
    if ctx.mode == InitMode::Smsig {
        ctx.states.set(0, st + QGS, capgs);
        ctx.states.set(0, st + QGD, capgd);
        return true;
    }
    if ctx.mode == InitMode::Tran {
        for o in [QGS, QGD] {
            let v = ctx.states.get(0, st + o);
            ctx.states.set(1, st + o, v);
        }
    }
    let (geq, _) = ctx.integrate(capgs, st + QGS);
    op.ggs += geq;
    op.cg += ctx.states.get(0, st + CQGS);
    let (geq, _) = ctx.integrate(capgd, st + QGD);
    op.ggd += geq;
    let cqgd = ctx.states.get(0, st + CQGD);
    op.cg += cqgd;
    op.cd -= cqgd;
    op.cgd += cqgd;
    if ctx.mode == InitMode::Tran {
        for o in [CQGS, CQGD] {
            let v = ctx.states.get(0, st + o);
            ctx.states.set(1, st + o, v);
        }
    }
    false
}

/// Convergence check, state save and the RHS/matrix stamps (the
/// tail of `jfetload.c`/`mesload.c` from "check convergence").
#[allow(clippy::too_many_arguments)]
pub(crate) fn fet3_finish(
    ctx: &mut LoadCtx,
    n: &Fet3Nodes,
    st: usize,
    typ: f64,
    gdpr: f64,
    gspr: f64,
    bias: &Fet3Bias,
    op: &Fet3Op,
) {
    if !(ctx.mode == InitMode::Fix && ctx.uic) {
        let disagree = match bias.hat {
            Some((cghat, cdhat)) => {
                (cghat - op.cg).abs()
                    >= ctx.reltol * f64::max(cghat.abs(), op.cg.abs()) + ctx.abstol
                    || (cdhat - op.cd).abs()
                        > ctx.reltol * f64::max(cdhat.abs(), op.cd.abs()) + ctx.abstol
            }
            None => true,
        };
        if bias.check || disagree {
            ctx.noncon = true;
        }
    }
    let vals = [
        (VGS, op.vgs),
        (VGD, op.vgd),
        (CG, op.cg),
        (CD, op.cd),
        (CGD, op.cgd),
        (GM, op.gm),
        (GDS, op.gds),
        (GGS, op.ggs),
        (GGD, op.ggd),
    ];
    for (o, v) in vals {
        ctx.states.set(0, st + o, v);
    }

    let vds = op.vgs - op.vgd;
    let ceqgd = typ * (op.cgd - op.ggd * op.vgd);
    let ceqgs = typ * ((op.cg - op.cgd) - op.ggs * op.vgs);
    let cdreq = typ * ((op.cd + op.cgd) - op.gds * vds - op.gm * op.vgs);
    ctx.add_rhs(n.gate, -ceqgs - ceqgd);
    ctx.add_rhs(n.d_p, -cdreq + ceqgd);
    ctx.add_rhs(n.s_p, cdreq + ceqgs);

    let (gm, gds, ggs, ggd) = (op.gm, op.gds, op.ggs, op.ggd);
    ctx.add(n.s[D_DP], -gdpr);
    ctx.add(n.s[G_DP], -ggd);
    ctx.add(n.s[G_SP], -ggs);
    ctx.add(n.s[S_SP], -gspr);
    ctx.add(n.s[DP_D], -gdpr);
    ctx.add(n.s[DP_G], gm - ggd);
    ctx.add(n.s[DP_SP], -gds - gm);
    ctx.add(n.s[SP_G], -ggs - gm);
    ctx.add(n.s[SP_S], -gspr);
    ctx.add(n.s[SP_DP], -gds);
    ctx.add(n.s[D_D], gdpr);
    ctx.add(n.s[G_G], ggd + ggs);
    ctx.add(n.s[S_S], gspr);
    ctx.add(n.s[DP_DP], gdpr + gds + ggd);
    ctx.add(n.s[SP_SP], gspr + gds + gm + ggs);
}

/// `JFETacLoad` / `MESacLoad`: the small-signal conductances from
/// `state0` and the capacitances the `MODEINITSMSIG` pass left in
/// the charge slots.
pub(crate) fn fet3_ac(ctx: &mut AcCtx, n: &Fet3Nodes, st: usize, gdpr: f64, gspr: f64) {
    let s0 = |o: usize| ctx.states.get(0, st + o);
    let gm = s0(GM);
    let gds = s0(GDS);
    let ggs = s0(GGS);
    let xgs = s0(QGS) * ctx.omega;
    let ggd = s0(GGD);
    let xgd = s0(QGD) * ctx.omega;
    ctx.add_g(n.s[D_D], gdpr);
    ctx.add_g(n.s[G_G], ggd + ggs);
    ctx.add_b(n.s[G_G], xgd + xgs);
    ctx.add_g(n.s[S_S], gspr);
    ctx.add_g(n.s[DP_DP], gdpr + gds + ggd);
    ctx.add_b(n.s[DP_DP], xgd);
    ctx.add_g(n.s[SP_SP], gspr + gds + gm + ggs);
    ctx.add_b(n.s[SP_SP], xgs);
    ctx.add_g(n.s[D_DP], -gdpr);
    ctx.add_g(n.s[G_DP], -ggd);
    ctx.add_b(n.s[G_DP], -xgd);
    ctx.add_g(n.s[G_SP], -ggs);
    ctx.add_b(n.s[G_SP], -xgs);
    ctx.add_g(n.s[S_SP], -gspr);
    ctx.add_g(n.s[DP_D], -gdpr);
    ctx.add_g(n.s[DP_G], -ggd + gm);
    ctx.add_b(n.s[DP_G], -xgd);
    ctx.add_g(n.s[DP_SP], -gds - gm);
    ctx.add_g(n.s[SP_G], -ggs - gm);
    ctx.add_b(n.s[SP_G], -xgs);
    ctx.add_g(n.s[SP_S], -gspr);
    ctx.add_g(n.s[SP_DP], -gds);
}

/// `JFETpzLoad` / `MESpzLoad` (`jfetpzld.c`, `mespzld.c`): the AC
/// stamps with `s` in place of `jω`. `gdpr`/`gspr` are the series
/// *conductances* times the area, as `mespzld.c` and ngspice's
/// `jfetpzld.c` read them; the 3f5's `jfetpzld.c` reads
/// `JFETdrainResist·area` — ohms stamped as siemens
/// (`corpus/KNOWN_DIFFS.md`).
pub(crate) fn fet3_pz(
    ctx: &mut crate::matrix::PzCtx,
    n: &Fet3Nodes,
    st: usize,
    gdpr: f64,
    gspr: f64,
) {
    let s0 = |o: usize| ctx.states.get(0, st + o);
    let gm = s0(GM);
    let gds = s0(GDS);
    let ggs = s0(GGS);
    let xgs = s0(QGS);
    let ggd = s0(GGD);
    let xgd = s0(QGD);
    ctx.add_g(n.s[D_D], gdpr);
    ctx.add_g(n.s[G_G], ggd + ggs);
    ctx.add_cap(n.s[G_G], xgd + xgs);
    ctx.add_g(n.s[S_S], gspr);
    ctx.add_g(n.s[DP_DP], gdpr + gds + ggd);
    ctx.add_cap(n.s[DP_DP], xgd);
    ctx.add_g(n.s[SP_SP], gspr + gds + gm + ggs);
    ctx.add_cap(n.s[SP_SP], xgs);
    ctx.add_g(n.s[D_DP], -gdpr);
    ctx.add_g(n.s[G_DP], -ggd);
    ctx.add_cap(n.s[G_DP], -xgd);
    ctx.add_g(n.s[G_SP], -ggs);
    ctx.add_cap(n.s[G_SP], -xgs);
    ctx.add_g(n.s[S_SP], -gspr);
    ctx.add_g(n.s[DP_D], -gdpr);
    ctx.add_g(n.s[DP_G], -ggd + gm);
    ctx.add_cap(n.s[DP_G], -xgd);
    ctx.add_g(n.s[DP_SP], -gds - gm);
    ctx.add_g(n.s[SP_G], -ggs - gm);
    ctx.add_cap(n.s[SP_G], -xgs);
    ctx.add_g(n.s[SP_S], -gspr);
    ctx.add_g(n.s[SP_DP], -gds);
}

/// Name suffixes of the JFET/MESFET noise generators
/// (`JFETnNames`, `MESnNames`).
pub(crate) const FET3_NOISE_SUFFIXES: &[&str] = &["_rd", "_rs", "_id", "_1overf", ""];

/// `JFETnoise` (ngspice-47, default `NLEV = 2`) / `MESnoise`:
/// thermal noise of RD and RS, channel thermal noise
/// `4kT·(2/3)|gm|`, flicker `KF·|Id|^AF/f` on the drain current of
/// `state0`, and the total. `gdpr`/`gspr` already carry the area.
pub(crate) fn fet3_noise(
    ctx: &crate::matrix::NoiseCtx,
    n: &Fet3Nodes,
    st: usize,
    gdpr: f64,
    gspr: f64,
    kf: f64,
    af: f64,
) -> Vec<f64> {
    const N_MINLOG: f64 = 1e-38;
    let rd = ctx.thermal(n.d_p, n.drain, gdpr);
    let rs = ctx.thermal(n.s_p, n.source, gspr);
    let gm = ctx.states.get(0, st + GM);
    let id = ctx.thermal(n.d_p, n.s_p, 2.0 / 3.0 * gm.abs());
    let cd = ctx.states.get(0, st + CD);
    let flicker = ctx.flicker(
        n.d_p,
        n.s_p,
        kf * f64::exp(af * f64::ln(f64::max(cd.abs(), N_MINLOG))),
        1.0,
    );
    vec![rd, rs, id, flicker, rd + rs + id + flicker]
}

/// A JFET instance (`JFETinstance`).
pub(crate) struct JfetI {
    n: Fet3Nodes,
    t: JfetTemp,
    area: f64,
    off: bool,
    /// `IC=VDS,VGS`.
    ic: (f64, f64),
    ic_given: crate::devices::IcGiven,
    st: usize,
}

impl JfetI {
    pub fn new(
        nodes: [Var; 3],
        primes: [Var; 2],
        t: JfetTemp,
        area: f64,
        off: bool,
        ic: (f64, f64),
        st: usize,
    ) -> Self {
        JfetI {
            n: Fet3Nodes::new(nodes, primes),
            t,
            area,
            off,
            ic,
            ic_given: crate::devices::IcGiven::ALL,
            st,
        }
    }
}

impl crate::devices::Device for JfetI {
    fn par_load(&self) -> bool {
        true
    }

    /// `JFETgetic` (`jfetic.c`): VDS and VGS not given on the card from the
    /// external nodes.
    fn set_ic(&mut self, rhs: &[f64]) {
        use crate::devices::getic;
        let g = self.ic_given;
        let n = &self.n;
        getic(g, 0, &mut self.ic.0, rhs, n.drain, n.source);
        getic(g, 1, &mut self.ic.1, rhs, n.gate, n.source);
    }

    fn set_ic_given(&mut self, given: crate::devices::IcGiven) {
        self.ic_given = given;
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.n.alloc(mb);
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        let t = &self.t;
        let st = self.st;
        let typ = t.type_;
        let beta = t.beta * self.area;
        let gdpr = t.drain_conduct * self.area;
        let gspr = t.source_conduct * self.area;
        let csat = t.t_sat_cur * self.area;

        let bias = fet3_bias(
            ctx, &self.n, st, typ, self.off, self.ic, t.vt, t.vcrit, t.vto,
        );
        let (vgs, vgd) = (bias.vgs, bias.vgd);

        // DC current and derivatives.
        let (ggs, cg) = gate_diode(vgs, csat, t.vt, ctx.gmin);
        let (ggd, cgd) = gate_diode(vgd, csat, t.vt, ctx.gmin);
        let (cdrain, gm, gds) = jfet_ids(vgs, vgd, t.vto, beta, t.lambda, t.b, t.b_fac);
        let mut op = Fet3Op {
            vgs,
            vgd,
            cg: cg + cgd,
            cd: cdrain - cgd,
            cgd,
            gm,
            gds,
            ggs,
            ggd,
        };

        // Charge storage: transient, the small-signal pass and the
        // UIC transient operating point.
        if ctx.kind == AnKind::Tran
            || ctx.mode == InitMode::Smsig
            || (ctx.kind == AnKind::TranOp && ctx.uic)
        {
            let czgs = t.t_cgs * self.area;
            let czgd = t.t_cgd * self.area;
            let twop = t.t_gate_pot + t.t_gate_pot;
            let fcpb2 = t.cor_dep_cap * t.cor_dep_cap;
            let junction = |v: f64, cz: f64| {
                if v < t.cor_dep_cap {
                    let sarg = f64::sqrt(1.0 - v / t.t_gate_pot);
                    (twop * cz * (1.0 - sarg), cz / sarg)
                } else {
                    let czf2 = cz / t.f2;
                    (
                        cz * t.f1
                            + czf2 * (t.f3 * (v - t.cor_dep_cap) + (v * v - fcpb2) / (twop + twop)),
                        czf2 * (t.f3 + v / twop),
                    )
                }
            };
            let (qgs, capgs) = junction(vgs, czgs);
            let (qgd, capgd) = junction(vgd, czgd);
            ctx.states.set(0, st + QGS, qgs);
            ctx.states.set(0, st + QGD, qgd);
            if fet3_charge(ctx, st, capgs, capgd, &mut op) {
                return;
            }
        }

        fet3_finish(ctx, &self.n, st, typ, gdpr, gspr, &bias, &op);
    }

    /// `JFETtrunc`.
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QGS, t, time_step);
        crate::integ::terr(self.st + QGD, t, time_step);
    }

    /// `JFETacLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        fet3_ac(ctx, &self.n, self.st, gdpr, gspr);
    }

    /// `JFETpzLoad` (`jfetpzld.c`): see [`fet3_pz`].
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        fet3_pz(ctx, &self.n, self.st, gdpr, gspr);
        Ok(())
    }

    /// `JFETnoise`: see [`fet3_noise`].
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(FET3_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        fet3_noise(ctx, &self.n, self.st, gdpr, gspr, self.t.kf, self.t.af)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_is_identity_at_tnom() {
        let mut m = JfetModel::new(true);
        m.cgs = 2e-12;
        m.cgd = 1e-12;
        m.pb = 0.8;
        m.rd = 50.0;
        let t = JfetTemp::new("j1", &m, REFTEMP, REFTEMP);
        assert_eq!(t.t_sat_cur, m.is);
        assert!((t.t_gate_pot - 0.8).abs() < 1e-12);
        assert!((t.t_cgs - 2e-12).abs() < 1e-24);
        assert!((t.t_cgd - 1e-12).abs() < 1e-24);
        assert_eq!(t.drain_conduct, 0.02);
        assert_eq!(t.source_conduct, 0.0);
        assert_eq!(t.b_fac, 0.0); // B = 1
        let vt = REFTEMP * KOVERQ;
        assert!((t.vcrit - vt * (vt / (ROOT2 * 1e-14)).ln()).abs() < 1e-15);
        // f1/f2/f3 for FC = 0.5.
        assert!((t.f2 - 0.5f64.powf(1.5)).abs() < 1e-15);
        assert!((t.f3 - 0.25).abs() < 1e-15);
        assert!((t.f1 - 0.8 * (1.0 - 0.5f64.sqrt()) / 0.5).abs() < 1e-14);
        assert!(t.notes.is_empty());
    }

    #[test]
    fn temp_clamps_fc_and_warms_up() {
        let mut m = JfetModel::new(false);
        m.fc = 0.99;
        let t = JfetTemp::new("j1", &m, REFTEMP + 50.0, REFTEMP);
        assert_eq!(t.notes.len(), 1);
        assert!((t.cor_dep_cap - 0.95 * t.t_gate_pot).abs() < 1e-15);
        // Hotter: more saturation current, lower junction potential.
        assert!(t.t_sat_cur > m.is);
        assert!(t.t_gate_pot < m.pb);
        assert_eq!(t.type_, -1.0);
    }

    #[test]
    fn ids_reduces_to_shichman_hodges_with_b_one() {
        let (beta, lambda, vto) = (1e-3, 0.02, -2.0);
        // Saturation: beta·(1+λ·vds)·vgst².
        let (vgs, vgd) = (-1.0, -6.0);
        let (id, gm, gds) = jfet_ids(vgs, vgd, vto, beta, lambda, 1.0, 0.0);
        let vgst = vgs - vto;
        let vds = vgs - vgd;
        assert!((id - beta * (1.0 + lambda * vds) * vgst * vgst).abs() < 1e-18);
        assert!((gm - 2.0 * beta * (1.0 + lambda * vds) * vgst).abs() < 1e-18);
        assert!((gds - lambda * beta * vgst * vgst).abs() < 1e-18);
        // Linear: beta·(1+λ·vds)·vds·(2·vgst − vds).
        let (vgs, vgd) = (0.0, -0.5);
        let (id, _, _) = jfet_ids(vgs, vgd, vto, beta, lambda, 1.0, 0.0);
        let vds = 0.5;
        let vgst = 2.0;
        assert!((id - beta * (1.0 + lambda * vds) * vds * (2.0 * vgst - vds)).abs() < 1e-18);
        // Cutoff.
        assert_eq!(
            jfet_ids(-3.0, -8.0, vto, beta, lambda, 1.0, 0.0),
            (0.0, 0.0, 0.0)
        );
    }

    #[test]
    fn ids_derivatives_match_finite_differences() {
        // B ≠ 1 exercises the doping-tail terms; check every region
        // in both modes against central differences.
        let (beta, lambda, vto, b, pb) = (1e-3, 0.05, -2.0, 0.8, 1.0);
        let b_fac = (1.0 - b) / (pb - vto);
        let h = 1e-6;
        for &(vgs, vgd) in &[
            (-0.5, -1.0), // normal linear
            (-0.5, -4.0), // normal saturation
            (-1.0, -0.6), // inverse linear
            (-4.0, -0.5), // inverse saturation
        ] {
            let f = |a: f64, c: f64| jfet_ids(a, c, vto, beta, lambda, b, b_fac).0;
            let (_, gm, gds) = jfet_ids(vgs, vgd, vto, beta, lambda, b, b_fac);
            // The C's (gm, gds) are ∂id/∂vgs at fixed vds and ∂id/∂vds
            // at fixed vgs, with vgd = vgs − vds.
            let vds = vgs - vgd;
            let dgs = (f(vgs + h, vgs + h - vds) - f(vgs - h, vgs - h - vds)) / (2.0 * h);
            let dds = (f(vgs, vgs - vds - h) - f(vgs, vgs - vds + h)) / (2.0 * h);
            assert!(
                (gm - dgs).abs() < 1e-8 * (1.0 + gm.abs()),
                "gm at {vgs},{vgd}"
            );
            assert!(
                (gds - dds).abs() < 1e-8 * (1.0 + gds.abs()),
                "gds at {vgs},{vgd}"
            );
        }
    }

    #[test]
    fn gate_diode_regions() {
        let vt = REFTEMP * KOVERQ;
        let (g, i) = gate_diode(-1.0, 1e-14, vt, 1e-12);
        assert!((i - (-1e-14 - 1e-12)).abs() < 1e-27);
        assert!((g - (1e-14 + 1e-12)).abs() < 1e-27);
        let (g, i) = gate_diode(0.6, 1e-14, vt, 0.0);
        let e = (0.6 / vt).exp();
        assert_eq!(i, 1e-14 * (e - 1.0));
        assert_eq!(g, 1e-14 * e / vt);
    }
}
