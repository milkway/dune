//! The ngspice-47 diode beyond the 3f5 card: `LEVEL=3` geometry, the
//! sidewall junction, recombination, tunnelling, high injection, the
//! `TLEV`/`TLEVC` temperature options and the parameter temperature
//! coefficients — translated from `src/spicelib/devices/dio/`
//! (`diosetup.c`, `diotemp.c`, `dioload.c`) of ngspice-47:
//!
//! > Copyright 1990 Regents of the University of California. All
//! > rights reserved. Author: 1985 Thomas L. Quarles. Modified: 2000
//! > AlansFixes. Modified by Paolo Nenzi 2003, Dietmar Warning 2012
//! > and Arpad Buermen 2025.
//!
//! (Modified BSD, as the rest of the ngspice device code.) A diode
//! whose model and instance stay within the 3f5 parameters never
//! reaches this module: [`crate::diode`] keeps its 3f5 path for it,
//! bit for bit. What ngspice applies to every diode regardless of
//! `LEVEL` is here too, because the 3f5 card has none of it; `LEVEL=3`
//! only adds the `W`/`L` area and perimeter, the metal and poly
//! overlap capacitances and the area-scaled breakdown current
//! (`docs/diode.md`). The separate sidewall diode (`RSW`), soft
//! reverse recovery (`VP`) and self-heating (`thermal`) are refused by
//! the elaborator.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};

/// `CKTepsmin` (`cktinit.c`): the floor of `IS` and of the knee
/// currents (`diosetup.c`).
const EPSMIN: f64 = 1e-28;

/// `CONSTepsSiO2` (`const.h`): `3.9·ε0` with `ε0 = 1/(μ0·c²)`.
const EPS_SIO2: f64 = 3.9 / (4.0 * std::f64::consts::PI * 1e-7 * 299_792_458.0 * 299_792_458.0);

/// The ngspice-47 model parameters a 3f5 card does not have
/// (`DIOmPTable` of `dio.c`, defaults of `diosetup.c`). `None` is
/// "not given" where the C tests `…Given`.
#[derive(Clone, Debug)]
pub(crate) struct DioModelExt {
    /// `LEVEL`: 1 or 3 (2 is refused by ngspice, others by Dune).
    pub level: i64,
    /// `JSW`/`ISW`: sidewall saturation current (A/m with `PJ`).
    pub jsw: Option<f64>,
    /// `NS`: sidewall emission coefficient (its own characteristic
    /// only when given).
    pub ns: Option<f64>,
    /// `TRS`/`TRS1`, `TRS2`: series-resistance temperature
    /// coefficients.
    pub trs: f64,
    pub trs2: f64,
    /// `TTT1`, `TTT2`: transit-time temperature coefficients.
    pub ttt1: f64,
    pub ttt2: f64,
    /// `TM1`, `TM2`: grading-coefficient temperature coefficients.
    pub tm1: f64,
    pub tm2: f64,
    /// `CJP`/`CJSW` (0), `PHP`/`VJSW` (1), `MJSW` (0.33), `FCS` (0.5).
    pub cjsw: f64,
    pub php: f64,
    pub mjsw: f64,
    pub fcs: f64,
    /// `IKF`/`IK`, `IKR`, `IKP`: knee currents (disabled below
    /// `CKTepsmin`, with ngspice's warning).
    pub ikf: Option<f64>,
    pub ikr: Option<f64>,
    pub ikp: Option<f64>,
    /// `NBV`/`NZ`: breakdown emission coefficient (default `N`).
    pub nbv: Option<f64>,
    /// Model `AREA` (1) and `PJ` (0): the instance defaults.
    pub area: f64,
    pub pj: f64,
    /// `TLEV` (0), `TLEVC` (0).
    pub tlev: i64,
    pub tlevc: i64,
    /// `EG` was given (else `TLEV=2` defaults it to 1.16).
    pub eg_given: bool,
    /// `GAP1` (7.02e-4), `GAP2` (1108).
    pub gap1: f64,
    pub gap2: f64,
    /// `CTA`/`CTC`, `CTP`, `TPB`/`TVJ`, `TPHP` (0).
    pub cta: f64,
    pub ctp: f64,
    pub tpb: f64,
    pub tphp: f64,
    /// `JTUN`, `JTUNSW`: tunnelling saturation currents.
    pub jtun: Option<f64>,
    pub jtunsw: Option<f64>,
    /// `NTUN` (30), `XTITUN` (3), `KEG` (1).
    pub ntun: f64,
    pub xtitun: f64,
    pub keg: f64,
    /// `TCV`/`TBV1`: breakdown-voltage temperature coefficient (0).
    pub tcv: f64,
    /// `ISR` (recombination current, only when given), `NR` (2).
    pub isr: Option<f64>,
    pub nr: f64,
    /// Level-3 parasitics: `LM`, `LP`, `WM`, `WP` (0), `XOM`, `XOI`
    /// (1e4), `XM`, `XP`, `XW` (0).
    pub lm: f64,
    pub lp: f64,
    pub wm: f64,
    pub wp: f64,
    pub xom: f64,
    pub xoi: f64,
    pub xm: f64,
    pub xp: f64,
    pub xw: f64,
}

impl Default for DioModelExt {
    fn default() -> Self {
        DioModelExt {
            level: 1,
            jsw: None,
            ns: None,
            trs: 0.0,
            trs2: 0.0,
            ttt1: 0.0,
            ttt2: 0.0,
            tm1: 0.0,
            tm2: 0.0,
            cjsw: 0.0,
            php: 1.0,
            mjsw: 0.33,
            fcs: 0.5,
            ikf: None,
            ikr: None,
            ikp: None,
            nbv: None,
            area: 1.0,
            pj: 0.0,
            tlev: 0,
            tlevc: 0,
            eg_given: false,
            gap1: 7.02e-4,
            gap2: 1108.0,
            cta: 0.0,
            ctp: 0.0,
            tpb: 0.0,
            tphp: 0.0,
            jtun: None,
            jtunsw: None,
            ntun: 30.0,
            xtitun: 3.0,
            keg: 1.0,
            tcv: 0.0,
            isr: None,
            nr: 2.0,
            lm: 0.0,
            lp: 0.0,
            wm: 0.0,
            wp: 0.0,
            xom: 1e4,
            xoi: 1e4,
            xm: 0.0,
            xp: 0.0,
            xw: 0.0,
        }
    }
}

/// The ngspice-47 instance geometry of a diode card (`DIOpTable`):
/// `PJ`/`PERIM`, `W`, `L`, `LM`, `LP`, `WM`, `WP`.
#[derive(Clone, Debug, Default)]
pub(crate) struct DioGeom {
    pub area_given: bool,
    pub pj: Option<f64>,
    pub w: Option<f64>,
    pub l: Option<f64>,
    pub lm: Option<f64>,
    pub lp: Option<f64>,
    pub wm: Option<f64>,
    pub wp: Option<f64>,
}

/// What the elaborator records for a diode that needs this module
/// (`Circuit::dio_ext`): the model extension, the instance geometry
/// and `.option scale` (read by `diosetup.c` for `LEVEL=3`).
#[derive(Clone, Debug)]
pub(crate) struct DioExtInst {
    pub model: DioModelExt,
    pub geom: DioGeom,
    pub scale: f64,
}

/// Temperature-adjusted ngspice-47 values of one instance
/// (`DIOtempUpdate`), all absolute (area, perimeter and `m` in).
#[derive(Clone, Debug)]
pub(crate) struct DioExtT {
    /// `tSatSWCur`, `vts = NS·vt`; `JSW`/`NS` given.
    pub csatsw: f64,
    pub vtesw: f64,
    pub sw_given: bool,
    pub ns_given: bool,
    /// `tTunSatCur`, `tTunSatSWCur`, `vtt = NTUN·vt`.
    pub tun: Option<f64>,
    pub tunsw: Option<f64>,
    pub vtetun: f64,
    /// `tRecSatCur`, `vtr = NR·vt`.
    pub rec: Option<f64>,
    pub vterec: f64,
    /// Instance knee currents (`IKF·AREA·M` …), when active.
    pub ikf: Option<f64>,
    pub ikr: Option<f64>,
    pub ikp: Option<f64>,
    /// `tJctPot`, `tGradingCoeff`, `tJctCap`, `tDepCap`, `tF1/2/3`.
    pub jct_pot: f64,
    pub grading: f64,
    pub jct_cap: f64,
    pub dep_cap: f64,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
    /// Sidewall: `tJctSWCap`, `tJctSWPot`, `tDepSWCap`, `tF2SW`,
    /// `tF3SW`, `MJSW`.
    pub sw_cap: f64,
    pub sw_pot: f64,
    pub sw_dep_cap: f64,
    pub sw_f2: f64,
    pub sw_f3: f64,
    pub mjsw: f64,
    /// `DIOcmetal + DIOcpoly` (level 3).
    pub cover: f64,
}

/// The base values [`crate::diode::DiodeTemp`] keeps for any diode,
/// as the ngspice-47 temperature pass computes them.
pub(crate) struct DioExtBase {
    pub csat: f64,
    pub gspr: f64,
    pub vt: f64,
    pub vte: f64,
    pub vtebrk: f64,
    pub vcrit: f64,
    pub brkdwn: Option<f64>,
    pub tt: f64,
    pub ext: DioExtT,
}

/// The effective area and perimeter of an instance (`DIOsetup`): the
/// card's, else the model's when neither `W` nor `L` is given (else
/// 1 and 0); at `LEVEL=3` with both `W` and `L` given,
/// `(W+XW)·(L+XW)·M·scale²` and `2·(W+XW)+2·(L+XW))·M·scale` — the
/// multiplier `M` enters here *and* again in every current and
/// capacitance (`docs/diode.md`, Fidelidade). Also the level-3 metal
/// and poly overlap capacitances.
pub(crate) fn setup_geometry(x: &DioExtInst, card_area: f64, m: f64) -> (f64, f64, f64) {
    let (md, g, scale) = (&x.model, &x.geom, x.scale);
    let wl = g.w.is_some() || g.l.is_some();
    let mut area = if g.area_given {
        card_area
    } else if wl {
        1.0
    } else {
        md.area
    };
    let mut pj = match g.pj {
        Some(p) => p,
        None if wl => 0.0,
        None => md.pj,
    };
    let mut cover = 0.0;
    if md.level == 3 {
        if let (Some(w), Some(l)) = (g.w, g.l) {
            area = (w + md.xw) * (l + md.xw) * m * scale * scale;
            pj = (2.0 * (w + md.xw) + 2.0 * (l + md.xw)) * m * scale;
        }
        let wm = g.wm.unwrap_or(md.wm);
        let lm = g.lm.unwrap_or(md.lm);
        let wp = g.wp.unwrap_or(md.wp);
        let lp = g.lp.unwrap_or(md.lp);
        let cmetal = EPS_SIO2 / md.xom * m * (wm * scale + md.xm) * (lm * scale + md.xm);
        let cpoly = EPS_SIO2 / md.xoi * m * (wp * scale + md.xp) * (lp * scale + md.xp);
        cover = cmetal + cpoly;
    }
    (area, pj, cover)
}

/// The 3f5 model values the ngspice pass reads, after Dune's 3f5
/// clamps (`M ≤ 0.9`, `EG ≥ 0.1`, `FC ≤ 0.95`).
pub(crate) struct DioBase3f5 {
    pub is: f64,
    pub rs: f64,
    pub n: f64,
    pub tt: f64,
    pub cjo: f64,
    pub vj: f64,
    pub m: f64,
    pub eg: f64,
    pub xti: f64,
    pub fc: f64,
    pub bv: Option<f64>,
    pub ibv: f64,
    pub tnom: f64,
}

/// `DIOsetup` (instance part) + `DIOtempUpdate` of ngspice-47 at
/// `temp`, without the self-heating derivatives. `area`/`pj` are the
/// effective values of [`setup_geometry`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn temp_update(
    name: &str,
    b: &DioBase3f5,
    md: &DioModelExt,
    area: f64,
    pj: f64,
    cover: f64,
    m: f64,
    temp: f64,
    reltol: f64,
    notes: &mut Vec<String>,
) -> DioExtBase {
    // `diosetup.c` defaults and floors.
    let ns = md.ns.unwrap_or(1.0);
    let nbv = md.nbv.unwrap_or(b.n);
    let eg = if md.eg_given || md.tlev != 2 {
        b.eg
    } else {
        1.16
    };
    let is = if b.is < EPSMIN { EPSMIN } else { b.is };
    let knee = |v: Option<f64>, what: &str, notes: &mut Vec<String>| match v {
        Some(k) if k < EPSMIN => {
            notes.push(format!("{name}: {what} too small - model effect disabled"));
            None
        }
        k => k,
    };
    let ikf = knee(md.ikf, "IKF", notes).map(|k| k * area * m);
    let ikr = knee(md.ikr, "IKR", notes).map(|k| k * area * m);
    let ikp = knee(md.ikp, "IKP", notes).map(|k| k * pj * m);
    let junction_cap = b.cjo * area * m;
    let junction_sw_cap = md.cjsw * pj * m;
    let conductance = if b.rs == 0.0 { 0.0 } else { 1.0 / b.rs };

    // `DIOtempUpdate`.
    let tnom = b.tnom;
    let vt = KOVERQ * temp;
    let vte = b.n * vt;
    let vts = ns * vt;
    let vtt = md.ntun * vt;
    let vtr = md.nr * vt;
    let vtnom = KOVERQ * tnom;
    let dt = temp - tnom;
    let ln_t_ratio = f64::ln(temp / tnom);

    let factor = 1.0 + (md.tm1 * dt) + (md.tm2 * dt * dt);
    let grading = b.m * factor;

    let (egfet, egfet1) = if md.tlev == 0 || md.tlev == 1 {
        (
            1.16 - (7.02e-4 * temp * temp) / (temp + 1108.0),
            1.16 - (7.02e-4 * tnom * tnom) / (tnom + 1108.0),
        )
    } else {
        (
            eg - (md.gap1 * temp * temp) / (temp + md.gap2),
            eg - (md.gap1 * tnom * tnom) / (tnom + md.gap2),
        )
    };
    let fact2 = temp / REFTEMP;
    let arg = -egfet / (2.0 * BOLTZ * temp) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
    let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);
    let arg1 = -egfet1 / (BOLTZ * 2.0 * tnom) + 1.1150877 / (2.0 * BOLTZ * REFTEMP);
    let fact1 = tnom / REFTEMP;
    let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);

    // TLEVC is 0 or 1 (the elaborator refuses the rest, which leave
    // the C's values unset).
    let (jct_cap, jct_pot) = if md.tlevc == 0 {
        let pbo = (b.vj - pbfact1) / fact1;
        let gmaold = (b.vj - pbo) / pbo;
        let mut c = junction_cap / (1.0 + grading * (400e-6 * (tnom - REFTEMP) - gmaold));
        let pot = pbfact + fact2 * pbo;
        let gmanew = (pot - pbo) / pbo;
        c *= 1.0 + grading * (400e-6 * (temp - REFTEMP) - gmanew);
        (c, pot)
    } else {
        (
            junction_cap * (1.0 + md.cta * (temp - REFTEMP)),
            b.vj - md.tpb * (temp - REFTEMP),
        )
    };
    let (sw_cap, sw_pot) = if md.tlevc == 0 {
        let pbo_sw = (md.php - pbfact1) / fact1;
        let gma_sw_old = (md.php - pbo_sw) / pbo_sw;
        let mut c = junction_sw_cap / (1.0 + md.mjsw * (400e-6 * (tnom - REFTEMP) - gma_sw_old));
        let pot = pbfact + fact2 * pbo_sw;
        let gma_sw_new = (pot - pbo_sw) / pbo_sw;
        c *= 1.0 + md.mjsw * (400e-6 * (temp - REFTEMP) - gma_sw_new);
        (c, pot)
    } else {
        (
            junction_sw_cap * (1.0 + md.ctp * (temp - REFTEMP)),
            md.php - md.tphp * (temp - REFTEMP),
        )
    };

    let (csat, csatsw, tun, tunsw, rec);
    if md.tlev == 0 || md.tlev == 1 {
        let arg1 = ((temp / tnom) - 1.0) * eg / vte;
        let arg2 = b.xti / b.n * ln_t_ratio;
        csat = is * area * m * f64::exp(arg1 + arg2);
        let arg1 = ((temp / tnom) - 1.0) * eg / vts;
        let arg2 = b.xti / ns * ln_t_ratio;
        csatsw = md.jsw.unwrap_or(0.0) * pj * m * f64::exp(arg1 + arg2);
        let arg1 = ((temp / tnom) - 1.0) * md.keg * eg / vtt;
        let arg2 = md.xtitun / md.ntun * ln_t_ratio;
        tun = md.jtun.unwrap_or(0.0) * area * m * f64::exp(arg1 + arg2);
        tunsw = md.jtunsw.unwrap_or(0.0) * pj * m * f64::exp(arg1 + arg2);
        let arg1 = ((temp / tnom) - 1.0) * eg / vtr;
        let arg2 = b.xti / md.nr * ln_t_ratio;
        rec = md.isr.unwrap_or(1e-14) * area * m * f64::exp(arg1 + arg2);
    } else {
        let arg0 = egfet1 / (b.n * vtnom);
        let arg1 = egfet / vte;
        let arg2 = b.xti / b.n * ln_t_ratio;
        csat = is * area * m * f64::exp(arg0 - arg1 + arg2);
        let arg0 = egfet1 / (ns * vtnom);
        let arg1 = egfet / vts;
        let arg2 = b.xti / ns * ln_t_ratio;
        csatsw = md.jsw.unwrap_or(0.0) * pj * m * f64::exp(arg0 - arg1 + arg2);
        let arg0 = md.keg * egfet1 / (md.ntun * vtnom);
        let arg1 = md.keg * egfet / vtt;
        let arg2 = md.xtitun / md.ntun * ln_t_ratio;
        tun = md.jtun.unwrap_or(0.0) * area * m * f64::exp(arg0 - arg1 + arg2);
        tunsw = md.jtunsw.unwrap_or(0.0) * pj * m * f64::exp(arg0 - arg1 + arg2);
        let arg0 = egfet1 / (md.nr * vtnom);
        let arg1 = egfet / vtr;
        let arg2 = b.xti / md.nr * ln_t_ratio;
        rec = md.isr.unwrap_or(1e-14) * area * m * f64::exp(arg0 - arg1 + arg2);
    }

    let xfc = f64::ln(1.0 - b.fc);
    let xfcs = f64::ln(1.0 - md.fcs);
    let f1 = jct_pot * (1.0 - f64::exp((1.0 - grading) * xfc)) / (1.0 - grading);
    let dep_cap = b.fc * jct_pot;
    let sw_dep_cap = md.fcs * sw_pot;
    let total_sat_cur = csat + csatsw;
    let vcrit = vte * f64::ln(vte / (ROOT2 * total_sat_cur));

    let brkdwn = b.bv.map(|bv| {
        let t_bv = if md.tlev == 0 {
            bv - md.tcv * dt
        } else {
            bv * (1.0 - md.tcv * dt)
        };
        let cbv = if md.level == 1 {
            m * b.ibv
        } else {
            b.ibv * area * m
        };
        if cbv < total_sat_cur * t_bv / vt {
            notes.push(format!(
                "{name}: breakdown current increased to {:.3e} to resolve \
                 incompatibility with specified saturation current",
                total_sat_cur * t_bv / vt
            ));
            t_bv
        } else {
            let tol = reltol * cbv;
            let mut xbv = t_bv - nbv * vt * f64::ln(1.0 + cbv / total_sat_cur);
            for _ in 0..25 {
                xbv = t_bv - nbv * vt * f64::ln(cbv / total_sat_cur + 1.0 - xbv / vt);
                let xcbv = total_sat_cur * (f64::exp((t_bv - xbv) / (nbv * vt)) - 1.0 + xbv / vt);
                if (xcbv - cbv).abs() <= tol {
                    return xbv;
                }
            }
            notes.push(format!(
                "{name}: unable to match forward and reverse diode regions: \
                 bv = {xbv:.5e}, ibv = {cbv:.3e}"
            ));
            xbv
        }
    });

    let factor = 1.0 + (md.ttt1 * dt) + (md.ttt2 * dt * dt);
    let tt = b.tt * factor;

    let mut gspr = conductance * area * m;
    if b.rs != 0.0 {
        let factor = 1.0 + md.trs * dt + (md.trs2 * dt * dt);
        gspr = conductance * area * m / factor;
    }

    let f2 = f64::exp((1.0 + grading) * xfc);
    let f3 = 1.0 - b.fc * (1.0 + grading);
    let sw_f2 = f64::exp((1.0 + md.mjsw) * xfcs);
    let sw_f3 = 1.0 - md.fcs * (1.0 + md.mjsw);

    DioExtBase {
        csat,
        gspr,
        vt,
        vte,
        vtebrk: nbv * vt,
        vcrit,
        brkdwn,
        tt,
        ext: DioExtT {
            csatsw,
            vtesw: vts,
            sw_given: md.jsw.is_some(),
            ns_given: md.ns.is_some(),
            tun: md.jtun.map(|_| tun),
            tunsw: md.jtunsw.map(|_| tunsw),
            vtetun: vtt,
            rec: md.isr.map(|_| rec),
            vterec: vtr,
            ikf,
            ikr,
            ikp,
            jct_pot,
            grading,
            jct_cap,
            dep_cap,
            f1,
            f2,
            f3,
            sw_cap,
            sw_pot,
            sw_dep_cap,
            sw_f2,
            sw_f3,
            mjsw: md.mjsw,
            cover,
        },
    }
}

impl DioExtT {
    /// The junction current and conductance of `dioload.c` (no
    /// `RSW`, no self-heating), `gmin` excluded: `(cd, gd)`.
    /// `csat`, `vte`, `vtebrk` and the breakdown `xbv` come from the
    /// base temperature pass.
    pub(crate) fn current(
        &self,
        vd: f64,
        csat: f64,
        vte: f64,
        vtebrk: f64,
        brkdwn: Option<f64>,
    ) -> (f64, f64) {
        let e = crate::consts::E;
        let csatsw = self.csatsw;
        let vtesw = self.vtesw;
        let (mut cdsw, mut gdsw) = (0.0, 0.0);
        // Without `RSW` the C's `vdsw` is never assigned in a normal
        // iteration: it reads 0 (the sidewall breakdown term below).
        let vdsw = 0.0;
        let below_bv = |v: f64| brkdwn.is_some_and(|bv| v < -bv);

        if self.sw_given && self.ns_given {
            let vds = vd;
            if vds >= -3.0 * vtesw {
                let evd = f64::exp(vds / vtesw);
                cdsw = csatsw * (evd - 1.0);
                gdsw = csatsw * evd / vtesw;
            } else if !below_bv(vds) {
                let argsw = 3.0 * vtesw / (vds * e);
                let argsw = argsw * argsw * argsw;
                cdsw = -csatsw * (1.0 + argsw);
                gdsw = csatsw * 3.0 * argsw / vds;
            } else {
                let bv = brkdwn.unwrap_or(0.0);
                let evrev = f64::exp(-(bv + vds) / vtebrk);
                cdsw = -csatsw * evrev;
                gdsw = csatsw * evrev / vtebrk;
            }
        }

        let (mut cdb, mut gdb);
        let gen_fac_at = |v: f64| {
            let t1 = (1.0 - v / self.jct_pot).powf(2.0) + 0.005;
            (t1, t1.powf(self.grading / 2.0))
        };
        if vd >= -3.0 * vte {
            let evd = f64::exp(vd / vte);
            cdb = csat * (evd - 1.0);
            gdb = csat * evd / vte;
            if self.sw_given && !self.ns_given {
                cdsw = csatsw * (evd - 1.0);
                gdsw = csatsw * evd / vte;
            }
            if let Some(rec) = self.rec {
                let evd_rec = f64::exp(vd / self.vterec);
                let mut cdb_rec = rec * (evd_rec - 1.0);
                let mut gdb_rec = rec * evd_rec / self.vterec;
                let (t1, gen_fac) = gen_fac_at(vd);
                let gen_fac_vd =
                    -self.grading * (1.0 - vd / self.jct_pot) * t1.powf(self.grading / 2.0 - 1.0);
                cdb_rec *= gen_fac;
                // `dioload.c` multiplies by the already-scaled
                // `cdb_rec` (docs/diode.md, Fidelidade).
                gdb_rec = gdb_rec * gen_fac + cdb_rec * gen_fac_vd;
                cdb += cdb_rec;
                gdb += gdb_rec;
            }
        } else if !below_bv(vd) {
            let arg = 3.0 * vte / (vd * e);
            let arg = arg * arg * arg;
            cdb = match self.rec {
                Some(rec) => {
                    let evd_rec = f64::exp((-3.0 * vte) / self.vterec);
                    let cdb_rec = rec * (evd_rec - 1.0);
                    let (_, gen_fac) = gen_fac_at(-3.0 * vte);
                    -csat * (1.0 + arg) + gen_fac * cdb_rec
                }
                None => -csat * (1.0 + arg),
            };
            gdb = csat * 3.0 * arg / vd;
            if self.sw_given && !self.ns_given {
                cdsw = -csatsw * (1.0 + arg);
                gdsw = csatsw * 3.0 * arg / vd;
            }
        } else {
            let bv = brkdwn.unwrap_or(0.0);
            let evrev = f64::exp(-(bv + vd) / vtebrk);
            cdb = match self.rec {
                Some(rec) => {
                    let evd_rec = f64::exp((-3.0 * vte) / self.vterec);
                    let cdb_rec = rec * (evd_rec - 1.0);
                    let (_, gen_fac) = gen_fac_at(-3.0 * vte);
                    -csat * evrev + gen_fac * cdb_rec
                }
                None => -csat * evrev,
            };
            gdb = csat * evrev / vtebrk;
            if self.sw_given && !self.ns_given {
                let evrev = f64::exp(-(bv + vdsw) / vtebrk);
                cdsw = -csatsw * evrev;
                gdsw = csatsw * evrev / vtebrk;
            }
        }

        if let Some(tunsw) = self.tunsw {
            let evd = f64::exp(-vd / self.vtetun);
            cdsw -= tunsw * (evd - 1.0);
            gdsw += tunsw * evd / self.vtetun;
        }
        if let Some(tun) = self.tun {
            let evd = f64::exp(-vd / self.vtetun);
            cdb -= tun * (evd - 1.0);
            gdb += tun * evd / self.vtetun;
        }

        if vd >= -3.0 * vte {
            if let Some(ikf) = self.ikf
                && cdb > 1.0e-18
            {
                let sqrt_ikx = f64::sqrt(cdb / ikf);
                gdb = ((1.0 + sqrt_ikx) * gdb - cdb * gdb / (2.0 * sqrt_ikx * ikf))
                    / (1.0 + 2.0 * sqrt_ikx + cdb / ikf);
                cdb /= 1.0 + sqrt_ikx;
            }
        } else if let Some(ikr) = self.ikr
            && cdb < -1.0e-18
        {
            let sqrt_ikx = f64::sqrt(cdb / (-ikr));
            gdb = ((1.0 + sqrt_ikx) * gdb + cdb * gdb / (2.0 * sqrt_ikx * ikr))
                / (1.0 + 2.0 * sqrt_ikx - cdb / ikr);
            cdb /= 1.0 + sqrt_ikx;
        }
        if let Some(ikp) = self.ikp
            && cdsw > 1.0e-18
        {
            let sqrt_ikx = f64::sqrt(cdsw / ikp);
            gdsw = ((1.0 + sqrt_ikx) * gdsw - cdsw * gdsw / (2.0 * sqrt_ikx * ikp))
                / (1.0 + 2.0 * sqrt_ikx + cdsw / ikp);
            cdsw /= 1.0 + sqrt_ikx;
        }
        (cdb + cdsw, gdb + gdsw)
    }

    /// The depletion and overlap charge and capacitance of
    /// `dioload.c` (bottom, sidewall, `cmetal + cpoly`), without the
    /// diffusion part: `(q, c)`.
    pub(crate) fn depletion(&self, vd: f64) -> (f64, f64) {
        let czero = self.jct_cap;
        let (pot, mj) = (self.jct_pot, self.grading);
        let (deplcharge, deplcap) = if vd < self.dep_cap {
            let arg = 1.0 - vd / pot;
            let sarg = f64::exp(-mj * f64::ln(arg));
            (pot * czero * (1.0 - arg * sarg) / (1.0 - mj), czero * sarg)
        } else {
            let czof2 = czero / self.f2;
            (
                czero * self.f1
                    + czof2
                        * (self.f3 * (vd - self.dep_cap)
                            + (mj / (pot + pot)) * (vd * vd - self.dep_cap * self.dep_cap)),
                czof2 * (self.f3 + mj * vd / pot),
            )
        };
        let czero_sw = self.sw_cap;
        let (pot_sw, mjsw) = (self.sw_pot, self.mjsw);
        let vdx = vd;
        let (charge_sw, cap_sw) = if vdx < self.sw_dep_cap {
            let arg_sw = 1.0 - vdx / pot_sw;
            let sarg_sw = f64::exp(-mjsw * f64::ln(arg_sw));
            (
                pot_sw * czero_sw * (1.0 - arg_sw * sarg_sw) / (1.0 - mjsw),
                czero_sw * sarg_sw,
            )
        } else {
            let czof2_sw = czero_sw / self.sw_f2;
            (
                // `dioload.c` reuses the bottom junction's `tF1` here
                // (docs/diode.md, Fidelidade).
                czero_sw * self.f1
                    + czof2_sw
                        * (self.sw_f3 * (vdx - self.sw_dep_cap)
                            + (mjsw / (pot_sw + pot_sw))
                                * (vdx * vdx - self.sw_dep_cap * self.sw_dep_cap)),
                czof2_sw * (self.sw_f3 + mjsw * vdx / pot_sw),
            )
        };
        (
            deplcharge + charge_sw + self.cover * vd,
            deplcap + cap_sw + self.cover,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diode::{DiodeModel, DiodeTemp};

    fn inst(model: DioModelExt) -> DioExtInst {
        DioExtInst {
            model,
            geom: DioGeom::default(),
            scale: 1.0,
        }
    }

    /// With every extension at its default the ngspice-47 pass
    /// reproduces the 3f5 path's numbers (the same equations; only
    /// the order of a few products differs), but for the limiting
    /// voltage `vcrit`, which the 3f5 computes without the area.
    #[test]
    fn defaults_reduce_to_the_3f5_path() {
        let model = DiodeModel {
            is: 2e-14,
            rs: 5.0,
            n: 1.1,
            cjo: 3e-12,
            bv: Some(7.0),
            ..DiodeModel::default()
        };
        let mut x = inst(DioModelExt::default());
        x.geom.area_given = true;
        for (temp, area, m) in [(300.15, 1.0, 1.0), (350.0, 2.5, 3.0), (250.0, 0.5, 1.0)] {
            let a = DiodeTemp::new("d", &model, area, m, temp, 300.15, 1e-3);
            let b = DiodeTemp::new_ext("d", &model, area, m, temp, 300.15, 1e-3, &x);
            let close = |p: f64, q: f64| (p - q).abs() <= 1e-12 * p.abs().max(q.abs());
            assert!(close(a.csat, b.csat), "{temp}: {} {}", a.csat, b.csat);
            assert!(close(a.gspr, b.gspr));
            // The 3f5 `vcrit` has no area (KNOWN_DIFFS); ngspice's
            // uses `IS·AREA·M`.
            if area * m == 1.0 {
                assert!(close(a.vcrit, b.vcrit));
            }
            assert!(close(a.brkdwn.unwrap(), b.brkdwn.unwrap()));
            assert!(close(a.t_jct_cap * area * m, b.t_jct_cap));
            assert_eq!(a.vte, b.vtebrk);
        }
    }

    /// `diosetup.c`: at `LEVEL=3` with `W` and `L` the area and
    /// perimeter come from the geometry, times `M` and `scale`; the
    /// card's `AREA`/`PJ` otherwise, else the model's (1/0 once `W`
    /// or `L` is given).
    #[test]
    fn level3_geometry() {
        let mut md = DioModelExt {
            level: 3,
            xw: 0.1,
            area: 7.0,
            pj: 9.0,
            ..DioModelExt::default()
        };
        let mut x = inst(md.clone());
        x.geom.w = Some(10.0);
        x.geom.l = Some(5.0);
        x.scale = 1e-6;
        let (a, p, c) = setup_geometry(&x, 1.0, 2.0);
        assert!((a - 10.1 * 5.1 * 2.0 * 1e-12).abs() < 1e-24);
        assert!((p - (2.0 * 10.1 + 2.0 * 5.1) * 2.0 * 1e-6).abs() < 1e-18);
        assert_eq!(c, 0.0);
        let x = inst(md.clone());
        assert_eq!(setup_geometry(&x, 1.0, 1.0), (7.0, 9.0, 0.0));
        let mut x = inst(md.clone());
        x.geom.w = Some(3.0);
        assert_eq!(setup_geometry(&x, 1.0, 1.0), (1.0, 0.0, 0.0));
        md.level = 1;
        md.lm = 10.0;
        md.wm = 20.0;
        let mut x = inst(md);
        x.geom.w = Some(10.0);
        x.geom.l = Some(5.0);
        x.geom.area_given = true;
        // Level 1: W/L only select the defaults; no overlap caps.
        assert_eq!(setup_geometry(&x, 4.0, 1.0), (4.0, 0.0, 0.0));
    }

    /// The knee currents follow `dioload.c`: `IKF` halves the
    /// current where `cd = IKF` would be, and `IKR < CKTepsmin` is
    /// disabled with ngspice's warning.
    #[test]
    fn knees_and_their_floor() {
        let b = DioBase3f5 {
            is: 1e-14,
            rs: 0.0,
            n: 1.0,
            tt: 0.0,
            cjo: 0.0,
            vj: 1.0,
            m: 0.5,
            eg: 1.11,
            xti: 3.0,
            fc: 0.5,
            bv: None,
            ibv: 1e-3,
            tnom: 300.15,
        };
        let md = DioModelExt {
            ikf: Some(1e-3),
            ikr: Some(0.0),
            ..DioModelExt::default()
        };
        let mut notes = Vec::new();
        let t = temp_update("d", &b, &md, 1.0, 0.0, 0.0, 1.0, 300.15, 1e-3, &mut notes);
        assert_eq!(t.ext.ikr, None);
        assert!(notes[0].contains("IKR too small"), "{notes:?}");
        let vd = t.vte * f64::ln(1e-3 / 1e-14 + 1.0);
        let (cd, _) = t.ext.current(vd, t.csat, t.vte, t.vtebrk, None);
        assert!((cd - 0.5e-3).abs() < 1e-12, "{cd}");
    }
}
