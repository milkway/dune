// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4geo.c as distributed with ngspice-47
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

//! `b4geo.c`: the finger-geometry functions (diffusion areas and
//! perimeters, S/D resistances by `GEOMOD`/`RGEOMOD`).

use super::*;

#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn num_finger_diff(
    nf: f64,
    minsd: i32,
    nuintd: &mut f64,
    nuendd: &mut f64,
    nuints: &mut f64,
    nuends: &mut f64,
) -> i32 {
    let mut nf_: i32 = 0;
    nf_ = nf as i32;
    if nf_ % 2 != 0 {
        *nuends = 1.0;
        *nuendd = *nuends;
        *nuints = 2.0 * max((nf - 1.0) / 2.0, 0.0);
        *nuintd = *nuints;
    } else if minsd == 1 {
        *nuendd = 2.0;
        *nuintd = 2.0 * max(nf / 2.0 - 1.0, 0.0);
        *nuends = 0.0;
        *nuints = nf;
    } else {
        *nuendd = 0.0;
        *nuintd = nf;
        *nuends = 2.0;
        *nuints = 2.0 * max(nf / 2.0 - 1.0, 0.0);
    }
    0
}

#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn pa_eff_geo(
    nf: f64,
    geo: i32,
    minsd: i32,
    weffcj: f64,
    dmcg: f64,
    dmci: f64,
    dmdg: f64,
    ps: &mut f64,
    pd: &mut f64,
    as_: &mut f64,
    ad: &mut f64,
) -> i32 {
    let mut adiso: f64 = 0.0;
    let mut admer: f64 = 0.0;
    let mut adsha: f64 = 0.0;
    let mut asiso: f64 = 0.0;
    let mut asmer: f64 = 0.0;
    let mut assha: f64 = 0.0;
    let mut nuendd: f64 = 0.0;
    let mut nuends: f64 = 0.0;
    let mut nuintd: f64 = 0.0;
    let mut nuints: f64 = 0.0;
    let mut pdiso: f64 = 0.0;
    let mut pdmer: f64 = 0.0;
    let mut pdsha: f64 = 0.0;
    let mut psiso: f64 = 0.0;
    let mut psmer: f64 = 0.0;
    let mut pssha: f64 = 0.0;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 0.0;
    let mut t2: f64 = 0.0;
    nuintd = 0.0;
    nuendd = 0.0;
    nuints = 0.0;
    nuends = 0.0;
    if geo < 9 {
        let _ = num_finger_diff(
            nf,
            minsd,
            &mut nuintd,
            &mut nuendd,
            &mut nuints,
            &mut nuends,
        );
    }
    t0 = dmcg + dmci;
    t1 = dmcg + dmcg;
    t2 = dmdg + dmdg;
    pdiso = t0 + t0 + weffcj;
    psiso = pdiso;
    pdsha = t1;
    pssha = pdsha;
    pdmer = t2;
    psmer = pdmer;
    adiso = t0 * weffcj;
    asiso = adiso;
    adsha = dmcg * weffcj;
    assha = adsha;
    admer = dmdg * weffcj;
    asmer = admer;
    match geo {
        0 => {
            *ps = nuends * psiso + nuints * pssha;
            *pd = nuendd * pdiso + nuintd * pdsha;
            *as_ = nuends * asiso + nuints * assha;
            *ad = nuendd * adiso + nuintd * adsha;
        }
        1 => {
            *ps = nuends * psiso + nuints * pssha;
            *pd = (nuendd + nuintd) * pdsha;
            *as_ = nuends * asiso + nuints * assha;
            *ad = (nuendd + nuintd) * adsha;
        }
        2 => {
            *ps = (nuends + nuints) * pssha;
            *pd = nuendd * pdiso + nuintd * pdsha;
            *as_ = (nuends + nuints) * assha;
            *ad = nuendd * adiso + nuintd * adsha;
        }
        3 => {
            *ps = (nuends + nuints) * pssha;
            *pd = (nuendd + nuintd) * pdsha;
            *as_ = (nuends + nuints) * assha;
            *ad = (nuendd + nuintd) * adsha;
        }
        4 => {
            *ps = nuends * psiso + nuints * pssha;
            *pd = nuendd * pdmer + nuintd * pdsha;
            *as_ = nuends * asiso + nuints * assha;
            *ad = nuendd * admer + nuintd * adsha;
        }
        5 => {
            *ps = (nuends + nuints) * pssha;
            *pd = nuendd * pdmer + nuintd * pdsha;
            *as_ = (nuends + nuints) * assha;
            *ad = nuendd * admer + nuintd * adsha;
        }
        6 => {
            *ps = nuends * psmer + nuints * pssha;
            *pd = nuendd * pdiso + nuintd * pdsha;
            *as_ = nuends * asmer + nuints * assha;
            *ad = nuendd * adiso + nuintd * adsha;
        }
        7 => {
            *ps = nuends * psmer + nuints * pssha;
            *pd = (nuendd + nuintd) * pdsha;
            *as_ = nuends * asmer + nuints * assha;
            *ad = (nuendd + nuintd) * adsha;
        }
        8 => {
            *ps = nuends * psmer + nuints * pssha;
            *pd = nuendd * pdmer + nuintd * pdsha;
            *as_ = nuends * asmer + nuints * assha;
            *ad = nuendd * admer + nuintd * adsha;
        }
        9 => {
            *ps = psiso + (nf - 1.0) * pssha;
            *pd = nf * pdsha;
            *as_ = asiso + (nf - 1.0) * assha;
            *ad = nf * adsha;
        }
        10 => {
            *ps = nf * pssha;
            *pd = pdiso + (nf - 1.0) * pdsha;
            *as_ = nf * assha;
            *ad = adiso + (nf - 1.0) * adsha;
        }
        _ => {
            // "Warning: Specified GEO = %d not matched\n"
        }
    }
    0
}

#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn rdseff_geo(
    nf: f64,
    geo: i32,
    rgeo: i32,
    minsd: i32,
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    dmci: f64,
    dmdg: f64,
    type_: i32,
    rtot: &mut f64,
) -> i32 {
    let mut nuendd: f64 = 0.0;
    let mut nuends: f64 = 0.0;
    let mut nuintd: f64 = 0.0;
    let mut nuints: f64 = 0.0;
    let mut rend: f64 = 0.0;
    let mut rint: f64 = 0.0;
    rint = 0.0;
    rend = 0.0;
    nuintd = 0.0;
    nuendd = 0.0;
    nuints = 0.0;
    nuends = 0.0;
    if geo < 9 {
        let _ = num_finger_diff(
            nf,
            minsd,
            &mut nuintd,
            &mut nuendd,
            &mut nuints,
            &mut nuends,
        );
        if type_ == 1 {
            if nuints == 0.0 {
                rint = 0.0;
            } else {
                rint = rsh * dmcg / (weffcj * nuints);
            }
        } else if nuintd == 0.0 {
            rint = 0.0;
        } else {
            rint = rsh * dmcg / (weffcj * nuintd);
        }
    }
    match geo {
        0 => {
            if type_ == 1 {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        1 => {
            if type_ == 1 {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        2 => {
            if type_ == 1 {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        3 => {
            if type_ == 1 {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        4 => {
            if type_ == 1 {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                rend = rsh * dmdg / weffcj;
            }
        }
        5 => {
            if type_ == 1 {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuends, rgeo, 1, &mut rend);
            } else {
                rend = rsh * dmdg / (weffcj * nuendd);
            }
        }
        6 => {
            if type_ == 1 {
                rend = rsh * dmdg / weffcj;
            } else {
                let _ = rds_end_iso(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        7 => {
            if type_ == 1 {
                rend = rsh * dmdg / (weffcj * nuends);
            } else {
                let _ = rds_end_sha(weffcj, rsh, dmcg, dmci, dmdg, nuendd, rgeo, 0, &mut rend);
            }
        }
        8 => {
            rend = rsh * dmdg / weffcj;
        }
        9 => {
            if type_ == 1 {
                rend = 0.5 * rsh * dmcg / weffcj;
                if nf == 2.0 {
                    rint = 0.0;
                } else {
                    rint = rsh * dmcg / (weffcj * (nf - 2.0));
                }
            } else {
                rend = 0.0;
                rint = rsh * dmcg / (weffcj * nf);
            }
        }
        10 => {
            if type_ == 1 {
                rend = 0.0;
                rint = rsh * dmcg / (weffcj * nf);
            } else {
                rend = 0.5 * rsh * dmcg / weffcj;
                if nf == 2.0 {
                    rint = 0.0;
                } else {
                    rint = rsh * dmcg / (weffcj * (nf - 2.0));
                }
            }
        }
        _ => {
            // "Warning: Specified GEO = %d not matched\n"
        }
    }
    if rint <= 0.0 {
        *rtot = rend;
    } else if rend <= 0.0 {
        *rtot = rint;
    } else {
        *rtot = rint * rend / (rint + rend);
    }
    if *rtot == 0.0 {
        // "Warning: Zero resistance returned from RdseffGeo\n"
    }
    0
}

#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn rds_end_iso(
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    dmci: f64,
    _dmdg: f64,
    nuend: f64,
    rgeo: i32,
    type_: i32,
    rend: &mut f64,
) -> i32 {
    if type_ == 1 {
        match rgeo {
            1 | 2 | 5 => {
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * dmcg / (weffcj * nuend);
                }
            }
            3 | 4 | 6 => {
                if dmcg + dmci == 0.0 {
                    // "(DMCG + DMCI) can not be equal to zero\n"
                }
                if nuend == 0.0 || dmcg + dmci == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * weffcj / (3.0 * nuend * (dmcg + dmci));
                }
            }
            _ => {
                // "Warning: Specified RGEO = %d not matched\n"
            }
        }
    } else {
        match rgeo {
            1 | 3 | 7 => {
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * dmcg / (weffcj * nuend);
                }
            }
            2 | 4 | 8 => {
                if dmcg + dmci == 0.0 {
                    // "(DMCG + DMCI) can not be equal to zero\n"
                }
                if nuend == 0.0 || dmcg + dmci == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * weffcj / (3.0 * nuend * (dmcg + dmci));
                }
            }
            _ => {
                // "Warning: Specified RGEO = %d not matched\n"
            }
        }
    }
    0
}

#[allow(unused_assignments, clippy::too_many_arguments)]
pub(crate) fn rds_end_sha(
    weffcj: f64,
    rsh: f64,
    dmcg: f64,
    _dmci: f64,
    _dmdg: f64,
    nuend: f64,
    rgeo: i32,
    type_: i32,
    rend: &mut f64,
) -> i32 {
    if type_ == 1 {
        match rgeo {
            1 | 2 | 5 => {
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * dmcg / (weffcj * nuend);
                }
            }
            3 | 4 | 6 => {
                if dmcg == 0.0 {
                    // "DMCG can not be equal to zero\n"
                }
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * weffcj / (6.0 * nuend * dmcg);
                }
            }
            _ => {
                // "Warning: Specified RGEO = %d not matched\n"
            }
        }
    } else {
        match rgeo {
            1 | 3 | 7 => {
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * dmcg / (weffcj * nuend);
                }
            }
            2 | 4 | 8 => {
                if dmcg == 0.0 {
                    // "DMCG can not be equal to zero\n"
                }
                if nuend == 0.0 {
                    *rend = 0.0;
                } else {
                    *rend = rsh * weffcj / (6.0 * nuend * dmcg);
                }
            }
            _ => {
                // "Warning: Specified RGEO = %d not matched\n"
            }
        }
    }
    0
}
