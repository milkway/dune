// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4set.c as distributed with ngspice-47
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

//! `BSIM4setup` (`b4set.c`): the model and instance parameter defaulting
//! and the internal-node decisions. The matrix allocation lives in the
//! device shell (`Device::alloc`).

use super::*;

/// The `BSIM4version` defaulting and the integer version of `b4set.c`.
/// Cards declaring 4.0–4.7 run on the 4.8.3 equations (the elaborator
/// has already noted it), so they get `BSIM4v48V483` silently.
fn version_setup(md: &mut Bsim4Model, notes: &mut Vec<String>) {
    if !md.given.version {
        md.version = "4.8.3".to_string();
    }
    let v = md.version.as_str();
    md.v48int_version = if v == "4.8.3" || v.starts_with("4.83") || v.starts_with("4.8") {
        483
    } else if v == "4.8.2" || v.starts_with("4.82") {
        482
    } else if v == "4.8.1" || v.starts_with("4.81") {
        481
    } else if v == "4.8.0" || v.starts_with("4.80") {
        480
    } else {
        if !super::older_version(v) {
            notes.push("Warning: unknown BSIM4 version. Working now with BSIM4.8.3.".to_string());
        }
        483
    };
}

/// The model part of `BSIM4setup` (`b4set.c`), `ck_tnom` = `CKTnomTemp`.
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn setup_model(md: &mut Bsim4Model, ck_tnom: f64, notes: &mut Vec<String>) {
    let mut _dmcgeff: f64 = 0.0;
    let mut _dmcieff: f64 = 0.0;
    let mut _dmdgeff: f64 = 0.0;
    if !md.given.type_ {
        md.type_ = NMOS;
    }
    if !md.given.mob_mod {
        md.mob_mod = 0;
    } else if md.mob_mod != 0
        && md.mob_mod != 1
        && md.mob_mod != 2
        && md.mob_mod != 3
        && md.mob_mod != 4
        && md.mob_mod != 5
        && md.mob_mod != 6
    {
        md.mob_mod = 0;
        notes.push("Warning: mobMod has been set to its default value: 0.".to_string());
    }
    if !md.given.bin_unit {
        md.bin_unit = 1;
    }
    if !md.given.param_chk {
        md.param_chk = 1;
    }
    if !md.given.dio_mod {
        md.dio_mod = 1;
    } else if md.dio_mod != 0 && md.dio_mod != 1 && md.dio_mod != 2 {
        md.dio_mod = 1;
        notes.push("Warning: dioMod has been set to its default value: 1.".to_string());
    }
    if !md.given.cvcharge_mod {
        md.cvcharge_mod = 0;
    }
    if !md.given.cap_mod {
        md.cap_mod = 2;
    } else if md.cap_mod != 0 && md.cap_mod != 1 && md.cap_mod != 2 {
        md.cap_mod = 2;
        notes.push("Warning: capMod has been set to its default value: 2.".to_string());
    }
    if !md.given.rds_mod {
        md.rds_mod = 0;
    } else if md.rds_mod != 0 && md.rds_mod != 1 {
        md.rds_mod = 0;
        notes.push("Warning: rdsMod has been set to its default value: 0.".to_string());
    }
    if !md.given.rbody_mod {
        md.rbody_mod = 0;
    } else if md.rbody_mod != 0 && md.rbody_mod != 1 && md.rbody_mod != 2 {
        md.rbody_mod = 0;
        notes.push("Warning: rbodyMod has been set to its default value: 0.".to_string());
    }
    if !md.given.rgate_mod {
        md.rgate_mod = 0;
    } else if md.rgate_mod != 0 && md.rgate_mod != 1 && md.rgate_mod != 2 && md.rgate_mod != 3 {
        md.rgate_mod = 0;
        notes.push("Warning: rgateMod has been set to its default value: 0.".to_string());
    }
    if !md.given.per_mod {
        md.per_mod = 1;
    } else if md.per_mod != 0 && md.per_mod != 1 {
        md.per_mod = 1;
        notes.push("Warning: perMod has been set to its default value: 1.".to_string());
    }
    if !md.given.geo_mod {
        md.geo_mod = 0;
    }
    if !md.given.fnoi_mod {
        md.fnoi_mod = 1;
    } else if md.fnoi_mod != 0 && md.fnoi_mod != 1 {
        md.fnoi_mod = 1;
        notes.push("Warning: fnoiMod has been set to its default value: 1.".to_string());
    }
    if !md.given.tnoi_mod {
        md.tnoi_mod = 0;
    } else if md.tnoi_mod != 0 && md.tnoi_mod != 1 && md.tnoi_mod != 2 {
        md.tnoi_mod = 0;
        notes.push("Warning: tnoiMod has been set to its default value: 0.".to_string());
    }
    if !md.given.trnqs_mod {
        md.trnqs_mod = 0;
    } else if md.trnqs_mod != 0 && md.trnqs_mod != 1 {
        md.trnqs_mod = 0;
        notes.push("Warning: trnqsMod has been set to its default value: 0.".to_string());
    }
    if !md.given.acnqs_mod {
        md.acnqs_mod = 0;
    } else if md.acnqs_mod != 0 && md.acnqs_mod != 1 {
        md.acnqs_mod = 0;
        notes.push("Warning: acnqsMod has been set to its default value: 0.".to_string());
    }
    if !md.given.mtrl_mod {
        md.mtrl_mod = 0;
    } else if md.mtrl_mod != 0 && md.mtrl_mod != 1 {
        md.mtrl_mod = 0;
        notes.push("Warning: mtrlMod has been set to its default value: 0.".to_string());
    }
    if !md.given.mtrl_compat_mod {
        md.mtrl_compat_mod = 0;
    } else if md.mtrl_compat_mod != 0 && md.mtrl_compat_mod != 1 {
        md.mtrl_compat_mod = 0;
        notes.push("Warning: mtrlCompatMod has been set to its default value: 0.".to_string());
    }
    if !md.given.igc_mod {
        md.igc_mod = 0;
    } else if md.igc_mod != 0 && md.igc_mod != 1 && md.igc_mod != 2 {
        md.igc_mod = 0;
        notes.push("Warning: igcMod has been set to its default value: 0.".to_string());
    }
    if !md.given.igb_mod {
        md.igb_mod = 0;
    } else if md.igb_mod != 0 && md.igb_mod != 1 {
        md.igb_mod = 0;
        notes.push("Warning: igbMod has been set to its default value: 0.".to_string());
    }
    if !md.given.temp_mod {
        md.temp_mod = 0;
    } else if md.temp_mod != 0 && md.temp_mod != 1 && md.temp_mod != 2 && md.temp_mod != 3 {
        md.temp_mod = 0;
        notes.push("Warning: tempMod has been set to its default value: 0.".to_string());
    }
    version_setup(md, notes);
    if !md.given.toxref {
        md.toxref = 30.0e-10;
    }
    if !md.given.eot {
        md.eot = 15.0e-10;
    }
    if !md.given.vddeot {
        md.vddeot = if md.type_ == NMOS { 1.5 } else { -1.5 };
    }
    if !md.given.tempeot {
        md.tempeot = 300.15;
    }
    if !md.given.leffeot {
        md.leffeot = 1.0;
    }
    if !md.given.weffeot {
        md.weffeot = 10.0;
    }
    if !md.given.ados {
        md.ados = 1.0;
    }
    if !md.given.bdos {
        md.bdos = 1.0;
    }
    if !md.given.toxe {
        md.toxe = 30.0e-10;
    }
    if !md.given.toxp {
        md.toxp = md.toxe;
    }
    if !md.given.toxm {
        md.toxm = md.toxe;
    }
    if !md.given.dtox {
        md.dtox = 0.0;
    }
    if !md.given.epsrox {
        md.epsrox = 3.9;
    }
    if !md.given.cdsc {
        md.cdsc = 2.4e-4;
    }
    if !md.given.cdscb {
        md.cdscb = 0.0;
    }
    if !md.given.cdscd {
        md.cdscd = 0.0;
    }
    if !md.given.cit {
        md.cit = 0.0;
    }
    if !md.given.nfactor {
        md.nfactor = 1.0;
    }
    if !md.given.xj {
        md.xj = 0.15e-6;
    }
    if !md.given.vsat {
        md.vsat = 8.0e4;
    }
    if !md.given.at {
        md.at = 3.3e4;
    }
    if !md.given.a0 {
        md.a0 = 1.0;
    }
    if !md.given.ags {
        md.ags = 0.0;
    }
    if !md.given.a1 {
        md.a1 = 0.0;
    }
    if !md.given.a2 {
        md.a2 = 1.0;
    }
    if !md.given.keta {
        md.keta = -0.047;
    }
    if !md.given.ketac {
        md.ketac = md.keta;
    }
    if !md.given.nsub {
        md.nsub = 6.0e16;
    }
    if !md.given.phig {
        md.phig = 4.05;
    }
    if !md.given.epsrgate {
        md.epsrgate = 11.7;
    }
    if !md.given.easub {
        md.easub = 4.05;
    }
    if !md.given.epsrsub {
        md.epsrsub = 11.7;
    }
    if !md.given.ni0sub {
        md.ni0sub = 1.45e10;
    }
    if !md.given.bg0sub {
        md.bg0sub = 1.16;
    }
    if !md.given.tbgasub {
        md.tbgasub = 7.02e-4;
    }
    if !md.given.tbgbsub {
        md.tbgbsub = 1108.0;
    }
    if !md.given.ndep {
        md.ndep = 1.7e17;
    }
    if !md.given.nsd {
        md.nsd = 1.0e20;
    }
    if !md.given.phin {
        md.phin = 0.0;
    }
    if !md.given.ngate {
        md.ngate = 0.0;
    }
    if !md.given.vbm {
        md.vbm = -3.0;
    }
    if !md.given.xt {
        md.xt = 1.55e-7;
    }
    if !md.given.kt1 {
        md.kt1 = -0.11;
    }
    if !md.given.kt1l {
        md.kt1l = 0.0;
    }
    if !md.given.kt2 {
        md.kt2 = 0.022;
    }
    if !md.given.k3 {
        md.k3 = 80.0;
    }
    if !md.given.k3b {
        md.k3b = 0.0;
    }
    if !md.given.w0 {
        md.w0 = 2.5e-6;
    }
    if !md.given.lpe0 {
        md.lpe0 = 1.74e-7;
    }
    if !md.given.lpeb {
        md.lpeb = 0.0;
    }
    if !md.given.dvtp0 {
        md.dvtp0 = 0.0;
    }
    if !md.given.dvtp1 {
        md.dvtp1 = 0.0;
    }
    if !md.given.dvtp2 {
        md.dvtp2 = 0.0;
    }
    if !md.given.dvtp3 {
        md.dvtp3 = 0.0;
    }
    if !md.given.dvtp4 {
        md.dvtp4 = 0.0;
    }
    if !md.given.dvtp5 {
        md.dvtp5 = 0.0;
    }
    if !md.given.dvt0 {
        md.dvt0 = 2.2;
    }
    if !md.given.dvt1 {
        md.dvt1 = 0.53;
    }
    if !md.given.dvt2 {
        md.dvt2 = -0.032;
    }
    if !md.given.dvt0w {
        md.dvt0w = 0.0;
    }
    if !md.given.dvt1w {
        md.dvt1w = 5.3e6;
    }
    if !md.given.dvt2w {
        md.dvt2w = -0.032;
    }
    if !md.given.drout {
        md.drout = 0.56;
    }
    if !md.given.dsub {
        md.dsub = md.drout;
    }
    if !md.given.vth0 {
        md.vth0 = if md.type_ == NMOS { 0.7 } else { -0.7 };
    }
    if !md.given.vfb {
        md.vfb = -1.0;
    }
    if !md.given.eu {
        md.eu = if md.type_ == NMOS { 1.67 } else { 1.0 };
    }
    if !md.given.ucs {
        md.ucs = if md.type_ == NMOS { 1.67 } else { 1.0 };
    }
    if md.v48int_version <= 480 {
        if !md.given.ua {
            md.ua = if md.mob_mod == 2 { 1.0e-15 } else { 1.0e-9 };
        }
        if !md.given.uc {
            md.uc = if md.mob_mod == 1 { -0.0465 } else { -0.0465e-9 };
        }
        if !md.given.uc1 {
            md.uc1 = if md.mob_mod == 1 { -0.056 } else { -0.056e-9 };
        }
    } else {
        if !md.given.ua {
            md.ua = if md.mob_mod == 2 || md.mob_mod == 6 {
                1.0e-15
            } else {
                1.0e-9
            };
        }
        if !md.given.uc {
            md.uc = if md.mob_mod == 1 || md.mob_mod == 5 {
                -0.0465
            } else {
                -0.0465e-9
            };
        }
        if !md.given.uc1 {
            md.uc1 = if md.mob_mod == 1 || md.mob_mod == 5 {
                -0.056
            } else {
                -0.056e-9
            };
        }
    }
    if !md.given.ua1 {
        md.ua1 = 1.0e-9;
    }
    if !md.given.ub {
        md.ub = 1.0e-19;
    }
    if !md.given.ub1 {
        md.ub1 = -1.0e-18;
    }
    if !md.given.ud {
        md.ud = 0.0;
    }
    if !md.given.ud1 {
        md.ud1 = 0.0;
    }
    if !md.given.up {
        md.up = 0.0;
    }
    if !md.given.lp {
        md.lp = 1.0e-8;
    }
    if !md.given.u0 {
        md.u0 = if md.type_ == NMOS { 0.067 } else { 0.025 };
    }
    if !md.given.ute {
        md.ute = -1.5;
    }
    if !md.given.ucste {
        md.ucste = -4.775e-3;
    }
    if !md.given.voff {
        md.voff = -0.08;
    }
    if !md.given.voffl {
        md.voffl = 0.0;
    }
    if !md.given.voffcvl {
        md.voffcvl = 0.0;
    }
    if !md.given.minv {
        md.minv = 0.0;
    }
    if !md.given.minvcv {
        md.minvcv = 0.0;
    }
    if !md.given.fprout {
        md.fprout = 0.0;
    }
    if !md.given.pdits {
        md.pdits = 0.0;
    }
    if !md.given.pditsd {
        md.pditsd = 0.0;
    }
    if !md.given.pditsl {
        md.pditsl = 0.0;
    }
    if !md.given.delta {
        md.delta = 0.01;
    }
    if !md.given.rdswmin {
        md.rdswmin = 0.0;
    }
    if !md.given.rdwmin {
        md.rdwmin = 0.0;
    }
    if !md.given.rswmin {
        md.rswmin = 0.0;
    }
    if !md.given.rdsw {
        md.rdsw = 200.0;
    }
    if !md.given.rdw {
        md.rdw = 100.0;
    }
    if !md.given.rsw {
        md.rsw = 100.0;
    }
    if !md.given.prwg {
        md.prwg = 1.0;
    }
    if !md.given.prwb {
        md.prwb = 0.0;
    }
    if !md.given.prt {
        md.prt = 0.0;
    }
    if !md.given.eta0 {
        md.eta0 = 0.08;
    }
    if !md.given.etab {
        md.etab = -0.07;
    }
    if !md.given.pclm {
        md.pclm = 1.3;
    }
    if !md.given.pdibl1 {
        md.pdibl1 = 0.39;
    }
    if !md.given.pdibl2 {
        md.pdibl2 = 0.0086;
    }
    if !md.given.pdiblb {
        md.pdiblb = 0.0;
    }
    if !md.given.pscbe1 {
        md.pscbe1 = 4.24e8;
    }
    if !md.given.pscbe2 {
        md.pscbe2 = 1.0e-5;
    }
    if !md.given.pvag {
        md.pvag = 0.0;
    }
    if !md.given.wr {
        md.wr = 1.0;
    }
    if !md.given.dwg {
        md.dwg = 0.0;
    }
    if !md.given.dwb {
        md.dwb = 0.0;
    }
    if !md.given.b0 {
        md.b0 = 0.0;
    }
    if !md.given.b1 {
        md.b1 = 0.0;
    }
    if !md.given.alpha0 {
        md.alpha0 = 0.0;
    }
    if !md.given.alpha1 {
        md.alpha1 = 0.0;
    }
    if !md.given.beta0 {
        md.beta0 = 0.0;
    }
    if !md.given.gidl_mod {
        md.gidl_mod = 0;
    }
    if !md.given.agidl {
        md.agidl = 0.0;
    }
    if !md.given.bgidl {
        md.bgidl = 2.3e9;
    }
    if !md.given.cgidl {
        md.cgidl = 0.5;
    }
    if !md.given.egidl {
        md.egidl = 0.8;
    }
    if !md.given.rgidl {
        md.rgidl = 1.0;
    }
    if !md.given.kgidl {
        md.kgidl = 0.0;
    }
    if !md.given.fgidl {
        md.fgidl = 1.0;
    }
    if !md.given.agisl {
        md.agisl = md.agidl;
    }
    if !md.given.bgisl {
        md.bgisl = md.bgidl;
    }
    if !md.given.cgisl {
        md.cgisl = md.cgidl;
    }
    if !md.given.egisl {
        md.egisl = md.egidl;
    }
    if !md.given.rgisl {
        md.rgisl = md.rgidl;
    }
    if !md.given.kgisl {
        md.kgisl = md.kgidl;
    }
    if !md.given.fgisl {
        md.fgisl = md.fgidl;
    }
    if !md.given.aigc {
        md.aigc = if md.type_ == NMOS { 1.36e-2 } else { 9.80e-3 };
    }
    if !md.given.bigc {
        md.bigc = if md.type_ == NMOS { 1.71e-3 } else { 7.59e-4 };
    }
    if !md.given.cigc {
        md.cigc = if md.type_ == NMOS { 0.075 } else { 0.03 };
    }
    if md.given.aigsd {
        md.aigd = md.aigsd;
        md.aigs = md.aigd;
    } else {
        md.aigsd = if md.type_ == NMOS { 1.36e-2 } else { 9.80e-3 };
        if !md.given.aigs {
            md.aigs = if md.type_ == NMOS { 1.36e-2 } else { 9.80e-3 };
        }
        if !md.given.aigd {
            md.aigd = if md.type_ == NMOS { 1.36e-2 } else { 9.80e-3 };
        }
    }
    if md.given.bigsd {
        md.bigd = md.bigsd;
        md.bigs = md.bigd;
    } else {
        md.bigsd = if md.type_ == NMOS { 1.71e-3 } else { 7.59e-4 };
        if !md.given.bigs {
            md.bigs = if md.type_ == NMOS { 1.71e-3 } else { 7.59e-4 };
        }
        if !md.given.bigd {
            md.bigd = if md.type_ == NMOS { 1.71e-3 } else { 7.59e-4 };
        }
    }
    if md.given.cigsd {
        md.cigd = md.cigsd;
        md.cigs = md.cigd;
    } else {
        md.cigsd = if md.type_ == NMOS { 0.075 } else { 0.03 };
        if !md.given.cigs {
            md.cigs = if md.type_ == NMOS { 0.075 } else { 0.03 };
        }
        if !md.given.cigd {
            md.cigd = if md.type_ == NMOS { 0.075 } else { 0.03 };
        }
    }
    if !md.given.aigbacc {
        md.aigbacc = 1.36e-2;
    }
    if !md.given.bigbacc {
        md.bigbacc = 1.71e-3;
    }
    if !md.given.cigbacc {
        md.cigbacc = 0.075;
    }
    if !md.given.aigbinv {
        md.aigbinv = 1.11e-2;
    }
    if !md.given.bigbinv {
        md.bigbinv = 9.49e-4;
    }
    if !md.given.cigbinv {
        md.cigbinv = 0.006;
    }
    if !md.given.nigc {
        md.nigc = 1.0;
    }
    if !md.given.nigbinv {
        md.nigbinv = 3.0;
    }
    if !md.given.nigbacc {
        md.nigbacc = 1.0;
    }
    if !md.given.ntox {
        md.ntox = 1.0;
    }
    if !md.given.eigbinv {
        md.eigbinv = 1.1;
    }
    if !md.given.pigcd {
        md.pigcd = 1.0;
    }
    if !md.given.poxedge {
        md.poxedge = 1.0;
    }
    if !md.given.xrcrg1 {
        md.xrcrg1 = 12.0;
    }
    if !md.given.xrcrg2 {
        md.xrcrg2 = 1.0;
    }
    if !md.given.ijthsfwd {
        md.ijthsfwd = 0.1;
    }
    if !md.given.ijthdfwd {
        md.ijthdfwd = md.ijthsfwd;
    }
    if !md.given.ijthsrev {
        md.ijthsrev = 0.1;
    }
    if !md.given.ijthdrev {
        md.ijthdrev = md.ijthsrev;
    }
    if !md.given.tnoia {
        md.tnoia = 1.5;
    }
    if !md.given.tnoib {
        md.tnoib = 3.5;
    }
    if !md.given.tnoic {
        md.tnoic = 0.0;
    }
    if !md.given.rnoia {
        md.rnoia = 0.577;
    }
    if !md.given.rnoib {
        md.rnoib = 0.5164;
    }
    if !md.given.gidlclamp {
        md.gidlclamp = -1e-5;
    }
    if !md.given.idovvdsc {
        md.idovvdsc = 1e-9;
    }
    if !md.given.rnoic {
        md.rnoic = 0.395;
    }
    if !md.given.ntnoi {
        md.ntnoi = 1.0;
    }
    if !md.given.lambda {
        md.lambda = 0.0;
    }
    if !md.given.vtl {
        md.vtl = 2.0e5;
    }
    if !md.given.xn {
        md.xn = 3.0;
    }
    if !md.given.lc {
        md.lc = 5.0e-9;
    }
    if !md.given.vfbsdoff {
        md.vfbsdoff = 0.0;
    }
    if !md.given.tvfbsdoff {
        md.tvfbsdoff = 0.0;
    }
    if !md.given.tvoff {
        md.tvoff = 0.0;
    }
    if !md.given.tnfactor {
        md.tnfactor = 0.0;
    }
    if !md.given.teta0 {
        md.teta0 = 0.0;
    }
    if !md.given.tvoffcv {
        md.tvoffcv = 0.0;
    }
    if !md.given.lintnoi {
        md.lintnoi = 0.0;
    }
    if !md.given.xjbvs {
        md.xjbvs = 1.0;
    }
    if !md.given.xjbvd {
        md.xjbvd = md.xjbvs;
    }
    if !md.given.bvs {
        md.bvs = 10.0;
    }
    if !md.given.bvd {
        md.bvd = md.bvs;
    }
    if !md.given.gbmin {
        md.gbmin = 1.0e-12;
    }
    if !md.given.rbdb {
        md.rbdb = 50.0;
    }
    if !md.given.rbpb {
        md.rbpb = 50.0;
    }
    if !md.given.rbsb {
        md.rbsb = 50.0;
    }
    if !md.given.rbps {
        md.rbps = 50.0;
    }
    if !md.given.rbpd {
        md.rbpd = 50.0;
    }
    if !md.given.rbps0 {
        md.rbps0 = 50.0;
    }
    if !md.given.rbpsl {
        md.rbpsl = 0.0;
    }
    if !md.given.rbpsw {
        md.rbpsw = 0.0;
    }
    if !md.given.rbpsnf {
        md.rbpsnf = 0.0;
    }
    if !md.given.rbpd0 {
        md.rbpd0 = 50.0;
    }
    if !md.given.rbpdl {
        md.rbpdl = 0.0;
    }
    if !md.given.rbpdw {
        md.rbpdw = 0.0;
    }
    if !md.given.rbpdnf {
        md.rbpdnf = 0.0;
    }
    if !md.given.rbpbx0 {
        md.rbpbx0 = 100.0;
    }
    if !md.given.rbpbxl {
        md.rbpbxl = 0.0;
    }
    if !md.given.rbpbxw {
        md.rbpbxw = 0.0;
    }
    if !md.given.rbpbxnf {
        md.rbpbxnf = 0.0;
    }
    if !md.given.rbpby0 {
        md.rbpby0 = 100.0;
    }
    if !md.given.rbpbyl {
        md.rbpbyl = 0.0;
    }
    if !md.given.rbpbyw {
        md.rbpbyw = 0.0;
    }
    if !md.given.rbpbynf {
        md.rbpbynf = 0.0;
    }
    if !md.given.rbsbx0 {
        md.rbsbx0 = 100.0;
    }
    if !md.given.rbsby0 {
        md.rbsby0 = 100.0;
    }
    if !md.given.rbdbx0 {
        md.rbdbx0 = 100.0;
    }
    if !md.given.rbdby0 {
        md.rbdby0 = 100.0;
    }
    if !md.given.rbsdbxl {
        md.rbsdbxl = 0.0;
    }
    if !md.given.rbsdbxw {
        md.rbsdbxw = 0.0;
    }
    if !md.given.rbsdbxnf {
        md.rbsdbxnf = 0.0;
    }
    if !md.given.rbsdbyl {
        md.rbsdbyl = 0.0;
    }
    if !md.given.rbsdbyw {
        md.rbsdbyw = 0.0;
    }
    if !md.given.rbsdbynf {
        md.rbsdbynf = 0.0;
    }
    if !md.given.cgsl {
        md.cgsl = 0.0;
    }
    if !md.given.cgdl {
        md.cgdl = 0.0;
    }
    if !md.given.ckappas {
        md.ckappas = 0.6;
    }
    if !md.given.ckappad {
        md.ckappad = md.ckappas;
    }
    if !md.given.clc {
        md.clc = 0.1e-6;
    }
    if !md.given.cle {
        md.cle = 0.6;
    }
    if !md.given.vfbcv {
        md.vfbcv = -1.0;
    }
    if !md.given.acde {
        md.acde = 1.0;
    }
    if !md.given.moin {
        md.moin = 15.0;
    }
    if !md.given.noff {
        md.noff = 1.0;
    }
    if !md.given.voffcv {
        md.voffcv = 0.0;
    }
    if !md.given.dmcg {
        md.dmcg = 0.0;
    }
    if !md.given.dmci {
        md.dmci = md.dmcg;
    }
    if !md.given.dmdg {
        md.dmdg = 0.0;
    }
    if !md.given.dmcgt {
        md.dmcgt = 0.0;
    }
    if !md.given.xgw {
        md.xgw = 0.0;
    }
    if !md.given.xgl {
        md.xgl = 0.0;
    }
    if !md.given.rshg {
        md.rshg = 0.1;
    }
    if !md.given.ngcon {
        md.ngcon = 1.0;
    }
    if !md.given.tcj {
        md.tcj = 0.0;
    }
    if !md.given.tpb {
        md.tpb = 0.0;
    }
    if !md.given.tcjsw {
        md.tcjsw = 0.0;
    }
    if !md.given.tpbsw {
        md.tpbsw = 0.0;
    }
    if !md.given.tcjswg {
        md.tcjswg = 0.0;
    }
    if !md.given.tpbswg {
        md.tpbswg = 0.0;
    }
    if !md.given.lcdsc {
        md.lcdsc = 0.0;
    }
    if !md.given.lcdscb {
        md.lcdscb = 0.0;
    }
    if !md.given.lcdscd {
        md.lcdscd = 0.0;
    }
    if !md.given.lcit {
        md.lcit = 0.0;
    }
    if !md.given.lnfactor {
        md.lnfactor = 0.0;
    }
    if !md.given.lxj {
        md.lxj = 0.0;
    }
    if !md.given.lvsat {
        md.lvsat = 0.0;
    }
    if !md.given.lat {
        md.lat = 0.0;
    }
    if !md.given.la0 {
        md.la0 = 0.0;
    }
    if !md.given.lags {
        md.lags = 0.0;
    }
    if !md.given.la1 {
        md.la1 = 0.0;
    }
    if !md.given.la2 {
        md.la2 = 0.0;
    }
    if !md.given.lketa {
        md.lketa = 0.0;
    }
    if !md.given.lketac {
        md.lketac = md.lketa;
    }
    if !md.given.lnsub {
        md.lnsub = 0.0;
    }
    if !md.given.lndep {
        md.lndep = 0.0;
    }
    if !md.given.lnsd {
        md.lnsd = 0.0;
    }
    if !md.given.lphin {
        md.lphin = 0.0;
    }
    if !md.given.lngate {
        md.lngate = 0.0;
    }
    if !md.given.lvbm {
        md.lvbm = 0.0;
    }
    if !md.given.lxt {
        md.lxt = 0.0;
    }
    if !md.given.lk1 {
        md.lk1 = 0.0;
    }
    if !md.given.lkt1 {
        md.lkt1 = 0.0;
    }
    if !md.given.lkt1l {
        md.lkt1l = 0.0;
    }
    if !md.given.lkt2 {
        md.lkt2 = 0.0;
    }
    if !md.given.lk2 {
        md.lk2 = 0.0;
    }
    if !md.given.lk3 {
        md.lk3 = 0.0;
    }
    if !md.given.lk3b {
        md.lk3b = 0.0;
    }
    if !md.given.lw0 {
        md.lw0 = 0.0;
    }
    if !md.given.llpe0 {
        md.llpe0 = 0.0;
    }
    if !md.given.llpeb {
        md.llpeb = 0.0;
    }
    if !md.given.ldvtp0 {
        md.ldvtp0 = 0.0;
    }
    if !md.given.ldvtp1 {
        md.ldvtp1 = 0.0;
    }
    if !md.given.ldvtp2 {
        md.ldvtp2 = 0.0;
    }
    if !md.given.ldvtp3 {
        md.ldvtp3 = 0.0;
    }
    if !md.given.ldvtp4 {
        md.ldvtp4 = 0.0;
    }
    if !md.given.ldvtp5 {
        md.ldvtp5 = 0.0;
    }
    if !md.given.ldvt0 {
        md.ldvt0 = 0.0;
    }
    if !md.given.ldvt1 {
        md.ldvt1 = 0.0;
    }
    if !md.given.ldvt2 {
        md.ldvt2 = 0.0;
    }
    if !md.given.ldvt0w {
        md.ldvt0w = 0.0;
    }
    if !md.given.ldvt1w {
        md.ldvt1w = 0.0;
    }
    if !md.given.ldvt2w {
        md.ldvt2w = 0.0;
    }
    if !md.given.ldrout {
        md.ldrout = 0.0;
    }
    if !md.given.ldsub {
        md.ldsub = 0.0;
    }
    if !md.given.lvth0 {
        md.lvth0 = 0.0;
    }
    if !md.given.lua {
        md.lua = 0.0;
    }
    if !md.given.lua1 {
        md.lua1 = 0.0;
    }
    if !md.given.lub {
        md.lub = 0.0;
    }
    if !md.given.lub1 {
        md.lub1 = 0.0;
    }
    if !md.given.luc {
        md.luc = 0.0;
    }
    if !md.given.luc1 {
        md.luc1 = 0.0;
    }
    if !md.given.lud {
        md.lud = 0.0;
    }
    if !md.given.lud1 {
        md.lud1 = 0.0;
    }
    if !md.given.lup {
        md.lup = 0.0;
    }
    if !md.given.llp {
        md.llp = 0.0;
    }
    if !md.given.lu0 {
        md.lu0 = 0.0;
    }
    if !md.given.lute {
        md.lute = 0.0;
    }
    if !md.given.lucste {
        md.lucste = 0.0;
    }
    if !md.given.lvoff {
        md.lvoff = 0.0;
    }
    if !md.given.lminv {
        md.lminv = 0.0;
    }
    if !md.given.lminvcv {
        md.lminvcv = 0.0;
    }
    if !md.given.lfprout {
        md.lfprout = 0.0;
    }
    if !md.given.lpdits {
        md.lpdits = 0.0;
    }
    if !md.given.lpditsd {
        md.lpditsd = 0.0;
    }
    if !md.given.ldelta {
        md.ldelta = 0.0;
    }
    if !md.given.lrdsw {
        md.lrdsw = 0.0;
    }
    if !md.given.lrdw {
        md.lrdw = 0.0;
    }
    if !md.given.lrsw {
        md.lrsw = 0.0;
    }
    if !md.given.lprwb {
        md.lprwb = 0.0;
    }
    if !md.given.lprwg {
        md.lprwg = 0.0;
    }
    if !md.given.lprt {
        md.lprt = 0.0;
    }
    if !md.given.leta0 {
        md.leta0 = 0.0;
    }
    if !md.given.letab {
        md.letab = -0.0;
    }
    if !md.given.lpclm {
        md.lpclm = 0.0;
    }
    if !md.given.lpdibl1 {
        md.lpdibl1 = 0.0;
    }
    if !md.given.lpdibl2 {
        md.lpdibl2 = 0.0;
    }
    if !md.given.lpdiblb {
        md.lpdiblb = 0.0;
    }
    if !md.given.lpscbe1 {
        md.lpscbe1 = 0.0;
    }
    if !md.given.lpscbe2 {
        md.lpscbe2 = 0.0;
    }
    if !md.given.lpvag {
        md.lpvag = 0.0;
    }
    if !md.given.lwr {
        md.lwr = 0.0;
    }
    if !md.given.ldwg {
        md.ldwg = 0.0;
    }
    if !md.given.ldwb {
        md.ldwb = 0.0;
    }
    if !md.given.lb0 {
        md.lb0 = 0.0;
    }
    if !md.given.lb1 {
        md.lb1 = 0.0;
    }
    if !md.given.lalpha0 {
        md.lalpha0 = 0.0;
    }
    if !md.given.lalpha1 {
        md.lalpha1 = 0.0;
    }
    if !md.given.lbeta0 {
        md.lbeta0 = 0.0;
    }
    if !md.given.lagidl {
        md.lagidl = 0.0;
    }
    if !md.given.lbgidl {
        md.lbgidl = 0.0;
    }
    if !md.given.lcgidl {
        md.lcgidl = 0.0;
    }
    if !md.given.legidl {
        md.legidl = 0.0;
    }
    if !md.given.lrgidl {
        md.lrgidl = 0.0;
    }
    if !md.given.lkgidl {
        md.lkgidl = 0.0;
    }
    if !md.given.lfgidl {
        md.lfgidl = 0.0;
    }
    if !md.given.lagisl {
        md.lagisl = md.lagidl;
    }
    if !md.given.lbgisl {
        md.lbgisl = md.lbgidl;
    }
    if !md.given.lcgisl {
        md.lcgisl = md.lcgidl;
    }
    if !md.given.legisl {
        md.legisl = md.legidl;
    }
    if !md.given.lrgisl {
        md.lrgisl = md.lrgidl;
    }
    if !md.given.lkgisl {
        md.lkgisl = md.lkgidl;
    }
    if !md.given.lfgisl {
        md.lfgisl = md.lfgidl;
    }
    if !md.given.laigc {
        md.laigc = 0.0;
    }
    if !md.given.lbigc {
        md.lbigc = 0.0;
    }
    if !md.given.lcigc {
        md.lcigc = 0.0;
    }
    if !md.given.aigsd && (md.given.aigs || md.given.aigd) {
        if !md.given.laigs {
            md.laigs = 0.0;
        }
        if !md.given.laigd {
            md.laigd = 0.0;
        }
    } else {
        if !md.given.laigsd {
            md.laigsd = 0.0;
        }
        md.laigd = md.laigsd;
        md.laigs = md.laigd;
    }
    if !md.given.bigsd && (md.given.bigs || md.given.bigd) {
        if !md.given.lbigs {
            md.lbigs = 0.0;
        }
        if !md.given.lbigd {
            md.lbigd = 0.0;
        }
    } else {
        if !md.given.lbigsd {
            md.lbigsd = 0.0;
        }
        md.lbigd = md.lbigsd;
        md.lbigs = md.lbigd;
    }
    if !md.given.cigsd && (md.given.cigs || md.given.cigd) {
        if !md.given.lcigs {
            md.lcigs = 0.0;
        }
        if !md.given.lcigd {
            md.lcigd = 0.0;
        }
    } else {
        if !md.given.lcigsd {
            md.lcigsd = 0.0;
        }
        md.lcigd = md.lcigsd;
        md.lcigs = md.lcigd;
    }
    if !md.given.laigbacc {
        md.laigbacc = 0.0;
    }
    if !md.given.lbigbacc {
        md.lbigbacc = 0.0;
    }
    if !md.given.lcigbacc {
        md.lcigbacc = 0.0;
    }
    if !md.given.laigbinv {
        md.laigbinv = 0.0;
    }
    if !md.given.lbigbinv {
        md.lbigbinv = 0.0;
    }
    if !md.given.lcigbinv {
        md.lcigbinv = 0.0;
    }
    if !md.given.lnigc {
        md.lnigc = 0.0;
    }
    if !md.given.lnigbinv {
        md.lnigbinv = 0.0;
    }
    if !md.given.lnigbacc {
        md.lnigbacc = 0.0;
    }
    if !md.given.lntox {
        md.lntox = 0.0;
    }
    if !md.given.leigbinv {
        md.leigbinv = 0.0;
    }
    if !md.given.lpigcd {
        md.lpigcd = 0.0;
    }
    if !md.given.lpoxedge {
        md.lpoxedge = 0.0;
    }
    if !md.given.lxrcrg1 {
        md.lxrcrg1 = 0.0;
    }
    if !md.given.lxrcrg2 {
        md.lxrcrg2 = 0.0;
    }
    if !md.given.leu {
        md.leu = 0.0;
    }
    if !md.given.lucs {
        md.lucs = 0.0;
    }
    if !md.given.lvfb {
        md.lvfb = 0.0;
    }
    if !md.given.llambda {
        md.llambda = 0.0;
    }
    if !md.given.lvtl {
        md.lvtl = 0.0;
    }
    if !md.given.lxn {
        md.lxn = 0.0;
    }
    if !md.given.lvfbsdoff {
        md.lvfbsdoff = 0.0;
    }
    if !md.given.ltvfbsdoff {
        md.ltvfbsdoff = 0.0;
    }
    if !md.given.ltvoff {
        md.ltvoff = 0.0;
    }
    if !md.given.ltnfactor {
        md.ltnfactor = 0.0;
    }
    if !md.given.lteta0 {
        md.lteta0 = 0.0;
    }
    if !md.given.ltvoffcv {
        md.ltvoffcv = 0.0;
    }
    if !md.given.lcgsl {
        md.lcgsl = 0.0;
    }
    if !md.given.lcgdl {
        md.lcgdl = 0.0;
    }
    if !md.given.lckappas {
        md.lckappas = 0.0;
    }
    if !md.given.lckappad {
        md.lckappad = 0.0;
    }
    if !md.given.lclc {
        md.lclc = 0.0;
    }
    if !md.given.lcle {
        md.lcle = 0.0;
    }
    if !md.given.lcf {
        md.lcf = 0.0;
    }
    if !md.given.lvfbcv {
        md.lvfbcv = 0.0;
    }
    if !md.given.lacde {
        md.lacde = 0.0;
    }
    if !md.given.lmoin {
        md.lmoin = 0.0;
    }
    if !md.given.lnoff {
        md.lnoff = 0.0;
    }
    if !md.given.lvoffcv {
        md.lvoffcv = 0.0;
    }
    if !md.given.wcdsc {
        md.wcdsc = 0.0;
    }
    if !md.given.wcdscb {
        md.wcdscb = 0.0;
    }
    if !md.given.wcdscd {
        md.wcdscd = 0.0;
    }
    if !md.given.wcit {
        md.wcit = 0.0;
    }
    if !md.given.wnfactor {
        md.wnfactor = 0.0;
    }
    if !md.given.wxj {
        md.wxj = 0.0;
    }
    if !md.given.wvsat {
        md.wvsat = 0.0;
    }
    if !md.given.wat {
        md.wat = 0.0;
    }
    if !md.given.wa0 {
        md.wa0 = 0.0;
    }
    if !md.given.wags {
        md.wags = 0.0;
    }
    if !md.given.wa1 {
        md.wa1 = 0.0;
    }
    if !md.given.wa2 {
        md.wa2 = 0.0;
    }
    if !md.given.wketa {
        md.wketa = 0.0;
    }
    if !md.given.wketac {
        md.wketac = md.wketa;
    }
    if !md.given.wnsub {
        md.wnsub = 0.0;
    }
    if !md.given.wndep {
        md.wndep = 0.0;
    }
    if !md.given.wnsd {
        md.wnsd = 0.0;
    }
    if !md.given.wphin {
        md.wphin = 0.0;
    }
    if !md.given.wngate {
        md.wngate = 0.0;
    }
    if !md.given.wvbm {
        md.wvbm = 0.0;
    }
    if !md.given.wxt {
        md.wxt = 0.0;
    }
    if !md.given.wk1 {
        md.wk1 = 0.0;
    }
    if !md.given.wkt1 {
        md.wkt1 = 0.0;
    }
    if !md.given.wkt1l {
        md.wkt1l = 0.0;
    }
    if !md.given.wkt2 {
        md.wkt2 = 0.0;
    }
    if !md.given.wk2 {
        md.wk2 = 0.0;
    }
    if !md.given.wk3 {
        md.wk3 = 0.0;
    }
    if !md.given.wk3b {
        md.wk3b = 0.0;
    }
    if !md.given.ww0 {
        md.ww0 = 0.0;
    }
    if !md.given.wlpe0 {
        md.wlpe0 = 0.0;
    }
    if !md.given.wlpeb {
        md.wlpeb = 0.0;
    }
    if !md.given.wdvtp0 {
        md.wdvtp0 = 0.0;
    }
    if !md.given.wdvtp1 {
        md.wdvtp1 = 0.0;
    }
    if !md.given.wdvtp2 {
        md.wdvtp2 = 0.0;
    }
    if !md.given.wdvtp3 {
        md.wdvtp3 = 0.0;
    }
    if !md.given.wdvtp4 {
        md.wdvtp4 = 0.0;
    }
    if !md.given.wdvtp5 {
        md.wdvtp5 = 0.0;
    }
    if !md.given.wdvt0 {
        md.wdvt0 = 0.0;
    }
    if !md.given.wdvt1 {
        md.wdvt1 = 0.0;
    }
    if !md.given.wdvt2 {
        md.wdvt2 = 0.0;
    }
    if !md.given.wdvt0w {
        md.wdvt0w = 0.0;
    }
    if !md.given.wdvt1w {
        md.wdvt1w = 0.0;
    }
    if !md.given.wdvt2w {
        md.wdvt2w = 0.0;
    }
    if !md.given.wdrout {
        md.wdrout = 0.0;
    }
    if !md.given.wdsub {
        md.wdsub = 0.0;
    }
    if !md.given.wvth0 {
        md.wvth0 = 0.0;
    }
    if !md.given.wua {
        md.wua = 0.0;
    }
    if !md.given.wua1 {
        md.wua1 = 0.0;
    }
    if !md.given.wub {
        md.wub = 0.0;
    }
    if !md.given.wub1 {
        md.wub1 = 0.0;
    }
    if !md.given.wuc {
        md.wuc = 0.0;
    }
    if !md.given.wuc1 {
        md.wuc1 = 0.0;
    }
    if !md.given.wud {
        md.wud = 0.0;
    }
    if !md.given.wud1 {
        md.wud1 = 0.0;
    }
    if !md.given.wup {
        md.wup = 0.0;
    }
    if !md.given.wlp {
        md.wlp = 0.0;
    }
    if !md.given.wu0 {
        md.wu0 = 0.0;
    }
    if !md.given.wute {
        md.wute = 0.0;
    }
    if !md.given.wucste {
        md.wucste = 0.0;
    }
    if !md.given.wvoff {
        md.wvoff = 0.0;
    }
    if !md.given.wminv {
        md.wminv = 0.0;
    }
    if !md.given.wminvcv {
        md.wminvcv = 0.0;
    }
    if !md.given.wfprout {
        md.wfprout = 0.0;
    }
    if !md.given.wpdits {
        md.wpdits = 0.0;
    }
    if !md.given.wpditsd {
        md.wpditsd = 0.0;
    }
    if !md.given.wdelta {
        md.wdelta = 0.0;
    }
    if !md.given.wrdsw {
        md.wrdsw = 0.0;
    }
    if !md.given.wrdw {
        md.wrdw = 0.0;
    }
    if !md.given.wrsw {
        md.wrsw = 0.0;
    }
    if !md.given.wprwb {
        md.wprwb = 0.0;
    }
    if !md.given.wprwg {
        md.wprwg = 0.0;
    }
    if !md.given.wprt {
        md.wprt = 0.0;
    }
    if !md.given.weta0 {
        md.weta0 = 0.0;
    }
    if !md.given.wetab {
        md.wetab = 0.0;
    }
    if !md.given.wpclm {
        md.wpclm = 0.0;
    }
    if !md.given.wpdibl1 {
        md.wpdibl1 = 0.0;
    }
    if !md.given.wpdibl2 {
        md.wpdibl2 = 0.0;
    }
    if !md.given.wpdiblb {
        md.wpdiblb = 0.0;
    }
    if !md.given.wpscbe1 {
        md.wpscbe1 = 0.0;
    }
    if !md.given.wpscbe2 {
        md.wpscbe2 = 0.0;
    }
    if !md.given.wpvag {
        md.wpvag = 0.0;
    }
    if !md.given.wwr {
        md.wwr = 0.0;
    }
    if !md.given.wdwg {
        md.wdwg = 0.0;
    }
    if !md.given.wdwb {
        md.wdwb = 0.0;
    }
    if !md.given.wb0 {
        md.wb0 = 0.0;
    }
    if !md.given.wb1 {
        md.wb1 = 0.0;
    }
    if !md.given.walpha0 {
        md.walpha0 = 0.0;
    }
    if !md.given.walpha1 {
        md.walpha1 = 0.0;
    }
    if !md.given.wbeta0 {
        md.wbeta0 = 0.0;
    }
    if !md.given.wagidl {
        md.wagidl = 0.0;
    }
    if !md.given.wbgidl {
        md.wbgidl = 0.0;
    }
    if !md.given.wcgidl {
        md.wcgidl = 0.0;
    }
    if !md.given.wegidl {
        md.wegidl = 0.0;
    }
    if !md.given.wrgidl {
        md.wrgidl = 0.0;
    }
    if !md.given.wkgidl {
        md.wkgidl = 0.0;
    }
    if !md.given.wfgidl {
        md.wfgidl = 0.0;
    }
    if !md.given.wagisl {
        md.wagisl = md.wagidl;
    }
    if !md.given.wbgisl {
        md.wbgisl = md.wbgidl;
    }
    if !md.given.wcgisl {
        md.wcgisl = md.wcgidl;
    }
    if !md.given.wegisl {
        md.wegisl = md.wegidl;
    }
    if !md.given.wrgisl {
        md.wrgisl = md.wrgidl;
    }
    if !md.given.wkgisl {
        md.wkgisl = md.wkgidl;
    }
    if !md.given.wfgisl {
        md.wfgisl = md.wfgidl;
    }
    if !md.given.waigc {
        md.waigc = 0.0;
    }
    if !md.given.wbigc {
        md.wbigc = 0.0;
    }
    if !md.given.wcigc {
        md.wcigc = 0.0;
    }
    if !md.given.aigsd && (md.given.aigs || md.given.aigd) {
        if !md.given.waigs {
            md.waigs = 0.0;
        }
        if !md.given.waigd {
            md.waigd = 0.0;
        }
    } else {
        if !md.given.waigsd {
            md.waigsd = 0.0;
        }
        md.waigd = md.waigsd;
        md.waigs = md.waigd;
    }
    if !md.given.bigsd && (md.given.bigs || md.given.bigd) {
        if !md.given.wbigs {
            md.wbigs = 0.0;
        }
        if !md.given.wbigd {
            md.wbigd = 0.0;
        }
    } else {
        if !md.given.wbigsd {
            md.wbigsd = 0.0;
        }
        md.wbigd = md.wbigsd;
        md.wbigs = md.wbigd;
    }
    if !md.given.cigsd && (md.given.cigs || md.given.cigd) {
        if !md.given.wcigs {
            md.wcigs = 0.0;
        }
        if !md.given.wcigd {
            md.wcigd = 0.0;
        }
    } else {
        if !md.given.wcigsd {
            md.wcigsd = 0.0;
        }
        md.wcigd = md.wcigsd;
        md.wcigs = md.wcigd;
    }
    if !md.given.waigbacc {
        md.waigbacc = 0.0;
    }
    if !md.given.wbigbacc {
        md.wbigbacc = 0.0;
    }
    if !md.given.wcigbacc {
        md.wcigbacc = 0.0;
    }
    if !md.given.waigbinv {
        md.waigbinv = 0.0;
    }
    if !md.given.wbigbinv {
        md.wbigbinv = 0.0;
    }
    if !md.given.wcigbinv {
        md.wcigbinv = 0.0;
    }
    if !md.given.wnigc {
        md.wnigc = 0.0;
    }
    if !md.given.wnigbinv {
        md.wnigbinv = 0.0;
    }
    if !md.given.wnigbacc {
        md.wnigbacc = 0.0;
    }
    if !md.given.wntox {
        md.wntox = 0.0;
    }
    if !md.given.weigbinv {
        md.weigbinv = 0.0;
    }
    if !md.given.wpigcd {
        md.wpigcd = 0.0;
    }
    if !md.given.wpoxedge {
        md.wpoxedge = 0.0;
    }
    if !md.given.wxrcrg1 {
        md.wxrcrg1 = 0.0;
    }
    if !md.given.wxrcrg2 {
        md.wxrcrg2 = 0.0;
    }
    if !md.given.weu {
        md.weu = 0.0;
    }
    if !md.given.wucs {
        md.wucs = 0.0;
    }
    if !md.given.wvfb {
        md.wvfb = 0.0;
    }
    if !md.given.wlambda {
        md.wlambda = 0.0;
    }
    if !md.given.wvtl {
        md.wvtl = 0.0;
    }
    if !md.given.wxn {
        md.wxn = 0.0;
    }
    if !md.given.wvfbsdoff {
        md.wvfbsdoff = 0.0;
    }
    if !md.given.wtvfbsdoff {
        md.wtvfbsdoff = 0.0;
    }
    if !md.given.wtvoff {
        md.wtvoff = 0.0;
    }
    if !md.given.wtnfactor {
        md.wtnfactor = 0.0;
    }
    if !md.given.wteta0 {
        md.wteta0 = 0.0;
    }
    if !md.given.wtvoffcv {
        md.wtvoffcv = 0.0;
    }
    if !md.given.wcgsl {
        md.wcgsl = 0.0;
    }
    if !md.given.wcgdl {
        md.wcgdl = 0.0;
    }
    if !md.given.wckappas {
        md.wckappas = 0.0;
    }
    if !md.given.wckappad {
        md.wckappad = 0.0;
    }
    if !md.given.wcf {
        md.wcf = 0.0;
    }
    if !md.given.wclc {
        md.wclc = 0.0;
    }
    if !md.given.wcle {
        md.wcle = 0.0;
    }
    if !md.given.wvfbcv {
        md.wvfbcv = 0.0;
    }
    if !md.given.wacde {
        md.wacde = 0.0;
    }
    if !md.given.wmoin {
        md.wmoin = 0.0;
    }
    if !md.given.wnoff {
        md.wnoff = 0.0;
    }
    if !md.given.wvoffcv {
        md.wvoffcv = 0.0;
    }
    if !md.given.pcdsc {
        md.pcdsc = 0.0;
    }
    if !md.given.pcdscb {
        md.pcdscb = 0.0;
    }
    if !md.given.pcdscd {
        md.pcdscd = 0.0;
    }
    if !md.given.pcit {
        md.pcit = 0.0;
    }
    if !md.given.pnfactor {
        md.pnfactor = 0.0;
    }
    if !md.given.pxj {
        md.pxj = 0.0;
    }
    if !md.given.pvsat {
        md.pvsat = 0.0;
    }
    if !md.given.pat {
        md.pat = 0.0;
    }
    if !md.given.pa0 {
        md.pa0 = 0.0;
    }
    if !md.given.pags {
        md.pags = 0.0;
    }
    if !md.given.pa1 {
        md.pa1 = 0.0;
    }
    if !md.given.pa2 {
        md.pa2 = 0.0;
    }
    if !md.given.pketa {
        md.pketa = 0.0;
    }
    if !md.given.pketac {
        md.pketac = md.pketa;
    }
    if !md.given.pnsub {
        md.pnsub = 0.0;
    }
    if !md.given.pndep {
        md.pndep = 0.0;
    }
    if !md.given.pnsd {
        md.pnsd = 0.0;
    }
    if !md.given.pphin {
        md.pphin = 0.0;
    }
    if !md.given.pngate {
        md.pngate = 0.0;
    }
    if !md.given.pvbm {
        md.pvbm = 0.0;
    }
    if !md.given.pxt {
        md.pxt = 0.0;
    }
    if !md.given.pk1 {
        md.pk1 = 0.0;
    }
    if !md.given.pkt1 {
        md.pkt1 = 0.0;
    }
    if !md.given.pkt1l {
        md.pkt1l = 0.0;
    }
    if !md.given.pk2 {
        md.pk2 = 0.0;
    }
    if !md.given.pkt2 {
        md.pkt2 = 0.0;
    }
    if !md.given.pk3 {
        md.pk3 = 0.0;
    }
    if !md.given.pk3b {
        md.pk3b = 0.0;
    }
    if !md.given.pw0 {
        md.pw0 = 0.0;
    }
    if !md.given.plpe0 {
        md.plpe0 = 0.0;
    }
    if !md.given.plpeb {
        md.plpeb = 0.0;
    }
    if !md.given.pdvtp0 {
        md.pdvtp0 = 0.0;
    }
    if !md.given.pdvtp1 {
        md.pdvtp1 = 0.0;
    }
    if !md.given.pdvtp2 {
        md.pdvtp2 = 0.0;
    }
    if !md.given.pdvtp3 {
        md.pdvtp3 = 0.0;
    }
    if !md.given.pdvtp4 {
        md.pdvtp4 = 0.0;
    }
    if !md.given.pdvtp5 {
        md.pdvtp5 = 0.0;
    }
    if !md.given.pdvt0 {
        md.pdvt0 = 0.0;
    }
    if !md.given.pdvt1 {
        md.pdvt1 = 0.0;
    }
    if !md.given.pdvt2 {
        md.pdvt2 = 0.0;
    }
    if !md.given.pdvt0w {
        md.pdvt0w = 0.0;
    }
    if !md.given.pdvt1w {
        md.pdvt1w = 0.0;
    }
    if !md.given.pdvt2w {
        md.pdvt2w = 0.0;
    }
    if !md.given.pdrout {
        md.pdrout = 0.0;
    }
    if !md.given.pdsub {
        md.pdsub = 0.0;
    }
    if !md.given.pvth0 {
        md.pvth0 = 0.0;
    }
    if !md.given.pua {
        md.pua = 0.0;
    }
    if !md.given.pua1 {
        md.pua1 = 0.0;
    }
    if !md.given.pub_ {
        md.pub_ = 0.0;
    }
    if !md.given.pub1 {
        md.pub1 = 0.0;
    }
    if !md.given.puc {
        md.puc = 0.0;
    }
    if !md.given.puc1 {
        md.puc1 = 0.0;
    }
    if !md.given.pud {
        md.pud = 0.0;
    }
    if !md.given.pud1 {
        md.pud1 = 0.0;
    }
    if !md.given.pup {
        md.pup = 0.0;
    }
    if !md.given.plp {
        md.plp = 0.0;
    }
    if !md.given.pu0 {
        md.pu0 = 0.0;
    }
    if !md.given.pute {
        md.pute = 0.0;
    }
    if !md.given.pucste {
        md.pucste = 0.0;
    }
    if !md.given.pvoff {
        md.pvoff = 0.0;
    }
    if !md.given.pminv {
        md.pminv = 0.0;
    }
    if !md.given.pminvcv {
        md.pminvcv = 0.0;
    }
    if !md.given.pfprout {
        md.pfprout = 0.0;
    }
    if !md.given.ppdits {
        md.ppdits = 0.0;
    }
    if !md.given.ppditsd {
        md.ppditsd = 0.0;
    }
    if !md.given.pdelta {
        md.pdelta = 0.0;
    }
    if !md.given.prdsw {
        md.prdsw = 0.0;
    }
    if !md.given.prdw {
        md.prdw = 0.0;
    }
    if !md.given.prsw {
        md.prsw = 0.0;
    }
    if !md.given.pprwb {
        md.pprwb = 0.0;
    }
    if !md.given.pprwg {
        md.pprwg = 0.0;
    }
    if !md.given.pprt {
        md.pprt = 0.0;
    }
    if !md.given.peta0 {
        md.peta0 = 0.0;
    }
    if !md.given.petab {
        md.petab = 0.0;
    }
    if !md.given.ppclm {
        md.ppclm = 0.0;
    }
    if !md.given.ppdibl1 {
        md.ppdibl1 = 0.0;
    }
    if !md.given.ppdibl2 {
        md.ppdibl2 = 0.0;
    }
    if !md.given.ppdiblb {
        md.ppdiblb = 0.0;
    }
    if !md.given.ppscbe1 {
        md.ppscbe1 = 0.0;
    }
    if !md.given.ppscbe2 {
        md.ppscbe2 = 0.0;
    }
    if !md.given.ppvag {
        md.ppvag = 0.0;
    }
    if !md.given.pwr {
        md.pwr = 0.0;
    }
    if !md.given.pdwg {
        md.pdwg = 0.0;
    }
    if !md.given.pdwb {
        md.pdwb = 0.0;
    }
    if !md.given.pb0 {
        md.pb0 = 0.0;
    }
    if !md.given.pb1 {
        md.pb1 = 0.0;
    }
    if !md.given.palpha0 {
        md.palpha0 = 0.0;
    }
    if !md.given.palpha1 {
        md.palpha1 = 0.0;
    }
    if !md.given.pbeta0 {
        md.pbeta0 = 0.0;
    }
    if !md.given.pagidl {
        md.pagidl = 0.0;
    }
    if !md.given.pbgidl {
        md.pbgidl = 0.0;
    }
    if !md.given.pcgidl {
        md.pcgidl = 0.0;
    }
    if !md.given.pegidl {
        md.pegidl = 0.0;
    }
    if !md.given.prgidl {
        md.prgidl = 0.0;
    }
    if !md.given.pkgidl {
        md.pkgidl = 0.0;
    }
    if !md.given.pfgidl {
        md.pfgidl = 0.0;
    }
    if !md.given.pagisl {
        md.pagisl = md.pagidl;
    }
    if !md.given.pbgisl {
        md.pbgisl = md.pbgidl;
    }
    if !md.given.pcgisl {
        md.pcgisl = md.pcgidl;
    }
    if !md.given.pegisl {
        md.pegisl = md.pegidl;
    }
    if !md.given.prgisl {
        md.prgisl = md.prgidl;
    }
    if !md.given.pkgisl {
        md.pkgisl = md.pkgidl;
    }
    if !md.given.pfgisl {
        md.pfgisl = md.pfgidl;
    }
    if !md.given.paigc {
        md.paigc = 0.0;
    }
    if !md.given.pbigc {
        md.pbigc = 0.0;
    }
    if !md.given.pcigc {
        md.pcigc = 0.0;
    }
    if !md.given.aigsd && (md.given.aigs || md.given.aigd) {
        if !md.given.paigs {
            md.paigs = 0.0;
        }
        if !md.given.paigd {
            md.paigd = 0.0;
        }
    } else {
        if !md.given.paigsd {
            md.paigsd = 0.0;
        }
        md.paigd = md.paigsd;
        md.paigs = md.paigd;
    }
    if !md.given.bigsd && (md.given.bigs || md.given.bigd) {
        if !md.given.pbigs {
            md.pbigs = 0.0;
        }
        if !md.given.pbigd {
            md.pbigd = 0.0;
        }
    } else {
        if !md.given.pbigsd {
            md.pbigsd = 0.0;
        }
        md.pbigd = md.pbigsd;
        md.pbigs = md.pbigd;
    }
    if !md.given.cigsd && (md.given.cigs || md.given.cigd) {
        if !md.given.pcigs {
            md.pcigs = 0.0;
        }
        if !md.given.pcigd {
            md.pcigd = 0.0;
        }
    } else {
        if !md.given.pcigsd {
            md.pcigsd = 0.0;
        }
        md.pcigd = md.pcigsd;
        md.pcigs = md.pcigd;
    }
    if !md.given.paigbacc {
        md.paigbacc = 0.0;
    }
    if !md.given.pbigbacc {
        md.pbigbacc = 0.0;
    }
    if !md.given.pcigbacc {
        md.pcigbacc = 0.0;
    }
    if !md.given.paigbinv {
        md.paigbinv = 0.0;
    }
    if !md.given.pbigbinv {
        md.pbigbinv = 0.0;
    }
    if !md.given.pcigbinv {
        md.pcigbinv = 0.0;
    }
    if !md.given.pnigc {
        md.pnigc = 0.0;
    }
    if !md.given.pnigbinv {
        md.pnigbinv = 0.0;
    }
    if !md.given.pnigbacc {
        md.pnigbacc = 0.0;
    }
    if !md.given.pntox {
        md.pntox = 0.0;
    }
    if !md.given.peigbinv {
        md.peigbinv = 0.0;
    }
    if !md.given.ppigcd {
        md.ppigcd = 0.0;
    }
    if !md.given.ppoxedge {
        md.ppoxedge = 0.0;
    }
    if !md.given.pxrcrg1 {
        md.pxrcrg1 = 0.0;
    }
    if !md.given.pxrcrg2 {
        md.pxrcrg2 = 0.0;
    }
    if !md.given.peu {
        md.peu = 0.0;
    }
    if !md.given.pucs {
        md.pucs = 0.0;
    }
    if !md.given.pvfb {
        md.pvfb = 0.0;
    }
    if !md.given.plambda {
        md.plambda = 0.0;
    }
    if !md.given.pvtl {
        md.pvtl = 0.0;
    }
    if !md.given.pxn {
        md.pxn = 0.0;
    }
    if !md.given.pvfbsdoff {
        md.pvfbsdoff = 0.0;
    }
    if !md.given.ptvfbsdoff {
        md.ptvfbsdoff = 0.0;
    }
    if !md.given.ptvoff {
        md.ptvoff = 0.0;
    }
    if !md.given.ptnfactor {
        md.ptnfactor = 0.0;
    }
    if !md.given.pteta0 {
        md.pteta0 = 0.0;
    }
    if !md.given.ptvoffcv {
        md.ptvoffcv = 0.0;
    }
    if !md.given.pcgsl {
        md.pcgsl = 0.0;
    }
    if !md.given.pcgdl {
        md.pcgdl = 0.0;
    }
    if !md.given.pckappas {
        md.pckappas = 0.0;
    }
    if !md.given.pckappad {
        md.pckappad = 0.0;
    }
    if !md.given.pcf {
        md.pcf = 0.0;
    }
    if !md.given.pclc {
        md.pclc = 0.0;
    }
    if !md.given.pcle {
        md.pcle = 0.0;
    }
    if !md.given.pvfbcv {
        md.pvfbcv = 0.0;
    }
    if !md.given.pacde {
        md.pacde = 0.0;
    }
    if !md.given.pmoin {
        md.pmoin = 0.0;
    }
    if !md.given.pnoff {
        md.pnoff = 0.0;
    }
    if !md.given.pvoffcv {
        md.pvoffcv = 0.0;
    }
    if !md.given.gamma1 {
        md.gamma1 = 0.0;
    }
    if !md.given.lgamma1 {
        md.lgamma1 = 0.0;
    }
    if !md.given.wgamma1 {
        md.wgamma1 = 0.0;
    }
    if !md.given.pgamma1 {
        md.pgamma1 = 0.0;
    }
    if !md.given.gamma2 {
        md.gamma2 = 0.0;
    }
    if !md.given.lgamma2 {
        md.lgamma2 = 0.0;
    }
    if !md.given.wgamma2 {
        md.wgamma2 = 0.0;
    }
    if !md.given.pgamma2 {
        md.pgamma2 = 0.0;
    }
    if !md.given.vbx {
        md.vbx = 0.0;
    }
    if !md.given.lvbx {
        md.lvbx = 0.0;
    }
    if !md.given.wvbx {
        md.wvbx = 0.0;
    }
    if !md.given.pvbx {
        md.pvbx = 0.0;
    }
    if !md.given.tnom {
        md.tnom = ck_tnom;
    }
    if !md.given.lint {
        md.lint = 0.0;
    }
    if !md.given.ll {
        md.ll = 0.0;
    }
    if !md.given.llc {
        md.llc = md.ll;
    }
    if !md.given.lln {
        md.lln = 1.0;
    }
    if !md.given.lw {
        md.lw = 0.0;
    }
    if !md.given.lwc {
        md.lwc = md.lw;
    }
    if !md.given.lwn {
        md.lwn = 1.0;
    }
    if !md.given.lwl {
        md.lwl = 0.0;
    }
    if !md.given.lwlc {
        md.lwlc = md.lwl;
    }
    if !md.given.lmin {
        md.lmin = 0.0;
    }
    if !md.given.lmax {
        md.lmax = 1.0;
    }
    if !md.given.wint {
        md.wint = 0.0;
    }
    if !md.given.wl {
        md.wl = 0.0;
    }
    if !md.given.wlc {
        md.wlc = md.wl;
    }
    if !md.given.wln {
        md.wln = 1.0;
    }
    if !md.given.ww {
        md.ww = 0.0;
    }
    if !md.given.wwc {
        md.wwc = md.ww;
    }
    if !md.given.wwn {
        md.wwn = 1.0;
    }
    if !md.given.wwl {
        md.wwl = 0.0;
    }
    if !md.given.wwlc {
        md.wwlc = md.wwl;
    }
    if !md.given.wmin {
        md.wmin = 0.0;
    }
    if !md.given.wmax {
        md.wmax = 1.0;
    }
    if !md.given.dwc {
        md.dwc = md.wint;
    }
    if !md.given.dlc {
        md.dlc = md.lint;
    }
    if !md.given.xl {
        md.xl = 0.0;
    }
    if !md.given.xw {
        md.xw = 0.0;
    }
    if !md.given.dlcig {
        md.dlcig = md.lint;
    }
    if !md.given.dlcigd {
        if md.given.dlcig {
            md.dlcigd = md.dlcig;
        } else {
            md.dlcigd = md.lint;
        }
    }
    if !md.given.dwj {
        md.dwj = md.dwc;
    }
    if !md.given.xpart {
        md.xpart = 0.0;
    }
    if !md.given.sheet_resistance {
        md.sheet_resistance = 0.0;
    }
    if !md.given.sunit_area_jct_cap {
        md.sunit_area_jct_cap = 5.0E-4;
    }
    if !md.given.dunit_area_jct_cap {
        md.dunit_area_jct_cap = md.sunit_area_jct_cap;
    }
    if !md.given.sunit_length_sidewall_jct_cap {
        md.sunit_length_sidewall_jct_cap = 5.0E-10;
    }
    if !md.given.dunit_length_sidewall_jct_cap {
        md.dunit_length_sidewall_jct_cap = md.sunit_length_sidewall_jct_cap;
    }
    if !md.given.sunit_length_gate_sidewall_jct_cap {
        md.sunit_length_gate_sidewall_jct_cap = md.sunit_length_sidewall_jct_cap;
    }
    if !md.given.dunit_length_gate_sidewall_jct_cap {
        md.dunit_length_gate_sidewall_jct_cap = md.sunit_length_gate_sidewall_jct_cap;
    }
    if !md.given.sjct_sat_cur_density {
        md.sjct_sat_cur_density = 1.0E-4;
    }
    if !md.given.djct_sat_cur_density {
        md.djct_sat_cur_density = md.sjct_sat_cur_density;
    }
    if !md.given.sjct_sidewall_sat_cur_density {
        md.sjct_sidewall_sat_cur_density = 0.0;
    }
    if !md.given.djct_sidewall_sat_cur_density {
        md.djct_sidewall_sat_cur_density = md.sjct_sidewall_sat_cur_density;
    }
    if !md.given.sjct_gate_sidewall_sat_cur_density {
        md.sjct_gate_sidewall_sat_cur_density = 0.0;
    }
    if !md.given.djct_gate_sidewall_sat_cur_density {
        md.djct_gate_sidewall_sat_cur_density = md.sjct_gate_sidewall_sat_cur_density;
    }
    if !md.given.sbulk_jct_potential {
        md.sbulk_jct_potential = 1.0;
    }
    if !md.given.dbulk_jct_potential {
        md.dbulk_jct_potential = md.sbulk_jct_potential;
    }
    if !md.given.ssidewall_jct_potential {
        md.ssidewall_jct_potential = 1.0;
    }
    if !md.given.dsidewall_jct_potential {
        md.dsidewall_jct_potential = md.ssidewall_jct_potential;
    }
    if !md.given.sgatesidewall_jct_potential {
        md.sgatesidewall_jct_potential = md.ssidewall_jct_potential;
    }
    if !md.given.dgatesidewall_jct_potential {
        md.dgatesidewall_jct_potential = md.sgatesidewall_jct_potential;
    }
    if !md.given.sbulk_jct_bot_grading_coeff {
        md.sbulk_jct_bot_grading_coeff = 0.5;
    }
    if !md.given.dbulk_jct_bot_grading_coeff {
        md.dbulk_jct_bot_grading_coeff = md.sbulk_jct_bot_grading_coeff;
    }
    if !md.given.sbulk_jct_side_grading_coeff {
        md.sbulk_jct_side_grading_coeff = 0.33;
    }
    if !md.given.dbulk_jct_side_grading_coeff {
        md.dbulk_jct_side_grading_coeff = md.sbulk_jct_side_grading_coeff;
    }
    if !md.given.sbulk_jct_gate_side_grading_coeff {
        md.sbulk_jct_gate_side_grading_coeff = md.sbulk_jct_side_grading_coeff;
    }
    if !md.given.dbulk_jct_gate_side_grading_coeff {
        md.dbulk_jct_gate_side_grading_coeff = md.sbulk_jct_gate_side_grading_coeff;
    }
    if !md.given.sjct_emission_coeff {
        md.sjct_emission_coeff = 1.0;
    }
    if !md.given.djct_emission_coeff {
        md.djct_emission_coeff = md.sjct_emission_coeff;
    }
    if !md.given.sjct_temp_exponent {
        md.sjct_temp_exponent = 3.0;
    }
    if !md.given.djct_temp_exponent {
        md.djct_temp_exponent = md.sjct_temp_exponent;
    }
    if !md.given.jtss {
        md.jtss = 0.0;
    }
    if !md.given.jtsd {
        md.jtsd = md.jtss;
    }
    if !md.given.jtssws {
        md.jtssws = 0.0;
    }
    if !md.given.jtsswd {
        md.jtsswd = md.jtssws;
    }
    if !md.given.jtsswgs {
        md.jtsswgs = 0.0;
    }
    if !md.given.jtsswgd {
        md.jtsswgd = md.jtsswgs;
    }
    if !md.given.jtweff {
        md.jtweff = 0.0;
    }
    if !md.given.njts {
        md.njts = 20.0;
    }
    if !md.given.njtssw {
        md.njtssw = 20.0;
    }
    if !md.given.njtsswg {
        md.njtsswg = 20.0;
    }
    if !md.given.njtsd {
        if md.given.njts {
            md.njtsd = md.njts;
        } else {
            md.njtsd = 20.0;
        }
    }
    if !md.given.njtsswd {
        if md.given.njtssw {
            md.njtsswd = md.njtssw;
        } else {
            md.njtsswd = 20.0;
        }
    }
    if !md.given.njtsswgd {
        if md.given.njtsswg {
            md.njtsswgd = md.njtsswg;
        } else {
            md.njtsswgd = 20.0;
        }
    }
    if !md.given.xtss {
        md.xtss = 0.02;
    }
    if !md.given.xtsd {
        md.xtsd = md.xtss;
    }
    if !md.given.xtssws {
        md.xtssws = 0.02;
    }
    if !md.given.xtsswd {
        md.xtsswd = md.xtssws;
    }
    if !md.given.xtsswgs {
        md.xtsswgs = 0.02;
    }
    if !md.given.xtsswgd {
        md.xtsswgd = md.xtsswgs;
    }
    if !md.given.tnjts {
        md.tnjts = 0.0;
    }
    if !md.given.tnjtssw {
        md.tnjtssw = 0.0;
    }
    if !md.given.tnjtsswg {
        md.tnjtsswg = 0.0;
    }
    if !md.given.tnjtsd {
        if md.given.tnjts {
            md.tnjtsd = md.tnjts;
        } else {
            md.tnjtsd = 0.0;
        }
    }
    if !md.given.tnjtsswd {
        if md.given.tnjtssw {
            md.tnjtsswd = md.tnjtssw;
        } else {
            md.tnjtsswd = 0.0;
        }
    }
    if !md.given.tnjtsswgd {
        if md.given.tnjtsswg {
            md.tnjtsswgd = md.tnjtsswg;
        } else {
            md.tnjtsswgd = 0.0;
        }
    }
    if !md.given.vtss {
        md.vtss = 10.0;
    }
    if !md.given.vtsd {
        md.vtsd = md.vtss;
    }
    if !md.given.vtssws {
        md.vtssws = 10.0;
    }
    if !md.given.vtsswd {
        md.vtsswd = md.vtssws;
    }
    if !md.given.vtsswgs {
        md.vtsswgs = 10.0;
    }
    if !md.given.vtsswgd {
        md.vtsswgd = md.vtsswgs;
    }
    if !md.given.oxide_trap_density_a {
        if md.type_ == NMOS {
            md.oxide_trap_density_a = 6.25e41;
        } else {
            md.oxide_trap_density_a = 6.188e40;
        }
    }
    if !md.given.oxide_trap_density_b {
        if md.type_ == NMOS {
            md.oxide_trap_density_b = 3.125e26;
        } else {
            md.oxide_trap_density_b = 1.5e25;
        }
    }
    if !md.given.oxide_trap_density_c {
        md.oxide_trap_density_c = 8.75e9;
    }
    if !md.given.em {
        md.em = 4.1e7;
    }
    if !md.given.ef {
        md.ef = 1.0;
    }
    if !md.given.af {
        md.af = 1.0;
    }
    if !md.given.kf {
        md.kf = 0.0;
    }
    if !md.given.vgs_max {
        md.vgs_max = 1e99;
    }
    if !md.given.vgd_max {
        md.vgd_max = 1e99;
    }
    if !md.given.vgb_max {
        md.vgb_max = 1e99;
    }
    if !md.given.vds_max {
        md.vds_max = 1e99;
    }
    if !md.given.vbs_max {
        md.vbs_max = 1e99;
    }
    if !md.given.vbd_max {
        md.vbd_max = 1e99;
    }
    if !md.given.vgsr_max {
        md.vgsr_max = 1e99;
    }
    if !md.given.vgdr_max {
        md.vgdr_max = 1e99;
    }
    if !md.given.vgbr_max {
        md.vgbr_max = 1e99;
    }
    if !md.given.vbsr_max {
        md.vbsr_max = 1e99;
    }
    if !md.given.vbdr_max {
        md.vbdr_max = 1e99;
    }
    if !md.given.saref {
        md.saref = 1e-6;
    }
    if !md.given.sbref {
        md.sbref = 1e-6;
    }
    if !md.given.wlod {
        md.wlod = 0.0;
    }
    if !md.given.ku0 {
        md.ku0 = 0.0;
    }
    if !md.given.kvsat {
        md.kvsat = 0.0;
    }
    if !md.given.kvth0 {
        md.kvth0 = 0.0;
    }
    if !md.given.tku0 {
        md.tku0 = 0.0;
    }
    if !md.given.llodku0 {
        md.llodku0 = 0.0;
    }
    if !md.given.wlodku0 {
        md.wlodku0 = 0.0;
    }
    if !md.given.llodvth {
        md.llodvth = 0.0;
    }
    if !md.given.wlodvth {
        md.wlodvth = 0.0;
    }
    if !md.given.lku0 {
        md.lku0 = 0.0;
    }
    if !md.given.wku0 {
        md.wku0 = 0.0;
    }
    if !md.given.pku0 {
        md.pku0 = 0.0;
    }
    if !md.given.lkvth0 {
        md.lkvth0 = 0.0;
    }
    if !md.given.wkvth0 {
        md.wkvth0 = 0.0;
    }
    if !md.given.pkvth0 {
        md.pkvth0 = 0.0;
    }
    if !md.given.stk2 {
        md.stk2 = 0.0;
    }
    if !md.given.lodk2 {
        md.lodk2 = 1.0;
    }
    if !md.given.steta0 {
        md.steta0 = 0.0;
    }
    if !md.given.lodeta0 {
        md.lodeta0 = 1.0;
    }
    if !md.given.web {
        md.web = 0.0;
    }
    if !md.given.wec {
        md.wec = 0.0;
    }
    if !md.given.kvth0we {
        md.kvth0we = 0.0;
    }
    if !md.given.k2we {
        md.k2we = 0.0;
    }
    if !md.given.ku0we {
        md.ku0we = 0.0;
    }
    if !md.given.scref {
        md.scref = 1.0E-6;
    }
    if !md.given.wpemod {
        md.wpemod = 0.0;
    } else if md.wpemod != 0.0 && md.wpemod != 1.0 {
        md.wpemod = 0.0;
        notes.push("Warning: wpemod has been set to its default value: 0.".to_string());
    }
    if !md.given.lkvth0we {
        md.lkvth0we = 0.0;
    }
    if !md.given.lk2we {
        md.lk2we = 0.0;
    }
    if !md.given.lku0we {
        md.lku0we = 0.0;
    }
    if !md.given.wkvth0we {
        md.wkvth0we = 0.0;
    }
    if !md.given.wk2we {
        md.wk2we = 0.0;
    }
    if !md.given.wku0we {
        md.wku0we = 0.0;
    }
    if !md.given.pkvth0we {
        md.pkvth0we = 0.0;
    }
    if !md.given.pk2we {
        md.pk2we = 0.0;
    }
    if !md.given.pku0we {
        md.pku0we = 0.0;
    }
    _dmcgeff = md.dmcg - md.dmcgt;
    _dmcieff = md.dmci;
    _dmdgeff = md.dmdg - md.dmcgt;
}

/// The instance-parameter defaulting of `BSIM4setup` (`b4set.c`).
#[allow(unused_assignments, clippy::too_many_lines)]
pub(super) fn setup_instance(md: &Bsim4Model, h: &mut Bsim4Here, notes: &mut Vec<String>) {
    if !h.given.l {
        h.l = 5.0e-6;
    }
    if !h.given.w {
        h.w = 5.0e-6;
    }
    if !h.given.m {
        h.m = 1.0;
    }
    if !h.given.mult_i {
        h.mult_i = 1.0;
    }
    if !h.given.mult_q {
        h.mult_q = 1.0;
    }
    if !h.given.mult_fn {
        h.mult_fn = h.mult_i;
    }
    if !h.given.nf {
        h.nf = 1.0;
    }
    if !h.given.min {
        h.min = 0;
    }
    if !h.given.ic_vds {
        h.ic_vds = 0.0;
    }
    if !h.given.ic_vgs {
        h.ic_vgs = 0.0;
    }
    if !h.given.ic_vbs {
        h.ic_vbs = 0.0;
    }
    if !h.given.drain_area {
        h.drain_area = 0.0;
    }
    if !h.given.drain_perimeter {
        h.drain_perimeter = 0.0;
    }
    if !h.given.drain_squares {
        h.drain_squares = 1.0;
    }
    if !h.given.source_area {
        h.source_area = 0.0;
    }
    if !h.given.source_perimeter {
        h.source_perimeter = 0.0;
    }
    if !h.given.source_squares {
        h.source_squares = 1.0;
    }
    if !h.given.rbdb {
        h.rbdb = md.rbdb;
    }
    if !h.given.rbsb {
        h.rbsb = md.rbsb;
    }
    if !h.given.rbpb {
        h.rbpb = md.rbpb;
    }
    if !h.given.rbps {
        h.rbps = md.rbps;
    }
    if !h.given.rbpd {
        h.rbpd = md.rbpd;
    }
    if !h.given.delvto {
        h.delvto = 0.0;
    }
    if !h.given.mulu0 {
        h.mulu0 = 1.0;
    }
    if !h.given.xgw {
        h.xgw = md.xgw;
    }
    if !h.given.ngcon {
        h.ngcon = md.ngcon;
    }
    if !h.given.dtemp {
        h.dtemp = 0.0;
    }
    if !h.given.rbody_mod {
        h.rbody_mod = md.rbody_mod;
    } else if h.rbody_mod != 0 && h.rbody_mod != 1 && h.rbody_mod != 2 {
        h.rbody_mod = md.rbody_mod;
        notes.push(format!(
            "Warning: rbodyMod has been set to its global value {}.",
            md.rbody_mod
        ));
    }
    if !h.given.rgate_mod {
        h.rgate_mod = md.rgate_mod;
    } else if h.rgate_mod != 0 && h.rgate_mod != 1 && h.rgate_mod != 2 && h.rgate_mod != 3 {
        h.rgate_mod = md.rgate_mod;
        notes.push(format!(
            "Warning: rgateMod has been set to its global value {}.",
            md.rgate_mod
        ));
    }
    if !h.given.geo_mod {
        h.geo_mod = md.geo_mod;
    }
    if !h.given.rgeo_mod {
        h.rgeo_mod = 0;
    }
    if !h.given.trnqs_mod {
        h.trnqs_mod = md.trnqs_mod;
    } else if h.trnqs_mod != 0 && h.trnqs_mod != 1 {
        h.trnqs_mod = md.trnqs_mod;
        notes.push(format!(
            "Warning: trnqsMod has been set to its global value {}.",
            md.trnqs_mod
        ));
    }
    if !h.given.acnqs_mod {
        h.acnqs_mod = md.acnqs_mod;
    } else if h.acnqs_mod != 0 && h.acnqs_mod != 1 {
        h.acnqs_mod = md.acnqs_mod;
        notes.push(format!(
            "Warning: acnqsMod has been set to its global value {}.",
            md.acnqs_mod
        ));
    }
    if !h.given.sa {
        h.sa = 0.0;
    }
    if !h.given.sb {
        h.sb = 0.0;
    }
    if !h.given.sd {
        h.sd = 2.0 * md.dmcg;
    }
    if !h.given.sca {
        h.sca = 0.0;
    }
    if !h.given.scb {
        h.scb = 0.0;
    }
    if !h.given.scc {
        h.scc = 0.0;
    }
    if !h.given.sc {
        h.sc = 0.0;
    }
}

/// A fresh `here` with the instance parameters as given on the card
/// (`BSIM4param`, `b4par.c`) followed by the `BSIM4setup` defaulting
/// against the set-up model `md`. An unknown keyword is an error.
pub(crate) fn new_here(
    md: &Bsim4Model,
    params: &[(String, f64)],
    off: bool,
    notes: &mut Vec<String>,
) -> Result<Bsim4Here, String> {
    let mut h = Bsim4Here::default();
    for (k, v) in params {
        if !h.set_param(k, *v) {
            return Err(format!("unknown BSIM4 instance parameter `{k}`"));
        }
    }
    if off {
        h.off = 1;
    }
    setup_instance(md, &mut h, notes);
    Ok(h)
}

/// The internal nodes `BSIM4setup` creates for one instance, in its
/// `CKTmkVolt` order and with its names: `drain`, `source`, `gate`,
/// `midgate`, `dbody`, `body`, `sbody`, `charge`. `noise_job` is the
/// C's `noiseAnalGiven` (a `.noise` analysis in the task).
pub(crate) fn internal_nodes(md: &Bsim4Model, h: &Bsim4Here, noise_job: bool) -> Vec<&'static str> {
    let mut v = Vec::new();
    let dmcg_eff = md.dmcg - md.dmcgt;
    let dmci_eff = md.dmci;
    let dmdg_eff = md.dmdg - md.dmcgt;
    let series = |given: bool, squares: f64, type_: i32| -> bool {
        if md.rds_mod != 0 || (md.tnoi_mod == 1 && noise_job) {
            true
        } else if md.sheet_resistance > 0.0 {
            if given && squares > 0.0 {
                true
            } else if !given && h.rgeo_mod != 0 {
                let mut rtot = 0.0;
                rdseff_geo(
                    h.nf,
                    h.geo_mod,
                    h.rgeo_mod,
                    h.min,
                    h.w,
                    md.sheet_resistance,
                    dmcg_eff,
                    dmci_eff,
                    dmdg_eff,
                    type_,
                    &mut rtot,
                );
                rtot > 0.0
            } else {
                false
            }
        } else {
            false
        }
    };
    if series(h.given.drain_squares, h.drain_squares, 0) {
        v.push("drain");
    }
    if series(h.given.source_squares, h.source_squares, 1) {
        v.push("source");
    }
    if h.rgate_mod > 0 {
        v.push("gate");
    }
    if h.rgate_mod == 3 {
        v.push("midgate");
    }
    if h.rbody_mod == 1 || h.rbody_mod == 2 {
        v.push("dbody");
        v.push("body");
        v.push("sbody");
    }
    if h.trnqs_mod != 0 {
        v.push("charge");
    }
    v
}

/// Fill the node fields of `h` from the external nodes (drain, gate,
/// source, bulk) and the internal ones `internal_nodes` named, looked
/// up by suffix (the C's `here->BSIM4…Node` assignments).
pub(crate) fn assign_nodes(
    h: &mut Bsim4Here,
    ext: [Var; 4],
    internal: impl Fn(&str) -> Option<Var>,
) {
    h.d_node = ext[0];
    h.g_node_ext = ext[1];
    h.s_node = ext[2];
    h.b_node = ext[3];
    h.d_node_prime = internal("drain").unwrap_or(h.d_node);
    h.s_node_prime = internal("source").unwrap_or(h.s_node);
    h.g_node_prime = internal("gate").unwrap_or(h.g_node_ext);
    h.g_node_mid = internal("midgate").unwrap_or(h.g_node_ext);
    match (internal("dbody"), internal("body"), internal("sbody")) {
        (Some(db), Some(b), Some(sb)) => {
            h.db_node = db;
            h.b_node_prime = b;
            h.sb_node = sb;
        }
        _ => {
            h.db_node = h.b_node;
            h.b_node_prime = h.b_node;
            h.sb_node = h.b_node;
        }
    }
    h.q_node = internal("charge").unwrap_or(0);
}
