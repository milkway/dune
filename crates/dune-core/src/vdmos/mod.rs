//! VDMOS — ngspice-47's vertical power MOSFET
//! (`spicelib/devices/vdmos/`: `vdmosload.c`, `vdmostemp.c`,
//! `vdmosset.c`, `vdmosmpar.c`, `vdmospar.c`, `vdmosacld.c`,
//! `vdmospzld.c`, `vdmosnoi.c`, `vdmosdset.c`, `vdmosdist.c`,
//! `vdmosconv.c`, `vdmostrun.c`, `vdmosic.c`). The 3f5 has no VDMOS,
//! so ngspice-47 is both the contract and the oracle.
//!
//! Upstream authorship (Modified BSD, as the rest of ngspice):
//! Copyright 1990 Regents of the University of California; Author:
//! 1985 Thomas L. Quarles; Modified: 2000 AlansFixes; VDMOS: 2018
//! Holger Vogt, 2020 Dietmar Warning. Distortion setup: 1988 Jaijeet
//! S Roychowdhury.
//!
//! A three-terminal device (drain, gate, source) built on the MOS1
//! load: a square-law channel with weak-inversion blending and
//! quasi-saturation, the `Cgd(vgd)`/`Cgs` gate charges, a body diode
//! from source to drain (with breakdown, depletion and diffusion
//! charge and optional soft reverse recovery through an extra `qp`
//! node), and optional self-heating (`thermal` flag with a junction
//! and a case temperature node, a thermal RC network to an internal
//! node held at the ambient temperature by a branch equation).
//! [`model`] holds the card, its defaults and the temperature update;
//! this file the instance, its loads and the convergence test. Notes
//! on the port: `docs/vdmos.md`.

mod model;
#[cfg(test)]
mod tests;

use model::{OXIDE_CAP_FACTOR, VdmosTemp};
pub use model::{VdmosModel, VdmosSoa};

use crate::disto::{DfCoeffs, DistoCtx, DistoSetupCtx, Kern, d1_term, df_term, jomega};
use crate::matrix::{AcCtx, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::MOS_NOISE_SUFFIXES;

// State offsets (`vdmosdefs.h`).
const VGS: usize = 0;
const VDS: usize = 1;
const DELTEMP: usize = 2;
const CAPGS: usize = 3;
const QGS: usize = 4;
const CQGS: usize = 5;
const CAPGD: usize = 6;
const QGD: usize = 7;
const CQGD: usize = 8;
const DVOLT: usize = 9;
const DCURRENT: usize = 10;
const DCONDUCT: usize = 11;
const DCAPCHARGE: usize = 12;
const DCAPCURRENT: usize = 13;
const CAPTH: usize = 14;
const QTH: usize = 15;
const CQTH: usize = 16;
const DIDIO_DT: usize = 17;
const SRCAPCHARGE: usize = 18;
const SRCAPCURRENT: usize = 19;
const DQP: usize = 20;
const RESCURRENT: usize = 21;
const RESCONDUCT: usize = 22;
const CQCSR: usize = 23;
const GQCSR: usize = 24;
/// `VDMOSnumStates`.
pub(crate) const VDMOS_NUM_STATES: usize = 25;

// Matrix slots in `VDMOSsetup`'s TSTALLOC order.
const D_D: usize = 0;
const G_G: usize = 1;
const S_S: usize = 2;
const DP_DP: usize = 3;
const SP_SP: usize = 4;
const GP_GP: usize = 5;
const D_DP: usize = 6;
const GP_DP: usize = 7;
const GP_SP: usize = 8;
const S_SP: usize = 9;
const DP_SP: usize = 10;
const DP_D: usize = 11;
const DP_GP: usize = 12;
const SP_GP: usize = 13;
const SP_S: usize = 14;
const SP_DP: usize = 15;
const G_GP: usize = 16;
const GP_G: usize = 17;
const D_S: usize = 18;
const S_D: usize = 19;
const RP_D: usize = 20;
const D_RP: usize = 21;
const S_RP: usize = 22;
const RP_S: usize = 23;
const RP_RP: usize = 24;
// Self-heating.
const T_T: usize = 25;
const T_DP: usize = 26;
const T_SP: usize = 27;
const T_GP: usize = 28;
/// Allocated by the C, stamped only by the (zero) `xcgT` of the AC load.
#[allow(dead_code)]
const GP_T: usize = 29;
const DP_T: usize = 30;
const SP_T: usize = 31;
const T_RP: usize = 32;
const T_D: usize = 33;
const RP_T: usize = 34;
const D_T: usize = 35;
const T_S: usize = 36;
const S_T: usize = 37;
const TC_TC: usize = 38;
const TC_T: usize = 39;
const T_TC: usize = 40;
const TP_TP: usize = 41;
const TP_TC: usize = 42;
const TC_TP: usize = 43;
/// Allocated by the C, never stamped.
#[allow(dead_code)]
const BR_BR: usize = 44;
const BR_TP: usize = 45;
const TP_BR: usize = 46;
// Soft reverse recovery.
const QP_QP: usize = 47;
const QP_RP: usize = 48;
const QP_D: usize = 49;
const RP_QP: usize = 50;
const D_QP: usize = 51;
// Dune's pins of temperature nodes a non-self-heating instance names.
const PIN0: usize = 52;
const NUM_SLOTS: usize = 54;

/// The unknowns of one VDMOS instance.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct VdmosVars {
    pub d: Var,
    pub g: Var,
    pub s: Var,
    /// Internal drain/gate/source (the external node when `RD`/`RG`/
    /// `RS` is 0).
    pub dp: Var,
    pub gp: Var,
    pub sp: Var,
    /// `VDIOposPrimeNode`: the body-diode anode past `RB` (the
    /// source when `RB` is 0).
    pub rp: Var,
    /// `VDIOqpNode` (0 without soft recovery).
    pub qp: Var,
    /// Junction and case temperature nodes, the internal node at the
    /// ambient temperature and its branch (0 without self-heating).
    pub tj: Var,
    pub tc: Var,
    pub tp: Var,
    pub br: Var,
    /// Temperature nodes named on the card of an instance without
    /// self-heating: nothing in the C stamps them, so its sparse
    /// matrix never sees them and they read 0; Dune pins them to 0
    /// with a unit diagonal (0 = none).
    pub pins: [Var; 2],
}

/// One VDMOS instance (`VDMOSinstance`).
pub(crate) struct VdmosI {
    v: VdmosVars,
    /// The prepared model (`VdmosModel::prepare`).
    p: VdmosModel,
    name: String,
    m: f64,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_given: crate::devices::IcGiven,
    selfheat: bool,
    /// The instance temperature (kelvin) and whether `TEMP=` was the
    /// source of it (`VDMOStempGiven`, read by the ambient source of
    /// the thermal network).
    temp: f64,
    temp_given: bool,
    /// `CKTtemp`, kelvin.
    ckt_temp: f64,
    reltol: f64,
    st: usize,
    s: [Slot; NUM_SLOTS],
    t: VdmosTemp,
    /// `VDMOSdsConductance`.
    ds_conductance: f64,
    /// `VDMOSdrainConductance`: set by the setup, then by every load
    /// (temperature and quasi-saturation).
    drain_conductance: f64,
    // Values that persist between iterations.
    mode: f64,
    von: f64,
    vdsat: f64,
    cd: f64,
    gm: f64,
    gds: f64,
    gmt: f64,
    gtempg: f64,
    gtempd: f64,
    gtemp_t: f64,
    /// `VDMOSTempSH`: the device temperature of the last load.
    temp_sh: f64,
    /// `.disto` Taylor coefficients (`VDMOSdSetup`): `cxx`, `cyy`,
    /// `cxy`, `cxxx`, `cyyy`, `cxxy`, `cxyy` of the drain current in
    /// x = vgs, y = vds, and the gate capacitance terms.
    dist: DfCoeffs,
    capgs2: f64,
    capgs3: f64,
    capgd2: f64,
    capgd3: f64,
}

/// What [`VdmosI::new`] needs besides the unknowns.
pub(crate) struct VdmosSetup {
    pub name: String,
    pub model: VdmosModel,
    pub m: f64,
    pub off: bool,
    pub ic: (f64, f64),
    pub thermal: bool,
    pub temp: f64,
    pub temp_given: bool,
    pub ckt_temp: f64,
    pub default_tnom: f64,
    pub reltol: f64,
}

impl VdmosI {
    /// `VDMOSsetup` + `VDMOStemp` for one instance.
    pub(crate) fn new(
        v: VdmosVars,
        su: VdmosSetup,
        st: usize,
        notes: &mut Vec<String>,
    ) -> Result<Self, String> {
        let p = su.model.prepare(&su.name, su.default_tnom, notes)?;
        let m = su.m;
        let ds_conductance = match p.rds {
            Some(rds) if rds > 0.0 => m / rds,
            _ => 1e-15,
        };
        let drain_conductance = if p.rd > 0.0 { m / p.rd } else { 0.0 };
        let t = VdmosTemp::new(&p, m, su.temp, su.reltol, notes, &su.name);
        let selfheat = su.thermal && p.rthjc.is_some();
        Ok(VdmosI {
            v,
            p,
            name: su.name,
            m,
            off: su.off,
            ic_vds: su.ic.0,
            ic_vgs: su.ic.1,
            ic_given: crate::devices::IcGiven::ALL,
            selfheat,
            temp: su.temp,
            temp_given: su.temp_given,
            ckt_temp: su.ckt_temp,
            reltol: su.reltol,
            st,
            s: [TRASH; NUM_SLOTS],
            t,
            ds_conductance,
            drain_conductance,
            mode: 1.0,
            von: 0.0,
            vdsat: 0.0,
            cd: 0.0,
            gm: 0.0,
            gds: 0.0,
            gmt: 0.0,
            gtempg: 0.0,
            gtempd: 0.0,
            gtemp_t: 0.0,
            temp_sh: su.temp,
            dist: DfCoeffs::default(),
            capgs2: 0.0,
            capgs3: 0.0,
            capgd2: 0.0,
            capgd3: 0.0,
        })
    }

    /// Soft reverse recovery is active (`revrec` in `vdmosload.c`).
    fn revrec(&self) -> bool {
        self.v.qp != 0 && self.p.vp != 0.0 && self.t.t_transit_time != 0.0
    }

    /// `ctx.states.get` shorthand.
    fn sget(ctx: &LoadCtx, bank: usize, idx: usize) -> f64 {
        ctx.states.get(bank, idx)
    }
}

mod load;

impl crate::devices::Device for VdmosI {
    fn par_load(&self) -> bool {
        true
    }

    /// `VDMOSgetic` (`vdmosic.c`): VDS and VGS not given on the card
    /// from the external nodes.
    fn set_ic(&mut self, rhs: &[f64]) {
        use crate::devices::getic;
        let g = self.ic_given;
        getic(g, 0, &mut self.ic_vds, rhs, self.v.d, self.v.s);
        getic(g, 1, &mut self.ic_vgs, rhs, self.v.g, self.v.s);
    }

    fn set_ic_given(&mut self, given: crate::devices::IcGiven) {
        self.ic_given = given;
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let v = self.v;
        let (d, g, s, dp, gp, sp, rp) = (v.d, v.g, v.s, v.dp, v.gp, v.sp, v.rp);
        let mut sl = [TRASH; NUM_SLOTS];
        let base = [
            (d, d),
            (g, g),
            (s, s),
            (dp, dp),
            (sp, sp),
            (gp, gp),
            (d, dp),
            (gp, dp),
            (gp, sp),
            (s, sp),
            (dp, sp),
            (dp, d),
            (dp, gp),
            (sp, gp),
            (sp, s),
            (sp, dp),
            (g, gp),
            (gp, g),
            (d, s),
            (s, d),
            (rp, d),
            (d, rp),
            (s, rp),
            (rp, s),
            (rp, rp),
        ];
        for (k, (r, c)) in base.iter().enumerate() {
            sl[k] = mb.slot(*r, *c);
        }
        if self.selfheat {
            let (t, tc, tp, br) = (v.tj, v.tc, v.tp, v.br);
            let th = [
                (t, t),
                (t, dp),
                (t, sp),
                (t, gp),
                (gp, t),
                (dp, t),
                (sp, t),
                (t, rp),
                (t, d),
                (rp, t),
                (d, t),
                (t, s),
                (s, t),
                (tc, tc),
                (tc, t),
                (t, tc),
                (tp, tp),
                (tp, tc),
                (tc, tp),
                (br, br),
                (br, tp),
                (tp, br),
            ];
            for (k, (r, c)) in th.iter().enumerate() {
                sl[T_T + k] = mb.slot(*r, *c);
            }
        }
        if self.revrec() {
            let qp = v.qp;
            for (k, (r, c)) in [(qp, qp), (qp, rp), (qp, d), (rp, qp), (d, qp)]
                .iter()
                .enumerate()
            {
                sl[QP_QP + k] = mb.slot(*r, *c);
            }
        }
        for (k, &n) in v.pins.iter().enumerate() {
            if n != 0 {
                sl[PIN0 + k] = mb.slot(n, n);
            }
        }
        self.s = sl;
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        self.do_load(ctx);
    }

    /// `VDMOStrunc`.
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QGS, t, time_step);
        crate::integ::terr(self.st + QGD, t, time_step);
        crate::integ::terr(self.st + DCAPCHARGE, t, time_step);
        if self.p.vp != 0.0 && self.t.t_transit_time != 0.0 {
            crate::integ::terr(self.st + SRCAPCHARGE, t, time_step);
        }
    }

    /// `VDMOSconvTest` (`vdmosconv.c`). The channel test reads the
    /// fresh solution; its `cdhat` subtracts `gm·Δvgs` in normal mode
    /// where the load's adds it — kept as in the C. The body-diode
    /// test reads `rhsOld`, like the C.
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.p.typ();
        let st = self.st;
        let v = &self.v;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let vgs = typ * (ctx.rhs[v.gp] - ctx.rhs[v.sp]);
        let vds = typ * (ctx.rhs[v.dp] - ctx.rhs[v.sp]);
        let vgd = vgs - vds;
        let vgdo = s0(VGS) - s0(VDS);
        let delvgs = vgs - s0(VGS);
        let delvds = vds - s0(VDS);
        let delvgd = vgd - vgdo;
        let deldel_temp = if self.selfheat {
            ctx.rhs[v.tj] - s0(DELTEMP)
        } else {
            0.0
        };
        let cdhat = if self.mode >= 0.0 {
            self.cd - self.gm * delvgs + self.gds * delvds + self.gmt * deldel_temp
        } else {
            self.cd - self.gm * delvgd + self.gds * delvds + self.gmt * deldel_temp
        };
        let tol = ctx.reltol * f64::max(cdhat.abs(), self.cd.abs()) + ctx.abstol;
        if (cdhat - self.cd).abs() >= tol {
            return false;
        }
        let vd = typ * (ctx.sol[v.rp] - ctx.sol[v.d]);
        let delvd = vd - s0(DVOLT);
        let cdhat = s0(DCURRENT) + s0(DCONDUCT) * delvd + s0(DIDIO_DT) * deldel_temp;
        let cd = s0(DCURRENT);
        let tol = ctx.reltol * f64::max(cdhat.abs(), cd.abs()) + ctx.abstol;
        (cdhat - cd).abs() <= tol
    }

    /// `VDMOSacLoad` (`vdmosacld.c`), self-heating stamps included as
    /// the C has them (see `docs/vdmos.md`).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let st = self.st;
        let typ = self.p.typ();
        let s = &self.s;
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        // `VDMOScgT`/`VDMOScdT` are never written in the C: zero.
        let (gmt, c_tt, gttg, gttdp, gtt_t, gttsp) = if self.mode >= 0.0 {
            let gttg = self.gtempg;
            let gttdp = self.gtempd;
            (
                typ * self.gmt,
                self.m * self.p.cthj,
                gttg,
                gttdp,
                self.gtemp_t,
                -(gttg + gttdp),
            )
        } else {
            let gttg = -self.gtempg;
            let gttdp = -self.gtempd;
            (
                -typ * self.gmt,
                -self.m * self.p.cthj,
                gttg,
                gttdp,
                -self.gtemp_t,
                gttg + gttdp,
            )
        };
        let capgs = ctx.states.get(0, st + CAPGS) + ctx.states.get(0, st + CAPGS);
        let capgd = ctx.states.get(0, st + CAPGD) + ctx.states.get(0, st + CAPGD);
        let xgs = capgs * ctx.omega;
        let xgd = capgd * ctx.omega;
        let xc_tt = c_tt * ctx.omega;

        let gspr = self.t.t_conductance;
        let geq = ctx.states.get(0, st + DCONDUCT);
        let xceq = ctx.states.get(0, st + DCAPCURRENT) * ctx.omega;

        let (gdrain, gsource, ggate, gdss) = (
            self.drain_conductance,
            self.t.source_conductance,
            self.t.gate_conductance,
            self.ds_conductance,
        );
        let (gm, gds) = (self.gm, self.gds);

        ctx.add_b(s[GP_GP], xgd + xgs);
        ctx.add_b(s[DP_DP], xgd);
        ctx.add_b(s[SP_SP], xgs);
        ctx.add_b(s[GP_DP], -xgd);
        ctx.add_b(s[GP_SP], -xgs);
        ctx.add_b(s[DP_GP], -xgd);
        ctx.add_b(s[SP_GP], -xgs);

        ctx.add_g(s[D_D], gdrain + gdss);
        ctx.add_g(s[S_S], gsource + gdss);
        ctx.add_g(s[DP_DP], gdrain + gds + xrev * gm);
        ctx.add_g(s[SP_SP], gsource + gds + xnrm * gm);
        ctx.add_g(s[D_DP], -gdrain);
        ctx.add_g(s[S_SP], -gsource);
        ctx.add_g(s[DP_D], -gdrain);
        ctx.add_g(s[DP_GP], (xnrm - xrev) * gm);
        ctx.add_g(s[DP_SP], -(gds + xnrm * gm));
        ctx.add_g(s[SP_GP], -((xnrm - xrev) * gm));
        ctx.add_g(s[SP_S], -gsource);
        ctx.add_g(s[SP_DP], -(gds + xrev * gm));
        ctx.add_g(s[D_S], -gdss);
        ctx.add_g(s[S_D], -gdss);
        ctx.add_g(s[G_G], ggate);
        ctx.add_g(s[GP_GP], ggate);
        ctx.add_g(s[G_GP], -ggate);
        ctx.add_g(s[GP_G], -ggate);
        ctx.add_g(s[S_S], gspr);
        ctx.add_g(s[D_D], geq);
        ctx.add_b(s[D_D], xceq);
        ctx.add_g(s[RP_RP], geq + gspr);
        ctx.add_b(s[RP_RP], xceq);
        ctx.add_g(s[S_RP], -gspr);
        ctx.add_g(s[D_RP], -geq);
        ctx.add_b(s[D_RP], -xceq);
        ctx.add_g(s[RP_S], -gspr);
        ctx.add_g(s[RP_D], -geq);
        ctx.add_b(s[RP_D], -xceq);
        if self.selfheat {
            let gthjc = self.m / self.p.rthjc.unwrap_or(1.0);
            let gthca = self.m / self.p.rthca;
            ctx.add_g(s[DP_T], gmt);
            ctx.add_g(s[SP_T], -gmt);
            ctx.add_g(s[T_T], gtt_t + gthjc);
            ctx.add_g(s[T_GP], gttg);
            ctx.add_g(s[T_DP], gttdp);
            ctx.add_g(s[T_SP], gttsp);
            ctx.add_g(s[T_TC], -gthjc);
            ctx.add_g(s[TC_T], -gthjc);
            ctx.add_g(s[TC_TC], gthjc + gthca);
            ctx.add_g(s[TP_TP], gthca);
            ctx.add_g(s[TP_TC], -gthca);
            ctx.add_g(s[TC_TP], -gthca);
            ctx.add_g(s[BR_TP], 1.0);
            ctx.add_g(s[TP_BR], 1.0);
            // `xcdT`, `xcsT`, `xcgT` (into DP_T, SP_T, GP_T) are zero.
            ctx.add_b(s[T_T], xc_tt);
        }
        if self.revrec() {
            let gdres = ctx.states.get(0, st + RESCONDUCT);
            let fac = self.t.t_transit_time / self.p.vp;
            let dcrrdvd = fac * gdres;
            ctx.add_g(s[QP_QP], 1.0 / self.p.vp);
            ctx.add_b(s[QP_QP], self.t.t_transit_time * ctx.omega);
            ctx.add_g(s[QP_RP], -dcrrdvd);
            ctx.add_g(s[QP_D], dcrrdvd);
            let xgain = (1.0 - self.p.vp) * ctx.omega;
            ctx.add_b(s[RP_QP], xgain);
            ctx.add_b(s[D_QP], -xgain);
        }
        for k in 0..2 {
            if self.v.pins[k] != 0 {
                ctx.add_g(s[PIN0 + k], 1.0);
            }
        }
    }

    /// `VDMOSpzLoad` (`vdmospzld.c`). The C stamps neither the
    /// thermal network nor the soft-recovery node, which would leave
    /// their rows empty: refused instead.
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        if self.selfheat {
            return Err("VDMOS self-heating (vdmospzld.c stamps no thermal network)");
        }
        if self.revrec() {
            return Err("VDMOS soft reverse recovery (vdmospzld.c stamps no qp node)");
        }
        let st = self.st;
        let s = &self.s;
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        let xgs = 2.0 * ctx.states.get(0, st + CAPGS);
        let xgd = 2.0 * ctx.states.get(0, st + CAPGD);
        let gspr = self.t.t_conductance;
        let geq = ctx.states.get(0, st + DCONDUCT);
        let xceq = ctx.states.get(0, st + DCAPCURRENT);
        let (gdrain, gsource, ggate, gdss) = (
            self.drain_conductance,
            self.t.source_conductance,
            self.t.gate_conductance,
            self.ds_conductance,
        );
        let (gm, gds) = (self.gm, self.gds);
        ctx.add_cap(s[GP_GP], xgd + xgs);
        ctx.add_cap(s[DP_DP], xgd);
        ctx.add_cap(s[SP_SP], xgs);
        ctx.add_cap(s[GP_DP], -xgd);
        ctx.add_cap(s[GP_SP], -xgs);
        ctx.add_cap(s[DP_GP], -xgd);
        ctx.add_cap(s[SP_GP], -xgs);
        ctx.add_g(s[D_D], gdrain + gdss);
        ctx.add_g(s[S_S], gsource + gdss);
        ctx.add_g(s[DP_DP], gdrain + gds + xrev * gm);
        ctx.add_g(s[SP_SP], gsource + gds + xnrm * gm);
        ctx.add_g(s[D_DP], -gdrain);
        ctx.add_g(s[S_SP], -gsource);
        ctx.add_g(s[DP_D], -gdrain);
        ctx.add_g(s[DP_GP], (xnrm - xrev) * gm);
        ctx.add_g(s[DP_SP], -(gds + xnrm * gm));
        ctx.add_g(s[SP_GP], -((xnrm - xrev) * gm));
        ctx.add_g(s[SP_S], -gsource);
        ctx.add_g(s[SP_DP], -(gds + xrev * gm));
        ctx.add_g(s[D_S], -gdss);
        ctx.add_g(s[S_D], -gdss);
        ctx.add_g(s[G_G], ggate);
        ctx.add_g(s[GP_GP], ggate);
        ctx.add_g(s[G_GP], -ggate);
        ctx.add_g(s[GP_G], -ggate);
        ctx.add_g(s[S_S], gspr);
        ctx.add_g(s[D_D], geq);
        ctx.add_cap(s[D_D], xceq);
        ctx.add_g(s[RP_RP], geq + gspr);
        ctx.add_cap(s[RP_RP], xceq);
        ctx.add_g(s[S_RP], -gspr);
        ctx.add_g(s[D_RP], -geq);
        ctx.add_cap(s[D_RP], -xceq);
        ctx.add_g(s[RP_S], -gspr);
        ctx.add_g(s[RP_D], -geq);
        ctx.add_cap(s[RP_D], -xceq);
        for k in 0..2 {
            if self.v.pins[k] != 0 {
                ctx.add_g(s[PIN0 + k], 1.0);
            }
        }
        Ok(())
    }

    /// `VDMOSnoise` (`vdmosnoi.c`): thermal noise of RD, RS and the
    /// channel (`(2/3)|gm|`) at `CKTtemp` — scaled by the junction to
    /// ambient temperature ratio under self-heating — and flicker
    /// `KF·|Id|^AF/(f·Cox²)` with the 100 nm oxide of the setup. The
    /// body diode and `RG` are noiseless in the C.
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(MOS_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        const N_MINLOG: f64 = 1e-38;
        let v = &self.v;
        let ratio = if self.selfheat {
            self.temp_sh / ctx.temp
        } else {
            1.0
        };
        let rd = ctx.thermal(v.dp, v.d, self.drain_conductance * ratio);
        let rs = ctx.thermal(v.sp, v.s, self.t.source_conductance * ratio);
        let id = ctx.thermal(v.dp, v.sp, (2.0 / 3.0 * self.gm.abs()) * ratio);
        let cox2 = OXIDE_CAP_FACTOR * OXIDE_CAP_FACTOR;
        let coef =
            self.p.kf * f64::exp(self.p.af * f64::ln(f64::max(self.cd.abs(), N_MINLOG))) / cox2;
        let fl = ctx.flicker(v.dp, v.sp, coef, 1.0);
        vec![rd, rs, id, fl, rd + rs + id + fl]
    }

    /// `VDMOSdSetup` (`vdmosdset.c`): MOS1-style Taylor coefficients
    /// of the drain current in (vgs, vds) and Meyer-style gate
    /// capacitance terms, as the C writes them (see `docs/vdmos.md`
    /// for what they leave out). `vgs` is read at the external gate.
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        let p = &self.p;
        let typ = p.typ();
        let sol = ctx.sol;
        let v = &self.v;
        let beta = self.t.t_transconductance;
        let oxide_cap = OXIDE_CAP_FACTOR * self.m;
        let vgs = typ * (sol[v.g] - sol[v.sp]);
        let vds = typ * (sol[v.dp] - sol[v.sp]);
        let vgd = vgs - vds;
        self.mode = if vds >= 0.0 { 1.0 } else { -1.0 };
        let mode = self.mode;
        let von = self.t.t_vth * typ;
        let mut vgst = (if mode == 1.0 { vgs } else { vgd }) - von;
        let vdsat = f64::max(vgst, 0.0);
        let slope = p.ksubthres;
        let lambda = p.lambda;
        let theta = p.theta;
        let shift = p.subshift;
        let mtr = p.mtriode;
        let vdss = vds * mtr * mode;
        let t0 = 1.0 + lambda * vds;
        let t1 = 1.0 + theta * vdsat;
        let betap = beta * t0 / t1;
        let t2 = f64::exp((vgst - shift) / slope);
        vgst = slope * f64::ln(1.0 + t2);
        let (gm2, gds2, gmds, gm3, gds3, gm2ds, gmds2);
        if vgst <= vdss {
            gm2 = betap;
            gds2 = 0.0;
            gmds = beta * lambda * vgst;
            gm3 = 0.0;
            gds3 = 0.0;
            gm2ds = beta * lambda;
            gmds2 = 0.0;
        } else {
            gm2 = 0.0;
            gds2 = 2.0 * beta * lambda * (vgst - vds * mode) - betap;
            gmds = beta * lambda * vds * mode + betap;
            gm3 = 0.0;
            gds3 = -beta * lambda * 3.0;
            gm2ds = 0.0;
            gmds2 = 2.0 * lambda * beta;
        }
        let phi = self.t.t_phi;
        let cox = oxide_cap;
        let (mut lcapgs2, mut lcapgs3, mut lcapgd2, mut lcapgd3) = (0.0, 0.0, 0.0, 0.0);
        if vgst <= 0.0 {
            lcapgs2 = cox / (3.0 * phi);
        } else if vdsat > mode * vds {
            let vddif = 2.0 * vdsat - mode * vds;
            let vddif1 = vdsat - mode * vds;
            let vddif2 = vddif * vddif;
            lcapgd2 = -vdsat * mode * vds * cox / (3.0 * vddif * vddif2);
            lcapgd3 = -mode * vds * cox * (vddif - 6.0 * vdsat) / (9.0 * vddif2 * vddif2);
            lcapgs2 = -vddif1 * mode * vds * cox / (3.0 * vddif * vddif2);
            lcapgs3 = -mode * vds * cox * (vddif - 6.0 * vddif1) / (9.0 * vddif2 * vddif2);
        }
        let mut c = DfCoeffs::default();
        if mode == 1.0 {
            c.cxx = gm2;
            c.cyy = gds2;
            c.cxy = gmds;
            c.cxxx = gm3;
            c.cyyy = gds3;
            c.cxxy = gm2ds;
            c.cxyy = gmds2;
            self.capgs2 = typ * lcapgs2;
            self.capgs3 = lcapgs3;
            self.capgd2 = typ * lcapgd2;
            self.capgd3 = lcapgd3;
        } else {
            c.cxx = -gm2;
            c.cyy = -(gm2 + gds2 + 2.0 * gmds);
            c.cxy = gm2 + gmds;
            c.cxxx = -gm3;
            c.cyyy = gm3 + gds3 + 3.0 * (gm2ds + gmds2);
            c.cxxy = gm3 + gm2ds;
            c.cxyy = -(gm3 + 2.0 * gm2ds + gmds2);
            self.capgs2 = typ * lcapgd2;
            self.capgs3 = lcapgd3;
            self.capgd2 = typ * lcapgs2;
            self.capgd3 = lcapgs3;
        }
        c.cxx *= 0.5 * typ;
        c.cyy *= 0.5 * typ;
        c.cxy *= typ;
        c.cxxx /= 6.0;
        c.cyyy /= 6.0;
        c.cxxy *= 0.5;
        c.cxyy *= 0.5;
        self.dist = c;
        Ok(())
    }

    /// `VDMOSdisto` (`vdmosdist.c`): x = vgs (external gate), y = vds;
    /// the drain current and the two gate charges. Like the MOS
    /// files, `D_2F1MF2` reads its `H2(f1,f1)` kernels from the
    /// first-order arrays (kept).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        use crate::disto::DistoMode;
        let v = &self.v;
        let mode = ctx.mode;
        let omega = ctx.omega;
        let mut kx = ctx.h.kern(mode, v.g, v.sp);
        let mut ky = ctx.h.kern(mode, v.dp, v.sp);
        if mode == DistoMode::TwoF1mF2 {
            for k in [&mut kx, &mut ky] {
                k.r2h11 = k.r1h1;
                k.i2h11 = k.i1h1;
            }
        }
        let kz = Kern::default();
        let cdrain = df_term(mode, &self.dist, &kx, &ky, &kz);
        ctx.stamp(v.dp, v.sp, cdrain);
        let capgs = jomega(omega, d1_term(mode, self.capgs2, self.capgs3, &kx));
        ctx.stamp(v.g, v.sp, capgs);
        let capgd = jomega(omega, d1_term(mode, self.capgd2, self.capgd3, &kx.sub(&ky)));
        ctx.stamp(v.g, v.dp, capgd);
    }
}
