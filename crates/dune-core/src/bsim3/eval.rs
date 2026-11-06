//! The bias evaluation of `BSIM3load` (`b3ld.c`, from the effective
//! body bias through the capMod charge blocks up to the `finished:`
//! label), for the normal-mode frame (`Vds ≥ 0`; the caller swaps
//! drain and source for the inverse mode, as the C does). Kept as a
//! pure function of `(Vds, Vgs, Vbs)` so the derivatives can be
//! checked by finite differences. The C's variable names are kept
//! lowercased, its evaluation order, guard thresholds and smoothing
//! functions literally.

use super::params::{Bsim3Model, EPSSI};
use super::temp::{Bsim3MTemp, Bsim3Size, CHARGE_Q, EXP_THRESHOLD, MAX_EXP, MIN_EXP};

const DELTA_3: f64 = 0.02;
const DELTA_4: f64 = 0.02;

/// The per-instance values `BSIM3temp` adds on top of the shared
/// `pParam` (`delvto`, `mulu0`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstTemp {
    pub vth0: f64,
    pub vfb: f64,
    pub vfbzb: f64,
    pub u0temp: f64,
}

/// Everything the evaluation hands back to the load shell (the
/// C's `here->BSIM3…` assignments plus the locals the shell reads).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Ev {
    pub von: f64,
    pub thetavth: f64,
    pub vgsteff: f64,
    pub rds: f64,
    pub abulk: f64,
    pub ueff: f64,
    pub abov_vgst2vtm: f64,
    pub vdsat: f64,
    pub vdseff: f64,
    pub ids: f64,
    pub gm: f64,
    pub gds: f64,
    pub gmbs: f64,
    pub isub: f64,
    pub gbgs: f64,
    pub gbds: f64,
    pub gbbs: f64,
    pub qgate: f64,
    pub qbulk: f64,
    pub qdrn: f64,
    pub cggb: f64,
    pub cgsb: f64,
    pub cgdb: f64,
    pub cdgb: f64,
    pub cdsb: f64,
    pub cddb: f64,
    pub cbgb: f64,
    pub cbsb: f64,
    pub cbdb: f64,
    /// `BSIM3qinv` when this call updates it (`None`: the C's
    /// `xpart < 0` / no-charge early exit leaves the old value).
    pub qinv: Option<f64>,
}

/// `b3ld.c` lines from `T0 = Vbs - pParam->BSIM3vbsc - 0.001` to
/// `finished:`. `charges` is `ChargeComputationNeeded`.
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub(crate) fn evaluate(
    model: &Bsim3Model,
    mt: &Bsim3MTemp,
    pp: &Bsim3Size,
    it: &InstTemp,
    vds: f64,
    vgs: f64,
    vbs: f64,
    charges: bool,
) -> Ev {
    let mut e = Ev::default();
    let typ = model.type_();
    let cox = model.cox;
    let tox = model.tox;

    let (mut t0, mut t1, mut t2, mut t3, mut t4, mut t5, mut t6, mut t7, mut t8, mut t9) = (
        0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64,
    );
    let (mut t10, mut t11, mut t12) = (0.0f64, 0.0f64, 0.0f64);
    let (mut dt0_dvg, mut dt0_dvd, mut dt0_dvb) = (0.0f64, 0.0f64, 0.0f64);
    let (mut dt1_dvg, mut dt1_dvd, mut dt1_dvb) = (0.0f64, 0.0f64, 0.0f64);
    let (mut dt2_dvg, mut dt2_dvd, mut dt2_dvb) = (0.0f64, 0.0f64, 0.0f64);
    let (mut dt3_dvg, mut dt3_dvd, mut dt3_dvb) = (0.0f64, 0.0f64, 0.0f64);
    let (mut tmp, mut tmp1, mut tmp2, mut tmp3, mut tmp4) =
        (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);

    t0 = vbs - pp.vbsc - 0.001;
    t1 = f64::sqrt(t0 * t0 - 0.004 * pp.vbsc);
    let mut vbseff = pp.vbsc + 0.5 * (t0 + t1);
    let mut dvbseff_dvb = 0.5 * (1.0 + t0 / t1);
    if vbseff < vbs {
        vbseff = vbs;
    }

    let (phis, dphis_dvb, sqrt_phis, dsqrt_phis_dvb);
    if vbseff > 0.0 {
        t0 = pp.phi / (pp.phi + vbseff);
        phis = pp.phi * t0;
        dphis_dvb = -t0 * t0;
        sqrt_phis = pp.phis3 / (pp.phi + 0.5 * vbseff);
        dsqrt_phis_dvb = -0.5 * sqrt_phis * sqrt_phis / pp.phis3;
    } else {
        phis = pp.phi - vbseff;
        dphis_dvb = -1.0;
        sqrt_phis = phis.sqrt();
        dsqrt_phis_dvb = -0.5 / sqrt_phis;
    }
    let xdep = pp.xdep0 * sqrt_phis / pp.sqrt_phi;
    let dxdep_dvb = (pp.xdep0 / pp.sqrt_phi) * dsqrt_phis_dvb;

    let leff = pp.leff;
    let vtm = mt.vtm;

    // Vth calculation.
    t3 = xdep.sqrt();
    let v0 = pp.vbi - pp.phi;

    t0 = pp.dvt2 * vbseff;
    if t0 >= -0.5 {
        t1 = 1.0 + t0;
        t2 = pp.dvt2;
    } else {
        // Added to avoid any discontinuity problems caused by dvt2.
        t4 = 1.0 / (3.0 + 8.0 * t0);
        t1 = (1.0 + 3.0 * t0) * t4;
        t2 = pp.dvt2 * t4 * t4;
    }
    let lt1 = mt.factor1 * t3 * t1;
    let dlt1_dvb = mt.factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);

    t0 = pp.dvt2w * vbseff;
    if t0 >= -0.5 {
        t1 = 1.0 + t0;
        t2 = pp.dvt2w;
    } else {
        // Added to avoid any discontinuity problems caused by dvt2w.
        t4 = 1.0 / (3.0 + 8.0 * t0);
        t1 = (1.0 + 3.0 * t0) * t4;
        t2 = pp.dvt2w * t4 * t4;
    }
    let ltw = mt.factor1 * t3 * t1;
    let dltw_dvb = mt.factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);

    t0 = -0.5 * pp.dvt1 * leff / lt1;
    let (theta0, dtheta0_dvb);
    if t0 > -EXP_THRESHOLD {
        t1 = t0.exp();
        theta0 = t1 * (1.0 + 2.0 * t1);
        dt1_dvb = -t0 / lt1 * t1 * dlt1_dvb;
        dtheta0_dvb = (1.0 + 4.0 * t1) * dt1_dvb;
    } else {
        t1 = MIN_EXP;
        theta0 = t1 * (1.0 + 2.0 * t1);
        dtheta0_dvb = 0.0;
    }

    e.thetavth = pp.dvt0 * theta0;
    let delt_vth = e.thetavth * v0;
    let ddelt_vth_dvb = pp.dvt0 * dtheta0_dvb * v0;

    t0 = -0.5 * pp.dvt1w * pp.weff * leff / ltw;
    if t0 > -EXP_THRESHOLD {
        t1 = t0.exp();
        t2 = t1 * (1.0 + 2.0 * t1);
        dt1_dvb = -t0 / ltw * t1 * dltw_dvb;
        dt2_dvb = (1.0 + 4.0 * t1) * dt1_dvb;
    } else {
        t1 = MIN_EXP;
        t2 = t1 * (1.0 + 2.0 * t1);
        dt2_dvb = 0.0;
    }

    t0 = pp.dvt0w * t2;
    t2 = t0 * v0;
    dt2_dvb = pp.dvt0w * dt2_dvb * v0;

    let temp_ratio = mt.temp / model.tnom - 1.0;
    t0 = f64::sqrt(1.0 + pp.nlx / leff);
    t1 = pp.k1ox * (t0 - 1.0) * pp.sqrt_phi
        + (pp.kt1 + pp.kt1l / leff + pp.kt2 * vbseff) * temp_ratio;
    tmp2 = tox * pp.phi / (pp.weff + pp.w0);

    t3 = pp.eta0 + pp.etab * vbseff;
    if t3 < 1.0e-4 {
        // Avoid discontinuity problems caused by etab.
        t9 = 1.0 / (3.0 - 2.0e4 * t3);
        t3 = (2.0e-4 - t3) * t9;
        t4 = t9 * t9;
    } else {
        t4 = 1.0;
    }
    let ddibl_sft_dvd = t3 * pp.theta0vb0;
    let dibl_sft = ddibl_sft_dvd * vds;

    let vth = typ * it.vth0 - pp.k1 * pp.sqrt_phi + pp.k1ox * sqrt_phis
        - pp.k2ox * vbseff
        - delt_vth
        - t2
        + (pp.k3 + pp.k3b * vbseff) * tmp2
        + t1
        - dibl_sft;

    e.von = vth;

    let dvth_dvb = pp.k1ox * dsqrt_phis_dvb - pp.k2ox - ddelt_vth_dvb - dt2_dvb + pp.k3b * tmp2
        - pp.etab * vds * pp.theta0vb0 * t4
        + pp.kt2 * temp_ratio;
    let dvth_dvd = -ddibl_sft_dvd;

    // Calculate n.
    tmp2 = pp.nfactor * EPSSI / xdep;
    tmp3 = pp.cdsc + pp.cdscb * vbseff + pp.cdscd * vds;
    tmp4 = (tmp2 + tmp3 * theta0 + pp.cit) / cox;
    let (n, dn_dvb, dn_dvd);
    if tmp4 >= -0.5 {
        n = 1.0 + tmp4;
        dn_dvb = (-tmp2 / xdep * dxdep_dvb + tmp3 * dtheta0_dvb + pp.cdscb * theta0) / cox;
        dn_dvd = pp.cdscd * theta0 / cox;
    } else {
        // Avoid discontinuity problems caused by tmp4.
        t0 = 1.0 / (3.0 + 8.0 * tmp4);
        n = (1.0 + 3.0 * tmp4) * t0;
        t0 *= t0;
        dn_dvb = (-tmp2 / xdep * dxdep_dvb + tmp3 * dtheta0_dvb + pp.cdscb * theta0) / cox * t0;
        dn_dvd = pp.cdscd * theta0 / cox * t0;
    }

    // Poly gate Si depletion effect.
    t0 = it.vfb + pp.phi;
    let (vgs_eff, dvgs_eff_dvg);
    if pp.ngate > 1.0e18 && pp.ngate < 1.0e25 && vgs > t0 {
        // Added to avoid the problem caused by ngate.
        t1 = 1.0e6 * CHARGE_Q * EPSSI * pp.ngate / (cox * cox);
        t4 = f64::sqrt(1.0 + 2.0 * (vgs - t0) / t1);
        t2 = t1 * (t4 - 1.0);
        t3 = 0.5 * t2 * t2 / t1; // T3 = Vpoly
        t7 = 1.12 - t3 - 0.05;
        t6 = f64::sqrt(t7 * t7 + 0.224);
        t5 = 1.12 - 0.5 * (t7 + t6);
        vgs_eff = vgs - t5;
        dvgs_eff_dvg = 1.0 - (0.5 - 0.5 / t4) * (1.0 + t7 / t6);
    } else {
        vgs_eff = vgs;
        dvgs_eff_dvg = 1.0;
    }
    let mut vgst = vgs_eff - vth;

    // Effective Vgst (Vgsteff) calculation.
    t10 = 2.0 * n * vtm;
    let mut vgst_nvt = vgst / t10;
    let exp_arg = (2.0 * pp.voff - vgst) / t10;

    let (mut vgsteff, mut dvgsteff_dvg, mut dvgsteff_dvd, mut dvgsteff_dvb);
    // MCJ: very small Vgst.
    if vgst_nvt > EXP_THRESHOLD {
        vgsteff = vgst;
        dvgsteff_dvg = dvgs_eff_dvg;
        dvgsteff_dvd = -dvth_dvd;
        dvgsteff_dvb = -dvth_dvb;
    } else if exp_arg > EXP_THRESHOLD {
        t0 = (vgst - pp.voff) / (n * vtm);
        let exp_vgst = t0.exp();
        vgsteff = vtm * pp.cdep0 / cox * exp_vgst;
        dvgsteff_dvg = vgsteff / (n * vtm);
        dvgsteff_dvd = -dvgsteff_dvg * (dvth_dvd + t0 * vtm * dn_dvd);
        dvgsteff_dvb = -dvgsteff_dvg * (dvth_dvb + t0 * vtm * dn_dvb);
        dvgsteff_dvg *= dvgs_eff_dvg;
    } else {
        let exp_vgst = vgst_nvt.exp();
        t1 = t10 * f64::ln(1.0 + exp_vgst);
        dt1_dvg = exp_vgst / (1.0 + exp_vgst);
        dt1_dvb = -dt1_dvg * (dvth_dvb + vgst / n * dn_dvb) + t1 / n * dn_dvb;
        dt1_dvd = -dt1_dvg * (dvth_dvd + vgst / n * dn_dvd) + t1 / n * dn_dvd;

        dt2_dvg = -cox / (vtm * pp.cdep0) * exp_arg.exp();
        t2 = 1.0 - t10 * dt2_dvg;
        dt2_dvd = -dt2_dvg * (dvth_dvd - 2.0 * vtm * exp_arg * dn_dvd) + (t2 - 1.0) / n * dn_dvd;
        dt2_dvb = -dt2_dvg * (dvth_dvb - 2.0 * vtm * exp_arg * dn_dvb) + (t2 - 1.0) / n * dn_dvb;

        vgsteff = t1 / t2;
        t3 = t2 * t2;
        dvgsteff_dvg = (t2 * dt1_dvg - t1 * dt2_dvg) / t3 * dvgs_eff_dvg;
        dvgsteff_dvd = (t2 * dt1_dvd - t1 * dt2_dvd) / t3;
        dvgsteff_dvb = (t2 * dt1_dvb - t1 * dt2_dvb) / t3;
    }
    e.vgsteff = vgsteff;

    // Calculate effective channel geometry.
    t9 = sqrt_phis - pp.sqrt_phi;
    let mut weff = pp.weff - 2.0 * (pp.dwg * vgsteff + pp.dwb * t9);
    let mut dweff_dvg = -2.0 * pp.dwg;
    let mut dweff_dvb = -2.0 * pp.dwb * dsqrt_phis_dvb;

    if weff < 2.0e-8 {
        // To avoid the discontinuity problem due to Weff.
        t0 = 1.0 / (6.0e-8 - 2.0 * weff);
        weff = 2.0e-8 * (4.0e-8 - weff) * t0;
        t0 = t0 * t0 * 4.0e-16;
        dweff_dvg *= t0;
        dweff_dvb *= t0;
    }

    t0 = pp.prwg * vgsteff + pp.prwb * t9;
    let (rds, drds_dvg, drds_dvb);
    if t0 >= -0.9 {
        rds = pp.rds0 * (1.0 + t0);
        drds_dvg = pp.rds0 * pp.prwg;
        drds_dvb = pp.rds0 * pp.prwb * dsqrt_phis_dvb;
    } else {
        // To avoid the discontinuity problem due to prwg and prwb.
        t1 = 1.0 / (17.0 + 20.0 * t0);
        rds = pp.rds0 * (0.8 + t0) * t1;
        t1 *= t1;
        drds_dvg = pp.rds0 * pp.prwg * t1;
        drds_dvb = pp.rds0 * pp.prwb * dsqrt_phis_dvb * t1;
    }
    e.rds = rds; // Noise bugfix

    // Calculate Abulk.
    t1 = 0.5 * pp.k1ox / sqrt_phis;
    dt1_dvb = -t1 / sqrt_phis * dsqrt_phis_dvb;

    t9 = f64::sqrt(pp.xj * xdep);
    tmp1 = leff + 2.0 * t9;
    t5 = leff / tmp1;
    tmp2 = pp.a0 * t5;
    tmp3 = pp.weff + pp.b1;
    tmp4 = pp.b0 / tmp3;
    t2 = tmp2 + tmp4;
    dt2_dvb = -t9 / tmp1 / xdep * dxdep_dvb;
    t6 = t5 * t5;
    t7 = t5 * t6;

    let mut abulk0 = 1.0 + t1 * t2;
    let mut dabulk0_dvb = t1 * tmp2 * dt2_dvb + t2 * dt1_dvb;

    t8 = pp.ags * pp.a0 * t7;
    let mut dabulk_dvg = -t1 * t8;
    let mut abulk = abulk0 + dabulk_dvg * vgsteff;
    let mut dabulk_dvb = dabulk0_dvb - t8 * vgsteff * (dt1_dvb + 3.0 * t1 * dt2_dvb);

    if abulk0 < 0.1 {
        // Added to avoid the problems caused by Abulk0.
        t9 = 1.0 / (3.0 - 20.0 * abulk0);
        abulk0 = (0.2 - abulk0) * t9;
        dabulk0_dvb *= t9 * t9;
    }

    if abulk < 0.1 {
        // Added to avoid the problems caused by Abulk.
        t9 = 1.0 / (3.0 - 20.0 * abulk);
        abulk = (0.2 - abulk) * t9;
        t10 = t9 * t9;
        dabulk_dvb *= t10;
        dabulk_dvg *= t10;
    }
    e.abulk = abulk;

    t2 = pp.keta * vbseff;
    if t2 >= -0.9 {
        t0 = 1.0 / (1.0 + t2);
        dt0_dvb = -pp.keta * t0 * t0;
    } else {
        // Added to avoid the problems caused by Keta.
        t1 = 1.0 / (0.8 + t2);
        t0 = (17.0 + 20.0 * t2) * t1;
        dt0_dvb = -pp.keta * t1 * t1;
    }
    dabulk_dvg *= t0;
    dabulk_dvb = dabulk_dvb * t0 + abulk * dt0_dvb;
    dabulk0_dvb = dabulk0_dvb * t0 + abulk0 * dt0_dvb;
    abulk *= t0;
    abulk0 *= t0;

    // Mobility calculation.
    let (mut ddenomi_dvg, mut ddenomi_dvd, mut ddenomi_dvb);
    if model.mob_mod == 1 {
        t0 = vgsteff + vth + vth;
        t2 = pp.ua + pp.uc * vbseff;
        t3 = t0 / tox;
        t5 = t3 * (t2 + pp.ub * t3);
        ddenomi_dvg = (t2 + 2.0 * pp.ub * t3) / tox;
        ddenomi_dvd = ddenomi_dvg * 2.0 * dvth_dvd;
        ddenomi_dvb = ddenomi_dvg * 2.0 * dvth_dvb + pp.uc * t3;
    } else if model.mob_mod == 2 {
        t5 = vgsteff / tox * (pp.ua + pp.uc * vbseff + pp.ub * vgsteff / tox);
        ddenomi_dvg = (pp.ua + pp.uc * vbseff + 2.0 * pp.ub * vgsteff / tox) / tox;
        ddenomi_dvd = 0.0;
        ddenomi_dvb = vgsteff * pp.uc / tox;
    } else {
        t0 = vgsteff + vth + vth;
        t2 = 1.0 + pp.uc * vbseff;
        t3 = t0 / tox;
        t4 = t3 * (pp.ua + pp.ub * t3);
        t5 = t4 * t2;
        ddenomi_dvg = (pp.ua + 2.0 * pp.ub * t3) * t2 / tox;
        ddenomi_dvd = ddenomi_dvg * 2.0 * dvth_dvd;
        ddenomi_dvb = ddenomi_dvg * 2.0 * dvth_dvb + pp.uc * t4;
    }

    let denomi;
    if t5 >= -0.8 {
        denomi = 1.0 + t5;
    } else {
        // Added to avoid the discontinuity problem caused by ua and ub.
        t9 = 1.0 / (7.0 + 10.0 * t5);
        denomi = (0.6 + t5) * t9;
        t9 *= t9;
        ddenomi_dvg *= t9;
        ddenomi_dvd *= t9;
        ddenomi_dvb *= t9;
    }

    let ueff = it.u0temp / denomi;
    e.ueff = ueff;
    t9 = -ueff / denomi;
    let dueff_dvg = t9 * ddenomi_dvg;
    let dueff_dvd = t9 * ddenomi_dvd;
    let dueff_dvb = t9 * ddenomi_dvb;

    // Saturation drain voltage Vdsat.
    let wvcox = weff * pp.vsattemp * cox;
    let wvcox_rds = wvcox * rds;

    let esat = 2.0 * pp.vsattemp / ueff;
    let esat_l = esat * leff;
    t0 = -esat_l / ueff;
    let desat_l_dvg = t0 * dueff_dvg;
    let desat_l_dvd = t0 * dueff_dvd;
    let desat_l_dvb = t0 * dueff_dvb;

    // Sqrt()
    let a1 = pp.a1;
    let (lambda, dlambda_dvg);
    if a1 == 0.0 {
        lambda = pp.a2;
        dlambda_dvg = 0.0;
    } else if a1 > 0.0 {
        // Added to avoid the discontinuity problem caused by a1 and
        // a2 (Lambda).
        t0 = 1.0 - pp.a2;
        t1 = t0 - pp.a1 * vgsteff - 0.0001;
        t2 = f64::sqrt(t1 * t1 + 0.0004 * t0);
        lambda = pp.a2 + t0 - 0.5 * (t1 + t2);
        dlambda_dvg = 0.5 * pp.a1 * (1.0 + t1 / t2);
    } else {
        t1 = pp.a2 + pp.a1 * vgsteff - 0.0001;
        t2 = f64::sqrt(t1 * t1 + 0.0004 * pp.a2);
        lambda = 0.5 * (t1 + t2);
        dlambda_dvg = 0.5 * pp.a1 * (1.0 + t1 / t2);
    }

    let vgst2vtm = vgsteff + 2.0 * vtm;
    e.abov_vgst2vtm = abulk / vgst2vtm;

    if rds > 0.0 {
        tmp2 = drds_dvg / rds + dweff_dvg / weff;
        tmp3 = drds_dvb / rds + dweff_dvb / weff;
    } else {
        tmp2 = dweff_dvg / weff;
        tmp3 = dweff_dvb / weff;
    }
    let (mut vdsat, dvdsat_dvg, dvdsat_dvd, dvdsat_dvb);
    if rds == 0.0 && lambda == 1.0 {
        t0 = 1.0 / (abulk * esat_l + vgst2vtm);
        tmp1 = 0.0;
        t1 = t0 * t0;
        t2 = vgst2vtm * t0;
        t3 = esat_l * vgst2vtm;
        vdsat = t3 * t0;

        dt0_dvg = -(abulk * desat_l_dvg + esat_l * dabulk_dvg + 1.0) * t1;
        dt0_dvd = -(abulk * desat_l_dvd) * t1;
        dt0_dvb = -(abulk * desat_l_dvb + dabulk_dvb * esat_l) * t1;

        dvdsat_dvg = t3 * dt0_dvg + t2 * desat_l_dvg + esat_l * t0;
        dvdsat_dvd = t3 * dt0_dvd + t2 * desat_l_dvd;
        dvdsat_dvb = t3 * dt0_dvb + t2 * desat_l_dvb;
    } else {
        tmp1 = dlambda_dvg / (lambda * lambda);
        t9 = abulk * wvcox_rds;
        t8 = abulk * t9;
        t7 = vgst2vtm * t9;
        t6 = vgst2vtm * wvcox_rds;
        t0 = 2.0 * abulk * (t9 - 1.0 + 1.0 / lambda);
        dt0_dvg = 2.0 * (t8 * tmp2 - abulk * tmp1 + (2.0 * t9 + 1.0 / lambda - 1.0) * dabulk_dvg);

        dt0_dvb =
            2.0 * (t8 * (2.0 / abulk * dabulk_dvb + tmp3) + (1.0 / lambda - 1.0) * dabulk_dvb);
        dt0_dvd = 0.0;
        t1 = vgst2vtm * (2.0 / lambda - 1.0) + abulk * esat_l + 3.0 * t7;

        dt1_dvg = (2.0 / lambda - 1.0) - 2.0 * vgst2vtm * tmp1
            + abulk * desat_l_dvg
            + esat_l * dabulk_dvg
            + 3.0 * (t9 + t7 * tmp2 + t6 * dabulk_dvg);
        dt1_dvb = abulk * desat_l_dvb + esat_l * dabulk_dvb + 3.0 * (t6 * dabulk_dvb + t7 * tmp3);
        dt1_dvd = abulk * desat_l_dvd;

        t2 = vgst2vtm * (esat_l + 2.0 * t6);
        dt2_dvg = esat_l + vgst2vtm * desat_l_dvg + t6 * (4.0 + 2.0 * vgst2vtm * tmp2);
        dt2_dvb = vgst2vtm * (desat_l_dvb + 2.0 * t6 * tmp3);
        dt2_dvd = vgst2vtm * desat_l_dvd;

        t3 = f64::sqrt(t1 * t1 - 2.0 * t0 * t2);
        vdsat = (t1 - t3) / t0;

        dt3_dvg = (t1 * dt1_dvg - 2.0 * (t0 * dt2_dvg + t2 * dt0_dvg)) / t3;
        dt3_dvd = (t1 * dt1_dvd - 2.0 * (t0 * dt2_dvd + t2 * dt0_dvd)) / t3;
        dt3_dvb = (t1 * dt1_dvb - 2.0 * (t0 * dt2_dvb + t2 * dt0_dvb)) / t3;

        dvdsat_dvg =
            (dt1_dvg - (t1 * dt1_dvg - dt0_dvg * t2 - t0 * dt2_dvg) / t3 - vdsat * dt0_dvg) / t0;
        dvdsat_dvb =
            (dt1_dvb - (t1 * dt1_dvb - dt0_dvb * t2 - t0 * dt2_dvb) / t3 - vdsat * dt0_dvb) / t0;
        dvdsat_dvd = (dt1_dvd - (t1 * dt1_dvd - t0 * dt2_dvd) / t3) / t0;
    }
    e.vdsat = vdsat;

    // Effective Vds (Vdseff) calculation.
    t1 = vdsat - vds - pp.delta;
    dt1_dvg = dvdsat_dvg;
    dt1_dvd = dvdsat_dvd - 1.0;
    dt1_dvb = dvdsat_dvb;

    t2 = f64::sqrt(t1 * t1 + 4.0 * pp.delta * vdsat);
    t0 = t1 / t2;
    t3 = 2.0 * pp.delta / t2;
    dt2_dvg = t0 * dt1_dvg + t3 * dvdsat_dvg;
    dt2_dvd = t0 * dt1_dvd + t3 * dvdsat_dvd;
    dt2_dvb = t0 * dt1_dvb + t3 * dvdsat_dvb;

    let mut vdseff = vdsat - 0.5 * (t1 + t2);
    let mut dvdseff_dvg = dvdsat_dvg - 0.5 * (dt1_dvg + dt2_dvg);
    let dvdseff_dvd = dvdsat_dvd - 0.5 * (dt1_dvd + dt2_dvd);
    let mut dvdseff_dvb = dvdsat_dvb - 0.5 * (dt1_dvb + dt2_dvb);
    // Added to eliminate non-zero Vdseff at Vds=0.0.
    if vds == 0.0 {
        vdseff = 0.0;
        dvdseff_dvg = 0.0;
        dvdseff_dvb = 0.0;
    }

    // Calculate VAsat.
    tmp4 = 1.0 - 0.5 * abulk * vdsat / vgst2vtm;
    t9 = wvcox_rds * vgsteff;
    t8 = t9 / vgst2vtm;
    t0 = esat_l + vdsat + 2.0 * t9 * tmp4;

    t7 = 2.0 * wvcox_rds * tmp4;
    dt0_dvg = desat_l_dvg + dvdsat_dvg + t7 * (1.0 + tmp2 * vgsteff)
        - t8 * (abulk * dvdsat_dvg - abulk * vdsat / vgst2vtm + vdsat * dabulk_dvg);

    dt0_dvb = desat_l_dvb + dvdsat_dvb + t7 * tmp3 * vgsteff
        - t8 * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
    dt0_dvd = desat_l_dvd + dvdsat_dvd - t8 * abulk * dvdsat_dvd;

    t9 = wvcox_rds * abulk;
    t1 = 2.0 / lambda - 1.0 + t9;
    dt1_dvg = -2.0 * tmp1 + wvcox_rds * (abulk * tmp2 + dabulk_dvg);
    dt1_dvb = dabulk_dvb * wvcox_rds + t9 * tmp3;

    let vasat = t0 / t1;
    let dvasat_dvg = (dt0_dvg - vasat * dt1_dvg) / t1;
    let dvasat_dvb = (dt0_dvb - vasat * dt1_dvb) / t1;
    let dvasat_dvd = dt0_dvd / t1;

    if vdseff > vds {
        vdseff = vds;
    }
    let diff_vds = vds - vdseff;
    e.vdseff = vdseff;

    // Calculate VACLM.
    let (vaclm, dvaclm_dvg, dvaclm_dvd, dvaclm_dvb);
    if pp.pclm > 0.0 && diff_vds > 1.0e-10 {
        t0 = 1.0 / (pp.pclm * abulk * pp.litl);
        dt0_dvb = -t0 / abulk * dabulk_dvb;
        dt0_dvg = -t0 / abulk * dabulk_dvg;

        t2 = vgsteff / esat_l;
        t1 = leff * (abulk + t2);
        dt1_dvg = leff * ((1.0 - t2 * desat_l_dvg) / esat_l + dabulk_dvg);
        dt1_dvb = leff * (dabulk_dvb - t2 * desat_l_dvb / esat_l);
        dt1_dvd = -t2 * desat_l_dvd / esat;

        t9 = t0 * t1;
        vaclm = t9 * diff_vds;
        dvaclm_dvg = t0 * dt1_dvg * diff_vds - t9 * dvdseff_dvg + t1 * diff_vds * dt0_dvg;
        dvaclm_dvb = (dt0_dvb * t1 + t0 * dt1_dvb) * diff_vds - t9 * dvdseff_dvb;
        dvaclm_dvd = t0 * dt1_dvd * diff_vds + t9 * (1.0 - dvdseff_dvd);
    } else {
        vaclm = MAX_EXP;
        dvaclm_dvd = 0.0;
        dvaclm_dvg = 0.0;
        dvaclm_dvb = 0.0;
    }

    // Calculate VADIBL.
    let (mut vadibl, mut dvadibl_dvg, mut dvadibl_dvd, mut dvadibl_dvb);
    if pp.theta_rout > 0.0 {
        t8 = abulk * vdsat;
        t0 = vgst2vtm * t8;
        dt0_dvg = vgst2vtm * abulk * dvdsat_dvg + t8 + vgst2vtm * vdsat * dabulk_dvg;
        dt0_dvb = vgst2vtm * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
        dt0_dvd = vgst2vtm * abulk * dvdsat_dvd;

        t1 = vgst2vtm + t8;
        dt1_dvg = 1.0 + abulk * dvdsat_dvg + vdsat * dabulk_dvg;
        dt1_dvb = abulk * dvdsat_dvb + dabulk_dvb * vdsat;
        dt1_dvd = abulk * dvdsat_dvd;

        t9 = t1 * t1;
        t2 = pp.theta_rout;
        vadibl = (vgst2vtm - t0 / t1) / t2;
        dvadibl_dvg = (1.0 - dt0_dvg / t1 + t0 * dt1_dvg / t9) / t2;
        dvadibl_dvb = (-dt0_dvb / t1 + t0 * dt1_dvb / t9) / t2;
        dvadibl_dvd = (-dt0_dvd / t1 + t0 * dt1_dvd / t9) / t2;

        t7 = pp.pdiblb * vbseff;
        if t7 >= -0.9 {
            t3 = 1.0 / (1.0 + t7);
            vadibl *= t3;
            dvadibl_dvg *= t3;
            dvadibl_dvb = (dvadibl_dvb - vadibl * pp.pdiblb) * t3;
            dvadibl_dvd *= t3;
        } else {
            // Added to avoid the discontinuity problem caused by
            // pdiblcb.
            t4 = 1.0 / (0.8 + t7);
            t3 = (17.0 + 20.0 * t7) * t4;
            dvadibl_dvg *= t3;
            dvadibl_dvb = dvadibl_dvb * t3 - vadibl * pp.pdiblb * t4 * t4;
            dvadibl_dvd *= t3;
            vadibl *= t3;
        }
    } else {
        vadibl = MAX_EXP;
        dvadibl_dvd = 0.0;
        dvadibl_dvg = 0.0;
        dvadibl_dvb = 0.0;
    }

    // Calculate VA.
    t8 = pp.pvag / esat_l;
    t9 = t8 * vgsteff;
    if t9 > -0.9 {
        t0 = 1.0 + t9;
        dt0_dvg = t8 * (1.0 - vgsteff * desat_l_dvg / esat_l);
        dt0_dvb = -t9 * desat_l_dvb / esat_l;
        dt0_dvd = -t9 * desat_l_dvd / esat_l;
    } else {
        // Added to avoid the discontinuity problems caused by pvag.
        t1 = 1.0 / (17.0 + 20.0 * t9);
        t0 = (0.8 + t9) * t1;
        t1 *= t1;
        dt0_dvg = t8 * (1.0 - vgsteff * desat_l_dvg / esat_l) * t1;

        t9 *= t1 / esat_l;
        dt0_dvb = -t9 * desat_l_dvb;
        dt0_dvd = -t9 * desat_l_dvd;
    }

    tmp1 = vaclm * vaclm;
    tmp2 = vadibl * vadibl;
    tmp3 = vaclm + vadibl;

    t1 = vaclm * vadibl / tmp3;
    tmp3 *= tmp3;
    dt1_dvg = (tmp1 * dvadibl_dvg + tmp2 * dvaclm_dvg) / tmp3;
    dt1_dvd = (tmp1 * dvadibl_dvd + tmp2 * dvaclm_dvd) / tmp3;
    dt1_dvb = (tmp1 * dvadibl_dvb + tmp2 * dvaclm_dvb) / tmp3;

    let va = vasat + t0 * t1;
    let dva_dvg = dvasat_dvg + t1 * dt0_dvg + t0 * dt1_dvg;
    let dva_dvd = dvasat_dvd + t1 * dt0_dvd + t0 * dt1_dvd;
    let dva_dvb = dvasat_dvb + t1 * dt0_dvb + t0 * dt1_dvb;

    // Calculate VASCBE.
    let (vascbe, dvascbe_dvg, dvascbe_dvd, dvascbe_dvb);
    if pp.pscbe2 > 0.0 {
        if diff_vds > pp.pscbe1 * pp.litl / EXP_THRESHOLD {
            t0 = pp.pscbe1 * pp.litl / diff_vds;
            vascbe = leff * t0.exp() / pp.pscbe2;
            t1 = t0 * vascbe / diff_vds;
            dvascbe_dvg = t1 * dvdseff_dvg;
            dvascbe_dvd = -t1 * (1.0 - dvdseff_dvd);
            dvascbe_dvb = t1 * dvdseff_dvb;
        } else {
            vascbe = MAX_EXP * leff / pp.pscbe2;
            dvascbe_dvg = 0.0;
            dvascbe_dvd = 0.0;
            dvascbe_dvb = 0.0;
        }
    } else {
        vascbe = MAX_EXP;
        dvascbe_dvg = 0.0;
        dvascbe_dvd = 0.0;
        dvascbe_dvb = 0.0;
    }

    // Calculate Ids.
    let cox_wov_l = cox * weff / leff;
    let beta = ueff * cox_wov_l;
    let dbeta_dvg = cox_wov_l * dueff_dvg + beta * dweff_dvg / weff;
    let dbeta_dvd = cox_wov_l * dueff_dvd;
    let dbeta_dvb = cox_wov_l * dueff_dvb + beta * dweff_dvb / weff;

    t0 = 1.0 - 0.5 * abulk * vdseff / vgst2vtm;
    dt0_dvg =
        -0.5 * (abulk * dvdseff_dvg - abulk * vdseff / vgst2vtm + vdseff * dabulk_dvg) / vgst2vtm;
    dt0_dvd = -0.5 * abulk * dvdseff_dvd / vgst2vtm;
    dt0_dvb = -0.5 * (abulk * dvdseff_dvb + dabulk_dvb * vdseff) / vgst2vtm;

    let fgche1 = vgsteff * t0;
    let dfgche1_dvg = vgsteff * dt0_dvg + t0;
    let dfgche1_dvd = vgsteff * dt0_dvd;
    let dfgche1_dvb = vgsteff * dt0_dvb;

    t9 = vdseff / esat_l;
    let fgche2 = 1.0 + t9;
    let dfgche2_dvg = (dvdseff_dvg - t9 * desat_l_dvg) / esat_l;
    let dfgche2_dvd = (dvdseff_dvd - t9 * desat_l_dvd) / esat_l;
    let dfgche2_dvb = (dvdseff_dvb - t9 * desat_l_dvb) / esat_l;

    let gche = beta * fgche1 / fgche2;
    let dgche_dvg = (beta * dfgche1_dvg + fgche1 * dbeta_dvg - gche * dfgche2_dvg) / fgche2;
    let dgche_dvd = (beta * dfgche1_dvd + fgche1 * dbeta_dvd - gche * dfgche2_dvd) / fgche2;
    let dgche_dvb = (beta * dfgche1_dvb + fgche1 * dbeta_dvb - gche * dfgche2_dvb) / fgche2;

    t0 = 1.0 + gche * rds;
    t9 = vdseff / t0;
    let idl = gche * t9;

    let didl_dvg = (gche * dvdseff_dvg + t9 * dgche_dvg) / t0 - idl * gche / t0 * drds_dvg;

    let didl_dvd = (gche * dvdseff_dvd + t9 * dgche_dvd) / t0;
    let didl_dvb = (gche * dvdseff_dvb + t9 * dgche_dvb - idl * drds_dvb * gche) / t0;

    t9 = diff_vds / va;
    t0 = 1.0 + t9;
    let idsa = idl * t0;
    let didsa_dvg = t0 * didl_dvg - idl * (dvdseff_dvg + t9 * dva_dvg) / va;
    let didsa_dvd = t0 * didl_dvd + idl * (1.0 - dvdseff_dvd - t9 * dva_dvd) / va;
    let didsa_dvb = t0 * didl_dvb - idl * (dvdseff_dvb + t9 * dva_dvb) / va;

    t9 = diff_vds / vascbe;
    t0 = 1.0 + t9;
    let ids = idsa * t0;

    let mut gm = t0 * didsa_dvg - idsa * (dvdseff_dvg + t9 * dvascbe_dvg) / vascbe;
    let mut gds = t0 * didsa_dvd + idsa * (1.0 - dvdseff_dvd - t9 * dvascbe_dvd) / vascbe;
    let mut gmb = t0 * didsa_dvb - idsa * (dvdseff_dvb + t9 * dvascbe_dvb) / vascbe;

    gds += gm * dvgsteff_dvd;
    gmb += gm * dvgsteff_dvb;
    gm *= dvgsteff_dvg;
    gmb *= dvbseff_dvb;

    // Substrate current begins.
    let (isub, mut gbd, mut gbb, mut gbg);
    tmp = pp.alpha0 + pp.alpha1 * leff;
    if tmp <= 0.0 || pp.beta0 <= 0.0 {
        isub = 0.0;
        gbd = 0.0;
        gbb = 0.0;
        gbg = 0.0;
    } else {
        t2 = tmp / leff;
        if diff_vds > pp.beta0 / EXP_THRESHOLD {
            t0 = -pp.beta0 / diff_vds;
            t1 = t2 * diff_vds * t0.exp();
            t3 = t1 / diff_vds * (t0 - 1.0);
            dt1_dvg = t3 * dvdseff_dvg;
            dt1_dvd = t3 * (dvdseff_dvd - 1.0);
            dt1_dvb = t3 * dvdseff_dvb;
        } else {
            t3 = t2 * MIN_EXP;
            t1 = t3 * diff_vds;
            dt1_dvg = -t3 * dvdseff_dvg;
            dt1_dvd = t3 * (1.0 - dvdseff_dvd);
            dt1_dvb = -t3 * dvdseff_dvb;
        }
        isub = t1 * idsa;
        gbg = t1 * didsa_dvg + idsa * dt1_dvg;
        gbd = t1 * didsa_dvd + idsa * dt1_dvd;
        gbb = t1 * didsa_dvb + idsa * dt1_dvb;

        gbd += gbg * dvgsteff_dvd;
        gbb += gbg * dvgsteff_dvb;
        gbg *= dvgsteff_dvg;
        gbb *= dvbseff_dvb; // bug fixing
    }

    e.ids = ids;
    e.gds = gds;
    e.gm = gm;
    e.gmbs = gmb;
    e.gbbs = gbb;
    e.gbgs = gbg;
    e.gbds = gbd;
    e.isub = isub;

    // BSIM3 thermal noise Qinv calculated from all capMod 0, 1, 2 & 3
    // stored in here->BSIM3qinv 1/1998.
    if model.xpart < 0.0 || !charges {
        // qgate = qdrn = qsrc = qbulk = 0 and all capacitances 0
        // (Ev::default()); BSIM3qinv untouched.
        return e;
    }

    let (mut qgate, mut qbulk, qdrn, qsrc);
    let cox_wl;
    if model.cap_mod == 0 {
        if vbseff < 0.0 {
            vbseff = vbs;
            dvbseff_dvb = 1.0;
        } else {
            vbseff = pp.phi - phis;
            dvbseff_dvb = -dphis_dvb;
        }

        let vfb = pp.vfbcv;
        let vth = vfb + pp.phi + pp.k1ox * sqrt_phis;
        vgst = vgs_eff - vth;
        let dvth_dvb = pp.k1ox * dsqrt_phis_dvb;
        let _dvgst_dvb = -dvth_dvb;
        let _dvgst_dvg = dvgs_eff_dvg;

        cox_wl = cox * pp.weff_cv * pp.leff_cv;
        let arg1 = vgs_eff - vbseff - vfb;

        if arg1 <= 0.0 {
            qgate = cox_wl * arg1;
            qbulk = -qgate;
            qdrn = 0.0;

            e.cggb = cox_wl * dvgs_eff_dvg;
            e.cgdb = 0.0;
            e.cgsb = cox_wl * (dvbseff_dvb - dvgs_eff_dvg);

            e.cdgb = 0.0;
            e.cddb = 0.0;
            e.cdsb = 0.0;

            e.cbgb = -cox_wl * dvgs_eff_dvg;
            e.cbdb = 0.0;
            e.cbsb = -e.cgsb;
            e.qinv = Some(0.0);
        } else if vgst <= 0.0 {
            t1 = 0.5 * pp.k1ox;
            t2 = f64::sqrt(t1 * t1 + arg1);
            qgate = cox_wl * pp.k1ox * (t2 - t1);
            qbulk = -qgate;
            qdrn = 0.0;

            t0 = cox_wl * t1 / t2;
            e.cggb = t0 * dvgs_eff_dvg;
            e.cgdb = 0.0;
            e.cgsb = t0 * (dvbseff_dvb - dvgs_eff_dvg);

            e.cdgb = 0.0;
            e.cddb = 0.0;
            e.cdsb = 0.0;

            e.cbgb = -e.cggb;
            e.cbdb = 0.0;
            e.cbsb = -e.cgsb;
            e.qinv = Some(0.0);
        } else {
            let one_third_cox_wl = cox_wl / 3.0;
            let two_third_cox_wl = 2.0 * one_third_cox_wl;

            let abulk_cv = abulk0 * pp.abulk_cv_factor;
            let dabulk_cv_dvb = pp.abulk_cv_factor * dabulk0_dvb;
            vdsat = vgst / abulk_cv;
            let dvdsat_dvg = dvgs_eff_dvg / abulk_cv;
            let dvdsat_dvb = -(vdsat * dabulk_cv_dvb + dvth_dvb) / abulk_cv;

            if model.xpart > 0.5 {
                // 0/100 charge partition model.
                if vdsat <= vds {
                    // Saturation region.
                    t1 = vdsat / 3.0;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - t1);
                    t2 = -two_third_cox_wl * vgst;
                    qbulk = -(qgate + t2);
                    qdrn = 0.0;

                    e.cggb = one_third_cox_wl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg;
                    t2 = -one_third_cox_wl * dvdsat_dvb;
                    e.cgsb = -(e.cggb + t2);
                    e.cgdb = 0.0;

                    e.cdgb = 0.0;
                    e.cddb = 0.0;
                    e.cdsb = 0.0;

                    e.cbgb = -(e.cggb - two_third_cox_wl * dvgs_eff_dvg);
                    t3 = -(t2 + two_third_cox_wl * dvth_dvb);
                    e.cbsb = -(e.cbgb + t3);
                    e.cbdb = 0.0;
                    e.qinv = Some(-(qgate + qbulk));
                } else {
                    // Linear region.
                    let alphaz = vgst / vdsat;
                    t1 = 2.0 * vdsat - vds;
                    t2 = vds / (3.0 * t1);
                    t3 = t2 * vds;
                    t9 = 0.25 * cox_wl;
                    t4 = t9 * alphaz;
                    t7 = 2.0 * vds - t1 - 3.0 * t3;
                    t8 = t3 - t1 - 2.0 * vds;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - 0.5 * (vds - t3));
                    t10 = t4 * t8;
                    qdrn = t4 * t7;
                    qbulk = -(qgate + qdrn + t10);

                    t5 = t3 / t1;
                    e.cggb = cox_wl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg;
                    t11 = -cox_wl * t5 * dvdsat_dvb;
                    e.cgdb = cox_wl * (t2 - 0.5 + 0.5 * t5);
                    e.cgsb = -(e.cggb + t11 + e.cgdb);
                    t6 = 1.0 / vdsat;
                    let dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                    let dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);
                    t7 *= t9;
                    t8 *= t9;
                    t9 = 2.0 * t4 * (1.0 - 3.0 * t5);
                    e.cdgb = (t7 * dalphaz_dvg - t9 * dvdsat_dvg) * dvgs_eff_dvg;
                    t12 = t7 * dalphaz_dvb - t9 * dvdsat_dvb;
                    e.cddb = t4 * (3.0 - 6.0 * t2 - 3.0 * t5);
                    e.cdsb = -(e.cdgb + t12 + e.cddb);

                    t9 = 2.0 * t4 * (1.0 + t5);
                    t10 = (t8 * dalphaz_dvg - t9 * dvdsat_dvg) * dvgs_eff_dvg;
                    t11 = t8 * dalphaz_dvb - t9 * dvdsat_dvb;
                    t12 = t4 * (2.0 * t2 + t5 - 1.0);
                    t0 = -(t10 + t11 + t12);

                    e.cbgb = -(e.cggb + e.cdgb + t10);
                    e.cbdb = -(e.cgdb + e.cddb + t12);
                    e.cbsb = -(e.cgsb + e.cdsb + t0);
                    e.qinv = Some(-(qgate + qbulk));
                }
            } else if model.xpart < 0.5 {
                // 40/60 charge partition model.
                if vds >= vdsat {
                    // Saturation region.
                    t1 = vdsat / 3.0;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - t1);
                    t2 = -two_third_cox_wl * vgst;
                    qbulk = -(qgate + t2);
                    qdrn = 0.4 * t2;

                    e.cggb = one_third_cox_wl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg;
                    t2 = -one_third_cox_wl * dvdsat_dvb;
                    e.cgsb = -(e.cggb + t2);
                    e.cgdb = 0.0;

                    t3 = 0.4 * two_third_cox_wl;
                    e.cdgb = -t3 * dvgs_eff_dvg;
                    e.cddb = 0.0;
                    t4 = t3 * dvth_dvb;
                    e.cdsb = -(t4 + e.cdgb);

                    e.cbgb = -(e.cggb - two_third_cox_wl * dvgs_eff_dvg);
                    t3 = -(t2 + two_third_cox_wl * dvth_dvb);
                    e.cbsb = -(e.cbgb + t3);
                    e.cbdb = 0.0;
                    e.qinv = Some(-(qgate + qbulk));
                } else {
                    // Linear region.
                    let alphaz = vgst / vdsat;
                    t1 = 2.0 * vdsat - vds;
                    t2 = vds / (3.0 * t1);
                    t3 = t2 * vds;
                    t9 = 0.25 * cox_wl;
                    t4 = t9 * alphaz;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - 0.5 * (vds - t3));

                    t5 = t3 / t1;
                    e.cggb = cox_wl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg;
                    tmp = -cox_wl * t5 * dvdsat_dvb;
                    e.cgdb = cox_wl * (t2 - 0.5 + 0.5 * t5);
                    e.cgsb = -(e.cggb + e.cgdb + tmp);

                    t6 = 1.0 / vdsat;
                    let dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                    let dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);

                    t6 = 8.0 * vdsat * vdsat - 6.0 * vdsat * vds + 1.2 * vds * vds;
                    t8 = t2 / t1;
                    t7 = vds - t1 - t8 * t6;
                    qdrn = t4 * t7;
                    t7 *= t9;
                    tmp = t8 / t1;
                    tmp1 = t4 * (2.0 - 4.0 * tmp * t6 + t8 * (16.0 * vdsat - 6.0 * vds));

                    e.cdgb = (t7 * dalphaz_dvg - tmp1 * dvdsat_dvg) * dvgs_eff_dvg;
                    t10 = t7 * dalphaz_dvb - tmp1 * dvdsat_dvb;
                    e.cddb = t4
                        * (2.0 - (1.0 / (3.0 * t1 * t1) + 2.0 * tmp) * t6
                            + t8 * (6.0 * vdsat - 2.4 * vds));
                    e.cdsb = -(e.cdgb + t10 + e.cddb);

                    t7 = 2.0 * (t1 + t3);
                    qbulk = -(qgate - t4 * t7);
                    t7 *= t9;
                    t0 = 4.0 * t4 * (1.0 - t5);
                    t12 = (-t7 * dalphaz_dvg - e.cdgb - t0 * dvdsat_dvg) * dvgs_eff_dvg;
                    t11 = -t7 * dalphaz_dvb - t10 - t0 * dvdsat_dvb;
                    t10 = -4.0 * t4 * (t2 - 0.5 + 0.5 * t5) - e.cddb;
                    tmp = -(t10 + t11 + t12);

                    e.cbgb = -(e.cggb + e.cdgb + t12);
                    e.cbdb = -(e.cgdb + e.cddb + t10); // bug fix
                    e.cbsb = -(e.cgsb + e.cdsb + tmp);
                    e.qinv = Some(-(qgate + qbulk));
                }
            } else {
                // 50/50 partitioning.
                if vds >= vdsat {
                    // Saturation region.
                    t1 = vdsat / 3.0;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - t1);
                    t2 = -two_third_cox_wl * vgst;
                    qbulk = -(qgate + t2);
                    qdrn = 0.5 * t2;

                    e.cggb = one_third_cox_wl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg;
                    t2 = -one_third_cox_wl * dvdsat_dvb;
                    e.cgsb = -(e.cggb + t2);
                    e.cgdb = 0.0;

                    e.cdgb = -one_third_cox_wl * dvgs_eff_dvg;
                    e.cddb = 0.0;
                    t4 = one_third_cox_wl * dvth_dvb;
                    e.cdsb = -(t4 + e.cdgb);

                    e.cbgb = -(e.cggb - two_third_cox_wl * dvgs_eff_dvg);
                    t3 = -(t2 + two_third_cox_wl * dvth_dvb);
                    e.cbsb = -(e.cbgb + t3);
                    e.cbdb = 0.0;
                    e.qinv = Some(-(qgate + qbulk));
                } else {
                    // Linear region.
                    let alphaz = vgst / vdsat;
                    t1 = 2.0 * vdsat - vds;
                    t2 = vds / (3.0 * t1);
                    t3 = t2 * vds;
                    t9 = 0.25 * cox_wl;
                    t4 = t9 * alphaz;
                    qgate = cox_wl * (vgs_eff - vfb - pp.phi - 0.5 * (vds - t3));

                    t5 = t3 / t1;
                    e.cggb = cox_wl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg;
                    tmp = -cox_wl * t5 * dvdsat_dvb;
                    e.cgdb = cox_wl * (t2 - 0.5 + 0.5 * t5);
                    e.cgsb = -(e.cggb + e.cgdb + tmp);

                    t6 = 1.0 / vdsat;
                    let dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                    let dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);

                    t7 = t1 + t3;
                    qdrn = -t4 * t7;
                    qbulk = -(qgate + qdrn + qdrn);
                    t7 *= t9;
                    t0 = t4 * (2.0 * t5 - 2.0);

                    e.cdgb = (t0 * dvdsat_dvg - t7 * dalphaz_dvg) * dvgs_eff_dvg;
                    t12 = t0 * dvdsat_dvb - t7 * dalphaz_dvb;
                    e.cddb = t4 * (1.0 - 2.0 * t2 - t5);
                    e.cdsb = -(e.cdgb + t12 + e.cddb);

                    e.cbgb = -(e.cggb + 2.0 * e.cdgb);
                    e.cbdb = -(e.cgdb + 2.0 * e.cddb);
                    e.cbsb = -(e.cgsb + 2.0 * e.cdsb);
                    e.qinv = Some(-(qgate + qbulk));
                }
            }
        }
        e.qgate = qgate;
        e.qbulk = qbulk;
        e.qdrn = qdrn;
        return e;
    }

    // capMod 1, 2, 3.
    let (vbseff_cv, dvbseff_cv_dvb);
    if vbseff < 0.0 {
        vbseff_cv = vbseff;
        dvbseff_cv_dvb = 1.0;
    } else {
        vbseff_cv = pp.phi - phis;
        dvbseff_cv_dvb = -dphis_dvb;
    }

    cox_wl = cox * pp.weff_cv * pp.leff_cv;

    // Separate VgsteffCV with noff and voffcv.
    let noff = n * pp.noff;
    let dnoff_dvd = pp.noff * dn_dvd;
    let dnoff_dvb = pp.noff * dn_dvb;
    t0 = vtm * noff;
    let voffcv = pp.voffcv;
    vgst_nvt = (vgst - voffcv) / t0;

    if vgst_nvt > EXP_THRESHOLD {
        vgsteff = vgst - voffcv;
        dvgsteff_dvg = dvgs_eff_dvg;
        dvgsteff_dvd = -dvth_dvd;
        dvgsteff_dvb = -dvth_dvb;
    } else if vgst_nvt < -EXP_THRESHOLD {
        vgsteff = t0 * f64::ln(1.0 + MIN_EXP);
        dvgsteff_dvg = 0.0;
        dvgsteff_dvd = vgsteff / noff;
        dvgsteff_dvb = dvgsteff_dvd * dnoff_dvb;
        dvgsteff_dvd *= dnoff_dvd;
    } else {
        let exp_vgst = vgst_nvt.exp();
        vgsteff = t0 * f64::ln(1.0 + exp_vgst);
        dvgsteff_dvg = exp_vgst / (1.0 + exp_vgst);
        dvgsteff_dvd = -dvgsteff_dvg * (dvth_dvd + (vgst - voffcv) / noff * dnoff_dvd)
            + vgsteff / noff * dnoff_dvd;
        dvgsteff_dvb = -dvgsteff_dvg * (dvth_dvb + (vgst - voffcv) / noff * dnoff_dvb)
            + vgsteff / noff * dnoff_dvb;
        dvgsteff_dvg *= dvgs_eff_dvg;
    } // End of VgsteffCV

    let (mut cgg, mut cgd, mut cgb, mut cbg, mut cbd, mut cbb);
    let (mut csg, csd, mut csb);
    if model.cap_mod == 1 {
        let vfb = it.vfbzb;
        let arg1 = vgs_eff - vbseff_cv - vfb - vgsteff;

        if arg1 <= 0.0 {
            qgate = cox_wl * arg1;
            cgg = cox_wl * (dvgs_eff_dvg - dvgsteff_dvg);
            cgd = -cox_wl * dvgsteff_dvd;
            cgb = -cox_wl * (dvbseff_cv_dvb + dvgsteff_dvb);
        } else {
            t0 = 0.5 * pp.k1ox;
            t1 = f64::sqrt(t0 * t0 + arg1);
            t2 = cox_wl * t0 / t1;

            qgate = cox_wl * pp.k1ox * (t1 - t0);

            cgg = t2 * (dvgs_eff_dvg - dvgsteff_dvg);
            cgd = -t2 * dvgsteff_dvd;
            cgb = -t2 * (dvbseff_cv_dvb + dvgsteff_dvb);
        }
        qbulk = -qgate;
        cbg = -cgg;
        cbd = -cgd;
        cbb = -cgb;

        let one_third_cox_wl = cox_wl / 3.0;
        let two_third_cox_wl = 2.0 * one_third_cox_wl;
        let abulk_cv = abulk0 * pp.abulk_cv_factor;
        let dabulk_cv_dvb = pp.abulk_cv_factor * dabulk0_dvb;
        let vdsat_cv = vgsteff / abulk_cv;
        if vdsat_cv < vds {
            let dvdsat_cv_dvg = 1.0 / abulk_cv;
            let dvdsat_cv_dvb = -vdsat_cv * dabulk_cv_dvb / abulk_cv;
            t0 = vgsteff - vdsat_cv / 3.0;
            dt0_dvg = 1.0 - dvdsat_cv_dvg / 3.0;
            dt0_dvb = -dvdsat_cv_dvb / 3.0;
            qgate += cox_wl * t0;
            let mut cgg1 = cox_wl * dt0_dvg;
            let cgb1 = cox_wl * dt0_dvb + cgg1 * dvgsteff_dvb;
            let cgd1 = cgg1 * dvgsteff_dvd;
            cgg1 *= dvgsteff_dvg;
            cgg += cgg1;
            cgb += cgb1;
            cgd += cgd1;

            t0 = vdsat_cv - vgsteff;
            dt0_dvg = dvdsat_cv_dvg - 1.0;
            dt0_dvb = dvdsat_cv_dvb;
            qbulk += one_third_cox_wl * t0;
            let mut cbg1 = one_third_cox_wl * dt0_dvg;
            let cbb1 = one_third_cox_wl * dt0_dvb + cbg1 * dvgsteff_dvb;
            let cbd1 = cbg1 * dvgsteff_dvd;
            cbg1 *= dvgsteff_dvg;
            cbg += cbg1;
            cbb += cbb1;
            cbd += cbd1;

            t0 = if model.xpart > 0.5 {
                -two_third_cox_wl
            } else if model.xpart < 0.5 {
                -0.4 * cox_wl
            } else {
                -one_third_cox_wl
            };

            qsrc = t0 * vgsteff;
            csg = t0 * dvgsteff_dvg;
            csb = t0 * dvgsteff_dvb;
            csd = t0 * dvgsteff_dvd;
            cgb *= dvbseff_dvb;
            cbb *= dvbseff_dvb;
            csb *= dvbseff_dvb;
        } else {
            t0 = abulk_cv * vds;
            t1 = 12.0 * (vgsteff - 0.5 * t0 + 1.0e-20);
            t2 = vds / t1;
            t3 = t0 * t2;
            dt3_dvg = -12.0 * t2 * t2 * abulk_cv;
            dt3_dvd = 6.0 * t0 * (4.0 * vgsteff - t0) / t1 / t1 - 0.5;
            dt3_dvb = 12.0 * t2 * t2 * dabulk_cv_dvb * vgsteff;

            qgate += cox_wl * (vgsteff - 0.5 * vds + t3);
            let mut cgg1 = cox_wl * (1.0 + dt3_dvg);
            let cgb1 = cox_wl * dt3_dvb + cgg1 * dvgsteff_dvb;
            let cgd1 = cox_wl * dt3_dvd + cgg1 * dvgsteff_dvd;
            cgg1 *= dvgsteff_dvg;
            cgg += cgg1;
            cgb += cgb1;
            cgd += cgd1;

            qbulk += cox_wl * (1.0 - abulk_cv) * (0.5 * vds - t3);
            let mut cbg1 = -cox_wl * ((1.0 - abulk_cv) * dt3_dvg);
            let cbb1 = -cox_wl * ((1.0 - abulk_cv) * dt3_dvb + (0.5 * vds - t3) * dabulk_cv_dvb)
                + cbg1 * dvgsteff_dvb;
            let cbd1 = -cox_wl * (1.0 - abulk_cv) * dt3_dvd + cbg1 * dvgsteff_dvd;
            cbg1 *= dvgsteff_dvg;
            cbg += cbg1;
            cbb += cbb1;
            cbd += cbd1;

            if model.xpart > 0.5 {
                // 0/100 charge petition model.
                t1 += t1;
                qsrc = -cox_wl * (0.5 * vgsteff + 0.25 * t0 - t0 * t0 / t1);
                csg = -cox_wl * (0.5 + 24.0 * t0 * vds / t1 / t1 * abulk_cv);
                csb = -cox_wl
                    * (0.25 * vds * dabulk_cv_dvb
                        - 12.0 * t0 * vds / t1 / t1 * (4.0 * vgsteff - t0) * dabulk_cv_dvb)
                    + csg * dvgsteff_dvb;
                csd = -cox_wl
                    * (0.25 * abulk_cv - 12.0 * abulk_cv * t0 / t1 / t1 * (4.0 * vgsteff - t0))
                    + csg * dvgsteff_dvd;
                csg *= dvgsteff_dvg;
            } else if model.xpart < 0.5 {
                // 40/60 charge petition model.
                t1 /= 12.0;
                t2 = 0.5 * cox_wl / (t1 * t1);
                t3 = vgsteff * (2.0 * t0 * t0 / 3.0 + vgsteff * (vgsteff - 4.0 * t0 / 3.0))
                    - 2.0 * t0 * t0 * t0 / 15.0;
                qsrc = -t2 * t3;
                t4 = 4.0 / 3.0 * vgsteff * (vgsteff - t0) + 0.4 * t0 * t0;
                csg = -2.0 * qsrc / t1
                    - t2 * (vgsteff * (3.0 * vgsteff - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
                csb = (qsrc / t1 * vds + t2 * t4 * vds) * dabulk_cv_dvb + csg * dvgsteff_dvb;
                csd = (qsrc / t1 + t2 * t4) * abulk_cv + csg * dvgsteff_dvd;
                csg *= dvgsteff_dvg;
            } else {
                // 50/50 charge petition model.
                qsrc = -0.5 * (qgate + qbulk);
                csg = -0.5 * (cgg1 + cbg1);
                csb = -0.5 * (cgb1 + cbb1);
                csd = -0.5 * (cgd1 + cbd1);
            }
            cgb *= dvbseff_dvb;
            cbb *= dvbseff_dvb;
            csb *= dvbseff_dvb;
        }
        let qdrn = -(qgate + qbulk + qsrc);
        e.cggb = cgg;
        e.cgsb = -(cgg + cgd + cgb);
        e.cgdb = cgd;
        e.cdgb = -(cgg + cbg + csg);
        e.cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
        e.cddb = -(cgd + cbd + csd);
        e.cbgb = cbg;
        e.cbsb = -(cbg + cbd + cbb);
        e.cbdb = cbd;
        e.qinv = Some(-(qgate + qbulk));
        e.qgate = qgate;
        e.qbulk = qbulk;
        e.qdrn = qdrn;
        return e;
    }

    if model.cap_mod == 2 {
        let vfb = it.vfbzb;
        let v3 = vfb - vgs_eff + vbseff_cv - DELTA_3;
        if vfb <= 0.0 {
            t0 = f64::sqrt(v3 * v3 - 4.0 * DELTA_3 * vfb);
            t2 = -DELTA_3 / t0;
        } else {
            t0 = f64::sqrt(v3 * v3 + 4.0 * DELTA_3 * vfb);
            t2 = DELTA_3 / t0;
        }

        t1 = 0.5 * (1.0 + v3 / t0);
        let vfbeff = vfb - 0.5 * (v3 + t0);
        let dvfbeff_dvg = t1 * dvgs_eff_dvg;
        let dvfbeff_dvb = -t1 * dvbseff_cv_dvb;
        let qac0 = cox_wl * (vfbeff - vfb);
        let dqac0_dvg = cox_wl * dvfbeff_dvg;
        let dqac0_dvb = cox_wl * dvfbeff_dvb;

        t0 = 0.5 * pp.k1ox;
        t3 = vgs_eff - vfbeff - vbseff_cv - vgsteff;
        if pp.k1ox == 0.0 {
            t1 = 0.0;
            t2 = 0.0;
        } else if t3 < 0.0 {
            t1 = t0 + t3 / pp.k1ox;
            t2 = cox_wl;
        } else {
            t1 = f64::sqrt(t0 * t0 + t3);
            t2 = cox_wl * t0 / t1;
        }

        let qsub0 = cox_wl * pp.k1ox * (t1 - t0);

        let dqsub0_dvg = t2 * (dvgs_eff_dvg - dvfbeff_dvg - dvgsteff_dvg);
        let dqsub0_dvd = -t2 * dvgsteff_dvd;
        let dqsub0_dvb = -t2 * (dvfbeff_dvb + dvbseff_cv_dvb + dvgsteff_dvb);

        let abulk_cv = abulk0 * pp.abulk_cv_factor;
        let dabulk_cv_dvb = pp.abulk_cv_factor * dabulk0_dvb;
        let vdsat_cv = vgsteff / abulk_cv;

        let v4 = vdsat_cv - vds - DELTA_4;
        t0 = f64::sqrt(v4 * v4 + 4.0 * DELTA_4 * vdsat_cv);
        let mut vdseff_cv = vdsat_cv - 0.5 * (v4 + t0);
        t1 = 0.5 * (1.0 + v4 / t0);
        t2 = DELTA_4 / t0;
        t3 = (1.0 - t1 - t2) / abulk_cv;
        let mut dvdseff_cv_dvg = t3;
        let dvdseff_cv_dvd = t1;
        let mut dvdseff_cv_dvb = -t3 * vdsat_cv * dabulk_cv_dvb;
        // Added to eliminate non-zero VdseffCV at Vds=0.0.
        if vds == 0.0 {
            vdseff_cv = 0.0;
            dvdseff_cv_dvg = 0.0;
            dvdseff_cv_dvb = 0.0;
        }

        t0 = abulk_cv * vdseff_cv;
        t1 = 12.0 * (vgsteff - 0.5 * t0 + 1e-20);
        t2 = vdseff_cv / t1;
        t3 = t0 * t2;

        t4 = 1.0 - 12.0 * t2 * t2 * abulk_cv;
        t5 = 6.0 * t0 * (4.0 * vgsteff - t0) / (t1 * t1) - 0.5;
        t6 = 12.0 * t2 * t2 * vgsteff;

        let qinoi = -cox_wl * (vgsteff - 0.5 * t0 + abulk_cv * t3);
        qgate = cox_wl * (vgsteff - 0.5 * vdseff_cv + t3);
        let mut cgg1 = cox_wl * (t4 + t5 * dvdseff_cv_dvg);
        let cgd1 = cox_wl * t5 * dvdseff_cv_dvd + cgg1 * dvgsteff_dvd;
        let cgb1 = cox_wl * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb) + cgg1 * dvgsteff_dvb;
        cgg1 *= dvgsteff_dvg;

        t7 = 1.0 - abulk_cv;
        qbulk = cox_wl * t7 * (0.5 * vdseff_cv - t3);
        t4 = -t7 * (t4 - 1.0);
        t5 *= -t7;
        t6 = -(t7 * t6 + (0.5 * vdseff_cv - t3));
        let mut cbg1 = cox_wl * (t4 + t5 * dvdseff_cv_dvg);
        let cbd1 = cox_wl * t5 * dvdseff_cv_dvd + cbg1 * dvgsteff_dvd;
        let cbb1 = cox_wl * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb) + cbg1 * dvgsteff_dvb;
        cbg1 *= dvgsteff_dvg;

        if model.xpart > 0.5 {
            // 0/100 charge petition model.
            t1 += t1;
            qsrc = -cox_wl * (0.5 * vgsteff + 0.25 * t0 - t0 * t0 / t1);
            t7 = (4.0 * vgsteff - t0) / (t1 * t1);
            t4 = -(0.5 + 24.0 * t0 * t0 / (t1 * t1));
            t5 = -(0.25 * abulk_cv - 12.0 * abulk_cv * t0 * t7);
            t6 = -(0.25 * vdseff_cv - 12.0 * t0 * vdseff_cv * t7);
            csg = cox_wl * (t4 + t5 * dvdseff_cv_dvg);
            csd = cox_wl * t5 * dvdseff_cv_dvd + csg * dvgsteff_dvd;
            csb = cox_wl * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb) + csg * dvgsteff_dvb;
            csg *= dvgsteff_dvg;
        } else if model.xpart < 0.5 {
            // 40/60 charge petition model.
            t1 /= 12.0;
            t2 = 0.5 * cox_wl / (t1 * t1);
            t3 = vgsteff * (2.0 * t0 * t0 / 3.0 + vgsteff * (vgsteff - 4.0 * t0 / 3.0))
                - 2.0 * t0 * t0 * t0 / 15.0;
            qsrc = -t2 * t3;
            t7 = 4.0 / 3.0 * vgsteff * (vgsteff - t0) + 0.4 * t0 * t0;
            t4 = -2.0 * qsrc / t1
                - t2 * (vgsteff * (3.0 * vgsteff - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
            t5 = (qsrc / t1 + t2 * t7) * abulk_cv;
            t6 = qsrc / t1 * vdseff_cv + t2 * t7 * vdseff_cv;
            csg = t4 + t5 * dvdseff_cv_dvg;
            csd = t5 * dvdseff_cv_dvd + csg * dvgsteff_dvd;
            csb = (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb) + csg * dvgsteff_dvb;
            csg *= dvgsteff_dvg;
        } else {
            // 50/50 charge petition model.
            qsrc = -0.5 * (qgate + qbulk);
            csg = -0.5 * (cgg1 + cbg1);
            csb = -0.5 * (cgb1 + cbb1);
            csd = -0.5 * (cgd1 + cbd1);
        }

        qgate += qac0 + qsub0;
        qbulk -= qac0 + qsub0;
        let qdrn = -(qgate + qbulk + qsrc);

        cgg = dqac0_dvg + dqsub0_dvg + cgg1;
        cgd = dqsub0_dvd + cgd1;
        cgb = dqac0_dvb + dqsub0_dvb + cgb1;

        cbg = cbg1 - dqac0_dvg - dqsub0_dvg;
        cbd = cbd1 - dqsub0_dvd;
        cbb = cbb1 - dqac0_dvb - dqsub0_dvb;

        cgb *= dvbseff_dvb;
        cbb *= dvbseff_dvb;
        csb *= dvbseff_dvb;

        e.cggb = cgg;
        e.cgsb = -(cgg + cgd + cgb);
        e.cgdb = cgd;
        e.cdgb = -(cgg + cbg + csg);
        e.cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
        e.cddb = -(cgd + cbd + csd);
        e.cbgb = cbg;
        e.cbsb = -(cbg + cbd + cbb);
        e.cbdb = cbd;
        e.qinv = Some(qinoi);
        e.qgate = qgate;
        e.qbulk = qbulk;
        e.qdrn = qdrn;
        return e;
    }

    if model.cap_mod == 3 {
        // New Charge-Thickness capMod (CTM) begins.
        let mut v3 = it.vfbzb - vgs_eff + vbseff_cv - DELTA_3;
        if it.vfbzb <= 0.0 {
            t0 = f64::sqrt(v3 * v3 - 4.0 * DELTA_3 * it.vfbzb);
            t2 = -DELTA_3 / t0;
        } else {
            t0 = f64::sqrt(v3 * v3 + 4.0 * DELTA_3 * it.vfbzb);
            t2 = DELTA_3 / t0;
        }

        t1 = 0.5 * (1.0 + v3 / t0);
        let vfbeff = it.vfbzb - 0.5 * (v3 + t0);
        let dvfbeff_dvg = t1 * dvgs_eff_dvg;
        let dvfbeff_dvb = -t1 * dvbseff_cv_dvb;

        let cox_c = cox;
        let mut tox_c = 1.0e8 * tox;
        t0 = (vgs_eff - vbseff_cv - it.vfbzb) / tox_c;
        dt0_dvg = dvgs_eff_dvg / tox_c;
        dt0_dvb = -dvbseff_cv_dvb / tox_c;

        let (mut tcen, mut dtcen_dvg, mut dtcen_dvd, mut dtcen_dvb);
        tmp = t0 * pp.acde;
        if -EXP_THRESHOLD < tmp && tmp < EXP_THRESHOLD {
            tcen = pp.ldeb * tmp.exp();
            dtcen_dvg = pp.acde * tcen;
            dtcen_dvb = dtcen_dvg * dt0_dvb;
            dtcen_dvg *= dt0_dvg;
        } else if tmp <= -EXP_THRESHOLD {
            tcen = pp.ldeb * MIN_EXP;
            dtcen_dvg = 0.0;
            dtcen_dvb = 0.0;
        } else {
            tcen = pp.ldeb * MAX_EXP;
            dtcen_dvg = 0.0;
            dtcen_dvb = 0.0;
        }

        let link = 1.0e-3 * tox;
        v3 = pp.ldeb - tcen - link;
        let v4 = f64::sqrt(v3 * v3 + 4.0 * link * pp.ldeb);
        tcen = pp.ldeb - 0.5 * (v3 + v4);
        t1 = 0.5 * (1.0 + v3 / v4);
        dtcen_dvg *= t1;
        dtcen_dvb *= t1;

        let mut ccen = EPSSI / tcen;
        t2 = cox_c / (cox_c + ccen);
        let mut coxeff = t2 * ccen;
        t3 = -ccen / tcen;
        let mut dcoxeff_dvg = t2 * t2 * t3;
        let mut dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb;
        dcoxeff_dvg *= dtcen_dvg;
        let mut cox_wlcen = cox_wl * coxeff / cox_c;

        let qac0 = cox_wlcen * (vfbeff - it.vfbzb);
        let mut qov_cox = qac0 / coxeff;
        let dqac0_dvg = cox_wlcen * dvfbeff_dvg + qov_cox * dcoxeff_dvg;
        let dqac0_dvb = cox_wlcen * dvfbeff_dvb + qov_cox * dcoxeff_dvb;

        t0 = 0.5 * pp.k1ox;
        t3 = vgs_eff - vfbeff - vbseff_cv - vgsteff;
        if pp.k1ox == 0.0 {
            t1 = 0.0;
            t2 = 0.0;
        } else if t3 < 0.0 {
            t1 = t0 + t3 / pp.k1ox;
            t2 = cox_wlcen;
        } else {
            t1 = f64::sqrt(t0 * t0 + t3);
            t2 = cox_wlcen * t0 / t1;
        }

        let qsub0 = cox_wlcen * pp.k1ox * (t1 - t0);
        qov_cox = qsub0 / coxeff;
        let dqsub0_dvg = t2 * (dvgs_eff_dvg - dvfbeff_dvg - dvgsteff_dvg) + qov_cox * dcoxeff_dvg;
        let dqsub0_dvd = -t2 * dvgsteff_dvd;
        let dqsub0_dvb =
            -t2 * (dvfbeff_dvb + dvbseff_cv_dvb + dvgsteff_dvb) + qov_cox * dcoxeff_dvb;

        // Gate-bias dependent delta Phis begins.
        let denomi;
        if pp.k1ox <= 0.0 {
            denomi = 0.25 * pp.moin * vtm;
            t0 = 0.5 * pp.sqrt_phi;
        } else {
            denomi = pp.moin * vtm * pp.k1ox * pp.k1ox;
            t0 = pp.k1ox * pp.sqrt_phi;
        }
        t1 = 2.0 * t0 + vgsteff;

        let delta_phi = vtm * f64::ln(1.0 + t1 * vgsteff / denomi);
        let ddelta_phi_dvg = 2.0 * vtm * (t1 - t0) / (denomi + t1 * vgsteff);
        // End of delta Phis.

        // VgDP = Vgsteff - DeltaPhi
        t0 = vgsteff - delta_phi - 0.001;
        dt0_dvg = 1.0 - ddelta_phi_dvg;
        t1 = f64::sqrt(t0 * t0 + vgsteff * 0.004);
        let vg_dp = 0.5 * (t0 + t1);
        let dvg_dp_dvg = 0.5 * (dt0_dvg + (t0 * dt0_dvg + 0.002) / t1);

        t3 = 4.0 * (vth - it.vfbzb - pp.phi);
        tox_c += tox_c;
        if t3 >= 0.0 {
            t0 = (vgsteff + t3) / tox_c;
            dt0_dvd = (dvgsteff_dvd + 4.0 * dvth_dvd) / tox_c;
            dt0_dvb = (dvgsteff_dvb + 4.0 * dvth_dvb) / tox_c;
        } else {
            t0 = (vgsteff + 1.0e-20) / tox_c;
            dt0_dvd = dvgsteff_dvd / tox_c;
            dt0_dvb = dvgsteff_dvb / tox_c;
        }
        tmp = f64::exp(0.7 * t0.ln());
        t1 = 1.0 + tmp;
        t2 = 0.7 * tmp / (t0 * tox_c);
        tcen = 1.9e-9 / t1;
        dtcen_dvg = -1.9e-9 * t2 / t1 / t1;
        dtcen_dvd = tox_c * dtcen_dvg;
        dtcen_dvb = dtcen_dvd * dt0_dvb;
        dtcen_dvd *= dt0_dvd;
        dtcen_dvg *= dvgsteff_dvg;

        ccen = EPSSI / tcen;
        t0 = cox_c / (cox_c + ccen);
        coxeff = t0 * ccen;
        t1 = -ccen / tcen;
        dcoxeff_dvg = t0 * t0 * t1;
        let dcoxeff_dvd = dcoxeff_dvg * dtcen_dvd;
        dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb;
        dcoxeff_dvg *= dtcen_dvg;
        cox_wlcen = cox_wl * coxeff / cox_c;

        let abulk_cv = abulk0 * pp.abulk_cv_factor;
        let dabulk_cv_dvb = pp.abulk_cv_factor * dabulk0_dvb;
        let vdsat_cv = vg_dp / abulk_cv;
        t0 = vdsat_cv - vds - DELTA_4;
        dt0_dvg = dvg_dp_dvg / abulk_cv;
        dt0_dvb = -vdsat_cv * dabulk_cv_dvb / abulk_cv;
        t1 = f64::sqrt(t0 * t0 + 4.0 * DELTA_4 * vdsat_cv);
        dt1_dvg = (t0 + DELTA_4 + DELTA_4) / t1;
        dt1_dvd = -t0 / t1;
        dt1_dvb = dt1_dvg * dt0_dvb;
        dt1_dvg *= dt0_dvg;
        let (mut vdseff_cv, mut dvdseff_cv_dvg, dvdseff_cv_dvd, mut dvdseff_cv_dvb);
        if t0 >= 0.0 {
            vdseff_cv = vdsat_cv - 0.5 * (t0 + t1);
            dvdseff_cv_dvg = 0.5 * (dt0_dvg - dt1_dvg);
            dvdseff_cv_dvd = 0.5 * (1.0 - dt1_dvd);
            dvdseff_cv_dvb = 0.5 * (dt0_dvb - dt1_dvb);
        } else {
            t3 = (DELTA_4 + DELTA_4) / (t1 - t0);
            t4 = 1.0 - t3;
            t5 = vdsat_cv * t3 / (t1 - t0);
            vdseff_cv = vdsat_cv * t4;
            dvdseff_cv_dvg = dt0_dvg * t4 + t5 * (dt1_dvg - dt0_dvg);
            dvdseff_cv_dvd = t5 * (dt1_dvd + 1.0);
            dvdseff_cv_dvb = dt0_dvb * (1.0 - t5) + t5 * dt1_dvb;
        }

        // Added to eliminate non-zero VdseffCV at Vds=0.0.
        if vds == 0.0 {
            vdseff_cv = 0.0;
            dvdseff_cv_dvg = 0.0;
            dvdseff_cv_dvb = 0.0;
        }

        t0 = abulk_cv * vdseff_cv;
        t1 = vg_dp;
        t2 = 12.0 * (t1 - 0.5 * t0 + 1.0e-20);
        t3 = t0 / t2;
        t4 = 1.0 - 12.0 * t3 * t3;
        t5 = abulk_cv * (6.0 * t0 * (4.0 * t1 - t0) / (t2 * t2) - 0.5);
        t6 = t5 * vdseff_cv / abulk_cv;

        qgate = cox_wlcen * (t1 - t0 * (0.5 - t3));
        let qinoi = qgate;
        qov_cox = qgate / coxeff;
        let mut cgg1 = cox_wlcen * (t4 * dvg_dp_dvg + t5 * dvdseff_cv_dvg);
        let cgd1 = cox_wlcen * t5 * dvdseff_cv_dvd + cgg1 * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
        let cgb1 = cox_wlcen * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb)
            + cgg1 * dvgsteff_dvb
            + qov_cox * dcoxeff_dvb;
        cgg1 = cgg1 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;

        t7 = 1.0 - abulk_cv;
        t8 = t2 * t2;
        t9 = 12.0 * t7 * t0 * t0 / (t8 * abulk_cv);
        t10 = t9 * dvg_dp_dvg;
        t11 = -t7 * t5 / abulk_cv;
        t12 = -(t9 * t1 / abulk_cv + vdseff_cv * (0.5 - t0 / t2));

        qbulk = cox_wlcen * t7 * (0.5 * vdseff_cv - t0 * vdseff_cv / t2);
        qov_cox = qbulk / coxeff;
        let mut cbg1 = cox_wlcen * (t10 + t11 * dvdseff_cv_dvg);
        let cbd1 = cox_wlcen * t11 * dvdseff_cv_dvd + cbg1 * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
        let cbb1 = cox_wlcen * (t11 * dvdseff_cv_dvb + t12 * dabulk_cv_dvb)
            + cbg1 * dvgsteff_dvb
            + qov_cox * dcoxeff_dvb;
        cbg1 = cbg1 * dvgsteff_dvg + qov_cox * dcoxeff_dvg;

        if model.xpart > 0.5 {
            // 0/100 partition.
            qsrc = -cox_wlcen * (t1 / 2.0 + t0 / 4.0 - 0.5 * t0 * t0 / t2);
            qov_cox = qsrc / coxeff;
            t2 += t2;
            t3 = t2 * t2;
            t7 = -(0.25 - 12.0 * t0 * (4.0 * t1 - t0) / t3);
            t4 = -(0.5 + 24.0 * t0 * t0 / t3) * dvg_dp_dvg;
            t5 = t7 * abulk_cv;
            t6 = t7 * vdseff_cv;

            csg = cox_wlcen * (t4 + t5 * dvdseff_cv_dvg);
            csd = cox_wlcen * t5 * dvdseff_cv_dvd + csg * dvgsteff_dvd + qov_cox * dcoxeff_dvd;
            csb = cox_wlcen * (t5 * dvdseff_cv_dvb + t6 * dabulk_cv_dvb)
                + csg * dvgsteff_dvb
                + qov_cox * dcoxeff_dvb;
            csg = csg * dvgsteff_dvg + qov_cox * dcoxeff_dvg;
        } else if model.xpart < 0.5 {
            // 40/60 partition.
            t2 /= 12.0;
            t3 = 0.5 * cox_wlcen / (t2 * t2);
            t4 =
                t1 * (2.0 * t0 * t0 / 3.0 + t1 * (t1 - 4.0 * t0 / 3.0)) - 2.0 * t0 * t0 * t0 / 15.0;
            qsrc = -t3 * t4;
            qov_cox = qsrc / coxeff;
            t8 = 4.0 / 3.0 * t1 * (t1 - t0) + 0.4 * t0 * t0;
            t5 = -2.0 * qsrc / t2 - t3 * (t1 * (3.0 * t1 - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
            t6 = abulk_cv * (qsrc / t2 + t3 * t8);
            t7 = t6 * vdseff_cv / abulk_cv;

            csg = t5 * dvg_dp_dvg + t6 * dvdseff_cv_dvg;
            csd = csg * dvgsteff_dvd + t6 * dvdseff_cv_dvd + qov_cox * dcoxeff_dvd;
            csb = csg * dvgsteff_dvb
                + t6 * dvdseff_cv_dvb
                + t7 * dabulk_cv_dvb
                + qov_cox * dcoxeff_dvb;
            csg = csg * dvgsteff_dvg + qov_cox * dcoxeff_dvg;
        } else {
            // 50/50 partition.
            qsrc = -0.5 * qgate;
            csg = -0.5 * cgg1;
            csd = -0.5 * cgd1;
            csb = -0.5 * cgb1;
        }

        qgate += qac0 + qsub0 - qbulk;
        qbulk -= qac0 + qsub0;
        let qdrn = -(qgate + qbulk + qsrc);

        cbg = cbg1 - dqac0_dvg - dqsub0_dvg;
        cbd = cbd1 - dqsub0_dvd;
        cbb = cbb1 - dqac0_dvb - dqsub0_dvb;

        cgg = cgg1 - cbg;
        cgd = cgd1 - cbd;
        cgb = cgb1 - cbb;

        cgb *= dvbseff_dvb;
        cbb *= dvbseff_dvb;
        csb *= dvbseff_dvb;

        e.cggb = cgg;
        e.cgsb = -(cgg + cgd + cgb);
        e.cgdb = cgd;
        e.cdgb = -(cgg + cbg + csg);
        e.cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
        e.cddb = -(cgd + cbd + csd);
        e.cbgb = cbg;
        e.cbsb = -(cbg + cbd + cbb);
        e.cbdb = cbd;
        e.qinv = Some(-qinoi);
        e.qgate = qgate;
        e.qbulk = qbulk;
        e.qdrn = qdrn;
        // End of CTM.
    }
    // capMod outside 0..=3 computes no charges (the C falls through
    // with qgate/qbulk/qdrn as initialized and the capacitances of
    // the previous call; the setup rejects such cards).
    e
}
