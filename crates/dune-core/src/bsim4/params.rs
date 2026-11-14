// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4.c, b4mpar.c, b4par.c and bsim4def.h as distributed with ngspice-47
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

//! The BSIM4 4.8.3 data structures of `bsim4def.h` and the keyword
//! setters of `BSIM4mParam` (`b4mpar.c`) and `BSIM4param` (`b4par.c`),
//! generated from the C (field names in snake case after the C).

use super::CONST_CTOK;
use crate::matrix::{Slot, Var};

/// `.model … nmos/pmos (level=14|54 …)`: a BSIM4 4.8.3 model card
/// (`bsim4def.h` `sBSIM4model`). Every real and integer field of the
/// C struct, named in snake case after the C field (`BSIM4capMod` ->
/// `cap_mod`); the fields `BSIM4temp` derives at the circuit
/// temperature live here too, as in the C. Parameters are set by deck
/// keyword through [`Bsim4Model::set`]; every parameter remembers
/// whether it was given, since `b4set.c`/`b4temp.c` branch on that.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub struct Bsim4Model {
    /// The card name (`BSIM4modName`), for messages.
    pub(crate) mod_name: String,
    /// `BSIM4type`
    pub(crate) type_: i32,
    /// `BSIM4mobMod`
    pub(crate) mob_mod: i32,
    /// `BSIM4cvchargeMod`
    pub(crate) cvcharge_mod: i32,
    /// `BSIM4capMod`
    pub(crate) cap_mod: i32,
    /// `BSIM4dioMod`
    pub(crate) dio_mod: i32,
    /// `BSIM4trnqsMod`
    pub(crate) trnqs_mod: i32,
    /// `BSIM4acnqsMod`
    pub(crate) acnqs_mod: i32,
    /// `BSIM4fnoiMod`
    pub(crate) fnoi_mod: i32,
    /// `BSIM4tnoiMod`
    pub(crate) tnoi_mod: i32,
    /// `BSIM4rdsMod`
    pub(crate) rds_mod: i32,
    /// `BSIM4rbodyMod`
    pub(crate) rbody_mod: i32,
    /// `BSIM4rgateMod`
    pub(crate) rgate_mod: i32,
    /// `BSIM4perMod`
    pub(crate) per_mod: i32,
    /// `BSIM4geoMod`
    pub(crate) geo_mod: i32,
    /// `BSIM4mtrlMod`
    pub(crate) mtrl_mod: i32,
    /// `BSIM4mtrlCompatMod`
    pub(crate) mtrl_compat_mod: i32,
    /// `BSIM4gidlMod`
    pub(crate) gidl_mod: i32,
    /// `BSIM4igcMod`
    pub(crate) igc_mod: i32,
    /// `BSIM4igbMod`
    pub(crate) igb_mod: i32,
    /// `BSIM4tempMod`
    pub(crate) temp_mod: i32,
    /// `BSIM4binUnit`
    pub(crate) bin_unit: i32,
    /// `BSIM4paramChk`
    pub(crate) param_chk: i32,
    /// `BSIM4version`
    pub(crate) version: String,
    /// `BSIM4v48intVersion`
    pub(crate) v48int_version: i32,
    /// `BSIM4eot`
    pub(crate) eot: f64,
    /// `BSIM4vddeot`
    pub(crate) vddeot: f64,
    /// `BSIM4tempeot`
    pub(crate) tempeot: f64,
    /// `BSIM4leffeot`
    pub(crate) leffeot: f64,
    /// `BSIM4weffeot`
    pub(crate) weffeot: f64,
    /// `BSIM4ados`
    pub(crate) ados: f64,
    /// `BSIM4bdos`
    pub(crate) bdos: f64,
    /// `BSIM4toxe`
    pub(crate) toxe: f64,
    /// `BSIM4toxp`
    pub(crate) toxp: f64,
    /// `BSIM4toxm`
    pub(crate) toxm: f64,
    /// `BSIM4dtox`
    pub(crate) dtox: f64,
    /// `BSIM4epsrox`
    pub(crate) epsrox: f64,
    /// `BSIM4cdsc`
    pub(crate) cdsc: f64,
    /// `BSIM4cdscb`
    pub(crate) cdscb: f64,
    /// `BSIM4cdscd`
    pub(crate) cdscd: f64,
    /// `BSIM4cit`
    pub(crate) cit: f64,
    /// `BSIM4nfactor`
    pub(crate) nfactor: f64,
    /// `BSIM4xj`
    pub(crate) xj: f64,
    /// `BSIM4vsat`
    pub(crate) vsat: f64,
    /// `BSIM4at`
    pub(crate) at: f64,
    /// `BSIM4a0`
    pub(crate) a0: f64,
    /// `BSIM4ags`
    pub(crate) ags: f64,
    /// `BSIM4a1`
    pub(crate) a1: f64,
    /// `BSIM4a2`
    pub(crate) a2: f64,
    /// `BSIM4keta`
    pub(crate) keta: f64,
    /// `BSIM4ketac`
    pub(crate) ketac: f64,
    /// `BSIM4nsub`
    pub(crate) nsub: f64,
    /// `BSIM4phig`
    pub(crate) phig: f64,
    /// `BSIM4epsrgate`
    pub(crate) epsrgate: f64,
    /// `BSIM4easub`
    pub(crate) easub: f64,
    /// `BSIM4epsrsub`
    pub(crate) epsrsub: f64,
    /// `BSIM4ni0sub`
    pub(crate) ni0sub: f64,
    /// `BSIM4bg0sub`
    pub(crate) bg0sub: f64,
    /// `BSIM4tbgasub`
    pub(crate) tbgasub: f64,
    /// `BSIM4tbgbsub`
    pub(crate) tbgbsub: f64,
    /// `BSIM4ndep`
    pub(crate) ndep: f64,
    /// `BSIM4nsd`
    pub(crate) nsd: f64,
    /// `BSIM4phin`
    pub(crate) phin: f64,
    /// `BSIM4ngate`
    pub(crate) ngate: f64,
    /// `BSIM4gamma1`
    pub(crate) gamma1: f64,
    /// `BSIM4gamma2`
    pub(crate) gamma2: f64,
    /// `BSIM4vbx`
    pub(crate) vbx: f64,
    /// `BSIM4vbm`
    pub(crate) vbm: f64,
    /// `BSIM4xt`
    pub(crate) xt: f64,
    /// `BSIM4k1`
    pub(crate) k1: f64,
    /// `BSIM4kt1`
    pub(crate) kt1: f64,
    /// `BSIM4kt1l`
    pub(crate) kt1l: f64,
    /// `BSIM4kt2`
    pub(crate) kt2: f64,
    /// `BSIM4k2`
    pub(crate) k2: f64,
    /// `BSIM4k3`
    pub(crate) k3: f64,
    /// `BSIM4k3b`
    pub(crate) k3b: f64,
    /// `BSIM4w0`
    pub(crate) w0: f64,
    /// `BSIM4dvtp0`
    pub(crate) dvtp0: f64,
    /// `BSIM4dvtp1`
    pub(crate) dvtp1: f64,
    /// `BSIM4dvtp2`
    pub(crate) dvtp2: f64,
    /// `BSIM4dvtp3`
    pub(crate) dvtp3: f64,
    /// `BSIM4dvtp4`
    pub(crate) dvtp4: f64,
    /// `BSIM4dvtp5`
    pub(crate) dvtp5: f64,
    /// `BSIM4lpe0`
    pub(crate) lpe0: f64,
    /// `BSIM4lpeb`
    pub(crate) lpeb: f64,
    /// `BSIM4dvt0`
    pub(crate) dvt0: f64,
    /// `BSIM4dvt1`
    pub(crate) dvt1: f64,
    /// `BSIM4dvt2`
    pub(crate) dvt2: f64,
    /// `BSIM4dvt0w`
    pub(crate) dvt0w: f64,
    /// `BSIM4dvt1w`
    pub(crate) dvt1w: f64,
    /// `BSIM4dvt2w`
    pub(crate) dvt2w: f64,
    /// `BSIM4drout`
    pub(crate) drout: f64,
    /// `BSIM4dsub`
    pub(crate) dsub: f64,
    /// `BSIM4vth0`
    pub(crate) vth0: f64,
    /// `BSIM4eu`
    pub(crate) eu: f64,
    /// `BSIM4ucs`
    pub(crate) ucs: f64,
    /// `BSIM4ua`
    pub(crate) ua: f64,
    /// `BSIM4ua1`
    pub(crate) ua1: f64,
    /// `BSIM4ub`
    pub(crate) ub: f64,
    /// `BSIM4ub1`
    pub(crate) ub1: f64,
    /// `BSIM4uc`
    pub(crate) uc: f64,
    /// `BSIM4uc1`
    pub(crate) uc1: f64,
    /// `BSIM4ud`
    pub(crate) ud: f64,
    /// `BSIM4ud1`
    pub(crate) ud1: f64,
    /// `BSIM4up`
    pub(crate) up: f64,
    /// `BSIM4lp`
    pub(crate) lp: f64,
    /// `BSIM4u0`
    pub(crate) u0: f64,
    /// `BSIM4ute`
    pub(crate) ute: f64,
    /// `BSIM4ucste`
    pub(crate) ucste: f64,
    /// `BSIM4voff`
    pub(crate) voff: f64,
    /// `BSIM4tvoff`
    pub(crate) tvoff: f64,
    /// `BSIM4tnfactor`
    pub(crate) tnfactor: f64,
    /// `BSIM4teta0`
    pub(crate) teta0: f64,
    /// `BSIM4tvoffcv`
    pub(crate) tvoffcv: f64,
    /// `BSIM4minv`
    pub(crate) minv: f64,
    /// `BSIM4minvcv`
    pub(crate) minvcv: f64,
    /// `BSIM4voffl`
    pub(crate) voffl: f64,
    /// `BSIM4voffcvl`
    pub(crate) voffcvl: f64,
    /// `BSIM4delta`
    pub(crate) delta: f64,
    /// `BSIM4rdsw`
    pub(crate) rdsw: f64,
    /// `BSIM4rdswmin`
    pub(crate) rdswmin: f64,
    /// `BSIM4rdwmin`
    pub(crate) rdwmin: f64,
    /// `BSIM4rswmin`
    pub(crate) rswmin: f64,
    /// `BSIM4rsw`
    pub(crate) rsw: f64,
    /// `BSIM4rdw`
    pub(crate) rdw: f64,
    /// `BSIM4prwg`
    pub(crate) prwg: f64,
    /// `BSIM4prwb`
    pub(crate) prwb: f64,
    /// `BSIM4prt`
    pub(crate) prt: f64,
    /// `BSIM4eta0`
    pub(crate) eta0: f64,
    /// `BSIM4etab`
    pub(crate) etab: f64,
    /// `BSIM4pclm`
    pub(crate) pclm: f64,
    /// `BSIM4pdibl1`
    pub(crate) pdibl1: f64,
    /// `BSIM4pdibl2`
    pub(crate) pdibl2: f64,
    /// `BSIM4pdiblb`
    pub(crate) pdiblb: f64,
    /// `BSIM4fprout`
    pub(crate) fprout: f64,
    /// `BSIM4pdits`
    pub(crate) pdits: f64,
    /// `BSIM4pditsd`
    pub(crate) pditsd: f64,
    /// `BSIM4pditsl`
    pub(crate) pditsl: f64,
    /// `BSIM4pscbe1`
    pub(crate) pscbe1: f64,
    /// `BSIM4pscbe2`
    pub(crate) pscbe2: f64,
    /// `BSIM4pvag`
    pub(crate) pvag: f64,
    /// `BSIM4wr`
    pub(crate) wr: f64,
    /// `BSIM4dwg`
    pub(crate) dwg: f64,
    /// `BSIM4dwb`
    pub(crate) dwb: f64,
    /// `BSIM4b0`
    pub(crate) b0: f64,
    /// `BSIM4b1`
    pub(crate) b1: f64,
    /// `BSIM4alpha0`
    pub(crate) alpha0: f64,
    /// `BSIM4alpha1`
    pub(crate) alpha1: f64,
    /// `BSIM4beta0`
    pub(crate) beta0: f64,
    /// `BSIM4agidl`
    pub(crate) agidl: f64,
    /// `BSIM4bgidl`
    pub(crate) bgidl: f64,
    /// `BSIM4cgidl`
    pub(crate) cgidl: f64,
    /// `BSIM4egidl`
    pub(crate) egidl: f64,
    /// `BSIM4fgidl`
    pub(crate) fgidl: f64,
    /// `BSIM4kgidl`
    pub(crate) kgidl: f64,
    /// `BSIM4rgidl`
    pub(crate) rgidl: f64,
    /// `BSIM4agisl`
    pub(crate) agisl: f64,
    /// `BSIM4bgisl`
    pub(crate) bgisl: f64,
    /// `BSIM4cgisl`
    pub(crate) cgisl: f64,
    /// `BSIM4egisl`
    pub(crate) egisl: f64,
    /// `BSIM4fgisl`
    pub(crate) fgisl: f64,
    /// `BSIM4kgisl`
    pub(crate) kgisl: f64,
    /// `BSIM4rgisl`
    pub(crate) rgisl: f64,
    /// `BSIM4aigc`
    pub(crate) aigc: f64,
    /// `BSIM4bigc`
    pub(crate) bigc: f64,
    /// `BSIM4cigc`
    pub(crate) cigc: f64,
    /// `BSIM4aigsd`
    pub(crate) aigsd: f64,
    /// `BSIM4bigsd`
    pub(crate) bigsd: f64,
    /// `BSIM4cigsd`
    pub(crate) cigsd: f64,
    /// `BSIM4aigs`
    pub(crate) aigs: f64,
    /// `BSIM4bigs`
    pub(crate) bigs: f64,
    /// `BSIM4cigs`
    pub(crate) cigs: f64,
    /// `BSIM4aigd`
    pub(crate) aigd: f64,
    /// `BSIM4bigd`
    pub(crate) bigd: f64,
    /// `BSIM4cigd`
    pub(crate) cigd: f64,
    /// `BSIM4aigbacc`
    pub(crate) aigbacc: f64,
    /// `BSIM4bigbacc`
    pub(crate) bigbacc: f64,
    /// `BSIM4cigbacc`
    pub(crate) cigbacc: f64,
    /// `BSIM4aigbinv`
    pub(crate) aigbinv: f64,
    /// `BSIM4bigbinv`
    pub(crate) bigbinv: f64,
    /// `BSIM4cigbinv`
    pub(crate) cigbinv: f64,
    /// `BSIM4nigc`
    pub(crate) nigc: f64,
    /// `BSIM4nigbacc`
    pub(crate) nigbacc: f64,
    /// `BSIM4nigbinv`
    pub(crate) nigbinv: f64,
    /// `BSIM4ntox`
    pub(crate) ntox: f64,
    /// `BSIM4eigbinv`
    pub(crate) eigbinv: f64,
    /// `BSIM4pigcd`
    pub(crate) pigcd: f64,
    /// `BSIM4poxedge`
    pub(crate) poxedge: f64,
    /// `BSIM4toxref`
    pub(crate) toxref: f64,
    /// `BSIM4ijthdfwd`
    pub(crate) ijthdfwd: f64,
    /// `BSIM4ijthsfwd`
    pub(crate) ijthsfwd: f64,
    /// `BSIM4ijthdrev`
    pub(crate) ijthdrev: f64,
    /// `BSIM4ijthsrev`
    pub(crate) ijthsrev: f64,
    /// `BSIM4xjbvd`
    pub(crate) xjbvd: f64,
    /// `BSIM4xjbvs`
    pub(crate) xjbvs: f64,
    /// `BSIM4bvd`
    pub(crate) bvd: f64,
    /// `BSIM4bvs`
    pub(crate) bvs: f64,
    /// `BSIM4jtss`
    pub(crate) jtss: f64,
    /// `BSIM4jtsd`
    pub(crate) jtsd: f64,
    /// `BSIM4jtssws`
    pub(crate) jtssws: f64,
    /// `BSIM4jtsswd`
    pub(crate) jtsswd: f64,
    /// `BSIM4jtsswgs`
    pub(crate) jtsswgs: f64,
    /// `BSIM4jtsswgd`
    pub(crate) jtsswgd: f64,
    /// `BSIM4jtweff`
    pub(crate) jtweff: f64,
    /// `BSIM4njts`
    pub(crate) njts: f64,
    /// `BSIM4njtssw`
    pub(crate) njtssw: f64,
    /// `BSIM4njtsswg`
    pub(crate) njtsswg: f64,
    /// `BSIM4njtsd`
    pub(crate) njtsd: f64,
    /// `BSIM4njtsswd`
    pub(crate) njtsswd: f64,
    /// `BSIM4njtsswgd`
    pub(crate) njtsswgd: f64,
    /// `BSIM4xtss`
    pub(crate) xtss: f64,
    /// `BSIM4xtsd`
    pub(crate) xtsd: f64,
    /// `BSIM4xtssws`
    pub(crate) xtssws: f64,
    /// `BSIM4xtsswd`
    pub(crate) xtsswd: f64,
    /// `BSIM4xtsswgs`
    pub(crate) xtsswgs: f64,
    /// `BSIM4xtsswgd`
    pub(crate) xtsswgd: f64,
    /// `BSIM4tnjts`
    pub(crate) tnjts: f64,
    /// `BSIM4tnjtssw`
    pub(crate) tnjtssw: f64,
    /// `BSIM4tnjtsswg`
    pub(crate) tnjtsswg: f64,
    /// `BSIM4tnjtsd`
    pub(crate) tnjtsd: f64,
    /// `BSIM4tnjtsswd`
    pub(crate) tnjtsswd: f64,
    /// `BSIM4tnjtsswgd`
    pub(crate) tnjtsswgd: f64,
    /// `BSIM4vtss`
    pub(crate) vtss: f64,
    /// `BSIM4vtsd`
    pub(crate) vtsd: f64,
    /// `BSIM4vtssws`
    pub(crate) vtssws: f64,
    /// `BSIM4vtsswd`
    pub(crate) vtsswd: f64,
    /// `BSIM4vtsswgs`
    pub(crate) vtsswgs: f64,
    /// `BSIM4vtsswgd`
    pub(crate) vtsswgd: f64,
    /// `BSIM4xrcrg1`
    pub(crate) xrcrg1: f64,
    /// `BSIM4xrcrg2`
    pub(crate) xrcrg2: f64,
    /// `BSIM4lambda`
    pub(crate) lambda: f64,
    /// `BSIM4vtl`
    pub(crate) vtl: f64,
    /// `BSIM4lc`
    pub(crate) lc: f64,
    /// `BSIM4xn`
    pub(crate) xn: f64,
    /// `BSIM4vfbsdoff`
    pub(crate) vfbsdoff: f64,
    /// `BSIM4lintnoi`
    pub(crate) lintnoi: f64,
    /// `BSIM4tvfbsdoff`
    pub(crate) tvfbsdoff: f64,
    /// `BSIM4vfb`
    pub(crate) vfb: f64,
    /// `BSIM4gbmin`
    pub(crate) gbmin: f64,
    /// `BSIM4rbdb`
    pub(crate) rbdb: f64,
    /// `BSIM4rbsb`
    pub(crate) rbsb: f64,
    /// `BSIM4rbpb`
    pub(crate) rbpb: f64,
    /// `BSIM4rbps`
    pub(crate) rbps: f64,
    /// `BSIM4rbpd`
    pub(crate) rbpd: f64,
    /// `BSIM4rbps0`
    pub(crate) rbps0: f64,
    /// `BSIM4rbpsl`
    pub(crate) rbpsl: f64,
    /// `BSIM4rbpsw`
    pub(crate) rbpsw: f64,
    /// `BSIM4rbpsnf`
    pub(crate) rbpsnf: f64,
    /// `BSIM4rbpd0`
    pub(crate) rbpd0: f64,
    /// `BSIM4rbpdl`
    pub(crate) rbpdl: f64,
    /// `BSIM4rbpdw`
    pub(crate) rbpdw: f64,
    /// `BSIM4rbpdnf`
    pub(crate) rbpdnf: f64,
    /// `BSIM4rbpbx0`
    pub(crate) rbpbx0: f64,
    /// `BSIM4rbpbxl`
    pub(crate) rbpbxl: f64,
    /// `BSIM4rbpbxw`
    pub(crate) rbpbxw: f64,
    /// `BSIM4rbpbxnf`
    pub(crate) rbpbxnf: f64,
    /// `BSIM4rbpby0`
    pub(crate) rbpby0: f64,
    /// `BSIM4rbpbyl`
    pub(crate) rbpbyl: f64,
    /// `BSIM4rbpbyw`
    pub(crate) rbpbyw: f64,
    /// `BSIM4rbpbynf`
    pub(crate) rbpbynf: f64,
    /// `BSIM4rbsbx0`
    pub(crate) rbsbx0: f64,
    /// `BSIM4rbsby0`
    pub(crate) rbsby0: f64,
    /// `BSIM4rbdbx0`
    pub(crate) rbdbx0: f64,
    /// `BSIM4rbdby0`
    pub(crate) rbdby0: f64,
    /// `BSIM4rbsdbxl`
    pub(crate) rbsdbxl: f64,
    /// `BSIM4rbsdbxw`
    pub(crate) rbsdbxw: f64,
    /// `BSIM4rbsdbxnf`
    pub(crate) rbsdbxnf: f64,
    /// `BSIM4rbsdbyl`
    pub(crate) rbsdbyl: f64,
    /// `BSIM4rbsdbyw`
    pub(crate) rbsdbyw: f64,
    /// `BSIM4rbsdbynf`
    pub(crate) rbsdbynf: f64,
    /// `BSIM4tnoia`
    pub(crate) tnoia: f64,
    /// `BSIM4tnoib`
    pub(crate) tnoib: f64,
    /// `BSIM4tnoic`
    pub(crate) tnoic: f64,
    /// `BSIM4rnoia`
    pub(crate) rnoia: f64,
    /// `BSIM4rnoib`
    pub(crate) rnoib: f64,
    /// `BSIM4rnoic`
    pub(crate) rnoic: f64,
    /// `BSIM4ntnoi`
    pub(crate) ntnoi: f64,
    /// `BSIM4cgsl`
    pub(crate) cgsl: f64,
    /// `BSIM4cgdl`
    pub(crate) cgdl: f64,
    /// `BSIM4ckappas`
    pub(crate) ckappas: f64,
    /// `BSIM4ckappad`
    pub(crate) ckappad: f64,
    /// `BSIM4cf`
    pub(crate) cf: f64,
    /// `BSIM4vfbcv`
    pub(crate) vfbcv: f64,
    /// `BSIM4clc`
    pub(crate) clc: f64,
    /// `BSIM4cle`
    pub(crate) cle: f64,
    /// `BSIM4dwc`
    pub(crate) dwc: f64,
    /// `BSIM4dlc`
    pub(crate) dlc: f64,
    /// `BSIM4xw`
    pub(crate) xw: f64,
    /// `BSIM4xl`
    pub(crate) xl: f64,
    /// `BSIM4dlcig`
    pub(crate) dlcig: f64,
    /// `BSIM4dlcigd`
    pub(crate) dlcigd: f64,
    /// `BSIM4dwj`
    pub(crate) dwj: f64,
    /// `BSIM4noff`
    pub(crate) noff: f64,
    /// `BSIM4voffcv`
    pub(crate) voffcv: f64,
    /// `BSIM4acde`
    pub(crate) acde: f64,
    /// `BSIM4moin`
    pub(crate) moin: f64,
    /// `BSIM4tcj`
    pub(crate) tcj: f64,
    /// `BSIM4tcjsw`
    pub(crate) tcjsw: f64,
    /// `BSIM4tcjswg`
    pub(crate) tcjswg: f64,
    /// `BSIM4tpb`
    pub(crate) tpb: f64,
    /// `BSIM4tpbsw`
    pub(crate) tpbsw: f64,
    /// `BSIM4tpbswg`
    pub(crate) tpbswg: f64,
    /// `BSIM4dmcg`
    pub(crate) dmcg: f64,
    /// `BSIM4dmci`
    pub(crate) dmci: f64,
    /// `BSIM4dmdg`
    pub(crate) dmdg: f64,
    /// `BSIM4dmcgt`
    pub(crate) dmcgt: f64,
    /// `BSIM4xgw`
    pub(crate) xgw: f64,
    /// `BSIM4xgl`
    pub(crate) xgl: f64,
    /// `BSIM4rshg`
    pub(crate) rshg: f64,
    /// `BSIM4ngcon`
    pub(crate) ngcon: f64,
    /// `BSIM4lcdsc`
    pub(crate) lcdsc: f64,
    /// `BSIM4lcdscb`
    pub(crate) lcdscb: f64,
    /// `BSIM4lcdscd`
    pub(crate) lcdscd: f64,
    /// `BSIM4lcit`
    pub(crate) lcit: f64,
    /// `BSIM4lnfactor`
    pub(crate) lnfactor: f64,
    /// `BSIM4lxj`
    pub(crate) lxj: f64,
    /// `BSIM4lvsat`
    pub(crate) lvsat: f64,
    /// `BSIM4lat`
    pub(crate) lat: f64,
    /// `BSIM4la0`
    pub(crate) la0: f64,
    /// `BSIM4lags`
    pub(crate) lags: f64,
    /// `BSIM4la1`
    pub(crate) la1: f64,
    /// `BSIM4la2`
    pub(crate) la2: f64,
    /// `BSIM4lketa`
    pub(crate) lketa: f64,
    /// `BSIM4lketac`
    pub(crate) lketac: f64,
    /// `BSIM4lnsub`
    pub(crate) lnsub: f64,
    /// `BSIM4lndep`
    pub(crate) lndep: f64,
    /// `BSIM4lnsd`
    pub(crate) lnsd: f64,
    /// `BSIM4lphin`
    pub(crate) lphin: f64,
    /// `BSIM4lngate`
    pub(crate) lngate: f64,
    /// `BSIM4lgamma1`
    pub(crate) lgamma1: f64,
    /// `BSIM4lgamma2`
    pub(crate) lgamma2: f64,
    /// `BSIM4lvbx`
    pub(crate) lvbx: f64,
    /// `BSIM4lvbm`
    pub(crate) lvbm: f64,
    /// `BSIM4lxt`
    pub(crate) lxt: f64,
    /// `BSIM4lk1`
    pub(crate) lk1: f64,
    /// `BSIM4lkt1`
    pub(crate) lkt1: f64,
    /// `BSIM4lkt1l`
    pub(crate) lkt1l: f64,
    /// `BSIM4lkt2`
    pub(crate) lkt2: f64,
    /// `BSIM4lk2`
    pub(crate) lk2: f64,
    /// `BSIM4lk3`
    pub(crate) lk3: f64,
    /// `BSIM4lk3b`
    pub(crate) lk3b: f64,
    /// `BSIM4lw0`
    pub(crate) lw0: f64,
    /// `BSIM4ldvtp0`
    pub(crate) ldvtp0: f64,
    /// `BSIM4ldvtp1`
    pub(crate) ldvtp1: f64,
    /// `BSIM4ldvtp2`
    pub(crate) ldvtp2: f64,
    /// `BSIM4ldvtp3`
    pub(crate) ldvtp3: f64,
    /// `BSIM4ldvtp4`
    pub(crate) ldvtp4: f64,
    /// `BSIM4ldvtp5`
    pub(crate) ldvtp5: f64,
    /// `BSIM4llpe0`
    pub(crate) llpe0: f64,
    /// `BSIM4llpeb`
    pub(crate) llpeb: f64,
    /// `BSIM4ldvt0`
    pub(crate) ldvt0: f64,
    /// `BSIM4ldvt1`
    pub(crate) ldvt1: f64,
    /// `BSIM4ldvt2`
    pub(crate) ldvt2: f64,
    /// `BSIM4ldvt0w`
    pub(crate) ldvt0w: f64,
    /// `BSIM4ldvt1w`
    pub(crate) ldvt1w: f64,
    /// `BSIM4ldvt2w`
    pub(crate) ldvt2w: f64,
    /// `BSIM4ldrout`
    pub(crate) ldrout: f64,
    /// `BSIM4ldsub`
    pub(crate) ldsub: f64,
    /// `BSIM4lvth0`
    pub(crate) lvth0: f64,
    /// `BSIM4lua`
    pub(crate) lua: f64,
    /// `BSIM4lua1`
    pub(crate) lua1: f64,
    /// `BSIM4lub`
    pub(crate) lub: f64,
    /// `BSIM4lub1`
    pub(crate) lub1: f64,
    /// `BSIM4luc`
    pub(crate) luc: f64,
    /// `BSIM4luc1`
    pub(crate) luc1: f64,
    /// `BSIM4lud`
    pub(crate) lud: f64,
    /// `BSIM4lud1`
    pub(crate) lud1: f64,
    /// `BSIM4lup`
    pub(crate) lup: f64,
    /// `BSIM4llp`
    pub(crate) llp: f64,
    /// `BSIM4lu0`
    pub(crate) lu0: f64,
    /// `BSIM4leu`
    pub(crate) leu: f64,
    /// `BSIM4lucs`
    pub(crate) lucs: f64,
    /// `BSIM4lute`
    pub(crate) lute: f64,
    /// `BSIM4lucste`
    pub(crate) lucste: f64,
    /// `BSIM4lvoff`
    pub(crate) lvoff: f64,
    /// `BSIM4ltvoff`
    pub(crate) ltvoff: f64,
    /// `BSIM4ltnfactor`
    pub(crate) ltnfactor: f64,
    /// `BSIM4lteta0`
    pub(crate) lteta0: f64,
    /// `BSIM4ltvoffcv`
    pub(crate) ltvoffcv: f64,
    /// `BSIM4lminv`
    pub(crate) lminv: f64,
    /// `BSIM4lminvcv`
    pub(crate) lminvcv: f64,
    /// `BSIM4ldelta`
    pub(crate) ldelta: f64,
    /// `BSIM4lrdsw`
    pub(crate) lrdsw: f64,
    /// `BSIM4lrsw`
    pub(crate) lrsw: f64,
    /// `BSIM4lrdw`
    pub(crate) lrdw: f64,
    /// `BSIM4lprwg`
    pub(crate) lprwg: f64,
    /// `BSIM4lprwb`
    pub(crate) lprwb: f64,
    /// `BSIM4lprt`
    pub(crate) lprt: f64,
    /// `BSIM4leta0`
    pub(crate) leta0: f64,
    /// `BSIM4letab`
    pub(crate) letab: f64,
    /// `BSIM4lpclm`
    pub(crate) lpclm: f64,
    /// `BSIM4lpdibl1`
    pub(crate) lpdibl1: f64,
    /// `BSIM4lpdibl2`
    pub(crate) lpdibl2: f64,
    /// `BSIM4lpdiblb`
    pub(crate) lpdiblb: f64,
    /// `BSIM4lfprout`
    pub(crate) lfprout: f64,
    /// `BSIM4lpdits`
    pub(crate) lpdits: f64,
    /// `BSIM4lpditsd`
    pub(crate) lpditsd: f64,
    /// `BSIM4lpscbe1`
    pub(crate) lpscbe1: f64,
    /// `BSIM4lpscbe2`
    pub(crate) lpscbe2: f64,
    /// `BSIM4lpvag`
    pub(crate) lpvag: f64,
    /// `BSIM4lwr`
    pub(crate) lwr: f64,
    /// `BSIM4ldwg`
    pub(crate) ldwg: f64,
    /// `BSIM4ldwb`
    pub(crate) ldwb: f64,
    /// `BSIM4lb0`
    pub(crate) lb0: f64,
    /// `BSIM4lb1`
    pub(crate) lb1: f64,
    /// `BSIM4lalpha0`
    pub(crate) lalpha0: f64,
    /// `BSIM4lalpha1`
    pub(crate) lalpha1: f64,
    /// `BSIM4lbeta0`
    pub(crate) lbeta0: f64,
    /// `BSIM4lvfb`
    pub(crate) lvfb: f64,
    /// `BSIM4lagidl`
    pub(crate) lagidl: f64,
    /// `BSIM4lbgidl`
    pub(crate) lbgidl: f64,
    /// `BSIM4lcgidl`
    pub(crate) lcgidl: f64,
    /// `BSIM4legidl`
    pub(crate) legidl: f64,
    /// `BSIM4lfgidl`
    pub(crate) lfgidl: f64,
    /// `BSIM4lkgidl`
    pub(crate) lkgidl: f64,
    /// `BSIM4lrgidl`
    pub(crate) lrgidl: f64,
    /// `BSIM4lagisl`
    pub(crate) lagisl: f64,
    /// `BSIM4lbgisl`
    pub(crate) lbgisl: f64,
    /// `BSIM4lcgisl`
    pub(crate) lcgisl: f64,
    /// `BSIM4legisl`
    pub(crate) legisl: f64,
    /// `BSIM4lfgisl`
    pub(crate) lfgisl: f64,
    /// `BSIM4lkgisl`
    pub(crate) lkgisl: f64,
    /// `BSIM4lrgisl`
    pub(crate) lrgisl: f64,
    /// `BSIM4laigc`
    pub(crate) laigc: f64,
    /// `BSIM4lbigc`
    pub(crate) lbigc: f64,
    /// `BSIM4lcigc`
    pub(crate) lcigc: f64,
    /// `BSIM4laigsd`
    pub(crate) laigsd: f64,
    /// `BSIM4lbigsd`
    pub(crate) lbigsd: f64,
    /// `BSIM4lcigsd`
    pub(crate) lcigsd: f64,
    /// `BSIM4laigs`
    pub(crate) laigs: f64,
    /// `BSIM4lbigs`
    pub(crate) lbigs: f64,
    /// `BSIM4lcigs`
    pub(crate) lcigs: f64,
    /// `BSIM4laigd`
    pub(crate) laigd: f64,
    /// `BSIM4lbigd`
    pub(crate) lbigd: f64,
    /// `BSIM4lcigd`
    pub(crate) lcigd: f64,
    /// `BSIM4laigbacc`
    pub(crate) laigbacc: f64,
    /// `BSIM4lbigbacc`
    pub(crate) lbigbacc: f64,
    /// `BSIM4lcigbacc`
    pub(crate) lcigbacc: f64,
    /// `BSIM4laigbinv`
    pub(crate) laigbinv: f64,
    /// `BSIM4lbigbinv`
    pub(crate) lbigbinv: f64,
    /// `BSIM4lcigbinv`
    pub(crate) lcigbinv: f64,
    /// `BSIM4lnigc`
    pub(crate) lnigc: f64,
    /// `BSIM4lnigbacc`
    pub(crate) lnigbacc: f64,
    /// `BSIM4lnigbinv`
    pub(crate) lnigbinv: f64,
    /// `BSIM4lntox`
    pub(crate) lntox: f64,
    /// `BSIM4leigbinv`
    pub(crate) leigbinv: f64,
    /// `BSIM4lpigcd`
    pub(crate) lpigcd: f64,
    /// `BSIM4lpoxedge`
    pub(crate) lpoxedge: f64,
    /// `BSIM4lxrcrg1`
    pub(crate) lxrcrg1: f64,
    /// `BSIM4lxrcrg2`
    pub(crate) lxrcrg2: f64,
    /// `BSIM4llambda`
    pub(crate) llambda: f64,
    /// `BSIM4lvtl`
    pub(crate) lvtl: f64,
    /// `BSIM4lxn`
    pub(crate) lxn: f64,
    /// `BSIM4lvfbsdoff`
    pub(crate) lvfbsdoff: f64,
    /// `BSIM4ltvfbsdoff`
    pub(crate) ltvfbsdoff: f64,
    /// `BSIM4lcgsl`
    pub(crate) lcgsl: f64,
    /// `BSIM4lcgdl`
    pub(crate) lcgdl: f64,
    /// `BSIM4lckappas`
    pub(crate) lckappas: f64,
    /// `BSIM4lckappad`
    pub(crate) lckappad: f64,
    /// `BSIM4lcf`
    pub(crate) lcf: f64,
    /// `BSIM4lclc`
    pub(crate) lclc: f64,
    /// `BSIM4lcle`
    pub(crate) lcle: f64,
    /// `BSIM4lvfbcv`
    pub(crate) lvfbcv: f64,
    /// `BSIM4lnoff`
    pub(crate) lnoff: f64,
    /// `BSIM4lvoffcv`
    pub(crate) lvoffcv: f64,
    /// `BSIM4lacde`
    pub(crate) lacde: f64,
    /// `BSIM4lmoin`
    pub(crate) lmoin: f64,
    /// `BSIM4wcdsc`
    pub(crate) wcdsc: f64,
    /// `BSIM4wcdscb`
    pub(crate) wcdscb: f64,
    /// `BSIM4wcdscd`
    pub(crate) wcdscd: f64,
    /// `BSIM4wcit`
    pub(crate) wcit: f64,
    /// `BSIM4wnfactor`
    pub(crate) wnfactor: f64,
    /// `BSIM4wxj`
    pub(crate) wxj: f64,
    /// `BSIM4wvsat`
    pub(crate) wvsat: f64,
    /// `BSIM4wat`
    pub(crate) wat: f64,
    /// `BSIM4wa0`
    pub(crate) wa0: f64,
    /// `BSIM4wags`
    pub(crate) wags: f64,
    /// `BSIM4wa1`
    pub(crate) wa1: f64,
    /// `BSIM4wa2`
    pub(crate) wa2: f64,
    /// `BSIM4wketa`
    pub(crate) wketa: f64,
    /// `BSIM4wketac`
    pub(crate) wketac: f64,
    /// `BSIM4wnsub`
    pub(crate) wnsub: f64,
    /// `BSIM4wndep`
    pub(crate) wndep: f64,
    /// `BSIM4wnsd`
    pub(crate) wnsd: f64,
    /// `BSIM4wphin`
    pub(crate) wphin: f64,
    /// `BSIM4wngate`
    pub(crate) wngate: f64,
    /// `BSIM4wgamma1`
    pub(crate) wgamma1: f64,
    /// `BSIM4wgamma2`
    pub(crate) wgamma2: f64,
    /// `BSIM4wvbx`
    pub(crate) wvbx: f64,
    /// `BSIM4wvbm`
    pub(crate) wvbm: f64,
    /// `BSIM4wxt`
    pub(crate) wxt: f64,
    /// `BSIM4wk1`
    pub(crate) wk1: f64,
    /// `BSIM4wkt1`
    pub(crate) wkt1: f64,
    /// `BSIM4wkt1l`
    pub(crate) wkt1l: f64,
    /// `BSIM4wkt2`
    pub(crate) wkt2: f64,
    /// `BSIM4wk2`
    pub(crate) wk2: f64,
    /// `BSIM4wk3`
    pub(crate) wk3: f64,
    /// `BSIM4wk3b`
    pub(crate) wk3b: f64,
    /// `BSIM4ww0`
    pub(crate) ww0: f64,
    /// `BSIM4wdvtp0`
    pub(crate) wdvtp0: f64,
    /// `BSIM4wdvtp1`
    pub(crate) wdvtp1: f64,
    /// `BSIM4wdvtp2`
    pub(crate) wdvtp2: f64,
    /// `BSIM4wdvtp3`
    pub(crate) wdvtp3: f64,
    /// `BSIM4wdvtp4`
    pub(crate) wdvtp4: f64,
    /// `BSIM4wdvtp5`
    pub(crate) wdvtp5: f64,
    /// `BSIM4wlpe0`
    pub(crate) wlpe0: f64,
    /// `BSIM4wlpeb`
    pub(crate) wlpeb: f64,
    /// `BSIM4wdvt0`
    pub(crate) wdvt0: f64,
    /// `BSIM4wdvt1`
    pub(crate) wdvt1: f64,
    /// `BSIM4wdvt2`
    pub(crate) wdvt2: f64,
    /// `BSIM4wdvt0w`
    pub(crate) wdvt0w: f64,
    /// `BSIM4wdvt1w`
    pub(crate) wdvt1w: f64,
    /// `BSIM4wdvt2w`
    pub(crate) wdvt2w: f64,
    /// `BSIM4wdrout`
    pub(crate) wdrout: f64,
    /// `BSIM4wdsub`
    pub(crate) wdsub: f64,
    /// `BSIM4wvth0`
    pub(crate) wvth0: f64,
    /// `BSIM4wua`
    pub(crate) wua: f64,
    /// `BSIM4wua1`
    pub(crate) wua1: f64,
    /// `BSIM4wub`
    pub(crate) wub: f64,
    /// `BSIM4wub1`
    pub(crate) wub1: f64,
    /// `BSIM4wuc`
    pub(crate) wuc: f64,
    /// `BSIM4wuc1`
    pub(crate) wuc1: f64,
    /// `BSIM4wud`
    pub(crate) wud: f64,
    /// `BSIM4wud1`
    pub(crate) wud1: f64,
    /// `BSIM4wup`
    pub(crate) wup: f64,
    /// `BSIM4wlp`
    pub(crate) wlp: f64,
    /// `BSIM4wu0`
    pub(crate) wu0: f64,
    /// `BSIM4weu`
    pub(crate) weu: f64,
    /// `BSIM4wucs`
    pub(crate) wucs: f64,
    /// `BSIM4wute`
    pub(crate) wute: f64,
    /// `BSIM4wucste`
    pub(crate) wucste: f64,
    /// `BSIM4wvoff`
    pub(crate) wvoff: f64,
    /// `BSIM4wtvoff`
    pub(crate) wtvoff: f64,
    /// `BSIM4wtnfactor`
    pub(crate) wtnfactor: f64,
    /// `BSIM4wteta0`
    pub(crate) wteta0: f64,
    /// `BSIM4wtvoffcv`
    pub(crate) wtvoffcv: f64,
    /// `BSIM4wminv`
    pub(crate) wminv: f64,
    /// `BSIM4wminvcv`
    pub(crate) wminvcv: f64,
    /// `BSIM4wdelta`
    pub(crate) wdelta: f64,
    /// `BSIM4wrdsw`
    pub(crate) wrdsw: f64,
    /// `BSIM4wrsw`
    pub(crate) wrsw: f64,
    /// `BSIM4wrdw`
    pub(crate) wrdw: f64,
    /// `BSIM4wprwg`
    pub(crate) wprwg: f64,
    /// `BSIM4wprwb`
    pub(crate) wprwb: f64,
    /// `BSIM4wprt`
    pub(crate) wprt: f64,
    /// `BSIM4weta0`
    pub(crate) weta0: f64,
    /// `BSIM4wetab`
    pub(crate) wetab: f64,
    /// `BSIM4wpclm`
    pub(crate) wpclm: f64,
    /// `BSIM4wpdibl1`
    pub(crate) wpdibl1: f64,
    /// `BSIM4wpdibl2`
    pub(crate) wpdibl2: f64,
    /// `BSIM4wpdiblb`
    pub(crate) wpdiblb: f64,
    /// `BSIM4wfprout`
    pub(crate) wfprout: f64,
    /// `BSIM4wpdits`
    pub(crate) wpdits: f64,
    /// `BSIM4wpditsd`
    pub(crate) wpditsd: f64,
    /// `BSIM4wpscbe1`
    pub(crate) wpscbe1: f64,
    /// `BSIM4wpscbe2`
    pub(crate) wpscbe2: f64,
    /// `BSIM4wpvag`
    pub(crate) wpvag: f64,
    /// `BSIM4wwr`
    pub(crate) wwr: f64,
    /// `BSIM4wdwg`
    pub(crate) wdwg: f64,
    /// `BSIM4wdwb`
    pub(crate) wdwb: f64,
    /// `BSIM4wb0`
    pub(crate) wb0: f64,
    /// `BSIM4wb1`
    pub(crate) wb1: f64,
    /// `BSIM4walpha0`
    pub(crate) walpha0: f64,
    /// `BSIM4walpha1`
    pub(crate) walpha1: f64,
    /// `BSIM4wbeta0`
    pub(crate) wbeta0: f64,
    /// `BSIM4wvfb`
    pub(crate) wvfb: f64,
    /// `BSIM4wagidl`
    pub(crate) wagidl: f64,
    /// `BSIM4wbgidl`
    pub(crate) wbgidl: f64,
    /// `BSIM4wcgidl`
    pub(crate) wcgidl: f64,
    /// `BSIM4wegidl`
    pub(crate) wegidl: f64,
    /// `BSIM4wfgidl`
    pub(crate) wfgidl: f64,
    /// `BSIM4wkgidl`
    pub(crate) wkgidl: f64,
    /// `BSIM4wrgidl`
    pub(crate) wrgidl: f64,
    /// `BSIM4wagisl`
    pub(crate) wagisl: f64,
    /// `BSIM4wbgisl`
    pub(crate) wbgisl: f64,
    /// `BSIM4wcgisl`
    pub(crate) wcgisl: f64,
    /// `BSIM4wegisl`
    pub(crate) wegisl: f64,
    /// `BSIM4wfgisl`
    pub(crate) wfgisl: f64,
    /// `BSIM4wkgisl`
    pub(crate) wkgisl: f64,
    /// `BSIM4wrgisl`
    pub(crate) wrgisl: f64,
    /// `BSIM4waigc`
    pub(crate) waigc: f64,
    /// `BSIM4wbigc`
    pub(crate) wbigc: f64,
    /// `BSIM4wcigc`
    pub(crate) wcigc: f64,
    /// `BSIM4waigsd`
    pub(crate) waigsd: f64,
    /// `BSIM4wbigsd`
    pub(crate) wbigsd: f64,
    /// `BSIM4wcigsd`
    pub(crate) wcigsd: f64,
    /// `BSIM4waigs`
    pub(crate) waigs: f64,
    /// `BSIM4wbigs`
    pub(crate) wbigs: f64,
    /// `BSIM4wcigs`
    pub(crate) wcigs: f64,
    /// `BSIM4waigd`
    pub(crate) waigd: f64,
    /// `BSIM4wbigd`
    pub(crate) wbigd: f64,
    /// `BSIM4wcigd`
    pub(crate) wcigd: f64,
    /// `BSIM4waigbacc`
    pub(crate) waigbacc: f64,
    /// `BSIM4wbigbacc`
    pub(crate) wbigbacc: f64,
    /// `BSIM4wcigbacc`
    pub(crate) wcigbacc: f64,
    /// `BSIM4waigbinv`
    pub(crate) waigbinv: f64,
    /// `BSIM4wbigbinv`
    pub(crate) wbigbinv: f64,
    /// `BSIM4wcigbinv`
    pub(crate) wcigbinv: f64,
    /// `BSIM4wnigc`
    pub(crate) wnigc: f64,
    /// `BSIM4wnigbacc`
    pub(crate) wnigbacc: f64,
    /// `BSIM4wnigbinv`
    pub(crate) wnigbinv: f64,
    /// `BSIM4wntox`
    pub(crate) wntox: f64,
    /// `BSIM4weigbinv`
    pub(crate) weigbinv: f64,
    /// `BSIM4wpigcd`
    pub(crate) wpigcd: f64,
    /// `BSIM4wpoxedge`
    pub(crate) wpoxedge: f64,
    /// `BSIM4wxrcrg1`
    pub(crate) wxrcrg1: f64,
    /// `BSIM4wxrcrg2`
    pub(crate) wxrcrg2: f64,
    /// `BSIM4wlambda`
    pub(crate) wlambda: f64,
    /// `BSIM4wvtl`
    pub(crate) wvtl: f64,
    /// `BSIM4wxn`
    pub(crate) wxn: f64,
    /// `BSIM4wvfbsdoff`
    pub(crate) wvfbsdoff: f64,
    /// `BSIM4wtvfbsdoff`
    pub(crate) wtvfbsdoff: f64,
    /// `BSIM4wcgsl`
    pub(crate) wcgsl: f64,
    /// `BSIM4wcgdl`
    pub(crate) wcgdl: f64,
    /// `BSIM4wckappas`
    pub(crate) wckappas: f64,
    /// `BSIM4wckappad`
    pub(crate) wckappad: f64,
    /// `BSIM4wcf`
    pub(crate) wcf: f64,
    /// `BSIM4wclc`
    pub(crate) wclc: f64,
    /// `BSIM4wcle`
    pub(crate) wcle: f64,
    /// `BSIM4wvfbcv`
    pub(crate) wvfbcv: f64,
    /// `BSIM4wnoff`
    pub(crate) wnoff: f64,
    /// `BSIM4wvoffcv`
    pub(crate) wvoffcv: f64,
    /// `BSIM4wacde`
    pub(crate) wacde: f64,
    /// `BSIM4wmoin`
    pub(crate) wmoin: f64,
    /// `BSIM4pcdsc`
    pub(crate) pcdsc: f64,
    /// `BSIM4pcdscb`
    pub(crate) pcdscb: f64,
    /// `BSIM4pcdscd`
    pub(crate) pcdscd: f64,
    /// `BSIM4pcit`
    pub(crate) pcit: f64,
    /// `BSIM4pnfactor`
    pub(crate) pnfactor: f64,
    /// `BSIM4pxj`
    pub(crate) pxj: f64,
    /// `BSIM4pvsat`
    pub(crate) pvsat: f64,
    /// `BSIM4pat`
    pub(crate) pat: f64,
    /// `BSIM4pa0`
    pub(crate) pa0: f64,
    /// `BSIM4pags`
    pub(crate) pags: f64,
    /// `BSIM4pa1`
    pub(crate) pa1: f64,
    /// `BSIM4pa2`
    pub(crate) pa2: f64,
    /// `BSIM4pketa`
    pub(crate) pketa: f64,
    /// `BSIM4pketac`
    pub(crate) pketac: f64,
    /// `BSIM4pnsub`
    pub(crate) pnsub: f64,
    /// `BSIM4pndep`
    pub(crate) pndep: f64,
    /// `BSIM4pnsd`
    pub(crate) pnsd: f64,
    /// `BSIM4pphin`
    pub(crate) pphin: f64,
    /// `BSIM4pngate`
    pub(crate) pngate: f64,
    /// `BSIM4pgamma1`
    pub(crate) pgamma1: f64,
    /// `BSIM4pgamma2`
    pub(crate) pgamma2: f64,
    /// `BSIM4pvbx`
    pub(crate) pvbx: f64,
    /// `BSIM4pvbm`
    pub(crate) pvbm: f64,
    /// `BSIM4pxt`
    pub(crate) pxt: f64,
    /// `BSIM4pk1`
    pub(crate) pk1: f64,
    /// `BSIM4pkt1`
    pub(crate) pkt1: f64,
    /// `BSIM4pkt1l`
    pub(crate) pkt1l: f64,
    /// `BSIM4pkt2`
    pub(crate) pkt2: f64,
    /// `BSIM4pk2`
    pub(crate) pk2: f64,
    /// `BSIM4pk3`
    pub(crate) pk3: f64,
    /// `BSIM4pk3b`
    pub(crate) pk3b: f64,
    /// `BSIM4pw0`
    pub(crate) pw0: f64,
    /// `BSIM4pdvtp0`
    pub(crate) pdvtp0: f64,
    /// `BSIM4pdvtp1`
    pub(crate) pdvtp1: f64,
    /// `BSIM4pdvtp2`
    pub(crate) pdvtp2: f64,
    /// `BSIM4pdvtp3`
    pub(crate) pdvtp3: f64,
    /// `BSIM4pdvtp4`
    pub(crate) pdvtp4: f64,
    /// `BSIM4pdvtp5`
    pub(crate) pdvtp5: f64,
    /// `BSIM4plpe0`
    pub(crate) plpe0: f64,
    /// `BSIM4plpeb`
    pub(crate) plpeb: f64,
    /// `BSIM4pdvt0`
    pub(crate) pdvt0: f64,
    /// `BSIM4pdvt1`
    pub(crate) pdvt1: f64,
    /// `BSIM4pdvt2`
    pub(crate) pdvt2: f64,
    /// `BSIM4pdvt0w`
    pub(crate) pdvt0w: f64,
    /// `BSIM4pdvt1w`
    pub(crate) pdvt1w: f64,
    /// `BSIM4pdvt2w`
    pub(crate) pdvt2w: f64,
    /// `BSIM4pdrout`
    pub(crate) pdrout: f64,
    /// `BSIM4pdsub`
    pub(crate) pdsub: f64,
    /// `BSIM4pvth0`
    pub(crate) pvth0: f64,
    /// `BSIM4pua`
    pub(crate) pua: f64,
    /// `BSIM4pua1`
    pub(crate) pua1: f64,
    /// `BSIM4pub`
    pub(crate) pub_: f64,
    /// `BSIM4pub1`
    pub(crate) pub1: f64,
    /// `BSIM4puc`
    pub(crate) puc: f64,
    /// `BSIM4puc1`
    pub(crate) puc1: f64,
    /// `BSIM4pud`
    pub(crate) pud: f64,
    /// `BSIM4pud1`
    pub(crate) pud1: f64,
    /// `BSIM4pup`
    pub(crate) pup: f64,
    /// `BSIM4plp`
    pub(crate) plp: f64,
    /// `BSIM4pu0`
    pub(crate) pu0: f64,
    /// `BSIM4peu`
    pub(crate) peu: f64,
    /// `BSIM4pucs`
    pub(crate) pucs: f64,
    /// `BSIM4pute`
    pub(crate) pute: f64,
    /// `BSIM4pucste`
    pub(crate) pucste: f64,
    /// `BSIM4pvoff`
    pub(crate) pvoff: f64,
    /// `BSIM4ptvoff`
    pub(crate) ptvoff: f64,
    /// `BSIM4ptnfactor`
    pub(crate) ptnfactor: f64,
    /// `BSIM4pteta0`
    pub(crate) pteta0: f64,
    /// `BSIM4ptvoffcv`
    pub(crate) ptvoffcv: f64,
    /// `BSIM4pminv`
    pub(crate) pminv: f64,
    /// `BSIM4pminvcv`
    pub(crate) pminvcv: f64,
    /// `BSIM4pdelta`
    pub(crate) pdelta: f64,
    /// `BSIM4prdsw`
    pub(crate) prdsw: f64,
    /// `BSIM4prsw`
    pub(crate) prsw: f64,
    /// `BSIM4prdw`
    pub(crate) prdw: f64,
    /// `BSIM4pprwg`
    pub(crate) pprwg: f64,
    /// `BSIM4pprwb`
    pub(crate) pprwb: f64,
    /// `BSIM4pprt`
    pub(crate) pprt: f64,
    /// `BSIM4peta0`
    pub(crate) peta0: f64,
    /// `BSIM4petab`
    pub(crate) petab: f64,
    /// `BSIM4ppclm`
    pub(crate) ppclm: f64,
    /// `BSIM4ppdibl1`
    pub(crate) ppdibl1: f64,
    /// `BSIM4ppdibl2`
    pub(crate) ppdibl2: f64,
    /// `BSIM4ppdiblb`
    pub(crate) ppdiblb: f64,
    /// `BSIM4pfprout`
    pub(crate) pfprout: f64,
    /// `BSIM4ppdits`
    pub(crate) ppdits: f64,
    /// `BSIM4ppditsd`
    pub(crate) ppditsd: f64,
    /// `BSIM4ppscbe1`
    pub(crate) ppscbe1: f64,
    /// `BSIM4ppscbe2`
    pub(crate) ppscbe2: f64,
    /// `BSIM4ppvag`
    pub(crate) ppvag: f64,
    /// `BSIM4pwr`
    pub(crate) pwr: f64,
    /// `BSIM4pdwg`
    pub(crate) pdwg: f64,
    /// `BSIM4pdwb`
    pub(crate) pdwb: f64,
    /// `BSIM4pb0`
    pub(crate) pb0: f64,
    /// `BSIM4pb1`
    pub(crate) pb1: f64,
    /// `BSIM4palpha0`
    pub(crate) palpha0: f64,
    /// `BSIM4palpha1`
    pub(crate) palpha1: f64,
    /// `BSIM4pbeta0`
    pub(crate) pbeta0: f64,
    /// `BSIM4pvfb`
    pub(crate) pvfb: f64,
    /// `BSIM4pagidl`
    pub(crate) pagidl: f64,
    /// `BSIM4pbgidl`
    pub(crate) pbgidl: f64,
    /// `BSIM4pcgidl`
    pub(crate) pcgidl: f64,
    /// `BSIM4pegidl`
    pub(crate) pegidl: f64,
    /// `BSIM4pfgidl`
    pub(crate) pfgidl: f64,
    /// `BSIM4pkgidl`
    pub(crate) pkgidl: f64,
    /// `BSIM4prgidl`
    pub(crate) prgidl: f64,
    /// `BSIM4pagisl`
    pub(crate) pagisl: f64,
    /// `BSIM4pbgisl`
    pub(crate) pbgisl: f64,
    /// `BSIM4pcgisl`
    pub(crate) pcgisl: f64,
    /// `BSIM4pegisl`
    pub(crate) pegisl: f64,
    /// `BSIM4pfgisl`
    pub(crate) pfgisl: f64,
    /// `BSIM4pkgisl`
    pub(crate) pkgisl: f64,
    /// `BSIM4prgisl`
    pub(crate) prgisl: f64,
    /// `BSIM4paigc`
    pub(crate) paigc: f64,
    /// `BSIM4pbigc`
    pub(crate) pbigc: f64,
    /// `BSIM4pcigc`
    pub(crate) pcigc: f64,
    /// `BSIM4paigsd`
    pub(crate) paigsd: f64,
    /// `BSIM4pbigsd`
    pub(crate) pbigsd: f64,
    /// `BSIM4pcigsd`
    pub(crate) pcigsd: f64,
    /// `BSIM4paigs`
    pub(crate) paigs: f64,
    /// `BSIM4pbigs`
    pub(crate) pbigs: f64,
    /// `BSIM4pcigs`
    pub(crate) pcigs: f64,
    /// `BSIM4paigd`
    pub(crate) paigd: f64,
    /// `BSIM4pbigd`
    pub(crate) pbigd: f64,
    /// `BSIM4pcigd`
    pub(crate) pcigd: f64,
    /// `BSIM4paigbacc`
    pub(crate) paigbacc: f64,
    /// `BSIM4pbigbacc`
    pub(crate) pbigbacc: f64,
    /// `BSIM4pcigbacc`
    pub(crate) pcigbacc: f64,
    /// `BSIM4paigbinv`
    pub(crate) paigbinv: f64,
    /// `BSIM4pbigbinv`
    pub(crate) pbigbinv: f64,
    /// `BSIM4pcigbinv`
    pub(crate) pcigbinv: f64,
    /// `BSIM4pnigc`
    pub(crate) pnigc: f64,
    /// `BSIM4pnigbacc`
    pub(crate) pnigbacc: f64,
    /// `BSIM4pnigbinv`
    pub(crate) pnigbinv: f64,
    /// `BSIM4pntox`
    pub(crate) pntox: f64,
    /// `BSIM4peigbinv`
    pub(crate) peigbinv: f64,
    /// `BSIM4ppigcd`
    pub(crate) ppigcd: f64,
    /// `BSIM4ppoxedge`
    pub(crate) ppoxedge: f64,
    /// `BSIM4pxrcrg1`
    pub(crate) pxrcrg1: f64,
    /// `BSIM4pxrcrg2`
    pub(crate) pxrcrg2: f64,
    /// `BSIM4plambda`
    pub(crate) plambda: f64,
    /// `BSIM4pvtl`
    pub(crate) pvtl: f64,
    /// `BSIM4pxn`
    pub(crate) pxn: f64,
    /// `BSIM4pvfbsdoff`
    pub(crate) pvfbsdoff: f64,
    /// `BSIM4ptvfbsdoff`
    pub(crate) ptvfbsdoff: f64,
    /// `BSIM4pcgsl`
    pub(crate) pcgsl: f64,
    /// `BSIM4pcgdl`
    pub(crate) pcgdl: f64,
    /// `BSIM4pckappas`
    pub(crate) pckappas: f64,
    /// `BSIM4pckappad`
    pub(crate) pckappad: f64,
    /// `BSIM4pcf`
    pub(crate) pcf: f64,
    /// `BSIM4pclc`
    pub(crate) pclc: f64,
    /// `BSIM4pcle`
    pub(crate) pcle: f64,
    /// `BSIM4pvfbcv`
    pub(crate) pvfbcv: f64,
    /// `BSIM4pnoff`
    pub(crate) pnoff: f64,
    /// `BSIM4pvoffcv`
    pub(crate) pvoffcv: f64,
    /// `BSIM4pacde`
    pub(crate) pacde: f64,
    /// `BSIM4pmoin`
    pub(crate) pmoin: f64,
    /// `BSIM4tnom`
    pub(crate) tnom: f64,
    /// `BSIM4cgso`
    pub(crate) cgso: f64,
    /// `BSIM4cgdo`
    pub(crate) cgdo: f64,
    /// `BSIM4cgbo`
    pub(crate) cgbo: f64,
    /// `BSIM4xpart`
    pub(crate) xpart: f64,
    /// `BSIM4cFringOut`
    pub(crate) c_fring_out: f64,
    /// `BSIM4cFringMax`
    pub(crate) c_fring_max: f64,
    /// `BSIM4sheetResistance`
    pub(crate) sheet_resistance: f64,
    /// `BSIM4SjctSatCurDensity`
    pub(crate) sjct_sat_cur_density: f64,
    /// `BSIM4DjctSatCurDensity`
    pub(crate) djct_sat_cur_density: f64,
    /// `BSIM4SjctSidewallSatCurDensity`
    pub(crate) sjct_sidewall_sat_cur_density: f64,
    /// `BSIM4DjctSidewallSatCurDensity`
    pub(crate) djct_sidewall_sat_cur_density: f64,
    /// `BSIM4SjctGateSidewallSatCurDensity`
    pub(crate) sjct_gate_sidewall_sat_cur_density: f64,
    /// `BSIM4DjctGateSidewallSatCurDensity`
    pub(crate) djct_gate_sidewall_sat_cur_density: f64,
    /// `BSIM4SbulkJctPotential`
    pub(crate) sbulk_jct_potential: f64,
    /// `BSIM4DbulkJctPotential`
    pub(crate) dbulk_jct_potential: f64,
    /// `BSIM4SbulkJctBotGradingCoeff`
    pub(crate) sbulk_jct_bot_grading_coeff: f64,
    /// `BSIM4DbulkJctBotGradingCoeff`
    pub(crate) dbulk_jct_bot_grading_coeff: f64,
    /// `BSIM4SbulkJctSideGradingCoeff`
    pub(crate) sbulk_jct_side_grading_coeff: f64,
    /// `BSIM4DbulkJctSideGradingCoeff`
    pub(crate) dbulk_jct_side_grading_coeff: f64,
    /// `BSIM4SbulkJctGateSideGradingCoeff`
    pub(crate) sbulk_jct_gate_side_grading_coeff: f64,
    /// `BSIM4DbulkJctGateSideGradingCoeff`
    pub(crate) dbulk_jct_gate_side_grading_coeff: f64,
    /// `BSIM4SsidewallJctPotential`
    pub(crate) ssidewall_jct_potential: f64,
    /// `BSIM4DsidewallJctPotential`
    pub(crate) dsidewall_jct_potential: f64,
    /// `BSIM4SGatesidewallJctPotential`
    pub(crate) sgatesidewall_jct_potential: f64,
    /// `BSIM4DGatesidewallJctPotential`
    pub(crate) dgatesidewall_jct_potential: f64,
    /// `BSIM4SunitAreaJctCap`
    pub(crate) sunit_area_jct_cap: f64,
    /// `BSIM4DunitAreaJctCap`
    pub(crate) dunit_area_jct_cap: f64,
    /// `BSIM4SunitLengthSidewallJctCap`
    pub(crate) sunit_length_sidewall_jct_cap: f64,
    /// `BSIM4DunitLengthSidewallJctCap`
    pub(crate) dunit_length_sidewall_jct_cap: f64,
    /// `BSIM4SunitLengthGateSidewallJctCap`
    pub(crate) sunit_length_gate_sidewall_jct_cap: f64,
    /// `BSIM4DunitLengthGateSidewallJctCap`
    pub(crate) dunit_length_gate_sidewall_jct_cap: f64,
    /// `BSIM4SjctEmissionCoeff`
    pub(crate) sjct_emission_coeff: f64,
    /// `BSIM4DjctEmissionCoeff`
    pub(crate) djct_emission_coeff: f64,
    /// `BSIM4SjctTempExponent`
    pub(crate) sjct_temp_exponent: f64,
    /// `BSIM4DjctTempExponent`
    pub(crate) djct_temp_exponent: f64,
    /// `BSIM4njtsstemp`
    pub(crate) njtsstemp: f64,
    /// `BSIM4njtsswstemp`
    pub(crate) njtsswstemp: f64,
    /// `BSIM4njtsswgstemp`
    pub(crate) njtsswgstemp: f64,
    /// `BSIM4njtsdtemp`
    pub(crate) njtsdtemp: f64,
    /// `BSIM4njtsswdtemp`
    pub(crate) njtsswdtemp: f64,
    /// `BSIM4njtsswgdtemp`
    pub(crate) njtsswgdtemp: f64,
    /// `BSIM4Lint`
    pub(crate) lint: f64,
    /// `BSIM4Ll`
    pub(crate) ll: f64,
    /// `BSIM4Llc`
    pub(crate) llc: f64,
    /// `BSIM4Lln`
    pub(crate) lln: f64,
    /// `BSIM4Lw`
    pub(crate) lw: f64,
    /// `BSIM4Lwc`
    pub(crate) lwc: f64,
    /// `BSIM4Lwn`
    pub(crate) lwn: f64,
    /// `BSIM4Lwl`
    pub(crate) lwl: f64,
    /// `BSIM4Lwlc`
    pub(crate) lwlc: f64,
    /// `BSIM4Lmin`
    pub(crate) lmin: f64,
    /// `BSIM4Lmax`
    pub(crate) lmax: f64,
    /// `BSIM4Wint`
    pub(crate) wint: f64,
    /// `BSIM4Wl`
    pub(crate) wl: f64,
    /// `BSIM4Wlc`
    pub(crate) wlc: f64,
    /// `BSIM4Wln`
    pub(crate) wln: f64,
    /// `BSIM4Ww`
    pub(crate) ww: f64,
    /// `BSIM4Wwc`
    pub(crate) wwc: f64,
    /// `BSIM4Wwn`
    pub(crate) wwn: f64,
    /// `BSIM4Wwl`
    pub(crate) wwl: f64,
    /// `BSIM4Wwlc`
    pub(crate) wwlc: f64,
    /// `BSIM4Wmin`
    pub(crate) wmin: f64,
    /// `BSIM4Wmax`
    pub(crate) wmax: f64,
    /// `BSIM4saref`
    pub(crate) saref: f64,
    /// `BSIM4sbref`
    pub(crate) sbref: f64,
    /// `BSIM4wlod`
    pub(crate) wlod: f64,
    /// `BSIM4ku0`
    pub(crate) ku0: f64,
    /// `BSIM4kvsat`
    pub(crate) kvsat: f64,
    /// `BSIM4kvth0`
    pub(crate) kvth0: f64,
    /// `BSIM4tku0`
    pub(crate) tku0: f64,
    /// `BSIM4llodku0`
    pub(crate) llodku0: f64,
    /// `BSIM4wlodku0`
    pub(crate) wlodku0: f64,
    /// `BSIM4llodvth`
    pub(crate) llodvth: f64,
    /// `BSIM4wlodvth`
    pub(crate) wlodvth: f64,
    /// `BSIM4lku0`
    pub(crate) lku0: f64,
    /// `BSIM4wku0`
    pub(crate) wku0: f64,
    /// `BSIM4pku0`
    pub(crate) pku0: f64,
    /// `BSIM4lkvth0`
    pub(crate) lkvth0: f64,
    /// `BSIM4wkvth0`
    pub(crate) wkvth0: f64,
    /// `BSIM4pkvth0`
    pub(crate) pkvth0: f64,
    /// `BSIM4stk2`
    pub(crate) stk2: f64,
    /// `BSIM4lodk2`
    pub(crate) lodk2: f64,
    /// `BSIM4steta0`
    pub(crate) steta0: f64,
    /// `BSIM4lodeta0`
    pub(crate) lodeta0: f64,
    /// `BSIM4web`
    pub(crate) web: f64,
    /// `BSIM4wec`
    pub(crate) wec: f64,
    /// `BSIM4kvth0we`
    pub(crate) kvth0we: f64,
    /// `BSIM4k2we`
    pub(crate) k2we: f64,
    /// `BSIM4ku0we`
    pub(crate) ku0we: f64,
    /// `BSIM4scref`
    pub(crate) scref: f64,
    /// `BSIM4wpemod`
    pub(crate) wpemod: f64,
    /// `BSIM4lkvth0we`
    pub(crate) lkvth0we: f64,
    /// `BSIM4lk2we`
    pub(crate) lk2we: f64,
    /// `BSIM4lku0we`
    pub(crate) lku0we: f64,
    /// `BSIM4wkvth0we`
    pub(crate) wkvth0we: f64,
    /// `BSIM4wk2we`
    pub(crate) wk2we: f64,
    /// `BSIM4wku0we`
    pub(crate) wku0we: f64,
    /// `BSIM4pkvth0we`
    pub(crate) pkvth0we: f64,
    /// `BSIM4pk2we`
    pub(crate) pk2we: f64,
    /// `BSIM4pku0we`
    pub(crate) pku0we: f64,
    /// `BSIM4Eg0`
    pub(crate) eg0: f64,
    /// `BSIM4vtm`
    pub(crate) vtm: f64,
    /// `BSIM4vtm0`
    pub(crate) vtm0: f64,
    /// `BSIM4coxe`
    pub(crate) coxe: f64,
    /// `BSIM4coxp`
    pub(crate) coxp: f64,
    /// `BSIM4cof1`
    pub(crate) cof1: f64,
    /// `BSIM4cof2`
    pub(crate) cof2: f64,
    /// `BSIM4cof3`
    pub(crate) cof3: f64,
    /// `BSIM4cof4`
    pub(crate) cof4: f64,
    /// `BSIM4vcrit`
    pub(crate) vcrit: f64,
    /// `BSIM4factor1`
    pub(crate) factor1: f64,
    /// `BSIM4PhiBS`
    pub(crate) phi_bs: f64,
    /// `BSIM4PhiBSWS`
    pub(crate) phi_bsws: f64,
    /// `BSIM4PhiBSWGS`
    pub(crate) phi_bswgs: f64,
    /// `BSIM4SjctTempSatCurDensity`
    pub(crate) sjct_temp_sat_cur_density: f64,
    /// `BSIM4SjctSidewallTempSatCurDensity`
    pub(crate) sjct_sidewall_temp_sat_cur_density: f64,
    /// `BSIM4SjctGateSidewallTempSatCurDensity`
    pub(crate) sjct_gate_sidewall_temp_sat_cur_density: f64,
    /// `BSIM4PhiBD`
    pub(crate) phi_bd: f64,
    /// `BSIM4PhiBSWD`
    pub(crate) phi_bswd: f64,
    /// `BSIM4PhiBSWGD`
    pub(crate) phi_bswgd: f64,
    /// `BSIM4DjctTempSatCurDensity`
    pub(crate) djct_temp_sat_cur_density: f64,
    /// `BSIM4DjctSidewallTempSatCurDensity`
    pub(crate) djct_sidewall_temp_sat_cur_density: f64,
    /// `BSIM4DjctGateSidewallTempSatCurDensity`
    pub(crate) djct_gate_sidewall_temp_sat_cur_density: f64,
    /// `BSIM4SunitAreaTempJctCap`
    pub(crate) sunit_area_temp_jct_cap: f64,
    /// `BSIM4DunitAreaTempJctCap`
    pub(crate) dunit_area_temp_jct_cap: f64,
    /// `BSIM4SunitLengthSidewallTempJctCap`
    pub(crate) sunit_length_sidewall_temp_jct_cap: f64,
    /// `BSIM4DunitLengthSidewallTempJctCap`
    pub(crate) dunit_length_sidewall_temp_jct_cap: f64,
    /// `BSIM4SunitLengthGateSidewallTempJctCap`
    pub(crate) sunit_length_gate_sidewall_temp_jct_cap: f64,
    /// `BSIM4DunitLengthGateSidewallTempJctCap`
    pub(crate) dunit_length_gate_sidewall_temp_jct_cap: f64,
    /// `BSIM4oxideTrapDensityA`
    pub(crate) oxide_trap_density_a: f64,
    /// `BSIM4oxideTrapDensityB`
    pub(crate) oxide_trap_density_b: f64,
    /// `BSIM4oxideTrapDensityC`
    pub(crate) oxide_trap_density_c: f64,
    /// `BSIM4em`
    pub(crate) em: f64,
    /// `BSIM4ef`
    pub(crate) ef: f64,
    /// `BSIM4af`
    pub(crate) af: f64,
    /// `BSIM4kf`
    pub(crate) kf: f64,
    /// `BSIM4vgsMax`
    pub(crate) vgs_max: f64,
    /// `BSIM4vgdMax`
    pub(crate) vgd_max: f64,
    /// `BSIM4vgbMax`
    pub(crate) vgb_max: f64,
    /// `BSIM4vdsMax`
    pub(crate) vds_max: f64,
    /// `BSIM4vbsMax`
    pub(crate) vbs_max: f64,
    /// `BSIM4vbdMax`
    pub(crate) vbd_max: f64,
    /// `BSIM4vgsrMax`
    pub(crate) vgsr_max: f64,
    /// `BSIM4vgdrMax`
    pub(crate) vgdr_max: f64,
    /// `BSIM4vgbrMax`
    pub(crate) vgbr_max: f64,
    /// `BSIM4vbsrMax`
    pub(crate) vbsr_max: f64,
    /// `BSIM4vbdrMax`
    pub(crate) vbdr_max: f64,
    /// `BSIM4gidlclamp`
    pub(crate) gidlclamp: f64,
    /// `BSIM4idovvdsc`
    pub(crate) idovvdsc: f64,
    pub(crate) given: Bsim4ModelGiven,
}

/// The C's `BSIM4xxxGiven` model flags.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub(crate) struct Bsim4ModelGiven {
    pub mob_mod: bool,
    pub bin_unit: bool,
    pub cvcharge_mod: bool,
    pub cap_mod: bool,
    pub dio_mod: bool,
    pub rds_mod: bool,
    pub rbody_mod: bool,
    pub rgate_mod: bool,
    pub per_mod: bool,
    pub geo_mod: bool,
    pub param_chk: bool,
    pub trnqs_mod: bool,
    pub acnqs_mod: bool,
    pub fnoi_mod: bool,
    pub tnoi_mod: bool,
    pub mtrl_mod: bool,
    pub mtrl_compat_mod: bool,
    pub gidl_mod: bool,
    pub igc_mod: bool,
    pub igb_mod: bool,
    pub temp_mod: bool,
    pub type_: bool,
    pub toxref: bool,
    pub eot: bool,
    pub vddeot: bool,
    pub tempeot: bool,
    pub leffeot: bool,
    pub weffeot: bool,
    pub ados: bool,
    pub bdos: bool,
    pub toxe: bool,
    pub toxp: bool,
    pub toxm: bool,
    pub dtox: bool,
    pub epsrox: bool,
    pub version: bool,
    pub cdsc: bool,
    pub cdscb: bool,
    pub cdscd: bool,
    pub cit: bool,
    pub nfactor: bool,
    pub xj: bool,
    pub vsat: bool,
    pub at: bool,
    pub a0: bool,
    pub ags: bool,
    pub a1: bool,
    pub a2: bool,
    pub keta: bool,
    pub ketac: bool,
    pub nsub: bool,
    pub phig: bool,
    pub epsrgate: bool,
    pub easub: bool,
    pub epsrsub: bool,
    pub ni0sub: bool,
    pub bg0sub: bool,
    pub tbgasub: bool,
    pub tbgbsub: bool,
    pub ndep: bool,
    pub nsd: bool,
    pub phin: bool,
    pub ngate: bool,
    pub gamma1: bool,
    pub gamma2: bool,
    pub vbx: bool,
    pub vbm: bool,
    pub xt: bool,
    pub k1: bool,
    pub kt1: bool,
    pub kt1l: bool,
    pub kt2: bool,
    pub k2: bool,
    pub k3: bool,
    pub k3b: bool,
    pub w0: bool,
    pub dvtp0: bool,
    pub dvtp1: bool,
    pub dvtp2: bool,
    pub dvtp3: bool,
    pub dvtp4: bool,
    pub dvtp5: bool,
    pub lpe0: bool,
    pub lpeb: bool,
    pub dvt0: bool,
    pub dvt1: bool,
    pub dvt2: bool,
    pub dvt0w: bool,
    pub dvt1w: bool,
    pub dvt2w: bool,
    pub drout: bool,
    pub dsub: bool,
    pub vth0: bool,
    pub eu: bool,
    pub ucs: bool,
    pub ua: bool,
    pub ua1: bool,
    pub ub: bool,
    pub ub1: bool,
    pub uc: bool,
    pub uc1: bool,
    pub ud: bool,
    pub ud1: bool,
    pub up: bool,
    pub lp: bool,
    pub u0: bool,
    pub ute: bool,
    pub ucste: bool,
    pub voff: bool,
    pub tvoff: bool,
    pub tnfactor: bool,
    pub teta0: bool,
    pub tvoffcv: bool,
    pub voffl: bool,
    pub voffcvl: bool,
    pub minv: bool,
    pub minvcv: bool,
    pub rdsw: bool,
    pub rdswmin: bool,
    pub rdwmin: bool,
    pub rswmin: bool,
    pub rsw: bool,
    pub rdw: bool,
    pub prwg: bool,
    pub prwb: bool,
    pub prt: bool,
    pub eta0: bool,
    pub etab: bool,
    pub pclm: bool,
    pub pdibl1: bool,
    pub pdibl2: bool,
    pub pdiblb: bool,
    pub fprout: bool,
    pub pdits: bool,
    pub pditsd: bool,
    pub pditsl: bool,
    pub pscbe1: bool,
    pub pscbe2: bool,
    pub pvag: bool,
    pub delta: bool,
    pub wr: bool,
    pub dwg: bool,
    pub dwb: bool,
    pub b0: bool,
    pub b1: bool,
    pub alpha0: bool,
    pub alpha1: bool,
    pub beta0: bool,
    pub agidl: bool,
    pub bgidl: bool,
    pub cgidl: bool,
    pub egidl: bool,
    pub fgidl: bool,
    pub kgidl: bool,
    pub rgidl: bool,
    pub agisl: bool,
    pub bgisl: bool,
    pub cgisl: bool,
    pub egisl: bool,
    pub fgisl: bool,
    pub kgisl: bool,
    pub rgisl: bool,
    pub aigc: bool,
    pub bigc: bool,
    pub cigc: bool,
    pub aigsd: bool,
    pub bigsd: bool,
    pub cigsd: bool,
    pub aigs: bool,
    pub bigs: bool,
    pub cigs: bool,
    pub aigd: bool,
    pub bigd: bool,
    pub cigd: bool,
    pub aigbacc: bool,
    pub bigbacc: bool,
    pub cigbacc: bool,
    pub aigbinv: bool,
    pub bigbinv: bool,
    pub cigbinv: bool,
    pub nigc: bool,
    pub nigbinv: bool,
    pub nigbacc: bool,
    pub ntox: bool,
    pub eigbinv: bool,
    pub pigcd: bool,
    pub poxedge: bool,
    pub ijthdfwd: bool,
    pub ijthsfwd: bool,
    pub ijthdrev: bool,
    pub ijthsrev: bool,
    pub xjbvd: bool,
    pub xjbvs: bool,
    pub bvd: bool,
    pub bvs: bool,
    pub jtss: bool,
    pub jtsd: bool,
    pub jtssws: bool,
    pub jtsswd: bool,
    pub jtsswgs: bool,
    pub jtsswgd: bool,
    pub jtweff: bool,
    pub njts: bool,
    pub njtssw: bool,
    pub njtsswg: bool,
    pub njtsd: bool,
    pub njtsswd: bool,
    pub njtsswgd: bool,
    pub xtss: bool,
    pub xtsd: bool,
    pub xtssws: bool,
    pub xtsswd: bool,
    pub xtsswgs: bool,
    pub xtsswgd: bool,
    pub tnjts: bool,
    pub tnjtssw: bool,
    pub tnjtsswg: bool,
    pub tnjtsd: bool,
    pub tnjtsswd: bool,
    pub tnjtsswgd: bool,
    pub vtss: bool,
    pub vtsd: bool,
    pub vtssws: bool,
    pub vtsswd: bool,
    pub vtsswgs: bool,
    pub vtsswgd: bool,
    pub vfb: bool,
    pub gbmin: bool,
    pub rbdb: bool,
    pub rbsb: bool,
    pub rbps: bool,
    pub rbpd: bool,
    pub rbpb: bool,
    pub rbps0: bool,
    pub rbpsl: bool,
    pub rbpsw: bool,
    pub rbpsnf: bool,
    pub rbpd0: bool,
    pub rbpdl: bool,
    pub rbpdw: bool,
    pub rbpdnf: bool,
    pub rbpbx0: bool,
    pub rbpbxl: bool,
    pub rbpbxw: bool,
    pub rbpbxnf: bool,
    pub rbpby0: bool,
    pub rbpbyl: bool,
    pub rbpbyw: bool,
    pub rbpbynf: bool,
    pub rbsbx0: bool,
    pub rbsby0: bool,
    pub rbdbx0: bool,
    pub rbdby0: bool,
    pub rbsdbxl: bool,
    pub rbsdbxw: bool,
    pub rbsdbxnf: bool,
    pub rbsdbyl: bool,
    pub rbsdbyw: bool,
    pub rbsdbynf: bool,
    pub xrcrg1: bool,
    pub xrcrg2: bool,
    pub tnoia: bool,
    pub tnoib: bool,
    pub tnoic: bool,
    pub rnoia: bool,
    pub rnoib: bool,
    pub rnoic: bool,
    pub ntnoi: bool,
    pub lambda: bool,
    pub vtl: bool,
    pub lc: bool,
    pub xn: bool,
    pub vfbsdoff: bool,
    pub lintnoi: bool,
    pub tvfbsdoff: bool,
    pub cgsl: bool,
    pub cgdl: bool,
    pub ckappas: bool,
    pub ckappad: bool,
    pub cf: bool,
    pub vfbcv: bool,
    pub clc: bool,
    pub cle: bool,
    pub dwc: bool,
    pub dlc: bool,
    pub xw: bool,
    pub xl: bool,
    pub dlcig: bool,
    pub dlcigd: bool,
    pub dwj: bool,
    pub noff: bool,
    pub voffcv: bool,
    pub acde: bool,
    pub moin: bool,
    pub tcj: bool,
    pub tcjsw: bool,
    pub tcjswg: bool,
    pub tpb: bool,
    pub tpbsw: bool,
    pub tpbswg: bool,
    pub dmcg: bool,
    pub dmci: bool,
    pub dmdg: bool,
    pub dmcgt: bool,
    pub xgw: bool,
    pub xgl: bool,
    pub rshg: bool,
    pub ngcon: bool,
    pub lcdsc: bool,
    pub lcdscb: bool,
    pub lcdscd: bool,
    pub lcit: bool,
    pub lnfactor: bool,
    pub lxj: bool,
    pub lvsat: bool,
    pub lat: bool,
    pub la0: bool,
    pub lags: bool,
    pub la1: bool,
    pub la2: bool,
    pub lketa: bool,
    pub lketac: bool,
    pub lnsub: bool,
    pub lndep: bool,
    pub lnsd: bool,
    pub lphin: bool,
    pub lngate: bool,
    pub lgamma1: bool,
    pub lgamma2: bool,
    pub lvbx: bool,
    pub lvbm: bool,
    pub lxt: bool,
    pub lk1: bool,
    pub lkt1: bool,
    pub lkt1l: bool,
    pub lkt2: bool,
    pub lk2: bool,
    pub lk3: bool,
    pub lk3b: bool,
    pub lw0: bool,
    pub ldvtp0: bool,
    pub ldvtp1: bool,
    pub ldvtp2: bool,
    pub ldvtp3: bool,
    pub ldvtp4: bool,
    pub ldvtp5: bool,
    pub llpe0: bool,
    pub llpeb: bool,
    pub ldvt0: bool,
    pub ldvt1: bool,
    pub ldvt2: bool,
    pub ldvt0w: bool,
    pub ldvt1w: bool,
    pub ldvt2w: bool,
    pub ldrout: bool,
    pub ldsub: bool,
    pub lvth0: bool,
    pub lua: bool,
    pub lua1: bool,
    pub lub: bool,
    pub lub1: bool,
    pub luc: bool,
    pub luc1: bool,
    pub lud: bool,
    pub lud1: bool,
    pub lup: bool,
    pub llp: bool,
    pub lu0: bool,
    pub leu: bool,
    pub lucs: bool,
    pub lute: bool,
    pub lucste: bool,
    pub lvoff: bool,
    pub ltvoff: bool,
    pub ltnfactor: bool,
    pub lteta0: bool,
    pub ltvoffcv: bool,
    pub lminv: bool,
    pub lminvcv: bool,
    pub lrdsw: bool,
    pub lrsw: bool,
    pub lrdw: bool,
    pub lprwg: bool,
    pub lprwb: bool,
    pub lprt: bool,
    pub leta0: bool,
    pub letab: bool,
    pub lpclm: bool,
    pub lpdibl1: bool,
    pub lpdibl2: bool,
    pub lpdiblb: bool,
    pub lfprout: bool,
    pub lpdits: bool,
    pub lpditsd: bool,
    pub lpscbe1: bool,
    pub lpscbe2: bool,
    pub lpvag: bool,
    pub ldelta: bool,
    pub lwr: bool,
    pub ldwg: bool,
    pub ldwb: bool,
    pub lb0: bool,
    pub lb1: bool,
    pub lalpha0: bool,
    pub lalpha1: bool,
    pub lbeta0: bool,
    pub lvfb: bool,
    pub lagidl: bool,
    pub lbgidl: bool,
    pub lcgidl: bool,
    pub legidl: bool,
    pub lfgidl: bool,
    pub lkgidl: bool,
    pub lrgidl: bool,
    pub lagisl: bool,
    pub lbgisl: bool,
    pub lcgisl: bool,
    pub legisl: bool,
    pub lfgisl: bool,
    pub lkgisl: bool,
    pub lrgisl: bool,
    pub laigc: bool,
    pub lbigc: bool,
    pub lcigc: bool,
    pub laigsd: bool,
    pub lbigsd: bool,
    pub lcigsd: bool,
    pub laigs: bool,
    pub lbigs: bool,
    pub lcigs: bool,
    pub laigd: bool,
    pub lbigd: bool,
    pub lcigd: bool,
    pub laigbacc: bool,
    pub lbigbacc: bool,
    pub lcigbacc: bool,
    pub laigbinv: bool,
    pub lbigbinv: bool,
    pub lcigbinv: bool,
    pub lnigc: bool,
    pub lnigbinv: bool,
    pub lnigbacc: bool,
    pub lntox: bool,
    pub leigbinv: bool,
    pub lpigcd: bool,
    pub lpoxedge: bool,
    pub lxrcrg1: bool,
    pub lxrcrg2: bool,
    pub llambda: bool,
    pub lvtl: bool,
    pub lxn: bool,
    pub lvfbsdoff: bool,
    pub ltvfbsdoff: bool,
    pub lcgsl: bool,
    pub lcgdl: bool,
    pub lckappas: bool,
    pub lckappad: bool,
    pub lcf: bool,
    pub lclc: bool,
    pub lcle: bool,
    pub lvfbcv: bool,
    pub lnoff: bool,
    pub lvoffcv: bool,
    pub lacde: bool,
    pub lmoin: bool,
    pub wcdsc: bool,
    pub wcdscb: bool,
    pub wcdscd: bool,
    pub wcit: bool,
    pub wnfactor: bool,
    pub wxj: bool,
    pub wvsat: bool,
    pub wat: bool,
    pub wa0: bool,
    pub wags: bool,
    pub wa1: bool,
    pub wa2: bool,
    pub wketa: bool,
    pub wketac: bool,
    pub wnsub: bool,
    pub wndep: bool,
    pub wnsd: bool,
    pub wphin: bool,
    pub wngate: bool,
    pub wgamma1: bool,
    pub wgamma2: bool,
    pub wvbx: bool,
    pub wvbm: bool,
    pub wxt: bool,
    pub wk1: bool,
    pub wkt1: bool,
    pub wkt1l: bool,
    pub wkt2: bool,
    pub wk2: bool,
    pub wk3: bool,
    pub wk3b: bool,
    pub ww0: bool,
    pub wdvtp0: bool,
    pub wdvtp1: bool,
    pub wdvtp2: bool,
    pub wdvtp3: bool,
    pub wdvtp4: bool,
    pub wdvtp5: bool,
    pub wlpe0: bool,
    pub wlpeb: bool,
    pub wdvt0: bool,
    pub wdvt1: bool,
    pub wdvt2: bool,
    pub wdvt0w: bool,
    pub wdvt1w: bool,
    pub wdvt2w: bool,
    pub wdrout: bool,
    pub wdsub: bool,
    pub wvth0: bool,
    pub wua: bool,
    pub wua1: bool,
    pub wub: bool,
    pub wub1: bool,
    pub wuc: bool,
    pub wuc1: bool,
    pub wud: bool,
    pub wud1: bool,
    pub wup: bool,
    pub wlp: bool,
    pub wu0: bool,
    pub weu: bool,
    pub wucs: bool,
    pub wute: bool,
    pub wucste: bool,
    pub wvoff: bool,
    pub wtvoff: bool,
    pub wtnfactor: bool,
    pub wteta0: bool,
    pub wtvoffcv: bool,
    pub wminv: bool,
    pub wminvcv: bool,
    pub wrdsw: bool,
    pub wrsw: bool,
    pub wrdw: bool,
    pub wprwg: bool,
    pub wprwb: bool,
    pub wprt: bool,
    pub weta0: bool,
    pub wetab: bool,
    pub wpclm: bool,
    pub wpdibl1: bool,
    pub wpdibl2: bool,
    pub wpdiblb: bool,
    pub wfprout: bool,
    pub wpdits: bool,
    pub wpditsd: bool,
    pub wpscbe1: bool,
    pub wpscbe2: bool,
    pub wpvag: bool,
    pub wdelta: bool,
    pub wwr: bool,
    pub wdwg: bool,
    pub wdwb: bool,
    pub wb0: bool,
    pub wb1: bool,
    pub walpha0: bool,
    pub walpha1: bool,
    pub wbeta0: bool,
    pub wvfb: bool,
    pub wagidl: bool,
    pub wbgidl: bool,
    pub wcgidl: bool,
    pub wegidl: bool,
    pub wfgidl: bool,
    pub wkgidl: bool,
    pub wrgidl: bool,
    pub wagisl: bool,
    pub wbgisl: bool,
    pub wcgisl: bool,
    pub wegisl: bool,
    pub wfgisl: bool,
    pub wkgisl: bool,
    pub wrgisl: bool,
    pub waigc: bool,
    pub wbigc: bool,
    pub wcigc: bool,
    pub waigsd: bool,
    pub wbigsd: bool,
    pub wcigsd: bool,
    pub waigs: bool,
    pub wbigs: bool,
    pub wcigs: bool,
    pub waigd: bool,
    pub wbigd: bool,
    pub wcigd: bool,
    pub waigbacc: bool,
    pub wbigbacc: bool,
    pub wcigbacc: bool,
    pub waigbinv: bool,
    pub wbigbinv: bool,
    pub wcigbinv: bool,
    pub wnigc: bool,
    pub wnigbinv: bool,
    pub wnigbacc: bool,
    pub wntox: bool,
    pub weigbinv: bool,
    pub wpigcd: bool,
    pub wpoxedge: bool,
    pub wxrcrg1: bool,
    pub wxrcrg2: bool,
    pub wlambda: bool,
    pub wvtl: bool,
    pub wxn: bool,
    pub wvfbsdoff: bool,
    pub wtvfbsdoff: bool,
    pub wcgsl: bool,
    pub wcgdl: bool,
    pub wckappas: bool,
    pub wckappad: bool,
    pub wcf: bool,
    pub wclc: bool,
    pub wcle: bool,
    pub wvfbcv: bool,
    pub wnoff: bool,
    pub wvoffcv: bool,
    pub wacde: bool,
    pub wmoin: bool,
    pub pcdsc: bool,
    pub pcdscb: bool,
    pub pcdscd: bool,
    pub pcit: bool,
    pub pnfactor: bool,
    pub pxj: bool,
    pub pvsat: bool,
    pub pat: bool,
    pub pa0: bool,
    pub pags: bool,
    pub pa1: bool,
    pub pa2: bool,
    pub pketa: bool,
    pub pketac: bool,
    pub pnsub: bool,
    pub pndep: bool,
    pub pnsd: bool,
    pub pphin: bool,
    pub pngate: bool,
    pub pgamma1: bool,
    pub pgamma2: bool,
    pub pvbx: bool,
    pub pvbm: bool,
    pub pxt: bool,
    pub pk1: bool,
    pub pkt1: bool,
    pub pkt1l: bool,
    pub pkt2: bool,
    pub pk2: bool,
    pub pk3: bool,
    pub pk3b: bool,
    pub pw0: bool,
    pub pdvtp0: bool,
    pub pdvtp1: bool,
    pub pdvtp2: bool,
    pub pdvtp3: bool,
    pub pdvtp4: bool,
    pub pdvtp5: bool,
    pub plpe0: bool,
    pub plpeb: bool,
    pub pdvt0: bool,
    pub pdvt1: bool,
    pub pdvt2: bool,
    pub pdvt0w: bool,
    pub pdvt1w: bool,
    pub pdvt2w: bool,
    pub pdrout: bool,
    pub pdsub: bool,
    pub pvth0: bool,
    pub pua: bool,
    pub pua1: bool,
    pub pub_: bool,
    pub pub1: bool,
    pub puc: bool,
    pub puc1: bool,
    pub pud: bool,
    pub pud1: bool,
    pub pup: bool,
    pub plp: bool,
    pub pu0: bool,
    pub peu: bool,
    pub pucs: bool,
    pub pute: bool,
    pub pucste: bool,
    pub pvoff: bool,
    pub ptvoff: bool,
    pub ptnfactor: bool,
    pub pteta0: bool,
    pub ptvoffcv: bool,
    pub pminv: bool,
    pub pminvcv: bool,
    pub prdsw: bool,
    pub prsw: bool,
    pub prdw: bool,
    pub pprwg: bool,
    pub pprwb: bool,
    pub pprt: bool,
    pub peta0: bool,
    pub petab: bool,
    pub ppclm: bool,
    pub ppdibl1: bool,
    pub ppdibl2: bool,
    pub ppdiblb: bool,
    pub pfprout: bool,
    pub ppdits: bool,
    pub ppditsd: bool,
    pub ppscbe1: bool,
    pub ppscbe2: bool,
    pub ppvag: bool,
    pub pdelta: bool,
    pub pwr: bool,
    pub pdwg: bool,
    pub pdwb: bool,
    pub pb0: bool,
    pub pb1: bool,
    pub palpha0: bool,
    pub palpha1: bool,
    pub pbeta0: bool,
    pub pvfb: bool,
    pub pagidl: bool,
    pub pbgidl: bool,
    pub pcgidl: bool,
    pub pegidl: bool,
    pub pfgidl: bool,
    pub pkgidl: bool,
    pub prgidl: bool,
    pub pagisl: bool,
    pub pbgisl: bool,
    pub pcgisl: bool,
    pub pegisl: bool,
    pub pfgisl: bool,
    pub pkgisl: bool,
    pub prgisl: bool,
    pub paigc: bool,
    pub pbigc: bool,
    pub pcigc: bool,
    pub paigsd: bool,
    pub pbigsd: bool,
    pub pcigsd: bool,
    pub paigs: bool,
    pub pbigs: bool,
    pub pcigs: bool,
    pub paigd: bool,
    pub pbigd: bool,
    pub pcigd: bool,
    pub paigbacc: bool,
    pub pbigbacc: bool,
    pub pcigbacc: bool,
    pub paigbinv: bool,
    pub pbigbinv: bool,
    pub pcigbinv: bool,
    pub pnigc: bool,
    pub pnigbinv: bool,
    pub pnigbacc: bool,
    pub pntox: bool,
    pub peigbinv: bool,
    pub ppigcd: bool,
    pub ppoxedge: bool,
    pub pxrcrg1: bool,
    pub pxrcrg2: bool,
    pub plambda: bool,
    pub pvtl: bool,
    pub pxn: bool,
    pub pvfbsdoff: bool,
    pub ptvfbsdoff: bool,
    pub pcgsl: bool,
    pub pcgdl: bool,
    pub pckappas: bool,
    pub pckappad: bool,
    pub pcf: bool,
    pub pclc: bool,
    pub pcle: bool,
    pub pvfbcv: bool,
    pub pnoff: bool,
    pub pvoffcv: bool,
    pub pacde: bool,
    pub pmoin: bool,
    pub use_fringe: bool,
    pub tnom: bool,
    pub cgso: bool,
    pub cgdo: bool,
    pub cgbo: bool,
    pub xpart: bool,
    pub sheet_resistance: bool,
    pub sjct_sat_cur_density: bool,
    pub sjct_sidewall_sat_cur_density: bool,
    pub sjct_gate_sidewall_sat_cur_density: bool,
    pub sbulk_jct_potential: bool,
    pub sbulk_jct_bot_grading_coeff: bool,
    pub ssidewall_jct_potential: bool,
    pub sgatesidewall_jct_potential: bool,
    pub sbulk_jct_side_grading_coeff: bool,
    pub sunit_area_jct_cap: bool,
    pub sunit_length_sidewall_jct_cap: bool,
    pub sbulk_jct_gate_side_grading_coeff: bool,
    pub sunit_length_gate_sidewall_jct_cap: bool,
    pub sjct_emission_coeff: bool,
    pub sjct_temp_exponent: bool,
    pub djct_sat_cur_density: bool,
    pub djct_sidewall_sat_cur_density: bool,
    pub djct_gate_sidewall_sat_cur_density: bool,
    pub dbulk_jct_potential: bool,
    pub dbulk_jct_bot_grading_coeff: bool,
    pub dsidewall_jct_potential: bool,
    pub dgatesidewall_jct_potential: bool,
    pub dbulk_jct_side_grading_coeff: bool,
    pub dunit_area_jct_cap: bool,
    pub dunit_length_sidewall_jct_cap: bool,
    pub dbulk_jct_gate_side_grading_coeff: bool,
    pub dunit_length_gate_sidewall_jct_cap: bool,
    pub djct_emission_coeff: bool,
    pub djct_temp_exponent: bool,
    pub oxide_trap_density_a: bool,
    pub oxide_trap_density_b: bool,
    pub oxide_trap_density_c: bool,
    pub em: bool,
    pub ef: bool,
    pub af: bool,
    pub kf: bool,
    pub vgs_max: bool,
    pub vgd_max: bool,
    pub vgb_max: bool,
    pub vds_max: bool,
    pub vbs_max: bool,
    pub vbd_max: bool,
    pub vgsr_max: bool,
    pub vgdr_max: bool,
    pub vgbr_max: bool,
    pub vbsr_max: bool,
    pub vbdr_max: bool,
    pub lint: bool,
    pub ll: bool,
    pub llc: bool,
    pub lln: bool,
    pub lw: bool,
    pub lwc: bool,
    pub lwn: bool,
    pub lwl: bool,
    pub lwlc: bool,
    pub lmin: bool,
    pub lmax: bool,
    pub wint: bool,
    pub wl: bool,
    pub wlc: bool,
    pub wln: bool,
    pub ww: bool,
    pub wwc: bool,
    pub wwn: bool,
    pub wwl: bool,
    pub wwlc: bool,
    pub wmin: bool,
    pub wmax: bool,
    pub saref: bool,
    pub sbref: bool,
    pub wlod: bool,
    pub ku0: bool,
    pub kvsat: bool,
    pub kvth0: bool,
    pub tku0: bool,
    pub llodku0: bool,
    pub wlodku0: bool,
    pub llodvth: bool,
    pub wlodvth: bool,
    pub lku0: bool,
    pub wku0: bool,
    pub pku0: bool,
    pub lkvth0: bool,
    pub wkvth0: bool,
    pub pkvth0: bool,
    pub stk2: bool,
    pub lodk2: bool,
    pub steta0: bool,
    pub lodeta0: bool,
    pub web: bool,
    pub wec: bool,
    pub kvth0we: bool,
    pub k2we: bool,
    pub ku0we: bool,
    pub scref: bool,
    pub wpemod: bool,
    pub lkvth0we: bool,
    pub lk2we: bool,
    pub lku0we: bool,
    pub wkvth0we: bool,
    pub wk2we: bool,
    pub wku0we: bool,
    pub pkvth0we: bool,
    pub pk2we: bool,
    pub pku0we: bool,
    pub gidlclamp: bool,
    pub idovvdsc: bool,
}

/// `struct bsim4SizeDependParam` (the C's `pParam`): the
/// size-dependent parameter set built once per drawn (L, W, NF).
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub(crate) struct Bsim4Size {
    pub width: f64,
    pub length: f64,
    pub nfinger: f64,
    pub cdsc: f64,
    pub cdscb: f64,
    pub cdscd: f64,
    pub cit: f64,
    pub nfactor: f64,
    pub xj: f64,
    pub vsat: f64,
    pub at: f64,
    pub a0: f64,
    pub ags: f64,
    pub a1: f64,
    pub a2: f64,
    pub keta: f64,
    pub ketac: f64,
    pub nsub: f64,
    pub ndep: f64,
    pub nsd: f64,
    pub phin: f64,
    pub ngate: f64,
    pub gamma1: f64,
    pub gamma2: f64,
    pub vbx: f64,
    pub vbi: f64,
    pub vbm: f64,
    pub xt: f64,
    pub phi: f64,
    pub litl: f64,
    pub k1: f64,
    pub kt1: f64,
    pub kt1l: f64,
    pub kt2: f64,
    pub k2: f64,
    pub k3: f64,
    pub k3b: f64,
    pub w0: f64,
    pub dvtp0: f64,
    pub dvtp1: f64,
    pub dvtp2: f64,
    pub dvtp3: f64,
    pub dvtp4: f64,
    pub dvtp5: f64,
    pub lpe0: f64,
    pub lpeb: f64,
    pub dvt0: f64,
    pub dvt1: f64,
    pub dvt2: f64,
    pub dvt0w: f64,
    pub dvt1w: f64,
    pub dvt2w: f64,
    pub drout: f64,
    pub dsub: f64,
    pub vth0: f64,
    pub ua: f64,
    pub ua1: f64,
    pub ub: f64,
    pub ub1: f64,
    pub uc: f64,
    pub uc1: f64,
    pub ud: f64,
    pub ud1: f64,
    pub up: f64,
    pub lp: f64,
    pub u0: f64,
    pub eu: f64,
    pub ucs: f64,
    pub ute: f64,
    pub ucste: f64,
    pub voff: f64,
    pub tvoff: f64,
    pub tnfactor: f64,
    pub teta0: f64,
    pub tvoffcv: f64,
    pub minv: f64,
    pub minvcv: f64,
    pub vfb: f64,
    pub delta: f64,
    pub rdsw: f64,
    pub rds0: f64,
    pub rs0: f64,
    pub rd0: f64,
    pub rsw: f64,
    pub rdw: f64,
    pub prwg: f64,
    pub prwb: f64,
    pub prt: f64,
    pub eta0: f64,
    pub etab: f64,
    pub pclm: f64,
    pub pdibl1: f64,
    pub pdibl2: f64,
    pub pdiblb: f64,
    pub fprout: f64,
    pub pdits: f64,
    pub pditsd: f64,
    pub pscbe1: f64,
    pub pscbe2: f64,
    pub pvag: f64,
    pub wr: f64,
    pub dwg: f64,
    pub dwb: f64,
    pub b0: f64,
    pub b1: f64,
    pub alpha0: f64,
    pub alpha1: f64,
    pub beta0: f64,
    pub agidl: f64,
    pub bgidl: f64,
    pub cgidl: f64,
    pub egidl: f64,
    pub fgidl: f64,
    pub kgidl: f64,
    pub rgidl: f64,
    pub agisl: f64,
    pub bgisl: f64,
    pub cgisl: f64,
    pub egisl: f64,
    pub fgisl: f64,
    pub kgisl: f64,
    pub rgisl: f64,
    pub aigc: f64,
    pub bigc: f64,
    pub cigc: f64,
    pub aigsd: f64,
    pub bigsd: f64,
    pub cigsd: f64,
    pub aigs: f64,
    pub bigs: f64,
    pub cigs: f64,
    pub aigd: f64,
    pub bigd: f64,
    pub cigd: f64,
    pub aigbacc: f64,
    pub bigbacc: f64,
    pub cigbacc: f64,
    pub aigbinv: f64,
    pub bigbinv: f64,
    pub cigbinv: f64,
    pub nigc: f64,
    pub nigbacc: f64,
    pub nigbinv: f64,
    pub ntox: f64,
    pub eigbinv: f64,
    pub pigcd: f64,
    pub poxedge: f64,
    pub xrcrg1: f64,
    pub xrcrg2: f64,
    pub lambda: f64,
    pub vtl: f64,
    pub xn: f64,
    pub lc: f64,
    pub tfactor: f64,
    pub vfbsdoff: f64,
    pub tvfbsdoff: f64,
    pub ku0: f64,
    pub kvth0: f64,
    pub ku0temp: f64,
    pub rho_ref: f64,
    pub inv_od_ref: f64,
    pub kvth0we: f64,
    pub k2we: f64,
    pub ku0we: f64,
    pub cgsl: f64,
    pub cgdl: f64,
    pub ckappas: f64,
    pub ckappad: f64,
    pub cf: f64,
    pub clc: f64,
    pub cle: f64,
    pub vfbcv: f64,
    pub noff: f64,
    pub voffcv: f64,
    pub acde: f64,
    pub moin: f64,
    pub dw: f64,
    pub dl: f64,
    pub leff: f64,
    pub weff: f64,
    pub dwc: f64,
    pub dlc: f64,
    pub dwj: f64,
    pub leff_cv: f64,
    pub weff_cv: f64,
    pub weff_cj: f64,
    pub abulk_cvfactor: f64,
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    pub u0temp: f64,
    pub vsattemp: f64,
    pub sqrt_phi: f64,
    pub phis3: f64,
    pub xdep0: f64,
    pub sqrt_xdep0: f64,
    pub theta0vb0: f64,
    pub theta_rout: f64,
    pub mstar: f64,
    pub vgsteff_vth: f64,
    pub mstarcv: f64,
    pub voffcbn: f64,
    pub voffcbncv: f64,
    pub rdswmin: f64,
    pub rdwmin: f64,
    pub rswmin: f64,
    pub vfbsd: f64,
    pub cof1: f64,
    pub cof2: f64,
    pub cof3: f64,
    pub cof4: f64,
    pub cdep0: f64,
    pub tox_ratio: f64,
    pub aechvb: f64,
    pub bechvb: f64,
    pub tox_ratio_edge: f64,
    pub aechvb_edge_s: f64,
    pub aechvb_edge_d: f64,
    pub bechvb_edge: f64,
    pub ldeb: f64,
    pub k1ox: f64,
    pub k2ox: f64,
    pub vfbzbfactor: f64,
    pub dvtp2factor: f64,
}

/// The `sBSIM4instance` fields (`here->BSIM4…`): node numbers, matrix
/// slots, instance parameters, the per-instance temperature results and
/// the operating point the load leaves for the AC, noise and
/// convergence routines.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct Bsim4Here {
    pub d_node: Var,
    pub g_node_ext: Var,
    pub s_node: Var,
    pub b_node: Var,
    pub d_node_prime: Var,
    pub g_node_prime: Var,
    pub g_node_mid: Var,
    pub s_node_prime: Var,
    pub b_node_prime: Var,
    pub db_node: Var,
    pub sb_node: Var,
    pub q_node: Var,
    pub ueff: f64,
    pub thetavth: f64,
    pub von: f64,
    pub vdsat: f64,
    pub cgdo: f64,
    pub qgdo: f64,
    pub cgso: f64,
    pub qgso: f64,
    pub grbsb: f64,
    pub grbdb: f64,
    pub grbpb: f64,
    pub grbps: f64,
    pub grbpd: f64,
    pub vjsm_fwd: f64,
    pub vjsm_rev: f64,
    pub vjdm_fwd: f64,
    pub vjdm_rev: f64,
    pub xexp_bvs: f64,
    pub xexp_bvd: f64,
    pub sslp_fwd: f64,
    pub sslp_rev: f64,
    pub dslp_fwd: f64,
    pub dslp_rev: f64,
    pub ivjsm_fwd: f64,
    pub ivjsm_rev: f64,
    pub ivjdm_fwd: f64,
    pub ivjdm_rev: f64,
    pub grgeltd: f64,
    pub pseff: f64,
    pub pdeff: f64,
    pub aseff: f64,
    pub adeff: f64,
    pub l: f64,
    pub w: f64,
    pub m: f64,
    pub drain_area: f64,
    pub source_area: f64,
    pub drain_squares: f64,
    pub source_squares: f64,
    pub drain_perimeter: f64,
    pub source_perimeter: f64,
    pub source_conductance: f64,
    pub drain_conductance: f64,
    pub sa: f64,
    pub sb: f64,
    pub sd: f64,
    pub sca: f64,
    pub scb: f64,
    pub scc: f64,
    pub sc: f64,
    pub rbdb: f64,
    pub rbsb: f64,
    pub rbpb: f64,
    pub rbps: f64,
    pub rbpd: f64,
    pub delvto: f64,
    pub mulu0: f64,
    pub wnflag: i32,
    pub xgw: f64,
    pub ngcon: f64,
    pub dtemp: f64,
    pub u0temp: f64,
    pub vsattemp: f64,
    pub vth0: f64,
    pub vfb: f64,
    pub vfbzb: f64,
    pub vtfbphi1: f64,
    pub vtfbphi2: f64,
    pub k2: f64,
    pub vbsc: f64,
    pub k2ox: f64,
    pub eta0: f64,
    pub toxp: f64,
    pub coxp: f64,
    pub ic_vds: f64,
    pub ic_vgs: f64,
    pub ic_vbs: f64,
    pub mult_i: f64,
    pub mult_q: f64,
    pub mult_fn: f64,
    pub nf: f64,
    pub off: i32,
    pub mode: i32,
    pub trnqs_mod: i32,
    pub acnqs_mod: i32,
    pub rbody_mod: i32,
    pub rgate_mod: i32,
    pub geo_mod: i32,
    pub rgeo_mod: i32,
    pub min: i32,
    pub vgsteff: f64,
    pub vgs_eff: f64,
    pub vgd_eff: f64,
    pub dvgs_eff_dvg: f64,
    pub dvgd_eff_dvg: f64,
    pub vdseff: f64,
    pub nstar: f64,
    pub abulk: f64,
    pub esat_l: f64,
    pub abov_vgst2_vtm: f64,
    pub qinv: f64,
    pub cd: f64,
    pub cbs: f64,
    pub cbd: f64,
    pub csub: f64,
    pub igidl: f64,
    pub igisl: f64,
    pub gm: f64,
    pub gds: f64,
    pub gmbs: f64,
    pub gbd: f64,
    pub gbs: f64,
    pub noi_gd0: f64,
    pub coxeff: f64,
    pub gbbs: f64,
    pub gbgs: f64,
    pub gbds: f64,
    pub ggidld: f64,
    pub ggidlg: f64,
    pub ggidls: f64,
    pub ggidlb: f64,
    pub ggisld: f64,
    pub ggislg: f64,
    pub ggisls: f64,
    pub ggislb: f64,
    pub igcs: f64,
    pub g_igcsg: f64,
    pub g_igcsd: f64,
    pub g_igcss: f64,
    pub g_igcsb: f64,
    pub igcd: f64,
    pub g_igcdg: f64,
    pub g_igcdd: f64,
    pub g_igcds: f64,
    pub g_igcdb: f64,
    pub igs: f64,
    pub g_igsg: f64,
    pub g_igss: f64,
    pub igd: f64,
    pub g_igdg: f64,
    pub g_igdd: f64,
    pub igb: f64,
    pub g_igbg: f64,
    pub g_igbd: f64,
    pub g_igbs: f64,
    pub g_igbb: f64,
    pub grdsw: f64,
    pub idov_vds: f64,
    pub gcrg: f64,
    pub gcrgd: f64,
    pub gcrgg: f64,
    pub gcrgs: f64,
    pub gcrgb: f64,
    pub gstot: f64,
    pub gstotd: f64,
    pub gstotg: f64,
    pub gstots: f64,
    pub gstotb: f64,
    pub gdtot: f64,
    pub gdtotd: f64,
    pub gdtotg: f64,
    pub gdtots: f64,
    pub gdtotb: f64,
    pub cggb: f64,
    pub cgdb: f64,
    pub cgsb: f64,
    pub cbgb: f64,
    pub cbdb: f64,
    pub cbsb: f64,
    pub cdgb: f64,
    pub cddb: f64,
    pub cdsb: f64,
    pub csgb: f64,
    pub csdb: f64,
    pub cssb: f64,
    pub cgbb: f64,
    pub cdbb: f64,
    pub csbb: f64,
    pub cbbb: f64,
    pub capbd: f64,
    pub capbs: f64,
    pub cqgb: f64,
    pub cqdb: f64,
    pub cqsb: f64,
    pub cqbb: f64,
    pub qgate: f64,
    pub qbulk: f64,
    pub qdrn: f64,
    pub qsrc: f64,
    pub qdef: f64,
    pub qchqs: f64,
    pub taunet: f64,
    pub gtau: f64,
    pub gtg: f64,
    pub gtd: f64,
    pub gts: f64,
    pub gtb: f64,
    pub sjct_temp_rev_sat_cur: f64,
    pub djct_temp_rev_sat_cur: f64,
    pub ssw_temp_rev_sat_cur: f64,
    pub dsw_temp_rev_sat_cur: f64,
    pub sswg_temp_rev_sat_cur: f64,
    pub dswg_temp_rev_sat_cur: f64,
    pub dpd_ptr: Slot,
    pub dpdp_ptr: Slot,
    pub dpgp_ptr: Slot,
    pub dpgm_ptr: Slot,
    pub dpsp_ptr: Slot,
    pub dpbp_ptr: Slot,
    pub dpdb_ptr: Slot,
    pub dd_ptr: Slot,
    pub ddp_ptr: Slot,
    pub gpdp_ptr: Slot,
    pub gpgp_ptr: Slot,
    pub gpgm_ptr: Slot,
    pub gpge_ptr: Slot,
    pub gpsp_ptr: Slot,
    pub gpbp_ptr: Slot,
    pub gmdp_ptr: Slot,
    pub gmgp_ptr: Slot,
    pub gmgm_ptr: Slot,
    pub gmge_ptr: Slot,
    pub gmsp_ptr: Slot,
    pub gmbp_ptr: Slot,
    pub gedp_ptr: Slot,
    pub gegp_ptr: Slot,
    pub gegm_ptr: Slot,
    pub gege_ptr: Slot,
    pub gesp_ptr: Slot,
    pub gebp_ptr: Slot,
    pub spdp_ptr: Slot,
    pub spgp_ptr: Slot,
    pub spgm_ptr: Slot,
    pub sps_ptr: Slot,
    pub spsp_ptr: Slot,
    pub spbp_ptr: Slot,
    pub spsb_ptr: Slot,
    pub ssp_ptr: Slot,
    pub ss_ptr: Slot,
    pub bpdp_ptr: Slot,
    pub bpgp_ptr: Slot,
    pub bpgm_ptr: Slot,
    pub bpsp_ptr: Slot,
    pub bpdb_ptr: Slot,
    pub bpb_ptr: Slot,
    pub bpsb_ptr: Slot,
    pub bpbp_ptr: Slot,
    pub dbdp_ptr: Slot,
    pub dbdb_ptr: Slot,
    pub dbbp_ptr: Slot,
    pub dbb_ptr: Slot,
    pub sbsp_ptr: Slot,
    pub sbbp_ptr: Slot,
    pub sbb_ptr: Slot,
    pub sbsb_ptr: Slot,
    pub bdb_ptr: Slot,
    pub bbp_ptr: Slot,
    pub bsb_ptr: Slot,
    pub bb_ptr: Slot,
    pub dgp_ptr: Slot,
    pub dsp_ptr: Slot,
    pub dbp_ptr: Slot,
    pub sdp_ptr: Slot,
    pub sgp_ptr: Slot,
    pub sbp_ptr: Slot,
    pub qdp_ptr: Slot,
    pub qgp_ptr: Slot,
    pub qsp_ptr: Slot,
    pub qbp_ptr: Slot,
    pub qq_ptr: Slot,
    pub dpq_ptr: Slot,
    pub gpq_ptr: Slot,
    pub spq_ptr: Slot,
    pub given: Bsim4InstGiven,
}

/// The C's `BSIM4xxxGiven` instance flags.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)]
pub(crate) struct Bsim4InstGiven {
    pub l: bool,
    pub w: bool,
    pub m: bool,
    pub mult_i: bool,
    pub mult_q: bool,
    pub mult_fn: bool,
    pub nf: bool,
    pub min: bool,
    pub drain_area: bool,
    pub source_area: bool,
    pub drain_squares: bool,
    pub source_squares: bool,
    pub drain_perimeter: bool,
    pub source_perimeter: bool,
    pub sa: bool,
    pub sb: bool,
    pub sd: bool,
    pub sca: bool,
    pub scb: bool,
    pub scc: bool,
    pub sc: bool,
    pub rbdb: bool,
    pub rbsb: bool,
    pub rbpb: bool,
    pub rbpd: bool,
    pub rbps: bool,
    pub delvto: bool,
    pub mulu0: bool,
    pub wnflag: bool,
    pub xgw: bool,
    pub ngcon: bool,
    pub dtemp: bool,
    pub ic_vds: bool,
    pub ic_vgs: bool,
    pub ic_vbs: bool,
    pub trnqs_mod: bool,
    pub acnqs_mod: bool,
    pub rbody_mod: bool,
    pub rgate_mod: bool,
    pub geo_mod: bool,
    pub rgeo_mod: bool,
}

impl Bsim4Model {
    /// `BSIM4mParam` (`b4mpar.c`) for every numeric keyword of
    /// `BSIM4mPTable` (`b4.c`). Integer parameters are rounded like
    /// ngspice's `INPgetValue` (`floor(0.5 + v)`). Returns `false` for an
    /// unknown keyword.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn set_param(&mut self, kw: &str, v: f64) -> bool {
        let iv = (0.5 + v).floor() as i32;
        let _ = iv;
        match kw {
            // Capacitance Charge model selector
            "cvchargemod" => {
                self.cvcharge_mod = iv;
                self.given.cvcharge_mod = true;
            }
            // Capacitance model selector
            "capmod" => {
                self.cap_mod = iv;
                self.given.cap_mod = true;
            }
            // Diode IV model selector
            "diomod" => {
                self.dio_mod = iv;
                self.given.dio_mod = true;
            }
            // Bias-dependent S/D resistance model selector
            "rdsmod" => {
                self.rds_mod = iv;
                self.given.rds_mod = true;
            }
            // Transient NQS model selector
            "trnqsmod" => {
                self.trnqs_mod = iv;
                self.given.trnqs_mod = true;
            }
            // AC NQS model selector
            "acnqsmod" => {
                self.acnqs_mod = iv;
                self.given.acnqs_mod = true;
            }
            // Mobility model selector
            "mobmod" => {
                self.mob_mod = iv;
                self.given.mob_mod = true;
            }
            // Distributed body R model selector
            "rbodymod" => {
                self.rbody_mod = iv;
                self.given.rbody_mod = true;
            }
            // Gate R model selector
            "rgatemod" => {
                self.rgate_mod = iv;
                self.given.rgate_mod = true;
            }
            // Pd and Ps model selector
            "permod" => {
                self.per_mod = iv;
                self.given.per_mod = true;
            }
            // Geometry dependent parasitics model selector
            "geomod" => {
                self.geo_mod = iv;
                self.given.geo_mod = true;
            }
            // Flicker noise model selector
            "fnoimod" => {
                self.fnoi_mod = iv;
                self.given.fnoi_mod = true;
            }
            // Thermal noise model selector
            "tnoimod" => {
                self.tnoi_mod = iv;
                self.given.tnoi_mod = true;
            }
            // parameter for non-silicon substrate or metal gate selector
            "mtrlmod" => {
                self.mtrl_mod = iv;
                self.given.mtrl_mod = true;
            }
            // New Material Mod backward compatibility selector
            "mtrlcompatmod" => {
                self.mtrl_compat_mod = iv;
                self.given.mtrl_compat_mod = true;
            }
            // Gate-to-channel Ig model selector
            "igcmod" => {
                self.igc_mod = iv;
                self.given.igc_mod = true;
            }
            // Gate-to-body Ig model selector
            "igbmod" => {
                self.igb_mod = iv;
                self.given.igb_mod = true;
            }
            // Temperature model selector
            "tempmod" => {
                self.temp_mod = iv;
                self.given.temp_mod = true;
            }
            // parameter for GIDL selector
            "gidlmod" => {
                self.gidl_mod = iv;
                self.given.gidl_mod = true;
            }
            // Model parameter checking selector
            "paramchk" => {
                self.param_chk = iv;
                self.given.param_chk = true;
            }
            // Bin  unit  selector
            "binunit" => {
                self.bin_unit = iv;
                self.given.bin_unit = true;
            }
            // Equivalent gate oxide thickness in meters
            "eot" => {
                self.eot = v;
                self.given.eot = true;
            }
            // Voltage for extraction of Equivalent gate oxide thickness
            "vddeot" => {
                self.vddeot = v;
                self.given.vddeot = true;
            }
            // Temperature for extraction of EOT
            "tempeot" => {
                self.tempeot = v;
                self.given.tempeot = true;
            }
            // Effective length for extraction of EOT
            "leffeot" => {
                self.leffeot = v;
                self.given.leffeot = true;
            }
            // Effective width for extraction of EOT
            "weffeot" => {
                self.weffeot = v;
                self.given.weffeot = true;
            }
            // Charge centroid parameter
            "ados" => {
                self.ados = v;
                self.given.ados = true;
            }
            // Charge centroid parameter
            "bdos" => {
                self.bdos = v;
                self.given.bdos = true;
            }
            // Electrical gate oxide thickness in meters
            "toxe" => {
                self.toxe = v;
                self.given.toxe = true;
            }
            // Physical gate oxide thickness in meters
            "toxp" => {
                self.toxp = v;
                self.given.toxp = true;
            }
            // Gate oxide thickness at which parameters are extracted
            "toxm" => {
                self.toxm = v;
                self.given.toxm = true;
            }
            // Target tox value
            "toxref" => {
                self.toxref = v;
                self.given.toxref = true;
            }
            // Defined as (toxe - toxp)
            "dtox" => {
                self.dtox = v;
                self.given.dtox = true;
            }
            // Dielectric constant of the gate oxide relative to vacuum
            "epsrox" => {
                self.epsrox = v;
                self.given.epsrox = true;
            }
            // Drain/Source and channel coupling capacitance
            "cdsc" => {
                self.cdsc = v;
                self.given.cdsc = true;
            }
            // Body-bias dependence of cdsc
            "cdscb" => {
                self.cdscb = v;
                self.given.cdscb = true;
            }
            // Drain-bias dependence of cdsc
            "cdscd" => {
                self.cdscd = v;
                self.given.cdscd = true;
            }
            // Interface state capacitance
            "cit" => {
                self.cit = v;
                self.given.cit = true;
            }
            // Subthreshold swing Coefficient
            "nfactor" => {
                self.nfactor = v;
                self.given.nfactor = true;
            }
            // Junction depth in meters
            "xj" => {
                self.xj = v;
                self.given.xj = true;
            }
            // Saturation velocity at tnom
            "vsat" => {
                self.vsat = v;
                self.given.vsat = true;
            }
            // Temperature coefficient of vsat
            "at" => {
                self.at = v;
                self.given.at = true;
            }
            // Non-uniform depletion width effect coefficient.
            "a0" => {
                self.a0 = v;
                self.given.a0 = true;
            }
            // Gate bias  coefficient of Abulk.
            "ags" => {
                self.ags = v;
                self.given.ags = true;
            }
            // Non-saturation effect coefficient
            "a1" => {
                self.a1 = v;
                self.given.a1 = true;
            }
            // Non-saturation effect coefficient
            "a2" => {
                self.a2 = v;
                self.given.a2 = true;
            }
            // Body-bias coefficient of non-uniform depletion width effect.
            "keta" => {
                self.keta = v;
                self.given.keta = true;
            }
            // Body-bias coefficient of non-uniform depletion width effect in dynamic evaluation.
            "ketac" => {
                self.ketac = v;
                self.given.ketac = true;
            }
            // Work function of gate
            "phig" => {
                self.phig = v;
                self.given.phig = true;
            }
            // Dielectric constant of gate relative to vacuum
            "epsrgate" => {
                self.epsrgate = v;
                self.given.epsrgate = true;
            }
            // Electron affinity of substrate
            "easub" => {
                self.easub = v;
                self.given.easub = true;
            }
            // Dielectric constant of substrate relative to vacuum
            "epsrsub" => {
                self.epsrsub = v;
                self.given.epsrsub = true;
            }
            // Intrinsic carrier concentration of substrate at 300.15K
            "ni0sub" => {
                self.ni0sub = v;
                self.given.ni0sub = true;
            }
            // Band-gap of substrate at T=0K
            "bg0sub" => {
                self.bg0sub = v;
                self.given.bg0sub = true;
            }
            // First parameter of band-gap change due to temperature
            "tbgasub" => {
                self.tbgasub = v;
                self.given.tbgasub = true;
            }
            // Second parameter of band-gap change due to temperature
            "tbgbsub" => {
                self.tbgbsub = v;
                self.given.tbgbsub = true;
            }
            // Substrate doping concentration
            "nsub" => {
                self.nsub = v;
                self.given.nsub = true;
            }
            // Channel doping concentration at the depletion edge
            "ndep" => {
                self.ndep = v;
                self.given.ndep = true;
                if self.ndep > 1.0e20 {
                    self.ndep *= 1.0e-6;
                }
            }
            // S/D doping concentration
            "nsd" => {
                self.nsd = v;
                self.given.nsd = true;
                if self.nsd > 1.0e23 {
                    self.nsd *= 1.0e-6;
                }
            }
            // Adjusting parameter for surface potential due to non-uniform vertical doping
            "phin" => {
                self.phin = v;
                self.given.phin = true;
            }
            // Poly-gate doping concentration
            "ngate" => {
                self.ngate = v;
                self.given.ngate = true;
                if self.ngate > 1.0e23 {
                    self.ngate *= 1.0e-6;
                }
            }
            // Vth body coefficient
            "gamma1" => {
                self.gamma1 = v;
                self.given.gamma1 = true;
            }
            // Vth body coefficient
            "gamma2" => {
                self.gamma2 = v;
                self.given.gamma2 = true;
            }
            // Vth transition body Voltage
            "vbx" => {
                self.vbx = v;
                self.given.vbx = true;
            }
            // Maximum body voltage
            "vbm" => {
                self.vbm = v;
                self.given.vbm = true;
            }
            // Doping depth
            "xt" => {
                self.xt = v;
                self.given.xt = true;
            }
            // Bulk effect coefficient 1
            "k1" => {
                self.k1 = v;
                self.given.k1 = true;
            }
            // Temperature coefficient of Vth
            "kt1" => {
                self.kt1 = v;
                self.given.kt1 = true;
            }
            // Temperature coefficient of Vth
            "kt1l" => {
                self.kt1l = v;
                self.given.kt1l = true;
            }
            // Body-coefficient of kt1
            "kt2" => {
                self.kt2 = v;
                self.given.kt2 = true;
            }
            // Bulk effect coefficient 2
            "k2" => {
                self.k2 = v;
                self.given.k2 = true;
            }
            // Narrow width effect coefficient
            "k3" => {
                self.k3 = v;
                self.given.k3 = true;
            }
            // Body effect coefficient of k3
            "k3b" => {
                self.k3b = v;
                self.given.k3b = true;
            }
            // Narrow width effect parameter
            "w0" => {
                self.w0 = v;
                self.given.w0 = true;
            }
            // First parameter for Vth shift due to pocket
            "dvtp0" => {
                self.dvtp0 = v;
                self.given.dvtp0 = true;
            }
            // Second parameter for Vth shift due to pocket
            "dvtp1" => {
                self.dvtp1 = v;
                self.given.dvtp1 = true;
            }
            // 3rd parameter for Vth shift due to pocket
            "dvtp2" => {
                self.dvtp2 = v;
                self.given.dvtp2 = true;
            }
            // 4th parameter for Vth shift due to pocket
            "dvtp3" => {
                self.dvtp3 = v;
                self.given.dvtp3 = true;
            }
            // 5th parameter for Vth shift due to pocket
            "dvtp4" => {
                self.dvtp4 = v;
                self.given.dvtp4 = true;
            }
            // 6th parameter for Vth shift due to pocket
            "dvtp5" => {
                self.dvtp5 = v;
                self.given.dvtp5 = true;
            }
            // Equivalent length of pocket region at zero bias
            "lpe0" => {
                self.lpe0 = v;
                self.given.lpe0 = true;
            }
            // Equivalent length of pocket region accounting for body bias
            "lpeb" => {
                self.lpeb = v;
                self.given.lpeb = true;
            }
            // Short channel effect coeff. 0
            "dvt0" => {
                self.dvt0 = v;
                self.given.dvt0 = true;
            }
            // Short channel effect coeff. 1
            "dvt1" => {
                self.dvt1 = v;
                self.given.dvt1 = true;
            }
            // Short channel effect coeff. 2
            "dvt2" => {
                self.dvt2 = v;
                self.given.dvt2 = true;
            }
            // Narrow Width coeff. 0
            "dvt0w" => {
                self.dvt0w = v;
                self.given.dvt0w = true;
            }
            // Narrow Width effect coeff. 1
            "dvt1w" => {
                self.dvt1w = v;
                self.given.dvt1w = true;
            }
            // Narrow Width effect coeff. 2
            "dvt2w" => {
                self.dvt2w = v;
                self.given.dvt2w = true;
            }
            // DIBL coefficient of output resistance
            "drout" => {
                self.drout = v;
                self.given.drout = true;
            }
            // DIBL coefficient in the subthreshold region
            "dsub" => {
                self.dsub = v;
                self.given.dsub = true;
            }
            // Threshold voltage
            "vth0" | "vtho" => {
                self.vth0 = v;
                self.given.vth0 = true;
            }
            // Linear gate dependence of mobility
            "ua" => {
                self.ua = v;
                self.given.ua = true;
            }
            // Temperature coefficient of ua
            "ua1" => {
                self.ua1 = v;
                self.given.ua1 = true;
            }
            // Quadratic gate dependence of mobility
            "ub" => {
                self.ub = v;
                self.given.ub = true;
            }
            // Temperature coefficient of ub
            "ub1" => {
                self.ub1 = v;
                self.given.ub1 = true;
            }
            // Body-bias dependence of mobility
            "uc" => {
                self.uc = v;
                self.given.uc = true;
            }
            // Temperature coefficient of uc
            "uc1" => {
                self.uc1 = v;
                self.given.uc1 = true;
            }
            // Coulomb scattering factor of mobility
            "ud" => {
                self.ud = v;
                self.given.ud = true;
            }
            // Temperature coefficient of ud
            "ud1" => {
                self.ud1 = v;
                self.given.ud1 = true;
            }
            // Channel length linear factor of mobility
            "up" => {
                self.up = v;
                self.given.up = true;
            }
            // Channel length exponential factor of mobility
            "lp" => {
                self.lp = v;
                self.given.lp = true;
            }
            // Low-field mobility at Tnom
            "u0" => {
                self.u0 = v;
                self.given.u0 = true;
            }
            // Mobility exponent
            "eu" => {
                self.eu = v;
                self.given.eu = true;
            }
            // Colombic scattering exponent
            "ucs" => {
                self.ucs = v;
                self.given.ucs = true;
            }
            // Temperature coefficient of mobility
            "ute" => {
                self.ute = v;
                self.given.ute = true;
            }
            // Temperature coefficient of colombic mobility
            "ucste" => {
                self.ucste = v;
                self.given.ucste = true;
            }
            // Threshold voltage offset
            "voff" => {
                self.voff = v;
                self.given.voff = true;
            }
            // Fitting parameter for moderate inversion in Vgsteff
            "minv" => {
                self.minv = v;
                self.given.minv = true;
            }
            // Fitting parameter for moderate inversion in Vgsteffcv
            "minvcv" => {
                self.minvcv = v;
                self.given.minvcv = true;
            }
            // Length dependence parameter for Vth offset
            "voffl" => {
                self.voffl = v;
                self.given.voffl = true;
            }
            // Length dependence parameter for Vth offset in CV
            "voffcvl" => {
                self.voffcvl = v;
                self.given.voffcvl = true;
            }
            // Parameter measurement temperature
            "tnom" => {
                self.tnom = v + CONST_CTOK;
                self.given.tnom = true;
            }
            // Gate-source overlap capacitance per width
            "cgso" => {
                self.cgso = v;
                self.given.cgso = true;
            }
            // Gate-drain overlap capacitance per width
            "cgdo" => {
                self.cgdo = v;
                self.given.cgdo = true;
            }
            // Gate-bulk overlap capacitance per length
            "cgbo" => {
                self.cgbo = v;
                self.given.cgbo = true;
            }
            // Channel charge partitioning
            "xpart" => {
                self.xpart = v;
                self.given.xpart = true;
            }
            // Effective Vds parameter
            "delta" => {
                self.delta = v;
                self.given.delta = true;
            }
            // Source-drain sheet resistance
            "rsh" => {
                self.sheet_resistance = v;
                self.given.sheet_resistance = true;
            }
            // Source-drain resistance per width
            "rdsw" => {
                self.rdsw = v;
                self.given.rdsw = true;
            }
            // Source-drain resistance per width at high Vg
            "rdswmin" => {
                self.rdswmin = v;
                self.given.rdswmin = true;
            }
            // Source resistance per width
            "rsw" => {
                self.rsw = v;
                self.given.rsw = true;
            }
            // Drain resistance per width
            "rdw" => {
                self.rdw = v;
                self.given.rdw = true;
            }
            // Drain resistance per width at high Vg
            "rdwmin" => {
                self.rdwmin = v;
                self.given.rdwmin = true;
            }
            // Source resistance per width at high Vg
            "rswmin" => {
                self.rswmin = v;
                self.given.rswmin = true;
            }
            // Gate-bias effect on parasitic resistance
            "prwg" => {
                self.prwg = v;
                self.given.prwg = true;
            }
            // Body-effect on parasitic resistance
            "prwb" => {
                self.prwb = v;
                self.given.prwb = true;
            }
            // Temperature coefficient of parasitic resistance
            "prt" => {
                self.prt = v;
                self.given.prt = true;
            }
            // Subthreshold region DIBL coefficient
            "eta0" => {
                self.eta0 = v;
                self.given.eta0 = true;
            }
            // Subthreshold region DIBL coefficient
            "etab" => {
                self.etab = v;
                self.given.etab = true;
            }
            // Channel length modulation Coefficient
            "pclm" => {
                self.pclm = v;
                self.given.pclm = true;
            }
            // Drain-induced barrier lowering coefficient
            "pdiblc1" => {
                self.pdibl1 = v;
                self.given.pdibl1 = true;
            }
            // Drain-induced barrier lowering coefficient
            "pdiblc2" => {
                self.pdibl2 = v;
                self.given.pdibl2 = true;
            }
            // Body-effect on drain-induced barrier lowering
            "pdiblcb" => {
                self.pdiblb = v;
                self.given.pdiblb = true;
            }
            // Rout degradation coefficient for pocket devices
            "fprout" => {
                self.fprout = v;
                self.given.fprout = true;
            }
            // Coefficient for drain-induced Vth shifts
            "pdits" => {
                self.pdits = v;
                self.given.pdits = true;
            }
            // Length dependence of drain-induced Vth shifts
            "pditsl" => {
                self.pditsl = v;
                self.given.pditsl = true;
            }
            // Vds dependence of drain-induced Vth shifts
            "pditsd" => {
                self.pditsd = v;
                self.given.pditsd = true;
            }
            // Substrate current body-effect coefficient
            "pscbe1" => {
                self.pscbe1 = v;
                self.given.pscbe1 = true;
            }
            // Substrate current body-effect coefficient
            "pscbe2" => {
                self.pscbe2 = v;
                self.given.pscbe2 = true;
            }
            // Gate dependence of output resistance parameter
            "pvag" => {
                self.pvag = v;
                self.given.pvag = true;
            }
            // Bottom source junction reverse saturation current density
            "jss" => {
                self.sjct_sat_cur_density = v;
                self.given.sjct_sat_cur_density = true;
            }
            // Isolation edge sidewall source junction reverse saturation current density
            "jsws" => {
                self.sjct_sidewall_sat_cur_density = v;
                self.given.sjct_sidewall_sat_cur_density = true;
            }
            // Gate edge source junction reverse saturation current density
            "jswgs" => {
                self.sjct_gate_sidewall_sat_cur_density = v;
                self.given.sjct_gate_sidewall_sat_cur_density = true;
            }
            // Source junction built-in potential
            "pbs" => {
                self.sbulk_jct_potential = v;
                self.given.sbulk_jct_potential = true;
            }
            // Source junction emission coefficient
            "njs" => {
                self.sjct_emission_coeff = v;
                self.given.sjct_emission_coeff = true;
            }
            // Source junction current temperature exponent
            "xtis" => {
                self.sjct_temp_exponent = v;
                self.given.sjct_temp_exponent = true;
            }
            // Source bottom junction capacitance grading coefficient
            "mjs" => {
                self.sbulk_jct_bot_grading_coeff = v;
                self.given.sbulk_jct_bot_grading_coeff = true;
            }
            // Source sidewall junction capacitance built in potential
            "pbsws" => {
                self.ssidewall_jct_potential = v;
                self.given.ssidewall_jct_potential = true;
            }
            // Source sidewall junction capacitance grading coefficient
            "mjsws" => {
                self.sbulk_jct_side_grading_coeff = v;
                self.given.sbulk_jct_side_grading_coeff = true;
            }
            // Source (gate side) sidewall junction capacitance built in potential
            "pbswgs" => {
                self.sgatesidewall_jct_potential = v;
                self.given.sgatesidewall_jct_potential = true;
            }
            // Source (gate side) sidewall junction capacitance grading coefficient
            "mjswgs" => {
                self.sbulk_jct_gate_side_grading_coeff = v;
                self.given.sbulk_jct_gate_side_grading_coeff = true;
            }
            // Source bottom junction capacitance per unit area
            "cjs" => {
                self.sunit_area_jct_cap = v;
                self.given.sunit_area_jct_cap = true;
            }
            // Source sidewall junction capacitance per unit periphery
            "cjsws" => {
                self.sunit_length_sidewall_jct_cap = v;
                self.given.sunit_length_sidewall_jct_cap = true;
            }
            // Source (gate side) sidewall junction capacitance per unit width
            "cjswgs" => {
                self.sunit_length_gate_sidewall_jct_cap = v;
                self.given.sunit_length_gate_sidewall_jct_cap = true;
            }
            // Bottom drain junction reverse saturation current density
            "jsd" => {
                self.djct_sat_cur_density = v;
                self.given.djct_sat_cur_density = true;
            }
            // Isolation edge sidewall drain junction reverse saturation current density
            "jswd" => {
                self.djct_sidewall_sat_cur_density = v;
                self.given.djct_sidewall_sat_cur_density = true;
            }
            // Gate edge drain junction reverse saturation current density
            "jswgd" => {
                self.djct_gate_sidewall_sat_cur_density = v;
                self.given.djct_gate_sidewall_sat_cur_density = true;
            }
            // Drain junction built-in potential
            "pbd" => {
                self.dbulk_jct_potential = v;
                self.given.dbulk_jct_potential = true;
            }
            // Drain junction emission coefficient
            "njd" => {
                self.djct_emission_coeff = v;
                self.given.djct_emission_coeff = true;
            }
            // Drainjunction current temperature exponent
            "xtid" => {
                self.djct_temp_exponent = v;
                self.given.djct_temp_exponent = true;
            }
            // Drain bottom junction capacitance grading coefficient
            "mjd" => {
                self.dbulk_jct_bot_grading_coeff = v;
                self.given.dbulk_jct_bot_grading_coeff = true;
            }
            // Drain sidewall junction capacitance built in potential
            "pbswd" => {
                self.dsidewall_jct_potential = v;
                self.given.dsidewall_jct_potential = true;
            }
            // Drain sidewall junction capacitance grading coefficient
            "mjswd" => {
                self.dbulk_jct_side_grading_coeff = v;
                self.given.dbulk_jct_side_grading_coeff = true;
            }
            // Drain (gate side) sidewall junction capacitance built in potential
            "pbswgd" => {
                self.dgatesidewall_jct_potential = v;
                self.given.dgatesidewall_jct_potential = true;
            }
            // Drain (gate side) sidewall junction capacitance grading coefficient
            "mjswgd" => {
                self.dbulk_jct_gate_side_grading_coeff = v;
                self.given.dbulk_jct_gate_side_grading_coeff = true;
            }
            // Drain bottom junction capacitance per unit area
            "cjd" => {
                self.dunit_area_jct_cap = v;
                self.given.dunit_area_jct_cap = true;
            }
            // Drain sidewall junction capacitance per unit periphery
            "cjswd" => {
                self.dunit_length_sidewall_jct_cap = v;
                self.given.dunit_length_sidewall_jct_cap = true;
            }
            // Drain (gate side) sidewall junction capacitance per unit width
            "cjswgd" => {
                self.dunit_length_gate_sidewall_jct_cap = v;
                self.given.dunit_length_gate_sidewall_jct_cap = true;
            }
            // Flat Band Voltage parameter for capmod=0 only
            "vfbcv" => {
                self.vfbcv = v;
                self.given.vfbcv = true;
            }
            // Flat Band Voltage
            "vfb" => {
                self.vfb = v;
                self.given.vfb = true;
            }
            // Temperature coefficient of pb
            "tpb" => {
                self.tpb = v;
                self.given.tpb = true;
            }
            // Temperature coefficient of cj
            "tcj" => {
                self.tcj = v;
                self.given.tcj = true;
            }
            // Temperature coefficient of pbsw
            "tpbsw" => {
                self.tpbsw = v;
                self.given.tpbsw = true;
            }
            // Temperature coefficient of cjsw
            "tcjsw" => {
                self.tcjsw = v;
                self.given.tcjsw = true;
            }
            // Temperature coefficient of pbswg
            "tpbswg" => {
                self.tpbswg = v;
                self.given.tpbswg = true;
            }
            // Temperature coefficient of cjswg
            "tcjswg" => {
                self.tcjswg = v;
                self.given.tcjswg = true;
            }
            // Exponential coefficient for finite charge thickness
            "acde" => {
                self.acde = v;
                self.given.acde = true;
            }
            // Coefficient for gate-bias dependent surface potential
            "moin" => {
                self.moin = v;
                self.given.moin = true;
            }
            // C-V turn-on/off parameter
            "noff" => {
                self.noff = v;
                self.given.noff = true;
            }
            // C-V lateral-shift parameter
            "voffcv" => {
                self.voffcv = v;
                self.given.voffcv = true;
            }
            // Distance of Mid-Contact to Gate edge
            "dmcg" => {
                self.dmcg = v;
                self.given.dmcg = true;
            }
            // Distance of Mid-Contact to Isolation
            "dmci" => {
                self.dmci = v;
                self.given.dmci = true;
            }
            // Distance of Mid-Diffusion to Gate edge
            "dmdg" => {
                self.dmdg = v;
                self.given.dmdg = true;
            }
            // Distance of Mid-Contact to Gate edge in Test structures
            "dmcgt" => {
                self.dmcgt = v;
                self.given.dmcgt = true;
            }
            // Distance from gate contact center to device edge
            "xgw" => {
                self.xgw = v;
                self.given.xgw = true;
            }
            // Variation in Ldrawn
            "xgl" => {
                self.xgl = v;
                self.given.xgl = true;
            }
            // Gate sheet resistance
            "rshg" => {
                self.rshg = v;
                self.given.rshg = true;
            }
            // Number of gate contacts
            "ngcon" => {
                self.ngcon = v;
                self.given.ngcon = true;
            }
            // First fitting parameter the bias-dependent Rg
            "xrcrg1" => {
                self.xrcrg1 = v;
                self.given.xrcrg1 = true;
            }
            // Second fitting parameter the bias-dependent Rg
            "xrcrg2" => {
                self.xrcrg2 = v;
                self.given.xrcrg2 = true;
            }
            // Velocity overshoot parameter
            "lambda" => {
                self.lambda = v;
                self.given.lambda = true;
            }
            // thermal velocity
            "vtl" => {
                self.vtl = v;
                self.given.vtl = true;
            }
            // back scattering parameter
            "lc" => {
                self.lc = v;
                self.given.lc = true;
            }
            // back scattering parameter
            "xn" => {
                self.xn = v;
                self.given.xn = true;
            }
            // S/D flatband voltage offset
            "vfbsdoff" => {
                self.vfbsdoff = v;
                self.given.vfbsdoff = true;
            }
            // Temperature parameter for vfbsdoff
            "tvfbsdoff" => {
                self.tvfbsdoff = v;
                self.given.tvfbsdoff = true;
            }
            // Temperature parameter for voff
            "tvoff" => {
                self.tvoff = v;
                self.given.tvoff = true;
            }
            // Temperature parameter for nfactor
            "tnfactor" => {
                self.tnfactor = v;
                self.given.tnfactor = true;
            }
            // Temperature parameter for eta0
            "teta0" => {
                self.teta0 = v;
                self.given.teta0 = true;
            }
            // Temperature parameter for tvoffcv
            "tvoffcv" => {
                self.tvoffcv = v;
                self.given.tvoffcv = true;
            }
            // lint offset for noise calculation
            "lintnoi" => {
                self.lintnoi = v;
                self.given.lintnoi = true;
            }
            // Length reduction parameter
            "lint" => {
                self.lint = v;
                self.given.lint = true;
            }
            // Length reduction parameter
            "ll" => {
                self.ll = v;
                self.given.ll = true;
            }
            // Length reduction parameter for CV
            "llc" => {
                self.llc = v;
                self.given.llc = true;
            }
            // Length reduction parameter
            "lln" => {
                self.lln = v;
                self.given.lln = true;
            }
            // Length reduction parameter
            "lw" => {
                self.lw = v;
                self.given.lw = true;
            }
            // Length reduction parameter for CV
            "lwc" => {
                self.lwc = v;
                self.given.lwc = true;
            }
            // Length reduction parameter
            "lwn" => {
                self.lwn = v;
                self.given.lwn = true;
            }
            // Length reduction parameter
            "lwl" => {
                self.lwl = v;
                self.given.lwl = true;
            }
            // Length reduction parameter for CV
            "lwlc" => {
                self.lwlc = v;
                self.given.lwlc = true;
            }
            // Minimum length for the model
            "lmin" => {
                self.lmin = v;
                self.given.lmin = true;
            }
            // Maximum length for the model
            "lmax" => {
                self.lmax = v;
                self.given.lmax = true;
            }
            // Width dependence of rds
            "wr" => {
                self.wr = v;
                self.given.wr = true;
            }
            // Width reduction parameter
            "wint" => {
                self.wint = v;
                self.given.wint = true;
            }
            // Width reduction parameter
            "dwg" => {
                self.dwg = v;
                self.given.dwg = true;
            }
            // Width reduction parameter
            "dwb" => {
                self.dwb = v;
                self.given.dwb = true;
            }
            // Width reduction parameter
            "wl" => {
                self.wl = v;
                self.given.wl = true;
            }
            // Width reduction parameter for CV
            "wlc" => {
                self.wlc = v;
                self.given.wlc = true;
            }
            // Width reduction parameter
            "wln" => {
                self.wln = v;
                self.given.wln = true;
            }
            // Width reduction parameter
            "ww" => {
                self.ww = v;
                self.given.ww = true;
            }
            // Width reduction parameter for CV
            "wwc" => {
                self.wwc = v;
                self.given.wwc = true;
            }
            // Width reduction parameter
            "wwn" => {
                self.wwn = v;
                self.given.wwn = true;
            }
            // Width reduction parameter
            "wwl" => {
                self.wwl = v;
                self.given.wwl = true;
            }
            // Width reduction parameter for CV
            "wwlc" => {
                self.wwlc = v;
                self.given.wwlc = true;
            }
            // Minimum width for the model
            "wmin" => {
                self.wmin = v;
                self.given.wmin = true;
            }
            // Maximum width for the model
            "wmax" => {
                self.wmax = v;
                self.given.wmax = true;
            }
            // Abulk narrow width parameter
            "b0" => {
                self.b0 = v;
                self.given.b0 = true;
            }
            // Abulk narrow width parameter
            "b1" => {
                self.b1 = v;
                self.given.b1 = true;
            }
            // New C-V model parameter
            "cgsl" => {
                self.cgsl = v;
                self.given.cgsl = true;
            }
            // New C-V model parameter
            "cgdl" => {
                self.cgdl = v;
                self.given.cgdl = true;
            }
            // S/G overlap C-V parameter
            "ckappas" => {
                self.ckappas = v;
                self.given.ckappas = true;
            }
            // D/G overlap C-V parameter
            "ckappad" => {
                self.ckappad = v;
                self.given.ckappad = true;
            }
            // Fringe capacitance parameter
            "cf" => {
                self.cf = v;
                self.given.cf = true;
            }
            // Vdsat parameter for C-V model
            "clc" => {
                self.clc = v;
                self.given.clc = true;
            }
            // Vdsat parameter for C-V model
            "cle" => {
                self.cle = v;
                self.given.cle = true;
            }
            // Delta W for C-V model
            "dwc" => {
                self.dwc = v;
                self.given.dwc = true;
            }
            // Delta L for C-V model
            "dlc" => {
                self.dlc = v;
                self.given.dlc = true;
            }
            // W offset for channel width due to mask/etch effect
            "xw" => {
                self.xw = v;
                self.given.xw = true;
            }
            // L offset for channel length due to mask/etch effect
            "xl" => {
                self.xl = v;
                self.given.xl = true;
            }
            // Delta L for Ig model
            "dlcig" => {
                self.dlcig = v;
                self.given.dlcig = true;
            }
            // Delta L for Ig model drain side
            "dlcigd" => {
                self.dlcigd = v;
                self.given.dlcigd = true;
            }
            // Delta W for S/D junctions
            "dwj" => {
                self.dwj = v;
                self.given.dwj = true;
            }
            // substrate current model parameter
            "alpha0" => {
                self.alpha0 = v;
                self.given.alpha0 = true;
            }
            // substrate current model parameter
            "alpha1" => {
                self.alpha1 = v;
                self.given.alpha1 = true;
            }
            // substrate current model parameter
            "beta0" => {
                self.beta0 = v;
                self.given.beta0 = true;
            }
            // Pre-exponential constant for GIDL
            "agidl" => {
                self.agidl = v;
                self.given.agidl = true;
            }
            // Exponential constant for GIDL
            "bgidl" => {
                self.bgidl = v;
                self.given.bgidl = true;
            }
            // Parameter for body-bias dependence of GIDL
            "cgidl" => {
                self.cgidl = v;
                self.given.cgidl = true;
            }
            // GIDL vg parameter
            "rgidl" => {
                self.rgidl = v;
                self.given.rgidl = true;
            }
            // GIDL vb parameter
            "kgidl" => {
                self.kgidl = v;
                self.given.kgidl = true;
            }
            // GIDL vb parameter
            "fgidl" => {
                self.fgidl = v;
                self.given.fgidl = true;
            }
            // Fitting parameter for Bandbending
            "egidl" => {
                self.egidl = v;
                self.given.egidl = true;
            }
            // Pre-exponential constant for GISL
            "agisl" => {
                self.agisl = v;
                self.given.agisl = true;
            }
            // Exponential constant for GISL
            "bgisl" => {
                self.bgisl = v;
                self.given.bgisl = true;
            }
            // Parameter for body-bias dependence of GISL
            "cgisl" => {
                self.cgisl = v;
                self.given.cgisl = true;
            }
            // GISL vg parameter
            "rgisl" => {
                self.rgisl = v;
                self.given.rgisl = true;
            }
            // GISL vb parameter
            "kgisl" => {
                self.kgisl = v;
                self.given.kgisl = true;
            }
            // GISL vb parameter
            "fgisl" => {
                self.fgisl = v;
                self.given.fgisl = true;
            }
            // Fitting parameter for Bandbending
            "egisl" => {
                self.egisl = v;
                self.given.egisl = true;
            }
            // Parameter for Igc
            "aigc" => {
                self.aigc = v;
                self.given.aigc = true;
            }
            // Parameter for Igc
            "bigc" => {
                self.bigc = v;
                self.given.bigc = true;
            }
            // Parameter for Igc
            "cigc" => {
                self.cigc = v;
                self.given.cigc = true;
            }
            // Parameter for Igs,d
            "aigsd" => {
                self.aigsd = v;
                self.given.aigsd = true;
            }
            // Parameter for Igs,d
            "bigsd" => {
                self.bigsd = v;
                self.given.bigsd = true;
            }
            // Parameter for Igs,d
            "cigsd" => {
                self.cigsd = v;
                self.given.cigsd = true;
            }
            // Parameter for Igs
            "aigs" => {
                self.aigs = v;
                self.given.aigs = true;
            }
            // Parameter for Igs
            "bigs" => {
                self.bigs = v;
                self.given.bigs = true;
            }
            // Parameter for Igs
            "cigs" => {
                self.cigs = v;
                self.given.cigs = true;
            }
            // Parameter for Igd
            "aigd" => {
                self.aigd = v;
                self.given.aigd = true;
            }
            // Parameter for Igd
            "bigd" => {
                self.bigd = v;
                self.given.bigd = true;
            }
            // Parameter for Igd
            "cigd" => {
                self.cigd = v;
                self.given.cigd = true;
            }
            // Parameter for Igb
            "aigbacc" => {
                self.aigbacc = v;
                self.given.aigbacc = true;
            }
            // Parameter for Igb
            "bigbacc" => {
                self.bigbacc = v;
                self.given.bigbacc = true;
            }
            // Parameter for Igb
            "cigbacc" => {
                self.cigbacc = v;
                self.given.cigbacc = true;
            }
            // Parameter for Igb
            "aigbinv" => {
                self.aigbinv = v;
                self.given.aigbinv = true;
            }
            // Parameter for Igb
            "bigbinv" => {
                self.bigbinv = v;
                self.given.bigbinv = true;
            }
            // Parameter for Igb
            "cigbinv" => {
                self.cigbinv = v;
                self.given.cigbinv = true;
            }
            // Parameter for Igc slope
            "nigc" => {
                self.nigc = v;
                self.given.nigc = true;
            }
            // Parameter for Igbinv slope
            "nigbinv" => {
                self.nigbinv = v;
                self.given.nigbinv = true;
            }
            // Parameter for Igbacc slope
            "nigbacc" => {
                self.nigbacc = v;
                self.given.nigbacc = true;
            }
            // Exponent for Tox ratio
            "ntox" => {
                self.ntox = v;
                self.given.ntox = true;
            }
            // Parameter for the Si bandgap for Igbinv
            "eigbinv" => {
                self.eigbinv = v;
                self.given.eigbinv = true;
            }
            // Parameter for Igc partition
            "pigcd" => {
                self.pigcd = v;
                self.given.pigcd = true;
            }
            // Factor for the gate edge Tox
            "poxedge" => {
                self.poxedge = v;
                self.given.poxedge = true;
            }
            // Forward drain diode forward limiting current
            "ijthdfwd" => {
                self.ijthdfwd = v;
                self.given.ijthdfwd = true;
            }
            // Forward source diode forward limiting current
            "ijthsfwd" => {
                self.ijthsfwd = v;
                self.given.ijthsfwd = true;
            }
            // Reverse drain diode forward limiting current
            "ijthdrev" => {
                self.ijthdrev = v;
                self.given.ijthdrev = true;
            }
            // Reverse source diode forward limiting current
            "ijthsrev" => {
                self.ijthsrev = v;
                self.given.ijthsrev = true;
            }
            // Fitting parameter for drain diode breakdown current
            "xjbvd" => {
                self.xjbvd = v;
                self.given.xjbvd = true;
            }
            // Fitting parameter for source diode breakdown current
            "xjbvs" => {
                self.xjbvs = v;
                self.given.xjbvs = true;
            }
            // Drain diode breakdown voltage
            "bvd" => {
                self.bvd = v;
                self.given.bvd = true;
            }
            // Source diode breakdown voltage
            "bvs" => {
                self.bvs = v;
                self.given.bvs = true;
            }
            // Source bottom trap-assisted saturation current density
            "jtss" => {
                self.jtss = v;
                self.given.jtss = true;
            }
            // Drain bottom trap-assisted saturation current density
            "jtsd" => {
                self.jtsd = v;
                self.given.jtsd = true;
            }
            // Source STI sidewall trap-assisted saturation current density
            "jtssws" => {
                self.jtssws = v;
                self.given.jtssws = true;
            }
            // Drain STI sidewall trap-assisted saturation current density
            "jtsswd" => {
                self.jtsswd = v;
                self.given.jtsswd = true;
            }
            // Source gate-edge sidewall trap-assisted saturation current density
            "jtsswgs" => {
                self.jtsswgs = v;
                self.given.jtsswgs = true;
            }
            // Drain gate-edge sidewall trap-assisted saturation current density
            "jtsswgd" => {
                self.jtsswgd = v;
                self.given.jtsswgd = true;
            }
            // TAT current width dependance
            "jtweff" => {
                self.jtweff = v;
                self.given.jtweff = true;
            }
            // Non-ideality factor for bottom junction
            "njts" => {
                self.njts = v;
                self.given.njts = true;
            }
            // Non-ideality factor for STI sidewall junction
            "njtssw" => {
                self.njtssw = v;
                self.given.njtssw = true;
            }
            // Non-ideality factor for gate-edge sidewall junction
            "njtsswg" => {
                self.njtsswg = v;
                self.given.njtsswg = true;
            }
            // Non-ideality factor for bottom junction drain side
            "njtsd" => {
                self.njtsd = v;
                self.given.njtsd = true;
            }
            // Non-ideality factor for STI sidewall junction drain side
            "njtsswd" => {
                self.njtsswd = v;
                self.given.njtsswd = true;
            }
            // Non-ideality factor for gate-edge sidewall junction drain side
            "njtsswgd" => {
                self.njtsswgd = v;
                self.given.njtsswgd = true;
            }
            // Power dependence of JTSS on temperature
            "xtss" => {
                self.xtss = v;
                self.given.xtss = true;
            }
            // Power dependence of JTSD on temperature
            "xtsd" => {
                self.xtsd = v;
                self.given.xtsd = true;
            }
            // Power dependence of JTSSWS on temperature
            "xtssws" => {
                self.xtssws = v;
                self.given.xtssws = true;
            }
            // Power dependence of JTSSWD on temperature
            "xtsswd" => {
                self.xtsswd = v;
                self.given.xtsswd = true;
            }
            // Power dependence of JTSSWGS on temperature
            "xtsswgs" => {
                self.xtsswgs = v;
                self.given.xtsswgs = true;
            }
            // Power dependence of JTSSWGD on temperature
            "xtsswgd" => {
                self.xtsswgd = v;
                self.given.xtsswgd = true;
            }
            // Temperature coefficient for NJTS
            "tnjts" => {
                self.tnjts = v;
                self.given.tnjts = true;
            }
            // Temperature coefficient for NJTSSW
            "tnjtssw" => {
                self.tnjtssw = v;
                self.given.tnjtssw = true;
            }
            // Temperature coefficient for NJTSSWG
            "tnjtsswg" => {
                self.tnjtsswg = v;
                self.given.tnjtsswg = true;
            }
            // Temperature coefficient for NJTSD
            "tnjtsd" => {
                self.tnjtsd = v;
                self.given.tnjtsd = true;
            }
            // Temperature coefficient for NJTSSWD
            "tnjtsswd" => {
                self.tnjtsswd = v;
                self.given.tnjtsswd = true;
            }
            // Temperature coefficient for NJTSSWGD
            "tnjtsswgd" => {
                self.tnjtsswgd = v;
                self.given.tnjtsswgd = true;
            }
            // Source bottom trap-assisted voltage dependent parameter
            "vtss" => {
                self.vtss = v;
                self.given.vtss = true;
            }
            // Drain bottom trap-assisted voltage dependent parameter
            "vtsd" => {
                self.vtsd = v;
                self.given.vtsd = true;
            }
            // Source STI sidewall trap-assisted voltage dependent parameter
            "vtssws" => {
                self.vtssws = v;
                self.given.vtssws = true;
            }
            // Drain STI sidewall trap-assisted voltage dependent parameter
            "vtsswd" => {
                self.vtsswd = v;
                self.given.vtsswd = true;
            }
            // Source gate-edge sidewall trap-assisted voltage dependent parameter
            "vtsswgs" => {
                self.vtsswgs = v;
                self.given.vtsswgs = true;
            }
            // Drain gate-edge sidewall trap-assisted voltage dependent parameter
            "vtsswgd" => {
                self.vtsswgd = v;
                self.given.vtsswgd = true;
            }
            // Minimum body conductance
            "gbmin" => {
                self.gbmin = v;
                self.given.gbmin = true;
            }
            // Resistance between bNode and dbNode
            "rbdb" => {
                self.rbdb = v;
                self.given.rbdb = true;
            }
            // Resistance between bNodePrime and bNode
            "rbpb" => {
                self.rbpb = v;
                self.given.rbpb = true;
            }
            // Resistance between bNode and sbNode
            "rbsb" => {
                self.rbsb = v;
                self.given.rbsb = true;
            }
            // Resistance between bNodePrime and sbNode
            "rbps" => {
                self.rbps = v;
                self.given.rbps = true;
            }
            // Resistance between bNodePrime and bNode
            "rbpd" => {
                self.rbpd = v;
                self.given.rbpd = true;
            }
            // Body resistance RBPS scaling
            "rbps0" => {
                self.rbps0 = v;
                self.given.rbps0 = true;
            }
            // Body resistance RBPS L scaling
            "rbpsl" => {
                self.rbpsl = v;
                self.given.rbpsl = true;
            }
            // Body resistance RBPS W scaling
            "rbpsw" => {
                self.rbpsw = v;
                self.given.rbpsw = true;
            }
            // Body resistance RBPS NF scaling
            "rbpsnf" => {
                self.rbpsnf = v;
                self.given.rbpsnf = true;
            }
            // Body resistance RBPD scaling
            "rbpd0" => {
                self.rbpd0 = v;
                self.given.rbpd0 = true;
            }
            // Body resistance RBPD L scaling
            "rbpdl" => {
                self.rbpdl = v;
                self.given.rbpdl = true;
            }
            // Body resistance RBPD W scaling
            "rbpdw" => {
                self.rbpdw = v;
                self.given.rbpdw = true;
            }
            // Body resistance RBPD NF scaling
            "rbpdnf" => {
                self.rbpdnf = v;
                self.given.rbpdnf = true;
            }
            // Body resistance RBPBX  scaling
            "rbpbx0" => {
                self.rbpbx0 = v;
                self.given.rbpbx0 = true;
            }
            // Body resistance RBPBX L scaling
            "rbpbxl" => {
                self.rbpbxl = v;
                self.given.rbpbxl = true;
            }
            // Body resistance RBPBX W scaling
            "rbpbxw" => {
                self.rbpbxw = v;
                self.given.rbpbxw = true;
            }
            // Body resistance RBPBX NF scaling
            "rbpbxnf" => {
                self.rbpbxnf = v;
                self.given.rbpbxnf = true;
            }
            // Body resistance RBPBY  scaling
            "rbpby0" => {
                self.rbpby0 = v;
                self.given.rbpby0 = true;
            }
            // Body resistance RBPBY L scaling
            "rbpbyl" => {
                self.rbpbyl = v;
                self.given.rbpbyl = true;
            }
            // Body resistance RBPBY W scaling
            "rbpbyw" => {
                self.rbpbyw = v;
                self.given.rbpbyw = true;
            }
            // Body resistance RBPBY NF scaling
            "rbpbynf" => {
                self.rbpbynf = v;
                self.given.rbpbynf = true;
            }
            // Body resistance RBSBX  scaling
            "rbsbx0" => {
                self.rbsbx0 = v;
                self.given.rbsbx0 = true;
            }
            // Body resistance RBSBY  scaling
            "rbsby0" => {
                self.rbsby0 = v;
                self.given.rbsby0 = true;
            }
            // Body resistance RBDBX  scaling
            "rbdbx0" => {
                self.rbdbx0 = v;
                self.given.rbdbx0 = true;
            }
            // Body resistance RBDBY  scaling
            "rbdby0" => {
                self.rbdby0 = v;
                self.given.rbdby0 = true;
            }
            // Body resistance RBSDBX L scaling
            "rbsdbxl" => {
                self.rbsdbxl = v;
                self.given.rbsdbxl = true;
            }
            // Body resistance RBSDBX W scaling
            "rbsdbxw" => {
                self.rbsdbxw = v;
                self.given.rbsdbxw = true;
            }
            // Body resistance RBSDBX NF scaling
            "rbsdbxnf" => {
                self.rbsdbxnf = v;
                self.given.rbsdbxnf = true;
            }
            // Body resistance RBSDBY L scaling
            "rbsdbyl" => {
                self.rbsdbyl = v;
                self.given.rbsdbyl = true;
            }
            // Body resistance RBSDBY W scaling
            "rbsdbyw" => {
                self.rbsdbyw = v;
                self.given.rbsdbyw = true;
            }
            // Body resistance RBSDBY NF scaling
            "rbsdbynf" => {
                self.rbsdbynf = v;
                self.given.rbsdbynf = true;
            }
            // Length dependence of cdsc
            "lcdsc" => {
                self.lcdsc = v;
                self.given.lcdsc = true;
            }
            // Length dependence of cdscb
            "lcdscb" => {
                self.lcdscb = v;
                self.given.lcdscb = true;
            }
            // Length dependence of cdscd
            "lcdscd" => {
                self.lcdscd = v;
                self.given.lcdscd = true;
            }
            // Length dependence of cit
            "lcit" => {
                self.lcit = v;
                self.given.lcit = true;
            }
            // Length dependence of nfactor
            "lnfactor" => {
                self.lnfactor = v;
                self.given.lnfactor = true;
            }
            // Length dependence of xj
            "lxj" => {
                self.lxj = v;
                self.given.lxj = true;
            }
            // Length dependence of vsat
            "lvsat" => {
                self.lvsat = v;
                self.given.lvsat = true;
            }
            // Length dependence of at
            "lat" => {
                self.lat = v;
                self.given.lat = true;
            }
            // Length dependence of a0
            "la0" => {
                self.la0 = v;
                self.given.la0 = true;
            }
            // Length dependence of ags
            "lags" => {
                self.lags = v;
                self.given.lags = true;
            }
            // Length dependence of a1
            "la1" => {
                self.la1 = v;
                self.given.la1 = true;
            }
            // Length dependence of a2
            "la2" => {
                self.la2 = v;
                self.given.la2 = true;
            }
            // Length dependence of keta
            "lketa" => {
                self.lketa = v;
                self.given.lketa = true;
            }
            // Length dependence of ketac
            "lketac" => {
                self.lketac = v;
                self.given.lketac = true;
            }
            // Length dependence of nsub
            "lnsub" => {
                self.lnsub = v;
                self.given.lnsub = true;
            }
            // Length dependence of ndep
            "lndep" => {
                self.lndep = v;
                self.given.lndep = true;
                if self.lndep > 1.0e20 {
                    self.lndep *= 1.0e-6;
                }
            }
            // Length dependence of nsd
            "lnsd" => {
                self.lnsd = v;
                self.given.lnsd = true;
                if self.lnsd > 1.0e23 {
                    self.lnsd *= 1.0e-6;
                }
            }
            // Length dependence of phin
            "lphin" => {
                self.lphin = v;
                self.given.lphin = true;
            }
            // Length dependence of ngate
            "lngate" => {
                self.lngate = v;
                self.given.lngate = true;
                if self.lngate > 1.0e23 {
                    self.lngate *= 1.0e-6;
                }
            }
            // Length dependence of gamma1
            "lgamma1" => {
                self.lgamma1 = v;
                self.given.lgamma1 = true;
            }
            // Length dependence of gamma2
            "lgamma2" => {
                self.lgamma2 = v;
                self.given.lgamma2 = true;
            }
            // Length dependence of vbx
            "lvbx" => {
                self.lvbx = v;
                self.given.lvbx = true;
            }
            // Length dependence of vbm
            "lvbm" => {
                self.lvbm = v;
                self.given.lvbm = true;
            }
            // Length dependence of xt
            "lxt" => {
                self.lxt = v;
                self.given.lxt = true;
            }
            // Length dependence of k1
            "lk1" => {
                self.lk1 = v;
                self.given.lk1 = true;
            }
            // Length dependence of kt1
            "lkt1" => {
                self.lkt1 = v;
                self.given.lkt1 = true;
            }
            // Length dependence of kt1l
            "lkt1l" => {
                self.lkt1l = v;
                self.given.lkt1l = true;
            }
            // Length dependence of kt2
            "lkt2" => {
                self.lkt2 = v;
                self.given.lkt2 = true;
            }
            // Length dependence of k2
            "lk2" => {
                self.lk2 = v;
                self.given.lk2 = true;
            }
            // Length dependence of k3
            "lk3" => {
                self.lk3 = v;
                self.given.lk3 = true;
            }
            // Length dependence of k3b
            "lk3b" => {
                self.lk3b = v;
                self.given.lk3b = true;
            }
            // Length dependence of w0
            "lw0" => {
                self.lw0 = v;
                self.given.lw0 = true;
            }
            // Length dependence of dvtp0
            "ldvtp0" => {
                self.ldvtp0 = v;
                self.given.ldvtp0 = true;
            }
            // Length dependence of dvtp1
            "ldvtp1" => {
                self.ldvtp1 = v;
                self.given.ldvtp1 = true;
            }
            // Length dependence of dvtp2
            "ldvtp2" => {
                self.ldvtp2 = v;
                self.given.ldvtp2 = true;
            }
            // Length dependence of dvtp3
            "ldvtp3" => {
                self.ldvtp3 = v;
                self.given.ldvtp3 = true;
            }
            // Length dependence of dvtp4
            "ldvtp4" => {
                self.ldvtp4 = v;
                self.given.ldvtp4 = true;
            }
            // Length dependence of dvtp5
            "ldvtp5" => {
                self.ldvtp5 = v;
                self.given.ldvtp5 = true;
            }
            // Length dependence of lpe0
            "llpe0" => {
                self.llpe0 = v;
                self.given.llpe0 = true;
            }
            // Length dependence of lpeb
            "llpeb" => {
                self.llpeb = v;
                self.given.llpeb = true;
            }
            // Length dependence of dvt0
            "ldvt0" => {
                self.ldvt0 = v;
                self.given.ldvt0 = true;
            }
            // Length dependence of dvt1
            "ldvt1" => {
                self.ldvt1 = v;
                self.given.ldvt1 = true;
            }
            // Length dependence of dvt2
            "ldvt2" => {
                self.ldvt2 = v;
                self.given.ldvt2 = true;
            }
            // Length dependence of dvt0w
            "ldvt0w" => {
                self.ldvt0w = v;
                self.given.ldvt0w = true;
            }
            // Length dependence of dvt1w
            "ldvt1w" => {
                self.ldvt1w = v;
                self.given.ldvt1w = true;
            }
            // Length dependence of dvt2w
            "ldvt2w" => {
                self.ldvt2w = v;
                self.given.ldvt2w = true;
            }
            // Length dependence of drout
            "ldrout" => {
                self.ldrout = v;
                self.given.ldrout = true;
            }
            // Length dependence of dsub
            "ldsub" => {
                self.ldsub = v;
                self.given.ldsub = true;
            }
            // Length dependence of vto
            "lvth0" | "lvtho" => {
                self.lvth0 = v;
                self.given.lvth0 = true;
            }
            // Length dependence of ua
            "lua" => {
                self.lua = v;
                self.given.lua = true;
            }
            // Length dependence of ua1
            "lua1" => {
                self.lua1 = v;
                self.given.lua1 = true;
            }
            // Length dependence of ub
            "lub" => {
                self.lub = v;
                self.given.lub = true;
            }
            // Length dependence of ub1
            "lub1" => {
                self.lub1 = v;
                self.given.lub1 = true;
            }
            // Length dependence of uc
            "luc" => {
                self.luc = v;
                self.given.luc = true;
            }
            // Length dependence of uc1
            "luc1" => {
                self.luc1 = v;
                self.given.luc1 = true;
            }
            // Length dependence of ud
            "lud" => {
                self.lud = v;
                self.given.lud = true;
            }
            // Length dependence of ud1
            "lud1" => {
                self.lud1 = v;
                self.given.lud1 = true;
            }
            // Length dependence of up
            "lup" => {
                self.lup = v;
                self.given.lup = true;
            }
            // Length dependence of lp
            "llp" => {
                self.llp = v;
                self.given.llp = true;
            }
            // Length dependence of u0
            "lu0" => {
                self.lu0 = v;
                self.given.lu0 = true;
            }
            // Length dependence of ute
            "lute" => {
                self.lute = v;
                self.given.lute = true;
            }
            // Length dependence of ucste
            "lucste" => {
                self.lucste = v;
                self.given.lucste = true;
            }
            // Length dependence of voff
            "lvoff" => {
                self.lvoff = v;
                self.given.lvoff = true;
            }
            // Length dependence of minv
            "lminv" => {
                self.lminv = v;
                self.given.lminv = true;
            }
            // Length dependence of minvcv
            "lminvcv" => {
                self.lminvcv = v;
                self.given.lminvcv = true;
            }
            // Length dependence of delta
            "ldelta" => {
                self.ldelta = v;
                self.given.ldelta = true;
            }
            // Length dependence of rdsw
            "lrdsw" => {
                self.lrdsw = v;
                self.given.lrdsw = true;
            }
            // Length dependence of rsw
            "lrsw" => {
                self.lrsw = v;
                self.given.lrsw = true;
            }
            // Length dependence of rdw
            "lrdw" => {
                self.lrdw = v;
                self.given.lrdw = true;
            }
            // Length dependence of prwg
            "lprwg" => {
                self.lprwg = v;
                self.given.lprwg = true;
            }
            // Length dependence of prwb
            "lprwb" => {
                self.lprwb = v;
                self.given.lprwb = true;
            }
            // Length dependence of prt
            "lprt" => {
                self.lprt = v;
                self.given.lprt = true;
            }
            // Length dependence of eta0
            "leta0" => {
                self.leta0 = v;
                self.given.leta0 = true;
            }
            // Length dependence of etab
            "letab" => {
                self.letab = v;
                self.given.letab = true;
            }
            // Length dependence of pclm
            "lpclm" => {
                self.lpclm = v;
                self.given.lpclm = true;
            }
            // Length dependence of pdiblc1
            "lpdiblc1" => {
                self.lpdibl1 = v;
                self.given.lpdibl1 = true;
            }
            // Length dependence of pdiblc2
            "lpdiblc2" => {
                self.lpdibl2 = v;
                self.given.lpdibl2 = true;
            }
            // Length dependence of pdiblcb
            "lpdiblcb" => {
                self.lpdiblb = v;
                self.given.lpdiblb = true;
            }
            // Length dependence of pdiblcb
            "lfprout" => {
                self.lfprout = v;
                self.given.lfprout = true;
            }
            // Length dependence of pdits
            "lpdits" => {
                self.lpdits = v;
                self.given.lpdits = true;
            }
            // Length dependence of pditsd
            "lpditsd" => {
                self.lpditsd = v;
                self.given.lpditsd = true;
            }
            // Length dependence of pscbe1
            "lpscbe1" => {
                self.lpscbe1 = v;
                self.given.lpscbe1 = true;
            }
            // Length dependence of pscbe2
            "lpscbe2" => {
                self.lpscbe2 = v;
                self.given.lpscbe2 = true;
            }
            // Length dependence of pvag
            "lpvag" => {
                self.lpvag = v;
                self.given.lpvag = true;
            }
            // Length dependence of wr
            "lwr" => {
                self.lwr = v;
                self.given.lwr = true;
            }
            // Length dependence of dwg
            "ldwg" => {
                self.ldwg = v;
                self.given.ldwg = true;
            }
            // Length dependence of dwb
            "ldwb" => {
                self.ldwb = v;
                self.given.ldwb = true;
            }
            // Length dependence of b0
            "lb0" => {
                self.lb0 = v;
                self.given.lb0 = true;
            }
            // Length dependence of b1
            "lb1" => {
                self.lb1 = v;
                self.given.lb1 = true;
            }
            // Length dependence of cgsl
            "lcgsl" => {
                self.lcgsl = v;
                self.given.lcgsl = true;
            }
            // Length dependence of cgdl
            "lcgdl" => {
                self.lcgdl = v;
                self.given.lcgdl = true;
            }
            // Length dependence of ckappas
            "lckappas" => {
                self.lckappas = v;
                self.given.lckappas = true;
            }
            // Length dependence of ckappad
            "lckappad" => {
                self.lckappad = v;
                self.given.lckappad = true;
            }
            // Length dependence of cf
            "lcf" => {
                self.lcf = v;
                self.given.lcf = true;
            }
            // Length dependence of clc
            "lclc" => {
                self.lclc = v;
                self.given.lclc = true;
            }
            // Length dependence of cle
            "lcle" => {
                self.lcle = v;
                self.given.lcle = true;
            }
            // Length dependence of alpha0
            "lalpha0" => {
                self.lalpha0 = v;
                self.given.lalpha0 = true;
            }
            // Length dependence of alpha1
            "lalpha1" => {
                self.lalpha1 = v;
                self.given.lalpha1 = true;
            }
            // Length dependence of beta0
            "lbeta0" => {
                self.lbeta0 = v;
                self.given.lbeta0 = true;
            }
            // Length dependence of agidl
            "lagidl" => {
                self.lagidl = v;
                self.given.lagidl = true;
            }
            // Length dependence of bgidl
            "lbgidl" => {
                self.lbgidl = v;
                self.given.lbgidl = true;
            }
            // Length dependence of cgidl
            "lcgidl" => {
                self.lcgidl = v;
                self.given.lcgidl = true;
            }
            // Length dependence of rgidl
            "lrgidl" => {
                self.lrgidl = v;
                self.given.lrgidl = true;
            }
            // Length dependence of kgidl
            "lkgidl" => {
                self.lkgidl = v;
                self.given.lkgidl = true;
            }
            // Length dependence of fgidl
            "lfgidl" => {
                self.lfgidl = v;
                self.given.lfgidl = true;
            }
            // Length dependence of egidl
            "legidl" => {
                self.legidl = v;
                self.given.legidl = true;
            }
            // Length dependence of agisl
            "lagisl" => {
                self.lagisl = v;
                self.given.lagisl = true;
            }
            // Length dependence of bgisl
            "lbgisl" => {
                self.lbgisl = v;
                self.given.lbgisl = true;
            }
            // Length dependence of cgisl
            "lcgisl" => {
                self.lcgisl = v;
                self.given.lcgisl = true;
            }
            // Length dependence of rgisl
            "lrgisl" => {
                self.lrgisl = v;
                self.given.lrgisl = true;
            }
            // Length dependence of kgisl
            "lkgisl" => {
                self.lkgisl = v;
                self.given.lkgisl = true;
            }
            // Length dependence of fgisl
            "lfgisl" => {
                self.lfgisl = v;
                self.given.lfgisl = true;
            }
            // Length dependence of egisl
            "legisl" => {
                self.legisl = v;
                self.given.legisl = true;
            }
            // Length dependence of aigc
            "laigc" => {
                self.laigc = v;
                self.given.laigc = true;
            }
            // Length dependence of bigc
            "lbigc" => {
                self.lbigc = v;
                self.given.lbigc = true;
            }
            // Length dependence of cigc
            "lcigc" => {
                self.lcigc = v;
                self.given.lcigc = true;
            }
            // Length dependence of aigsd
            "laigsd" => {
                self.laigsd = v;
                self.given.laigsd = true;
            }
            // Length dependence of bigsd
            "lbigsd" => {
                self.lbigsd = v;
                self.given.lbigsd = true;
            }
            // Length dependence of cigsd
            "lcigsd" => {
                self.lcigsd = v;
                self.given.lcigsd = true;
            }
            // Length dependence of aigs
            "laigs" => {
                self.laigs = v;
                self.given.laigs = true;
            }
            // Length dependence of bigs
            "lbigs" => {
                self.lbigs = v;
                self.given.lbigs = true;
            }
            // Length dependence of cigs
            "lcigs" => {
                self.lcigs = v;
                self.given.lcigs = true;
            }
            // Length dependence of aigd
            "laigd" => {
                self.laigd = v;
                self.given.laigd = true;
            }
            // Length dependence of bigd
            "lbigd" => {
                self.lbigd = v;
                self.given.lbigd = true;
            }
            // Length dependence of cigd
            "lcigd" => {
                self.lcigd = v;
                self.given.lcigd = true;
            }
            // Length dependence of aigbacc
            "laigbacc" => {
                self.laigbacc = v;
                self.given.laigbacc = true;
            }
            // Length dependence of bigbacc
            "lbigbacc" => {
                self.lbigbacc = v;
                self.given.lbigbacc = true;
            }
            // Length dependence of cigbacc
            "lcigbacc" => {
                self.lcigbacc = v;
                self.given.lcigbacc = true;
            }
            // Length dependence of aigbinv
            "laigbinv" => {
                self.laigbinv = v;
                self.given.laigbinv = true;
            }
            // Length dependence of bigbinv
            "lbigbinv" => {
                self.lbigbinv = v;
                self.given.lbigbinv = true;
            }
            // Length dependence of cigbinv
            "lcigbinv" => {
                self.lcigbinv = v;
                self.given.lcigbinv = true;
            }
            // Length dependence of nigc
            "lnigc" => {
                self.lnigc = v;
                self.given.lnigc = true;
            }
            // Length dependence of nigbinv
            "lnigbinv" => {
                self.lnigbinv = v;
                self.given.lnigbinv = true;
            }
            // Length dependence of nigbacc
            "lnigbacc" => {
                self.lnigbacc = v;
                self.given.lnigbacc = true;
            }
            // Length dependence of ntox
            "lntox" => {
                self.lntox = v;
                self.given.lntox = true;
            }
            // Length dependence for eigbinv
            "leigbinv" => {
                self.leigbinv = v;
                self.given.leigbinv = true;
            }
            // Length dependence for pigcd
            "lpigcd" => {
                self.lpigcd = v;
                self.given.lpigcd = true;
            }
            // Length dependence for poxedge
            "lpoxedge" => {
                self.lpoxedge = v;
                self.given.lpoxedge = true;
            }
            // Length dependence of vfbcv
            "lvfbcv" => {
                self.lvfbcv = v;
                self.given.lvfbcv = true;
            }
            // Length dependence of vfb
            "lvfb" => {
                self.lvfb = v;
                self.given.lvfb = true;
            }
            // Length dependence of acde
            "lacde" => {
                self.lacde = v;
                self.given.lacde = true;
            }
            // Length dependence of moin
            "lmoin" => {
                self.lmoin = v;
                self.given.lmoin = true;
            }
            // Length dependence of noff
            "lnoff" => {
                self.lnoff = v;
                self.given.lnoff = true;
            }
            // Length dependence of voffcv
            "lvoffcv" => {
                self.lvoffcv = v;
                self.given.lvoffcv = true;
            }
            // Length dependence of xrcrg1
            "lxrcrg1" => {
                self.lxrcrg1 = v;
                self.given.lxrcrg1 = true;
            }
            // Length dependence of xrcrg2
            "lxrcrg2" => {
                self.lxrcrg2 = v;
                self.given.lxrcrg2 = true;
            }
            // Length dependence of lambda
            "llambda" => {
                self.llambda = v;
                self.given.llambda = true;
            }
            // Length dependence of vtl
            "lvtl" => {
                self.lvtl = v;
                self.given.lvtl = true;
            }
            // Length dependence of xn
            "lxn" => {
                self.lxn = v;
                self.given.lxn = true;
            }
            // Length dependence of eu
            "leu" => {
                self.leu = v;
                self.given.leu = true;
            }
            // Length dependence of lucs
            "lucs" => {
                self.lucs = v;
                self.given.lucs = true;
            }
            // Length dependence of vfbsdoff
            "lvfbsdoff" => {
                self.lvfbsdoff = v;
                self.given.lvfbsdoff = true;
            }
            // Length dependence of tvfbsdoff
            "ltvfbsdoff" => {
                self.ltvfbsdoff = v;
                self.given.ltvfbsdoff = true;
            }
            // Length dependence of tvoff
            "ltvoff" => {
                self.ltvoff = v;
                self.given.ltvoff = true;
            }
            // Length dependence of tnfactor
            "ltnfactor" => {
                self.ltnfactor = v;
                self.given.ltnfactor = true;
            }
            // Length dependence of teta0
            "lteta0" => {
                self.lteta0 = v;
                self.given.lteta0 = true;
            }
            // Length dependence of tvoffcv
            "ltvoffcv" => {
                self.ltvoffcv = v;
                self.given.ltvoffcv = true;
            }
            // Width dependence of cdsc
            "wcdsc" => {
                self.wcdsc = v;
                self.given.wcdsc = true;
            }
            // Width dependence of cdscb
            "wcdscb" => {
                self.wcdscb = v;
                self.given.wcdscb = true;
            }
            // Width dependence of cdscd
            "wcdscd" => {
                self.wcdscd = v;
                self.given.wcdscd = true;
            }
            // Width dependence of cit
            "wcit" => {
                self.wcit = v;
                self.given.wcit = true;
            }
            // Width dependence of nfactor
            "wnfactor" => {
                self.wnfactor = v;
                self.given.wnfactor = true;
            }
            // Width dependence of xj
            "wxj" => {
                self.wxj = v;
                self.given.wxj = true;
            }
            // Width dependence of vsat
            "wvsat" => {
                self.wvsat = v;
                self.given.wvsat = true;
            }
            // Width dependence of at
            "wat" => {
                self.wat = v;
                self.given.wat = true;
            }
            // Width dependence of a0
            "wa0" => {
                self.wa0 = v;
                self.given.wa0 = true;
            }
            // Width dependence of ags
            "wags" => {
                self.wags = v;
                self.given.wags = true;
            }
            // Width dependence of a1
            "wa1" => {
                self.wa1 = v;
                self.given.wa1 = true;
            }
            // Width dependence of a2
            "wa2" => {
                self.wa2 = v;
                self.given.wa2 = true;
            }
            // Width dependence of keta
            "wketa" => {
                self.wketa = v;
                self.given.wketa = true;
            }
            // Width dependence of ketac
            "wketac" => {
                self.wketac = v;
                self.given.wketac = true;
            }
            // Width dependence of nsub
            "wnsub" => {
                self.wnsub = v;
                self.given.wnsub = true;
            }
            // Width dependence of ndep
            "wndep" => {
                self.wndep = v;
                self.given.wndep = true;
                if self.wndep > 1.0e20 {
                    self.wndep *= 1.0e-6;
                }
            }
            // Width dependence of nsd
            "wnsd" => {
                self.wnsd = v;
                self.given.wnsd = true;
                if self.wnsd > 1.0e23 {
                    self.wnsd *= 1.0e-6;
                }
            }
            // Width dependence of phin
            "wphin" => {
                self.wphin = v;
                self.given.wphin = true;
            }
            // Width dependence of ngate
            "wngate" => {
                self.wngate = v;
                self.given.wngate = true;
                if self.wngate > 1.0e23 {
                    self.wngate *= 1.0e-6;
                }
            }
            // Width dependence of gamma1
            "wgamma1" => {
                self.wgamma1 = v;
                self.given.wgamma1 = true;
            }
            // Width dependence of gamma2
            "wgamma2" => {
                self.wgamma2 = v;
                self.given.wgamma2 = true;
            }
            // Width dependence of vbx
            "wvbx" => {
                self.wvbx = v;
                self.given.wvbx = true;
            }
            // Width dependence of vbm
            "wvbm" => {
                self.wvbm = v;
                self.given.wvbm = true;
            }
            // Width dependence of xt
            "wxt" => {
                self.wxt = v;
                self.given.wxt = true;
            }
            // Width dependence of k1
            "wk1" => {
                self.wk1 = v;
                self.given.wk1 = true;
            }
            // Width dependence of kt1
            "wkt1" => {
                self.wkt1 = v;
                self.given.wkt1 = true;
            }
            // Width dependence of kt1l
            "wkt1l" => {
                self.wkt1l = v;
                self.given.wkt1l = true;
            }
            // Width dependence of kt2
            "wkt2" => {
                self.wkt2 = v;
                self.given.wkt2 = true;
            }
            // Width dependence of k2
            "wk2" => {
                self.wk2 = v;
                self.given.wk2 = true;
            }
            // Width dependence of k3
            "wk3" => {
                self.wk3 = v;
                self.given.wk3 = true;
            }
            // Width dependence of k3b
            "wk3b" => {
                self.wk3b = v;
                self.given.wk3b = true;
            }
            // Width dependence of w0
            "ww0" => {
                self.ww0 = v;
                self.given.ww0 = true;
            }
            // Width dependence of dvtp0
            "wdvtp0" => {
                self.wdvtp0 = v;
                self.given.wdvtp0 = true;
            }
            // Width dependence of dvtp1
            "wdvtp1" => {
                self.wdvtp1 = v;
                self.given.wdvtp1 = true;
            }
            // Width dependence of dvtp2
            "wdvtp2" => {
                self.wdvtp2 = v;
                self.given.wdvtp2 = true;
            }
            // Width dependence of dvtp3
            "wdvtp3" => {
                self.wdvtp3 = v;
                self.given.wdvtp3 = true;
            }
            // Width dependence of dvtp4
            "wdvtp4" => {
                self.wdvtp4 = v;
                self.given.wdvtp4 = true;
            }
            // Width dependence of dvtp5
            "wdvtp5" => {
                self.wdvtp5 = v;
                self.given.wdvtp5 = true;
            }
            // Width dependence of lpe0
            "wlpe0" => {
                self.wlpe0 = v;
                self.given.wlpe0 = true;
            }
            // Width dependence of lpeb
            "wlpeb" => {
                self.wlpeb = v;
                self.given.wlpeb = true;
            }
            // Width dependence of dvt0
            "wdvt0" => {
                self.wdvt0 = v;
                self.given.wdvt0 = true;
            }
            // Width dependence of dvt1
            "wdvt1" => {
                self.wdvt1 = v;
                self.given.wdvt1 = true;
            }
            // Width dependence of dvt2
            "wdvt2" => {
                self.wdvt2 = v;
                self.given.wdvt2 = true;
            }
            // Width dependence of dvt0w
            "wdvt0w" => {
                self.wdvt0w = v;
                self.given.wdvt0w = true;
            }
            // Width dependence of dvt1w
            "wdvt1w" => {
                self.wdvt1w = v;
                self.given.wdvt1w = true;
            }
            // Width dependence of dvt2w
            "wdvt2w" => {
                self.wdvt2w = v;
                self.given.wdvt2w = true;
            }
            // Width dependence of drout
            "wdrout" => {
                self.wdrout = v;
                self.given.wdrout = true;
            }
            // Width dependence of dsub
            "wdsub" => {
                self.wdsub = v;
                self.given.wdsub = true;
            }
            // Width dependence of vto
            "wvth0" | "wvtho" => {
                self.wvth0 = v;
                self.given.wvth0 = true;
            }
            // Width dependence of ua
            "wua" => {
                self.wua = v;
                self.given.wua = true;
            }
            // Width dependence of ua1
            "wua1" => {
                self.wua1 = v;
                self.given.wua1 = true;
            }
            // Width dependence of ub
            "wub" => {
                self.wub = v;
                self.given.wub = true;
            }
            // Width dependence of ub1
            "wub1" => {
                self.wub1 = v;
                self.given.wub1 = true;
            }
            // Width dependence of uc
            "wuc" => {
                self.wuc = v;
                self.given.wuc = true;
            }
            // Width dependence of uc1
            "wuc1" => {
                self.wuc1 = v;
                self.given.wuc1 = true;
            }
            // Width dependence of ud
            "wud" => {
                self.wud = v;
                self.given.wud = true;
            }
            // Width dependence of ud1
            "wud1" => {
                self.wud1 = v;
                self.given.wud1 = true;
            }
            // Width dependence of up
            "wup" => {
                self.wup = v;
                self.given.wup = true;
            }
            // Width dependence of lp
            "wlp" => {
                self.wlp = v;
                self.given.wlp = true;
            }
            // Width dependence of u0
            "wu0" => {
                self.wu0 = v;
                self.given.wu0 = true;
            }
            // Width dependence of ute
            "wute" => {
                self.wute = v;
                self.given.wute = true;
            }
            // Width dependence of ucste
            "wucste" => {
                self.wucste = v;
                self.given.wucste = true;
            }
            // Width dependence of voff
            "wvoff" => {
                self.wvoff = v;
                self.given.wvoff = true;
            }
            // Width dependence of minv
            "wminv" => {
                self.wminv = v;
                self.given.wminv = true;
            }
            // Width dependence of minvcv
            "wminvcv" => {
                self.wminvcv = v;
                self.given.wminvcv = true;
            }
            // Width dependence of delta
            "wdelta" => {
                self.wdelta = v;
                self.given.wdelta = true;
            }
            // Width dependence of rdsw
            "wrdsw" => {
                self.wrdsw = v;
                self.given.wrdsw = true;
            }
            // Width dependence of rsw
            "wrsw" => {
                self.wrsw = v;
                self.given.wrsw = true;
            }
            // Width dependence of rdw
            "wrdw" => {
                self.wrdw = v;
                self.given.wrdw = true;
            }
            // Width dependence of prwg
            "wprwg" => {
                self.wprwg = v;
                self.given.wprwg = true;
            }
            // Width dependence of prwb
            "wprwb" => {
                self.wprwb = v;
                self.given.wprwb = true;
            }
            // Width dependence of prt
            "wprt" => {
                self.wprt = v;
                self.given.wprt = true;
            }
            // Width dependence of eta0
            "weta0" => {
                self.weta0 = v;
                self.given.weta0 = true;
            }
            // Width dependence of etab
            "wetab" => {
                self.wetab = v;
                self.given.wetab = true;
            }
            // Width dependence of pclm
            "wpclm" => {
                self.wpclm = v;
                self.given.wpclm = true;
            }
            // Width dependence of pdiblc1
            "wpdiblc1" => {
                self.wpdibl1 = v;
                self.given.wpdibl1 = true;
            }
            // Width dependence of pdiblc2
            "wpdiblc2" => {
                self.wpdibl2 = v;
                self.given.wpdibl2 = true;
            }
            // Width dependence of pdiblcb
            "wpdiblcb" => {
                self.wpdiblb = v;
                self.given.wpdiblb = true;
            }
            // Width dependence of pdiblcb
            "wfprout" => {
                self.wfprout = v;
                self.given.wfprout = true;
            }
            // Width dependence of pdits
            "wpdits" => {
                self.wpdits = v;
                self.given.wpdits = true;
            }
            // Width dependence of pditsd
            "wpditsd" => {
                self.wpditsd = v;
                self.given.wpditsd = true;
            }
            // Width dependence of pscbe1
            "wpscbe1" => {
                self.wpscbe1 = v;
                self.given.wpscbe1 = true;
            }
            // Width dependence of pscbe2
            "wpscbe2" => {
                self.wpscbe2 = v;
                self.given.wpscbe2 = true;
            }
            // Width dependence of pvag
            "wpvag" => {
                self.wpvag = v;
                self.given.wpvag = true;
            }
            // Width dependence of wr
            "wwr" => {
                self.wwr = v;
                self.given.wwr = true;
            }
            // Width dependence of dwg
            "wdwg" => {
                self.wdwg = v;
                self.given.wdwg = true;
            }
            // Width dependence of dwb
            "wdwb" => {
                self.wdwb = v;
                self.given.wdwb = true;
            }
            // Width dependence of b0
            "wb0" => {
                self.wb0 = v;
                self.given.wb0 = true;
            }
            // Width dependence of b1
            "wb1" => {
                self.wb1 = v;
                self.given.wb1 = true;
            }
            // Width dependence of cgsl
            "wcgsl" => {
                self.wcgsl = v;
                self.given.wcgsl = true;
            }
            // Width dependence of cgdl
            "wcgdl" => {
                self.wcgdl = v;
                self.given.wcgdl = true;
            }
            // Width dependence of ckappas
            "wckappas" => {
                self.wckappas = v;
                self.given.wckappas = true;
            }
            // Width dependence of ckappad
            "wckappad" => {
                self.wckappad = v;
                self.given.wckappad = true;
            }
            // Width dependence of cf
            "wcf" => {
                self.wcf = v;
                self.given.wcf = true;
            }
            // Width dependence of clc
            "wclc" => {
                self.wclc = v;
                self.given.wclc = true;
            }
            // Width dependence of cle
            "wcle" => {
                self.wcle = v;
                self.given.wcle = true;
            }
            // Width dependence of alpha0
            "walpha0" => {
                self.walpha0 = v;
                self.given.walpha0 = true;
            }
            // Width dependence of alpha1
            "walpha1" => {
                self.walpha1 = v;
                self.given.walpha1 = true;
            }
            // Width dependence of beta0
            "wbeta0" => {
                self.wbeta0 = v;
                self.given.wbeta0 = true;
            }
            // Width dependence of agidl
            "wagidl" => {
                self.wagidl = v;
                self.given.wagidl = true;
            }
            // Width dependence of bgidl
            "wbgidl" => {
                self.wbgidl = v;
                self.given.wbgidl = true;
            }
            // Width dependence of cgidl
            "wcgidl" => {
                self.wcgidl = v;
                self.given.wcgidl = true;
            }
            // Width dependence of rgidl
            "wrgidl" => {
                self.wrgidl = v;
                self.given.wrgidl = true;
            }
            // Width dependence of kgidl
            "wkgidl" => {
                self.wkgidl = v;
                self.given.wkgidl = true;
            }
            // Width dependence of fgidl
            "wfgidl" => {
                self.wfgidl = v;
                self.given.wfgidl = true;
            }
            // Width dependence of egidl
            "wegidl" => {
                self.wegidl = v;
                self.given.wegidl = true;
            }
            // Width dependence of agisl
            "wagisl" => {
                self.wagisl = v;
                self.given.wagisl = true;
            }
            // Width dependence of bgisl
            "wbgisl" => {
                self.wbgisl = v;
                self.given.wbgisl = true;
            }
            // Width dependence of cgisl
            "wcgisl" => {
                self.wcgisl = v;
                self.given.wcgisl = true;
            }
            // Width dependence of rgisl
            "wrgisl" => {
                self.wrgisl = v;
                self.given.wrgisl = true;
            }
            // Width dependence of kgisl
            "wkgisl" => {
                self.wkgisl = v;
                self.given.wkgisl = true;
            }
            // Width dependence of fgisl
            "wfgisl" => {
                self.wfgisl = v;
                self.given.wfgisl = true;
            }
            // Width dependence of egisl
            "wegisl" => {
                self.wegisl = v;
                self.given.wegisl = true;
            }
            // Width dependence of aigc
            "waigc" => {
                self.waigc = v;
                self.given.waigc = true;
            }
            // Width dependence of bigc
            "wbigc" => {
                self.wbigc = v;
                self.given.wbigc = true;
            }
            // Width dependence of cigc
            "wcigc" => {
                self.wcigc = v;
                self.given.wcigc = true;
            }
            // Width dependence of aigsd
            "waigsd" => {
                self.waigsd = v;
                self.given.waigsd = true;
            }
            // Width dependence of bigsd
            "wbigsd" => {
                self.wbigsd = v;
                self.given.wbigsd = true;
            }
            // Width dependence of cigsd
            "wcigsd" => {
                self.wcigsd = v;
                self.given.wcigsd = true;
            }
            // Width dependence of aigs
            "waigs" => {
                self.waigs = v;
                self.given.waigs = true;
            }
            // Width dependence of bigs
            "wbigs" => {
                self.wbigs = v;
                self.given.wbigs = true;
            }
            // Width dependence of cigs
            "wcigs" => {
                self.wcigs = v;
                self.given.wcigs = true;
            }
            // Width dependence of aigd
            "waigd" => {
                self.waigd = v;
                self.given.waigd = true;
            }
            // Width dependence of bigd
            "wbigd" => {
                self.wbigd = v;
                self.given.wbigd = true;
            }
            // Width dependence of cigd
            "wcigd" => {
                self.wcigd = v;
                self.given.wcigd = true;
            }
            // Width dependence of aigbacc
            "waigbacc" => {
                self.waigbacc = v;
                self.given.waigbacc = true;
            }
            // Width dependence of bigbacc
            "wbigbacc" => {
                self.wbigbacc = v;
                self.given.wbigbacc = true;
            }
            // Width dependence of cigbacc
            "wcigbacc" => {
                self.wcigbacc = v;
                self.given.wcigbacc = true;
            }
            // Width dependence of aigbinv
            "waigbinv" => {
                self.waigbinv = v;
                self.given.waigbinv = true;
            }
            // Width dependence of bigbinv
            "wbigbinv" => {
                self.wbigbinv = v;
                self.given.wbigbinv = true;
            }
            // Width dependence of cigbinv
            "wcigbinv" => {
                self.wcigbinv = v;
                self.given.wcigbinv = true;
            }
            // Width dependence of nigc
            "wnigc" => {
                self.wnigc = v;
                self.given.wnigc = true;
            }
            // Width dependence of nigbinv
            "wnigbinv" => {
                self.wnigbinv = v;
                self.given.wnigbinv = true;
            }
            // Width dependence of nigbacc
            "wnigbacc" => {
                self.wnigbacc = v;
                self.given.wnigbacc = true;
            }
            // Width dependence of ntox
            "wntox" => {
                self.wntox = v;
                self.given.wntox = true;
            }
            // Width dependence for eigbinv
            "weigbinv" => {
                self.weigbinv = v;
                self.given.weigbinv = true;
            }
            // Width dependence for pigcd
            "wpigcd" => {
                self.wpigcd = v;
                self.given.wpigcd = true;
            }
            // Width dependence for poxedge
            "wpoxedge" => {
                self.wpoxedge = v;
                self.given.wpoxedge = true;
            }
            // Width dependence of vfbcv
            "wvfbcv" => {
                self.wvfbcv = v;
                self.given.wvfbcv = true;
            }
            // Width dependence of vfb
            "wvfb" => {
                self.wvfb = v;
                self.given.wvfb = true;
            }
            // Width dependence of acde
            "wacde" => {
                self.wacde = v;
                self.given.wacde = true;
            }
            // Width dependence of moin
            "wmoin" => {
                self.wmoin = v;
                self.given.wmoin = true;
            }
            // Width dependence of noff
            "wnoff" => {
                self.wnoff = v;
                self.given.wnoff = true;
            }
            // Width dependence of voffcv
            "wvoffcv" => {
                self.wvoffcv = v;
                self.given.wvoffcv = true;
            }
            // Width dependence of xrcrg1
            "wxrcrg1" => {
                self.wxrcrg1 = v;
                self.given.wxrcrg1 = true;
            }
            // Width dependence of xrcrg2
            "wxrcrg2" => {
                self.wxrcrg2 = v;
                self.given.wxrcrg2 = true;
            }
            // Width dependence of lambda
            "wlambda" => {
                self.wlambda = v;
                self.given.wlambda = true;
            }
            // Width dependence of vtl
            "wvtl" => {
                self.wvtl = v;
                self.given.wvtl = true;
            }
            // Width dependence of xn
            "wxn" => {
                self.wxn = v;
                self.given.wxn = true;
            }
            // Width dependence of eu
            "weu" => {
                self.weu = v;
                self.given.weu = true;
            }
            // Width dependence of ucs
            "wucs" => {
                self.wucs = v;
                self.given.wucs = true;
            }
            // Width dependence of vfbsdoff
            "wvfbsdoff" => {
                self.wvfbsdoff = v;
                self.given.wvfbsdoff = true;
            }
            // Width dependence of tvfbsdoff
            "wtvfbsdoff" => {
                self.wtvfbsdoff = v;
                self.given.wtvfbsdoff = true;
            }
            // Width dependence of tvoff
            "wtvoff" => {
                self.wtvoff = v;
                self.given.wtvoff = true;
            }
            // Width dependence of tnfactor
            "wtnfactor" => {
                self.wtnfactor = v;
                self.given.wtnfactor = true;
            }
            // Width dependence of teta0
            "wteta0" => {
                self.wteta0 = v;
                self.given.wteta0 = true;
            }
            // Width dependence of tvoffcv
            "wtvoffcv" => {
                self.wtvoffcv = v;
                self.given.wtvoffcv = true;
            }
            // Cross-term dependence of cdsc
            "pcdsc" => {
                self.pcdsc = v;
                self.given.pcdsc = true;
            }
            // Cross-term dependence of cdscb
            "pcdscb" => {
                self.pcdscb = v;
                self.given.pcdscb = true;
            }
            // Cross-term dependence of cdscd
            "pcdscd" => {
                self.pcdscd = v;
                self.given.pcdscd = true;
            }
            // Cross-term dependence of cit
            "pcit" => {
                self.pcit = v;
                self.given.pcit = true;
            }
            // Cross-term dependence of nfactor
            "pnfactor" => {
                self.pnfactor = v;
                self.given.pnfactor = true;
            }
            // Cross-term dependence of xj
            "pxj" => {
                self.pxj = v;
                self.given.pxj = true;
            }
            // Cross-term dependence of vsat
            "pvsat" => {
                self.pvsat = v;
                self.given.pvsat = true;
            }
            // Cross-term dependence of at
            "pat" => {
                self.pat = v;
                self.given.pat = true;
            }
            // Cross-term dependence of a0
            "pa0" => {
                self.pa0 = v;
                self.given.pa0 = true;
            }
            // Cross-term dependence of ags
            "pags" => {
                self.pags = v;
                self.given.pags = true;
            }
            // Cross-term dependence of a1
            "pa1" => {
                self.pa1 = v;
                self.given.pa1 = true;
            }
            // Cross-term dependence of a2
            "pa2" => {
                self.pa2 = v;
                self.given.pa2 = true;
            }
            // Cross-term dependence of keta
            "pketa" => {
                self.pketa = v;
                self.given.pketa = true;
            }
            // Cross-term dependence of ketac
            "pketac" => {
                self.pketac = v;
                self.given.pketac = true;
            }
            // Cross-term dependence of nsub
            "pnsub" => {
                self.pnsub = v;
                self.given.pnsub = true;
            }
            // Cross-term dependence of ndep
            "pndep" => {
                self.pndep = v;
                self.given.pndep = true;
                if self.pndep > 1.0e20 {
                    self.pndep *= 1.0e-6;
                }
            }
            // Cross-term dependence of nsd
            "pnsd" => {
                self.pnsd = v;
                self.given.pnsd = true;
                if self.pnsd > 1.0e23 {
                    self.pnsd *= 1.0e-6;
                }
            }
            // Cross-term dependence of phin
            "pphin" => {
                self.pphin = v;
                self.given.pphin = true;
            }
            // Cross-term dependence of ngate
            "pngate" => {
                self.pngate = v;
                self.given.pngate = true;
                if self.pngate > 1.0e23 {
                    self.pngate *= 1.0e-6;
                }
            }
            // Cross-term dependence of gamma1
            "pgamma1" => {
                self.pgamma1 = v;
                self.given.pgamma1 = true;
            }
            // Cross-term dependence of gamma2
            "pgamma2" => {
                self.pgamma2 = v;
                self.given.pgamma2 = true;
            }
            // Cross-term dependence of vbx
            "pvbx" => {
                self.pvbx = v;
                self.given.pvbx = true;
            }
            // Cross-term dependence of vbm
            "pvbm" => {
                self.pvbm = v;
                self.given.pvbm = true;
            }
            // Cross-term dependence of xt
            "pxt" => {
                self.pxt = v;
                self.given.pxt = true;
            }
            // Cross-term dependence of k1
            "pk1" => {
                self.pk1 = v;
                self.given.pk1 = true;
            }
            // Cross-term dependence of kt1
            "pkt1" => {
                self.pkt1 = v;
                self.given.pkt1 = true;
            }
            // Cross-term dependence of kt1l
            "pkt1l" => {
                self.pkt1l = v;
                self.given.pkt1l = true;
            }
            // Cross-term dependence of kt2
            "pkt2" => {
                self.pkt2 = v;
                self.given.pkt2 = true;
            }
            // Cross-term dependence of k2
            "pk2" => {
                self.pk2 = v;
                self.given.pk2 = true;
            }
            // Cross-term dependence of k3
            "pk3" => {
                self.pk3 = v;
                self.given.pk3 = true;
            }
            // Cross-term dependence of k3b
            "pk3b" => {
                self.pk3b = v;
                self.given.pk3b = true;
            }
            // Cross-term dependence of w0
            "pw0" => {
                self.pw0 = v;
                self.given.pw0 = true;
            }
            // Cross-term dependence of dvtp0
            "pdvtp0" => {
                self.pdvtp0 = v;
                self.given.pdvtp0 = true;
            }
            // Cross-term dependence of dvtp1
            "pdvtp1" => {
                self.pdvtp1 = v;
                self.given.pdvtp1 = true;
            }
            // Cross-term dependence of dvtp2
            "pdvtp2" => {
                self.pdvtp2 = v;
                self.given.pdvtp2 = true;
            }
            // Cross-term dependence of dvtp3
            "pdvtp3" => {
                self.pdvtp3 = v;
                self.given.pdvtp3 = true;
            }
            // Cross-term dependence of dvtp4
            "pdvtp4" => {
                self.pdvtp4 = v;
                self.given.pdvtp4 = true;
            }
            // Cross-term dependence of dvtp5
            "pdvtp5" => {
                self.pdvtp5 = v;
                self.given.pdvtp5 = true;
            }
            // Cross-term dependence of lpe0
            "plpe0" => {
                self.plpe0 = v;
                self.given.plpe0 = true;
            }
            // Cross-term dependence of lpeb
            "plpeb" => {
                self.plpeb = v;
                self.given.plpeb = true;
            }
            // Cross-term dependence of dvt0
            "pdvt0" => {
                self.pdvt0 = v;
                self.given.pdvt0 = true;
            }
            // Cross-term dependence of dvt1
            "pdvt1" => {
                self.pdvt1 = v;
                self.given.pdvt1 = true;
            }
            // Cross-term dependence of dvt2
            "pdvt2" => {
                self.pdvt2 = v;
                self.given.pdvt2 = true;
            }
            // Cross-term dependence of dvt0w
            "pdvt0w" => {
                self.pdvt0w = v;
                self.given.pdvt0w = true;
            }
            // Cross-term dependence of dvt1w
            "pdvt1w" => {
                self.pdvt1w = v;
                self.given.pdvt1w = true;
            }
            // Cross-term dependence of dvt2w
            "pdvt2w" => {
                self.pdvt2w = v;
                self.given.pdvt2w = true;
            }
            // Cross-term dependence of drout
            "pdrout" => {
                self.pdrout = v;
                self.given.pdrout = true;
            }
            // Cross-term dependence of dsub
            "pdsub" => {
                self.pdsub = v;
                self.given.pdsub = true;
            }
            // Cross-term dependence of vto
            "pvth0" | "pvtho" => {
                self.pvth0 = v;
                self.given.pvth0 = true;
            }
            // Cross-term dependence of ua
            "pua" => {
                self.pua = v;
                self.given.pua = true;
            }
            // Cross-term dependence of ua1
            "pua1" => {
                self.pua1 = v;
                self.given.pua1 = true;
            }
            // Cross-term dependence of ub
            "pub" => {
                self.pub_ = v;
                self.given.pub_ = true;
            }
            // Cross-term dependence of ub1
            "pub1" => {
                self.pub1 = v;
                self.given.pub1 = true;
            }
            // Cross-term dependence of uc
            "puc" => {
                self.puc = v;
                self.given.puc = true;
            }
            // Cross-term dependence of uc1
            "puc1" => {
                self.puc1 = v;
                self.given.puc1 = true;
            }
            // Cross-term dependence of ud
            "pud" => {
                self.pud = v;
                self.given.pud = true;
            }
            // Cross-term dependence of ud1
            "pud1" => {
                self.pud1 = v;
                self.given.pud1 = true;
            }
            // Cross-term dependence of up
            "pup" => {
                self.pup = v;
                self.given.pup = true;
            }
            // Cross-term dependence of lp
            "plp" => {
                self.plp = v;
                self.given.plp = true;
            }
            // Cross-term dependence of u0
            "pu0" => {
                self.pu0 = v;
                self.given.pu0 = true;
            }
            // Cross-term dependence of ute
            "pute" => {
                self.pute = v;
                self.given.pute = true;
            }
            // Cross-term dependence of ucste
            "pucste" => {
                self.pucste = v;
                self.given.pucste = true;
            }
            // Cross-term dependence of voff
            "pvoff" => {
                self.pvoff = v;
                self.given.pvoff = true;
            }
            // Cross-term dependence of minv
            "pminv" => {
                self.pminv = v;
                self.given.pminv = true;
            }
            // Cross-term dependence of minvcv
            "pminvcv" => {
                self.pminvcv = v;
                self.given.pminvcv = true;
            }
            // Cross-term dependence of delta
            "pdelta" => {
                self.pdelta = v;
                self.given.pdelta = true;
            }
            // Cross-term dependence of rdsw
            "prdsw" => {
                self.prdsw = v;
                self.given.prdsw = true;
            }
            // Cross-term dependence of rsw
            "prsw" => {
                self.prsw = v;
                self.given.prsw = true;
            }
            // Cross-term dependence of rdw
            "prdw" => {
                self.prdw = v;
                self.given.prdw = true;
            }
            // Cross-term dependence of prwg
            "pprwg" => {
                self.pprwg = v;
                self.given.pprwg = true;
            }
            // Cross-term dependence of prwb
            "pprwb" => {
                self.pprwb = v;
                self.given.pprwb = true;
            }
            // Cross-term dependence of prt
            "pprt" => {
                self.pprt = v;
                self.given.pprt = true;
            }
            // Cross-term dependence of eta0
            "peta0" => {
                self.peta0 = v;
                self.given.peta0 = true;
            }
            // Cross-term dependence of etab
            "petab" => {
                self.petab = v;
                self.given.petab = true;
            }
            // Cross-term dependence of pclm
            "ppclm" => {
                self.ppclm = v;
                self.given.ppclm = true;
            }
            // Cross-term dependence of pdiblc1
            "ppdiblc1" => {
                self.ppdibl1 = v;
                self.given.ppdibl1 = true;
            }
            // Cross-term dependence of pdiblc2
            "ppdiblc2" => {
                self.ppdibl2 = v;
                self.given.ppdibl2 = true;
            }
            // Cross-term dependence of pdiblcb
            "ppdiblcb" => {
                self.ppdiblb = v;
                self.given.ppdiblb = true;
            }
            // Cross-term dependence of pdiblcb
            "pfprout" => {
                self.pfprout = v;
                self.given.pfprout = true;
            }
            // Cross-term dependence of pdits
            "ppdits" => {
                self.ppdits = v;
                self.given.ppdits = true;
            }
            // Cross-term dependence of pditsd
            "ppditsd" => {
                self.ppditsd = v;
                self.given.ppditsd = true;
            }
            // Cross-term dependence of pscbe1
            "ppscbe1" => {
                self.ppscbe1 = v;
                self.given.ppscbe1 = true;
            }
            // Cross-term dependence of pscbe2
            "ppscbe2" => {
                self.ppscbe2 = v;
                self.given.ppscbe2 = true;
            }
            // Cross-term dependence of pvag
            "ppvag" => {
                self.ppvag = v;
                self.given.ppvag = true;
            }
            // Cross-term dependence of wr
            "pwr" => {
                self.pwr = v;
                self.given.pwr = true;
            }
            // Cross-term dependence of dwg
            "pdwg" => {
                self.pdwg = v;
                self.given.pdwg = true;
            }
            // Cross-term dependence of dwb
            "pdwb" => {
                self.pdwb = v;
                self.given.pdwb = true;
            }
            // Cross-term dependence of b0
            "pb0" => {
                self.pb0 = v;
                self.given.pb0 = true;
            }
            // Cross-term dependence of b1
            "pb1" => {
                self.pb1 = v;
                self.given.pb1 = true;
            }
            // Cross-term dependence of cgsl
            "pcgsl" => {
                self.pcgsl = v;
                self.given.pcgsl = true;
            }
            // Cross-term dependence of cgdl
            "pcgdl" => {
                self.pcgdl = v;
                self.given.pcgdl = true;
            }
            // Cross-term dependence of ckappas
            "pckappas" => {
                self.pckappas = v;
                self.given.pckappas = true;
            }
            // Cross-term dependence of ckappad
            "pckappad" => {
                self.pckappad = v;
                self.given.pckappad = true;
            }
            // Cross-term dependence of cf
            "pcf" => {
                self.pcf = v;
                self.given.pcf = true;
            }
            // Cross-term dependence of clc
            "pclc" => {
                self.pclc = v;
                self.given.pclc = true;
            }
            // Cross-term dependence of cle
            "pcle" => {
                self.pcle = v;
                self.given.pcle = true;
            }
            // Cross-term dependence of alpha0
            "palpha0" => {
                self.palpha0 = v;
                self.given.palpha0 = true;
            }
            // Cross-term dependence of alpha1
            "palpha1" => {
                self.palpha1 = v;
                self.given.palpha1 = true;
            }
            // Cross-term dependence of beta0
            "pbeta0" => {
                self.pbeta0 = v;
                self.given.pbeta0 = true;
            }
            // Cross-term dependence of agidl
            "pagidl" => {
                self.pagidl = v;
                self.given.pagidl = true;
            }
            // Cross-term dependence of bgidl
            "pbgidl" => {
                self.pbgidl = v;
                self.given.pbgidl = true;
            }
            // Cross-term dependence of cgidl
            "pcgidl" => {
                self.pcgidl = v;
                self.given.pcgidl = true;
            }
            // Cross-term dependence of rgidl
            "prgidl" => {
                self.prgidl = v;
                self.given.prgidl = true;
            }
            // Cross-term dependence of kgidl
            "pkgidl" => {
                self.pkgidl = v;
                self.given.pkgidl = true;
            }
            // Cross-term dependence of fgidl
            "pfgidl" => {
                self.pfgidl = v;
                self.given.pfgidl = true;
            }
            // Cross-term dependence of egidl
            "pegidl" => {
                self.pegidl = v;
                self.given.pegidl = true;
            }
            // Cross-term dependence of agisl
            "pagisl" => {
                self.pagisl = v;
                self.given.pagisl = true;
            }
            // Cross-term dependence of bgisl
            "pbgisl" => {
                self.pbgisl = v;
                self.given.pbgisl = true;
            }
            // Cross-term dependence of cgisl
            "pcgisl" => {
                self.pcgisl = v;
                self.given.pcgisl = true;
            }
            // Cross-term dependence of egisl
            "pegisl" => {
                self.pegisl = v;
                self.given.pegisl = true;
            }
            // Cross-term dependence of rgisl
            "prgisl" => {
                self.prgisl = v;
                self.given.prgisl = true;
            }
            // Cross-term dependence of kgisl
            "pkgisl" => {
                self.pkgisl = v;
                self.given.pkgisl = true;
            }
            // Cross-term dependence of fgisl
            "pfgisl" => {
                self.pfgisl = v;
                self.given.pfgisl = true;
            }
            // Cross-term dependence of aigc
            "paigc" => {
                self.paigc = v;
                self.given.paigc = true;
            }
            // Cross-term dependence of bigc
            "pbigc" => {
                self.pbigc = v;
                self.given.pbigc = true;
            }
            // Cross-term dependence of cigc
            "pcigc" => {
                self.pcigc = v;
                self.given.pcigc = true;
            }
            // Cross-term dependence of aigsd
            "paigsd" => {
                self.paigsd = v;
                self.given.paigsd = true;
            }
            // Cross-term dependence of bigsd
            "pbigsd" => {
                self.pbigsd = v;
                self.given.pbigsd = true;
            }
            // Cross-term dependence of cigsd
            "pcigsd" => {
                self.pcigsd = v;
                self.given.pcigsd = true;
            }
            // Cross-term dependence of aigs
            "paigs" => {
                self.paigs = v;
                self.given.paigs = true;
            }
            // Cross-term dependence of bigs
            "pbigs" => {
                self.pbigs = v;
                self.given.pbigs = true;
            }
            // Cross-term dependence of cigs
            "pcigs" => {
                self.pcigs = v;
                self.given.pcigs = true;
            }
            // Cross-term dependence of aigd
            "paigd" => {
                self.paigd = v;
                self.given.paigd = true;
            }
            // Cross-term dependence of bigd
            "pbigd" => {
                self.pbigd = v;
                self.given.pbigd = true;
            }
            // Cross-term dependence of cigd
            "pcigd" => {
                self.pcigd = v;
                self.given.pcigd = true;
            }
            // Cross-term dependence of aigbacc
            "paigbacc" => {
                self.paigbacc = v;
                self.given.paigbacc = true;
            }
            // Cross-term dependence of bigbacc
            "pbigbacc" => {
                self.pbigbacc = v;
                self.given.pbigbacc = true;
            }
            // Cross-term dependence of cigbacc
            "pcigbacc" => {
                self.pcigbacc = v;
                self.given.pcigbacc = true;
            }
            // Cross-term dependence of aigbinv
            "paigbinv" => {
                self.paigbinv = v;
                self.given.paigbinv = true;
            }
            // Cross-term dependence of bigbinv
            "pbigbinv" => {
                self.pbigbinv = v;
                self.given.pbigbinv = true;
            }
            // Cross-term dependence of cigbinv
            "pcigbinv" => {
                self.pcigbinv = v;
                self.given.pcigbinv = true;
            }
            // Cross-term dependence of nigc
            "pnigc" => {
                self.pnigc = v;
                self.given.pnigc = true;
            }
            // Cross-term dependence of nigbinv
            "pnigbinv" => {
                self.pnigbinv = v;
                self.given.pnigbinv = true;
            }
            // Cross-term dependence of nigbacc
            "pnigbacc" => {
                self.pnigbacc = v;
                self.given.pnigbacc = true;
            }
            // Cross-term dependence of ntox
            "pntox" => {
                self.pntox = v;
                self.given.pntox = true;
            }
            // Cross-term dependence for eigbinv
            "peigbinv" => {
                self.peigbinv = v;
                self.given.peigbinv = true;
            }
            // Cross-term dependence for pigcd
            "ppigcd" => {
                self.ppigcd = v;
                self.given.ppigcd = true;
            }
            // Cross-term dependence for poxedge
            "ppoxedge" => {
                self.ppoxedge = v;
                self.given.ppoxedge = true;
            }
            // Cross-term dependence of vfbcv
            "pvfbcv" => {
                self.pvfbcv = v;
                self.given.pvfbcv = true;
            }
            // Cross-term dependence of vfb
            "pvfb" => {
                self.pvfb = v;
                self.given.pvfb = true;
            }
            // Cross-term dependence of acde
            "pacde" => {
                self.pacde = v;
                self.given.pacde = true;
            }
            // Cross-term dependence of moin
            "pmoin" => {
                self.pmoin = v;
                self.given.pmoin = true;
            }
            // Cross-term dependence of noff
            "pnoff" => {
                self.pnoff = v;
                self.given.pnoff = true;
            }
            // Cross-term dependence of voffcv
            "pvoffcv" => {
                self.pvoffcv = v;
                self.given.pvoffcv = true;
            }
            // Cross-term dependence of xrcrg1
            "pxrcrg1" => {
                self.pxrcrg1 = v;
                self.given.pxrcrg1 = true;
            }
            // Cross-term dependence of xrcrg2
            "pxrcrg2" => {
                self.pxrcrg2 = v;
                self.given.pxrcrg2 = true;
            }
            // Cross-term dependence of lambda
            "plambda" => {
                self.plambda = v;
                self.given.plambda = true;
            }
            // Cross-term dependence of vtl
            "pvtl" => {
                self.pvtl = v;
                self.given.pvtl = true;
            }
            // Cross-term dependence of xn
            "pxn" => {
                self.pxn = v;
                self.given.pxn = true;
            }
            // Cross-term dependence of eu
            "peu" => {
                self.peu = v;
                self.given.peu = true;
            }
            // Cross-term dependence of ucs
            "pucs" => {
                self.pucs = v;
                self.given.pucs = true;
            }
            // Cross-term dependence of vfbsdoff
            "pvfbsdoff" => {
                self.pvfbsdoff = v;
                self.given.pvfbsdoff = true;
            }
            // Cross-term dependence of tvfbsdoff
            "ptvfbsdoff" => {
                self.ptvfbsdoff = v;
                self.given.ptvfbsdoff = true;
            }
            // Cross-term dependence of tvoff
            "ptvoff" => {
                self.ptvoff = v;
                self.given.ptvoff = true;
            }
            // Cross-term dependence of tnfactor
            "ptnfactor" => {
                self.ptnfactor = v;
                self.given.ptnfactor = true;
            }
            // Cross-term dependence of teta0
            "pteta0" => {
                self.pteta0 = v;
                self.given.pteta0 = true;
            }
            // Cross-term dependence of tvoffcv
            "ptvoffcv" => {
                self.ptvoffcv = v;
                self.given.ptvoffcv = true;
            }
            // Reference distance between OD edge to poly of one side
            "saref" => {
                self.saref = v;
                self.given.saref = true;
            }
            // Reference distance between OD edge to poly of the other side
            "sbref" => {
                self.sbref = v;
                self.given.sbref = true;
            }
            // Width parameter for stress effect
            "wlod" => {
                self.wlod = v;
                self.given.wlod = true;
            }
            // Mobility degradation/enhancement coefficient for LOD
            "ku0" => {
                self.ku0 = v;
                self.given.ku0 = true;
            }
            // Saturation velocity degradation/enhancement parameter for LOD
            "kvsat" => {
                self.kvsat = v;
                self.given.kvsat = true;
            }
            // Threshold degradation/enhancement parameter for LOD
            "kvth0" => {
                self.kvth0 = v;
                self.given.kvth0 = true;
            }
            // Temperature coefficient of KU0
            "tku0" => {
                self.tku0 = v;
                self.given.tku0 = true;
            }
            // Length parameter for u0 LOD effect
            "llodku0" => {
                self.llodku0 = v;
                self.given.llodku0 = true;
            }
            // Width parameter for u0 LOD effect
            "wlodku0" => {
                self.wlodku0 = v;
                self.given.wlodku0 = true;
            }
            // Length parameter for vth LOD effect
            "llodvth" => {
                self.llodvth = v;
                self.given.llodvth = true;
            }
            // Width parameter for vth LOD effect
            "wlodvth" => {
                self.wlodvth = v;
                self.given.wlodvth = true;
            }
            // Length dependence of ku0
            "lku0" => {
                self.lku0 = v;
                self.given.lku0 = true;
            }
            // Width dependence of ku0
            "wku0" => {
                self.wku0 = v;
                self.given.wku0 = true;
            }
            // Cross-term dependence of ku0
            "pku0" => {
                self.pku0 = v;
                self.given.pku0 = true;
            }
            // Length dependence of kvth0
            "lkvth0" => {
                self.lkvth0 = v;
                self.given.lkvth0 = true;
            }
            // Width dependence of kvth0
            "wkvth0" => {
                self.wkvth0 = v;
                self.given.wkvth0 = true;
            }
            // Cross-term dependence of kvth0
            "pkvth0" => {
                self.pkvth0 = v;
                self.given.pkvth0 = true;
            }
            // K2 shift factor related to stress effect on vth
            "stk2" => {
                self.stk2 = v;
                self.given.stk2 = true;
            }
            // K2 shift modification factor for stress effect
            "lodk2" => {
                self.lodk2 = v;
                self.given.lodk2 = true;
            }
            // eta0 shift factor related to stress effect on vth
            "steta0" => {
                self.steta0 = v;
                self.given.steta0 = true;
            }
            // eta0 shift modification factor for stress effect
            "lodeta0" => {
                self.lodeta0 = v;
                self.given.lodeta0 = true;
            }
            // Coefficient for SCB
            "web" => {
                self.web = v;
                self.given.web = true;
            }
            // Coefficient for SCC
            "wec" => {
                self.wec = v;
                self.given.wec = true;
            }
            // Threshold shift factor for well proximity effect
            "kvth0we" => {
                self.kvth0we = v;
                self.given.kvth0we = true;
            }
            // K2 shift factor for well proximity effect
            "k2we" => {
                self.k2we = v;
                self.given.k2we = true;
            }
            // Mobility degradation factor for well proximity effect
            "ku0we" => {
                self.ku0we = v;
                self.given.ku0we = true;
            }
            // Reference distance to calculate SCA, SCB and SCC
            "scref" => {
                self.scref = v;
                self.given.scref = true;
            }
            // Flag for WPE model (WPEMOD=1 to activate this model)
            "wpemod" => {
                self.wpemod = v;
                self.given.wpemod = true;
            }
            // Length dependence of kvth0we
            "lkvth0we" => {
                self.lkvth0we = v;
                self.given.lkvth0we = true;
            }
            // Length dependence of k2we
            "lk2we" => {
                self.lk2we = v;
                self.given.lk2we = true;
            }
            // Length dependence of ku0we
            "lku0we" => {
                self.lku0we = v;
                self.given.lku0we = true;
            }
            // Width dependence of kvth0we
            "wkvth0we" => {
                self.wkvth0we = v;
                self.given.wkvth0we = true;
            }
            // Width dependence of k2we
            "wk2we" => {
                self.wk2we = v;
                self.given.wk2we = true;
            }
            // Width dependence of ku0we
            "wku0we" => {
                self.wku0we = v;
                self.given.wku0we = true;
            }
            // Cross-term dependence of kvth0we
            "pkvth0we" => {
                self.pkvth0we = v;
                self.given.pkvth0we = true;
            }
            // Cross-term dependence of k2we
            "pk2we" => {
                self.pk2we = v;
                self.given.pk2we = true;
            }
            // Cross-term dependence of ku0we
            "pku0we" => {
                self.pku0we = v;
                self.given.pku0we = true;
            }
            // Flicker noise parameter
            "noia" => {
                self.oxide_trap_density_a = v;
                self.given.oxide_trap_density_a = true;
            }
            // Flicker noise parameter
            "noib" => {
                self.oxide_trap_density_b = v;
                self.given.oxide_trap_density_b = true;
            }
            // Flicker noise parameter
            "noic" => {
                self.oxide_trap_density_c = v;
                self.given.oxide_trap_density_c = true;
            }
            // Thermal noise parameter
            "tnoia" => {
                self.tnoia = v;
                self.given.tnoia = true;
            }
            // Thermal noise parameter
            "tnoib" => {
                self.tnoib = v;
                self.given.tnoib = true;
            }
            // Thermal noise parameter
            "tnoic" => {
                self.tnoic = v;
                self.given.tnoic = true;
            }
            // Thermal noise coefficient
            "rnoia" => {
                self.rnoia = v;
                self.given.rnoia = true;
            }
            // Thermal noise coefficient
            "rnoib" => {
                self.rnoib = v;
                self.given.rnoib = true;
            }
            // Thermal noise coefficient
            "rnoic" => {
                self.rnoic = v;
                self.given.rnoic = true;
            }
            // gidl clamp value
            "gidlclamp" => {
                self.gidlclamp = v;
                self.given.gidlclamp = true;
            }
            // noise clamping limit parameter
            "idovvds" => {
                self.idovvdsc = v;
                self.given.idovvdsc = true;
            }
            // Thermal noise parameter
            "ntnoi" => {
                self.ntnoi = v;
                self.given.ntnoi = true;
            }
            // Flicker noise parameter
            "em" => {
                self.em = v;
                self.given.em = true;
            }
            // Flicker noise frequency exponent
            "ef" => {
                self.ef = v;
                self.given.ef = true;
            }
            // Flicker noise exponent
            "af" => {
                self.af = v;
                self.given.af = true;
            }
            // Flicker noise coefficient
            "kf" => {
                self.kf = v;
                self.given.kf = true;
            }
            // maximum voltage G-S branch
            "vgs_max" => {
                self.vgs_max = v;
                self.given.vgs_max = true;
            }
            // maximum voltage G-D branch
            "vgd_max" => {
                self.vgd_max = v;
                self.given.vgd_max = true;
            }
            // maximum voltage G-B branch
            "vgb_max" => {
                self.vgb_max = v;
                self.given.vgb_max = true;
            }
            // maximum voltage D-S branch
            "vds_max" => {
                self.vds_max = v;
                self.given.vds_max = true;
            }
            // maximum voltage B-S branch
            "vbs_max" => {
                self.vbs_max = v;
                self.given.vbs_max = true;
            }
            // maximum voltage B-D branch
            "vbd_max" => {
                self.vbd_max = v;
                self.given.vbd_max = true;
            }
            // maximum voltage G-S branch
            "vgsr_max" => {
                self.vgsr_max = v;
                self.given.vgsr_max = true;
            }
            // maximum voltage G-D branch
            "vgdr_max" => {
                self.vgdr_max = v;
                self.given.vgdr_max = true;
            }
            // maximum voltage G-B branch
            "vgbr_max" => {
                self.vgbr_max = v;
                self.given.vgbr_max = true;
            }
            // maximum voltage B-S branch
            "vbsr_max" => {
                self.vbsr_max = v;
                self.given.vbsr_max = true;
            }
            // maximum voltage B-D branch
            "vbdr_max" => {
                self.vbdr_max = v;
                self.given.vbdr_max = true;
            }
            _ => return false,
        }
        true
    }
}

impl Bsim4Here {
    /// `BSIM4param` (`b4par.c`) for every numeric keyword of
    /// `BSIM4pTable` (`b4.c`) but the `IC` vector; `scale` is 1 (Dune
    /// has no `.option scale`). Returns `false` for an unknown keyword.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn set_param(&mut self, kw: &str, v: f64) -> bool {
        let iv = (0.5 + v).floor() as i32;
        let _ = iv;
        match kw {
            // Length
            "l" => {
                self.l = v;
                self.given.l = true;
            }
            // Width
            "w" => {
                self.w = v;
                self.given.w = true;
            }
            // Separate Parallel multiplier
            "m" => {
                self.m = v;
                self.given.m = true;
            }
            // Number of fingers
            "nf" => {
                self.nf = v;
                self.given.nf = true;
            }
            // distance between  OD edge to poly of one side
            "sa" => {
                self.sa = v;
                self.given.sa = true;
            }
            // distance between  OD edge to poly of the other side
            "sb" => {
                self.sb = v;
                self.given.sb = true;
            }
            // distance between neighbour fingers
            "sd" => {
                self.sd = v;
                self.given.sd = true;
            }
            // Integral of the first distribution function for scattered well dopant
            "sca" => {
                self.sca = v;
                self.given.sca = true;
            }
            // Integral of the second distribution function for scattered well dopant
            "scb" => {
                self.scb = v;
                self.given.scb = true;
            }
            // Integral of the third distribution function for scattered well dopant
            "scc" => {
                self.scc = v;
                self.given.scc = true;
            }
            // Distance to a single well edge
            "sc" => {
                self.sc = v;
                self.given.sc = true;
            }
            // Minimize either D or S
            "min" => {
                self.min = iv;
                self.given.min = true;
            }
            // Drain area
            "ad" => {
                self.drain_area = v;
                self.given.drain_area = true;
            }
            // Source area
            "as" => {
                self.source_area = v;
                self.given.source_area = true;
            }
            // Drain perimeter
            "pd" => {
                self.drain_perimeter = v;
                self.given.drain_perimeter = true;
            }
            // Source perimeter
            "ps" => {
                self.source_perimeter = v;
                self.given.source_perimeter = true;
            }
            // Number of squares in drain
            "nrd" => {
                self.drain_squares = v;
                self.given.drain_squares = true;
            }
            // Number of squares in source
            "nrs" => {
                self.source_squares = v;
                self.given.source_squares = true;
            }
            // Device is initially off
            "off" => {
                self.off = iv;
            }
            // Body resistance
            "rbdb" => {
                self.rbdb = v;
                self.given.rbdb = true;
            }
            // Body resistance
            "rbsb" => {
                self.rbsb = v;
                self.given.rbsb = true;
            }
            // Body resistance
            "rbpb" => {
                self.rbpb = v;
                self.given.rbpb = true;
            }
            // Body resistance
            "rbps" => {
                self.rbps = v;
                self.given.rbps = true;
            }
            // Body resistance
            "rbpd" => {
                self.rbpd = v;
                self.given.rbpd = true;
            }
            // Zero bias threshold voltage variation
            "delvto" | "delvt0" => {
                self.delvto = v;
                self.given.delvto = true;
            }
            // Low field mobility multiplier
            "mulu0" => {
                self.mulu0 = v;
                self.given.mulu0 = true;
            }
            // Distance from gate contact center to device edge
            "xgw" => {
                self.xgw = v;
                self.given.xgw = true;
            }
            // Number of gate contacts
            "ngcon" => {
                self.ngcon = v;
                self.given.ngcon = true;
            }
            // W/NF device flag for bin selection
            "wnflag" => {
                self.wnflag = iv;
                self.given.wnflag = true;
            }
            // Instance temperature difference
            "dtemp" => {
                self.dtemp = v;
                self.given.dtemp = true;
            }
            // Transient NQS model selector
            "trnqsmod" => {
                self.trnqs_mod = iv;
                self.given.trnqs_mod = true;
            }
            // AC NQS model selector
            "acnqsmod" => {
                self.acnqs_mod = iv;
                self.given.acnqs_mod = true;
            }
            // Distributed body R model selector
            "rbodymod" => {
                self.rbody_mod = iv;
                self.given.rbody_mod = true;
            }
            // Gate resistance model selector
            "rgatemod" => {
                self.rgate_mod = iv;
                self.given.rgate_mod = true;
            }
            // Geometry dependent parasitics model selector
            "geomod" => {
                self.geo_mod = iv;
                self.given.geo_mod = true;
            }
            // S/D resistance and contact model selector
            "rgeomod" => {
                self.rgeo_mod = iv;
                self.given.rgeo_mod = true;
            }
            // Variability in current
            "mult_i" => {
                self.mult_i = v;
                self.given.mult_i = true;
            }
            // Variability in charge
            "mult_q" => {
                self.mult_q = v;
                self.given.mult_q = true;
            }
            // Variability in flicker noise
            "mult_fn" => {
                self.mult_fn = v;
                self.given.mult_fn = true;
            }
            // GS initial voltage
            "icvgs" => {
                self.ic_vgs = v;
                self.given.ic_vgs = true;
            }
            // DS initial voltage
            "icvds" => {
                self.ic_vds = v;
                self.given.ic_vds = true;
            }
            // BS initial voltage
            "icvbs" => {
                self.ic_vbs = v;
                self.given.ic_vbs = true;
            }
            _ => return false,
        }
        true
    }
}

/// Instance keywords of `BSIM4pTable` (`b4.c`) that take a value.
pub(crate) const INST_KEYWORDS: &[&str] = &[
    "l", "w", "m", "nf", "sa", "sb", "sd", "sca", "scb", "scc", "sc", "min", "ad", "as", "pd",
    "ps", "nrd", "nrs", "rbdb", "rbsb", "rbpb", "rbps", "rbpd", "delvto", "delvt0", "mulu0", "xgw",
    "ngcon", "wnflag", "dtemp", "trnqsmod", "acnqsmod", "rbodymod", "rgatemod", "geomod",
    "rgeomod", "mult_i", "mult_q", "mult_fn", "icvgs", "icvds", "icvbs",
];
