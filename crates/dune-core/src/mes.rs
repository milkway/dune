//! MESFET — the GaAs FET model of Statz et al., from the 3f5
//! (`dev/mes/mesload.c`, `mestemp.c`, `messetup.c`, `mesacl.c`,
//! `mestrunc.c`). Drain current with the `tanh` replaced by the cubic
//! `1 − (1 − α·vds/3)³` below `3/α`, doping-tail denominator
//! `1 + B·(vgs − VTO)`, gate junction diodes at the fixed `CONSTvt0`
//! (the 3f5 MESFET has no temperature dependence), and Statz's total
//! gate charge (`qggnew`) split between G-S and G-D by the
//! four-point average around the previous timepoint.
//!
//! The bias ladder, gate diodes, stamps and AC load are the JFET's
//! (`jfet.rs`): `mesload.c` is the same code outside the drain
//! current and the charge.

use crate::consts::{ROOT2, VT0};
use crate::dderivs::Dderivs;
use crate::disto::{DfCoeffs, DistoCtx, DistoSetupCtx};
use crate::jfet::{
    CQGD, CQGS, FET3_NUM_STATES, Fet3DistoCoeffs, Fet3DrainDerivs, Fet3GateQ, Fet3Nodes, Fet3Op,
    QGD, QGS, VGD, VGS, fet3_ac, fet3_bias, fet3_cdr, fet3_charge, fet3_disto_load, fet3_finish,
    fet3_gate_diode_disto, gate_diode,
};
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Var};

pub(crate) const MES_NUM_STATES: usize = FET3_NUM_STATES;
const _: () = assert!(CQGS == QGS + 1 && CQGD == QGD + 1);

/// `.model … nmf(…)/pmf(…)` parameters, 3f5 names and the defaults of
/// `messetup.c` (the manual's `BETA` default of 1e-4 is not what the
/// code does: `MESsetup` uses 2.5e-3).
#[derive(Clone, Debug)]
pub struct MesModel {
    /// `NMF` (true) or `PMF`.
    pub nmf: bool,
    /// `VTO`/`VT0`: pinch-off voltage, default −2.
    pub vto: f64,
    /// `BETA`: transconductance parameter, A/V², default 2.5e-3.
    pub beta: f64,
    /// `B`: doping tail extending parameter, 1/V, default 0.3.
    pub b: f64,
    /// `ALPHA`: saturation voltage parameter, 1/V, default 2.
    pub alpha: f64,
    /// `LAMBDA`: channel-length modulation, 1/V, default 0.
    pub lambda: f64,
    /// `RD`/`RS`: drain/source ohmic resistance, default 0.
    pub rd: f64,
    pub rs: f64,
    /// `CGS`/`CGD`: zero-bias G-S/G-D junction capacitance, default 0.
    pub cgs: f64,
    pub cgd: f64,
    /// `PB`: gate junction potential, default 1.
    pub pb: f64,
    /// `IS`: junction saturation current, default 1e-14.
    pub is: f64,
    /// `FC`: forward-bias depletion capacitance coefficient, default
    /// 0.5 (`MEStemp` derives `f1`–`f3` from it, which no load uses).
    pub fc: f64,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
}

impl MesModel {
    pub fn new(nmf: bool) -> Self {
        MesModel {
            nmf,
            vto: -2.0,
            beta: 2.5e-3,
            b: 0.3,
            alpha: 2.0,
            lambda: 0.0,
            rd: 0.0,
            rs: 0.0,
            cgs: 0.0,
            cgd: 0.0,
            pb: 1.0,
            is: 1e-14,
            fc: 0.5,
            kf: 0.0,
            af: 1.0,
        }
    }
}

impl Default for MesModel {
    fn default() -> Self {
        MesModel::new(true)
    }
}

/// `MEStemp`: series conductances and the gate-junction `vcrit` at
/// `CONSTvt0` (model level; there is no instance block).
#[derive(Clone, Debug)]
pub(crate) struct MesTemp {
    pub type_: f64,
    pub m: MesModel,
    pub drain_conduct: f64,
    pub source_conduct: f64,
    pub vcrit: f64,
}

impl MesTemp {
    pub fn new(m: &MesModel) -> Self {
        let conduct = |r: f64| if r != 0.0 { 1.0 / r } else { 0.0 };
        MesTemp {
            type_: if m.nmf { 1.0 } else { -1.0 },
            m: m.clone(),
            drain_conduct: conduct(m.rd),
            source_conduct: conduct(m.rs),
            vcrit: VT0 * f64::ln(VT0 / (ROOT2 * m.is)),
        }
    }
}

/// Statz drain current and derivatives (`mesload.c`): `(cdrain, gm,
/// gds)`, normal and inverse mode.
pub(crate) fn mes_ids(
    vgs: f64,
    vgd: f64,
    vto: f64,
    beta: f64,
    lambda: f64,
    b: f64,
    alpha: f64,
) -> (f64, f64, f64) {
    let vds = vgs - vgd;
    if vds >= 0.0 {
        let vgst = vgs - vto;
        if vgst <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let prod = 1.0 + lambda * vds;
        let betap = beta * prod;
        let denom = 1.0 + b * vgst;
        let invdenom = 1.0 / denom;
        if vds >= 3.0 / alpha {
            // Normal mode, saturation region.
            (
                betap * vgst * vgst * invdenom,
                betap * vgst * (1.0 + denom) * invdenom * invdenom,
                lambda * beta * vgst * vgst * invdenom,
            )
        } else {
            // Normal mode, linear region.
            let afact = 1.0 - alpha * vds / 3.0;
            let lfact = 1.0 - afact * afact * afact;
            (
                betap * vgst * vgst * invdenom * lfact,
                betap * vgst * (1.0 + denom) * invdenom * invdenom * lfact,
                beta * vgst * vgst * invdenom * (alpha * afact * afact * prod + lfact * lambda),
            )
        }
    } else {
        let vgdt = vgd - vto;
        if vgdt <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let prod = 1.0 - lambda * vds;
        let betap = beta * prod;
        let denom = 1.0 + b * vgdt;
        let invdenom = 1.0 / denom;
        if -vds >= 3.0 / alpha {
            // Inverse mode, saturation region.
            let gm = -betap * vgdt * (1.0 + denom) * invdenom * invdenom;
            (
                -betap * vgdt * vgdt * invdenom,
                gm,
                lambda * beta * vgdt * vgdt * invdenom - gm,
            )
        } else {
            // Inverse mode, linear region.
            let afact = 1.0 + alpha * vds / 3.0;
            let lfact = 1.0 - afact * afact * afact;
            let gm = -betap * vgdt * (1.0 + denom) * invdenom * invdenom * lfact;
            (
                -betap * vgdt * vgdt * invdenom * lfact,
                gm,
                beta * vgdt * vgdt * invdenom * (alpha * afact * afact * prod + lfact * lambda)
                    - gm,
            )
        }
    }
}

/// `qggnew` (`mesload.c`): Statz's total gate charge at
/// `(vgs, vgd)` and its partial capacitances `(q, cgs, cgd)`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn qggnew(
    vgs: f64,
    vgd: f64,
    phib: f64,
    vcap: f64,
    vto: f64,
    cgs: f64,
    cgd: f64,
) -> (f64, f64, f64) {
    let veroot = f64::sqrt((vgs - vgd) * (vgs - vgd) + vcap * vcap);
    let veff1 = 0.5 * (vgs + vgd + veroot);
    let veff2 = veff1 - veroot;
    let del = 0.2;
    let vnroot = f64::sqrt((veff1 - vto) * (veff1 - vto) + del * del);
    let mut vnew1 = 0.5 * (veff1 + vto + vnroot);
    let vnew3 = vnew1;
    let vmax = 0.5;
    let ext;
    if vnew1 < vmax {
        ext = 0.0;
    } else {
        vnew1 = vmax;
        ext = (vnew3 - vmax) / f64::sqrt(1.0 - vmax / phib);
    }
    let qroot = f64::sqrt(1.0 - vnew1 / phib);
    let qggval = cgs * (2.0 * phib * (1.0 - qroot) + ext) + cgd * veff2;
    let par1 = 0.5 * (1.0 + (veff1 - vto) / vnroot);
    let cfact = (vgs - vgd) / veroot;
    let cplus = 0.5 * (1.0 + cfact);
    let cminus = cplus - cfact;
    (
        qggval,
        cgs / qroot * par1 * cplus + cgd * cminus,
        cgs / qroot * par1 * cminus + cgd * cplus,
    )
}

/// `MESdSetup` (`dev/mes/mesdset.c`, identical in ngspice-47 but for
/// its `m`): Taylor coefficients at the operating point `(vgs, vgd)`
/// (type-normalized, from `CKTrhsOld`), carried through the C's
/// `Dderivs` chain operation by operation.
///
/// The drain current is the load's Statz current (p = vgs, r = vds
/// of the mode-normalized device). The gate diodes are evaluated on
/// the real `vgs`/`vgd` (before the source/drain interchange, unlike
/// the JFET). The gate charges are NOT the load's `qggnew` split:
/// the C assumes `qgs = vgs·cgsnew`, `qgd = vgd·cgdnew` ("probably
/// wrong but then so is the a.c. analysis routine", its comment) and
/// builds `cgsnew` with `czgs/(qroot·par1)` where `qggnew` has
/// `czgs/qroot·par1` — both kept literally.
#[allow(clippy::too_many_lines)]
pub(crate) fn mes_dsetup(
    t: &MesTemp,
    area: f64,
    gmin: f64,
    vgs_real: f64,
    vgd_real: f64,
) -> Fet3DistoCoeffs {
    let m = &t.m;
    let typ = t.type_;
    let beta = m.beta * area;
    let csat = m.is * area;
    let vto = m.vto;
    let vds_real = vgs_real - vgd_real;
    let (_, lggs2, lggs3) = fet3_gate_diode_disto(vgs_real, csat, VT0, gmin);
    let (_, lggd2, lggd3) = fet3_gate_diode_disto(vgd_real, csat, VT0, gmin);

    // Drain current, local (mode-normalized) voltages.
    let (vgs, vds, mode) = if vds_real < 0.0 {
        (vgd_real, -vds_real, -1.0)
    } else {
        (vgs_real, vds_real, 1.0)
    };
    let mut d_p = Dderivs::var_p();
    let mut d_r = Dderivs::var_r();
    let d_zero = Dderivs::default();
    d_p.value = vgs;
    d_r.value = vds;
    let vgst = vgs - vto;
    let mut d_vgst = d_p;
    d_vgst.value = vgst;
    let d_cdrain = if vgst <= 0.0 {
        d_zero
    } else {
        let mut d_prod = d_r.times(m.lambda);
        d_prod.value = 1.0 + m.lambda * vds;
        let d_betap = d_prod.times(beta);
        let mut d_denom = d_vgst.times(m.b);
        d_denom.value = 1.0 + m.b * vgst;
        let d_invdenom = d_denom.inv();
        // Saturation region.
        let mut d_cdrain = d_betap.mult(&d_vgst).mult(&d_vgst).mult(&d_invdenom);
        if vds < 3.0 / m.alpha {
            // Linear region.
            let mut d_afact = d_r.times(-m.alpha / 3.0);
            d_afact.value = 1.0 - m.alpha * vds / 3.0;
            let mut d_lfact = d_afact.cube().times(-1.0);
            d_lfact.value += 1.0;
            d_cdrain = d_betap
                .mult(&d_vgst)
                .mult(&d_vgst)
                .mult(&d_invdenom)
                .mult(&d_lfact);
        }
        d_cdrain
    };

    // Charge storage: p = vgs, q = vgd (the real ones).
    let czgs = m.cgs * area;
    let czgd = m.cgd * area;
    let phib = m.pb;
    let vcap = 1.0 / m.alpha;
    let mut d_p = Dderivs::var_p();
    let mut d_q = Dderivs::var_q();
    d_p.value = vgs_real;
    d_q.value = vgd_real;
    let mut d_veroot = d_q.times(-1.0).plus(&d_p);
    d_veroot = d_veroot.mult(&d_veroot);
    d_veroot.value += vcap * vcap;
    let d_veroot = d_veroot.sqrt();
    let d_veff1 = d_veroot.plus(&d_p).plus(&d_q).times(0.5);
    let del = 0.2;
    // The region test reads the plain-double `vnew1` of the C.
    let veroot = f64::sqrt((vgs_real - vgd_real) * (vgs_real - vgd_real) + vcap * vcap);
    let veff1 = 0.5 * (vgs_real + vgd_real + veroot);
    let vnroot = f64::sqrt((veff1 - vto) * (veff1 - vto) + del * del);
    let vnew1 = 0.5 * (veff1 + vto + vnroot);
    let mut d_vnroot = d_veff1;
    d_vnroot.value -= vto;
    d_vnroot = d_vnroot.mult(&d_vnroot);
    d_vnroot.value += del * del;
    let d_vnroot = d_vnroot.sqrt();
    let mut d_vnew1 = d_veff1.plus(&d_vnroot);
    d_vnew1.value += vto;
    let mut d_vnew1 = d_vnew1.times(0.5);
    let vmax = 0.5;
    if vnew1 >= vmax {
        // (the C also builds `d_ext`, which no charge uses)
        d_vnew1 = d_zero;
        d_vnew1.value = vmax;
    }
    let mut d_qroot = d_vnew1.times(-1.0 / phib);
    d_qroot.value += 1.0;
    let d_qroot = d_qroot.sqrt();
    let mut d_par1 = d_veff1;
    d_par1.value -= vto;
    let mut d_par1 = d_par1.div(&d_vnroot);
    d_par1.value += 1.0;
    let d_par1 = d_par1.times(0.5);
    let d_cfact = d_q.times(-1.0).plus(&d_p).div(&d_veroot);
    let mut d_cplus = d_cfact.times(0.5);
    d_cplus.value += 0.5;
    let mut d_cminus = d_cfact.times(-0.5);
    d_cminus.value += 0.5;
    let d_dummy = d_qroot.mult(&d_par1).inv().times(czgs);
    let d_cgsnew = d_cminus.times(czgd).plus(&d_dummy.mult(&d_cplus));
    let d_cgdnew = d_cplus.times(czgd).plus(&d_dummy.mult(&d_cminus));
    let d_qgs = d_cgsnew.mult(&d_p);
    let d_qgd = d_cgdnew.mult(&d_q);

    let dc = &d_cdrain;
    let g = Fet3DrainDerivs {
        gm1: dc.d1_p,
        gds1: dc.d1_r,
        gm2: dc.d2_p2,
        gds2: dc.d2_r2,
        gmds: dc.d2_pr,
        gm3: dc.d3_p3,
        gds3: dc.d3_r3,
        gm2ds: dc.d3_p2r,
        gmds2: dc.d3_pr2,
    };
    let q2 = |d: &Dderivs| DfCoeffs {
        cxx: 0.5 * typ * d.d2_p2,
        cyy: 0.5 * typ * d.d2_q2,
        cxy: typ * d.d2_pq,
        cxxx: d.d3_p3 / 6.,
        cyyy: d.d3_q3 / 6.,
        cxxy: 0.5 * d.d3_p2q,
        cxyy: 0.5 * d.d3_pq2,
        ..DfCoeffs::default()
    };
    Fet3DistoCoeffs {
        cdr: fet3_cdr(typ, mode, &g),
        ggs2: typ * lggs2,
        ggs3: lggs3,
        ggd2: typ * lggd2,
        ggd3: lggd3,
        q: Fet3GateQ::Statz {
            qgs: q2(&d_qgs),
            qgd: q2(&d_qgd),
        },
    }
}

/// A MESFET instance (`MESinstance`).
pub(crate) struct MesI {
    n: Fet3Nodes,
    t: MesTemp,
    area: f64,
    off: bool,
    /// `IC=VDS,VGS`.
    ic: (f64, f64),
    ic_given: crate::devices::IcGiven,
    st: usize,
    /// `.disto` Taylor coefficients (`MESdSetup`).
    dist: Fet3DistoCoeffs,
}

impl MesI {
    pub fn new(
        nodes: [Var; 3],
        primes: [Var; 2],
        t: MesTemp,
        area: f64,
        off: bool,
        ic: (f64, f64),
        st: usize,
    ) -> Self {
        MesI {
            n: Fet3Nodes::new(nodes, primes),
            t,
            area,
            off,
            ic,
            ic_given: crate::devices::IcGiven::ALL,
            st,
            dist: Fet3DistoCoeffs::default(),
        }
    }
}

impl crate::devices::Device for MesI {
    fn par_load(&self) -> bool {
        true
    }

    /// `MESgetic` (`mesgetic.c`): VDS and VGS not given on the card from the
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
        let m = &self.t.m;
        let st = self.st;
        let typ = self.t.type_;
        let beta = m.beta * self.area;
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        let csat = m.is * self.area;

        let bias = fet3_bias(
            ctx,
            &self.n,
            st,
            typ,
            self.off,
            self.ic,
            VT0,
            self.t.vcrit,
            m.vto,
        );
        let (vgs, vgd) = (bias.vgs, bias.vgd);

        let (ggs, cg) = gate_diode(vgs, csat, VT0, ctx.gmin);
        let (ggd, cgd) = gate_diode(vgd, csat, VT0, ctx.gmin);
        let (cdrain, gm, gds) = mes_ids(vgs, vgd, m.vto, beta, m.lambda, m.b, m.alpha);
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

        // Charge storage (`MODETRAN | MODEINITSMSIG`, or the UIC
        // transient operating point — no `MODEAC` here, unlike the
        // JFET; the small-signal pass covers it).
        if ctx.kind == AnKind::Tran
            || ctx.mode == InitMode::Smsig
            || (ctx.kind == AnKind::TranOp && ctx.uic)
        {
            let czgs = m.cgs * self.area;
            let czgd = m.cgd * self.area;
            let phib = m.pb;
            let vgs1 = ctx.states.get(1, st + VGS);
            let vgd1 = ctx.states.get(1, st + VGD);
            let vcap = 1.0 / m.alpha;
            let q = |a: f64, c: f64| qggnew(a, c, phib, vcap, m.vto, czgs, czgd);
            let (qgga, cgsna, cgdna) = q(vgs, vgd);
            let (qggb, _, _) = q(vgs1, vgd);
            let (qggc, _, _) = q(vgs, vgd1);
            let (qggd, _, _) = q(vgs1, vgd1);
            if ctx.mode == InitMode::Tran {
                // Both slots take the total charge, as in the C.
                ctx.states.set(1, st + QGS, qgga);
                ctx.states.set(1, st + QGD, qgga);
            }
            let qgs = ctx.states.get(1, st + QGS) + 0.5 * (qgga - qggb + qggc - qggd);
            let qgd = ctx.states.get(1, st + QGD) + 0.5 * (qgga - qggc + qggb - qggd);
            ctx.states.set(0, st + QGS, qgs);
            ctx.states.set(0, st + QGD, qgd);
            if fet3_charge(ctx, st, cgsna, cgdna, &mut op) {
                return;
            }
        }

        fet3_finish(ctx, &self.n, st, typ, gdpr, gspr, &bias, &op);
    }

    /// `MEStrunc`.
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QGS, t, time_step);
        crate::integ::terr(self.st + QGD, t, time_step);
    }

    /// `MESacLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        fet3_ac(ctx, &self.n, self.st, gdpr, gspr);
    }

    /// `MESnoise`: see [`crate::jfet::fet3_noise`].
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(crate::jfet::FET3_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let gdpr = self.t.drain_conduct * self.area;
        let gspr = self.t.source_conduct * self.area;
        crate::jfet::fet3_noise(ctx, &self.n, self.st, gdpr, gspr, self.t.m.kf, self.t.m.af)
    }

    /// `MESdSetup`: see [`mes_dsetup`].
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        let n = &self.n;
        let typ = self.t.type_;
        let vgs = typ * (ctx.sol[n.gate] - ctx.sol[n.s_p]);
        let vgd = typ * (ctx.sol[n.gate] - ctx.sol[n.d_p]);
        self.dist = mes_dsetup(&self.t, self.area, ctx.gmin, vgs, vgd);
        Ok(())
    }

    /// `MESdisto` (`mesdisto.c`).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        fet3_disto_load(&self.dist, &self.n, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_matches_mestemp() {
        let mut m = MesModel::new(true);
        m.rs = 10.0;
        let t = MesTemp::new(&m);
        assert_eq!(t.source_conduct, 0.1);
        assert_eq!(t.drain_conduct, 0.0);
        assert!((t.vcrit - VT0 * (VT0 / (ROOT2 * 1e-14)).ln()).abs() < 1e-15);
        assert_eq!(MesTemp::new(&MesModel::new(false)).type_, -1.0);
    }

    #[test]
    fn ids_saturation_is_the_statz_formula() {
        let (beta, lambda, vto, b, alpha) = (2.5e-3, 0.01, -2.0, 0.3, 2.0);
        let (vgs, vgd) = (-0.5, -3.5); // vds = 3 > 3/alpha
        let (id, _, _) = mes_ids(vgs, vgd, vto, beta, lambda, b, alpha);
        let vgst: f64 = 1.5;
        let want = beta * (1.0 + lambda * 3.0) * vgst * vgst / (1.0 + b * vgst);
        assert!((id - want).abs() < 1e-18);
        // Continuity at vds = 3/alpha.
        let a = mes_ids(-0.5, -0.5 - 1.5 + 1e-12, vto, beta, lambda, b, alpha).0;
        let c = mes_ids(-0.5, -0.5 - 1.5 - 1e-12, vto, beta, lambda, b, alpha).0;
        assert!((a - c).abs() < 1e-12);
    }

    #[test]
    fn ids_derivatives_match_finite_differences() {
        let (beta, lambda, vto, b, alpha) = (2.5e-3, 0.05, -2.0, 0.3, 2.0);
        let h = 1e-6;
        for &(vgs, vgd) in &[(-0.5, -1.0), (-0.5, -3.0), (-1.0, -0.5), (-3.0, -0.5)] {
            let f = |a: f64, c: f64| mes_ids(a, c, vto, beta, lambda, b, alpha).0;
            let (_, gm, gds) = mes_ids(vgs, vgd, vto, beta, lambda, b, alpha);
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
    fn qggnew_capacitances_are_its_partials() {
        let (phib, vcap, vto, cgs, cgd) = (0.8, 0.5, -2.0, 2e-12, 0.5e-12);
        let h = 1e-6;
        for &(vgs, vgd) in &[(-0.5, -2.0), (0.3, -1.0), (-1.0, -0.2), (-3.0, -3.5)] {
            let (_, cgsn, cgdn) = qggnew(vgs, vgd, phib, vcap, vto, cgs, cgd);
            let q = |a: f64, c: f64| qggnew(a, c, phib, vcap, vto, cgs, cgd).0;
            let dgs = (q(vgs + h, vgd) - q(vgs - h, vgd)) / (2.0 * h);
            let dgd = (q(vgs, vgd + h) - q(vgs, vgd - h)) / (2.0 * h);
            assert!((cgsn - dgs).abs() < 1e-6 * cgs, "cgs at {vgs},{vgd}");
            assert!((cgdn - dgd).abs() < 1e-6 * cgs, "cgd at {vgs},{vgd}");
        }
    }

    /// The plain-number form of `mesdset.c`'s charge model: `qgs =
    /// vgs·cgsnew`, `qgd = vgd·cgdnew` with `cgsnew` built from
    /// `czgs/(qroot·par1)` (see [`mes_dsetup`]).
    fn dset_charges(
        vgs: f64,
        vgd: f64,
        phib: f64,
        vcap: f64,
        vto: f64,
        czgs: f64,
        czgd: f64,
    ) -> (f64, f64) {
        let veroot = f64::sqrt((vgs - vgd) * (vgs - vgd) + vcap * vcap);
        let veff1 = 0.5 * (vgs + vgd + veroot);
        let vnroot = f64::sqrt((veff1 - vto) * (veff1 - vto) + 0.04);
        let vnew1 = f64::min(0.5 * (veff1 + vto + vnroot), 0.5);
        let qroot = f64::sqrt(1.0 - vnew1 / phib);
        let par1 = 0.5 * (1.0 + (veff1 - vto) / vnroot);
        let cfact = (vgs - vgd) / veroot;
        let cplus = 0.5 * (1.0 + cfact);
        let cminus = cplus - cfact;
        let dummy = czgs / (qroot * par1);
        (
            vgs * (dummy * cplus + czgd * cminus),
            vgd * (dummy * cminus + czgd * cplus),
        )
    }

    /// `MESdSetup`'s coefficients against finite differences: the
    /// drain current is the load's Statz current (`mes_ids`) in the
    /// real node voltages (x = vgs, y = vds), both modes, both
    /// polarities, linear/saturation/cutoff; the gate diodes are the
    /// load's at `CONSTvt0`; the charges are the setup's own model in
    /// (vgs, vgd).
    #[test]
    fn mes_dsetup_matches_finite_differences() {
        use crate::jfet::tests::{assert_taylor, df_seven, taylor2_fd_rich};
        let gmin = 1e-12;
        let mut worst_cdr = 0.0f64;
        let mut worst_q = 0.0f64;
        for nmf in [true, false] {
            let mut m = MesModel::new(nmf);
            m.vto = -1.8;
            m.beta = 3e-3;
            m.b = 0.3;
            m.alpha = 2.5;
            m.lambda = 0.05;
            m.cgs = 1e-12;
            m.cgd = 0.2e-12;
            m.pb = 0.8;
            m.is = 1e-13;
            let t = MesTemp::new(&m);
            let typ = t.type_;
            let area = 1.3;
            let beta = m.beta * area;
            let csat = m.is * area;
            for &(vgs_n, vds_n) in &[
                (-0.5, 0.4),  // normal linear (vds < 3/α)
                (-0.5, 3.0),  // normal saturation
                (-0.6, -0.5), // inverse linear
                (-0.3, -2.5), // inverse saturation
                (0.4, 2.0),   // forward gate, vnew1 clamped
                (-2.5, 2.0),  // cutoff
            ] {
                let (vgs_a, vds_a) = (typ * vgs_n, typ * vds_n);
                let d = mes_dsetup(&t, area, gmin, vgs_n, vgs_n - vds_n);
                let id = |x: f64, y: f64| {
                    typ * mes_ids(typ * x, typ * (x - y), m.vto, beta, m.lambda, m.b, m.alpha).0
                };
                let want = taylor2_fd_rich(&id, vgs_a, vds_a, 4e-3);
                worst_cdr = worst_cdr.max(assert_taylor(
                    df_seven(&d.cdr),
                    want,
                    1e-5,
                    &format!("cdr nmf={nmf} at ({vgs_n},{vds_n})"),
                ));
                let Fet3GateQ::Statz { qgs, qgd } = d.q else {
                    panic!("MESFET charges are two-variable")
                };
                let q = |which: usize| {
                    move |x: f64, y: f64| {
                        // x = vgs, y = vgd (real); polarity as the
                        // other branches.
                        let (a, b) = dset_charges(
                            typ * x,
                            typ * y,
                            m.pb,
                            1.0 / m.alpha,
                            m.vto,
                            m.cgs * area,
                            m.cgd * area,
                        );
                        typ * if which == 0 { a } else { b }
                    }
                };
                let vgd_a = vgs_a - vds_a;
                for (k, c) in [qgs, qgd].iter().enumerate() {
                    let want = taylor2_fd_rich(&q(k), vgs_a, vgd_a, 4e-3);
                    worst_q = worst_q.max(assert_taylor(
                        df_seven(c),
                        want,
                        1e-5,
                        &format!("q{k} nmf={nmf} at ({vgs_n},{vds_n})"),
                    ));
                }
                let one = |v: f64, h: f64| {
                    let f = |v: f64| typ * gate_diode(typ * v, csat, VT0, gmin).1;
                    let c2 = 0.5 * (f(v + h) - 2.0 * f(v) + f(v - h)) / (h * h);
                    let c3 = (f(v + 2.0 * h) - 2.0 * f(v + h) + 2.0 * f(v - h) - f(v - 2.0 * h))
                        / (2.0 * h * h * h)
                        / 6.0;
                    (c2, c3)
                };
                for (got, v) in [((d.ggs2, d.ggs3), vgs_a), ((d.ggd2, d.ggd3), vgd_a)] {
                    let want = one(v, 2e-4);
                    for (a, b) in [(got.0, want.0), (got.1, want.1)] {
                        let s = a.abs().max(b.abs());
                        assert!((a - b).abs() <= 2e-3 * s + 1e-15, "gate diode {a} vs {b}");
                    }
                }
            }
        }
        eprintln!("MES cdr worst {worst_cdr:.2e}, charges worst {worst_q:.2e}");
    }
}
