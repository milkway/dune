//! MOSFET level 6 — Sakurai–Newton's n-th power-law model from the
//! 3f5 (`dev/mos6/mos6load.c`, `mos6temp.c`, `mos6set.c`,
//! `mos6trunc.c`; author Takayasu Sakurai, 1989). Saturation current
//! `KC·(vgs−von)^NC` and saturation voltage `KV·(vgs−von)^NV`, with
//! `LAMBDA0−LAMBDA1·vbs` channel-length modulation and the
//! `GAMMA1`/`SIGMA` threshold corrections. The scaffolding (bias
//! ladder, limiting, bulk diodes, depletion and Meyer charges,
//! companions, stamps, 17 states, 22 slots) is the MOS1's.
//!
//! The 3f5 has no AC load for MOS6 (`DEVacLoad` is null), so
//! [`crate::devices::Device::load_ac`] keeps its no-op default.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::diode::pnjlim;
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{MosGeom, fetlim, limvds, qmeyer};

/// `MAX_EXP_ARG` (`hw_ieee.h`).
const MAX_EXP_ARG: f64 = 709.0;

/// `.model … nmos(level=6 …)/pmos(level=6 …)` parameters, 3f5 names
/// and defaults (`mos6.c` keyword table; defaults from `mos6set.c`).
#[derive(Clone, Debug)]
pub struct Mos6Model {
    /// `NMOS` (+1) or `PMOS` (−1).
    pub nmos: bool,
    /// `VTO`/`VT0`, default 0, or computed from `NSUB`/`NSS`/`TPG`.
    pub vto: Option<f64>,
    /// `KV`: saturation-voltage factor, default 2.
    pub kv: f64,
    /// `NV`: saturation-voltage exponent, default 0.5.
    pub nv: f64,
    /// `KC`: saturation-current factor, default 5e-5, or
    /// `0.5·U0·COX` when `TOX` is given.
    pub kc: Option<f64>,
    /// `NC`: saturation-current exponent, default 1.
    pub nc: f64,
    /// `NVTH`/`PS`: parsed by the 3f5 but never used by its load.
    pub nvth: f64,
    pub ps: f64,
    /// `GAMMA`, default 0, or computed from `NSUB`.
    pub gamma: Option<f64>,
    /// `GAMMA1`: bulk threshold slope, default 0.
    pub gamma1: f64,
    /// `SIGMA`: static feedback, default 0 (dead in the 3f5's `von`
    /// due to a stray semicolon; ngspice fixed it and so do we —
    /// `corpus/KNOWN_DIFFS.md`).
    pub sigma: f64,
    /// `PHI`, default 0.6, or from `NSUB`.
    pub phi: Option<f64>,
    /// `LAMBDA`: alias that seeds `LAMBDA0` when that is not given.
    pub lambda: Option<f64>,
    /// `LAMBDA0`/`LAMBDA1`, default 0.
    pub lambda0: Option<f64>,
    pub lambda1: f64,
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
    /// `MJSW`, default 0.5.
    pub mjsw: f64,
    /// `JS`, per m², default 0.
    pub js: f64,
    /// `TOX`, meters.
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
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `TNOM` in kelvin.
    pub tnom: Option<f64>,
}

impl Mos6Model {
    pub fn new(nmos: bool) -> Self {
        Mos6Model {
            nmos,
            vto: None,
            kv: 2.0,
            nv: 0.5,
            kc: None,
            nc: 1.0,
            nvth: 0.5,
            ps: 0.0,
            gamma: None,
            gamma1: 0.0,
            sigma: 0.0,
            phi: None,
            lambda: None,
            lambda0: None,
            lambda1: 0.0,
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

    /// Same prime-node rule as MOS1 (`mos6set.c`).
    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rd.unwrap_or(0.0) != 0.0 || (self.rsh.unwrap_or(0.0) != 0.0 && squares != 0.0)
    }
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

/// Model- and temperature-derived values (`MOS6temp`).
#[derive(Debug)]
pub(crate) struct Mos6Temp {
    pub type_: f64,
    pub vt: f64,
    pub lat_diff: f64,
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    pub oxide_cap_factor: f64,
    pub gamma: f64,
    pub gamma1: f64,
    pub sigma: f64,
    pub kv: f64,
    pub nv: f64,
    pub nc: f64,
    pub lambda0: f64,
    pub lambda1: f64,
    pub mj: f64,
    pub mjsw: f64,
    /// `MOS6tKc` (`KC / ratio^1.5`).
    pub t_kc: f64,
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

impl Mos6Temp {
    pub fn new(
        name: &str,
        m: &Mos6Model,
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

        // Model preprocessing (`mos6temp.c`): KC from `0.5·U0·COX`,
        // PHI/GAMMA/VTO from NSUB — same structure as MOS1's.
        let mut kc = m.kc;
        let mut phi = m.phi;
        let mut gamma = m.gamma;
        let mut vto = m.vto;
        let oxide_cap_factor = match m.tox {
            Some(tox) if tox != 0.0 => {
                let ocf = 3.9 * 8.854214871e-12 / tox;
                if kc.is_none() {
                    let u0 = m.u0.unwrap_or(600.0);
                    kc = Some(0.5 * u0 * ocf * 1e-4 /* m²/cm² */);
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
                            let vfb = wkfngs - nss * 1e4 * CHARGE / ocf;
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
        let kc = kc.unwrap_or(5e-5);
        let phi = phi.unwrap_or(0.6);
        let gamma = gamma.unwrap_or(0.0);
        let vto = vto.unwrap_or(0.0);
        // `LAMBDA` seeds `LAMBDA0` when the latter is absent.
        let lambda0 = m.lambda0.or(m.lambda).unwrap_or(0.0);

        // Per-instance temperature block (same formulas as MOS1).
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
        let t_kc = kc / ratio4;
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

        Ok(Mos6Temp {
            type_: typ,
            vt,
            lat_diff: m.ld,
            cgso: m.cgso,
            cgdo: m.cgdo,
            cgbo: m.cgbo,
            oxide_cap_factor,
            gamma,
            gamma1: m.gamma1,
            sigma: m.sigma,
            kv: m.kv,
            nv: m.nv,
            nc: m.nc,
            lambda0,
            lambda1: m.lambda1,
            mj: m.mj,
            mjsw: m.mjsw,
            t_kc,
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

// State offsets (`mos6defs.h` — identical to MOS1's).
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
pub(crate) const MOS6_NUM_STATES: usize = 17;

// Slot order (matches `mos6set.c`'s TSTALLOC list — same as MOS1).
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

pub(crate) struct Mos6I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Mos6Temp,
    geom: MosGeom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    st: usize,
    s: [Slot; 22],
    // Values that persist between iterations (`MOS6instance`).
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
}

impl Mos6I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],  // d g s b
        primes: [Var; 2], // d' s'
        t: Mos6Temp,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64), // (vds, vgs, vbs)
        st: usize,
    ) -> Self {
        Mos6I {
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
        }
    }
}

impl crate::devices::Device for Mos6I {
    /// `MOS6convTest` (same shape as MOS1's).
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
        let betac = t.t_kc * self.geom.w / eff_l;
        let oxide_cap = t.oxide_cap_factor * eff_l * self.geom.w;

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

        // Bulk diodes, in the ngspice form (`corpus/KNOWN_DIFFS.md`).
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

        // Sakurai-Newton n-th power drain current (`mos6load.c`,
        // with ngspice's fix of the stray semicolon that killed the
        // SIGMA term in `von`).
        let cdrain;
        let (gm, gds, gmbs);
        let (von, vdsat);
        {
            let vbsvbd = if mode == 1.0 { vbs } else { vbd };
            let sarg = if vbsvbd <= 0.0 {
                (t.t_phi - vbsvbd).sqrt()
            } else {
                let s = t.t_phi.sqrt();
                f64::max(0.0, s - vbsvbd / (s + s))
            };
            let vdshere = vds * mode;
            von = t.t_vbi * typ + t.gamma * sarg - t.gamma1 * vbsvbd - t.sigma * vdshere;
            let vgon = (if mode == 1.0 { vgs } else { vgd }) - von;
            if vgon <= 0.0 {
                // Cutoff region.
                vdsat = 0.0;
                cdrain = 0.0;
                gm = 0.0;
                gds = 0.0;
                gmbs = 0.0;
            } else {
                // The 3f5 leaves `vonbm` uninitialized when
                // `sarg <= 0`; we use 0.
                let mut vonbm = 0.0;
                if sarg > 0.0 {
                    vonbm = if vbsvbd <= 0.0 {
                        t.gamma1 + t.gamma / (sarg + sarg)
                    } else {
                        t.gamma1 + t.gamma / 2.0 / t.t_phi.sqrt()
                    };
                }
                let lnvgon = vgon.ln();
                vdsat = t.kv * f64::exp(lnvgon * t.nv);
                let idsat = betac * f64::exp(lnvgon * t.nc);
                let lambda = t.lambda0 - t.lambda1 * vbsvbd;
                // Saturation region first…
                let mut cdr = idsat * (1.0 + lambda * vdshere);
                let mut gm_l = cdr * t.nc / vgon;
                let mut gds_l = gm_l * t.sigma + idsat * lambda;
                let mut gmbs_l = gm_l * vonbm - idsat * t.lambda1 * vdshere;
                if vdsat > vdshere {
                    // …then the linear-region correction.
                    let vdst = vdshere / vdsat;
                    let vdst2 = (2.0 - vdst) * vdst;
                    let vdstg = -vdst * t.nv / vgon;
                    let ivdst1 = cdr * (2.0 - vdst - vdst);
                    cdr *= vdst2;
                    gm_l = gm_l * vdst2 + ivdst1 * vdstg;
                    gds_l = gds_l * vdst2 + ivdst1 * (1.0 / vdsat + vdstg * t.sigma);
                    gmbs_l = gmbs_l * vdst2 + ivdst1 * vdstg * vonbm;
                }
                cdrain = cdr;
                gm = gm_l;
                gds = gds_l;
                gmbs = gmbs_l;
            }
        }
        self.von = typ * von;
        self.vdsat = typ * vdsat;
        self.gm = gm;
        self.gds = gds;
        self.gmbs = gmbs;

        let mut cd = mode * cdrain - cbd;

        let smsig = ctx.mode == InitMode::Smsig;
        if ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp || smsig {
            // Bulk junction depletion charge and capacitance.
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
        // Saved for the convergence test (`mos6conv.c`).
        self.cd = cd;
        self.cbs = cbs;
        self.cbd = cbd;
        ctx.rhs[self.gate] -= typ * (ceqgs + ceqgb + ceqgd);
        ctx.rhs[self.bulk] -= ceqbs + ceqbd - typ * ceqgb;
        ctx.rhs[self.d_p] += ceqbd - cdreq + typ * ceqgd;
        ctx.rhs[self.s_p] += cdreq + ceqbs + typ * ceqgs;

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

    /// The 3f5 has no `mos6acld.c`; a MOS6 contributes nothing to
    /// the AC matrix, exactly like the C's null `DEVacLoad`.
    fn load_ac(&mut self, _ctx: &mut AcCtx) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// mos6inv's N10L5 model card, at TNOM.
    fn n10l5() -> Mos6Model {
        let mut m = Mos6Model::new(true);
        m.kc = Some(3.8921e-5);
        m.nc = 1.1739;
        m.kv = 0.91602;
        m.nv = 0.87225;
        m.lambda0 = Some(0.013333);
        m.lambda1 = 0.0046901;
        m.vto = Some(0.69486);
        m.gamma = Some(0.60309);
        m.phi = Some(1.0);
        m.tox = Some(1.98e-8);
        m.ld = 0.1e-6;
        m.nsub = Some(4.9999999e16);
        m.nss = Some(0.0);
        m.cj = Some(4.091e-4);
        m.mj = 0.307;
        m.pb = 1.0;
        m.cjsw = Some(3.078e-10);
        m.mjsw = 1.0e-2;
        m.cgso = 3.93e-10;
        m.cgdo = 3.93e-10;
        m.tpg = Some(1.0);
        m
    }

    #[test]
    fn temp_uses_given_parameters_at_tnom() {
        let g = MosGeom {
            l: 1e-6,
            w: 5e-6,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        };
        let t = Mos6Temp::new("mn11", &n10l5(), &g, REFTEMP, REFTEMP).unwrap();
        assert!((t.t_kc - 3.8921e-5).abs() < 1e-12);
        assert!((t.t_phi - 1.0).abs() < 1e-12);
        assert!((t.t_vto - 0.69486).abs() < 1e-9);
        assert!((t.lambda0 - 0.013333).abs() < 1e-12);
        assert_eq!(t.drain_conductance, 0.0);
    }

    #[test]
    fn lambda_alias_seeds_lambda0() {
        let mut m = Mos6Model::new(true);
        m.lambda = Some(0.02);
        let g = MosGeom {
            l: 1e-6,
            w: 1e-6,
            ad: 0.0,
            as_: 0.0,
            pd: 0.0,
            ps: 0.0,
            nrd: 1.0,
            nrs: 1.0,
        };
        let t = Mos6Temp::new("m1", &m, &g, REFTEMP, REFTEMP).unwrap();
        assert!((t.lambda0 - 0.02).abs() < 1e-15);
        // An explicit LAMBDA0 wins.
        m.lambda0 = Some(0.05);
        let t = Mos6Temp::new("m1", &m, &g, REFTEMP, REFTEMP).unwrap();
        assert!((t.lambda0 - 0.05).abs() < 1e-15);
    }
}
