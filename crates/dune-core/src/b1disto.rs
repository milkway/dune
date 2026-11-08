//! `.disto` for BSIM1 (`dev/bsim1/b1dset.c`, `b1disto.c`; Jaijeet
//! Roychowdhury 1988 over Park/Quarles' `B1evaluate`). The setup
//! re-derives the drain current and the three terminal charges with
//! [`Dderivs`] (p = vgs, q = vbs, r = vds of the mode-normalized
//! device), adds the bulk junctions, maps the derivatives back to the
//! real terminals for the inverse mode and turns them into Taylor
//! coefficients; the load contracts them with the Volterra kernels.
//!
//! The setup is **not** `B1evaluate` differentiated: it is its own
//! transcription, quirks included, and ngspice-47 carries it unchanged
//! (apart from the `M` multiplier, which Dune's BSIM1 does not have).
//! Kept literally, each flagged where it happens:
//!
//! - the quadratic Beta0 interpolation multiplies by `d_dummy`
//!   (`1 + Ugs·VgsVth`, left over from `Arg`) where `vds` belongs
//!   (`b1dset.c:397`);
//! - `Von`'s `−Eta·vds` term is differentiated with `Eta` constant
//!   (no `ETAB`/`ETAD` chain, `b1dset.c:307`);
//! - the subthreshold current is added only inside the window
//!   `−40·N0·vt < VgsVth < 20·N0·vt` (the pre-6/8/90 `B1evaluate`;
//!   the load's adds it whenever `N0 < 200`);
//! - the 40/60 saturation drain charge uses `1/15` where
//!   `B1evaluate` uses `4/15` (`co4v15 = 1./15.`, `b1dset.c:744`);
//! - the forward-biased drain junction's `capbd2` reads the *source*
//!   areas (`czbs`, `czbssw`, `b1dset.c:991`);
//! - `b1disto.c` stamps the drain-junction current `gbd` between the
//!   **gate** and drain' in `D_TWOF1`, `D_F1PF2` and `D_F1MF2` (bulk
//!   and drain' in `D_THRF1`/`D_2F1MF2`).

use crate::bsim1::Bsim1Size;
use crate::consts::VT0;
use crate::dderivs::Dderivs;
use crate::disto::{DfCoeffs, DistoCtx, DistoMode, d1_term, df_term, jomega};
use crate::matrix::Var;
use crate::mos1::MosGeom;

/// The Taylor coefficients `B1dSetup` leaves on the instance
/// (`DrC_*`, `qg_*`, `qb_*`, `qd_*`, `gbs2/3`, `gbd2/3`; the
/// first-order ones are not read by the load and not kept).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct B1DistoCoeffs {
    pub drc: DfCoeffs,
    pub qg: DfCoeffs,
    pub qb: DfCoeffs,
    pub qd: DfCoeffs,
    pub gbs2: f64,
    pub gbs3: f64,
    pub gbd2: f64,
    pub gbd3: f64,
}

/// `EqualDeriv(&d, &d_p|q|r)` followed by field assignments: a
/// linear function with the given value and first derivatives.
fn lin(value: f64, d1_p: f64, d1_q: f64, d1_r: f64) -> Dderivs {
    Dderivs {
        value,
        d1_p,
        d1_q,
        d1_r,
        ..Dderivs::default()
    }
}

/// `d.value += k` (the C's in-place constant shifts).
fn shift(mut d: Dderivs, k: f64) -> Dderivs {
    d.value += k;
    d
}

/// The drain current and the gate, bulk and drain charges of the
/// mode-normalized device at local `(vgs, vbs, vds)`, `vds ≥ 0` — the
/// `B1dSetup` block from "as usual p=vgs, q=vbs, r=vds" to the
/// `finished:` label, before the junction and overlap additions.
#[allow(clippy::too_many_lines, clippy::similar_names)]
pub(crate) fn dset_core(
    t: &Bsim1Size,
    geom: &MosGeom,
    vgs: f64,
    vbs: f64,
    vds: f64,
) -> (Dderivs, Dderivs, Dderivs, Dderivs) {
    let vfb = t.vfb;
    let mut phi = t.phi;
    let k1 = t.k1;
    let k2 = t.k2;
    let vdd = t.vdd;
    let d_zero = Dderivs::default();
    let d_p = lin(vgs, 1.0, 0.0, 0.0);
    let d_r = lin(vds, 0.0, 0.0, 1.0);

    let ugs = t.ugs + t.ugs_b * vbs;
    let d_ugs = if ugs <= 0.0 {
        d_zero
    } else {
        lin(ugs, 0.0, t.ugs_b, 0.0)
    };
    let mut uds = t.uds + t.uds_b * vbs + t.uds_d * (vds - vdd);
    let d_uds = if uds <= 0.0 {
        uds = 0.0;
        d_zero
    } else {
        let leff = geom.l * 1.0e6 - t.delta_l; // µm
        uds /= leff;
        lin(uds, 0.0, t.uds_b / leff, t.uds_d / leff)
    };
    // `Eta <= 0 → 0`, `Eta > 1 → 1` (a clamp, written as the C's
    // if/else chain).
    let eta = (t.eta + t.eta_b * vbs + t.eta_d * (vds - vdd)).clamp(0.0, 1.0);
    // (`d_Eta` is built and clamped in the C but never read.)
    let (vpb, d_vpb) = if vbs < 0.0 {
        (phi - vbs, lin(phi - vbs, 0.0, -1.0, 0.0))
    } else {
        (phi, Dderivs::constant(phi))
    };
    let sqrt_vpb = vpb.sqrt();
    let d_sqrt_vpb = d_vpb.sqrt();
    let von = vfb + phi + k1 * sqrt_vpb - k2 * vpb - eta * vds;
    // Quirk (b1dset.c:307): `−Eta·vds` with Eta held constant.
    let d_dummy = lin(-eta * vds, 0.0, 0.0, -eta);
    let mut d_von = d_vpb.times(-k2).plus(&d_dummy);
    d_von = d_sqrt_vpb.times(k1).plus(&d_von);
    d_von.value = von;
    let vth = von;
    let d_vth = d_von;
    let vgs_vth = vgs - vth;
    let mut d_vgs_vth = d_vth.times(-1.0);
    d_vgs_vth.value = vgs_vth;
    d_vgs_vth.d1_p += 1.0;

    let mut d_g = shift(d_vpb.times(0.8364), 1.744).inv();
    d_g = shift(d_g.times(-1.0), 1.0);
    let g = 1.0 - 1.0 / (1.744 + 0.8364 * vpb);
    let mut a = g / sqrt_vpb;
    let mut d_a = d_g.div(&d_sqrt_vpb);
    a = 1.0 + 0.5 * k1 * a;
    d_a = shift(d_a.times(0.5 * k1), 1.0);
    a = f64::max(a, 1.0);
    if a <= 1.0 {
        d_a = Dderivs::constant(1.0);
    }
    let mut d_dummy = shift(d_ugs.mult(&d_vgs_vth), 1.0);
    let d_arg = if d_dummy.value <= 1.0 {
        Dderivs::constant(1.0)
    } else {
        d_dummy
    };

    let mut d_dr_cur;
    if vgs_vth < 0.0 {
        d_dr_cur = d_zero;
    } else {
        let d_beta_vds_0 = lin(t.beta_zero + t.beta_zero_b * vbs, 0.0, t.beta_zero_b, 0.0);
        let bvdd = t.beta_vdd + t.beta_vdd_b * vbs;
        let d_bvdd = lin(bvdd, 0.0, t.beta_vdd_b, 0.0);
        let dbvdd_dvds = f64::max(t.beta_vdd_d, 0.0);
        let d_beta0 = if vds > vdd {
            let mut d = d_r;
            d.value = vds - vdd;
            d.times(dbvdd_dvds).plus(&d_bvdd)
        } else {
            let vdd_square = vdd * vdd;
            let mut d_c1 = d_bvdd.times(-1.0).plus(&d_beta_vds_0);
            d_c1.value += dbvdd_dvds * vdd;
            d_c1 = d_c1.times(1.0 / vdd_square);
            let mut d_c2 = d_beta_vds_0.times(-1.0).plus(&d_bvdd).times(2.0 / vdd);
            d_c2.value -= dbvdd_dvds;
            // Quirk (b1dset.c:397): `MultDeriv(&d_Beta0,&d_Beta0,
            // &d_dummy)` — d_dummy still holds 1 + Ugs·VgsVth, not
            // vds, so Beta0 = (C1·vds + C2)·(1 + Ugs·VgsVth) + B0.
            d_r.mult(&d_c1)
                .plus(&d_c2)
                .mult(&d_dummy)
                .plus(&d_beta_vds_0)
        };
        let d_beta = d_beta0.div(&d_arg);
        let mut vc = uds * vgs_vth / a;
        let mut d_vc = d_vgs_vth.div(&d_a).mult(&d_uds);
        if vc < 0.0 {
            d_vc = d_zero;
            vc = 0.0;
        }
        let term1 = (1.0 + 2.0 * vc).sqrt();
        let d_term1 = shift(d_vc.times(2.0), 1.0).sqrt();
        let k = 0.5 * (1.0 + vc + term1);
        let d_k = shift(d_vc.plus(&d_term1), 1.0).times(0.5);
        let vds_sat = vgs_vth / (a * k.sqrt());
        // (`d_VdsSat` is built and never read.)
        if vds < f64::max(vds_sat, 0.0) {
            // Triode region.
            let argl1 = 1.0 + uds * vds;
            let d_argl1 = if argl1 < 1.0 {
                Dderivs::constant(1.0)
            } else {
                shift(d_r.mult(&d_uds), 1.0)
            };
            let d_argl2 = d_r.mult(&d_a).times(-0.5).plus(&d_vgs_vth);
            d_dr_cur = d_r.div(&d_argl1).mult(&d_argl2).mult(&d_beta);
        } else {
            // Pinchoff (saturation) region.
            let d_args2 = d_vgs_vth.div(&d_a.mult(&d_k));
            let d_args3 = d_args2.mult(&d_vgs_vth);
            d_dr_cur = d_beta.mult(&d_args3).times(0.5);
        }
    }

    // SubthresholdComputation (only inside the window, see the
    // module notes).
    let n0 = t.n0;
    let vcut = -40.0 * n0 * VT0;
    if !(n0 >= 200.0 || vgs_vth < vcut || vgs_vth > -0.5 * vcut) {
        let nb = t.nb;
        let nd = t.nd;
        let n = n0 + nb * vbs + nd * vds;
        let d_n = if n < 0.5 {
            Dderivs::constant(0.5)
        } else {
            lin(n, 0.0, nb, nd)
        };
        let d_warg1 = d_r.times(-1.0 / VT0).exp();
        let d_wds = shift(d_warg1.times(-1.0), 1.0);
        let d_wgs = d_vgs_vth.div(&d_n).times(1.0 / VT0).exp();
        let vtsquare = VT0 * VT0;
        let warg2 = 6.04965 * vtsquare * t.beta_zero;
        let ilimit = 4.5 * vtsquare * t.beta_zero;
        let d_iexp = d_wgs.mult(&d_wds).times(warg2);
        d_dummy = shift(d_iexp, ilimit).inv().mult(&d_iexp).times(ilimit);
        d_dr_cur = d_dr_cur.plus(&d_dummy);
    }

    // ChargeComputation.
    let wl_cox = t.cox * (geom.l - t.delta_l * 1.0e-6) * (geom.w - t.delta_w * 1.0e-6) * 1.0e4; // F
    let mut d_g = shift(d_vpb.times(-0.8364), -1.744).inv();
    d_g.value += 1.0;
    let g = 1.0 - 1.0 / (1.744 + 0.8364 * vpb);
    let a = 1.0 + 0.5 * k1 * g / sqrt_vpb;
    let (a, d_a) = if a < 1.0 {
        (1.0, Dderivs::constant(1.0))
    } else {
        (a, shift(d_g.div(&d_sqrt_vpb).times(0.5 * k1), 1.0))
    };
    phi = f64::max(0.1, phi);

    let vth0 = vfb + phi + k1 * sqrt_vpb;
    let d_vth0 = shift(d_sqrt_vpb.times(k1), vfb + phi);
    let vgb = vgs - vbs;
    let vgb_vfb = vgb - vfb;
    let d_vgb_vfb = lin(vgb_vfb, 1.0, -1.0, 0.0);

    // The 0/100 flag: the Dune load's own test (`B1evaluate` port);
    // ngspice's parser stores `XPART != 0` as 1, the 3f5 tests ≥ 1 —
    // identical for the non-negative integers the cards carry.
    let flag = t.xpart >= 1.0;
    let (d_vgs_vth, d_arg3) = if flag {
        let d_arg1 = d_a.mult(&d_r);
        (d_vth0.times(-1.0).plus(&d_p), d_arg1.times(-1.0).plus(&d_r))
    } else {
        let mut v = d_vth0.times(-1.0);
        v.value += vgs;
        v.d1_p += 1.0;
        let arg3 = vds - a * vds;
        let mut a3 = d_a.mult(&d_r).times(-1.0);
        a3.value = arg3;
        a3.d1_r += 1.0;
        (v, a3)
    };
    let vgs_vth = vgs - vth0;
    let d_arg1 = d_a.mult(&d_r);
    let d_arg2 = d_arg1.times(-0.5).plus(&d_vgs_vth);
    let d_arg5 = d_arg1.mult(&d_arg1);
    let arg2 = d_arg2.value;
    let d_ent = if arg2 < 1.0e-8 {
        Dderivs::constant(1.0e-8)
    } else {
        d_arg2
    };
    let ent = d_ent.value;
    // Vcom (flag branch: computed and unused; else branch: Argl6).
    let d_vcom = if flag {
        let d_dummy = d_arg1.times(-0.125);
        d_vgs_vth
            .times(1.0 / 6.0)
            .plus(&d_dummy)
            .mult(&d_vgs_vth)
            .plus(&d_arg5.times(2.5e-2))
    } else {
        let d_dummy = d_vgs_vth.times(1.0 / 6.0);
        d_arg1
            .times(-0.125)
            .plus(&d_dummy)
            .mult(&d_vgs_vth)
            .plus(&d_arg5.times(2.5e-2))
    };
    let vds_pinchoff = f64::max(vgs_vth / a, 0.0);

    let (d_qg, d_qb, d_qd);
    if vgb_vfb < 0.0 {
        // Accumulation.
        d_qg = d_vgb_vfb.times(wl_cox);
        d_qb = d_qg.times(-1.0);
        d_qd = d_zero;
    } else if vgs < vth0 {
        // Subthreshold.
        let mut q = shift(d_vgb_vfb.times(4.0 / (k1 * k1)), 1.0).sqrt();
        q.value -= 1.0;
        d_qg = q.times(0.5 * wl_cox * k1 * k1);
        d_qb = d_qg.times(-1.0);
        d_qd = d_zero;
    } else if vds < vds_pinchoff {
        // Triode.
        let d_ent_square = d_ent.mult(&d_ent);
        let (d_argl5, d_argl6) = if ent > 1.0e-8 {
            (d_arg1.div(&d_ent), d_vcom.div(&d_ent_square))
        } else {
            (Dderivs::constant(2.0), Dderivs::constant(4.0 / 1.5e1))
        };
        let d_argl7 = d_argl5.times(1.0 / 12.0);
        let mut qg = shift(d_argl7, -0.5).mult(&d_r);
        qg.value += vgs - vfb - phi;
        qg.d1_p += 1.0;
        d_qg = qg.times(wl_cox);
        let mut qb = shift(d_argl7.times(-1.0), 0.5).mult(&d_arg3);
        qb.value += vfb + phi;
        d_qb = qb.plus(&d_vth0.times(-1.0)).times(wl_cox);
        d_qd = if flag {
            shift(d_argl5.times(0.125), -0.75)
                .mult(&d_arg1)
                .plus(&d_vgs_vth.times(0.5))
                .times(-wl_cox)
        } else {
            let d_dummy = d_arg1.mult(&d_argl6);
            d_arg1
                .times(-1.0)
                .plus(&d_vgs_vth)
                .times(0.5)
                .plus(&d_dummy)
                .times(-wl_cox)
        };
    } else {
        // Saturation.
        let d_args1 = d_a.times(3.0).inv();
        let mut qg = d_vgs_vth.mult(&d_args1);
        qg.value += vfb + phi - vgs;
        qg.d1_p -= 1.0;
        d_qg = qg.times(-wl_cox);
        let d_dummy = shift(d_a.times(-1.0), 1.0);
        let mut qb = d_vgs_vth.mult(&d_dummy).mult(&d_args1);
        qb.value += vfb + phi;
        d_qb = qb.plus(&d_vth0.times(-1.0)).times(wl_cox);
        d_qd = if flag {
            d_zero
        } else {
            // Quirk (b1dset.c:744): `co4v15 = 1./15.` here, 4/15 in
            // `B1evaluate`.
            d_vgs_vth.times(-(1.0 / 15.0) * wl_cox)
        };
    }
    (d_dr_cur, d_qg, d_qb, d_qd)
}

/// The bulk-junction depletion terms of one junction (`capbs1..3`):
/// value of the capacitance and its two Taylor coefficients.
/// `capbd`'s forward-biased `…2` reads the source areas — the
/// caller passes them (b1dset.c:991).
fn junction_caps(v: f64, cz: f64, czsw: f64, t: &Bsim1Size) -> (f64, f64, f64) {
    let (phib, phibsw, mj, mjsw) = (t.pb, t.pbsw, t.mj, t.mjsw);
    if v < 0.0 {
        let arg = 1.0 - v / phib;
        let argsw = 1.0 - v / phibsw;
        let sarg = f64::exp(-mj * f64::ln(arg));
        let sargsw = f64::exp(-mjsw * f64::ln(argsw));
        let c1 = cz * sarg + czsw * sargsw;
        let c2 = (cz * mj * sarg / (phib * arg) + czsw * mjsw * sargsw / (phibsw * argsw)) / 2.0;
        let c3 = (cz * mj * (mj + 1.0) * sarg / ((phib * arg) * (phib * arg))
            + czsw * mjsw * (mjsw + 1.0) * sargsw / (phibsw * argsw * phibsw * argsw))
            / 6.0;
        (c1, c2, c3)
    } else {
        let c1 = cz + czsw + v * (cz * mj / phib + czsw * mjsw / phibsw);
        let c2 = (cz * mj / phib + czsw * mjsw / phibsw) * 0.5;
        (c1, c2, 0.0)
    }
}

/// The 16 second/third-order entries of a `Dderivs` in `DfCoeffs`
/// layout, still derivatives (the type/Taylor factors come later).
fn coeffs_of(d: &Dderivs) -> DfCoeffs {
    DfCoeffs {
        cxx: d.d2_p2,
        cyy: d.d2_q2,
        czz: d.d2_r2,
        cxy: d.d2_pq,
        cyz: d.d2_qr,
        cxz: d.d2_pr,
        cxxx: d.d3_p3,
        cyyy: d.d3_q3,
        czzz: d.d3_r3,
        cxxy: d.d3_p2q,
        cxxz: d.d3_p2r,
        cxyy: d.d3_pq2,
        cyyz: d.d3_q2r,
        cxzz: d.d3_pr2,
        cyzz: d.d3_qr2,
        cxyz: d.d3_pqr,
    }
}

/// The inverse-mode chain rule of `b1dset.c` for a quantity that
/// keeps its sign (qg, qb): real `(x, y, z) = (vgs, vbs, vds)` with
/// the local `(p, q, r) = (x − z, y − z, −z)`. The drain current
/// takes the negated form ([`inverse_neg`]).
fn inverse_keep(d: &Dderivs) -> DfCoeffs {
    let n = inverse_neg(d);
    DfCoeffs {
        cxx: -n.cxx,
        cyy: -n.cyy,
        czz: -n.czz,
        cxy: -n.cxy,
        cyz: -n.cyz,
        cxz: -n.cxz,
        cxxx: -n.cxxx,
        cyyy: -n.cyyy,
        czzz: -n.czzz,
        cxxy: -n.cxxy,
        cxxz: -n.cxxz,
        cxyy: -n.cxyy,
        cyyz: -n.cyyz,
        cxzz: -n.cxzz,
        cyzz: -n.cyzz,
        cxyz: -n.cxyz,
    }
}

/// `here->DrC_* = …` of the inverse-mode block: the derivatives of
/// `−f(x − z, y − z, −z)`.
fn inverse_neg(d: &Dderivs) -> DfCoeffs {
    DfCoeffs {
        cxx: -d.d2_p2,
        cyy: -d.d2_q2,
        czz: -(d.d2_p2 + d.d2_q2 + d.d2_r2 + 2.0 * (d.d2_pq + d.d2_pr + d.d2_qr)),
        cxy: -d.d2_pq,
        cyz: d.d2_pq + d.d2_q2 + d.d2_qr,
        cxz: d.d2_p2 + d.d2_pq + d.d2_pr,
        cxxx: -d.d3_p3,
        cyyy: -d.d3_q3,
        czzz: d.d3_p3
            + d.d3_q3
            + d.d3_r3
            + 3.0 * (d.d3_p2q + d.d3_p2r + d.d3_pq2 + d.d3_q2r + d.d3_pr2 + d.d3_qr2)
            + 6.0 * d.d3_pqr,
        cxxz: d.d3_p3 + d.d3_p2q + d.d3_p2r,
        cxxy: -d.d3_p2q,
        cyyz: d.d3_pq2 + d.d3_q3 + d.d3_q2r,
        cxyy: -d.d3_pq2,
        cxzz: -(d.d3_p3 + 2.0 * (d.d3_p2q + d.d3_p2r + d.d3_pqr) + d.d3_pq2 + d.d3_pr2),
        cyzz: -(d.d3_q3 + 2.0 * (d.d3_pq2 + d.d3_q2r + d.d3_pqr) + d.d3_p2q + d.d3_qr2),
        cxyz: d.d3_p2q + d.d3_pq2 + d.d3_pqr,
    }
}

/// Field-wise `a + b`.
fn add(a: &DfCoeffs, b: &DfCoeffs) -> DfCoeffs {
    DfCoeffs {
        cxx: a.cxx + b.cxx,
        cyy: a.cyy + b.cyy,
        czz: a.czz + b.czz,
        cxy: a.cxy + b.cxy,
        cyz: a.cyz + b.cyz,
        cxz: a.cxz + b.cxz,
        cxxx: a.cxxx + b.cxxx,
        cyyy: a.cyyy + b.cyyy,
        czzz: a.czzz + b.czzz,
        cxxy: a.cxxy + b.cxxy,
        cxxz: a.cxxz + b.cxxz,
        cxyy: a.cxyy + b.cxyy,
        cyyz: a.cyyz + b.cyyz,
        cxzz: a.cxzz + b.cxzz,
        cyzz: a.cyzz + b.cyzz,
        cxyz: a.cxyz + b.cxyz,
    }
}

/// "adjust for type and multiply by factors to convert to Taylor
/// coeffs": second order ×½·type (cross terms ×type), third order
/// ÷6 (pure) or ×½ (mixed; `xyz` as is).
fn taylor(c: &DfCoeffs, typ: f64) -> DfCoeffs {
    DfCoeffs {
        cxx: 0.5 * typ * c.cxx,
        cyy: 0.5 * typ * c.cyy,
        czz: 0.5 * typ * c.czz,
        cxy: typ * c.cxy,
        cyz: typ * c.cyz,
        cxz: typ * c.cxz,
        cxxx: c.cxxx / 6.,
        cyyy: c.cyyy / 6.,
        czzz: c.czzz / 6.,
        cxxz: 0.5 * c.cxxz,
        cxxy: 0.5 * c.cxxy,
        cyyz: 0.5 * c.cyyz,
        cxyy: 0.5 * c.cxyy,
        cxzz: 0.5 * c.cxzz,
        cyzz: 0.5 * c.cyzz,
        cxyz: c.cxyz,
    }
}

/// `B1dSetup` for one instance at the operating point `sol`
/// (`CKTrhsOld`): nodes are (gate, bulk, drain', source').
pub(crate) fn dsetup(
    t: &Bsim1Size,
    geom: &MosGeom,
    (g, b, dp, sp): (Var, Var, Var, Var),
    sol: &[f64],
    gmin: f64,
) -> B1DistoCoeffs {
    let typ = t.type_;
    let drain_area = geom.ad;
    let source_area = geom.as_;
    let drain_perimeter = geom.pd;
    let source_perimeter = geom.ps;
    let drain_sat_current = f64::max(drain_area * t.js, 1.0e-15);
    let source_sat_current = f64::max(source_area * t.js, 1.0e-15);

    let mut vbs = typ * (sol[b] - sol[sp]);
    let mut vgs = typ * (sol[g] - sol[sp]);
    let mut vds = typ * (sol[dp] - sol[sp]);
    let mode = if vds >= 0.0 {
        1.0
    } else {
        vds = -vds;
        vgs += vds; // the local (fake) values now
        vbs += vds;
        -1.0
    };
    let vbd = vbs - vds;

    let diode = |v: f64, sat: f64| {
        if v <= 0.0 {
            (0.0, 0.0)
        } else {
            let ev = f64::exp(v / VT0);
            let g1 = sat * ev / VT0 + gmin;
            let g2 = (g1 - gmin) / (VT0 * 2.0);
            (g2, g2 / (VT0 * 3.0))
        }
    };
    let (lgbs2, lgbs3) = diode(vbs, source_sat_current);
    let (lgbd2, lgbd3) = diode(vbd, drain_sat_current);

    let (d_dr_cur, d_qg, mut d_qb, mut d_qd) = dset_core(t, geom, vgs, vbs, vds);

    // Bulk junction depletion charges (on the local voltages).
    let czbd = t.cj * drain_area;
    let czbs = t.cj * source_area;
    let czbdsw = t.cjsw * drain_perimeter;
    let czbssw = t.cjsw * source_perimeter;
    let (_capbs1, capbs2, capbs3) = junction_caps(vbs, czbs, czbssw, t);
    let (_capbd1, mut capbd2, capbd3) = junction_caps(vbd, czbd, czbdsw, t);
    if vbd >= 0.0 {
        // Quirk (b1dset.c:991): the forward drain junction's second
        // coefficient is built from the source areas.
        capbd2 = junction_caps(vbd, czbs, czbssw, t).1;
    }
    // (The overlap and `capb*1` first-order additions are not read
    // by the load.)
    d_qb.d2_q2 += 2.0 * (capbd2 + capbs2);
    d_qb.d3_q3 += 6.0 * (capbd3 + capbs3);
    d_qb.d2_qr += -2.0 * capbd2;
    d_qb.d3_q2r += -capbd3 * 6.0;
    d_qb.d3_qr2 += capbd3 * 6.0;
    d_qb.d2_r2 += 2.0 * capbd2;
    d_qb.d3_r3 += -6.0 * capbd3;
    d_qd.d2_q2 -= 2.0 * capbd2;
    d_qd.d3_q3 -= 6.0 * capbd3;
    d_qd.d2_r2 -= 2.0 * capbd2;
    d_qd.d3_r3 -= -6.0 * capbd3;
    d_qd.d2_qr -= -2.0 * capbd2;
    d_qd.d3_q2r -= -6.0 * capbd3;
    d_qd.d3_qr2 -= 6.0 * capbd3;
    // (`d_qg` gets only first-order overlap terms.)

    let (drc, qg, qb, qd, gbs, gbd);
    if mode == 1.0 {
        drc = coeffs_of(&d_dr_cur);
        qg = coeffs_of(&d_qg);
        qb = coeffs_of(&d_qb);
        qd = coeffs_of(&d_qd);
        gbs = (lgbs2, lgbs3);
        gbd = (lgbd2, lgbd3);
    } else {
        // Inverse mode: qg, qb chain through unchanged; the drain
        // current changes sign; the real drain charge is the fake
        // source's, −(qg + qb + qd_fake).
        drc = inverse_neg(&d_dr_cur);
        qg = inverse_keep(&d_qg);
        qb = inverse_keep(&d_qb);
        let qd_fake = inverse_neg(&d_qd);
        let mut s = add(&qg, &qb);
        s = DfCoeffs {
            cxx: -s.cxx,
            cyy: -s.cyy,
            czz: -s.czz,
            cxy: -s.cxy,
            cyz: -s.cyz,
            cxz: -s.cxz,
            cxxx: -s.cxxx,
            cyyy: -s.cyyy,
            czzz: -s.czzz,
            cxxy: -s.cxxy,
            cxxz: -s.cxxz,
            cxyy: -s.cxyy,
            cyyz: -s.cyyz,
            cxzz: -s.cxzz,
            cyzz: -s.cyzz,
            cxyz: -s.cxyz,
        };
        qd = add(&s, &qd_fake);
        gbs = (lgbd2, lgbd3);
        gbd = (lgbs2, lgbs3);
    }
    B1DistoCoeffs {
        drc: taylor(&drc, typ),
        qg: taylor(&qg, typ),
        qb: taylor(&qb, typ),
        qd: taylor(&qd, typ),
        gbs2: typ * gbs.0,
        gbs3: gbs.1,
        gbd2: typ * gbd.0,
        gbd3: gbd.1,
    }
}

/// `B1disto` in the load modes: x = vgs, y = vbs, z = vds on the
/// internal nodes; the drain current, the three charges and the two
/// bulk diodes.
pub(crate) fn dload(c: &B1DistoCoeffs, (g, b, dp, sp): (Var, Var, Var, Var), ctx: &mut DistoCtx) {
    let mode = ctx.mode;
    let omega = ctx.omega;
    let kx = ctx.h.kern(mode, g, sp);
    let ky = ctx.h.kern(mode, b, sp);
    let kz = ctx.h.kern(mode, dp, sp);
    let dr = df_term(mode, &c.drc, &kx, &ky, &kz);
    ctx.stamp(dp, sp, dr);
    let qg = jomega(omega, df_term(mode, &c.qg, &kx, &ky, &kz));
    ctx.stamp(g, sp, qg);
    let qb = jomega(omega, df_term(mode, &c.qb, &kx, &ky, &kz));
    ctx.stamp(b, sp, qb);
    let qd = jomega(omega, df_term(mode, &c.qd, &kx, &ky, &kz));
    ctx.stamp(dp, sp, qd);
    let gbs = d1_term(mode, c.gbs2, c.gbs3, &ky);
    ctx.stamp(b, sp, gbs);
    let kbd = ctx.h.kern(mode, b, dp);
    let gbd = d1_term(mode, c.gbd2, c.gbd3, &kbd);
    // Quirk (b1disto.c, D_TWOF1/D_F1PF2/D_F1MF2): the drain-junction
    // current is stamped from the gate, not the bulk.
    let from = match mode {
        DistoMode::TwoF1 | DistoMode::F1pF2 | DistoMode::F1mF2 => g,
        DistoMode::ThrF1 | DistoMode::TwoF1mF2 => b,
    };
    ctx.stamp(from, dp, gbd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsim1::Bsim1Model;

    /// The bsim1tst/b1overlap card (`ETA` made positive, `ETAB`,
    /// `ETAD` zero: with them nonzero `d_Von` drops Eta's own
    /// dependence and the chain is no longer the derivative of its
    /// value — the documented quirk), sized like the probes.
    fn model(xpart: f64, vdd: f64, n0: f64) -> (Bsim1Size, MosGeom) {
        let mut m = Bsim1Model::new(true);
        m.tox = 3.0e-2;
        m.vdd = vdd;
        m.deltal = 7.97991e-1;
        m.deltaw = 4.77402e-1;
        m.vfb0 = -1.0087;
        m.phi0 = 7.96434e-1;
        m.k10 = 1.31191;
        m.k20 = 1.46640e-1;
        m.eta0 = 1.0e-2;
        m.mobzero = 5.34334e+2;
        m.ugs0 = 4.38497e-2;
        m.uds0 = 5.0e-1;
        m.udsb0 = 1.0e-2;
        m.ugsb0 = -7.6911e-4;
        m.mobzerob0 = 8.25434;
        m.mobvdd0 = 5.40612e+2;
        m.mobvddb0 = -1.2992e+1;
        m.mobvddd0 = 2.0;
        m.n00 = n0;
        m.nb0 = 0.09;
        m.nd0 = 0.05;
        m.xpart = xpart;
        m.cj = 2.75e-4;
        m.cjsw = 1.9e-10;
        m.js = 1.0e-2;
        m.pb = 0.7;
        m.pbsw = 0.8;
        m.mj = 0.5;
        m.mjsw = 0.33;
        let g = MosGeom {
            l: 2e-6,
            w: 20e-6,
            ad: 60e-12,
            as_: 40e-12,
            pd: 30e-6,
            ps: 24e-6,
            nrd: 0.0,
            nrs: 0.0,
        };
        (Bsim1Size::new("m1", &m, &g).unwrap(), g)
    }

    /// Central finite differences of `f` at `x0`: value, gradient
    /// (unused), Hessian and third-derivative tensor entries in the
    /// `DfCoeffs` layout, as Taylor coefficients (½ f_xx, f_xy,
    /// f_xxx/6, ½ f_xxy, f_xyz).
    fn fd_taylor(f: &dyn Fn([f64; 3]) -> f64, x0: [f64; 3], h: f64) -> DfCoeffs {
        let at = |d: [f64; 3]| f([x0[0] + d[0], x0[1] + d[1], x0[2] + d[2]]);
        let e = |i: usize, s: f64| {
            let mut v = [0.0; 3];
            v[i] = s;
            v
        };
        let sum = |a: [f64; 3], b: [f64; 3]| [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
        let d2 = |i: usize, j: usize| {
            (at(sum(e(i, h), e(j, h))) - at(sum(e(i, h), e(j, -h))) - at(sum(e(i, -h), e(j, h)))
                + at(sum(e(i, -h), e(j, -h))))
                / (4.0 * h * h)
        };
        let d3 = |i: usize, j: usize, k: usize| {
            let mut s = 0.0;
            for a in [-1.0, 1.0] {
                for b in [-1.0, 1.0] {
                    for c in [-1.0, 1.0] {
                        s += a * b * c * at(sum(sum(e(i, a * h), e(j, b * h)), e(k, c * h)));
                    }
                }
            }
            s / (8.0 * h * h * h)
        };
        DfCoeffs {
            cxx: 0.5 * d2(0, 0),
            cyy: 0.5 * d2(1, 1),
            czz: 0.5 * d2(2, 2),
            cxy: d2(0, 1),
            cyz: d2(1, 2),
            cxz: d2(0, 2),
            cxxx: d3(0, 0, 0) / 6.0,
            cyyy: d3(1, 1, 1) / 6.0,
            czzz: d3(2, 2, 2) / 6.0,
            cxxy: 0.5 * d3(0, 0, 1),
            cxxz: 0.5 * d3(0, 0, 2),
            cxyy: 0.5 * d3(0, 1, 1),
            cyyz: 0.5 * d3(1, 1, 2),
            cxzz: 0.5 * d3(0, 2, 2),
            cyzz: 0.5 * d3(1, 2, 2),
            cxyz: d3(0, 1, 2),
        }
    }

    fn as_array(c: &DfCoeffs) -> [f64; 16] {
        [
            c.cxx, c.cyy, c.czz, c.cxy, c.cyz, c.cxz, c.cxxx, c.cyyy, c.czzz, c.cxxy, c.cxxz,
            c.cxyy, c.cyyz, c.cxzz, c.cyzz, c.cxyz,
        ]
    }

    /// Worst error relative to the coefficient set's own scale.
    fn rel_err(got: &DfCoeffs, want: &DfCoeffs) -> f64 {
        let (g, w) = (as_array(got), as_array(want));
        let scale = w.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        g.iter()
            .zip(w.iter())
            .fold(0.0f64, |m, (a, b)| m.max((a - b).abs() / scale))
    }

    /// The Dderivs chain of every region carries the derivatives of
    /// its own value: drain current (triode, saturation with and
    /// without the vds > VDD branch, subthreshold window) and the
    /// three charges in both partitions.
    #[test]
    fn dset_core_matches_finite_differences() {
        // (xpart, vdd, n0, vgs, vbs, vds): saturation below VDD,
        // saturation above, triode, subthreshold window.
        let cases = [
            (0.0, 5.0, 1.55, 2.0, -1.0, 3.0),
            (0.0, 2.0, 1.55, 2.0, -1.0, 3.0),
            (1.0, 5.0, 1.55, 2.0, -1.0, 3.0),
            (0.0, 5.0, 1.55, 2.5, -0.5, 0.3),
            (1.0, 5.0, 1.55, 2.5, -0.5, 0.3),
            (0.0, 5.0, 1.55, 0.9, -0.2, 2.0),
        ];
        let mut worst = 0.0f64;
        for &(xpart, vdd, n0, vgs, vbs, vds) in &cases {
            let (t, g) = model(xpart, vdd, n0);
            let (d, qg, qb, qd) = dset_core(&t, &g, vgs, vbs, vds);
            for (k, dd) in [d, qg, qb, qd].iter().enumerate() {
                let f = |x: [f64; 3]| {
                    let r = dset_core(&t, &g, x[0], x[1], x[2]);
                    [r.0, r.1, r.2, r.3][k].value
                };
                let want = fd_taylor(&f, [vgs, vbs, vds], 5e-4);
                let got = taylor(&coeffs_of(dd), 1.0);
                let e = rel_err(&got, &want);
                worst = worst.max(e);
                assert!(
                    e < 1e-4,
                    "case {xpart} {vgs} {vbs} {vds}, quantity {k}: rel err {e:e}\n got {got:?}\nwant {want:?}"
                );
            }
        }
        eprintln!("dset_core vs FD: worst relative error {worst:e}");
    }

    /// Away from the transcription's quirks (vds > VDD for Beta0,
    /// N0 ≥ 200 for the subthreshold window, Eta's chain only touching
    /// derivatives)
    /// the setup's values are `B1evaluate`'s; the 40/60 saturation
    /// drain charge is a quarter of it (1/15 vs 4/15).
    #[test]
    fn dset_values_agree_with_the_load() {
        for &(xpart, vgs, vbs, vds) in &[
            (1.0, 2.0, -1.0, 3.0),
            (0.0, 2.5, -0.5, 0.3),
            (1.0, 2.5, -0.5, 0.3),
            (0.0, 2.0, -1.0, 3.0),
        ] {
            // VDD below vds (no Beta0 quirk); N0 ≥ 200 switches the
            // subthreshold term off in both.
            let vdd = 0.9 * vds;
            let (t, g) = model(xpart, vdd, 250.0);
            let (d, qg, qb, qd) = dset_core(&t, &g, vgs, vbs, vds);
            let (t2, g2) = model(xpart, vdd, 250.0);
            let e = crate::bsim1::evaluate_for_test(t2, g2, vds, vbs, vgs);
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1e-30);
            assert!(close(d.value, e.0), "cdrain {} vs {}", d.value, e.0);
            assert!(close(qg.value, e.1), "qg {} vs {}", qg.value, e.1);
            assert!(close(qb.value, e.2), "qb {} vs {}", qb.value, e.2);
            let sat_4060 = xpart == 0.0 && vds > 1.0;
            let want_qd = if sat_4060 { e.3 / 4.0 } else { e.3 };
            assert!(close(qd.value, want_qd), "qd {} vs {}", qd.value, want_qd);
        }
    }

    /// The full setup in the inverse mode (and the normal one) gives
    /// the Taylor coefficients of the *real-terminal* functions:
    /// I_d = −I(vgd, vbd, −vds), qg/qb unchanged, qd the fake
    /// source's charge −(qg + qb + qd_fake). Junction capacitances
    /// zeroed here (they are tested on their own below).
    #[test]
    fn mode_mapping_matches_finite_differences() {
        let (mut t, g) = model(0.0, 5.0, 1.55);
        t.cj = 0.0;
        t.cjsw = 0.0;
        let nodes = (1usize, 2usize, 3usize, 4usize); // g, b, d', s'
        let real = |x: [f64; 3]| {
            // x = real (vgs, vbs, vds) → local values and the mapped
            // quantities, as the C defines them.
            let (vgs, vbs, vds) = (x[0], x[1], x[2]);
            if vds >= 0.0 {
                let r = dset_core(&t, &g, vgs, vbs, vds);
                [r.0.value, r.1.value, r.2.value, r.3.value]
            } else {
                let r = dset_core(&t, &g, vgs - vds, vbs - vds, -vds);
                [
                    -r.0.value,
                    r.1.value,
                    r.2.value,
                    -(r.1.value + r.2.value + r.3.value),
                ]
            }
        };
        for &(vgs, vbs, vds) in &[(2.0, -1.0, 3.0), (-1.0, -4.0, -3.0), (-2.2, -2.8, -0.3)] {
            let sol = [0.0, vgs, vbs, vds, 0.0];
            let c = dsetup(&t, &g, nodes, &sol, 1e-12);
            for (k, got) in [c.drc, c.qg, c.qb, c.qd].iter().enumerate() {
                let want = fd_taylor(&|x| real(x)[k], [vgs, vbs, vds], 2e-3);
                let e = rel_err(got, &want);
                assert!(
                    e < 1e-4,
                    "({vgs},{vbs},{vds}) quantity {k}: {e:e}\n{got:?}\n{want:?}"
                );
            }
        }
    }

    /// `capb*2/3` are the Taylor coefficients of the depletion charge
    /// ∫C(v)dv on both sides of zero bias.
    #[test]
    fn junction_caps_are_charge_taylor_coefficients() {
        let (t, _) = model(0.0, 5.0, 1.55);
        let (cz, czsw) = (1.0e-14, 5.0e-15);
        // Charge (b1ld.c form): reverse, PB·cz·(1−arg^(1−MJ))/(1−MJ)…;
        // forward, the quadratic continuation.
        let q = |v: f64| {
            if v < 0.0 {
                let arg = 1.0 - v / t.pb;
                let argsw = 1.0 - v / t.pbsw;
                t.pb * cz * (1.0 - arg * arg.powf(-t.mj)) / (1.0 - t.mj)
                    + t.pbsw * czsw * (1.0 - argsw * argsw.powf(-t.mjsw)) / (1.0 - t.mjsw)
            } else {
                v * (cz + czsw) + v * v * (cz * t.mj * 0.5 / t.pb + czsw * t.mjsw * 0.5 / t.pbsw)
            }
        };
        let h = 1e-3;
        for v in [-1.5, -0.3, 0.2] {
            let (c1, c2, c3) = junction_caps(v, cz, czsw, &t);
            let d1 = (q(v + h) - q(v - h)) / (2.0 * h);
            let d2 = (q(v + h) - 2.0 * q(v) + q(v - h)) / (h * h);
            let d3 = (q(v + 2.0 * h) - 2.0 * q(v + h) + 2.0 * q(v - h) - q(v - 2.0 * h))
                / (2.0 * h * h * h);
            assert!((c1 - d1).abs() < 1e-6 * c1.abs(), "c1 {c1} vs {d1}");
            assert!(
                (c2 - d2 / 2.0).abs() < 1e-5 * c2.abs(),
                "c2 {c2} vs {}",
                d2 / 2.0
            );
            let w3 = d3 / 6.0;
            assert!((c3 - w3).abs() < 1e-4 * c2.abs(), "c3 {c3} vs {w3}");
        }
    }
}
