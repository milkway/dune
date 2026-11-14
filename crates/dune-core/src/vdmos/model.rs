//! The VDMOS model card (`vdmos.c` keyword table, `vdmosmpar.c`) with
//! the `VDMOSsetup` defaults and model preprocessing (`vdmosset.c`),
//! and the temperature update `VDMOStempUpdate` (`vdmostemp.c`).

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};

/// `CKTepsmin` (`cktinit.c`): the floor of the body-diode saturation
/// current.
const EPSMIN: f64 = 1e-28;

/// `.model NAME VDMOS (…)` parameters — ngspice-47's power MOSFET
/// (`vdmos.c`), with ngspice's names and the defaults `VDMOSsetup`
/// gives them (`vdmosset.c`). `Option` marks the parameters whose
/// *given-ness* changes the equations (or whose default depends on
/// the channel type); the others hold their default until set.
///
/// The model is a square-law channel (`KP`, `VTO`, `LAMBDA`, `THETA`,
/// `MTRIODE`) with a logarithmic weak-inversion blend (`KSUBTHRES`,
/// `SUBSHIFT`), quasi-saturation (`RQ`, `VQ`), a gate-drain
/// capacitance `Cgd(vgd)` that goes from `CGDMIN` to `CGDMAX` through
/// a tanh/atan shape (`A`), a constant `CGS`, and a body diode between
/// source (anode, n-channel) and drain with series resistance `RB`,
/// breakdown (`BV`, `IBV`, `NBV`), depletion (`CJO`, `VJ`, `M`, `FC`)
/// and diffusion (`TT`) charge and optional soft reverse recovery
/// (`VP`). Self-heating (`RTHJC`, `RTHCA`, `CTHJ`) acts on instances
/// with the `thermal` flag.
#[derive(Clone, Debug)]
pub struct VdmosModel {
    /// N-channel (`vdmosn`, the default) or P-channel (`pchan` on the
    /// card, `vdmosp`).
    pub nmos: bool,
    /// `VTO`/`VTH0`: threshold voltage; default `3·type` (IRF540,
    /// IRF9540).
    pub vto: Option<f64>,
    /// `KP`: transconductance parameter, A/V²; default `15 + 5·type`
    /// (20 for n-channel, 10 for p-channel).
    pub kp: Option<f64>,
    /// `PHI`: surface potential, default 0.6 (must be positive).
    pub phi: f64,
    /// `LAMBDA`: channel-length modulation, default 0.
    pub lambda: f64,
    /// `THETA`: mobility degradation with gate drive, default 0.
    pub theta: f64,
    /// `RD`: drain ohmic resistance, default 0.
    pub rd: f64,
    /// `RS`: source ohmic resistance, default 0.
    pub rs: f64,
    /// `RG`: gate ohmic resistance, default 0.
    pub rg: f64,
    /// `TNOM`, kelvin (`.options tnom` when absent).
    pub tnom: Option<f64>,
    /// `KF`: flicker-noise coefficient, default 0.
    pub kf: f64,
    /// `AF`: flicker-noise exponent, default 1.
    pub af: f64,
    /// `RQ`: quasi-saturation resistance; acts only together with
    /// `VQ`.
    pub rq: Option<f64>,
    /// `VQ`: quasi-saturation voltage; acts only together with `RQ`.
    pub vq: Option<f64>,
    /// `MTRIODE`: conductance multiplier in the triode region,
    /// default 1.
    pub mtriode: f64,
    /// `TCVTH`/`VTOTC`: linear temperature coefficient of `VTO`,
    /// default 0.
    pub tcvth: f64,
    /// `MU`/`BEX`: exponent of the `KP` temperature law, default −1.5.
    pub mu: f64,
    /// `TEXP0`: temperature exponent of `RD`; when given it replaces
    /// the `TRD1`/`TRD2` polynomial (default 1.5).
    pub texp0: Option<f64>,
    /// `TEXP1`: temperature exponent of `RQ`, default 0.3.
    pub texp1: f64,
    /// `TRD1`/`TRD2`, `TRG1`/`TRG2`, `TRS1`/`TRS2`, `TRB1`/`TRB2`:
    /// linear and quadratic temperature coefficients of `RD`, `RG`,
    /// `RS` and `RB`; default 0.
    pub trd1: f64,
    pub trd2: f64,
    pub trg1: f64,
    pub trg2: f64,
    pub trs1: f64,
    pub trs2: f64,
    pub trb1: f64,
    pub trb2: f64,
    /// `SUBSHIFT`: shift of the weak-inversion curve along vgs,
    /// default 0.
    pub subshift: f64,
    /// `KSUBTHRES`: weak-inversion slope, default 0.1.
    pub ksubthres: f64,
    /// `TKSUBTHRES1`/`TKSUBTHRES2`: temperature coefficients of
    /// `KSUBTHRES`, default 0.
    pub tksubthres1: f64,
    pub tksubthres2: f64,
    /// `BV`: body-diode breakdown voltage (no breakdown when absent;
    /// the C's default 1e99 is never read without the flag).
    pub bv: Option<f64>,
    /// `IBV`: current at `BV`, default 1e-10.
    pub ibv: f64,
    /// `NBV`: breakdown emission coefficient, default 1.
    pub nbv: f64,
    /// `RDS`: drain-source shunt resistance (absent: a 1e-15 S shunt
    /// regardless of `M`).
    pub rds: Option<f64>,
    /// `RB`: body-diode ohmic resistance, default 0.
    pub rb: f64,
    /// `N`: body-diode emission coefficient, default 1.
    pub n: f64,
    /// `TT`: body-diode transit time, default 0.
    pub tt: f64,
    /// `EG`: activation energy of the `IS` temperature law, default
    /// 1.11 (floored at 0.1).
    pub eg: f64,
    /// `XTI`: saturation-current temperature exponent, default 3.
    pub xti: f64,
    /// `IS`: body-diode saturation current, default 1e-14.
    pub is: f64,
    /// `VJ`: body-diode junction potential, default 0.8.
    pub vj: f64,
    /// `VP`: soft reverse recovery parameter, default 0 (off).
    pub vp: f64,
    /// `CJO`: zero-bias body-diode capacitance, default 5e-10.
    pub cjo: f64,
    /// `M` (model): body-diode grading coefficient, default 0.5.
    pub mj: f64,
    /// `FC`: forward-bias depletion coefficient, default 0.5
    /// (capped at 0.95).
    pub fc: f64,
    /// `CGDMIN`, `CGDMAX`: the limits of the gate-drain capacitance,
    /// default 2e-11 and 2e-9.
    pub cgdmin: f64,
    pub cgdmax: f64,
    /// `A`: shape parameter of `Cgd(vgd)`, default 1.
    pub a: f64,
    /// `CGS`: gate-source capacitance, default 1.4e-9.
    pub cgs: f64,
    /// `RTHJC`: junction-to-case thermal resistance, default 1;
    /// self-heating needs it given.
    pub rthjc: Option<f64>,
    /// `RTHCA`: case-to-ambient thermal resistance, default 23.
    pub rthca: f64,
    /// `CTHJ`: junction thermal capacitance, default 10e-6.
    pub cthj: f64,
    /// The safe-operating-area limits (`VGS_MAX`, `VGD_MAX`,
    /// `VDS_MAX`, `VGSR_MAX`, `VGDR_MAX`, `PD_MAX`, `ID_MAX`,
    /// `IDR_MAX`, `TE_MAX`, `RTH_EXT`, `DERATING`): accepted and kept
    /// with ngspice's defaults, but Dune has no SOA check
    /// (`vdmossoachk.c`), so they have no effect.
    pub soa: VdmosSoa,
}

/// The SOA-check parameters of a [`VdmosModel`] (inert in Dune).
#[derive(Clone, Debug)]
pub struct VdmosSoa {
    /// `VGS_MAX`, default 1e99.
    pub vgs_max: f64,
    /// `VGD_MAX`, default 1e99.
    pub vgd_max: f64,
    /// `VDS_MAX`, default 1e99.
    pub vds_max: f64,
    /// `VGSR_MAX`, default 1e99.
    pub vgsr_max: f64,
    /// `VGDR_MAX`, default 1e99.
    pub vgdr_max: f64,
    /// `PD_MAX`, default 1e99.
    pub pd_max: f64,
    /// `ID_MAX`: 0 when absent — `vdmosset.c` writes the 1e99
    /// default into `PD_MAX` instead (a copy-paste slip in the C).
    pub id_max: f64,
    /// `IDR_MAX`, default 1e99.
    pub idr_max: f64,
    /// `TE_MAX`, default 1e99.
    pub te_max: f64,
    /// `RTH_EXT`: default `RTHCA`.
    pub rth_ext: Option<f64>,
    /// `DERATING`, default 0.
    pub derating: f64,
}

impl VdmosModel {
    /// A model card with every parameter at its `VDMOSsetup` default.
    pub fn new(nmos: bool) -> Self {
        VdmosModel {
            nmos,
            vto: None,
            kp: None,
            phi: 0.6,
            lambda: 0.0,
            theta: 0.0,
            rd: 0.0,
            rs: 0.0,
            rg: 0.0,
            tnom: None,
            kf: 0.0,
            af: 1.0,
            rq: None,
            vq: None,
            mtriode: 1.0,
            tcvth: 0.0,
            mu: -1.5,
            texp0: None,
            texp1: 0.3,
            trd1: 0.0,
            trd2: 0.0,
            trg1: 0.0,
            trg2: 0.0,
            trs1: 0.0,
            trs2: 0.0,
            trb1: 0.0,
            trb2: 0.0,
            subshift: 0.0,
            ksubthres: 0.1,
            tksubthres1: 0.0,
            tksubthres2: 0.0,
            bv: None,
            ibv: 1e-10,
            nbv: 1.0,
            rds: None,
            rb: 0.0,
            n: 1.0,
            tt: 0.0,
            eg: 1.11,
            xti: 3.0,
            is: 1e-14,
            vj: 0.8,
            vp: 0.0,
            cjo: 5e-10,
            mj: 0.5,
            fc: 0.5,
            cgdmin: 2e-11,
            cgdmax: 2e-9,
            a: 1.0,
            cgs: 1.4e-9,
            rthjc: None,
            rthca: 23.0,
            cthj: 10e-6,
            soa: VdmosSoa {
                vgs_max: 1e99,
                vgd_max: 1e99,
                vds_max: 1e99,
                vgsr_max: 1e99,
                vgdr_max: 1e99,
                pd_max: 1e99,
                id_max: 0.0,
                idr_max: 1e99,
                te_max: 1e99,
                rth_ext: None,
                derating: 0.0,
            },
        }
    }

    /// Set one model parameter by its ngspice keyword (`VDMOSmParam`).
    /// Unknown keywords are an `Err`. The type flags (`vdmosn`,
    /// `vdmosp`, `vdmos`) are taken as values here: nonzero selects.
    pub fn set(&mut self, key: &str, v: f64) -> Result<(), String> {
        match key {
            "vto" | "vth0" => self.vto = Some(v),
            "kp" => self.kp = Some(v),
            "phi" => self.phi = v,
            "lambda" => self.lambda = v,
            "theta" => self.theta = v,
            "rd" => self.rd = v,
            "rs" => self.rs = v,
            "rg" => self.rg = v,
            "tnom" => self.tnom = Some(v + crate::consts::CTOK),
            "kf" => self.kf = v,
            "af" => self.af = v,
            "vdmosn" | "vdmos" if v != 0.0 => self.nmos = true,
            "vdmosp" if v != 0.0 => self.nmos = false,
            "vdmosn" | "vdmosp" | "vdmos" => {}
            "rq" => self.rq = Some(v),
            "vq" => self.vq = Some(v),
            "mtriode" => self.mtriode = v,
            "tcvth" | "vtotc" => self.tcvth = v,
            "mu" | "bex" => self.mu = v,
            "texp0" => self.texp0 = Some(v),
            "texp1" => self.texp1 = v,
            "trd1" => self.trd1 = v,
            "trd2" => self.trd2 = v,
            "trg1" => self.trg1 = v,
            "trg2" => self.trg2 = v,
            "trs1" => self.trs1 = v,
            "trs2" => self.trs2 = v,
            "trb1" => self.trb1 = v,
            "trb2" => self.trb2 = v,
            "subshift" => self.subshift = v,
            "ksubthres" => self.ksubthres = v,
            "tksubthres1" => self.tksubthres1 = v,
            "tksubthres2" => self.tksubthres2 = v,
            "bv" => self.bv = Some(v),
            "ibv" => self.ibv = v,
            "nbv" => self.nbv = v,
            "rds" => self.rds = Some(v),
            "rb" => self.rb = v,
            "n" => self.n = v,
            "tt" => self.tt = v,
            "eg" => self.eg = v,
            "xti" => self.xti = v,
            "is" => self.is = v,
            "vj" => self.vj = v,
            "vp" => self.vp = v,
            "cjo" => self.cjo = v,
            "m" => self.mj = v,
            "fc" => self.fc = v,
            "cgdmin" => self.cgdmin = v,
            "cgdmax" => self.cgdmax = v,
            "a" => self.a = v,
            "cgs" => self.cgs = v,
            "rthjc" => self.rthjc = Some(v),
            "rthca" => self.rthca = v,
            "cthj" => self.cthj = v,
            "vgs_max" => self.soa.vgs_max = v,
            "vgd_max" => self.soa.vgd_max = v,
            "vds_max" => self.soa.vds_max = v,
            "vgsr_max" => self.soa.vgsr_max = v,
            "vgdr_max" => self.soa.vgdr_max = v,
            "pd_max" => self.soa.pd_max = v,
            "id_max" => self.soa.id_max = v,
            "idr_max" => self.soa.idr_max = v,
            "te_max" => self.soa.te_max = v,
            "rth_ext" => self.soa.rth_ext = Some(v),
            "derating" => self.soa.derating = v,
            _ => return Err(format!("unrecognized parameter ({key})")),
        }
        Ok(())
    }

    /// The channel type as the C's `VDMOStype` (+1 / −1).
    pub(crate) fn typ(&self) -> f64 {
        if self.nmos { 1.0 } else { -1.0 }
    }

    /// Quasi-saturation is on only with both `RQ` and `VQ` given
    /// (`VDMOSqsGiven`).
    pub(crate) fn qs(&self) -> bool {
        self.rq.is_some() && self.vq.is_some()
    }

    /// Soft reverse recovery (`VDIOqpNode`): `VP` nonzero and `TT`
    /// nonzero.
    pub(crate) fn revrec(&self) -> bool {
        self.vp != 0.0 && self.tt != 0.0
    }

    /// `VDMOSsetup`'s model pass: the type-dependent defaults of `KP`
    /// and `VTO`, `TNOM`, and the clamps on `EG` (≥ 0.1), `FC`
    /// (≤ 0.95) and `IS` (≥ `CKTepsmin`), with the C's warnings in
    /// `notes`; a non-positive `PHI` is fatal.
    pub(crate) fn prepare(
        &self,
        name: &str,
        default_tnom: f64,
        notes: &mut Vec<String>,
    ) -> Result<VdmosModel, String> {
        let mut p = self.clone();
        let typ = self.typ();
        p.kp = Some(self.kp.unwrap_or(15.0 + 5.0 * typ));
        p.vto = Some(self.vto.unwrap_or(3.0 * typ));
        p.tnom = Some(self.tnom.unwrap_or(default_tnom));
        if p.phi <= 0.0 {
            return Err(format!("{name}: Phi is not positive."));
        }
        if p.eg < 0.1 {
            notes.push(format!(
                "{name}: body diode activation energy too small, limited to 0.1"
            ));
            p.eg = 0.1;
        }
        if p.fc > 0.95 {
            notes.push(format!("{name}: coefficient Fc too large, limited to 0.95"));
            p.fc = 0.95;
        }
        if p.is < EPSMIN {
            p.is = EPSMIN;
        }
        Ok(p)
    }
}

/// `VDMOSoxideCapFactor`: the setup assumes a 100 nm oxide (it only
/// enters the flicker noise and the distortion setup).
pub(crate) const OXIDE_CAP_FACTOR: f64 = 3.9 * 8.854214871e-12 / 1e-07;

/// The temperature-dependent instance values of `VDMOStempUpdate`
/// (`vdmostemp.c`) at one device temperature.
#[derive(Clone, Debug, Default)]
pub(crate) struct VdmosTemp {
    pub t_transconductance: f64,
    pub t_vth: f64,
    pub t_ksubthres: f64,
    /// `RD/M` at temperature (instance `VDMOSdrainResistance`).
    pub drain_resistance: f64,
    pub gate_conductance: f64,
    pub source_conductance: f64,
    /// `RQ/M` at temperature (0 without quasi-saturation).
    pub qs_resistance: f64,
    pub t_phi: f64,
    // Body diode.
    pub t_grading: f64,
    pub t_jct_cap: f64,
    pub t_jct_pot: f64,
    pub t_sat_cur: f64,
    pub t_sat_cur_dt: f64,
    pub t_f1: f64,
    pub t_f2: f64,
    pub t_f3: f64,
    pub t_dep_cap: f64,
    pub t_vcrit: f64,
    /// 0 without `BV` (the C leaves the field at its calloc zero).
    pub t_brkdwn_v: f64,
    pub t_transit_time: f64,
    pub t_conductance: f64,
    pub t_conductance_dt: f64,
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

impl VdmosTemp {
    /// `VDMOStempUpdate(model, here, Temp)`: `p` is the prepared
    /// model ([`VdmosModel::prepare`]), `m` the instance multiplier,
    /// `temp` kelvin. Warnings (the `VJ` clamp) go to `notes`.
    pub(crate) fn new(
        p: &VdmosModel,
        m: f64,
        temp: f64,
        reltol: f64,
        notes: &mut Vec<String>,
        name: &str,
    ) -> VdmosTemp {
        let tnom = p.tnom.unwrap_or(REFTEMP);
        let fact1 = tnom / REFTEMP;
        let vtnom = tnom * KOVERQ;
        let kt1 = BOLTZ * tnom;
        let egfet1 = egfet(tnom);
        let arg1 = -egfet1 / (kt1 + kt1) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);

        let xfc = f64::ln(1.0 - p.fc);
        let dt = temp - tnom;

        let ratio = temp / tnom;
        let t_transconductance = p.kp.unwrap_or(0.0) * m * ratio.powf(p.mu);
        let t_vth = p.vto.unwrap_or(0.0) + p.tcvth * dt;
        let t_ksubthres = p.ksubthres * (1.0 + (p.tksubthres1 * dt) + (p.tksubthres2 * dt * dt));
        let drain_resistance = match p.texp0 {
            Some(texp0) => p.rd / m * ratio.powf(texp0),
            None => p.rd / m * (1.0 + (p.trd1 * dt) + (p.trd2 * dt * dt)),
        };
        let gate_conductance = if p.rg > 0.0 {
            m / p.rg / (1.0 + (p.trg1 * dt) + (p.trg2 * dt * dt))
        } else {
            0.0
        };
        let source_conductance = if p.rs > 0.0 {
            m / p.rs / (1.0 + (p.trs1 * dt) + (p.trs2 * dt * dt))
        } else {
            0.0
        };
        let qs_resistance = if p.qs() {
            p.rq.unwrap_or(0.0) / m * ratio.powf(p.texp1)
        } else {
            0.0
        };

        let vt = temp * KOVERQ;
        let fact2 = temp / REFTEMP;
        let kt = temp * BOLTZ;
        let egfet_t = egfet(temp);
        let arg = -egfet_t / (kt + kt) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);

        let phio = (p.phi - pbfact1) / fact1;
        let t_phi = fact2 * phio + pbfact;

        // Body diode. The grading and transit-time temperature
        // coefficients are always 0 (`vdmosmpar.c` zeroes them and no
        // keyword sets them).
        let t_grading = p.mj;
        let pbo = (p.vj - pbfact1) / fact1;
        let gmaold = (p.vj - pbo) / pbo;
        let mut t_jct_cap = m * p.cjo / (1.0 + t_grading * (400e-6 * (tnom - REFTEMP) - gmaold));
        let mut t_jct_pot = pbfact + fact2 * pbo;
        let gmanew = (t_jct_pot - pbo) / pbo;
        t_jct_cap *= 1.0 + t_grading * (400e-6 * (temp - REFTEMP) - gmanew);

        let vte = p.n * vt;
        let arg1 = ((temp / tnom) - 1.0) * p.eg / vte;
        let arg1_dt = p.eg / (vte * tnom) - p.eg * (temp / tnom - 1.0) / (vte * temp);
        let arg2 = p.xti / p.n * f64::ln(temp / tnom);
        let arg2_dt = p.xti / p.n / temp;
        let t_sat_cur = m * p.is * f64::exp(arg1 + arg2);
        let t_sat_cur_dt = m * p.is * f64::exp(arg1 + arg2) * (arg1_dt + arg2_dt);

        let t_f1 = t_jct_pot * (1.0 - f64::exp((1.0 - t_grading) * xfc)) / (1.0 - t_grading);
        let mut t_dep_cap = p.fc * t_jct_pot;
        let t_vcrit = vte * f64::ln(vte / (ROOT2 * t_sat_cur));

        if t_dep_cap > 2.5 {
            t_jct_pot = 2.5 / p.n;
            t_dep_cap = p.n * t_jct_pot;
            notes.push(format!(
                "{name}: junction potential VJ too large, limited to {t_jct_pot:.6}"
            ));
        }

        let mut t_brkdwn_v = 0.0;
        if let Some(bv) = p.bv {
            let t_bv = bv.abs();
            let cbv = p.ibv;
            let xbv = if cbv < t_sat_cur * t_bv / vt {
                t_bv
            } else {
                let tol = reltol * cbv;
                let mut x = t_bv - p.nbv * vt * f64::ln(1.0 + cbv / t_sat_cur);
                for _ in 0..25 {
                    x = t_bv - p.nbv * vt * f64::ln(cbv / t_sat_cur + 1.0 - x / vt);
                    let xcbv = t_sat_cur * (f64::exp((t_bv - x) / (p.nbv * vt)) - 1.0 + x / vt);
                    if (xcbv - cbv).abs() <= tol {
                        break;
                    }
                }
                x
            };
            t_brkdwn_v = xbv;
        }

        let t_transit_time = p.tt;

        let dio_conductance = if p.rb > 0.0 { m / p.rb } else { 0.0 };
        let factor = 1.0 + p.trb1 * dt + (p.trb2 * dt * dt);
        let t_conductance = dio_conductance / factor;
        let t_conductance_dt = -dio_conductance * (p.trb1 + p.trb2 * dt) / (factor * factor);

        let t_f2 = f64::exp((1.0 + t_grading) * xfc);
        let t_f3 = 1.0 - p.fc * (1.0 + t_grading);

        VdmosTemp {
            t_transconductance,
            t_vth,
            t_ksubthres,
            drain_resistance,
            gate_conductance,
            source_conductance,
            qs_resistance,
            t_phi,
            t_grading,
            t_jct_cap,
            t_jct_pot,
            t_sat_cur,
            t_sat_cur_dt,
            t_f1,
            t_f2,
            t_f3,
            t_dep_cap,
            t_vcrit,
            t_brkdwn_v,
            t_transit_time,
            t_conductance,
            t_conductance_dt,
        }
    }
}

/// `DevCapVDMOS` (`devsup.c`): half the gate-source and gate-drain
/// capacitances at `vgd` — `Cgs` constant, `Cgd` from `CGDMIN` to
/// `CGDMAX` through `s·tanh(a·vgd)` above `vgd = 0` and
/// `s·atan(a·vgd)` below, `s = (CGDMAX − CGDMIN)/(1 + π/2)`.
pub(crate) fn dev_cap(vgd: f64, cgdmin: f64, cgdmax: f64, a: f64, cgs: f64) -> (f64, f64) {
    let s = (cgdmax - cgdmin) / (1.0 + std::f64::consts::PI / 2.0);
    let y = cgdmax - s;
    let capgd = if vgd > 0.0 {
        0.5 * (s * f64::tanh(a * vgd) + y)
    } else {
        0.5 * (s * f64::atan(a * vgd) + y)
    };
    (0.5 * cgs, capgd)
}

/// `DEVlimitlog` (`devsup.c`): logarithmic damping of the
/// junction-temperature step beyond `lim_tol`. A NaN iterate restarts
/// from 0 every time (the C does it once per process, behind a
/// `static` flag).
pub(crate) fn limitlog(deltemp: f64, deltemp_old: f64, lim_tol: f64, check: &mut bool) -> f64 {
    *check = false;
    let mut deltemp = deltemp;
    if deltemp.is_nan() || deltemp_old.is_nan() {
        deltemp = 0.0;
        *check = true;
    }
    if deltemp > deltemp_old + lim_tol {
        deltemp = deltemp_old + lim_tol + f64::log10((deltemp - deltemp_old) / lim_tol);
        *check = true;
    } else if deltemp < deltemp_old - lim_tol {
        deltemp = deltemp_old - lim_tol - f64::log10((deltemp_old - deltemp) / lim_tol);
        *check = true;
    }
    deltemp
}
