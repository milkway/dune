//! BSIM1 — the Berkeley Short-Channel IGFET Model, level 4 in the
//! 3f5 (`dev/bsim1/b1ld.c`, `b1eval.c`, `b1temp.c`, `b1set.c`,
//! `b1moscap.c`, `b1acld.c`, `b1trunc.c`, `b1cvtest.c`; authors
//! Hong J. Park and Thomas L. Quarles). Every electrical parameter
//! comes in a (value, L-dependence, W-dependence) triple resolved
//! per instance as `p0 + pL/Leff + pW/Weff` with Leff/Weff in µm;
//! TOX and DL/DW are in µm, mobilities in cm² — the unit dance of
//! the C is kept to the letter. ngspice-47 carries this code
//! unchanged, so the 3f5 is also the oracle's reference here.

use crate::consts::{ROOT2, VT0};
use crate::diode::pnjlim;
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};
use crate::mos1::{MosGeom, fetlim, limvds};

/// `.model … nmos(level=4 …)` parameters, 3f5 names (`b1.c` keyword
/// table). Everything defaults to 0 (`b1set.c`); the junction
/// potentials are clamped to ≥ 0.1 at temp time.
#[derive(Clone, Debug, Default)]
pub struct Bsim1Model {
    pub nmos: bool,
    // (value, L, W) triples.
    pub vfb0: f64,
    pub vfbl: f64,
    pub vfbw: f64,
    pub phi0: f64,
    pub phil: f64,
    pub phiw: f64,
    pub k10: f64,
    pub k1l: f64,
    pub k1w: f64,
    pub k20: f64,
    pub k2l: f64,
    pub k2w: f64,
    pub eta0: f64,
    pub etal: f64,
    pub etaw: f64,
    pub etab0: f64,
    pub etabl: f64,
    pub etabw: f64,
    pub etad0: f64,
    pub etadl: f64,
    pub etadw: f64,
    /// `MUZ` has no geometry dependence of its own.
    pub mobzero: f64,
    pub mobzerob0: f64,
    pub mobzerobl: f64,
    pub mobzerobw: f64,
    pub mobvdd0: f64,
    pub mobvddl: f64,
    pub mobvddw: f64,
    pub mobvddb0: f64,
    pub mobvddbl: f64,
    pub mobvddbw: f64,
    pub mobvddd0: f64,
    pub mobvdddl: f64,
    pub mobvdddw: f64,
    pub ugs0: f64,
    pub ugsl: f64,
    pub ugsw: f64,
    pub ugsb0: f64,
    pub ugsbl: f64,
    pub ugsbw: f64,
    pub uds0: f64,
    pub udsl: f64,
    pub udsw: f64,
    pub udsb0: f64,
    pub udsbl: f64,
    pub udsbw: f64,
    pub udsd0: f64,
    pub udsdl: f64,
    pub udsdw: f64,
    pub n00: f64,
    pub n0l: f64,
    pub n0w: f64,
    pub nb0: f64,
    pub nbl: f64,
    pub nbw: f64,
    pub nd0: f64,
    pub ndl: f64,
    pub ndw: f64,
    /// `DL`/`DW`: channel reduction, **µm**.
    pub deltal: f64,
    pub deltaw: f64,
    /// `TOX` in **µm**.
    pub tox: f64,
    /// `TEMP` (accepted and unused: BSIM1 runs on `CONSTvt0`).
    pub temp: f64,
    /// `VDD`: supply at which `MUS` was measured.
    pub vdd: f64,
    /// Overlap capacitances per meter.
    pub cgso: f64,
    pub cgdo: f64,
    pub cgbo: f64,
    /// `XPART`: ≥ 1 selects the 0/100 charge partition, else 40/60.
    pub xpart: f64,
    pub rsh: f64,
    pub js: f64,
    pub pb: f64,
    pub mj: f64,
    pub pbsw: f64,
    pub mjsw: f64,
    pub cj: f64,
    pub cjsw: f64,
    /// `WDF`/`DELL` (accepted and unused by the load, like the C).
    pub wdf: f64,
    pub dell: f64,
    /// `KF`/`AF` (noise only; ngspice-47's `b1noi.c` — the 3f5's
    /// BSIM1 has no noise and no such parameters). Defaults 0 and 1
    /// (`b1set.c` of ngspice) — `new` sets `AF`; the derived
    /// `Default` leaves it 0.
    pub kf: f64,
    pub af: f64,
}

impl Bsim1Model {
    pub fn new(nmos: bool) -> Self {
        Bsim1Model {
            nmos,
            af: 1.0,
            ..Default::default()
        }
    }

    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.rsh != 0.0 && squares != 0.0
    }
}

/// Name suffixes of the BSIM1/BSIM2 noise generators (`B1nNames`,
/// `B2nNames` of ngspice-47).
pub(crate) const B12_NOISE_SUFFIXES: &[&str] = &["_rd", "_rs", "_id", "_1overf", ""];

/// What `B1noise`/`B2noise` read.
pub(crate) struct B12Noise {
    pub drain: Var,
    pub source: Var,
    pub d_p: Var,
    pub s_p: Var,
    pub gdpr: f64,
    pub gspr: f64,
    /// `state0` gm and drain current.
    pub gm: f64,
    pub cd: f64,
    pub kf: f64,
    pub af: f64,
    /// `(W − DW·1e-6)·(L − DL·1e-6)`, m².
    pub flicker_area: f64,
    /// `B*Cox`, F/cm².
    pub cox: f64,
}

#[cfg(test)]
thread_local! {
    /// Test hook reproducing ngspice-47's reading of `B1gm`/`B1cd`
    /// (see [`b12_noise`]): `Some(GENstate)` makes the channel and
    /// flicker terms use `GENstate + 7` and `GENstate + 4`.
    pub(crate) static NGSPICE_STATE_OFFSET: std::cell::Cell<Option<f64>> =
        const { std::cell::Cell::new(None) };
}

/// `B1noise` / `B2noise` (ngspice-47 `b1noi.c`/`b2noi.c`, the same
/// file with the prefix renamed; the 3f5 has neither): thermal noise
/// of the drain/source series resistances and of the channel
/// `4kT·(2/3)|gm|`, at `CKTtemp` (plain `NevalSrc`); flicker
/// `KF·|Id|^AF/(f·(W−DW)·(L−DL)·Cox²)` — the old 3f5-MOS form, with
/// `Cox` in F/cm².
///
/// **Deliberate divergence from the oracle.** `bsim1def.h` defines
/// `B1gm` and `B1cd` as *state offsets* (`#define B1gm B1states+ 7`),
/// so the C's `fabs(inst->B1gm * inst->B1m)` evaluates to
/// `GENstate + 7·m` and `(double)inst->B1cd` to `GENstate + 4` — the
/// instance's index in the state vector, not its gm and current
/// (measured: one BSIM1 in a deck, `GENstate = 0`, channel noise of a
/// 7 S conductance). Dune reads the values those offsets point at,
/// `*(CKTstate0 + B1gm)`, as the author evidently meant (and as
/// `jfetnoi.c` does); everything else is the C's.
pub(crate) fn b12_noise(ctx: &crate::matrix::NoiseCtx, n: &B12Noise) -> Vec<f64> {
    const N_MINLOG: f64 = 1e-38;
    let (gm, cd) = (n.gm, n.cd);
    #[cfg(test)]
    let (gm, cd) = match NGSPICE_STATE_OFFSET.get() {
        Some(state) => (state + 7.0, state + 4.0),
        None => (gm, cd),
    };
    let rd = ctx.thermal(n.d_p, n.drain, n.gdpr);
    let rs = ctx.thermal(n.s_p, n.source, n.gspr);
    let id = ctx.thermal(n.d_p, n.s_p, 2.0 / 3.0 * gm.abs());
    let flicker = ctx.flicker(
        n.d_p,
        n.s_p,
        n.kf * f64::exp(n.af * f64::ln(f64::max(cd.abs(), N_MINLOG)))
            / (n.flicker_area * n.cox * n.cox),
        1.0,
    );
    vec![rd, rs, id, flicker, rd + rs + id + flicker]
}

/// Per-instance size-resolved parameters (`B1temp`).
#[derive(Debug)]
pub(crate) struct Bsim1Size {
    pub type_: f64,
    pub vfb: f64,
    pub phi: f64,
    pub k1: f64,
    pub k2: f64,
    pub eta: f64,
    pub eta_b: f64,
    pub eta_d: f64,
    pub beta_zero: f64,
    pub beta_zero_b: f64,
    pub ugs: f64,
    pub ugs_b: f64,
    pub uds: f64,
    pub uds_b: f64,
    pub uds_d: f64,
    pub beta_vdd: f64,
    pub beta_vdd_b: f64,
    pub beta_vdd_d: f64,
    pub n0: f64,
    pub nb: f64,
    pub nd: f64,
    pub vt0: f64,
    /// `b1temp.c` overlaps (effective W, drawn L) — the AC load's.
    pub gd_overlap: f64,
    pub gs_overlap: f64,
    pub gb_overlap: f64,
    /// `b1ld.c` overlaps (drawn W, effective L) — the transient
    /// load recomputes them per iteration from the model card.
    pub ld_gd_overlap: f64,
    pub ld_gs_overlap: f64,
    pub ld_gb_overlap: f64,
    pub drain_conductance: f64,
    pub source_conductance: f64,
    /// `B1Cox` in F/cm².
    pub cox: f64,
    // Model copies the load needs.
    pub vdd: f64,
    pub delta_l: f64,
    /// `DW` in µm.
    pub delta_w: f64,
    pub js: f64,
    pub pb: f64,
    pub pbsw: f64,
    pub mj: f64,
    pub mjsw: f64,
    pub cj: f64,
    pub cjsw: f64,
    pub xpart: f64,
    pub kf: f64,
    pub af: f64,
}

impl Bsim1Size {
    pub fn new(name: &str, m: &Bsim1Model, g: &MosGeom) -> Result<Self, String> {
        let typ = if m.nmos { 1.0 } else { -1.0 };
        let pb = f64::max(m.pb, 0.1);
        let pbsw = f64::max(m.pbsw, 0.1);
        if m.tox <= 0.0 {
            return Err(format!("{name}: BSIM1 model needs TOX > 0"));
        }
        let cox = 3.453e-13 / (m.tox * 1.0e-4); // F/cm²

        let eff_l = g.l - m.deltal * 1e-6;
        let eff_w = g.w - m.deltaw * 1e-6;
        if eff_l <= 0.0 {
            return Err(format!("{name}: effective channel length <= 0"));
        }
        if eff_w <= 0.0 {
            return Err(format!("{name}: effective channel width <= 0"));
        }
        let leff = eff_l * 1e6; // µm
        let weff = eff_w * 1e6;
        let cox_w_over_l = cox * weff / leff;

        let p = |p0: f64, pl: f64, pw: f64| p0 + pl / leff + pw / weff;

        let vfb = p(m.vfb0, m.vfbl, m.vfbw);
        let mut phi = p(m.phi0, m.phil, m.phiw);
        let mut k1 = p(m.k10, m.k1l, m.k1w);
        let mut k2 = p(m.k20, m.k2l, m.k2w);
        if phi < 0.1 {
            phi = 0.1;
        }
        if k1 < 0.0 {
            k1 = 0.0;
        }
        if k2 < 0.0 {
            k2 = 0.0;
        }
        let vt0 = vfb + phi + k1 * phi.sqrt() - k2 * phi;

        let series = |squares: f64| {
            let r = m.rsh * squares;
            if r != 0.0 { 1.0 / r } else { 0.0 }
        };

        Ok(Bsim1Size {
            type_: typ,
            vfb,
            phi,
            k1,
            k2,
            eta: p(m.eta0, m.etal, m.etaw),
            eta_b: p(m.etab0, m.etabl, m.etabw),
            eta_d: p(m.etad0, m.etadl, m.etadw),
            beta_zero: m.mobzero * cox_w_over_l,
            beta_zero_b: p(m.mobzerob0, m.mobzerobl, m.mobzerobw) * cox_w_over_l,
            ugs: p(m.ugs0, m.ugsl, m.ugsw),
            ugs_b: p(m.ugsb0, m.ugsbl, m.ugsbw),
            uds: p(m.uds0, m.udsl, m.udsw),
            uds_b: p(m.udsb0, m.udsbl, m.udsbw),
            uds_d: p(m.udsd0, m.udsdl, m.udsdw),
            beta_vdd: p(m.mobvdd0, m.mobvddl, m.mobvddw) * cox_w_over_l,
            beta_vdd_b: p(m.mobvddb0, m.mobvddbl, m.mobvddbw) * cox_w_over_l,
            beta_vdd_d: f64::max(p(m.mobvddd0, m.mobvdddl, m.mobvdddw) * cox_w_over_l, 0.0),
            n0: p(m.n00, m.n0l, m.n0w),
            nb: p(m.nb0, m.nbl, m.nbw),
            nd: p(m.nd0, m.ndl, m.ndw),
            vt0,
            gd_overlap: eff_w * m.cgdo,
            gs_overlap: eff_w * m.cgso,
            gb_overlap: g.l * m.cgbo,
            ld_gd_overlap: g.w * m.cgdo,
            ld_gs_overlap: g.w * m.cgso,
            ld_gb_overlap: eff_l * m.cgbo,
            drain_conductance: series(g.nrd),
            source_conductance: series(g.nrs),
            cox,
            vdd: m.vdd,
            delta_l: m.deltal,
            delta_w: m.deltaw,
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
        })
    }
}

// State offsets (`bsim1def.h`).
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
pub(crate) const BSIM1_NUM_STATES: usize = 35;

// Slot order (matches `b1set.c`'s TSTALLOC list — same as MOS1's).
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

/// Everything `B1evaluate` hands back.
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

pub(crate) struct Bsim1I {
    drain: Var,
    gate: Var,
    source: Var,
    bulk: Var,
    d_p: Var,
    s_p: Var,
    t: Bsim1Size,
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
    /// `B1dSetup`'s Taylor coefficients (`b1disto.rs`).
    dist: crate::b1disto::B1DistoCoeffs,
}

impl Bsim1I {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],
        primes: [Var; 2],
        t: Bsim1Size,
        geom: MosGeom,
        off: bool,
        ic: (f64, f64, f64),
        st: usize,
    ) -> Self {
        let von = t.vt0; // "added for initialization" (b1temp.c)
        Bsim1I {
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
            dist: crate::b1disto::B1DistoCoeffs::default(),
        }
    }

    /// `B1evaluate`: drain current, derivatives and terminal
    /// charges. `vds ≥ 0` here; the caller swaps terminals for the
    /// inverse mode.
    #[allow(clippy::too_many_lines)]
    fn evaluate(&self, vds: f64, vbs: f64, vgs: f64, charges: bool) -> Eval {
        let t = &self.t;
        let mut e = Eval::default();

        let vfb = t.vfb;
        let mut phi = t.phi;
        let k1 = t.k1;
        let k2 = t.k2;
        let vdd = t.vdd;

        let (ugs, d_ugs_dvbs);
        let u = t.ugs + t.ugs_b * vbs;
        if u <= 0.0 {
            ugs = 0.0;
            d_ugs_dvbs = 0.0;
        } else {
            ugs = u;
            d_ugs_dvbs = t.ugs_b;
        }
        let (uds, d_uds_dvbs, d_uds_dvds);
        let u = t.uds + t.uds_b * vbs + t.uds_d * (vds - vdd);
        if u <= 0.0 {
            uds = 0.0;
            d_uds_dvbs = 0.0;
            d_uds_dvds = 0.0;
        } else {
            let leff = self.geom.l * 1e6 - t.delta_l; // µm
            uds = u / leff;
            d_uds_dvbs = t.uds_b / leff;
            d_uds_dvds = t.uds_d / leff;
        }
        let mut eta = t.eta + t.eta_b * vbs + t.eta_d * (vds - vdd);
        let (d_eta_dvds, d_eta_dvbs);
        if eta <= 0.0 {
            eta = 0.0;
            d_eta_dvds = 0.0;
            d_eta_dvbs = 0.0;
        } else if eta > 1.0 {
            eta = 1.0;
            d_eta_dvds = 0.0;
            d_eta_dvbs = 0.0;
        } else {
            d_eta_dvds = t.eta_d;
            d_eta_dvbs = t.eta_b;
        }
        let vpb = if vbs < 0.0 { phi - vbs } else { phi };
        let sqrt_vpb = vpb.sqrt();
        let von = vfb + phi + k1 * sqrt_vpb - k2 * vpb - eta * vds;
        let vth = von;
        e.von = von;
        let d_vth_dvds = -eta - d_eta_dvds * vds;
        let mut d_vth_dvbs = k2 - 0.5 * k1 / sqrt_vpb - d_eta_dvbs * vds;
        let mut vgs_vth = vgs - vth;

        let g = 1.0 - 1.0 / (1.744 + 0.8364 * vpb);
        let mut a = 1.0 + 0.5 * g * k1 / sqrt_vpb;
        a = f64::max(a, 1.0);
        let arg = f64::max(1.0 + ugs * vgs_vth, 1.0);
        let d_g_dvbs = -0.8364 * (1.0 - g) * (1.0 - g);
        let d_a_dvbs = 0.25 * k1 / sqrt_vpb * (2.0 * d_g_dvbs + g / vpb);

        let mut drain_current = 0.0;
        let mut gm = 0.0;
        let mut gds = 0.0;
        let mut gmbs = 0.0;

        if vgs_vth >= 0.0 {
            // Quadratic interpolation for Beta0.
            let beta_vds_0 = t.beta_zero + t.beta_zero_b * vbs;
            let beta_vdd = t.beta_vdd + t.beta_vdd_b * vbs;
            let d_beta_vdd_dvds = f64::max(t.beta_vdd_d, 0.0);
            let (beta0, d_beta0_dvds, d_beta0_dvbs);
            if vds > vdd {
                beta0 = beta_vdd + d_beta_vdd_dvds * (vds - vdd);
                d_beta0_dvds = d_beta_vdd_dvds;
                d_beta0_dvbs = t.beta_vdd_b;
            } else {
                let vdd_square = vdd * vdd;
                let c1 = (-beta_vdd + beta_vds_0 + d_beta_vdd_dvds * vdd) / vdd_square;
                let c2 = 2.0 * (beta_vdd - beta_vds_0) / vdd - d_beta_vdd_dvds;
                let d_beta_vds_0_dvbs = t.beta_zero_b;
                let d_beta_vdd_dvbs = t.beta_vdd_b;
                let d_c1_dvbs = (d_beta_vds_0_dvbs - d_beta_vdd_dvbs) / vdd_square;
                let d_c2_dvbs = d_c1_dvbs * (-2.0) * vdd;
                beta0 = (c1 * vds + c2) * vds + beta_vds_0;
                d_beta0_dvds = 2.0 * c1 * vds + c2;
                d_beta0_dvbs = d_c1_dvbs * vds * vds + d_c2_dvbs * vds + d_beta_vds_0_dvbs;
            }

            let beta = beta0 / arg;
            let d_beta_dvgs = -beta * ugs / arg;
            let d_beta_dvds = d_beta0_dvds / arg - d_beta_dvgs * d_vth_dvds;
            let d_beta_dvbs = d_beta0_dvbs / arg + beta * ugs * d_vth_dvbs / arg
                - beta * vgs_vth * d_ugs_dvbs / arg;

            let mut vc = uds * vgs_vth / a;
            if vc < 0.0 {
                vc = 0.0;
            }
            let term1 = (1.0 + 2.0 * vc).sqrt();
            let k = 0.5 * (1.0 + vc + term1);
            e.vdsat = f64::max(vgs_vth / (a * k.sqrt()), 0.0);

            if vds < e.vdsat {
                // Triode region.
                let argl1 = f64::max(1.0 + uds * vds, 1.0);
                let argl2 = vgs_vth - 0.5 * a * vds;
                drain_current = beta * argl2 * vds / argl1;
                gm = (d_beta_dvgs * argl2 * vds + beta * vds) / argl1;
                gds = (d_beta_dvds * argl2 * vds + beta * (vgs_vth - vds * d_vth_dvds - a * vds)
                    - drain_current * (vds * d_uds_dvds + uds))
                    / argl1;
                gmbs = (d_beta_dvbs * argl2 * vds
                    + beta * vds * (-d_vth_dvbs - 0.5 * vds * d_a_dvbs)
                    - drain_current * vds * d_uds_dvbs)
                    / argl1;
            } else {
                // Pinchoff (saturation) region.
                let args1 = 1.0 + 1.0 / term1;
                let d_vc_dvgs = uds / a;
                let d_vc_dvds = vgs_vth * d_uds_dvds / a - d_vc_dvgs * d_vth_dvds;
                let d_vc_dvbs =
                    (vgs_vth * d_uds_dvbs - uds * (d_vth_dvbs + vgs_vth * d_a_dvbs / a)) / a;
                let d_k_dvc = 0.5 * args1;
                let d_k_dvgs = d_k_dvc * d_vc_dvgs;
                let d_k_dvds = d_k_dvc * d_vc_dvds;
                let d_k_dvbs = d_k_dvc * d_vc_dvbs;
                let args2 = vgs_vth / a / k;
                let args3 = args2 * vgs_vth;
                drain_current = 0.5 * beta * args3;
                gm = 0.5 * args3 * d_beta_dvgs + beta * args2 - drain_current * d_k_dvgs / k;
                gds = 0.5 * args3 * d_beta_dvds
                    - beta * args2 * d_vth_dvds
                    - drain_current * d_k_dvds / k;
                gmbs = 0.5 * d_beta_dvbs * args3
                    - beta * args2 * d_vth_dvbs
                    - drain_current * (d_a_dvbs / a + d_k_dvbs / k);
            }
        }

        // Subthreshold (always evaluated unless N0 >= 200, per the
        // Peter M. Lee fix of 6/8/90).
        let n0 = t.n0;
        if n0 < 200.0 {
            let nb = t.nb;
            let nd = t.nd;
            let mut n = n0 + nb * vbs + nd * vds;
            if n < 0.5 {
                n = 0.5;
            }
            let warg1 = f64::exp(-vds / VT0);
            let wds = 1.0 - warg1;
            let wgs = f64::exp(vgs_vth / (n * VT0));
            let vtsquare = VT0 * VT0;
            let warg2 = 6.04965 * vtsquare * t.beta_zero;
            let ilimit = 4.5 * vtsquare * t.beta_zero;
            let iexp = warg2 * wgs * wds;
            drain_current += ilimit * iexp / (ilimit + iexp);
            let mut temp1 = ilimit / (ilimit + iexp);
            temp1 *= temp1;
            let mut temp3 = ilimit / (ilimit + wgs * warg2);
            temp3 = temp3 * temp3 * warg2 * wgs;
            gm += temp1 * iexp / (n * VT0);
            gds += temp3 * (-wds / n / VT0 * (d_vth_dvds + vgs_vth * nd / n) + warg1 / VT0);
            gmbs -= temp1 * iexp * (d_vth_dvbs + vgs_vth * nb / n) / (n * VT0);
        }

        // Charge computation.
        if drain_current < 0.0 {
            drain_current = 0.0;
        }
        if gm < 0.0 {
            gm = 0.0;
        }
        if gds < 0.0 {
            gds = 0.0;
        }
        if gmbs < 0.0 {
            gmbs = 0.0;
        }

        // `WLCox = Cox·Leff·Weff·1e4` (Cox in F/cm², lengths in m).
        let wl_cox =
            t.cox * (self.geom.l - t.delta_l * 1e-6) * (self.geom.w - t.delta_w * 1e-6) * 1e4; // F

        if !charges {
            e.gm = f64::max(gm, 0.0);
            e.gds = f64::max(gds, 0.0);
            e.gmbs = f64::max(gmbs, 0.0);
            e.cdrain = f64::max(drain_current, 0.0);
            return e;
        }

        let g = 1.0 - 1.0 / (1.744 + 0.8364 * vpb);
        let mut a = 1.0 + 0.5 * g * k1 / sqrt_vpb;
        a = f64::max(a, 1.0);
        phi = f64::max(0.1, phi);

        let co4v15 = 4.0 / 15.0;
        let vth0 = vfb + phi + k1 * sqrt_vpb;
        vgs_vth = vgs - vth0;
        let arg1 = a * vds;
        let arg2 = vgs_vth - 0.5 * arg1;
        let arg3 = vds - arg1;
        let arg5 = arg1 * arg1;
        d_vth_dvbs = -0.5 * k1 / sqrt_vpb;
        let d_a_dvbs2 = 0.5 * k1 * (0.5 * g / vpb - 0.8364 * (1.0 - g) * (1.0 - g)) / sqrt_vpb;
        let ent = f64::max(arg2, 1.0e-8);
        let d_ent_dvds = -0.5 * a;
        let d_ent_dvbs = -d_vth_dvbs - 0.5 * vds * d_a_dvbs2;
        let vcom = vgs_vth * vgs_vth / 6.0 - 1.25e-1 * arg1 * vgs_vth + 2.5e-2 * arg5;
        let vds_pinchoff = f64::max(vgs_vth / a, 0.0);
        let vgb = vgs - vbs;
        let vgb_vfb = vgb - vfb;

        // The charge section's own dAdVbs (the C recomputes it).
        let d_a_dvbs = d_a_dvbs2;

        if t.xpart >= 1.0 {
            // 0/100 partitioning.
            if vgb_vfb < 0.0 {
                // Accumulation.
                e.qg = wl_cox * vgb_vfb;
                e.qb = -e.qg;
                e.cggb = wl_cox;
                e.cbgb = -wl_cox;
            } else if vgs < vth0 {
                // Subthreshold.
                e.qg = 0.5 * wl_cox * k1 * k1 * (-1.0 + (1.0 + 4.0 * vgb_vfb / (k1 * k1)).sqrt());
                e.qb = -e.qg;
                e.cggb = wl_cox / (1.0 + 4.0 * vgb_vfb / (k1 * k1)).sqrt();
                e.cbgb = -e.cggb;
            } else if vds < vds_pinchoff {
                // Triode.
                let ent_square = ent * ent;
                let argl1 = 1.2e1 * ent_square;
                let argl2 = 1.0 - a;
                let argl3 = arg1 * vds;
                let argl5 = if ent > 1.0e-8 { arg1 / ent } else { 2.0 };
                let argl7 = argl5 / 1.2e1;
                let argl8 = 6.0 * ent;
                let argl9 = 0.125 * argl5 * argl5;
                e.qg = wl_cox * (vgs - vfb - phi - 0.5 * vds + vds * argl7);
                e.qb = wl_cox * (-vth0 + vfb + phi + 0.5 * arg3 - arg3 * argl7);
                e.qd = -wl_cox * (0.5 * vgs_vth - 0.75 * arg1 + 0.125 * arg1 * argl5);
                e.cggb = wl_cox * (1.0 - argl3 / argl1);
                e.cgdb = wl_cox * (-0.5 + arg1 / argl8 - argl3 * d_ent_dvds / argl1);
                let cgbb = wl_cox * (vds * vds * d_a_dvbs * ent - argl3 * d_ent_dvbs) / argl1;
                e.cgsb = -(e.cggb + e.cgdb + cgbb);
                e.cbgb = wl_cox * argl3 * argl2 / argl1;
                e.cbdb = wl_cox * argl2 * (0.5 - arg1 / argl8 + argl3 * d_ent_dvds / argl1);
                let cbbb = -wl_cox
                    * (d_vth_dvbs
                        + 0.5 * vds * d_a_dvbs
                        + vds * vds * ((1.0 - 2.0 * a) * d_a_dvbs * ent - argl2 * a * d_ent_dvbs)
                            / argl1);
                e.cbsb = -(e.cbgb + e.cbdb + cbbb);
                e.cdgb = -wl_cox * (0.5 - argl9);
                e.cddb = wl_cox * (0.75 * a - 0.25 * a * arg1 / ent + argl9 * d_ent_dvds);
                let cdbb = wl_cox
                    * (0.5 * d_vth_dvbs
                        + vds * d_a_dvbs * (0.75 - 0.25 * argl5)
                        + argl9 * d_ent_dvbs);
                e.cdsb = -(e.cdgb + e.cddb + cdbb);
            } else {
                // Saturation.
                let args1 = 1.0 / (3.0 * a);
                e.qg = wl_cox * (vgs - vfb - phi - vgs_vth * args1);
                e.qb = wl_cox * (vfb + phi - vth0 + (1.0 - a) * vgs_vth * args1);
                e.cggb = wl_cox * (1.0 - args1);
                let cgbb = wl_cox * args1 * (d_vth_dvbs + vgs_vth * d_a_dvbs / a);
                e.cgsb = -(e.cggb + cgbb);
                e.cbgb = wl_cox * (args1 - 1.0 / 3.0);
                let cbbb =
                    -wl_cox * ((2.0 / 3.0 + args1) * d_vth_dvbs + vgs_vth * args1 * d_a_dvbs / a);
                e.cbsb = -(e.cbgb + cbbb);
            }
        } else {
            // 40/60 partitioning.
            if vgb_vfb < 0.0 {
                e.qg = wl_cox * vgb_vfb;
                e.qb = -e.qg;
                e.cggb = wl_cox;
                e.cbgb = -wl_cox;
            } else if vgs < vth0 {
                e.qg = 0.5 * wl_cox * k1 * k1 * (-1.0 + (1.0 + 4.0 * vgb_vfb / (k1 * k1)).sqrt());
                e.qb = -e.qg;
                e.cggb = wl_cox / (1.0 + 4.0 * vgb_vfb / (k1 * k1)).sqrt();
                e.cbgb = -e.cggb;
            } else if vds < vds_pinchoff {
                let vgs_vth_square = vgs_vth * vgs_vth;
                let ent_square = ent * ent;
                let argl1 = 1.2e1 * ent_square;
                let argl2 = 1.0 - a;
                let argl3 = arg1 * vds;
                let argl4 = vcom / ent / ent_square;
                let (argl5, argl6) = if ent > 1.0e-8 {
                    (arg1 / ent, vcom / ent_square)
                } else {
                    (2.0, 4.0 / 1.5e1)
                };
                let argl7 = argl5 / 1.2e1;
                let argl8 = 6.0 * ent;
                e.qg = wl_cox * (vgs - vfb - phi - 0.5 * vds + vds * argl7);
                e.qb = wl_cox * (-vth0 + vfb + phi + 0.5 * arg3 - arg3 * argl7);
                e.qd = -wl_cox * (0.5 * (vgs_vth - arg1) + arg1 * argl6);
                e.cggb = wl_cox * (1.0 - argl3 / argl1);
                e.cgdb = wl_cox * (-0.5 + arg1 / argl8 - argl3 * d_ent_dvds / argl1);
                let cgbb = wl_cox * (vds * vds * d_a_dvbs * ent - argl3 * d_ent_dvbs) / argl1;
                e.cgsb = -(e.cggb + e.cgdb + cgbb);
                e.cbgb = wl_cox * argl3 * argl2 / argl1;
                e.cbdb = wl_cox * argl2 * (0.5 - arg1 / argl8 + argl3 * d_ent_dvds / argl1);
                let cbbb = -wl_cox
                    * (d_vth_dvbs
                        + 0.5 * vds * d_a_dvbs
                        + vds * vds * ((1.0 - 2.0 * a) * d_a_dvbs * ent - argl2 * a * d_ent_dvbs)
                            / argl1);
                e.cbsb = -(e.cbgb + e.cbdb + cbbb);
                e.cdgb = -wl_cox
                    * (0.5 + arg1 * (4.0 * vgs_vth - 1.5 * arg1) / argl1 - 2.0 * arg1 * argl4);
                e.cddb = wl_cox
                    * (0.5 * a + 2.0 * arg1 * d_ent_dvds * argl4
                        - a * (2.0 * vgs_vth_square - 3.0 * arg1 * vgs_vth + 0.9 * arg5) / argl1);
                let cdbb = wl_cox
                    * (0.5 * d_vth_dvbs + 0.5 * vds * d_a_dvbs + 2.0 * arg1 * d_ent_dvbs * argl4
                        - vds
                            * (2.0 * vgs_vth_square * d_a_dvbs
                                - 4.0 * a * vgs_vth * d_vth_dvbs
                                - 3.0 * arg1 * vgs_vth * d_a_dvbs
                                + 1.5 * a * arg1 * d_vth_dvbs
                                + 0.9 * arg5 * d_a_dvbs)
                            / argl1);
                e.cdsb = -(e.cdgb + e.cddb + cdbb);
            } else {
                let args1 = 1.0 / (3.0 * a);
                e.qg = wl_cox * (vgs - vfb - phi - vgs_vth * args1);
                e.qb = wl_cox * (vfb + phi - vth0 + (1.0 - a) * vgs_vth * args1);
                e.qd = -co4v15 * wl_cox * vgs_vth;
                e.cggb = wl_cox * (1.0 - args1);
                let cgbb = wl_cox * args1 * (d_vth_dvbs + vgs_vth * d_a_dvbs / a);
                e.cgsb = -(e.cggb + cgbb);
                e.cbgb = wl_cox * (args1 - 1.0 / 3.0);
                let cbbb =
                    -wl_cox * ((2.0 / 3.0 + args1) * d_vth_dvbs + vgs_vth * args1 * d_a_dvbs / a);
                e.cbsb = -(e.cbgb + cbbb);
                e.cdgb = -co4v15 * wl_cox;
                let cdbb = co4v15 * wl_cox * d_vth_dvbs;
                e.cdsb = -(e.cdgb + cdbb);
            }
        }

        e.gm = f64::max(gm, 0.0);
        e.gds = f64::max(gds, 0.0);
        e.gmbs = f64::max(gmbs, 0.0);
        e.cdrain = f64::max(drain_current, 0.0);
        e
    }
}

/// `B1evaluate` on a bare size/geometry (the `.disto` setup's
/// cross-check): `(cdrain, qg, qb, qd)` at local `vds ≥ 0`.
#[cfg(test)]
pub(crate) fn evaluate_for_test(
    t: Bsim1Size,
    geom: MosGeom,
    vds: f64,
    vbs: f64,
    vgs: f64,
) -> (f64, f64, f64, f64) {
    let i = Bsim1I::new([1, 2, 3, 4], [1, 3], t, geom, false, (0.0, 0.0, 0.0), 0);
    let e = i.evaluate(vds, vbs, vgs, true);
    (e.cdrain, e.qg, e.qb, e.qd)
}

impl crate::devices::Device for Bsim1I {
    fn par_load(&self) -> bool {
        true
    }

    /// `B1getic` (`b1getic.c`): VBS, VDS, VGS not given
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

    /// `B1convTest` (`b1cvtest.c`).
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

        // `B1evaluate`, with the terminal swap for the inverse mode:
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

        // Gate charges and companion conductances (`B1mosCap`).
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
            // `B1mosCap`: equivalent conductances…
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

    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QB, t, time_step);
        crate::integ::terr(self.st + QG, t, time_step);
        crate::integ::terr(self.st + QD, t, time_step);
    }

    /// `B1noise` (ngspice-47): see [`b12_noise`].
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(B12_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        let t = &self.t;
        let n = B12Noise {
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
        b12_noise(ctx, &n)
    }

    /// `B1dSetup` (`dev/bsim1/b1dset.c`): see `b1disto.rs`.
    fn disto_setup(&mut self, ctx: &crate::disto::DistoSetupCtx) -> Result<(), &'static str> {
        self.dist = crate::b1disto::dsetup(
            &self.t,
            &self.geom,
            (self.gate, self.bulk, self.d_p, self.s_p),
            ctx.sol,
            ctx.gmin,
        );
        Ok(())
    }

    /// `B1disto` (`dev/bsim1/b1disto.c`) in the load modes.
    fn disto_load(&self, ctx: &mut crate::disto::DistoCtx) {
        crate::b1disto::dload(&self.dist, (self.gate, self.bulk, self.d_p, self.s_p), ctx);
    }

    /// `B1acLoad`.
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

#[cfg(test)]
mod noise_tests {
    use crate::{Circuit, Options};
    use std::path::Path;

    /// Run `regress/<stem>.cir` (one `.noise`) and compare every
    /// vector of both plots with the ngspice-47 golden: relative
    /// `1e-3` (the oracle's reltol; the residual is the 3f5-vs-CODATA
    /// constants, ~2e-5) plus an absolute floor far below the
    /// densities. Returns the worst relative error.
    fn compare(stem: &str, ngspice_offsets: bool, only: &[&str]) -> Result<f64, String> {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus"));
        let text = std::fs::read_to_string(root.join(format!("regress/{stem}.cir"))).unwrap();
        let parsed = dune_netlist::parse_str(stem, &text);
        assert!(parsed.is_clean(), "{:?}", parsed.diags);
        let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
        let mut opts = Options::default();
        let mut d = Vec::new();
        opts.apply(&parsed.netlist.options, &mut d);
        super::NGSPICE_STATE_OFFSET.set(ngspice_offsets.then_some(0.0));
        let run = c.run(&parsed.netlist.analyses[0], &opts);
        super::NGSPICE_STATE_OFFSET.set(None);
        let (sols, _) = run.unwrap();
        let golden = dune_raw::Rawfile::read_from(root.join(format!("golden/{stem}.raw"))).unwrap();
        assert_eq!(sols.len(), golden.plots.len());
        let mut worst = 0.0f64;
        for (sol, plot) in sols.iter().zip(&golden.plots) {
            for v in &plot.variables {
                if !only.is_empty() && !only.iter().any(|o| v.name.contains(o)) {
                    continue;
                }
                let want = plot.real_values(&v.name).unwrap();
                let got = sol
                    .column(&v.name)
                    .ok_or_else(|| format!("{stem}: Dune lacks {}", v.name))?;
                for (k, (a, b)) in got.iter().zip(want).enumerate() {
                    let e = (a - b).abs() / (b.abs() + 1e-30);
                    if (a - b).abs() > 1e-3 * b.abs() + 1e-25 {
                        return Err(format!("{stem} {}[{k}]: Dune {a} vs ngspice {b}", v.name));
                    }
                    worst = worst.max(e);
                }
            }
        }
        Ok(worst)
    }

    /// The gate of the `b1noise`/`b2noise` probes (in `corpus/SKIP`
    /// for the oracle): with ngspice-47's state-offset reading of gm
    /// and Id emulated (`GENstate = 0`, the only BSIM in each deck),
    /// Dune reproduces the golden vector for vector — so the nodes,
    /// conductances, coefficients, W/L/DW/DL/Cox and the integration
    /// are b1noi.c/b2noi.c's — and without it, the channel and
    /// flicker vectors leave the oracle by orders of magnitude while
    /// rd/rs stay equal.
    #[test]
    fn b12_noise_matches_ngspice_offsets() {
        for stem in ["b1noise", "b2noise"] {
            let worst = compare(stem, true, &[]).unwrap();
            eprintln!("{stem}: worst relative error {worst:.2e} (ngspice offsets emulated)");
            assert!(worst < 1e-4, "{stem}: {worst}");
            // Dune's own reading: series resistances (and frequency)
            // unchanged, channel and flicker not.
            compare(stem, false, &["frequency", "m1_rd", "m1_rs"]).unwrap();
            compare(stem, false, &["m1_id"]).unwrap_err();
            compare(stem, false, &["m1_1overf"]).unwrap_err();
        }
    }
}
