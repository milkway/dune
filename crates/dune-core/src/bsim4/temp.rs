// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4temp.c as distributed with ngspice-47
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

//! `BSIM4temp` (`b4temp.c`) in its three parts: the model part (run once
//! per card and temperature), the size-dependent parameter set built
//! once per drawn (L, W, NF) and the per-instance part. The C keeps all
//! three in one function whose locals persist across the instance loop
//! of a model; [`TempVars`] carries them the same way (one per model
//! entry of the knot list), so a value one part leaves behind is what
//! the next part reads, as in the C.

use super::*;

/// The locals of `BSIM4temp`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct TempVars {
    pub bodymode: f64,
    pub chktol: f64,
    pub delt_vth: f64,
    pub deltemp: f64,
    pub deta0_lod: f64,
    pub dk2_lod: f64,
    pub dmcgeff: f64,
    pub dmcieff: f64,
    pub dmdgeff: f64,
    pub drainsatcurrent: f64,
    pub dumad: f64,
    pub dumas: f64,
    pub dumpd: f64,
    pub dumps: f64,
    pub dvth0_lod: f64,
    pub eg: f64,
    pub eg0: f64,
    pub epsrox: f64,
    pub epssub: f64,
    pub fatal_flag: i32,
    pub i: i32,
    pub inv_l: f64,
    pub inv_lw: f64,
    pub inv_odeff: f64,
    pub inv_sa: f64,
    pub inv_saref: f64,
    pub inv_sb: f64,
    pub inv_sbref: f64,
    pub inv_w: f64,
    pub kvsat: f64,
    pub ldrn: f64,
    pub lnew: f64,
    pub lnl: f64,
    pub lnnf: f64,
    pub lnw: f64,
    pub lpe_vb: f64,
    pub lt1: f64,
    pub ltw: f64,
    pub n: f64,
    pub n0: f64,
    pub ni: f64,
    pub niter: f64,
    pub nvtmd: f64,
    pub nvtms: f64,
    pub od_offset: f64,
    pub phieot: f64,
    pub powweffwr: f64,
    pub rbdbx: f64,
    pub rbdby: f64,
    pub rbpbx: f64,
    pub rbpby: f64,
    pub rbsbx: f64,
    pub rbsby: f64,
    pub rho: f64,
    pub sceff: f64,
    pub size_not_found: i32,
    pub sourcesatcurrent: f64,
    pub t0: f64,
    pub t1: f64,
    pub t10: f64,
    pub t11: f64,
    pub t2: f64,
    pub t3: f64,
    pub t4: f64,
    pub t5: f64,
    pub t6: f64,
    pub t7: f64,
    pub t8: f64,
    pub t9: f64,
    pub tcen: f64,
    pub temp: f64,
    pub tempratioeot: f64,
    pub theta0: f64,
    pub tmp: f64,
    pub tmp1: f64,
    pub tmp2: f64,
    pub tmp3: f64,
    pub tnom: f64,
    pub toxe: f64,
    pub toxpf: f64,
    pub toxpi: f64,
    pub tratio: f64,
    pub v0: f64,
    pub vbieot: f64,
    pub vddeot: f64,
    pub vgs_eff: f64,
    pub vgsteff: f64,
    pub vtfbphi2eot: f64,
    pub vth: f64,
    pub vth_narroww: f64,
    pub vtm0: f64,
    pub vtm0eot: f64,
    pub vtmeot: f64,
    pub w_tmp: f64,
    pub wdrn: f64,
    pub wlod: f64,
    pub wnew: f64,
}

/// The model part of `BSIM4temp`.
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn temp_model(
    md: &mut Bsim4Model,
    tv: &mut TempVars,
    ck_temp: f64,
    notes: &mut Vec<String>,
) {
    tv.lnew = 0.0;
    tv.fatal_flag = 0;
    tv.temp = ck_temp;
    if md.sbulk_jct_potential < 0.1 {
        md.sbulk_jct_potential = 0.1;
        notes.push("Given pbs is less than 0.1. Pbs is set to 0.1.".to_string());
    }
    if md.ssidewall_jct_potential < 0.1 {
        md.ssidewall_jct_potential = 0.1;
        notes.push("Given pbsws is less than 0.1. Pbsws is set to 0.1.".to_string());
    }
    if md.sgatesidewall_jct_potential < 0.1 {
        md.sgatesidewall_jct_potential = 0.1;
        notes.push("Given pbswgs is less than 0.1. Pbswgs is set to 0.1.".to_string());
    }
    if md.dbulk_jct_potential < 0.1 {
        md.dbulk_jct_potential = 0.1;
        notes.push("Given pbd is less than 0.1. Pbd is set to 0.1.".to_string());
    }
    if md.dsidewall_jct_potential < 0.1 {
        md.dsidewall_jct_potential = 0.1;
        notes.push("Given pbswd is less than 0.1. Pbswd is set to 0.1.".to_string());
    }
    if md.dgatesidewall_jct_potential < 0.1 {
        md.dgatesidewall_jct_potential = 0.1;
        notes.push("Given pbswgd is less than 0.1. Pbswgd is set to 0.1.".to_string());
    }
    if md.mtrl_mod == 0 {
        tv.chktol = fabs(md.toxe);
        if fabs(md.toxp) > tv.chktol {
            tv.chktol = fabs(md.toxp);
        }
        if fabs(md.dtox) > tv.chktol {
            tv.chktol = fabs(md.dtox);
        }
        tv.chktol *= 1e-14;
        if md.given.toxe
            && md.given.toxp
            && md.given.dtox
            && fabs(md.toxe - (md.toxp + md.dtox)) > tv.chktol
        {
            notes.push(
                "Warning: toxe, toxp and dtox all given and toxe != toxp + dtox; dtox ignored."
                    .to_string(),
            );
        } else if md.given.toxe && !md.given.toxp {
            md.toxp = md.toxe - md.dtox;
        } else if !md.given.toxe && md.given.toxp {
            md.toxe = md.toxp + md.dtox;
            if !md.given.toxm {
                md.toxm = md.toxe;
            }
        }
    } else if md.mtrl_compat_mod != 0 {
        tv.t0 = md.epsrox / 3.9;
        if md.given.eot
            && md.given.toxp
            && md.given.dtox
            && fabs(md.eot * tv.t0 - (md.toxp + md.dtox)) > 1.0e-20
        {
            notes.push("Warning: eot, toxp and dtox all given and eot * EPSROX / 3.9 != toxp + dtox; dtox ignored.".to_string());
        } else if md.given.eot && !md.given.toxp {
            md.toxp = tv.t0 * md.eot - md.dtox;
        } else if !md.given.eot && md.given.toxp {
            md.eot = (md.toxp + md.dtox) / tv.t0;
            if !md.given.toxm {
                md.toxm = md.eot;
            }
        }
    }
    if md.mtrl_mod != 0 {
        tv.epsrox = 3.9;
        tv.toxe = md.eot;
        tv.epssub = EPS0 * md.epsrsub;
    } else {
        tv.epsrox = md.epsrox;
        tv.toxe = md.toxe;
        tv.epssub = EPSSI;
    }
    if !md.given.cf {
        md.cf = 2.0 * tv.epsrox * EPS0 / PI * log(1.0 + 0.4e-6 / tv.toxe);
    }
    md.coxe = tv.epsrox * EPS0 / tv.toxe;
    if md.mtrl_mod == 0 || md.mtrl_compat_mod != 0 {
        md.coxp = md.epsrox * EPS0 / md.toxp;
    }
    if !md.given.cgdo {
        if md.given.dlc && md.dlc > 0.0 {
            md.cgdo = md.dlc * md.coxe - md.cgdl;
        } else {
            md.cgdo = 0.6 * md.xj * md.coxe;
        }
    }
    if !md.given.cgso {
        if md.given.dlc && md.dlc > 0.0 {
            md.cgso = md.dlc * md.coxe - md.cgsl;
        } else {
            md.cgso = 0.6 * md.xj * md.coxe;
        }
    }
    if !md.given.cgbo {
        md.cgbo = 2.0 * md.dwc * md.coxe;
    }
    tv.tnom = md.tnom;
    tv.tratio = tv.temp / tv.tnom;
    md.vcrit = VT0 * log(VT0 / (ROOT2 * 1.0e-14));
    md.factor1 = sqrt(tv.epssub / (tv.epsrox * EPS0) * tv.toxe);
    md.vtm0 = KBOQ * tv.tnom;
    tv.vtm0 = md.vtm0;
    if md.mtrl_mod == 0 {
        tv.eg0 = 1.16 - 7.02e-4 * tv.tnom * tv.tnom / (tv.tnom + 1108.0);
        tv.ni = 1.45e10
            * (tv.tnom / 300.15)
            * sqrt(tv.tnom / 300.15)
            * exp(21.5565981 - tv.eg0 / (2.0 * tv.vtm0));
    } else {
        tv.eg0 = md.bg0sub - md.tbgasub * tv.tnom * tv.tnom / (tv.tnom + md.tbgbsub);
        tv.t0 = md.bg0sub - md.tbgasub * 90090.0225 / (300.15 + md.tbgbsub);
        tv.ni = md.ni0sub
            * (tv.tnom / 300.15)
            * sqrt(tv.tnom / 300.15)
            * exp((tv.t0 - tv.eg0) / (2.0 * tv.vtm0));
    }
    md.eg0 = tv.eg0;
    md.vtm = KBOQ * tv.temp;
    if md.mtrl_mod == 0 {
        tv.eg = 1.16 - 7.02e-4 * tv.temp * tv.temp / (tv.temp + 1108.0);
    } else {
        tv.eg = md.bg0sub - md.tbgasub * tv.temp * tv.temp / (tv.temp + md.tbgbsub);
    }
    if tv.temp != tv.tnom {
        tv.t0 = tv.eg0 / tv.vtm0 - tv.eg / md.vtm;
        tv.t1 = log(tv.temp / tv.tnom);
        tv.t2 = tv.t0 + md.sjct_temp_exponent * tv.t1;
        tv.t3 = exp(tv.t2 / md.sjct_emission_coeff);
        md.sjct_temp_sat_cur_density = md.sjct_sat_cur_density * tv.t3;
        md.sjct_sidewall_temp_sat_cur_density = md.sjct_sidewall_sat_cur_density * tv.t3;
        md.sjct_gate_sidewall_temp_sat_cur_density = md.sjct_gate_sidewall_sat_cur_density * tv.t3;
        tv.t2 = tv.t0 + md.djct_temp_exponent * tv.t1;
        tv.t3 = exp(tv.t2 / md.djct_emission_coeff);
        md.djct_temp_sat_cur_density = md.djct_sat_cur_density * tv.t3;
        md.djct_sidewall_temp_sat_cur_density = md.djct_sidewall_sat_cur_density * tv.t3;
        md.djct_gate_sidewall_temp_sat_cur_density = md.djct_gate_sidewall_sat_cur_density * tv.t3;
    } else {
        md.sjct_temp_sat_cur_density = md.sjct_sat_cur_density;
        md.sjct_sidewall_temp_sat_cur_density = md.sjct_sidewall_sat_cur_density;
        md.sjct_gate_sidewall_temp_sat_cur_density = md.sjct_gate_sidewall_sat_cur_density;
        md.djct_temp_sat_cur_density = md.djct_sat_cur_density;
        md.djct_sidewall_temp_sat_cur_density = md.djct_sidewall_sat_cur_density;
        md.djct_gate_sidewall_temp_sat_cur_density = md.djct_gate_sidewall_sat_cur_density;
    }
    if md.sjct_temp_sat_cur_density < 0.0 {
        md.sjct_temp_sat_cur_density = 0.0;
    }
    if md.sjct_sidewall_temp_sat_cur_density < 0.0 {
        md.sjct_sidewall_temp_sat_cur_density = 0.0;
    }
    if md.sjct_gate_sidewall_temp_sat_cur_density < 0.0 {
        md.sjct_gate_sidewall_temp_sat_cur_density = 0.0;
    }
    if md.djct_temp_sat_cur_density < 0.0 {
        md.djct_temp_sat_cur_density = 0.0;
    }
    if md.djct_sidewall_temp_sat_cur_density < 0.0 {
        md.djct_sidewall_temp_sat_cur_density = 0.0;
    }
    if md.djct_gate_sidewall_temp_sat_cur_density < 0.0 {
        md.djct_gate_sidewall_temp_sat_cur_density = 0.0;
    }
    tv.deltemp = ck_temp - md.tnom;
    tv.t0 = md.tcj * tv.deltemp;
    if tv.t0 >= -1.0 {
        md.sunit_area_temp_jct_cap = md.sunit_area_jct_cap * (1.0 + tv.t0);
        md.dunit_area_temp_jct_cap = md.dunit_area_jct_cap * (1.0 + tv.t0);
    } else {
        if md.sunit_area_jct_cap > 0.0 {
            md.sunit_area_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjs to be negative. Cjs is clamped to zero."
                    .to_string(),
            );
        }
        if md.dunit_area_jct_cap > 0.0 {
            md.dunit_area_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjd to be negative. Cjd is clamped to zero."
                    .to_string(),
            );
        }
    }
    tv.t0 = md.tcjsw * tv.deltemp;
    if md.sunit_length_sidewall_jct_cap < 0.0 {
        md.sunit_length_sidewall_jct_cap = 0.0;
        notes.push("CJSWS is negative. Cjsws is clamped to zero.".to_string());
    }
    if md.dunit_length_sidewall_jct_cap < 0.0 {
        md.dunit_length_sidewall_jct_cap = 0.0;
        notes.push("CJSWD is negative. Cjswd is clamped to zero.".to_string());
    }
    if tv.t0 >= -1.0 {
        md.sunit_length_sidewall_temp_jct_cap = md.sunit_length_sidewall_jct_cap * (1.0 + tv.t0);
        md.dunit_length_sidewall_temp_jct_cap = md.dunit_length_sidewall_jct_cap * (1.0 + tv.t0);
    } else {
        if md.sunit_length_sidewall_jct_cap > 0.0 {
            md.sunit_length_sidewall_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjsws to be negative. Cjsws is clamped to zero."
                    .to_string(),
            );
        }
        if md.dunit_length_sidewall_jct_cap > 0.0 {
            md.dunit_length_sidewall_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjswd to be negative. Cjswd is clamped to zero."
                    .to_string(),
            );
        }
    }
    tv.t0 = md.tcjswg * tv.deltemp;
    if tv.t0 >= -1.0 {
        md.sunit_length_gate_sidewall_temp_jct_cap =
            md.sunit_length_gate_sidewall_jct_cap * (1.0 + tv.t0);
        md.dunit_length_gate_sidewall_temp_jct_cap =
            md.dunit_length_gate_sidewall_jct_cap * (1.0 + tv.t0);
    } else {
        if md.sunit_length_gate_sidewall_jct_cap > 0.0 {
            md.sunit_length_gate_sidewall_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjswgs to be negative. Cjswgs is clamped to zero."
                    .to_string(),
            );
        }
        if md.dunit_length_gate_sidewall_jct_cap > 0.0 {
            md.dunit_length_gate_sidewall_temp_jct_cap = 0.0;
            notes.push(
                "Temperature effect has caused cjswgd to be negative. Cjswgd is clamped to zero."
                    .to_string(),
            );
        }
    }
    md.phi_bs = md.sbulk_jct_potential - md.tpb * tv.deltemp;
    if md.phi_bs < 0.01 {
        md.phi_bs = 0.01;
        notes.push(
            "Temperature effect has caused pbs to be less than 0.01. Pbs is clamped to 0.01."
                .to_string(),
        );
    }
    md.phi_bd = md.dbulk_jct_potential - md.tpb * tv.deltemp;
    if md.phi_bd < 0.01 {
        md.phi_bd = 0.01;
        notes.push(
            "Temperature effect has caused pbd to be less than 0.01. Pbd is clamped to 0.01."
                .to_string(),
        );
    }
    md.phi_bsws = md.ssidewall_jct_potential - md.tpbsw * tv.deltemp;
    if md.phi_bsws < 0.01 {
        md.phi_bsws = 0.01;
        notes.push(
            "Temperature effect has caused pbsws to be less than 0.01. Pbsws is clamped to 0.01."
                .to_string(),
        );
    }
    md.phi_bswd = md.dsidewall_jct_potential - md.tpbsw * tv.deltemp;
    if md.phi_bswd < 0.01 {
        md.phi_bswd = 0.01;
        notes.push(
            "Temperature effect has caused pbswd to be less than 0.01. Pbswd is clamped to 0.01."
                .to_string(),
        );
    }
    md.phi_bswgs = md.sgatesidewall_jct_potential - md.tpbswg * tv.deltemp;
    if md.phi_bswgs < 0.01 {
        md.phi_bswgs = 0.01;
        notes.push(
            "Temperature effect has caused pbswgs to be less than 0.01. Pbswgs is clamped to 0.01."
                .to_string(),
        );
    }
    md.phi_bswgd = md.dgatesidewall_jct_potential - md.tpbswg * tv.deltemp;
    if md.phi_bswgd < 0.01 {
        md.phi_bswgd = 0.01;
        notes.push(
            "Temperature effect has caused pbswgd to be less than 0.01. Pbswgd is clamped to 0.01."
                .to_string(),
        );
    }
    if md.ijthdfwd <= 0.0 {
        md.ijthdfwd = 0.0;
        notes.push(format!("Ijthdfwd reset to {}.", md.ijthdfwd));
    }
    if md.ijthsfwd <= 0.0 {
        md.ijthsfwd = 0.0;
        notes.push(format!("Ijthsfwd reset to {}.", md.ijthsfwd));
    }
    if md.ijthdrev <= 0.0 {
        md.ijthdrev = 0.0;
        notes.push(format!("Ijthdrev reset to {}.", md.ijthdrev));
    }
    if md.ijthsrev <= 0.0 {
        md.ijthsrev = 0.0;
        notes.push(format!("Ijthsrev reset to {}.", md.ijthsrev));
    }
    if (md.xjbvd <= 0.0 && md.dio_mod == 2) || (md.xjbvd < 0.0 && md.dio_mod == 0) {
        md.xjbvd = 0.0;
        notes.push(format!("Xjbvd reset to {}.", md.xjbvd));
    }
    if md.bvd <= 0.0 {
        md.bvd = 0.0;
        notes.push(format!("BVD reset to {}.", md.bvd));
    }
    if (md.xjbvs <= 0.0 && md.dio_mod == 2) || (md.xjbvs < 0.0 && md.dio_mod == 0) {
        md.xjbvs = 0.0;
        notes.push(format!("Xjbvs reset to {}.", md.xjbvs));
    }
    if md.bvs <= 0.0 {
        md.bvs = 0.0;
        notes.push(format!("BVS reset to {}.", md.bvs));
    }
}

/// The size-dependent part of `BSIM4temp` (`if (Size_Not_Found)`):
/// fills the `bsim4SizeDependParam` knot `pp` for `h`'s L, W and NF.
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub(super) fn temp_size(
    md: &mut Bsim4Model,
    h: &Bsim4Here,
    pp: &mut Bsim4Size,
    tv: &mut TempVars,
    _ck_temp: f64,
    modname: &str,
    name: &str,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    tv.ldrn = h.l;
    tv.wdrn = h.w / h.nf;
    pp.length = h.l;
    pp.width = h.w;
    pp.nfinger = h.nf;
    tv.lnew = h.l + md.xl;
    tv.wnew = h.w / h.nf + md.xw;
    tv.t0 = pow(tv.lnew, md.lln);
    tv.t1 = pow(tv.wnew, md.lwn);
    tv.tmp1 = md.ll / tv.t0 + md.lw / tv.t1 + md.lwl / (tv.t0 * tv.t1);
    pp.dl = md.lint + tv.tmp1;
    tv.tmp2 = md.llc / tv.t0 + md.lwc / tv.t1 + md.lwlc / (tv.t0 * tv.t1);
    pp.dlc = md.dlc + tv.tmp2;
    tv.t2 = pow(tv.lnew, md.wln);
    tv.t3 = pow(tv.wnew, md.wwn);
    tv.tmp1 = md.wl / tv.t2 + md.ww / tv.t3 + md.wwl / (tv.t2 * tv.t3);
    pp.dw = md.wint + tv.tmp1;
    tv.tmp2 = md.wlc / tv.t2 + md.wwc / tv.t3 + md.wwlc / (tv.t2 * tv.t3);
    pp.dwc = md.dwc + tv.tmp2;
    pp.dwj = md.dwj + tv.tmp2;
    pp.leff = tv.lnew - 2.0 * pp.dl;
    if pp.leff <= 0.0 {
        return Err(format!(
            "BSIM4: mosfet {modname}, model {name}: Effective channel length <= 0"
        ));
    }
    pp.weff = tv.wnew - 2.0 * pp.dw;
    if pp.weff <= 0.0 {
        return Err(format!(
            "BSIM4: mosfet {modname}, model {name}: Effective channel width <= 0"
        ));
    }
    pp.leff_cv = tv.lnew - 2.0 * pp.dlc;
    if pp.leff_cv <= 0.0 {
        return Err(format!(
            "BSIM4: mosfet {modname}, model {name}: Effective channel length for C-V <= 0"
        ));
    }
    pp.weff_cv = tv.wnew - 2.0 * pp.dwc;
    if pp.weff_cv <= 0.0 {
        return Err(format!(
            "BSIM4: mosfet {modname}, model {name}: Effective channel width for C-V <= 0"
        ));
    }
    pp.weff_cj = tv.wnew - 2.0 * pp.dwj;
    if pp.weff_cj <= 0.0 {
        return Err(format!(
            "BSIM4: mosfet {modname}, model {name}: Effective channel width for S/D junctions <= 0"
        ));
    }
    if md.bin_unit == 1 {
        tv.inv_l = 1.0e-6 / pp.leff;
        tv.inv_w = 1.0e-6 / pp.weff;
        tv.inv_lw = 1.0e-12 / (pp.leff * pp.weff);
    } else {
        tv.inv_l = 1.0 / pp.leff;
        tv.inv_w = 1.0 / pp.weff;
        tv.inv_lw = 1.0 / (pp.leff * pp.weff);
    }
    pp.cdsc = md.cdsc + md.lcdsc * tv.inv_l + md.wcdsc * tv.inv_w + md.pcdsc * tv.inv_lw;
    pp.cdscb = md.cdscb + md.lcdscb * tv.inv_l + md.wcdscb * tv.inv_w + md.pcdscb * tv.inv_lw;
    pp.cdscd = md.cdscd + md.lcdscd * tv.inv_l + md.wcdscd * tv.inv_w + md.pcdscd * tv.inv_lw;
    pp.cit = md.cit + md.lcit * tv.inv_l + md.wcit * tv.inv_w + md.pcit * tv.inv_lw;
    pp.nfactor =
        md.nfactor + md.lnfactor * tv.inv_l + md.wnfactor * tv.inv_w + md.pnfactor * tv.inv_lw;
    pp.tnfactor =
        md.tnfactor + md.ltnfactor * tv.inv_l + md.wtnfactor * tv.inv_w + md.ptnfactor * tv.inv_lw;
    pp.xj = md.xj + md.lxj * tv.inv_l + md.wxj * tv.inv_w + md.pxj * tv.inv_lw;
    pp.vsat = md.vsat + md.lvsat * tv.inv_l + md.wvsat * tv.inv_w + md.pvsat * tv.inv_lw;
    pp.at = md.at + md.lat * tv.inv_l + md.wat * tv.inv_w + md.pat * tv.inv_lw;
    pp.a0 = md.a0 + md.la0 * tv.inv_l + md.wa0 * tv.inv_w + md.pa0 * tv.inv_lw;
    pp.ags = md.ags + md.lags * tv.inv_l + md.wags * tv.inv_w + md.pags * tv.inv_lw;
    pp.a1 = md.a1 + md.la1 * tv.inv_l + md.wa1 * tv.inv_w + md.pa1 * tv.inv_lw;
    pp.a2 = md.a2 + md.la2 * tv.inv_l + md.wa2 * tv.inv_w + md.pa2 * tv.inv_lw;
    pp.keta = md.keta + md.lketa * tv.inv_l + md.wketa * tv.inv_w + md.pketa * tv.inv_lw;
    pp.ketac = md.ketac + md.lketac * tv.inv_l + md.wketac * tv.inv_w + md.pketac * tv.inv_lw;
    pp.nsub = md.nsub + md.lnsub * tv.inv_l + md.wnsub * tv.inv_w + md.pnsub * tv.inv_lw;
    pp.ndep = md.ndep + md.lndep * tv.inv_l + md.wndep * tv.inv_w + md.pndep * tv.inv_lw;
    pp.nsd = md.nsd + md.lnsd * tv.inv_l + md.wnsd * tv.inv_w + md.pnsd * tv.inv_lw;
    pp.phin = md.phin + md.lphin * tv.inv_l + md.wphin * tv.inv_w + md.pphin * tv.inv_lw;
    pp.ngate = md.ngate + md.lngate * tv.inv_l + md.wngate * tv.inv_w + md.pngate * tv.inv_lw;
    pp.gamma1 = md.gamma1 + md.lgamma1 * tv.inv_l + md.wgamma1 * tv.inv_w + md.pgamma1 * tv.inv_lw;
    pp.gamma2 = md.gamma2 + md.lgamma2 * tv.inv_l + md.wgamma2 * tv.inv_w + md.pgamma2 * tv.inv_lw;
    pp.vbx = md.vbx + md.lvbx * tv.inv_l + md.wvbx * tv.inv_w + md.pvbx * tv.inv_lw;
    pp.vbm = md.vbm + md.lvbm * tv.inv_l + md.wvbm * tv.inv_w + md.pvbm * tv.inv_lw;
    pp.xt = md.xt + md.lxt * tv.inv_l + md.wxt * tv.inv_w + md.pxt * tv.inv_lw;
    pp.vfb = md.vfb + md.lvfb * tv.inv_l + md.wvfb * tv.inv_w + md.pvfb * tv.inv_lw;
    pp.k1 = md.k1 + md.lk1 * tv.inv_l + md.wk1 * tv.inv_w + md.pk1 * tv.inv_lw;
    pp.kt1 = md.kt1 + md.lkt1 * tv.inv_l + md.wkt1 * tv.inv_w + md.pkt1 * tv.inv_lw;
    pp.kt1l = md.kt1l + md.lkt1l * tv.inv_l + md.wkt1l * tv.inv_w + md.pkt1l * tv.inv_lw;
    pp.k2 = md.k2 + md.lk2 * tv.inv_l + md.wk2 * tv.inv_w + md.pk2 * tv.inv_lw;
    pp.kt2 = md.kt2 + md.lkt2 * tv.inv_l + md.wkt2 * tv.inv_w + md.pkt2 * tv.inv_lw;
    pp.k3 = md.k3 + md.lk3 * tv.inv_l + md.wk3 * tv.inv_w + md.pk3 * tv.inv_lw;
    pp.k3b = md.k3b + md.lk3b * tv.inv_l + md.wk3b * tv.inv_w + md.pk3b * tv.inv_lw;
    pp.w0 = md.w0 + md.lw0 * tv.inv_l + md.ww0 * tv.inv_w + md.pw0 * tv.inv_lw;
    pp.lpe0 = md.lpe0 + md.llpe0 * tv.inv_l + md.wlpe0 * tv.inv_w + md.plpe0 * tv.inv_lw;
    pp.lpeb = md.lpeb + md.llpeb * tv.inv_l + md.wlpeb * tv.inv_w + md.plpeb * tv.inv_lw;
    pp.dvtp0 = md.dvtp0 + md.ldvtp0 * tv.inv_l + md.wdvtp0 * tv.inv_w + md.pdvtp0 * tv.inv_lw;
    pp.dvtp1 = md.dvtp1 + md.ldvtp1 * tv.inv_l + md.wdvtp1 * tv.inv_w + md.pdvtp1 * tv.inv_lw;
    pp.dvtp2 = md.dvtp2 + md.ldvtp2 * tv.inv_l + md.wdvtp2 * tv.inv_w + md.pdvtp2 * tv.inv_lw;
    pp.dvtp3 = md.dvtp3 + md.ldvtp3 * tv.inv_l + md.wdvtp3 * tv.inv_w + md.pdvtp3 * tv.inv_lw;
    pp.dvtp4 = md.dvtp4 + md.ldvtp4 * tv.inv_l + md.wdvtp4 * tv.inv_w + md.pdvtp4 * tv.inv_lw;
    pp.dvtp5 = md.dvtp5 + md.ldvtp5 * tv.inv_l + md.wdvtp5 * tv.inv_w + md.pdvtp5 * tv.inv_lw;
    pp.dvt0 = md.dvt0 + md.ldvt0 * tv.inv_l + md.wdvt0 * tv.inv_w + md.pdvt0 * tv.inv_lw;
    pp.dvt1 = md.dvt1 + md.ldvt1 * tv.inv_l + md.wdvt1 * tv.inv_w + md.pdvt1 * tv.inv_lw;
    pp.dvt2 = md.dvt2 + md.ldvt2 * tv.inv_l + md.wdvt2 * tv.inv_w + md.pdvt2 * tv.inv_lw;
    pp.dvt0w = md.dvt0w + md.ldvt0w * tv.inv_l + md.wdvt0w * tv.inv_w + md.pdvt0w * tv.inv_lw;
    pp.dvt1w = md.dvt1w + md.ldvt1w * tv.inv_l + md.wdvt1w * tv.inv_w + md.pdvt1w * tv.inv_lw;
    pp.dvt2w = md.dvt2w + md.ldvt2w * tv.inv_l + md.wdvt2w * tv.inv_w + md.pdvt2w * tv.inv_lw;
    pp.drout = md.drout + md.ldrout * tv.inv_l + md.wdrout * tv.inv_w + md.pdrout * tv.inv_lw;
    pp.dsub = md.dsub + md.ldsub * tv.inv_l + md.wdsub * tv.inv_w + md.pdsub * tv.inv_lw;
    pp.vth0 = md.vth0 + md.lvth0 * tv.inv_l + md.wvth0 * tv.inv_w + md.pvth0 * tv.inv_lw;
    pp.ua = md.ua + md.lua * tv.inv_l + md.wua * tv.inv_w + md.pua * tv.inv_lw;
    pp.ua1 = md.ua1 + md.lua1 * tv.inv_l + md.wua1 * tv.inv_w + md.pua1 * tv.inv_lw;
    pp.ub = md.ub + md.lub * tv.inv_l + md.wub * tv.inv_w + md.pub_ * tv.inv_lw;
    pp.ub1 = md.ub1 + md.lub1 * tv.inv_l + md.wub1 * tv.inv_w + md.pub1 * tv.inv_lw;
    pp.uc = md.uc + md.luc * tv.inv_l + md.wuc * tv.inv_w + md.puc * tv.inv_lw;
    pp.uc1 = md.uc1 + md.luc1 * tv.inv_l + md.wuc1 * tv.inv_w + md.puc1 * tv.inv_lw;
    pp.ud = md.ud + md.lud * tv.inv_l + md.wud * tv.inv_w + md.pud * tv.inv_lw;
    pp.ud1 = md.ud1 + md.lud1 * tv.inv_l + md.wud1 * tv.inv_w + md.pud1 * tv.inv_lw;
    pp.up = md.up + md.lup * tv.inv_l + md.wup * tv.inv_w + md.pup * tv.inv_lw;
    pp.lp = md.lp + md.llp * tv.inv_l + md.wlp * tv.inv_w + md.plp * tv.inv_lw;
    pp.eu = md.eu + md.leu * tv.inv_l + md.weu * tv.inv_w + md.peu * tv.inv_lw;
    pp.u0 = md.u0 + md.lu0 * tv.inv_l + md.wu0 * tv.inv_w + md.pu0 * tv.inv_lw;
    pp.ute = md.ute + md.lute * tv.inv_l + md.wute * tv.inv_w + md.pute * tv.inv_lw;
    pp.ucs = md.ucs + md.lucs * tv.inv_l + md.wucs * tv.inv_w + md.pucs * tv.inv_lw;
    pp.ucste = md.ucste + md.lucste * tv.inv_l + md.wucste * tv.inv_w + md.pucste * tv.inv_lw;
    pp.voff = md.voff + md.lvoff * tv.inv_l + md.wvoff * tv.inv_w + md.pvoff * tv.inv_lw;
    pp.tvoff = md.tvoff + md.ltvoff * tv.inv_l + md.wtvoff * tv.inv_w + md.ptvoff * tv.inv_lw;
    pp.minv = md.minv + md.lminv * tv.inv_l + md.wminv * tv.inv_w + md.pminv * tv.inv_lw;
    pp.minvcv = md.minvcv + md.lminvcv * tv.inv_l + md.wminvcv * tv.inv_w + md.pminvcv * tv.inv_lw;
    pp.fprout = md.fprout + md.lfprout * tv.inv_l + md.wfprout * tv.inv_w + md.pfprout * tv.inv_lw;
    pp.pdits = md.pdits + md.lpdits * tv.inv_l + md.wpdits * tv.inv_w + md.ppdits * tv.inv_lw;
    pp.pditsd = md.pditsd + md.lpditsd * tv.inv_l + md.wpditsd * tv.inv_w + md.ppditsd * tv.inv_lw;
    pp.delta = md.delta + md.ldelta * tv.inv_l + md.wdelta * tv.inv_w + md.pdelta * tv.inv_lw;
    pp.rdsw = md.rdsw + md.lrdsw * tv.inv_l + md.wrdsw * tv.inv_w + md.prdsw * tv.inv_lw;
    pp.rdw = md.rdw + md.lrdw * tv.inv_l + md.wrdw * tv.inv_w + md.prdw * tv.inv_lw;
    pp.rsw = md.rsw + md.lrsw * tv.inv_l + md.wrsw * tv.inv_w + md.prsw * tv.inv_lw;
    pp.prwg = md.prwg + md.lprwg * tv.inv_l + md.wprwg * tv.inv_w + md.pprwg * tv.inv_lw;
    pp.prwb = md.prwb + md.lprwb * tv.inv_l + md.wprwb * tv.inv_w + md.pprwb * tv.inv_lw;
    pp.prt = md.prt + md.lprt * tv.inv_l + md.wprt * tv.inv_w + md.pprt * tv.inv_lw;
    pp.eta0 = md.eta0 + md.leta0 * tv.inv_l + md.weta0 * tv.inv_w + md.peta0 * tv.inv_lw;
    pp.teta0 = md.teta0 + md.lteta0 * tv.inv_l + md.wteta0 * tv.inv_w + md.pteta0 * tv.inv_lw;
    pp.tvoffcv =
        md.tvoffcv + md.ltvoffcv * tv.inv_l + md.wtvoffcv * tv.inv_w + md.ptvoffcv * tv.inv_lw;
    pp.etab = md.etab + md.letab * tv.inv_l + md.wetab * tv.inv_w + md.petab * tv.inv_lw;
    pp.pclm = md.pclm + md.lpclm * tv.inv_l + md.wpclm * tv.inv_w + md.ppclm * tv.inv_lw;
    pp.pdibl1 = md.pdibl1 + md.lpdibl1 * tv.inv_l + md.wpdibl1 * tv.inv_w + md.ppdibl1 * tv.inv_lw;
    pp.pdibl2 = md.pdibl2 + md.lpdibl2 * tv.inv_l + md.wpdibl2 * tv.inv_w + md.ppdibl2 * tv.inv_lw;
    pp.pdiblb = md.pdiblb + md.lpdiblb * tv.inv_l + md.wpdiblb * tv.inv_w + md.ppdiblb * tv.inv_lw;
    pp.pscbe1 = md.pscbe1 + md.lpscbe1 * tv.inv_l + md.wpscbe1 * tv.inv_w + md.ppscbe1 * tv.inv_lw;
    pp.pscbe2 = md.pscbe2 + md.lpscbe2 * tv.inv_l + md.wpscbe2 * tv.inv_w + md.ppscbe2 * tv.inv_lw;
    pp.pvag = md.pvag + md.lpvag * tv.inv_l + md.wpvag * tv.inv_w + md.ppvag * tv.inv_lw;
    pp.wr = md.wr + md.lwr * tv.inv_l + md.wwr * tv.inv_w + md.pwr * tv.inv_lw;
    pp.dwg = md.dwg + md.ldwg * tv.inv_l + md.wdwg * tv.inv_w + md.pdwg * tv.inv_lw;
    pp.dwb = md.dwb + md.ldwb * tv.inv_l + md.wdwb * tv.inv_w + md.pdwb * tv.inv_lw;
    pp.b0 = md.b0 + md.lb0 * tv.inv_l + md.wb0 * tv.inv_w + md.pb0 * tv.inv_lw;
    pp.b1 = md.b1 + md.lb1 * tv.inv_l + md.wb1 * tv.inv_w + md.pb1 * tv.inv_lw;
    pp.alpha0 = md.alpha0 + md.lalpha0 * tv.inv_l + md.walpha0 * tv.inv_w + md.palpha0 * tv.inv_lw;
    pp.alpha1 = md.alpha1 + md.lalpha1 * tv.inv_l + md.walpha1 * tv.inv_w + md.palpha1 * tv.inv_lw;
    pp.beta0 = md.beta0 + md.lbeta0 * tv.inv_l + md.wbeta0 * tv.inv_w + md.pbeta0 * tv.inv_lw;
    pp.agidl = md.agidl + md.lagidl * tv.inv_l + md.wagidl * tv.inv_w + md.pagidl * tv.inv_lw;
    pp.bgidl = md.bgidl + md.lbgidl * tv.inv_l + md.wbgidl * tv.inv_w + md.pbgidl * tv.inv_lw;
    pp.cgidl = md.cgidl + md.lcgidl * tv.inv_l + md.wcgidl * tv.inv_w + md.pcgidl * tv.inv_lw;
    pp.egidl = md.egidl + md.legidl * tv.inv_l + md.wegidl * tv.inv_w + md.pegidl * tv.inv_lw;
    pp.rgidl = md.rgidl + md.lrgidl * tv.inv_l + md.wrgidl * tv.inv_w + md.prgidl * tv.inv_lw;
    pp.kgidl = md.kgidl + md.lkgidl * tv.inv_l + md.wkgidl * tv.inv_w + md.pkgidl * tv.inv_lw;
    pp.fgidl = md.fgidl + md.lfgidl * tv.inv_l + md.wfgidl * tv.inv_w + md.pfgidl * tv.inv_lw;
    pp.agisl = md.agisl + md.lagisl * tv.inv_l + md.wagisl * tv.inv_w + md.pagisl * tv.inv_lw;
    pp.bgisl = md.bgisl + md.lbgisl * tv.inv_l + md.wbgisl * tv.inv_w + md.pbgisl * tv.inv_lw;
    pp.cgisl = md.cgisl + md.lcgisl * tv.inv_l + md.wcgisl * tv.inv_w + md.pcgisl * tv.inv_lw;
    pp.egisl = md.egisl + md.legisl * tv.inv_l + md.wegisl * tv.inv_w + md.pegisl * tv.inv_lw;
    pp.rgisl = md.rgisl + md.lrgisl * tv.inv_l + md.wrgisl * tv.inv_w + md.prgisl * tv.inv_lw;
    pp.kgisl = md.kgisl + md.lkgisl * tv.inv_l + md.wkgisl * tv.inv_w + md.pkgisl * tv.inv_lw;
    pp.fgisl = md.fgisl + md.lfgisl * tv.inv_l + md.wfgisl * tv.inv_w + md.pfgisl * tv.inv_lw;
    pp.aigc = md.aigc + md.laigc * tv.inv_l + md.waigc * tv.inv_w + md.paigc * tv.inv_lw;
    pp.bigc = md.bigc + md.lbigc * tv.inv_l + md.wbigc * tv.inv_w + md.pbigc * tv.inv_lw;
    pp.cigc = md.cigc + md.lcigc * tv.inv_l + md.wcigc * tv.inv_w + md.pcigc * tv.inv_lw;
    pp.aigsd = md.aigsd + md.laigsd * tv.inv_l + md.waigsd * tv.inv_w + md.paigsd * tv.inv_lw;
    pp.bigsd = md.bigsd + md.lbigsd * tv.inv_l + md.wbigsd * tv.inv_w + md.pbigsd * tv.inv_lw;
    pp.cigsd = md.cigsd + md.lcigsd * tv.inv_l + md.wcigsd * tv.inv_w + md.pcigsd * tv.inv_lw;
    pp.aigs = md.aigs + md.laigs * tv.inv_l + md.waigs * tv.inv_w + md.paigs * tv.inv_lw;
    pp.bigs = md.bigs + md.lbigs * tv.inv_l + md.wbigs * tv.inv_w + md.pbigs * tv.inv_lw;
    pp.cigs = md.cigs + md.lcigs * tv.inv_l + md.wcigs * tv.inv_w + md.pcigs * tv.inv_lw;
    pp.aigd = md.aigd + md.laigd * tv.inv_l + md.waigd * tv.inv_w + md.paigd * tv.inv_lw;
    pp.bigd = md.bigd + md.lbigd * tv.inv_l + md.wbigd * tv.inv_w + md.pbigd * tv.inv_lw;
    pp.cigd = md.cigd + md.lcigd * tv.inv_l + md.wcigd * tv.inv_w + md.pcigd * tv.inv_lw;
    pp.aigbacc =
        md.aigbacc + md.laigbacc * tv.inv_l + md.waigbacc * tv.inv_w + md.paigbacc * tv.inv_lw;
    pp.bigbacc =
        md.bigbacc + md.lbigbacc * tv.inv_l + md.wbigbacc * tv.inv_w + md.pbigbacc * tv.inv_lw;
    pp.cigbacc =
        md.cigbacc + md.lcigbacc * tv.inv_l + md.wcigbacc * tv.inv_w + md.pcigbacc * tv.inv_lw;
    pp.aigbinv =
        md.aigbinv + md.laigbinv * tv.inv_l + md.waigbinv * tv.inv_w + md.paigbinv * tv.inv_lw;
    pp.bigbinv =
        md.bigbinv + md.lbigbinv * tv.inv_l + md.wbigbinv * tv.inv_w + md.pbigbinv * tv.inv_lw;
    pp.cigbinv =
        md.cigbinv + md.lcigbinv * tv.inv_l + md.wcigbinv * tv.inv_w + md.pcigbinv * tv.inv_lw;
    pp.nigc = md.nigc + md.lnigc * tv.inv_l + md.wnigc * tv.inv_w + md.pnigc * tv.inv_lw;
    pp.nigbacc =
        md.nigbacc + md.lnigbacc * tv.inv_l + md.wnigbacc * tv.inv_w + md.pnigbacc * tv.inv_lw;
    pp.nigbinv =
        md.nigbinv + md.lnigbinv * tv.inv_l + md.wnigbinv * tv.inv_w + md.pnigbinv * tv.inv_lw;
    pp.ntox = md.ntox + md.lntox * tv.inv_l + md.wntox * tv.inv_w + md.pntox * tv.inv_lw;
    pp.eigbinv =
        md.eigbinv + md.leigbinv * tv.inv_l + md.weigbinv * tv.inv_w + md.peigbinv * tv.inv_lw;
    pp.pigcd = md.pigcd + md.lpigcd * tv.inv_l + md.wpigcd * tv.inv_w + md.ppigcd * tv.inv_lw;
    pp.poxedge =
        md.poxedge + md.lpoxedge * tv.inv_l + md.wpoxedge * tv.inv_w + md.ppoxedge * tv.inv_lw;
    pp.xrcrg1 = md.xrcrg1 + md.lxrcrg1 * tv.inv_l + md.wxrcrg1 * tv.inv_w + md.pxrcrg1 * tv.inv_lw;
    pp.xrcrg2 = md.xrcrg2 + md.lxrcrg2 * tv.inv_l + md.wxrcrg2 * tv.inv_w + md.pxrcrg2 * tv.inv_lw;
    pp.lambda = md.lambda + md.llambda * tv.inv_l + md.wlambda * tv.inv_w + md.plambda * tv.inv_lw;
    pp.vtl = md.vtl + md.lvtl * tv.inv_l + md.wvtl * tv.inv_w + md.pvtl * tv.inv_lw;
    pp.xn = md.xn + md.lxn * tv.inv_l + md.wxn * tv.inv_w + md.pxn * tv.inv_lw;
    pp.vfbsdoff =
        md.vfbsdoff + md.lvfbsdoff * tv.inv_l + md.wvfbsdoff * tv.inv_w + md.pvfbsdoff * tv.inv_lw;
    pp.tvfbsdoff = md.tvfbsdoff
        + md.ltvfbsdoff * tv.inv_l
        + md.wtvfbsdoff * tv.inv_w
        + md.ptvfbsdoff * tv.inv_lw;
    pp.cgsl = md.cgsl + md.lcgsl * tv.inv_l + md.wcgsl * tv.inv_w + md.pcgsl * tv.inv_lw;
    pp.cgdl = md.cgdl + md.lcgdl * tv.inv_l + md.wcgdl * tv.inv_w + md.pcgdl * tv.inv_lw;
    pp.ckappas =
        md.ckappas + md.lckappas * tv.inv_l + md.wckappas * tv.inv_w + md.pckappas * tv.inv_lw;
    pp.ckappad =
        md.ckappad + md.lckappad * tv.inv_l + md.wckappad * tv.inv_w + md.pckappad * tv.inv_lw;
    pp.cf = md.cf + md.lcf * tv.inv_l + md.wcf * tv.inv_w + md.pcf * tv.inv_lw;
    pp.clc = md.clc + md.lclc * tv.inv_l + md.wclc * tv.inv_w + md.pclc * tv.inv_lw;
    pp.cle = md.cle + md.lcle * tv.inv_l + md.wcle * tv.inv_w + md.pcle * tv.inv_lw;
    pp.vfbcv = md.vfbcv + md.lvfbcv * tv.inv_l + md.wvfbcv * tv.inv_w + md.pvfbcv * tv.inv_lw;
    pp.acde = md.acde + md.lacde * tv.inv_l + md.wacde * tv.inv_w + md.pacde * tv.inv_lw;
    pp.moin = md.moin + md.lmoin * tv.inv_l + md.wmoin * tv.inv_w + md.pmoin * tv.inv_lw;
    pp.noff = md.noff + md.lnoff * tv.inv_l + md.wnoff * tv.inv_w + md.pnoff * tv.inv_lw;
    pp.voffcv = md.voffcv + md.lvoffcv * tv.inv_l + md.wvoffcv * tv.inv_w + md.pvoffcv * tv.inv_lw;
    pp.kvth0we =
        md.kvth0we + md.lkvth0we * tv.inv_l + md.wkvth0we * tv.inv_w + md.pkvth0we * tv.inv_lw;
    pp.k2we = md.k2we + md.lk2we * tv.inv_l + md.wk2we * tv.inv_w + md.pk2we * tv.inv_lw;
    pp.ku0we = md.ku0we + md.lku0we * tv.inv_l + md.wku0we * tv.inv_w + md.pku0we * tv.inv_lw;
    pp.abulk_cvfactor = 1.0 + pow(pp.clc / pp.leff_cv, pp.cle);
    tv.t0 = tv.tratio - 1.0;
    tv.powweffwr = pow(pp.weff_cj * 1.0e6, pp.wr) * h.nf;
    tv.t4 = 0.0;
    tv.t3 = tv.t4;
    tv.t2 = tv.t3;
    tv.t1 = tv.t2;
    pp.ucs *= pow(tv.tratio, pp.ucste);
    if md.temp_mod == 0 {
        pp.ua += pp.ua1 * tv.t0;
        pp.ub += pp.ub1 * tv.t0;
        pp.uc += pp.uc1 * tv.t0;
        pp.ud += pp.ud1 * tv.t0;
        pp.vsattemp = pp.vsat - pp.at * tv.t0;
        tv.t10 = pp.prt * tv.t0;
        if md.rds_mod != 0 {
            tv.t1 = pp.rdw + tv.t10;
            tv.t2 = md.rdwmin + tv.t10;
            tv.t3 = pp.rsw + tv.t10;
            tv.t4 = md.rswmin + tv.t10;
        }
        pp.rds0 = (pp.rdsw + tv.t10) * h.nf / tv.powweffwr;
        pp.rdswmin = (md.rdswmin + tv.t10) * h.nf / tv.powweffwr;
    } else {
        if md.temp_mod == 3 {
            pp.ua *= pow(tv.tratio, pp.ua1);
            pp.ub *= pow(tv.tratio, pp.ub1);
            pp.uc *= pow(tv.tratio, pp.uc1);
            pp.ud *= pow(tv.tratio, pp.ud1);
        } else {
            pp.ua *= 1.0 + pp.ua1 * tv.deltemp;
            pp.ub *= 1.0 + pp.ub1 * tv.deltemp;
            pp.uc *= 1.0 + pp.uc1 * tv.deltemp;
            pp.ud *= 1.0 + pp.ud1 * tv.deltemp;
        }
        pp.vsattemp = pp.vsat * (1.0 - pp.at * tv.deltemp);
        tv.t10 = 1.0 + pp.prt * tv.deltemp;
        if md.rds_mod != 0 {
            tv.t1 = pp.rdw * tv.t10;
            tv.t2 = md.rdwmin * tv.t10;
            tv.t3 = pp.rsw * tv.t10;
            tv.t4 = md.rswmin * tv.t10;
        }
        pp.rds0 = pp.rdsw * tv.t10 * h.nf / tv.powweffwr;
        pp.rdswmin = md.rdswmin * tv.t10 * h.nf / tv.powweffwr;
    }
    if tv.t1 < 0.0 {
        tv.t1 = 0.0;
        notes.push("Warning: Rdw at current temperature is negative; set to 0.".to_string());
    }
    if tv.t2 < 0.0 {
        tv.t2 = 0.0;
        notes.push("Warning: Rdwmin at current temperature is negative; set to 0.".to_string());
    }
    pp.rd0 = tv.t1 / tv.powweffwr;
    pp.rdwmin = tv.t2 / tv.powweffwr;
    if tv.t3 < 0.0 {
        tv.t3 = 0.0;
        notes.push("Warning: Rsw at current temperature is negative; set to 0.".to_string());
    }
    if tv.t4 < 0.0 {
        tv.t4 = 0.0;
        notes.push("Warning: Rswmin at current temperature is negative; set to 0.".to_string());
    }
    pp.rs0 = tv.t3 / tv.powweffwr;
    pp.rswmin = tv.t4 / tv.powweffwr;
    if pp.u0 > 1.0 {
        pp.u0 /= 1.0e4;
    }
    tv.t5 = 1.0 - pp.up * exp(-pp.leff / pp.lp);
    pp.u0temp = pp.u0 * tv.t5 * pow(tv.tratio, pp.ute);
    if pp.eu < 0.0 {
        pp.eu = 0.0;
        notes.push("Warning: eu has been negative; reset to 0.0.".to_string());
    }
    if pp.ucs < 0.0 {
        pp.ucs = 0.0;
        notes.push("Warning: ucs has been negative; reset to 0.0.".to_string());
    }
    pp.vfbsdoff *= 1.0 + pp.tvfbsdoff * tv.deltemp;
    pp.voff *= 1.0 + pp.tvoff * tv.deltemp;
    pp.nfactor += pp.tnfactor * tv.deltemp / tv.tnom;
    pp.voffcv *= 1.0 + pp.tvoffcv * tv.deltemp;
    pp.eta0 += pp.teta0 * tv.deltemp / tv.tnom;
    if md.given.vtl && md.vtl > 0.0 {
        if md.lc < 0.0 {
            pp.lc = 0.0;
        } else {
            pp.lc = md.lc;
        }
        tv.t0 = pp.leff / (pp.xn * pp.leff + pp.lc);
        pp.tfactor = (1.0 - tv.t0) / (1.0 + tv.t0);
    }
    pp.cgdo = (md.cgdo + pp.cf) * pp.weff_cv;
    pp.cgso = (md.cgso + pp.cf) * pp.weff_cv;
    pp.cgbo = md.cgbo * pp.leff_cv * h.nf;
    if !md.given.ndep && md.given.gamma1 {
        tv.t0 = pp.gamma1 * md.coxe;
        pp.ndep = 3.01248e22 * tv.t0 * tv.t0;
    }
    pp.phi = tv.vtm0 * log(pp.ndep / tv.ni) + pp.phin + 0.4;
    if pp.phi <= 0.0 {
        notes.push(format!(
            "Fatal: Phi = {} is not positive. Please check Phin and Ndep",
            pp.phi
        ));
        notes.push(format!("Phin = {}  Ndep = {}", pp.phin, pp.ndep));
        tv.fatal_flag = 1;
    }
    pp.sqrt_phi = sqrt(pp.phi);
    pp.phis3 = pp.sqrt_phi * pp.phi;
    pp.xdep0 = sqrt(2.0 * tv.epssub / (CHARGE_Q * pp.ndep * 1.0e6)) * pp.sqrt_phi;
    pp.sqrt_xdep0 = sqrt(pp.xdep0);
    if md.mtrl_mod == 0 {
        pp.litl = sqrt(3.0 * 3.9 / tv.epsrox * pp.xj * tv.toxe);
    } else {
        pp.litl = sqrt(md.epsrsub / tv.epsrox * pp.xj * tv.toxe);
    }
    pp.vbi = tv.vtm0 * log(pp.nsd * pp.ndep / (tv.ni * tv.ni));
    if md.mtrl_mod == 0 {
        if pp.ngate > 0.0 {
            pp.vfbsd = tv.vtm0 * log(pp.ngate / pp.nsd);
        } else {
            pp.vfbsd = 0.0;
        }
    } else {
        tv.t0 = tv.vtm0 * log(pp.nsd / tv.ni);
        tv.t1 = 0.5 * tv.eg0;
        if tv.t0 > tv.t1 {
            tv.t0 = tv.t1;
        }
        tv.t2 = md.easub + tv.t1 - md.type_ as f64 * tv.t0;
        pp.vfbsd = md.phig - tv.t2;
    }
    pp.cdep0 = sqrt(CHARGE_Q * tv.epssub * pp.ndep * 1.0e6 / 2.0 / pp.phi);
    pp.tox_ratio = exp(pp.ntox * log(md.toxref / tv.toxe)) / tv.toxe / tv.toxe;
    pp.tox_ratio_edge = exp(pp.ntox * log(md.toxref / (tv.toxe * pp.poxedge)))
        / tv.toxe
        / tv.toxe
        / pp.poxedge
        / pp.poxedge;
    pp.aechvb = if md.type_ == NMOS {
        4.97232e-7
    } else {
        3.42537e-7
    };
    pp.bechvb = if md.type_ == NMOS {
        7.45669e11
    } else {
        1.16645e12
    };
    if md.v48int_version <= 480 {
        pp.aechvb_edge_s = pp.aechvb * pp.weff * md.dlcig * pp.tox_ratio_edge;
        pp.aechvb_edge_d = pp.aechvb * pp.weff * md.dlcigd * pp.tox_ratio_edge;
    } else {
        if md.dlcig < 0.0 {
            notes.push(format!(
                "Warning: dlcig = {} is negative. Set to zero.",
                md.dlcig
            ));
            md.dlcig = 0.0;
        }
        pp.aechvb_edge_s = pp.aechvb * pp.weff * md.dlcig * pp.tox_ratio_edge;
        if md.dlcigd < 0.0 {
            notes.push(format!(
                "Warning: dlcigd = {} is negative. Set to zero.",
                md.dlcigd
            ));
            md.dlcigd = 0.0;
        }
        pp.aechvb_edge_d = pp.aechvb * pp.weff * md.dlcigd * pp.tox_ratio_edge;
    }
    pp.bechvb_edge = -pp.bechvb * tv.toxe * pp.poxedge;
    pp.aechvb *= pp.weff * pp.leff * pp.tox_ratio;
    pp.bechvb *= -tv.toxe;
    pp.mstar = 0.5 + atan(pp.minv) / PI;
    pp.mstarcv = 0.5 + atan(pp.minvcv) / PI;
    pp.voffcbn = pp.voff + md.voffl / pp.leff;
    pp.voffcbncv = pp.voffcv + md.voffcvl / pp.leff;
    pp.ldeb = sqrt(tv.epssub * tv.vtm0 / (CHARGE_Q * pp.ndep * 1.0e6)) / 3.0;
    pp.acde *= pow(pp.ndep / 2.0e16, -0.25);
    if md.given.k1 || md.given.k2 {
        if !md.given.k1 {
            notes.push("Warning: k1 should be specified with k2.".to_string());
            pp.k1 = 0.53;
        }
        if !md.given.k2 {
            notes.push("Warning: k2 should be specified with k1.".to_string());
            pp.k2 = -0.0186;
        }
        if md.given.nsub {
            notes.push("Warning: nsub is ignored because k1 or k2 is given.".to_string());
        }
        if md.given.xt {
            notes.push("Warning: xt is ignored because k1 or k2 is given.".to_string());
        }
        if md.given.vbx {
            notes.push("Warning: vbx is ignored because k1 or k2 is given.".to_string());
        }
        if md.given.gamma1 {
            notes.push("Warning: gamma1 is ignored because k1 or k2 is given.".to_string());
        }
        if md.given.gamma2 {
            notes.push("Warning: gamma2 is ignored because k1 or k2 is given.".to_string());
        }
    } else {
        if !md.given.vbx {
            pp.vbx = pp.phi - 7.7348e-4 * pp.ndep * pp.xt * pp.xt;
        }
        if pp.vbx > 0.0 {
            pp.vbx = -pp.vbx;
        }
        if pp.vbm > 0.0 {
            pp.vbm = -pp.vbm;
        }
        if !md.given.gamma1 {
            pp.gamma1 = 5.753e-12 * sqrt(pp.ndep) / md.coxe;
        }
        if !md.given.gamma2 {
            pp.gamma2 = 5.753e-12 * sqrt(pp.nsub) / md.coxe;
        }
        tv.t0 = pp.gamma1 - pp.gamma2;
        tv.t1 = sqrt(pp.phi - pp.vbx) - pp.sqrt_phi;
        tv.t2 = sqrt(pp.phi * (pp.phi - pp.vbm)) - pp.phi;
        pp.k2 = tv.t0 * tv.t1 / (2.0 * tv.t2 + pp.vbm);
        pp.k1 = pp.gamma2 - 2.0 * pp.k2 * sqrt(pp.phi - pp.vbm);
    }
    if !md.given.vfb {
        if md.given.vth0 {
            pp.vfb = md.type_ as f64 * pp.vth0 - pp.phi - pp.k1 * pp.sqrt_phi;
        } else if md.mtrl_mod != 0 && md.given.phig && md.given.nsub {
            tv.t0 = tv.vtm0 * log(pp.nsub / tv.ni);
            tv.t1 = 0.5 * tv.eg0;
            if tv.t0 > tv.t1 {
                tv.t0 = tv.t1;
            }
            tv.t2 = md.easub + tv.t1 + md.type_ as f64 * tv.t0;
            pp.vfb = md.phig - tv.t2;
        } else {
            pp.vfb = -1.0;
        }
    }
    if !md.given.vth0 {
        pp.vth0 = md.type_ as f64 * (pp.vfb + pp.phi + pp.k1 * pp.sqrt_phi);
    }
    pp.k1ox = pp.k1 * tv.toxe / md.toxm;
    tv.tmp = sqrt(tv.epssub / (tv.epsrox * EPS0) * tv.toxe * pp.xdep0);
    tv.t0 = pp.dsub * pp.leff / tv.tmp;
    if tv.t0 < EXP_THRESHOLD {
        tv.t1 = exp(tv.t0);
        tv.t2 = tv.t1 - 1.0;
        tv.t3 = tv.t2 * tv.t2;
        tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
        pp.theta0vb0 = tv.t1 / tv.t4;
    } else {
        pp.theta0vb0 = 1.0 / (MAX_EXP - 2.0);
    }
    tv.t0 = pp.drout * pp.leff / tv.tmp;
    if tv.t0 < EXP_THRESHOLD {
        tv.t1 = exp(tv.t0);
        tv.t2 = tv.t1 - 1.0;
        tv.t3 = tv.t2 * tv.t2;
        tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
        tv.t5 = tv.t1 / tv.t4;
    } else {
        tv.t5 = 1.0 / (MAX_EXP - 2.0);
    }
    pp.theta_rout = pp.pdibl1 * tv.t5 + pp.pdibl2;
    tv.tmp = sqrt(pp.xdep0);
    tv.tmp1 = pp.vbi - pp.phi;
    tv.tmp2 = md.factor1 * tv.tmp;
    tv.t0 = pp.dvt1w * pp.weff * pp.leff / tv.tmp2;
    if tv.t0 < EXP_THRESHOLD {
        tv.t1 = exp(tv.t0);
        tv.t2 = tv.t1 - 1.0;
        tv.t3 = tv.t2 * tv.t2;
        tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
        tv.t8 = tv.t1 / tv.t4;
    } else {
        tv.t8 = 1.0 / (MAX_EXP - 2.0);
    }
    tv.t0 = pp.dvt0w * tv.t8;
    tv.t8 = tv.t0 * tv.tmp1;
    tv.t0 = pp.dvt1 * pp.leff / tv.tmp2;
    if tv.t0 < EXP_THRESHOLD {
        tv.t1 = exp(tv.t0);
        tv.t2 = tv.t1 - 1.0;
        tv.t3 = tv.t2 * tv.t2;
        tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
        tv.t9 = tv.t1 / tv.t4;
    } else {
        tv.t9 = 1.0 / (MAX_EXP - 2.0);
    }
    tv.t9 = pp.dvt0 * tv.t9 * tv.tmp1;
    tv.t4 = tv.toxe * pp.phi / (pp.weff + pp.w0);
    tv.t0 = sqrt(1.0 + pp.lpe0 / pp.leff);
    if md.temp_mod == 1 || md.temp_mod == 0 {
        tv.t3 = (pp.kt1 + pp.kt1l / pp.leff) * (tv.tratio - 1.0);
    }
    if md.temp_mod == 2 || md.temp_mod == 3 {
        tv.t3 = -pp.kt1 * (tv.tratio - 1.0);
    }
    tv.t5 = pp.k1ox * (tv.t0 - 1.0) * pp.sqrt_phi + tv.t3;
    pp.vfbzbfactor = -tv.t8 - tv.t9 + pp.k3 * tv.t4 + tv.t5 - pp.phi - pp.k1 * pp.sqrt_phi;
    tv.wlod = md.wlod;
    if md.wlod < 0.0 {
        notes.push(format!(
            "Warning: WLOD = {} is less than 0. 0.0 is used",
            md.wlod
        ));
        tv.wlod = 0.0;
    }
    tv.t0 = pow(tv.lnew, md.llodku0);
    tv.w_tmp = tv.wnew + tv.wlod;
    tv.t1 = pow(tv.w_tmp, md.wlodku0);
    tv.tmp1 = md.lku0 / tv.t0 + md.wku0 / tv.t1 + md.pku0 / (tv.t0 * tv.t1);
    pp.ku0 = 1.0 + tv.tmp1;
    tv.t0 = pow(tv.lnew, md.llodvth);
    tv.t1 = pow(tv.w_tmp, md.wlodvth);
    tv.tmp1 = md.lkvth0 / tv.t0 + md.wkvth0 / tv.t1 + md.pkvth0 / (tv.t0 * tv.t1);
    pp.kvth0 = 1.0 + tv.tmp1;
    pp.kvth0 = sqrt(pp.kvth0 * pp.kvth0 + DELTA);
    tv.t0 = tv.tratio - 1.0;
    pp.ku0temp = pp.ku0 * (1.0 + md.tku0 * tv.t0) + DELTA;
    tv.inv_saref = 1.0 / (md.saref + 0.5 * tv.ldrn);
    tv.inv_sbref = 1.0 / (md.sbref + 0.5 * tv.ldrn);
    pp.inv_od_ref = tv.inv_saref + tv.inv_sbref;
    pp.rho_ref = md.ku0 / pp.ku0temp * pp.inv_od_ref;
    if md.mob_mod == 3 {
        tv.lt1 = md.factor1 * pp.sqrt_xdep0;
        tv.t0 = pp.dvt1 * pp.leff / tv.lt1;
        if tv.t0 < EXP_THRESHOLD {
            tv.t1 = exp(tv.t0);
            tv.t2 = tv.t1 - 1.0;
            tv.t3 = tv.t2 * tv.t2;
            tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
            tv.theta0 = tv.t1 / tv.t4;
        } else {
            tv.theta0 = 1.0 / (MAX_EXP - 2.0);
        }
        tv.tmp1 = tv.epssub / pp.xdep0;
        tv.tmp2 = pp.nfactor * tv.tmp1;
        tv.tmp3 = (tv.tmp2 + pp.cdsc * tv.theta0 + pp.cit) / md.coxe;
        if tv.tmp3 >= -0.5 {
            tv.n0 = 1.0 + tv.tmp3;
        } else {
            tv.t0 = 1.0 / (3.0 + 8.0 * tv.tmp3);
            tv.n0 = (1.0 + 3.0 * tv.tmp3) * tv.t0;
        }
        tv.t0 = tv.n0 * md.vtm;
        tv.t1 = pp.voffcbn;
        tv.t2 = tv.t1 / tv.t0;
        if tv.t2 < -EXP_THRESHOLD {
            tv.t3 = md.coxe * MIN_EXP / pp.cdep0;
            tv.t4 = pp.mstar + tv.t3 * tv.n0;
        } else if tv.t2 > EXP_THRESHOLD {
            tv.t3 = md.coxe * MAX_EXP / pp.cdep0;
            tv.t4 = pp.mstar + tv.t3 * tv.n0;
        } else {
            tv.t3 = exp(tv.t2) * md.coxe / pp.cdep0;
            tv.t4 = pp.mstar + tv.t3 * tv.n0;
        }
        pp.vgsteff_vth = tv.t0 * log(2.0) / tv.t4;
    }
    tv.t0 = -pp.dvtp3 * log(pp.leff);
    tv.t1 = dexp2(tv.t0);
    pp.dvtp2factor = pp.dvtp5 + pp.dvtp2 * tv.t1;
    Ok(())
}

/// The per-instance part of `BSIM4temp` (stress and well-proximity
/// effects, junction and geometry processing, `toxp`).
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn temp_instance(
    md: &mut Bsim4Model,
    h: &mut Bsim4Here,
    pp: &Bsim4Size,
    tv: &mut TempVars,
    _ck_temp: f64,
    notes: &mut Vec<String>,
) {
    tv.ldrn = h.l;
    tv.wdrn = h.w / h.nf;
    if h.sa > 0.0 && h.sb > 0.0 && (h.nf == 1.0 || h.nf > 1.0 && h.sd > 0.0) {
        tv.inv_sa = 0.0;
        tv.inv_sb = 0.0;
        tv.kvsat = md.kvsat;
        if md.kvsat < -1.0 {
            notes.push(format!(
                "Warning: KVSAT = {} is too small; -1.0 is used.",
                md.kvsat
            ));
            tv.kvsat = -1.0;
        }
        if md.kvsat > 1.0 {
            notes.push(format!(
                "Warning: KVSAT = {} is too big; 1.0 is used.",
                md.kvsat
            ));
            tv.kvsat = 1.0;
        }
        tv.i = 0;
        while (tv.i as f64) < h.nf {
            tv.t0 = 1.0 / h.nf / (h.sa + 0.5 * tv.ldrn + tv.i as f64 * (h.sd + tv.ldrn));
            tv.t1 = 1.0 / h.nf / (h.sb + 0.5 * tv.ldrn + tv.i as f64 * (h.sd + tv.ldrn));
            tv.inv_sa += tv.t0;
            tv.inv_sb += tv.t1;
            tv.i += 1;
        }
        tv.inv_odeff = tv.inv_sa + tv.inv_sb;
        tv.rho = md.ku0 / pp.ku0temp * tv.inv_odeff;
        tv.t0 = (1.0 + tv.rho) / (1.0 + pp.rho_ref);
        h.u0temp = pp.u0temp * tv.t0;
        tv.t1 = (1.0 + tv.kvsat * tv.rho) / (1.0 + tv.kvsat * pp.rho_ref);
        h.vsattemp = pp.vsattemp * tv.t1;
        tv.od_offset = tv.inv_odeff - pp.inv_od_ref;
        tv.dvth0_lod = md.kvth0 / pp.kvth0 * tv.od_offset;
        tv.dk2_lod = md.stk2 / pow(pp.kvth0, md.lodk2) * tv.od_offset;
        tv.deta0_lod = md.steta0 / pow(pp.kvth0, md.lodeta0) * tv.od_offset;
        h.vth0 = pp.vth0 + tv.dvth0_lod;
        h.eta0 = pp.eta0 + tv.deta0_lod;
        h.k2 = pp.k2 + tv.dk2_lod;
    } else {
        h.u0temp = pp.u0temp;
        h.vth0 = pp.vth0;
        h.vsattemp = pp.vsattemp;
        h.eta0 = pp.eta0;
        h.k2 = pp.k2;
    }
    if md.wpemod != 0.0 {
        if !h.given.sca && !h.given.scb && !h.given.scc {
            if h.given.sc && h.sc > 0.0 {
                tv.t1 = h.sc + tv.wdrn;
                tv.t2 = 1.0 / md.scref;
                h.sca = md.scref * md.scref / (h.sc * tv.t1);
                h.scb = ((0.1 * h.sc + 0.01 * md.scref) * exp(-10.0 * h.sc * tv.t2)
                    - (0.1 * tv.t1 + 0.01 * md.scref) * exp(-10.0 * tv.t1 * tv.t2))
                    / tv.wdrn;
                h.scc = ((0.05 * h.sc + 0.0025 * md.scref) * exp(-20.0 * h.sc * tv.t2)
                    - (0.05 * tv.t1 + 0.0025 * md.scref) * exp(-20.0 * tv.t1 * tv.t2))
                    / tv.wdrn;
            } else {
                notes.push(
                    "Warning: No WPE as none of SCA, SCB, SCC, SC is given and/or SC not positive."
                        .to_string(),
                );
            }
        }
        if h.sca < 0.0 {
            notes.push(format!("Warning: SCA = {} is negative. Set to 0.0.", h.sca));
            h.sca = 0.0;
        }
        if h.scb < 0.0 {
            notes.push(format!("Warning: SCB = {} is negative. Set to 0.0.", h.scb));
            h.scb = 0.0;
        }
        if h.scc < 0.0 {
            notes.push(format!("Warning: SCC = {} is negative. Set to 0.0.", h.scc));
            h.scc = 0.0;
        }
        if h.sc < 0.0 {
            notes.push(format!("Warning: SC = {} is negative. Set to 0.0.", h.sc));
            h.sc = 0.0;
        }
        tv.sceff = h.sca + md.web * h.scb + md.wec * h.scc;
        h.vth0 += pp.kvth0we * tv.sceff;
        h.k2 += pp.k2we * tv.sceff;
        tv.t3 = 1.0 + pp.ku0we * tv.sceff;
        if tv.t3 <= 0.0 {
            tv.t3 = 0.0;
            notes.push(format!(
                "Warning: ku0we = {} is negatively too high. Negative mobility!",
                pp.ku0we
            ));
        }
        h.u0temp *= tv.t3;
    }
    h.vth0 += h.delvto;
    h.vfb = pp.vfb + md.type_ as f64 * h.delvto;
    h.u0temp = pp.u0temp * h.mulu0;
    tv.t3 = md.type_ as f64 * h.vth0 - h.vfb - pp.phi;
    tv.t4 = tv.t3 + tv.t3;
    tv.t5 = 2.5 * tv.t3;
    h.vtfbphi1 = if md.type_ == NMOS { tv.t4 } else { tv.t5 };
    if h.vtfbphi1 < 0.0 {
        h.vtfbphi1 = 0.0;
    }
    h.vtfbphi2 = 4.0 * tv.t3;
    if h.vtfbphi2 < 0.0 {
        h.vtfbphi2 = 0.0;
    }
    if h.k2 < 0.0 {
        tv.t0 = 0.5 * pp.k1 / h.k2;
        h.vbsc = 0.9 * (pp.phi - tv.t0 * tv.t0);
        h.vbsc = h.vbsc.clamp(-30.0, -3.0);
    } else {
        h.vbsc = -30.0;
    }
    if h.vbsc > pp.vbm {
        h.vbsc = pp.vbm;
    }
    h.k2ox = h.k2 * tv.toxe / md.toxm;
    h.vfbzb = pp.vfbzbfactor + md.type_ as f64 * h.vth0;
    h.cgso = pp.cgso;
    h.cgdo = pp.cgdo;
    tv.lnl = log(pp.leff * 1.0e6);
    tv.lnw = log(pp.weff * 1.0e6);
    tv.lnnf = log(h.nf);
    tv.bodymode = 5.0;
    if !md.given.rbps0 || !md.given.rbpd0 {
        tv.bodymode = 1.0;
    } else if !md.given.rbsbx0 && !md.given.rbsby0 || !md.given.rbdbx0 && !md.given.rbdby0 {
        tv.bodymode = 3.0;
    }
    if h.rbody_mod == 2 {
        if tv.bodymode == 5.0 {
            tv.rbsbx =
                md.rbsbx0 * exp(md.rbsdbxl * tv.lnl + md.rbsdbxw * tv.lnw + md.rbsdbxnf * tv.lnnf);
            tv.rbsby =
                md.rbsby0 * exp(md.rbsdbyl * tv.lnl + md.rbsdbyw * tv.lnw + md.rbsdbynf * tv.lnnf);
            h.rbsb = tv.rbsbx * tv.rbsby / (tv.rbsbx + tv.rbsby);
            tv.rbdbx =
                md.rbdbx0 * exp(md.rbsdbxl * tv.lnl + md.rbsdbxw * tv.lnw + md.rbsdbxnf * tv.lnnf);
            tv.rbdby =
                md.rbdby0 * exp(md.rbsdbyl * tv.lnl + md.rbsdbyw * tv.lnw + md.rbsdbynf * tv.lnnf);
            h.rbdb = tv.rbdbx * tv.rbdby / (tv.rbdbx + tv.rbdby);
        }
        if tv.bodymode == 3.0 || tv.bodymode == 5.0 {
            h.rbps = md.rbps0 * exp(md.rbpsl * tv.lnl + md.rbpsw * tv.lnw + md.rbpsnf * tv.lnnf);
            h.rbpd = md.rbpd0 * exp(md.rbpdl * tv.lnl + md.rbpdw * tv.lnw + md.rbpdnf * tv.lnnf);
        }
        tv.rbpbx = md.rbpbx0 * exp(md.rbpbxl * tv.lnl + md.rbpbxw * tv.lnw + md.rbpbxnf * tv.lnnf);
        tv.rbpby = md.rbpby0 * exp(md.rbpbyl * tv.lnl + md.rbpbyw * tv.lnw + md.rbpbynf * tv.lnnf);
        h.rbpb = tv.rbpbx * tv.rbpby / (tv.rbpbx + tv.rbpby);
    }
    if h.rbody_mod == 1 || h.rbody_mod == 2 && tv.bodymode == 5.0 {
        if h.rbdb < 1.0e-3 {
            h.grbdb = 1.0e3;
        } else {
            h.grbdb = md.gbmin + 1.0 / h.rbdb;
        }
        if h.rbpb < 1.0e-3 {
            h.grbpb = 1.0e3;
        } else {
            h.grbpb = md.gbmin + 1.0 / h.rbpb;
        }
        if h.rbps < 1.0e-3 {
            h.grbps = 1.0e3;
        } else {
            h.grbps = md.gbmin + 1.0 / h.rbps;
        }
        if h.rbsb < 1.0e-3 {
            h.grbsb = 1.0e3;
        } else {
            h.grbsb = md.gbmin + 1.0 / h.rbsb;
        }
        if h.rbpd < 1.0e-3 {
            h.grbpd = 1.0e3;
        } else {
            h.grbpd = md.gbmin + 1.0 / h.rbpd;
        }
    }
    if h.rbody_mod == 2 && tv.bodymode == 3.0 {
        h.grbsb = md.gbmin;
        h.grbdb = h.grbsb;
        if h.rbpb < 1.0e-3 {
            h.grbpb = 1.0e3;
        } else {
            h.grbpb = md.gbmin + 1.0 / h.rbpb;
        }
        if h.rbps < 1.0e-3 {
            h.grbps = 1.0e3;
        } else {
            h.grbps = md.gbmin + 1.0 / h.rbps;
        }
        if h.rbpd < 1.0e-3 {
            h.grbpd = 1.0e3;
        } else {
            h.grbpd = md.gbmin + 1.0 / h.rbpd;
        }
    }
    if h.rbody_mod == 2 && tv.bodymode == 1.0 {
        h.grbsb = md.gbmin;
        h.grbdb = h.grbsb;
        h.grbpd = 1.0e3;
        h.grbps = h.grbpd;
        if h.rbpb < 1.0e-3 {
            h.grbpb = 1.0e3;
        } else {
            h.grbpb = md.gbmin + 1.0 / h.rbpb;
        }
    }
    h.grgeltd =
        md.rshg * (h.xgw + pp.weff_cj / 3.0 / h.ngcon) / (h.ngcon * h.nf * (tv.lnew - md.xgl));
    if h.grgeltd > 0.0 {
        h.grgeltd = 1.0 / h.grgeltd;
    } else {
        h.grgeltd = 1.0e3;
        if h.rgate_mod != 0 {
            notes.push("Warning: The gate conductance reset to 1.0e3 mho.".to_string());
        }
    }
    tv.dmcgeff = md.dmcg - md.dmcgt;
    tv.dmcieff = md.dmci;
    tv.dmdgeff = md.dmdg - md.dmcgt;
    if h.given.source_perimeter {
        if h.source_perimeter == 0.0 {
            h.pseff = 0.0;
        } else if h.source_perimeter < 0.0 {
            notes.push(
                "Warning: Source Perimeter is specified as negative, it is set to zero."
                    .to_string(),
            );
            h.pseff = 0.0;
        } else if md.per_mod == 0 {
            h.pseff = h.source_perimeter;
        } else {
            h.pseff = h.source_perimeter - pp.weff_cj * h.nf;
        }
    } else {
        let _ = pa_eff_geo(
            h.nf,
            h.geo_mod,
            h.min,
            pp.weff_cj,
            tv.dmcgeff,
            tv.dmcieff,
            tv.dmdgeff,
            &mut h.pseff,
            &mut tv.dumpd,
            &mut tv.dumas,
            &mut tv.dumad,
        );
    }
    if h.pseff < 0.0 {
        h.pseff = 0.0;
        notes.push("Warning: Pseff is negative, it is set to zero.".to_string());
    }
    if h.given.drain_perimeter {
        if h.drain_perimeter == 0.0 {
            h.pdeff = 0.0;
        } else if h.drain_perimeter < 0.0 {
            notes.push(
                "Warning: Drain Perimeter is specified as negative, it is set to zero.".to_string(),
            );
            h.pdeff = 0.0;
        } else if md.per_mod == 0 {
            h.pdeff = h.drain_perimeter;
        } else {
            h.pdeff = h.drain_perimeter - pp.weff_cj * h.nf;
        }
    } else {
        let _ = pa_eff_geo(
            h.nf,
            h.geo_mod,
            h.min,
            pp.weff_cj,
            tv.dmcgeff,
            tv.dmcieff,
            tv.dmdgeff,
            &mut tv.dumps,
            &mut h.pdeff,
            &mut tv.dumas,
            &mut tv.dumad,
        );
    }
    if h.pdeff < 0.0 {
        h.pdeff = 0.0;
        notes.push("Warning: Pdeff is negative, it is set to zero.".to_string());
    }
    if h.given.source_area {
        h.aseff = h.source_area;
    } else {
        let _ = pa_eff_geo(
            h.nf,
            h.geo_mod,
            h.min,
            pp.weff_cj,
            tv.dmcgeff,
            tv.dmcieff,
            tv.dmdgeff,
            &mut tv.dumps,
            &mut tv.dumpd,
            &mut h.aseff,
            &mut tv.dumad,
        );
    }
    if h.aseff < 0.0 {
        h.aseff = 0.0;
        notes.push("Warning: Aseff is negative, it is set to zero.".to_string());
    }
    if h.given.drain_area {
        h.adeff = h.drain_area;
    } else {
        let _ = pa_eff_geo(
            h.nf,
            h.geo_mod,
            h.min,
            pp.weff_cj,
            tv.dmcgeff,
            tv.dmcieff,
            tv.dmdgeff,
            &mut tv.dumps,
            &mut tv.dumpd,
            &mut tv.dumas,
            &mut h.adeff,
        );
    }
    if h.adeff < 0.0 {
        h.adeff = 0.0;
        notes.push("Warning: Adeff is negative, it is set to zero.".to_string());
    }
    if h.s_node_prime != h.s_node {
        h.source_conductance = 0.0;
        if h.given.source_squares {
            h.source_conductance = md.sheet_resistance * h.source_squares;
        } else if h.rgeo_mod > 0 {
            let _ = rdseff_geo(
                h.nf,
                h.geo_mod,
                h.rgeo_mod,
                h.min,
                pp.weff_cj,
                md.sheet_resistance,
                tv.dmcgeff,
                tv.dmcieff,
                tv.dmdgeff,
                1,
                &mut h.source_conductance,
            );
        } else {
            h.source_conductance = 0.0;
        }
        if h.source_conductance > 0.0 {
            h.source_conductance = 1.0 / h.source_conductance;
        } else {
            h.source_conductance = 1.0e3;
            notes.push("Warning: Source conductance reset to 1.0e3 mho.".to_string());
        }
    } else {
        h.source_conductance = 0.0;
    }
    if h.d_node_prime != h.d_node {
        h.drain_conductance = 0.0;
        if h.given.drain_squares {
            h.drain_conductance = md.sheet_resistance * h.drain_squares;
        } else if h.rgeo_mod > 0 {
            let _ = rdseff_geo(
                h.nf,
                h.geo_mod,
                h.rgeo_mod,
                h.min,
                pp.weff_cj,
                md.sheet_resistance,
                tv.dmcgeff,
                tv.dmcieff,
                tv.dmdgeff,
                0,
                &mut h.drain_conductance,
            );
        } else {
            h.drain_conductance = 0.0;
        }
        if h.drain_conductance > 0.0 {
            h.drain_conductance = 1.0 / h.drain_conductance;
        } else {
            h.drain_conductance = 1.0e3;
            notes.push("Warning: Drain conductance reset to 1.0e3 mho.".to_string());
        }
    } else {
        h.drain_conductance = 0.0;
    }
    tv.nvtms = md.vtm * md.sjct_emission_coeff;
    if h.aseff <= 0.0 && h.pseff <= 0.0 {
        tv.sourcesatcurrent = 0.0;
    } else {
        tv.sourcesatcurrent = h.aseff * md.sjct_temp_sat_cur_density
            + h.pseff * md.sjct_sidewall_temp_sat_cur_density
            + pp.weff_cj * h.nf * md.sjct_gate_sidewall_temp_sat_cur_density;
    }
    if tv.sourcesatcurrent > 0.0 {
        match md.dio_mod {
            0 => {
                if md.bvs / tv.nvtms > EXP_THRESHOLD {
                    h.xexp_bvs = md.xjbvs * MIN_EXP;
                } else {
                    h.xexp_bvs = md.xjbvs * exp(-md.bvs / tv.nvtms);
                }
            }
            1 => {
                let _ = dio_ijth_vjm_eval(
                    tv.nvtms,
                    md.ijthsfwd,
                    tv.sourcesatcurrent,
                    0.0,
                    &mut h.vjsm_fwd,
                );
                h.ivjsm_fwd = tv.sourcesatcurrent * exp(h.vjsm_fwd / tv.nvtms);
            }
            2 => {
                if md.bvs / tv.nvtms > EXP_THRESHOLD {
                    h.xexp_bvs = md.xjbvs * MIN_EXP;
                    tv.tmp = MIN_EXP;
                } else {
                    h.xexp_bvs = exp(-md.bvs / tv.nvtms);
                    tv.tmp = h.xexp_bvs;
                    h.xexp_bvs *= md.xjbvs;
                }
                let _ = dio_ijth_vjm_eval(
                    tv.nvtms,
                    md.ijthsfwd,
                    tv.sourcesatcurrent,
                    h.xexp_bvs,
                    &mut h.vjsm_fwd,
                );
                tv.t0 = exp(h.vjsm_fwd / tv.nvtms);
                h.ivjsm_fwd = tv.sourcesatcurrent * (tv.t0 - h.xexp_bvs / tv.t0 + h.xexp_bvs - 1.0);
                h.sslp_fwd = tv.sourcesatcurrent * (tv.t0 + h.xexp_bvs / tv.t0) / tv.nvtms;
                tv.t2 = md.ijthsrev / tv.sourcesatcurrent;
                if tv.t2 < 1.0 {
                    tv.t2 = 10.0;
                    notes.push(
                        "Warning: ijthsrev too small and set to 10 times IsbSat.".to_string(),
                    );
                }
                h.vjsm_rev = -md.bvs - tv.nvtms * log((tv.t2 - 1.0) / md.xjbvs);
                tv.t1 = md.xjbvs * exp(-(md.bvs + h.vjsm_rev) / tv.nvtms);
                h.ivjsm_rev = tv.sourcesatcurrent * (1.0 + tv.t1);
                h.sslp_rev = -tv.sourcesatcurrent * tv.t1 / tv.nvtms;
            }
            _ => {
                notes.push(format!("Specified dioMod = {} not matched", md.dio_mod));
            }
        }
    }
    tv.nvtmd = md.vtm * md.djct_emission_coeff;
    if h.adeff <= 0.0 && h.pdeff <= 0.0 {
        tv.drainsatcurrent = 0.0;
    } else {
        tv.drainsatcurrent = h.adeff * md.djct_temp_sat_cur_density
            + h.pdeff * md.djct_sidewall_temp_sat_cur_density
            + pp.weff_cj * h.nf * md.djct_gate_sidewall_temp_sat_cur_density;
    }
    if tv.drainsatcurrent > 0.0 {
        match md.dio_mod {
            0 => {
                if md.bvd / tv.nvtmd > EXP_THRESHOLD {
                    h.xexp_bvd = md.xjbvd * MIN_EXP;
                } else {
                    h.xexp_bvd = md.xjbvd * exp(-md.bvd / tv.nvtmd);
                }
            }
            1 => {
                let _ = dio_ijth_vjm_eval(
                    tv.nvtmd,
                    md.ijthdfwd,
                    tv.drainsatcurrent,
                    0.0,
                    &mut h.vjdm_fwd,
                );
                h.ivjdm_fwd = tv.drainsatcurrent * exp(h.vjdm_fwd / tv.nvtmd);
            }
            2 => {
                if md.bvd / tv.nvtmd > EXP_THRESHOLD {
                    h.xexp_bvd = md.xjbvd * MIN_EXP;
                    tv.tmp = MIN_EXP;
                } else {
                    h.xexp_bvd = exp(-md.bvd / tv.nvtmd);
                    tv.tmp = h.xexp_bvd;
                    h.xexp_bvd *= md.xjbvd;
                }
                let _ = dio_ijth_vjm_eval(
                    tv.nvtmd,
                    md.ijthdfwd,
                    tv.drainsatcurrent,
                    h.xexp_bvd,
                    &mut h.vjdm_fwd,
                );
                tv.t0 = exp(h.vjdm_fwd / tv.nvtmd);
                h.ivjdm_fwd = tv.drainsatcurrent * (tv.t0 - h.xexp_bvd / tv.t0 + h.xexp_bvd - 1.0);
                h.dslp_fwd = tv.drainsatcurrent * (tv.t0 + h.xexp_bvd / tv.t0) / tv.nvtmd;
                tv.t2 = md.ijthdrev / tv.drainsatcurrent;
                if tv.t2 < 1.0 {
                    tv.t2 = 10.0;
                    notes.push(
                        "Warning: ijthdrev too small and set to 10 times IdbSat.".to_string(),
                    );
                }
                h.vjdm_rev = -md.bvd - tv.nvtmd * log((tv.t2 - 1.0) / md.xjbvd);
                tv.t1 = md.xjbvd * exp(-(md.bvd + h.vjdm_rev) / tv.nvtmd);
                h.ivjdm_rev = tv.drainsatcurrent * (1.0 + tv.t1);
                h.dslp_rev = -tv.drainsatcurrent * tv.t1 / tv.nvtmd;
            }
            _ => {
                notes.push(format!("Specified dioMod = {} not matched", md.dio_mod));
            }
        }
    }
    tv.t0 = tv.tratio - 1.0;
    md.njtsstemp = md.njts * (1.0 + md.tnjts * tv.t0);
    md.njtsswstemp = md.njtssw * (1.0 + md.tnjtssw * tv.t0);
    md.njtsswgstemp = md.njtsswg * (1.0 + md.tnjtsswg * tv.t0);
    md.njtsdtemp = md.njtsd * (1.0 + md.tnjtsd * tv.t0);
    md.njtsswdtemp = md.njtsswd * (1.0 + md.tnjtsswd * tv.t0);
    md.njtsswgdtemp = md.njtsswgd * (1.0 + md.tnjtsswgd * tv.t0);
    tv.t7 = tv.eg0 / md.vtm * tv.t0;
    tv.t9 = md.xtss * tv.t7;
    tv.t1 = dexp2(tv.t9);
    tv.t9 = md.xtsd * tv.t7;
    tv.t2 = dexp2(tv.t9);
    tv.t9 = md.xtssws * tv.t7;
    tv.t3 = dexp2(tv.t9);
    tv.t9 = md.xtsswd * tv.t7;
    tv.t4 = dexp2(tv.t9);
    tv.t9 = md.xtsswgs * tv.t7;
    tv.t5 = dexp2(tv.t9);
    tv.t9 = md.xtsswgd * tv.t7;
    tv.t6 = dexp2(tv.t9);
    if md.jtweff < 0.0 {
        md.jtweff = 0.0;
        notes.push(
            "TAT width dependence effect is negative. Jtweff is clamped to zero.".to_string(),
        );
    }
    tv.t11 = sqrt(md.jtweff / pp.weff_cj) + 1.0;
    tv.t10 = pp.weff_cj * h.nf;
    h.sjct_temp_rev_sat_cur = tv.t1 * h.aseff * md.jtss;
    h.djct_temp_rev_sat_cur = tv.t2 * h.adeff * md.jtsd;
    h.ssw_temp_rev_sat_cur = tv.t3 * h.pseff * md.jtssws;
    h.dsw_temp_rev_sat_cur = tv.t4 * h.pdeff * md.jtsswd;
    h.sswg_temp_rev_sat_cur = tv.t5 * tv.t10 * tv.t11 * md.jtsswgs;
    h.dswg_temp_rev_sat_cur = tv.t6 * tv.t10 * tv.t11 * md.jtsswgd;
    if md.mtrl_mod != 0 && md.mtrl_compat_mod == 0 {
        tv.vtm0eot = KBOQ * md.tempeot;
        tv.vtmeot = tv.vtm0eot;
        tv.vbieot = tv.vtm0eot * log(pp.nsd * pp.ndep / (tv.ni * tv.ni));
        tv.phieot = tv.vtm0eot * log(pp.ndep / tv.ni) + pp.phin + 0.4;
        if tv.phieot <= 0.0 {
            notes.push(format!(
                "Fatal: phieot = {} is not positive. Please Check Phin and Ndep",
                pp.phi
            ));
            notes.push(format!("phieot = {} Ndep = {}", pp.phin, pp.ndep));
            tv.fatal_flag = 1;
        }
        tv.tmp2 = h.vfb + tv.phieot;
        tv.vddeot = md.type_ as f64 * md.vddeot;
        tv.t0 = md.epsrgate * EPS0;
        if pp.ngate > 1.0e18 && pp.ngate < 1.0e25 && tv.vddeot > tv.tmp2 && tv.t0 != 0.0 {
            tv.t1 = 1.0e6 * CHARGE * tv.t0 * pp.ngate / (md.coxe * md.coxe);
            tv.t8 = tv.vddeot - tv.tmp2;
            tv.t4 = sqrt(1.0 + 2.0 * tv.t8 / tv.t1);
            tv.t2 = 2.0 * tv.t8 / (tv.t4 + 1.0);
            tv.t3 = 0.5 * tv.t2 * tv.t2 / tv.t1;
            tv.t7 = 1.12 - tv.t3 - 0.05;
            tv.t6 = sqrt(tv.t7 * tv.t7 + 0.224);
            tv.t5 = 1.12 - 0.5 * (tv.t7 + tv.t6);
            tv.vgs_eff = tv.vddeot - tv.t5;
        } else {
            tv.vgs_eff = tv.vddeot;
        }
        tv.v0 = tv.vbieot - tv.phieot;
        tv.lt1 = md.factor1 * pp.sqrt_xdep0;
        tv.ltw = tv.lt1;
        tv.t0 = pp.dvt1 * md.leffeot / tv.lt1;
        if tv.t0 < EXP_THRESHOLD {
            tv.t1 = exp(tv.t0);
            tv.t2 = tv.t1 - 1.0;
            tv.t3 = tv.t2 * tv.t2;
            tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
            tv.theta0 = tv.t1 / tv.t4;
        } else {
            tv.theta0 = 1.0 / (MAX_EXP - 2.0);
        }
        tv.delt_vth = pp.dvt0 * tv.theta0 * tv.v0;
        tv.t0 = pp.dvt1w * md.weffeot * md.leffeot / tv.ltw;
        if tv.t0 < EXP_THRESHOLD {
            tv.t1 = exp(tv.t0);
            tv.t2 = tv.t1 - 1.0;
            tv.t3 = tv.t2 * tv.t2;
            tv.t4 = tv.t3 + 2.0 * tv.t1 * MIN_EXP;
            tv.t5 = tv.t1 / tv.t4;
        } else {
            tv.t5 = 1.0 / (MAX_EXP - 2.0);
        }
        tv.t2 = pp.dvt0w * tv.t5 * tv.v0;
        tv.tempratioeot = md.tempeot / md.tnom - 1.0;
        tv.t0 = sqrt(1.0 + pp.lpe0 / md.leffeot);
        tv.t1 = pp.k1ox * (tv.t0 - 1.0) * sqrt(tv.phieot)
            + (pp.kt1 + pp.kt1l / md.leffeot) * tv.tempratioeot;
        tv.vth_narroww = tv.toxe * tv.phieot / (md.weffeot + pp.w0);
        tv.lpe_vb = sqrt(1.0 + pp.lpeb / md.leffeot);
        tv.vth = md.type_ as f64 * h.vth0 + (pp.k1ox - pp.k1) * sqrt(tv.phieot) * tv.lpe_vb
            - tv.delt_vth
            - tv.t2
            + pp.k3 * tv.vth_narroww
            + tv.t1;
        tv.tmp1 = tv.epssub / pp.xdep0;
        tv.tmp2 = pp.nfactor * tv.tmp1;
        tv.tmp3 = (tv.tmp2 + pp.cdsc * tv.theta0 + pp.cit) / md.coxe;
        if tv.tmp3 >= -0.5 {
            tv.n = 1.0 + tv.tmp3;
        } else {
            tv.t0 = 1.0 / (3.0 + 8.0 * tv.tmp3);
            tv.n = (1.0 + 3.0 * tv.tmp3) * tv.t0;
        }
        if pp.dvtp0 > 0.0 {
            tv.t3 = md.leffeot + pp.dvtp0 * 2.0;
            if md.temp_mod < 2 {
                tv.t4 = tv.vtmeot * log(md.leffeot / tv.t3);
            } else {
                tv.t4 = tv.vtm0eot * log(md.leffeot / tv.t3);
            }
            tv.vth -= tv.n * tv.t4;
        }
        tv.vgsteff = tv.vgs_eff - tv.vth;
        tv.t3 = md.type_ as f64 * h.vth0 - h.vfb - tv.phieot;
        tv.t4 = tv.t3 + tv.t3;
        tv.t5 = 2.5 * tv.t3;
        tv.vtfbphi2eot = 4.0 * tv.t3;
        if tv.vtfbphi2eot < 0.0 {
            tv.vtfbphi2eot = 0.0;
        }
        tv.niter = 0.0;
        tv.toxpf = tv.toxe;
        loop {
            tv.toxpi = tv.toxpf;
            tv.tmp2 = 2.0e8 * tv.toxpf;
            tv.t0 = (tv.vgsteff + tv.vtfbphi2eot) / tv.tmp2;
            tv.t1 = 1.0 + exp(md.bdos * 0.7 * log(tv.t0));
            tv.tcen = md.ados * 1.9e-9 / tv.t1;
            tv.toxpf = tv.toxe - tv.epsrox / md.epsrsub * tv.tcen;
            tv.niter += 1.0;
            if !(tv.niter <= 4.0 && fabs(tv.toxpf - tv.toxpi) > 1e-12) {
                break;
            }
        }
        h.toxp = tv.toxpf;
        h.coxp = tv.epsrox * EPS0 / h.toxp;
    } else {
        h.toxp = md.toxp;
        h.coxp = md.coxp;
    }
}

/// `BSIM4DioIjthVjmEval` (`b4temp.c`).
#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn dio_ijth_vjm_eval(nvtm: f64, ijth: f64, isb: f64, xexpbv: f64, vjm: &mut f64) -> i32 {
    let mut evjmovnv: f64 = 0.0;
    let mut tb: f64 = 0.0;
    let mut tc: f64 = 0.0;
    tc = xexpbv;
    tb = 1.0 + ijth / isb - tc;
    evjmovnv = 0.5 * (tb + sqrt(tb * tb + 4.0 * tc));
    *vjm = nvtm * log(evjmovnv);
    0
}
