// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4check.c as distributed with ngspice-47
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

//! `BSIM4checkModel` (`b4check.c`): the parameter sanity checks run for a
//! model's first instance. Warnings and fatal messages go to `notes`
//! (the C writes them to stderr and `bsim4.out`); the return value is
//! the C's `Fatal_Flag`. Like the C, some checks clamp parameters.

use super::*;

#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn check_model(
    md: &mut Bsim4Model,
    h: &mut Bsim4Here,
    pp: &mut Bsim4Size,
    ck_temp: f64,
    notes: &mut Vec<String>,
) -> i32 {
    let mut fatal_flag: i32 = 0;
    fatal_flag = 0;
    if (h.rgate_mod == 2 || h.rgate_mod == 3) && (h.trnqs_mod == 1 || h.acnqs_mod == 1) {
        notes.push(
            "Warning: You've selected both Rg and charge deficit NQS; select one only.".to_string(),
        );
    }
    if md.toxe <= 0.0 {
        notes.push(format!("Fatal: Toxe = {} is not positive.", md.toxe));
        fatal_flag = 1;
    }
    if h.toxp <= 0.0 {
        notes.push(format!("Fatal: Toxp = {} is not positive.", h.toxp));
        fatal_flag = 1;
    }
    if md.eot <= 0.0 {
        notes.push(format!("Fatal: EOT = {} is not positive.", md.eot));
        fatal_flag = 1;
    }
    if md.tempeot <= 0.0 {
        notes.push(format!("Fatal: TEMPEOT = {} is not positive.", md.tempeot));
        fatal_flag = 1;
    }
    if md.epsrgate < 0.0 {
        notes.push(format!(
            "Fatal: Epsrgate = {} is not positive.",
            md.epsrgate
        ));
        fatal_flag = 1;
    }
    if md.epsrsub < 0.0 {
        notes.push(format!("Fatal: Epsrsub = {} is not positive.", md.epsrsub));
        fatal_flag = 1;
    }
    if md.easub < 0.0 {
        notes.push(format!("Fatal: Easub = {} is not positive.", md.easub));
        fatal_flag = 1;
    }
    if md.ni0sub <= 0.0 {
        notes.push(format!("Fatal: Easub = {} is not positive.", md.ni0sub));
        fatal_flag = 1;
    }
    if md.toxm <= 0.0 {
        notes.push(format!("Fatal: Toxm = {} is not positive.", md.toxm));
        fatal_flag = 1;
    }
    if md.toxref <= 0.0 {
        notes.push(format!("Fatal: Toxref = {} is not positive.", md.toxref));
        fatal_flag = 1;
    }
    if pp.lpe0 < -pp.leff {
        notes.push(format!("Fatal: Lpe0 = {} is less than -Leff.", pp.lpe0));
        fatal_flag = 1;
    }
    if md.lintnoi > pp.leff / 2.0 {
        notes.push(format!(
            "Fatal: Lintnoi = {} is too large - Leff for noise is negative.",
            md.lintnoi
        ));
        fatal_flag = 1;
    }
    if pp.lpeb < -pp.leff {
        notes.push(format!("Fatal: Lpeb = {} is less than -Leff.", pp.lpeb));
        fatal_flag = 1;
    }
    if pp.ndep <= 0.0 {
        notes.push(format!("Fatal: Ndep = {} is not positive.", pp.ndep));
        fatal_flag = 1;
    }
    if pp.phi <= 0.0 {
        notes.push(format!(
            "Fatal: Phi = {} is not positive. Please check Phin and Ndep",
            pp.phi
        ));
        notes.push(format!("Phin = {}  Ndep = {}", pp.phin, pp.ndep));
        fatal_flag = 1;
    }
    if pp.nsub <= 0.0 {
        notes.push(format!("Fatal: Nsub = {} is not positive.", pp.nsub));
        fatal_flag = 1;
    }
    if pp.ngate < 0.0 {
        notes.push(format!(
            "Fatal: Ngate = {} Ngate is not positive.",
            pp.ngate
        ));
        fatal_flag = 1;
    }
    if pp.ngate > 1.0e25 {
        notes.push(format!("Fatal: Ngate = {} Ngate is too high", pp.ngate));
        fatal_flag = 1;
    }
    if pp.xj <= 0.0 {
        notes.push(format!("Fatal: Xj = {} is not positive.", pp.xj));
        fatal_flag = 1;
    }
    if pp.dvt1 < 0.0 {
        notes.push(format!("Fatal: Dvt1 = {} is negative.", pp.dvt1));
        fatal_flag = 1;
    }
    if pp.dvt1w < 0.0 {
        notes.push(format!("Fatal: Dvt1w = {} is negative.", pp.dvt1w));
        fatal_flag = 1;
    }
    if pp.w0 == -pp.weff {
        notes.push("Fatal: (W0 + Weff) = 0 causing divided-by-zero.".to_string());
        fatal_flag = 1;
    }
    if pp.dsub < 0.0 {
        notes.push(format!("Fatal: Dsub = {} is negative.", pp.dsub));
        fatal_flag = 1;
    }
    if pp.b1 == -pp.weff {
        notes.push("Fatal: (B1 + Weff) = 0 causing divided-by-zero.".to_string());
        fatal_flag = 1;
    }
    if h.u0temp <= 0.0 {
        notes.push(format!(
            "Fatal: u0 at current temperature = {} is not positive.",
            h.u0temp
        ));
        fatal_flag = 1;
    }
    if pp.delta < 0.0 {
        notes.push(format!("Fatal: Delta = {} is less than zero.", pp.delta));
        fatal_flag = 1;
    }
    if h.vsattemp <= 0.0 {
        notes.push(format!(
            "Fatal: Vsat at current temperature = {} is not positive.",
            h.vsattemp
        ));
        fatal_flag = 1;
    }
    if pp.pclm <= 0.0 {
        notes.push(format!("Fatal: Pclm = {} is not positive.", pp.pclm));
        fatal_flag = 1;
    }
    if pp.drout < 0.0 {
        notes.push(format!("Fatal: Drout = {} is negative.", pp.drout));
        fatal_flag = 1;
    }
    if h.m <= 0.0 {
        notes.push(format!("Fatal: multiplier = {} is not positive.", h.m));
        fatal_flag = 1;
    }
    if h.nf < 1.0 {
        notes.push(format!(
            "Fatal: Number of finger = {} is smaller than one.",
            h.nf
        ));
        fatal_flag = 1;
    }
    if h.sa > 0.0 && h.sb > 0.0 && (h.nf == 1.0 || h.nf > 1.0 && h.sd > 0.0) {
        if md.saref <= 0.0 {
            notes.push(format!("Fatal: SAref = {} is not positive.", md.saref));
            fatal_flag = 1;
        }
        if md.sbref <= 0.0 {
            notes.push(format!("Fatal: SBref = {} is not positive.", md.sbref));
            fatal_flag = 1;
        }
    }
    if h.l + md.xl <= md.xgl {
        notes.push("Fatal: The parameter xgl must be smaller than Ldrawn+XL.".to_string());
        fatal_flag = 1;
    }
    if h.ngcon < 1.0 {
        notes.push("Fatal: The parameter ngcon cannot be smaller than one.".to_string());
        fatal_flag = 1;
    }
    if h.ngcon != 1.0 && h.ngcon != 2.0 {
        h.ngcon = 1.0;
        notes.push("Warning: Ngcon must be equal to one or two; reset to 1.0.".to_string());
    }
    if md.gbmin < 1.0e-20 {
        notes.push(format!("Warning: Gbmin = {} is too small.", md.gbmin));
    }
    if pp.fprout < 0.0 {
        notes.push(format!("Fatal: fprout = {} is negative.", pp.fprout));
        fatal_flag = 1;
    }
    if pp.pdits < 0.0 {
        notes.push(format!("Fatal: pdits = {} is negative.", pp.pdits));
        fatal_flag = 1;
    }
    if md.pditsl < 0.0 {
        notes.push(format!("Fatal: pditsl = {} is negative.", md.pditsl));
        fatal_flag = 1;
    }
    if md.igb_mod != 0 {
        if pp.nigbinv <= 0.0 {
            notes.push(format!("Fatal: nigbinv = {} is non-positive.", pp.nigbinv));
            fatal_flag = 1;
        }
        if pp.nigbacc <= 0.0 {
            notes.push(format!("Fatal: nigbacc = {} is non-positive.", pp.nigbacc));
            fatal_flag = 1;
        }
    }
    if md.igc_mod != 0 {
        if pp.nigc <= 0.0 {
            notes.push(format!("Fatal: nigc = {} is non-positive.", pp.nigc));
            fatal_flag = 1;
        }
        if pp.poxedge <= 0.0 {
            notes.push(format!("Fatal: poxedge = {} is non-positive.", pp.poxedge));
            fatal_flag = 1;
        }
        if pp.pigcd <= 0.0 {
            notes.push(format!("Fatal: pigcd = {} is non-positive.", pp.pigcd));
            fatal_flag = 1;
        }
    }
    if pp.clc < 0.0 {
        notes.push(format!("Fatal: Clc = {} is negative.", pp.clc));
        fatal_flag = 1;
    }
    if pp.ckappas < 0.02 {
        notes.push(format!("Warning: ckappas = {} is too small.", pp.ckappas));
        pp.ckappas = 0.02;
    }
    if pp.ckappad < 0.02 {
        notes.push(format!("Warning: ckappad = {} is too small.", pp.ckappad));
        pp.ckappad = 0.02;
    }
    if md.vtss < 0.0 {
        notes.push(format!("Fatal: Vtss = {} is negative.", md.vtss));
        fatal_flag = 1;
    }
    if md.vtsd < 0.0 {
        notes.push(format!("Fatal: Vtsd = {} is negative.", md.vtsd));
        fatal_flag = 1;
    }
    if md.vtssws < 0.0 {
        notes.push(format!("Fatal: Vtssws = {} is negative.", md.vtssws));
        fatal_flag = 1;
    }
    if md.vtsswd < 0.0 {
        notes.push(format!("Fatal: Vtsswd = {} is negative.", md.vtsswd));
        fatal_flag = 1;
    }
    if md.vtsswgs < 0.0 {
        notes.push(format!("Fatal: Vtsswgs = {} is negative.", md.vtsswgs));
        fatal_flag = 1;
    }
    if md.vtsswgd < 0.0 {
        notes.push(format!("Fatal: Vtsswgd = {} is negative.", md.vtsswgd));
        fatal_flag = 1;
    }
    if md.param_chk == 1 {
        if pp.leff <= 1.0e-9 {
            notes.push(format!(
                "Warning: Leff = {} <= 1.0e-9. Recommended Leff >= 1e-8",
                pp.leff
            ));
        }
        if pp.leff_cv <= 1.0e-9 {
            notes.push(format!(
                "Warning: Leff for CV = {} <= 1.0e-9. Recommended LeffCV >=1e-8",
                pp.leff_cv
            ));
        }
        if pp.weff <= 1.0e-9 {
            notes.push(format!(
                "Warning: Weff = {} <= 1.0e-9. Recommended Weff >=1e-7",
                pp.weff
            ));
        }
        if pp.weff_cv <= 1.0e-9 {
            notes.push(format!(
                "Warning: Weff for CV = {} <= 1.0e-9. Recommended WeffCV >= 1e-7",
                pp.weff_cv
            ));
        }
        if md.toxe < 1.0e-10 {
            notes.push(format!(
                "Warning: Toxe = {} is less than 1A. Recommended Toxe >= 5A",
                md.toxe
            ));
        }
        if h.toxp < 1.0e-10 {
            notes.push(format!(
                "Warning: Toxp = {} is less than 1A. Recommended Toxp >= 5A",
                h.toxp
            ));
        }
        if md.toxm < 1.0e-10 {
            notes.push(format!(
                "Warning: Toxm = {} is less than 1A. Recommended Toxm >= 5A",
                md.toxm
            ));
        }
        if pp.ndep <= 1.0e12 {
            notes.push(format!("Warning: Ndep = {} may be too small.", pp.ndep));
        } else if pp.ndep >= 1.0e21 {
            notes.push(format!("Warning: Ndep = {} may be too large.", pp.ndep));
        }
        if pp.nsub <= 1.0e14 {
            notes.push(format!("Warning: Nsub = {} may be too small.", pp.nsub));
        } else if pp.nsub >= 1.0e21 {
            notes.push(format!("Warning: Nsub = {} may be too large.", pp.nsub));
        }
        if pp.ngate > 0.0 && pp.ngate <= 1.0e18 {
            notes.push(format!(
                "Warning: Ngate = {} is less than 1.E18cm^-3.",
                pp.ngate
            ));
        }
        if pp.dvt0 < 0.0 {
            notes.push(format!("Warning: Dvt0 = {} is negative.", pp.dvt0));
        }
        if fabs(1.0e-8 / (pp.w0 + pp.weff)) > 10.0 {
            notes.push("Warning: (W0 + Weff) may be too small.".to_string());
        }
        if pp.nfactor < 0.0 {
            notes.push(format!("Warning: Nfactor = {} is negative.", pp.nfactor));
        }
        if pp.cdsc < 0.0 {
            notes.push(format!("Warning: Cdsc = {} is negative.", pp.cdsc));
        }
        if pp.cdscd < 0.0 {
            notes.push(format!("Warning: Cdscd = {} is negative.", pp.cdscd));
        }
        if h.eta0 < 0.0 {
            notes.push(format!("Warning: Eta0 = {} is negative.", h.eta0));
        }
        if md.gidlclamp >= 0.0 {
            notes.push(format!(
                "Warning: gidlclamp = {} is zero or positive.",
                md.gidlclamp
            ));
        }
        if fabs(1.0e-8 / (pp.b1 + pp.weff)) > 10.0 {
            notes.push("Warning: (B1 + Weff) may be too small.".to_string());
        }
        if pp.a2 < 0.01 {
            notes.push(format!(
                "Warning: A2 = {} is too small. Set to 0.01.",
                pp.a2
            ));
            pp.a2 = 0.01;
        } else if pp.a2 > 1.0 {
            notes.push(format!(
                "Warning: A2 = {} is larger than 1. A2 is set to 1 and A1 is set to 0.",
                pp.a2
            ));
            pp.a2 = 1.0;
            pp.a1 = 0.0;
        }
        if pp.prwg < 0.0 {
            notes.push(format!(
                "Warning: Prwg = {} is negative. Set to zero.",
                pp.prwg
            ));
            pp.prwg = 0.0;
        }
        if pp.rdsw < 0.0 {
            notes.push(format!(
                "Warning: Rdsw = {} is negative. Set to zero.",
                pp.rdsw
            ));
            pp.rdsw = 0.0;
            pp.rds0 = 0.0;
        }
        if pp.rds0 < 0.0 {
            notes.push(format!(
                "Warning: Rds at current temperature = {} is negative. Set to zero.",
                pp.rds0
            ));
            pp.rds0 = 0.0;
        }
        if pp.rdswmin < 0.0 {
            notes.push(format!(
                "Warning: Rdswmin at current temperature = {} is negative. Set to zero.",
                pp.rdswmin
            ));
            pp.rdswmin = 0.0;
        }
        if pp.pscbe2 <= 0.0 {
            notes.push(format!("Warning: Pscbe2 = {} is not positive.", pp.pscbe2));
        }
        if pp.vsattemp < 1.0e3 {
            notes.push(format!(
                "Warning: Vsat at current temperature = {} may be too small.",
                pp.vsattemp
            ));
        }
        if md.given.lambda && pp.lambda > 0.0 && pp.lambda > 1.0e-9 {
            notes.push(format!("Warning: Lambda = {} may be too large.", pp.lambda));
        }
        if md.given.vtl && pp.vtl > 0.0 {
            if pp.vtl < 6.0e4 {
                notes.push(format!(
                    "Warning: Thermal velocity vtl = {} may be too small.",
                    pp.vtl
                ));
            }
            if pp.xn < 3.0 {
                notes.push(format!(
                    "Warning: back scattering coeff xn = {} is too small. Reset to 3.0",
                    pp.xn
                ));
                pp.xn = 3.0;
            }
            if md.lc < 0.0 {
                notes.push(format!(
                    "Warning: back scattering coeff lc = {} is too small. Reset to 0.0",
                    md.lc
                ));
                pp.lc = 0.0;
            }
        }
        if pp.pdibl1 < 0.0 {
            notes.push(format!("Warning: Pdibl1 = {} is negative.", pp.pdibl1));
        }
        if pp.pdibl2 < 0.0 {
            notes.push(format!("Warning: Pdibl2 = {} is negative.", pp.pdibl2));
        }
        if h.sa > 0.0 && h.sb > 0.0 && (h.nf == 1.0 || h.nf > 1.0 && h.sd > 0.0) {
            if md.lodk2 <= 0.0 {
                notes.push(format!("Warning: LODK2 = {} is not positive.", md.lodk2));
            }
            if md.lodeta0 <= 0.0 {
                notes.push(format!(
                    "Warning: LODETA0 = {} is not positive.",
                    md.lodeta0
                ));
            }
        }
        if h.rgate_mod == 1 {
            if md.rshg <= 0.0 {
                notes.push("Warning: rshg should be positive for rgateMod = 1.".to_string());
            }
        } else if h.rgate_mod == 2 {
            if md.rshg <= 0.0 {
                notes.push("Warning: rshg <= 0.0 for rgateMod = 2.".to_string());
            } else if pp.xrcrg1 <= 0.0 {
                notes.push("Warning: xrcrg1 <= 0.0 for rgateMod = 2.".to_string());
            }
        }
        if h.rgate_mod == 3 {
            if md.rshg <= 0.0 {
                notes.push("Warning: rshg should be positive for rgateMod = 3.".to_string());
            } else if pp.xrcrg1 <= 0.0 {
                notes.push("Warning: xrcrg1 should be positive for rgateMod = 3.".to_string());
            }
        }
        if md.rbps0 <= 0.0 {
            notes.push(format!("Fatal: RBPS0 = {} is not positive.", md.rbps0));
            fatal_flag = 1;
        }
        if md.rbpd0 <= 0.0 {
            notes.push(format!("Fatal: RBPD0 = {} is not positive.", md.rbpd0));
            fatal_flag = 1;
        }
        if md.rbpbx0 <= 0.0 {
            notes.push(format!("Fatal: RBPBX0 = {} is not positive.", md.rbpbx0));
            fatal_flag = 1;
        }
        if md.rbpby0 <= 0.0 {
            notes.push(format!("Fatal: RBPBY0 = {} is not positive.", md.rbpby0));
            fatal_flag = 1;
        }
        if md.rbdbx0 <= 0.0 {
            notes.push(format!("Fatal: RBDBX0 = {} is not positive.", md.rbdbx0));
            fatal_flag = 1;
        }
        if md.rbdby0 <= 0.0 {
            notes.push(format!("Fatal: RBDBY0 = {} is not positive.", md.rbdby0));
            fatal_flag = 1;
        }
        if md.rbsbx0 <= 0.0 {
            notes.push(format!("Fatal: RBSBX0 = {} is not positive.", md.rbsbx0));
            fatal_flag = 1;
        }
        if md.rbsby0 <= 0.0 {
            notes.push(format!("Fatal: RBSBY0 = {} is not positive.", md.rbsby0));
            fatal_flag = 1;
        }
        if pp.noff < 0.1 {
            notes.push(format!("Warning: Noff = {} is too small.", pp.noff));
        }
        if pp.voffcv < -0.5 {
            notes.push(format!("Warning: Voffcv = {} is too small.", pp.voffcv));
        }
        if pp.moin < 5.0 {
            notes.push(format!("Warning: Moin = {} is too small.", pp.moin));
        }
        if pp.moin > 25.0 {
            notes.push(format!("Warning: Moin = {} is too large.", pp.moin));
        }
        if md.cap_mod == 2 {
            if pp.acde < 0.1 {
                notes.push(format!("Warning: Acde = {} is too small.", pp.acde));
            }
            if pp.acde > 1.6 {
                notes.push(format!("Warning: Acde = {} is too large.", pp.acde));
            }
        }
        if md.cgdo < 0.0 {
            notes.push(format!(
                "Warning: cgdo = {} is negative. Set to zero.",
                md.cgdo
            ));
            md.cgdo = 0.0;
        }
        if md.cgso < 0.0 {
            notes.push(format!(
                "Warning: cgso = {} is negative. Set to zero.",
                md.cgso
            ));
            md.cgso = 0.0;
        }
        if md.cgbo < 0.0 {
            notes.push(format!(
                "Warning: cgbo = {} is negative. Set to zero.",
                md.cgbo
            ));
            md.cgbo = 0.0;
        }
        if md.tnoi_mod == 1 {
            notes.push(format!(
                "Warning: TNOIMOD={} is not supported and may be removed from future version.",
                md.tnoi_mod
            ));
        }
        if md.idovvdsc <= 0.0 {
            notes.push(format!(
                "Warning: idovvdsc = {} is zero or negative.",
                md.idovvdsc
            ));
        }
        if md.tnoi_mod == 1 {
            if md.tnoia < 0.0 {
                notes.push(format!(
                    "Warning: tnoia = {} is negative. Set to zero.",
                    md.tnoia
                ));
                md.tnoia = 0.0;
            }
            if md.tnoib < 0.0 {
                notes.push(format!(
                    "Warning: tnoib = {} is negative. Set to zero.",
                    md.tnoib
                ));
                md.tnoib = 0.0;
            }
            if md.rnoia < 0.0 {
                notes.push(format!(
                    "Warning: rnoia = {} is negative. Set to zero.",
                    md.rnoia
                ));
                md.rnoia = 0.0;
            }
            if md.rnoib < 0.0 {
                notes.push(format!(
                    "Warning: rnoib = {} is negative. Set to zero.",
                    md.rnoib
                ));
                md.rnoib = 0.0;
            }
        }
        if md.sjct_emission_coeff < 0.1 {
            notes.push(format!(
                "Warning: Njs = {} is less than 0.1. Setting Njs to 0.1.",
                md.sjct_emission_coeff
            ));
            md.sjct_emission_coeff = 0.1;
        } else if md.sjct_emission_coeff < 0.7 {
            notes.push(format!(
                "Warning: Njs = {} is less than 0.7.",
                md.sjct_emission_coeff
            ));
        }
        if md.djct_emission_coeff < 0.1 {
            notes.push(format!(
                "Warning: Njd = {} is less than 0.1. Setting Njd to 0.1.",
                md.djct_emission_coeff
            ));
            md.djct_emission_coeff = 0.1;
        } else if md.djct_emission_coeff < 0.7 {
            notes.push(format!(
                "Warning: Njd = {} is less than 0.7.",
                md.djct_emission_coeff
            ));
        }
        if md.njtsstemp < 0.0 {
            notes.push(format!(
                "Warning: Njts = {} is negative at temperature = {}.",
                md.njtsstemp, ck_temp
            ));
        }
        if md.njtsswstemp < 0.0 {
            notes.push(format!(
                "Warning: Njtssw = {} is negative at temperature = {}.",
                md.njtsswstemp, ck_temp
            ));
        }
        if md.njtsswgstemp < 0.0 {
            notes.push(format!(
                "Warning: Njtsswg = {} is negative at temperature = {}.",
                md.njtsswgstemp, ck_temp
            ));
        }
        if md.given.njtsd && md.njtsdtemp < 0.0 {
            notes.push(format!(
                "Warning: Njtsd = {} is negative at temperature = {}.",
                md.njtsdtemp, ck_temp
            ));
        }
        if md.given.njtsswd && md.njtsswdtemp < 0.0 {
            notes.push(format!(
                "Warning: Njtsswd = {} is negative at temperature = {}.",
                md.njtsswdtemp, ck_temp
            ));
        }
        if md.given.njtsswgd && md.njtsswgdtemp < 0.0 {
            notes.push(format!(
                "Warning: Njtsswgd = {} is negative at temperature = {}.",
                md.njtsswgdtemp, ck_temp
            ));
        }
        if md.ntnoi < 0.0 {
            notes.push(format!(
                "Warning: ntnoi = {} is negative. Set to zero.",
                md.ntnoi
            ));
            md.ntnoi = 0.0;
        }
        if md.sbulk_jct_bot_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJS = {} is too big. Set to 0.99.",
                md.sbulk_jct_bot_grading_coeff
            ));
            md.sbulk_jct_bot_grading_coeff = 0.99;
        }
        if md.sbulk_jct_side_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJSWS = {} is too big. Set to 0.99.",
                md.sbulk_jct_side_grading_coeff
            ));
            md.sbulk_jct_side_grading_coeff = 0.99;
        }
        if md.sbulk_jct_gate_side_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJSWGS = {} is too big. Set to 0.99.",
                md.sbulk_jct_gate_side_grading_coeff
            ));
            md.sbulk_jct_gate_side_grading_coeff = 0.99;
        }
        if md.dbulk_jct_bot_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJD = {} is too big. Set to 0.99.",
                md.dbulk_jct_bot_grading_coeff
            ));
            md.dbulk_jct_bot_grading_coeff = 0.99;
        }
        if md.dbulk_jct_side_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJSWD = {} is too big. Set to 0.99.",
                md.dbulk_jct_side_grading_coeff
            ));
            md.dbulk_jct_side_grading_coeff = 0.99;
        }
        if md.dbulk_jct_gate_side_grading_coeff >= 0.99 {
            notes.push(format!(
                "Warning: MJSWGD = {} is too big. Set to 0.99.",
                md.dbulk_jct_gate_side_grading_coeff
            ));
            md.dbulk_jct_gate_side_grading_coeff = 0.99;
        }
        if md.wpemod == 1.0 && md.scref <= 0.0 {
            notes.push(format!(
                "Warning: SCREF = {} is not positive. Set to 1e-6.",
                md.scref
            ));
            md.scref = 1e-6;
        }
    }
    fatal_flag
}
