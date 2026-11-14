// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4.c, bsim4def.h, b4set.c (matrix allocation), b4cvtest.c, b4trunc.c, b4getic.c and BSIM4temp's size-knot handling (b4temp.c) as distributed with ngspice-47
// (src/spicelib/devices/bsim4/). Changes from the original: the C was
// translated into Rust statement by statement; global state, the OpenMP and
// KLU paths and the bypass were removed; ngspice's circuit and matrix data
// structures were replaced by Dune's; diagnostics are returned to the caller
// instead of printed. See docs/bsim4.md for the full list of changes.
//
// The original copyright notice and licence of the translated files follow.
//
// ******************************************************************************
// *  BSIM4 4.8.3 released on 05/19/2025                                        *
// *  BSIM4 Model Equations                                                     *
// ******************************************************************************
//
// ******************************************************************************
// *  Copyright (c) 2025 University of California                               *
// *                                                                            *
// *  Project Directors: Prof. Sayeef Salahuddin and Prof. Chenming Hu          *
// *  Developers list: https://www.bsim.berkeley.edu/models/bsim4/auth_bsim4/   *
// ******************************************************************************
//
// Licensed under Educational Community License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain a copy of the license at
// http://opensource.org/licenses/ECL-2.0
// Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the License for the specific language governing permissions and limitations
// under the License.
//
// BSIM4 model is supported by the members of Silicon Integration Initiative's Compact Model Coalition. A link to the most recent version of this
// standard can be found at: http://www.si2.org/cmc

//! BSIM4 4.8.3 — ngspice-47's `LEVEL=14`/`54` MOSFET
//! (`spicelib/devices/bsim4/`, Educational Community License 2.0). The
//! 3f5 predates BSIM4, so ngspice-47 is both the contract and the
//! oracle. Only the 4.8.3 code exists here: cards declaring `VERSION`
//! 4.8.x run natively, cards declaring 4.0–4.7 (which ngspice routes to
//! its older BSIM4 code bases) run on the 4.8.3 equations with a note,
//! and anything else is refused like ngspice refuses it (see
//! `docs/bsim4.md`, "Política de versão").
//!
//! Layout mirrors the C: [`params`] is the generated parameter table and
//! the C structs (`sBSIM4model`, `bsim4SizeDependParam`,
//! `sBSIM4instance`); `setup` is `b4set.c`, `temp` is `b4temp.c`, `geo`
//! is `b4geo.c`, `check` is `b4check.c`, `load` is `b4ld.c`, `acld`,
//! `pzld` and `noise` are `b4acld.c`, `b4pzld.c` and `b4noi.c`; this file
//! holds the device shell (`b4cvtest.c`, `b4trunc.c`, `b4getic.c`, the
//! matrix allocation of `BSIM4setup`) and the per-model knot list of
//! `BSIM4temp`.

mod acld;
mod check;
mod geo;
mod load;
mod noise;
mod params;
mod pzld;
mod setup;
mod temp;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, TRASH, Var};

pub use params::Bsim4Model;
pub(crate) use params::{Bsim4Here, Bsim4Size};

// ---------------------------------------------------------------------
// Constants of the BSIM4 files (`b4ld.c`, `b4temp.c`, `b4set.c`).

const MAX_EXPL: f64 = 2.688117142e+43;
const MIN_EXPL: f64 = 3.720075976e-44;
const EXPL_THRESHOLD: f64 = 100.0;
const MAX_EXP: f64 = 5.834617425e14;
const MIN_EXP: f64 = 1.713908431e-15;
const EXP_THRESHOLD: f64 = 34.0;
const EPS0: f64 = 8.85418e-12;
const EPSSI: f64 = 1.03594e-10;
const CHARGE_Q: f64 = 1.60219e-19;
const DELTA_1: f64 = 0.02;
const DELTA_2: f64 = 0.02;
const DELTA_3: f64 = 0.02;
const DELTA_4: f64 = 0.02;
const MM: f64 = 3.0;
const KB: f64 = 1.3806226e-23;
const KBOQ: f64 = 8.617087e-5;
/// The BSIM files' own `PI` (`3.141592654`), not `std::f64::consts::PI`.
#[allow(clippy::approx_constant)]
const PI: f64 = 3.141592654;
const DELTA: f64 = 1.0E-9;
/// `ScalingFactor` of `b4ld.c`.
const SCALING_FACTOR: f64 = 1.0e-9;
const NMOS: i32 = 1;
const PMOS: i32 = -1;
/// `N_MINLOG` (`noisedef.h`).
const N_MINLOG: f64 = 1e-38;
// ngspice's `const.h` names, with Dune's 3f5 values (D-0002).
use crate::consts::{BOLTZ, CHARGE, CTOK as CONST_CTOK, ROOT2, VT0};

// `CKTmode` bits (`cktdefs.h`).
const MODETRAN: i32 = 0x1;
const MODEAC: i32 = 0x2;
const MODEDCOP: i32 = 0x10;
const MODETRANOP: i32 = 0x20;
const MODEDCTRANCURVE: i32 = 0x40;
#[allow(dead_code)]
const MODEDC: i32 = 0x70;
const MODEINITFLOAT: i32 = 0x100;
const MODEINITJCT: i32 = 0x200;
const MODEINITFIX: i32 = 0x400;
const MODEINITSMSIG: i32 = 0x800;
const MODEINITTRAN: i32 = 0x1000;
const MODEINITPRED: i32 = 0x2000;
const MODEUIC: i32 = 0x10000;

// State offsets (`bsim4def.h`).
const ST_VBD: usize = 0;
const ST_VBS: usize = 1;
const ST_VGS: usize = 2;
const ST_VDS: usize = 3;
const ST_VDBS: usize = 4;
const ST_VDBD: usize = 5;
const ST_VSBS: usize = 6;
const ST_VGES: usize = 7;
const ST_VGMS: usize = 8;
const ST_VSES: usize = 9;
const ST_VDES: usize = 10;
const ST_QB: usize = 11;
const ST_CQB: usize = 12;
const ST_QG: usize = 13;
const ST_CQG: usize = 14;
const ST_QD: usize = 15;
const ST_CQD: usize = 16;
const ST_QGMID: usize = 17;
const ST_CQGMID: usize = 18;
const ST_QBS: usize = 19;
const ST_CQBS: usize = 20;
const ST_QBD: usize = 21;
const ST_CQBD: usize = 22;
const ST_QCHEQ: usize = 23;
const ST_CQCHEQ: usize = 24;
const ST_QCDUMP: usize = 25;
const ST_CQCDUMP: usize = 26;
const ST_QDEF: usize = 27;
const ST_QS: usize = 28;
/// `BSIM4numStates`.
pub(crate) const BSIM4_NUM_STATES: usize = 29;

/// BSIM4 noise generator names (`BSIM4nNames`, `b4noi.c`): ngspice prints
/// them as `onoise.<inst>.rd` etc.
pub(crate) const BSIM4_NOISE_SUFFIXES: &[&str] = &[
    ".rd", ".rs", ".rg", ".rbps", ".rbpd", ".rbpb", ".rbsb", ".rbdb", ".id", ".1overf", ".igs",
    ".igd", ".igb", ".corl", "",
];

// ---------------------------------------------------------------------
// The C library calls the translated code makes, with the C semantics.

#[inline]
fn exp(x: f64) -> f64 {
    x.exp()
}
#[inline]
fn log(x: f64) -> f64 {
    x.ln()
}
#[inline]
fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
#[inline]
fn pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}
#[inline]
fn fabs(x: f64) -> f64 {
    x.abs()
}
#[inline]
#[allow(dead_code)]
fn atan(x: f64) -> f64 {
    x.atan()
}
/// The `MAX` macro: `((a) > (b) ? (a) : (b))`.
#[inline]
fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}
/// The three-argument `DEXP(A, B, C)` of `b4ld.c`: (B, C).
#[inline]
fn dexp3(a: f64) -> (f64, f64) {
    if a > EXP_THRESHOLD {
        (MAX_EXP * (1.0 + a - EXP_THRESHOLD), MAX_EXP)
    } else if a < -EXP_THRESHOLD {
        (MIN_EXP, 0.0)
    } else {
        let b = a.exp();
        (b, b)
    }
}
/// The two-argument `DEXP(A, B)` of `b4temp.c`.
#[inline]
fn dexp2(a: f64) -> f64 {
    if a > EXP_THRESHOLD {
        MAX_EXP * (1.0 + a - EXP_THRESHOLD)
    } else if a < -EXP_THRESHOLD {
        MIN_EXP
    } else {
        a.exp()
    }
}
/// `DEVfetlim`.
#[inline]
fn fetlim(vnew: f64, vold: f64, vto: f64) -> f64 {
    crate::mos1::fetlim(vnew, vold, vto)
}
/// `DEVlimvds`.
#[inline]
fn limvds(vnew: f64, vold: f64) -> f64 {
    crate::mos1::limvds(vnew, vold)
}
/// `DEVpnjlim` with the C's integer `icheck`.
#[inline]
fn pnjlim_i(vnew: f64, vold: f64, vt: f64, vcrit: f64, icheck: &mut i32) -> f64 {
    let mut c = false;
    let v = crate::diode::pnjlim(vnew, vold, vt, vcrit, &mut c);
    *icheck = c as i32;
    v
}

use geo::{pa_eff_geo, rdseff_geo};

/// The `CKTmode` bits of one load.
fn mode_bits(ctx: &LoadCtx) -> i32 {
    let mut m = match ctx.kind {
        AnKind::DcOp => MODEDCOP,
        AnKind::TranOp => MODETRANOP,
        AnKind::DcSweep => MODEDCTRANCURVE,
        AnKind::Tran => MODETRAN,
    };
    m |= match ctx.mode {
        InitMode::Jct => MODEINITJCT,
        InitMode::Fix => MODEINITFIX,
        InitMode::Float => MODEINITFLOAT,
        InitMode::Tran => MODEINITTRAN,
        InitMode::Pred => MODEINITPRED,
        InitMode::Smsig => MODEINITSMSIG,
    };
    if ctx.uic {
        m |= MODEUIC;
    }
    m
}

// ---------------------------------------------------------------------
// The public card.

impl Bsim4Model {
    /// An empty card of the given polarity (`nmos`/`pmos`): nothing
    /// given, every default still to be applied by `BSIM4setup`.
    pub fn new(nmos: bool) -> Self {
        let mut m = Bsim4Model {
            type_: if nmos { NMOS } else { PMOS },
            ..Default::default()
        };
        m.given.type_ = true;
        m
    }

    /// `true` for an `nmos` card.
    pub fn is_nmos(&self) -> bool {
        self.type_ == NMOS
    }

    /// The card name, used in messages (`BSIM4modName`).
    pub fn set_name(&mut self, name: &str) {
        self.mod_name = name.to_string();
    }

    /// Set one model parameter by its deck keyword (lowercase, the
    /// `BSIM4mPTable` names of `b4.c`, aliases included), marking it
    /// given (`BSIM4mParam`). Integer selectors round like ngspice's
    /// `INPgetValue` (`floor(0.5 + v)`); `tnom` is in °C; densities
    /// above the C's thresholds are taken in cm⁻³ like `b4mpar.c`.
    /// Unknown keywords are an error.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        if name == "nmos" || name == "pmos" {
            self.type_ = if name == "nmos" { NMOS } else { PMOS };
            self.given.type_ = true;
            return Ok(());
        }
        if self.set_param(name, value) {
            Ok(())
        } else {
            Err(format!("unknown BSIM4 model parameter `{name}`"))
        }
    }

    /// `VERSION=`, kept as given.
    pub fn set_version(&mut self, version: &str) {
        self.version = version.to_string();
        self.given.version = true;
    }

    /// Why Dune cannot run this card faithfully, if it cannot: a
    /// selector value whose code is not ported.
    pub(crate) fn unsupported(&self) -> Option<String> {
        if self.given.tnoi_mod && self.tnoi_mod == 2 {
            return Some(
                "BSIM4 TNOIMOD = 2 (correlated drain/induced-gate noise, ngspice's NevalSrc2) is not implemented"
                    .into(),
            );
        }
        if self.given.geo_mod && !(0..=10).contains(&self.geo_mod) {
            return Some(format!(
                "BSIM4 GEOMOD = {} is outside 0..10 (b4geo.c matches none)",
                self.geo_mod
            ));
        }
        None
    }
}

/// `true` for a numeric instance keyword of `BSIM4pTable` (`b4.c`).
pub(crate) fn is_instance_keyword(kw: &str) -> bool {
    params::INST_KEYWORDS.contains(&kw)
}

/// Why Dune cannot run this instance faithfully, if it cannot.
pub(crate) fn unsupported_instance(s: &crate::circuit::Bsim4) -> Option<String> {
    if let Some(g) = s.param("geomod")
        && !(0.0..=10.0).contains(&(0.5 + g).floor())
    {
        return Some(format!(
            "BSIM4 GEOMOD = {g} is outside 0..10 (b4geo.c matches none)"
        ));
    }
    None
}

// ---------------------------------------------------------------------
// The device.

/// One BSIM4 instance: the C's `sBSIM4instance` plus the shared model
/// card and size-dependent parameter set.
pub(crate) struct Bsim4I {
    h: Bsim4Here,
    model: Arc<Bsim4Model>,
    pp: Arc<Bsim4Size>,
    st: usize,
    /// `CKTtemp`.
    ckt_temp: f64,
    /// The `CKTmode` bits of the last load (read by the convergence
    /// test, which ngspice runs with the same mode).
    last_mode: i32,
}

impl Bsim4I {
    /// An instance whose `here` has been set up (`BSIM4setup`) and
    /// temperature-processed (`BSIM4temp`).
    pub fn new(
        h: Bsim4Here,
        model: Arc<Bsim4Model>,
        pp: Arc<Bsim4Size>,
        st: usize,
        ckt_temp: f64,
    ) -> Self {
        Bsim4I {
            h,
            model,
            pp,
            st,
            ckt_temp,
            last_mode: 0,
        }
    }
}

impl crate::devices::Device for Bsim4I {
    fn par_load(&self) -> bool {
        true
    }

    /// `BSIM4getic` (`b4getic.c`).
    fn set_ic(&mut self, rhs: &[f64]) {
        let h = &mut self.h;
        if !h.given.ic_vds {
            h.ic_vds = rhs[h.d_node] - rhs[h.s_node];
        }
        if !h.given.ic_vgs {
            h.ic_vgs = rhs[h.g_node_ext] - rhs[h.s_node];
        }
        if !h.given.ic_vbs {
            h.ic_vbs = rhs[h.b_node] - rhs[h.s_node];
        }
    }

    /// The `TSTALLOC` sequence of `BSIM4setup` (`b4set.c`).
    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let md: &Bsim4Model = &self.model;
        let h = &mut self.h;
        h.dpbp_ptr = mb.slot(h.d_node_prime, h.b_node_prime);
        h.gpbp_ptr = mb.slot(h.g_node_prime, h.b_node_prime);
        h.spbp_ptr = mb.slot(h.s_node_prime, h.b_node_prime);
        h.bpdp_ptr = mb.slot(h.b_node_prime, h.d_node_prime);
        h.bpgp_ptr = mb.slot(h.b_node_prime, h.g_node_prime);
        h.bpsp_ptr = mb.slot(h.b_node_prime, h.s_node_prime);
        h.bpbp_ptr = mb.slot(h.b_node_prime, h.b_node_prime);
        h.dd_ptr = mb.slot(h.d_node, h.d_node);
        h.gpgp_ptr = mb.slot(h.g_node_prime, h.g_node_prime);
        h.ss_ptr = mb.slot(h.s_node, h.s_node);
        h.dpdp_ptr = mb.slot(h.d_node_prime, h.d_node_prime);
        h.spsp_ptr = mb.slot(h.s_node_prime, h.s_node_prime);
        h.ddp_ptr = mb.slot(h.d_node, h.d_node_prime);
        h.gpdp_ptr = mb.slot(h.g_node_prime, h.d_node_prime);
        h.gpsp_ptr = mb.slot(h.g_node_prime, h.s_node_prime);
        h.ssp_ptr = mb.slot(h.s_node, h.s_node_prime);
        h.dpsp_ptr = mb.slot(h.d_node_prime, h.s_node_prime);
        h.dpd_ptr = mb.slot(h.d_node_prime, h.d_node);
        h.dpgp_ptr = mb.slot(h.d_node_prime, h.g_node_prime);
        h.spgp_ptr = mb.slot(h.s_node_prime, h.g_node_prime);
        h.sps_ptr = mb.slot(h.s_node_prime, h.s_node);
        h.spdp_ptr = mb.slot(h.s_node_prime, h.d_node_prime);
        h.qq_ptr = mb.slot(h.q_node, h.q_node);
        h.qbp_ptr = mb.slot(h.q_node, h.b_node_prime);
        h.qdp_ptr = mb.slot(h.q_node, h.d_node_prime);
        h.qsp_ptr = mb.slot(h.q_node, h.s_node_prime);
        h.qgp_ptr = mb.slot(h.q_node, h.g_node_prime);
        h.dpq_ptr = mb.slot(h.d_node_prime, h.q_node);
        h.spq_ptr = mb.slot(h.s_node_prime, h.q_node);
        h.gpq_ptr = mb.slot(h.g_node_prime, h.q_node);
        if h.rgate_mod != 0 {
            h.gege_ptr = mb.slot(h.g_node_ext, h.g_node_ext);
            h.gegp_ptr = mb.slot(h.g_node_ext, h.g_node_prime);
            h.gpge_ptr = mb.slot(h.g_node_prime, h.g_node_ext);
            h.gedp_ptr = mb.slot(h.g_node_ext, h.d_node_prime);
            h.gesp_ptr = mb.slot(h.g_node_ext, h.s_node_prime);
            h.gebp_ptr = mb.slot(h.g_node_ext, h.b_node_prime);
            h.gmdp_ptr = mb.slot(h.g_node_mid, h.d_node_prime);
            h.gmgp_ptr = mb.slot(h.g_node_mid, h.g_node_prime);
            h.gmgm_ptr = mb.slot(h.g_node_mid, h.g_node_mid);
            h.gmge_ptr = mb.slot(h.g_node_mid, h.g_node_ext);
            h.gmsp_ptr = mb.slot(h.g_node_mid, h.s_node_prime);
            h.gmbp_ptr = mb.slot(h.g_node_mid, h.b_node_prime);
            h.dpgm_ptr = mb.slot(h.d_node_prime, h.g_node_mid);
            h.gpgm_ptr = mb.slot(h.g_node_prime, h.g_node_mid);
            h.gegm_ptr = mb.slot(h.g_node_ext, h.g_node_mid);
            h.spgm_ptr = mb.slot(h.s_node_prime, h.g_node_mid);
            h.bpgm_ptr = mb.slot(h.b_node_prime, h.g_node_mid);
        }
        if h.rbody_mod == 1 || h.rbody_mod == 2 {
            h.dpdb_ptr = mb.slot(h.d_node_prime, h.db_node);
            h.spsb_ptr = mb.slot(h.s_node_prime, h.sb_node);
            h.dbdp_ptr = mb.slot(h.db_node, h.d_node_prime);
            h.dbdb_ptr = mb.slot(h.db_node, h.db_node);
            h.dbbp_ptr = mb.slot(h.db_node, h.b_node_prime);
            h.dbb_ptr = mb.slot(h.db_node, h.b_node);
            h.bpdb_ptr = mb.slot(h.b_node_prime, h.db_node);
            h.bpb_ptr = mb.slot(h.b_node_prime, h.b_node);
            h.bpsb_ptr = mb.slot(h.b_node_prime, h.sb_node);
            h.sbsp_ptr = mb.slot(h.sb_node, h.s_node_prime);
            h.sbbp_ptr = mb.slot(h.sb_node, h.b_node_prime);
            h.sbb_ptr = mb.slot(h.sb_node, h.b_node);
            h.sbsb_ptr = mb.slot(h.sb_node, h.sb_node);
            h.bdb_ptr = mb.slot(h.b_node, h.db_node);
            h.bbp_ptr = mb.slot(h.b_node, h.b_node_prime);
            h.bsb_ptr = mb.slot(h.b_node, h.sb_node);
            h.bb_ptr = mb.slot(h.b_node, h.b_node);
        }
        if md.rds_mod != 0 {
            h.dgp_ptr = mb.slot(h.d_node, h.g_node_prime);
            h.dsp_ptr = mb.slot(h.d_node, h.s_node_prime);
            h.dbp_ptr = mb.slot(h.d_node, h.b_node_prime);
            h.sdp_ptr = mb.slot(h.s_node, h.d_node_prime);
            h.sgp_ptr = mb.slot(h.s_node, h.g_node_prime);
            h.sbp_ptr = mb.slot(h.s_node, h.b_node_prime);
        }
    }

    /// `BSIM4convTest` (`b4cvtest.c`).
    #[allow(unused_assignments, clippy::too_many_lines)]
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let md: &Bsim4Model = &self.model;
        let h = &self.h;
        let st = self.st;
        let ck_mode = self.last_mode;
        let mut cbhat: f64 = 0.0;
        let mut cdedhat: f64 = 0.0;
        let mut cdhat: f64 = 0.0;
        let mut cgbhat: f64 = 0.0;
        let mut cgdhat: f64 = 0.0;
        let mut cgshat: f64 = 0.0;
        let mut cseshat: f64 = 0.0;
        let mut delvbd: f64 = 0.0;
        let mut delvbd_jct: f64 = 0.0;
        let mut delvbs: f64 = 0.0;
        let mut delvbs_jct: f64 = 0.0;
        let mut delvdbd: f64 = 0.0;
        let mut delvded: f64 = 0.0;
        let mut _delvdes: f64 = 0.0;
        let mut delvds: f64 = 0.0;
        let mut delvgd: f64 = 0.0;
        let mut delvgs: f64 = 0.0;
        let mut delvsbs: f64 = 0.0;
        let mut delvses: f64 = 0.0;
        let mut ibtot: f64 = 0.0;
        let mut idedtot: f64 = 0.0;
        let mut idtot: f64 = 0.0;
        let mut igbtot: f64 = 0.0;
        let mut igdtot: f64 = 0.0;
        let mut igstot: f64 = 0.0;
        let mut isestot: f64 = 0.0;
        let mut tol0: f64 = 0.0;
        let mut tol1: f64 = 0.0;
        let mut tol2: f64 = 0.0;
        let mut tol3: f64 = 0.0;
        let mut tol4: f64 = 0.0;
        let mut tol5: f64 = 0.0;
        let mut tol6: f64 = 0.0;
        let mut vbd: f64 = 0.0;
        let mut vbs: f64 = 0.0;
        let mut vdbd: f64 = 0.0;
        let mut vdbs: f64 = 0.0;
        let mut vdedo: f64 = 0.0;
        let mut vdes: f64 = 0.0;
        let mut vds: f64 = 0.0;
        let mut vgd: f64 = 0.0;
        let mut vgdo: f64 = 0.0;
        let mut vgs: f64 = 0.0;
        let mut vsbs: f64 = 0.0;
        let mut vses: f64 = 0.0;
        vds = md.type_ as f64 * (ctx.rhs[h.d_node_prime] - ctx.rhs[h.s_node_prime]);
        vgs = md.type_ as f64 * (ctx.rhs[h.g_node_prime] - ctx.rhs[h.s_node_prime]);
        vbs = md.type_ as f64 * (ctx.rhs[h.b_node_prime] - ctx.rhs[h.s_node_prime]);
        vdbs = md.type_ as f64 * (ctx.rhs[h.db_node] - ctx.rhs[h.s_node_prime]);
        vsbs = md.type_ as f64 * (ctx.rhs[h.sb_node] - ctx.rhs[h.s_node_prime]);
        vses = md.type_ as f64 * (ctx.rhs[h.s_node] - ctx.rhs[h.s_node_prime]);
        vdes = md.type_ as f64 * (ctx.rhs[h.d_node] - ctx.rhs[h.s_node_prime]);
        vgdo = ctx.states.get(0, st + ST_VGS) - ctx.states.get(0, st + ST_VDS);
        vbd = vbs - vds;
        vdbd = vdbs - vds;
        vgd = vgs - vds;
        delvbd = vbd - ctx.states.get(0, st + ST_VBD);
        delvdbd = vdbd - ctx.states.get(0, st + ST_VDBD);
        delvgd = vgd - vgdo;
        delvds = vds - ctx.states.get(0, st + ST_VDS);
        delvgs = vgs - ctx.states.get(0, st + ST_VGS);
        delvbs = vbs - ctx.states.get(0, st + ST_VBS);
        delvsbs = vsbs - ctx.states.get(0, st + ST_VSBS);
        delvses = vses - ctx.states.get(0, st + ST_VSES);
        vdedo = ctx.states.get(0, st + ST_VDES) - ctx.states.get(0, st + ST_VDS);
        _delvdes = vdes - ctx.states.get(0, st + ST_VDES);
        delvded = vdes - vds - vdedo;
        delvbd_jct = if h.rbody_mod == 0 { delvbd } else { delvdbd };
        delvbs_jct = if h.rbody_mod == 0 { delvbs } else { delvsbs };
        if h.mode >= 0 {
            idtot = h.cd + h.csub - h.cbd + h.igidl;
            cdhat = idtot - h.gbd * delvbd_jct
                + (h.gmbs + h.gbbs + h.ggidlb) * delvbs
                + (h.gm + h.gbgs + h.ggidlg) * delvgs
                + (h.gds + h.gbds + h.ggidld) * delvds;
            igstot = h.igs + h.igcs;
            cgshat =
                igstot + (h.g_igsg + h.g_igcsg) * delvgs + h.g_igcsd * delvds + h.g_igcsb * delvbs;
            igdtot = h.igd + h.igcd;
            cgdhat = igdtot
                + h.g_igdg * delvgd
                + h.g_igcdg * delvgs
                + h.g_igcdd * delvds
                + h.g_igcdb * delvbs;
            igbtot = h.igb;
            cgbhat = h.igb + h.g_igbg * delvgs + h.g_igbd * delvds + h.g_igbb * delvbs;
        } else {
            idtot = h.cd + h.cbd - h.igidl;
            cdhat = idtot + h.gbd * delvbd_jct + h.gmbs * delvbd + h.gm * delvgd
                - (h.gds + h.ggidls) * delvds
                - h.ggidlg * delvgs
                - h.ggidlb * delvbs;
            igstot = h.igs + h.igcd;
            cgshat = igstot + h.g_igsg * delvgs + h.g_igcdg * delvgd - h.g_igcdd * delvds
                + h.g_igcdb * delvbd;
            igdtot = h.igd + h.igcs;
            cgdhat =
                igdtot + (h.g_igdg + h.g_igcsg) * delvgd - h.g_igcsd * delvds + h.g_igcsb * delvbd;
            igbtot = h.igb;
            cgbhat = h.igb + h.g_igbg * delvgd - h.g_igbd * delvds + h.g_igbb * delvbd;
        }
        isestot = h.gstot * ctx.states.get(0, st + ST_VSES);
        cseshat =
            isestot + h.gstot * delvses + h.gstotd * delvds + h.gstotg * delvgs + h.gstotb * delvbs;
        idedtot = h.gdtot * vdedo;
        cdedhat =
            idedtot + h.gdtot * delvded + h.gdtotd * delvds + h.gdtotg * delvgs + h.gdtotb * delvbs;
        if h.off == 0 || ck_mode & MODEINITFIX == 0 {
            tol0 = ctx.reltol * max(fabs(cdhat), fabs(idtot)) + ctx.abstol;
            tol1 = ctx.reltol * max(fabs(cseshat), fabs(isestot)) + ctx.abstol;
            tol2 = ctx.reltol * max(fabs(cdedhat), fabs(idedtot)) + ctx.abstol;
            tol3 = ctx.reltol * max(fabs(cgshat), fabs(igstot)) + ctx.abstol;
            tol4 = ctx.reltol * max(fabs(cgdhat), fabs(igdtot)) + ctx.abstol;
            tol5 = ctx.reltol * max(fabs(cgbhat), fabs(igbtot)) + ctx.abstol;
            if fabs(cdhat - idtot) >= tol0
                || fabs(cseshat - isestot) >= tol1
                || fabs(cdedhat - idedtot) >= tol2
            {
                return false;
            }
            if fabs(cgshat - igstot) >= tol3
                || fabs(cgdhat - igdtot) >= tol4
                || fabs(cgbhat - igbtot) >= tol5
            {
                return false;
            }
            ibtot = h.cbs + h.cbd - h.igidl - h.igisl - h.csub;
            if h.mode >= 0 {
                cbhat = ibtot + h.gbd * delvbd_jct + h.gbs * delvbs_jct
                    - (h.gbbs + h.ggidlb) * delvbs
                    - (h.gbgs + h.ggidlg) * delvgs
                    - (h.gbds + h.ggidld) * delvds
                    - h.ggislg * delvgd
                    - h.ggislb * delvbd
                    + h.ggisls * delvds;
            } else {
                cbhat = ibtot + h.gbs * delvbs_jct + h.gbd * delvbd_jct
                    - (h.gbbs + h.ggislb) * delvbd
                    - (h.gbgs + h.ggislg) * delvgd
                    + (h.gbds + h.ggisld - h.ggidls) * delvds
                    - h.ggidlg * delvgs
                    - h.ggidlb * delvbs;
            }
            tol6 = ctx.reltol * max(fabs(cbhat), fabs(ibtot)) + ctx.abstol;
            if fabs(cbhat - ibtot) > tol6 {
                return false;
            }
        }
        true
    }

    /// `BSIM4load` (`b4ld.c`), without the bypass and the OpenMP path.
    #[allow(
        unused_assignments,
        clippy::too_many_lines,
        clippy::cognitive_complexity
    )]
    fn load(&mut self, ctx: &mut LoadCtx) {
        let ck_mode = mode_bits(ctx);
        self.last_mode = ck_mode;
        let ck_temp = self.ckt_temp;
        // Borrow the shared model cards, never clone the `Arc`s (the
        // refcount would serialize the parallel load, issue #9).
        let md: &Bsim4Model = &self.model;
        let pp: &Bsim4Size = &self.pp;
        let h = &mut self.h;
        let st = self.st;
        let charge_computation_needed: i32 =
            if (ck_mode & (MODEDCTRANCURVE | MODEAC | MODETRAN | MODEINITSMSIG)) != 0
                || ((ck_mode & MODETRANOP) != 0 && (ck_mode & MODEUIC) != 0)
            {
                1
            } else {
                0
            };
        load::load(
            md,
            pp,
            h,
            st,
            ck_mode,
            ck_temp,
            charge_computation_needed,
            ctx,
        );
    }

    /// `BSIM4trunc` (`b4trunc.c`).
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        let h = &self.h;
        let st = self.st;
        crate::integ::terr(st + ST_QB, t, time_step);
        crate::integ::terr(st + ST_QG, t, time_step);
        crate::integ::terr(st + ST_QD, t, time_step);
        if h.trnqs_mod != 0 {
            crate::integ::terr(st + ST_QCDUMP, t, time_step);
        }
        if h.rbody_mod != 0 {
            crate::integ::terr(st + ST_QBS, t, time_step);
            crate::integ::terr(st + ST_QBD, t, time_step);
        }
        if h.rgate_mod == 3 {
            crate::integ::terr(st + ST_QGMID, t, time_step);
        }
    }

    /// `BSIM4acLoad` (`b4acld.c`).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        acld::ac_load(&self.model, &self.pp, &self.h, self.st, ctx);
    }

    /// `BSIM4pzLoad` (`b4pzld.c`).
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        pzld::pz_load(&self.model, &self.pp, &self.h, self.st, ctx);
        Ok(())
    }

    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(BSIM4_NOISE_SUFFIXES)
    }

    fn noise_sep(&self) -> &'static str {
        "."
    }

    /// `BSIM4noise` (`b4noi.c`), `N_DENS`.
    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        noise::noise_dens(&self.model, &self.pp, &self.h, self.st, self.ckt_temp, ctx)
    }
}

// ---------------------------------------------------------------------
// Set-up and temperature processing at circuit build.

/// One model's entry in the build-time knot list: the card as given,
/// its set-up and temperature-processed copy (`BSIM4setup` +
/// `BSIM4temp`, model part), the `BSIM4temp` locals that persist
/// across its instances, and the `bsim4SizeDependParam` knots built
/// so far (`pSizeDependParamKnot`, searched by exact drawn L, W and NF
/// like the C).
pub(crate) struct Bsim4Knots {
    given: Bsim4Model,
    model: Bsim4Model,
    shared: Option<Arc<Bsim4Model>>,
    tv: temp::TempVars,
    knots: Vec<Arc<Bsim4Size>>,
    checked: bool,
}

impl Bsim4Knots {
    /// The `BSIM4setup` model defaults of `given` (cached per card).
    pub fn setup_model(given: &Bsim4Model, tnom: f64, notes: &mut Vec<String>) -> Bsim4Model {
        let mut m = given.clone();
        setup::setup_model(&mut m, tnom, notes);
        m
    }

    /// Set up and temperature-process one instance: find or build the
    /// model entry, find or build the size knot for (L, W, NF), run the
    /// instance part of `BSIM4temp` and, for the model's first instance,
    /// `BSIM4checkModel`. `h` must already hold the instance parameters,
    /// the `BSIM4setup` defaults and the node numbers.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        list: &mut Vec<Bsim4Knots>,
        name: &str,
        given: &Bsim4Model,
        h: &mut Bsim4Here,
        temp: f64,
        tnom: f64,
        notes: &mut Vec<String>,
    ) -> Result<(Arc<Bsim4Model>, Arc<Bsim4Size>), String> {
        let model_name = given.mod_name.as_str();
        let idx = match list.iter().position(|k| k.given == *given) {
            Some(i) => i,
            None => {
                let mut mnotes = Vec::new();
                let mut m = Self::setup_model(given, tnom, &mut mnotes);
                let mut tv = temp::TempVars::default();
                temp::temp_model(&mut m, &mut tv, temp, &mut mnotes);
                notes.extend(
                    mnotes
                        .into_iter()
                        .map(|n| format!("BSIM4 model `{model_name}`: {n}")),
                );
                list.push(Bsim4Knots {
                    given: given.clone(),
                    model: m,
                    shared: None,
                    tv,
                    knots: Vec::new(),
                    checked: false,
                });
                list.len() - 1
            }
        };
        let k = &mut list[idx];
        let mut inotes = Vec::new();
        let found = k
            .knots
            .iter()
            .find(|p| p.length == h.l && p.width == h.w && p.nfinger == h.nf)
            .cloned();
        let mut fresh = None;
        if found.is_none() {
            let mut p = Bsim4Size::default();
            temp::temp_size(
                &mut k.model,
                h,
                &mut p,
                &mut k.tv,
                temp,
                model_name,
                name,
                &mut inotes,
            )?;
            fresh = Some(p);
        }
        temp::temp_instance(
            &mut k.model,
            h,
            found.as_deref().or(fresh.as_ref()).expect("a knot"),
            &mut k.tv,
            temp,
            &mut inotes,
        );
        if !k.checked {
            k.checked = true;
            let mut cnotes = Vec::new();
            // The model's first instance always builds its knot, which
            // `BSIM4checkModel` may clamp (it writes through `pParam`).
            let p = fresh.as_mut().expect("first instance builds its knot");
            let fatal = check::check_model(&mut k.model, h, p, temp, &mut cnotes);
            if !cnotes.is_empty() {
                inotes.push(format!(
                    "Checking parameters for BSIM 4.8 model {model_name}"
                ));
                inotes.extend(cnotes);
            }
            if fatal != 0 {
                notes.extend(inotes.into_iter().map(|n| format!("`{name}`: {n}")));
                return Err(format!(
                    "detected during BSIM4.8.3 parameter checking for model {model_name} of device instance {name}"
                ));
            }
        }
        notes.extend(inotes.into_iter().map(|n| format!("`{name}`: {n}")));
        let pp = match (found, fresh) {
            (Some(p), _) => p,
            (None, Some(p)) => {
                let p = Arc::new(p);
                k.knots.push(Arc::clone(&p));
                p
            }
            (None, None) => unreachable!(),
        };
        let model = match &k.shared {
            Some(s) if **s == k.model => Arc::clone(s),
            _ => {
                let s = Arc::new(k.model.clone());
                k.shared = Some(Arc::clone(&s));
                s
            }
        };
        Ok((model, pp))
    }
}

pub(crate) use setup::{assign_nodes, internal_nodes, new_here};

/// `VERSION` strings ngspice-47 routes to its older BSIM4 code bases
/// (`inpdomod.c`: 4.0–4.5 → `bsim4v5`, 4.6 → `bsim4v6`, 4.7 →
/// `bsim4v7`), which Dune runs on the 4.8.3 equations.
pub(crate) fn older_version(v: &str) -> bool {
    ["4.0", "4.1", "4.2", "4.3", "4.4", "4.5", "4.6", "4.7"]
        .iter()
        .any(|p| v.starts_with(p))
}

/// Unused handles kept so the generated code compiles in every
/// configuration.
#[allow(dead_code)]
const _UNUSED: (f64, f64, f64, f64, f64, f64, Var) = (
    MIN_EXPL,
    MAX_EXPL,
    EXPL_THRESHOLD,
    DELTA_2,
    DELTA_3,
    KB,
    TRASH.0,
);
