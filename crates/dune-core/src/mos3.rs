//! MOSFET level 3 — the semi-empirical short-channel model from the
//! 3f5 (`dev/mos3/mos3load.c`, `mos3temp.c`, `mos3set.c`,
//! `mos3acld.c`, `mos3conv.c`, `mos3trun.c`). Threshold with the
//! static-feedback term (`ETA`), the junction-depth short-channel
//! factor (`XJ`, Dang's trapezoid fit `coeff0/1/2`) and the
//! narrow-width term (`DELTA`); gate-field mobility degradation
//! (`THETA`); velocity saturation through `VMAX` with the
//! `KAPPA`-scaled lateral-field channel-length modulation (or the
//! plain depletion form without `VMAX`), the punch-through clamp at
//! half the channel, and the `NFS`/`xn` weak-inversion exponential.
//! The scaffolding around the current evaluation (bias ladder,
//! limiting, bulk diodes, depletion and Meyer charges, companion
//! models, stamps) is MOS1's, which is why this file mirrors
//! `mos1.rs`/`mos2.rs` there.
//!
//! Where ngspice-47 (the oracle) departs from the 3f5 and the golden
//! shows it, Dune follows ngspice: the alansfixes drain-current
//! derivatives and smoothed no-`VMAX` channel-length modulation in
//! `moseq3`, the cubic reverse tail of the bulk diodes
//! (`bulk_diode`), `ni(TNOM)` in the PHI-from-NSUB preprocessing,
//! and the `f3/f4` junction coefficients (`junction_coeffs`).

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::dderivs::Dderivs;
use crate::diode::pnjlim;
use crate::disto::{DistoCtx, DistoSetupCtx};
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{MosGeom, fetlim, limvds, qmeyer};
use crate::mosdisto::{
    DrainDerivs, MosDistoCoeffs, MosJunctions, bulk_diode as disto_bulk_diode, mos_disto_load,
    setup_tail,
};

/// Epsilon of silicon (`mos3temp.c`: `EPSSIL`).
const EPSSIL: f64 = 11.7 * 8.854214871e-12;

/// `.model … nmos(level=3 …)/pmos(level=3 …)` parameters, 3f5 names
/// and defaults (`mos3.c` keyword table; defaults from `mos3set.c`
/// and `mos3temp.c`). `Option` marks parameters whose given-ness
/// changes the preprocessing, as in [`crate::Mos1Model`].
#[derive(Clone, Debug)]
pub struct Mos3Model {
    /// `NMOS` (+1) or `PMOS` (−1).
    pub nmos: bool,
    /// `VTO`/`VT0`, default 0, or computed from `NSUB`/`NSS`/`TPG`.
    pub vto: Option<f64>,
    /// `KP`, default `U0·COX`.
    pub kp: Option<f64>,
    /// `GAMMA`, default 0, or computed from `NSUB`.
    pub gamma: Option<f64>,
    /// `PHI`, default 0.6, or from `NSUB`.
    pub phi: Option<f64>,
    /// `RD`/`RS`, default 0.
    pub rd: Option<f64>,
    pub rs: Option<f64>,
    /// `CBD`/`CBS`, default 0.
    pub cbd: Option<f64>,
    pub cbs: Option<f64>,
    /// `IS`, default 1e-14.
    pub is: f64,
    /// `PB`, default 0.8.
    pub pb: f64,
    /// `CGSO`/`CGDO`/`CGBO`, default 0.
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    /// `RSH`, default 0.
    pub rsh: Option<f64>,
    /// `CJ`, per m², default 0.
    pub cj: Option<f64>,
    /// `MJ`, default 0.5.
    pub mj: f64,
    /// `CJSW`, per meter, default 0.
    pub cjsw: Option<f64>,
    /// `MJSW`, default **0.33** (as MOS2).
    pub mjsw: f64,
    /// `JS`, per m², default 0.
    pub js: f64,
    /// `TOX`, default 1e-7 m (MOS3 always has an oxide).
    pub tox: Option<f64>,
    /// `LD`, default 0.
    pub ld: f64,
    /// `U0`/`UO`, cm²/V·s, default 600.
    pub u0: Option<f64>,
    /// `FC`, default 0.5.
    pub fc: f64,
    /// `NSUB`, 1/cm³.
    pub nsub: Option<f64>,
    /// `TPG`, default 1 when `NSUB` is given.
    pub tpg: Option<f64>,
    /// `NSS`, 1/cm², default 0.
    pub nss: Option<f64>,
    /// `VMAX`: maximum carrier drift velocity, m/s, default 0 (off).
    pub vmax: f64,
    /// `XJ`: metallurgical junction depth, default 0.
    pub xj: f64,
    /// `NFS`: fast surface state density, 1/cm², default 0.
    pub nfs: f64,
    /// `ETA`: static feedback (Vds dependence of threshold),
    /// default 0.
    pub eta: f64,
    /// `DELTA`: width effect on threshold, default 0.
    pub delta: f64,
    /// `THETA`: mobility modulation by the gate field, 1/V,
    /// default 0.
    pub theta: f64,
    /// `KAPPA`: saturation field factor, default 0.2.
    pub kappa: f64,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `TNOM` in kelvin.
    pub tnom: Option<f64>,
}

impl Mos3Model {
    pub fn new(nmos: bool) -> Self {
        Mos3Model {
            nmos,
            vto: None,
            kp: None,
            gamma: None,
            phi: None,
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
            mjsw: 0.33,
            js: 0.0,
            tox: None,
            ld: 0.0,
            u0: None,
            fc: 0.5,
            nsub: None,
            tpg: None,
            nss: None,
            vmax: 0.0,
            xj: 0.0,
            nfs: 0.0,
            eta: 0.0,
            delta: 0.0,
            theta: 0.0,
            kappa: 0.2,
            kf: 0.0,
            af: 1.0,
            tnom: None,
        }
    }

    /// Same prime-node rule as MOS1 (`mos3set.c`).
    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rd.unwrap_or(0.0) != 0.0 || (self.rsh.unwrap_or(0.0) != 0.0 && squares != 0.0)
    }
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

/// Model- and temperature-derived values (`MOS3temp`).
#[derive(Debug)]
pub(crate) struct Mos3Temp {
    pub type_: f64,
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
    pub mj: f64,
    pub mjsw: f64,
    pub xj: f64,
    pub nfs: f64,
    pub vmax: f64,
    pub eta: f64,
    pub theta: f64,
    pub kappa: f64,
    /// `MOS3alpha`: `2·EPSSIL/(q·NSUB)` (0 without `NSUB`).
    pub alpha: f64,
    /// `MOS3coeffDepLayWidth`: `sqrt(alpha)`.
    pub coeff_dep_lay_width: f64,
    /// `MOS3narrowFactor`: `DELTA·π/2·EPSSIL/COX`.
    pub narrow_factor: f64,
    // Temperature-adjusted values.
    pub t_transconductance: f64,
    /// `MOS3tSurfMob`: `U0/ratio^1.5`, cm²/V·s.
    pub t_surf_mob: f64,
    pub t_phi: f64,
    pub t_vbi: f64,
    pub t_vto: f64,
    pub t_sat_cur: f64,
    pub t_sat_cur_dens: f64,
    pub t_bulk_pot: f64,
    pub t_dep_cap: f64,
    pub drain_vcrit: f64,
    pub source_vcrit: f64,
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
}

impl Mos3Temp {
    pub fn new(
        name: &str,
        m: &Mos3Model,
        g: &MosGeom,
        temp: f64,
        default_tnom: f64,
    ) -> Result<Self, String> {
        let typ = if m.nmos { 1.0 } else { -1.0 };
        let tnom = m.tnom.unwrap_or(default_tnom);

        let fact1 = tnom / REFTEMP;
        let vtnom = tnom * KOVERQ;
        let kt1 = BOLTZ * tnom;
        let egfet1 = egfet(tnom);
        let arg1 = -egfet1 / (kt1 + kt1) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact1 = -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1);
        // ngspice-47 (MOS3 only): intrinsic density at TNOM instead of
        // the 3f5's fixed 1.45e16 m⁻³, both in the `Nsub < Ni` check
        // and in PHI from NSUB (≈0.6 mV at 27 °C, visible in weak
        // inversion; `corpus/KNOWN_DIFFS.md`).
        let mut nifact = (tnom / 300.0) * (tnom / 300.0).sqrt();
        nifact *= f64::exp(0.5 * egfet1 * ((1.0 / 300.0) - (1.0 / tnom)) / KOVERQ);
        let ni_temp = 1.45e16 * nifact;

        // Model preprocessing (TOX defaults to 1e-7 in `mos3set.c`,
        // so the oxide always exists).
        let tox = m.tox.unwrap_or(1e-7);
        let oxide_cap_factor = 3.9 * 8.854214871e-12 / tox;
        let u0 = m.u0.unwrap_or(600.0);
        let kp = m.kp.unwrap_or(u0 * oxide_cap_factor * 1e-4 /* m²/cm² */);
        let mut phi = m.phi;
        let mut gamma = m.gamma;
        let mut vto = m.vto;
        let mut alpha = 0.0;
        let mut coeff_dep_lay_width = 0.0;
        if let Some(nsub) = m.nsub {
            if nsub * 1e6 /* cm³/m³ */ > ni_temp {
                if phi.is_none() {
                    phi = Some(f64::max(0.1, 2.0 * vtnom * f64::ln(nsub * 1e6 / ni_temp)));
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
                    gamma = Some(f64::sqrt(2.0 * EPSSIL * CHARGE * nsub * 1e6) / oxide_cap_factor);
                }
                if vto.is_none() {
                    let nss = m.nss.unwrap_or(0.0);
                    let vfb = wkfngs - nss * 1e4 /* cm²/m² */ * CHARGE / oxide_cap_factor;
                    vto = Some(vfb + typ * (gamma.unwrap_or(0.0) * phi_v.sqrt() + phi_v));
                }
                alpha = (EPSSIL + EPSSIL) / (CHARGE * nsub * 1e6);
                coeff_dep_lay_width = alpha.sqrt();
            } else {
                return Err(format!("{name}: Nsub < Ni"));
            }
        }
        let phi = phi.unwrap_or(0.6);
        // ngspice-47 rejects a non-positive PHI up front (the 3f5
        // would take sqrt of it in the load).
        if phi <= 0.0 {
            return Err(format!("{name}: Phi is not positive."));
        }
        let gamma = gamma.unwrap_or(0.0);
        let vto = vto.unwrap_or(0.0);
        let narrow_factor = m.delta * 0.5 * std::f64::consts::PI * EPSSIL / oxide_cap_factor;

        // Per-instance temperature block.
        let vt = temp * KOVERQ;
        let ratio = temp / tnom;
        let fact2 = temp / REFTEMP;
        let kt = temp * BOLTZ;
        let egfet_t = egfet(temp);
        let arg = -egfet_t / (kt + kt) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);

        // MOS3 makes this fatal (E_BADPARM), unlike MOS1/MOS2's note.
        if g.l - 2.0 * m.ld <= 0.0 {
            return Err(format!("{name}: effective channel length less than zero"));
        }
        let ratio4 = ratio * ratio.sqrt();
        let t_transconductance = kp / ratio4;
        let t_surf_mob = u0 / ratio4;
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

        // The 3f5 keys vcrit off the model IS/JS; ngspice-47 (and
        // Dune's MOS1/MOS2) off the temperature-adjusted ones. Only
        // the limiting sees it.
        let (drain_vcrit, source_vcrit) = if m.js == 0.0 || g.ad == 0.0 || g.as_ == 0.0 {
            let v = vt * f64::ln(vt / (ROOT2 * t_sat_cur));
            (v, v)
        } else {
            (
                vt * f64::ln(vt / (ROOT2 * t_sat_cur_dens * g.ad)),
                vt * f64::ln(vt / (ROOT2 * t_sat_cur_dens * g.as_)),
            )
        };

        let czbd = if m.cbd.is_some() {
            t_cbd
        } else if m.cj.is_some() {
            t_cj * g.ad
        } else {
            0.0
        };
        let czbdsw = if m.cjsw.is_some() { t_cjsw * g.pd } else { 0.0 };
        let czbs = if m.cbs.is_some() {
            t_cbs
        } else if m.cj.is_some() {
            t_cj * g.as_
        } else {
            0.0
        };
        let czbssw = if m.cjsw.is_some() { t_cjsw * g.ps } else { 0.0 };
        let arg = 1.0 - m.fc;
        let sarg = f64::exp(-m.mj * f64::ln(arg));
        let sargsw = f64::exp(-m.mjsw * f64::ln(arg));
        let (f2d, f3d, f4d) =
            junction_coeffs(m, czbd, czbdsw, arg, sarg, sargsw, t_bulk_pot, t_dep_cap);
        let (f2s, f3s, f4s) =
            junction_coeffs(m, czbs, czbssw, arg, sarg, sargsw, t_bulk_pot, t_dep_cap);

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

        Ok(Mos3Temp {
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
            mj: m.mj,
            mjsw: m.mjsw,
            xj: m.xj,
            nfs: m.nfs,
            vmax: m.vmax,
            eta: m.eta,
            theta: m.theta,
            kappa: m.kappa,
            alpha,
            coeff_dep_lay_width,
            narrow_factor,
            t_transconductance,
            t_surf_mob,
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
            drain_conductance: series(m.rd, g.nrd),
            source_conductance: series(m.rs, g.nrs),
        })
    }
}

/// Forward-bias continuation coefficients `f2/f3/f4` of one bulk
/// junction, in ngspice-47's `mos3temp.c` form: the 3f5 divides
/// `f3` by the model `PB` instead of the temperature-adjusted one,
/// writes `f4` with the model `PB`, and (source side only) puts
/// `tBulkPot` where `tDepCap` belongs, so its `qbs` jumps at
/// `vbs = FC·PB`. ngspice uses the MOS1/MOS2 forms throughout.
#[allow(clippy::too_many_arguments)]
fn junction_coeffs(
    m: &Mos3Model,
    cz: f64,
    czsw: f64,
    arg: f64,
    sarg: f64,
    sargsw: f64,
    t_bulk_pot: f64,
    t_dep_cap: f64,
) -> (f64, f64, f64) {
    let f2 = cz * (1.0 - m.fc * (1.0 + m.mj)) * sarg / arg
        + czsw * (1.0 - m.fc * (1.0 + m.mjsw)) * sargsw / arg;
    let f3 = cz * m.mj * sarg / arg / t_bulk_pot + czsw * m.mjsw * sargsw / arg / t_bulk_pot;
    let f4 = cz * t_bulk_pot * (1.0 - arg * sarg) / (1.0 - m.mj)
        + czsw * t_bulk_pot * (1.0 - arg * sargsw) / (1.0 - m.mjsw)
        - f3 / 2.0 * (t_dep_cap * t_dep_cap)
        - t_dep_cap * f2;
    (f2, f3, f4)
}

// State offsets (`mos3defs.h` — identical to MOS1's).
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
pub(crate) const MOS3_NUM_STATES: usize = 17;

// Slot order (matches `mos3set.c`'s TSTALLOC list — same as MOS1).
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

/// One bulk junction in ngspice-47's `mos3load.c` form: returns
/// `(g, i)` with gmin inside both.
fn bulk_diode(v: f64, vt: f64, sat_cur: f64, gmin: f64) -> (f64, f64) {
    const MAX_EXP_ARG: f64 = 709.0;
    if v <= -3.0 * vt {
        let arg = 3.0 * vt / (v * crate::consts::E);
        let arg = arg * arg * arg;
        (
            sat_cur * 3.0 * arg / v + gmin,
            -sat_cur * (1.0 + arg) + gmin * v,
        )
    } else {
        let ev = f64::exp(f64::min(MAX_EXP_ARG, v / vt));
        (sat_cur * ev / vt + gmin, sat_cur * (ev - 1.0) + gmin * v)
    }
}

/// Dang's fit of the junction-sidewall depletion width
/// (`mos3load.c`: `coeff0`/`coeff1`/`coeff2`).
const COEFF0: f64 = 0.0631353;
const COEFF1: f64 = 0.8013292;
const COEFF2: f64 = -0.01110777;

/// The drain current of [`Mos3I::eval`] and its derivatives, in the
/// mode-normalized space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Mos3Eval {
    pub cdrain: f64,
    pub gm: f64,
    pub gds: f64,
    pub gmbs: f64,
    pub von: f64,
    pub vdsat: f64,
}

pub(crate) struct Mos3I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Mos3Temp,
    geom: MosGeom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    st: usize,
    s: [Slot; 22],
    // Values that persist between iterations (`MOS3instance`).
    mode: f64,
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
    /// `.disto` Taylor coefficients (`MOS3dSetup`).
    dist: MosDistoCoeffs,
}

impl Mos3I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],  // d g s b
        primes: [Var; 2], // d' s'
        t: Mos3Temp,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64), // (vds, vgs, vbs)
        st: usize,
    ) -> Self {
        Mos3I {
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

/// The semi-empirical current evaluation (the `moseq3` block of
/// `mos3load.c`), ported literally with the C's intermediate names —
/// in ngspice-47's form from the normalized drain current on, since
/// the oracle shows it (`corpus/KNOWN_DIFFS.md`): the alansfixes
/// corrected the 3f5's saturation derivatives (`gds` from the
/// linear-region formula, `dfddvd`'s `−fd2·onvdsc`, the missing
/// `dfgdvb·cd1` in `gmbs`, the CLM bookkeeping) and, without
/// `VMAX`, smoothed the channel-length modulation through vdsat
/// (`vds−vdsat+vdsat/8` above, `sqrt(KAPPA·alpha·vdsat/8)·r⁴`
/// below), which changes the drain current itself by up to a few
/// percent around vdsat.
/// `lvbs`/`lvgs`/`lvds` are the mode-normalized biases
/// (`mode==1 ? vbs : vbd`, …, `mode·vds`); `beta` is
/// `tTransconductance·W/Leff` and `oxide_cap` `COX·Leff·W`. Every
/// path writes all of `gm`/`gds`/`gmbs`, so (unlike MOS2) nothing
/// is carried over from the previous iteration.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn moseq3(
    t: &Mos3Temp,
    w: f64,
    lvbs: f64,
    lvgs: f64,
    lvds: f64,
    beta: f64,
    oxide_cap: f64,
    eff_l: f64,
    vt: f64,
) -> Mos3Eval {
    let typ = t.type_;
    let mut beta = beta;
    let mut vdsat = 0.0;
    let oneoverxl = 1.0 / eff_l;
    let eta = t.eta * 8.15e-22 / (t.oxide_cap_factor * eff_l * eff_l * eff_l);

    // Square root term.
    let (phibs, sqphbs, dsqdvb);
    if lvbs <= 0.0 {
        phibs = t.t_phi - lvbs;
        sqphbs = phibs.sqrt();
        dsqdvb = -0.5 / sqphbs;
    } else {
        let sqphis = t.t_phi.sqrt();
        let sqphs3 = t.t_phi * sqphis;
        sqphbs = sqphis / (1.0 + lvbs / (t.t_phi + t.t_phi));
        phibs = sqphbs * sqphbs;
        dsqdvb = -phibs / (sqphs3 + sqphs3);
    }

    // Short channel effect factor.
    let (fshort, dfsdvb);
    if t.xj != 0.0 && t.coeff_dep_lay_width != 0.0 {
        let wps = t.coeff_dep_lay_width * sqphbs;
        let oneoverxj = 1.0 / t.xj;
        let xjonxl = t.xj * oneoverxl;
        let djonxj = t.lat_diff * oneoverxj;
        let wponxj = wps * oneoverxj;
        let wconxj = COEFF0 + COEFF1 * wponxj + COEFF2 * wponxj * wponxj;
        let arga = wconxj + djonxj;
        let argc = wponxj / (1.0 + wponxj);
        let argb = (1.0 - argc * argc).sqrt();
        fshort = 1.0 - xjonxl * (arga * argb - djonxj);
        let dwpdvb = t.coeff_dep_lay_width * dsqdvb;
        let dadvb = (COEFF1 + COEFF2 * (wponxj + wponxj)) * dwpdvb * oneoverxj;
        let dbdvb = -argc * argc * (1.0 - argc) * dwpdvb / (argb * wps);
        dfsdvb = -xjonxl * (dadvb * argb + arga * dbdvb);
    } else {
        fshort = 1.0;
        dfsdvb = 0.0;
    }

    // Body effect.
    let gammas = t.gamma * fshort;
    let fbodys = 0.5 * gammas / (sqphbs + sqphbs);
    let fbody = fbodys + t.narrow_factor / w;
    let onfbdy = 1.0 / (1.0 + fbody);
    let dfbdvb = -fbodys * dsqdvb / sqphbs + fbodys * dfsdvb / fshort;
    let qbonco = gammas * sqphbs + t.narrow_factor * phibs / w;
    let dqbdvb = gammas * dsqdvb + t.gamma * dfsdvb * sqphbs - t.narrow_factor / w;

    // Static feedback effect.
    let vbix = t.t_vbi * typ - eta * lvds;

    // Threshold voltage.
    let vth = vbix + qbonco;
    let dvtdvd = -eta;
    let dvtdvb = dqbdvb;

    // Joint weak inversion and strong inversion.
    let mut von = vth;
    let (mut xn, mut dxndvb, mut dvodvd, mut dvodvb) = (0.0, 0.0, 0.0, 0.0);
    if t.nfs != 0.0 {
        let csonco = CHARGE * t.nfs * 1e4 /* cm²/m² */ * eff_l * w / oxide_cap;
        let cdonco = qbonco / (phibs + phibs);
        xn = 1.0 + csonco + cdonco;
        von = vth + vt * xn;
        dxndvb = dqbdvb / (phibs + phibs) - qbonco * dsqdvb / (phibs * sqphbs);
        dvodvd = dvtdvd;
        dvodvb = dvtdvb + vt * dxndvb;
    } else if lvgs <= von {
        // Cutoff region.
        return Mos3Eval {
            cdrain: 0.0,
            gm: 0.0,
            gds: 0.0,
            gmbs: 0.0,
            von,
            vdsat,
        };
    }

    // Device is on.
    let vgsx = f64::max(lvgs, von);

    // Mobility modulation by gate voltage.
    let onfg = 1.0 + t.theta * (vgsx - vth);
    let fgate = 1.0 / onfg;
    let us = t.t_surf_mob * 1e-4 /* m²/cm² */ * fgate;
    let dfgdvg = -t.theta * fgate * fgate;
    let dfgdvd = -dfgdvg * dvtdvd;
    let dfgdvb = -dfgdvg * dvtdvb;

    // Saturation voltage.
    vdsat = (vgsx - vth) * onfbdy;
    let (dvsdvg, dvsdvd, dvsdvb);
    let mut onvdsc = 0.0;
    if t.vmax <= 0.0 {
        dvsdvg = onfbdy;
        dvsdvd = -dvsdvg * dvtdvd;
        dvsdvb = -dvsdvg * dvtdvb - vdsat * dfbdvb * onfbdy;
    } else {
        let vdsc = eff_l * t.vmax / us;
        onvdsc = 1.0 / vdsc;
        let arga = (vgsx - vth) * onfbdy;
        let argb = (arga * arga + vdsc * vdsc).sqrt();
        vdsat = arga + vdsc - argb;
        let dvsdga = (1.0 - arga / argb) * onfbdy;
        dvsdvg = dvsdga - (1.0 - vdsc / argb) * vdsc * dfgdvg * onfg;
        dvsdvd = -dvsdvg * dvtdvd;
        dvsdvb = -dvsdvg * dvtdvb - arga * dvsdga * dfbdvb;
    }

    // Current factors in linear region.
    let vdsx = f64::min(lvds, vdsat);
    if vdsx == 0.0 {
        // line900: special case of vds = 0.
        beta *= fgate;
        let mut gds = beta * (vgsx - vth);
        if t.nfs != 0.0 && lvgs < von {
            gds *= f64::exp((lvgs - von) / (vt * xn));
        }
        return Mos3Eval {
            cdrain: 0.0,
            gm: 0.0,
            gds,
            gmbs: 0.0,
            von,
            vdsat,
        };
    }
    let cdo = vgsx - vth - 0.5 * (1.0 + fbody) * vdsx;
    let dcodvb = -dvtdvb - 0.5 * dfbdvb * vdsx;

    // Normalized drain current. From here on Dune follows ngspice-47
    // (alansfixes), not the 3f5 — see the function docs.
    let saturated = lvds > vdsat;
    let cdnorm = cdo * vdsx;
    let mut gm = vdsx;
    // In saturation vdsx = vdsat, so the only direct vds dependence
    // left is the static feedback through vth (the 3f5 used the
    // linear-region form here too, a derivative bug).
    let mut gds = if saturated {
        -dvtdvd * vdsx
    } else {
        vgsx - vth - (1.0 + fbody + dvtdvd) * vdsx
    };
    let mut gmbs = dcodvb * vdsx;

    // Drain current without velocity saturation effect.
    let cd1 = beta * cdnorm;
    beta *= fgate;
    let mut cdrain = beta * cdnorm;
    gm = beta * gm + dfgdvg * cd1;
    gds = beta * gds + dfgdvd * cd1;
    // (the 3f5 dropped the `dfgdvb·cd1` term)
    gmbs = beta * gmbs + dfgdvb * cd1;

    // Velocity saturation factor.
    let (mut fdrain, mut dfddvg, mut dfddvd, mut dfddvb) = (0.0, 0.0, 0.0, 0.0);
    if t.vmax > 0.0 {
        fdrain = 1.0 / (1.0 + vdsx * onvdsc);
        let fd2 = fdrain * fdrain;
        let arga = fd2 * vdsx * onvdsc * onfg;
        dfddvg = -dfgdvg * arga;
        dfddvd = if saturated {
            -dfgdvd * arga
        } else {
            -dfgdvd * arga - fd2 * onvdsc
        };
        dfddvb = -dfgdvb * arga;
        // Drain current.
        gm = fdrain * gm + dfddvg * cdrain;
        gds = fdrain * gds + dfddvd * cdrain;
        gmbs = fdrain * gmbs + dfddvb * cdrain;
        cdrain *= fdrain;
        #[allow(unused_assignments)] // kept from the C
        {
            beta *= fdrain;
        }
    }

    // Channel length modulation. `gds0` stays 0 on the paths that
    // skip it (the weak-inversion block reads it in saturation).
    let mut gds0 = 0.0;
    'line700: {
        let (mut delxl, mut dldvd, mut ddldvg, mut ddldvd, mut ddldvb);
        if !saturated {
            // ngspice: without VMAX the channel already shortens
            // below vdsat, `delxl = sqrt(KAPPA·alpha·vdsat/8)·r⁴`
            // with r = vds/vdsat, which joins the smoothed line510
            // form with a continuous slope at vds = vdsat.
            if t.vmax > 0.0 || t.alpha == 0.0 {
                break 'line700;
            }
            let mut arga = lvds / vdsat;
            delxl = (t.kappa * t.alpha * vdsat / 8.0).sqrt();
            dldvd = 4.0 * delxl * arga * arga * arga / vdsat;
            arga *= arga;
            arga *= arga;
            delxl *= arga;
            ddldvg = 0.0;
            ddldvd = -dldvd;
            ddldvb = 0.0;
        } else if t.vmax <= 0.0 {
            // line510, ngspice's smoothed form: `vds−vdsat` becomes
            // `vds−vdsat+vdsat/8` (`.options badmos3` would restore
            // the 3f5's; accepted and ignored by Dune's options).
            let arg = lvds - vdsat + vdsat / 8.0;
            delxl = (t.kappa * t.alpha * arg).sqrt();
            dldvd = 0.5 * delxl / arg;
            ddldvg = 0.0;
            ddldvd = -dldvd;
            ddldvb = 0.0;
        } else {
            if t.alpha == 0.0 {
                break 'line700;
            }
            let cdsat = cdrain;
            let mut gdsat = cdsat * (1.0 - fdrain) * onvdsc;
            gdsat = f64::max(1.0e-12, gdsat);
            let gdoncd = gdsat / cdsat;
            let gdonfd = gdsat / (1.0 - fdrain);
            let gdonfg = gdsat * onfg;
            let dgdvg = gdoncd * gm - gdonfd * dfddvg + gdonfg * dfgdvg;
            let dgdvd = gdoncd * gds - gdonfd * dfddvd + gdonfg * dfgdvd;
            let dgdvb = gdoncd * gmbs - gdonfd * dfddvb + gdonfg * dfgdvb;

            let emax = t.kappa * cdsat * oneoverxl / gdsat;
            let emoncd = emax / cdsat;
            let emongd = emax / gdsat;
            let demdvg = emoncd * gm - emongd * dgdvg;
            let demdvd = emoncd * gds - emongd * dgdvd;
            let demdvb = emoncd * gmbs - emongd * dgdvb;

            let arga = 0.5 * emax * t.alpha;
            let argc = t.kappa * t.alpha;
            let argb = (arga * arga + argc * (lvds - vdsat)).sqrt();
            delxl = argb - arga;
            let dldem;
            if argb != 0.0 {
                dldvd = argc / (argb + argb);
                dldem = 0.5 * (arga / argb - 1.0) * t.alpha;
            } else {
                dldvd = 0.0;
                dldem = 0.0;
            }
            ddldvg = dldem * demdvg;
            ddldvd = dldem * demdvd - dldvd;
            ddldvb = dldem * demdvb;
        }
        // line520: punch through approximation.
        if delxl > 0.5 * eff_l {
            delxl = eff_l - (eff_l * eff_l / (4.0 * delxl));
            let arga = 4.0 * (eff_l - delxl) * (eff_l - delxl) / (eff_l * eff_l);
            ddldvg *= arga;
            ddldvd *= arga;
            ddldvb *= arga;
            dldvd *= arga;
        }
        // Saturation region (and ngspice's sub-vdsat CLM).
        let dlonxl = delxl * oneoverxl;
        let xlfact = 1.0 / (1.0 - dlonxl);
        cdrain *= xlfact;
        let diddl = cdrain / (eff_l - delxl);
        gm = gm * xlfact + diddl * ddldvg;
        gmbs = gmbs * xlfact + diddl * ddldvb;
        gds0 = diddl * ddldvd;
        gm += gds0 * dvsdvg;
        gmbs += gds0 * dvsdvb;
        gds = gds * xlfact + diddl * dldvd + gds0 * dvsdvd;
    }

    // line700: finish strong inversion case.
    if lvgs < von {
        // Weak inversion.
        let onxn = 1.0 / xn;
        let ondvt = onxn / vt;
        let wfact = f64::exp((lvgs - von) * ondvt);
        cdrain *= wfact;
        let gms = gm * wfact;
        let gmw = cdrain * ondvt;
        gm = gmw;
        if saturated {
            gm += gds0 * dvsdvg * wfact;
        }
        gds = gds * wfact + (gms - gmw) * dvodvd;
        gmbs = gmbs * wfact + (gms - gmw) * dvodvb - gmw * (lvgs - von) * onxn * dxndvb;
    }

    Mos3Eval {
        cdrain,
        gm,
        gds,
        gmbs,
        von,
        vdsat,
    }
}

impl Mos3I {
    /// `MOS3dSetup` (`dev/mos3/mos3dset.c`): the 3f5 semi-empirical
    /// drain current re-evaluated with the `Dderivs` algebra (p = vgs,
    /// q = vbs, r = vds, mode-normalized) — the setup's own equations,
    /// which ngspice-47 carries unchanged apart from the effective
    /// width (equal to `W` here, Dune has no `WD`/`XW`), not the
    /// smoothed alansfixes current the load uses. Kept as the C has
    /// it: the narrow-width term of `qbonco` *multiplies* by `W` in
    /// the derivative chain while the value divides, and `fbody`
    /// takes the derivatives of `fbodys` alone.
    #[allow(unused_assignments, clippy::too_many_lines)]
    fn disto_coeffs(&self, sol: &[f64], gmin: f64) -> MosDistoCoeffs {
        let t = &self.t;
        let g = &self.geom;
        let typ = t.type_;
        let vt = t.vt;
        let eff_l = g.l - 2.0 * t.lat_diff;
        let (drain_sat_cur, source_sat_cur) =
            if t.t_sat_cur_dens == 0.0 || g.ad == 0.0 || g.as_ == 0.0 {
                (t.t_sat_cur, t.t_sat_cur)
            } else {
                (t.t_sat_cur_dens * g.ad, t.t_sat_cur_dens * g.as_)
            };
        let mut beta = t.t_transconductance * g.w / eff_l;
        let oxide_cap = t.oxide_cap_factor * eff_l * g.w;

        let vbs = typ * (sol[self.bulk] - sol[self.s_p]);
        let vgs = typ * (sol[self.gate] - sol[self.s_p]);
        let vds = typ * (sol[self.d_p] - sol[self.s_p]);
        let vbd = vbs - vds;
        let vgd = vgs - vds;

        let gbs = disto_bulk_diode(vbs, source_sat_cur, vt, gmin, typ);
        let gbd = disto_bulk_diode(vbd, drain_sat_cur, vt, gmin, typ);
        let mode = if vds >= 0.0 { 1.0 } else { -1.0 };

        let (lvgs, lvds, lvbs) = if mode == 1.0 {
            (vgs, vds, vbs)
        } else {
            (vgd, -vds, vbd)
        };

        let d_p = Dderivs::var_p();
        let d_q = Dderivs::var_q();
        let d_r = Dderivs::var_r();
        let d_zero = Dderivs::default();
        let mut d_cdrain = d_zero;
        let mut vdsat = 0.0;
        let mut von;

        'eval: {
            let mut d_vdsat;
            let oneoverxl = 1.0 / eff_l;
            let eta = t.eta * 8.15e-22 / (t.oxide_cap_factor * eff_l * eff_l * eff_l);

            // Square-root term.
            let (phibs, sqphbs, d_phibs, d_sqphbs);
            if lvbs <= 0.0 {
                phibs = t.t_phi - lvbs;
                let mut d = Dderivs { value: lvbs, ..d_q }.times(-1.0);
                d.value += t.t_phi;
                d_phibs = d;
                sqphbs = f64::sqrt(phibs);
                d_sqphbs = d_phibs.sqrt();
            } else {
                let sqphis = f64::sqrt(t.t_phi);
                sqphbs = sqphis / (1.0 + lvbs / (t.t_phi + t.t_phi));
                let mut d = Dderivs { value: lvbs, ..d_q }.times(1.0 / (t.t_phi + t.t_phi));
                d.value += 1.0;
                d_sqphbs = d.inv().times(sqphis);
                phibs = sqphbs * sqphbs;
                d_phibs = d_sqphbs.mult(&d_sqphbs);
            }

            // Short-channel effect factor.
            const COEFF0: f64 = 0.0631353e0;
            const COEFF1: f64 = 0.8013292e0;
            const COEFF2: f64 = -0.01110777e0;
            let (fshort, d_fshort);
            if t.xj != 0.0 && t.coeff_dep_lay_width != 0.0 {
                let wps = t.coeff_dep_lay_width * sqphbs;
                let d_wps = d_sqphbs.times(t.coeff_dep_lay_width);
                let oneoverxj = 1.0 / t.xj;
                let xjonxl = t.xj * oneoverxl;
                let djonxj = t.lat_diff * oneoverxj;
                let wponxj = wps * oneoverxj;
                let d_wponxj = d_wps.times(oneoverxj);
                let wconxj = COEFF0 + COEFF1 * wponxj + COEFF2 * wponxj * wponxj;
                let mut d_wconxj = d_wponxj.times(COEFF2);
                d_wconxj.value += COEFF1;
                let mut d_wconxj = d_wconxj.mult(&d_wponxj);
                d_wconxj.value += COEFF0;
                let arga = wconxj + djonxj;
                let mut d_arga = d_wconxj;
                d_arga.value += djonxj;
                let argc = wponxj / (1.0 + wponxj);
                let mut d_argc = d_wponxj;
                d_argc.value += 1.0;
                let d_argc = d_argc.inv().mult(&d_wponxj);
                let argb = f64::sqrt(1.0 - argc * argc);
                let mut d_argb = d_argc.mult(&d_argc).times(-1.0);
                d_argb.value += 1.0;
                let d_argb = d_argb.sqrt();
                fshort = 1.0 - xjonxl * (arga * argb - djonxj);
                let mut d = d_arga.mult(&d_argb);
                d.value -= djonxj;
                let mut d = d.times(-xjonxl);
                d.value += 1.0;
                d_fshort = d;
            } else {
                fshort = 1.0;
                d_fshort = Dderivs::constant(1.0);
            }

            // Body effect.
            let gammas = t.gamma * fshort;
            let d_gammas = d_fshort.times(t.gamma);
            let fbodys = 0.5 * gammas / (sqphbs + sqphbs);
            let d_fbodys = d_gammas.div(&d_sqphbs).times(0.25);
            let fbody = fbodys + t.narrow_factor / g.w;
            let mut d_fbody = d_fbodys;
            d_fbody.value += fbody - fbodys;
            let onfbdy = 1.0 / (1.0 + fbody);
            let mut d_onfbdy = d_fbody;
            d_onfbdy.value += 1.0;
            let d_onfbdy = d_onfbdy.inv();
            let qbonco = gammas * sqphbs + t.narrow_factor * phibs / g.w;
            let d_dummy = d_phibs.times(t.narrow_factor * g.w);
            let d_qbonco = d_gammas.mult(&d_sqphbs).plus(&d_dummy);

            // Static feedback, threshold voltage.
            let vbix = t.t_vbi * typ - eta * lvds;
            let d_vbix = Dderivs {
                value: vbix,
                d1_r: -eta,
                ..d_r
            };
            let vth = vbix + qbonco;
            let d_vth = d_vbix.plus(&d_qbonco);
            von = vth;
            let mut d_von = d_vth;
            let mut xn = 0.0;
            let mut d_xn = d_zero;
            if t.nfs != 0.0 {
                let csonco = CHARGE * t.nfs * 1e4 * eff_l * g.w / oxide_cap;
                let cdonco = 0.5 * qbonco / phibs;
                let d_cdonco = d_qbonco.div(&d_phibs).times(0.5);
                xn = 1.0 + csonco + cdonco;
                d_xn = d_cdonco;
                d_xn.value += 1.0 + csonco;
                von = vth + vt * xn;
                d_von = d_xn.times(vt).plus(&d_vth);
            } else if lvgs <= von {
                // Cutoff.
                d_cdrain = d_zero;
                break 'eval;
            }

            // Device is on.
            let vgsx = f64::max(lvgs, von);
            let d_vgsx = if lvgs >= von {
                Dderivs { value: lvgs, ..d_p }
            } else {
                d_von
            };
            // Mobility modulation by gate voltage.
            let onfg = 1.0 + t.theta * (vgsx - vth);
            let mut d_onfg = d_vth.times(-1.0).plus(&d_vgsx).times(t.theta);
            d_onfg.value += 1.0;
            let fgate = 1.0 / onfg;
            let d_fgate = d_onfg.inv();
            let us = t.t_surf_mob * 1e-4 * fgate;
            let d_us = d_fgate.times(t.t_surf_mob * 1e-4);
            // Saturation voltage.
            vdsat = (vgsx - vth) * onfbdy;
            d_vdsat = d_vth.times(-1.0).plus(&d_vgsx).mult(&d_onfbdy);
            let (mut onvdsc, mut d_onvdsc) = (0.0, d_zero);
            if t.vmax > 0.0 {
                let vdsc = eff_l * t.vmax / us;
                let d_vdsc = d_us.inv().times(eff_l * t.vmax);
                onvdsc = 1.0 / vdsc;
                d_onvdsc = d_vdsc.inv();
                let arga = (vgsx - vth) * onfbdy;
                let d_arga = d_vdsat;
                let argb = f64::sqrt(arga * arga + vdsc * vdsc);
                let d_argb = d_vdsc.mult(&d_vdsc).plus(&d_arga.mult(&d_arga)).sqrt();
                vdsat = arga + vdsc - argb;
                d_vdsat = d_argb.times(-1.0).plus(&d_vdsc).plus(&d_arga);
            }

            // Current factors in the linear region.
            let vdsx = f64::min(lvds, vdsat);
            let d_vdsx = if lvds < vdsat {
                Dderivs { value: lvds, ..d_r }
            } else {
                d_vdsat
            };
            if vdsx == 0.0 {
                // `line900`: the special case vds = 0 (`Beta` is
                // updated before the derivative is scaled by it, as
                // in the main path).
                beta *= fgate;
                let d_beta = d_fgate.times(beta);
                d_cdrain = d_zero;
                let mut d_gds = d_vth.times(-1.0).plus(&d_vgsx).mult(&d_beta);
                if t.nfs != 0.0 && lvgs < von {
                    let mut d_dummy = d_von.times(-1.0);
                    d_dummy.value += lvgs;
                    d_dummy.d1_p += 1.0;
                    let d_dummy = d_dummy.div(&d_xn).times(1.0 / vt).exp();
                    d_gds = d_gds.mult(&d_dummy);
                }
                d_cdrain.d1_r = d_gds.value;
                d_cdrain.d2_r2 = d_gds.d1_r;
                d_cdrain.d3_r3 = d_gds.d2_r2;
                break 'eval;
            }
            let cdo = vgsx - vth - 0.5 * (1.0 + fbody) * vdsx;
            let mut d_cdo = d_fbody;
            d_cdo.value += 1.0;
            let d_cdo = d_cdo
                .mult(&d_vdsx)
                .times(0.5)
                .plus(&d_vth)
                .times(-1.0)
                .plus(&d_vgsx);
            // Normalized drain current, then without velocity
            // saturation. The C updates `Beta = Beta·fgate` *before*
            // `TimesDeriv(&d_Beta, &d_fgate, Beta)`, so the
            // derivative chain carries `fgate²` — one extra factor
            // of `1/(1+THETA·(vgsx−vth))` in every drain-current
            // coefficient (kept; measured against the oracle).
            let cdnorm = cdo * vdsx;
            let d_cdnorm = d_cdo.mult(&d_vdsx);
            beta *= fgate;
            let d_beta = d_fgate.times(beta);
            let mut cdrain = beta * cdnorm;
            d_cdrain = d_beta.mult(&d_cdnorm);
            let (mut fdrain, mut d_fdrain) = (0.0, d_zero);
            if t.vmax != 0.0 {
                fdrain = 1.0 / (1.0 + vdsx * onvdsc);
                let mut f = d_vdsx.mult(&d_onvdsc);
                f.value += 1.0;
                d_fdrain = f.inv();
                cdrain *= fdrain;
                d_cdrain = d_cdrain.mult(&d_fdrain);
            }

            // Channel-length modulation (`line510`/`line520`).
            if lvds > vdsat && !(t.vmax != 0.0 && t.alpha == 0.0) {
                let (mut delxl, mut d_delxl);
                if t.vmax == 0.0 {
                    delxl = f64::sqrt(t.kappa * (lvds - vdsat) * t.alpha);
                    let mut d = d_vdsat.times(-1.0);
                    d.value += lvds;
                    d.d1_r += 1.0;
                    d_delxl = d.times(t.kappa * t.alpha).sqrt();
                } else {
                    let cdsat = cdrain;
                    let d_cdsat = d_cdrain;
                    let mut gdsat = cdsat * (1.0 - fdrain) * onvdsc;
                    let mut d_dummy = d_fdrain.times(-1.0);
                    d_dummy.value += 1.0;
                    let mut d_gdsat = d_cdsat.mult(&d_dummy).mult(&d_onvdsc);
                    gdsat = f64::max(1.0e-12, gdsat);
                    if gdsat == 1.0e-12 {
                        d_gdsat = Dderivs::constant(gdsat);
                    }
                    let emax = cdsat * oneoverxl / gdsat;
                    let d_emax = d_cdsat.div(&d_gdsat).times(oneoverxl);
                    let arga = 0.5 * emax * t.alpha;
                    let d_arga = d_emax.times(0.5 * t.alpha);
                    let argc = t.kappa * t.alpha;
                    let argb = f64::sqrt(arga * arga + argc * (lvds - vdsat));
                    let mut d_dummy = d_vdsat.times(-1.0);
                    d_dummy.value += lvds;
                    d_dummy.d1_r += 1.0;
                    let d_argb = d_dummy.times(argc).plus(&d_arga.mult(&d_arga)).sqrt();
                    delxl = argb - arga;
                    d_delxl = d_argb.plus(&d_arga.times(-1.0));
                }
                // Punch-through approximation.
                if delxl > 0.5 * eff_l {
                    delxl = eff_l - (eff_l * eff_l / delxl * 0.25);
                    d_delxl = d_delxl.inv().times(-eff_l * eff_l * 0.25);
                    d_delxl.value += eff_l;
                }
                let _ = delxl;
                let d_dlonxl = d_delxl.times(oneoverxl);
                let mut d_xlfact = d_dlonxl.times(-1.0);
                d_xlfact.value += 1.0;
                d_cdrain = d_cdrain.mult(&d_xlfact.inv());
            }

            // `line700`: weak inversion.
            if lvgs < von {
                let d_onxn = d_xn.inv();
                let d_ondvt = d_onxn.times(1.0 / vt);
                let mut d_wfact = d_von.times(-1.0);
                d_wfact.value += lvgs;
                d_wfact.d1_p += 1.0;
                let d_wfact = d_wfact.mult(&d_ondvt).exp();
                d_cdrain = d_cdrain.mult(&d_wfact);
            }
            let _ = (xn, cdrain, us);
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
        setup_tail(
            typ,
            mode,
            (vbs, vbd, vds),
            (lvgs - von, vdsat),
            t.t_phi,
            oxide_cap,
            &junctions,
            &DrainDerivs::from_dderivs(&d_cdrain),
            gbs,
            gbd,
        )
    }
}

impl crate::devices::Device for Mos3I {
    /// `Mos3convTest`: predicted vs computed drain and bulk
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
        let typ = self.t.type_;
        let vt = self.t.vt;
        let st = self.st;

        let eff_l = self.geom.l - 2.0 * self.t.lat_diff;
        let (drain_sat_cur, source_sat_cur) =
            if self.t.t_sat_cur_dens == 0.0 || self.geom.ad == 0.0 || self.geom.as_ == 0.0 {
                (self.t.t_sat_cur, self.t.t_sat_cur)
            } else {
                (
                    self.t.t_sat_cur_dens * self.geom.ad,
                    self.t.t_sat_cur_dens * self.geom.as_,
                )
            };
        let gate_source_overlap_cap = self.t.cgso * self.geom.w;
        let gate_drain_overlap_cap = self.t.cgdo * self.geom.w;
        let gate_bulk_overlap_cap = self.t.cgbo * eff_l;
        let beta = self.t.t_transconductance * self.geom.w / eff_l;
        let oxide_cap = self.t.oxide_cap_factor * eff_l * self.geom.w;

        // Bias resolution — same ladder as MOS1.
        let mut check = true;
        let (mut vbs, mut vgs, mut vds);
        let predictor = matches!(ctx.mode, InitMode::Pred | InitMode::Tran);
        let general = matches!(ctx.mode, InitMode::Float | InitMode::Smsig)
            || (ctx.mode == InitMode::Fix && !self.off);
        if predictor || general {
            if predictor {
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

            let von = typ * self.von;
            if ctx.states.get(0, st + VDS) >= 0.0 {
                vgs = fetlim(vgs, ctx.states.get(0, st + VGS), von);
                vds = vgs - vgd;
                vds = limvds(vds, ctx.states.get(0, st + VDS));
            } else {
                vgd = fetlim(vgd, vgdo, von);
                vds = vgs - vgd;
                vds = -limvds(-vds, -ctx.states.get(0, st + VDS));
                vgs = vgd + vds;
            }
            if vds >= 0.0 {
                vbs = pnjlim(
                    vbs,
                    ctx.states.get(0, st + VBS),
                    vt,
                    self.t.source_vcrit,
                    &mut check,
                );
            } else {
                vbd = pnjlim(
                    vbd,
                    ctx.states.get(0, st + VBD),
                    vt,
                    self.t.drain_vcrit,
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
                vgs = typ * self.t.t_vto;
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

        // Bulk diodes, in the ngspice form — which for MOS3 is not
        // MOS1/MOS2's flat leakage but the diode's C³ cubic tail
        // below −3·vt (`arg = (3·vt/(v·e))³`), clamped exponential
        // above, gmin folded into the current
        // (`corpus/KNOWN_DIFFS.md`).
        let (mut gbs, mut cbs) = bulk_diode(vbs, vt, source_sat_cur, ctx.gmin);
        let (mut gbd, mut cbd) = bulk_diode(vbd, vt, drain_sat_cur, ctx.gmin);

        self.mode = if vds >= 0.0 { 1.0 } else { -1.0 };
        let mode = self.mode;

        // Semi-empirical evaluation (`moseq3`).
        let (lvbs, lvgs) = if mode == 1.0 { (vbs, vgs) } else { (vbd, vgd) };
        let r = moseq3(
            &self.t,
            self.geom.w,
            lvbs,
            lvgs,
            mode * vds,
            beta,
            oxide_cap,
            eff_l,
            vt,
        );
        let (cdrain, von, vdsat) = (r.cdrain, r.von, r.vdsat);
        let (gm, gds, gmbs) = (r.gm, r.gds, r.gmbs);
        self.gm = gm;
        self.gds = gds;
        self.gmbs = gmbs;
        self.von = typ * von;
        self.vdsat = typ * vdsat;

        let mut cd = mode * cdrain - cbd;

        let t = &self.t;
        let smsig = ctx.mode == InitMode::Smsig;
        if ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp || smsig {
            // Bulk junction depletion charge and capacitance (same
            // sqrt fast path and f2/f3/f4 continuation as MOS1).
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

        if (!self.off || !matches!(ctx.mode, InitMode::Fix | InitMode::Smsig)) && check {
            ctx.noncon = true;
        }

        ctx.states.set(0, st + VBS, vbs);
        ctx.states.set(0, st + VBD, vbd);
        ctx.states.set(0, st + VGS, vgs);
        ctx.states.set(0, st + VDS, vds);

        // Meyer's capacitor model.
        let (mut capgs, mut capgd, mut capgb) = (0.0, 0.0, 0.0);
        if ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp || smsig {
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
                return;
            }

            if predictor {
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
                ctx.states.set(0, st + QGS, capgs * vgs);
                ctx.states.set(0, st + QGD, capgd * vgd);
                ctx.states.set(0, st + QGB, capgb * vgb);
            }
        }

        // Meyer companion models.
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
        // Saved for the convergence test (`mos3conv.c`).
        self.cd = cd;
        self.cbs = cbs;
        self.cbd = cbd;
        ctx.rhs[self.gate] -= typ * (ceqgs + ceqgb + ceqgd);
        ctx.rhs[self.bulk] -= ceqbs + ceqbd - typ * ceqgb;
        ctx.rhs[self.d_p] += ceqbd - cdreq + typ * ceqgd;
        ctx.rhs[self.s_p] += cdreq + ceqbs + typ * ceqgs;

        // Load the Y matrix.
        let gdpr = self.t.drain_conductance;
        let gspr = self.t.source_conductance;
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

    /// `MOS3noise` (ngspice-47, default `NLEV`): see
    /// [`crate::mos1::mos_noise`].
    /// (`MOS3noise` narrows W by `2·WD`; WD is an ngspice extension
    /// this port does not carry, so W enters as given.)
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(crate::mos1::MOS_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let n = crate::mos1::MosNoise {
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
            cox: self.t.oxide_cap_factor,
        };
        crate::mos1::mos_noise(ctx, &n)
    }

    /// `MOS3dSetup`: see [`Mos3I::disto_coeffs`].
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        self.dist = self.disto_coeffs(ctx.sol, ctx.gmin);
        Ok(())
    }

    /// `MOS3disto` (`dev/mos3/mos3dist.c`, the MOS1 file renamed).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        mos_disto_load(&self.dist, (self.gate, self.bulk, self.d_p, self.s_p), ctx);
    }

    /// `MOS3acLoad` (same stamps as MOS1's).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let t = &self.t;
        let st = self.st;
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        let eff_l = self.geom.l - 2.0 * t.lat_diff;
        let capgs = 2.0 * ctx.states.get(0, st + CAPGS) + t.cgso * self.geom.w;
        let capgd = 2.0 * ctx.states.get(0, st + CAPGD) + t.cgdo * self.geom.w;
        let capgb = 2.0 * ctx.states.get(0, st + CAPGB) + t.cgbo * eff_l;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(l: f64, w: f64) -> MosGeom {
        MosGeom {
            l,
            w,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        }
    }

    /// The `n3` card of `corpus/regress/mos3_iv.cir`, with or
    /// without VMAX.
    fn n3(vmax: f64) -> Mos3Model {
        let mut m = Mos3Model::new(true);
        m.vto = Some(0.7);
        m.nsub = Some(3e16);
        m.tox = Some(2e-8);
        m.xj = 0.2e-6;
        m.ld = 0.05e-6;
        m.u0 = Some(500.0);
        m.theta = 0.08;
        m.eta = 0.3;
        m.delta = 1.2;
        m.vmax = vmax;
        m.kappa = 0.5;
        m.nfs = 8e11;
        m
    }

    struct Dev {
        t: Mos3Temp,
        w: f64,
        eff_l: f64,
        beta: f64,
        cox: f64,
    }

    impl Dev {
        fn new(m: &Mos3Model, l: f64, w: f64) -> Dev {
            let g = geom(l, w);
            let t = Mos3Temp::new("m1", m, &g, REFTEMP, REFTEMP).unwrap();
            let eff_l = l - 2.0 * t.lat_diff;
            let beta = t.t_transconductance * w / eff_l;
            let cox = t.oxide_cap_factor * eff_l * w;
            Dev {
                t,
                w,
                eff_l,
                beta,
                cox,
            }
        }
        fn eval(&self, vgs: f64, vds: f64, vbs: f64) -> Mos3Eval {
            moseq3(
                &self.t, self.w, vbs, vgs, vds, self.beta, self.cox, self.eff_l, self.t.vt,
            )
        }
        /// Central finite differences of the drain current:
        /// (∂/∂vgs, ∂/∂vds, ∂/∂vbs).
        fn fd(&self, vgs: f64, vds: f64, vbs: f64) -> (f64, f64, f64) {
            let h = 1e-6;
            let c = |a: f64, b: f64, d: f64| self.eval(a, b, d).cdrain;
            (
                (c(vgs + h, vds, vbs) - c(vgs - h, vds, vbs)) / (2.0 * h),
                (c(vgs, vds + h, vbs) - c(vgs, vds - h, vbs)) / (2.0 * h),
                (c(vgs, vds, vbs + h) - c(vgs, vds, vbs - h)) / (2.0 * h),
            )
        }
    }

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / f64::max(a.abs(), b.abs())
    }

    #[test]
    fn temp_derives_from_nsub_with_ngspice_ni() {
        let d = Dev::new(&n3(1.5e5), 2e-6, 2e-6);
        let t = &d.t;
        let cox = 3.9 * 8.854214871e-12 / 2e-8;
        assert!(rel(t.oxide_cap_factor, cox) < 1e-15);
        // GAMMA = sqrt(2·EPSSIL·q·NSUB)/COX.
        let gamma = f64::sqrt(2.0 * EPSSIL * CHARGE * 3e16 * 1e6) / cox;
        assert!(rel(t.gamma, gamma) < 1e-14);
        // alpha = 2·EPSSIL/(q·NSUB), coeffDepLayWidth = sqrt(alpha).
        let alpha = 2.0 * EPSSIL / (CHARGE * 3e16 * 1e6);
        assert!(rel(t.alpha, alpha) < 1e-14);
        assert!(rel(t.coeff_dep_lay_width, alpha.sqrt()) < 1e-14);
        // narrowFactor = DELTA·π/2·EPSSIL/COX.
        let nf = 1.2 * 0.5 * std::f64::consts::PI * EPSSIL / cox;
        assert!(rel(t.narrow_factor, nf) < 1e-14);
        // KP from U0·COX; tSurfMob = U0 at TNOM.
        assert!(rel(t.t_transconductance, 500.0 * cox * 1e-4) < 1e-14);
        assert!(rel(t.t_surf_mob, 500.0) < 1e-15);
        // PHI from NSUB over ngspice's ni(TNOM), slightly above the
        // 3f5's fixed-ni value (ni(300.15 K) > 1.45e16).
        let vtnom = REFTEMP * KOVERQ;
        let phi_3f5 = 2.0 * vtnom * f64::ln(3e16 * 1e6 / 1.45e16);
        assert!(t.t_phi < phi_3f5 && phi_3f5 - t.t_phi < 1e-3);
        // VTO given: tVto = VTO at TNOM.
        assert!((t.t_vto - 0.7).abs() < 1e-12);
    }

    #[test]
    fn temp_rejects_bad_cards() {
        let mut m = n3(0.0);
        m.nsub = Some(1e9);
        let e = Mos3Temp::new("m1", &m, &geom(2e-6, 2e-6), REFTEMP, REFTEMP).unwrap_err();
        assert!(e.contains("Nsub < Ni"));
        // Unlike MOS1/MOS2, a non-positive effective length is fatal.
        let e = Mos3Temp::new("m1", &n3(0.0), &geom(0.1e-6, 2e-6), REFTEMP, REFTEMP).unwrap_err();
        assert!(e.contains("effective channel length"));
        let mut m = n3(0.0);
        m.nsub = None;
        m.phi = Some(-0.1);
        let e = Mos3Temp::new("m1", &m, &geom(2e-6, 2e-6), REFTEMP, REFTEMP).unwrap_err();
        assert!(e.contains("Phi"));
    }

    #[test]
    fn temp_scales_mobility_and_junctions_with_temperature() {
        let mut m = n3(0.0);
        m.cj = Some(4e-4);
        m.cjsw = Some(3e-10);
        let mut g = geom(2e-6, 2e-6);
        g.ad = 8e-12;
        g.pd = 12e-6;
        let hot = REFTEMP + 60.0;
        let t = Mos3Temp::new("m1", &m, &g, hot, REFTEMP).unwrap();
        let ratio = hot / REFTEMP;
        assert!(rel(t.t_surf_mob, 500.0 / (ratio * ratio.sqrt())) < 1e-14);
        // The forward continuation joins the depletion charge at
        // FC·PB with matching value and slope (ngspice's f2/f3/f4).
        let v = t.t_dep_cap;
        let arg = 1.0 - v / t.t_bulk_pot;
        let sarg = f64::exp(-m.mj * f64::ln(arg));
        let sargsw = f64::exp(-m.mjsw * f64::ln(arg));
        let q = t.t_bulk_pot
            * (t.cbd * (1.0 - arg * sarg) / (1.0 - m.mj)
                + t.cbdsw * (1.0 - arg * sargsw) / (1.0 - m.mjsw));
        let c = t.cbd * sarg + t.cbdsw * sargsw;
        assert!(rel(t.f4d + v * (t.f2d + v * t.f3d / 2.0), q) < 1e-12);
        assert!(rel(t.f2d + v * t.f3d, c) < 1e-12);
    }

    #[test]
    fn linear_region_with_vmax_has_exact_derivatives() {
        // No CLM below vdsat on the VMAX path, so gm/gds/gmbs are the
        // exact derivatives (THETA, ETA, XJ, DELTA all active).
        let d = Dev::new(&n3(1.5e5), 2e-6, 2e-6);
        for (vgs, vds, vbs) in [(2.0, 0.3, 0.0), (3.0, 0.5, -1.0), (1.5, 0.1, -0.5)] {
            let r = d.eval(vgs, vds, vbs);
            assert!(vds < r.vdsat);
            let (gm, gds, gmbs) = d.fd(vgs, vds, vbs);
            assert!(rel(r.gm, gm) < 1e-5, "gm {} vs {gm}", r.gm);
            assert!(rel(r.gds, gds) < 1e-5, "gds {} vs {gds}", r.gds);
            assert!(rel(r.gmbs, gmbs) < 1e-5, "gmbs {} vs {gmbs}", r.gmbs);
        }
    }

    #[test]
    fn weak_inversion_derivatives() {
        let d = Dev::new(&n3(1.5e5), 2e-6, 2e-6);
        let r = d.eval(0.5, 3.0, -1.0);
        assert!(0.5 < r.von && r.cdrain > 0.0);
        let (gm, gds, gmbs) = d.fd(0.5, 3.0, -1.0);
        assert!(rel(r.gm, gm) < 1e-3);
        assert!(rel(r.gds, gds) < 1e-3);
        assert!(rel(r.gmbs, gmbs) < 1e-3);
    }

    #[test]
    fn saturation_derivatives_track_the_current() {
        // ngspice's CLM bookkeeping is approximate (∂delxl/∂vdsat is
        // taken as −∂delxl/∂vds), so gm/gmbs are a few 1e-3–1e-2 off
        // the exact derivative; gds is exact without ETA/VMAX.
        let mut m = n3(0.0);
        m.eta = 0.0;
        let d = Dev::new(&m, 3e-6, 6e-6);
        for (vgs, vds, vbs) in [(2.0, 3.0, 0.0), (3.0, 2.5, -1.0), (2.0, 0.5, 0.0)] {
            let r = d.eval(vgs, vds, vbs);
            let (gm, gds, gmbs) = d.fd(vgs, vds, vbs);
            assert!(rel(r.gds, gds) < 1e-5, "gds {} vs {gds}", r.gds);
            assert!(rel(r.gm, gm) < 2e-2, "gm {} vs {gm}", r.gm);
            assert!(rel(r.gmbs, gmbs) < 2e-2, "gmbs {} vs {gmbs}", r.gmbs);
        }
        let d = Dev::new(&n3(1.5e5), 2e-6, 2e-6);
        let r = d.eval(2.5, 3.0, -1.0);
        assert!(3.0 > r.vdsat);
        let (gm, gds, gmbs) = d.fd(2.5, 3.0, -1.0);
        assert!(rel(r.gm, gm) < 3e-2);
        assert!(rel(r.gds, gds) < 3e-2);
        assert!(rel(r.gmbs, gmbs) < 3e-2);
    }

    #[test]
    fn smoothed_clm_is_continuous_through_vdsat() {
        // Without VMAX ngspice's CLM is C¹ at vds = vdsat (the 3f5's
        // sqrt(vds−vdsat) has an infinite slope there).
        let d = Dev::new(&n3(0.0), 3e-6, 6e-6);
        let vdsat = d.eval(2.0, 1.0, 0.0).vdsat;
        let lo = d.eval(2.0, vdsat - 1e-9, 0.0);
        let hi = d.eval(2.0, vdsat + 1e-9, 0.0);
        assert!(rel(lo.cdrain, hi.cdrain) < 1e-8);
        assert!(rel(lo.gds, hi.gds) < 1e-4);
        // And the channel already shortens below vdsat.
        let mut m = n3(0.0);
        m.kappa = 0.0;
        let flat = Dev::new(&m, 3e-6, 6e-6);
        assert!(lo.cdrain > flat.eval(2.0, vdsat - 1e-9, 0.0).cdrain);
    }

    #[test]
    fn cutoff_without_nfs_and_vds_zero() {
        let mut m = n3(0.0);
        m.nfs = 0.0;
        let d = Dev::new(&m, 2e-6, 2e-6);
        let r = d.eval(0.2, 1.0, 0.0);
        assert_eq!((r.cdrain, r.gm, r.gds, r.gmbs), (0.0, 0.0, 0.0, 0.0));
        // line900: vds = 0 gives the channel conductance only.
        let r = d.eval(2.0, 0.0, 0.0);
        assert_eq!((r.cdrain, r.gm, r.gmbs), (0.0, 0.0, 0.0));
        let (_, gds, _) = d.fd(2.0, 1e-5, 0.0);
        assert!(rel(r.gds, gds) < 1e-3);
    }

    #[test]
    fn bulk_diode_tail_is_c1_at_minus_3vt() {
        let vt = REFTEMP * KOVERQ;
        let v = -3.0 * vt;
        let (g0, i0) = bulk_diode(v, vt, 1e-14, 0.0);
        let (g1, i1) = bulk_diode(v + 1e-9, vt, 1e-14, 0.0);
        assert!(rel(i0, i1) < 1e-6);
        assert!(rel(g0, g1) < 1e-5);
        let (g, i) = bulk_diode(-5.0, vt, 1e-14, 1e-12);
        assert!(i < -1e-14 && g > 1e-12);
    }
}
