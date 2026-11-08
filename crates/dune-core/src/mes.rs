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
use crate::jfet::{
    CQGD, CQGS, FET3_NUM_STATES, Fet3Nodes, Fet3Op, QGD, QGS, VGD, VGS, fet3_ac, fet3_bias,
    fet3_charge, fet3_finish, gate_diode,
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
}
