// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4pzld.c as distributed with ngspice-47
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

//! `BSIM4pzLoad` (`b4pzld.c`): the stamps at a complex frequency `s`.

use super::*;

#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn pz_load(
    md: &Bsim4Model,
    pp: &Bsim4Size,
    h: &Bsim4Here,
    st: usize,
    ctx: &mut crate::matrix::PzCtx,
) {
    let mut _capbd: f64 = 0.0;
    let mut _capbs: f64 = 0.0;
    let mut cdd: f64 = 0.0;
    let mut cdg: f64 = 0.0;
    let mut cds: f64 = 0.0;
    let mut _cgbo: f64 = 0.0;
    let mut cgdo: f64 = 0.0;
    let mut cgso: f64 = 0.0;
    let mut coxwl: f64 = 0.0;
    let mut csd: f64 = 0.0;
    let mut csg: f64 = 0.0;
    let mut css: f64 = 0.0;
    let mut ddxpart_dvb: f64 = 0.0;
    let mut ddxpart_dvd: f64 = 0.0;
    let mut ddxpart_dvg: f64 = 0.0;
    let mut ddxpart_dvs: f64 = 0.0;
    let mut dsxpart_dvb: f64 = 0.0;
    let mut dsxpart_dvd: f64 = 0.0;
    let mut dsxpart_dvg: f64 = 0.0;
    let mut dsxpart_dvs: f64 = 0.0;
    let mut dxpart: f64 = 0.0;
    let mut fwdsum: f64 = 0.0;
    let mut gbbdp: f64 = 0.0;
    let mut gbbsp: f64 = 0.0;
    let mut gbdpb: f64 = 0.0;
    let mut gbdpdp: f64 = 0.0;
    let mut gbdpg: f64 = 0.0;
    let mut gbdpsp: f64 = 0.0;
    let mut gbspb: f64 = 0.0;
    let mut gbspdp: f64 = 0.0;
    let mut gbspg: f64 = 0.0;
    let mut gbspsp: f64 = 0.0;
    let mut gcrg: f64 = 0.0;
    let mut gcrgb: f64 = 0.0;
    let mut gcrgd: f64 = 0.0;
    let mut gcrgg: f64 = 0.0;
    let mut gcrgs: f64 = 0.0;
    let mut gdpr: f64 = 0.0;
    let mut gds: f64 = 0.0;
    let mut gdtot: f64 = 0.0;
    let mut gdtotb: f64 = 0.0;
    let mut gdtotd: f64 = 0.0;
    let mut gdtotg: f64 = 0.0;
    let mut gdtots: f64 = 0.0;
    let mut geltd: f64 = 0.0;
    let mut ggidlb: f64 = 0.0;
    let mut ggidld: f64 = 0.0;
    let mut ggidlg: f64 = 0.0;
    let mut ggislb: f64 = 0.0;
    let mut ggislg: f64 = 0.0;
    let mut ggisls: f64 = 0.0;
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
    let mut gmbs: f64 = 0.0;
    let mut gspr: f64 = 0.0;
    let mut gstot: f64 = 0.0;
    let mut gstotb: f64 = 0.0;
    let mut gstotd: f64 = 0.0;
    let mut gstotg: f64 = 0.0;
    let mut gstots: f64 = 0.0;
    let mut m: f64 = 0.0;
    let mut mult_i: f64 = 0.0;
    let mut mult_q: f64 = 0.0;
    let mut qcheq: f64 = 0.0;
    let mut revsum: f64 = 0.0;
    let mut scalingfactor: f64 = 0.0;
    let mut sxpart: f64 = 0.0;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 0.0;
    let mut xcbbb: f64 = 0.0;
    let mut xcbdb: f64 = 0.0;
    let mut xcbgb: f64 = 0.0;
    let mut xcbgmb: f64 = 0.0;
    let mut xcbsb: f64 = 0.0;
    let mut xcdbb: f64 = 0.0;
    let mut xcdbdb: f64 = 0.0;
    let mut xcddb: f64 = 0.0;
    let mut xcdgb: f64 = 0.0;
    let mut xcdgmb: f64 = 0.0;
    let mut xcdsb: f64 = 0.0;
    let mut xcgbb: f64 = 0.0;
    let mut xcgdb: f64 = 0.0;
    let mut xcggb: f64 = 0.0;
    let mut xcgmbb: f64 = 0.0;
    let mut xcgmdb: f64 = 0.0;
    let mut xcgmgmb: f64 = 0.0;
    let mut xcgmsb: f64 = 0.0;
    let mut xcgsb: f64 = 0.0;
    let mut xcqbb: f64 = 0.0;
    let mut xcqdb: f64 = 0.0;
    let mut xcqgb: f64 = 0.0;
    let mut xcqsb: f64 = 0.0;
    let mut xcsbb: f64 = 0.0;
    let mut xcsbsb: f64 = 0.0;
    let mut xcsdb: f64 = 0.0;
    let mut xcsgb: f64 = 0.0;
    let mut xcsgmb: f64 = 0.0;
    let mut xcssb: f64 = 0.0;
    let mut xgtb: f64 = 0.0;
    let mut xgtd: f64 = 0.0;
    let mut xgtg: f64 = 0.0;
    let mut xgts: f64 = 0.0;
    xcdbdb = 0.0;
    xcsbsb = 0.0;
    xcgmgmb = 0.0;
    xcgmdb = 0.0;
    xcgmsb = 0.0;
    xcdgmb = 0.0;
    xcsgmb = 0.0;
    xcgmbb = 0.0;
    xcbgmb = 0.0;
    xcqgb = 0.0;
    xcqdb = 0.0;
    xcqsb = 0.0;
    xcqbb = 0.0;
    t0 = 0.0;
    scalingfactor = 1.0e-9;
    _capbd = h.capbd;
    _capbs = h.capbs;
    cgso = h.cgso;
    cgdo = h.cgdo;
    _cgbo = pp.cgbo;
    if h.mode >= 0 {
        gm = h.gm;
        gmbs = h.gmbs;
        fwdsum = gm + gmbs;
        revsum = 0.0;
        gbbdp = -h.gbds;
        gbbsp = h.gbds + h.gbgs + h.gbbs;
        gbdpg = h.gbgs;
        gbdpdp = h.gbds;
        gbdpb = h.gbbs;
        gbdpsp = -(gbdpg + gbdpdp + gbdpb);
        gbspdp = 0.0;
        gbspg = 0.0;
        gbspb = 0.0;
        gbspsp = 0.0;
        if md.igc_mod != 0 {
            gistotg = h.g_igsg + h.g_igcsg;
            gistotd = h.g_igcsd;
            gistots = h.g_igss + h.g_igcss;
            gistotb = h.g_igcsb;
            gidtotg = h.g_igdg + h.g_igcdg;
            gidtotd = h.g_igdd + h.g_igcdd;
            gidtots = h.g_igcds;
            gidtotb = h.g_igcdb;
        } else {
            gistotb = 0.0;
            gistots = gistotb;
            gistotd = gistots;
            gistotg = gistotd;
            gidtotb = 0.0;
            gidtots = gidtotb;
            gidtotd = gidtots;
            gidtotg = gidtotd;
        }
        if md.igb_mod != 0 {
            gibtotg = h.g_igbg;
            gibtotd = h.g_igbd;
            gibtots = h.g_igbs;
            gibtotb = h.g_igbb;
        } else {
            gibtotb = 0.0;
            gibtots = gibtotb;
            gibtotd = gibtots;
            gibtotg = gibtotd;
        }
        if md.igc_mod != 0 || md.igb_mod != 0 {
            gigtotg = gistotg + gidtotg + gibtotg;
            gigtotd = gistotd + gidtotd + gibtotd;
            gigtots = gistots + gidtots + gibtots;
            gigtotb = gistotb + gidtotb + gibtotb;
        } else {
            gigtotb = 0.0;
            gigtots = gigtotb;
            gigtotd = gigtots;
            gigtotg = gigtotd;
        }
        if h.rgate_mod == 2 {
            t0 = ctx.states.get(0, st + ST_VGES) - ctx.states.get(0, st + ST_VGS);
        } else if h.rgate_mod == 3 {
            t0 = ctx.states.get(0, st + ST_VGMS) - ctx.states.get(0, st + ST_VGS);
        }
        if h.rgate_mod > 1 {
            gcrgd = h.gcrgd * t0;
            gcrgg = h.gcrgg * t0;
            gcrgs = h.gcrgs * t0;
            gcrgb = h.gcrgb * t0;
            gcrgg -= h.gcrg;
            gcrg = h.gcrg;
        } else {
            gcrgb = 0.0;
            gcrgs = gcrgb;
            gcrgg = gcrgs;
            gcrgd = gcrgg;
            gcrg = gcrgd;
        }
        if h.acnqs_mod == 0 {
            if h.rgate_mod == 3 {
                xcgmgmb = cgdo + cgso + pp.cgbo;
                xcgmdb = -cgdo;
                xcgmsb = -cgso;
                xcgmbb = -pp.cgbo;
                xcdgmb = xcgmdb;
                xcsgmb = xcgmsb;
                xcbgmb = xcgmbb;
                xcggb = h.cggb;
                xcgdb = h.cgdb;
                xcgsb = h.cgsb;
                xcgbb = -(xcggb + xcgdb + xcgsb);
                xcdgb = h.cdgb;
                xcsgb = -(h.cggb + h.cbgb + h.cdgb);
                xcbgb = h.cbgb;
            } else {
                xcggb = h.cggb + cgdo + cgso + pp.cgbo;
                xcgdb = h.cgdb - cgdo;
                xcgsb = h.cgsb - cgso;
                xcgbb = -(xcggb + xcgdb + xcgsb);
                xcdgb = h.cdgb - cgdo;
                xcsgb = -(h.cggb + h.cbgb + h.cdgb + cgso);
                xcbgb = h.cbgb - pp.cgbo;
                xcbgmb = 0.0;
                xcsgmb = xcbgmb;
                xcdgmb = xcsgmb;
            }
            xcddb = h.cddb + h.capbd + cgdo;
            xcdsb = h.cdsb;
            xcsdb = -(h.cgdb + h.cbdb + h.cddb);
            xcssb = h.capbs + cgso - (h.cgsb + h.cbsb + h.cdsb);
            if h.rbody_mod == 0 {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb);
                xcbdb = h.cbdb - h.capbd;
                xcbsb = h.cbsb - h.capbs;
                xcdbdb = 0.0;
            } else {
                xcdbb = -(h.cddb + h.cdgb + h.cdsb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb) + h.capbs;
                xcbdb = h.cbdb;
                xcbsb = h.cbsb;
                xcdbdb = -h.capbd;
                xcsbsb = -h.capbs;
            }
            xcbbb = -(xcbdb + xcbgb + xcbsb + xcbgmb);
            xgtb = 0.0;
            xgts = xgtb;
            xgtd = xgts;
            xgtg = xgtd;
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
            xcgbb = 0.0;
            xcgsb = xcgbb;
            xcgdb = xcgsb;
            xcggb = xcgdb;
            xcbbb = 0.0;
            xcbsb = xcbbb;
            xcbdb = xcbsb;
            xcbgb = xcbdb;
            xcdbb = 0.0;
            xcdsb = xcdbb;
            xcddb = xcdsb;
            xcdgb = xcddb;
            xcsbb = 0.0;
            xcssb = xcsbb;
            xcsdb = xcssb;
            xcsgb = xcsdb;
            xgtg = h.gtg;
            xgtd = h.gtd;
            xgts = h.gts;
            xgtb = h.gtb;
            xcqgb = h.cqgb;
            xcqdb = h.cqdb;
            xcqsb = h.cqsb;
            xcqbb = h.cqbb;
            coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
            qcheq = -(h.qgate + h.qbulk);
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
                dxpart = h.qdrn / qcheq;
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
        }
    } else {
        gm = -h.gm;
        gmbs = -h.gmbs;
        fwdsum = 0.0;
        revsum = -(gm + gmbs);
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
            gidtotg = h.g_igdg + h.g_igcsg;
            gidtotd = h.g_igdd + h.g_igcss;
            gidtots = h.g_igcsd;
            gidtotb = h.g_igcsb;
        } else {
            gistotb = 0.0;
            gistots = gistotb;
            gistotd = gistots;
            gistotg = gistotd;
            gidtotb = 0.0;
            gidtots = gidtotb;
            gidtotd = gidtots;
            gidtotg = gidtotd;
        }
        if md.igb_mod != 0 {
            gibtotg = h.g_igbg;
            gibtotd = h.g_igbs;
            gibtots = h.g_igbd;
            gibtotb = h.g_igbb;
        } else {
            gibtotb = 0.0;
            gibtots = gibtotb;
            gibtotd = gibtots;
            gibtotg = gibtotd;
        }
        if md.igc_mod != 0 || md.igb_mod != 0 {
            gigtotg = gistotg + gidtotg + gibtotg;
            gigtotd = gistotd + gidtotd + gibtotd;
            gigtots = gistots + gidtots + gibtots;
            gigtotb = gistotb + gidtotb + gibtotb;
        } else {
            gigtotb = 0.0;
            gigtots = gigtotb;
            gigtotd = gigtots;
            gigtotg = gigtotd;
        }
        if h.rgate_mod == 2 {
            t0 = ctx.states.get(0, st + ST_VGES) - ctx.states.get(0, st + ST_VGS);
        } else if h.rgate_mod == 3 {
            t0 = ctx.states.get(0, st + ST_VGMS) - ctx.states.get(0, st + ST_VGS);
        }
        if h.rgate_mod > 1 {
            gcrgd = h.gcrgs * t0;
            gcrgg = h.gcrgg * t0;
            gcrgs = h.gcrgd * t0;
            gcrgb = h.gcrgb * t0;
            gcrgg -= h.gcrg;
            gcrg = h.gcrg;
        } else {
            gcrgb = 0.0;
            gcrgs = gcrgb;
            gcrgg = gcrgs;
            gcrgd = gcrgg;
            gcrg = gcrgd;
        }
        if h.acnqs_mod == 0 {
            if h.rgate_mod == 3 {
                xcgmgmb = cgdo + cgso + pp.cgbo;
                xcgmdb = -cgdo;
                xcgmsb = -cgso;
                xcgmbb = -pp.cgbo;
                xcdgmb = xcgmdb;
                xcsgmb = xcgmsb;
                xcbgmb = xcgmbb;
                xcggb = h.cggb;
                xcgdb = h.cgsb;
                xcgsb = h.cgdb;
                xcgbb = -(xcggb + xcgdb + xcgsb);
                xcdgb = -(h.cggb + h.cbgb + h.cdgb);
                xcsgb = h.cdgb;
                xcbgb = h.cbgb;
            } else {
                xcggb = h.cggb + cgdo + cgso + pp.cgbo;
                xcgdb = h.cgsb - cgdo;
                xcgsb = h.cgdb - cgso;
                xcgbb = -(xcggb + xcgdb + xcgsb);
                xcdgb = -(h.cggb + h.cbgb + h.cdgb + cgdo);
                xcsgb = h.cdgb - cgso;
                xcbgb = h.cbgb - pp.cgbo;
                xcbgmb = 0.0;
                xcsgmb = xcbgmb;
                xcdgmb = xcsgmb;
            }
            xcddb = h.capbd + cgdo - (h.cgsb + h.cbsb + h.cdsb);
            xcdsb = -(h.cgdb + h.cbdb + h.cddb);
            xcsdb = h.cdsb;
            xcssb = h.cddb + h.capbs + cgso;
            if h.rbody_mod == 0 {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb);
                xcsbb = -(xcsgb + xcsdb + xcssb + xcsgmb);
                xcbdb = h.cbsb - h.capbd;
                xcbsb = h.cbdb - h.capbs;
                xcdbdb = 0.0;
            } else {
                xcdbb = -(xcdgb + xcddb + xcdsb + xcdgmb) + h.capbd;
                xcsbb = -(h.cddb + h.cdgb + h.cdsb);
                xcbdb = h.cbsb;
                xcbsb = h.cbdb;
                xcdbdb = -h.capbd;
                xcsbsb = -h.capbs;
            }
            xcbbb = -(xcbgb + xcbdb + xcbsb + xcbgmb);
            xgtb = 0.0;
            xgts = xgtb;
            xgtd = xgts;
            xgtg = xgtd;
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
            xcgbb = 0.0;
            xcgsb = xcgbb;
            xcgdb = xcgsb;
            xcggb = xcgdb;
            xcbbb = 0.0;
            xcbsb = xcbbb;
            xcbdb = xcbsb;
            xcbgb = xcbdb;
            xcdbb = 0.0;
            xcdsb = xcdbb;
            xcddb = xcdsb;
            xcdgb = xcddb;
            xcsbb = 0.0;
            xcssb = xcsbb;
            xcsdb = xcssb;
            xcsgb = xcsdb;
            xgtg = h.gtg;
            xgtd = h.gts;
            xgts = h.gtd;
            xgtb = h.gtb;
            xcqgb = h.cqgb;
            xcqdb = h.cqsb;
            xcqsb = h.cqdb;
            xcqbb = h.cqbb;
            coxwl = md.coxe * pp.weff_cv * h.nf * pp.leff_cv;
            qcheq = -(h.qgate + h.qbulk);
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
                sxpart = h.qdrn / qcheq;
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
        }
    }
    if md.rds_mod == 1 {
        gstot = h.gstot;
        gstotd = h.gstotd;
        gstotg = h.gstotg;
        gstots = h.gstots - gstot;
        gstotb = h.gstotb;
        gdtot = h.gdtot;
        gdtotd = h.gdtotd - gdtot;
        gdtotg = h.gdtotg;
        gdtots = h.gdtots;
        gdtotb = h.gdtotb;
    } else {
        gstotb = 0.0;
        gstots = gstotb;
        gstotg = gstots;
        gstotd = gstotg;
        gstot = gstotd;
        gdtotb = 0.0;
        gdtots = gdtotb;
        gdtotg = gdtots;
        gdtotd = gdtotg;
        gdtot = gdtotd;
    }
    t1 = ctx.states.get(0, st + ST_QDEF) * h.gtau;
    gds = h.gds;
    m = h.m;
    mult_i = h.mult_i * h.m;
    mult_q = h.mult_q * h.m;
    if md.rds_mod == 0 {
        gdpr = h.drain_conductance;
        gspr = h.source_conductance;
    } else {
        gspr = 0.0;
        gdpr = gspr;
    }
    if h.rbody_mod == 0 {
        gjbd = h.gbd;
        gjbs = h.gbs;
    } else {
        gjbs = 0.0;
        gjbd = gjbs;
    }
    geltd = h.grgeltd;
    if h.rgate_mod == 1 {
        ctx.add_g(h.gege_ptr, mult_i * geltd);
        ctx.add_g(h.gpge_ptr, -(mult_i * geltd));
        ctx.add_g(h.gegp_ptr, -(mult_i * geltd));
        ctx.add_g(h.gpgp_ptr, mult_q * xcggb * ctx.s.re);
        ctx.add_b(h.gpgp_ptr, mult_q * xcggb * ctx.s.im);
        ctx.add_g(h.gpgp_ptr, mult_i * (geltd + gigtotg) - mult_q * xgtg);
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdb * ctx.s.re);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdb * ctx.s.im);
        ctx.add_g(h.gpdp_ptr, -(mult_q * xgtd - mult_i * gigtotd));
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsb * ctx.s.re);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsb * ctx.s.im);
        ctx.add_g(h.gpsp_ptr, -(mult_q * xgts - mult_i * gigtots));
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbb * ctx.s.re);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbb * ctx.s.im);
        ctx.add_g(h.gpbp_ptr, -(mult_q * xgtb - mult_i * gigtotb));
    } else if h.rgate_mod == 2 {
        ctx.add_g(h.gege_ptr, mult_i * gcrg);
        ctx.add_g(h.gegp_ptr, mult_i * gcrgg);
        ctx.add_g(h.gedp_ptr, mult_i * gcrgd);
        ctx.add_g(h.gesp_ptr, mult_i * gcrgs);
        ctx.add_g(h.gebp_ptr, mult_i * gcrgb);
        ctx.add_g(h.gpge_ptr, -(mult_i * gcrg));
        ctx.add_g(h.gpgp_ptr, mult_q * xcggb * ctx.s.re);
        ctx.add_b(h.gpgp_ptr, mult_q * xcggb * ctx.s.im);
        ctx.add_g(h.gpgp_ptr, -(mult_i * (gcrgg - gigtotg) + mult_q * xgtg));
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdb * ctx.s.re);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdb * ctx.s.im);
        ctx.add_g(h.gpdp_ptr, -(mult_i * (gcrgd - gigtotd) + mult_q * xgtd));
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsb * ctx.s.re);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsb * ctx.s.im);
        ctx.add_g(h.gpsp_ptr, -(mult_i * (gcrgs - gigtots) + mult_q * xgts));
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbb * ctx.s.re);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbb * ctx.s.im);
        ctx.add_g(h.gpbp_ptr, -(mult_i * (gcrgb - gigtotb) + mult_q * xgtb));
    } else if h.rgate_mod == 3 {
        ctx.add_g(h.gege_ptr, mult_i * geltd);
        ctx.add_g(h.gegm_ptr, -(mult_i * geltd));
        ctx.add_g(h.gmge_ptr, -(mult_i * geltd));
        ctx.add_g(h.gmgm_ptr, mult_i * (geltd + gcrg));
        ctx.add_g(h.gmgm_ptr, mult_q * xcgmgmb * ctx.s.re);
        ctx.add_b(h.gmgm_ptr, mult_q * xcgmgmb * ctx.s.im);
        ctx.add_g(h.gmdp_ptr, mult_i * gcrgd);
        ctx.add_g(h.gmdp_ptr, mult_q * xcgmdb * ctx.s.re);
        ctx.add_b(h.gmdp_ptr, mult_q * xcgmdb * ctx.s.im);
        ctx.add_g(h.gmgp_ptr, mult_i * gcrgg);
        ctx.add_g(h.gmsp_ptr, mult_i * gcrgs);
        ctx.add_g(h.gmsp_ptr, mult_q * xcgmsb * ctx.s.re);
        ctx.add_b(h.gmsp_ptr, mult_q * xcgmsb * ctx.s.im);
        ctx.add_g(h.gmbp_ptr, mult_i * gcrgb);
        ctx.add_g(h.gmbp_ptr, mult_q * xcgmbb * ctx.s.re);
        ctx.add_b(h.gmbp_ptr, mult_q * xcgmbb * ctx.s.im);
        ctx.add_g(h.dpgm_ptr, mult_q * xcdgmb * ctx.s.re);
        ctx.add_b(h.dpgm_ptr, mult_q * xcdgmb * ctx.s.im);
        ctx.add_g(h.gpgm_ptr, -(mult_i * gcrg));
        ctx.add_g(h.spgm_ptr, mult_q * xcsgmb * ctx.s.re);
        ctx.add_b(h.spgm_ptr, mult_q * xcsgmb * ctx.s.im);
        ctx.add_g(h.bpgm_ptr, mult_q * xcbgmb * ctx.s.re);
        ctx.add_b(h.bpgm_ptr, mult_q * xcbgmb * ctx.s.im);
        ctx.add_g(h.gpgp_ptr, -(mult_i * (gcrgg - gigtotg) + mult_q * xgtg));
        ctx.add_g(h.gpgp_ptr, mult_q * xcggb * ctx.s.re);
        ctx.add_b(h.gpgp_ptr, mult_q * xcggb * ctx.s.im);
        ctx.add_g(h.gpdp_ptr, -(mult_i * (gcrgd - gigtotd) + mult_q * xgtd));
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdb * ctx.s.re);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdb * ctx.s.im);
        ctx.add_g(h.gpsp_ptr, -(mult_i * (gcrgs - gigtots) + mult_q * xgts));
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsb * ctx.s.re);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsb * ctx.s.im);
        ctx.add_g(h.gpbp_ptr, -(mult_i * (gcrgb - gigtotb) + mult_q * xgtb));
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbb * ctx.s.re);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbb * ctx.s.im);
    } else {
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdb * ctx.s.re);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdb * ctx.s.im);
        ctx.add_g(h.gpdp_ptr, -(mult_q * xgtd - mult_i * gigtotd));
        ctx.add_g(h.gpgp_ptr, mult_q * xcggb * ctx.s.re);
        ctx.add_b(h.gpgp_ptr, mult_q * xcggb * ctx.s.im);
        ctx.add_g(h.gpgp_ptr, -(mult_q * xgtg - mult_i * gigtotg));
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsb * ctx.s.re);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsb * ctx.s.im);
        ctx.add_g(h.gpsp_ptr, -(mult_q * xgts - mult_i * gigtots));
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbb * ctx.s.re);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbb * ctx.s.im);
        ctx.add_g(h.gpbp_ptr, -(mult_q * xgtb - mult_i * gigtotb));
    }
    if md.rds_mod != 0 {
        ctx.add_g(h.dgp_ptr, mult_i * gdtotg);
        ctx.add_g(h.dsp_ptr, mult_i * gdtots);
        ctx.add_g(h.dbp_ptr, mult_i * gdtotb);
        ctx.add_g(h.sdp_ptr, mult_i * gstotd);
        ctx.add_g(h.sgp_ptr, mult_i * gstotg);
        ctx.add_g(h.sbp_ptr, mult_i * gstotb);
    }
    ctx.add_g(h.dpdp_ptr, mult_q * xcddb * ctx.s.re);
    ctx.add_b(h.dpdp_ptr, mult_q * xcddb * ctx.s.im);
    ctx.add_g(
        h.dpdp_ptr,
        mult_i * (gdpr + gds + h.gbd - gdtotd + revsum + gbdpdp - gidtotd)
            + mult_q * (dxpart * xgtd + t1 * ddxpart_dvd),
    );
    ctx.add_g(h.dpd_ptr, -(mult_i * (gdpr + gdtot)));
    ctx.add_g(h.dpgp_ptr, mult_q * xcdgb * ctx.s.re);
    ctx.add_b(h.dpgp_ptr, mult_q * xcdgb * ctx.s.im);
    ctx.add_g(
        h.dpgp_ptr,
        mult_i * (gm - gdtotg + gbdpg - gidtotg) + mult_q * (t1 * ddxpart_dvg + dxpart * xgtg),
    );
    ctx.add_g(h.dpsp_ptr, mult_q * xcdsb * ctx.s.re);
    ctx.add_b(h.dpsp_ptr, mult_q * xcdsb * ctx.s.im);
    ctx.add_g(
        h.dpsp_ptr,
        -(mult_i * (gds + fwdsum + gdtots - gbdpsp + gidtots)
            - mult_q * (t1 * ddxpart_dvs + dxpart * xgts)),
    );
    ctx.add_g(h.dpbp_ptr, mult_q * xcdbb * ctx.s.re);
    ctx.add_b(h.dpbp_ptr, mult_q * xcdbb * ctx.s.im);
    ctx.add_g(
        h.dpbp_ptr,
        -(mult_i * (gjbd + gdtotb - gmbs - gbdpb + gidtotb)
            - mult_q * t1 * (ddxpart_dvb + dxpart * xgtb)),
    );
    ctx.add_g(h.ddp_ptr, -(mult_i * (gdpr - gdtotd)));
    ctx.add_g(h.dd_ptr, mult_i * (gdpr + gdtot));
    ctx.add_g(h.spdp_ptr, mult_q * xcsdb * ctx.s.re);
    ctx.add_b(h.spdp_ptr, mult_q * xcsdb * ctx.s.im);
    ctx.add_g(
        h.spdp_ptr,
        -(mult_i * (gds + gstotd + revsum - gbspdp + gistotd)
            - mult_q * (t1 * dsxpart_dvd + sxpart * xgtd)),
    );
    ctx.add_g(h.spgp_ptr, mult_q * xcsgb * ctx.s.re);
    ctx.add_b(h.spgp_ptr, mult_q * xcsgb * ctx.s.im);
    ctx.add_g(
        h.spgp_ptr,
        -(mult_i * (gm + gstotg - gbspg + gistotg) - mult_q * (t1 * dsxpart_dvg + sxpart * xgtg)),
    );
    ctx.add_g(h.spsp_ptr, mult_q * xcssb * ctx.s.re);
    ctx.add_b(h.spsp_ptr, mult_q * xcssb * ctx.s.im);
    ctx.add_g(
        h.spsp_ptr,
        mult_i * (gspr + gds + h.gbs - gistots - gstots + fwdsum + gbspsp)
            + mult_q * (sxpart * xgts + t1 * dsxpart_dvs),
    );
    ctx.add_g(h.sps_ptr, -(mult_i * (gspr + gstot)));
    ctx.add_g(h.spbp_ptr, mult_q * xcsbb * ctx.s.re);
    ctx.add_b(h.spbp_ptr, mult_q * xcsbb * ctx.s.im);
    ctx.add_g(
        h.spbp_ptr,
        -(mult_i * (gjbs + gstotb + gmbs - gbspb + gistotb)
            - mult_q * (t1 * dsxpart_dvb + sxpart * xgtb)),
    );
    ctx.add_g(h.ssp_ptr, -(mult_i * (gspr - gstots)));
    ctx.add_g(h.ss_ptr, mult_i * (gspr + gstot));
    ctx.add_g(h.bpdp_ptr, mult_q * xcbdb * ctx.s.re);
    ctx.add_b(h.bpdp_ptr, mult_q * xcbdb * ctx.s.im);
    ctx.add_g(h.bpdp_ptr, -(mult_i * (gjbd - gbbdp + gibtotd)));
    ctx.add_g(h.bpgp_ptr, mult_q * xcbgb * ctx.s.re);
    ctx.add_b(h.bpgp_ptr, mult_q * xcbgb * ctx.s.im);
    ctx.add_g(h.bpgp_ptr, -(mult_i * (h.gbgs + gibtotg)));
    ctx.add_g(h.bpsp_ptr, mult_q * xcbsb * ctx.s.re);
    ctx.add_b(h.bpsp_ptr, mult_q * xcbsb * ctx.s.im);
    ctx.add_g(h.bpsp_ptr, -(mult_i * (gjbs - gbbsp + gibtots)));
    ctx.add_g(h.bpbp_ptr, mult_q * xcbbb * ctx.s.re);
    ctx.add_b(h.bpbp_ptr, mult_q * xcbbb * ctx.s.im);
    ctx.add_g(h.bpbp_ptr, mult_i * (gjbd + gjbs - h.gbbs - gibtotb));
    ggidld = h.ggidld;
    ggidlg = h.ggidlg;
    ggidlb = h.ggidlb;
    ggislg = h.ggislg;
    ggisls = h.ggisls;
    ggislb = h.ggislb;
    ctx.add_g(h.dpdp_ptr, mult_i * ggidld);
    ctx.add_g(h.dpgp_ptr, mult_i * ggidlg);
    ctx.add_g(h.dpsp_ptr, -(mult_i * (ggidlg + ggidld + ggidlb)));
    ctx.add_g(h.dpbp_ptr, mult_i * ggidlb);
    ctx.add_g(h.bpdp_ptr, -(mult_i * ggidld));
    ctx.add_g(h.bpgp_ptr, -(mult_i * ggidlg));
    ctx.add_g(h.bpsp_ptr, mult_i * (ggidlg + ggidld + ggidlb));
    ctx.add_g(h.bpbp_ptr, -(mult_i * ggidlb));
    ctx.add_g(h.spdp_ptr, -(mult_i * (ggisls + ggislg + ggislb)));
    ctx.add_g(h.spgp_ptr, mult_i * ggislg);
    ctx.add_g(h.spsp_ptr, mult_i * ggisls);
    ctx.add_g(h.spbp_ptr, mult_i * ggislb);
    ctx.add_g(h.bpdp_ptr, mult_i * (ggislg + ggisls + ggislb));
    ctx.add_g(h.bpgp_ptr, -(mult_i * ggislg));
    ctx.add_g(h.bpsp_ptr, -(mult_i * ggisls));
    ctx.add_g(h.bpbp_ptr, -(mult_i * ggislb));
    if h.rbody_mod != 0 {
        ctx.add_g(h.dpdb_ptr, mult_q * xcdbdb * ctx.s.re);
        ctx.add_b(h.dpdb_ptr, mult_q * xcdbdb * ctx.s.im);
        ctx.add_g(h.dpdb_ptr, -(mult_i * h.gbd));
        ctx.add_g(h.spsb_ptr, mult_q * xcsbsb * ctx.s.re);
        ctx.add_b(h.spsb_ptr, mult_q * xcsbsb * ctx.s.im);
        ctx.add_g(h.spsb_ptr, -(mult_i * h.gbs));
        ctx.add_g(h.dbdp_ptr, mult_q * xcdbdb * ctx.s.re);
        ctx.add_b(h.dbdp_ptr, mult_q * xcdbdb * ctx.s.im);
        ctx.add_g(h.dbdp_ptr, -(mult_i * h.gbd));
        ctx.add_g(h.dbdb_ptr, -(mult_q * xcdbdb * ctx.s.re));
        ctx.add_b(h.dbdb_ptr, -(mult_q * xcdbdb * ctx.s.im));
        ctx.add_g(h.dbdb_ptr, mult_i * (h.gbd + h.grbpd + h.grbdb));
        ctx.add_g(h.dbbp_ptr, -(mult_i * h.grbpd));
        ctx.add_g(h.dbb_ptr, -(mult_i * h.grbdb));
        ctx.add_g(h.bpdb_ptr, -(mult_i * h.grbpd));
        ctx.add_g(h.bpb_ptr, -(mult_i * h.grbpb));
        ctx.add_g(h.bpsb_ptr, -(mult_i * h.grbps));
        ctx.add_g(h.bpbp_ptr, mult_i * (h.grbpd + h.grbps + h.grbpb));
        ctx.add_g(h.sbsp_ptr, mult_q * xcsbsb * ctx.s.re);
        ctx.add_b(h.sbsp_ptr, mult_q * xcsbsb * ctx.s.im);
        ctx.add_g(h.sbsp_ptr, -(mult_i * h.gbs));
        ctx.add_g(h.sbbp_ptr, -(mult_i * h.grbps));
        ctx.add_g(h.sbb_ptr, -(mult_i * h.grbsb));
        ctx.add_g(h.sbsb_ptr, -(mult_q * xcsbsb * ctx.s.re));
        ctx.add_b(h.sbsb_ptr, -(mult_q * xcsbsb * ctx.s.im));
        ctx.add_g(h.sbsb_ptr, mult_i * (h.gbs + h.grbps + h.grbsb));
        ctx.add_g(h.bdb_ptr, -(mult_i * h.grbdb));
        ctx.add_g(h.bbp_ptr, -(mult_i * h.grbpb));
        ctx.add_g(h.bsb_ptr, -(mult_i * h.grbsb));
        ctx.add_g(h.bb_ptr, mult_i * (h.grbsb + h.grbdb + h.grbpb));
    }
    if h.acnqs_mod != 0 {
        ctx.add_g(h.qq_ptr, m * ctx.s.re * scalingfactor);
        ctx.add_b(h.qq_ptr, m * ctx.s.im * scalingfactor);
        ctx.add_g(h.qgp_ptr, -(m * xcqgb * ctx.s.re));
        ctx.add_b(h.qgp_ptr, -(m * xcqgb * ctx.s.im));
        ctx.add_g(h.qdp_ptr, -(m * xcqdb * ctx.s.re));
        ctx.add_b(h.qdp_ptr, -(m * xcqdb * ctx.s.im));
        ctx.add_g(h.qbp_ptr, -(m * xcqbb * ctx.s.re));
        ctx.add_b(h.qbp_ptr, -(m * xcqbb * ctx.s.im));
        ctx.add_g(h.qsp_ptr, -(m * xcqsb * ctx.s.re));
        ctx.add_b(h.qsp_ptr, -(m * xcqsb * ctx.s.im));
        ctx.add_g(h.gpq_ptr, -(m * h.gtau));
        ctx.add_g(h.dpq_ptr, m * dxpart * h.gtau);
        ctx.add_g(h.spq_ptr, m * sxpart * h.gtau);
        ctx.add_g(h.qq_ptr, m * h.gtau);
        ctx.add_g(h.qgp_ptr, m * xgtg);
        ctx.add_g(h.qdp_ptr, m * xgtd);
        ctx.add_g(h.qbp_ptr, m * xgtb);
        ctx.add_g(h.qsp_ptr, m * xgts);
    }
}
