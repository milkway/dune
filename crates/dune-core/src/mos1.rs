//! MOSFET level 1 — the Shichman–Hodges model from the 3f5
//! (`dev/mos1/mos1load.c`, `mos1temp.c`, `mos1set.c`, `mos1acld.c`,
//! `mos1trunc.c`). Square-law drain current with channel-length
//! modulation, bulk junction diodes with depletion charge, Meyer
//! gate capacitances (`DEVqmeyer`), and series drain/source
//! resistance via prime nodes.
//!
//! The FET limiting helpers (`DEVfetlim`, `DEVlimvds`) and the Meyer
//! capacitance evaluation live here too; the other MOS levels reuse
//! them the way the BJT reuses the diode's `pnjlim`.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::diode::pnjlim;
use crate::disto::{DistoCtx, DistoSetupCtx};
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mosdisto::{
    DrainDerivs, MosDistoCoeffs, MosJunctions, bulk_diode, mos_disto_load, setup_tail,
};

/// `MAX_EXP_ARG` (`hw_ieee.h`): largest argument handed to `exp`.
const MAX_EXP_ARG: f64 = 709.0;

/// Name suffixes of the MOSFET noise generators
/// (`MOS{1,2,3}nNames`): drain and source series resistances,
/// channel thermal noise, flicker, total.
pub(crate) const MOS_NOISE_SUFFIXES: &[&str] = &["_rd", "_rs", "_id", "_1overf", ""];

/// The terminals and operating-point values a MOSFET noise
/// evaluation reads.
pub(crate) struct MosNoise {
    pub drain: Var,
    pub source: Var,
    pub d_p: Var,
    pub s_p: Var,
    pub gdpr: f64,
    pub gspr: f64,
    pub gm: f64,
    pub kf: f64,
    pub af: f64,
    /// Channel width and effective length (`W`, `L − 2·LD`).
    pub w: f64,
    pub eff_l: f64,
    /// Oxide capacitance per area used by the flicker term.
    pub cox: f64,
}

/// `MOS1noise`/`MOS2noise`/`MOS3noise` (ngspice-47) at their
/// default `NLEV = 2`: thermal noise of RD and RS, channel thermal
/// noise `4kT·(2/3)|gm|` (`nlev < 3`), and the flicker density
/// `KF·gm²/(f^AF·W·Leff·Cox)`. The 3f5 used
/// `KF·|Id|^AF/(f·W·Leff·Cox²)` instead (ngspice keeps that form
/// only under `ngbehavior=spice3`); the goldens hold ngspice's.
pub(crate) fn mos_noise(ctx: &crate::matrix::NoiseCtx, n: &MosNoise) -> Vec<f64> {
    let rd = ctx.thermal(n.d_p, n.drain, n.gdpr);
    let rs = ctx.thermal(n.s_p, n.source, n.gspr);
    let id = ctx.thermal(n.d_p, n.s_p, 2.0 / 3.0 * n.gm.abs());
    let flicker = ctx.gain2(n.d_p, n.s_p) * n.kf * n.gm * n.gm
        / (ctx.freq.powf(n.af) * n.w * n.eff_l * n.cox);
    vec![rd, rs, id, flicker, rd + rs + id + flicker]
}

/// `.model … nmos(level=1 …)/pmos(level=1 …)` parameters, 3f5 names
/// and defaults (`mos1.c` keyword table; defaults from `mos1set.c`).
/// `Option` marks parameters whose *given-ness* changes the
/// preprocessing in `MOS1temp` (e.g. `KP` computed from `U0`/`TOX`,
/// `VTO`/`PHI`/`GAMMA` from `NSUB`, `CBD` vs `CJ·AD`).
#[derive(Clone, Debug)]
pub struct Mos1Model {
    /// `NMOS` (+1) or `PMOS` (−1).
    pub nmos: bool,
    /// `VTO`/`VT0`: threshold voltage; default 0, or computed from
    /// `NSUB`/`NSS`/`TPG` when those are given.
    pub vto: Option<f64>,
    /// `KP`: transconductance; default 2e-5, or `U0·COX` with `TOX`.
    pub kp: Option<f64>,
    /// `GAMMA`: bulk threshold; default 0, or computed from `NSUB`.
    pub gamma: Option<f64>,
    /// `PHI`: surface potential; default 0.6, or from `NSUB`.
    pub phi: Option<f64>,
    /// `LAMBDA`: channel-length modulation, default 0.
    pub lambda: f64,
    /// `RD`: drain ohmic resistance, default 0.
    pub rd: Option<f64>,
    /// `RS`: source ohmic resistance, default 0.
    pub rs: Option<f64>,
    /// `CBD`: zero-bias B-D junction capacitance, default 0.
    pub cbd: Option<f64>,
    /// `CBS`: zero-bias B-S junction capacitance, default 0.
    pub cbs: Option<f64>,
    /// `IS`: bulk junction saturation current, default 1e-14.
    pub is: f64,
    /// `PB`: bulk junction potential, default 0.8.
    pub pb: f64,
    /// `CGSO`/`CGDO`/`CGBO`: overlap capacitances per meter,
    /// default 0.
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    /// `RSH`: drain/source diffusion sheet resistance, default 0.
    pub rsh: Option<f64>,
    /// `CJ`: bottom junction capacitance per m², default 0.
    pub cj: Option<f64>,
    /// `MJ`: bottom grading coefficient, default 0.5.
    pub mj: f64,
    /// `CJSW`: sidewall capacitance per meter, default 0.
    pub cjsw: Option<f64>,
    /// `MJSW`: sidewall grading coefficient, default 0.5.
    pub mjsw: f64,
    /// `JS`: bulk junction saturation current per m², default 0.
    pub js: f64,
    /// `TOX`: oxide thickness, meters.
    pub tox: Option<f64>,
    /// `LD`: lateral diffusion, default 0.
    pub ld: f64,
    /// `U0`/`UO`: surface mobility, cm²/V·s; default 600.
    pub u0: Option<f64>,
    /// `FC`: forward-bias depletion capacitance coefficient,
    /// default 0.5.
    pub fc: f64,
    /// `NSUB`: substrate doping, 1/cm³.
    pub nsub: Option<f64>,
    /// `TPG`: gate material type (+1 opposite of substrate, −1 same,
    /// 0 aluminum); default 1 when `NSUB` is given.
    pub tpg: Option<f64>,
    /// `NSS`: surface state density, 1/cm².
    pub nss: Option<f64>,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `TNOM` in kelvin.
    pub tnom: Option<f64>,
}

impl Mos1Model {
    pub fn new(nmos: bool) -> Self {
        Mos1Model {
            nmos,
            vto: None,
            kp: None,
            gamma: None,
            phi: None,
            lambda: 0.0,
            rd: None,
            rs: None,
            cbd: None,
            cbs: None,
            is: 1e-14,
            pb: 0.8,
            cgso: 0.0,
            cgdo: 0.0,
            cgbo: 0.0,
            rsh: None,
            cj: None,
            mj: 0.5,
            cjsw: None,
            mjsw: 0.5,
            js: 0.0,
            tox: None,
            ld: 0.0,
            u0: None,
            fc: 0.5,
            nsub: None,
            tpg: None,
            nss: None,
            kf: 0.0,
            af: 1.0,
            tnom: None,
        }
    }

    /// Does the drain (source) side get a prime node? (`mos1set.c`:
    /// `drainResistance != 0 || (sheetResistance != 0 && squares != 0)`.)
    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rd.unwrap_or(0.0) != 0.0 || (self.rsh.unwrap_or(0.0) != 0.0 && squares != 0.0)
    }
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

/// Instance geometry as the setup pass sees it (`mos1set.c`
/// instance defaults; `l`/`w`/`ad`/`as` fall back to the
/// `.options defl/defw/defad/defas` values).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MosGeom {
    pub l: f64,
    pub w: f64,
    pub ad: f64,
    pub as_: f64,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
}

/// Model- and temperature-derived values (`MOS1temp`, both the
/// model-level preprocessing and the per-instance block).
pub(crate) struct Mos1Temp {
    pub type_: f64, // +1 NMOS, -1 PMOS
    pub vt: f64,
    // Model copies the load needs.
    pub lat_diff: f64,
    /// `KF`/`AF`: flicker-noise coefficient and exponent.
    pub kf: f64,
    pub af: f64,
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    pub oxide_cap_factor: f64,
    pub gamma: f64,
    pub lambda: f64,
    pub mj: f64,
    pub mjsw: f64,
    // Temperature-adjusted values.
    pub t_transconductance: f64,
    pub t_phi: f64,
    pub t_vbi: f64,
    pub t_vto: f64,
    pub t_sat_cur: f64,
    pub t_sat_cur_dens: f64,
    pub t_bulk_pot: f64,
    pub t_dep_cap: f64,
    pub drain_vcrit: f64,
    pub source_vcrit: f64,
    // Zero-bias junction capacitances and their linearization past
    // `FC·PB` (`f2`/`f3`/`f4`, drain and source sides).
    pub cbd: f64,
    pub cbdsw: f64,
    pub cbs: f64,
    pub cbssw: f64,
    pub f2d: f64,
    pub f3d: f64,
    pub f4d: f64,
    pub f2s: f64,
    pub f3s: f64,
    pub f4s: f64,
    pub drain_conductance: f64,
    pub source_conductance: f64,
    pub notes: Vec<String>,
}

impl Mos1Temp {
    pub fn new(
        name: &str,
        m: &Mos1Model,
        g: &MosGeom,
        temp: f64,
        default_tnom: f64,
    ) -> Result<Self, String> {
        let mut notes = Vec::new();
        let typ = if m.nmos { 1.0 } else { -1.0 };
        let tnom = m.tnom.unwrap_or(default_tnom);

        let fact1 = tnom / REFTEMP;
        let vtnom = tnom * KOVERQ;
        let kt1 = BOLTZ * tnom;
        let egfet1 = egfet(tnom);
        let arg1 = -egfet1 / (kt1 + kt1) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);

        // Model parameter preprocessing: KP from U0·COX, and
        // PHI/GAMMA/VTO from the process description when NSUB is
        // given (all only when the parameter itself was not).
        let mut kp = m.kp;
        let mut phi = m.phi;
        let mut gamma = m.gamma;
        let mut vto = m.vto;
        let oxide_cap_factor = match m.tox {
            Some(tox) if tox != 0.0 => {
                let ocf = 3.9 * 8.854214871e-12 / tox;
                if kp.is_none() {
                    let u0 = m.u0.unwrap_or(600.0);
                    kp = Some(u0 * ocf * 1e-4 /* m²/cm² */);
                }
                if let Some(nsub) = m.nsub {
                    if nsub * 1e6 /* cm³/m³ */ > 1.45e16 {
                        if phi.is_none() {
                            phi = Some(f64::max(0.1, 2.0 * vtnom * f64::ln(nsub * 1e6 / 1.45e16)));
                        }
                        let phi_v = phi.unwrap_or(0.6);
                        let fermis = typ * 0.5 * phi_v;
                        let tpg = m.tpg.unwrap_or(1.0);
                        let mut wkfng = 3.2;
                        if tpg != 0.0 {
                            let fermig = typ * tpg * 0.5 * egfet1;
                            wkfng = 3.25 + 0.5 * egfet1 - fermig;
                        }
                        let wkfngs = wkfng - (3.25 + 0.5 * egfet1 + fermis);
                        if gamma.is_none() {
                            gamma = Some(
                                f64::sqrt(2.0 * 11.70 * 8.854214871e-12 * CHARGE * nsub * 1e6)
                                    / ocf,
                            );
                        }
                        if vto.is_none() {
                            let nss = m.nss.unwrap_or(0.0);
                            let vfb = wkfngs - nss * 1e4 /* cm²/m² */ * CHARGE / ocf;
                            vto = Some(vfb + typ * (gamma.unwrap_or(0.0) * phi_v.sqrt() + phi_v));
                        }
                    } else {
                        return Err(format!("{name}: Nsub < Ni"));
                    }
                }
                ocf
            }
            _ => 0.0,
        };
        let kp = kp.unwrap_or(2e-5);
        let phi = phi.unwrap_or(0.6);
        let gamma = gamma.unwrap_or(0.0);
        let vto = vto.unwrap_or(0.0);

        // Per-instance temperature block.
        let vt = temp * KOVERQ;
        let ratio = temp / tnom;
        let fact2 = temp / REFTEMP;
        let kt = temp * BOLTZ;
        let egfet_t = egfet(temp);
        let arg = -egfet_t / (kt + kt) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);

        if g.l - 2.0 * m.ld <= 0.0 {
            notes.push(format!("{name}: effective channel length less than zero"));
        }
        let ratio4 = ratio * ratio.sqrt();
        let t_transconductance = kp / ratio4;
        let phio = (phi - pbfact1) / fact1;
        let t_phi = fact2 * phio + pbfact;
        let t_vbi =
            vto - typ * (gamma * phi.sqrt()) + 0.5 * (egfet1 - egfet_t) + typ * 0.5 * (t_phi - phi);
        let t_vto = t_vbi + typ * gamma * t_phi.sqrt();
        let t_sat_cur = m.is * f64::exp(-egfet_t / vt + egfet1 / vtnom);
        let t_sat_cur_dens = m.js * f64::exp(-egfet_t / vt + egfet1 / vtnom);

        let pbo = (m.pb - pbfact1) / fact1;
        let gmaold = (m.pb - pbo) / pbo;
        let mut capfact = 1.0 / (1.0 + m.mj * (4e-4 * (tnom - REFTEMP) - gmaold));
        let mut t_cbd = m.cbd.unwrap_or(0.0) * capfact;
        let mut t_cbs = m.cbs.unwrap_or(0.0) * capfact;
        let mut t_cj = m.cj.unwrap_or(0.0) * capfact;
        capfact = 1.0 / (1.0 + m.mjsw * (4e-4 * (tnom - REFTEMP) - gmaold));
        let mut t_cjsw = m.cjsw.unwrap_or(0.0) * capfact;
        let t_bulk_pot = fact2 * pbo + pbfact;
        let gmanew = (t_bulk_pot - pbo) / pbo;
        capfact = 1.0 + m.mj * (4e-4 * (temp - REFTEMP) - gmanew);
        t_cbd *= capfact;
        t_cbs *= capfact;
        t_cj *= capfact;
        capfact = 1.0 + m.mjsw * (4e-4 * (temp - REFTEMP) - gmanew);
        t_cjsw *= capfact;
        let t_dep_cap = m.fc * t_bulk_pot;

        let (drain_vcrit, source_vcrit) = if t_sat_cur_dens == 0.0 || g.ad == 0.0 || g.as_ == 0.0 {
            let v = vt * f64::ln(vt / (ROOT2 * t_sat_cur));
            (v, v)
        } else {
            (
                vt * f64::ln(vt / (ROOT2 * t_sat_cur_dens * g.ad)),
                vt * f64::ln(vt / (ROOT2 * t_sat_cur_dens * g.as_)),
            )
        };

        // Zero-bias junction capacitances and the `f2`/`f3`/`f4`
        // coefficients that continue the charge past `FC·PB`.
        let czbd = if m.cbd.is_some() {
            t_cbd
        } else if m.cj.is_some() {
            t_cj * g.ad
        } else {
            0.0
        };
        let czbdsw = if m.cjsw.is_some() { t_cjsw * g.pd } else { 0.0 };
        let arg = 1.0 - m.fc;
        let sarg = f64::exp(-m.mj * f64::ln(arg));
        let sargsw = f64::exp(-m.mjsw * f64::ln(arg));
        let f2d = czbd * (1.0 - m.fc * (1.0 + m.mj)) * sarg / arg
            + czbdsw * (1.0 - m.fc * (1.0 + m.mjsw)) * sargsw / arg;
        let f3d =
            czbd * m.mj * sarg / arg / t_bulk_pot + czbdsw * m.mjsw * sargsw / arg / t_bulk_pot;
        let f4d = czbd * t_bulk_pot * (1.0 - arg * sarg) / (1.0 - m.mj)
            + czbdsw * t_bulk_pot * (1.0 - arg * sargsw) / (1.0 - m.mjsw)
            - f3d / 2.0 * (t_dep_cap * t_dep_cap)
            - t_dep_cap * f2d;
        let czbs = if m.cbs.is_some() {
            t_cbs
        } else if m.cj.is_some() {
            t_cj * g.as_
        } else {
            0.0
        };
        let czbssw = if m.cjsw.is_some() { t_cjsw * g.ps } else { 0.0 };
        let f2s = czbs * (1.0 - m.fc * (1.0 + m.mj)) * sarg / arg
            + czbssw * (1.0 - m.fc * (1.0 + m.mjsw)) * sargsw / arg;
        let f3s =
            czbs * m.mj * sarg / arg / t_bulk_pot + czbssw * m.mjsw * sargsw / arg / t_bulk_pot;
        let f4s = czbs * t_bulk_pot * (1.0 - arg * sarg) / (1.0 - m.mj)
            + czbssw * t_bulk_pot * (1.0 - arg * sargsw) / (1.0 - m.mjsw)
            - f3s / 2.0 * (t_dep_cap * t_dep_cap)
            - t_dep_cap * f2s;

        let series = |r: Option<f64>, squares: f64| match (r, m.rsh) {
            (Some(r), _) => {
                if r != 0.0 {
                    1.0 / r
                } else {
                    0.0
                }
            }
            (None, Some(rsh)) => {
                if rsh != 0.0 {
                    1.0 / (rsh * squares)
                } else {
                    0.0
                }
            }
            (None, None) => 0.0,
        };
        let drain_conductance = series(m.rd, g.nrd);
        let source_conductance = series(m.rs, g.nrs);

        Ok(Mos1Temp {
            type_: typ,
            vt,
            lat_diff: m.ld,
            kf: m.kf,
            af: m.af,
            cgso: m.cgso,
            cgdo: m.cgdo,
            cgbo: m.cgbo,
            oxide_cap_factor,
            gamma,
            lambda: m.lambda,
            mj: m.mj,
            mjsw: m.mjsw,
            t_transconductance,
            t_phi,
            t_vbi,
            t_vto,
            t_sat_cur,
            t_sat_cur_dens,
            t_bulk_pot,
            t_dep_cap,
            drain_vcrit,
            source_vcrit,
            cbd: czbd,
            cbdsw: czbdsw,
            cbs: czbs,
            cbssw: czbssw,
            f2d,
            f3d,
            f4d,
            f2s,
            f3s,
            f4s,
            drain_conductance,
            source_conductance,
            notes,
        })
    }
}

/// `DEVlimvds` (`devsup.c`): limit the per-iteration change of VDS.
pub(crate) fn limvds(vnew: f64, vold: f64) -> f64 {
    if vold >= 3.5 {
        if vnew > vold {
            f64::min(vnew, 3.0 * vold + 2.0)
        } else if vnew < 3.5 {
            f64::max(vnew, 2.0)
        } else {
            vnew
        }
    } else if vnew > vold {
        f64::min(vnew, 4.0)
    } else {
        f64::max(vnew, -0.5)
    }
}

/// `DEVfetlim` (`devsup.c`): limit the per-iteration change of FET
/// gate voltages around the threshold `vto`.
pub(crate) fn fetlim(vnew: f64, vold: f64, vto: f64) -> f64 {
    let vtsthi = f64::abs(2.0 * (vold - vto)) + 2.0;
    let vtstlo = vtsthi / 2.0 + 2.0;
    let vtox = vto + 3.5;
    let delv = vnew - vold;
    if vold >= vto {
        if vold >= vtox {
            if delv <= 0.0 {
                // Going off.
                if vnew >= vtox {
                    if -delv > vtstlo { vold - vtstlo } else { vnew }
                } else {
                    f64::max(vnew, vto + 2.0)
                }
            } else {
                // Staying on.
                if delv >= vtsthi { vold + vtsthi } else { vnew }
            }
        } else if delv <= 0.0 {
            f64::max(vnew, vto - 0.5)
        } else {
            f64::min(vnew, vto + 4.0)
        }
    } else if delv <= 0.0 {
        if -delv > vtsthi { vold - vtsthi } else { vnew }
    } else {
        let vtemp = vto + 0.5;
        if vnew <= vtemp {
            if delv > vtstlo { vold + vtstlo } else { vnew }
        } else {
            vtemp
        }
    }
}

/// `DEVqmeyer` (ngspice `devsup.c`): Meyer gate capacitances.
/// Returns half of the non-constant portion of (capgs, capgd,
/// capgb) at the current bias; the caller averages with the
/// previous timepoint and adds the constant overlap part.
///
/// This is the **ngspice** version, not the 3f5's: `vdsat` is
/// clamped to `MAGIC_VDS` (25 mV) and the weak-inversion region
/// splits the gate capacitance between source and drain with the
/// saturation smoothing (`corpus/KNOWN_DIFFS.md`).
pub(crate) fn qmeyer(
    vgs: f64,
    vgd: f64,
    _vgb: f64,
    von: f64,
    vdsat: f64,
    phi: f64,
    cox: f64,
) -> (f64, f64, f64) {
    const MAGIC_VDS: f64 = 0.025;
    let vgst = vgs - von;
    let vdsat = vdsat.max(MAGIC_VDS);
    if vgst <= -phi {
        (0.0, 0.0, cox / 2.0)
    } else if vgst <= -phi / 2.0 {
        (0.0, 0.0, -vgst * cox / (2.0 * phi))
    } else if vgst <= 0.0 {
        let capgb = -vgst * cox / (2.0 * phi);
        let mut capgs = vgst * cox / (1.5 * phi) + cox / 3.0;
        let vds = vgs - vgd;
        let capgd;
        if vds >= vdsat {
            capgd = 0.0;
        } else {
            let vddif = 2.0 * vdsat - vds;
            let vddif1 = vdsat - vds;
            let vddif2 = vddif * vddif;
            capgd = capgs * (1.0 - vdsat * vdsat / vddif2);
            capgs *= 1.0 - vddif1 * vddif1 / vddif2;
        }
        (capgs, capgd, capgb)
    } else {
        let vds = vgs - vgd;
        if vdsat <= vds {
            (cox / 3.0, 0.0, 0.0)
        } else {
            let vddif = 2.0 * vdsat - vds;
            let vddif1 = vdsat - vds;
            let vddif2 = vddif * vddif;
            (
                cox * (1.0 - vddif1 * vddif1 / vddif2) / 3.0,
                cox * (1.0 - vdsat * vdsat / vddif2) / 3.0,
                0.0,
            )
        }
    }
}

// State offsets (`mos1defs.h`).
const VBD: usize = 0;
const VBS: usize = 1;
const VGS: usize = 2;
const VDS: usize = 3;
const CAPGS: usize = 4;
const QGS: usize = 5;
const CQGS: usize = 6;
const CAPGD: usize = 7;
const QGD: usize = 8;
const CQGD: usize = 9;
const CAPGB: usize = 10;
const QGB: usize = 11;
const CQGB: usize = 12;
const QBD: usize = 13;
const CQBD: usize = 14;
const QBS: usize = 15;
const CQBS: usize = 16;
pub(crate) const MOS1_NUM_STATES: usize = 17;

// Slot order (matches `mos1set.c`'s TSTALLOC list).
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

pub(crate) struct Mos1I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Mos1Temp,
    geom: MosGeom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    ic_given: crate::devices::IcGiven,
    st: usize,
    s: [Slot; 22],
    // Values that persist between iterations (`MOS1instance`).
    mode: f64, // +1 normal, −1 source/drain swapped
    von: f64,
    vdsat: f64,
    gm: f64,
    gds: f64,
    gmbs: f64,
    gbd: f64,
    gbs: f64,
    capbd: f64,
    capbs: f64,
    cd: f64,
    cbs: f64,
    cbd: f64,
    /// `.disto` Taylor coefficients (`MOS1dSetup`).
    dist: MosDistoCoeffs,
}

impl Mos1I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],  // d g s b
        primes: [Var; 2], // d' s'
        t: Mos1Temp,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64), // (vds, vgs, vbs)
        st: usize,
    ) -> Self {
        Mos1I {
            drain: nodes[0],
            gate: nodes[1],
            source: nodes[2],
            bulk: nodes[3],
            d_p: primes[0],
            s_p: primes[1],
            t,
            geom,
            off,
            ic_vds: ic.0,
            ic_vgs: ic.1,
            ic_vbs: ic.2,
            ic_given: crate::devices::IcGiven::ALL,
            st,
            s: [TRASH; 22],
            mode: 1.0,
            von: 0.0,
            vdsat: 0.0,
            gm: 0.0,
            gds: 0.0,
            gmbs: 0.0,
            gbd: 0.0,
            gbs: 0.0,
            capbd: 0.0,
            capbs: 0.0,
            cd: 0.0,
            cbs: 0.0,
            cbd: 0.0,
            dist: MosDistoCoeffs::default(),
        }
    }
}

impl crate::devices::Device for Mos1I {
    fn par_load(&self) -> bool {
        true
    }

    /// `MOS1getic` (`mos1ic.c`): VBS, VDS, VGS not given
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

    /// `Mos1convTest`: predicted vs computed drain and bulk
    /// currents, evaluated at the fresh solution (`CKTrhs`).
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.t.type_;
        let st = self.st;
        let vbs = typ * (ctx.rhs[self.bulk] - ctx.rhs[self.s_p]);
        let vgs = typ * (ctx.rhs[self.gate] - ctx.rhs[self.s_p]);
        let vds = typ * (ctx.rhs[self.d_p] - ctx.rhs[self.s_p]);
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgdo = ctx.states.get(0, st + VGS) - ctx.states.get(0, st + VDS);
        let delvbs = vbs - ctx.states.get(0, st + VBS);
        let delvbd = vbd - ctx.states.get(0, st + VBD);
        let delvgs = vgs - ctx.states.get(0, st + VGS);
        let delvds = vds - ctx.states.get(0, st + VDS);
        let delvgd = vgd - vgdo;
        let cdhat = if self.mode >= 0.0 {
            self.cd - self.gbd * delvbd + self.gmbs * delvbs + self.gm * delvgs + self.gds * delvds
        } else {
            self.cd - (self.gbd - self.gmbs) * delvbd - self.gm * delvgd + self.gds * delvds
        };
        let cbhat = self.cbs + self.cbd + self.gbd * delvbd + self.gbs * delvbs;
        let tol = ctx.reltol * f64::max(cdhat.abs(), self.cd.abs()) + ctx.abstol;
        if (cdhat - self.cd).abs() >= tol {
            return false;
        }
        let cb = self.cbs + self.cbd;
        let tol = ctx.reltol * f64::max(cbhat.abs(), cb.abs()) + ctx.abstol;
        (cbhat - cb).abs() <= tol
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

    #[allow(clippy::too_many_lines)]
    fn load(&mut self, ctx: &mut LoadCtx) {
        let t = &self.t;
        let typ = t.type_;
        let vt = t.vt;
        let st = self.st;

        let eff_l = self.geom.l - 2.0 * t.lat_diff;
        let (drain_sat_cur, source_sat_cur) =
            if t.t_sat_cur_dens == 0.0 || self.geom.ad == 0.0 || self.geom.as_ == 0.0 {
                (t.t_sat_cur, t.t_sat_cur)
            } else {
                (
                    t.t_sat_cur_dens * self.geom.ad,
                    t.t_sat_cur_dens * self.geom.as_,
                )
            };
        let gate_source_overlap_cap = t.cgso * self.geom.w;
        let gate_drain_overlap_cap = t.cgdo * self.geom.w;
        let gate_bulk_overlap_cap = t.cgbo * eff_l;
        let beta = t.t_transconductance * self.geom.w / eff_l;
        let oxide_cap = t.oxide_cap_factor * eff_l * self.geom.w;

        // Bias resolution (the C's initialization ladder). `Check`
        // starts at 1; only `pnjlim` rewrites it.
        let mut check = true;
        let (mut vbs, mut vgs, mut vds);
        let predictor = matches!(ctx.mode, InitMode::Pred | InitMode::Tran);
        let general = matches!(ctx.mode, InitMode::Float | InitMode::Smsig)
            || (ctx.mode == InitMode::Fix && !self.off);
        if predictor || general {
            if predictor {
                // Predictor step: copy state1 → state0 and
                // extrapolate the junction voltages.
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

            // Limiting (`DEVfetlim`/`DEVlimvds`/`DEVpnjlim`).
            let von = typ * self.von;
            if ctx.states.get(0, st + VDS) >= 0.0 {
                vgs = fetlim(vgs, ctx.states.get(0, st + VGS), von);
                vds = vgs - vgd;
                vds = limvds(vds, ctx.states.get(0, st + VDS));
            } else {
                vgd = fetlim(vgd, vgdo, von);
                vds = vgs - vgd;
                // `CKTfixLimit` defaults to 0: always the symmetric
                // form.
                vds = -limvds(-vds, -ctx.states.get(0, st + VDS));
                vgs = vgd + vds;
            }
            if vds >= 0.0 {
                vbs = pnjlim(
                    vbs,
                    ctx.states.get(0, st + VBS),
                    vt,
                    t.source_vcrit,
                    &mut check,
                );
            } else {
                vbd = pnjlim(
                    vbd,
                    ctx.states.get(0, st + VBD),
                    vt,
                    t.drain_vcrit,
                    &mut check,
                );
                vbs = vbd + vds;
            }
        } else if ctx.mode == InitMode::Jct && !self.off {
            vds = typ * self.ic_vds;
            vgs = typ * self.ic_vgs;
            vbs = typ * self.ic_vbs;
            if vds == 0.0 && vgs == 0.0 && vbs == 0.0 && !(ctx.kind == AnKind::TranOp && ctx.uic) {
                vbs = -1.0;
                vgs = typ * t.t_vto;
                vds = 0.0;
            }
        } else {
            vbs = 0.0;
            vgs = 0.0;
            vds = 0.0;
        }

        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgb = vgs - vbs;

        // Bulk-source and bulk-drain diodes, in the ngspice form:
        // flat leakage below −3·vt, exponential (clamped) above,
        // with gmin folded into the current (`corpus/KNOWN_DIFFS.md`).
        let (mut gbs, mut cbs);
        if vbs <= -3.0 * vt {
            gbs = ctx.gmin;
            cbs = gbs * vbs - source_sat_cur;
        } else {
            let evbs = f64::exp(f64::min(MAX_EXP_ARG, vbs / vt));
            gbs = source_sat_cur * evbs / vt + ctx.gmin;
            cbs = source_sat_cur * (evbs - 1.0) + ctx.gmin * vbs;
        }
        let (mut gbd, mut cbd);
        if vbd <= -3.0 * vt {
            gbd = ctx.gmin;
            cbd = gbd * vbd - drain_sat_cur;
        } else {
            let evbd = f64::exp(f64::min(MAX_EXP_ARG, vbd / vt));
            gbd = drain_sat_cur * evbd / vt + ctx.gmin;
            cbd = drain_sat_cur * (evbd - 1.0) + ctx.gmin * vbd;
        }

        // Normal (vds ≥ 0) or inverse mode.
        self.mode = if vds >= 0.0 { 1.0 } else { -1.0 };
        let mode = self.mode;

        // Shichman-Hodges drain current and derivatives.
        let cdrain;
        let (gm, gds, gmbs);
        let (von, vdsat);
        {
            let vx = if mode == 1.0 { vbs } else { vbd };
            let sarg = if vx <= 0.0 {
                (t.t_phi - vx).sqrt()
            } else {
                let s = t.t_phi.sqrt();
                f64::max(0.0, s - vx / (s + s))
            };
            von = t.t_vbi * typ + t.gamma * sarg;
            let vgst = (if mode == 1.0 { vgs } else { vgd }) - von;
            vdsat = f64::max(vgst, 0.0);
            let arg = if sarg <= 0.0 {
                0.0
            } else {
                t.gamma / (sarg + sarg)
            };
            if vgst <= 0.0 {
                // Cutoff region.
                cdrain = 0.0;
                gm = 0.0;
                gds = 0.0;
                gmbs = 0.0;
            } else {
                let betap = beta * (1.0 + t.lambda * (vds * mode));
                if vgst <= vds * mode {
                    // Saturation region.
                    cdrain = betap * vgst * vgst * 0.5;
                    gm = betap * vgst;
                    gds = t.lambda * beta * vgst * vgst * 0.5;
                    gmbs = gm * arg;
                } else {
                    // Linear region.
                    cdrain = betap * (vds * mode) * (vgst - 0.5 * (vds * mode));
                    gm = betap * (vds * mode);
                    gds = betap * (vgst - vds * mode)
                        + t.lambda * beta * (vds * mode) * (vgst - 0.5 * (vds * mode));
                    gmbs = gm * arg;
                }
            }
        }
        self.von = typ * von;
        self.vdsat = typ * vdsat;
        self.gm = gm;
        self.gds = gds;
        self.gmbs = gmbs;

        // Equivalent drain current source.
        let mut cd = mode * cdrain - cbd;

        let smsig = ctx.mode == InitMode::Smsig;
        if ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp || smsig {
            // Bulk junction depletion charge and capacitance, with
            // the sqrt fast path for grading coefficient 0.5 and the
            // linearized continuation past `FC·PB` (CAPZEROBYPASS
            // kept: zero-capacitance junctions skip the math).
            let dep = |v: f64, cz: f64, czsw: f64, f2: f64, f3: f64, f4: f64| {
                if cz != 0.0 || czsw != 0.0 {
                    if v < t.t_dep_cap {
                        let arg = 1.0 - v / t.t_bulk_pot;
                        let (sarg, sargsw);
                        if t.mj == t.mjsw {
                            if t.mj == 0.5 {
                                sarg = 1.0 / arg.sqrt();
                                sargsw = sarg;
                            } else {
                                sarg = f64::exp(-t.mj * f64::ln(arg));
                                sargsw = sarg;
                            }
                        } else {
                            sarg = if t.mj == 0.5 {
                                1.0 / arg.sqrt()
                            } else {
                                f64::exp(-t.mj * f64::ln(arg))
                            };
                            sargsw = if t.mjsw == 0.5 {
                                1.0 / arg.sqrt()
                            } else {
                                f64::exp(-t.mjsw * f64::ln(arg))
                            };
                        }
                        let q = t.t_bulk_pot
                            * (cz * (1.0 - arg * sarg) / (1.0 - t.mj)
                                + czsw * (1.0 - arg * sargsw) / (1.0 - t.mjsw));
                        (q, cz * sarg + czsw * sargsw)
                    } else {
                        (f4 + v * (f2 + v * f3 / 2.0), f2 + v * f3)
                    }
                } else {
                    (0.0, 0.0)
                }
            };
            let (qbs, capbs) = dep(vbs, t.cbs, t.cbssw, t.f2s, t.f3s, t.f4s);
            ctx.states.set(0, st + QBS, qbs);
            self.capbs = capbs;
            let (qbd, capbd) = dep(vbd, t.cbd, t.cbdsw, t.f2d, t.f3d, t.f4d);
            ctx.states.set(0, st + QBD, qbd);
            self.capbd = capbd;

            if ctx.kind == AnKind::Tran {
                // Integrate the depletion capacitors.
                let (geq, _) = ctx.integrate(self.capbd, st + QBD);
                gbd += geq;
                cbd += ctx.states.get(0, st + CQBD);
                cd -= ctx.states.get(0, st + CQBD);
                let (geq, _) = ctx.integrate(self.capbs, st + QBS);
                gbs += geq;
                cbs += ctx.states.get(0, st + CQBS);
            }
        }
        self.gbs = gbs;
        self.gbd = gbd;

        // Convergence: limiting counts as non-convergence (NEWCONV).
        if (!self.off || !matches!(ctx.mode, InitMode::Fix | InitMode::Smsig)) && check {
            ctx.noncon = true;
        }

        // Save things away for next time.
        ctx.states.set(0, st + VBS, vbs);
        ctx.states.set(0, st + VBD, vbd);
        ctx.states.set(0, st + VGS, vgs);
        ctx.states.set(0, st + VDS, vds);

        // Meyer's capacitor model.
        let (mut capgs, mut capgd, mut capgb) = (0.0, 0.0, 0.0);
        if ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp || smsig {
            // Half of the non-constant portion at this bias; the
            // other half comes from the previous timepoint.
            let (half_gs, half_gd, half_gb) = if mode > 0.0 {
                qmeyer(vgs, vgd, vgb, von, vdsat, t.t_phi, oxide_cap)
            } else {
                let (o1, o2, o3) = qmeyer(vgd, vgs, vgb, von, vdsat, t.t_phi, oxide_cap);
                (o2, o1, o3)
            };
            ctx.states.set(0, st + CAPGS, half_gs);
            ctx.states.set(0, st + CAPGD, half_gd);
            ctx.states.set(0, st + CAPGB, half_gb);
            let vgs1 = ctx.states.get(1, st + VGS);
            let vgd1 = vgs1 - ctx.states.get(1, st + VDS);
            let vgb1 = vgs1 - ctx.states.get(1, st + VBS);
            if ctx.kind == AnKind::TranOp || smsig {
                capgs = 2.0 * ctx.states.get(0, st + CAPGS) + gate_source_overlap_cap;
                capgd = 2.0 * ctx.states.get(0, st + CAPGD) + gate_drain_overlap_cap;
                capgb = 2.0 * ctx.states.get(0, st + CAPGB) + gate_bulk_overlap_cap;
            } else {
                capgs = ctx.states.get(0, st + CAPGS)
                    + ctx.states.get(1, st + CAPGS)
                    + gate_source_overlap_cap;
                capgd = ctx.states.get(0, st + CAPGD)
                    + ctx.states.get(1, st + CAPGD)
                    + gate_drain_overlap_cap;
                capgb = ctx.states.get(0, st + CAPGB)
                    + ctx.states.get(1, st + CAPGB)
                    + gate_bulk_overlap_cap;
            }

            if smsig {
                // The AC pass reads the Meyer halves from state0 and
                // the rest from the instance; nothing to stamp.
                return;
            }

            if predictor {
                // Extrapolate the gate charges like the voltages.
                let xfact = ctx.pred_xfact;
                for o in [QGS, QGD, QGB] {
                    let v = (1.0 + xfact) * ctx.states.get(1, st + o)
                        - xfact * ctx.states.get(2, st + o);
                    ctx.states.set(0, st + o, v);
                }
            } else if ctx.kind == AnKind::Tran {
                let q = (vgs - vgs1) * capgs + ctx.states.get(1, st + QGS);
                ctx.states.set(0, st + QGS, q);
                let q = (vgd - vgd1) * capgd + ctx.states.get(1, st + QGD);
                ctx.states.set(0, st + QGD, q);
                let q = (vgb - vgb1) * capgb + ctx.states.get(1, st + QGB);
                ctx.states.set(0, st + QGB, q);
            } else {
                // Transient operating point only.
                ctx.states.set(0, st + QGS, vgs * capgs);
                ctx.states.set(0, st + QGD, vgd * capgd);
                ctx.states.set(0, st + QGB, vgb * capgb);
            }
        }

        // Meyer companion models (zero at the first timepoint and in
        // every DC mode).
        let (mut gcgs, mut ceqgs, mut gcgd, mut ceqgd, mut gcgb, mut ceqgb) =
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        if ctx.mode != InitMode::Tran && ctx.kind == AnKind::Tran {
            if capgs == 0.0 {
                ctx.states.set(0, st + CQGS, 0.0);
            }
            if capgd == 0.0 {
                ctx.states.set(0, st + CQGD, 0.0);
            }
            if capgb == 0.0 {
                ctx.states.set(0, st + CQGB, 0.0);
            }
            let (g, ceq) = ctx.integrate(capgs, st + QGS);
            gcgs = g;
            ceqgs = ceq - gcgs * vgs + ctx.ag[0] * ctx.states.get(0, st + QGS);
            let (g, ceq) = ctx.integrate(capgd, st + QGD);
            gcgd = g;
            ceqgd = ceq - gcgd * vgd + ctx.ag[0] * ctx.states.get(0, st + QGD);
            let (g, ceq) = ctx.integrate(capgb, st + QGB);
            gcgb = g;
            ceqgb = ceq - gcgb * vgb + ctx.ag[0] * ctx.states.get(0, st + QGB);
        }

        // Load the current vector.
        // ngspice folds gmin into the junction current, so the
        // Norton companion uses the full conductance (the 3f5
        // subtracted gmin here).
        let ceqbs = typ * (cbs - gbs * vbs);
        let ceqbd = typ * (cbd - gbd * vbd);
        let (xnrm, xrev, cdreq);
        if mode >= 0.0 {
            xnrm = 1.0;
            xrev = 0.0;
            cdreq = typ * (cdrain - gds * vds - gm * vgs - gmbs * vbs);
        } else {
            xnrm = 0.0;
            xrev = 1.0;
            cdreq = -typ * (cdrain - gds * (-vds) - gm * vgd - gmbs * vbd);
        }
        // Saved for the convergence test (`mos1conv.c`).
        self.cd = cd;
        self.cbs = cbs;
        self.cbd = cbd;
        ctx.add_rhs(self.gate, -(typ * (ceqgs + ceqgb + ceqgd)));
        ctx.add_rhs(self.bulk, -(ceqbs + ceqbd - typ * ceqgb));
        ctx.add_rhs(self.d_p, ceqbd - cdreq + typ * ceqgd);
        ctx.add_rhs(self.s_p, cdreq + ceqbs + typ * ceqgs);

        // Load the Y matrix.
        let gdpr = t.drain_conductance;
        let gspr = t.source_conductance;
        ctx.add(self.s[D_D], gdpr);
        ctx.add(self.s[G_G], gcgd + gcgs + gcgb);
        ctx.add(self.s[S_S], gspr);
        ctx.add(self.s[B_B], gbd + gbs + gcgb);
        ctx.add(self.s[DP_DP], gdpr + gds + gbd + xrev * (gm + gmbs) + gcgd);
        ctx.add(self.s[SP_SP], gspr + gds + gbs + xnrm * (gm + gmbs) + gcgs);
        ctx.add(self.s[D_DP], -gdpr);
        ctx.add(self.s[G_B], -gcgb);
        ctx.add(self.s[G_DP], -gcgd);
        ctx.add(self.s[G_SP], -gcgs);
        ctx.add(self.s[S_SP], -gspr);
        ctx.add(self.s[B_G], -gcgb);
        ctx.add(self.s[B_DP], -gbd);
        ctx.add(self.s[B_SP], -gbs);
        ctx.add(self.s[DP_D], -gdpr);
        ctx.add(self.s[DP_G], (xnrm - xrev) * gm - gcgd);
        ctx.add(self.s[DP_B], -gbd + (xnrm - xrev) * gmbs);
        ctx.add(self.s[DP_SP], -gds - xnrm * (gm + gmbs));
        ctx.add(self.s[SP_G], -(xnrm - xrev) * gm - gcgs);
        ctx.add(self.s[SP_S], -gspr);
        ctx.add(self.s[SP_B], -gbs - (xnrm - xrev) * gmbs);
        ctx.add(self.s[SP_DP], -gds - xrev * (gm + gmbs));
    }

    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QGS, t, time_step);
        crate::integ::terr(self.st + QGD, t, time_step);
        crate::integ::terr(self.st + QGB, t, time_step);
    }

    /// `MOS1noise` (ngspice-47, default `NLEV`): see
    /// [`mos_noise`].
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(MOS_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let n = MosNoise {
            drain: self.drain,
            source: self.source,
            d_p: self.d_p,
            s_p: self.s_p,
            gdpr: self.t.drain_conductance,
            gspr: self.t.source_conductance,
            gm: self.gm,
            kf: self.t.kf,
            af: self.t.af,
            w: self.geom.w,
            eff_l: self.geom.l - 2.0 * self.t.lat_diff,
            cox: if self.t.oxide_cap_factor == 0.0 {
                // `mos1noi.c`: a level-1 model without TOX assumes
                // tox = 1e-7 m for the flicker term.
                3.9 * 8.854214871e-12 / 1e-7
            } else {
                self.t.oxide_cap_factor
            },
        };
        mos_noise(ctx, &n)
    }

    /// `MOS1acLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let t = &self.t;
        let st = self.st;
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        let eff_l = self.geom.l - 2.0 * t.lat_diff;
        let gate_source_overlap_cap = t.cgso * self.geom.w;
        let gate_drain_overlap_cap = t.cgdo * self.geom.w;
        let gate_bulk_overlap_cap = t.cgbo * eff_l;
        let capgs = 2.0 * ctx.states.get(0, st + CAPGS) + gate_source_overlap_cap;
        let capgd = 2.0 * ctx.states.get(0, st + CAPGD) + gate_drain_overlap_cap;
        let capgb = 2.0 * ctx.states.get(0, st + CAPGB) + gate_bulk_overlap_cap;
        let xgs = capgs * ctx.omega;
        let xgd = capgd * ctx.omega;
        let xgb = capgb * ctx.omega;
        let xbd = self.capbd * ctx.omega;
        let xbs = self.capbs * ctx.omega;
        let (gm, gds, gmbs) = (self.gm, self.gds, self.gmbs);
        let (gbd, gbs) = (self.gbd, self.gbs);
        let gdpr = t.drain_conductance;
        let gspr = t.source_conductance;

        ctx.add_b(self.s[G_G], xgd + xgs + xgb);
        ctx.add_b(self.s[B_B], xgb + xbd + xbs);
        ctx.add_b(self.s[DP_DP], xgd + xbd);
        ctx.add_b(self.s[SP_SP], xgs + xbs);
        ctx.add_b(self.s[G_B], -xgb);
        ctx.add_b(self.s[G_DP], -xgd);
        ctx.add_b(self.s[G_SP], -xgs);
        ctx.add_b(self.s[B_G], -xgb);
        ctx.add_b(self.s[B_DP], -xbd);
        ctx.add_b(self.s[B_SP], -xbs);
        ctx.add_b(self.s[DP_G], -xgd);
        ctx.add_b(self.s[DP_B], -xbd);
        ctx.add_b(self.s[SP_G], -xgs);
        ctx.add_b(self.s[SP_B], -xbs);
        ctx.add_g(self.s[D_D], gdpr);
        ctx.add_g(self.s[S_S], gspr);
        ctx.add_g(self.s[B_B], gbd + gbs);
        ctx.add_g(self.s[DP_DP], gdpr + gds + gbd + xrev * (gm + gmbs));
        ctx.add_g(self.s[SP_SP], gspr + gds + gbs + xnrm * (gm + gmbs));
        ctx.add_g(self.s[D_DP], -gdpr);
        ctx.add_g(self.s[S_SP], -gspr);
        ctx.add_g(self.s[B_DP], -gbd);
        ctx.add_g(self.s[B_SP], -gbs);
        ctx.add_g(self.s[DP_D], -gdpr);
        ctx.add_g(self.s[DP_G], (xnrm - xrev) * gm);
        ctx.add_g(self.s[DP_B], -gbd + (xnrm - xrev) * gmbs);
        ctx.add_g(self.s[DP_SP], -gds - xnrm * (gm + gmbs));
        ctx.add_g(self.s[SP_G], -(xnrm - xrev) * gm);
        ctx.add_g(self.s[SP_S], -gspr);
        ctx.add_g(self.s[SP_B], -gbs - (xnrm - xrev) * gmbs);
        ctx.add_g(self.s[SP_DP], -gds - xrev * (gm + gmbs));
    }

    /// `MOS1dSetup` (`dev/mos1/mos1dset.c`): the Shichman–Hodges
    /// drain current's derivatives up to third order written out by
    /// region (cutoff, saturation, linear), with the body effect
    /// through `von(vbs)`; then the shared diode/capacitance/type
    /// bookkeeping (`mosdisto.rs`).
    #[allow(clippy::too_many_lines)]
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        let t = &self.t;
        let g = &self.geom;
        let typ = t.type_;
        let vt = t.vt;
        let sol = ctx.sol;
        let gmin = ctx.gmin;
        let eff_l = g.l - 2.0 * t.lat_diff;
        let (drain_sat_cur, source_sat_cur) =
            if t.t_sat_cur_dens == 0.0 || g.ad == 0.0 || g.as_ == 0.0 {
                (t.t_sat_cur, t.t_sat_cur)
            } else {
                (t.t_sat_cur_dens * g.ad, t.t_sat_cur_dens * g.as_)
            };
        let beta = t.t_transconductance * g.w / eff_l;
        let oxide_cap = t.oxide_cap_factor * eff_l * g.w;

        let vbs = typ * (sol[self.bulk] - sol[self.s_p]);
        let vgs = typ * (sol[self.gate] - sol[self.s_p]);
        let vds = typ * (sol[self.d_p] - sol[self.s_p]);
        let vbd = vbs - vds;
        let vgd = vgs - vds;

        let gbs = bulk_diode(vbs, source_sat_cur, vt, gmin, typ);
        let gbd = bulk_diode(vbd, drain_sat_cur, vt, gmin, typ);
        let mode = if vds >= 0.0 { 1.0 } else { -1.0 };

        let lvbs = if mode == 1.0 { vbs } else { vbd };
        let (sarg, dvondvbs, d2vondvbs2, d3vondvbs3);
        if lvbs <= 0.0 {
            sarg = f64::sqrt(t.t_phi - lvbs);
            if -t.gamma != 0.0 {
                let d1 = -t.gamma * 0.5 / sarg;
                let d2 = -d1 * 0.5 / (sarg * sarg);
                dvondvbs = d1;
                d2vondvbs2 = d2;
                d3vondvbs3 = 1.5 * d2 / (sarg * sarg);
            } else {
                (dvondvbs, d2vondvbs2, d3vondvbs3) = (0.0, 0.0, 0.0);
            }
        } else {
            let s0 = f64::sqrt(t.t_phi);
            let d1 = if t.gamma != 0.0 {
                -t.gamma / (s0 + s0)
            } else {
                0.0
            };
            (d2vondvbs2, d3vondvbs3) = (0.0, 0.0);
            sarg = f64::max(0.0, s0 - lvbs / (s0 + s0));
            dvondvbs = if sarg <= 0.0 { 0.0 } else { d1 };
        }
        let von = t.t_vbi * typ + t.gamma * sarg;
        let vgst = (if mode == 1.0 { vgs } else { vgd }) - von;
        let vdsat = f64::max(vgst, 0.0);
        let lambda = t.lambda;
        let mut dd = DrainDerivs::default();
        if vgst > 0.0 {
            let betap = beta * (1.0 + lambda * (vds * mode));
            if vgst <= vds * mode {
                // Saturation: cdrain = betap·vgst²/2.
                let gm = betap * vgst;
                let gmds = vgst * lambda * beta;
                let gmb = -betap * dvondvbs;
                dd = DrainDerivs {
                    gm2: betap,
                    gds2: 0.0,
                    gb2: -(gm * d2vondvbs2 - betap * dvondvbs * dvondvbs),
                    gmds,
                    gbds: -gmds * dvondvbs,
                    gmb,
                    gm3: 0.0,
                    gb3: -(gmb * d2vondvbs2 + gm * d3vondvbs3
                        - betap * 2.0 * dvondvbs * d2vondvbs2),
                    gds3: 0.0,
                    gm2ds: beta * lambda,
                    gm2b: 0.0,
                    gmb2: -betap * d2vondvbs2,
                    gb2ds: -(gmds * d2vondvbs2 - dvondvbs * dvondvbs * beta * lambda),
                    gmds2: 0.0,
                    gbds2: 0.0,
                    gmbds: -beta * lambda * dvondvbs,
                };
            } else {
                // Linear: cdrain = betap·vds·(vgst − vds/2).
                let gm = betap * (vds * mode);
                let gmds = beta * lambda * vds * mode + betap;
                let gmds2 = 2.0 * lambda * beta;
                dd = DrainDerivs {
                    gm2: 0.0,
                    gb2: -(gm * d2vondvbs2),
                    gds2: 2.0 * beta * lambda * (vgst - vds * mode) - betap,
                    gmds,
                    gbds: -gmds * dvondvbs,
                    gmb: 0.0,
                    gm3: 0.0,
                    gb3: -gm * d3vondvbs3,
                    gds3: -beta * lambda * 3.,
                    gm2ds: 0.0,
                    gm2b: 0.0,
                    gmb2: 0.0,
                    gmds2,
                    gb2ds: -(gmds * d2vondvbs2),
                    gbds2: -gmds2 * dvondvbs,
                    gmbds: 0.0,
                };
            }
        }

        let junctions = MosJunctions {
            cbs: t.cbs,
            cbssw: t.cbssw,
            f3s: t.f3s,
            cbd: t.cbd,
            cbdsw: t.cbdsw,
            f3d: t.f3d,
            dep_cap: t.t_dep_cap,
            bulk_pot: t.t_bulk_pot,
            mj: t.mj,
            mjsw: t.mjsw,
        };
        self.dist = setup_tail(
            typ,
            mode,
            (vbs, vbd, vds),
            (vgst, vdsat),
            t.t_phi,
            oxide_cap,
            &junctions,
            &dd,
            gbs,
            gbd,
        );
        Ok(())
    }

    /// `MOS1disto` (`dev/mos1/mos1dist.c`).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        mos_disto_load(&self.dist, (self.gate, self.bulk, self.d_p, self.s_p), ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetlim_matches_the_c() {
        // On, above vtox, big positive step: clamp to vold+vtsthi.
        let vto = 1.0;
        let vold = 5.0;
        let vtsthi = (2.0f64 * (vold - vto)).abs() + 2.0;
        assert_eq!(fetlim(30.0, vold, vto), vold + vtsthi);
        // Off, small step passes through.
        assert_eq!(fetlim(0.2, 0.0, 1.0), 0.2);
        // Off, trying to jump far above threshold: clamp to vto+0.5.
        assert_eq!(fetlim(10.0, 0.0, 1.0), 1.5);
    }

    #[test]
    fn limvds_matches_the_c() {
        assert_eq!(limvds(10.0, 1.0), 4.0);
        assert_eq!(limvds(-3.0, 1.0), -0.5);
        assert_eq!(limvds(20.0, 5.0), 17.0); // 3*5+2
        assert_eq!(limvds(1.0, 5.0), 2.0);
    }

    #[test]
    fn qmeyer_regions() {
        let cox = 1e-12;
        let phi = 0.6;
        // Accumulation: all gate-bulk.
        let (gs, gd, gb) = qmeyer(-2.0, -2.0, 0.0, 0.0, 0.0, phi, cox);
        assert_eq!((gs, gd), (0.0, 0.0));
        assert_eq!(gb, cox / 2.0);
        // Saturation: capgs = cox/3.
        let (gs, gd, gb) = qmeyer(2.0, -3.0, 0.0, 0.0, 2.0, phi, cox);
        assert_eq!(gs, cox / 3.0);
        assert_eq!((gd, gb), (0.0, 0.0));
        // Linear region: both nonzero, sum tends to 2/3 cox.
        let (gs, gd, _) = qmeyer(2.0, 1.9, 0.0, 0.0, 2.0, phi, cox);
        assert!(gs > 0.0 && gd > 0.0);
        assert!((gs + gd) < 2.0 * cox / 3.0 + 1e-15);
    }

    #[test]
    fn temp_defaults_match_mos1temp() {
        // mosmem's model: vto/phi/kp/gamma/lambda given, no tox.
        let mut m = Mos1Model::new(true);
        m.vto = Some(0.5);
        m.phi = Some(0.7);
        m.kp = Some(1.0e-6);
        m.gamma = Some(1.83);
        m.lambda = 0.115;
        m.cgso = 1e-6;
        m.cgdo = 1e-6;
        m.cbd = Some(50e-12);
        m.cbs = Some(50e-12);
        let g = MosGeom {
            l: 5e-6,
            w: 250e-6,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        };
        let t = Mos1Temp::new("m1", &m, &g, REFTEMP, REFTEMP).unwrap();
        // At TNOM the adjusted values collapse to the given ones.
        assert!((t.t_phi - 0.7).abs() < 1e-12);
        assert!((t.t_vto - 0.5).abs() < 1e-12);
        assert!((t.t_transconductance - 1.0e-6).abs() < 1e-18);
        assert_eq!(t.drain_conductance, 0.0);
        assert!((t.cbd - 50e-12).abs() < 1e-24);
        // No prime nodes for this model.
        assert!(!m.needs_prime(1.0));
    }
}
