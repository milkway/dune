//! MOSFET level 2 — the Grove-Frohman model from the 3f5
//! (`dev/mos2/mos2load.c`, `mos2temp.c`, `mos2set.c`, `mos2acld.c`,
//! `mos2trunc.c`). Analytical bulk-charge drain current with
//! short-channel (`XJ`) and narrow-width (`DELTA`) threshold
//! corrections, mobility degradation (`UCRIT`/`UEXP`), velocity
//! saturation by Baum's quartic (`VMAX`/`NEFF`), channel-length
//! modulation from the depletion approximation, and the `NFS`
//! subthreshold region. The scaffolding around the current
//! evaluation (bias ladder, limiting, bulk diodes, depletion and
//! Meyer charges, companion models, stamps) is byte-equivalent to
//! MOS1's, which is why this file mirrors `mos1.rs` there.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::dderivs::Dderivs;
use crate::diode::pnjlim;
use crate::disto::{DistoCtx, DistoSetupCtx};
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{MeyerPzCaps, MosGeom, MosPzConductances, fetlim, limvds, meyer_pz_load, qmeyer};
use crate::mosdisto::{
    DrainDerivs, MosDistoCoeffs, MosJunctions, bulk_diode, mos_disto_load, setup_tail,
};

/// Epsilon of silicon (`mos2load.c`: `EPSSIL`).
const EPSSIL: f64 = 11.7 * 8.854214871e-12;

/// `.model … nmos(level=2 …)/pmos(level=2 …)` parameters, 3f5 names
/// and defaults (`mos2.c` keyword table; defaults from `mos2set.c`).
/// `Option` marks parameters whose given-ness changes the
/// preprocessing, as in [`crate::Mos1Model`].
#[derive(Clone, Debug)]
pub struct Mos2Model {
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
    /// `LAMBDA`, default 0 (0 enables the depletion-layer
    /// channel-length modulation when `NSUB` is given).
    pub lambda: f64,
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
    /// `CJ`, per m²; the 3f5 computes a default from `NSUB` but the
    /// zero-bias capacitances still key off the given flag, so the
    /// computed default never reaches them.
    pub cj: Option<f64>,
    /// `MJ`, default 0.5.
    pub mj: f64,
    /// `CJSW`, per meter, default 0.
    pub cjsw: Option<f64>,
    /// `MJSW`, default **0.33** (not 0.5 as in MOS1).
    pub mjsw: f64,
    /// `JS`, per m², default 0.
    pub js: f64,
    /// `TOX`, default 1e-7 m (MOS2 always has an oxide).
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
    /// `NSS`, 1/cm².
    pub nss: Option<f64>,
    /// `DELTA`: width effect on threshold, default 0.
    pub delta: f64,
    /// `UEXP`: critical-field exponent for mobility degradation,
    /// default 0.
    pub uexp: f64,
    /// `UCRIT`: critical field, V/cm, default 1e4.
    pub ucrit: f64,
    /// `VMAX`: maximum carrier drift velocity, m/s, default 0 (off).
    pub vmax: f64,
    /// `XJ`: metallurgical junction depth, default 0.
    pub xj: f64,
    /// `NEFF`: total channel charge coefficient, default 1.
    pub neff: f64,
    /// `NFS`: fast surface state density, 1/cm², default 0.
    pub nfs: f64,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `NLEV` (ngspice-47 only, default 2): noise equation selector
    /// — `< 3` channel thermal `4kT·(2/3)|gm|`, `3` the
    /// `GDSNOI`-scaled form; for the MOSFETs also the flicker form
    /// (0/1/2–3).
    pub nlev: i64,
    /// `GDSNOI` (ngspice-47 only, default 1): channel noise
    /// coefficient of `NLEV = 3`.
    pub gdsnoi: f64,
    /// `TNOM` in kelvin.
    pub tnom: Option<f64>,
}

impl Mos2Model {
    pub fn new(nmos: bool) -> Self {
        Mos2Model {
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
            mjsw: 0.33,
            js: 0.0,
            tox: None,
            ld: 0.0,
            u0: None,
            fc: 0.5,
            nsub: None,
            tpg: None,
            nss: None,
            delta: 0.0,
            uexp: 0.0,
            ucrit: 1e4,
            vmax: 0.0,
            xj: 0.0,
            neff: 1.0,
            nfs: 0.0,
            kf: 0.0,
            af: 1.0,
            nlev: 2,
            gdsnoi: 1.0,
            tnom: None,
        }
    }

    /// Same prime-node rule as MOS1 (`mos2set.c`).
    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rd.unwrap_or(0.0) != 0.0 || (self.rsh.unwrap_or(0.0) != 0.0 && squares != 0.0)
    }
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

/// Model- and temperature-derived values (`MOS2temp`).
#[derive(Debug)]
pub(crate) struct Mos2Temp {
    /// The instance temperature, kelvin (`*temp`: the card's
    /// `TEMP=` or `CKTtemp`) — the thermal noise generators' T
    /// (ngspice-47 `NevalSrcInstanceTemp`).
    pub temp: f64,
    pub type_: f64,
    pub vt: f64,
    // Model copies the load needs.
    pub lat_diff: f64,
    /// `KF`/`AF`: flicker-noise coefficient and exponent.
    pub kf: f64,
    pub af: f64,
    /// `NLEV`/`GDSNOI` (noise only).
    pub nlev: i64,
    pub gdsnoi: f64,
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    pub oxide_cap_factor: f64,
    pub gamma: f64,
    pub lambda: f64,
    pub mj: f64,
    pub mjsw: f64,
    pub delta: f64,
    pub xj: f64,
    pub nfs: f64,
    pub ucrit: f64,
    pub uexp: f64,
    pub vmax: f64,
    pub neff: f64,
    /// Raw `U0` after defaulting — the mobility section of the load
    /// uses the model value, not the temperature-adjusted one
    /// (3f5 quirk kept).
    pub u0: f64,
    /// `NSUB·?` as the load sees it (0 when not given).
    pub nsub: f64,
    /// `MOS2xd`: depletion-layer width factor from `NSUB`.
    pub xd: f64,
    // Temperature-adjusted values (same formulas as MOS1).
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

impl Mos2Temp {
    pub fn new(
        name: &str,
        m: &Mos2Model,
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

        // Model preprocessing. Unlike MOS1, the oxide always exists
        // (TOX defaults to 1e-7) and KP always falls back to U0·COX.
        let tox = m.tox.unwrap_or(1e-7);
        let oxide_cap_factor = 3.9 * 8.854214871e-12 / tox;
        let u0 = m.u0.unwrap_or(600.0);
        let mut kp = m.kp;
        if kp.is_none() {
            kp = Some(u0 * 1e-4 /* m²/cm² */ * oxide_cap_factor);
        }
        let mut phi = m.phi;
        let mut gamma = m.gamma;
        let mut vto = m.vto;
        let mut xd = 0.0;
        let mut nsub = 0.0;
        if let Some(nsub_given) = m.nsub {
            if nsub_given * 1e6 /* cm³/m³ */ > 1.45e16 {
                nsub = nsub_given;
                if phi.is_none() {
                    phi = Some(f64::max(
                        0.1,
                        2.0 * vtnom * f64::ln(nsub_given * 1e6 / 1.45e16),
                    ));
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
                        f64::sqrt(2.0 * 11.70 * 8.854214871e-12 * CHARGE * nsub_given * 1e6)
                            / oxide_cap_factor,
                    );
                }
                if vto.is_none() {
                    let nss = m.nss.unwrap_or(0.0);
                    let vfb = wkfngs - nss * 1e4 /* cm²/m² */ * CHARGE / oxide_cap_factor;
                    vto = Some(vfb + typ * (gamma.unwrap_or(0.0) * phi_v.sqrt() + phi_v));
                }
                xd = f64::sqrt((EPSSIL + EPSSIL) / (CHARGE * nsub_given * 1e6));
            } else {
                return Err(format!("{name}: Nsub < Ni"));
            }
        }
        let kp = kp.unwrap_or(2e-5);
        let phi = phi.unwrap_or(0.6);
        let gamma = gamma.unwrap_or(0.0);
        let vto = vto.unwrap_or(0.0);

        // Per-instance temperature block (identical to MOS1's).
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

        Ok(Mos2Temp {
            temp,
            type_: typ,
            vt,
            lat_diff: m.ld,
            kf: m.kf,
            af: m.af,
            nlev: m.nlev,
            gdsnoi: m.gdsnoi,
            cgso: m.cgso,
            cgdo: m.cgdo,
            cgbo: m.cgbo,
            oxide_cap_factor,
            gamma,
            lambda: m.lambda,
            mj: m.mj,
            mjsw: m.mjsw,
            delta: m.delta,
            xj: m.xj,
            nfs: m.nfs,
            ucrit: m.ucrit,
            uexp: m.uexp,
            vmax: m.vmax,
            neff: m.neff,
            u0,
            nsub,
            xd,
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
            drain_conductance: series(m.rd, g.nrd),
            source_conductance: series(m.rs, g.nrs),
            notes,
        })
    }
}

// State offsets (`mos2defs.h` — identical to MOS1's).
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
pub(crate) const MOS2_NUM_STATES: usize = 17;

// Slot order (matches `mos2set.c`'s TSTALLOC list — same as MOS1).
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

/// Baum-quartic sign tables (`mos2load.c`).
const SIG1: [f64; 4] = [1.0, -1.0, 1.0, -1.0];
const SIG2: [f64; 4] = [1.0, 1.0, -1.0, -1.0];

pub(crate) struct Mos2I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Mos2Temp,
    geom: MosGeom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    ic_given: crate::devices::IcGiven,
    st: usize,
    s: [Slot; 22],
    // Values that persist between iterations (`MOS2instance`).
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
    /// `.disto` Taylor coefficients (`MOS2dSetup`).
    dist: MosDistoCoeffs,
}

impl Mos2I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],  // d g s b
        primes: [Var; 2], // d' s'
        t: Mos2Temp,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64), // (vds, vgs, vbs)
        st: usize,
    ) -> Self {
        Mos2I {
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

    /// The Grove-Frohman current evaluation (the `moseq2` block of
    /// `mos2load.c`). Returns `(cdrain, von, vdsat)` in the mode-
    /// normalized space and updates `gm`/`gds`/`gmbs` exactly where
    /// the C writes the instance fields — including the paths that
    /// leave some of them untouched.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn eval(
        &mut self,
        vbs: f64,
        vbd: f64,
        vgs: f64,
        vgd: f64,
        vds: f64,
        beta: f64,
        oxide_cap: f64,
        eff_l: f64,
        vt: f64,
    ) -> (f64, f64, f64) {
        let t = &self.t;
        let typ = t.type_;
        let mode = self.mode;
        let mut xlamda = t.lambda;
        let lvbs = if mode == 1.0 { vbs } else { vbd };
        let lvds = mode * vds;
        let lvgs = if mode == 1.0 { vgs } else { vgd };
        let phi_min_vbs = t.t_phi - lvbs;

        // sqrt(phi − vbs) and derivatives, with the series form for
        // forward bias.
        let sphi = t.t_phi.sqrt();
        let sphi3 = t.t_phi * sphi;
        let (sarg, dsrgdb, d2sdb2);
        if lvbs <= 0.0 {
            sarg = phi_min_vbs.sqrt();
            dsrgdb = -0.5 / sarg;
            d2sdb2 = 0.5 * dsrgdb / phi_min_vbs;
        } else {
            sarg = sphi / (1.0 + 0.5 * lvbs / t.t_phi);
            let tmp = sarg / sphi3;
            dsrgdb = -0.5 * sarg * tmp;
            d2sdb2 = -dsrgdb * tmp;
        }
        let (mut barg, mut dbrgdb, d2bdb2);
        if lvds - lvbs >= 0.0 {
            barg = (phi_min_vbs + lvds).sqrt();
            dbrgdb = -0.5 / barg;
            d2bdb2 = 0.5 * dbrgdb / (phi_min_vbs + lvds);
        } else {
            barg = sphi / (1.0 + 0.5 * (lvbs - lvds) / t.t_phi);
            let tmp = barg / sphi3;
            dbrgdb = -0.5 * barg * tmp;
            d2bdb2 = -dbrgdb * tmp;
        }

        // Threshold voltage: narrow-channel and short-channel
        // corrections.
        let factor = 0.125 * t.delta * 2.0 * std::f64::consts::PI * EPSSIL / oxide_cap * eff_l;
        let eta = 1.0 + factor;
        let vbin = t.t_vbi * typ + factor * phi_min_vbs;
        let (gamasd, dgddvb, dgdvds, dgddb2);
        if t.gamma > 0.0 || t.nsub > 0.0 {
            let xwd = t.xd * barg;
            let xws = t.xd * sarg;
            let mut argss = 0.0;
            let mut argsd = 0.0;
            let mut dbargs = 0.0;
            let mut dbargd = 0.0;
            let mut dgdvds_l = 0.0;
            let mut dgddb2_l = 0.0;
            let mut args = 0.0;
            let mut argd = 0.0;
            let mut argxs = 0.0;
            let mut argxd = 0.0;
            if t.xj > 0.0 {
                let tmp = 2.0 / t.xj;
                argxs = 1.0 + xws * tmp;
                argxd = 1.0 + xwd * tmp;
                args = argxs.sqrt();
                argd = argxd.sqrt();
                let tmp = 0.5 * t.xj / eff_l;
                argss = tmp * (args - 1.0);
                argsd = tmp * (argd - 1.0);
            }
            gamasd = t.gamma * (1.0 - argss - argsd);
            let dbxwd = t.xd * dbrgdb;
            let dbxws = t.xd * dsrgdb;
            if t.xj > 0.0 {
                let tmp = 0.5 / eff_l;
                dbargs = tmp * dbxws / args;
                dbargd = tmp * dbxwd / argd;
                let dasdb2 =
                    -t.xd * (d2sdb2 + dsrgdb * dsrgdb * t.xd / (t.xj * argxs)) / (eff_l * args);
                let daddb2 =
                    -t.xd * (d2bdb2 + dbrgdb * dbrgdb * t.xd / (t.xj * argxd)) / (eff_l * argd);
                dgddb2_l = -0.5 * t.gamma * (dasdb2 + daddb2);
            }
            dgddvb = -t.gamma * (dbargs + dbargd);
            if t.xj > 0.0 {
                let ddxwd = -dbxwd;
                dgdvds_l = -t.gamma * 0.5 * ddxwd / (eff_l * argd);
            }
            dgdvds = dgdvds_l;
            dgddb2 = dgddb2_l;
        } else {
            gamasd = t.gamma;
            dgddvb = 0.0;
            dgdvds = 0.0;
            dgddb2 = 0.0;
        }

        let cdrain;
        let mut von = vbin + gamasd * sarg;
        let vth = von;
        let mut vdsat = 0.0;
        // `gm`/`gmbs` keep their previous-iteration values on the
        // one path the C leaves them untouched; `gds` and `cdrain`
        // are written on every path.
        let mut gm = self.gm;
        let gds;
        let mut gmbs = self.gmbs;

        'doneval: {
            let mut xn = 1.0;
            let mut argg = 0.0;
            let vgst;
            if t.nfs != 0.0 && oxide_cap != 0.0 {
                let cfs = CHARGE * t.nfs * 1e4 /* cm²/m² */;
                let cdonco = -(gamasd * dsrgdb + dgddvb * sarg) + factor;
                xn = 1.0 + cfs / oxide_cap * self.geom.w * eff_l + cdonco;
                let tmp = vt * xn;
                von += tmp;
                argg = 1.0 / tmp;
                vgst = lvgs - von;
            } else {
                vgst = lvgs - von;
                // ngspice (alansfixes) draws this boundary at
                // `vbin`, which zeroes the current but leaves a
                // negative gm in (vbin, von]; its Newton only
                // survives that with ngspice's dynamic gmin
                // stepping. The currents are the same either way,
                // so Dune keeps the 3f5 boundary
                // (`corpus/KNOWN_DIFFS.md`).
                if lvgs <= von {
                    // Cutoff region.
                    gds = 0.0;
                    gm = 0.0;
                    gmbs = 0.0;
                    cdrain = 0.0;
                    break 'doneval;
                }
            }

            // More useful quantities.
            let sarg3 = sarg * sarg * sarg;
            let sbiarg = t.t_bulk_pot.sqrt();
            let mut gammad = gamasd;
            let mut dgdvbs = dgddvb;
            let mut body = barg * barg * barg - sarg3;
            let mut gdbdv = 2.0 * gammad * (barg * barg * dbrgdb - sarg * sarg * dsrgdb);
            let mut dodvbs = -factor + dgdvbs * sarg + gammad * dsrgdb;
            let mut dxndvb = 0.0;
            let mut dxndvd = 0.0;
            let mut dodvds = 0.0;
            if t.nfs != 0.0 && oxide_cap != 0.0 {
                dxndvb = 2.0 * dgdvbs * dsrgdb + gammad * d2sdb2 + dgddb2 * sarg;
                dodvbs += vt * dxndvb;
                dxndvd = dgdvds * dsrgdb;
                dodvds = dgdvds * sarg + vt * dxndvd;
            }

            // Effective mobility (lines 400/410/500 of the C).
            let (ufact, ueff, dudvgs, dudvds, dudvbs);
            let crit = t.ucrit * 100.0 /* cm/m */ * EPSSIL / t.oxide_cap_factor;
            if oxide_cap > 0.0 && vgst > crit {
                let udenom = vgst;
                ufact = f64::exp(t.uexp * f64::ln(crit / udenom));
                ueff = t.u0 * 1e-4 /* m²/cm² */ * ufact;
                dudvgs = -ufact * t.uexp / udenom;
                dudvds = 0.0;
                dudvbs = t.uexp * ufact * dodvbs / vgst;
            } else {
                ufact = 1.0;
                ueff = t.u0 * 1e-4;
                dudvgs = 0.0;
                dudvds = 0.0;
                dudvbs = 0.0;
            }

            // Saturation voltage (Grove-Frohman), then Baum's
            // scattering-velocity theory when VMAX is given.
            let mut vgsx = lvgs;
            gammad = gamasd / eta;
            dgdvbs = dgddvb;
            if t.nfs != 0.0 && oxide_cap != 0.0 {
                vgsx = f64::max(lvgs, von);
            }
            let mut dsdvgs;
            let mut dsdvbs;
            if gammad > 0.0 {
                let gammd2 = gammad * gammad;
                let argv = (vgsx - vbin) / eta + phi_min_vbs;
                if argv <= 0.0 {
                    vdsat = 0.0;
                    dsdvgs = 0.0;
                    dsdvbs = 0.0;
                } else {
                    let arg = (1.0 + 4.0 * argv / gammd2).sqrt();
                    vdsat = (vgsx - vbin) / eta + gammd2 * (1.0 - arg) / 2.0;
                    vdsat = f64::max(vdsat, 0.0);
                    dsdvgs = (1.0 - 1.0 / arg) / eta;
                    dsdvbs = (gammad * (1.0 - arg) + 2.0 * argv / (gammad * arg)) / eta * dgdvbs
                        + 1.0 / arg
                        + factor * dsdvgs;
                }
            } else {
                vdsat = f64::max((vgsx - vbin) / eta, 0.0);
                dsdvgs = 1.0;
                dsdvbs = 0.0;
            }
            if t.vmax > 0.0 {
                // (the C computes gammd2 here too and never uses it)
                let v1 = (vgsx - vbin) / eta + phi_min_vbs;
                let v2 = phi_min_vbs;
                let xv = t.vmax * eff_l / ueff;
                let a1 = gammad / 0.75;
                let b1 = -2.0 * (v1 + xv);
                let c1 = -2.0 * gammad * xv;
                let d1 = 2.0 * v1 * (v2 + xv) - v2 * v2 - 4.0 / 3.0 * gammad * sarg3;
                let a = -b1;
                let b = a1 * c1 - 4.0 * d1;
                let c = -d1 * (a1 * a1 - 4.0 * b1) - c1 * c1;
                let r = -a * a / 3.0 + b;
                let s = 2.0 * a * a * a / 27.0 - a * b / 3.0 + c;
                let r3 = r * r * r;
                let s2 = s * s;
                let p = s2 / 4.0 + r3 / 27.0;
                let p0 = p.abs();
                let p2 = p0.sqrt();
                let y3 = if p < 0.0 {
                    let ro = (s2 / 4.0 + p0).sqrt();
                    let ro = f64::exp(f64::ln(ro) / 3.0);
                    let fi = f64::atan(-2.0 * p2 / s);
                    2.0 * ro * f64::cos(fi / 3.0) - a / 3.0
                } else {
                    let p3 = f64::exp(f64::ln((-s / 2.0 + p2).abs()) / 3.0);
                    let p4 = f64::exp(f64::ln((-s / 2.0 - p2).abs()) / 3.0);
                    p3 + p4 - a / 3.0
                };
                let a3 = (a1 * a1 / 4.0 - b1 + y3).sqrt();
                let b3 = (y3 * y3 / 4.0 - d1).sqrt();
                let mut x4 = [0.0f64; 8];
                let mut iknt = 0usize;
                for i in 0..4 {
                    let a4 = a1 / 2.0 + SIG1[i] * a3;
                    let b4 = y3 / 2.0 + SIG2[i] * b3;
                    let delta4 = a4 * a4 / 4.0 - b4;
                    if delta4 < 0.0 {
                        continue;
                    }
                    let tmp = delta4.sqrt();
                    x4[iknt] = -a4 / 2.0 + tmp;
                    iknt += 1;
                    x4[iknt] = -a4 / 2.0 - tmp;
                    iknt += 1;
                }
                let mut jknt = 0usize;
                let mut xvalid = 0.0;
                for &x in x4.iter().take(iknt) {
                    if x <= 0.0 {
                        continue;
                    }
                    let mut poly4 = x * x * x * x + a1 * x * x * x;
                    poly4 += b1 * x * x + c1 * x + d1;
                    if poly4.abs() > 1.0e-6 {
                        continue;
                    }
                    jknt += 1;
                    if jknt <= 1 {
                        xvalid = x;
                    }
                    if x > xvalid {
                        continue;
                    }
                    xvalid = x;
                }
                if jknt > 0 {
                    vdsat = xvalid * xvalid - phi_min_vbs;
                }
            }

            // Effective channel length and its derivatives.
            let mut dldvgs = 0.0;
            let mut dldvds = 0.0;
            let mut dldvbs = 0.0;
            let mut bsarg = 0.0;
            let mut dbsrdb = 0.0;
            let mut bodys = 0.0;
            let mut gdbdvs = 0.0;
            if lvds != 0.0 {
                gammad = gamasd;
                if lvbs - vdsat <= 0.0 {
                    bsarg = (vdsat + phi_min_vbs).sqrt();
                    dbsrdb = -0.5 / bsarg;
                } else {
                    bsarg = sphi / (1.0 + 0.5 * (lvbs - vdsat) / t.t_phi);
                    dbsrdb = -0.5 * bsarg * bsarg / sphi3;
                }
                bodys = bsarg * bsarg * bsarg - sarg3;
                gdbdvs = 2.0 * gammad * (bsarg * bsarg * dbsrdb - sarg * sarg * dsrgdb);
                'line610: {
                    let dldsat;
                    if t.vmax <= 0.0 {
                        if t.nsub == 0.0 || xlamda > 0.0 {
                            break 'line610;
                        }
                        let argv = (lvds - vdsat) / 4.0;
                        let sargv = (1.0 + argv * argv).sqrt();
                        let arg = (argv + sargv).sqrt();
                        let xlfact = t.xd / (eff_l * lvds);
                        xlamda = xlfact * arg;
                        dldsat = lvds * xlamda / (8.0 * sargv);
                    } else {
                        let argv = (vgsx - vbin) / eta - vdsat;
                        let xdv = t.xd / t.neff.sqrt();
                        let xlv = t.vmax * xdv / (2.0 * ueff);
                        let vqchan = argv - gammad * bsarg;
                        let dqdsat = -1.0 + gammad * dbsrdb;
                        let vl = t.vmax * eff_l;
                        let dfunds = vl * dqdsat - ueff * vqchan;
                        let dfundg = (vl - ueff * vdsat) / eta;
                        let dfundb = -vl * (1.0 + dqdsat - factor / eta)
                            + ueff * (gdbdvs - dgdvbs * bodys / 1.5) / eta;
                        dsdvgs = -dfundg / dfunds;
                        dsdvbs = -dfundb / dfunds;
                        if t.nsub == 0.0 || xlamda > 0.0 {
                            break 'line610;
                        }
                        let argv = f64::max(lvds - vdsat, 0.0);
                        let xls = (xlv * xlv + argv).sqrt();
                        let xlfact = xdv / (eff_l * lvds);
                        xlamda = xlfact * (xls - xlv);
                        dldsat = xdv / (2.0 * xls) / eff_l;
                    }
                    dldvgs = dldsat * dsdvgs;
                    dldvds = -xlamda + dldsat;
                    dldvbs = dldsat * dsdvbs;
                }
            }

            // Punch-through limit on channel shortening.
            let mut xwb = t.xd * sbiarg;
            let xld = eff_l - xwb;
            let mut clfact = 1.0 - xlamda * lvds;
            dldvds = -xlamda - dldvds;
            let mut xleff = eff_l * clfact;
            let deltal = xlamda * lvds * eff_l;
            if t.nsub == 0.0 {
                xwb = 0.25e-6;
            }
            if xleff < xwb {
                xleff = xwb / (1.0 + (deltal - xld) / xwb);
                clfact = xleff / eff_l;
                let dfact = xleff * xleff / (xwb * xwb);
                dldvgs *= dfact;
                dldvds *= dfact;
                dldvbs *= dfact;
            }

            // Effective beta.
            let beta1 = beta * ufact / clfact;

            // Mode of operation.
            gammad = gamasd;
            dgdvbs = dgddvb;
            if lvds <= 1.0e-10 {
                if lvgs <= von {
                    if t.nfs == 0.0 || oxide_cap == 0.0 {
                        gds = 0.0;
                    } else {
                        gds = beta1 * (von - vbin - gammad * sarg) * f64::exp(argg * (lvgs - von));
                    }
                } else {
                    gds = beta1 * (lvgs - vbin - gammad * sarg);
                }
                gm = 0.0;
                gmbs = 0.0;
                cdrain = 0.0;
                break 'doneval;
            }

            if lvgs <= von {
                // Subthreshold region.
                if vdsat <= 0.0 {
                    gds = 0.0;
                    if lvgs > vth {
                        // The C jumps to `doneval` without setting
                        // cdrain (reads stale memory); we use 0.
                        cdrain = 0.0;
                        break 'doneval;
                    }
                    gm = 0.0;
                    gmbs = 0.0;
                    cdrain = 0.0;
                    break 'doneval;
                }
                let vdson = f64::min(vdsat, lvds);
                if lvds > vdsat {
                    barg = bsarg;
                    #[allow(unused_assignments)] // kept from the C
                    {
                        dbrgdb = dbsrdb;
                    }
                    body = bodys;
                    gdbdv = gdbdvs;
                }
                let cdson =
                    beta1 * ((von - vbin - eta * vdson * 0.5) * vdson - gammad * body / 1.5);
                let didvds = beta1 * (von - vbin - eta * vdson - gammad * barg);
                let mut gdson = -cdson * dldvds / clfact - beta1 * dgdvds * body / 1.5;
                if lvds < vdsat {
                    gdson += didvds;
                }
                let mut gbson = -cdson * dldvbs / clfact
                    + beta1 * (dodvbs * vdson + factor * vdson - dgdvbs * body / 1.5 - gdbdv);
                if lvds > vdsat {
                    gbson += didvds * dsdvbs;
                }
                let expg = f64::exp(argg * (lvgs - von));
                cdrain = cdson * expg;
                let gmw = cdrain * argg;
                gm = gmw;
                if lvds > vdsat {
                    gm = gmw + didvds * dsdvgs * expg;
                }
                let tmp = gmw * (lvgs - von) / xn;
                gds = gdson * expg - gm * dodvds - tmp * dxndvd;
                gmbs = gbson * expg - gm * dodvbs - tmp * dxndvb;
                break 'doneval;
            }

            if lvds <= vdsat {
                // Linear region.
                cdrain = beta1 * ((lvgs - vbin - eta * lvds / 2.0) * lvds - gammad * body / 1.5);
                let arg = cdrain * (dudvgs / ufact - dldvgs / clfact);
                gm = arg + beta1 * lvds;
                let arg = cdrain * (dudvds / ufact - dldvds / clfact);
                gds =
                    arg + beta1 * (lvgs - vbin - eta * lvds - gammad * barg - dgdvds * body / 1.5);
                let arg = cdrain * (dudvbs / ufact - dldvbs / clfact);
                gmbs = arg - beta1 * (gdbdv + dgdvbs * body / 1.5 - factor * lvds);
            } else {
                // Saturation region.
                cdrain = beta1 * ((lvgs - vbin - eta * vdsat / 2.0) * vdsat - gammad * bodys / 1.5);
                let arg = cdrain * (dudvgs / ufact - dldvgs / clfact);
                gm = arg
                    + beta1 * vdsat
                    + beta1 * (lvgs - vbin - eta * vdsat - gammad * bsarg) * dsdvgs;
                gds = -cdrain * dldvds / clfact - beta1 * dgdvds * bodys / 1.5;
                let arg = cdrain * (dudvbs / ufact - dldvbs / clfact);
                gmbs = arg - beta1 * (gdbdvs + dgdvbs * bodys / 1.5 - factor * vdsat)
                    + beta1 * (lvgs - vbin - eta * vdsat - gammad * bsarg) * dsdvbs;
            }
        }

        self.gm = gm;
        self.gds = gds;
        self.gmbs = gmbs;
        (cdrain, von, vdsat)
    }
}

impl Mos2I {
    /// `MOS2dSetup` (`dev/mos2/mos2dset.c`): the Grove–Frohman drain
    /// current re-evaluated with the `Dderivs` algebra (p = vgs,
    /// q = vbs, r = vds of the mode-normalized device), then the
    /// shared diode/capacitance/type bookkeeping. A literal port,
    /// slips included, since ngspice-47 carries the same file: the
    /// mobility factor is 0 (not 1) below the critical field, so the
    /// drain current contributes nothing there; `argv` doubles
    /// `vgsx` instead of subtracting `vbin`; the linear-region gds
    /// loses its `lvgs`; the second Baum root adds `tmp`; the
    /// channel-length factor's seed has value `lvds` with slope −1.
    /// Where the C reads storage it never wrote (UB in both
    /// simulators) Dune reads zero, which is what the ngspice-47
    /// oracle does — measured: `d_xlamda` (any `LAMBDA > 0`, or no
    /// `NSUB`) and `d_dumarg` (`GAMMA > 0`) both reproduce the
    /// golden bit for bit as zero; the same rule covers `d_p0` when
    /// `p ≥ 0`, `d_dbargs`/`d_dbargd` without `XJ` and `d_cdrain`
    /// at the start of every instance (ngspice zeroes it once per
    /// call). `sphi` is set before its use in the `vds < vbs` branch
    /// (ngspice's HT 050523 fix), and the Baum root arrays are sized
    /// for every root (the C's `d_a4[3]`/`d_x4[3]` overrun).
    // A literal port keeps the C's dead stores.
    #[allow(
        unused_assignments,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
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

        let d_p = Dderivs::var_p();
        let d_q = Dderivs::var_q();
        let d_r = Dderivs::var_r();
        let d_zero = Dderivs::default();
        let mut d_cdrain = Dderivs::default();

        let lvbs = if mode == 1.0 { vbs } else { vbd };
        let lvds = mode * vds;
        let lvgs = if mode == 1.0 { vgs } else { vgd };
        let phi_min_vbs = t.t_phi - lvbs;
        let mut xlamda = t.lambda;
        // Never written on most paths (see above).
        let mut d_xlamda = d_zero;
        let mut vdsat = 0.0;
        let mut von;
        let vgst;
        let mut sphi = 0.0;
        let mut sphi3 = 0.0;
        let mut doneval = false;

        let d_phi_min_vbs = Dderivs {
            value: phi_min_vbs,
            d1_q: -1.0,
            ..d_zero
        };

        // The C's `goto`s, as a labelled block: `break 'eval` is
        // `goto line1050`; falling off the end is `doneval`.
        'eval: {
            let (sarg, d_sarg, dsrgdb, d_dsrgdb);
            if lvbs <= 0.0 {
                sarg = f64::sqrt(phi_min_vbs);
                d_sarg = d_phi_min_vbs.sqrt();
                dsrgdb = -0.5 / sarg;
                d_dsrgdb = d_sarg.inv().times(-0.5);
            } else {
                sphi = f64::sqrt(t.t_phi);
                sphi3 = t.t_phi * sphi;
                sarg = sphi / (1.0 + 0.5 * lvbs / t.t_phi);
                let mut s = Dderivs { value: lvbs, ..d_q }.times(0.5 / t.t_phi);
                s.value += 1.0;
                d_sarg = s.inv().times(sphi);
                dsrgdb = -0.5 * sarg * sarg / sphi3;
                d_dsrgdb = d_sarg.mult(&d_sarg).times(-0.5 / sphi3);
            }
            let (mut barg, mut d_barg, dbrgdb, d_dbrgdb);
            if lvds - lvbs >= 0.0 {
                barg = f64::sqrt(phi_min_vbs + lvds);
                let mut b = d_phi_min_vbs;
                b.value += lvds;
                b.d1_r += 1.0;
                d_barg = b.sqrt();
                dbrgdb = -0.5 / barg;
                d_dbrgdb = d_barg.inv().times(-0.5);
            } else {
                sphi = f64::sqrt(t.t_phi);
                sphi3 = t.t_phi * sphi;
                barg = sphi / (1.0 + 0.5 * (lvbs - lvds) / t.t_phi);
                let mut b = Dderivs {
                    value: lvbs - lvds,
                    ..d_q
                };
                b.d1_r -= 1.0;
                let mut b = b.times(0.5 / t.t_phi);
                b.value += 1.0;
                d_barg = b.inv().times(sphi);
                dbrgdb = -0.5 * barg * barg / sphi3;
                d_dbrgdb = d_barg.mult(&d_barg).times(-0.5 / sphi3);
            }

            // Threshold voltage: narrow-channel effect.
            let factor = 0.125 * t.delta * 2.0 * std::f64::consts::PI * EPSSIL / oxide_cap * eff_l;
            let eta = 1.0 + factor;
            let vbin = t.t_vbi * typ + factor * phi_min_vbs;
            let mut d_vbin = d_phi_min_vbs.times(factor);
            d_vbin.value += t.t_vbi * typ;

            let (gamasd, d_gamasd, dgddvb, d_dgddvb);
            if t.gamma > 0.0 || t.nsub > 0.0 {
                let xwd = t.xd * barg;
                let xws = t.xd * sarg;
                let d_xwd = d_barg.times(t.xd);
                let d_xws = d_sarg.times(t.xd);
                let mut argss = 0.0;
                let mut argsd = 0.0;
                let mut d_argss = d_zero;
                let mut d_argsd = d_zero;
                let (mut args, mut argd) = (0.0, 0.0);
                let (mut d_args, mut d_argd) = (d_zero, d_zero);
                if t.xj > 0.0 {
                    let tmp = 2.0 / t.xj;
                    let argxs = 1.0 + xws * tmp;
                    let mut d_argxs = d_xws.times(tmp);
                    d_argxs.value += 1.0;
                    let argxd = 1.0 + xwd * tmp;
                    let mut d_argxd = d_xwd.times(tmp);
                    d_argxd.value += 1.0;
                    args = f64::sqrt(argxs);
                    d_args = d_argxs.sqrt();
                    argd = f64::sqrt(argxd);
                    d_argd = d_argxd.sqrt();
                    let tmp = 0.5 * t.xj / eff_l;
                    argss = tmp * (args - 1.0);
                    d_argss = d_args.times(tmp);
                    d_argss.value -= tmp;
                    argsd = tmp * (argd - 1.0);
                    d_argsd = d_argd.times(tmp);
                    d_argsd.value -= tmp;
                }
                gamasd = t.gamma * (1.0 - argss - argsd);
                let mut gs = d_argss.plus(&d_argsd);
                gs.value -= 1.0;
                d_gamasd = gs.times(-t.gamma);
                let dbxwd = t.xd * dbrgdb;
                let dbxws = t.xd * dsrgdb;
                let d_dbxwd = d_dbrgdb.times(t.xd);
                let d_dbxws = d_dsrgdb.times(t.xd);
                let (mut dbargs, mut dbargd) = (0.0, 0.0);
                let (mut d_dbargs, mut d_dbargd) = (d_zero, d_zero);
                if t.xj > 0.0 {
                    let tmp = 0.5 / eff_l;
                    dbargs = tmp * dbxws / args;
                    dbargd = tmp * dbxwd / argd;
                    d_dbargs = d_dbxws.div(&d_args).times(tmp);
                    d_dbargd = d_dbxwd.div(&d_argd).times(tmp);
                }
                dgddvb = -t.gamma * (dbargs + dbargd);
                d_dgddvb = d_dbargs.plus(&d_dbargd).times(-t.gamma);
            } else {
                gamasd = t.gamma;
                d_gamasd = Dderivs::constant(t.gamma);
                dgddvb = 0.0;
                d_dgddvb = d_zero;
            }
            von = vbin + gamasd * sarg;
            let mut d_von = d_gamasd.mult(&d_sarg).plus(&d_vbin);
            let mut d_vdsat = d_zero;
            let mut d_argg = d_zero;
            let mut d_vgst;
            if t.nfs != 0.0 && oxide_cap != 0.0 {
                let cfs = CHARGE * t.nfs * 1e4;
                let cdonco = -(gamasd * dsrgdb + dgddvb * sarg) + factor;
                let d_dummy = d_dgddvb.mult(&d_sarg);
                let mut d_cdonco = d_gamasd.mult(&d_dsrgdb).plus(&d_dummy).times(-1.0);
                d_cdonco.value += factor;
                let xn = 1.0 + cfs / oxide_cap * g.w * eff_l + cdonco;
                let d_xn = Dderivs {
                    value: xn,
                    ..d_cdonco
                };
                let tmp = vt * xn;
                let d_tmp = d_xn.times(vt);
                von += tmp;
                d_von = d_von.plus(&d_tmp);
                d_argg = d_tmp.inv();
                vgst = lvgs - von;
                d_vgst = d_von.times(-1.0).plus(&d_p);
                d_vgst.value += lvgs;
            } else {
                vgst = lvgs - von;
                d_vgst = d_von.times(-1.0).plus(&d_p);
                d_vgst.value += lvgs;
                if lvgs <= von {
                    // Cutoff.
                    break 'eval;
                }
            }

            let sarg3 = sarg * sarg * sarg;
            let d_sarg3 = d_sarg.cube();
            let sbiarg = f64::sqrt(t.t_bulk_pot);
            let mut gammad;
            let mut d_gammad;
            let mut body = barg * barg * barg - sarg3;
            let mut d_body = d_sarg3.times(-1.0).plus(&d_barg.cube());
            let _ = body;

            // Effective mobility (`line400`/`line410`).
            let (ufact, d_ufact, ueff, d_ueff);
            let tmp = t.ucrit * 100.0 * EPSSIL / t.oxide_cap_factor;
            if oxide_cap > 0.0 && vgst > tmp {
                let udenom = vgst;
                let d_udenom = d_vgst;
                ufact = f64::exp(t.uexp * f64::ln(tmp / udenom));
                let d_dummy = d_udenom.inv().times(tmp);
                d_ufact = d_dummy.pow(t.uexp);
                ueff = t.u0 * 1e-4 * ufact;
                d_ueff = d_ufact.times(t.u0 * 1e-4);
            } else {
                ufact = 0.0;
                d_ufact = d_zero;
                ueff = t.u0 * 1e-4;
                d_ueff = Dderivs::constant(ueff);
            }
            let _ = ufact;

            // Saturation voltage (Grove–Frohman), `line500`.
            let mut vgsx = lvgs;
            let mut d_vgsx = Dderivs { value: lvgs, ..d_p };
            gammad = gamasd / eta;
            d_gammad = d_gamasd.times(1.0 / eta);
            if t.nfs != 0.0 && oxide_cap != 0.0 {
                vgsx = f64::max(lvgs, von);
                d_vgsx = if lvgs > von {
                    Dderivs { value: lvgs, ..d_p }
                } else {
                    d_von
                };
            }
            if gammad > 0.0 {
                let gammd2 = gammad * gammad;
                let d_gammd2 = d_gammad.mult(&d_gammad);
                let argv = (vgsx - vbin) / eta + phi_min_vbs;
                let d_argv = d_vgsx.plus(&d_vgsx).times(1.0 / eta).plus(&d_phi_min_vbs);
                if argv <= 0.0 {
                    vdsat = 0.0;
                    d_vdsat = d_zero;
                } else {
                    let mut d_arg = d_argv.div(&d_gammd2).times(4.0);
                    d_arg.value += 1.0;
                    let _ = (d_arg.sqrt(), gammd2);
                    // `d_dumarg`: its computation is commented out
                    // in the C (read as zero, see above).
                    let d_dumarg = d_zero;
                    let d_dummy = d_dumarg.times(-1.0).plus(&d_gammd2).times(0.5);
                    let v = d_phi_min_vbs.times(-1.0).plus(&d_argv);
                    d_vdsat = d_dummy.plus(&v);
                    vdsat = f64::max(vdsat, 0.0);
                }
            } else {
                vdsat = (vgsx - vbin) / eta;
                d_vdsat = d_vbin.times(-1.0).plus(&d_vgsx).times(1.0 / eta);
                vdsat = f64::max(vdsat, 0.0);
            }
            if t.vmax > 0.0 {
                // Baum's scattering-velocity saturation.
                let gammd2 = gammad * gammad;
                let d_gammd2 = d_gammad.mult(&d_gammad);
                let v1 = (vgsx - vbin) / eta + phi_min_vbs;
                let d_v1 = d_vbin
                    .times(-1.0)
                    .plus(&d_vgsx)
                    .times(1.0 / eta)
                    .plus(&d_phi_min_vbs);
                let v2 = phi_min_vbs;
                let d_v2 = d_phi_min_vbs;
                let xv = t.vmax * eff_l / ueff;
                let d_xv = d_ueff.inv().times(t.vmax * eff_l);
                let a1 = gammad / 0.75;
                let d_a1 = d_gammad.times(4.0 / 3.0);
                let b1 = -2.0 * (v1 + xv);
                let d_b1 = d_v1.plus(&d_xv).times(-2.0);
                let c1 = -2.0 * gammad * xv;
                let d_c1 = d_gammad.mult(&d_xv).times(-2.0);
                let d1 = 2.0 * v1 * (v2 + xv) - v2 * v2 - 4.0 / 3.0 * gammad * sarg3;
                let mut d_d1 = d_gammad.mult(&d_sarg3).times(4.0 / 3.0);
                d_d1 = d_d1.plus(&d_v2.mult(&d_v2)).times(-1.0);
                d_d1 = d_d1.plus(&d_v2.plus(&d_xv).mult(&d_v1).times(2.0));
                let a = -b1;
                let d_a = d_b1.times(-1.0);
                let b = a1 * c1 - 4.0 * d1;
                let d_b = d_d1.times(-4.0).plus(&d_a1.mult(&d_c1));
                let c = -d1 * (a1 * a1 - 4.0 * b1) - c1 * c1;
                let d_dummy = d_b1.times(-4.0).plus(&d_a1.mult(&d_a1));
                let d_c = d_dummy.mult(&d_d1).plus(&d_c1.mult(&d_c1)).times(-1.0);
                let r = -a * a / 3.0 + b;
                let d_arr = d_a.mult(&d_a).times(-1.0 / 3.0).plus(&d_b);
                let s = 2.0 * a * a * a / 27.0 - a * b / 3.0 + c;
                let d_s = d_a
                    .cube()
                    .times(2.0 / 27.0)
                    .plus(&d_c)
                    .plus(&d_a.mult(&d_b).times(-1.0 / 3.0));
                let r3 = r * r * r;
                let d_r3 = d_arr.cube();
                let s2 = s * s;
                let d_s2 = d_s.mult(&d_s);
                let p = s2 / 4.0 + r3 / 27.0;
                let d_pee = d_s2.times(0.25).plus(&d_r3.times(1.0 / 27.0));
                let p0 = p.abs();
                let d_p0 = if p < 0.0 { d_pee.times(-1.0) } else { d_zero };
                let p2 = f64::sqrt(p0);
                let d_p2 = d_p0.sqrt();
                let (y3, d_y3);
                if p < 0.0 {
                    let ro = f64::exp(f64::ln(f64::sqrt(s2 / 4.0 + p0)) / 3.0);
                    let d_ro = d_s2.times(0.25).plus(&d_p0).pow(1.0 / 6.0);
                    let fi = f64::atan(-2.0 * p2 / s);
                    let d_fi = d_p2.div(&d_s).times(-2.0).atan();
                    y3 = 2.0 * ro * f64::cos(fi / 3.0) - a / 3.0;
                    let d_dummy = d_fi.times(1.0 / 3.0).cos();
                    d_y3 = d_ro.mult(&d_dummy).times(2.0).plus(&d_a.times(-1.0 / 3.0));
                } else {
                    let mut p3 = -s / 2.0 + p2;
                    let mut d_p3 = d_s.times(-0.5).plus(&d_p2);
                    p3 = f64::exp(f64::ln(p3.abs()) / 3.0);
                    if p3 < 0.0 {
                        d_p3 = d_p3.times(-1.0);
                    }
                    d_p3 = d_p3.pow(1.0 / 3.0);
                    let mut p4 = -s / 2.0 - p2;
                    let mut d_p4 = d_s.times(0.5).plus(&d_p2);
                    if p4 < 0.0 {
                        d_p4 = d_p4.times(-1.0);
                    }
                    p4 = f64::exp(f64::ln(p4.abs()) / 3.0);
                    d_p4 = d_p4.pow(1.0 / 3.0);
                    y3 = p3 + p4 - a / 3.0;
                    d_y3 = d_a.times(-1.0 / 3.0).plus(&d_p4).plus(&d_p3);
                }
                let a3 = f64::sqrt(a1 * a1 / 4.0 - b1 + y3);
                let d_a3 = d_a1
                    .mult(&d_a1)
                    .times(0.25)
                    .plus(&d_y3)
                    .plus(&d_b1.times(-1.0))
                    .sqrt();
                let b3 = f64::sqrt(y3 * y3 / 4.0 - d1);
                let d_b3 = d_y3.mult(&d_y3).times(0.25).plus(&d_d1.times(-1.0)).sqrt();
                let mut x4 = [0.0; 8];
                let mut d_x4 = [d_zero; 8];
                let mut iknt = 0;
                for i in 0..4 {
                    let a4 = a1 / 2.0 + SIG1[i] * a3;
                    let d_a4 = d_a1.times(0.5).plus(&d_a3.times(SIG1[i]));
                    let b4 = y3 / 2.0 + SIG2[i] * b3;
                    let d_b4 = d_y3.times(0.5).plus(&d_b3.times(SIG2[i]));
                    let delta4 = a4 * a4 / 4.0 - b4;
                    let d_delta4 = d_a4.mult(&d_a4).times(0.25).plus(&d_b4.times(-1.0));
                    if delta4 < 0.0 {
                        continue;
                    }
                    let tmp = f64::sqrt(delta4);
                    let d_tmp = d_delta4.sqrt();
                    x4[iknt] = -a4 / 2.0 + tmp;
                    d_x4[iknt] = d_a4.times(-0.5).plus(&d_tmp);
                    iknt += 1;
                    x4[iknt] = -a4 / 2.0 - tmp;
                    d_x4[iknt] = d_a4.times(-0.5).plus(&d_tmp);
                    iknt += 1;
                }
                let mut jknt = 0;
                let mut xvalid = 0.0;
                let mut d_xvalid = d_zero;
                for j in 0..iknt {
                    if x4[j] <= 0.0 {
                        continue;
                    }
                    let x = x4[j];
                    let poly4 = x * x * x * x + a1 * x * x * x + b1 * x * x + c1 * x + d1;
                    if poly4.abs() > 1.0e-6 {
                        continue;
                    }
                    jknt += 1;
                    if jknt <= 1 {
                        xvalid = x;
                        d_xvalid = d_x4[j];
                    }
                    if x > xvalid {
                        continue;
                    }
                    xvalid = x;
                    d_xvalid = d_x4[j];
                }
                if jknt > 0 {
                    vdsat = xvalid * xvalid - phi_min_vbs;
                    d_vdsat = d_xvalid.mult(&d_xvalid).plus(&d_phi_min_vbs.times(-1.0));
                }
                let _ = (gammd2, d_gammd2, v2);
            }

            // Effective channel length.
            let (mut bsarg, mut bodys) = (0.0, 0.0);
            let (mut d_bsarg, mut d_bodys) = (d_zero, d_zero);
            if lvds != 0.0 {
                gammad = gamasd;
                d_gammad = d_gamasd;
                if lvbs - vdsat <= 0.0 {
                    bsarg = f64::sqrt(vdsat + phi_min_vbs);
                    d_bsarg = d_vdsat.plus(&d_phi_min_vbs).sqrt();
                } else {
                    bsarg = sphi / (1.0 + 0.5 * (lvbs - vdsat) / t.t_phi);
                    let mut b = d_vdsat.times(-1.0);
                    b.value += lvbs;
                    b.d1_r += 1.0;
                    let mut b = b.times(0.5 / t.t_phi);
                    b.value += 1.0;
                    d_bsarg = b.inv().times(sphi);
                }
                bodys = bsarg * bsarg * bsarg - sarg3;
                d_bodys = d_bsarg.cube().plus(&d_sarg3.times(-1.0));
                if t.vmax <= 0.0 {
                    if t.nsub != 0.0 && xlamda <= 0.0 {
                        let argv = (lvds - vdsat) / 4.0;
                        let mut a = d_vdsat.times(-1.0);
                        a.value += lvds;
                        a.d1_r += 1.0;
                        let d_argv = a.times(0.25);
                        let sargv = f64::sqrt(1.0 + argv * argv);
                        let mut sv = d_argv.mult(&d_argv);
                        sv.value += 1.0;
                        let d_sargv = sv.sqrt();
                        let arg = f64::sqrt(argv + sargv);
                        let d_arg = d_sargv.plus(&d_argv).sqrt();
                        let xlfact = t.xd / (eff_l * lvds);
                        let d_xlfact = Dderivs { value: lvds, ..d_r }.inv().times(t.xd / eff_l);
                        xlamda = xlfact * arg;
                        d_xlamda = d_xlfact.mult(&d_arg);
                    }
                } else {
                    let xdv = t.xd / f64::sqrt(t.neff);
                    let xlv = t.vmax * xdv / (2.0 * ueff);
                    let d_xlv = d_ueff.inv().times(t.vmax * xdv * 0.5);
                    if t.nsub != 0.0 && xlamda <= 0.0 {
                        let mut argv = lvds - vdsat;
                        let mut d_argv = d_vdsat.times(-1.0);
                        d_argv.value += lvds;
                        d_argv.d1_r += 1.0;
                        if argv < 0.0 {
                            d_argv = d_zero;
                        }
                        argv = f64::max(argv, 0.0);
                        let xls = f64::sqrt(xlv * xlv + argv);
                        let d_xls = d_xlv.mult(&d_xlv).plus(&d_argv).sqrt();
                        let xlfact = xdv / (eff_l * lvds);
                        let mut xf = d_r;
                        xf.value += lvds;
                        let d_xlfact = xf.inv().times(xdv / eff_l);
                        xlamda = xlfact * (xls - xlv);
                        d_xlamda = d_xlv.times(-1.0).plus(&d_xls).mult(&d_xlfact);
                    }
                }
            }

            // `line610`: limit channel shortening at punch-through.
            let mut xwb = t.xd * sbiarg;
            let xld = eff_l - xwb;
            let mut clfact = 1.0 - xlamda * lvds;
            let mut d_clfact = Dderivs {
                value: lvds,
                d1_r: -1.0,
                ..d_r
            }
            .mult(&d_xlamda);
            d_clfact.value += 1.0;
            let mut xleff = eff_l * clfact;
            let deltal = xlamda * lvds * eff_l;
            let d_delta1 = Dderivs {
                value: eff_l * lvds,
                d1_r: eff_l,
                ..d_r
            }
            .mult(&d_xlamda);
            if t.nsub == 0.0 {
                xwb = 0.25e-6;
            }
            if xleff < xwb {
                xleff = xwb / (1.0 + (deltal - xld) / xwb);
                let mut xl = d_delta1;
                xl.value -= xld;
                let mut xl = xl.times(1.0 / xwb);
                xl.value += 1.0;
                let d_xleff = xl.inv().times(xwb);
                clfact = xleff / eff_l;
                d_clfact = d_xleff.times(1.0 / eff_l);
            }
            let _ = clfact;

            // Effective beta.
            let d_beta1 = d_ufact.div(&d_clfact).times(beta);
            gammad = gamasd;
            d_gammad = d_gamasd;
            let _ = gammad;

            if lvds <= 1.0e-10 {
                if lvgs <= von {
                    if t.nfs == 0.0 || oxide_cap == 0.0 {
                        d_cdrain.d1_q = 0.0;
                        d_cdrain.d2_q2 = 0.0;
                        d_cdrain.d3_q3 = 0.0;
                        break 'eval;
                    }
                    let d_dummy = d_gammad
                        .mult(&d_sarg)
                        .plus(&d_vbin)
                        .times(-1.0)
                        .plus(&d_von);
                    let mut d_mos2gds = d_beta1.mult(&d_dummy);
                    let mut d_dummy = d_von.times(-1.0).plus(&d_p);
                    d_dummy.value += lvgs;
                    let d_dummy = d_dummy.mult(&d_argg).exp();
                    d_mos2gds = d_mos2gds.mult(&d_dummy);
                    d_cdrain.d1_r = d_mos2gds.value;
                    d_cdrain.d2_r2 = d_mos2gds.d1_r;
                    d_cdrain.d3_r3 = d_mos2gds.d2_r2;
                    break 'eval;
                }
                let d_mos2gds = d_gammad
                    .mult(&d_sarg)
                    .plus(&d_vbin)
                    .times(-1.0)
                    .mult(&d_beta1);
                d_cdrain.d1_r = d_mos2gds.value;
                d_cdrain.d2_r2 = d_mos2gds.d1_r;
                d_cdrain.d3_r3 = d_mos2gds.d2_r2;
                break 'eval;
            }

            if lvgs <= von {
                // Subthreshold.
                if vdsat <= 0.0 {
                    d_cdrain.d1_r = 0.0;
                    d_cdrain.d2_r2 = 0.0;
                    d_cdrain.d3_r3 = 0.0;
                    break 'eval;
                }
                let d_vdson = if vdsat <= lvds {
                    d_vdsat
                } else {
                    Dderivs { value: lvds, ..d_r }
                };
                if lvds > vdsat {
                    barg = bsarg;
                    d_barg = d_bsarg;
                    body = bodys;
                    d_body = d_bodys;
                }
                let _ = (barg, d_barg, body);
                let d_cdson = d_gammad.mult(&d_body).times(-1.0 / 1.5);
                let d_dummy = d_vdson
                    .times(0.5 * eta)
                    .plus(&d_vbin)
                    .times(-1.0)
                    .plus(&d_von)
                    .mult(&d_vdson)
                    .plus(&d_cdson);
                let d_cdson = d_dummy.mult(&d_beta1);
                let mut d_expg = d_von.times(-1.0);
                d_expg.value += lvgs;
                d_expg.d1_p += 1.0;
                let d_expg = d_expg.mult(&d_argg).exp();
                d_cdrain = d_cdson.mult(&d_expg);
                break 'eval;
            }

            // `line900`.
            if lvds <= vdsat {
                // Linear region.
                let mut c = Dderivs {
                    value: eta * lvds * 0.5,
                    d1_r: 0.5 * eta,
                    ..d_r
                }
                .plus(&d_vbin)
                .times(-1.0);
                c.value += lvgs;
                c.d1_p += 1.0;
                let c = c.mult(&Dderivs { value: lvds, ..d_r });
                let d_dummy = d_gammad.mult(&d_body).times(-1.0 / 1.5);
                d_cdrain = c.plus(&d_dummy).mult(&d_beta1);
            } else {
                // Saturation region.
                let mut c = d_vdsat.times(0.5 * eta).plus(&d_vbin).times(-1.0);
                c.value += lvgs;
                c.d1_p += 1.0;
                let c = c.mult(&d_vdsat);
                let d_dummy = d_gammad.mult(&d_bodys).times(-1.0 / 1.5);
                d_cdrain = c.plus(&d_dummy).mult(&d_beta1);
            }
            // `goto doneval`: skip the `line1050` zeroing.
            doneval = true;
        }
        if !doneval {
            // `line1050`: everything but the vds terms is zeroed.
            d_cdrain = Dderivs {
                d1_r: d_cdrain.d1_r,
                d2_r2: d_cdrain.d2_r2,
                d3_r3: d_cdrain.d3_r3,
                ..d_zero
            };
        }
        let _ = von;
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
            (vgst, vdsat),
            t.t_phi,
            oxide_cap,
            &junctions,
            &DrainDerivs::from_dderivs(&d_cdrain),
            gbs,
            gbd,
        )
    }
}

impl crate::devices::Device for Mos2I {
    fn par_load(&self) -> bool {
        true
    }

    /// `MOS2getic` (`mos2ic.c`): VBS, VDS, VGS not given
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

    /// `Mos2convTest`: predicted vs computed drain and bulk
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

        // Bulk diodes, in the ngspice form: flat leakage below
        // −3·vt, clamped exponential above, gmin folded into the
        // current (`corpus/KNOWN_DIFFS.md`).
        const MAX_EXP_ARG: f64 = 709.0;
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

        self.mode = if vds >= 0.0 { 1.0 } else { -1.0 };
        let mode = self.mode;

        // Grove-Frohman evaluation.
        let (cdrain, von, vdsat) = self.eval(vbs, vbd, vgs, vgd, vds, beta, oxide_cap, eff_l, vt);
        self.von = typ * von;
        self.vdsat = typ * vdsat;
        let (gm, gds, gmbs) = (self.gm, self.gds, self.gmbs);

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
        // Saved for the convergence test (`mos2conv.c`).
        self.cd = cd;
        self.cbs = cbs;
        self.cbd = cbd;
        ctx.add_rhs(self.gate, -(typ * (ceqgs + ceqgb + ceqgd)));
        ctx.add_rhs(self.bulk, -(ceqbs + ceqbd - typ * ceqgb));
        ctx.add_rhs(self.d_p, ceqbd - cdreq + typ * ceqgd);
        ctx.add_rhs(self.s_p, cdreq + ceqbs + typ * ceqgs);

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

    /// `MOS2noise` (ngspice-47, default `NLEV`): see
    /// [`crate::mos1::mos_noise`].
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
            cd: self.cd,
            kf: self.t.kf,
            af: self.t.af,
            nlev: self.t.nlev,
            gdsnoi: self.t.gdsnoi,
            w: self.geom.w,
            eff_l: self.geom.l - 2.0 * self.t.lat_diff,
            cox: self.t.oxide_cap_factor,
            temp: self.t.temp,
            vgs: ctx.states.get(0, self.st + VGS),
            vds: ctx.states.get(0, self.st + VDS),
            mode: self.mode,
            von: self.von,
            vdsat: self.vdsat,
            typ: self.t.type_,
            t_transconductance: self.t.t_transconductance,
        };
        crate::mos1::mos_noise(ctx, &n)
    }

    /// `MOS2dSetup`: see [`Mos2I::disto_coeffs`].
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        self.dist = self.disto_coeffs(ctx.sol, ctx.gmin);
        Ok(())
    }

    /// `MOS2disto` (`dev/mos2/mos2dist.c`, the MOS1 file renamed).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        mos_disto_load(&self.dist, (self.gate, self.bulk, self.d_p, self.s_p), ctx);
    }

    /// `MOS2acLoad` (same stamps as MOS1's).
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

    /// `MOS2pzLoad` (`mos2pzld.c`, the MOS1 file renamed): see
    /// [`meyer_pz_load`].
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        let t = &self.t;
        let st = self.st;
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        let eff_l = self.geom.l - 2.0 * t.lat_diff;
        let caps = MeyerPzCaps {
            capgs: 2.0 * ctx.states.get(0, st + CAPGS) + t.cgso * self.geom.w,
            capgd: 2.0 * ctx.states.get(0, st + CAPGD) + t.cgdo * self.geom.w,
            capgb: 2.0 * ctx.states.get(0, st + CAPGB) + t.cgbo * eff_l,
            capbd: self.capbd,
            capbs: self.capbs,
        };
        let g = MosPzConductances {
            xnrm,
            xrev,
            gm: self.gm,
            gds: self.gds,
            gmbs: self.gmbs,
            gbd: self.gbd,
            gbs: self.gbs,
            gdpr: t.drain_conductance,
            gspr: t.source_conductance,
        };
        meyer_pz_load(ctx, &self.s, &caps, &g);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mosamp2's model card, at TNOM.
    fn amp_model() -> Mos2Model {
        let mut m = Mos2Model::new(true);
        m.nsub = Some(2.2e15);
        m.u0 = Some(575.0);
        m.ucrit = 49e3;
        m.uexp = 0.1;
        m.tox = Some(0.11e-6);
        m.xj = 2.95e-6;
        m.ld = 2.4485e-6;
        m.nss = Some(3.2e10);
        m.kp = Some(2e-5);
        m.phi = Some(0.6);
        m.cgso = 1.5e-9;
        m.cgdo = 1.5e-9;
        m.cbd = Some(4.5e-15);
        m.cbs = Some(4.5e-15);
        m
    }

    #[test]
    fn temp_derives_gamma_vto_xd_from_nsub() {
        let g = MosGeom {
            l: 25.4e-6,
            w: 88.9e-6,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        };
        let t = Mos2Temp::new("m1", &amp_model(), &g, REFTEMP, REFTEMP).unwrap();
        // GAMMA = sqrt(2·eps·q·Nsub)/Cox with Cox = 3.9·e0/tox.
        let cox = 3.9 * 8.854214871e-12 / 0.11e-6;
        let gamma = f64::sqrt(2.0 * 11.70 * 8.854214871e-12 * CHARGE * 2.2e15 * 1e6) / cox;
        assert!((t.gamma - gamma).abs() < 1e-12);
        // XD = sqrt(2·epsSil/(q·Nsub)).
        let xd = f64::sqrt(2.0 * EPSSIL / (CHARGE * 2.2e15 * 1e6));
        assert!((t.xd - xd).abs() < 1e-18);
        // KP was given, so tTransconductance is the given value at TNOM.
        assert!((t.t_transconductance - 2e-5).abs() < 1e-18);
        // PHI given → tPhi = PHI at TNOM.
        assert!((t.t_phi - 0.6).abs() < 1e-12);
        // VTO computed from NSUB/NSS/TPG must be finite and sensible.
        assert!(t.t_vto.is_finite());
        assert!(t.t_vto.abs() < 5.0);
    }

    #[test]
    fn nsub_below_ni_is_fatal() {
        let mut m = Mos2Model::new(true);
        m.nsub = Some(1e9);
        let g = MosGeom {
            l: 1e-5,
            w: 1e-5,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        };
        let e = Mos2Temp::new("m1", &m, &g, REFTEMP, REFTEMP).unwrap_err();
        assert!(e.contains("Nsub < Ni"));
    }
}
