//! `.disto` for the MOSFET levels that have it in the 3f5 — MOS1,
//! MOS2 and MOS3 (`dev/mos{1,2,3}/mos*dset.c`, `mos*dist.c`). The
//! three `dist.c` files are the same file with the prefix renamed,
//! and the three setups share everything but the drain-current
//! derivatives: the bulk diodes, the depletion and Meyer capacitance
//! coefficients and the final "type and mode" bookkeeping live here,
//! once. MOS6 and BSIM1/2 have no `DEVdisto` in this port (BSIM1's
//! `b1disto.c` is not ported) and keep the structured refusal.

use crate::dderivs::Dderivs;
use crate::disto::{DfCoeffs, DistoCtx, DistoMode, d1_term, df_term, jomega};
use crate::matrix::Var;

/// `MAX_EXP_ARG` (`hw_ieee.h`).
const MAX_EXP_ARG: f64 = 709.0;

/// Taylor coefficients of the MOSFET nonlinearities at the operating
/// point (the `cdr_*`, `gbs2 …`, `cap*2/3` instance fields): the
/// drain current in x = vgs, y = vbs, z = vds; the two bulk diodes;
/// the Meyer gate capacitances and the bulk depletion capacitances,
/// all one-variable.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MosDistoCoeffs {
    pub cdr: DfCoeffs,
    pub gbs2: f64,
    pub gbs3: f64,
    pub gbd2: f64,
    pub gbd3: f64,
    pub capgs2: f64,
    pub capgs3: f64,
    pub capgd2: f64,
    pub capgd3: f64,
    pub capgb2: f64,
    pub capgb3: f64,
    pub capbs2: f64,
    pub capbs3: f64,
    pub capbd2: f64,
    pub capbd3: f64,
}

/// Second and third derivatives of the drain current in (vgs, vbs,
/// vds) of the mode-normalized device (`gm2 … gmbds` in the C;
/// derivatives, not Taylor coefficients).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DrainDerivs {
    pub gm2: f64,
    pub gb2: f64,
    pub gds2: f64,
    pub gmb: f64,
    pub gbds: f64,
    pub gmds: f64,
    pub gm3: f64,
    pub gb3: f64,
    pub gds3: f64,
    pub gm2ds: f64,
    pub gm2b: f64,
    pub gb2ds: f64,
    pub gmb2: f64,
    pub gmds2: f64,
    pub gbds2: f64,
    pub gmbds: f64,
}

impl DrainDerivs {
    /// From a `Dderivs` with p = vgs, q = vbs, r = vds (the MOS2/MOS3
    /// setups' `gm2 = d_cdrain.d2_p2; …`).
    pub(crate) fn from_dderivs(d: &Dderivs) -> Self {
        DrainDerivs {
            gm2: d.d2_p2,
            gb2: d.d2_q2,
            gds2: d.d2_r2,
            gmb: d.d2_pq,
            gbds: d.d2_qr,
            gmds: d.d2_pr,
            gm3: d.d3_p3,
            gb3: d.d3_q3,
            gds3: d.d3_r3,
            gm2ds: d.d3_p2r,
            gm2b: d.d3_p2q,
            gb2ds: d.d3_q2r,
            gmb2: d.d3_pq2,
            gmds2: d.d3_pr2,
            gbds2: d.d3_qr2,
            gmbds: d.d3_pqr,
        }
    }
}

/// The bulk-diode terms (`lgbs2`/`lgbs3`, `lgbd2`/`lgbd3`): zero for
/// a reverse-biased junction (the C keeps only the linear `sat/vt`
/// there), `type·½·(g − gmin)/vt` and `type·g2/(3·vt)` forward.
pub(crate) fn bulk_diode(v: f64, sat_cur: f64, vt: f64, gmin: f64, typ: f64) -> (f64, f64) {
    if v <= 0.0 {
        (0.0, 0.0)
    } else {
        let ev = f64::exp(f64::min(MAX_EXP_ARG, v / vt));
        let g = sat_cur * ev / vt + gmin;
        let g2 = typ * 0.5 * (g - gmin) / vt;
        let g3 = typ * g2 / (vt * 3.0);
        (g2, g3)
    }
}

/// The depletion-capacitance terms of one bulk junction
/// (`lcapbs2`/`lcapbs3`): below `FC·PB` the derivatives of
/// `Cj·(1−v/PB)^−MJ + Cjsw·(1−v/PB)^−MJSW` (the `.5` grading
/// special-cased to `1/sqrt` like the C), above it the linear
/// continuation's `½·f3` — without the type factor the other branch
/// has, as in the C.
#[allow(clippy::too_many_arguments)]
pub(crate) fn depletion(
    v: f64,
    cz: f64,
    czsw: f64,
    f3: f64,
    dep_cap: f64,
    bulk_pot: f64,
    mj: f64,
    mjsw: f64,
    typ: f64,
) -> (f64, f64) {
    if v < dep_cap {
        let arg = 1.0 - v / bulk_pot;
        let sarg = if mj == 0.5 {
            1.0 / arg.sqrt()
        } else {
            f64::exp(-mj * f64::ln(arg))
        };
        let sargsw = if mjsw == 0.5 {
            1.0 / arg.sqrt()
        } else {
            f64::exp(-mjsw * f64::ln(arg))
        };
        let c2 = typ * 0.5 / bulk_pot * (cz * mj * sarg / arg + czsw * mjsw * sargsw / arg);
        let mut c3 = cz * sarg * mj * (mj + 1.0);
        c3 += czsw * sargsw * mjsw * (mjsw + 1.0);
        c3 /= 6.0 * bulk_pot * bulk_pot * arg * arg;
        (c2, c3)
    } else {
        (0.5 * f3, 0.0)
    }
}

/// The Meyer gate-capacitance terms `(lcapgs2, lcapgs3, lcapgd2,
/// lcapgd3, lcapgb2, lcapgb3)` (the setups' "meyer's capacitor
/// model" block: one controlling variable per charge, the C's own
/// caveat). `vgst`, `vdsat` are mode-normalized; `vds` is not, hence
/// the `mode·vds`.
pub(crate) fn meyer(
    vgst: f64,
    phi: f64,
    cox: f64,
    vdsat: f64,
    mode: f64,
    vds: f64,
) -> (f64, f64, f64, f64, f64, f64) {
    let (mut gs2, mut gs3, mut gd2, mut gd3, mut gb2, mut gb3) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    if vgst <= -phi {
    } else if vgst <= -phi / 2.0 {
        gb2 = -cox / (4.0 * phi);
    } else if vgst <= 0.0 {
        gb2 = -cox / (4.0 * phi);
        gs2 = cox / (3.0 * phi);
    } else if vdsat <= mode * vds {
    } else {
        let vddif = 2.0 * vdsat - mode * vds;
        let vddif1 = vdsat - mode * vds;
        let vddif2 = vddif * vddif;
        gd2 = -vdsat * mode * vds * cox / (3.0 * vddif * vddif2);
        gd3 = -mode * vds * cox * (vddif - 6.0 * vdsat) / (9.0 * vddif2 * vddif2);
        gs2 = -vddif1 * mode * vds * cox / (3.0 * vddif * vddif2);
        gs3 = -mode * vds * cox * (vddif - 6.0 * vddif1) / (9.0 * vddif2 * vddif2);
        gb2 = 0.0;
        gb3 = 0.0;
    }
    (gs2, gs3, gd2, gd3, gb2, gb3)
}

/// The setups' closing block: store the diode and capacitance terms,
/// then turn the drain-current derivatives into Taylor coefficients
/// "taking into account type and mode" — in inverse mode the source
/// and drain swap, which the C expresses as the chain rule for
/// `vgd = vgs − vds`, `vbd = vbs − vds`, including its
/// `cdr_yz = gmb + gb2 + gbds` (kept).
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish(
    typ: f64,
    mode: f64,
    g: &DrainDerivs,
    (gbs2, gbs3, gbd2, gbd3): (f64, f64, f64, f64),
    (capbs2, capbs3, capbd2, capbd3): (f64, f64, f64, f64),
    (lcapgs2, lcapgs3, lcapgd2, lcapgd3, lcapgb2, lcapgb3): (f64, f64, f64, f64, f64, f64),
) -> MosDistoCoeffs {
    let mut c = DfCoeffs::default();
    let (capgs2, capgs3, capgd2, capgd3);
    if mode == 1.0 {
        c.cxx = g.gm2;
        c.cyy = g.gb2;
        c.czz = g.gds2;
        c.cxy = g.gmb;
        c.cyz = g.gbds;
        c.cxz = g.gmds;
        c.cxxx = g.gm3;
        c.cyyy = g.gb3;
        c.czzz = g.gds3;
        c.cxxz = g.gm2ds;
        c.cxxy = g.gm2b;
        c.cyyz = g.gb2ds;
        c.cxyy = g.gmb2;
        c.cxzz = g.gmds2;
        c.cyzz = g.gbds2;
        c.cxyz = g.gmbds;
        capgs2 = typ * lcapgs2;
        capgs3 = lcapgs3;
        capgd2 = typ * lcapgd2;
        capgd3 = lcapgd3;
    } else {
        c.cxx = -g.gm2;
        c.cyy = -g.gb2;
        c.czz = -(g.gm2 + g.gb2 + g.gds2 + 2.0 * (g.gmb + g.gmds + g.gbds));
        c.cxy = -g.gmb;
        c.cyz = g.gmb + g.gb2 + g.gbds;
        c.cxz = g.gm2 + g.gmb + g.gmds;
        c.cxxx = -g.gm3;
        c.cyyy = -g.gb3;
        c.czzz = g.gm3
            + g.gb3
            + g.gds3
            + 3.0 * (g.gm2b + g.gm2ds + g.gmb2 + g.gb2ds + g.gmds2 + g.gbds2)
            + 6.0 * g.gmbds;
        c.cxxz = g.gm3 + g.gm2b + g.gm2ds;
        c.cxxy = -g.gm2b;
        c.cyyz = g.gmb2 + g.gb3 + g.gb2ds;
        c.cxyy = -g.gmb2;
        c.cxzz = -(g.gm3 + 2.0 * (g.gm2b + g.gm2ds + g.gmbds) + g.gmb2 + g.gmds2);
        c.cyzz = -(g.gb3 + 2.0 * (g.gmb2 + g.gb2ds + g.gmbds) + g.gm2b + g.gbds2);
        c.cxyz = g.gm2b + g.gmb2 + g.gmbds;
        capgs2 = typ * lcapgd2;
        capgs3 = lcapgd3;
        capgd2 = typ * lcapgs2;
        capgd3 = lcapgs3;
    }
    c.cxx *= 0.5 * typ;
    c.cyy *= 0.5 * typ;
    c.czz *= 0.5 * typ;
    c.cxy *= typ;
    c.cyz *= typ;
    c.cxz *= typ;
    c.cxxx /= 6.;
    c.cyyy /= 6.;
    c.czzz /= 6.;
    c.cxxz *= 0.5;
    c.cxxy *= 0.5;
    c.cyyz *= 0.5;
    c.cxyy *= 0.5;
    c.cxzz *= 0.5;
    c.cyzz *= 0.5;
    MosDistoCoeffs {
        cdr: c,
        gbs2,
        gbs3,
        gbd2,
        gbd3,
        capgs2,
        capgs3,
        capgd2,
        capgd3,
        capgb2: typ * lcapgb2,
        capgb3: lcapgb3,
        capbs2,
        capbs3,
        capbd2,
        capbd3,
    }
}

/// The junction parameters the closing block reads (`MOS*Cbs`,
/// `MOS*Cbssw`, `MOS*f3s`, the drain twins, `tDepCap`, `tBulkPot`,
/// the grading coefficients).
pub(crate) struct MosJunctions {
    pub cbs: f64,
    pub cbssw: f64,
    pub f3s: f64,
    pub cbd: f64,
    pub cbdsw: f64,
    pub f3d: f64,
    pub dep_cap: f64,
    pub bulk_pot: f64,
    pub mj: f64,
    pub mjsw: f64,
}

/// Everything after the drain-current block of a `MOS*dSetup`: the
/// bulk depletion and Meyer terms, then [`finish`]. `gbs`/`gbd` are
/// the [`bulk_diode`] pairs; `vgst`/`vdsat` the block's
/// mode-normalized values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn setup_tail(
    typ: f64,
    mode: f64,
    (vbs, vbd, vds): (f64, f64, f64),
    (vgst, vdsat): (f64, f64),
    phi: f64,
    oxide_cap: f64,
    j: &MosJunctions,
    dd: &DrainDerivs,
    gbs: (f64, f64),
    gbd: (f64, f64),
) -> MosDistoCoeffs {
    let capbs = depletion(
        vbs, j.cbs, j.cbssw, j.f3s, j.dep_cap, j.bulk_pot, j.mj, j.mjsw, typ,
    );
    let capbd = depletion(
        vbd, j.cbd, j.cbdsw, j.f3d, j.dep_cap, j.bulk_pot, j.mj, j.mjsw, typ,
    );
    let gate = meyer(vgst, phi, oxide_cap, vdsat, mode, vds);
    finish(
        typ,
        mode,
        dd,
        (gbs.0, gbs.1, gbd.0, gbd.1),
        (capbs.0, capbs.1, capbd.0, capbd.1),
        gate,
    )
}

/// `MOS{1,2,3}disto` in the load modes: x = vgs, y = vbs, z = vds on
/// the internal nodes; the drain current, the two bulk diodes and
/// the five charges. The C's `D_2F1MF2` case reads its "H2(f1,f1)"
/// kernels from `r1H1ptr`/`i1H1ptr` — the first-order ones — in all
/// three files and in ngspice-47 alike; kept.
pub(crate) fn mos_disto_load(
    d: &MosDistoCoeffs,
    (g, b, dp, sp): (Var, Var, Var, Var),
    ctx: &mut DistoCtx,
) {
    let mode = ctx.mode;
    let omega = ctx.omega;
    let mut kx = ctx.h.kern(mode, g, sp);
    let mut ky = ctx.h.kern(mode, b, sp);
    let mut kz = ctx.h.kern(mode, dp, sp);
    if mode == DistoMode::TwoF1mF2 {
        for k in [&mut kx, &mut ky, &mut kz] {
            k.r2h11 = k.r1h1;
            k.i2h11 = k.i1h1;
        }
    }
    let cdrain = df_term(mode, &d.cdr, &kx, &ky, &kz);
    ctx.stamp(dp, sp, cdrain);
    let gbs = d1_term(mode, d.gbs2, d.gbs3, &ky);
    ctx.stamp(b, sp, gbs);
    let gbd = d1_term(mode, d.gbd2, d.gbd3, &ky.sub(&kz));
    ctx.stamp(b, dp, gbd);
    let capgs = jomega(omega, d1_term(mode, d.capgs2, d.capgs3, &kx));
    ctx.stamp(g, sp, capgs);
    let capgd = jomega(omega, d1_term(mode, d.capgd2, d.capgd3, &kx.sub(&kz)));
    ctx.stamp(g, dp, capgd);
    let capgb = jomega(omega, d1_term(mode, d.capgb2, d.capgb3, &kx.sub(&ky)));
    ctx.stamp(g, b, capgb);
    let capbs = jomega(omega, d1_term(mode, d.capbs2, d.capbs3, &ky));
    ctx.stamp(b, sp, capbs);
    let capbd = jomega(omega, d1_term(mode, d.capbd2, d.capbd3, &ky.sub(&kz)));
    ctx.stamp(b, dp, capbd);
}
