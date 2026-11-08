//! BSIM3v3.3.0 — ngspice-47's `LEVEL=8`/`49` MOSFET
//! (`spicelib/devices/bsim3/`: `b3ld.c`, `b3temp.c`, `b3set.c`,
//! `b3mpar.c`, `b3cvtest.c`, `b3acld.c`, `b3noi.c`, `b3check.c`).
//! The 3f5 predates BSIM3, so here ngspice-47 is both the contract
//! and the oracle. Only the default revision is implemented
//! (`VERSION` 3.3.x; ngspice routes 3.0/3.1/3.2 to separate older
//! code bases), with the quasi-static charge model (`NQSMOD = 0`,
//! `ACNQSMOD = 0`) and `ACM = 0` junction geometry; the elaborator
//! refuses cards and instances asking for the others.
//!
//! Layout: [`params`] holds the generated parameter table and the
//! `b3set.c` defaulting, [`temp`] the `b3temp.c` model/size
//! preprocessing, [`eval`] the bias evaluation of `b3ld.c`; this
//! file is the load shell (limiting, junction diodes, overlap
//! charges, integration, stamps), the AC and noise loads and the
//! convergence test.

mod eval;
mod params;
mod temp;

use std::sync::Arc;

use crate::consts::VT0;
use crate::diode::pnjlim;
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{fetlim, limvds};

pub use params::Bsim3Model;
pub(crate) use temp::{Bsim3MTemp, Bsim3Size};

use eval::{Ev, InstTemp, evaluate};

const DELTA_1: f64 = 0.02;

// State offsets (`bsim3def.h`).
const VBD: usize = 0;
const VBS: usize = 1;
const VGS: usize = 2;
const VDS: usize = 3;
const QB: usize = 4;
const CQB: usize = 5;
const QG: usize = 6;
const CQG: usize = 7;
const QD: usize = 8;
const CQD: usize = 9;
const QBS: usize = 10;
const QBD: usize = 11;
/// `BSIM3numStates` (the NQS slots 12–16 stay unused).
pub(crate) const BSIM3_NUM_STATES: usize = 17;

// Matrix slots, in `BSIM3setup`'s TSTALLOC order (the charge-node
// entries of the NQS model are not allocated).
const D_D: usize = 0;
const G_G: usize = 1;
const S_S: usize = 2;
const B_B: usize = 3;
const DP_DP: usize = 4;
const SP_SP: usize = 5;
const D_DP: usize = 6;
const G_B: usize = 7;
const G_DP: usize = 8;
const G_SP: usize = 9;
const S_SP: usize = 10;
const B_DP: usize = 11;
const B_SP: usize = 12;
const DP_SP: usize = 13;
const DP_D: usize = 14;
const B_G: usize = 15;
const DP_G: usize = 16;
const SP_G: usize = 17;
const SP_S: usize = 18;
const DP_B: usize = 19;
const SP_B: usize = 20;
const SP_DP: usize = 21;

/// BSIM3 noise generator names (`BSIM3nNames`): ngspice prints them
/// as `onoise.<inst>.rd` etc.
pub(crate) const BSIM3_NOISE_SUFFIXES: &[&str] = &[".rd", ".rs", ".id", ".1overf", ""];

/// Instance geometry and options (`BSIM3instance` inputs).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bsim3Geom {
    pub l: f64,
    pub w: f64,
    pub ad: f64,
    pub as_: f64,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    /// `M`, the parallel multiplier.
    pub m: f64,
    pub delvto: f64,
    pub mulu0: f64,
}

/// One BSIM3 instance: the shared model and size knot plus the
/// `here->BSIM3…` fields the C keeps between loads (the BSIM3 load,
/// unlike the older MOS loads, keeps currents and conductances in
/// the instance, not the state vector).
pub(crate) struct Bsim3I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    model: Arc<Bsim3Model>,
    mt: Arc<Bsim3MTemp>,
    pp: Arc<Bsim3Size>,
    it: InstTemp,
    g: Bsim3Geom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    ic_given: crate::devices::IcGiven,
    drain_conductance: f64,
    source_conductance: f64,
    vjsm: f64,
    is_evjsm: f64,
    vjdm: f64,
    is_evjdm: f64,
    st: usize,
    s: [Slot; 22],
    // Load outputs.
    mode: i32,
    ev: Ev,
    qinv: f64,
    cd: f64,
    cbs: f64,
    cbd: f64,
    gbs: f64,
    gbd: f64,
    capbs: f64,
    capbd: f64,
    cgso: f64,
    cgdo: f64,
}

impl Bsim3I {
    /// The per-instance tail of `BSIM3temp`: `delvto`, `mulu0`, the
    /// series conductances and the `ijth` junction limits.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],
        primes: [Var; 2],
        model: Arc<Bsim3Model>,
        mt: Arc<Bsim3MTemp>,
        pp: Arc<Bsim3Size>,
        g: Bsim3Geom,
        off: bool,
        ic: (f64, f64, f64),
        st: usize,
    ) -> Self {
        let typ = model.type_();
        let it = InstTemp {
            vth0: pp.vth0 + g.delvto,
            vfb: pp.vfb + typ * g.delvto,
            vfbzb: pp.vfbzb + typ * g.delvto,
            u0temp: pp.u0temp * g.mulu0,
        };
        let drain_resistance = model.sheet_resistance * g.nrd;
        let source_resistance = model.sheet_resistance * g.nrs;
        let drain_conductance = if drain_resistance > 0.0 {
            1.0 / drain_resistance
        } else {
            0.0
        };
        let source_conductance = if source_resistance > 0.0 {
            1.0 / source_resistance
        } else {
            0.0
        };
        let nvtm = mt.vtm * model.jct_emission_coeff;
        let (src_sat, drn_sat) = sat_currents(&mt, &g);
        let (mut vjsm, mut is_evjsm, mut vjdm, mut is_evjdm) = (0.0, 0.0, 0.0, 0.0);
        if src_sat > 0.0 && model.ijth > 0.0 {
            vjsm = nvtm * f64::ln(model.ijth / src_sat + 1.0);
            is_evjsm = src_sat * f64::exp(vjsm / nvtm);
        }
        if drn_sat > 0.0 && model.ijth > 0.0 {
            vjdm = nvtm * f64::ln(model.ijth / drn_sat + 1.0);
            is_evjdm = drn_sat * f64::exp(vjdm / nvtm);
        }
        let (cgso, cgdo) = (pp.cgso_p, pp.cgdo_p);
        Bsim3I {
            drain: nodes[0],
            gate: nodes[1],
            source: nodes[2],
            bulk: nodes[3],
            d_p: primes[0],
            s_p: primes[1],
            model,
            mt,
            pp,
            it,
            g,
            off,
            ic_vds: ic.0,
            ic_vgs: ic.1,
            ic_vbs: ic.2,
            ic_given: crate::devices::IcGiven::ALL,
            drain_conductance,
            source_conductance,
            vjsm,
            is_evjsm,
            vjdm,
            is_evjdm,
            st,
            s: [TRASH; 22],
            mode: 1,
            ev: Ev::default(),
            qinv: 0.0,
            cd: 0.0,
            cbs: 0.0,
            cbd: 0.0,
            gbs: 0.0,
            gbd: 0.0,
            capbs: 0.0,
            capbd: 0.0,
            cgso,
            cgdo,
        }
    }
}

/// `SourceSatCurrent`/`DrainSatCurrent` with `ACM = 0` (`b3ld.c`,
/// `b3temp.c`): area and perimeter densities, `1e-14` when neither
/// geometry is given.
fn sat_currents(mt: &Bsim3MTemp, g: &Bsim3Geom) -> (f64, f64) {
    let js = mt.jct_temp_sat_cur_density;
    let jssw = mt.jct_sidewall_temp_sat_cur_density;
    let src = if g.as_ <= 0.0 && g.ps <= 0.0 {
        1.0e-14
    } else {
        g.as_ * js + g.ps * jssw
    };
    let drn = if g.ad <= 0.0 && g.pd <= 0.0 {
        1.0e-14
    } else {
        g.ad * js + g.pd * jssw
    };
    (src, drn)
}

/// One bulk-junction diode with the `ijth` linear extension
/// (`b3ld.c`, "Source/drain junction diode DC model"): (g, i).
fn junction(
    sat: f64,
    v: f64,
    nvtm: f64,
    ijth: f64,
    vjm: f64,
    is_evjm: f64,
    gmin: f64,
) -> (f64, f64) {
    if sat <= 0.0 {
        (gmin, gmin * v)
    } else if ijth == 0.0 || v < vjm {
        let ev = f64::exp(v / nvtm);
        (sat * ev / nvtm + gmin, sat * (ev - 1.0) + gmin * v)
    } else {
        let t0 = is_evjm / nvtm;
        (t0 + gmin, is_evjm - sat + t0 * (v - vjm) + gmin * v)
    }
}

/// Depletion charge and capacitance of one junction (bottom,
/// field-oxide sidewall, gate-edge sidewall) at `v` (`b3ld.c`).
#[allow(clippy::too_many_arguments)]
fn junction_charge(
    v: f64,
    cz: f64,
    czsw: f64,
    czswg: f64,
    pb: f64,
    pbsw: f64,
    pbswg: f64,
    mj: f64,
    mjsw: f64,
    mjswg: f64,
) -> (f64, f64) {
    let sarg_of = |arg: f64, mj: f64| {
        if mj == 0.5 {
            1.0 / arg.sqrt()
        } else {
            f64::exp(-mj * arg.ln())
        }
    };
    if v == 0.0 {
        (0.0, cz + czsw + czswg)
    } else if v < 0.0 {
        let (mut q, mut c);
        if cz > 0.0 {
            let arg = 1.0 - v / pb;
            let sarg = sarg_of(arg, mj);
            q = pb * cz * (1.0 - arg * sarg) / (1.0 - mj);
            c = cz * sarg;
        } else {
            q = 0.0;
            c = 0.0;
        }
        if czsw > 0.0 {
            let arg = 1.0 - v / pbsw;
            let sarg = sarg_of(arg, mjsw);
            q += pbsw * czsw * (1.0 - arg * sarg) / (1.0 - mjsw);
            c += czsw * sarg;
        }
        if czswg > 0.0 {
            let arg = 1.0 - v / pbswg;
            let sarg = sarg_of(arg, mjswg);
            q += pbswg * czswg * (1.0 - arg * sarg) / (1.0 - mjswg);
            c += czswg * sarg;
        }
        (q, c)
    } else {
        let t0 = cz + czsw + czswg;
        let t1 = v * (cz * mj / pb + czsw * mjsw / pbsw + czswg * mjswg / pbswg);
        (v * (t0 + 0.5 * t1), t0 + t1)
    }
}

impl crate::devices::Device for Bsim3I {
    fn par_load(&self) -> bool {
        true
    }

    /// `BSIM3getic` (ngspice-47 `b3getic.c`): VBS, VDS, VGS not given
    /// on the card from the external nodes.
    fn set_ic(&mut self, rhs: &[f64]) {
        use crate::devices::getic;
        let g = self.ic_given;
        getic(g, 2, &mut self.ic_vbs, rhs, self.bulk, self.source);
        getic(g, 0, &mut self.ic_vds, rhs, self.drain, self.source);
        getic(g, 1, &mut self.ic_vgs, rhs, self.gate, self.source);
    }

    fn set_ic_given(&mut self, given: crate::devices::IcGiven) {
        self.ic_given = given;
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (d, g, s, b) = (self.drain, self.gate, self.source, self.bulk);
        let (dp, sp) = (self.d_p, self.s_p);
        self.s = [
            mb.slot(d, d),
            mb.slot(g, g),
            mb.slot(s, s),
            mb.slot(b, b),
            mb.slot(dp, dp),
            mb.slot(sp, sp),
            mb.slot(d, dp),
            mb.slot(g, b),
            mb.slot(g, dp),
            mb.slot(g, sp),
            mb.slot(s, sp),
            mb.slot(b, dp),
            mb.slot(b, sp),
            mb.slot(dp, sp),
            mb.slot(dp, d),
            mb.slot(b, g),
            mb.slot(dp, g),
            mb.slot(sp, g),
            mb.slot(sp, s),
            mb.slot(dp, b),
            mb.slot(sp, b),
            mb.slot(sp, dp),
        ];
    }

    /// `BSIM3convTest` (`b3cvtest.c`), reading the instance values
    /// the last load left.
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.model.type_();
        let st = self.st;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let vbs = typ * (ctx.rhs[self.bulk] - ctx.rhs[self.s_p]);
        let vgs = typ * (ctx.rhs[self.gate] - ctx.rhs[self.s_p]);
        let vds = typ * (ctx.rhs[self.d_p] - ctx.rhs[self.s_p]);
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgdo = s0(VGS) - s0(VDS);
        let delvbs = vbs - s0(VBS);
        let delvbd = vbd - s0(VBD);
        let delvgs = vgs - s0(VGS);
        let delvds = vds - s0(VDS);
        let delvgd = vgd - vgdo;
        let e = &self.ev;

        let mut cd = self.cd - self.cbd;
        if self.mode >= 0 {
            cd += e.isub;
        }
        let cdhat = if self.mode >= 0 {
            cd - self.gbd * delvbd
                + (e.gmbs + e.gbbs) * delvbs
                + (e.gm + e.gbgs) * delvgs
                + (e.gds + e.gbds) * delvds
        } else {
            cd + (e.gmbs - self.gbd) * delvbd + e.gm * delvgd - e.gds * delvds
        };

        // The C skips the test only for an `off` device in INITFIX;
        // the Newton loop never calls it there.
        let tol = ctx.reltol * f64::max(cdhat.abs(), cd.abs()) + ctx.abstol;
        if (cdhat - cd).abs() >= tol {
            return false;
        }
        let cbs = self.cbs;
        let cbd = self.cbd;
        let cbhat = if self.mode >= 0 {
            cbs + cbd - e.isub + self.gbd * delvbd + (self.gbs - e.gbbs) * delvbs
                - e.gbgs * delvgs
                - e.gbds * delvds
        } else {
            cbs + cbd - e.isub + self.gbs * delvbs + (self.gbd - e.gbbs) * delvbd - e.gbgs * delvgd
                + e.gbds * delvds
        };
        let ib = cbs + cbd - e.isub;
        let tol = ctx.reltol * f64::max(cbhat.abs(), ib.abs()) + ctx.abstol;
        (cbhat - ib).abs() <= tol
    }

    /// `BSIM3load` (`b3ld.c`) without the NQS branches and the
    /// bypass (`NOBYPASS`-equivalent, like every Dune device).
    #[allow(clippy::too_many_lines)]
    fn load(&mut self, ctx: &mut LoadCtx) {
        // Borrow the shared model cards, never clone the `Arc`s: a
        // refcount bump per load is an atomic on a cache line every
        // instance of the model shares, which serializes the parallel
        // load (issue #9).
        let model: &Bsim3Model = &self.model;
        let mt: &Bsim3MTemp = &self.mt;
        let pp: &Bsim3Size = &self.pp;
        let typ = model.type_();
        let st = self.st;

        let charges = matches!(ctx.kind, AnKind::DcSweep | AnKind::Tran)
            || ctx.mode == InitMode::Smsig
            || (ctx.kind == AnKind::TranOp && ctx.uic);

        let mut check = true;
        let (mut vbs, mut vgs, mut vds);
        if ctx.mode == InitMode::Smsig {
            vbs = ctx.states.get(0, st + VBS);
            vgs = ctx.states.get(0, st + VGS);
            vds = ctx.states.get(0, st + VDS);
        } else if ctx.mode == InitMode::Tran {
            vbs = ctx.states.get(1, st + VBS);
            vgs = ctx.states.get(1, st + VGS);
            vds = ctx.states.get(1, st + VDS);
        } else if ctx.mode == InitMode::Jct && !self.off {
            vds = typ * self.ic_vds;
            vgs = typ * self.ic_vgs;
            vbs = typ * self.ic_vbs;
            if vds == 0.0 && vgs == 0.0 && vbs == 0.0 && !(ctx.kind == AnKind::TranOp && ctx.uic) {
                vbs = 0.0;
                vgs = typ * self.it.vth0 + 0.1;
                vds = 0.1;
            }
        } else if matches!(ctx.mode, InitMode::Jct | InitMode::Fix) && self.off {
            vbs = 0.0;
            vgs = 0.0;
            vds = 0.0;
        } else {
            if ctx.mode == InitMode::Pred {
                let xfact = ctx.pred_xfact;
                let mut pred = |o: usize| {
                    let s1 = ctx.states.get(1, st + o);
                    ctx.states.set(0, st + o, s1);
                    (1.0 + xfact) * s1 - xfact * ctx.states.get(2, st + o)
                };
                vbs = pred(VBS);
                vgs = pred(VGS);
                vds = pred(VDS);
                let vbd0 = ctx.states.get(0, st + VBS) - ctx.states.get(0, st + VDS);
                ctx.states.set(0, st + VBD, vbd0);
            } else {
                vbs = typ * (ctx.sol[self.bulk] - ctx.sol[self.s_p]);
                vgs = typ * (ctx.sol[self.gate] - ctx.sol[self.s_p]);
                vds = typ * (ctx.sol[self.d_p] - ctx.sol[self.s_p]);
            }
            let mut vbd = vbs - vds;
            let mut vgd = vgs - vds;
            let vgdo = ctx.states.get(0, st + VGS) - ctx.states.get(0, st + VDS);

            let von = self.ev.von;
            if ctx.states.get(0, st + VDS) >= 0.0 {
                vgs = fetlim(vgs, ctx.states.get(0, st + VGS), von);
                vds = vgs - vgd;
                vds = limvds(vds, ctx.states.get(0, st + VDS));
                vgd = vgs - vds;
            } else {
                vgd = fetlim(vgd, vgdo, von);
                vds = vgs - vgd;
                vds = -limvds(-vds, -ctx.states.get(0, st + VDS));
                vgs = vgd + vds;
            }
            let _ = vgd;

            if vds >= 0.0 {
                vbs = pnjlim(vbs, ctx.states.get(0, st + VBS), VT0, mt.vcrit, &mut check);
            } else {
                vbd = pnjlim(vbd, ctx.states.get(0, st + VBD), VT0, mt.vcrit, &mut check);
                vbs = vbd + vds;
            }
        }

        // Determine DC current and derivatives.
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgb = vgs - vbs;

        // Source/drain junction diode DC model.
        let nvtm = mt.vtm * model.jct_emission_coeff;
        let (src_sat, drn_sat) = sat_currents(mt, &self.g);
        (self.gbs, self.cbs) = junction(
            src_sat,
            vbs,
            nvtm,
            model.ijth,
            self.vjsm,
            self.is_evjsm,
            ctx.gmin,
        );
        (self.gbd, self.cbd) = junction(
            drn_sat,
            vbd,
            nvtm,
            model.ijth,
            self.vjdm,
            self.is_evjdm,
            ctx.gmin,
        );

        let (vds_m, vgs_m, vbs_m);
        if vds >= 0.0 {
            // Normal mode.
            self.mode = 1;
            vds_m = vds;
            vgs_m = vgs;
            vbs_m = vbs;
        } else {
            // Inverse mode.
            self.mode = -1;
            vds_m = -vds;
            vgs_m = vgd;
            vbs_m = vbd;
        }

        let ev = evaluate(model, mt, pp, &self.it, vds_m, vgs_m, vbs_m, charges);
        if let Some(q) = ev.qinv {
            self.qinv = q;
        }
        self.ev = ev;
        let cdrain = ev.ids;
        let (mut qgate, mut qbulk, mut qdrn) = (ev.qgate, ev.qbulk, ev.qdrn);
        self.cd = cdrain;

        if charges {
            // Bulk-drain and bulk-source depletion capacitances
            // (`ACM = 0`): sidewalls split at the effective width.
            let (czbd, czbs) = (
                mt.unit_area_temp_jct_cap * self.g.ad,
                mt.unit_area_temp_jct_cap * self.g.as_,
            );
            let (czbdsw, czbdswg) = if self.g.pd < pp.weff {
                (0.0, mt.unit_length_gate_sidewall_temp_jct_cap * self.g.pd)
            } else {
                (
                    mt.unit_length_sidewall_temp_jct_cap * (self.g.pd - pp.weff),
                    mt.unit_length_gate_sidewall_temp_jct_cap * pp.weff,
                )
            };
            let (czbssw, czbsswg) = if self.g.ps < pp.weff {
                (0.0, mt.unit_length_gate_sidewall_temp_jct_cap * self.g.ps)
            } else {
                (
                    mt.unit_length_sidewall_temp_jct_cap * (self.g.ps - pp.weff),
                    mt.unit_length_gate_sidewall_temp_jct_cap * pp.weff,
                )
            };
            let mj = model.bulk_jct_bot_grading_coeff;
            let mjsw = model.bulk_jct_side_grading_coeff;
            let mjswg = model.bulk_jct_gate_side_grading_coeff;
            let (qbs, capbs) = junction_charge(
                vbs,
                czbs,
                czbssw,
                czbsswg,
                mt.phi_b,
                mt.phi_bsw,
                mt.phi_bswg,
                mj,
                mjsw,
                mjswg,
            );
            ctx.states.set(0, st + QBS, qbs);
            self.capbs = capbs;
            let (qbd, capbd) = junction_charge(
                vbd,
                czbd,
                czbdsw,
                czbdswg,
                mt.phi_b,
                mt.phi_bsw,
                mt.phi_bswg,
                mj,
                mjsw,
                mjswg,
            );
            ctx.states.set(0, st + QBD, qbd);
            self.capbd = capbd;
        }

        // Check convergence (NEWCONV: only the limiting flag here).
        if (!self.off || ctx.mode != InitMode::Fix) && check {
            ctx.noncon = true;
        }
        ctx.states.set(0, st + VBS, vbs);
        ctx.states.set(0, st + VBD, vbd);
        ctx.states.set(0, st + VGS, vgs);
        ctx.states.set(0, st + VDS, vds);

        // Bulk and channel charge plus overlaps.
        let (mut ceqqg, mut ceqqb, mut ceqqd) = (0.0, 0.0, 0.0);
        let (mut gcggb, mut gcgdb, mut gcgsb) = (0.0, 0.0, 0.0);
        let (mut gcbgb, mut gcbdb, mut gcbsb) = (0.0, 0.0, 0.0);
        let (mut gcdgb, mut gcddb, mut gcdsb) = (0.0, 0.0, 0.0);
        let (mut gcsgb, mut gcsdb, mut gcssb) = (0.0, 0.0, 0.0);
        if charges {
            // line755:
            let (cgdo, qgdo, cgso, qgso);
            if model.cap_mod == 0 {
                cgdo = pp.cgdo_p;
                qgdo = pp.cgdo_p * vgd;
                cgso = pp.cgso_p;
                qgso = pp.cgso_p * vgs;
            } else if model.cap_mod == 1 {
                if vgd < 0.0 {
                    let t1 = f64::sqrt(1.0 - 4.0 * vgd / pp.ckappa);
                    cgdo = pp.cgdo_p + pp.weff_cv * pp.cgdl / t1;
                    qgdo = pp.cgdo_p * vgd - pp.weff_cv * 0.5 * pp.cgdl * pp.ckappa * (t1 - 1.0);
                } else {
                    cgdo = pp.cgdo_p + pp.weff_cv * pp.cgdl;
                    qgdo = (pp.weff_cv * pp.cgdl + pp.cgdo_p) * vgd;
                }
                if vgs < 0.0 {
                    let t1 = f64::sqrt(1.0 - 4.0 * vgs / pp.ckappa);
                    cgso = pp.cgso_p + pp.weff_cv * pp.cgsl / t1;
                    qgso = pp.cgso_p * vgs - pp.weff_cv * 0.5 * pp.cgsl * pp.ckappa * (t1 - 1.0);
                } else {
                    cgso = pp.cgso_p + pp.weff_cv * pp.cgsl;
                    qgso = (pp.weff_cv * pp.cgsl + pp.cgso_p) * vgs;
                }
            } else {
                let t0 = vgd + DELTA_1;
                let t1 = f64::sqrt(t0 * t0 + 4.0 * DELTA_1);
                let t2 = 0.5 * (t0 - t1);
                let t3 = pp.weff_cv * pp.cgdl;
                let t4 = f64::sqrt(1.0 - 4.0 * t2 / pp.ckappa);
                cgdo = pp.cgdo_p + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
                qgdo = (pp.cgdo_p + t3) * vgd - t3 * (t2 + 0.5 * pp.ckappa * (t4 - 1.0));

                let t0 = vgs + DELTA_1;
                let t1 = f64::sqrt(t0 * t0 + 4.0 * DELTA_1);
                let t2 = 0.5 * (t0 - t1);
                let t3 = pp.weff_cv * pp.cgsl;
                let t4 = f64::sqrt(1.0 - 4.0 * t2 / pp.ckappa);
                cgso = pp.cgso_p + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
                qgso = (pp.cgso_p + t3) * vgs - t3 * (t2 + 0.5 * pp.ckappa * (t4 - 1.0));
            }
            self.cgdo = cgdo;
            self.cgso = cgso;

            let ag0 = ctx.ag[0];
            let (capbd, capbs) = (self.capbd, self.capbs);
            let cgbo = pp.cgbo_p;
            if self.mode > 0 {
                gcggb = (ev.cggb + cgdo + cgso + cgbo) * ag0;
                gcgdb = (ev.cgdb - cgdo) * ag0;
                gcgsb = (ev.cgsb - cgso) * ag0;

                gcdgb = (ev.cdgb - cgdo) * ag0;
                gcddb = (ev.cddb + capbd + cgdo) * ag0;
                gcdsb = ev.cdsb * ag0;

                gcsgb = -(ev.cggb + ev.cbgb + ev.cdgb + cgso) * ag0;
                gcsdb = -(ev.cgdb + ev.cbdb + ev.cddb) * ag0;
                gcssb = (capbs + cgso - (ev.cgsb + ev.cbsb + ev.cdsb)) * ag0;

                gcbgb = (ev.cbgb - cgbo) * ag0;
                gcbdb = (ev.cbdb - capbd) * ag0;
                gcbsb = (ev.cbsb - capbs) * ag0;

                let qgd = qgdo;
                let qgs = qgso;
                let qgb = cgbo * vgb;
                qgate += qgd + qgs + qgb;
                qbulk -= qgb;
                qdrn -= qgd;
            } else {
                gcggb = (ev.cggb + cgdo + cgso + cgbo) * ag0;
                gcgdb = (ev.cgsb - cgdo) * ag0;
                gcgsb = (ev.cgdb - cgso) * ag0;

                gcdgb = -(ev.cggb + ev.cbgb + ev.cdgb + cgdo) * ag0;
                gcddb = (capbd + cgdo - (ev.cgsb + ev.cbsb + ev.cdsb)) * ag0;
                gcdsb = -(ev.cgdb + ev.cbdb + ev.cddb) * ag0;

                gcsgb = (ev.cdgb - cgso) * ag0;
                gcsdb = ev.cdsb * ag0;
                gcssb = (ev.cddb + capbs + cgso) * ag0;

                gcbgb = (ev.cbgb - cgbo) * ag0;
                gcbdb = (ev.cbsb - capbd) * ag0;
                gcbsb = (ev.cbdb - capbs) * ag0;

                let qgd = qgdo;
                let qgs = qgso;
                let qgb = cgbo * vgb;
                qgate += qgd + qgs + qgb;
                qbulk -= qgb;
                let qsrc = ev.qdrn - qgs;
                qdrn = -(qgate + qbulk + qsrc);
            }

            ctx.states.set(0, st + QG, qgate);
            let qbd = ctx.states.get(0, st + QBD);
            let qbs = ctx.states.get(0, st + QBS);
            ctx.states.set(0, st + QD, qdrn - qbd);
            ctx.states.set(0, st + QB, qbulk + qbd + qbs);

            // Store small-signal parameters.
            if ctx.mode == InitMode::Smsig {
                return; // line1000
            }
            // No integration if DC sweep, but keep evaluating
            // capacitances.
            if ctx.kind != AnKind::DcSweep {
                if ctx.mode == InitMode::Tran {
                    for o in [QB, QG, QD] {
                        let v = ctx.states.get(0, st + o);
                        ctx.states.set(1, st + o, v);
                    }
                }
                ctx.integrate(0.0, st + QB);
                ctx.integrate(0.0, st + QG);
                ctx.integrate(0.0, st + QD);

                // line860: equivalent charge currents.
                let cqgate = ctx.states.get(0, st + CQG);
                let cqbulk = ctx.states.get(0, st + CQB);
                let cqdrn = ctx.states.get(0, st + CQD);
                ceqqg = cqgate - gcggb * vgb + gcgdb * vbd + gcgsb * vbs;
                ceqqb = cqbulk - gcbgb * vgb + gcbdb * vbd + gcbsb * vbs;
                ceqqd = cqdrn - gcdgb * vgb + gcddb * vbd + gcdsb * vbs;
                if ctx.mode == InitMode::Tran {
                    for o in [CQB, CQG, CQD] {
                        let v = ctx.states.get(0, st + o);
                        ctx.states.set(1, st + o, v);
                    }
                }
            } else {
                // line850 for the DC sweep.
                (gcggb, gcgdb, gcgsb) = (0.0, 0.0, 0.0);
                (gcbgb, gcbdb, gcbsb) = (0.0, 0.0, 0.0);
                (gcdgb, gcddb, gcdsb) = (0.0, 0.0, 0.0);
                (gcsgb, gcsdb, gcssb) = (0.0, 0.0, 0.0);
            }
        }

        // line900: load current vector.
        let (gm, gmbs, fwd_sum, rev_sum, cdreq, mut ceqbd, mut ceqbs);
        let (gbbdp, gbbsp, gbdpg, gbdpdp, gbdpb, gbdpsp, gbspg, gbspdp, gbspb, gbspsp);
        if self.mode >= 0 {
            gm = ev.gm;
            gmbs = ev.gmbs;
            fwd_sum = gm + gmbs;
            rev_sum = 0.0;
            cdreq = typ * (cdrain - ev.gds * vds - gm * vgs - gmbs * vbs);
            ceqbd = -typ * (ev.isub - ev.gbds * vds - ev.gbgs * vgs - ev.gbbs * vbs);
            ceqbs = 0.0;

            gbbdp = -ev.gbds;
            gbbsp = ev.gbds + ev.gbgs + ev.gbbs;
            gbdpg = ev.gbgs;
            gbdpdp = ev.gbds;
            gbdpb = ev.gbbs;
            gbdpsp = -(gbdpg + gbdpdp + gbdpb);
            gbspg = 0.0;
            gbspdp = 0.0;
            gbspb = 0.0;
            gbspsp = 0.0;
        } else {
            gm = -ev.gm;
            gmbs = -ev.gmbs;
            fwd_sum = 0.0;
            rev_sum = -(gm + gmbs);
            cdreq = -typ * (cdrain + ev.gds * vds + gm * vgd + gmbs * vbd);
            ceqbs = -typ * (ev.isub + ev.gbds * vds - ev.gbgs * vgd - ev.gbbs * vbd);
            ceqbd = 0.0;

            gbbsp = -ev.gbds;
            gbbdp = ev.gbds + ev.gbgs + ev.gbbs;
            gbdpg = 0.0;
            gbdpsp = 0.0;
            gbdpb = 0.0;
            gbdpdp = 0.0;
            gbspg = ev.gbgs;
            gbspsp = ev.gbds;
            gbspb = ev.gbbs;
            gbspdp = -(gbspg + gbspsp + gbspb);
        }

        if typ > 0.0 {
            ceqbs += self.cbs - self.gbs * vbs;
            ceqbd += self.cbd - self.gbd * vbd;
        } else {
            ceqbs -= self.cbs - self.gbs * vbs;
            ceqbd -= self.cbd - self.gbd * vbd;
            ceqqg = -ceqqg;
            ceqqb = -ceqqb;
            ceqqd = -ceqqd;
        }

        let m = self.g.m;
        ctx.add_rhs(self.gate, -(m * ceqqg));
        ctx.add_rhs(self.bulk, -(m * (ceqbs + ceqbd + ceqqb)));
        ctx.add_rhs(self.d_p, m * (ceqbd - cdreq - ceqqd));
        ctx.add_rhs(self.s_p, m * (cdreq + ceqbs + ceqqg + ceqqb + ceqqd));

        // Load y matrix (the NQS `T1·ddxpart` terms are zero).
        let gdpr = self.drain_conductance;
        let gspr = self.source_conductance;
        let (gbd, gbs, gds) = (self.gbd, self.gbs, ev.gds);
        ctx.add(self.s[D_D], m * gdpr);
        ctx.add(self.s[G_G], m * gcggb);
        ctx.add(self.s[S_S], m * gspr);
        ctx.add(
            self.s[B_B],
            m * (gbd + gbs - gcbgb - gcbdb - gcbsb - ev.gbbs),
        );
        ctx.add(
            self.s[DP_DP],
            m * (gdpr + gds + gbd + rev_sum + gcddb + gbdpdp),
        );
        ctx.add(
            self.s[SP_SP],
            m * (gspr + gds + gbs + fwd_sum + gcssb + gbspsp),
        );
        ctx.add(self.s[D_DP], -m * gdpr);
        ctx.add(self.s[G_B], -m * (gcggb + gcgdb + gcgsb));
        ctx.add(self.s[G_DP], m * gcgdb);
        ctx.add(self.s[G_SP], m * gcgsb);
        ctx.add(self.s[S_SP], -m * gspr);
        ctx.add(self.s[B_G], m * (gcbgb - ev.gbgs));
        ctx.add(self.s[B_DP], m * (gcbdb - gbd + gbbdp));
        ctx.add(self.s[B_SP], m * (gcbsb - gbs + gbbsp));
        ctx.add(self.s[DP_D], -m * gdpr);
        ctx.add(self.s[DP_G], m * (gm + gcdgb + gbdpg));
        ctx.add(
            self.s[DP_B],
            -m * (gbd - gmbs + gcdgb + gcddb + gcdsb - gbdpb),
        );
        ctx.add(self.s[DP_SP], -m * (gds + fwd_sum - gcdsb - gbdpsp));
        ctx.add(self.s[SP_G], m * (gcsgb - gm + gbspg));
        ctx.add(self.s[SP_S], -m * gspr);
        ctx.add(
            self.s[SP_B],
            -m * (gbs + gmbs + gcsgb + gcsdb + gcssb - gbspb),
        );
        ctx.add(self.s[SP_DP], -m * (gds + rev_sum - gcsdb - gbspdp));
    }

    /// `BSIM3trunc` (`b3trunc.c`).
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QB, t, time_step);
        crate::integ::terr(self.st + QG, t, time_step);
        crate::integ::terr(self.st + QD, t, time_step);
    }

    /// `BSIM3acLoad` (`b3acld.c`), quasi-static branch.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let (caps, gs) = self.small_signal();
        let m = self.g.m;
        for (k, c) in caps {
            ctx.add_b(self.s[k], m * (c * ctx.omega));
        }
        for (k, g) in gs {
            ctx.add_g(self.s[k], m * g);
        }
    }

    /// `BSIM3pzLoad` (`b3pzld.c`): the AC stamps with `s` in place
    /// of `jω`.
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        let (caps, gs) = self.small_signal();
        let m = self.g.m;
        for (k, c) in caps {
            ctx.add_cap(self.s[k], m * c);
        }
        for (k, g) in gs {
            ctx.add_g(self.s[k], m * g);
        }
        Ok(())
    }

    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(BSIM3_NOISE_SUFFIXES)
    }

    fn noise_sep(&self) -> &'static str {
        "."
    }

    /// `BSIM3noise` (`b3noi.c`), `N_DENS`: series-resistance thermal
    /// noise, channel thermal noise and flicker noise in the form
    /// `noiMod` selects (1, the default: SPICE2 thermal
    /// `4kT·(2/3)|gm+gds+gmbs|` and SPICE2 flicker
    /// `KF·|Id|^AF/(f^EF·Leff²·Cox)`).
    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let model = &self.model;
        let pp = &self.pp;
        let e = &self.ev;
        let m = self.g.m;
        let rd = ctx.thermal(self.d_p, self.drain, self.drain_conductance * m);
        let rs = ctx.thermal(self.s_p, self.source, self.source_conductance * m);
        let gsum = (e.gm + e.gds + e.gmbs).abs();
        let id = match model.noi_mod {
            1 | 3 => ctx.thermal(self.d_p, self.s_p, 2.0 * gsum / 3.0 * m),
            5 | 6 => {
                let vds = f64::min(ctx.states.get(0, self.st + VDS), e.vdsat);
                ctx.thermal(self.d_p, self.s_p, (3.0 - vds / e.vdsat) * gsum / 3.0 * m)
            }
            2 | 4 => ctx.thermal(
                self.d_p,
                self.s_p,
                m * e.ueff * self.qinv.abs()
                    / (pp.leff * pp.leff + e.ueff * self.qinv.abs() * e.rds),
            ),
            _ => 0.0,
        };
        let gain = ctx.gain2(self.d_p, self.s_p);
        let flicker = match model.noi_mod {
            1 | 4 | 5 => {
                gain * m
                    * model.kf
                    * f64::exp(model.af * f64::ln(f64::max(self.cd.abs(), N_MINLOG)))
                    / (ctx.freq.powf(model.ef) * pp.leff * pp.leff * model.cox)
            }
            2 | 3 | 6 => {
                let vds = ctx.states.get(0, self.st + VDS).abs();
                let ssi = self.strong_inversion_noise(vds, ctx.freq, ctx.temp);
                let t10 = model.oxide_trap_density_a * 8.62e-5 * ctx.temp;
                let t11 = pp.weff * pp.leff * ctx.freq.powf(model.ef) * 4.0e36;
                let swi = t10 / t11 * self.cd * self.cd;
                let t1 = swi + ssi;
                if t1 > 0.0 {
                    gain * m * (ssi * swi) / t1
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        vec![rd, rs, id, flicker, rd + rs + id + flicker]
    }
}

/// `N_MINLOG` (`noisedef.h`).
const N_MINLOG: f64 = 1e-38;

impl Bsim3I {
    /// The quasi-static small-signal matrix shared by `BSIM3acLoad`
    /// and `BSIM3pzLoad`: the capacitance entries (to be scaled by
    /// `jω` or `s`) and the conductance entries, per slot, before
    /// the `M` multiplier.
    #[allow(clippy::type_complexity)]
    fn small_signal(&self) -> ([(usize, f64); 16], [(usize, f64); 18]) {
        let e = &self.ev;
        let (gmr, gmbsr, gds) = (e.gm, e.gmbs, e.gds);
        let (cddr, cdgr, cdsr) = (e.cddb, e.cdgb, e.cdsb);
        let (cgdr, cggr, cgsr) = (e.cgdb, e.cggb, e.cgsb);

        let (gm, gmbs, fwd_sum, rev_sum);
        let (gbbdp, gbbsp, gbdpg, gbdpb, gbdpdp, gbdpsp, gbspdp, gbspg, gbspb, gbspsp);
        let (cggb, cgsb, cgdb, cbgb, cbsb, cbdb, cdgb, cdsb, cddb);
        if self.mode >= 0 {
            gm = gmr;
            gmbs = gmbsr;
            fwd_sum = gm + gmbs;
            rev_sum = 0.0;
            gbbdp = -e.gbds;
            gbbsp = e.gbds + e.gbgs + e.gbbs;
            gbdpg = e.gbgs;
            gbdpb = e.gbbs;
            gbdpdp = e.gbds;
            gbdpsp = -(gbdpg + gbdpb + gbdpdp);
            gbspdp = 0.0;
            gbspg = 0.0;
            gbspb = 0.0;
            gbspsp = 0.0;
            cggb = cggr;
            cgsb = cgsr;
            cgdb = cgdr;
            cbgb = e.cbgb;
            cbsb = e.cbsb;
            cbdb = e.cbdb;
            cdgb = cdgr;
            cdsb = cdsr;
            cddb = cddr;
        } else {
            gm = -gmr;
            gmbs = -gmbsr;
            fwd_sum = 0.0;
            rev_sum = -(gm + gmbs);
            gbbsp = -e.gbds;
            gbbdp = e.gbds + e.gbgs + e.gbbs;
            gbdpg = 0.0;
            gbdpsp = 0.0;
            gbdpb = 0.0;
            gbdpdp = 0.0;
            gbspg = e.gbgs;
            gbspsp = e.gbds;
            gbspb = e.gbbs;
            gbspdp = -(gbspg + gbspsp + gbspb);
            cggb = cggr;
            cgsb = cgdr;
            cgdb = cgsr;
            cbgb = e.cbgb;
            cbsb = e.cbdb;
            cbdb = e.cbsb;
            cdgb = -(cdgr + cggb + cbgb);
            cdsb = -(cddr + cgsb + cbsb);
            cddb = -(cdsr + cgdb + cbdb);
        }

        let gdpr = self.drain_conductance;
        let gspr = self.source_conductance;
        let (gbd, gbs) = (self.gbd, self.gbs);
        let (capbd, capbs) = (self.capbd, self.capbs);
        let gs_overlap = self.cgso;
        let gd_overlap = self.cgdo;
        let gb_overlap = self.pp.cgbo_p;
        let xcdgb = cdgb - gd_overlap;
        let xcddb = cddb + capbd + gd_overlap;
        let xcdsb = cdsb;
        let xcsgb = -(cggb + cbgb + cdgb + gs_overlap);
        let xcsdb = -(cgdb + cbdb + cddb);
        let xcssb = capbs + gs_overlap - (cgsb + cbsb + cdsb);
        let xcggb = cggb + gd_overlap + gs_overlap + gb_overlap;
        let xcgdb = cgdb - gd_overlap;
        let xcgsb = cgsb - gs_overlap;
        let xcbgb = cbgb - gb_overlap;
        let xcbdb = cbdb - capbd;
        let xcbsb = cbsb - capbs;

        let caps = [
            (G_G, xcggb),
            (B_B, -(xcbgb + xcbdb + xcbsb)),
            (DP_DP, xcddb),
            (SP_SP, xcssb),
            (G_B, -(xcggb + xcgdb + xcgsb)),
            (G_DP, xcgdb),
            (G_SP, xcgsb),
            (B_G, xcbgb),
            (B_DP, xcbdb),
            (B_SP, xcbsb),
            (DP_G, xcdgb),
            (DP_B, -(xcdgb + xcddb + xcdsb)),
            (DP_SP, xcdsb),
            (SP_G, xcsgb),
            (SP_B, -(xcsgb + xcsdb + xcssb)),
            (SP_DP, xcsdb),
        ];
        let gs = [
            (D_D, gdpr),
            (S_S, gspr),
            (B_B, gbd + gbs - e.gbbs),
            (DP_DP, gdpr + gds + gbd + rev_sum + gbdpdp),
            (SP_SP, gspr + gds + gbs + fwd_sum + gbspsp),
            (D_DP, -gdpr),
            (S_SP, -gspr),
            (B_G, -e.gbgs),
            (B_DP, -(gbd - gbbdp)),
            (B_SP, -(gbs - gbbsp)),
            (DP_D, -gdpr),
            (DP_G, gm + gbdpg),
            (DP_B, -(gbd - gmbs - gbdpb)),
            (DP_SP, -(gds + fwd_sum - gbdpsp)),
            (SP_G, -(gm - gbspg)),
            (SP_S, -gspr),
            (SP_B, -(gbs + gmbs - gbspb)),
            (SP_DP, -(gds + rev_sum - gbspdp)),
        ];
        (caps, gs)
    }

    /// `StrongInversionNoiseEval` (`b3noi.c`), the BSIM3 flicker
    /// model of `noiMod` 2/3/6.
    fn strong_inversion_noise(&self, vds: f64, freq: f64, temp: f64) -> f64 {
        let model = &self.model;
        let pp = &self.pp;
        let e = &self.ev;
        let charge = crate::consts::CHARGE;
        let cd = self.cd.abs();
        let leff = pp.leff - 2.0 * model.lintnoi;
        let leffsq = leff * leff;
        let esat = 2.0 * pp.vsattemp / e.ueff;
        let del_clm = if model.em <= 0.0 {
            0.0
        } else {
            let t0 = ((vds - e.vdseff) / pp.litl + model.em) / esat;
            let d = pp.litl * f64::ln(f64::max(t0, N_MINLOG));
            if d < 0.0 { 0.0 } else { d }
        };
        let eff_freq = freq.powf(model.ef);
        let t1 = charge * charge * 8.62e-5 * cd * temp * e.ueff;
        let t2 = 1.0e8 * eff_freq * e.abulk * model.cox * leffsq;
        let n0 = model.cox * e.vgsteff / charge;
        let nl = model.cox * e.vgsteff * (1.0 - e.abov_vgst2vtm * e.vdseff) / charge;
        let t3 =
            model.oxide_trap_density_a * f64::ln(f64::max((n0 + 2.0e14) / (nl + 2.0e14), N_MINLOG));
        let t4 = model.oxide_trap_density_b * (n0 - nl);
        let t5 = model.oxide_trap_density_c * 0.5 * (n0 * n0 - nl * nl);
        let t6 = 8.62e-5 * temp * cd * cd;
        let t7 = 1.0e8 * eff_freq * leffsq * pp.weff;
        let t8 = model.oxide_trap_density_a
            + model.oxide_trap_density_b * nl
            + model.oxide_trap_density_c * nl * nl;
        let t9 = (nl + 2.0e14) * (nl + 2.0e14);
        t1 / t2 * (t3 + t4 + t5) + t6 / t7 * del_clm * t8 / t9
    }
}

/// One model's entry in the build-time knot list: the card as given,
/// its `BSIM3setup`/`BSIM3temp` model part, and the
/// `bsim3SizeDependParam` knots built so far (`pSizeDependParamKnot`,
/// searched by exact drawn L and W like the C).
pub(crate) struct Bsim3Knots {
    given: Bsim3Model,
    model: Arc<Bsim3Model>,
    mt: Arc<Bsim3MTemp>,
    knots: Vec<Arc<Bsim3Size>>,
}

impl Bsim3Knots {
    /// Find or build the set-up model, its temperature part and the
    /// size knot for (L, W).
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn resolve(
        list: &mut Vec<Bsim3Knots>,
        name: &str,
        given: &Bsim3Model,
        l: f64,
        w: f64,
        temp: f64,
        tnom: f64,
        notes: &mut Vec<String>,
    ) -> Result<(Arc<Bsim3Model>, Arc<Bsim3MTemp>, Arc<Bsim3Size>), String> {
        let idx = match list.iter().position(|k| k.given == *given) {
            Some(i) => i,
            None => {
                let mut m = given.clone();
                m.setup(tnom);
                let mut mnotes = Vec::new();
                let mt = Bsim3MTemp::new(&mut m, temp, &mut mnotes);
                notes.extend(mnotes.into_iter().map(|n| format!("BSIM3: {n}")));
                list.push(Bsim3Knots {
                    given: given.clone(),
                    model: Arc::new(m),
                    mt: Arc::new(mt),
                    knots: Vec::new(),
                });
                list.len() - 1
            }
        };
        let k = &mut list[idx];
        let pp = match k.knots.iter().find(|p| p.length == l && p.width == w) {
            Some(p) => Arc::clone(p),
            None => {
                let mut snotes = Vec::new();
                let p = Arc::new(Bsim3Size::new(name, &k.model, &k.mt, l, w, &mut snotes)?);
                notes.extend(snotes.into_iter().map(|n| format!("`{name}`: {n}")));
                k.knots.push(Arc::clone(&p));
                p
            }
        };
        Ok((Arc::clone(&k.model), Arc::clone(&k.mt), pp))
    }
}

#[cfg(test)]
mod tests;
