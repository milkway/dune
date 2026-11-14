// This file is a modified version of BSIM4 4.8.3: a translation into Rust,
// for the Dune circuit simulator, of b4acld.c as distributed with ngspice-47
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

//! `BSIM4acLoad` (`b4acld.c`): the small-signal stamps (real part into
//! G, imaginary part into B), including the AC NQS model.

use super::*;

#[allow(
    unused_assignments,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]
pub(super) fn ac_load(md: &Bsim4Model, pp: &Bsim4Size, h: &Bsim4Here, st: usize, ctx: &mut AcCtx) {
    let mut _capbd: f64 = 0.0;
    let mut _capbs: f64 = 0.0;
    let mut cdbi: f64 = 0.0;
    let mut cdbr: f64 = 0.0;
    let mut cddi: f64 = 0.0;
    let mut cddr: f64 = 0.0;
    let mut cdgi: f64 = 0.0;
    let mut cdgr: f64 = 0.0;
    let mut cdsi: f64 = 0.0;
    let mut cdsr: f64 = 0.0;
    let mut cgbi: f64 = 0.0;
    let mut _cgbo: f64 = 0.0;
    let mut _cgbr: f64 = 0.0;
    let mut cgdi: f64 = 0.0;
    let mut cgdo: f64 = 0.0;
    let mut cgdr: f64 = 0.0;
    let mut cggi: f64 = 0.0;
    let mut cggr: f64 = 0.0;
    let mut cgsi: f64 = 0.0;
    let mut cgso: f64 = 0.0;
    let mut cgsr: f64 = 0.0;
    let mut csbi: f64 = 0.0;
    let mut _csbr: f64 = 0.0;
    let mut csd: f64 = 0.0;
    let mut csdi: f64 = 0.0;
    let mut csdr: f64 = 0.0;
    let mut csg: f64 = 0.0;
    let mut csgi: f64 = 0.0;
    let mut csgr: f64 = 0.0;
    let mut css: f64 = 0.0;
    let mut cssi: f64 = 0.0;
    let mut cssr: f64 = 0.0;
    let mut fwdsumi: f64 = 0.0;
    let mut fwdsumr: f64 = 0.0;
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
    let mut gdsi: f64 = 0.0;
    let mut gdsr: f64 = 0.0;
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
    let mut gmbsi: f64 = 0.0;
    let mut gmbsi_: f64 = 0.0;
    let mut gmbsr: f64 = 0.0;
    let mut gmbsr_: f64 = 0.0;
    let mut gmi: f64 = 0.0;
    let mut gmi_: f64 = 0.0;
    let mut gmr: f64 = 0.0;
    let mut gmr_: f64 = 0.0;
    let mut gspr: f64 = 0.0;
    let mut gstot: f64 = 0.0;
    let mut gstotb: f64 = 0.0;
    let mut gstotd: f64 = 0.0;
    let mut gstotg: f64 = 0.0;
    let mut gstots: f64 = 0.0;
    let mut m: f64 = 0.0;
    let mut mult_i: f64 = 0.0;
    let mut mult_q: f64 = 0.0;
    let mut revsumi: f64 = 0.0;
    let mut revsumr: f64 = 0.0;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 0.0;
    let mut t2: f64 = 0.0;
    let mut t3: f64 = 0.0;
    let mut xcbbb: f64 = 0.0;
    let mut xcbdb: f64 = 0.0;
    let mut xcbgb: f64 = 0.0;
    let mut xcbgmb: f64 = 0.0;
    let mut xcbsb: f64 = 0.0;
    let mut xcdbbi: f64 = 0.0;
    let mut xcdbbr: f64 = 0.0;
    let mut xcdbdb: f64 = 0.0;
    let mut xcddbi: f64 = 0.0;
    let mut xcddbr: f64 = 0.0;
    let mut xcdgbi: f64 = 0.0;
    let mut xcdgbr: f64 = 0.0;
    let mut xcdgmb: f64 = 0.0;
    let mut xcdsbi: f64 = 0.0;
    let mut xcdsbr: f64 = 0.0;
    let mut xcgbbi: f64 = 0.0;
    let mut xcgbbr: f64 = 0.0;
    let mut xcgdbi: f64 = 0.0;
    let mut xcgdbr: f64 = 0.0;
    let mut xcggbi: f64 = 0.0;
    let mut xcggbr: f64 = 0.0;
    let mut xcgmbb: f64 = 0.0;
    let mut xcgmdb: f64 = 0.0;
    let mut xcgmgmb: f64 = 0.0;
    let mut xcgmsb: f64 = 0.0;
    let mut xcgsbi: f64 = 0.0;
    let mut xcgsbr: f64 = 0.0;
    let mut xcsbbi: f64 = 0.0;
    let mut xcsbbr: f64 = 0.0;
    let mut xcsbsb: f64 = 0.0;
    let mut xcsdbi: f64 = 0.0;
    let mut xcsdbr: f64 = 0.0;
    let mut xcsgbi: f64 = 0.0;
    let mut xcsgbr: f64 = 0.0;
    let mut xcsgmb: f64 = 0.0;
    let mut xcssbi: f64 = 0.0;
    let mut xcssbr: f64 = 0.0;
    xcsbsb = 0.0;
    xcgmgmb = 0.0;
    xcgmdb = 0.0;
    xcgmsb = 0.0;
    xcgmbb = 0.0;
    t0 = 0.0;
    _capbd = h.capbd;
    _capbs = h.capbs;
    cgso = h.cgso;
    cgdo = h.cgdo;
    _cgbo = pp.cgbo;
    csd = -(h.cddb + h.cgdb + h.cbdb);
    csg = -(h.cdgb + h.cggb + h.cbgb);
    css = -(h.cdsb + h.cgsb + h.cbsb);
    if h.acnqs_mod != 0 {
        t0 = ctx.omega * h.taunet;
        t1 = t0 * t0;
        t2 = 1.0 / (1.0 + t1);
        t3 = t0 * t2;
        gmr = h.gm * t2;
        gmbsr = h.gmbs * t2;
        gdsr = h.gds * t2;
        gmi = -h.gm * t3;
        gmbsi = -h.gmbs * t3;
        gdsi = -h.gds * t3;
        cddr = h.cddb * t2;
        cdgr = h.cdgb * t2;
        cdsr = h.cdsb * t2;
        cdbr = -(cddr + cdgr + cdsr);
        cddi = h.cddb * t3 * ctx.omega;
        cdgi = h.cdgb * t3 * ctx.omega;
        cdsi = h.cdsb * t3 * ctx.omega;
        cdbi = -(cddi + cdgi + cdsi);
        csdr = csd * t2;
        csgr = csg * t2;
        cssr = css * t2;
        _csbr = -(csdr + csgr + cssr);
        csdi = csd * t3 * ctx.omega;
        csgi = csg * t3 * ctx.omega;
        cssi = css * t3 * ctx.omega;
        csbi = -(csdi + csgi + cssi);
        cgdr = -(cddr + csdr + h.cbdb);
        cggr = -(cdgr + csgr + h.cbgb);
        cgsr = -(cdsr + cssr + h.cbsb);
        _cgbr = -(cgdr + cggr + cgsr);
        cgdi = -(cddi + csdi);
        cggi = -(cdgi + csgi);
        cgsi = -(cdsi + cssi);
        cgbi = -(cgdi + cggi + cgsi);
    } else {
        gmr = h.gm;
        gmbsr = h.gmbs;
        gdsr = h.gds;
        gdsi = 0.0;
        gmbsi = gdsi;
        gmi = gmbsi;
        cddr = h.cddb;
        cdgr = h.cdgb;
        cdsr = h.cdsb;
        cdbr = -(cddr + cdgr + cdsr);
        cdbi = 0.0;
        cdsi = cdbi;
        cdgi = cdsi;
        cddi = cdgi;
        csdr = csd;
        csgr = csg;
        cssr = css;
        _csbr = -(csdr + csgr + cssr);
        csbi = 0.0;
        cssi = csbi;
        csgi = cssi;
        csdi = csgi;
        cgdr = h.cgdb;
        cggr = h.cggb;
        cgsr = h.cgsb;
        _cgbr = -(cgdr + cggr + cgsr);
        cgbi = 0.0;
        cgsi = cgbi;
        cggi = cgsi;
        cgdi = cggi;
    }
    if h.mode >= 0 {
        gmr_ = gmr;
        gmbsr_ = gmbsr;
        fwdsumr = gmr_ + gmbsr_;
        revsumr = 0.0;
        gmi_ = gmi;
        gmbsi_ = gmbsi;
        fwdsumi = gmi_ + gmbsi_;
        revsumi = 0.0;
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
        if h.rgate_mod == 3 {
            xcgmgmb = (cgdo + cgso + pp.cgbo) * ctx.omega;
            xcgmdb = -cgdo * ctx.omega;
            xcgmsb = -cgso * ctx.omega;
            xcgmbb = -pp.cgbo * ctx.omega;
            xcdgmb = xcgmdb;
            xcsgmb = xcgmsb;
            xcbgmb = xcgmbb;
            xcggbr = cggr * ctx.omega;
            xcgdbr = cgdr * ctx.omega;
            xcgsbr = cgsr * ctx.omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);
            xcdgbr = cdgr * ctx.omega;
            xcsgbr = csgr * ctx.omega;
            xcbgb = h.cbgb * ctx.omega;
        } else {
            xcggbr = (cggr + cgdo + cgso + pp.cgbo) * ctx.omega;
            xcgdbr = (cgdr - cgdo) * ctx.omega;
            xcgsbr = (cgsr - cgso) * ctx.omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);
            xcdgbr = (cdgr - cgdo) * ctx.omega;
            xcsgbr = (csgr - cgso) * ctx.omega;
            xcbgb = (h.cbgb - pp.cgbo) * ctx.omega;
            xcbgmb = 0.0;
            xcsgmb = xcbgmb;
            xcdgmb = xcsgmb;
        }
        xcddbr = (cddr + h.capbd + cgdo) * ctx.omega;
        xcdsbr = cdsr * ctx.omega;
        xcsdbr = csdr * ctx.omega;
        xcssbr = (h.capbs + cgso + cssr) * ctx.omega;
        if h.rbody_mod == 0 {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb);
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb);
            xcbdb = (h.cbdb - h.capbd) * ctx.omega;
            xcbsb = (h.cbsb - h.capbs) * ctx.omega;
            xcdbdb = 0.0;
        } else {
            xcdbbr = cdbr * ctx.omega;
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb) + h.capbs * ctx.omega;
            xcbdb = h.cbdb * ctx.omega;
            xcbsb = h.cbsb * ctx.omega;
            xcdbdb = -h.capbd * ctx.omega;
            xcsbsb = -h.capbs * ctx.omega;
        }
        xcbbb = -(xcbdb + xcbgb + xcbsb + xcbgmb);
        xcdgbi = cdgi;
        xcsgbi = csgi;
        xcddbi = cddi;
        xcdsbi = cdsi;
        xcsdbi = csdi;
        xcssbi = cssi;
        xcdbbi = cdbi;
        xcsbbi = csbi;
        xcggbi = cggi;
        xcgdbi = cgdi;
        xcgsbi = cgsi;
        xcgbbi = cgbi;
    } else {
        gmr_ = -gmr;
        gmbsr_ = -gmbsr;
        fwdsumr = 0.0;
        revsumr = -(gmr_ + gmbsr_);
        gmi_ = -gmi;
        gmbsi_ = -gmbsi;
        fwdsumi = 0.0;
        revsumi = -(gmi_ + gmbsi_);
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
        if h.rgate_mod == 3 {
            xcgmgmb = (cgdo + cgso + pp.cgbo) * ctx.omega;
            xcgmdb = -cgdo * ctx.omega;
            xcgmsb = -cgso * ctx.omega;
            xcgmbb = -pp.cgbo * ctx.omega;
            xcdgmb = xcgmdb;
            xcsgmb = xcgmsb;
            xcbgmb = xcgmbb;
            xcggbr = cggr * ctx.omega;
            xcgdbr = cgsr * ctx.omega;
            xcgsbr = cgdr * ctx.omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);
            xcdgbr = csgr * ctx.omega;
            xcsgbr = cdgr * ctx.omega;
            xcbgb = h.cbgb * ctx.omega;
        } else {
            xcggbr = (cggr + cgdo + cgso + pp.cgbo) * ctx.omega;
            xcgdbr = (cgsr - cgdo) * ctx.omega;
            xcgsbr = (cgdr - cgso) * ctx.omega;
            xcgbbr = -(xcggbr + xcgdbr + xcgsbr);
            xcdgbr = (csgr - cgdo) * ctx.omega;
            xcsgbr = (cdgr - cgso) * ctx.omega;
            xcbgb = (h.cbgb - pp.cgbo) * ctx.omega;
            xcbgmb = 0.0;
            xcsgmb = xcbgmb;
            xcdgmb = xcsgmb;
        }
        xcddbr = (h.capbd + cgdo + cssr) * ctx.omega;
        xcdsbr = csdr * ctx.omega;
        xcsdbr = cdsr * ctx.omega;
        xcssbr = (cddr + h.capbs + cgso) * ctx.omega;
        if h.rbody_mod == 0 {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb);
            xcsbbr = -(xcsgbr + xcsdbr + xcssbr + xcsgmb);
            xcbdb = (h.cbsb - h.capbd) * ctx.omega;
            xcbsb = (h.cbdb - h.capbs) * ctx.omega;
            xcdbdb = 0.0;
        } else {
            xcdbbr = -(xcdgbr + xcddbr + xcdsbr + xcdgmb) + h.capbd * ctx.omega;
            xcsbbr = cdbr * ctx.omega;
            xcbdb = h.cbsb * ctx.omega;
            xcbsb = h.cbdb * ctx.omega;
            xcdbdb = -h.capbd * ctx.omega;
            xcsbsb = -h.capbs * ctx.omega;
        }
        xcbbb = -(xcbgb + xcbdb + xcbsb + xcbgmb);
        xcdgbi = csgi;
        xcsgbi = cdgi;
        xcddbi = cssi;
        xcdsbi = csdi;
        xcsdbi = cdsi;
        xcssbi = cddi;
        xcdbbi = csbi;
        xcsbbi = cdbi;
        xcggbi = cggi;
        xcgdbi = cgsi;
        xcgsbi = cgdi;
        xcgbbi = cgbi;
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
        ctx.add_b(h.gpgp_ptr, mult_q * xcggbr);
        ctx.add_g(h.gpgp_ptr, mult_i * (geltd + gigtotg) + mult_q * xcggbi);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdbr);
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdbi + mult_i * gigtotd);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsbr);
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsbi + mult_i * gigtots);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbbr);
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbbi + mult_i * gigtotb);
    } else if h.rgate_mod == 2 {
        ctx.add_g(h.gege_ptr, mult_i * gcrg);
        ctx.add_g(h.gegp_ptr, mult_i * gcrgg);
        ctx.add_g(h.gedp_ptr, mult_i * gcrgd);
        ctx.add_g(h.gesp_ptr, mult_i * gcrgs);
        ctx.add_g(h.gebp_ptr, mult_i * gcrgb);
        ctx.add_g(h.gpge_ptr, -(mult_i * gcrg));
        ctx.add_b(h.gpgp_ptr, mult_q * xcggbr);
        ctx.add_g(h.gpgp_ptr, -(mult_i * (gcrgg - gigtotg) - mult_q * xcggbi));
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdbr);
        ctx.add_g(h.gpdp_ptr, -(mult_i * (gcrgd - gigtotd) - mult_q * xcgdbi));
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsbr);
        ctx.add_g(h.gpsp_ptr, -(mult_i * (gcrgs - gigtots) - mult_q * xcgsbi));
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbbr);
        ctx.add_g(h.gpbp_ptr, -(mult_i * (gcrgb - gigtotb) - mult_q * xcgbbi));
    } else if h.rgate_mod == 3 {
        ctx.add_g(h.gege_ptr, mult_i * geltd);
        ctx.add_g(h.gegm_ptr, -(mult_i * geltd));
        ctx.add_g(h.gmge_ptr, -(mult_i * geltd));
        ctx.add_g(h.gmgm_ptr, mult_i * (geltd + gcrg));
        ctx.add_b(h.gmgm_ptr, mult_q * xcgmgmb);
        ctx.add_g(h.gmdp_ptr, mult_i * gcrgd);
        ctx.add_b(h.gmdp_ptr, mult_q * xcgmdb);
        ctx.add_g(h.gmgp_ptr, mult_i * gcrgg);
        ctx.add_g(h.gmsp_ptr, mult_i * gcrgs);
        ctx.add_b(h.gmsp_ptr, mult_q * xcgmsb);
        ctx.add_g(h.gmbp_ptr, mult_i * gcrgb);
        ctx.add_b(h.gmbp_ptr, mult_q * xcgmbb);
        ctx.add_b(h.dpgm_ptr, mult_q * xcdgmb);
        ctx.add_g(h.gpgm_ptr, -(mult_i * gcrg));
        ctx.add_b(h.spgm_ptr, mult_q * xcsgmb);
        ctx.add_b(h.bpgm_ptr, mult_q * xcbgmb);
        ctx.add_g(h.gpgp_ptr, -(mult_i * (gcrgg - gigtotg) - mult_q * xcggbi));
        ctx.add_b(h.gpgp_ptr, mult_q * xcggbr);
        ctx.add_g(h.gpdp_ptr, -(mult_i * (gcrgd - gigtotd) - mult_q * xcgdbi));
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdbr);
        ctx.add_g(h.gpsp_ptr, -(mult_i * (gcrgs - gigtots) - mult_q * xcgsbi));
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsbr);
        ctx.add_g(h.gpbp_ptr, -(mult_i * (gcrgb - gigtotb) - mult_q * xcgbbi));
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbbr);
    } else {
        ctx.add_b(h.gpgp_ptr, mult_q * xcggbr);
        ctx.add_g(h.gpgp_ptr, mult_q * xcggbi + mult_i * gigtotg);
        ctx.add_b(h.gpdp_ptr, mult_q * xcgdbr);
        ctx.add_g(h.gpdp_ptr, mult_q * xcgdbi + mult_i * gigtotd);
        ctx.add_b(h.gpsp_ptr, mult_q * xcgsbr);
        ctx.add_g(h.gpsp_ptr, mult_q * xcgsbi + mult_i * gigtots);
        ctx.add_b(h.gpbp_ptr, mult_q * xcgbbr);
        ctx.add_g(h.gpbp_ptr, mult_q * xcgbbi + mult_i * gigtotb);
    }
    if md.rds_mod != 0 {
        ctx.add_g(h.dgp_ptr, mult_i * gdtotg);
        ctx.add_g(h.dsp_ptr, mult_i * gdtots);
        ctx.add_g(h.dbp_ptr, mult_i * gdtotb);
        ctx.add_g(h.sdp_ptr, mult_i * gstotd);
        ctx.add_g(h.sgp_ptr, mult_i * gstotg);
        ctx.add_g(h.sbp_ptr, mult_i * gstotb);
    }
    ctx.add_b(h.dpdp_ptr, mult_q * xcddbr + mult_i * (gdsi + revsumi));
    ctx.add_g(
        h.dpdp_ptr,
        mult_i * (gdpr + gdsr + h.gbd - gdtotd + revsumr + gbdpdp - gidtotd) + mult_q * xcddbi,
    );
    ctx.add_g(h.dpd_ptr, -(mult_i * (gdpr + gdtot)));
    ctx.add_b(h.dpgp_ptr, mult_q * xcdgbr + mult_i * gmi_);
    ctx.add_g(
        h.dpgp_ptr,
        mult_i * (gmr_ - gdtotg + gbdpg - gidtotg) + mult_q * xcdgbi,
    );
    ctx.add_b(h.dpsp_ptr, mult_q * xcdsbr - mult_i * (gdsi + fwdsumi));
    ctx.add_g(
        h.dpsp_ptr,
        -(mult_i * (gdsr + fwdsumr + gdtots - gbdpsp + gidtots) - mult_q * xcdsbi),
    );
    ctx.add_b(h.dpbp_ptr, mult_q * xcdbbr + mult_i * gmbsi_);
    ctx.add_g(
        h.dpbp_ptr,
        -(mult_i * (gjbd + gdtotb - gmbsr_ - gbdpb + gidtotb) - mult_q * xcdbbi),
    );
    ctx.add_g(h.ddp_ptr, -(mult_i * (gdpr - gdtotd)));
    ctx.add_g(h.dd_ptr, mult_i * (gdpr + gdtot));
    ctx.add_b(h.spdp_ptr, mult_q * xcsdbr - mult_i * (gdsi + revsumi));
    ctx.add_g(
        h.spdp_ptr,
        -(mult_i * (gdsr + gstotd + revsumr - gbspdp + gistotd) - mult_q * xcsdbi),
    );
    ctx.add_b(h.spgp_ptr, mult_q * xcsgbr - mult_i * gmi_);
    ctx.add_g(
        h.spgp_ptr,
        -(mult_i * (gmr_ + gstotg - gbspg + gistotg) - mult_q * xcsgbi),
    );
    ctx.add_b(h.spsp_ptr, mult_q * xcssbr + mult_i * (gdsi + fwdsumi));
    ctx.add_g(
        h.spsp_ptr,
        mult_i * (gspr + gdsr + h.gbs - gstots + fwdsumr + gbspsp - gistots) + mult_q * xcssbi,
    );
    ctx.add_g(h.sps_ptr, -(mult_i * (gspr + gstot)));
    ctx.add_b(h.spbp_ptr, mult_q * xcsbbr - mult_i * gmbsi_);
    ctx.add_g(
        h.spbp_ptr,
        -(mult_i * (gjbs + gstotb + gmbsr_ - gbspb + gistotb) - mult_q * xcsbbi),
    );
    ctx.add_g(h.ssp_ptr, -(mult_i * (gspr - gstots)));
    ctx.add_g(h.ss_ptr, mult_i * (gspr + gstot));
    ctx.add_b(h.bpdp_ptr, mult_q * xcbdb);
    ctx.add_g(h.bpdp_ptr, -(mult_i * (gjbd - gbbdp + gibtotd)));
    ctx.add_b(h.bpgp_ptr, mult_q * xcbgb);
    ctx.add_g(h.bpgp_ptr, -(mult_i * (h.gbgs + gibtotg)));
    ctx.add_b(h.bpsp_ptr, mult_q * xcbsb);
    ctx.add_g(h.bpsp_ptr, -(mult_i * (gjbs - gbbsp + gibtots)));
    ctx.add_b(h.bpbp_ptr, mult_q * xcbbb);
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
        ctx.add_b(h.dpdb_ptr, mult_q * xcdbdb);
        ctx.add_g(h.dpdb_ptr, -(mult_i * h.gbd));
        ctx.add_b(h.spsb_ptr, mult_q * xcsbsb);
        ctx.add_g(h.spsb_ptr, -(mult_i * h.gbs));
        ctx.add_b(h.dbdp_ptr, mult_q * xcdbdb);
        ctx.add_g(h.dbdp_ptr, -(mult_i * h.gbd));
        ctx.add_b(h.dbdb_ptr, -(mult_q * xcdbdb));
        ctx.add_g(h.dbdb_ptr, mult_i * (h.gbd + h.grbpd + h.grbdb));
        ctx.add_g(h.dbbp_ptr, -(mult_i * h.grbpd));
        ctx.add_g(h.dbb_ptr, -(mult_i * h.grbdb));
        ctx.add_g(h.bpdb_ptr, -(mult_i * h.grbpd));
        ctx.add_g(h.bpb_ptr, -(mult_i * h.grbpb));
        ctx.add_g(h.bpsb_ptr, -(mult_i * h.grbps));
        ctx.add_g(h.bpbp_ptr, mult_i * (h.grbpd + h.grbps + h.grbpb));
        ctx.add_b(h.sbsp_ptr, mult_q * xcsbsb);
        ctx.add_g(h.sbsp_ptr, -(mult_i * h.gbs));
        ctx.add_g(h.sbbp_ptr, -(mult_i * h.grbps));
        ctx.add_g(h.sbb_ptr, -(mult_i * h.grbsb));
        ctx.add_b(h.sbsb_ptr, -(mult_q * xcsbsb));
        ctx.add_g(h.sbsb_ptr, mult_i * (h.gbs + h.grbps + h.grbsb));
        ctx.add_g(h.bdb_ptr, -(mult_i * h.grbdb));
        ctx.add_g(h.bbp_ptr, -(mult_i * h.grbpb));
        ctx.add_g(h.bsb_ptr, -(mult_i * h.grbsb));
        ctx.add_g(h.bb_ptr, mult_i * (h.grbsb + h.grbdb + h.grbpb));
    }
    if h.trnqs_mod != 0 {
        ctx.add_g(h.qq_ptr, m * 1.0);
        ctx.add_g(h.qgp_ptr, 0.0);
        ctx.add_g(h.qdp_ptr, 0.0);
        ctx.add_g(h.qsp_ptr, 0.0);
        ctx.add_g(h.qbp_ptr, 0.0);
        ctx.add_g(h.dpq_ptr, 0.0);
        ctx.add_g(h.spq_ptr, 0.0);
        ctx.add_g(h.gpq_ptr, 0.0);
    }
}
