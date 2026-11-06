//! `BSIM3temp` (`b3temp.c`): the model-level temperature update and
//! the size-dependent parameter set (`struct bsim3SizeDependParam`,
//! the C's `pParam`) built once per drawn (L, W) and shared through
//! the model's knot list, plus the fatal half of `BSIM3checkModel`
//! (`b3check.c`).

use super::params::{Bsim3Model, EPSOX, EPSSI};

/// `KboQ` of `b3temp.c`.
pub(crate) const KBOQ: f64 = 8.617087e-5;
pub(crate) const MAX_EXP: f64 = 5.834617425e14;
pub(crate) const MIN_EXP: f64 = 1.713908431e-15;
pub(crate) const EXP_THRESHOLD: f64 = 34.0;
/// `Charge_q` of the BSIM3 files.
pub(crate) const CHARGE_Q: f64 = 1.60219e-19;

/// The model fields `BSIM3temp` derives at the circuit temperature
/// (`BSIM3vcrit`, `BSIM3factor1`, `BSIM3vtm`, the temperature-scaled
/// junction densities and potentials).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Bsim3MTemp {
    pub vcrit: f64,
    pub factor1: f64,
    pub vtm: f64,
    pub jct_temp_sat_cur_density: f64,
    pub jct_sidewall_temp_sat_cur_density: f64,
    pub unit_area_temp_jct_cap: f64,
    pub unit_length_sidewall_temp_jct_cap: f64,
    pub unit_length_gate_sidewall_temp_jct_cap: f64,
    pub phi_b: f64,
    pub phi_bsw: f64,
    pub phi_bswg: f64,
    /// `Vtm0 = KboQ·Tnom` and `ni` at TNOM, read by the size loop.
    pub vtm0: f64,
    pub ni: f64,
    /// `Temp / Tnom`.
    pub t_ratio: f64,
    pub temp: f64,
}

impl Bsim3MTemp {
    /// The model part of `BSIM3temp`. Clamps the junction
    /// potentials of `m` like the C (which writes them back into the
    /// model) and pushes the C's warnings into `notes`.
    pub fn new(m: &mut Bsim3Model, temp: f64, notes: &mut Vec<String>) -> Self {
        if m.bulk_jct_potential < 0.1 {
            m.bulk_jct_potential = 0.1;
            notes.push("Given pb is less than 0.1. Pb is set to 0.1.".into());
        }
        if m.sidewall_jct_potential < 0.1 {
            m.sidewall_jct_potential = 0.1;
            notes.push("Given pbsw is less than 0.1. Pbsw is set to 0.1.".into());
        }
        if m.gatesidewall_jct_potential < 0.1 {
            m.gatesidewall_jct_potential = 0.1;
            notes.push("Given pbswg is less than 0.1. Pbswg is set to 0.1.".into());
        }
        let tnom = m.tnom;
        let t_ratio = temp / tnom;
        let vcrit =
            crate::consts::VT0 * f64::ln(crate::consts::VT0 / (crate::consts::ROOT2 * 1.0e-14));
        let factor1 = f64::sqrt(EPSSI / EPSOX * m.tox);

        let vtm0 = KBOQ * tnom;
        let eg0 = 1.16 - 7.02e-4 * tnom * tnom / (tnom + 1108.0);
        let ni = 1.45e10
            * (tnom / 300.15)
            * f64::sqrt(tnom / 300.15)
            * f64::exp(21.5565981 - eg0 / (2.0 * vtm0));

        let vtm = KBOQ * temp;
        let eg = 1.16 - 7.02e-4 * temp * temp / (temp + 1108.0);
        let (mut js, mut jssw);
        if temp != tnom {
            let t0 = eg0 / vtm0 - eg / vtm + m.jct_temp_exponent * f64::ln(temp / tnom);
            let t1 = f64::exp(t0 / m.jct_emission_coeff);
            js = m.jct_sat_cur_density * t1;
            jssw = m.jct_sidewall_sat_cur_density * t1;
        } else {
            js = m.jct_sat_cur_density;
            jssw = m.jct_sidewall_sat_cur_density;
        }
        if js < 0.0 {
            js = 0.0;
        }
        if jssw < 0.0 {
            jssw = 0.0;
        }

        // Junction capacitances; a clamp below −1 leaves the
        // (calloc-zero) temperature value when the nominal is ≤ 0.
        let del_temp = temp - tnom;
        let scale = |c: f64, tc: f64, what: &str, notes: &mut Vec<String>| {
            let t0 = tc * del_temp;
            if t0 >= -1.0 {
                c * (1.0 + t0)
            } else {
                if c > 0.0 {
                    notes.push(format!(
                        "Temperature effect has caused {what} to be negative. {what} is clamped to zero."
                    ));
                }
                0.0
            }
        };
        let cj = scale(m.unit_area_jct_cap, m.tcj, "cj", notes);
        let cjsw = scale(m.unit_length_sidewall_jct_cap, m.tcjsw, "cjsw", notes);
        let cjswg = scale(
            m.unit_length_gate_sidewall_jct_cap,
            m.tcjswg,
            "cjswg",
            notes,
        );

        let mut phi_b = m.bulk_jct_potential - m.tpb * del_temp;
        if phi_b < 0.01 {
            phi_b = 0.01;
            notes.push(
                "Temperature effect has caused pb to be less than 0.01. Pb is clamped to 0.01."
                    .into(),
            );
        }
        let mut phi_bsw = m.sidewall_jct_potential - m.tpbsw * del_temp;
        if phi_bsw <= 0.01 {
            phi_bsw = 0.01;
            notes.push(
                "Temperature effect has caused pbsw to be less than 0.01. Pbsw is clamped to 0.01."
                    .into(),
            );
        }
        let mut phi_bswg = m.gatesidewall_jct_potential - m.tpbswg * del_temp;
        if phi_bswg <= 0.01 {
            phi_bswg = 0.01;
            notes.push(
                "Temperature effect has caused pbswg to be less than 0.01. Pbswg is clamped to 0.01."
                    .into(),
            );
        }
        Bsim3MTemp {
            vcrit,
            factor1,
            vtm,
            jct_temp_sat_cur_density: js,
            jct_sidewall_temp_sat_cur_density: jssw,
            unit_area_temp_jct_cap: cj,
            unit_length_sidewall_temp_jct_cap: cjsw,
            unit_length_gate_sidewall_temp_jct_cap: cjswg,
            phi_b,
            phi_bsw,
            phi_bswg,
            vtm0,
            ni,
            t_ratio,
            temp,
        }
    }
}

/// `struct bsim3SizeDependParam`: the binned parameters
/// `P = P0 + PL/Leff + PW/Weff + PP/(Leff·Weff)` (µm units with
/// `BINUNIT = 1`) and everything `BSIM3temp` derives from them.
#[derive(Clone, Debug, Default)]
// `dl`/`dlc`/`dw`/`dwc` and `elm` (NQS) are kept as in the C struct.
#[allow(dead_code)]
pub(crate) struct Bsim3Size {
    /// Drawn `Length`/`Width` the knot is keyed on.
    pub length: f64,
    pub width: f64,
    pub dl: f64,
    pub dlc: f64,
    pub dw: f64,
    pub dwc: f64,
    pub leff: f64,
    pub weff: f64,
    pub leff_cv: f64,
    pub weff_cv: f64,
    pub cdsc: f64,
    pub cdscb: f64,
    pub cdscd: f64,
    pub cit: f64,
    pub nfactor: f64,
    pub xj: f64,
    pub vsat: f64,
    pub at: f64,
    pub a0: f64,
    pub ags: f64,
    pub a1: f64,
    pub a2: f64,
    pub keta: f64,
    pub nsub: f64,
    pub npeak: f64,
    pub ngate: f64,
    pub gamma1: f64,
    pub gamma2: f64,
    pub vbx: f64,
    pub vbm: f64,
    pub xt: f64,
    pub vfb: f64,
    pub k1: f64,
    pub kt1: f64,
    pub kt1l: f64,
    pub k2: f64,
    pub kt2: f64,
    pub k3: f64,
    pub k3b: f64,
    pub w0: f64,
    pub nlx: f64,
    pub dvt0: f64,
    pub dvt1: f64,
    pub dvt2: f64,
    pub dvt0w: f64,
    pub dvt1w: f64,
    pub dvt2w: f64,
    pub drout: f64,
    pub dsub: f64,
    pub vth0: f64,
    pub ua: f64,
    pub ua1: f64,
    pub ub: f64,
    pub ub1: f64,
    pub uc: f64,
    pub uc1: f64,
    pub u0: f64,
    pub ute: f64,
    pub voff: f64,
    pub delta: f64,
    pub rdsw: f64,
    pub prwg: f64,
    pub prwb: f64,
    pub prt: f64,
    pub eta0: f64,
    pub etab: f64,
    pub pclm: f64,
    pub pdibl1: f64,
    pub pdibl2: f64,
    pub pdiblb: f64,
    pub pscbe1: f64,
    pub pscbe2: f64,
    pub pvag: f64,
    pub wr: f64,
    pub dwg: f64,
    pub dwb: f64,
    pub b0: f64,
    pub b1: f64,
    pub alpha0: f64,
    pub alpha1: f64,
    pub beta0: f64,
    pub elm: f64,
    pub cgsl: f64,
    pub cgdl: f64,
    pub ckappa: f64,
    pub cf: f64,
    pub clc: f64,
    pub cle: f64,
    pub vfbcv: f64,
    pub acde: f64,
    pub moin: f64,
    pub noff: f64,
    pub voffcv: f64,
    pub abulk_cv_factor: f64,
    pub u0temp: f64,
    pub vsattemp: f64,
    pub rds0: f64,
    pub cgdo_p: f64,
    pub cgso_p: f64,
    pub cgbo_p: f64,
    pub phi: f64,
    pub sqrt_phi: f64,
    pub phis3: f64,
    pub xdep0: f64,
    pub sqrt_xdep0: f64,
    pub litl: f64,
    pub vbi: f64,
    pub cdep0: f64,
    pub ldeb: f64,
    pub vbsc: f64,
    pub k1ox: f64,
    pub k2ox: f64,
    pub theta0vb0: f64,
    pub theta_rout: f64,
    pub vfbzb: f64,
}

impl Bsim3Size {
    /// The `Size_Not_Found` block of `BSIM3temp` for one drawn
    /// (L, W); `m` is the set-up model, `mt` its temperature part.
    /// Errors are the C's fatal `IFerrorf`s (effective sizes ≤ 0,
    /// `BSIM3checkModel` fatals); warnings go to `notes`.
    #[allow(clippy::too_many_lines)]
    pub fn new(
        name: &str,
        m: &Bsim3Model,
        mt: &Bsim3MTemp,
        l: f64,
        w: f64,
        notes: &mut Vec<String>,
    ) -> Result<Self, String> {
        let typ = m.type_();
        let ldrn = l;
        let wdrn = w;
        let t0 = ldrn.powf(m.lln);
        let t1 = wdrn.powf(m.lwn);
        let tmp1 = m.ll / t0 + m.lw / t1 + m.lwl / (t0 * t1);
        let dl = m.lint + tmp1;
        let tmp2 = m.llc / t0 + m.lwc / t1 + m.lwlc / (t0 * t1);
        let dlc = m.dlc + tmp2;

        let t2 = ldrn.powf(m.wln);
        let t3 = wdrn.powf(m.wwn);
        let tmp1 = m.wl / t2 + m.ww / t3 + m.wwl / (t2 * t3);
        let dw = m.wint + tmp1;
        let tmp2 = m.wlc / t2 + m.wwc / t3 + m.wwlc / (t2 * t3);
        let dwc = m.dwc + tmp2;

        let leff = l + m.xl - 2.0 * dl;
        if leff <= 0.0 {
            return Err(format!(
                "BSIM3: mosfet {name}: Effective channel length <= 0"
            ));
        }
        let weff = w + m.xw - 2.0 * dw;
        if weff <= 0.0 {
            return Err(format!(
                "BSIM3: mosfet {name}: Effective channel width <= 0"
            ));
        }
        let leff_cv = l + m.xl - 2.0 * dlc;
        if leff_cv <= 0.0 {
            return Err(format!(
                "BSIM3: mosfet {name}: Effective channel length for C-V <= 0"
            ));
        }
        let weff_cv = w + m.xw - 2.0 * dwc;
        if weff_cv <= 0.0 {
            return Err(format!(
                "BSIM3: mosfet {name}: Effective channel width for C-V <= 0"
            ));
        }

        let (inv_l, inv_w, inv_lw) = if m.bin_unit == 1 {
            (1.0e-6 / leff, 1.0e-6 / weff, 1.0e-12 / (leff * weff))
        } else {
            (1.0 / leff, 1.0 / weff, 1.0 / (leff * weff))
        };
        let p = |p0: f64, pl: f64, pw: f64, pp: f64| p0 + pl * inv_l + pw * inv_w + pp * inv_lw;
        let mut s = Bsim3Size {
            length: ldrn,
            width: wdrn,
            dl,
            dlc,
            dw,
            dwc,
            leff,
            weff,
            leff_cv,
            weff_cv,
            cdsc: p(m.cdsc, m.lcdsc, m.wcdsc, m.pcdsc),
            cdscb: p(m.cdscb, m.lcdscb, m.wcdscb, m.pcdscb),
            cdscd: p(m.cdscd, m.lcdscd, m.wcdscd, m.pcdscd),
            cit: p(m.cit, m.lcit, m.wcit, m.pcit),
            nfactor: p(m.nfactor, m.lnfactor, m.wnfactor, m.pnfactor),
            xj: p(m.xj, m.lxj, m.wxj, m.pxj),
            vsat: p(m.vsat, m.lvsat, m.wvsat, m.pvsat),
            at: p(m.at, m.lat, m.wat, m.pat),
            a0: p(m.a0, m.la0, m.wa0, m.pa0),
            ags: p(m.ags, m.lags, m.wags, m.pags),
            a1: p(m.a1, m.la1, m.wa1, m.pa1),
            a2: p(m.a2, m.la2, m.wa2, m.pa2),
            keta: p(m.keta, m.lketa, m.wketa, m.pketa),
            nsub: p(m.nsub, m.lnsub, m.wnsub, m.pnsub),
            npeak: p(m.npeak, m.lnpeak, m.wnpeak, m.pnpeak),
            ngate: p(m.ngate, m.lngate, m.wngate, m.pngate),
            gamma1: p(m.gamma1, m.lgamma1, m.wgamma1, m.pgamma1),
            gamma2: p(m.gamma2, m.lgamma2, m.wgamma2, m.pgamma2),
            vbx: p(m.vbx, m.lvbx, m.wvbx, m.pvbx),
            vbm: p(m.vbm, m.lvbm, m.wvbm, m.pvbm),
            xt: p(m.xt, m.lxt, m.wxt, m.pxt),
            vfb: p(m.vfb, m.lvfb, m.wvfb, m.pvfb),
            k1: p(m.k1, m.lk1, m.wk1, m.pk1),
            kt1: p(m.kt1, m.lkt1, m.wkt1, m.pkt1),
            kt1l: p(m.kt1l, m.lkt1l, m.wkt1l, m.pkt1l),
            k2: p(m.k2, m.lk2, m.wk2, m.pk2),
            kt2: p(m.kt2, m.lkt2, m.wkt2, m.pkt2),
            k3: p(m.k3, m.lk3, m.wk3, m.pk3),
            k3b: p(m.k3b, m.lk3b, m.wk3b, m.pk3b),
            w0: p(m.w0, m.lw0, m.ww0, m.pw0),
            nlx: p(m.nlx, m.lnlx, m.wnlx, m.pnlx),
            dvt0: p(m.dvt0, m.ldvt0, m.wdvt0, m.pdvt0),
            dvt1: p(m.dvt1, m.ldvt1, m.wdvt1, m.pdvt1),
            dvt2: p(m.dvt2, m.ldvt2, m.wdvt2, m.pdvt2),
            dvt0w: p(m.dvt0w, m.ldvt0w, m.wdvt0w, m.pdvt0w),
            dvt1w: p(m.dvt1w, m.ldvt1w, m.wdvt1w, m.pdvt1w),
            dvt2w: p(m.dvt2w, m.ldvt2w, m.wdvt2w, m.pdvt2w),
            drout: p(m.drout, m.ldrout, m.wdrout, m.pdrout),
            dsub: p(m.dsub, m.ldsub, m.wdsub, m.pdsub),
            vth0: p(m.vth0, m.lvth0, m.wvth0, m.pvth0),
            ua: p(m.ua, m.lua, m.wua, m.pua),
            ua1: p(m.ua1, m.lua1, m.wua1, m.pua1),
            ub: p(m.ub, m.lub, m.wub, m.pub_),
            ub1: p(m.ub1, m.lub1, m.wub1, m.pub1),
            uc: p(m.uc, m.luc, m.wuc, m.puc),
            uc1: p(m.uc1, m.luc1, m.wuc1, m.puc1),
            u0: p(m.u0, m.lu0, m.wu0, m.pu0),
            ute: p(m.ute, m.lute, m.wute, m.pute),
            voff: p(m.voff, m.lvoff, m.wvoff, m.pvoff),
            delta: p(m.delta, m.ldelta, m.wdelta, m.pdelta),
            rdsw: p(m.rdsw, m.lrdsw, m.wrdsw, m.prdsw),
            prwg: p(m.prwg, m.lprwg, m.wprwg, m.pprwg),
            prwb: p(m.prwb, m.lprwb, m.wprwb, m.pprwb),
            prt: p(m.prt, m.lprt, m.wprt, m.pprt),
            eta0: p(m.eta0, m.leta0, m.weta0, m.peta0),
            etab: p(m.etab, m.letab, m.wetab, m.petab),
            pclm: p(m.pclm, m.lpclm, m.wpclm, m.ppclm),
            pdibl1: p(m.pdibl1, m.lpdibl1, m.wpdibl1, m.ppdibl1),
            pdibl2: p(m.pdibl2, m.lpdibl2, m.wpdibl2, m.ppdibl2),
            pdiblb: p(m.pdiblb, m.lpdiblb, m.wpdiblb, m.ppdiblb),
            pscbe1: p(m.pscbe1, m.lpscbe1, m.wpscbe1, m.ppscbe1),
            pscbe2: p(m.pscbe2, m.lpscbe2, m.wpscbe2, m.ppscbe2),
            pvag: p(m.pvag, m.lpvag, m.wpvag, m.ppvag),
            wr: p(m.wr, m.lwr, m.wwr, m.pwr),
            dwg: p(m.dwg, m.ldwg, m.wdwg, m.pdwg),
            dwb: p(m.dwb, m.ldwb, m.wdwb, m.pdwb),
            b0: p(m.b0, m.lb0, m.wb0, m.pb0),
            b1: p(m.b1, m.lb1, m.wb1, m.pb1),
            alpha0: p(m.alpha0, m.lalpha0, m.walpha0, m.palpha0),
            alpha1: p(m.alpha1, m.lalpha1, m.walpha1, m.palpha1),
            beta0: p(m.beta0, m.lbeta0, m.wbeta0, m.pbeta0),
            elm: p(m.elm, m.lelm, m.welm, m.pelm),
            cgsl: p(m.cgsl, m.lcgsl, m.wcgsl, m.pcgsl),
            cgdl: p(m.cgdl, m.lcgdl, m.wcgdl, m.pcgdl),
            ckappa: p(m.ckappa, m.lckappa, m.wckappa, m.pckappa),
            cf: p(m.cf, m.lcf, m.wcf, m.pcf),
            clc: p(m.clc, m.lclc, m.wclc, m.pclc),
            cle: p(m.cle, m.lcle, m.wcle, m.pcle),
            vfbcv: p(m.vfbcv, m.lvfbcv, m.wvfbcv, m.pvfbcv),
            acde: p(m.acde, m.lacde, m.wacde, m.pacde),
            moin: p(m.moin, m.lmoin, m.wmoin, m.pmoin),
            noff: p(m.noff, m.lnoff, m.wnoff, m.pnoff),
            voffcv: p(m.voffcv, m.lvoffcv, m.wvoffcv, m.pvoffcv),
            ..Default::default()
        };

        s.abulk_cv_factor = 1.0 + (s.clc / s.leff_cv).powf(s.cle);

        let t0 = mt.t_ratio - 1.0;
        s.ua += s.ua1 * t0;
        s.ub += s.ub1 * t0;
        s.uc += s.uc1 * t0;
        if s.u0 > 1.0 {
            s.u0 /= 1.0e4;
        }
        s.u0temp = s.u0 * mt.t_ratio.powf(s.ute);
        s.vsattemp = s.vsat - s.at * t0;
        s.rds0 = (s.rdsw + s.prt * t0) / (s.weff * 1e6).powf(s.wr);

        check_model(name, m, &s, notes)?;

        s.cgdo_p = (m.cgdo + s.cf) * s.weff_cv;
        s.cgso_p = (m.cgso + s.cf) * s.weff_cv;
        s.cgbo_p = m.cgbo * s.leff_cv;

        // (`BSIM3tconst` only feeds the NQS model, which is not
        // implemented.)

        if !m.given.npeak && m.given.gamma1 {
            let t0 = s.gamma1 * m.cox;
            s.npeak = 3.021e22 * t0 * t0;
        }

        let vtm0 = mt.vtm0;
        let ni = mt.ni;
        s.phi = 2.0 * vtm0 * f64::ln(s.npeak / ni);
        s.sqrt_phi = s.phi.sqrt();
        s.phis3 = s.sqrt_phi * s.phi;
        s.xdep0 = f64::sqrt(2.0 * EPSSI / (CHARGE_Q * s.npeak * 1.0e6)) * s.sqrt_phi;
        s.sqrt_xdep0 = s.xdep0.sqrt();
        s.litl = f64::sqrt(3.0 * s.xj * m.tox);
        s.vbi = vtm0 * f64::ln(1.0e20 * s.npeak / (ni * ni));
        s.cdep0 = f64::sqrt(CHARGE_Q * EPSSI * s.npeak * 1.0e6 / 2.0 / s.phi);
        s.ldeb = f64::sqrt(EPSSI * vtm0 / (CHARGE_Q * s.npeak * 1.0e6)) / 3.0;
        s.acde *= (s.npeak / 2.0e16).powf(-0.25);

        if m.given.k1 || m.given.k2 {
            if !m.given.k1 {
                notes.push("Warning: k1 should be specified with k2.".into());
                s.k1 = 0.53;
            }
            if !m.given.k2 {
                notes.push("Warning: k2 should be specified with k1.".into());
                s.k2 = -0.0186;
            }
            if m.given.nsub {
                notes.push("Warning: nsub is ignored because k1 or k2 is given.".into());
            }
            if m.given.xt {
                notes.push("Warning: xt is ignored because k1 or k2 is given.".into());
            }
            if m.given.vbx {
                notes.push("Warning: vbx is ignored because k1 or k2 is given.".into());
            }
            if m.given.gamma1 {
                notes.push("Warning: gamma1 is ignored because k1 or k2 is given.".into());
            }
            if m.given.gamma2 {
                notes.push("Warning: gamma2 is ignored because k1 or k2 is given.".into());
            }
        } else {
            if !m.given.vbx {
                s.vbx = s.phi - 7.7348e-4 * s.npeak * s.xt * s.xt;
            }
            if s.vbx > 0.0 {
                s.vbx = -s.vbx;
            }
            if s.vbm > 0.0 {
                s.vbm = -s.vbm;
            }
            if !m.given.gamma1 {
                s.gamma1 = 5.753e-12 * s.npeak.sqrt() / m.cox;
            }
            if !m.given.gamma2 {
                s.gamma2 = 5.753e-12 * s.nsub.sqrt() / m.cox;
            }
            let t0 = s.gamma1 - s.gamma2;
            let t1 = f64::sqrt(s.phi - s.vbx) - s.sqrt_phi;
            let t2 = f64::sqrt(s.phi * (s.phi - s.vbm)) - s.phi;
            s.k2 = t0 * t1 / (2.0 * t2 + s.vbm);
            s.k1 = s.gamma2 - 2.0 * s.k2 * f64::sqrt(s.phi - s.vbm);
        }

        if s.k2 < 0.0 {
            let t0 = 0.5 * s.k1 / s.k2;
            s.vbsc = 0.9 * (s.phi - t0 * t0);
            s.vbsc = s.vbsc.clamp(-30.0, -3.0);
        } else {
            s.vbsc = -30.0;
        }
        if s.vbsc > s.vbm {
            s.vbsc = s.vbm;
        }

        if !m.given.vfb {
            s.vfb = if m.given.vth0 {
                typ * s.vth0 - s.phi - s.k1 * s.sqrt_phi
            } else {
                -1.0
            };
        }
        if !m.given.vth0 {
            s.vth0 = typ * (s.vfb + s.phi + s.k1 * s.sqrt_phi);
        }

        s.k1ox = s.k1 * m.tox / m.toxm;
        s.k2ox = s.k2 * m.tox / m.toxm;

        let t1 = f64::sqrt(EPSSI / EPSOX * m.tox * s.xdep0);
        let t0 = f64::exp(-0.5 * s.dsub * s.leff / t1);
        s.theta0vb0 = t0 + 2.0 * t0 * t0;

        let t0 = f64::exp(-0.5 * s.drout * s.leff / t1);
        let t2 = t0 + 2.0 * t0 * t0;
        s.theta_rout = s.pdibl1 * t2 + s.pdibl2;

        let tmp = s.xdep0.sqrt();
        let tmp1 = s.vbi - s.phi;
        let tmp2 = mt.factor1 * tmp;

        let t0 = -0.5 * s.dvt1w * s.weff * s.leff / tmp2;
        let t1 = if t0 > -EXP_THRESHOLD {
            t0.exp()
        } else {
            MIN_EXP
        };
        let t2 = t1 * (1.0 + 2.0 * t1);
        let t0 = s.dvt0w * t2;
        let t2 = t0 * tmp1;

        let t0 = -0.5 * s.dvt1 * s.leff / tmp2;
        let t1 = if t0 > -EXP_THRESHOLD {
            t0.exp()
        } else {
            MIN_EXP
        };
        let t3 = t1 * (1.0 + 2.0 * t1);
        let t3 = s.dvt0 * t3 * tmp1;

        let t4 = m.tox * s.phi / (s.weff + s.w0);

        let t0 = f64::sqrt(1.0 + s.nlx / s.leff);
        let t5 = s.k1ox * (t0 - 1.0) * s.sqrt_phi + (s.kt1 + s.kt1l / s.leff) * (mt.t_ratio - 1.0);

        let tmp3 = typ * s.vth0 - t2 - t3 + s.k3 * t4 + t5;
        s.vfbzb = tmp3 - s.phi - s.k1 * s.sqrt_phi;
        Ok(s)
    }
}

/// The fatal checks of `BSIM3checkModel` (`b3check.c`), run where
/// `BSIM3temp` runs them (after `rds0`, before the `npeak`/`k1`
/// derivations). The informational warnings are not reproduced
/// except the version one.
fn check_model(
    name: &str,
    m: &Bsim3Model,
    s: &Bsim3Size,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    let mut fatal: Vec<String> = Vec::new();
    if let Some(v) = &m.version
        && !v.starts_with("3.3")
    {
        notes.push(
            "Warning: This model is BSIM3v3.3.0; you specified a wrong version number.".into(),
        );
    }
    if s.nlx < -s.leff {
        fatal.push(format!("Nlx = {} is less than -Leff.", s.nlx));
    }
    if m.tox <= 0.0 {
        fatal.push(format!("Tox = {} is not positive.", m.tox));
    }
    if m.toxm <= 0.0 {
        fatal.push(format!("Toxm = {} is not positive.", m.toxm));
    }
    if m.lintnoi > s.leff / 2.0 {
        fatal.push(format!(
            "Lintnoi = {} is too large - Leff for noise is negative.",
            m.lintnoi
        ));
    }
    if s.npeak <= 0.0 {
        fatal.push(format!("Nch = {} is not positive.", s.npeak));
    }
    if s.nsub <= 0.0 {
        fatal.push(format!("Nsub = {} is not positive.", s.nsub));
    }
    if s.ngate < 0.0 {
        fatal.push(format!("Ngate = {} is not positive.", s.ngate));
    }
    if s.ngate > 1.0e25 {
        fatal.push(format!("Ngate = {} is too high.", s.ngate));
    }
    if s.xj <= 0.0 {
        fatal.push(format!("Xj = {} is not positive.", s.xj));
    }
    if s.dvt1 < 0.0 {
        fatal.push(format!("Dvt1 = {} is negative.", s.dvt1));
    }
    if s.dvt1w < 0.0 {
        fatal.push(format!("Dvt1w = {} is negative.", s.dvt1w));
    }
    if s.w0 == -s.weff {
        fatal.push("(W0 + Weff) = 0 causing divided-by-zero.".into());
    }
    if s.dsub < 0.0 {
        fatal.push(format!("Dsub = {} is negative.", s.dsub));
    }
    if s.b1 == -s.weff {
        fatal.push("(B1 + Weff) = 0 causing divided-by-zero.".into());
    }
    if s.u0temp <= 0.0 {
        fatal.push(format!(
            "u0 at current temperature = {} is not positive.",
            s.u0temp
        ));
    }
    if s.delta < 0.0 {
        fatal.push(format!("Delta = {} is less than zero.", s.delta));
    }
    if s.vsattemp <= 0.0 {
        fatal.push(format!(
            "Vsat at current temperature = {} is not positive.",
            s.vsattemp
        ));
    }
    if s.pclm <= 0.0 {
        fatal.push(format!("Pclm = {} is not positive.", s.pclm));
    }
    if s.drout < 0.0 {
        fatal.push(format!("Drout = {} is negative.", s.drout));
    }
    if m.ijth < 0.0 {
        fatal.push(format!("Ijth = {} cannot be negative.", m.ijth));
    }
    if s.clc < 0.0 {
        fatal.push(format!("Clc = {} is negative.", s.clc));
    }
    if fatal.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Fatal error(s) detected during BSIM3V3.3 parameter checking for {name}: {}",
            fatal.join(" ")
        ))
    }
}
