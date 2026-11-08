//! BSIM2 — level 5 in the 3f5 (`dev/bsim2/b2ld.c`, `b2eval.c`,
//! `b2temp.c`, `b2set.c`, `b2mpar.c`; the load shell is BSIM1's
//! with the size-dependent parameters in a per-geometry struct).
//! The drain current blends a subthreshold exponential and the
//! strong-inversion law through a cubic spline on `Vgeff` between
//! `VGLOW` and `VGHIGH`, with vertical-field mobility (`UA`/`UB`),
//! velocity saturation (`U1`), the tanh Beta interpolation up to
//! `VDD`, and the `AI`/`BI` impact-ionization factor. ngspice-47
//! carries this code unchanged.

use crate::consts::{ROOT2, VT0};
use crate::diode::pnjlim;
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{MosGeom, fetlim, limvds};

/// `.model … nmos(level=5 …)` parameters, 3f5 names (`b2.c`).
/// [`Bsim2Model::new`] carries the `b2set.c` defaults; junction
/// potentials are clamped to ≥ 0.1 like BSIM1's.
#[derive(Clone, Debug, Default)]
pub struct Bsim2Model {
    pub nmos: bool,
    /// `VFB` and its L/W dependences.
    pub vfb: f64,
    pub lvfb: f64,
    pub wvfb: f64,
    /// `PHI` and its L/W dependences.
    pub phi: f64,
    pub lphi: f64,
    pub wphi: f64,
    /// `K1` and its L/W dependences.
    pub k1: f64,
    pub lk1: f64,
    pub wk1: f64,
    /// `K2` and its L/W dependences.
    pub k2: f64,
    pub lk2: f64,
    pub wk2: f64,
    /// `ETA0` and its L/W dependences.
    pub eta0: f64,
    pub leta0: f64,
    pub weta0: f64,
    /// `ETAB` and its L/W dependences.
    pub etab: f64,
    pub letab: f64,
    pub wetab: f64,
    /// `MU0B` and its L/W dependences.
    pub mu0b: f64,
    pub lmu0b: f64,
    pub wmu0b: f64,
    /// `MUS0` and its L/W dependences.
    pub mus0: f64,
    pub lmus0: f64,
    pub wmus0: f64,
    /// `MUSB` and its L/W dependences.
    pub musb: f64,
    pub lmusb: f64,
    pub wmusb: f64,
    /// `MU20` and its L/W dependences.
    pub mu20: f64,
    pub lmu20: f64,
    pub wmu20: f64,
    /// `MU2B` and its L/W dependences.
    pub mu2b: f64,
    pub lmu2b: f64,
    pub wmu2b: f64,
    /// `MU2G` and its L/W dependences.
    pub mu2g: f64,
    pub lmu2g: f64,
    pub wmu2g: f64,
    /// `MU30` and its L/W dependences.
    pub mu30: f64,
    pub lmu30: f64,
    pub wmu30: f64,
    /// `MU3B` and its L/W dependences.
    pub mu3b: f64,
    pub lmu3b: f64,
    pub wmu3b: f64,
    /// `MU3G` and its L/W dependences.
    pub mu3g: f64,
    pub lmu3g: f64,
    pub wmu3g: f64,
    /// `MU40` and its L/W dependences.
    pub mu40: f64,
    pub lmu40: f64,
    pub wmu40: f64,
    /// `MU4B` and its L/W dependences.
    pub mu4b: f64,
    pub lmu4b: f64,
    pub wmu4b: f64,
    /// `MU4G` and its L/W dependences.
    pub mu4g: f64,
    pub lmu4g: f64,
    pub wmu4g: f64,
    /// `UA0` and its L/W dependences.
    pub ua0: f64,
    pub lua0: f64,
    pub wua0: f64,
    /// `UAB` and its L/W dependences.
    pub uab: f64,
    pub luab: f64,
    pub wuab: f64,
    /// `UB0` and its L/W dependences.
    pub ub0: f64,
    pub lub0: f64,
    pub wub0: f64,
    /// `UBB` and its L/W dependences.
    pub ubb: f64,
    pub lubb: f64,
    pub wubb: f64,
    /// `U10` and its L/W dependences.
    pub u10: f64,
    pub lu10: f64,
    pub wu10: f64,
    /// `U1B` and its L/W dependences.
    pub u1b: f64,
    pub lu1b: f64,
    pub wu1b: f64,
    /// `U1D` and its L/W dependences.
    pub u1d: f64,
    pub lu1d: f64,
    pub wu1d: f64,
    /// `N0` and its L/W dependences.
    pub n0: f64,
    pub ln0: f64,
    pub wn0: f64,
    /// `NB` and its L/W dependences.
    pub nb: f64,
    pub lnb: f64,
    pub wnb: f64,
    /// `ND` and its L/W dependences.
    pub nd: f64,
    pub lnd: f64,
    pub wnd: f64,
    /// `VOF0` and its L/W dependences.
    pub vof0: f64,
    pub lvof0: f64,
    pub wvof0: f64,
    /// `VOFB` and its L/W dependences.
    pub vofb: f64,
    pub lvofb: f64,
    pub wvofb: f64,
    /// `VOFD` and its L/W dependences.
    pub vofd: f64,
    pub lvofd: f64,
    pub wvofd: f64,
    /// `AI0` and its L/W dependences.
    pub ai0: f64,
    pub lai0: f64,
    pub wai0: f64,
    /// `AIB` and its L/W dependences.
    pub aib: f64,
    pub laib: f64,
    pub waib: f64,
    /// `BI0` and its L/W dependences.
    pub bi0: f64,
    pub lbi0: f64,
    pub wbi0: f64,
    /// `BIB` and its L/W dependences.
    pub bib: f64,
    pub lbib: f64,
    pub wbib: f64,
    /// `VGHIGH` and its L/W dependences.
    pub vghigh: f64,
    pub lvghigh: f64,
    pub wvghigh: f64,
    /// `VGLOW` and its L/W dependences.
    pub vglow: f64,
    pub lvglow: f64,
    pub wvglow: f64,
    /// `MU0` (no L/W triple of its own).
    pub mu0: f64,
    /// `DL`/`DW` in µm; `TOX` in µm.
    pub dl: f64,
    pub dw: f64,
    pub tox: f64,
    /// `TEMP` in °C (BSIM2 uses it for its own Vtm).
    pub temp: f64,
    /// `VDD`/`VGG`/`VBB`: measurement ranges.
    pub vdd: f64,
    pub vgg: f64,
    pub vbb: f64,
    /// Overlap capacitances per meter.
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    /// `XPART` (`channelChargePartitionFlag`).
    pub xpart: f64,
    pub rsh: f64,
    pub js: f64,
    pub pb: f64,
    pub mj: f64,
    pub pbsw: f64,
    pub mjsw: f64,
    pub cj: f64,
    pub cjsw: f64,
    /// `WDF`/`DELL` (accepted, unused by the load).
    pub wdf: f64,
    pub dell: f64,
    /// `KF`/`AF` (noise only; ngspice-47's `b2noi.c` — the 3f5's
    /// BSIM2 has no noise and no such parameters). Defaults 0 and 1;
    /// `new` sets `AF`, the derived `Default` leaves it 0.
    pub kf: f64,
    pub af: f64,
}

impl Bsim2Model {
    /// The `b2set.c` defaults (everything not listed is 0).
    pub fn new(nmos: bool) -> Self {
        Bsim2Model {
            nmos,
            vfb: -1.0,
            phi: 0.75,
            k1: 0.8,
            ua0: 0.2,
            u10: 0.1,
            mu0: 400.0,
            mus0: 500.0,
            mu20: 1.5,
            mu30: 10.0,
            n0: 1.4,
            nb: 0.5,
            vof0: 1.8,
            vghigh: 0.2,
            vglow: -0.15,
            tox: 0.03,
            temp: 27.0,
            vdd: 5.0,
            vgg: 5.0,
            // Positive in the 3f5 too (`b2set.c`), though it is a
            // lower Vbs clamp.
            vbb: 5.0,
            wdf: 10.0,
            af: 1.0,
            ..Default::default()
        }
    }

    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rsh != 0.0 && squares != 0.0
    }
}

/// Per-geometry parameters (`bsim2SizeDependParam`, `b2temp.c`):
/// `p0 + pL·(1e-6/EffL) + pW·(1e-6/EffW)` plus the derived
/// constants the load needs.
#[derive(Debug)]
pub(crate) struct Bsim2Size {
    pub type_: f64,
    pub vfb: f64,
    pub phi: f64,
    pub k1: f64,
    pub k2: f64,
    pub eta0: f64,
    pub eta_b: f64,
    pub beta0: f64,
    pub beta0_b: f64,
    pub betas0: f64,
    pub betas_b: f64,
    pub beta20: f64,
    pub beta2_b: f64,
    pub beta2_g: f64,
    pub beta30: f64,
    pub beta3_b: f64,
    pub beta3_g: f64,
    pub beta40: f64,
    pub beta4_b: f64,
    pub beta4_g: f64,
    pub ua0: f64,
    pub ua_b: f64,
    pub ub0: f64,
    pub ub_b: f64,
    pub u10: f64,
    pub u1_b: f64,
    pub u1_d: f64,
    pub n0: f64,
    pub n_b: f64,
    pub n_d: f64,
    pub vof0: f64,
    pub vof_b: f64,
    pub vof_d: f64,
    pub ai0: f64,
    pub ai_b: f64,
    pub bi0: f64,
    pub bi_b: f64,
    pub vghigh: f64,
    pub vglow: f64,
    pub cox_wl: f64,
    pub one_third_cox_wl: f64,
    pub two_third_cox_wl: f64,
    pub gs_overlap: f64,
    pub gd_overlap: f64,
    pub gb_overlap: f64,
    /// The overlap capacitances `B2load` recomputes per call: drawn
    /// W for gate-source/drain and effective L for gate-bulk (the
    /// `pParam` ones above, on effective W, feed only `B2acLoad`).
    pub ld_gs_overlap: f64,
    pub ld_gd_overlap: f64,
    pub ld_gb_overlap: f64,
    pub phis3: f64,
    /// `pParam->Arg`: dBeta1/dVbs.
    pub arg: f64,
    pub vt0: f64,
    pub drain_conductance: f64,
    pub source_conductance: f64,
    // Model-level copies.
    pub vdd: f64,
    pub vdd2: f64,
    pub vgg2: f64,
    pub vbb2: f64,
    /// `B2Vtm = 8.625e-5·(temp°C + 273)`.
    pub vtm: f64,
    pub js: f64,
    pub pb: f64,
    pub pbsw: f64,
    pub mj: f64,
    pub mjsw: f64,
    pub cj: f64,
    pub cjsw: f64,
    pub xpart: f64,
    /// Noise: `KF`, `AF`, `B2Cox` (F/cm²) and `DW`/`DL` (µm).
    pub kf: f64,
    pub af: f64,
    pub cox: f64,
    pub delta_w: f64,
    pub delta_l: f64,
}

impl Bsim2Size {
    pub fn new(name: &str, m: &Bsim2Model, g: &MosGeom) -> Result<Self, String> {
        let typ = if m.nmos { 1.0 } else { -1.0 };
        let pb = f64::max(m.pb, 0.1);
        let pbsw = f64::max(m.pbsw, 0.1);
        if m.tox <= 0.0 {
            return Err(format!("{name}: BSIM2 model needs TOX > 0"));
        }
        let cox = 3.453e-13 / (m.tox * 1.0e-4); // F/cm²

        let eff_l = g.l - m.dl * 1e-6;
        let eff_w = g.w - m.dw * 1e-6;
        if eff_l <= 0.0 {
            return Err(format!("{name}: effective channel length <= 0"));
        }
        if eff_w <= 0.0 {
            return Err(format!("{name}: effective channel width <= 0"));
        }
        let inv_l = 1.0e-6 / eff_l;
        let inv_w = 1.0e-6 / eff_w;
        let p = |p0: f64, pl: f64, pw: f64| p0 + pl * inv_l + pw * inv_w;

        let vfb = p(m.vfb, m.lvfb, m.wvfb);
        let phi = p(m.phi, m.lphi, m.wphi);
        let k1 = p(m.k1, m.lk1, m.wk1);
        let k2 = p(m.k2, m.lk2, m.wk2);
        let mut beta0 = m.mu0;
        let beta0_b = p(m.mu0b, m.lmu0b, m.wmu0b);
        let mut betas0 = p(m.mus0, m.lmus0, m.wmus0);
        if betas0 < 1.01 * beta0 {
            betas0 = 1.01 * beta0;
        }
        let mut betas_b = p(m.musb, m.lmusb, m.wmusb);
        let tmp = betas0 - beta0 - beta0_b * m.vbb;
        if -betas_b * m.vbb > tmp {
            betas_b = -tmp / m.vbb;
        }
        let beta20 = p(m.mu20, m.lmu20, m.wmu20);
        let beta2_b = p(m.mu2b, m.lmu2b, m.wmu2b);
        let beta2_g = p(m.mu2g, m.lmu2g, m.wmu2g);
        let mut beta30 = p(m.mu30, m.lmu30, m.wmu30);
        let mut beta3_b = p(m.mu3b, m.lmu3b, m.wmu3b);
        let beta3_g = p(m.mu3g, m.lmu3g, m.wmu3g);
        let mut beta40 = p(m.mu40, m.lmu40, m.wmu40);
        let mut beta4_b = p(m.mu4b, m.lmu4b, m.wmu4b);
        let beta4_g = p(m.mu4g, m.lmu4g, m.wmu4g);

        let cox_w_over_l = cox * eff_w / eff_l;
        beta0 *= cox_w_over_l;
        let beta0_b = beta0_b * cox_w_over_l;
        betas0 *= cox_w_over_l;
        betas_b *= cox_w_over_l;
        beta30 *= cox_w_over_l;
        beta3_b *= cox_w_over_l;
        let beta3_g = beta3_g * cox_w_over_l;
        beta40 *= cox_w_over_l;
        beta4_b *= cox_w_over_l;
        let beta4_g = beta4_g * cox_w_over_l;

        let mut n0 = p(m.n0, m.ln0, m.wn0);
        if n0 < 0.0 {
            n0 = 0.0;
        }

        let sqrt_phi = phi.sqrt();
        let phis3 = sqrt_phi * phi;
        let cox_wl = cox * eff_l * eff_w * 1.0e4;
        let series = |squares: f64| {
            let r = m.rsh * squares;
            if r != 0.0 { 1.0 / r } else { 0.0 }
        };
        let vt0 = vfb + phi + k1 * sqrt_phi - k2 * phi;

        Ok(Bsim2Size {
            type_: typ,
            vfb,
            phi,
            k1,
            k2,
            eta0: p(m.eta0, m.leta0, m.weta0),
            eta_b: p(m.etab, m.letab, m.wetab),
            beta0,
            beta0_b,
            betas0,
            betas_b,
            beta20,
            beta2_b,
            beta2_g,
            beta30,
            beta3_b,
            beta3_g,
            beta40,
            beta4_b,
            beta4_g,
            ua0: p(m.ua0, m.lua0, m.wua0),
            ua_b: p(m.uab, m.luab, m.wuab),
            ub0: p(m.ub0, m.lub0, m.wub0),
            ub_b: p(m.ubb, m.lubb, m.wubb),
            u10: p(m.u10, m.lu10, m.wu10),
            u1_b: p(m.u1b, m.lu1b, m.wu1b),
            u1_d: p(m.u1d, m.lu1d, m.wu1d),
            n0,
            n_b: p(m.nb, m.lnb, m.wnb),
            n_d: p(m.nd, m.lnd, m.wnd),
            vof0: p(m.vof0, m.lvof0, m.wvof0),
            vof_b: p(m.vofb, m.lvofb, m.wvofb),
            vof_d: p(m.vofd, m.lvofd, m.wvofd),
            ai0: p(m.ai0, m.lai0, m.wai0),
            ai_b: p(m.aib, m.laib, m.waib),
            bi0: p(m.bi0, m.lbi0, m.wbi0),
            bi_b: p(m.bib, m.lbib, m.wbib),
            vghigh: p(m.vghigh, m.lvghigh, m.wvghigh),
            vglow: p(m.vglow, m.lvglow, m.wvglow),
            cox_wl,
            one_third_cox_wl: cox_wl / 3.0,
            two_third_cox_wl: 2.0 * cox_wl / 3.0,
            gs_overlap: m.cgso * eff_w,
            gd_overlap: m.cgdo * eff_w,
            gb_overlap: m.cgbo * eff_l,
            ld_gs_overlap: m.cgso * g.w,
            ld_gd_overlap: m.cgdo * g.w,
            ld_gb_overlap: m.cgbo * eff_l,
            phis3,
            arg: betas_b - beta0_b - m.vdd * (beta3_b - m.vdd * beta4_b),
            vt0,
            drain_conductance: series(g.nrd),
            source_conductance: series(g.nrs),
            vdd: m.vdd,
            vdd2: 2.0 * m.vdd,
            vgg2: 2.0 * m.vgg,
            vbb2: 2.0 * m.vbb,
            vtm: 8.625e-5 * (m.temp + 273.0),
            js: m.js,
            pb,
            pbsw,
            mj: m.mj,
            mjsw: m.mjsw,
            cj: m.cj,
            cjsw: m.cjsw,
            xpart: m.xpart,
            kf: m.kf,
            af: m.af,
            cox,
            delta_w: m.dw,
            delta_l: m.dl,
        })
    }
}

// State offsets (`bsim2def.h` — identical to BSIM1's layout).
const VBD: usize = 0;
const VBS: usize = 1;
const VGS: usize = 2;
const VDS: usize = 3;
const CD: usize = 4;
const CBS: usize = 5;
const CBD: usize = 6;
const GM: usize = 7;
const GDS: usize = 8;
const GMBS: usize = 9;
const GBD: usize = 10;
const GBS: usize = 11;
const QB: usize = 12;
const IQB: usize = 13;
const QG: usize = 14;
const IQG: usize = 15;
const QD: usize = 16;
const IQD: usize = 17;
const CGGB: usize = 18;
const CGDB: usize = 19;
const CGSB: usize = 20;
const CBGB: usize = 21;
const CBDB: usize = 22;
const CBSB: usize = 23;
const CAPBD: usize = 24;
const CAPBS: usize = 26;
const CDGB: usize = 28;
const CDDB: usize = 29;
const CDSB: usize = 30;
const QBS: usize = 33;
const QBD: usize = 34;
pub(crate) const BSIM2_NUM_STATES: usize = 35;

// Slot order (`b2set.c` — same as BSIM1/MOS1).
const D_D: usize = 0;
const G_G: usize = 1;
const S_S: usize = 2;
const B_B: usize = 3;
const DP_DP: usize = 4;
const SP_SP: usize = 5;
const D_DP: usize = 6;
const G_B: usize = 7;
const G_DP: usize = 8;
const G_SP: usize = 9;
const S_SP: usize = 10;
const B_DP: usize = 11;
const B_SP: usize = 12;
const DP_SP: usize = 13;
const DP_D: usize = 14;
const B_G: usize = 15;
const DP_G: usize = 16;
const SP_G: usize = 17;
const SP_S: usize = 18;
const DP_B: usize = 19;
const SP_B: usize = 20;
const SP_DP: usize = 21;

/// Everything `B2evaluate` hands back.
#[derive(Clone, Copy, Default)]
struct Eval {
    gm: f64,
    gds: f64,
    gmbs: f64,
    qg: f64,
    qb: f64,
    qd: f64,
    cggb: f64,
    cgdb: f64,
    cgsb: f64,
    cbgb: f64,
    cbdb: f64,
    cbsb: f64,
    cdgb: f64,
    cddb: f64,
    cdsb: f64,
    cdrain: f64,
    von: f64,
    vdsat: f64,
}

pub(crate) struct Bsim2I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Bsim2Size,
    geom: MosGeom,
    off: bool,
    ic_vds: f64,
    ic_vgs: f64,
    ic_vbs: f64,
    ic_given: crate::devices::IcGiven,
    st: usize,
    s: [Slot; 22],
    mode: f64,
    von: f64,
    vdsat: f64,
}

impl Bsim2I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],
        primes: [Var; 2],
        t: Bsim2Size,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64),
        st: usize,
    ) -> Self {
        let von = t.vt0;
        Bsim2I {
            drain: nodes[0],
            gate: nodes[1],
            source: nodes[2],
            bulk: nodes[3],
            d_p: primes[0],
            s_p: primes[1],
            t,
            geom,
            off,
            ic_vds: ic.0,
            ic_vgs: ic.1,
            ic_vbs: ic.2,
            ic_given: crate::devices::IcGiven::ALL,
            st,
            s: [TRASH; 22],
            mode: 1.0,
            von,
            vdsat: 0.0,
        }
    }
}

impl Bsim2I {
    /// `B2evaluate`: drain current, derivatives and terminal
    /// charges at `vds ≥ 0` (the caller swaps terminals for the
    /// inverse mode). The diagnostic `valuetypeflag` modes of the C
    /// (negative XPART values turning Ids into a probe of gm, qg,
    /// …) are not carried; every flag gets the plain
    /// `Ids = max(Ids, 1e-50)` clamp.
    #[allow(clippy::too_many_lines)]
    fn evaluate(&self, vds_in: f64, vbs_in: f64, vgs_in: f64, charges: bool) -> Eval {
        let p = &self.t;
        let mut e = Eval::default();

        // Range clamps (`b2eval.c` top).
        let vbs = f64::max(vbs_in, p.vbb2);
        let vgs = f64::min(vgs_in, p.vgg2);
        let vds = f64::min(vds_in, p.vdd2);

        // Threshold voltage.
        let (phisb, d_phisb_dvb, t1s, d_t1s_dvb);
        if vbs <= 0.0 {
            phisb = p.phi - vbs;
            d_phisb_dvb = -1.0;
            t1s = phisb.sqrt();
            d_t1s_dvb = -0.5 / t1s;
        } else {
            let tmp = p.phi / (p.phi + vbs);
            phisb = p.phi * tmp;
            d_phisb_dvb = -tmp * tmp;
            t1s = p.phis3 / (p.phi + 0.5 * vbs);
            d_t1s_dvb = -0.5 * t1s * t1s / p.phis3;
        }
        let eta = p.eta0 + p.eta_b * vbs;
        let ua = p.ua0 + p.ua_b * vbs;
        let ub = p.ub0 + p.ub_b * vbs;
        let u1s = p.u10 + p.u1_b * vbs;
        let vth = p.vfb + p.phi + p.k1 * t1s - p.k2 * phisb - eta * vds;
        e.von = vth;
        let d_vth_dvd = -eta;
        let d_vth_dvb = p.k1 * d_t1s_dvb + p.k2 - p.eta_b * vds;
        let vgst = vgs - vth;
        let tmp = 1.0 / (1.744 + 0.8364 * phisb);
        let gg = 1.0 - tmp;
        let d_gg_dvb = 0.8364 * tmp * tmp * d_phisb_dvb;
        let t0 = gg / t1s;
        let tmp1 = 0.5 * t0 * p.k1;
        let aa = 1.0 + tmp1;
        let d_aa_dvb = (aa - 1.0) * (d_gg_dvb / gg - d_t1s_dvb / t1s);
        let inv_aa = 1.0 / aa;
        let vghigh = p.vghigh;
        let vglow = p.vglow;

        // Effective gate drive: direct, exponential subthreshold,
        // or the cubic spline between VGLOW and VGHIGH.
        let (vgeff, d_vgeff_dvg, d_vgeff_dvd, d_vgeff_dvb);
        let mut exp0 = 0.0;
        let mut exp1 = 0.0;
        let mut n = 0.0;
        if vgst >= vghigh || p.n0 == 0.0 {
            vgeff = vgst;
            d_vgeff_dvg = 1.0;
            d_vgeff_dvd = -d_vth_dvd;
            d_vgeff_dvb = -d_vth_dvb;
        } else {
            let vof = p.vof0 + p.vof_b * vbs + p.vof_d * vds;
            n = p.n0 + p.n_b / t1s + p.n_d * vds;
            let tmp = 0.5 / (n * p.vtm);
            let exparg1 = f64::max(-vds / p.vtm, -30.0);
            exp1 = exparg1.exp();
            let tmp1 = f64::max(1.0 - exp1, 1.0e-18);
            let tmp2 = 2.0 * aa * tmp1;
            if vgst <= vglow {
                let exparg = f64::max(vgst * tmp, -30.0);
                exp0 = (0.5 * vof + exparg).exp();
                vgeff = tmp2.sqrt() * p.vtm * exp0;
                let t0 = n * p.vtm;
                d_vgeff_dvg = vgeff * tmp;
                d_vgeff_dvd =
                    d_vgeff_dvg * (n / tmp1 * exp1 - d_vth_dvd - vgst * p.n_d / n + t0 * p.vof_d);
                d_vgeff_dvb = d_vgeff_dvg
                    * (p.vof_b * t0 - d_vth_dvb
                        + p.n_b * vgst / (n * t1s * t1s) * d_t1s_dvb
                        + t0 * inv_aa * d_aa_dvb);
            } else {
                let exparg = f64::max(vglow * tmp, -30.0);
                exp0 = (0.5 * vof + exparg).exp();
                let con3 = (2.0 * aa * (1.0 - exp1)).sqrt() * p.vtm * exp0;
                let con1 = vghigh;
                let con4 = con3 * tmp;
                let sqr_high = vghigh * vghigh;
                let sqr_low = vglow * vglow;
                let cub_high = vghigh * sqr_high;
                let cub_low = vglow * sqr_low;
                let t0 = 2.0 * vghigh;
                let t1 = 2.0 * vglow;
                let t2 = 3.0 * sqr_high;
                let t3 = 3.0 * sqr_low;
                let t4 = vghigh - vglow;
                let t5 = sqr_high - sqr_low;
                let t6 = cub_high - cub_low;
                let t7 = con1 - con3;
                let delta = 1.0 / ((t1 - t0) * t6 + (t2 - t3) * t5 + (t0 * t3 - t1 * t2) * t4);
                let coeffb =
                    (t1 - con4 * t0) * t6 + (con4 * t2 - t3) * t5 + (t0 * t3 - t1 * t2) * t7;
                let coeffc = (con4 - 1.0) * t6 + (t2 - t3) * t7 + (t3 - con4 * t2) * t4;
                let coeffd = (t1 - t0) * t7 + (1.0 - con4) * t5 + (con4 * t0 - t1) * t4;
                let coeffa = sqr_high * (coeffc + coeffd * t0);
                vgeff = (coeffa + vgst * (coeffb + vgst * (coeffc + vgst * coeffd))) * delta;
                d_vgeff_dvg = (coeffb + vgst * (2.0 * coeffc + 3.0 * vgst * coeffd)) * delta;
                let t7b = con3 * tmp;
                let t8 = d_t1s_dvb * p.n_b / (t1s * t1s * n);
                let t9 = n * p.vtm;
                let d_con3_dvd = t7b * (n * exp1 / tmp1 - vglow * p.n_d / n + t9 * p.vof_d);
                let d_con3_dvb = t7b * (t9 * inv_aa * d_aa_dvb + vglow * t8 + t9 * p.vof_b);
                let d_con4_dvd = tmp * d_con3_dvd - t7b * p.n_d / n;
                let d_con4_dvb = tmp * d_con3_dvb + t7b * t8;
                let d_coeffb_dvd =
                    d_con4_dvd * (t2 * t5 - t0 * t6) + d_con3_dvd * (t1 * t2 - t0 * t3);
                let d_coeffc_dvd = d_con4_dvd * (t6 - t2 * t4) + d_con3_dvd * (t3 - t2);
                let d_coeffd_dvd = d_con4_dvd * (t0 * t4 - t5) + d_con3_dvd * (t0 - t1);
                let d_coeffa_dvd = sqr_high * (d_coeffc_dvd + d_coeffd_dvd * t0);
                d_vgeff_dvd = -d_vgeff_dvg * d_vth_dvd
                    + (d_coeffa_dvd
                        + vgst * (d_coeffb_dvd + vgst * (d_coeffc_dvd + vgst * d_coeffd_dvd)))
                        * delta;
                let d_coeffb_dvb =
                    d_con4_dvb * (t2 * t5 - t0 * t6) + d_con3_dvb * (t1 * t2 - t0 * t3);
                let d_coeffc_dvb = d_con4_dvb * (t6 - t2 * t4) + d_con3_dvb * (t3 - t2);
                let d_coeffd_dvb = d_con4_dvb * (t0 * t4 - t5) + d_con3_dvb * (t0 - t1);
                let d_coeffa_dvb = sqr_high * (d_coeffc_dvb + d_coeffd_dvb * t0);
                d_vgeff_dvb = -d_vgeff_dvg * d_vth_dvb
                    + (d_coeffa_dvb
                        + vgst * (d_coeffb_dvb + vgst * (d_coeffc_dvb + vgst * d_coeffd_dvb)))
                        * delta;
            }
        }

        let mut ids = 0.0;
        let mut gm = 0.0;
        let mut gds = 0.0;
        let mut gmb = 0.0;
        let mut vdsat = 0.0;
        // The charge block reuses these when `Vgst >= VGHIGH` (the C
        // keeps the DC section's values in the same locals).
        let (mut dc_dvdsat_dvd, mut dc_dvdsat_dvg, mut dc_dvdsat_dvb) = (0.0, 0.0, 0.0);

        if vgeff > 0.0 {
            let mut uvert = 1.0 + vgeff * (ua + vgeff * ub);
            uvert = f64::max(uvert, 0.2);
            let inv_uvert = 1.0 / uvert;
            let t8 = ua + 2.0 * ub * vgeff;
            let d_uvert_dvg = t8 * d_vgeff_dvg;
            let d_uvert_dvd = t8 * d_vgeff_dvd;
            let d_uvert_dvb = t8 * d_vgeff_dvb + vgeff * (p.ua_b + vgeff * p.ub_b);
            let t8 = u1s * inv_aa * inv_uvert;
            let vc = t8 * vgeff;
            let t9 = vc * inv_uvert;
            let d_vc_dvg = t8 * d_vgeff_dvg - t9 * d_uvert_dvg;
            let d_vc_dvd = t8 * d_vgeff_dvd - t9 * d_uvert_dvd;
            let d_vc_dvb = t8 * d_vgeff_dvb + p.u1_b * vgeff * inv_aa * inv_uvert
                - vc * inv_aa * d_aa_dvb
                - t9 * d_uvert_dvb;
            let tmp2 = (1.0 + 2.0 * vc).sqrt();
            let kk = 0.5 * (1.0 + vc + tmp2);
            let inv_kk = 1.0 / kk;
            let d_kk_dvc = 0.5 + 0.5 / tmp2;
            let sqrt_kk = kk.sqrt();
            let t8 = inv_aa / sqrt_kk;
            vdsat = f64::max(vgeff * t8, 1.0e-18);
            let inv_vdsat = 1.0 / vdsat;
            let t9 = 0.5 * vdsat * inv_kk * d_kk_dvc;
            let d_vdsat_dvd = t8 * d_vgeff_dvd - t9 * d_vc_dvd;
            let d_vdsat_dvg = t8 * d_vgeff_dvg - t9 * d_vc_dvg;
            let d_vdsat_dvb = t8 * d_vgeff_dvb - t9 * d_vc_dvb - vdsat * inv_aa * d_aa_dvb;
            dc_dvdsat_dvd = d_vdsat_dvd;
            dc_dvdsat_dvg = d_vdsat_dvg;
            dc_dvdsat_dvb = d_vdsat_dvb;

            let beta0 = p.beta0 + p.beta0_b * vbs;
            let betas = p.betas0 + p.betas_b * vbs;
            let beta2 = p.beta20 + p.beta2_b * vbs + p.beta2_g * vgs;
            let beta3 = p.beta30 + p.beta3_b * vbs + p.beta3_g * vgs;
            let beta4 = p.beta40 + p.beta4_b * vbs + p.beta4_g * vgs;
            let beta1 = betas - (beta0 + p.vdd * (beta3 - p.vdd * beta4));
            let t0 = f64::min(vds * beta2 * inv_vdsat, 30.0);
            let t1 = t0.exp();
            let t2 = t1 * t1;
            let t3 = t2 + 1.0;
            let tanh = (t2 - 1.0) / t3;
            let sqrsech = 4.0 * t2 / (t3 * t3);
            let beta = beta0 + beta1 * tanh + vds * (beta3 - beta4 * vds);
            let t4 = beta1 * sqrsech * inv_vdsat;
            let t5 = p.vdd * tanh;
            let d_beta_dvd = beta3 - 2.0 * beta4 * vds + t4 * (beta2 - t0 * d_vdsat_dvd);
            let d_beta_dvg = t4 * (p.beta2_g * vds - t0 * d_vdsat_dvg) + p.beta3_g * (vds - t5)
                - p.beta4_g * (vds * vds - p.vdd * t5);
            let d_beta1_dvb = p.arg;
            let d_beta_dvb = p.beta0_b
                + d_beta1_dvb * tanh
                + vds * (p.beta3_b - vds * p.beta4_b)
                + t4 * (p.beta2_b * vds - t0 * d_vdsat_dvb);

            if vgst > vglow {
                if vds <= vdsat {
                    // Triode region.
                    let t3 = vds * inv_vdsat;
                    let t4 = t3 - 1.0;
                    let t2 = 1.0 - p.u1_d * t4 * t4;
                    let u1 = u1s * t2;
                    let mut utot = uvert + u1 * vds;
                    utot = f64::max(utot, 0.5);
                    let inv_utot = 1.0 / utot;
                    let t5 = 2.0 * u1s * p.u1_d * inv_vdsat * t4;
                    let d_u1_dvd = t5 * (t3 * d_vdsat_dvd - 1.0);
                    let d_u1_dvg = t5 * t3 * d_vdsat_dvg;
                    let d_u1_dvb = t5 * t3 * d_vdsat_dvb + p.u1_b * t2;
                    let d_utot_dvd = d_uvert_dvd + u1 + vds * d_u1_dvd;
                    let d_utot_dvg = d_uvert_dvg + vds * d_u1_dvg;
                    let d_utot_dvb = d_uvert_dvb + vds * d_u1_dvb;
                    let tmp1 = vgeff - 0.5 * aa * vds;
                    let tmp3 = tmp1 * vds;
                    let betaeff = beta * inv_utot;
                    ids = betaeff * tmp3;
                    let t6 = ids / betaeff * inv_utot;
                    gds = t6 * (d_beta_dvd - betaeff * d_utot_dvd)
                        + betaeff * (tmp1 + (d_vgeff_dvd - 0.5 * aa) * vds);
                    gm = t6 * (d_beta_dvg - betaeff * d_utot_dvg) + betaeff * vds * d_vgeff_dvg;
                    gmb = t6 * (d_beta_dvb - betaeff * d_utot_dvb)
                        + betaeff * vds * (d_vgeff_dvb - 0.5 * vds * d_aa_dvb);
                } else {
                    // Saturation.
                    let tmp1 = vgeff * inv_aa * inv_kk;
                    let tmp3 = 0.5 * vgeff * tmp1;
                    let betaeff = beta * inv_uvert;
                    ids = betaeff * tmp3;
                    let t0 = ids / betaeff * inv_uvert;
                    let t1 = betaeff * vgeff * inv_aa * inv_kk;
                    let t2 = ids * inv_kk * d_kk_dvc;
                    if p.ai0 != 0.0 {
                        let ai = p.ai0 + p.ai_b * vbs;
                        let bi = p.bi0 + p.bi_b * vbs;
                        let t5 = f64::min(bi / (vds - vdsat), 30.0);
                        let t6 = (-t5).exp();
                        let fr = 1.0 + ai * t6;
                        let t7 = t5 / (vds - vdsat);
                        let t8 = (1.0 - fr) * t7;
                        let d_fr_dvd = t8 * (d_vdsat_dvd - 1.0);
                        let d_fr_dvg = t8 * d_vdsat_dvg;
                        let d_fr_dvb =
                            t8 * d_vdsat_dvb + t6 * (p.ai_b - ai * p.bi_b / (vds - vdsat));
                        gds = (t0 * (d_beta_dvd - betaeff * d_uvert_dvd) + t1 * d_vgeff_dvd
                            - t2 * d_vc_dvd)
                            * fr
                            + ids * d_fr_dvd;
                        gm = (t0 * (d_beta_dvg - betaeff * d_uvert_dvg) + t1 * d_vgeff_dvg
                            - t2 * d_vc_dvg)
                            * fr
                            + ids * d_fr_dvg;
                        gmb = (t0 * (d_beta_dvb - betaeff * d_uvert_dvb) + t1 * d_vgeff_dvb
                            - t2 * d_vc_dvb
                            - ids * inv_aa * d_aa_dvb)
                            * fr
                            + ids * d_fr_dvb;
                        ids *= fr;
                    } else {
                        gds = t0 * (d_beta_dvd - betaeff * d_uvert_dvd) + t1 * d_vgeff_dvd
                            - t2 * d_vc_dvd;
                        gm = t0 * (d_beta_dvg - betaeff * d_uvert_dvg) + t1 * d_vgeff_dvg
                            - t2 * d_vc_dvg;
                        gmb = t0 * (d_beta_dvb - betaeff * d_uvert_dvb) + t1 * d_vgeff_dvb
                            - t2 * d_vc_dvb
                            - ids * inv_aa * d_aa_dvb;
                    }
                }
            } else {
                // Subthreshold current (Vgst <= Vglow).
                let t0 = exp0 * exp0;
                let t1 = exp1;
                ids = beta * p.vtm * p.vtm * t0 * (1.0 - t1);
                let t2 = ids / beta;
                let t4 = n * p.vtm;
                let t3 = ids / t4;
                let (fr, d_fr_dvd, d_fr_dvg, d_fr_dvb);
                if vds > vdsat && p.ai0 != 0.0 {
                    let ai = p.ai0 + p.ai_b * vbs;
                    let bi = p.bi0 + p.bi_b * vbs;
                    let t5 = f64::min(bi / (vds - vdsat), 30.0);
                    let t6 = (-t5).exp();
                    fr = 1.0 + ai * t6;
                    let t7 = t5 / (vds - vdsat);
                    let t8 = (1.0 - fr) * t7;
                    d_fr_dvd = t8 * (d_vdsat_dvd - 1.0);
                    d_fr_dvg = t8 * d_vdsat_dvg;
                    d_fr_dvb = t8 * d_vdsat_dvb + t6 * (p.ai_b - ai * p.bi_b / (vds - vdsat));
                } else {
                    fr = 1.0;
                    d_fr_dvd = 0.0;
                    d_fr_dvg = 0.0;
                    d_fr_dvb = 0.0;
                }
                gds = (t2 * d_beta_dvd
                    + t3 * (p.vof_d * t4 - d_vth_dvd - p.n_d * vgst / n)
                    + beta * p.vtm * t0 * t1)
                    * fr
                    + ids * d_fr_dvd;
                gm = (t2 * d_beta_dvg + t3) * fr + ids * d_fr_dvg;
                gmb = (t2 * d_beta_dvb
                    + t3 * (p.vof_b * t4 - d_vth_dvb + p.n_b * vgst / (n * t1s * t1s) * d_t1s_dvb))
                    * fr
                    + ids * d_fr_dvb;
                ids *= fr;
            }
        }

        // `gds = MAX(gds, 1e-20)`.
        gds = f64::max(gds, 1.0e-20);

        // Charges: zeroed outside transient needs (XPART > 1 would
        // also zero them, as in the C).
        if charges && p.xpart <= 1.0 {
            let (vbseff, d_vbseff_dvb);
            if vbs < 0.0 {
                vbseff = vbs;
                d_vbseff_dvb = 1.0;
            } else {
                vbseff = p.phi - phisb;
                d_vbseff_dvb = -d_phisb_dvb;
            }
            let arg1 = vgs - vbseff - p.vfb;
            let arg2 = arg1 - vgst;
            let qbulk = p.one_third_cox_wl * arg2;
            let d_qbulk_dvb = p.one_third_cox_wl * (d_vth_dvb - d_vbseff_dvb);
            let d_qbulk_dvd = p.one_third_cox_wl * d_vth_dvd;
            if arg1 <= 0.0 {
                // Accumulation.
                e.qg = p.cox_wl * arg1;
                e.qb = -e.qg;
                e.cggb = p.cox_wl;
                e.cgsb = -e.cggb * (1.0 - d_vbseff_dvb);
                e.cbgb = -p.cox_wl;
                e.cbsb = -e.cgsb;
            } else if vgst <= 0.0 {
                // Subthreshold.
                let t2 = arg1 / arg2;
                let t3 = t2 * t2 * (p.cox_wl - p.two_third_cox_wl * t2);
                e.qg = p.cox_wl * arg1 * (1.0 - t2 * (1.0 - t2 / 3.0));
                e.qb = -e.qg;
                e.cggb = p.cox_wl * (1.0 - t2 * (2.0 - t2));
                let tmp = t3 * d_vth_dvb - (e.cggb + t3) * d_vbseff_dvb;
                e.cgdb = t3 * d_vth_dvd;
                e.cgsb = -(e.cggb + e.cgdb + tmp);
                e.cbgb = -e.cggb;
                e.cbdb = -e.cgdb;
                e.cbsb = -e.cgsb;
            } else {
                // Strong inversion: recompute Vdsat without Vgeff
                // smoothing below VGHIGH (the C's inner block).
                let mut vdsat_c = vdsat;
                let mut d_vdsat_dvd = dc_dvdsat_dvd;
                let mut d_vdsat_dvg = dc_dvdsat_dvg;
                let mut d_vdsat_dvb = dc_dvdsat_dvb;
                if vgst < p.vghigh {
                    let mut uvert = 1.0 + vgst * (ua + vgst * ub);
                    uvert = f64::max(uvert, 0.2);
                    let inv_uvert = 1.0 / uvert;
                    let d_uvert_dvg = ua + 2.0 * ub * vgst;
                    let d_uvert_dvd = -d_uvert_dvg * d_vth_dvd;
                    let d_uvert_dvb = -d_uvert_dvg * d_vth_dvb + vgst * (p.ua_b + vgst * p.ub_b);
                    let t8 = u1s * inv_aa * inv_uvert;
                    let vc = t8 * vgst;
                    let t9 = vc * inv_uvert;
                    let d_vc_dvg = t8 - t9 * d_uvert_dvg;
                    let d_vc_dvd = -t8 * d_vth_dvd - t9 * d_uvert_dvd;
                    let d_vc_dvb = -t8 * d_vth_dvb + p.u1_b * vgst * inv_aa * inv_uvert
                        - vc * inv_aa * d_aa_dvb
                        - t9 * d_uvert_dvb;
                    let tmp2 = (1.0 + 2.0 * vc).sqrt();
                    let kk = 0.5 * (1.0 + vc + tmp2);
                    let inv_kk = 1.0 / kk;
                    let d_kk_dvc = 0.5 + 0.5 / tmp2;
                    let t8 = inv_aa / kk.sqrt();
                    vdsat_c = vgst * t8;
                    // `*vdsat` reports the charge block's Vdsat.
                    vdsat = vdsat_c;
                    let t9 = 0.5 * vdsat_c * inv_kk * d_kk_dvc;
                    d_vdsat_dvd = -t8 * d_vth_dvd - t9 * d_vc_dvd;
                    d_vdsat_dvg = t8 - t9 * d_vc_dvg;
                    d_vdsat_dvb = -t8 * d_vth_dvb - t9 * d_vc_dvb - vdsat_c * inv_aa * d_aa_dvb;
                }
                if vds >= vdsat_c {
                    // Saturation region.
                    e.cggb = p.two_third_cox_wl;
                    e.cgdb = -e.cggb * d_vth_dvd + d_qbulk_dvd;
                    let tmp = -e.cggb * d_vth_dvb + d_qbulk_dvb;
                    e.cgsb = -(e.cggb + e.cgdb + tmp);
                    e.cbgb = 0.0;
                    e.cbdb = -d_qbulk_dvd;
                    e.cbsb = d_qbulk_dvd + d_qbulk_dvb;
                    e.cdgb = -0.4 * e.cggb;
                    let tmp = -e.cdgb * d_vth_dvb;
                    e.cddb = -e.cdgb * d_vth_dvd;
                    e.cdsb = -(e.cdgb + e.cddb + tmp);
                    e.qb = -qbulk;
                    e.qg = p.two_third_cox_wl * vgst + qbulk;
                    e.qd = e.cdgb * vgst;
                } else {
                    // Linear region.
                    let t7 = vds / vdsat_c;
                    let t8 = vgst / vdsat_c;
                    let t6 = t7 * t8;
                    let t9 = 1.0 - t7;
                    let vgdt = vgst * t9;
                    let t0 = vgst / (vgst + vgdt);
                    let t1 = vgdt / (vgst + vgdt);
                    let t5 = t0 * t1;
                    let t2 = 1.0 - t1 + t5;
                    let t3 = 1.0 - t0 + t5;
                    let d_vgdt_dvg = t9 + t6 * d_vdsat_dvg;
                    let d_vgdt_dvd = t6 * d_vdsat_dvd - t8 - t9 * d_vth_dvd;
                    let d_vgdt_dvb = t6 * d_vdsat_dvb - t9 * d_vth_dvb;
                    e.qg = p.two_third_cox_wl * (vgst + vgdt - vgdt * t0) + qbulk;
                    e.qb = -qbulk;
                    e.qd = -p.one_third_cox_wl
                        * (0.2 * vgdt + 0.8 * vgst + vgdt * t1 + 0.2 * t5 * (vgdt - vgst));
                    e.cggb = p.two_third_cox_wl * (t2 + t3 * d_vgdt_dvg);
                    let tmp = d_qbulk_dvb + p.two_third_cox_wl * (t3 * d_vgdt_dvb - t2 * d_vth_dvb);
                    e.cgdb = p.two_third_cox_wl * (t3 * d_vgdt_dvd - t2 * d_vth_dvd) + d_qbulk_dvd;
                    e.cgsb = -(e.cggb + e.cgdb + tmp);
                    let t2b = 0.8 - 0.4 * t1 * (2.0 * t1 + t0 + t0 * (t1 - t0));
                    let t3b = 0.2 + t1 + t0 * (1.0 - 0.4 * t0 * (t1 + 3.0 * t0));
                    e.cdgb = -p.one_third_cox_wl * (t2b + t3b * d_vgdt_dvg);
                    let tmp = p.one_third_cox_wl * (t2b * d_vth_dvb - t3b * d_vgdt_dvb);
                    e.cddb = p.one_third_cox_wl * (t2b * d_vth_dvd - t3b * d_vgdt_dvd);
                    e.cdsb = -(e.cdgb + tmp + e.cddb);
                    e.cbgb = 0.0;
                    e.cbdb = -d_qbulk_dvd;
                    e.cbsb = d_qbulk_dvd + d_qbulk_dvb;
                }
            }
        }

        e.gm = gm;
        e.gds = gds;
        e.gmbs = gmb;
        e.cdrain = f64::max(ids, 1.0e-50);
        e.vdsat = vdsat;
        e
    }
}

impl crate::devices::Device for Bsim2I {
    fn par_load(&self) -> bool {
        true
    }

    /// `B2getic` (`b2getic.c`): VBS, VDS, VGS not given
    /// on the card from the external nodes.
    fn set_ic(&mut self, rhs: &[f64]) {
        use crate::devices::getic;
        let g = self.ic_given;
        getic(g, 2, &mut self.ic_vbs, rhs, self.bulk, self.source);
        getic(g, 0, &mut self.ic_vds, rhs, self.drain, self.source);
        getic(g, 1, &mut self.ic_vgs, rhs, self.gate, self.source);
    }

    fn set_ic_given(&mut self, given: crate::devices::IcGiven) {
        self.ic_given = given;
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (d, g, s, b) = (self.drain, self.gate, self.source, self.bulk);
        let (dp, sp) = (self.d_p, self.s_p);
        self.s = [
            mb.slot(d, d),
            mb.slot(g, g),
            mb.slot(s, s),
            mb.slot(b, b),
            mb.slot(dp, dp),
            mb.slot(sp, sp),
            mb.slot(d, dp),
            mb.slot(g, b),
            mb.slot(g, dp),
            mb.slot(g, sp),
            mb.slot(s, sp),
            mb.slot(b, dp),
            mb.slot(b, sp),
            mb.slot(dp, sp),
            mb.slot(dp, d),
            mb.slot(b, g),
            mb.slot(dp, g),
            mb.slot(sp, g),
            mb.slot(sp, s),
            mb.slot(dp, b),
            mb.slot(sp, b),
            mb.slot(sp, dp),
        ];
    }

    /// `B2convTest` (`b2cvtest.c`).
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.t.type_;
        let st = self.st;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let vbs = typ * (ctx.rhs[self.bulk] - ctx.rhs[self.s_p]);
        let vgs = typ * (ctx.rhs[self.gate] - ctx.rhs[self.s_p]);
        let vds = typ * (ctx.rhs[self.d_p] - ctx.rhs[self.s_p]);
        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgdo = s0(VGS) - s0(VDS);
        let delvbs = vbs - s0(VBS);
        let delvbd = vbd - s0(VBD);
        let delvgs = vgs - s0(VGS);
        let delvds = vds - s0(VDS);
        let delvgd = vgd - vgdo;
        let cdhat = if self.mode >= 0.0 {
            s0(CD) - s0(GBD) * delvbd + s0(GMBS) * delvbs + s0(GM) * delvgs + s0(GDS) * delvds
        } else {
            s0(CD) - (s0(GBD) - s0(GMBS)) * delvbd - s0(GM) * delvgd + s0(GDS) * delvds
        };
        let cbhat = s0(CBS) + s0(CBD) + s0(GBD) * delvbd + s0(GBS) * delvbs;
        let cd = s0(CD);
        let cb = s0(CBS) + s0(CBD);
        let tol = ctx.reltol * f64::max(cdhat.abs(), cd.abs()) + ctx.abstol;
        if (cdhat - cd).abs() >= tol {
            return false;
        }
        let tol = ctx.reltol * f64::max(cbhat.abs(), cb.abs()) + ctx.abstol;
        (cbhat - cb).abs() <= tol
    }

    #[allow(clippy::too_many_lines)]
    fn load(&mut self, ctx: &mut LoadCtx) {
        let typ = self.t.type_;
        let st = self.st;

        let drain_sat_cur = f64::max(self.geom.ad * self.t.js, 1.0e-15);
        let source_sat_cur = f64::max(self.geom.as_ * self.t.js, 1.0e-15);
        let von_inst = typ * self.von;
        let vt0_inst = typ * self.t.vt0;

        let mut check = true;
        let (mut vbs, mut vgs, mut vds);
        if ctx.mode == InitMode::Smsig {
            vbs = ctx.states.get(0, st + VBS);
            vgs = ctx.states.get(0, st + VGS);
            vds = ctx.states.get(0, st + VDS);
        } else if ctx.mode == InitMode::Tran {
            vbs = ctx.states.get(1, st + VBS);
            vgs = ctx.states.get(1, st + VGS);
            vds = ctx.states.get(1, st + VDS);
        } else if ctx.mode == InitMode::Jct && !self.off {
            vds = typ * self.ic_vds;
            vgs = typ * self.ic_vgs;
            vbs = typ * self.ic_vbs;
            if vds == 0.0 && vgs == 0.0 && vbs == 0.0 && !(ctx.kind == AnKind::TranOp && ctx.uic) {
                vbs = -1.0;
                vgs = vt0_inst;
                vds = 0.0;
            }
        } else if matches!(ctx.mode, InitMode::Jct | InitMode::Fix) && self.off {
            vbs = 0.0;
            vgs = 0.0;
            vds = 0.0;
        } else {
            if ctx.mode == InitMode::Pred {
                let xfact = ctx.pred_xfact;
                let mut pred = |o: usize| {
                    let s1 = ctx.states.get(1, st + o);
                    ctx.states.set(0, st + o, s1);
                    (1.0 + xfact) * s1 - xfact * ctx.states.get(2, st + o)
                };
                vbs = pred(VBS);
                vgs = pred(VGS);
                vds = pred(VDS);
                let vbd0 = ctx.states.get(0, st + VBS) - ctx.states.get(0, st + VDS);
                ctx.states.set(0, st + VBD, vbd0);
                for o in [
                    CD, CBS, CBD, GM, GDS, GMBS, GBD, GBS, CGGB, CBGB, CBSB, CGDB, CGSB, CBDB,
                    CDGB, CDDB, CDSB,
                ] {
                    let v = ctx.states.get(1, st + o);
                    ctx.states.set(0, st + o, v);
                }
            } else {
                vbs = typ * (ctx.sol[self.bulk] - ctx.sol[self.s_p]);
                vgs = typ * (ctx.sol[self.gate] - ctx.sol[self.s_p]);
                vds = typ * (ctx.sol[self.d_p] - ctx.sol[self.s_p]);
            }
            let mut vbd = vbs - vds;
            let mut vgd = vgs - vds;
            let vgdo = ctx.states.get(0, st + VGS) - ctx.states.get(0, st + VDS);

            if ctx.states.get(0, st + VDS) >= 0.0 {
                vgs = fetlim(vgs, ctx.states.get(0, st + VGS), von_inst);
                vds = vgs - vgd;
                vds = limvds(vds, ctx.states.get(0, st + VDS));
            } else {
                vgd = fetlim(vgd, vgdo, von_inst);
                vds = vgs - vgd;
                vds = -limvds(-vds, -ctx.states.get(0, st + VDS));
                vgs = vgd + vds;
            }
            if vds >= 0.0 {
                let vcrit = VT0 * f64::ln(VT0 / (ROOT2 * source_sat_cur));
                vbs = pnjlim(vbs, ctx.states.get(0, st + VBS), VT0, vcrit, &mut check);
            } else {
                let vcrit = VT0 * f64::ln(VT0 / (ROOT2 * drain_sat_cur));
                vbd = pnjlim(vbd, ctx.states.get(0, st + VBD), VT0, vcrit, &mut check);
                vbs = vbd + vds;
            }
        }

        let vbd = vbs - vds;
        let vgd = vgs - vds;
        let vgb = vgs - vbs;

        // Bulk junctions on `CONSTvt0` (not the device temperature).
        let (gbs, cbs);
        if vbs <= 0.0 {
            gbs = source_sat_cur / VT0 + ctx.gmin;
            cbs = gbs * vbs;
        } else {
            let evbs = f64::exp(vbs / VT0);
            gbs = source_sat_cur * evbs / VT0 + ctx.gmin;
            cbs = source_sat_cur * (evbs - 1.0) + ctx.gmin * vbs;
        }
        let (gbd, cbd);
        if vbd <= 0.0 {
            gbd = drain_sat_cur / VT0 + ctx.gmin;
            cbd = gbd * vbd;
        } else {
            let evbd = f64::exp(vbd / VT0);
            gbd = drain_sat_cur * evbd / VT0 + ctx.gmin;
            cbd = drain_sat_cur * (evbd - 1.0) + ctx.gmin * vbd;
        }

        self.mode = if vds >= 0.0 { 1.0 } else { -1.0 };
        let mode = self.mode;

        let charges = ctx.kind == AnKind::Tran
            || ctx.mode == InitMode::Smsig
            || (ctx.kind == AnKind::TranOp && ctx.uic);

        // `B2evaluate`, with the terminal swap for the inverse mode:
        // the drain-referred charge/capacitance rows come back on
        // the source side when `vds < 0`.
        let ev = if vds >= 0.0 {
            self.evaluate(vds, vbs, vgs, charges)
        } else {
            self.evaluate(-vds, vbd, vgd, charges)
        };
        let (cggb, cgdb, cgsb, cbgb, cbdb, cbsb);
        let (cdgb, cddb, cdsb, csgb, cssb, csdb);
        let (qgate, qbulk, qdrn, qsrc);
        if vds >= 0.0 {
            cggb = ev.cggb;
            cgdb = ev.cgdb;
            cgsb = ev.cgsb;
            cbgb = ev.cbgb;
            cbdb = ev.cbdb;
            cbsb = ev.cbsb;
            cdgb = ev.cdgb;
            cddb = ev.cddb;
            cdsb = ev.cdsb;
            csgb = 0.0;
            cssb = 0.0;
            csdb = 0.0;
            qgate = ev.qg;
            qbulk = ev.qb;
            qdrn = ev.qd;
            qsrc = 0.0;
        } else {
            cggb = ev.cggb;
            cgsb = ev.cgdb;
            cgdb = ev.cgsb;
            cbgb = ev.cbgb;
            cbsb = ev.cbdb;
            cbdb = ev.cbsb;
            csgb = ev.cdgb;
            cssb = ev.cddb;
            csdb = ev.cdsb;
            cdgb = 0.0;
            cddb = 0.0;
            cdsb = 0.0;
            qgate = ev.qg;
            qbulk = ev.qb;
            qsrc = ev.qd;
            qdrn = 0.0;
        }
        let (gm, gds, gmbs, cdrain) = (ev.gm, ev.gds, ev.gmbs, ev.cdrain);
        self.von = typ * ev.von;
        self.vdsat = typ * ev.vdsat;

        let cd = mode * cdrain - cbd;

        // Depletion capacitances.
        let (mut capbs, mut capbd) = (0.0, 0.0);
        if charges {
            let t = &self.t;
            let czbd = t.cj * self.geom.ad;
            let czbs = t.cj * self.geom.as_;
            let czbdsw = t.cjsw * self.geom.pd;
            let czbssw = t.cjsw * self.geom.ps;
            let (phi_b, phi_bsw, mj, mjsw) = (t.pb, t.pbsw, t.mj, t.mjsw);
            if vbs < 0.0 {
                let arg = 1.0 - vbs / phi_b;
                let argsw = 1.0 - vbs / phi_bsw;
                let sarg = f64::exp(-mj * arg.ln());
                let sargsw = f64::exp(-mjsw * argsw.ln());
                ctx.states.set(
                    0,
                    st + QBS,
                    phi_b * czbs * (1.0 - arg * sarg) / (1.0 - mj)
                        + phi_bsw * czbssw * (1.0 - argsw * sargsw) / (1.0 - mjsw),
                );
                capbs = czbs * sarg + czbssw * sargsw;
            } else {
                ctx.states.set(
                    0,
                    st + QBS,
                    vbs * (czbs + czbssw)
                        + vbs * vbs * (czbs * mj * 0.5 / phi_b + czbssw * mjsw * 0.5 / phi_bsw),
                );
                capbs = czbs + czbssw + vbs * (czbs * mj / phi_b + czbssw * mjsw / phi_bsw);
            }
            if vbd < 0.0 {
                let arg = 1.0 - vbd / phi_b;
                let argsw = 1.0 - vbd / phi_bsw;
                let sarg = f64::exp(-mj * arg.ln());
                let sargsw = f64::exp(-mjsw * argsw.ln());
                ctx.states.set(
                    0,
                    st + QBD,
                    phi_b * czbd * (1.0 - arg * sarg) / (1.0 - mj)
                        + phi_bsw * czbdsw * (1.0 - argsw * sargsw) / (1.0 - mjsw),
                );
                capbd = czbd * sarg + czbdsw * sargsw;
            } else {
                ctx.states.set(
                    0,
                    st + QBD,
                    vbd * (czbd + czbdsw)
                        + vbd * vbd * (czbd * mj * 0.5 / phi_b + czbdsw * mjsw * 0.5 / phi_bsw),
                );
                capbd = czbd + czbdsw + vbd * (czbd * mj / phi_b + czbdsw * mjsw / phi_bsw);
            }
        }

        // Convergence.
        if (!self.off || ctx.mode != InitMode::Fix) && check {
            ctx.noncon = true;
        }

        ctx.states.set(0, st + VBS, vbs);
        ctx.states.set(0, st + VBD, vbd);
        ctx.states.set(0, st + VGS, vgs);
        ctx.states.set(0, st + VDS, vds);
        ctx.states.set(0, st + CD, cd);
        ctx.states.set(0, st + CBS, cbs);
        ctx.states.set(0, st + CBD, cbd);
        ctx.states.set(0, st + GM, gm);
        ctx.states.set(0, st + GDS, gds);
        ctx.states.set(0, st + GMBS, gmbs);
        ctx.states.set(0, st + GBD, gbd);
        ctx.states.set(0, st + GBS, gbs);
        ctx.states.set(0, st + CGGB, cggb);
        ctx.states.set(0, st + CGDB, cgdb);
        ctx.states.set(0, st + CGSB, cgsb);
        ctx.states.set(0, st + CBGB, cbgb);
        ctx.states.set(0, st + CBDB, cbdb);
        ctx.states.set(0, st + CBSB, cbsb);
        ctx.states.set(0, st + CDGB, cdgb);
        ctx.states.set(0, st + CDDB, cddb);
        ctx.states.set(0, st + CDSB, cdsb);
        ctx.states.set(0, st + CAPBS, capbs);
        ctx.states.set(0, st + CAPBD, capbd);

        // Gate charges and companion conductances (`B2mosCap`).
        let (mut ceqqg, mut ceqqb, mut ceqqd) = (0.0, 0.0, 0.0);
        let mut gc = [0.0f64; 12]; // gcggb gcgdb gcgsb gcbgb gcbdb gcbsb gcdgb gcddb gcdsb gcsgb gcsdb gcssb
        let t = &self.t;
        if charges {
            let ag0 = ctx.ag[0];
            let (args, c_b, c_d): ([f64; 8], [f64; 3], [f64; 3]);
            if mode > 0.0 {
                args = [
                    t.ld_gd_overlap,
                    t.ld_gs_overlap,
                    t.ld_gb_overlap,
                    capbd,
                    capbs,
                    cggb,
                    cgdb,
                    cgsb,
                ];
                c_b = [cbgb, cbdb, cbsb];
                c_d = [cdgb, cddb, cdsb];
            } else {
                args = [
                    t.ld_gs_overlap,
                    t.ld_gd_overlap,
                    t.ld_gb_overlap,
                    capbs,
                    capbd,
                    cggb,
                    cgsb,
                    cgdb,
                ];
                c_b = [cbgb, cbsb, cbdb];
                c_d = [csgb, cssb, csdb];
            }
            // `B2mosCap`: equivalent conductances…
            let gcdgb_l = (c_d[0] - args[0]) * ag0;
            let gcddb_l = (c_d[1] + args[3] + args[0]) * ag0;
            let gcdsb_l = c_d[2] * ag0;
            let gcsgb_l = -(args[5] + c_b[0] + c_d[0] + args[1]) * ag0;
            let gcsdb_l = -(args[6] + c_b[1] + c_d[1]) * ag0;
            let gcssb_l = (args[4] + args[1] - (args[7] + c_b[2] + c_d[2])) * ag0;
            let gcggb_l = (args[5] + args[0] + args[1] + args[2]) * ag0;
            let gcgdb_l = (args[6] - args[0]) * ag0;
            let gcgsb_l = (args[7] - args[1]) * ag0;
            let gcbgb_l = (c_b[0] - args[2]) * ag0;
            let gcbdb_l = (c_b[1] - args[3]) * ag0;
            let gcbsb_l = (c_b[2] - args[4]) * ag0;
            // …and total terminal charges.
            let (vg1, vg2) = if mode > 0.0 { (vgd, vgs) } else { (vgs, vgd) };
            let qgd = args[0] * vg1;
            let qgs = args[1] * vg2;
            let qgb = args[2] * vgb;
            let qgate = qgate + qgd + qgs + qgb;
            let qbulk = qbulk - qgb;
            let (mut qdrn, mut qsrc) = (qdrn, qsrc);
            if mode > 0.0 {
                qdrn -= qgd;
                qsrc = -(qgate + qbulk + qdrn);
            } else {
                qsrc -= qgs;
                qdrn = -(qgate + qbulk + qsrc);
            }
            let _ = qsrc;

            if mode > 0.0 {
                gc = [
                    gcggb_l, gcgdb_l, gcgsb_l, gcbgb_l, gcbdb_l, gcbsb_l, gcdgb_l, gcddb_l,
                    gcdsb_l, gcsgb_l, gcsdb_l, gcssb_l,
                ];
            } else {
                // The reverse call swaps d and s in the outputs.
                gc = [
                    gcggb_l, gcgsb_l, gcgdb_l, gcbgb_l, gcbsb_l, gcbdb_l, gcsgb_l, gcssb_l,
                    gcsdb_l, gcdgb_l, gcdsb_l, gcddb_l,
                ];
                // and the gc arrays above are already in (g,b,d,s)
                // row order for the swapped call, matching the C's
                // pointer permutation.
            }

            ctx.states.set(0, st + QG, qgate);
            ctx.states
                .set(0, st + QD, qdrn - ctx.states.get(0, st + QBD));
            ctx.states.set(
                0,
                st + QB,
                qbulk + ctx.states.get(0, st + QBD) + ctx.states.get(0, st + QBS),
            );

            if ctx.mode == InitMode::Smsig {
                return;
            }
            if ctx.mode == InitMode::Tran {
                for o in [QB, QG, QD] {
                    let v = ctx.states.get(0, st + o);
                    ctx.states.set(1, st + o, v);
                }
            }
            ctx.integrate(0.0, st + QB);
            ctx.integrate(0.0, st + QG);
            ctx.integrate(0.0, st + QD);

            // Equivalent charge currents.
            let cqgate = ctx.states.get(0, st + IQG);
            let cqbulk = ctx.states.get(0, st + IQB);
            let cqdrn = ctx.states.get(0, st + IQD);
            ceqqg = cqgate - gc[0] * vgb + gc[1] * vbd + gc[2] * vbs;
            ceqqb = cqbulk - gc[3] * vgb + gc[4] * vbd + gc[5] * vbs;
            ceqqd = cqdrn - gc[6] * vgb + gc[7] * vbd + gc[8] * vbs;
            if ctx.mode == InitMode::Tran {
                for o in [IQB, IQG, IQD] {
                    let v = ctx.states.get(0, st + o);
                    ctx.states.set(1, st + o, v);
                }
            }
        }

        // Load the current vector.
        let ceqbs = typ * (cbs - (gbs - ctx.gmin) * vbs);
        let ceqbd = typ * (cbd - (gbd - ctx.gmin) * vbd);
        let ceqqg = typ * ceqqg;
        let ceqqb = typ * ceqqb;
        let ceqqd = typ * ceqqd;
        let (xnrm, xrev, cdreq);
        if mode >= 0.0 {
            xnrm = 1.0;
            xrev = 0.0;
            cdreq = typ * (cdrain - gds * vds - gm * vgs - gmbs * vbs);
        } else {
            xnrm = 0.0;
            xrev = 1.0;
            cdreq = -typ * (cdrain + gds * vds - gm * vgd - gmbs * vbd);
        }
        ctx.add_rhs(self.gate, -ceqqg);
        ctx.add_rhs(self.bulk, -(ceqbs + ceqbd + ceqqb));
        ctx.add_rhs(self.d_p, ceqbd - cdreq - ceqqd);
        ctx.add_rhs(self.s_p, cdreq + ceqbs + ceqqg + ceqqb + ceqqd);

        // Load the Y matrix.
        let gdpr = t.drain_conductance;
        let gspr = t.source_conductance;
        ctx.add(self.s[D_D], gdpr);
        ctx.add(self.s[G_G], gc[0]);
        ctx.add(self.s[S_S], gspr);
        ctx.add(self.s[B_B], gbd + gbs - gc[3] - gc[4] - gc[5]);
        ctx.add(self.s[DP_DP], gdpr + gds + gbd + xrev * (gm + gmbs) + gc[7]);
        ctx.add(
            self.s[SP_SP],
            gspr + gds + gbs + xnrm * (gm + gmbs) + gc[11],
        );
        ctx.add(self.s[D_DP], -gdpr);
        ctx.add(self.s[G_B], -gc[0] - gc[1] - gc[2]);
        ctx.add(self.s[G_DP], gc[1]);
        ctx.add(self.s[G_SP], gc[2]);
        ctx.add(self.s[S_SP], -gspr);
        ctx.add(self.s[B_G], gc[3]);
        ctx.add(self.s[B_DP], -gbd + gc[4]);
        ctx.add(self.s[B_SP], -gbs + gc[5]);
        ctx.add(self.s[DP_D], -gdpr);
        ctx.add(self.s[DP_G], (xnrm - xrev) * gm + gc[6]);
        ctx.add(
            self.s[DP_B],
            -gbd + (xnrm - xrev) * gmbs - gc[6] - gc[7] - gc[8],
        );
        ctx.add(self.s[DP_SP], -gds - xnrm * (gm + gmbs) + gc[8]);
        ctx.add(self.s[SP_G], -(xnrm - xrev) * gm + gc[9]);
        ctx.add(self.s[SP_S], -gspr);
        ctx.add(
            self.s[SP_B],
            -gbs - (xnrm - xrev) * gmbs - gc[9] - gc[10] - gc[11],
        );
        ctx.add(self.s[SP_DP], -gds - xrev * (gm + gmbs) + gc[10]);
    }

    /// `B2noise` (ngspice-47 `b2noi.c`): see [`crate::bsim1::b12_noise`].
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(crate::bsim1::B12_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let t = &self.t;
        let n = crate::bsim1::B12Noise {
            drain: self.drain,
            source: self.source,
            d_p: self.d_p,
            s_p: self.s_p,
            gdpr: t.drain_conductance,
            gspr: t.source_conductance,
            gm: ctx.states.get(0, self.st + GM),
            cd: ctx.states.get(0, self.st + CD),
            kf: t.kf,
            af: t.af,
            flicker_area: (self.geom.w - t.delta_w * 1e-6) * (self.geom.l - t.delta_l * 1e-6),
            cox: t.cox,
        };
        crate::bsim1::b12_noise(ctx, &n)
    }

    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QB, t, time_step);
        crate::integ::terr(self.st + QG, t, time_step);
        crate::integ::terr(self.st + QD, t, time_step);
    }

    /// `B2acLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let st = self.st;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let (xnrm, xrev) = if self.mode < 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        let gdpr = self.t.drain_conductance;
        let gspr = self.t.source_conductance;
        let gm = s0(GM);
        let gds = s0(GDS);
        let gmbs = s0(GMBS);
        let gbd = s0(GBD);
        let gbs = s0(GBS);
        let capbd = s0(CAPBD);
        let capbs = s0(CAPBS);
        let cggb = s0(CGGB);
        let cgsb = s0(CGSB);
        let cgdb = s0(CGDB);
        let cbgb = s0(CBGB);
        let cbsb = s0(CBSB);
        let cbdb = s0(CBDB);
        let cdgb = s0(CDGB);
        let cdsb = s0(CDSB);
        let cddb = s0(CDDB);
        let o = ctx.omega;
        let xcdgb = (cdgb - self.t.gd_overlap) * o;
        let xcddb = (cddb + capbd + self.t.gd_overlap) * o;
        let xcdsb = cdsb * o;
        let xcsgb = -(cggb + cbgb + cdgb + self.t.gs_overlap) * o;
        let xcsdb = -(cgdb + cbdb + cddb) * o;
        let xcssb = (capbs + self.t.gs_overlap - (cgsb + cbsb + cdsb)) * o;
        let xcggb = (cggb + self.t.gd_overlap + self.t.gs_overlap + self.t.gb_overlap) * o;
        let xcgdb = (cgdb - self.t.gd_overlap) * o;
        let xcgsb = (cgsb - self.t.gs_overlap) * o;
        let xcbgb = (cbgb - self.t.gb_overlap) * o;
        let xcbdb = (cbdb - capbd) * o;
        let xcbsb = (cbsb - capbs) * o;

        ctx.add_b(self.s[G_G], xcggb);
        ctx.add_b(self.s[B_B], -xcbgb - xcbdb - xcbsb);
        ctx.add_b(self.s[DP_DP], xcddb);
        ctx.add_b(self.s[SP_SP], xcssb);
        ctx.add_b(self.s[G_B], -xcggb - xcgdb - xcgsb);
        ctx.add_b(self.s[G_DP], xcgdb);
        ctx.add_b(self.s[G_SP], xcgsb);
        ctx.add_b(self.s[B_G], xcbgb);
        ctx.add_b(self.s[B_DP], xcbdb);
        ctx.add_b(self.s[B_SP], xcbsb);
        ctx.add_b(self.s[DP_G], xcdgb);
        ctx.add_b(self.s[DP_B], -xcdgb - xcddb - xcdsb);
        ctx.add_b(self.s[DP_SP], xcdsb);
        ctx.add_b(self.s[SP_G], xcsgb);
        ctx.add_b(self.s[SP_B], -xcsgb - xcsdb - xcssb);
        ctx.add_b(self.s[SP_DP], xcsdb);
        ctx.add_g(self.s[D_D], gdpr);
        ctx.add_g(self.s[S_S], gspr);
        ctx.add_g(self.s[B_B], gbd + gbs);
        ctx.add_g(self.s[DP_DP], gdpr + gds + gbd + xrev * (gm + gmbs));
        ctx.add_g(self.s[SP_SP], gspr + gds + gbs + xnrm * (gm + gmbs));
        ctx.add_g(self.s[D_DP], -gdpr);
        ctx.add_g(self.s[S_SP], -gspr);
        ctx.add_g(self.s[B_DP], -gbd);
        ctx.add_g(self.s[B_SP], -gbs);
        ctx.add_g(self.s[DP_D], -gdpr);
        ctx.add_g(self.s[DP_G], (xnrm - xrev) * gm);
        ctx.add_g(self.s[DP_B], -gbd + (xnrm - xrev) * gmbs);
        ctx.add_g(self.s[DP_SP], -gds - xnrm * (gm + gmbs));
        ctx.add_g(self.s[SP_G], -(xnrm - xrev) * gm);
        ctx.add_g(self.s[SP_S], -gspr);
        ctx.add_g(self.s[SP_B], -gbs - (xnrm - xrev) * gmbs);
        ctx.add_g(self.s[SP_DP], -gds - xrev * (gm + gmbs));
    }
}
