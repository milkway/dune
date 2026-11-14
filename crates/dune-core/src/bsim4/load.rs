// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4ld.c as distributed with ngspice-47
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

//! `BSIM4load` (`b4ld.c`), statement by statement: limiting, junction
//! diodes, the I-V and C-V models, gate tunnelling, GIDL/GISL, the
//! bias-dependent S/D and gate resistances, the body network, NQS, and
//! the stamps. The C's `goto finished` / `line850` / `line860` /
//! `line900` / `line1000` structure is expressed with the flags
//! `goto_finished` and `path`; the bypass (`NOBYPASS`) and the OpenMP
//! path are not ported.

use super::*;

/// The body of `BSIM4load` for one instance.
#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::too_many_arguments,
    clippy::cognitive_complexity
)]
pub(super) fn load(
    md: &Bsim4Model,
    pp: &Bsim4Size,
    h: &mut Bsim4Here,
    st: usize,
    ck_mode: i32,
    ck_temp: f64,
    charge_computation_needed: i32,
    ctx: &mut LoadCtx,
) {
    let mut a1: f64 = 0.0;
    let mut abulk: f64 = 0.0;
    let mut abulk0: f64 = 0.0;
    let mut abulk0_q: f64 = 0.0;
    let mut abulkcv: f64 = 0.0;
    let mut ag0: f64 = 0.0;
    let mut alphaz: f64 = 0.0;
    let mut arg: f64 = 0.0;
    let mut arg1: f64 = 0.0;
    let mut beta: f64 = 0.0;
    let mut _bypass: i32 = 0;
    let mut cbb: f64 = 0.0;
    let mut cbb1: f64 = 0.0;
    let mut cbd: f64 = 0.0;
    let mut cbd1: f64 = 0.0;
    let mut cbg: f64 = 0.0;
    let mut cbg1: f64 = 0.0;
    let mut _cbhat: f64 = 0.0;
    let mut ccen: f64 = 0.0;
    let mut cclm: f64 = 0.0;
    let mut cdd: f64 = 0.0;
    let mut _cdedhat: f64 = 0.0;
    let mut cdg: f64 = 0.0;
    let mut _cdhat: f64 = 0.0;
    let mut cdrain: f64 = 0.0;
    let mut cds: f64 = 0.0;
    let mut ceqbd: f64 = 0.0;
    let mut ceqbs: f64 = 0.0;
    let mut ceqdrn: f64 = 0.0;
    let mut ceqgcrg: f64 = 0.0;
    let mut ceqgdtot: f64 = 0.0;
    let mut ceqgstot: f64 = 0.0;
    let mut ceqjd: f64 = 0.0;
    let mut ceqjs: f64 = 0.0;
    let mut ceqqb: f64 = 0.0;
    let mut ceqqd: f64 = 0.0;
    let mut ceqqg: f64 = 0.0;
    let mut ceqqgmid: f64 = 0.0;
    let mut ceqqjd: f64 = 0.0;
    let mut ceqqjs: f64 = 0.0;
    let mut cgb: f64 = 0.0;
    let mut cgb1: f64 = 0.0;
    let mut _cgbhat: f64 = 0.0;
    let mut cgd: f64 = 0.0;
    let mut cgd1: f64 = 0.0;
    let mut _cgdhat: f64 = 0.0;
    let mut cgdo: f64 = 0.0;
    let mut cgg: f64 = 0.0;
    let mut cgg1: f64 = 0.0;
    let mut _cgshat: f64 = 0.0;
    let mut cgso: f64 = 0.0;
    let mut check: i32 = 0;
    let mut check1: i32 = 0;
    let mut check2: i32 = 0;
    let mut cox: f64 = 0.0;
    let mut coxeff: f64 = 0.0;
    let mut coxeffwovl: f64 = 0.0;
    let mut coxwl: f64 = 0.0;
    let mut coxwlcen: f64 = 0.0;
    let mut cqbody: f64 = 0.0;
    let mut cqcheq: f64 = 0.0;
    let mut cqdef: f64 = 0.0;
    let mut cqdrn: f64 = 0.0;
    let mut cqgate: f64 = 0.0;
    let mut csb: f64 = 0.0;
    let mut csd: f64 = 0.0;
    let mut _cseshat: f64 = 0.0;
    let mut csg: f64 = 0.0;
    let mut css: f64 = 0.0;
    let mut czbd: f64 = 0.0;
    let mut czbdsw: f64 = 0.0;
    let mut czbdswg: f64 = 0.0;
    let mut czbs: f64 = 0.0;
    let mut czbssw: f64 = 0.0;
    let mut czbsswg: f64 = 0.0;
    let mut dabulk0_dvb: f64 = 0.0;
    let mut dabulk0_q_dvb: f64 = 0.0;
    let mut dabulk_dvb: f64 = 0.0;
    let mut dabulk_dvg: f64 = 0.0;
    let mut dabulkcv_dvb: f64 = 0.0;
    let mut dalphaz_dvb: f64 = 0.0;
    let mut dalphaz_dvg: f64 = 0.0;
    let mut dbeta_dvb: f64 = 0.0;
    let mut dbeta_dvd: f64 = 0.0;
    let mut dbeta_dvg: f64 = 0.0;
    let mut dcclm_dvb: f64 = 0.0;
    let mut dcclm_dvd: f64 = 0.0;
    let mut dcclm_dvg: f64 = 0.0;
    let mut dcoxeff_dvb: f64 = 0.0;
    let mut dcoxeff_dvd: f64 = 0.0;
    let mut dcoxeff_dvg: f64 = 0.0;
    let mut ddelt_vth_dvb: f64 = 0.0;
    let mut ddeltaphi_dvg: f64 = 0.0;
    let mut ddenomi_dvb: f64 = 0.0;
    let mut ddenomi_dvd: f64 = 0.0;
    let mut ddenomi_dvg: f64 = 0.0;
    let mut ddibl_sft_dvd: f64 = 0.0;
    let mut ddits_sft2_dvd: f64 = 0.0;
    let mut ddits_sft_dvb: f64 = 0.0;
    let mut ddits_sft_dvd: f64 = 0.0;
    let mut ddxpart_dvb: f64 = 0.0;
    let mut ddxpart_dvd: f64 = 0.0;
    let mut ddxpart_dvg: f64 = 0.0;
    let mut ddxpart_dvs: f64 = 0.0;
    let mut delt_vth: f64 = 0.0;
    let mut deltaphi: f64 = 0.0;
    let mut delvbd: f64 = 0.0;
    let mut delvbd_jct: f64 = 0.0;
    let mut delvbs: f64 = 0.0;
    let mut delvbs_jct: f64 = 0.0;
    let mut delvdbd: f64 = 0.0;
    let mut _delvdbs: f64 = 0.0;
    let mut delvded: f64 = 0.0;
    let mut _delvdes: f64 = 0.0;
    let mut delvds: f64 = 0.0;
    let mut delvgd: f64 = 0.0;
    let mut _delvged: f64 = 0.0;
    let mut _delvges: f64 = 0.0;
    let mut _delvgmd: f64 = 0.0;
    let mut _delvgms: f64 = 0.0;
    let mut delvgs: f64 = 0.0;
    let mut delvsbs: f64 = 0.0;
    let mut delvses: f64 = 0.0;
    let mut denomi: f64 = 0.0;
    let mut desatl_dvb: f64 = 0.0;
    let mut desatl_dvd: f64 = 0.0;
    let mut desatl_dvg: f64 = 0.0;
    let mut devbd_dvb: f64 = 0.0;
    let mut devbs_dvb: f64 = 0.0;
    let mut dfgche1_dvb: f64 = 0.0;
    let mut dfgche1_dvd: f64 = 0.0;
    let mut dfgche1_dvg: f64 = 0.0;
    let mut dfgche2_dvb: f64 = 0.0;
    let mut dfgche2_dvd: f64 = 0.0;
    let mut dfgche2_dvg: f64 = 0.0;
    let mut dfp_dvg: f64 = 0.0;
    let mut dfsevl_dvb: f64 = 0.0;
    let mut dfsevl_dvd: f64 = 0.0;
    let mut dfsevl_dvg: f64 = 0.0;
    let mut dgche_dvb: f64 = 0.0;
    let mut dgche_dvd: f64 = 0.0;
    let mut dgche_dvg: f64 = 0.0;
    let mut dgdtot_dvb: f64 = 0.0;
    let mut dgdtot_dvd: f64 = 0.0;
    let mut dgdtot_dvg: f64 = 0.0;
    let mut dgdtot_dvs: f64 = 0.0;
    let mut dgstot_dvb: f64 = 0.0;
    let mut dgstot_dvd: f64 = 0.0;
    let mut dgstot_dvg: f64 = 0.0;
    let mut dgstot_dvs: f64 = 0.0;
    let mut dibl_sft: f64 = 0.0;
    let mut didl_dvb: f64 = 0.0;
    let mut didl_dvd: f64 = 0.0;
    let mut didl_dvg: f64 = 0.0;
    let mut didsa_dvb: f64 = 0.0;
    let mut didsa_dvd: f64 = 0.0;
    let mut didsa_dvg: f64 = 0.0;
    let mut diffvds: f64 = 0.0;
    let mut digbacc_dvb: f64 = 0.0;
    let mut digbacc_dvg: f64 = 0.0;
    let mut digbinv_dvb: f64 = 0.0;
    let mut digbinv_dvd: f64 = 0.0;
    let mut digbinv_dvg: f64 = 0.0;
    let mut digc_dvb: f64 = 0.0;
    let mut digc_dvd: f64 = 0.0;
    let mut digc_dvg: f64 = 0.0;
    let mut digcd_dvb: f64 = 0.0;
    let mut digcd_dvd: f64 = 0.0;
    let mut digcd_dvg: f64 = 0.0;
    let mut digcs_dvb: f64 = 0.0;
    let mut digcs_dvd: f64 = 0.0;
    let mut digcs_dvg: f64 = 0.0;
    let mut digd_dvd: f64 = 0.0;
    let mut digd_dvg: f64 = 0.0;
    let mut digs_dvg: f64 = 0.0;
    let mut digs_dvs: f64 = 0.0;
    let mut dits_sft2: f64 = 0.0;
    let mut dlambda_dvg: f64 = 0.0;
    let mut dlt1_dvb: f64 = 0.0;
    let mut dltw_dvb: f64 = 0.0;
    let mut dn_dvb: f64 = 0.0;
    let mut dn_dvd: f64 = 0.0;
    let mut dnoff_dvb: f64 = 0.0;
    let mut dnoff_dvd: f64 = 0.0;
    let mut dphis_dvb: f64 = 0.0;
    let mut dpigcd_dvb: f64 = 0.0;
    let mut dpigcd_dvd: f64 = 0.0;
    let mut dpigcd_dvg: f64 = 0.0;
    let mut dpvagterm_dvb: f64 = 0.0;
    let mut dpvagterm_dvd: f64 = 0.0;
    let mut dpvagterm_dvg: f64 = 0.0;
    let mut dqac0_dvb: f64 = 0.0;
    let mut dqac0_dvg: f64 = 0.0;
    let mut dqsub0_dvb: f64 = 0.0;
    let mut dqsub0_dvd: f64 = 0.0;
    let mut dqsub0_dvg: f64 = 0.0;
    let mut drainsatcurrent: f64 = 0.0;
    let mut drd_dvb: f64 = 0.0;
    let mut drd_dvg: f64 = 0.0;
    let mut drds_dvb: f64 = 0.0;
    let mut drds_dvg: f64 = 0.0;
    let mut drs_dvb: f64 = 0.0;
    let mut drs_dvg: f64 = 0.0;
    let mut dsqrtphis_dvb: f64 = 0.0;
    let mut dsxpart_dvb: f64 = 0.0;
    let mut dsxpart_dvd: f64 = 0.0;
    let mut dsxpart_dvg: f64 = 0.0;
    let mut dsxpart_dvs: f64 = 0.0;
    let mut dt0_dvb: f64 = 0.0;
    let mut dt0_dvd: f64 = 0.0;
    let mut dt0_dvg: f64 = 0.0;
    let mut dt0_dvg_: f64 = 0.0;
    let mut dt10_dvb: f64 = 0.0;
    let mut dt10_dvd: f64 = 0.0;
    let mut dt10_dvg: f64 = 0.0;
    let mut dt11_dvg: f64 = 0.0;
    let mut dt1_dvb: f64 = 0.0;
    let mut dt1_dvb_: f64 = 0.0;
    let mut dt1_dvd: f64 = 0.0;
    let mut dt1_dvg: f64 = 0.0;
    let mut dt2_dvb: f64 = 0.0;
    let mut dt2_dvd: f64 = 0.0;
    let mut dt2_dvg: f64 = 0.0;
    let mut dt3_dvb: f64 = 0.0;
    let mut dt3_dvb_: f64 = 0.0;
    let mut dt3_dvd: f64 = 0.0;
    let mut dt3_dvg: f64 = 0.0;
    let mut dt3_dvg_: f64 = 0.0;
    let mut dt4_dvb: f64 = 0.0;
    let mut dt4_dvd: f64 = 0.0;
    let mut dt5_dvb: f64 = 0.0;
    let mut dt5_dvd: f64 = 0.0;
    let mut dt5_dvg: f64 = 0.0;
    let mut dt6_dvb: f64 = 0.0;
    let mut dt6_dvd: f64 = 0.0;
    let mut dt6_dvg: f64 = 0.0;
    let mut dt7_dvb: f64 = 0.0;
    let mut dt7_dvd: f64 = 0.0;
    let mut dt7_dvg: f64 = 0.0;
    let mut dt8_dvb: f64 = 0.0;
    let mut dt8_dvd: f64 = 0.0;
    let mut dt8_dvg: f64 = 0.0;
    let mut dt9_dvb: f64 = 0.0;
    let mut dt9_dvd: f64 = 0.0;
    let mut dt9_dvg: f64 = 0.0;
    let mut dtcen_dvb: f64 = 0.0;
    let mut dtcen_dvd: f64 = 0.0;
    let mut dtcen_dvg: f64 = 0.0;
    let mut dtheta0_dvb: f64 = 0.0;
    let mut dueff_dvb: f64 = 0.0;
    let mut dueff_dvd: f64 = 0.0;
    let mut dueff_dvg: f64 = 0.0;
    let mut dva_dvb: f64 = 0.0;
    let mut dva_dvd: f64 = 0.0;
    let mut dva_dvg: f64 = 0.0;
    let mut dvaclm_dvb: f64 = 0.0;
    let mut dvaclm_dvd: f64 = 0.0;
    let mut dvaclm_dvg: f64 = 0.0;
    let mut dvadibl_dvb: f64 = 0.0;
    let mut dvadibl_dvd: f64 = 0.0;
    let mut dvadibl_dvg: f64 = 0.0;
    let mut dvadits_dvd: f64 = 0.0;
    let mut dvadits_dvg: f64 = 0.0;
    let mut dvasat_dvb: f64 = 0.0;
    let mut dvasat_dvd: f64 = 0.0;
    let mut dvasat_dvg: f64 = 0.0;
    let mut dvascbe_dvb: f64 = 0.0;
    let mut dvascbe_dvd: f64 = 0.0;
    let mut dvascbe_dvg: f64 = 0.0;
    let mut dvaux_dvb: f64 = 0.0;
    let mut dvaux_dvd: f64 = 0.0;
    let mut dvaux_dvg: f64 = 0.0;
    let mut dvbseff_dvb: f64 = 0.0;
    let mut dvbseffcv_dvb: f64 = 0.0;
    let mut dvdsat_dvb: f64 = 0.0;
    let mut dvdsat_dvd: f64 = 0.0;
    let mut dvdsat_dvg: f64 = 0.0;
    let mut dvdseff_dvb: f64 = 0.0;
    let mut dvdseff_dvd: f64 = 0.0;
    let mut dvdseff_dvg: f64 = 0.0;
    let mut dvdseffcv_dvb: f64 = 0.0;
    let mut dvdseffcv_dvd: f64 = 0.0;
    let mut dvdseffcv_dvg: f64 = 0.0;
    let mut dvfbeff_dvb: f64 = 0.0;
    let mut dvfbeff_dvg: f64 = 0.0;
    let mut dvgd_eff_dvg: f64 = 0.0;
    let mut dvgdp_dvg: f64 = 0.0;
    let mut dvgs_eff_dvg: f64 = 0.0;
    let mut dvgs_eff_dvg_: f64 = 0.0;
    let mut _dvgst_dvb: f64 = 0.0;
    let mut _dvgst_dvg: f64 = 0.0;
    let mut dvgsteff_dvb: f64 = 0.0;
    let mut dvgsteff_dvd: f64 = 0.0;
    let mut dvgsteff_dvg: f64 = 0.0;
    let mut dvoxacc_dvb: f64 = 0.0;
    let mut dvoxacc_dvg: f64 = 0.0;
    let mut dvoxdepinv_dvb: f64 = 0.0;
    let mut dvoxdepinv_dvd: f64 = 0.0;
    let mut dvoxdepinv_dvg: f64 = 0.0;
    let mut dvs_dvb: f64 = 0.0;
    let mut dvs_dvd: f64 = 0.0;
    let mut dvs_dvg: f64 = 0.0;
    let mut dvth_dvb: f64 = 0.0;
    let mut dvth_dvd: f64 = 0.0;
    let mut dweff_dvb: f64 = 0.0;
    let mut dweff_dvg: f64 = 0.0;
    let mut dxdep_dvb: f64 = 0.0;
    let mut dxpart: f64 = 0.0;
    let mut epsrox: f64 = 0.0;
    let mut epssub: f64 = 0.0;
    let mut esat: f64 = 0.0;
    let mut esatl: f64 = 0.0;
    let mut evbd: f64 = 0.0;
    let mut evbs: f64 = 0.0;
    let mut expvgst: f64 = 0.0;
    let mut expvxnvt: f64 = 0.0;
    let mut fgche1: f64 = 0.0;
    let mut fgche2: f64 = 0.0;
    let mut fp: f64 = 0.0;
    let mut fsevl: f64 = 0.0;
    let mut fwdsum: f64 = 0.0;
    let mut gbb: f64 = 0.0;
    let mut gbbdp: f64 = 0.0;
    let mut gbbsp: f64 = 0.0;
    let mut gbd: f64 = 0.0;
    let mut gbdpb: f64 = 0.0;
    let mut gbdpdp: f64 = 0.0;
    let mut gbdpg: f64 = 0.0;
    let mut gbdpsp: f64 = 0.0;
    let mut gbg: f64 = 0.0;
    let mut gbspb: f64 = 0.0;
    let mut gbspdp: f64 = 0.0;
    let mut gbspg: f64 = 0.0;
    let mut gbspsp: f64 = 0.0;
    let mut gcbbb: f64 = 0.0;
    let mut gcbdb: f64 = 0.0;
    let mut gcbgb: f64 = 0.0;
    let mut gcbgmb: f64 = 0.0;
    let mut gcbsb: f64 = 0.0;
    let mut gcdbb: f64 = 0.0;
    let mut gcdbdb: f64 = 0.0;
    let mut gcddb: f64 = 0.0;
    let mut gcdgb: f64 = 0.0;
    let mut gcdgmb: f64 = 0.0;
    let mut gcdsb: f64 = 0.0;
    let mut gcgbb: f64 = 0.0;
    let mut gcgdb: f64 = 0.0;
    let mut gcggb: f64 = 0.0;
    let mut gcgmbb: f64 = 0.0;
    let mut gcgmdb: f64 = 0.0;
    let mut gcgmgmb: f64 = 0.0;
    let mut gcgmsb: f64 = 0.0;
    let mut gcgsb: f64 = 0.0;
    let mut gche: f64 = 0.0;
    let mut gcqbb: f64 = 0.0;
    let mut gcqdb: f64 = 0.0;
    let mut gcqgb: f64 = 0.0;
    let mut gcqsb: f64 = 0.0;
    let mut gcrg: f64 = 0.0;
    let mut gcrgb: f64 = 0.0;
    let mut gcrgd: f64 = 0.0;
    let mut gcrgg: f64 = 0.0;
    let mut gcrgs: f64 = 0.0;
    let mut gcsbb: f64 = 0.0;
    let mut gcsbsb: f64 = 0.0;
    let mut gcsdb: f64 = 0.0;
    let mut gcsgb: f64 = 0.0;
    let mut gcsgmb: f64 = 0.0;
    let mut gcssb: f64 = 0.0;
    let mut gdpr: f64 = 0.0;
    let mut gds: f64 = 0.0;
    let mut gdtot: f64 = 0.0;
    let mut gdtotb: f64 = 0.0;
    let mut gdtotd: f64 = 0.0;
    let mut gdtotg: f64 = 0.0;
    let mut gdtots: f64 = 0.0;
    let mut geltd: f64 = 0.0;
    let mut ggidlb: f64 = 0.0;
    let mut ggidlb_: f64 = 0.0;
    let mut ggidld: f64 = 0.0;
    let mut ggidld_: f64 = 0.0;
    let mut ggidlg: f64 = 0.0;
    let mut ggidlg_: f64 = 0.0;
    let mut ggislb: f64 = 0.0;
    let mut ggislb_: f64 = 0.0;
    let mut ggislg: f64 = 0.0;
    let mut ggislg_: f64 = 0.0;
    let mut ggisls: f64 = 0.0;
    let mut ggisls_: f64 = 0.0;
    let mut ggtb: f64 = 0.0;
    let mut ggtd: f64 = 0.0;
    let mut ggtg: f64 = 0.0;
    let mut ggts: f64 = 0.0;
    let mut gibtotb: f64 = 0.0;
    let mut gibtotd: f64 = 0.0;
    let mut gibtotg: f64 = 0.0;
    let mut gibtots: f64 = 0.0;
    let mut gidtotb: f64 = 0.0;
    let mut gidtotd: f64 = 0.0;
    let mut gidtotg: f64 = 0.0;
    let mut gidtots: f64 = 0.0;
    let mut gigtotb: f64 = 0.0;
    let mut gigtotd: f64 = 0.0;
    let mut gigtotg: f64 = 0.0;
    let mut gigtots: f64 = 0.0;
    let mut gistotb: f64 = 0.0;
    let mut gistotd: f64 = 0.0;
    let mut gistotg: f64 = 0.0;
    let mut gistots: f64 = 0.0;
    let mut gjbd: f64 = 0.0;
    let mut gjbs: f64 = 0.0;
    let mut gm: f64 = 0.0;
    let mut gmb: f64 = 0.0;
    let mut gmbs: f64 = 0.0;
    let mut goto_finished: i32 = 0;
    let mut gqdef: f64 = 0.0;
    let mut gspr: f64 = 0.0;
    let mut gstot: f64 = 0.0;
    let mut gstotb: f64 = 0.0;
    let mut gstotd: f64 = 0.0;
    let mut gstotg: f64 = 0.0;
    let mut gstots: f64 = 0.0;
    let mut ibtot: f64 = 0.0;
    let mut ibtoteq: f64 = 0.0;
    let mut idedtot: f64 = 0.0;
    let mut idl: f64 = 0.0;
    let mut ids: f64 = 0.0;
    let mut idsa: f64 = 0.0;
    let mut idtot: f64 = 0.0;
    let mut idtoteq: f64 = 0.0;
    let mut igbacc: f64 = 0.0;
    let mut igbinv: f64 = 0.0;
    let mut _igbtot: f64 = 0.0;
    let mut igc: f64 = 0.0;
    let mut igcd: f64 = 0.0;
    let mut igcs: f64 = 0.0;
    let mut igd: f64 = 0.0;
    let mut igdtot: f64 = 0.0;
    let mut igidl: f64 = 0.0;
    let mut igisl: f64 = 0.0;
    let mut igs: f64 = 0.0;
    let mut igstot: f64 = 0.0;
    let mut igtoteq: f64 = 0.0;
    let mut isestot: f64 = 0.0;
    let mut istoteq: f64 = 0.0;
    let mut isub: f64 = 0.0;
    let mut lambda: f64 = 0.0;
    let mut leff: f64 = 0.0;
    let mut link: f64 = 0.0;
    let mut lpe_vb: f64 = 0.0;
    let mut lt1: f64 = 0.0;
    let mut ltw: f64 = 0.0;
    let mut m: f64 = 0.0;
    let mut mjd: f64 = 0.0;
    let mut mjs: f64 = 0.0;
    let mut mjswd: f64 = 0.0;
    let mut mjswgd: f64 = 0.0;
    let mut mjswgs: f64 = 0.0;
    let mut mjsws: f64 = 0.0;
    let mut mult_i: f64 = 0.0;
    let mut mult_q: f64 = 0.0;
    let mut n: f64 = 0.0;
    let mut noff: f64 = 0.0;
    let mut nvtmd: f64 = 0.0;
    let mut nvtmrsd: f64 = 0.0;
    let mut nvtmrss: f64 = 0.0;
    let mut nvtmrsswd: f64 = 0.0;
    let mut nvtmrsswgd: f64 = 0.0;
    let mut nvtmrsswgs: f64 = 0.0;
    let mut nvtmrssws: f64 = 0.0;
    let mut nvtms: f64 = 0.0;
    let mut one_third_coxwl: f64 = 0.0;
    let mut path: i32 = 0;
    let mut phis: f64 = 0.0;
    let mut pigcd: f64 = 0.0;
    let mut pvagterm: f64 = 0.0;
    let mut qac0: f64 = 0.0;
    let mut qbulk: f64 = 0.0;
    let mut qcheq: f64 = 0.0;
    let mut qdef: f64 = 0.0;
    let mut qdrn: f64 = 0.0;
    let mut qgate: f64 = 0.0;
    let mut qgb: f64 = 0.0;
    let mut qgdo: f64 = 0.0;
    let mut qgmb: f64 = 0.0;
    let mut qgmid: f64 = 0.0;
    let mut qgso: f64 = 0.0;
    let mut qovcox: f64 = 0.0;
    let mut qsrc: f64 = 0.0;
    let mut qsub0: f64 = 0.0;
    let mut rd: f64 = 0.0;
    let mut rds: f64 = 0.0;
    let mut revsum: f64 = 0.0;
    let mut rs: f64 = 0.0;
    let mut sarg: f64 = 0.0;
    let mut sourcesatcurrent: f64 = 0.0;
    let mut sqrtphis: f64 = 0.0;
    let mut sxpart: f64 = 0.0;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 0.0;
    let mut t10: f64 = 0.0;
    let mut t11: f64 = 0.0;
    let mut t12: f64 = 0.0;
    let mut t13: f64 = 0.0;
    let mut t14: f64 = 0.0;
    let mut t2: f64 = 0.0;
    let mut t3: f64 = 0.0;
    let mut t4: f64 = 0.0;
    let mut t5: f64 = 0.0;
    let mut t6: f64 = 0.0;
    let mut t7: f64 = 0.0;
    let mut t8: f64 = 0.0;
    let mut t9: f64 = 0.0;
    let mut tcen: f64 = 0.0;
    let mut tempratio: f64 = 0.0;
    let mut theta0: f64 = 0.0;
    let mut tmp: f64 = 0.0;
    let mut tmp1: f64 = 0.0;
    let mut tmp2: f64 = 0.0;
    let mut tmp3: f64 = 0.0;
    let mut tmp4: f64 = 0.0;
    let mut tox: f64 = 0.0;
    let mut toxe: f64 = 0.0;
    let mut two_third_coxwl: f64 = 0.0;
    let mut ueff: f64 = 0.0;
    let mut v0: f64 = 0.0;
    let mut v3: f64 = 0.0;
    let mut v4: f64 = 0.0;
    let mut va: f64 = 0.0;
    let mut vaclm: f64 = 0.0;
    let mut vadibl: f64 = 0.0;
    let mut vadits: f64 = 0.0;
    let mut vasat: f64 = 0.0;
    let mut vascbe: f64 = 0.0;
    let mut vaux: f64 = 0.0;
    let mut vbd: f64 = 0.0;
    let mut vbd_jct: f64 = 0.0;
    let mut vbs: f64 = 0.0;
    let mut vbs_: f64 = 0.0;
    let mut vbs_jct: f64 = 0.0;
    let mut vbseff: f64 = 0.0;
    let mut vbseffcv: f64 = 0.0;
    let mut _vdb: f64 = 0.0;
    let mut vdbd: f64 = 0.0;
    let mut vdbs: f64 = 0.0;
    let mut vdedo: f64 = 0.0;
    let mut vdes: f64 = 0.0;
    let mut vds: f64 = 0.0;
    let mut vds_: f64 = 0.0;
    let mut vdsat: f64 = 0.0;
    let mut vdsatcv: f64 = 0.0;
    let mut vdseff: f64 = 0.0;
    let mut vdseffcv: f64 = 0.0;
    let mut vfb: f64 = 0.0;
    let mut vfbeff: f64 = 0.0;
    let mut vgb: f64 = 0.0;
    let mut vgd: f64 = 0.0;
    let mut vgd_eff: f64 = 0.0;
    let mut vgdo: f64 = 0.0;
    let mut vgdp: f64 = 0.0;
    let mut vgdx: f64 = 0.0;
    let mut vged: f64 = 0.0;
    let mut vgedo: f64 = 0.0;
    let mut vges: f64 = 0.0;
    let mut vgmb: f64 = 0.0;
    let mut vgmd: f64 = 0.0;
    let mut vgmdo: f64 = 0.0;
    let mut vgms: f64 = 0.0;
    let mut vgs: f64 = 0.0;
    let mut vgs_: f64 = 0.0;
    let mut vgs_eff: f64 = 0.0;
    let mut vgs_eff_: f64 = 0.0;
    let mut vgst: f64 = 0.0;
    let mut vgst2vtm: f64 = 0.0;
    let mut vgsteff: f64 = 0.0;
    let mut vgsteffvth: f64 = 0.0;
    let mut vgstnvt: f64 = 0.0;
    let mut vgsx: f64 = 0.0;
    let mut voffcv: f64 = 0.0;
    let mut von: f64 = 0.0;
    let mut voxacc: f64 = 0.0;
    let mut voxdepinv: f64 = 0.0;
    let mut vs: f64 = 0.0;
    let mut vsbd: f64 = 0.0;
    let mut vsbdo: f64 = 0.0;
    let mut vsbs: f64 = 0.0;
    let mut vses: f64 = 0.0;
    let mut vth: f64 = 0.0;
    let mut vth_narroww: f64 = 0.0;
    let mut vtm: f64 = 0.0;
    let mut vtm0: f64 = 0.0;
    let mut vxnvt: f64 = 0.0;
    let mut weff: f64 = 0.0;
    let mut wvcox: f64 = 0.0;
    let mut wvcoxrds: f64 = 0.0;
    let mut xdep: f64 = 0.0;
    let mut xfact: f64 = 0.0;
    gcgmgmb = 0.0;
    gcgmdb = 0.0;
    gcgmsb = 0.0;
    gcgmbb = 0.0;
    qgmid = 0.0;
    ceqqjd = 0.0;
    ceqqjs = 0.0;
    qgate = 0.0;
    qbulk = 0.0;
    qdrn = 0.0;
    voxacc = 0.0;
    dvoxacc_dvg = 0.0;
    dvoxacc_dvb = 0.0;
    voxdepinv = 0.0;
    dvoxdepinv_dvg = 0.0;
    dvoxdepinv_dvd = 0.0;
    dvoxdepinv_dvb = 0.0;
    vxnvt = 0.0;
    vaux = 0.0;
    dvaux_dvg = 0.0;
    dvaux_dvd = 0.0;
    dvaux_dvb = 0.0;
    vfb = 0.0;
    t0 = 0.0;
    gqdef = 0.0;
    cqdef = 0.0;
    cqcheq = 0.0;
    gcqdb = 0.0;
    gcqsb = 0.0;
    gcqgb = 0.0;
    gcqbb = 0.0;
    goto_finished = 0;
    check2 = 1;
    check1 = check2;
    check = check1;
    _bypass = 0;

    if ck_mode & MODEINITSMSIG != 0 {
        vds = ctx.states.get(0, st + ST_VDS);
        vgs = ctx.states.get(0, st + ST_VGS);
        vbs = ctx.states.get(0, st + ST_VBS);
        vges = ctx.states.get(0, st + ST_VGES);
        vgms = ctx.states.get(0, st + ST_VGMS);
        vdbs = ctx.states.get(0, st + ST_VDBS);
        vsbs = ctx.states.get(0, st + ST_VSBS);
        vses = ctx.states.get(0, st + ST_VSES);
        vdes = ctx.states.get(0, st + ST_VDES);
        qdef = ctx.states.get(0, st + ST_QDEF);
    } else if ck_mode & MODEINITTRAN != 0 {
        vds = ctx.states.get(1, st + ST_VDS);
        vgs = ctx.states.get(1, st + ST_VGS);
        vbs = ctx.states.get(1, st + ST_VBS);
        vges = ctx.states.get(1, st + ST_VGES);
        vgms = ctx.states.get(1, st + ST_VGMS);
        vdbs = ctx.states.get(1, st + ST_VDBS);
        vsbs = ctx.states.get(1, st + ST_VSBS);
        vses = ctx.states.get(1, st + ST_VSES);
        vdes = ctx.states.get(1, st + ST_VDES);
        qdef = ctx.states.get(1, st + ST_QDEF);
    } else if ck_mode & MODEINITJCT != 0 && h.off == 0 {
        vds = md.type_ as f64 * h.ic_vds;
        vgms = md.type_ as f64 * h.ic_vgs;
        vges = vgms;
        vgs = vges;
        vsbs = md.type_ as f64 * h.ic_vbs;
        vdbs = vsbs;
        vbs = vdbs;
        if vds > 0.0 {
            vdes = vds + 0.01;
            vses = -0.01;
        } else if vds < 0.0 {
            vdes = vds - 0.01;
            vses = 0.01;
        } else {
            vses = 0.0;
            vdes = vses;
        }
        qdef = 0.0;
        if vds == 0.0
            && vgs == 0.0
            && vbs == 0.0
            && (ck_mode & (MODETRAN | MODEAC | MODEDCOP | MODEDCTRANCURVE) != 0
                || ck_mode & MODEUIC == 0)
        {
            vds = 0.1;
            vdes = 0.11;
            vses = -0.01;
            vgms = md.type_ as f64 * h.vth0 + 0.1;
            vges = vgms;
            vgs = vges;
            vsbs = 0.0;
            vdbs = vsbs;
            vbs = vdbs;
        }
    } else if ck_mode & (MODEINITJCT | MODEINITFIX) != 0 && h.off != 0 {
        vgms = 0.0;
        vges = vgms;
        vbs = vges;
        vgs = vbs;
        vds = vgs;
        qdef = 0.0;
        vses = qdef;
        vdes = vses;
        vsbs = vdes;
        vdbs = vsbs;
    } else {
        if ck_mode & MODEINITPRED != 0 {
            xfact = ctx.pred_xfact;
            ctx.states
                .set(0, st + ST_VDS, ctx.states.get(1, st + ST_VDS));
            vds = (1.0 + xfact) * ctx.states.get(1, st + ST_VDS)
                - xfact * ctx.states.get(2, st + ST_VDS);
            ctx.states
                .set(0, st + ST_VGS, ctx.states.get(1, st + ST_VGS));
            vgs = (1.0 + xfact) * ctx.states.get(1, st + ST_VGS)
                - xfact * ctx.states.get(2, st + ST_VGS);
            ctx.states
                .set(0, st + ST_VGES, ctx.states.get(1, st + ST_VGES));
            vges = (1.0 + xfact) * ctx.states.get(1, st + ST_VGES)
                - xfact * ctx.states.get(2, st + ST_VGES);
            ctx.states
                .set(0, st + ST_VGMS, ctx.states.get(1, st + ST_VGMS));
            vgms = (1.0 + xfact) * ctx.states.get(1, st + ST_VGMS)
                - xfact * ctx.states.get(2, st + ST_VGMS);
            ctx.states
                .set(0, st + ST_VBS, ctx.states.get(1, st + ST_VBS));
            vbs = (1.0 + xfact) * ctx.states.get(1, st + ST_VBS)
                - xfact * ctx.states.get(2, st + ST_VBS);
            ctx.states.set(
                0,
                st + ST_VBD,
                ctx.states.get(0, st + ST_VBS) - ctx.states.get(0, st + ST_VDS),
            );
            ctx.states
                .set(0, st + ST_VDBS, ctx.states.get(1, st + ST_VDBS));
            vdbs = (1.0 + xfact) * ctx.states.get(1, st + ST_VDBS)
                - xfact * ctx.states.get(2, st + ST_VDBS);
            ctx.states.set(
                0,
                st + ST_VDBD,
                ctx.states.get(0, st + ST_VDBS) - ctx.states.get(0, st + ST_VDS),
            );
            ctx.states
                .set(0, st + ST_VSBS, ctx.states.get(1, st + ST_VSBS));
            vsbs = (1.0 + xfact) * ctx.states.get(1, st + ST_VSBS)
                - xfact * ctx.states.get(2, st + ST_VSBS);
            ctx.states
                .set(0, st + ST_VSES, ctx.states.get(1, st + ST_VSES));
            vses = (1.0 + xfact) * ctx.states.get(1, st + ST_VSES)
                - xfact * ctx.states.get(2, st + ST_VSES);
            ctx.states
                .set(0, st + ST_VDES, ctx.states.get(1, st + ST_VDES));
            vdes = (1.0 + xfact) * ctx.states.get(1, st + ST_VDES)
                - xfact * ctx.states.get(2, st + ST_VDES);
            ctx.states
                .set(0, st + ST_QDEF, ctx.states.get(1, st + ST_QDEF));
            qdef = (1.0 + xfact) * ctx.states.get(1, st + ST_QDEF)
                - xfact * ctx.states.get(2, st + ST_QDEF);
        } else {
            vds = md.type_ as f64 * (ctx.sol[h.d_node_prime] - ctx.sol[h.s_node_prime]);
            vgs = md.type_ as f64 * (ctx.sol[h.g_node_prime] - ctx.sol[h.s_node_prime]);
            vbs = md.type_ as f64 * (ctx.sol[h.b_node_prime] - ctx.sol[h.s_node_prime]);
            vges = md.type_ as f64 * (ctx.sol[h.g_node_ext] - ctx.sol[h.s_node_prime]);
            vgms = md.type_ as f64 * (ctx.sol[h.g_node_mid] - ctx.sol[h.s_node_prime]);
            vdbs = md.type_ as f64 * (ctx.sol[h.db_node] - ctx.sol[h.s_node_prime]);
            vsbs = md.type_ as f64 * (ctx.sol[h.sb_node] - ctx.sol[h.s_node_prime]);
            vses = md.type_ as f64 * (ctx.sol[h.s_node] - ctx.sol[h.s_node_prime]);
            vdes = md.type_ as f64 * (ctx.sol[h.d_node] - ctx.sol[h.s_node_prime]);
            qdef = md.type_ as f64 * ctx.sol[h.q_node];
        }
        vgdo = ctx.states.get(0, st + ST_VGS) - ctx.states.get(0, st + ST_VDS);
        vgedo = ctx.states.get(0, st + ST_VGES) - ctx.states.get(0, st + ST_VDS);
        vgmdo = ctx.states.get(0, st + ST_VGMS) - ctx.states.get(0, st + ST_VDS);
        vbd = vbs - vds;
        vdbd = vdbs - vds;
        vgd = vgs - vds;
        vged = vges - vds;
        vgmd = vgms - vds;
        delvbd = vbd - ctx.states.get(0, st + ST_VBD);
        delvdbd = vdbd - ctx.states.get(0, st + ST_VDBD);
        delvgd = vgd - vgdo;
        _delvged = vged - vgedo;
        _delvgmd = vgmd - vgmdo;
        delvds = vds - ctx.states.get(0, st + ST_VDS);
        delvgs = vgs - ctx.states.get(0, st + ST_VGS);
        _delvges = vges - ctx.states.get(0, st + ST_VGES);
        _delvgms = vgms - ctx.states.get(0, st + ST_VGMS);
        delvbs = vbs - ctx.states.get(0, st + ST_VBS);
        _delvdbs = vdbs - ctx.states.get(0, st + ST_VDBS);
        delvsbs = vsbs - ctx.states.get(0, st + ST_VSBS);
        delvses = vses - ctx.states.get(0, st + ST_VSES);
        vdedo = ctx.states.get(0, st + ST_VDES) - ctx.states.get(0, st + ST_VDS);
        _delvdes = vdes - ctx.states.get(0, st + ST_VDES);
        delvded = vdes - vds - vdedo;
        delvbd_jct = if h.rbody_mod == 0 { delvbd } else { delvdbd };
        delvbs_jct = if h.rbody_mod == 0 { delvbs } else { delvsbs };
        if h.mode >= 0 {
            idtot = h.cd + h.csub - h.cbd + h.igidl;
            _cdhat = idtot - h.gbd * delvbd_jct
                + (h.gmbs + h.gbbs + h.ggidlb) * delvbs
                + (h.gm + h.gbgs + h.ggidlg) * delvgs
                + (h.gds + h.gbds + h.ggidld) * delvds;
            ibtot = h.cbs + h.cbd - h.igidl - h.igisl - h.csub;
            _cbhat = ibtot + h.gbd * delvbd_jct + h.gbs * delvbs_jct
                - (h.gbbs + h.ggidlb) * delvbs
                - (h.gbgs + h.ggidlg) * delvgs
                - (h.gbds + h.ggidld - h.ggisls) * delvds
                - h.ggislg * delvgd
                - h.ggislb * delvbd;
            igstot = h.igs + h.igcs;
            _cgshat =
                igstot + (h.g_igsg + h.g_igcsg) * delvgs + h.g_igcsd * delvds + h.g_igcsb * delvbs;
            igdtot = h.igd + h.igcd;
            _cgdhat = igdtot
                + h.g_igdg * delvgd
                + h.g_igcdg * delvgs
                + h.g_igcdd * delvds
                + h.g_igcdb * delvbs;
            _igbtot = h.igb;
            _cgbhat = h.igb + h.g_igbg * delvgs + h.g_igbd * delvds + h.g_igbb * delvbs;
        } else {
            idtot = h.cd + h.cbd - h.igidl;
            _cdhat = idtot + h.gbd * delvbd_jct + h.gmbs * delvbd + h.gm * delvgd
                - (h.gds + h.ggidls) * delvds
                - h.ggidlg * delvgs
                - h.ggidlb * delvbs;
            ibtot = h.cbs + h.cbd - h.igidl - h.igisl - h.csub;
            _cbhat = ibtot + h.gbs * delvbs_jct + h.gbd * delvbd_jct
                - (h.gbbs + h.ggislb) * delvbd
                - (h.gbgs + h.ggislg) * delvgd
                + (h.gbds + h.ggisld - h.ggidls) * delvds
                - h.ggidlg * delvgs
                - h.ggidlb * delvbs;
            igstot = h.igs + h.igcd;
            _cgshat = igstot + h.g_igsg * delvgs + h.g_igcdg * delvgd - h.g_igcdd * delvds
                + h.g_igcdb * delvbd;
            igdtot = h.igd + h.igcs;
            _cgdhat =
                igdtot + (h.g_igdg + h.g_igcsg) * delvgd - h.g_igcsd * delvds + h.g_igcsb * delvbd;
            _igbtot = h.igb;
            _cgbhat = h.igb + h.g_igbg * delvgd - h.g_igbd * delvds + h.g_igbb * delvbd;
        }
        isestot = h.gstot * ctx.states.get(0, st + ST_VSES);
        _cseshat =
            isestot + h.gstot * delvses + h.gstotd * delvds + h.gstotg * delvgs + h.gstotb * delvbs;
        idedtot = h.gdtot * vdedo;
        _cdedhat =
            idedtot + h.gdtot * delvded + h.gdtotd * delvds + h.gdtotg * delvgs + h.gdtotb * delvbs;
        von = h.von;
        if ctx.states.get(0, st + ST_VDS) >= 0.0 {
            vgs = fetlim(vgs, ctx.states.get(0, st + ST_VGS), von);
            vds = vgs - vgd;
            vds = limvds(vds, ctx.states.get(0, st + ST_VDS));
            vgd = vgs - vds;
            if h.rgate_mod == 3 {
                vges = fetlim(vges, ctx.states.get(0, st + ST_VGES), von);
                vgms = fetlim(vgms, ctx.states.get(0, st + ST_VGMS), von);
                vged = vges - vds;
                vgmd = vgms - vds;
            } else if h.rgate_mod == 1 || h.rgate_mod == 2 {
                vges = fetlim(vges, ctx.states.get(0, st + ST_VGES), von);
                vged = vges - vds;
            }
            if md.rds_mod != 0 {
                vdes = limvds(vdes, ctx.states.get(0, st + ST_VDES));
                vses = -limvds(-vses, -ctx.states.get(0, st + ST_VSES));
            }
        } else {
            vgd = fetlim(vgd, vgdo, von);
            vds = vgs - vgd;
            vds = -limvds(-vds, -ctx.states.get(0, st + ST_VDS));
            vgs = vgd + vds;
            if h.rgate_mod == 3 {
                vged = fetlim(vged, vgedo, von);
                vges = vged + vds;
                vgmd = fetlim(vgmd, vgmdo, von);
                vgms = vgmd + vds;
            }
            if h.rgate_mod == 1 || h.rgate_mod == 2 {
                vged = fetlim(vged, vgedo, von);
                vges = vged + vds;
            }
            if md.rds_mod != 0 {
                vdes = -limvds(-vdes, -ctx.states.get(0, st + ST_VDES));
                vses = limvds(vses, ctx.states.get(0, st + ST_VSES));
            }
        }
        if vds >= 0.0 {
            vbs = pnjlim_i(
                vbs,
                ctx.states.get(0, st + ST_VBS),
                VT0,
                md.vcrit,
                &mut check,
            );
            vbd = vbs - vds;
            if h.rbody_mod != 0 {
                vdbs = pnjlim_i(
                    vdbs,
                    ctx.states.get(0, st + ST_VDBS),
                    VT0,
                    md.vcrit,
                    &mut check1,
                );
                vdbd = vdbs - vds;
                vsbs = pnjlim_i(
                    vsbs,
                    ctx.states.get(0, st + ST_VSBS),
                    VT0,
                    md.vcrit,
                    &mut check2,
                );
                if check1 == 0 && check2 == 0 {
                    check = 0;
                } else {
                    check = 1;
                }
            }
        } else {
            vbd = pnjlim_i(
                vbd,
                ctx.states.get(0, st + ST_VBD),
                VT0,
                md.vcrit,
                &mut check,
            );
            vbs = vbd + vds;
            if h.rbody_mod != 0 {
                vdbd = pnjlim_i(
                    vdbd,
                    ctx.states.get(0, st + ST_VDBD),
                    VT0,
                    md.vcrit,
                    &mut check1,
                );
                vdbs = vdbd + vds;
                vsbdo = ctx.states.get(0, st + ST_VSBS) - ctx.states.get(0, st + ST_VDS);
                vsbd = vsbs - vds;
                vsbd = pnjlim_i(vsbd, vsbdo, VT0, md.vcrit, &mut check2);
                vsbs = vsbd + vds;
                if check1 == 0 && check2 == 0 {
                    check = 0;
                } else {
                    check = 1;
                }
            }
        }
    }
    vbd = vbs - vds;
    vgd = vgs - vds;
    vgb = vgs - vbs;
    vged = vges - vds;
    vgmd = vgms - vds;
    vgmb = vgms - vbs;
    vdbd = vdbs - vds;
    vbs_jct = if h.rbody_mod == 0 { vbs } else { vsbs };
    vbd_jct = if h.rbody_mod == 0 { vbd } else { vdbd };
    nvtms = md.vtm * md.sjct_emission_coeff;
    if h.aseff <= 0.0 && h.pseff <= 0.0 {
        sourcesatcurrent = 0.0;
    } else {
        sourcesatcurrent = h.aseff * md.sjct_temp_sat_cur_density
            + h.pseff * md.sjct_sidewall_temp_sat_cur_density
            + pp.weff_cj * h.nf * md.sjct_gate_sidewall_temp_sat_cur_density;
    }
    if sourcesatcurrent <= 0.0 {
        h.gbs = ctx.gmin;
        h.cbs = h.gbs * vbs_jct;
    } else {
        match md.dio_mod {
            0 => {
                evbs = exp(vbs_jct / nvtms);
                t1 = md.xjbvs * exp(-(md.bvs + vbs_jct) / nvtms);
                h.gbs = sourcesatcurrent * (evbs + t1) / nvtms + ctx.gmin;
                h.cbs = sourcesatcurrent * (evbs + h.xexp_bvs - t1 - 1.0) + ctx.gmin * vbs_jct;
            }
            1 => {
                t2 = vbs_jct / nvtms;
                if t2 < -EXP_THRESHOLD {
                    h.gbs = ctx.gmin;
                    h.cbs = sourcesatcurrent * (MIN_EXP - 1.0) + ctx.gmin * vbs_jct;
                } else if vbs_jct <= h.vjsm_fwd {
                    evbs = exp(t2);
                    h.gbs = sourcesatcurrent * evbs / nvtms + ctx.gmin;
                    h.cbs = sourcesatcurrent * (evbs - 1.0) + ctx.gmin * vbs_jct;
                } else {
                    t0 = h.ivjsm_fwd / nvtms;
                    h.gbs = t0 + ctx.gmin;
                    h.cbs = h.ivjsm_fwd - sourcesatcurrent
                        + t0 * (vbs_jct - h.vjsm_fwd)
                        + ctx.gmin * vbs_jct;
                }
            }
            2 => {
                if vbs_jct < h.vjsm_rev {
                    t0 = vbs_jct / nvtms;
                    if t0 < -EXP_THRESHOLD {
                        evbs = MIN_EXP;
                        devbs_dvb = 0.0;
                    } else {
                        evbs = exp(t0);
                        devbs_dvb = evbs / nvtms;
                    }
                    t1 = evbs - 1.0;
                    t2 = h.ivjsm_rev + h.sslp_rev * (vbs_jct - h.vjsm_rev);
                    h.gbs = devbs_dvb * t2 + t1 * h.sslp_rev + ctx.gmin;
                    h.cbs = t1 * t2 + ctx.gmin * vbs_jct;
                } else if vbs_jct <= h.vjsm_fwd {
                    t0 = vbs_jct / nvtms;
                    if t0 < -EXP_THRESHOLD {
                        evbs = MIN_EXP;
                        devbs_dvb = 0.0;
                    } else {
                        evbs = exp(t0);
                        devbs_dvb = evbs / nvtms;
                    }
                    t1 = (md.bvs + vbs_jct) / nvtms;
                    if t1 > EXP_THRESHOLD {
                        t2 = MIN_EXP;
                        t3 = 0.0;
                    } else {
                        t2 = exp(-t1);
                        t3 = -t2 / nvtms;
                    }
                    h.gbs = sourcesatcurrent * (devbs_dvb - md.xjbvs * t3) + ctx.gmin;
                    h.cbs = sourcesatcurrent * (evbs + h.xexp_bvs - 1.0 - md.xjbvs * t2)
                        + ctx.gmin * vbs_jct;
                } else {
                    h.gbs = h.sslp_fwd + ctx.gmin;
                    h.cbs = h.ivjsm_fwd + h.sslp_fwd * (vbs_jct - h.vjsm_fwd) + ctx.gmin * vbs_jct;
                }
            }
            _ => {}
        }
    }
    nvtmd = md.vtm * md.djct_emission_coeff;
    if h.adeff <= 0.0 && h.pdeff <= 0.0 {
        drainsatcurrent = 0.0;
    } else {
        drainsatcurrent = h.adeff * md.djct_temp_sat_cur_density
            + h.pdeff * md.djct_sidewall_temp_sat_cur_density
            + pp.weff_cj * h.nf * md.djct_gate_sidewall_temp_sat_cur_density;
    }
    if drainsatcurrent <= 0.0 {
        h.gbd = ctx.gmin;
        h.cbd = h.gbd * vbd_jct;
    } else {
        match md.dio_mod {
            0 => {
                evbd = exp(vbd_jct / nvtmd);
                t1 = md.xjbvd * exp(-(md.bvd + vbd_jct) / nvtmd);
                h.gbd = drainsatcurrent * (evbd + t1) / nvtmd + ctx.gmin;
                h.cbd = drainsatcurrent * (evbd + h.xexp_bvd - t1 - 1.0) + ctx.gmin * vbd_jct;
            }
            1 => {
                t2 = vbd_jct / nvtmd;
                if t2 < -EXP_THRESHOLD {
                    h.gbd = ctx.gmin;
                    h.cbd = drainsatcurrent * (MIN_EXP - 1.0) + ctx.gmin * vbd_jct;
                } else if vbd_jct <= h.vjdm_fwd {
                    evbd = exp(t2);
                    h.gbd = drainsatcurrent * evbd / nvtmd + ctx.gmin;
                    h.cbd = drainsatcurrent * (evbd - 1.0) + ctx.gmin * vbd_jct;
                } else {
                    t0 = h.ivjdm_fwd / nvtmd;
                    h.gbd = t0 + ctx.gmin;
                    h.cbd = h.ivjdm_fwd - drainsatcurrent
                        + t0 * (vbd_jct - h.vjdm_fwd)
                        + ctx.gmin * vbd_jct;
                }
            }
            2 => {
                if vbd_jct < h.vjdm_rev {
                    t0 = vbd_jct / nvtmd;
                    if t0 < -EXP_THRESHOLD {
                        evbd = MIN_EXP;
                        devbd_dvb = 0.0;
                    } else {
                        evbd = exp(t0);
                        devbd_dvb = evbd / nvtmd;
                    }
                    t1 = evbd - 1.0;
                    t2 = h.ivjdm_rev + h.dslp_rev * (vbd_jct - h.vjdm_rev);
                    h.gbd = devbd_dvb * t2 + t1 * h.dslp_rev + ctx.gmin;
                    h.cbd = t1 * t2 + ctx.gmin * vbd_jct;
                } else if vbd_jct <= h.vjdm_fwd {
                    t0 = vbd_jct / nvtmd;
                    if t0 < -EXP_THRESHOLD {
                        evbd = MIN_EXP;
                        devbd_dvb = 0.0;
                    } else {
                        evbd = exp(t0);
                        devbd_dvb = evbd / nvtmd;
                    }
                    t1 = (md.bvd + vbd_jct) / nvtmd;
                    if t1 > EXP_THRESHOLD {
                        t2 = MIN_EXP;
                        t3 = 0.0;
                    } else {
                        t2 = exp(-t1);
                        t3 = -t2 / nvtmd;
                    }
                    h.gbd = drainsatcurrent * (devbd_dvb - md.xjbvd * t3) + ctx.gmin;
                    h.cbd = drainsatcurrent * (evbd + h.xexp_bvd - 1.0 - md.xjbvd * t2)
                        + ctx.gmin * vbd_jct;
                } else {
                    h.gbd = h.dslp_fwd + ctx.gmin;
                    h.cbd = h.ivjdm_fwd + h.dslp_fwd * (vbd_jct - h.vjdm_fwd) + ctx.gmin * vbd_jct;
                }
            }
            _ => {}
        }
    }
    nvtmrssws = md.vtm0 * md.njtsswstemp;
    nvtmrsswgs = md.vtm0 * md.njtsswgstemp;
    nvtmrss = md.vtm0 * md.njtsstemp;
    nvtmrsswd = md.vtm0 * md.njtsswdtemp;
    nvtmrsswgd = md.vtm0 * md.njtsswgdtemp;
    nvtmrsd = md.vtm0 * md.njtsdtemp;
    if md.vtss - vbs_jct < md.vtss * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbs_jct / nvtmrss * t9;
        (t1, t10) = dexp3(t0);
        dt1_dvb = t10 / nvtmrss * t9;
    } else {
        t9 = 1.0 / (md.vtss - vbs_jct);
        t0 = -vbs_jct / nvtmrss * md.vtss * t9;
        dt0_dvb = md.vtss / nvtmrss * (t9 + vbs_jct * t9 * t9);
        (t1, t10) = dexp3(t0);
        dt1_dvb = t10 * dt0_dvb;
    }
    if md.vtsd - vbd_jct < md.vtsd * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbd_jct / nvtmrsd * t9;
        (t2, t10) = dexp3(t0);
        dt2_dvb = t10 / nvtmrsd * t9;
    } else {
        t9 = 1.0 / (md.vtsd - vbd_jct);
        t0 = -vbd_jct / nvtmrsd * md.vtsd * t9;
        dt0_dvb = md.vtsd / nvtmrsd * (t9 + vbd_jct * t9 * t9);
        (t2, t10) = dexp3(t0);
        dt2_dvb = t10 * dt0_dvb;
    }
    if md.vtssws - vbs_jct < md.vtssws * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbs_jct / nvtmrssws * t9;
        (t3, t10) = dexp3(t0);
        dt3_dvb = t10 / nvtmrssws * t9;
    } else {
        t9 = 1.0 / (md.vtssws - vbs_jct);
        t0 = -vbs_jct / nvtmrssws * md.vtssws * t9;
        dt0_dvb = md.vtssws / nvtmrssws * (t9 + vbs_jct * t9 * t9);
        (t3, t10) = dexp3(t0);
        dt3_dvb = t10 * dt0_dvb;
    }
    if md.vtsswd - vbd_jct < md.vtsswd * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbd_jct / nvtmrsswd * t9;
        (t4, t10) = dexp3(t0);
        dt4_dvb = t10 / nvtmrsswd * t9;
    } else {
        t9 = 1.0 / (md.vtsswd - vbd_jct);
        t0 = -vbd_jct / nvtmrsswd * md.vtsswd * t9;
        dt0_dvb = md.vtsswd / nvtmrsswd * (t9 + vbd_jct * t9 * t9);
        (t4, t10) = dexp3(t0);
        dt4_dvb = t10 * dt0_dvb;
    }
    if md.vtsswgs - vbs_jct < md.vtsswgs * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbs_jct / nvtmrsswgs * t9;
        (t5, t10) = dexp3(t0);
        dt5_dvb = t10 / nvtmrsswgs * t9;
    } else {
        t9 = 1.0 / (md.vtsswgs - vbs_jct);
        t0 = -vbs_jct / nvtmrsswgs * md.vtsswgs * t9;
        dt0_dvb = md.vtsswgs / nvtmrsswgs * (t9 + vbs_jct * t9 * t9);
        (t5, t10) = dexp3(t0);
        dt5_dvb = t10 * dt0_dvb;
    }
    if md.vtsswgd - vbd_jct < md.vtsswgd * 1e-3 {
        t9 = 1.0e3;
        t0 = -vbd_jct / nvtmrsswgd * t9;
        (t6, t10) = dexp3(t0);
        dt6_dvb = t10 / nvtmrsswgd * t9;
    } else {
        t9 = 1.0 / (md.vtsswgd - vbd_jct);
        t0 = -vbd_jct / nvtmrsswgd * md.vtsswgd * t9;
        dt0_dvb = md.vtsswgd / nvtmrsswgd * (t9 + vbd_jct * t9 * t9);
        (t6, t10) = dexp3(t0);
        dt6_dvb = t10 * dt0_dvb;
    }
    h.gbs += h.sjct_temp_rev_sat_cur * dt1_dvb
        + h.ssw_temp_rev_sat_cur * dt3_dvb
        + h.sswg_temp_rev_sat_cur * dt5_dvb;
    h.cbs -= h.sjct_temp_rev_sat_cur * (t1 - 1.0)
        + h.ssw_temp_rev_sat_cur * (t3 - 1.0)
        + h.sswg_temp_rev_sat_cur * (t5 - 1.0);
    h.gbd += h.djct_temp_rev_sat_cur * dt2_dvb
        + h.dsw_temp_rev_sat_cur * dt4_dvb
        + h.dswg_temp_rev_sat_cur * dt6_dvb;
    h.cbd -= h.djct_temp_rev_sat_cur * (t2 - 1.0)
        + h.dsw_temp_rev_sat_cur * (t4 - 1.0)
        + h.dswg_temp_rev_sat_cur * (t6 - 1.0);
    if vds >= 0.0 {
        h.mode = 1;
        vds_ = vds;
        vgs_ = vgs;
        vbs_ = vbs;
        _vdb = vds - vbs;
    } else {
        h.mode = -1;
        vds_ = -vds;
        vgs_ = vgd;
        vbs_ = vbd;
        _vdb = -vbs;
    }
    if md.mtrl_mod != 0 {
        epsrox = 3.9;
        toxe = md.eot;
        epssub = EPS0 * md.epsrsub;
    } else {
        epsrox = md.epsrox;
        toxe = md.toxe;
        epssub = EPSSI;
    }
    t0 = vbs_ - h.vbsc - 0.001;
    t1 = sqrt(t0 * t0 - 0.004 * h.vbsc);
    if t0 >= 0.0 {
        vbseff = h.vbsc + 0.5 * (t0 + t1);
        dvbseff_dvb = 0.5 * (1.0 + t0 / t1);
    } else {
        t2 = -0.002 / (t1 - t0);
        vbseff = h.vbsc * (1.0 + t2);
        dvbseff_dvb = t2 * h.vbsc / t1;
    }
    t9 = 0.95 * pp.phi;
    t0 = t9 - vbseff - 0.001;
    t1 = sqrt(t0 * t0 + 0.004 * t9);
    vbseff = t9 - 0.5 * (t0 + t1);
    dvbseff_dvb *= 0.5 * (1.0 + t0 / t1);
    phis = pp.phi - vbseff;
    dphis_dvb = -1.0;
    sqrtphis = sqrt(phis);
    dsqrtphis_dvb = -0.5 / sqrtphis;
    xdep = pp.xdep0 * sqrtphis / pp.sqrt_phi;
    dxdep_dvb = pp.xdep0 / pp.sqrt_phi * dsqrtphis_dvb;
    leff = pp.leff;
    vtm = md.vtm;
    vtm0 = md.vtm0;
    t3 = sqrt(xdep);
    v0 = pp.vbi - pp.phi;
    t0 = pp.dvt2 * vbseff;
    if t0 >= -0.5 {
        t1 = 1.0 + t0;
        t2 = pp.dvt2;
    } else {
        t4 = 1.0 / (3.0 + 8.0 * t0);
        t1 = (1.0 + 3.0 * t0) * t4;
        t2 = pp.dvt2 * t4 * t4;
    }
    lt1 = md.factor1 * t3 * t1;
    dlt1_dvb = md.factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);
    t0 = pp.dvt2w * vbseff;
    if t0 >= -0.5 {
        t1 = 1.0 + t0;
        t2 = pp.dvt2w;
    } else {
        t4 = 1.0 / (3.0 + 8.0 * t0);
        t1 = (1.0 + 3.0 * t0) * t4;
        t2 = pp.dvt2w * t4 * t4;
    }
    ltw = md.factor1 * t3 * t1;
    dltw_dvb = md.factor1 * (0.5 / t3 * t1 * dxdep_dvb + t3 * t2);
    t0 = pp.dvt1 * leff / lt1;
    if t0 < EXP_THRESHOLD {
        t1 = exp(t0);
        t2 = t1 - 1.0;
        t3 = t2 * t2;
        t4 = t3 + 2.0 * t1 * MIN_EXP;
        theta0 = t1 / t4;
        dt1_dvb = -t0 * t1 * dlt1_dvb / lt1;
        dtheta0_dvb = dt1_dvb * (t4 - 2.0 * t1 * (t2 + MIN_EXP)) / t4 / t4;
    } else {
        theta0 = 1.0 / (MAX_EXP - 2.0);
        dtheta0_dvb = 0.0;
    }
    h.thetavth = pp.dvt0 * theta0;
    delt_vth = h.thetavth * v0;
    ddelt_vth_dvb = pp.dvt0 * dtheta0_dvb * v0;
    t0 = pp.dvt1w * pp.weff * leff / ltw;
    if t0 < EXP_THRESHOLD {
        t1 = exp(t0);
        t2 = t1 - 1.0;
        t3 = t2 * t2;
        t4 = t3 + 2.0 * t1 * MIN_EXP;
        t5 = t1 / t4;
        dt1_dvb = -t0 * t1 * dltw_dvb / ltw;
        dt5_dvb = dt1_dvb * (t4 - 2.0 * t1 * (t2 + MIN_EXP)) / t4 / t4;
    } else {
        t5 = 1.0 / (MAX_EXP - 2.0);
        dt5_dvb = 0.0;
    }
    t0 = pp.dvt0w * t5;
    t2 = t0 * v0;
    dt2_dvb = pp.dvt0w * dt5_dvb * v0;
    tempratio = ck_temp / md.tnom - 1.0;
    t0 = sqrt(1.0 + pp.lpe0 / leff);
    t1 = pp.k1ox * (t0 - 1.0) * pp.sqrt_phi
        + (pp.kt1 + pp.kt1l / leff + pp.kt2 * vbseff) * tempratio;
    vth_narroww = toxe * pp.phi / (pp.weff + pp.w0);
    t3 = h.eta0 + pp.etab * vbseff;
    if t3 < 1.0e-4 {
        t9 = 1.0 / (3.0 - 2.0e4 * t3);
        t3 = (2.0e-4 - t3) * t9;
        t4 = t9 * t9;
    } else {
        t4 = 1.0;
    }
    ddibl_sft_dvd = t3 * pp.theta0vb0;
    dibl_sft = ddibl_sft_dvd * vds_;
    lpe_vb = sqrt(1.0 + pp.lpeb / leff);
    vth = md.type_ as f64 * h.vth0 + (pp.k1ox * sqrtphis - pp.k1 * pp.sqrt_phi) * lpe_vb
        - h.k2ox * vbseff
        - delt_vth
        - t2
        + (pp.k3 + pp.k3b * vbseff) * vth_narroww
        + t1
        - dibl_sft;
    dvth_dvb = lpe_vb * pp.k1ox * dsqrtphis_dvb - h.k2ox - ddelt_vth_dvb - dt2_dvb
        + pp.k3b * vth_narroww
        - pp.etab * vds_ * pp.theta0vb0 * t4
        + pp.kt2 * tempratio;
    dvth_dvd = -ddibl_sft_dvd;
    tmp1 = epssub / xdep;
    h.nstar = md.vtm / CHARGE_Q * (md.coxe + tmp1 + pp.cit);
    tmp2 = pp.nfactor * tmp1;
    tmp3 = pp.cdsc + pp.cdscb * vbseff + pp.cdscd * vds_;
    tmp4 = (tmp2 + tmp3 * theta0 + pp.cit) / md.coxe;
    if tmp4 >= -0.5 {
        n = 1.0 + tmp4;
        dn_dvb = (-tmp2 / xdep * dxdep_dvb + tmp3 * dtheta0_dvb + pp.cdscb * theta0) / md.coxe;
        dn_dvd = pp.cdscd * theta0 / md.coxe;
    } else {
        t0 = 1.0 / (3.0 + 8.0 * tmp4);
        n = (1.0 + 3.0 * tmp4) * t0;
        t0 *= t0;
        dn_dvb = (-tmp2 / xdep * dxdep_dvb + tmp3 * dtheta0_dvb + pp.cdscb * theta0) / md.coxe * t0;
        dn_dvd = pp.cdscd * theta0 / md.coxe * t0;
    }
    if pp.dvtp0 > 0.0 {
        t0 = -pp.dvtp1 * vds_;
        if t0 < -EXP_THRESHOLD {
            t2 = MIN_EXP;
            dt2_dvd = 0.0;
        } else {
            t2 = exp(t0);
            dt2_dvd = -pp.dvtp1 * t2;
        }
        t3 = leff + pp.dvtp0 * (1.0 + t2);
        dt3_dvd = pp.dvtp0 * dt2_dvd;
        if md.temp_mod < 2 {
            t4 = vtm * log(leff / t3);
            dt4_dvd = -vtm * dt3_dvd / t3;
        } else {
            t4 = md.vtm0 * log(leff / t3);
            dt4_dvd = -md.vtm0 * dt3_dvd / t3;
        }
        ddits_sft_dvd = dn_dvd * t4 + n * dt4_dvd;
        ddits_sft_dvb = t4 * dn_dvb;
        vth -= n * t4;
        dvth_dvd -= ddits_sft_dvd;
        dvth_dvb -= ddits_sft_dvb;
    }
    if pp.dvtp4 == 0.0 || pp.dvtp2factor == 0.0 {
        t0 = 0.0;
        dits_sft2 = 0.0;
    } else {
        t1 = 2.0 * pp.dvtp4 * vds_;
        (t0, t10) = dexp3(t1);
        dits_sft2 = pp.dvtp2factor * (t0 - 1.0) / (t0 + 1.0);
        ddits_sft2_dvd = pp.dvtp2factor * pp.dvtp4 * 4.0 * t10 / ((t0 + 1.0) * (t0 + 1.0));
        vth -= dits_sft2;
        dvth_dvd -= ddits_sft2_dvd;
    }
    h.von = vth;
    t0 = h.vfb + pp.phi;
    if md.mtrl_mod == 0 {
        t1 = EPSSI;
    } else {
        t1 = md.epsrgate * EPS0;
    }
    let _ = poly_depletion(
        t0,
        pp.ngate,
        t1,
        md.coxe,
        vgs,
        &mut vgs_eff,
        &mut dvgs_eff_dvg,
    );
    let _ = poly_depletion(
        t0,
        pp.ngate,
        t1,
        md.coxe,
        vgd,
        &mut vgd_eff,
        &mut dvgd_eff_dvg,
    );
    if h.mode > 0 {
        vgs_eff_ = vgs_eff;
        dvgs_eff_dvg_ = dvgs_eff_dvg;
    } else {
        vgs_eff_ = vgd_eff;
        dvgs_eff_dvg_ = dvgd_eff_dvg;
    }
    h.vgs_eff = vgs_eff;
    h.vgd_eff = vgd_eff;
    h.dvgs_eff_dvg = dvgs_eff_dvg;
    h.dvgd_eff_dvg = dvgd_eff_dvg;
    vgst = vgs_eff_ - vth;
    t0 = n * vtm;
    t1 = pp.mstar * vgst;
    t2 = t1 / t0;
    if t2 > EXP_THRESHOLD {
        t10 = t1;
        dt10_dvg = pp.mstar * dvgs_eff_dvg_;
        dt10_dvd = -dvth_dvd * pp.mstar;
        dt10_dvb = -dvth_dvb * pp.mstar;
    } else if t2 < -EXP_THRESHOLD {
        t10 = vtm * log(1.0 + MIN_EXP);
        dt10_dvg = 0.0;
        dt10_dvd = t10 * dn_dvd;
        dt10_dvb = t10 * dn_dvb;
        t10 *= n;
    } else {
        expvgst = exp(t2);
        t3 = vtm * log(1.0 + expvgst);
        t10 = n * t3;
        dt10_dvg = pp.mstar * expvgst / (1.0 + expvgst);
        dt10_dvb = t3 * dn_dvb - dt10_dvg * (dvth_dvb + vgst * dn_dvb / n);
        dt10_dvd = t3 * dn_dvd - dt10_dvg * (dvth_dvd + vgst * dn_dvd / n);
        dt10_dvg *= dvgs_eff_dvg_;
    }
    t1 = pp.voffcbn - (1.0 - pp.mstar) * vgst;
    t2 = t1 / t0;
    if t2 < -EXP_THRESHOLD {
        t3 = md.coxe * MIN_EXP / pp.cdep0;
        t9 = pp.mstar + t3 * n;
        dt9_dvg = 0.0;
        dt9_dvd = dn_dvd * t3;
        dt9_dvb = dn_dvb * t3;
    } else if t2 > EXP_THRESHOLD {
        t3 = md.coxe * MAX_EXP / pp.cdep0;
        t9 = pp.mstar + t3 * n;
        dt9_dvg = 0.0;
        dt9_dvd = dn_dvd * t3;
        dt9_dvb = dn_dvb * t3;
    } else {
        expvgst = exp(t2);
        t3 = md.coxe / pp.cdep0;
        t4 = t3 * expvgst;
        t5 = t1 * t4 / t0;
        t9 = pp.mstar + n * t4;
        dt9_dvg = t3 * (pp.mstar - 1.0) * expvgst / vtm;
        dt9_dvb = t4 * dn_dvb - dt9_dvg * dvth_dvb - t5 * dn_dvb;
        dt9_dvd = t4 * dn_dvd - dt9_dvg * dvth_dvd - t5 * dn_dvd;
        dt9_dvg *= dvgs_eff_dvg_;
    }
    vgsteff = t10 / t9;
    h.vgsteff = vgsteff;
    t11 = t9 * t9;
    dvgsteff_dvg = (t9 * dt10_dvg - t10 * dt9_dvg) / t11;
    dvgsteff_dvd = (t9 * dt10_dvd - t10 * dt9_dvd) / t11;
    dvgsteff_dvb = (t9 * dt10_dvb - t10 * dt9_dvb) / t11;
    t9 = sqrtphis - pp.sqrt_phi;
    weff = pp.weff - 2.0 * (pp.dwg * vgsteff + pp.dwb * t9);
    dweff_dvg = -2.0 * pp.dwg;
    dweff_dvb = -2.0 * pp.dwb * dsqrtphis_dvb;
    if weff < 2.0e-8 {
        t0 = 1.0 / (6.0e-8 - 2.0 * weff);
        weff = 2.0e-8 * (4.0e-8 - weff) * t0;
        let rhs_tmp = t0 * 4.0e-16;
        t0 *= rhs_tmp;
        dweff_dvg *= t0;
        dweff_dvb *= t0;
    }
    if md.rds_mod == 1 {
        drds_dvb = 0.0;
        drds_dvg = drds_dvb;
        rds = drds_dvg;
    } else {
        t0 = 1.0 + pp.prwg * vgsteff;
        dt0_dvg = -pp.prwg / t0 / t0;
        t1 = pp.prwb * t9;
        dt1_dvb = pp.prwb * dsqrtphis_dvb;
        t2 = 1.0 / t0 + t1;
        t3 = t2 + sqrt(t2 * t2 + 0.01);
        dt3_dvg = 1.0 + t2 / (t3 - t2);
        dt3_dvb = dt3_dvg * dt1_dvb;
        dt3_dvg *= dt0_dvg;
        t4 = pp.rds0 * 0.5;
        rds = pp.rdswmin + t3 * t4;
        drds_dvg = t4 * dt3_dvg;
        drds_dvb = t4 * dt3_dvb;
        if rds > 0.0 {
            h.grdsw = 1.0 / rds * h.nf;
        } else {
            h.grdsw = 0.0;
        }
    }
    t9 = 0.5 * pp.k1ox * lpe_vb / sqrtphis;
    t1 = t9 + h.k2ox - pp.k3b * vth_narroww;
    dt1_dvb = -t9 / sqrtphis * dsqrtphis_dvb;
    t9 = sqrt(pp.xj * xdep);
    tmp1 = leff + 2.0 * t9;
    t5 = leff / tmp1;
    tmp2 = pp.a0 * t5;
    tmp3 = pp.weff + pp.b1;
    tmp4 = pp.b0 / tmp3;
    t2 = tmp2 + tmp4;
    dt2_dvb = -t9 / tmp1 / xdep * dxdep_dvb;
    t6 = t5 * t5;
    t7 = t5 * t6;
    abulk0 = 1.0 + t1 * t2;
    dabulk0_dvb = t1 * tmp2 * dt2_dvb + t2 * dt1_dvb;
    t8 = pp.ags * pp.a0 * t7;
    dabulk_dvg = -t1 * t8;
    abulk = abulk0 + dabulk_dvg * vgsteff;
    dabulk_dvb = dabulk0_dvb - t8 * vgsteff * (dt1_dvb + 3.0 * t1 * dt2_dvb);
    if abulk0 < 0.1 {
        t9 = 1.0 / (3.0 - 20.0 * abulk0);
        abulk0 = (0.2 - abulk0) * t9;
        dabulk0_dvb *= t9 * t9;
    }
    if abulk < 0.1 {
        t9 = 1.0 / (3.0 - 20.0 * abulk);
        abulk = (0.2 - abulk) * t9;
        t10 = t9 * t9;
        dabulk_dvb *= t10;
        dabulk_dvg *= t10;
    }
    h.abulk = abulk;
    t2 = pp.keta * vbseff;
    if t2 >= -0.9 {
        t0 = 1.0 / (1.0 + t2);
        dt0_dvb = -pp.keta * t0 * t0;
    } else {
        t1 = 1.0 / (0.8 + t2);
        t0 = (17.0 + 20.0 * t2) * t1;
        dt0_dvb = -pp.keta * t1 * t1;
    }
    dabulk_dvg *= t0;
    dabulk_dvb = dabulk_dvb * t0 + abulk * dt0_dvb;
    dabulk0_q_dvb = dabulk0_dvb;
    dabulk0_dvb = dabulk0_dvb * t0 + abulk0 * dt0_dvb;
    abulk *= t0;
    abulk0_q = abulk0;
    abulk0 *= t0;
    if pp.ketac != pp.keta {
        t2 = pp.ketac * vbseff;
        if t2 >= -0.9 {
            t0 = 1.0 / (1.0 + t2);
            dt0_dvb = -pp.ketac * t0 * t0;
        } else {
            t1 = 1.0 / (0.8 + t2);
            t0 = (17.0 + 20.0 * t2) * t1;
            dt0_dvb = -pp.ketac * t1 * t1;
        }
        dabulk0_q_dvb = dabulk0_q_dvb * t0 + abulk0_q * dt0_dvb;
        abulk0_q *= t0;
    } else {
        dabulk0_q_dvb = dabulk0_dvb;
        abulk0_q = abulk0;
    }
    if md.mtrl_mod != 0 && md.mtrl_compat_mod == 0 {
        t14 = 2.0 * (md.type_ as f64) * (md.phig - md.easub - 0.5 * md.eg0 + 0.45);
    } else {
        t14 = 0.0;
    }
    if md.mob_mod == 0 {
        t0 = vgsteff + vth + vth - t14;
        t2 = pp.ua + pp.uc * vbseff;
        t3 = t0 / toxe;
        t12 = sqrt(vth * vth + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * vth;
        t6 = t8 * vth;
        t5 = t3 * (t2 + pp.ub * t3) + t6;
        t7 = -2.0 * t6 * t9;
        t11 = t7 * vth / t12;
        ddenomi_dvg = (t2 + 2.0 * pp.ub * t3) / toxe;
        t13 = 2.0 * (ddenomi_dvg + t11 + t8);
        ddenomi_dvd = t13 * dvth_dvd;
        ddenomi_dvb = t13 * dvth_dvb + pp.uc * t3;
        ddenomi_dvg += t7;
    } else if md.mob_mod == 1 {
        t0 = vgsteff + vth + vth - t14;
        t2 = 1.0 + pp.uc * vbseff;
        t3 = t0 / toxe;
        t4 = t3 * (pp.ua + pp.ub * t3);
        t12 = sqrt(vth * vth + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * vth;
        t6 = t8 * vth;
        t5 = t4 * t2 + t6;
        t7 = -2.0 * t6 * t9;
        t11 = t7 * vth / t12;
        ddenomi_dvg = (pp.ua + 2.0 * pp.ub * t3) * t2 / toxe;
        t13 = 2.0 * (ddenomi_dvg + t11 + t8);
        ddenomi_dvd = t13 * dvth_dvd;
        ddenomi_dvb = t13 * dvth_dvb + pp.uc * t4;
        ddenomi_dvg += t7;
    } else if md.mob_mod == 2 {
        t0 = (vgsteff + h.vtfbphi1) / toxe;
        t1 = exp(pp.eu * log(t0));
        dt1_dvg = t1 * pp.eu / t0 / toxe;
        t2 = pp.ua + pp.uc * vbseff;
        t12 = sqrt(vth * vth + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * vth;
        t6 = t8 * vth;
        t5 = t1 * t2 + t6;
        t7 = -2.0 * t6 * t9;
        t11 = t7 * vth / t12;
        ddenomi_dvg = t2 * dt1_dvg + t7;
        t13 = 2.0 * (t11 + t8);
        ddenomi_dvd = t13 * dvth_dvd;
        ddenomi_dvb = t13 * dvth_dvb + t1 * pp.uc;
    } else if md.mob_mod == 4 {
        t0 = vgsteff + h.vtfbphi1 - t14;
        t2 = pp.ua + pp.uc * vbseff;
        t3 = t0 / toxe;
        t12 = sqrt(h.vtfbphi1 * h.vtfbphi1 + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * h.vtfbphi1;
        t6 = t8 * h.vtfbphi1;
        t5 = t3 * (t2 + pp.ub * t3) + t6;
        t7 = -2.0 * t6 * t9;
        ddenomi_dvg = (t2 + 2.0 * pp.ub * t3) / toxe;
        ddenomi_dvd = 0.0;
        ddenomi_dvb = pp.uc * t3;
        ddenomi_dvg += t7;
    } else if md.mob_mod == 5 {
        t0 = vgsteff + h.vtfbphi1 - t14;
        t2 = 1.0 + pp.uc * vbseff;
        t3 = t0 / toxe;
        t4 = t3 * (pp.ua + pp.ub * t3);
        t12 = sqrt(h.vtfbphi1 * h.vtfbphi1 + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * h.vtfbphi1;
        t6 = t8 * h.vtfbphi1;
        t5 = t4 * t2 + t6;
        t7 = -2.0 * t6 * t9;
        ddenomi_dvg = (pp.ua + 2.0 * pp.ub * t3) * t2 / toxe;
        ddenomi_dvd = 0.0;
        ddenomi_dvb = pp.uc * t4;
        ddenomi_dvg += t7;
    } else if md.mob_mod == 6 {
        t0 = (vgsteff + h.vtfbphi1) / toxe;
        t1 = exp(pp.eu * log(t0));
        dt1_dvg = t1 * pp.eu / t0 / toxe;
        t2 = pp.ua + pp.uc * vbseff;
        t12 = sqrt(h.vtfbphi1 * h.vtfbphi1 + 0.0001);
        t9 = 1.0 / (vgsteff + 2.0 * t12);
        t10 = t9 * toxe;
        t8 = pp.ud * t10 * t10 * h.vtfbphi1;
        t6 = t8 * h.vtfbphi1;
        t5 = t1 * t2 + t6;
        t7 = -2.0 * t6 * t9;
        ddenomi_dvg = t2 * dt1_dvg + t7;
        ddenomi_dvd = 0.0;
        ddenomi_dvb = t1 * pp.uc;
    } else {
        t0 = (vgsteff + h.vtfbphi1) * 1.0e-8 / toxe / 6.0;
        t1 = exp(pp.eu * log(t0));
        dt1_dvg = t1 * pp.eu * 1.0e-8 / t0 / toxe / 6.0;
        t2 = pp.ua + pp.uc * vbseff;
        vgsteffvth = pp.vgsteff_vth;
        t10 = exp(pp.ucs * log(0.5 + 0.5 * vgsteff / vgsteffvth));
        t11 = pp.ud / t10;
        dt11_dvg = -0.5 * pp.ucs * t11 / (0.5 + 0.5 * vgsteff / vgsteffvth) / vgsteffvth;
        ddenomi_dvg = t2 * dt1_dvg + dt11_dvg;
        ddenomi_dvd = 0.0;
        ddenomi_dvb = t1 * pp.uc;
        t5 = t1 * t2 + t11;
    }
    if t5 >= -0.8 {
        denomi = 1.0 + t5;
    } else {
        t9 = 1.0 / (7.0 + 10.0 * t5);
        denomi = (0.6 + t5) * t9;
        t9 *= t9;
        ddenomi_dvg *= t9;
        ddenomi_dvd *= t9;
        ddenomi_dvb *= t9;
    }
    ueff = h.u0temp / denomi;
    h.ueff = ueff;
    t9 = -ueff / denomi;
    dueff_dvg = t9 * ddenomi_dvg;
    dueff_dvd = t9 * ddenomi_dvd;
    dueff_dvb = t9 * ddenomi_dvb;
    wvcox = weff * h.vsattemp * md.coxe;
    wvcoxrds = wvcox * rds;
    esat = 2.0 * h.vsattemp / ueff;
    esatl = esat * leff;
    h.esat_l = esatl;
    t0 = -esatl / ueff;
    desatl_dvg = t0 * dueff_dvg;
    desatl_dvd = t0 * dueff_dvd;
    desatl_dvb = t0 * dueff_dvb;
    a1 = pp.a1;
    if a1 == 0.0 {
        lambda = pp.a2;
        dlambda_dvg = 0.0;
    } else if a1 > 0.0 {
        t0 = 1.0 - pp.a2;
        t1 = t0 - pp.a1 * vgsteff - 0.0001;
        t2 = sqrt(t1 * t1 + 0.0004 * t0);
        lambda = pp.a2 + t0 - 0.5 * (t1 + t2);
        dlambda_dvg = 0.5 * pp.a1 * (1.0 + t1 / t2);
    } else {
        t1 = pp.a2 + pp.a1 * vgsteff - 0.0001;
        t2 = sqrt(t1 * t1 + 0.0004 * pp.a2);
        lambda = 0.5 * (t1 + t2);
        dlambda_dvg = 0.5 * pp.a1 * (1.0 + t1 / t2);
    }
    vgst2vtm = vgsteff + 2.0 * vtm;
    if rds > 0.0 {
        tmp2 = drds_dvg / rds + dweff_dvg / weff;
        tmp3 = drds_dvb / rds + dweff_dvb / weff;
    } else {
        tmp2 = dweff_dvg / weff;
        tmp3 = dweff_dvb / weff;
    }
    if rds == 0.0 && lambda == 1.0 {
        t0 = 1.0 / (abulk * esatl + vgst2vtm);
        tmp1 = 0.0;
        t1 = t0 * t0;
        t2 = vgst2vtm * t0;
        t3 = esatl * vgst2vtm;
        vdsat = t3 * t0;
        dt0_dvg = -(abulk * desatl_dvg + esatl * dabulk_dvg + 1.0) * t1;
        dt0_dvd = -(abulk * desatl_dvd) * t1;
        dt0_dvb = -(abulk * desatl_dvb + dabulk_dvb * esatl) * t1;
        dvdsat_dvg = t3 * dt0_dvg + t2 * desatl_dvg + esatl * t0;
        dvdsat_dvd = t3 * dt0_dvd + t2 * desatl_dvd;
        dvdsat_dvb = t3 * dt0_dvb + t2 * desatl_dvb;
    } else {
        tmp1 = dlambda_dvg / (lambda * lambda);
        t9 = abulk * wvcoxrds;
        t8 = abulk * t9;
        t7 = vgst2vtm * t9;
        t6 = vgst2vtm * wvcoxrds;
        t0 = 2.0 * abulk * (t9 - 1.0 + 1.0 / lambda);
        dt0_dvg = 2.0 * (t8 * tmp2 - abulk * tmp1 + (2.0 * t9 + 1.0 / lambda - 1.0) * dabulk_dvg);
        dt0_dvb =
            2.0 * (t8 * (2.0 / abulk * dabulk_dvb + tmp3) + (1.0 / lambda - 1.0) * dabulk_dvb);
        dt0_dvd = 0.0;
        t1 = vgst2vtm * (2.0 / lambda - 1.0) + abulk * esatl + 3.0 * t7;
        dt1_dvg = 2.0 / lambda - 1.0 - 2.0 * vgst2vtm * tmp1
            + abulk * desatl_dvg
            + esatl * dabulk_dvg
            + 3.0 * (t9 + t7 * tmp2 + t6 * dabulk_dvg);
        dt1_dvb = abulk * desatl_dvb + esatl * dabulk_dvb + 3.0 * (t6 * dabulk_dvb + t7 * tmp3);
        dt1_dvd = abulk * desatl_dvd;
        t2 = vgst2vtm * (esatl + 2.0 * t6);
        dt2_dvg = esatl + vgst2vtm * desatl_dvg + t6 * (4.0 + 2.0 * vgst2vtm * tmp2);
        dt2_dvb = vgst2vtm * (desatl_dvb + 2.0 * t6 * tmp3);
        dt2_dvd = vgst2vtm * desatl_dvd;
        t3 = sqrt(t1 * t1 - 2.0 * t0 * t2);
        vdsat = (t1 - t3) / t0;
        dt3_dvg = (t1 * dt1_dvg - 2.0 * (t0 * dt2_dvg + t2 * dt0_dvg)) / t3;
        dt3_dvd = (t1 * dt1_dvd - 2.0 * (t0 * dt2_dvd + t2 * dt0_dvd)) / t3;
        dt3_dvb = (t1 * dt1_dvb - 2.0 * (t0 * dt2_dvb + t2 * dt0_dvb)) / t3;
        dvdsat_dvg =
            (dt1_dvg - (t1 * dt1_dvg - dt0_dvg * t2 - t0 * dt2_dvg) / t3 - vdsat * dt0_dvg) / t0;
        dvdsat_dvb =
            (dt1_dvb - (t1 * dt1_dvb - dt0_dvb * t2 - t0 * dt2_dvb) / t3 - vdsat * dt0_dvb) / t0;
        dvdsat_dvd = (dt1_dvd - (t1 * dt1_dvd - t0 * dt2_dvd) / t3) / t0;
    }
    h.vdsat = vdsat;
    t1 = vdsat - vds_ - pp.delta;
    dt1_dvg = dvdsat_dvg;
    dt1_dvd = dvdsat_dvd - 1.0;
    dt1_dvb = dvdsat_dvb;
    t2 = sqrt(t1 * t1 + 4.0 * pp.delta * vdsat);
    t0 = t1 / t2;
    t9 = 2.0 * pp.delta;
    t3 = t9 / t2;
    dt2_dvg = t0 * dt1_dvg + t3 * dvdsat_dvg;
    dt2_dvd = t0 * dt1_dvd + t3 * dvdsat_dvd;
    dt2_dvb = t0 * dt1_dvb + t3 * dvdsat_dvb;
    if t1 >= 0.0 {
        vdseff = vdsat - 0.5 * (t1 + t2);
        dvdseff_dvg = dvdsat_dvg - 0.5 * (dt1_dvg + dt2_dvg);
        dvdseff_dvd = dvdsat_dvd - 0.5 * (dt1_dvd + dt2_dvd);
        dvdseff_dvb = dvdsat_dvb - 0.5 * (dt1_dvb + dt2_dvb);
    } else {
        t4 = t9 / (t2 - t1);
        t5 = 1.0 - t4;
        t6 = vdsat * t4 / (t2 - t1);
        vdseff = vdsat * t5;
        dvdseff_dvg = dvdsat_dvg * t5 + t6 * (dt2_dvg - dt1_dvg);
        dvdseff_dvd = dvdsat_dvd * t5 + t6 * (dt2_dvd - dt1_dvd);
        dvdseff_dvb = dvdsat_dvb * t5 + t6 * (dt2_dvb - dt1_dvb);
    }
    if vds_ == 0.0 {
        vdseff = 0.0;
        dvdseff_dvg = 0.0;
        dvdseff_dvb = 0.0;
    }
    if vdseff > vds_ {
        vdseff = vds_;
    }
    diffvds = vds_ - vdseff;
    h.vdseff = vdseff;
    if md.given.lambda && md.lambda > 0.0 {
        t1 = leff * ueff;
        t2 = pp.lambda / t1;
        t3 = -t2 / t1 * leff;
        dt2_dvd = t3 * dueff_dvd;
        dt2_dvg = t3 * dueff_dvg;
        dt2_dvb = t3 * dueff_dvb;
        t5 = 1.0 / (esat * pp.litl);
        t4 = -t5 / esatl;
        dt5_dvg = desatl_dvg * t4;
        dt5_dvd = desatl_dvd * t4;
        dt5_dvb = desatl_dvb * t4;
        t6 = 1.0 + diffvds * t5;
        dt6_dvg = dt5_dvg * diffvds - dvdseff_dvg * t5;
        dt6_dvd = dt5_dvd * diffvds + (1.0 - dvdseff_dvd) * t5;
        dt6_dvb = dt5_dvb * diffvds - dvdseff_dvb * t5;
        t7 = 2.0 / (t6 * t6 + 1.0);
        t8 = 1.0 - t7;
        t9 = t6 * t7 * t7;
        dt8_dvg = t9 * dt6_dvg;
        dt8_dvd = t9 * dt6_dvd;
        dt8_dvb = t9 * dt6_dvb;
        t10 = 1.0 + t2 * t8;
        dt10_dvg = dt2_dvg * t8 + t2 * dt8_dvg;
        dt10_dvd = dt2_dvd * t8 + t2 * dt8_dvd;
        dt10_dvb = dt2_dvb * t8 + t2 * dt8_dvb;
        if t10 == 1.0 {
            dt10_dvb = 0.0;
            dt10_dvd = dt10_dvb;
            dt10_dvg = dt10_dvd;
        }
        desatl_dvg *= t10;
        desatl_dvg += esatl * dt10_dvg;
        desatl_dvd *= t10;
        desatl_dvd += esatl * dt10_dvd;
        desatl_dvb *= t10;
        desatl_dvb += esatl * dt10_dvb;
        esatl *= t10;
        esat = esatl / leff;
        h.esat_l = esatl;
    }
    tmp4 = 1.0 - 0.5 * abulk * vdsat / vgst2vtm;
    t9 = wvcoxrds * vgsteff;
    t8 = t9 / vgst2vtm;
    t0 = esatl + vdsat + 2.0 * t9 * tmp4;
    t7 = 2.0 * wvcoxrds * tmp4;
    dt0_dvg = desatl_dvg + dvdsat_dvg + t7 * (1.0 + tmp2 * vgsteff)
        - t8 * (abulk * dvdsat_dvg - abulk * vdsat / vgst2vtm + vdsat * dabulk_dvg);
    dt0_dvb = desatl_dvb + dvdsat_dvb + t7 * tmp3 * vgsteff
        - t8 * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
    dt0_dvd = desatl_dvd + dvdsat_dvd - t8 * abulk * dvdsat_dvd;
    t9 = wvcoxrds * abulk;
    t1 = 2.0 / lambda - 1.0 + t9;
    dt1_dvg = -2.0 * tmp1 + wvcoxrds * (abulk * tmp2 + dabulk_dvg);
    dt1_dvb = dabulk_dvb * wvcoxrds + t9 * tmp3;
    vasat = t0 / t1;
    dvasat_dvg = (dt0_dvg - vasat * dt1_dvg) / t1;
    dvasat_dvb = (dt0_dvb - vasat * dt1_dvb) / t1;
    dvasat_dvd = dt0_dvd / t1;
    tmp1 = h.vtfbphi2;
    tmp2 = 2.0e8 * h.toxp;
    dt0_dvg = 1.0 / tmp2;
    t0 = (vgsteff + tmp1) * dt0_dvg;
    tmp3 = exp(md.bdos * 0.7 * log(t0));
    t1 = 1.0 + tmp3;
    t2 = md.bdos * 0.7 * tmp3 / t0;
    tcen = md.ados * 1.9e-9 / t1;
    dtcen_dvg = -tcen * t2 * dt0_dvg / t1;
    coxeff = epssub * h.coxp / (epssub + h.coxp * tcen);
    h.coxeff = coxeff;
    dcoxeff_dvg = -coxeff * coxeff * dtcen_dvg / epssub;
    coxeffwovl = coxeff * weff / leff;
    beta = ueff * coxeffwovl;
    t3 = ueff / leff;
    dbeta_dvg = coxeffwovl * dueff_dvg + t3 * (weff * dcoxeff_dvg + coxeff * dweff_dvg);
    dbeta_dvd = coxeffwovl * dueff_dvd;
    dbeta_dvb = coxeffwovl * dueff_dvb + t3 * coxeff * dweff_dvb;
    h.abov_vgst2_vtm = abulk / vgst2vtm;
    t0 = 1.0 - 0.5 * vdseff * h.abov_vgst2_vtm;
    dt0_dvg =
        -0.5 * (abulk * dvdseff_dvg - abulk * vdseff / vgst2vtm + vdseff * dabulk_dvg) / vgst2vtm;
    dt0_dvd = -0.5 * abulk * dvdseff_dvd / vgst2vtm;
    dt0_dvb = -0.5 * (abulk * dvdseff_dvb + dabulk_dvb * vdseff) / vgst2vtm;
    fgche1 = vgsteff * t0;
    dfgche1_dvg = vgsteff * dt0_dvg + t0;
    dfgche1_dvd = vgsteff * dt0_dvd;
    dfgche1_dvb = vgsteff * dt0_dvb;
    t9 = vdseff / esatl;
    fgche2 = 1.0 + t9;
    dfgche2_dvg = (dvdseff_dvg - t9 * desatl_dvg) / esatl;
    dfgche2_dvd = (dvdseff_dvd - t9 * desatl_dvd) / esatl;
    dfgche2_dvb = (dvdseff_dvb - t9 * desatl_dvb) / esatl;
    gche = beta * fgche1 / fgche2;
    dgche_dvg = (beta * dfgche1_dvg + fgche1 * dbeta_dvg - gche * dfgche2_dvg) / fgche2;
    dgche_dvd = (beta * dfgche1_dvd + fgche1 * dbeta_dvd - gche * dfgche2_dvd) / fgche2;
    dgche_dvb = (beta * dfgche1_dvb + fgche1 * dbeta_dvb - gche * dfgche2_dvb) / fgche2;
    t0 = 1.0 + gche * rds;
    idl = gche / t0;
    t1 = (1.0 - idl * rds) / t0;
    t2 = idl * idl;
    didl_dvg = t1 * dgche_dvg - t2 * drds_dvg;
    didl_dvd = t1 * dgche_dvd;
    didl_dvb = t1 * dgche_dvb - t2 * drds_dvb;
    if pp.fprout <= 0.0 {
        fp = 1.0;
        dfp_dvg = 0.0;
    } else {
        t9 = pp.fprout * sqrt(leff) / vgst2vtm;
        fp = 1.0 / (1.0 + t9);
        dfp_dvg = fp * fp * t9 / vgst2vtm;
    }
    t8 = pp.pvag / esatl;
    t9 = t8 * vgsteff;
    if t9 > -0.9 {
        pvagterm = 1.0 + t9;
        dpvagterm_dvg = t8 * (1.0 - vgsteff * desatl_dvg / esatl);
        dpvagterm_dvb = -t9 * desatl_dvb / esatl;
        dpvagterm_dvd = -t9 * desatl_dvd / esatl;
    } else {
        t4 = 1.0 / (17.0 + 20.0 * t9);
        pvagterm = (0.8 + t9) * t4;
        t4 *= t4;
        dpvagterm_dvg = t8 * (1.0 - vgsteff * desatl_dvg / esatl) * t4;
        t9 *= t4 / esatl;
        dpvagterm_dvb = -t9 * desatl_dvb;
        dpvagterm_dvd = -t9 * desatl_dvd;
    }
    if pp.pclm > MIN_EXP && diffvds > 1.0e-10 {
        t0 = 1.0 + rds * idl;
        dt0_dvg = drds_dvg * idl + rds * didl_dvg;
        dt0_dvd = rds * didl_dvd;
        dt0_dvb = drds_dvb * idl + rds * didl_dvb;
        t2 = vdsat / esat;
        t1 = leff + t2;
        dt1_dvg = (dvdsat_dvg - t2 * desatl_dvg / leff) / esat;
        dt1_dvd = (dvdsat_dvd - t2 * desatl_dvd / leff) / esat;
        dt1_dvb = (dvdsat_dvb - t2 * desatl_dvb / leff) / esat;
        cclm = fp * pvagterm * t0 * t1 / (pp.pclm * pp.litl);
        dcclm_dvg = cclm * (dfp_dvg / fp + dpvagterm_dvg / pvagterm + dt0_dvg / t0 + dt1_dvg / t1);
        dcclm_dvb = cclm * (dpvagterm_dvb / pvagterm + dt0_dvb / t0 + dt1_dvb / t1);
        dcclm_dvd = cclm * (dpvagterm_dvd / pvagterm + dt0_dvd / t0 + dt1_dvd / t1);
        vaclm = cclm * diffvds;
        dvaclm_dvg = dcclm_dvg * diffvds - dvdseff_dvg * cclm;
        dvaclm_dvb = dcclm_dvb * diffvds - dvdseff_dvb * cclm;
        dvaclm_dvd = dcclm_dvd * diffvds + (1.0 - dvdseff_dvd) * cclm;
    } else {
        cclm = MAX_EXP;
        vaclm = cclm;
        dvaclm_dvb = 0.0;
        dvaclm_dvg = dvaclm_dvb;
        dvaclm_dvd = dvaclm_dvg;
        dcclm_dvb = 0.0;
        dcclm_dvg = dcclm_dvb;
        dcclm_dvd = dcclm_dvg;
    }
    if pp.theta_rout > MIN_EXP {
        t8 = abulk * vdsat;
        t0 = vgst2vtm * t8;
        dt0_dvg = vgst2vtm * abulk * dvdsat_dvg + t8 + vgst2vtm * vdsat * dabulk_dvg;
        dt0_dvb = vgst2vtm * (dabulk_dvb * vdsat + abulk * dvdsat_dvb);
        dt0_dvd = vgst2vtm * abulk * dvdsat_dvd;
        t1 = vgst2vtm + t8;
        dt1_dvg = 1.0 + abulk * dvdsat_dvg + vdsat * dabulk_dvg;
        dt1_dvb = abulk * dvdsat_dvb + dabulk_dvb * vdsat;
        dt1_dvd = abulk * dvdsat_dvd;
        t9 = t1 * t1;
        t2 = pp.theta_rout;
        vadibl = (vgst2vtm - t0 / t1) / t2;
        dvadibl_dvg = (1.0 - dt0_dvg / t1 + t0 * dt1_dvg / t9) / t2;
        dvadibl_dvb = (-dt0_dvb / t1 + t0 * dt1_dvb / t9) / t2;
        dvadibl_dvd = (-dt0_dvd / t1 + t0 * dt1_dvd / t9) / t2;
        t7 = pp.pdiblb * vbseff;
        if t7 >= -0.9 {
            t3 = 1.0 / (1.0 + t7);
            vadibl *= t3;
            dvadibl_dvg *= t3;
            dvadibl_dvb = (dvadibl_dvb - vadibl * pp.pdiblb) * t3;
            dvadibl_dvd *= t3;
        } else {
            t4 = 1.0 / (0.8 + t7);
            t3 = (17.0 + 20.0 * t7) * t4;
            dvadibl_dvg *= t3;
            dvadibl_dvb = dvadibl_dvb * t3 - vadibl * pp.pdiblb * t4 * t4;
            dvadibl_dvd *= t3;
            vadibl *= t3;
        }
        dvadibl_dvg = dvadibl_dvg * pvagterm + vadibl * dpvagterm_dvg;
        dvadibl_dvb = dvadibl_dvb * pvagterm + vadibl * dpvagterm_dvb;
        dvadibl_dvd = dvadibl_dvd * pvagterm + vadibl * dpvagterm_dvd;
        vadibl *= pvagterm;
    } else {
        vadibl = MAX_EXP;
        dvadibl_dvb = 0.0;
        dvadibl_dvg = dvadibl_dvb;
        dvadibl_dvd = dvadibl_dvg;
    }
    va = vasat + vaclm;
    dva_dvg = dvasat_dvg + dvaclm_dvg;
    dva_dvb = dvasat_dvb + dvaclm_dvb;
    dva_dvd = dvasat_dvd + dvaclm_dvd;
    t0 = pp.pditsd * vds_;
    if t0 > EXP_THRESHOLD {
        t1 = MAX_EXP;
        dt1_dvd = 0.0;
    } else {
        t1 = exp(t0);
        dt1_dvd = t1 * pp.pditsd;
    }
    if pp.pdits > MIN_EXP {
        t2 = 1.0 + md.pditsl * leff;
        vadits = (1.0 + t2 * t1) / pp.pdits;
        dvadits_dvg = vadits * dfp_dvg;
        dvadits_dvd = fp * t2 * dt1_dvd / pp.pdits;
        vadits *= fp;
    } else {
        vadits = MAX_EXP;
        dvadits_dvd = 0.0;
        dvadits_dvg = dvadits_dvd;
    }
    if pp.pscbe2 > 0.0 && pp.pscbe1 >= 0.0 {
        if diffvds > pp.pscbe1 * pp.litl / EXP_THRESHOLD {
            t0 = pp.pscbe1 * pp.litl / diffvds;
            vascbe = leff * exp(t0) / pp.pscbe2;
            t1 = t0 * vascbe / diffvds;
            dvascbe_dvg = t1 * dvdseff_dvg;
            dvascbe_dvd = -t1 * (1.0 - dvdseff_dvd);
            dvascbe_dvb = t1 * dvdseff_dvb;
        } else {
            vascbe = MAX_EXP * leff / pp.pscbe2;
            dvascbe_dvb = 0.0;
            dvascbe_dvd = dvascbe_dvb;
            dvascbe_dvg = dvascbe_dvd;
        }
    } else {
        vascbe = MAX_EXP;
        dvascbe_dvb = 0.0;
        dvascbe_dvd = dvascbe_dvb;
        dvascbe_dvg = dvascbe_dvd;
    }
    t9 = diffvds / vadibl;
    t0 = 1.0 + t9;
    idsa = idl * t0;
    didsa_dvg = t0 * didl_dvg - idl * (dvdseff_dvg + t9 * dvadibl_dvg) / vadibl;
    didsa_dvd = t0 * didl_dvd + idl * (1.0 - dvdseff_dvd - t9 * dvadibl_dvd) / vadibl;
    didsa_dvb = t0 * didl_dvb - idl * (dvdseff_dvb + t9 * dvadibl_dvb) / vadibl;
    t9 = diffvds / vadits;
    t0 = 1.0 + t9;
    didsa_dvg = t0 * didsa_dvg - idsa * (dvdseff_dvg + t9 * dvadits_dvg) / vadits;
    didsa_dvd = t0 * didsa_dvd + idsa * (1.0 - dvdseff_dvd - t9 * dvadits_dvd) / vadits;
    didsa_dvb = t0 * didsa_dvb - idsa * dvdseff_dvb / vadits;
    idsa *= t0;
    t0 = log(va / vasat);
    dt0_dvg = dva_dvg / va - dvasat_dvg / vasat;
    dt0_dvb = dva_dvb / va - dvasat_dvb / vasat;
    dt0_dvd = dva_dvd / va - dvasat_dvd / vasat;
    t1 = t0 / cclm;
    t9 = 1.0 + t1;
    dt9_dvg = (dt0_dvg - t1 * dcclm_dvg) / cclm;
    dt9_dvb = (dt0_dvb - t1 * dcclm_dvb) / cclm;
    dt9_dvd = (dt0_dvd - t1 * dcclm_dvd) / cclm;
    didsa_dvg = didsa_dvg * t9 + idsa * dt9_dvg;
    didsa_dvb = didsa_dvb * t9 + idsa * dt9_dvb;
    didsa_dvd = didsa_dvd * t9 + idsa * dt9_dvd;
    idsa *= t9;
    tmp = pp.alpha0 + pp.alpha1 * leff;
    if tmp <= 0.0 || pp.beta0 <= 0.0 {
        gbg = 0.0;
        gbb = gbg;
        gbd = gbb;
        isub = gbd;
    } else {
        t2 = tmp / leff;
        if diffvds > pp.beta0 / EXP_THRESHOLD {
            t0 = -pp.beta0 / diffvds;
            t1 = t2 * diffvds * exp(t0);
            t3 = t1 / diffvds * (t0 - 1.0);
            dt1_dvg = t3 * dvdseff_dvg;
            dt1_dvd = t3 * (dvdseff_dvd - 1.0);
            dt1_dvb = t3 * dvdseff_dvb;
        } else {
            t3 = t2 * MIN_EXP;
            t1 = t3 * diffvds;
            dt1_dvg = -t3 * dvdseff_dvg;
            dt1_dvd = t3 * (1.0 - dvdseff_dvd);
            dt1_dvb = -t3 * dvdseff_dvb;
        }
        t4 = idsa * vdseff;
        isub = t1 * t4;
        gbg = t1 * (didsa_dvg * vdseff + idsa * dvdseff_dvg) + t4 * dt1_dvg;
        gbd = t1 * (didsa_dvd * vdseff + idsa * dvdseff_dvd) + t4 * dt1_dvd;
        gbb = t1 * (didsa_dvb * vdseff + idsa * dvdseff_dvb) + t4 * dt1_dvb;
        gbd += gbg * dvgsteff_dvd;
        gbb += gbg * dvgsteff_dvb;
        gbg *= dvgsteff_dvg;
        gbb *= dvbseff_dvb;
    }
    h.csub = isub;
    h.gbbs = gbb;
    h.gbgs = gbg;
    h.gbds = gbd;
    t9 = diffvds / vascbe;
    t0 = 1.0 + t9;
    ids = idsa * t0;
    gm = t0 * didsa_dvg - idsa * (dvdseff_dvg + t9 * dvascbe_dvg) / vascbe;
    gds = t0 * didsa_dvd + idsa * (1.0 - dvdseff_dvd - t9 * dvascbe_dvd) / vascbe;
    gmb = t0 * didsa_dvb - idsa * (dvdseff_dvb + t9 * dvascbe_dvb) / vascbe;
    tmp1 = gds + gm * dvgsteff_dvd;
    tmp2 = gmb + gm * dvgsteff_dvb;
    tmp3 = gm;
    gm = (ids * dvdseff_dvg + vdseff * tmp3) * dvgsteff_dvg;
    gds = ids * (dvdseff_dvd + dvdseff_dvg * dvgsteff_dvd) + vdseff * tmp1;
    gmb = (ids * (dvdseff_dvb + dvdseff_dvg * dvgsteff_dvb) + vdseff * tmp2) * dvbseff_dvb;
    cdrain = ids * vdseff;
    if md.given.vtl && md.vtl > 0.0 {
        t12 = 1.0 / leff / coxeffwovl;
        t11 = t12 / vgsteff;
        t10 = -t11 / vgsteff;
        vs = cdrain * t11;
        dvs_dvg = gm * t11 + cdrain * t10 * dvgsteff_dvg;
        dvs_dvd = gds * t11 + cdrain * t10 * dvgsteff_dvd;
        dvs_dvb = gmb * t11 + cdrain * t10 * dvgsteff_dvb;
        t0 = 2.0 * MM;
        t1 = vs / (pp.vtl * pp.tfactor);
        if t1 > 0.0 {
            t2 = 1.0 + exp(t0 * log(t1));
            t3 = (t2 - 1.0) * t0 / vs;
            fsevl = 1.0 / exp(log(t2) / t0);
            dt2_dvg = t3 * dvs_dvg;
            dt2_dvd = t3 * dvs_dvd;
            dt2_dvb = t3 * dvs_dvb;
            t4 = -1.0 / t0 * fsevl / t2;
            dfsevl_dvg = t4 * dt2_dvg;
            dfsevl_dvd = t4 * dt2_dvd;
            dfsevl_dvb = t4 * dt2_dvb;
        } else {
            fsevl = 1.0;
            dfsevl_dvg = 0.0;
            dfsevl_dvd = 0.0;
            dfsevl_dvb = 0.0;
        }
        gm *= fsevl;
        gm += cdrain * dfsevl_dvg;
        gmb *= fsevl;
        gmb += cdrain * dfsevl_dvb;
        gds *= fsevl;
        gds += cdrain * dfsevl_dvd;
        cdrain *= fsevl;
    }
    h.gds = gds;
    h.gm = gm;
    h.gmbs = gmb;
    h.idov_vds = ids;
    if h.idov_vds <= md.idovvdsc {
        h.idov_vds = md.idovvdsc;
    }
    if h.rgate_mod > 1 || h.trnqs_mod != 0 || h.acnqs_mod != 0 {
        t9 = pp.xrcrg2 * md.vtm;
        t0 = t9 * beta;
        dt0_dvd = (dbeta_dvd + dbeta_dvg * dvgsteff_dvd) * t9;
        dt0_dvb = (dbeta_dvb + dbeta_dvg * dvgsteff_dvb) * t9;
        dt0_dvg = dbeta_dvg * t9;
        h.gcrg = pp.xrcrg1 * (t0 + ids);
        h.gcrgd = pp.xrcrg1 * (dt0_dvd + tmp1);
        h.gcrgb = pp.xrcrg1 * (dt0_dvb + tmp2) * dvbseff_dvb;
        h.gcrgg = pp.xrcrg1 * (dt0_dvg + tmp3) * dvgsteff_dvg;
        if h.nf != 1.0 {
            h.gcrg *= h.nf;
            h.gcrgg *= h.nf;
            h.gcrgd *= h.nf;
            h.gcrgb *= h.nf;
        }
        if h.rgate_mod == 2 {
            t10 = h.grgeltd * h.grgeltd;
            t11 = h.grgeltd + h.gcrg;
            h.gcrg = h.grgeltd * h.gcrg / t11;
            t12 = t10 / t11 / t11;
            h.gcrgg *= t12;
            h.gcrgd *= t12;
            h.gcrgb *= t12;
        }
        h.gcrgs = -(h.gcrgg + h.gcrgd + h.gcrgb);
    }
    if md.rds_mod != 0 {
        t0 = vgs - pp.vfbsd;
        t1 = sqrt(t0 * t0 + 1.0e-4);
        vgs_eff = 0.5 * (t0 + t1);
        dvgs_eff_dvg = vgs_eff / t1;
        t0 = 1.0 + pp.prwg * vgs_eff;
        dt0_dvg_ = -pp.prwg / t0 / t0 * dvgs_eff_dvg;
        t1 = -pp.prwb * vbs;
        dt1_dvb_ = -pp.prwb;
        t2 = 1.0 / t0 + t1;
        t3 = t2 + sqrt(t2 * t2 + 0.01);
        dt3_dvg_ = t3 / (t3 - t2);
        dt3_dvb_ = dt3_dvg_ * dt1_dvb_;
        dt3_dvg_ *= dt0_dvg_;
        t4 = pp.rs0 * 0.5;
        rs = pp.rswmin + t3 * t4;
        drs_dvg = t4 * dt3_dvg_;
        drs_dvb = t4 * dt3_dvb_;
        t0 = 1.0 + h.source_conductance * rs;
        h.gstot = h.source_conductance / t0;
        t0 = -h.gstot * h.gstot;
        dgstot_dvd = 0.0;
        dgstot_dvg = t0 * drs_dvg;
        dgstot_dvb = t0 * drs_dvb;
        dgstot_dvs = -(dgstot_dvg + dgstot_dvb + dgstot_dvd);
        t0 = vgd - pp.vfbsd;
        t1 = sqrt(t0 * t0 + 1.0e-4);
        vgd_eff = 0.5 * (t0 + t1);
        dvgd_eff_dvg = vgd_eff / t1;
        t0 = 1.0 + pp.prwg * vgd_eff;
        dt0_dvg_ = -pp.prwg / t0 / t0 * dvgd_eff_dvg;
        t1 = -pp.prwb * vbd;
        dt1_dvb_ = -pp.prwb;
        t2 = 1.0 / t0 + t1;
        t3 = t2 + sqrt(t2 * t2 + 0.01);
        dt3_dvg_ = t3 / (t3 - t2);
        dt3_dvb_ = dt3_dvg_ * dt1_dvb_;
        dt3_dvg_ *= dt0_dvg_;
        t4 = pp.rd0 * 0.5;
        rd = pp.rdwmin + t3 * t4;
        drd_dvg = t4 * dt3_dvg_;
        drd_dvb = t4 * dt3_dvb_;
        t0 = 1.0 + h.drain_conductance * rd;
        h.gdtot = h.drain_conductance / t0;
        t0 = -h.gdtot * h.gdtot;
        dgdtot_dvs = 0.0;
        dgdtot_dvg = t0 * drd_dvg;
        dgdtot_dvb = t0 * drd_dvb;
        dgdtot_dvd = -(dgdtot_dvg + dgdtot_dvb + dgdtot_dvs);
        h.gstotd = vses * dgstot_dvd;
        h.gstotg = vses * dgstot_dvg;
        h.gstots = vses * dgstot_dvs;
        h.gstotb = vses * dgstot_dvb;
        t2 = vdes - vds;
        h.gdtotd = t2 * dgdtot_dvd;
        h.gdtotg = t2 * dgdtot_dvg;
        h.gdtots = t2 * dgdtot_dvs;
        h.gdtotb = t2 * dgdtot_dvb;
    } else {
        h.gstotg = 0.0;
        h.gstotd = h.gstotg;
        h.gstot = h.gstotd;
        h.gstotb = 0.0;
        h.gstots = h.gstotb;
        h.gdtotg = 0.0;
        h.gdtotd = h.gdtotg;
        h.gdtot = h.gdtotd;
        h.gdtotb = 0.0;
        h.gdtots = h.gdtotb;
    }
    if md.mtrl_mod == 0 {
        t0 = 3.0 * toxe;
    } else {
        t0 = md.epsrsub * toxe / epsrox;
    }
    vgs_eff = h.vgs_eff;
    dvgs_eff_dvg = h.dvgs_eff_dvg;
    vgd_eff = h.vgd_eff;
    dvgd_eff_dvg = h.dvgd_eff_dvg;
    if md.gidl_mod == 0 {
        if md.mtrl_mod == 0 {
            t1 = (vds - vgs_eff - pp.egidl) / t0;
        } else {
            t1 = (vds - vgs_eff - pp.egidl + pp.vfbsd) / t0;
        }
        if pp.agidl <= 0.0 || pp.bgidl <= 0.0 || t1 <= 0.0 || pp.cgidl <= 0.0 || vbd > 0.0 {
            ggidlb_ = 0.0;
            ggidlg_ = ggidlb_;
            ggidld_ = ggidlg_;
            igidl = ggidld_;
        } else {
            dt1_dvd = 1.0 / t0;
            dt1_dvg = -dvgs_eff_dvg * dt1_dvd;
            t2 = pp.bgidl / t1;
            if t2 < 100.0 {
                igidl = pp.agidl * pp.weff_cj * t1 * exp(-t2);
                t3 = igidl * (1.0 + t2) / t1;
                ggidld_ = t3 * dt1_dvd;
                ggidlg_ = t3 * dt1_dvg;
            } else {
                igidl = pp.agidl * pp.weff_cj * 3.720075976e-44;
                ggidld_ = igidl * dt1_dvd;
                ggidlg_ = igidl * dt1_dvg;
                igidl *= t1;
            }
            t4 = vbd * vbd;
            t5 = -vbd * t4;
            t6 = pp.cgidl + t5;
            t7 = t5 / t6;
            t8 = 3.0 * pp.cgidl * t4 / t6 / t6;
            ggidld_ = ggidld_ * t7 + igidl * t8;
            ggidlg_ *= t7;
            ggidlb_ = -igidl * t8;
            igidl *= t7;
        }
        h.igidl = igidl;
        h.ggidld = ggidld_;
        h.ggidlg = ggidlg_;
        h.ggidlb = ggidlb_;
        if md.mtrl_mod == 0 {
            t1 = (-vds - vgd_eff - pp.egisl) / t0;
        } else {
            t1 = (-vds - vgd_eff - pp.egisl + pp.vfbsd) / t0;
        }
        if pp.agisl <= 0.0 || pp.bgisl <= 0.0 || t1 <= 0.0 || pp.cgisl <= 0.0 || vbs > 0.0 {
            ggislb_ = 0.0;
            ggislg_ = ggislb_;
            ggisls_ = ggislg_;
            igisl = ggisls_;
        } else {
            dt1_dvd = 1.0 / t0;
            dt1_dvg = -dvgd_eff_dvg * dt1_dvd;
            t2 = pp.bgisl / t1;
            if t2 < 100.0 {
                igisl = pp.agisl * pp.weff_cj * t1 * exp(-t2);
                t3 = igisl * (1.0 + t2) / t1;
                ggisls_ = t3 * dt1_dvd;
                ggislg_ = t3 * dt1_dvg;
            } else {
                igisl = pp.agisl * pp.weff_cj * 3.720075976e-44;
                ggisls_ = igisl * dt1_dvd;
                ggislg_ = igisl * dt1_dvg;
                igisl *= t1;
            }
            t4 = vbs * vbs;
            t5 = -vbs * t4;
            t6 = pp.cgisl + t5;
            t7 = t5 / t6;
            t8 = 3.0 * pp.cgisl * t4 / t6 / t6;
            ggisls_ = ggisls_ * t7 + igisl * t8;
            ggislg_ *= t7;
            ggislb_ = -igisl * t8;
            igisl *= t7;
        }
        h.igisl = igisl;
        h.ggisls = ggisls_;
        h.ggislg = ggislg_;
        h.ggislb = ggislb_;
    } else {
        if md.mtrl_mod == 0 {
            t1 = (-vds - pp.rgisl * vgd_eff - pp.egisl) / t0;
        } else {
            t1 = (-vds - pp.rgisl * vgd_eff - pp.egisl + pp.vfbsd) / t0;
        }
        if pp.agisl <= 0.0 || pp.bgisl <= 0.0 || t1 <= 0.0 || pp.cgisl < 0.0 {
            ggislb_ = 0.0;
            ggislg_ = ggislb_;
            ggisls_ = ggislg_;
            igisl = ggisls_;
        } else {
            dt1_dvd = 1.0 / t0;
            dt1_dvg = -pp.rgisl * dt1_dvd * dvgd_eff_dvg;
            t2 = pp.bgisl / t1;
            if t2 < EXPL_THRESHOLD {
                igisl = pp.weff_cj * pp.agisl * t1 * exp(-t2);
                t3 = igisl / t1 * (t2 + 1.0);
                ggisls_ = t3 * dt1_dvd;
                ggislg_ = t3 * dt1_dvg;
            } else {
                t3 = pp.weff_cj * pp.agisl * MIN_EXPL;
                igisl = t3 * t1;
                ggisls_ = t3 * dt1_dvd;
                ggislg_ = t3 * dt1_dvg;
            }
            t4 = vbs - pp.fgisl;
            if t4 > md.gidlclamp {
                t4 = md.gidlclamp;
            }
            if t4 == 0.0 {
                t5 = EXPL_THRESHOLD;
            } else {
                t5 = pp.kgisl / t4;
            }
            if t5 < EXPL_THRESHOLD {
                t6 = exp(t5);
                ggislb_ = -igisl * t6 * t5 / t4;
            } else {
                t6 = MAX_EXPL;
                ggislb_ = 0.0;
            }
            ggisls_ *= t6;
            ggislg_ *= t6;
            igisl *= t6;
        }
        h.igisl = igisl;
        h.ggisls = ggisls_;
        h.ggislg = ggislg_;
        h.ggislb = ggislb_;
        if md.mtrl_mod == 0 {
            t1 = (vds - pp.rgidl * vgs_eff - pp.egidl) / t0;
        } else {
            t1 = (vds - pp.rgidl * vgs_eff - pp.egidl + pp.vfbsd) / t0;
        }
        if pp.agidl <= 0.0 || pp.bgidl <= 0.0 || t1 <= 0.0 || pp.cgidl < 0.0 {
            ggidlb_ = 0.0;
            ggidlg_ = ggidlb_;
            ggidld_ = ggidlg_;
            igidl = ggidld_;
        } else {
            dt1_dvd = 1.0 / t0;
            dt1_dvg = -pp.rgidl * dt1_dvd * dvgs_eff_dvg;
            t2 = pp.bgidl / t1;
            if t2 < EXPL_THRESHOLD {
                igidl = pp.weff_cj * pp.agidl * t1 * exp(-t2);
                t3 = igidl / t1 * (t2 + 1.0);
                ggidld_ = t3 * dt1_dvd;
                ggidlg_ = t3 * dt1_dvg;
            } else {
                t3 = pp.weff_cj * pp.agidl * MIN_EXPL;
                igidl = t3 * t1;
                ggidld_ = t3 * dt1_dvd;
                ggidlg_ = t3 * dt1_dvg;
            }
            t4 = vbd - pp.fgidl;
            if t4 > md.gidlclamp {
                t4 = md.gidlclamp;
            }
            if t4 == 0.0 {
                t5 = EXPL_THRESHOLD;
            } else {
                t5 = pp.kgidl / t4;
            }
            if t5 < EXPL_THRESHOLD {
                t6 = exp(t5);
                ggidlb_ = -igidl * t6 * t5 / t4;
            } else {
                t6 = MAX_EXPL;
                ggidlb_ = 0.0;
            }
            ggidld_ *= t6;
            ggidlg_ *= t6;
            igidl *= t6;
        }
        h.igidl = igidl;
        h.ggidld = ggidld_;
        h.ggidlg = ggidlg_;
        h.ggidlb = ggidlb_;
    }
    if md.igc_mod != 0 || md.igb_mod != 0 {
        vfb = h.vfbzb;
        v3 = vfb - vgs_eff_ + vbseff - DELTA_3;
        if vfb <= 0.0 {
            t0 = sqrt(v3 * v3 - 4.0 * DELTA_3 * vfb);
        } else {
            t0 = sqrt(v3 * v3 + 4.0 * DELTA_3 * vfb);
        }
        t1 = 0.5 * (1.0 + v3 / t0);
        vfbeff = vfb - 0.5 * (v3 + t0);
        dvfbeff_dvg = t1 * dvgs_eff_dvg_;
        dvfbeff_dvb = -t1;
        voxacc = vfb - vfbeff;
        dvoxacc_dvg = -dvfbeff_dvg;
        dvoxacc_dvb = -dvfbeff_dvb;
        if voxacc < 0.0 {
            dvoxacc_dvb = 0.0;
            dvoxacc_dvg = dvoxacc_dvb;
            voxacc = dvoxacc_dvg;
        }
        t0 = 0.5 * pp.k1ox;
        t3 = vgs_eff_ - vfbeff - vbseff - vgsteff;
        if pp.k1ox == 0.0 {
            dvoxdepinv_dvb = 0.0;
            dvoxdepinv_dvd = dvoxdepinv_dvb;
            dvoxdepinv_dvg = dvoxdepinv_dvd;
            voxdepinv = dvoxdepinv_dvg;
        } else if t3 < 0.0 {
            voxdepinv = -t3;
            dvoxdepinv_dvg = -dvgs_eff_dvg_ + dvfbeff_dvg + dvgsteff_dvg;
            dvoxdepinv_dvd = dvgsteff_dvd;
            dvoxdepinv_dvb = dvfbeff_dvb + 1.0 + dvgsteff_dvb;
        } else {
            t1 = sqrt(t0 * t0 + t3);
            t2 = t0 / t1;
            voxdepinv = pp.k1ox * (t1 - t0);
            dvoxdepinv_dvg = t2 * (dvgs_eff_dvg_ - dvfbeff_dvg - dvgsteff_dvg);
            dvoxdepinv_dvd = -t2 * dvgsteff_dvd;
            dvoxdepinv_dvb = -t2 * (dvfbeff_dvb + 1.0 + dvgsteff_dvb);
        }
        voxdepinv += vgsteff;
        dvoxdepinv_dvg += dvgsteff_dvg;
        dvoxdepinv_dvd += dvgsteff_dvd;
        dvoxdepinv_dvb += dvgsteff_dvb;
    }
    if md.temp_mod < 2 {
        tmp = vtm;
    } else {
        tmp = vtm0;
    }
    if md.igc_mod != 0 {
        t0 = tmp * pp.nigc;
        if md.igc_mod == 1 {
            vxnvt = (vgs_eff_ - md.type_ as f64 * h.vth0) / t0;
            if vxnvt > EXP_THRESHOLD {
                vaux = vgs_eff_ - md.type_ as f64 * h.vth0;
                dvaux_dvg = dvgs_eff_dvg_;
                dvaux_dvd = 0.0;
                dvaux_dvb = 0.0;
            }
        } else if md.igc_mod == 2 {
            vxnvt = (vgs_eff_ - h.von) / t0;
            if vxnvt > EXP_THRESHOLD {
                vaux = vgs_eff_ - h.von;
                dvaux_dvg = dvgs_eff_dvg_;
                dvaux_dvd = -dvth_dvd;
                dvaux_dvb = -dvth_dvb;
            }
        }
        if vxnvt < -EXP_THRESHOLD {
            vaux = t0 * log(1.0 + MIN_EXP);
            dvaux_dvb = 0.0;
            dvaux_dvd = dvaux_dvb;
            dvaux_dvg = dvaux_dvd;
        } else if (-EXP_THRESHOLD..=EXP_THRESHOLD).contains(&vxnvt) {
            expvxnvt = exp(vxnvt);
            vaux = t0 * log(1.0 + expvxnvt);
            dvaux_dvg = expvxnvt / (1.0 + expvxnvt);
            if md.igc_mod == 1 {
                dvaux_dvd = 0.0;
                dvaux_dvb = 0.0;
            } else if md.igc_mod == 2 {
                dvaux_dvd = -dvaux_dvg * dvth_dvd;
                dvaux_dvb = -dvaux_dvg * dvth_dvb;
            }
            dvaux_dvg *= dvgs_eff_dvg_;
        }
        t2 = vgs_ * vaux;
        dt2_dvg = vaux + vgs_ * dvaux_dvg;
        dt2_dvd = vgs_ * dvaux_dvd;
        dt2_dvb = vgs_ * dvaux_dvb;
        t11 = pp.aechvb;
        t12 = pp.bechvb;
        t3 = pp.aigc * pp.cigc - pp.bigc;
        t4 = pp.bigc * pp.cigc;
        t5 = t12 * (pp.aigc + t3 * voxdepinv - t4 * voxdepinv * voxdepinv);
        if t5 > EXP_THRESHOLD {
            t6 = MAX_EXP;
            dt6_dvb = 0.0;
            dt6_dvd = dt6_dvb;
            dt6_dvg = dt6_dvd;
        } else if t5 < -EXP_THRESHOLD {
            t6 = MIN_EXP;
            dt6_dvb = 0.0;
            dt6_dvd = dt6_dvb;
            dt6_dvg = dt6_dvd;
        } else {
            t6 = exp(t5);
            dt6_dvg = t6 * t12 * (t3 - 2.0 * t4 * voxdepinv);
            dt6_dvd = dt6_dvg * dvoxdepinv_dvd;
            dt6_dvb = dt6_dvg * dvoxdepinv_dvb;
            dt6_dvg *= dvoxdepinv_dvg;
        }
        igc = t11 * t2 * t6;
        digc_dvg = t11 * (t2 * dt6_dvg + t6 * dt2_dvg);
        digc_dvd = t11 * (t2 * dt6_dvd + t6 * dt2_dvd);
        digc_dvb = t11 * (t2 * dt6_dvb + t6 * dt2_dvb);
        if md.given.pigcd {
            pigcd = pp.pigcd;
            dpigcd_dvb = 0.0;
            dpigcd_dvd = dpigcd_dvb;
            dpigcd_dvg = dpigcd_dvd;
        } else {
            t11 = -pp.bechvb;
            t12 = vgsteff + 1.0e-20;
            t13 = t11 / t12 / t12;
            t14 = -t13 / t12;
            pigcd = t13 * (1.0 - 0.5 * vdseff / t12);
            dpigcd_dvg = t14 * (2.0 + 0.5 * (dvdseff_dvg - 3.0 * vdseff / t12));
            dpigcd_dvd = 0.5 * t14 * dvdseff_dvd;
            dpigcd_dvb = 0.5 * t14 * dvdseff_dvb;
        }
        t7 = -pigcd * vdseff;
        dt7_dvg = -vdseff * dpigcd_dvg - pigcd * dvdseff_dvg;
        dt7_dvd = -vdseff * dpigcd_dvd - pigcd * dvdseff_dvd + dt7_dvg * dvgsteff_dvd;
        dt7_dvb = -vdseff * dpigcd_dvb - pigcd * dvdseff_dvb + dt7_dvg * dvgsteff_dvb;
        dt7_dvg *= dvgsteff_dvg;
        t8 = t7 * t7 + 2.0e-4;
        dt8_dvg = 2.0 * t7;
        dt8_dvd = dt8_dvg * dt7_dvd;
        dt8_dvb = dt8_dvg * dt7_dvb;
        dt8_dvg *= dt7_dvg;
        if t7 > EXP_THRESHOLD {
            t9 = MAX_EXP;
            dt9_dvb = 0.0;
            dt9_dvd = dt9_dvb;
            dt9_dvg = dt9_dvd;
        } else if t7 < -EXP_THRESHOLD {
            t9 = MIN_EXP;
            dt9_dvb = 0.0;
            dt9_dvd = dt9_dvb;
            dt9_dvg = dt9_dvd;
        } else {
            t9 = exp(t7);
            dt9_dvg = t9 * dt7_dvg;
            dt9_dvd = t9 * dt7_dvd;
            dt9_dvb = t9 * dt7_dvb;
        }
        t0 = t8 * t8;
        t1 = t9 - 1.0 + 1.0e-4;
        t10 = (t1 - t7) / t8;
        dt10_dvg = (dt9_dvg - dt7_dvg - t10 * dt8_dvg) / t8;
        dt10_dvd = (dt9_dvd - dt7_dvd - t10 * dt8_dvd) / t8;
        dt10_dvb = (dt9_dvb - dt7_dvb - t10 * dt8_dvb) / t8;
        igcs = igc * t10;
        digcs_dvg = digc_dvg * t10 + igc * dt10_dvg;
        digcs_dvd = digc_dvd * t10 + igc * dt10_dvd;
        digcs_dvb = digc_dvb * t10 + igc * dt10_dvb;
        t1 = t9 - 1.0 - 1.0e-4;
        t10 = (t7 * t9 - t1) / t8;
        dt10_dvg = (dt7_dvg * t9 + (t7 - 1.0) * dt9_dvg - t10 * dt8_dvg) / t8;
        dt10_dvd = (dt7_dvd * t9 + (t7 - 1.0) * dt9_dvd - t10 * dt8_dvd) / t8;
        dt10_dvb = (dt7_dvb * t9 + (t7 - 1.0) * dt9_dvb - t10 * dt8_dvb) / t8;
        igcd = igc * t10;
        digcd_dvg = digc_dvg * t10 + igc * dt10_dvg;
        digcd_dvd = digc_dvd * t10 + igc * dt10_dvd;
        digcd_dvb = digc_dvb * t10 + igc * dt10_dvb;
        h.igcs = igcs;
        h.g_igcsg = digcs_dvg;
        h.g_igcsd = digcs_dvd;
        h.g_igcsb = digcs_dvb * dvbseff_dvb;
        h.igcd = igcd;
        h.g_igcdg = digcd_dvg;
        h.g_igcdd = digcd_dvd;
        h.g_igcdb = digcd_dvb * dvbseff_dvb;
        t0 = vgs - (pp.vfbsd + pp.vfbsdoff);
        vgs_eff = sqrt(t0 * t0 + 1.0e-4);
        dvgs_eff_dvg = t0 / vgs_eff;
        t2 = vgs * vgs_eff;
        dt2_dvg = vgs * dvgs_eff_dvg + vgs_eff;
        t11 = pp.aechvb_edge_s;
        t12 = pp.bechvb_edge;
        t3 = pp.aigs * pp.cigs - pp.bigs;
        t4 = pp.bigs * pp.cigs;
        t5 = t12 * (pp.aigs + t3 * vgs_eff - t4 * vgs_eff * vgs_eff);
        if t5 > EXP_THRESHOLD {
            t6 = MAX_EXP;
            dt6_dvg = 0.0;
        } else if t5 < -EXP_THRESHOLD {
            t6 = MIN_EXP;
            dt6_dvg = 0.0;
        } else {
            t6 = exp(t5);
            dt6_dvg = t6 * t12 * (t3 - 2.0 * t4 * vgs_eff) * dvgs_eff_dvg;
        }
        igs = t11 * t2 * t6;
        digs_dvg = t11 * (t2 * dt6_dvg + t6 * dt2_dvg);
        digs_dvs = -digs_dvg;
        t0 = vgd - (pp.vfbsd + pp.vfbsdoff);
        vgd_eff = sqrt(t0 * t0 + 1.0e-4);
        dvgd_eff_dvg = t0 / vgd_eff;
        t2 = vgd * vgd_eff;
        dt2_dvg = vgd * dvgd_eff_dvg + vgd_eff;
        t11 = pp.aechvb_edge_d;
        t3 = pp.aigd * pp.cigd - pp.bigd;
        t4 = pp.bigd * pp.cigd;
        t5 = t12 * (pp.aigd + t3 * vgd_eff - t4 * vgd_eff * vgd_eff);
        if t5 > EXP_THRESHOLD {
            t6 = MAX_EXP;
            dt6_dvg = 0.0;
        } else if t5 < -EXP_THRESHOLD {
            t6 = MIN_EXP;
            dt6_dvg = 0.0;
        } else {
            t6 = exp(t5);
            dt6_dvg = t6 * t12 * (t3 - 2.0 * t4 * vgd_eff) * dvgd_eff_dvg;
        }
        igd = t11 * t2 * t6;
        digd_dvg = t11 * (t2 * dt6_dvg + t6 * dt2_dvg);
        digd_dvd = -digd_dvg;
        h.igs = igs;
        h.g_igsg = digs_dvg;
        h.g_igss = digs_dvs;
        h.igd = igd;
        h.g_igdg = digd_dvg;
        h.g_igdd = digd_dvd;
    } else {
        h.g_igcsb = 0.0;
        h.g_igcsd = h.g_igcsb;
        h.g_igcsg = h.g_igcsd;
        h.igcs = h.g_igcsg;
        h.g_igcdb = 0.0;
        h.g_igcdd = h.g_igcdb;
        h.g_igcdg = h.g_igcdd;
        h.igcd = h.g_igcdg;
        h.g_igss = 0.0;
        h.g_igsg = h.g_igss;
        h.igs = h.g_igsg;
        h.g_igdd = 0.0;
        h.g_igdg = h.g_igdd;
        h.igd = h.g_igdg;
    }
    if md.igb_mod != 0 {
        t0 = tmp * pp.nigbacc;
        t1 = -vgs_eff_ + vbseff + vfb;
        vxnvt = t1 / t0;
        if vxnvt > EXP_THRESHOLD {
            vaux = t1;
            dvaux_dvg = -dvgs_eff_dvg_;
            dvaux_dvb = 1.0;
        } else if vxnvt < -EXP_THRESHOLD {
            vaux = t0 * log(1.0 + MIN_EXP);
            dvaux_dvb = 0.0;
            dvaux_dvg = dvaux_dvb;
        } else {
            expvxnvt = exp(vxnvt);
            vaux = t0 * log(1.0 + expvxnvt);
            dvaux_dvb = expvxnvt / (1.0 + expvxnvt);
            dvaux_dvg = -dvaux_dvb * dvgs_eff_dvg_;
        }
        t2 = (vgs_ - vbs_) * vaux;
        dt2_dvg = vaux + (vgs_ - vbs_) * dvaux_dvg;
        dt2_dvb = -vaux + (vgs_ - vbs_) * dvaux_dvb;
        t11 = 4.97232e-7 * pp.weff * pp.leff * pp.tox_ratio;
        t12 = -7.45669e11 * toxe;
        t3 = pp.aigbacc * pp.cigbacc - pp.bigbacc;
        t4 = pp.bigbacc * pp.cigbacc;
        t5 = t12 * (pp.aigbacc + t3 * voxacc - t4 * voxacc * voxacc);
        if t5 > EXP_THRESHOLD {
            t6 = MAX_EXP;
            dt6_dvb = 0.0;
            dt6_dvg = dt6_dvb;
        } else if t5 < -EXP_THRESHOLD {
            t6 = MIN_EXP;
            dt6_dvb = 0.0;
            dt6_dvg = dt6_dvb;
        } else {
            t6 = exp(t5);
            dt6_dvg = t6 * t12 * (t3 - 2.0 * t4 * voxacc);
            dt6_dvb = dt6_dvg * dvoxacc_dvb;
            dt6_dvg *= dvoxacc_dvg;
        }
        igbacc = t11 * t2 * t6;
        digbacc_dvg = t11 * (t2 * dt6_dvg + t6 * dt2_dvg);
        digbacc_dvb = t11 * (t2 * dt6_dvb + t6 * dt2_dvb);
        t0 = tmp * pp.nigbinv;
        t1 = voxdepinv - pp.eigbinv;
        vxnvt = t1 / t0;
        if vxnvt > EXP_THRESHOLD {
            vaux = t1;
            dvaux_dvg = dvoxdepinv_dvg;
            dvaux_dvd = dvoxdepinv_dvd;
            dvaux_dvb = dvoxdepinv_dvb;
        } else if vxnvt < -EXP_THRESHOLD {
            vaux = t0 * log(1.0 + MIN_EXP);
            dvaux_dvb = 0.0;
            dvaux_dvd = dvaux_dvb;
            dvaux_dvg = dvaux_dvd;
        } else {
            expvxnvt = exp(vxnvt);
            vaux = t0 * log(1.0 + expvxnvt);
            dvaux_dvg = expvxnvt / (1.0 + expvxnvt);
            dvaux_dvd = dvaux_dvg * dvoxdepinv_dvd;
            dvaux_dvb = dvaux_dvg * dvoxdepinv_dvb;
            dvaux_dvg *= dvoxdepinv_dvg;
        }
        t2 = (vgs_ - vbs_) * vaux;
        dt2_dvg = vaux + (vgs_ - vbs_) * dvaux_dvg;
        dt2_dvd = (vgs_ - vbs_) * dvaux_dvd;
        dt2_dvb = -vaux + (vgs_ - vbs_) * dvaux_dvb;
        t11 *= 0.75610;
        t12 *= 1.31724;
        t3 = pp.aigbinv * pp.cigbinv - pp.bigbinv;
        t4 = pp.bigbinv * pp.cigbinv;
        t5 = t12 * (pp.aigbinv + t3 * voxdepinv - t4 * voxdepinv * voxdepinv);
        if t5 > EXP_THRESHOLD {
            t6 = MAX_EXP;
            dt6_dvb = 0.0;
            dt6_dvd = dt6_dvb;
            dt6_dvg = dt6_dvd;
        } else if t5 < -EXP_THRESHOLD {
            t6 = MIN_EXP;
            dt6_dvb = 0.0;
            dt6_dvd = dt6_dvb;
            dt6_dvg = dt6_dvd;
        } else {
            t6 = exp(t5);
            dt6_dvg = t6 * t12 * (t3 - 2.0 * t4 * voxdepinv);
            dt6_dvd = dt6_dvg * dvoxdepinv_dvd;
            dt6_dvb = dt6_dvg * dvoxdepinv_dvb;
            dt6_dvg *= dvoxdepinv_dvg;
        }
        igbinv = t11 * t2 * t6;
        digbinv_dvg = t11 * (t2 * dt6_dvg + t6 * dt2_dvg);
        digbinv_dvd = t11 * (t2 * dt6_dvd + t6 * dt2_dvd);
        digbinv_dvb = t11 * (t2 * dt6_dvb + t6 * dt2_dvb);
        h.igb = igbinv + igbacc;
        h.g_igbg = digbinv_dvg + digbacc_dvg;
        h.g_igbd = digbinv_dvd;
        h.g_igbb = (digbinv_dvb + digbacc_dvb) * dvbseff_dvb;
    } else {
        h.g_igbb = 0.0;
        h.g_igbs = h.g_igbb;
        h.g_igbd = h.g_igbs;
        h.g_igbg = h.g_igbd;
        h.igb = h.g_igbg;
    }
    if h.nf != 1.0 {
        cdrain *= h.nf;
        h.gds *= h.nf;
        h.gm *= h.nf;
        h.gmbs *= h.nf;
        h.idov_vds *= h.nf;
        h.gbbs *= h.nf;
        h.gbgs *= h.nf;
        h.gbds *= h.nf;
        h.csub *= h.nf;
        h.igidl *= h.nf;
        h.ggidld *= h.nf;
        h.ggidlg *= h.nf;
        h.ggidlb *= h.nf;
        h.igisl *= h.nf;
        h.ggisls *= h.nf;
        h.ggislg *= h.nf;
        h.ggislb *= h.nf;
        h.igcs *= h.nf;
        h.g_igcsg *= h.nf;
        h.g_igcsd *= h.nf;
        h.g_igcsb *= h.nf;
        h.igcd *= h.nf;
        h.g_igcdg *= h.nf;
        h.g_igcdd *= h.nf;
        h.g_igcdb *= h.nf;
        h.igs *= h.nf;
        h.g_igsg *= h.nf;
        h.g_igss *= h.nf;
        h.igd *= h.nf;
        h.g_igdg *= h.nf;
        h.g_igdd *= h.nf;
        h.igb *= h.nf;
        h.g_igbg *= h.nf;
        h.g_igbd *= h.nf;
        h.g_igbb *= h.nf;
    }
    h.ggidls = -(h.ggidld + h.ggidlg + h.ggidlb);
    h.ggisld = -(h.ggisls + h.ggislg + h.ggislb);
    h.g_igbs = -(h.g_igbg + h.g_igbd + h.g_igbb);
    h.g_igcss = -(h.g_igcsg + h.g_igcsd + h.g_igcsb);
    h.g_igcds = -(h.g_igcdg + h.g_igcdd + h.g_igcdb);
    h.cd = cdrain;
    if md.tnoi_mod == 0 {
        abulk = abulk0_q * pp.abulk_cvfactor;
        vdsat = vgsteff / abulk;
        t0 = vdsat - vds_ - DELTA_4;
        t1 = sqrt(t0 * t0 + 4.0 * DELTA_4 * vdsat);
        if t0 >= 0.0 {
            vdseff = vdsat - 0.5 * (t0 + t1);
        } else {
            t3 = (DELTA_4 + DELTA_4) / (t1 - t0);
            t4 = 1.0 - t3;
            t5 = vdsat * t3 / (t1 - t0);
            vdseff = vdsat * t4;
        }
        if vds_ == 0.0 {
            vdseff = 0.0;
        }
        t0 = abulk * vdseff;
        t1 = 12.0 * (vgsteff - 0.5 * t0 + 1.0e-20);
        t2 = vdseff / t1;
        t3 = t0 * t2;
        h.qinv = coxeff * pp.weff_cv * h.nf * pp.leff_cv * (vgsteff - 0.5 * t0 + abulk * t3);
    } else if md.tnoi_mod == 2 {
        h.noi_gd0 = h.nf * beta * vgsteff / (1.0 + gche * rds);
    }
    if md.xpart < 0.0 || charge_computation_needed == 0 {
        qbulk = 0.0;
        qsrc = qbulk;
        qdrn = qsrc;
        qgate = qdrn;
        h.cgdb = 0.0;
        h.cgsb = h.cgdb;
        h.cggb = h.cgsb;
        h.cddb = 0.0;
        h.cdsb = h.cddb;
        h.cdgb = h.cdsb;
        h.cbdb = 0.0;
        h.cbsb = h.cbdb;
        h.cbgb = h.cbsb;
        h.csdb = 0.0;
        h.cssb = h.csdb;
        h.csgb = h.cssb;
        h.cbbb = 0.0;
        h.cdbb = h.cbbb;
        h.csbb = h.cdbb;
        h.cgbb = h.csbb;
        h.cqbb = 0.0;
        h.cqgb = h.cqbb;
        h.cqsb = h.cqgb;
        h.cqdb = h.cqsb;
        h.gtau = 0.0;
        goto_finished = 1;
    } else if md.cap_mod == 0 {
        if vbseff < 0.0 {
            vbseffcv = vbs_;
            dvbseffcv_dvb = 1.0;
        } else {
            vbseffcv = pp.phi - phis;
            dvbseffcv_dvb = -dphis_dvb * dvbseff_dvb;
        }
        vfb = pp.vfbcv;
        vth = vfb + pp.phi + pp.k1ox * sqrtphis;
        vgst = vgs_eff_ - vth;
        dvth_dvb = pp.k1ox * dsqrtphis_dvb * dvbseff_dvb;
        _dvgst_dvb = -dvth_dvb;
        _dvgst_dvg = dvgs_eff_dvg_;
        coxwl = md.coxe * pp.weff_cv * pp.leff_cv * h.nf;
        arg1 = vgs_eff_ - vbseffcv - vfb;
        if arg1 <= 0.0 {
            qgate = coxwl * arg1;
            qbulk = -qgate;
            qdrn = 0.0;
            h.cggb = coxwl * dvgs_eff_dvg_;
            h.cgdb = 0.0;
            h.cgsb = coxwl * (dvbseffcv_dvb - dvgs_eff_dvg_);
            h.cdgb = 0.0;
            h.cddb = 0.0;
            h.cdsb = 0.0;
            h.cbgb = -coxwl * dvgs_eff_dvg_;
            h.cbdb = 0.0;
            h.cbsb = -h.cgsb;
        } else if vgst <= 0.0 {
            t1 = 0.5 * pp.k1ox;
            t2 = sqrt(t1 * t1 + arg1);
            qgate = coxwl * pp.k1ox * (t2 - t1);
            qbulk = -qgate;
            qdrn = 0.0;
            t0 = coxwl * t1 / t2;
            h.cggb = t0 * dvgs_eff_dvg_;
            h.cgdb = 0.0;
            h.cgsb = t0 * (dvbseffcv_dvb - dvgs_eff_dvg_);
            h.cdgb = 0.0;
            h.cddb = 0.0;
            h.cdsb = 0.0;
            h.cbgb = -h.cggb;
            h.cbdb = 0.0;
            h.cbsb = -h.cgsb;
        } else {
            one_third_coxwl = coxwl / 3.0;
            two_third_coxwl = 2.0 * one_third_coxwl;
            abulkcv = abulk0_q * pp.abulk_cvfactor;
            dabulkcv_dvb = pp.abulk_cvfactor * dabulk0_q_dvb * dvbseff_dvb;
            dvdsat_dvg = 1.0 / abulkcv;
            vdsat = vgst * dvdsat_dvg;
            dvdsat_dvb = -(vdsat * dabulkcv_dvb + dvth_dvb) * dvdsat_dvg;
            if md.xpart > 0.5 {
                if vdsat <= vds_ {
                    t1 = vdsat / 3.0;
                    qgate = coxwl * (vgs_eff_ - vfb - pp.phi - t1);
                    t2 = -two_third_coxwl * vgst;
                    qbulk = -(qgate + t2);
                    qdrn = 0.0;
                    h.cggb = one_third_coxwl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg_;
                    t2 = -one_third_coxwl * dvdsat_dvb;
                    h.cgsb = -(h.cggb + t2);
                    h.cgdb = 0.0;
                    h.cdgb = 0.0;
                    h.cddb = 0.0;
                    h.cdsb = 0.0;
                    h.cbgb = -(h.cggb - two_third_coxwl * dvgs_eff_dvg_);
                    t3 = -(t2 + two_third_coxwl * dvth_dvb);
                    h.cbsb = -(h.cbgb + t3);
                    h.cbdb = 0.0;
                } else {
                    alphaz = vgst / vdsat;
                    t1 = 2.0 * vdsat - vds_;
                    t2 = vds_ / (3.0 * t1);
                    t3 = t2 * vds_;
                    t9 = 0.25 * coxwl;
                    t4 = t9 * alphaz;
                    t7 = 2.0 * vds_ - t1 - 3.0 * t3;
                    t8 = t3 - t1 - 2.0 * vds_;
                    qgate = coxwl * (vgs_eff_ - vfb - pp.phi - 0.5 * (vds_ - t3));
                    t10 = t4 * t8;
                    qdrn = t4 * t7;
                    qbulk = -(qgate + qdrn + t10);
                    t5 = t3 / t1;
                    h.cggb = coxwl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg_;
                    t11 = -coxwl * t5 * dvdsat_dvb;
                    h.cgdb = coxwl * (t2 - 0.5 + 0.5 * t5);
                    h.cgsb = -(h.cggb + t11 + h.cgdb);
                    t6 = 1.0 / vdsat;
                    dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                    dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);
                    t7 *= t9;
                    t8 *= t9;
                    t9 = 2.0 * t4 * (1.0 - 3.0 * t5);
                    h.cdgb = (t7 * dalphaz_dvg - t9 * dvdsat_dvg) * dvgs_eff_dvg_;
                    t12 = t7 * dalphaz_dvb - t9 * dvdsat_dvb;
                    h.cddb = t4 * (3.0 - 6.0 * t2 - 3.0 * t5);
                    h.cdsb = -(h.cdgb + t12 + h.cddb);
                    t9 = 2.0 * t4 * (1.0 + t5);
                    t10 = (t8 * dalphaz_dvg - t9 * dvdsat_dvg) * dvgs_eff_dvg_;
                    t11 = t8 * dalphaz_dvb - t9 * dvdsat_dvb;
                    t12 = t4 * (2.0 * t2 + t5 - 1.0);
                    t0 = -(t10 + t11 + t12);
                    h.cbgb = -(h.cggb + h.cdgb + t10);
                    h.cbdb = -(h.cgdb + h.cddb + t12);
                    h.cbsb = -(h.cgsb + h.cdsb + t0);
                }
            } else if md.xpart < 0.5 {
                if vds_ >= vdsat {
                    t1 = vdsat / 3.0;
                    qgate = coxwl * (vgs_eff_ - vfb - pp.phi - t1);
                    t2 = -two_third_coxwl * vgst;
                    qbulk = -(qgate + t2);
                    qdrn = 0.4 * t2;
                    h.cggb = one_third_coxwl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg_;
                    t2 = -one_third_coxwl * dvdsat_dvb;
                    h.cgsb = -(h.cggb + t2);
                    h.cgdb = 0.0;
                    t3 = 0.4 * two_third_coxwl;
                    h.cdgb = -t3 * dvgs_eff_dvg_;
                    h.cddb = 0.0;
                    t4 = t3 * dvth_dvb;
                    h.cdsb = -(t4 + h.cdgb);
                    h.cbgb = -(h.cggb - two_third_coxwl * dvgs_eff_dvg_);
                    t3 = -(t2 + two_third_coxwl * dvth_dvb);
                    h.cbsb = -(h.cbgb + t3);
                    h.cbdb = 0.0;
                } else {
                    alphaz = vgst / vdsat;
                    t1 = 2.0 * vdsat - vds_;
                    t2 = vds_ / (3.0 * t1);
                    t3 = t2 * vds_;
                    t9 = 0.25 * coxwl;
                    t4 = t9 * alphaz;
                    qgate = coxwl * (vgs_eff_ - vfb - pp.phi - 0.5 * (vds_ - t3));
                    t5 = t3 / t1;
                    h.cggb = coxwl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg_;
                    tmp = -coxwl * t5 * dvdsat_dvb;
                    h.cgdb = coxwl * (t2 - 0.5 + 0.5 * t5);
                    h.cgsb = -(h.cggb + h.cgdb + tmp);
                    t6 = 1.0 / vdsat;
                    dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                    dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);
                    t6 = 8.0 * vdsat * vdsat - 6.0 * vdsat * vds_ + 1.2 * vds_ * vds_;
                    t8 = t2 / t1;
                    t7 = vds_ - t1 - t8 * t6;
                    qdrn = t4 * t7;
                    t7 *= t9;
                    tmp = t8 / t1;
                    tmp1 = t4 * (2.0 - 4.0 * tmp * t6 + t8 * (16.0 * vdsat - 6.0 * vds_));
                    h.cdgb = (t7 * dalphaz_dvg - tmp1 * dvdsat_dvg) * dvgs_eff_dvg_;
                    t10 = t7 * dalphaz_dvb - tmp1 * dvdsat_dvb;
                    h.cddb = t4
                        * (2.0 - (1.0 / (3.0 * t1 * t1) + 2.0 * tmp) * t6
                            + t8 * (6.0 * vdsat - 2.4 * vds_));
                    h.cdsb = -(h.cdgb + t10 + h.cddb);
                    t7 = 2.0 * (t1 + t3);
                    qbulk = -(qgate - t4 * t7);
                    t7 *= t9;
                    t0 = 4.0 * t4 * (1.0 - t5);
                    t12 = (-t7 * dalphaz_dvg - t0 * dvdsat_dvg) * dvgs_eff_dvg_ - h.cdgb;
                    t11 = -t7 * dalphaz_dvb - t10 - t0 * dvdsat_dvb;
                    t10 = -4.0 * t4 * (t2 - 0.5 + 0.5 * t5) - h.cddb;
                    tmp = -(t10 + t11 + t12);
                    h.cbgb = -(h.cggb + h.cdgb + t12);
                    h.cbdb = -(h.cgdb + h.cddb + t10);
                    h.cbsb = -(h.cgsb + h.cdsb + tmp);
                }
            } else if vds_ >= vdsat {
                t1 = vdsat / 3.0;
                qgate = coxwl * (vgs_eff_ - vfb - pp.phi - t1);
                t2 = -two_third_coxwl * vgst;
                qbulk = -(qgate + t2);
                qdrn = 0.5 * t2;
                h.cggb = one_third_coxwl * (3.0 - dvdsat_dvg) * dvgs_eff_dvg_;
                t2 = -one_third_coxwl * dvdsat_dvb;
                h.cgsb = -(h.cggb + t2);
                h.cgdb = 0.0;
                h.cdgb = -one_third_coxwl * dvgs_eff_dvg_;
                h.cddb = 0.0;
                t4 = one_third_coxwl * dvth_dvb;
                h.cdsb = -(t4 + h.cdgb);
                h.cbgb = -(h.cggb - two_third_coxwl * dvgs_eff_dvg_);
                t3 = -(t2 + two_third_coxwl * dvth_dvb);
                h.cbsb = -(h.cbgb + t3);
                h.cbdb = 0.0;
            } else {
                alphaz = vgst / vdsat;
                t1 = 2.0 * vdsat - vds_;
                t2 = vds_ / (3.0 * t1);
                t3 = t2 * vds_;
                t9 = 0.25 * coxwl;
                t4 = t9 * alphaz;
                qgate = coxwl * (vgs_eff_ - vfb - pp.phi - 0.5 * (vds_ - t3));
                t5 = t3 / t1;
                h.cggb = coxwl * (1.0 - t5 * dvdsat_dvg) * dvgs_eff_dvg_;
                tmp = -coxwl * t5 * dvdsat_dvb;
                h.cgdb = coxwl * (t2 - 0.5 + 0.5 * t5);
                h.cgsb = -(h.cggb + h.cgdb + tmp);
                t6 = 1.0 / vdsat;
                dalphaz_dvg = t6 * (1.0 - alphaz * dvdsat_dvg);
                dalphaz_dvb = -t6 * (dvth_dvb + alphaz * dvdsat_dvb);
                t7 = t1 + t3;
                qdrn = -t4 * t7;
                qbulk = -(qgate + qdrn + qdrn);
                t7 *= t9;
                t0 = t4 * (2.0 * t5 - 2.0);
                h.cdgb = (t0 * dvdsat_dvg - t7 * dalphaz_dvg) * dvgs_eff_dvg_;
                t12 = t0 * dvdsat_dvb - t7 * dalphaz_dvb;
                h.cddb = t4 * (1.0 - 2.0 * t2 - t5);
                h.cdsb = -(h.cdgb + t12 + h.cddb);
                h.cbgb = -(h.cggb + 2.0 * h.cdgb);
                h.cbdb = -(h.cgdb + 2.0 * h.cddb);
                h.cbsb = -(h.cgsb + 2.0 * h.cdsb);
            }
        }
    } else {
        if vbseff < 0.0 {
            vbseffcv = vbseff;
            dvbseffcv_dvb = 1.0;
        } else {
            vbseffcv = pp.phi - phis;
            dvbseffcv_dvb = -dphis_dvb;
        }
        coxwl = md.coxe * pp.weff_cv * pp.leff_cv * h.nf;
        if md.cvcharge_mod == 0 {
            noff = n * pp.noff;
            dnoff_dvd = pp.noff * dn_dvd;
            dnoff_dvb = pp.noff * dn_dvb;
            t0 = vtm * noff;
            voffcv = pp.voffcv;
            vgstnvt = (vgst - voffcv) / t0;
            if vgstnvt > EXP_THRESHOLD {
                vgsteff = vgst - voffcv;
                dvgsteff_dvg = dvgs_eff_dvg_;
                dvgsteff_dvd = -dvth_dvd;
                dvgsteff_dvb = -dvth_dvb;
            } else if vgstnvt < -EXP_THRESHOLD {
                vgsteff = t0 * log(1.0 + MIN_EXP);
                dvgsteff_dvg = 0.0;
                dvgsteff_dvd = vgsteff / noff;
                dvgsteff_dvb = dvgsteff_dvd * dnoff_dvb;
                dvgsteff_dvd *= dnoff_dvd;
            } else {
                expvgst = exp(vgstnvt);
                vgsteff = t0 * log(1.0 + expvgst);
                dvgsteff_dvg = expvgst / (1.0 + expvgst);
                dvgsteff_dvd = -dvgsteff_dvg * (dvth_dvd + (vgst - voffcv) / noff * dnoff_dvd)
                    + vgsteff / noff * dnoff_dvd;
                dvgsteff_dvb = -dvgsteff_dvg * (dvth_dvb + (vgst - voffcv) / noff * dnoff_dvb)
                    + vgsteff / noff * dnoff_dvb;
                dvgsteff_dvg *= dvgs_eff_dvg_;
            }
        } else {
            t0 = n * vtm;
            t1 = pp.mstarcv * vgst;
            t2 = t1 / t0;
            if t2 > EXP_THRESHOLD {
                t10 = t1;
                dt10_dvg = pp.mstarcv * dvgs_eff_dvg_;
                dt10_dvd = -dvth_dvd * pp.mstarcv;
                dt10_dvb = -dvth_dvb * pp.mstarcv;
            } else if t2 < -EXP_THRESHOLD {
                t10 = vtm * log(1.0 + MIN_EXP);
                dt10_dvg = 0.0;
                dt10_dvd = t10 * dn_dvd;
                dt10_dvb = t10 * dn_dvb;
                t10 *= n;
            } else {
                expvgst = exp(t2);
                t3 = vtm * log(1.0 + expvgst);
                t10 = n * t3;
                dt10_dvg = pp.mstarcv * expvgst / (1.0 + expvgst);
                dt10_dvb = t3 * dn_dvb - dt10_dvg * (dvth_dvb + vgst * dn_dvb / n);
                dt10_dvd = t3 * dn_dvd - dt10_dvg * (dvth_dvd + vgst * dn_dvd / n);
                dt10_dvg *= dvgs_eff_dvg_;
            }
            t1 = pp.voffcbncv - (1.0 - pp.mstarcv) * vgst;
            t2 = t1 / t0;
            if t2 < -EXP_THRESHOLD {
                t3 = md.coxe * MIN_EXP / pp.cdep0;
                t9 = pp.mstarcv + t3 * n;
                dt9_dvg = 0.0;
                dt9_dvd = dn_dvd * t3;
                dt9_dvb = dn_dvb * t3;
            } else if t2 > EXP_THRESHOLD {
                t3 = md.coxe * MAX_EXP / pp.cdep0;
                t9 = pp.mstarcv + t3 * n;
                dt9_dvg = 0.0;
                dt9_dvd = dn_dvd * t3;
                dt9_dvb = dn_dvb * t3;
            } else {
                expvgst = exp(t2);
                t3 = md.coxe / pp.cdep0;
                t4 = t3 * expvgst;
                t5 = t1 * t4 / t0;
                t9 = pp.mstarcv + n * t4;
                dt9_dvg = t3 * (pp.mstarcv - 1.0) * expvgst / vtm;
                dt9_dvb = t4 * dn_dvb - dt9_dvg * dvth_dvb - t5 * dn_dvb;
                dt9_dvd = t4 * dn_dvd - dt9_dvg * dvth_dvd - t5 * dn_dvd;
                dt9_dvg *= dvgs_eff_dvg_;
            }
            vgsteff = t10 / t9;
            t11 = t9 * t9;
            dvgsteff_dvg = (t9 * dt10_dvg - t10 * dt9_dvg) / t11;
            dvgsteff_dvd = (t9 * dt10_dvd - t10 * dt9_dvd) / t11;
            dvgsteff_dvb = (t9 * dt10_dvb - t10 * dt9_dvb) / t11;
        }
        if md.cap_mod == 1 {
            vfb = h.vfbzb;
            v3 = vfb - vgs_eff_ + vbseffcv - DELTA_3;
            if vfb <= 0.0 {
                t0 = sqrt(v3 * v3 - 4.0 * DELTA_3 * vfb);
            } else {
                t0 = sqrt(v3 * v3 + 4.0 * DELTA_3 * vfb);
            }
            t1 = 0.5 * (1.0 + v3 / t0);
            vfbeff = vfb - 0.5 * (v3 + t0);
            dvfbeff_dvg = t1 * dvgs_eff_dvg_;
            dvfbeff_dvb = -t1 * dvbseffcv_dvb;
            qac0 = coxwl * (vfbeff - vfb);
            dqac0_dvg = coxwl * dvfbeff_dvg;
            dqac0_dvb = coxwl * dvfbeff_dvb;
            t0 = 0.5 * pp.k1ox;
            t3 = vgs_eff_ - vfbeff - vbseffcv - vgsteff;
            if pp.k1ox == 0.0 {
                t1 = 0.0;
                t2 = 0.0;
            } else if t3 < 0.0 {
                t1 = t0 + t3 / pp.k1ox;
                t2 = coxwl;
            } else {
                t1 = sqrt(t0 * t0 + t3);
                t2 = coxwl * t0 / t1;
            }
            qsub0 = coxwl * pp.k1ox * (t1 - t0);
            dqsub0_dvg = t2 * (dvgs_eff_dvg_ - dvfbeff_dvg - dvgsteff_dvg);
            dqsub0_dvd = -t2 * dvgsteff_dvd;
            dqsub0_dvb = -t2 * (dvfbeff_dvb + dvbseffcv_dvb + dvgsteff_dvb);
            abulkcv = abulk0_q * pp.abulk_cvfactor;
            dabulkcv_dvb = pp.abulk_cvfactor * dabulk0_q_dvb;
            vdsatcv = vgsteff / abulkcv;
            t0 = vdsatcv - vds_ - DELTA_4;
            dt0_dvg = 1.0 / abulkcv;
            dt0_dvb = -vdsatcv * dabulkcv_dvb / abulkcv;
            t1 = sqrt(t0 * t0 + 4.0 * DELTA_4 * vdsatcv);
            dt1_dvg = (t0 + DELTA_4 + DELTA_4) / t1;
            dt1_dvd = -t0 / t1;
            dt1_dvb = dt1_dvg * dt0_dvb;
            dt1_dvg *= dt0_dvg;
            if t0 >= 0.0 {
                vdseffcv = vdsatcv - 0.5 * (t0 + t1);
                dvdseffcv_dvg = 0.5 * (dt0_dvg - dt1_dvg);
                dvdseffcv_dvd = 0.5 * (1.0 - dt1_dvd);
                dvdseffcv_dvb = 0.5 * (dt0_dvb - dt1_dvb);
            } else {
                t3 = (DELTA_4 + DELTA_4) / (t1 - t0);
                t4 = 1.0 - t3;
                t5 = vdsatcv * t3 / (t1 - t0);
                vdseffcv = vdsatcv * t4;
                dvdseffcv_dvg = dt0_dvg * t4 + t5 * (dt1_dvg - dt0_dvg);
                dvdseffcv_dvd = t5 * (dt1_dvd + 1.0);
                dvdseffcv_dvb = dt0_dvb * (t4 - t5) + t5 * dt1_dvb;
            }
            if vds_ == 0.0 {
                vdseffcv = 0.0;
                dvdseffcv_dvg = 0.0;
                dvdseffcv_dvb = 0.0;
            }
            t0 = abulkcv * vdseffcv;
            t1 = 12.0 * (vgsteff - 0.5 * t0 + 1.0e-20);
            t2 = vdseffcv / t1;
            t3 = t0 * t2;
            t4 = 1.0 - 12.0 * t2 * t2 * abulkcv;
            t5 = 6.0 * t0 * (4.0 * vgsteff - t0) / (t1 * t1) - 0.5;
            t6 = 12.0 * t2 * t2 * vgsteff;
            qgate = coxwl * (vgsteff - 0.5 * vdseffcv + t3);
            cgg1 = coxwl * (t4 + t5 * dvdseffcv_dvg);
            cgd1 = coxwl * t5 * dvdseffcv_dvd + cgg1 * dvgsteff_dvd;
            cgb1 = coxwl * (t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb) + cgg1 * dvgsteff_dvb;
            cgg1 *= dvgsteff_dvg;
            t7 = 1.0 - abulkcv;
            qbulk = coxwl * t7 * (0.5 * vdseffcv - t3);
            t4 = -t7 * (t4 - 1.0);
            t5 *= -t7;
            t6 = -(t7 * t6 + (0.5 * vdseffcv - t3));
            cbg1 = coxwl * (t4 + t5 * dvdseffcv_dvg);
            cbd1 = coxwl * t5 * dvdseffcv_dvd + cbg1 * dvgsteff_dvd;
            cbb1 = coxwl * (t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb) + cbg1 * dvgsteff_dvb;
            cbg1 *= dvgsteff_dvg;
            if md.xpart > 0.5 {
                t1 += t1;
                qsrc = -coxwl * (0.5 * vgsteff + 0.25 * t0 - t0 * t0 / t1);
                t7 = (4.0 * vgsteff - t0) / (t1 * t1);
                t4 = -(0.5 + 24.0 * t0 * t0 / (t1 * t1));
                t5 = -(0.25 * abulkcv - 12.0 * abulkcv * t0 * t7);
                t6 = -(0.25 * vdseffcv - 12.0 * t0 * vdseffcv * t7);
                csg = coxwl * (t4 + t5 * dvdseffcv_dvg);
                csd = coxwl * t5 * dvdseffcv_dvd + csg * dvgsteff_dvd;
                csb = coxwl * (t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb) + csg * dvgsteff_dvb;
                csg *= dvgsteff_dvg;
            } else if md.xpart < 0.5 {
                t1 /= 12.0;
                t2 = 0.5 * coxwl / (t1 * t1);
                t3 = vgsteff * (2.0 * t0 * t0 / 3.0 + vgsteff * (vgsteff - 4.0 * t0 / 3.0))
                    - 2.0 * t0 * t0 * t0 / 15.0;
                qsrc = -t2 * t3;
                t7 = 4.0 / 3.0 * vgsteff * (vgsteff - t0) + 0.4 * t0 * t0;
                t4 = -2.0 * qsrc / t1
                    - t2 * (vgsteff * (3.0 * vgsteff - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
                t5 = (qsrc / t1 + t2 * t7) * abulkcv;
                t6 = qsrc / t1 * vdseffcv + t2 * t7 * vdseffcv;
                csg = t4 + t5 * dvdseffcv_dvg;
                csd = t5 * dvdseffcv_dvd + csg * dvgsteff_dvd;
                csb = t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb + csg * dvgsteff_dvb;
                csg *= dvgsteff_dvg;
            } else {
                qsrc = -0.5 * (qgate + qbulk);
                csg = -0.5 * (cgg1 + cbg1);
                csb = -0.5 * (cgb1 + cbb1);
                csd = -0.5 * (cgd1 + cbd1);
            }
            qgate += qac0 + qsub0;
            qbulk -= qac0 + qsub0;
            qdrn = -(qgate + qbulk + qsrc);
            cgg = dqac0_dvg + dqsub0_dvg + cgg1;
            cgd = dqsub0_dvd + cgd1;
            cgb = dqac0_dvb + dqsub0_dvb + cgb1;
            cbg = cbg1 - dqac0_dvg - dqsub0_dvg;
            cbd = cbd1 - dqsub0_dvd;
            cbb = cbb1 - dqac0_dvb - dqsub0_dvb;
            cgb *= dvbseff_dvb;
            cbb *= dvbseff_dvb;
            csb *= dvbseff_dvb;
            h.cggb = cgg;
            h.cgsb = -(cgg + cgd + cgb);
            h.cgdb = cgd;
            h.cdgb = -(cgg + cbg + csg);
            h.cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
            h.cddb = -(cgd + cbd + csd);
            h.cbgb = cbg;
            h.cbsb = -(cbg + cbd + cbb);
            h.cbdb = cbd;
        } else if md.cap_mod == 2 {
            v3 = h.vfbzb - vgs_eff_ + vbseffcv - DELTA_3;
            if h.vfbzb <= 0.0 {
                t0 = sqrt(v3 * v3 - 4.0 * DELTA_3 * h.vfbzb);
            } else {
                t0 = sqrt(v3 * v3 + 4.0 * DELTA_3 * h.vfbzb);
            }
            t1 = 0.5 * (1.0 + v3 / t0);
            vfbeff = h.vfbzb - 0.5 * (v3 + t0);
            dvfbeff_dvg = t1 * dvgs_eff_dvg_;
            dvfbeff_dvb = -t1 * dvbseffcv_dvb;
            cox = h.coxp;
            tox = 1.0e8 * h.toxp;
            t0 = (vgs_eff_ - vbseffcv - h.vfbzb) / tox;
            dt0_dvg = dvgs_eff_dvg_ / tox;
            dt0_dvb = -dvbseffcv_dvb / tox;
            tmp = t0 * pp.acde;
            if -EXP_THRESHOLD < tmp && tmp < EXP_THRESHOLD {
                tcen = pp.ldeb * exp(tmp);
                dtcen_dvg = pp.acde * tcen;
                dtcen_dvb = dtcen_dvg * dt0_dvb;
                dtcen_dvg *= dt0_dvg;
            } else if tmp <= -EXP_THRESHOLD {
                tcen = pp.ldeb * MIN_EXP;
                dtcen_dvb = 0.0;
                dtcen_dvg = dtcen_dvb;
            } else {
                tcen = pp.ldeb * MAX_EXP;
                dtcen_dvb = 0.0;
                dtcen_dvg = dtcen_dvb;
            }
            link = 1.0e-3 * h.toxp;
            v3 = pp.ldeb - tcen - link;
            v4 = sqrt(v3 * v3 + 4.0 * link * pp.ldeb);
            tcen = pp.ldeb - 0.5 * (v3 + v4);
            t1 = 0.5 * (1.0 + v3 / v4);
            dtcen_dvg *= t1;
            dtcen_dvb *= t1;
            ccen = epssub / tcen;
            t2 = cox / (cox + ccen);
            coxeff = t2 * ccen;
            t3 = -ccen / tcen;
            dcoxeff_dvg = t2 * t2 * t3;
            dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb;
            dcoxeff_dvg *= dtcen_dvg;
            coxwlcen = coxwl * coxeff / md.coxe;
            qac0 = coxwlcen * (vfbeff - h.vfbzb);
            qovcox = qac0 / coxeff;
            dqac0_dvg = coxwlcen * dvfbeff_dvg + qovcox * dcoxeff_dvg;
            dqac0_dvb = coxwlcen * dvfbeff_dvb + qovcox * dcoxeff_dvb;
            t0 = 0.5 * pp.k1ox;
            t3 = vgs_eff_ - vfbeff - vbseffcv - vgsteff;
            if pp.k1ox == 0.0 {
                t1 = 0.0;
                t2 = 0.0;
            } else if t3 < 0.0 {
                t1 = t0 + t3 / pp.k1ox;
                t2 = coxwlcen;
            } else {
                t1 = sqrt(t0 * t0 + t3);
                t2 = coxwlcen * t0 / t1;
            }
            qsub0 = coxwlcen * pp.k1ox * (t1 - t0);
            qovcox = qsub0 / coxeff;
            dqsub0_dvg = t2 * (dvgs_eff_dvg_ - dvfbeff_dvg - dvgsteff_dvg) + qovcox * dcoxeff_dvg;
            dqsub0_dvd = -t2 * dvgsteff_dvd;
            dqsub0_dvb = -t2 * (dvfbeff_dvb + dvbseffcv_dvb + dvgsteff_dvb) + qovcox * dcoxeff_dvb;
            if pp.k1ox <= 0.0 {
                denomi = 0.25 * pp.moin * vtm;
                t0 = 0.5 * pp.sqrt_phi;
            } else {
                denomi = pp.moin * vtm * pp.k1ox * pp.k1ox;
                t0 = pp.k1ox * pp.sqrt_phi;
            }
            t1 = 2.0 * t0 + vgsteff;
            deltaphi = vtm * log(1.0 + t1 * vgsteff / denomi);
            ddeltaphi_dvg = 2.0 * vtm * (t1 - t0) / (denomi + t1 * vgsteff);
            t0 = vgsteff - deltaphi - 0.001;
            dt0_dvg = 1.0 - ddeltaphi_dvg;
            t1 = sqrt(t0 * t0 + vgsteff * 0.004);
            vgdp = 0.5 * (t0 + t1);
            dvgdp_dvg = 0.5 * (dt0_dvg + (t0 * dt0_dvg + 0.002) / t1);
            tox += tox;
            t0 = (vgsteff + h.vtfbphi2) / tox;
            tmp = exp(md.bdos * 0.7 * log(t0));
            t1 = 1.0 + tmp;
            t2 = md.bdos * 0.7 * tmp / (t0 * tox);
            tcen = md.ados * 1.9e-9 / t1;
            dtcen_dvg = -tcen * t2 / t1;
            dtcen_dvd = dtcen_dvg * dvgsteff_dvd;
            dtcen_dvb = dtcen_dvg * dvgsteff_dvb;
            dtcen_dvg *= dvgsteff_dvg;
            ccen = epssub / tcen;
            t0 = cox / (cox + ccen);
            coxeff = t0 * ccen;
            t1 = -ccen / tcen;
            dcoxeff_dvg = t0 * t0 * t1;
            dcoxeff_dvd = dcoxeff_dvg * dtcen_dvd;
            dcoxeff_dvb = dcoxeff_dvg * dtcen_dvb;
            dcoxeff_dvg *= dtcen_dvg;
            coxwlcen = coxwl * coxeff / md.coxe;
            abulkcv = abulk0_q * pp.abulk_cvfactor;
            dabulkcv_dvb = pp.abulk_cvfactor * dabulk0_q_dvb;
            vdsatcv = vgdp / abulkcv;
            t0 = vdsatcv - vds_ - DELTA_4;
            dt0_dvg = dvgdp_dvg / abulkcv;
            dt0_dvb = -vdsatcv * dabulkcv_dvb / abulkcv;
            t1 = sqrt(t0 * t0 + 4.0 * DELTA_4 * vdsatcv);
            dt1_dvg = (t0 + DELTA_4 + DELTA_4) / t1;
            dt1_dvd = -t0 / t1;
            dt1_dvb = dt1_dvg * dt0_dvb;
            dt1_dvg *= dt0_dvg;
            if t0 >= 0.0 {
                vdseffcv = vdsatcv - 0.5 * (t0 + t1);
                dvdseffcv_dvg = 0.5 * (dt0_dvg - dt1_dvg);
                dvdseffcv_dvd = 0.5 * (1.0 - dt1_dvd);
                dvdseffcv_dvb = 0.5 * (dt0_dvb - dt1_dvb);
            } else {
                t3 = (DELTA_4 + DELTA_4) / (t1 - t0);
                t4 = 1.0 - t3;
                t5 = vdsatcv * t3 / (t1 - t0);
                vdseffcv = vdsatcv * t4;
                dvdseffcv_dvg = dt0_dvg * t4 + t5 * (dt1_dvg - dt0_dvg);
                dvdseffcv_dvd = t5 * (dt1_dvd + 1.0);
                dvdseffcv_dvb = dt0_dvb * (t4 - t5) + t5 * dt1_dvb;
            }
            if vds_ == 0.0 {
                vdseffcv = 0.0;
                dvdseffcv_dvg = 0.0;
                dvdseffcv_dvb = 0.0;
            }
            t0 = abulkcv * vdseffcv;
            t1 = vgdp;
            t2 = 12.0 * (t1 - 0.5 * t0 + 1.0e-20);
            t3 = t0 / t2;
            t4 = 1.0 - 12.0 * t3 * t3;
            t5 = abulkcv * (6.0 * t0 * (4.0 * t1 - t0) / (t2 * t2) - 0.5);
            t6 = t5 * vdseffcv / abulkcv;
            qgate = coxwlcen * (t1 - t0 * (0.5 - t3));
            qovcox = qgate / coxeff;
            cgg1 = coxwlcen * (t4 * dvgdp_dvg + t5 * dvdseffcv_dvg);
            cgd1 = coxwlcen * t5 * dvdseffcv_dvd + cgg1 * dvgsteff_dvd + qovcox * dcoxeff_dvd;
            cgb1 = coxwlcen * (t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb)
                + cgg1 * dvgsteff_dvb
                + qovcox * dcoxeff_dvb;
            cgg1 = cgg1 * dvgsteff_dvg + qovcox * dcoxeff_dvg;
            t7 = 1.0 - abulkcv;
            t8 = t2 * t2;
            t9 = 12.0 * t7 * t0 * t0 / (t8 * abulkcv);
            t10 = t9 * dvgdp_dvg;
            t11 = -t7 * t5 / abulkcv;
            t12 = -(t9 * t1 / abulkcv + vdseffcv * (0.5 - t0 / t2));
            qbulk = coxwlcen * t7 * (0.5 * vdseffcv - t0 * vdseffcv / t2);
            qovcox = qbulk / coxeff;
            cbg1 = coxwlcen * (t10 + t11 * dvdseffcv_dvg);
            cbd1 = coxwlcen * t11 * dvdseffcv_dvd + cbg1 * dvgsteff_dvd + qovcox * dcoxeff_dvd;
            cbb1 = coxwlcen * (t11 * dvdseffcv_dvb + t12 * dabulkcv_dvb)
                + cbg1 * dvgsteff_dvb
                + qovcox * dcoxeff_dvb;
            cbg1 = cbg1 * dvgsteff_dvg + qovcox * dcoxeff_dvg;
            if md.xpart > 0.5 {
                qsrc = -coxwlcen * (t1 / 2.0 + t0 / 4.0 - 0.5 * t0 * t0 / t2);
                qovcox = qsrc / coxeff;
                t2 += t2;
                t3 = t2 * t2;
                t7 = -(0.25 - 12.0 * t0 * (4.0 * t1 - t0) / t3);
                t4 = -(0.5 + 24.0 * t0 * t0 / t3) * dvgdp_dvg;
                t5 = t7 * abulkcv;
                t6 = t7 * vdseffcv;
                csg = coxwlcen * (t4 + t5 * dvdseffcv_dvg);
                csd = coxwlcen * t5 * dvdseffcv_dvd + csg * dvgsteff_dvd + qovcox * dcoxeff_dvd;
                csb = coxwlcen * (t5 * dvdseffcv_dvb + t6 * dabulkcv_dvb)
                    + csg * dvgsteff_dvb
                    + qovcox * dcoxeff_dvb;
                csg = csg * dvgsteff_dvg + qovcox * dcoxeff_dvg;
            } else if md.xpart < 0.5 {
                t2 /= 12.0;
                t3 = 0.5 * coxwlcen / (t2 * t2);
                t4 = t1 * (2.0 * t0 * t0 / 3.0 + t1 * (t1 - 4.0 * t0 / 3.0))
                    - 2.0 * t0 * t0 * t0 / 15.0;
                qsrc = -t3 * t4;
                qovcox = qsrc / coxeff;
                t8 = 4.0 / 3.0 * t1 * (t1 - t0) + 0.4 * t0 * t0;
                t5 = -2.0 * qsrc / t2
                    - t3 * (t1 * (3.0 * t1 - 8.0 * t0 / 3.0) + 2.0 * t0 * t0 / 3.0);
                t6 = abulkcv * (qsrc / t2 + t3 * t8);
                t7 = t6 * vdseffcv / abulkcv;
                csg = t5 * dvgdp_dvg + t6 * dvdseffcv_dvg;
                csd = csg * dvgsteff_dvd + t6 * dvdseffcv_dvd + qovcox * dcoxeff_dvd;
                csb = csg * dvgsteff_dvb
                    + t6 * dvdseffcv_dvb
                    + t7 * dabulkcv_dvb
                    + qovcox * dcoxeff_dvb;
                csg = csg * dvgsteff_dvg + qovcox * dcoxeff_dvg;
            } else {
                qsrc = -0.5 * qgate;
                csg = -0.5 * cgg1;
                csd = -0.5 * cgd1;
                csb = -0.5 * cgb1;
            }
            qgate += qac0 + qsub0 - qbulk;
            qbulk -= qac0 + qsub0;
            qdrn = -(qgate + qbulk + qsrc);
            cbg = cbg1 - dqac0_dvg - dqsub0_dvg;
            cbd = cbd1 - dqsub0_dvd;
            cbb = cbb1 - dqac0_dvb - dqsub0_dvb;
            cgg = cgg1 - cbg;
            cgd = cgd1 - cbd;
            cgb = cgb1 - cbb;
            cgb *= dvbseff_dvb;
            cbb *= dvbseff_dvb;
            csb *= dvbseff_dvb;
            h.cggb = cgg;
            h.cgsb = -(cgg + cgd + cgb);
            h.cgdb = cgd;
            h.cdgb = -(cgg + cbg + csg);
            h.cdsb = cgg + cgd + cgb + cbg + cbd + cbb + csg + csd + csb;
            h.cddb = -(cgd + cbd + csd);
            h.cbgb = cbg;
            h.cbsb = -(cbg + cbd + cbb);
            h.cbdb = cbd;
        }
    }
    if goto_finished == 0 {
        h.csgb = -h.cggb - h.cdgb - h.cbgb;
        h.csdb = -h.cgdb - h.cddb - h.cbdb;
        h.cssb = -h.cgsb - h.cdsb - h.cbsb;
        h.cgbb = -h.cgdb - h.cggb - h.cgsb;
        h.cdbb = -h.cddb - h.cdgb - h.cdsb;
        h.cbbb = -h.cbgb - h.cbdb - h.cbsb;
        h.csbb = -h.cgbb - h.cdbb - h.cbbb;
        h.qgate = qgate;
        h.qbulk = qbulk;
        h.qdrn = qdrn;
        h.qsrc = -(qgate + qbulk + qdrn);
        if h.trnqs_mod != 0 || h.acnqs_mod != 0 {
            qcheq = -(qbulk + qgate);
            h.qchqs = qcheq;
            h.cqgb = -(h.cggb + h.cbgb);
            h.cqdb = -(h.cgdb + h.cbdb);
            h.cqsb = -(h.cgsb + h.cbsb);
            h.cqbb = -(h.cqgb + h.cqdb + h.cqsb);
            coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
            t1 = h.gcrg / coxwl;
            h.gtau = t1 * SCALING_FACTOR;
            if h.acnqs_mod != 0 {
                h.taunet = 1.0 / t1;
            }
            ctx.states.set(0, st + ST_QCHEQ, qcheq);
            if ck_mode & MODEINITTRAN != 0 {
                ctx.states
                    .set(1, st + ST_QCHEQ, ctx.states.get(0, st + ST_QCHEQ));
            }
            if h.trnqs_mod != 0 {
                let _ = ctx.integrate(0.0, st + ST_QCHEQ);
            }
        }
    }
    if charge_computation_needed != 0 {
        czbd = md.dunit_area_temp_jct_cap * h.adeff;
        czbs = md.sunit_area_temp_jct_cap * h.aseff;
        czbdsw = md.dunit_length_sidewall_temp_jct_cap * h.pdeff;
        czbdswg = md.dunit_length_gate_sidewall_temp_jct_cap * pp.weff_cj * h.nf;
        czbssw = md.sunit_length_sidewall_temp_jct_cap * h.pseff;
        czbsswg = md.sunit_length_gate_sidewall_temp_jct_cap * pp.weff_cj * h.nf;
        mjs = md.sbulk_jct_bot_grading_coeff;
        mjsws = md.sbulk_jct_side_grading_coeff;
        mjswgs = md.sbulk_jct_gate_side_grading_coeff;
        mjd = md.dbulk_jct_bot_grading_coeff;
        mjswd = md.dbulk_jct_side_grading_coeff;
        mjswgd = md.dbulk_jct_gate_side_grading_coeff;
        if vbs_jct == 0.0 {
            ctx.states.set(0, st + ST_QBS, 0.0);
            h.capbs = czbs + czbssw + czbsswg;
        } else if vbs_jct < 0.0 {
            if czbs > 0.0 {
                arg = 1.0 - vbs_jct / md.phi_bs;
                if mjs == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjs * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBS,
                    md.phi_bs * czbs * (1.0 - arg * sarg) / (1.0 - mjs),
                );
                h.capbs = czbs * sarg;
            } else {
                ctx.states.set(0, st + ST_QBS, 0.0);
                h.capbs = 0.0;
            }
            if czbssw > 0.0 {
                arg = 1.0 - vbs_jct / md.phi_bsws;
                if mjsws == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjsws * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBS,
                    ctx.states.get(0, st + ST_QBS)
                        + md.phi_bsws * czbssw * (1.0 - arg * sarg) / (1.0 - mjsws),
                );
                h.capbs += czbssw * sarg;
            }
            if czbsswg > 0.0 {
                arg = 1.0 - vbs_jct / md.phi_bswgs;
                if mjswgs == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjswgs * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBS,
                    ctx.states.get(0, st + ST_QBS)
                        + md.phi_bswgs * czbsswg * (1.0 - arg * sarg) / (1.0 - mjswgs),
                );
                h.capbs += czbsswg * sarg;
            }
        } else {
            t0 = czbs + czbssw + czbsswg;
            t1 = vbs_jct
                * (czbs * mjs / md.phi_bs
                    + czbssw * mjsws / md.phi_bsws
                    + czbsswg * mjswgs / md.phi_bswgs);
            ctx.states.set(0, st + ST_QBS, vbs_jct * (t0 + 0.5 * t1));
            h.capbs = t0 + t1;
        }
        if vbd_jct == 0.0 {
            ctx.states.set(0, st + ST_QBD, 0.0);
            h.capbd = czbd + czbdsw + czbdswg;
        } else if vbd_jct < 0.0 {
            if czbd > 0.0 {
                arg = 1.0 - vbd_jct / md.phi_bd;
                if mjd == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjd * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBD,
                    md.phi_bd * czbd * (1.0 - arg * sarg) / (1.0 - mjd),
                );
                h.capbd = czbd * sarg;
            } else {
                ctx.states.set(0, st + ST_QBD, 0.0);
                h.capbd = 0.0;
            }
            if czbdsw > 0.0 {
                arg = 1.0 - vbd_jct / md.phi_bswd;
                if mjswd == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjswd * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBD,
                    ctx.states.get(0, st + ST_QBD)
                        + md.phi_bswd * czbdsw * (1.0 - arg * sarg) / (1.0 - mjswd),
                );
                h.capbd += czbdsw * sarg;
            }
            if czbdswg > 0.0 {
                arg = 1.0 - vbd_jct / md.phi_bswgd;
                if mjswgd == 0.5 {
                    sarg = 1.0 / sqrt(arg);
                } else {
                    sarg = exp(-mjswgd * log(arg));
                }
                ctx.states.set(
                    0,
                    st + ST_QBD,
                    ctx.states.get(0, st + ST_QBD)
                        + md.phi_bswgd * czbdswg * (1.0 - arg * sarg) / (1.0 - mjswgd),
                );
                h.capbd += czbdswg * sarg;
            }
        } else {
            t0 = czbd + czbdsw + czbdswg;
            t1 = vbd_jct
                * (czbd * mjd / md.phi_bd
                    + czbdsw * mjswd / md.phi_bswd
                    + czbdswg * mjswgd / md.phi_bswgd);
            ctx.states.set(0, st + ST_QBD, vbd_jct * (t0 + 0.5 * t1));
            h.capbd = t0 + t1;
        }
    }
    if (h.off == 0 || ck_mode & MODEINITFIX == 0) && check == 1 {
        ctx.noncon = true;
    }
    ctx.states.set(0, st + ST_VDS, vds);
    ctx.states.set(0, st + ST_VGS, vgs);
    ctx.states.set(0, st + ST_VBS, vbs);
    ctx.states.set(0, st + ST_VBD, vbd);
    ctx.states.set(0, st + ST_VGES, vges);
    ctx.states.set(0, st + ST_VGMS, vgms);
    ctx.states.set(0, st + ST_VDBS, vdbs);
    ctx.states.set(0, st + ST_VDBD, vdbd);
    ctx.states.set(0, st + ST_VSBS, vsbs);
    ctx.states.set(0, st + ST_VSES, vses);
    ctx.states.set(0, st + ST_VDES, vdes);
    ctx.states.set(0, st + ST_QDEF, qdef);
    if charge_computation_needed == 0 {
        path = 850;
    } else {
        if h.rgate_mod == 3 {
            vgdx = vgmd;
            vgsx = vgms;
        } else {
            vgdx = vgd;
            vgsx = vgs;
        }
        if md.cap_mod == 0 {
            cgdo = pp.cgdo;
            qgdo = pp.cgdo * vgdx;
            cgso = pp.cgso;
            qgso = pp.cgso * vgsx;
        } else {
            t0 = vgdx + DELTA_1;
            t1 = sqrt(t0 * t0 + 4.0 * DELTA_1);
            t2 = 0.5 * (t0 - t1);
            t3 = pp.weff_cv * pp.cgdl;
            t4 = sqrt(1.0 - 4.0 * t2 / pp.ckappad);
            cgdo = pp.cgdo + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
            qgdo = (pp.cgdo + t3) * vgdx - t3 * (t2 + 0.5 * pp.ckappad * (t4 - 1.0));
            t0 = vgsx + DELTA_1;
            t1 = sqrt(t0 * t0 + 4.0 * DELTA_1);
            t2 = 0.5 * (t0 - t1);
            t3 = pp.weff_cv * pp.cgsl;
            t4 = sqrt(1.0 - 4.0 * t2 / pp.ckappas);
            cgso = pp.cgso + t3 - t3 * (1.0 - 1.0 / t4) * (0.5 - 0.5 * t0 / t1);
            qgso = (pp.cgso + t3) * vgsx - t3 * (t2 + 0.5 * pp.ckappas * (t4 - 1.0));
        }
        if h.nf != 1.0 {
            cgdo *= h.nf;
            cgso *= h.nf;
            qgdo *= h.nf;
            qgso *= h.nf;
        }
        h.cgdo = cgdo;
        h.qgdo = qgdo;
        h.cgso = cgso;
        h.qgso = qgso;
        ag0 = ctx.ag[0];
        if h.mode > 0 {
            if h.trnqs_mod == 0 {
                qdrn -= qgdo;
                if h.rgate_mod == 3 {
                    gcgmgmb = (cgdo + cgso + pp.cgbo) * ag0;
                    gcgmdb = -cgdo * ag0;
                    gcgmsb = -cgso * ag0;
                    gcgmbb = -pp.cgbo * ag0;
                    gcdgmb = gcgmdb;
                    gcsgmb = gcgmsb;
                    gcbgmb = gcgmbb;
                    gcggb = h.cggb * ag0;
                    gcgdb = h.cgdb * ag0;
                    gcgsb = h.cgsb * ag0;
                    gcgbb = -(gcggb + gcgdb + gcgsb);
                    gcdgb = h.cdgb * ag0;
                    gcsgb = -(h.cggb + h.cbgb + h.cdgb) * ag0;
                    gcbgb = h.cbgb * ag0;
                    qgmb = pp.cgbo * vgmb;
                    qgmid = qgdo + qgso + qgmb;
                    qbulk -= qgmb;
                    qsrc = -(qgate + qgmid + qbulk + qdrn);
                } else {
                    gcggb = (h.cggb + cgdo + cgso + pp.cgbo) * ag0;
                    gcgdb = (h.cgdb - cgdo) * ag0;
                    gcgsb = (h.cgsb - cgso) * ag0;
                    gcgbb = -(gcggb + gcgdb + gcgsb);
                    gcdgb = (h.cdgb - cgdo) * ag0;
                    gcsgb = -(h.cggb + h.cbgb + h.cdgb + cgso) * ag0;
                    gcbgb = (h.cbgb - pp.cgbo) * ag0;
                    gcbgmb = 0.0;
                    gcsgmb = gcbgmb;
                    gcdgmb = gcsgmb;
                    qgb = pp.cgbo * vgb;
                    qgate += qgdo + qgso + qgb;
                    qbulk -= qgb;
                    qsrc = -(qgate + qbulk + qdrn);
                }
                gcddb = (h.cddb + h.capbd + cgdo) * ag0;
                gcdsb = h.cdsb * ag0;
                gcsdb = -(h.cgdb + h.cbdb + h.cddb) * ag0;
                gcssb = (h.capbs + cgso - (h.cgsb + h.cbsb + h.cdsb)) * ag0;
                if h.rbody_mod == 0 {
                    gcdbb = -(gcdgb + gcddb + gcdsb + gcdgmb);
                    gcsbb = -(gcsgb + gcsdb + gcssb + gcsgmb);
                    gcbdb = (h.cbdb - h.capbd) * ag0;
                    gcbsb = (h.cbsb - h.capbs) * ag0;
                    gcdbdb = 0.0;
                    gcsbsb = 0.0;
                } else {
                    gcdbb = -(h.cddb + h.cdgb + h.cdsb) * ag0;
                    gcsbb = -(gcsgb + gcsdb + gcssb + gcsgmb) + h.capbs * ag0;
                    gcbdb = h.cbdb * ag0;
                    gcbsb = h.cbsb * ag0;
                    gcdbdb = -h.capbd * ag0;
                    gcsbsb = -h.capbs * ag0;
                }
                gcbbb = -(gcbdb + gcbgb + gcbsb + gcbgmb);
                ggts = 0.0;
                ggtb = ggts;
                ggtd = ggtb;
                ggtg = ggtd;
                sxpart = 0.6;
                dxpart = 0.4;
                ddxpart_dvs = 0.0;
                ddxpart_dvb = ddxpart_dvs;
                ddxpart_dvg = ddxpart_dvb;
                ddxpart_dvd = ddxpart_dvg;
                dsxpart_dvs = 0.0;
                dsxpart_dvb = dsxpart_dvs;
                dsxpart_dvg = dsxpart_dvb;
                dsxpart_dvd = dsxpart_dvg;
            } else {
                qcheq = h.qchqs;
                coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
                t0 = qdef * SCALING_FACTOR / coxwl;
                h.gtg = t0 * h.gcrgg;
                ggtg = h.gtg;
                h.gtd = t0 * h.gcrgd;
                ggtd = h.gtd;
                h.gts = t0 * h.gcrgs;
                ggts = h.gts;
                h.gtb = t0 * h.gcrgb;
                ggtb = h.gtb;
                gqdef = SCALING_FACTOR * ag0;
                gcqgb = h.cqgb * ag0;
                gcqdb = h.cqdb * ag0;
                gcqsb = h.cqsb * ag0;
                gcqbb = h.cqbb * ag0;
                if fabs(qcheq) <= 1.0e-5 * coxwl {
                    if md.xpart < 0.5 {
                        dxpart = 0.4;
                    } else if md.xpart > 0.5 {
                        dxpart = 0.0;
                    } else {
                        dxpart = 0.5;
                    }
                    ddxpart_dvs = 0.0;
                    ddxpart_dvb = ddxpart_dvs;
                    ddxpart_dvg = ddxpart_dvb;
                    ddxpart_dvd = ddxpart_dvg;
                } else {
                    dxpart = qdrn / qcheq;
                    cdd = h.cddb;
                    csd = -(h.cgdb + h.cddb + h.cbdb);
                    ddxpart_dvd = (cdd - dxpart * (cdd + csd)) / qcheq;
                    cdg = h.cdgb;
                    csg = -(h.cggb + h.cdgb + h.cbgb);
                    ddxpart_dvg = (cdg - dxpart * (cdg + csg)) / qcheq;
                    cds = h.cdsb;
                    css = -(h.cgsb + h.cdsb + h.cbsb);
                    ddxpart_dvs = (cds - dxpart * (cds + css)) / qcheq;
                    ddxpart_dvb = -(ddxpart_dvd + ddxpart_dvg + ddxpart_dvs);
                }
                sxpart = 1.0 - dxpart;
                dsxpart_dvd = -ddxpart_dvd;
                dsxpart_dvg = -ddxpart_dvg;
                dsxpart_dvs = -ddxpart_dvs;
                dsxpart_dvb = -(dsxpart_dvd + dsxpart_dvg + dsxpart_dvs);
                if h.rgate_mod == 3 {
                    gcgmgmb = (cgdo + cgso + pp.cgbo) * ag0;
                    gcgmdb = -cgdo * ag0;
                    gcgmsb = -cgso * ag0;
                    gcgmbb = -pp.cgbo * ag0;
                    gcdgmb = gcgmdb;
                    gcsgmb = gcgmsb;
                    gcbgmb = gcgmbb;
                    gcbgb = 0.0;
                    gcsgb = gcbgb;
                    gcdgb = gcsgb;
                    gcgbb = 0.0;
                    gcgsb = gcgbb;
                    gcgdb = gcgsb;
                    gcggb = gcgdb;
                    qgmb = pp.cgbo * vgmb;
                    qgmid = qgdo + qgso + qgmb;
                    qgate = 0.0;
                    qbulk = -qgmb;
                    qdrn = -qgdo;
                    qsrc = -(qgmid + qbulk + qdrn);
                } else {
                    gcggb = (cgdo + cgso + pp.cgbo) * ag0;
                    gcgdb = -cgdo * ag0;
                    gcgsb = -cgso * ag0;
                    gcgbb = -pp.cgbo * ag0;
                    gcdgb = gcgdb;
                    gcsgb = gcgsb;
                    gcbgb = gcgbb;
                    gcbgmb = 0.0;
                    gcsgmb = gcbgmb;
                    gcdgmb = gcsgmb;
                    qgb = pp.cgbo * vgb;
                    qgate = qgdo + qgso + qgb;
                    qbulk = -qgb;
                    qdrn = -qgdo;
                    qsrc = -(qgate + qbulk + qdrn);
                }
                gcddb = (h.capbd + cgdo) * ag0;
                gcsdb = 0.0;
                gcdsb = gcsdb;
                gcssb = (h.capbs + cgso) * ag0;
                if h.rbody_mod == 0 {
                    gcdbb = -(gcdgb + gcddb + gcdgmb);
                    gcsbb = -(gcsgb + gcssb + gcsgmb);
                    gcbdb = -h.capbd * ag0;
                    gcbsb = -h.capbs * ag0;
                    gcdbdb = 0.0;
                    gcsbsb = 0.0;
                } else {
                    gcbsb = 0.0;
                    gcbdb = gcbsb;
                    gcsbb = gcbdb;
                    gcdbb = gcsbb;
                    gcdbdb = -h.capbd * ag0;
                    gcsbsb = -h.capbs * ag0;
                }
                gcbbb = -(gcbdb + gcbgb + gcbsb + gcbgmb);
            }
        } else if h.trnqs_mod == 0 {
            qsrc = qdrn - qgso;
            if h.rgate_mod == 3 {
                gcgmgmb = (cgdo + cgso + pp.cgbo) * ag0;
                gcgmdb = -cgdo * ag0;
                gcgmsb = -cgso * ag0;
                gcgmbb = -pp.cgbo * ag0;
                gcdgmb = gcgmdb;
                gcsgmb = gcgmsb;
                gcbgmb = gcgmbb;
                gcggb = h.cggb * ag0;
                gcgdb = h.cgsb * ag0;
                gcgsb = h.cgdb * ag0;
                gcgbb = -(gcggb + gcgdb + gcgsb);
                gcdgb = -(h.cggb + h.cbgb + h.cdgb) * ag0;
                gcsgb = h.cdgb * ag0;
                gcbgb = h.cbgb * ag0;
                qgmb = pp.cgbo * vgmb;
                qgmid = qgdo + qgso + qgmb;
                qbulk -= qgmb;
                qdrn = -(qgate + qgmid + qbulk + qsrc);
            } else {
                gcggb = (h.cggb + cgdo + cgso + pp.cgbo) * ag0;
                gcgdb = (h.cgsb - cgdo) * ag0;
                gcgsb = (h.cgdb - cgso) * ag0;
                gcgbb = -(gcggb + gcgdb + gcgsb);
                gcdgb = -(h.cggb + h.cbgb + h.cdgb + cgdo) * ag0;
                gcsgb = (h.cdgb - cgso) * ag0;
                gcbgb = (h.cbgb - pp.cgbo) * ag0;
                gcbgmb = 0.0;
                gcsgmb = gcbgmb;
                gcdgmb = gcsgmb;
                qgb = pp.cgbo * vgb;
                qgate += qgdo + qgso + qgb;
                qbulk -= qgb;
                qdrn = -(qgate + qbulk + qsrc);
            }
            gcddb = (h.capbd + cgdo - (h.cgsb + h.cbsb + h.cdsb)) * ag0;
            gcdsb = -(h.cgdb + h.cbdb + h.cddb) * ag0;
            gcsdb = h.cdsb * ag0;
            gcssb = (h.cddb + h.capbs + cgso) * ag0;
            if h.rbody_mod == 0 {
                gcdbb = -(gcdgb + gcddb + gcdsb + gcdgmb);
                gcsbb = -(gcsgb + gcsdb + gcssb + gcsgmb);
                gcbdb = (h.cbsb - h.capbd) * ag0;
                gcbsb = (h.cbdb - h.capbs) * ag0;
                gcdbdb = 0.0;
                gcsbsb = 0.0;
            } else {
                gcdbb = -(gcdgb + gcddb + gcdsb + gcdgmb) + h.capbd * ag0;
                gcsbb = -(h.cddb + h.cdgb + h.cdsb) * ag0;
                gcbdb = h.cbsb * ag0;
                gcbsb = h.cbdb * ag0;
                gcdbdb = -h.capbd * ag0;
                gcsbsb = -h.capbs * ag0;
            }
            gcbbb = -(gcbgb + gcbdb + gcbsb + gcbgmb);
            ggts = 0.0;
            ggtb = ggts;
            ggtd = ggtb;
            ggtg = ggtd;
            sxpart = 0.4;
            dxpart = 0.6;
            ddxpart_dvs = 0.0;
            ddxpart_dvb = ddxpart_dvs;
            ddxpart_dvg = ddxpart_dvb;
            ddxpart_dvd = ddxpart_dvg;
            dsxpart_dvs = 0.0;
            dsxpart_dvb = dsxpart_dvs;
            dsxpart_dvg = dsxpart_dvb;
            dsxpart_dvd = dsxpart_dvg;
        } else {
            qcheq = h.qchqs;
            coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
            t0 = qdef * SCALING_FACTOR / coxwl;
            h.gtg = t0 * h.gcrgg;
            ggtg = h.gtg;
            h.gts = t0 * h.gcrgd;
            ggts = h.gts;
            h.gtd = t0 * h.gcrgs;
            ggtd = h.gtd;
            h.gtb = t0 * h.gcrgb;
            ggtb = h.gtb;
            gqdef = SCALING_FACTOR * ag0;
            gcqgb = h.cqgb * ag0;
            gcqdb = h.cqsb * ag0;
            gcqsb = h.cqdb * ag0;
            gcqbb = h.cqbb * ag0;
            if fabs(qcheq) <= 1.0e-5 * coxwl {
                if md.xpart < 0.5 {
                    sxpart = 0.4;
                } else if md.xpart > 0.5 {
                    sxpart = 0.0;
                } else {
                    sxpart = 0.5;
                }
                dsxpart_dvs = 0.0;
                dsxpart_dvb = dsxpart_dvs;
                dsxpart_dvg = dsxpart_dvb;
                dsxpart_dvd = dsxpart_dvg;
            } else {
                sxpart = qdrn / qcheq;
                css = h.cddb;
                cds = -(h.cgdb + h.cddb + h.cbdb);
                dsxpart_dvs = (css - sxpart * (css + cds)) / qcheq;
                csg = h.cdgb;
                cdg = -(h.cggb + h.cdgb + h.cbgb);
                dsxpart_dvg = (csg - sxpart * (csg + cdg)) / qcheq;
                csd = h.cdsb;
                cdd = -(h.cgsb + h.cdsb + h.cbsb);
                dsxpart_dvd = (csd - sxpart * (csd + cdd)) / qcheq;
                dsxpart_dvb = -(dsxpart_dvd + dsxpart_dvg + dsxpart_dvs);
            }
            dxpart = 1.0 - sxpart;
            ddxpart_dvd = -dsxpart_dvd;
            ddxpart_dvg = -dsxpart_dvg;
            ddxpart_dvs = -dsxpart_dvs;
            ddxpart_dvb = -(ddxpart_dvd + ddxpart_dvg + ddxpart_dvs);
            if h.rgate_mod == 3 {
                gcgmgmb = (cgdo + cgso + pp.cgbo) * ag0;
                gcgmdb = -cgdo * ag0;
                gcgmsb = -cgso * ag0;
                gcgmbb = -pp.cgbo * ag0;
                gcdgmb = gcgmdb;
                gcsgmb = gcgmsb;
                gcbgmb = gcgmbb;
                gcbgb = 0.0;
                gcsgb = gcbgb;
                gcdgb = gcsgb;
                gcgbb = 0.0;
                gcgsb = gcgbb;
                gcgdb = gcgsb;
                gcggb = gcgdb;
                qgmb = pp.cgbo * vgmb;
                qgmid = qgdo + qgso + qgmb;
                qgate = 0.0;
                qbulk = -qgmb;
                qdrn = -qgdo;
                qsrc = -qgso;
            } else {
                gcggb = (cgdo + cgso + pp.cgbo) * ag0;
                gcgdb = -cgdo * ag0;
                gcgsb = -cgso * ag0;
                gcgbb = -pp.cgbo * ag0;
                gcdgb = gcgdb;
                gcsgb = gcgsb;
                gcbgb = gcgbb;
                gcbgmb = 0.0;
                gcsgmb = gcbgmb;
                gcdgmb = gcsgmb;
                qgb = pp.cgbo * vgb;
                qgate = qgdo + qgso + qgb;
                qbulk = -qgb;
                qdrn = -qgdo;
                qsrc = -qgso;
            }
            gcddb = (h.capbd + cgdo) * ag0;
            gcsdb = 0.0;
            gcdsb = gcsdb;
            gcssb = (h.capbs + cgso) * ag0;
            if h.rbody_mod == 0 {
                gcdbb = -(gcdgb + gcddb + gcdgmb);
                gcsbb = -(gcsgb + gcssb + gcsgmb);
                gcbdb = -h.capbd * ag0;
                gcbsb = -h.capbs * ag0;
                gcdbdb = 0.0;
                gcsbsb = 0.0;
            } else {
                gcbsb = 0.0;
                gcbdb = gcbsb;
                gcsbb = gcbdb;
                gcdbb = gcsbb;
                gcdbdb = -h.capbd * ag0;
                gcsbsb = -h.capbs * ag0;
            }
            gcbbb = -(gcbdb + gcbgb + gcbsb + gcbgmb);
        }
        if h.trnqs_mod != 0 {
            ctx.states.set(0, st + ST_QCDUMP, qdef * SCALING_FACTOR);
            if ck_mode & MODEINITTRAN != 0 {
                ctx.states
                    .set(1, st + ST_QCDUMP, ctx.states.get(0, st + ST_QCDUMP));
            }
            let _ = ctx.integrate(0.0, st + ST_QCDUMP);
        }
        ctx.states.set(0, st + ST_QG, qgate);
        ctx.states
            .set(0, st + ST_QD, qdrn - ctx.states.get(0, st + ST_QBD));
        ctx.states
            .set(0, st + ST_QS, qsrc - ctx.states.get(0, st + ST_QBS));
        if h.rgate_mod == 3 {
            ctx.states.set(0, st + ST_QGMID, qgmid);
        }
        if h.rbody_mod == 0 {
            ctx.states.set(
                0,
                st + ST_QB,
                qbulk + ctx.states.get(0, st + ST_QBD) + ctx.states.get(0, st + ST_QBS),
            );
        } else {
            ctx.states.set(0, st + ST_QB, qbulk);
        }
        if ck_mode & MODEINITSMSIG != 0 {
            path = 1000;
        } else if charge_computation_needed == 0 || ck_mode & MODEDCTRANCURVE != 0 {
            path = 850;
        } else {
            if ck_mode & MODEINITTRAN != 0 {
                ctx.states.set(1, st + ST_QB, ctx.states.get(0, st + ST_QB));
                ctx.states.set(1, st + ST_QG, ctx.states.get(0, st + ST_QG));
                ctx.states.set(1, st + ST_QD, ctx.states.get(0, st + ST_QD));
                if h.rgate_mod == 3 {
                    ctx.states
                        .set(1, st + ST_QGMID, ctx.states.get(0, st + ST_QGMID));
                }
                if h.rbody_mod != 0 {
                    ctx.states
                        .set(1, st + ST_QBS, ctx.states.get(0, st + ST_QBS));
                    ctx.states
                        .set(1, st + ST_QBD, ctx.states.get(0, st + ST_QBD));
                }
            }
            let _ = ctx.integrate(0.0, st + ST_QB);
            let _ = ctx.integrate(0.0, st + ST_QG);
            let _ = ctx.integrate(0.0, st + ST_QD);
            if h.rgate_mod == 3 {
                let _ = ctx.integrate(0.0, st + ST_QGMID);
            }
            if h.rbody_mod != 0 {
                let _ = ctx.integrate(0.0, st + ST_QBS);
                let _ = ctx.integrate(0.0, st + ST_QBD);
            }
            path = 860;
        }
    }
    if path == 850 {
        ceqqd = 0.0;
        ceqqb = ceqqd;
        ceqqg = ceqqb;
        ceqqjs = 0.0;
        ceqqjd = ceqqjs;
        cqdef = 0.0;
        cqcheq = cqdef;
        gcdbb = 0.0;
        gcdsb = gcdbb;
        gcddb = gcdsb;
        gcdgb = gcddb;
        gcsbb = 0.0;
        gcssb = gcsbb;
        gcsdb = gcssb;
        gcsgb = gcsdb;
        gcgbb = 0.0;
        gcgsb = gcgbb;
        gcgdb = gcgsb;
        gcggb = gcgdb;
        gcbbb = 0.0;
        gcbsb = gcbbb;
        gcbgb = gcbsb;
        gcbdb = gcbgb;
        gcgmbb = 0.0;
        gcgmsb = gcgmbb;
        gcgmdb = gcgmsb;
        gcgmgmb = gcgmdb;
        ceqqgmid = 0.0;
        gcbgmb = ceqqgmid;
        gcsgmb = gcbgmb;
        gcdgmb = gcsgmb;
        gcsbsb = 0.0;
        gcdbdb = gcsbsb;
        gcqbb = 0.0;
        gcqsb = gcqbb;
        gcqdb = gcqsb;
        gcqgb = gcqdb;
        gqdef = gcqgb;
        ggts = 0.0;
        ggtb = ggts;
        ggtd = ggtb;
        ggtg = ggtd;
        dxpart = if h.mode > 0 { 0.4 } else { 0.6 };
        sxpart = 1.0 - dxpart;
        ddxpart_dvs = 0.0;
        ddxpart_dvb = ddxpart_dvs;
        ddxpart_dvg = ddxpart_dvb;
        ddxpart_dvd = ddxpart_dvg;
        dsxpart_dvs = 0.0;
        dsxpart_dvb = dsxpart_dvs;
        dsxpart_dvg = dsxpart_dvb;
        dsxpart_dvd = dsxpart_dvg;
        if h.trnqs_mod != 0 {
            coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
            t1 = h.gcrg / coxwl;
            h.gtau = t1 * SCALING_FACTOR;
        } else {
            h.gtau = 0.0;
        }
    } else if path == 860 {
        cqgate = ctx.states.get(0, st + ST_CQG);
        cqbody = ctx.states.get(0, st + ST_CQB);
        cqdrn = ctx.states.get(0, st + ST_CQD);
        ceqqg = cqgate - gcggb * vgb + gcgdb * vbd + gcgsb * vbs;
        ceqqd = cqdrn - gcdgb * vgb - gcdgmb * vgmb + (gcddb + gcdbdb) * vbd - gcdbdb * vbd_jct
            + gcdsb * vbs;
        ceqqb = cqbody - gcbgb * vgb - gcbgmb * vgmb + gcbdb * vbd + gcbsb * vbs;
        if h.rgate_mod == 3 {
            ceqqgmid =
                ctx.states.get(0, st + ST_CQGMID) + gcgmdb * vbd + gcgmsb * vbs - gcgmgmb * vgmb;
        } else {
            ceqqgmid = 0.0;
        }
        if h.rbody_mod != 0 {
            ceqqjs = ctx.states.get(0, st + ST_CQBS) + gcsbsb * vbs_jct;
            ceqqjd = ctx.states.get(0, st + ST_CQBD) + gcdbdb * vbd_jct;
        }
        if h.trnqs_mod != 0 {
            t0 = ggtg * vgb - ggtd * vbd - ggts * vbs;
            ceqqg += t0;
            t1 = qdef * h.gtau;
            ceqqd -= dxpart * t0 + t1 * (ddxpart_dvg * vgb - ddxpart_dvd * vbd - ddxpart_dvs * vbs);
            cqdef = ctx.states.get(0, st + ST_CQCDUMP) - gqdef * qdef;
            cqcheq =
                ctx.states.get(0, st + ST_CQCHEQ) - (gcqgb * vgb - gcqdb * vbd - gcqsb * vbs) + t0;
        }
        if ck_mode & MODEINITTRAN != 0 {
            ctx.states
                .set(1, st + ST_CQB, ctx.states.get(0, st + ST_CQB));
            ctx.states
                .set(1, st + ST_CQG, ctx.states.get(0, st + ST_CQG));
            ctx.states
                .set(1, st + ST_CQD, ctx.states.get(0, st + ST_CQD));
            if h.rgate_mod == 3 {
                ctx.states
                    .set(1, st + ST_CQGMID, ctx.states.get(0, st + ST_CQGMID));
            }
            if h.rbody_mod != 0 {
                ctx.states
                    .set(1, st + ST_CQBS, ctx.states.get(0, st + ST_CQBS));
                ctx.states
                    .set(1, st + ST_CQBD, ctx.states.get(0, st + ST_CQBD));
            }
        }
    }
    if path != 1000 {
        if h.mode >= 0 {
            gm = h.gm;
            gmbs = h.gmbs;
            fwdsum = gm + gmbs;
            revsum = 0.0;
            ceqdrn = md.type_ as f64 * (cdrain - h.gds * vds - gm * vgs - gmbs * vbs);
            ceqbd = md.type_ as f64
                * (h.csub + h.igidl
                    - (h.gbds + h.ggidld) * vds
                    - (h.gbgs + h.ggidlg) * vgs
                    - (h.gbbs + h.ggidlb) * vbs);
            ceqbs = md.type_ as f64 * (h.igisl + h.ggisls * vds - h.ggislg * vgd - h.ggislb * vbd);
            gbbdp = -h.gbds;
            gbbsp = h.gbds + h.gbgs + h.gbbs;
            gbdpg = h.gbgs;
            gbdpdp = h.gbds;
            gbdpb = h.gbbs;
            gbdpsp = -(gbdpg + gbdpdp + gbdpb);
            gbspg = 0.0;
            gbspdp = 0.0;
            gbspb = 0.0;
            gbspsp = 0.0;
            if md.igc_mod != 0 {
                gistotg = h.g_igsg + h.g_igcsg;
                gistotd = h.g_igcsd;
                gistots = h.g_igss + h.g_igcss;
                gistotb = h.g_igcsb;
                istoteq = md.type_ as f64
                    * (h.igs + h.igcs - gistotg * vgs - h.g_igcsd * vds - h.g_igcsb * vbs);
                gidtotg = h.g_igdg + h.g_igcdg;
                gidtotd = h.g_igdd + h.g_igcdd;
                gidtots = h.g_igcds;
                gidtotb = h.g_igcdb;
                idtoteq = md.type_ as f64
                    * (h.igd + h.igcd
                        - h.g_igdg * vgd
                        - h.g_igcdg * vgs
                        - h.g_igcdd * vds
                        - h.g_igcdb * vbs);
            } else {
                istoteq = 0.0;
                gistotb = istoteq;
                gistots = gistotb;
                gistotd = gistots;
                gistotg = gistotd;
                idtoteq = 0.0;
                gidtotb = idtoteq;
                gidtots = gidtotb;
                gidtotd = gidtots;
                gidtotg = gidtotd;
            }
            if md.igb_mod != 0 {
                gibtotg = h.g_igbg;
                gibtotd = h.g_igbd;
                gibtots = h.g_igbs;
                gibtotb = h.g_igbb;
                ibtoteq =
                    md.type_ as f64 * (h.igb - h.g_igbg * vgs - h.g_igbd * vds - h.g_igbb * vbs);
            } else {
                ibtoteq = 0.0;
                gibtotb = ibtoteq;
                gibtots = gibtotb;
                gibtotd = gibtots;
                gibtotg = gibtotd;
            }
            if md.igc_mod != 0 || md.igb_mod != 0 {
                gigtotg = gistotg + gidtotg + gibtotg;
                gigtotd = gistotd + gidtotd + gibtotd;
                gigtots = gistots + gidtots + gibtots;
                gigtotb = gistotb + gidtotb + gibtotb;
                igtoteq = istoteq + idtoteq + ibtoteq;
            } else {
                igtoteq = 0.0;
                gigtotb = igtoteq;
                gigtots = gigtotb;
                gigtotd = gigtots;
                gigtotg = gigtotd;
            }
            if h.rgate_mod == 2 {
                t0 = vges - vgs;
            } else if h.rgate_mod == 3 {
                t0 = vgms - vgs;
            }
            if h.rgate_mod > 1 {
                gcrgd = h.gcrgd * t0;
                gcrgg = h.gcrgg * t0;
                gcrgs = h.gcrgs * t0;
                gcrgb = h.gcrgb * t0;
                ceqgcrg = -(gcrgd * vds + gcrgg * vgs + gcrgb * vbs);
                gcrgg -= h.gcrg;
                gcrg = h.gcrg;
            } else {
                gcrgb = 0.0;
                gcrgs = gcrgb;
                gcrgg = gcrgs;
                gcrgd = gcrgg;
                gcrg = gcrgd;
                ceqgcrg = gcrg;
            }
        } else {
            gm = -h.gm;
            gmbs = -h.gmbs;
            fwdsum = 0.0;
            revsum = -(gm + gmbs);
            ceqdrn = -(md.type_ as f64) * (cdrain + h.gds * vds + gm * vgd + gmbs * vbd);
            ceqbs = md.type_ as f64
                * (h.csub + h.igisl + (h.gbds + h.ggisls) * vds
                    - (h.gbgs + h.ggislg) * vgd
                    - (h.gbbs + h.ggislb) * vbd);
            ceqbd = md.type_ as f64 * (h.igidl - h.ggidld * vds - h.ggidlg * vgs - h.ggidlb * vbs);
            gbbsp = -h.gbds;
            gbbdp = h.gbds + h.gbgs + h.gbbs;
            gbdpg = 0.0;
            gbdpsp = 0.0;
            gbdpb = 0.0;
            gbdpdp = 0.0;
            gbspg = h.gbgs;
            gbspsp = h.gbds;
            gbspb = h.gbbs;
            gbspdp = -(gbspg + gbspsp + gbspb);
            if md.igc_mod != 0 {
                gistotg = h.g_igsg + h.g_igcdg;
                gistotd = h.g_igcds;
                gistots = h.g_igss + h.g_igcdd;
                gistotb = h.g_igcdb;
                istoteq = md.type_ as f64
                    * (h.igs + h.igcd - h.g_igsg * vgs - h.g_igcdg * vgd + h.g_igcdd * vds
                        - h.g_igcdb * vbd);
                gidtotg = h.g_igdg + h.g_igcsg;
                gidtotd = h.g_igdd + h.g_igcss;
                gidtots = h.g_igcsd;
                gidtotb = h.g_igcsb;
                idtoteq = md.type_ as f64
                    * (h.igd + h.igcs - (h.g_igdg + h.g_igcsg) * vgd + h.g_igcsd * vds
                        - h.g_igcsb * vbd);
            } else {
                istoteq = 0.0;
                gistotb = istoteq;
                gistots = gistotb;
                gistotd = gistots;
                gistotg = gistotd;
                idtoteq = 0.0;
                gidtotb = idtoteq;
                gidtots = gidtotb;
                gidtotd = gidtots;
                gidtotg = gidtotd;
            }
            if md.igb_mod != 0 {
                gibtotg = h.g_igbg;
                gibtotd = h.g_igbs;
                gibtots = h.g_igbd;
                gibtotb = h.g_igbb;
                ibtoteq =
                    md.type_ as f64 * (h.igb - h.g_igbg * vgd + h.g_igbd * vds - h.g_igbb * vbd);
            } else {
                ibtoteq = 0.0;
                gibtotb = ibtoteq;
                gibtots = gibtotb;
                gibtotd = gibtots;
                gibtotg = gibtotd;
            }
            if md.igc_mod != 0 || md.igb_mod != 0 {
                gigtotg = gistotg + gidtotg + gibtotg;
                gigtotd = gistotd + gidtotd + gibtotd;
                gigtots = gistots + gidtots + gibtots;
                gigtotb = gistotb + gidtotb + gibtotb;
                igtoteq = istoteq + idtoteq + ibtoteq;
            } else {
                igtoteq = 0.0;
                gigtotb = igtoteq;
                gigtots = gigtotb;
                gigtotd = gigtots;
                gigtotg = gigtotd;
            }
            if h.rgate_mod == 2 {
                t0 = vges - vgs;
            } else if h.rgate_mod == 3 {
                t0 = vgms - vgs;
            }
            if h.rgate_mod > 1 {
                gcrgd = h.gcrgs * t0;
                gcrgg = h.gcrgg * t0;
                gcrgs = h.gcrgd * t0;
                gcrgb = h.gcrgb * t0;
                ceqgcrg = -(gcrgg * vgd - gcrgs * vds + gcrgb * vbd);
                gcrgg -= h.gcrg;
                gcrg = h.gcrg;
            } else {
                gcrgb = 0.0;
                gcrgs = gcrgb;
                gcrgg = gcrgs;
                gcrgd = gcrgg;
                gcrg = gcrgd;
                ceqgcrg = gcrg;
            }
        }
        if md.rds_mod == 1 {
            ceqgstot = md.type_ as f64 * (h.gstotd * vds + h.gstotg * vgs + h.gstotb * vbs);
            gstot = h.gstot;
            gstotd = h.gstotd;
            gstotg = h.gstotg;
            gstots = h.gstots - gstot;
            gstotb = h.gstotb;
            ceqgdtot = -(md.type_ as f64) * (h.gdtotd * vds + h.gdtotg * vgs + h.gdtotb * vbs);
            gdtot = h.gdtot;
            gdtotd = h.gdtotd - gdtot;
            gdtotg = h.gdtotg;
            gdtots = h.gdtots;
            gdtotb = h.gdtotb;
        } else {
            ceqgstot = 0.0;
            gstotb = ceqgstot;
            gstots = gstotb;
            gstotg = gstots;
            gstotd = gstotg;
            gstot = gstotd;
            ceqgdtot = 0.0;
            gdtotb = ceqgdtot;
            gdtots = gdtotb;
            gdtotg = gdtots;
            gdtotd = gdtotg;
            gdtot = gdtotd;
        }
        if md.type_ > 0 {
            ceqjs = h.cbs - h.gbs * vbs_jct;
            ceqjd = h.cbd - h.gbd * vbd_jct;
        } else {
            ceqjs = -(h.cbs - h.gbs * vbs_jct);
            ceqjd = -(h.cbd - h.gbd * vbd_jct);
            ceqqg = -ceqqg;
            ceqqd = -ceqqd;
            ceqqb = -ceqqb;
            ceqgcrg = -ceqgcrg;
            if h.trnqs_mod != 0 {
                cqdef = -cqdef;
                cqcheq = -cqcheq;
            }
            if h.rbody_mod != 0 {
                ceqqjs = -ceqqjs;
                ceqqjd = -ceqqjd;
            }
            if h.rgate_mod == 3 {
                ceqqgmid = -ceqqgmid;
            }
        }
        m = h.m;
        mult_i = h.mult_i * h.m;
        mult_q = h.mult_q * h.m;
        ctx.add_rhs(
            h.d_node_prime,
            mult_i * (ceqjd - ceqbd + ceqgdtot - ceqdrn + idtoteq) - mult_q * ceqqd,
        );
        ctx.add_rhs(
            h.g_node_prime,
            -(mult_q * ceqqg - mult_i * (ceqgcrg - igtoteq)),
        );
        if h.rgate_mod == 2 {
            ctx.add_rhs(h.g_node_ext, -(mult_i * ceqgcrg));
        } else if h.rgate_mod == 3 {
            ctx.add_rhs(h.g_node_mid, -(mult_q * ceqqgmid + mult_i * ceqgcrg));
        }
        if h.rbody_mod == 0 {
            ctx.add_rhs(
                h.b_node_prime,
                mult_i * (ceqbd + ceqbs - ceqjd - ceqjs + ibtoteq) - mult_q * ceqqb,
            );
            ctx.add_rhs(
                h.s_node_prime,
                mult_i * (ceqdrn - ceqbs + ceqjs - ceqgstot + istoteq)
                    + mult_q * (ceqqg + ceqqb + ceqqd + ceqqgmid),
            );
        } else {
            ctx.add_rhs(h.db_node, -(mult_i * ceqjd + mult_q * ceqqjd));
            ctx.add_rhs(
                h.b_node_prime,
                mult_i * (ceqbd + ceqbs + ibtoteq) - mult_q * ceqqb,
            );
            ctx.add_rhs(h.sb_node, -(mult_i * ceqjs + mult_q * ceqqjs));
            ctx.add_rhs(
                h.s_node_prime,
                mult_i * (ceqdrn - ceqbs + ceqjs - ceqgstot + istoteq)
                    + mult_q * (ceqqd + ceqqg + ceqqb + ceqqjd + ceqqjs + ceqqgmid),
            );
        }
        if md.rds_mod != 0 {
            ctx.add_rhs(h.d_node, -(mult_i * ceqgdtot));
            ctx.add_rhs(h.s_node, mult_i * ceqgstot);
        }
        if h.trnqs_mod != 0 {
            ctx.add_rhs(h.q_node, m * (cqcheq - cqdef));
        }
        if h.rbody_mod == 0 {
            gjbd = h.gbd;
            gjbs = h.gbs;
        } else {
            gjbs = 0.0;
            gjbd = gjbs;
        }
        if md.rds_mod == 0 {
            gdpr = h.drain_conductance;
            gspr = h.source_conductance;
        } else {
            gspr = 0.0;
            gdpr = gspr;
        }
        geltd = h.grgeltd;
        t1 = qdef * h.gtau;
        if h.rgate_mod == 1 {
            ctx.add(h.gege_ptr, mult_i * geltd);
            ctx.add(h.gpge_ptr, -(mult_i * geltd));
            ctx.add(h.gegp_ptr, -(mult_i * geltd));
            ctx.add(
                h.gpgp_ptr,
                mult_q * (gcggb - ggtg) + mult_i * (geltd + gigtotg),
            );
            ctx.add(h.gpdp_ptr, mult_q * (gcgdb - ggtd) + mult_i * gigtotd);
            ctx.add(h.gpsp_ptr, mult_q * (gcgsb - ggts) + mult_i * gigtots);
            ctx.add(h.gpbp_ptr, mult_q * (gcgbb - ggtb) + mult_i * gigtotb);
        } else if h.rgate_mod == 2 {
            ctx.add(h.gege_ptr, mult_i * gcrg);
            ctx.add(h.gegp_ptr, mult_i * gcrgg);
            ctx.add(h.gedp_ptr, mult_i * gcrgd);
            ctx.add(h.gesp_ptr, mult_i * gcrgs);
            ctx.add(h.gebp_ptr, mult_i * gcrgb);
            ctx.add(h.gpge_ptr, -(mult_i * gcrg));
            ctx.add(
                h.gpgp_ptr,
                mult_q * (gcggb - ggtg) + mult_i * (gigtotg - gcrgg),
            );
            ctx.add(
                h.gpdp_ptr,
                mult_q * (gcgdb - ggtd) + mult_i * (gigtotd - gcrgd),
            );
            ctx.add(
                h.gpsp_ptr,
                mult_q * (gcgsb - ggts) + mult_i * (gigtots - gcrgs),
            );
            ctx.add(
                h.gpbp_ptr,
                mult_q * (gcgbb - ggtb) + mult_i * (gigtotb - gcrgb),
            );
        } else if h.rgate_mod == 3 {
            ctx.add(h.gege_ptr, mult_i * geltd);
            ctx.add(h.gegm_ptr, -(mult_i * geltd));
            ctx.add(h.gmge_ptr, -(mult_i * geltd));
            ctx.add(h.gmgm_ptr, mult_i * (geltd + gcrg) + mult_q * gcgmgmb);
            ctx.add(h.gmdp_ptr, mult_i * gcrgd + mult_q * gcgmdb);
            ctx.add(h.gmgp_ptr, mult_i * gcrgg);
            ctx.add(h.gmsp_ptr, mult_i * gcrgs + mult_q * gcgmsb);
            ctx.add(h.gmbp_ptr, mult_i * gcrgb + mult_q * gcgmbb);
            ctx.add(h.dpgm_ptr, mult_q * gcdgmb);
            ctx.add(h.gpgm_ptr, -(mult_i * gcrg));
            ctx.add(h.spgm_ptr, mult_q * gcsgmb);
            ctx.add(h.bpgm_ptr, mult_q * gcbgmb);
            ctx.add(
                h.gpgp_ptr,
                mult_q * (gcggb - ggtg) + mult_i * (gigtotg - gcrgg),
            );
            ctx.add(
                h.gpdp_ptr,
                mult_q * (gcgdb - ggtd) + mult_i * (gigtotd - gcrgd),
            );
            ctx.add(
                h.gpsp_ptr,
                mult_q * (gcgsb - ggts) + mult_i * (gigtots - gcrgs),
            );
            ctx.add(
                h.gpbp_ptr,
                mult_q * (gcgbb - ggtb) + mult_i * (gigtotb - gcrgb),
            );
        } else {
            ctx.add(h.gpgp_ptr, mult_q * (gcggb - ggtg) + mult_i * gigtotg);
            ctx.add(h.gpdp_ptr, mult_q * (gcgdb - ggtd) + mult_i * gigtotd);
            ctx.add(h.gpsp_ptr, mult_q * (gcgsb - ggts) + mult_i * gigtots);
            ctx.add(h.gpbp_ptr, mult_q * (gcgbb - ggtb) + mult_i * gigtotb);
        }
        if md.rds_mod != 0 {
            ctx.add(h.dgp_ptr, mult_i * gdtotg);
            ctx.add(h.dsp_ptr, mult_i * gdtots);
            ctx.add(h.dbp_ptr, mult_i * gdtotb);
            ctx.add(h.sdp_ptr, mult_i * gstotd);
            ctx.add(h.sgp_ptr, mult_i * gstotg);
            ctx.add(h.sbp_ptr, mult_i * gstotb);
        }
        ctx.add(
            h.dpdp_ptr,
            mult_i * (gdpr + h.gds + h.gbd - gdtotd + revsum + gbdpdp - gidtotd)
                + mult_q * (t1 * ddxpart_dvd + gcddb + dxpart * ggtd),
        );
        ctx.add(h.dpd_ptr, -(mult_i * (gdpr + gdtot)));
        ctx.add(
            h.dpgp_ptr,
            mult_i * (gm - gdtotg + gbdpg - gidtotg)
                + mult_q * (dxpart * ggtg + t1 * ddxpart_dvg + gcdgb),
        );
        ctx.add(
            h.dpsp_ptr,
            -(mult_i * (h.gds + gdtots + gidtots + fwdsum - gbdpsp)
                - mult_q * (dxpart * ggts + t1 * ddxpart_dvs + gcdsb)),
        );
        ctx.add(
            h.dpbp_ptr,
            -(mult_i * (gjbd + gdtotb - gmbs - gbdpb + gidtotb)
                - mult_q * (dxpart * ggtb + gcdbb + t1 * ddxpart_dvb)),
        );
        ctx.add(h.ddp_ptr, -(mult_i * (gdpr - gdtotd)));
        ctx.add(h.dd_ptr, mult_i * (gdpr + gdtot));
        ctx.add(
            h.spdp_ptr,
            -(mult_i * (h.gds + gstotd + revsum - gbspdp + gistotd)
                - mult_q * (t1 * dsxpart_dvd + sxpart * ggtd + gcsdb)),
        );
        ctx.add(
            h.spgp_ptr,
            mult_q * (gcsgb + sxpart * ggtg + t1 * dsxpart_dvg)
                + mult_i * (gbspg - gm - gstotg - gistotg),
        );
        ctx.add(
            h.spsp_ptr,
            mult_i * (gspr + h.gds + h.gbs - gistots - gstots + fwdsum + gbspsp)
                + mult_q * (sxpart * ggts + t1 * dsxpart_dvs + gcssb),
        );
        ctx.add(h.sps_ptr, -(mult_i * (gspr + gstot)));
        ctx.add(
            h.spbp_ptr,
            -(mult_i * (gjbs + gstotb + gmbs - gbspb + gistotb)
                - mult_q * (gcsbb + sxpart * ggtb + t1 * dsxpart_dvb)),
        );
        ctx.add(h.ssp_ptr, -(mult_i * (gspr - gstots)));
        ctx.add(h.ss_ptr, mult_i * (gspr + gstot));
        ctx.add(
            h.bpdp_ptr,
            mult_q * gcbdb - mult_i * (gjbd - gbbdp + gibtotd),
        );
        ctx.add(h.bpgp_ptr, mult_q * gcbgb - mult_i * (h.gbgs + gibtotg));
        ctx.add(
            h.bpsp_ptr,
            mult_q * gcbsb - mult_i * (gjbs - gbbsp + gibtots),
        );
        ctx.add(
            h.bpbp_ptr,
            mult_i * (gjbd + gjbs - h.gbbs - gibtotb) + mult_q * gcbbb,
        );
        ggidld = h.ggidld;
        ggidlg = h.ggidlg;
        ggidlb = h.ggidlb;
        ggislg = h.ggislg;
        ggisls = h.ggisls;
        ggislb = h.ggislb;
        ctx.add(h.dpdp_ptr, mult_i * ggidld);
        ctx.add(h.dpgp_ptr, mult_i * ggidlg);
        ctx.add(h.dpsp_ptr, -(mult_i * (ggidlg + ggidld + ggidlb)));
        ctx.add(h.dpbp_ptr, mult_i * ggidlb);
        ctx.add(h.bpdp_ptr, -(mult_i * ggidld));
        ctx.add(h.bpgp_ptr, -(mult_i * ggidlg));
        ctx.add(h.bpsp_ptr, mult_i * (ggidlg + ggidld + ggidlb));
        ctx.add(h.bpbp_ptr, -(mult_i * ggidlb));
        ctx.add(h.spdp_ptr, -(mult_i * (ggisls + ggislg + ggislb)));
        ctx.add(h.spgp_ptr, mult_i * ggislg);
        ctx.add(h.spsp_ptr, mult_i * ggisls);
        ctx.add(h.spbp_ptr, mult_i * ggislb);
        ctx.add(h.bpdp_ptr, mult_i * (ggislg + ggisls + ggislb));
        ctx.add(h.bpgp_ptr, -(mult_i * ggislg));
        ctx.add(h.bpsp_ptr, -(mult_i * ggisls));
        ctx.add(h.bpbp_ptr, -(mult_i * ggislb));
        if h.rbody_mod != 0 {
            ctx.add(h.dpdb_ptr, mult_q * gcdbdb - mult_i * h.gbd);
            ctx.add(h.spsb_ptr, -(mult_i * h.gbs - mult_q * gcsbsb));
            ctx.add(h.dbdp_ptr, mult_q * gcdbdb - mult_i * h.gbd);
            ctx.add(
                h.dbdb_ptr,
                mult_i * (h.gbd + h.grbpd + h.grbdb) - mult_q * gcdbdb,
            );
            ctx.add(h.dbbp_ptr, -(mult_i * h.grbpd));
            ctx.add(h.dbb_ptr, -(mult_i * h.grbdb));
            ctx.add(h.bpdb_ptr, -(mult_i * h.grbpd));
            ctx.add(h.bpb_ptr, -(mult_i * h.grbpb));
            ctx.add(h.bpsb_ptr, -(mult_i * h.grbps));
            ctx.add(h.bpbp_ptr, mult_i * (h.grbpd + h.grbps + h.grbpb));
            ctx.add(h.sbsp_ptr, mult_q * gcsbsb - mult_i * h.gbs);
            ctx.add(h.sbbp_ptr, -(mult_i * h.grbps));
            ctx.add(h.sbb_ptr, -(mult_i * h.grbsb));
            ctx.add(
                h.sbsb_ptr,
                mult_i * (h.gbs + h.grbps + h.grbsb) - mult_q * gcsbsb,
            );
            ctx.add(h.bdb_ptr, -(mult_i * h.grbdb));
            ctx.add(h.bbp_ptr, -(mult_i * h.grbpb));
            ctx.add(h.bsb_ptr, -(mult_i * h.grbsb));
            ctx.add(h.bb_ptr, mult_i * (h.grbsb + h.grbdb + h.grbpb));
        }
        if h.trnqs_mod != 0 {
            ctx.add(h.qq_ptr, m * (gqdef + h.gtau));
            ctx.add(h.qgp_ptr, m * (ggtg - gcqgb));
            ctx.add(h.qdp_ptr, m * (ggtd - gcqdb));
            ctx.add(h.qsp_ptr, m * (ggts - gcqsb));
            ctx.add(h.qbp_ptr, m * (ggtb - gcqbb));
            ctx.add(h.dpq_ptr, mult_q * (dxpart * h.gtau));
            ctx.add(h.spq_ptr, mult_q * (sxpart * h.gtau));
            ctx.add(h.gpq_ptr, -(mult_q * h.gtau));
        }
    }
}

/// `BSIM4polyDepletion` (`b4ld.c`).
#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn poly_depletion(
    phi: f64,
    ngate: f64,
    epsgate: f64,
    coxe: f64,
    vgs: f64,
    vgs_eff: &mut f64,
    dvgs_eff_dvg: &mut f64,
) -> i32 {
    let mut t1: f64 = 0.0;
    let mut t2: f64 = 0.0;
    let mut t3: f64 = 0.0;
    let mut t4: f64 = 0.0;
    let mut t5: f64 = 0.0;
    let mut t6: f64 = 0.0;
    let mut t7: f64 = 0.0;
    let mut t8: f64 = 0.0;
    if ngate > 1.0e18 && ngate < 1.0e25 && vgs > phi && epsgate != 0.0 {
        t1 = 1.0e6 * CHARGE * epsgate * ngate / (coxe * coxe);
        t8 = vgs - phi;
        t4 = sqrt(1.0 + 2.0 * t8 / t1);
        t2 = 2.0 * t8 / (t4 + 1.0);
        t3 = 0.5 * t2 * t2 / t1;
        t7 = 1.12 - t3 - 0.05;
        t6 = sqrt(t7 * t7 + 0.224);
        t5 = 1.12 - 0.5 * (t7 + t6);
        *vgs_eff = vgs - t5;
        *dvgs_eff_dvg = 1.0 - (0.5 - 0.5 / t4) * (1.0 + t7 / t6);
    } else {
        *vgs_eff = vgs;
        *dvgs_eff_dvg = 1.0;
    }
    0
}
