// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4noi.c as distributed with ngspice-47
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

//! `BSIM4noise` (`b4noi.c`), the `N_DENS` branch: the densities of the
//! fifteen generators of `BSIM4nNames`, each through the adjoint gain
//! (`NevalSrc`). The `INT_NOIZ` integration and the summary bookkeeping
//! belong to Dune's noise analysis. `TNOIMOD = 2` (the correlated
//! drain/induced-gate pair of `NevalSrc2`) is refused at elaboration.

use super::*;

// Generator indices (`bsim4def.h`).
const RDNOIZ: usize = 0;
const RSNOIZ: usize = 1;
const RGNOIZ: usize = 2;
const RBPSNOIZ: usize = 3;
const RBPDNOIZ: usize = 4;
const RBPBNOIZ: usize = 5;
const RBSBNOIZ: usize = 6;
const RBDBNOIZ: usize = 7;
const IDNOIZ: usize = 8;
const FLNOIZ: usize = 9;
const IGSNOIZ: usize = 10;
const IGDNOIZ: usize = 11;
const IGBNOIZ: usize = 12;
const CORLNOIZ: usize = 13;
const TOTNOIZ: usize = 14;
const NSRCS: usize = 15;

/// `Eval1ovFNoise` (`b4noi.c`): the unified flicker noise of the
/// strong-inversion region at `freq`.
fn eval_1ov_f_noise(
    vds: f64,
    md: &Bsim4Model,
    pp: &Bsim4Size,
    h: &Bsim4Here,
    freq: f64,
    temp: f64,
) -> f64 {
    let cd = fabs(h.cd);
    let leff = pp.leff - 2.0 * md.lintnoi;
    let leffsq = leff * leff;
    let esat = 2.0 * h.vsattemp / h.ueff;
    let del_clm = if md.em <= 0.0 {
        0.0
    } else {
        let t0 = ((vds - h.vdseff) / pp.litl + md.em) / esat;
        let d = pp.litl * log(max(t0, N_MINLOG));
        if d < 0.0 { 0.0 } else { d }
    };
    let eff_freq = pow(freq, md.ef);
    let t1 = CHARGE * CHARGE * BOLTZ * cd * temp * h.ueff;
    let t2 = 1.0e10 * eff_freq * h.abulk * md.coxe * leffsq;
    let n0 = md.coxe * h.vgsteff / CHARGE;
    let nl = md.coxe * h.vgsteff * (1.0 - h.abov_vgst2_vtm * h.vdseff) / CHARGE;
    let t3 = md.oxide_trap_density_a * log(max((n0 + h.nstar) / (nl + h.nstar), N_MINLOG));
    let t4 = md.oxide_trap_density_b * (n0 - nl);
    let t5 = md.oxide_trap_density_c * 0.5 * (n0 * n0 - nl * nl);
    let t6 = BOLTZ * temp * cd * cd;
    let t7 = 1.0e10 * eff_freq * leffsq * pp.weff * h.nf;
    let t8 =
        md.oxide_trap_density_a + md.oxide_trap_density_b * nl + md.oxide_trap_density_c * nl * nl;
    let t9 = (nl + h.nstar) * (nl + h.nstar);
    t1 / t2 * (t3 + t4 + t5) + t6 / t7 * del_clm * t8 / t9
}

/// The `N_DENS` densities, in `BSIM4nNames` order (the last one the
/// total).
#[allow(clippy::too_many_lines)]
pub(super) fn noise_dens(
    md: &Bsim4Model,
    pp: &Bsim4Size,
    h: &Bsim4Here,
    st: usize,
    ckt_temp: f64,
    ctx: &crate::matrix::NoiseCtx,
) -> Vec<f64> {
    let mut nd = [0.0f64; NSRCS];
    let mult_i = h.mult_i * h.m;
    let mult_fn = h.mult_fn * h.m;
    let mut tmp = 0.0;
    let mut npart_theta = 0.0;
    let mut npart_beta = 0.0;
    let mut gspr;
    let mut gdpr;
    if md.tnoi_mod == 0 {
        if md.rds_mod == 0 {
            gspr = h.source_conductance;
            gdpr = h.drain_conductance;
            tmp = if h.grdsw > 0.0 { 1.0 / h.grdsw } else { 0.0 };
        } else {
            gspr = h.gstot;
            gdpr = h.gdtot;
            tmp = 0.0;
        }
    } else if md.tnoi_mod == 1 {
        let mut t5 = h.vgsteff / h.esat_l;
        t5 *= t5;
        npart_beta = md.rnoia * (1.0 + t5 * md.tnoia * pp.leff);
        npart_theta = md.rnoib * (1.0 + t5 * md.tnoib * pp.leff);
        if npart_theta > 0.9 {
            npart_theta = 0.9;
        }
        if npart_theta > 0.9 * npart_beta {
            npart_theta = 0.9 * npart_beta;
        }
        if md.rds_mod == 0 {
            gspr = h.source_conductance;
            gdpr = h.drain_conductance;
        } else {
            gspr = h.gstot;
            gdpr = h.gdtot;
        }
        if ctx.states.get(0, st + ST_VDS) >= 0.0 {
            gspr *= 1.0 + npart_theta * npart_theta * gspr / h.idov_vds;
        } else {
            gdpr *= 1.0 + npart_theta * npart_theta * gdpr / h.idov_vds;
        }
    } else {
        // TNOIMOD = 2 is refused at elaboration (`NevalSrc2`).
        if md.rds_mod == 0 {
            gspr = h.source_conductance;
            gdpr = h.drain_conductance;
        } else {
            gspr = h.gstot;
            gdpr = h.gdtot;
        }
    }

    nd[RDNOIZ] = ctx.thermal(h.d_node_prime, h.d_node, gdpr * mult_i);
    nd[RSNOIZ] = ctx.thermal(h.s_node_prime, h.s_node, gspr * mult_i);

    if h.rgate_mod == 1 {
        nd[RGNOIZ] = ctx.thermal(h.g_node_prime, h.g_node_ext, h.grgeltd * mult_i);
    } else if h.rgate_mod == 2 {
        let t0 = 1.0 + h.grgeltd / h.gcrg;
        let t1 = t0 * t0;
        nd[RGNOIZ] = ctx.thermal(h.g_node_prime, h.g_node_ext, h.grgeltd * mult_i / t1);
    } else if h.rgate_mod == 3 {
        nd[RGNOIZ] = ctx.thermal(h.g_node_mid, h.g_node_ext, h.grgeltd * mult_i);
    } else {
        nd[RGNOIZ] = 0.0;
    }

    let mut bodymode = 5;
    if h.rbody_mod == 2 {
        if !md.given.rbps0 || !md.given.rbpd0 {
            bodymode = 1;
        } else if (!md.given.rbsbx0 && !md.given.rbsby0) || (!md.given.rbdbx0 && !md.given.rbdby0) {
            bodymode = 3;
        }
    }
    if h.rbody_mod != 0 {
        if bodymode == 5 {
            nd[RBPSNOIZ] = ctx.thermal(h.b_node_prime, h.sb_node, h.grbps * mult_i);
            nd[RBPDNOIZ] = ctx.thermal(h.b_node_prime, h.db_node, h.grbpd * mult_i);
            nd[RBPBNOIZ] = ctx.thermal(h.b_node_prime, h.b_node, h.grbpb * mult_i);
            nd[RBSBNOIZ] = ctx.thermal(h.b_node, h.sb_node, h.grbsb * mult_i);
            nd[RBDBNOIZ] = ctx.thermal(h.b_node, h.db_node, h.grbdb * mult_i);
        }
        if bodymode == 3 {
            nd[RBPSNOIZ] = ctx.thermal(h.b_node_prime, h.sb_node, h.grbps * mult_i);
            nd[RBPDNOIZ] = ctx.thermal(h.b_node_prime, h.db_node, h.grbpd * mult_i);
            nd[RBPBNOIZ] = ctx.thermal(h.b_node_prime, h.b_node, h.grbpb * mult_i);
            nd[RBSBNOIZ] = 0.0;
            nd[RBDBNOIZ] = 0.0;
        }
        if bodymode == 1 {
            nd[RBPBNOIZ] = ctx.thermal(h.b_node_prime, h.b_node, h.grbpb * mult_i);
            nd[RBPSNOIZ] = 0.0;
            nd[RBPDNOIZ] = 0.0;
            nd[RBSBNOIZ] = 0.0;
            nd[RBDBNOIZ] = 0.0;
        }
    }

    match md.tnoi_mod {
        0 => {
            let t0 = h.ueff * fabs(h.qinv);
            let t1 = t0 * tmp + pp.leff * pp.leff;
            nd[IDNOIZ] = ctx.thermal(
                h.d_node_prime,
                h.s_node_prime,
                (t0 / t1) * md.ntnoi * mult_i,
            );
            nd[CORLNOIZ] = 0.0;
        }
        1 => {
            let mut t0 = h.gm + h.gmbs + h.gds;
            t0 *= t0;
            let igsquare = npart_theta * npart_theta * t0 / h.idov_vds;
            let t1 = npart_beta * (h.gm + h.gmbs) + h.gds;
            let t2 = t1 * t1 / h.idov_vds;
            nd[IDNOIZ] = ctx.thermal(h.d_node_prime, h.s_node_prime, (t2 - igsquare) * mult_i);
            nd[CORLNOIZ] = 0.0;
        }
        _ => {}
    }

    match md.fnoi_mod {
        0 => {
            nd[FLNOIZ] = ctx.flicker(
                h.d_node_prime,
                h.s_node_prime,
                mult_fn * md.kf * exp(md.af * log(max(fabs(h.cd), N_MINLOG)))
                    / (pp.leff * pp.leff * md.coxe),
                md.ef,
            );
        }
        1 => {
            // A power law in f as well: Ssi and Swi both scale as
            // f^-EF, so their parallel combination is evaluated at 1 Hz
            // and handed over as `coef / f^EF`.
            let mut vds = ctx.states.get(0, st + ST_VDS);
            if vds < 0.0 {
                vds = -vds;
            }
            let ssi = eval_1ov_f_noise(vds, md, pp, h, 1.0, ckt_temp);
            let t10 = md.oxide_trap_density_a * BOLTZ * ckt_temp;
            let t11 = pp.weff * h.nf * pp.leff * pow(1.0, md.ef) * 1.0e10 * h.nstar * h.nstar;
            let swi = t10 / t11 * h.cd * h.cd;
            let t1 = swi + ssi;
            let coef = if t1 > 0.0 {
                mult_fn * (ssi * swi) / t1
            } else {
                0.0
            };
            nd[FLNOIZ] = ctx.flicker(h.d_node_prime, h.s_node_prime, coef, md.ef);
        }
        _ => {}
    }

    if h.mode >= 0 {
        nd[IGSNOIZ] = ctx.shot(h.g_node_prime, h.s_node_prime, mult_i * (h.igs + h.igcs));
        nd[IGDNOIZ] = ctx.shot(h.g_node_prime, h.d_node_prime, mult_i * (h.igd + h.igcd));
    } else {
        nd[IGSNOIZ] = ctx.shot(h.g_node_prime, h.s_node_prime, mult_i * (h.igs + h.igcd));
        nd[IGDNOIZ] = ctx.shot(h.g_node_prime, h.d_node_prime, mult_i * (h.igd + h.igcs));
    }
    nd[IGBNOIZ] = ctx.shot(h.g_node_prime, h.b_node_prime, mult_i * h.igb);

    nd[TOTNOIZ] = nd[RDNOIZ]
        + nd[RSNOIZ]
        + nd[RGNOIZ]
        + nd[RBPSNOIZ]
        + nd[RBPDNOIZ]
        + nd[RBPBNOIZ]
        + nd[RBSBNOIZ]
        + nd[RBDBNOIZ]
        + nd[IDNOIZ]
        + nd[FLNOIZ]
        + nd[IGSNOIZ]
        + nd[IGDNOIZ]
        + nd[IGBNOIZ]
        + nd[CORLNOIZ];
    nd.to_vec()
}
