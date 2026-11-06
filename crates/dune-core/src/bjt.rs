//! Bipolar junction transistor — the full 3f5 Gummel–Poon model
//! (`dev/bjt/bjtload.c`, `bjttemp.c`, `bjtsetup.c`, `bjtacld.c`,
//! `bjttrunc.c`). One junction pair with leakage diodes, base-width
//! modulation (Early), high-injection roll-off (`qb`), bias-dependent
//! base resistance (`irb`), four charge storages (B-E, B-C internal
//! and external split by `xcjc`, C-S), and Weil's excess-phase
//! approximation in transient.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::dderivs::Dderivs;
use crate::diode::pnjlim;
use crate::disto::{DfCoeffs, DistoCtx, DistoMode, DistoSetupCtx, Kern, d1_term, df_term, jomega};
use crate::matrix::{AcCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};

/// `.model … npn(…)/pnp(…)` parameters, 3f5 names and defaults
/// (`bjt.c` keyword table; defaults from `bjtsetup.c`).
#[derive(Clone, Debug)]
pub struct BjtModel {
    /// `NPN` (+1) or `PNP` (−1).
    pub npn: bool,
    /// `IS`, default 1e-16.
    pub is: f64,
    /// `BF`, default 100.
    pub bf: f64,
    /// `NF`, default 1.
    pub nf: f64,
    /// `VAF`/`VA`, default 0 (= infinite).
    pub vaf: f64,
    /// `IKF`/`IK`, default 0 (= infinite).
    pub ikf: f64,
    /// `ISE`, default 0.
    pub ise: f64,
    /// `NE`, default 1.5.
    pub ne: f64,
    /// `BR`, default 1.
    pub br: f64,
    /// `NR`, default 1.
    pub nr: f64,
    /// `VAR`/`VB`, default 0.
    pub var: f64,
    /// `IKR`, default 0.
    pub ikr: f64,
    /// `ISC`, default 0.
    pub isc: f64,
    /// `NC`, default 2.
    pub nc: f64,
    /// `RB`, default 0.
    pub rb: f64,
    /// `IRB`, default 0.
    pub irb: f64,
    /// `RBM`, default = `RB`.
    pub rbm: Option<f64>,
    /// `RE`, default 0.
    pub re: f64,
    /// `RC`, default 0.
    pub rc: f64,
    /// `CJE`, default 0.
    pub cje: f64,
    /// `VJE`/`PE`, default 0.75.
    pub vje: f64,
    /// `MJE`/`ME`, default 0.33.
    pub mje: f64,
    /// `TF`, default 0.
    pub tf: f64,
    /// `XTF`, default 0.
    pub xtf: f64,
    /// `VTF`, default 0.
    pub vtf: f64,
    /// `ITF`, default 0.
    pub itf: f64,
    /// `PTF` (degrees), default 0.
    pub ptf: f64,
    /// `CJC`, default 0.
    pub cjc: f64,
    /// `VJC`/`PC`, default 0.75.
    pub vjc: f64,
    /// `MJC`/`MC`, default 0.33.
    pub mjc: f64,
    /// `XCJC`, default 1.
    pub xcjc: f64,
    /// `TR`, default 0.
    pub tr: f64,
    /// `CJS`/`CCS`, default 0.
    pub cjs: f64,
    /// `VJS`/`PS`, default 0.75.
    pub vjs: f64,
    /// `MJS`/`MS`, default 0.
    pub mjs: f64,
    /// `XTB`, default 0.
    pub xtb: f64,
    /// `EG`, default 1.11.
    pub eg: f64,
    /// `XTI`, default 3.
    pub xti: f64,
    /// `FC`, default 0.5 (clamped to 0.9999 when given).
    pub fc: Option<f64>,
    /// `KF`/`AF` (noise only).
    pub kf: f64,
    pub af: f64,
    /// `TNOM` in kelvin.
    pub tnom: Option<f64>,
}

impl BjtModel {
    pub fn new(npn: bool) -> Self {
        BjtModel {
            npn,
            is: 1e-16,
            bf: 100.0,
            nf: 1.0,
            vaf: 0.0,
            ikf: 0.0,
            ise: 0.0,
            ne: 1.5,
            br: 1.0,
            nr: 1.0,
            var: 0.0,
            ikr: 0.0,
            isc: 0.0,
            nc: 2.0,
            rb: 0.0,
            irb: 0.0,
            rbm: None,
            re: 0.0,
            rc: 0.0,
            cje: 0.0,
            vje: 0.75,
            mje: 0.33,
            tf: 0.0,
            xtf: 0.0,
            vtf: 0.0,
            itf: 0.0,
            ptf: 0.0,
            cjc: 0.0,
            vjc: 0.75,
            mjc: 0.33,
            xcjc: 1.0,
            tr: 0.0,
            cjs: 0.0,
            vjs: 0.75,
            mjs: 0.0,
            xtb: 0.0,
            eg: 1.11,
            xti: 3.0,
            fc: None,
            kf: 0.0,
            af: 1.0,
            tnom: None,
        }
    }
}

/// Model- and temperature-derived values (`BJTtemp`).
pub(crate) struct BjtTemp {
    pub type_: f64, // +1 NPN, -1 PNP
    pub inv_early_f: f64,
    pub inv_early_r: f64,
    pub inv_roll_f: f64,
    pub inv_roll_r: f64,
    pub col_conduct: f64,
    pub emit_conduct: f64,
    pub base_resist: f64,
    pub min_base_resist: f64,
    pub irb: f64,
    pub tt_vbc_factor: f64,
    pub excess_phase_factor: f64,
    pub f2: f64,
    pub f3: f64,
    pub f6: f64,
    pub f7: f64,
    // Per-instance temperature-adjusted values.
    pub t_sat_cur: f64,
    pub t_beta_f: f64,
    pub t_beta_r: f64,
    pub t_be_leak: f64,
    pub t_bc_leak: f64,
    pub t_be_cap: f64,
    pub t_be_pot: f64,
    pub t_bc_cap: f64,
    pub t_bc_pot: f64,
    pub t_dep_cap: f64,
    pub t_f1: f64,
    pub t_f4: f64,
    pub t_f5: f64,
    pub t_vcrit: f64,
    pub vt: f64,
    // Straight model copies the load needs.
    pub nf: f64,
    pub nr: f64,
    pub ne: f64,
    pub nc: f64,
    pub tf: f64,
    pub tr: f64,
    pub xtf: f64,
    pub itf: f64,
    pub xcjc: f64,
    pub cjs: f64,
    pub vjs: f64,
    pub mjs: f64,
    pub mje: f64,
    pub mjc: f64,
    pub kf: f64,
    pub af: f64,
    pub notes: Vec<String>,
}

fn egfet(t: f64) -> f64 {
    1.16 - (7.02e-4 * t * t) / (t + 1108.0)
}

impl BjtTemp {
    pub fn new(name: &str, m: &BjtModel, temp: f64, default_tnom: f64) -> Self {
        let mut notes = Vec::new();
        let tnom = m.tnom.unwrap_or(default_tnom);
        let fact1 = tnom / REFTEMP;

        let fc = match m.fc {
            Some(fc) if fc > 0.9999 => {
                notes.push(format!("BJT model {name}, parameter fc limited to 0.9999"));
                0.9999
            }
            Some(fc) => fc,
            None => 0.5,
        };
        let xfc = f64::ln(1.0 - fc);
        let f2 = f64::exp((1.0 + m.mje) * xfc);
        let f3 = 1.0 - fc * (1.0 + m.mje);
        let f6 = f64::exp((1.0 + m.mjc) * xfc);
        let f7 = 1.0 - fc * (1.0 + m.mjc);

        let inv = |given: f64| if given != 0.0 { 1.0 / given } else { 0.0 };

        let vt = temp * KOVERQ;
        let fact2 = temp / REFTEMP;
        let eg_t = egfet(temp);
        let arg = -eg_t / (2.0 * BOLTZ * temp) + 1.1150877 / (BOLTZ * (REFTEMP + REFTEMP));
        let pbfact = -2.0 * vt * (1.5 * f64::ln(fact2) + CHARGE * arg);
        let ratlog = f64::ln(temp / tnom);
        let ratio1 = temp / tnom - 1.0;
        let factlog = ratio1 * m.eg / vt + m.xti * ratlog;
        let factor = f64::exp(factlog);
        let t_sat_cur = m.is * factor;
        let bfactor = f64::exp(ratlog * m.xtb);
        let t_beta_f = m.bf * bfactor;
        let t_beta_r = m.br * bfactor;
        let t_be_leak = m.ise * f64::exp(factlog / m.ne) / bfactor;
        let t_bc_leak = m.isc * f64::exp(factlog / m.nc) / bfactor;

        let junction = |pot: f64, mj: f64, cap: f64| {
            let pbo = (pot - {
                // pbfact at tnom
                let eg_nom = egfet(tnom);
                let arg1 = -eg_nom / (BOLTZ * 2.0 * tnom) + 1.1150877 / (2.0 * BOLTZ * REFTEMP);
                let vtnom = tnom * KOVERQ;
                -2.0 * vtnom * (1.5 * f64::ln(fact1) + CHARGE * arg1)
            }) / fact1;
            let gmaold = (pot - pbo) / pbo;
            let mut tcap = cap / (1.0 + mj * (4e-4 * (tnom - REFTEMP) - gmaold));
            let tpot = fact2 * pbo + pbfact;
            let gmanew = (tpot - pbo) / pbo;
            tcap *= 1.0 + mj * (4e-4 * (temp - REFTEMP) - gmanew);
            (tcap, tpot)
        };
        let (t_be_cap, t_be_pot) = junction(m.vje, m.mje, m.cje);
        let (t_bc_cap, t_bc_pot) = junction(m.vjc, m.mjc, m.cjc);

        let t_dep_cap = fc * t_be_pot;
        let t_f1 = t_be_pot * (1.0 - f64::exp((1.0 - m.mje) * xfc)) / (1.0 - m.mje);
        let t_f4 = fc * t_bc_pot;
        let t_f5 = t_bc_pot * (1.0 - f64::exp((1.0 - m.mjc) * xfc)) / (1.0 - m.mjc);
        let t_vcrit = vt * f64::ln(vt / (ROOT2 * m.is));

        BjtTemp {
            type_: if m.npn { 1.0 } else { -1.0 },
            inv_early_f: inv(m.vaf),
            inv_early_r: inv(m.var),
            inv_roll_f: inv(m.ikf),
            inv_roll_r: inv(m.ikr),
            col_conduct: inv(m.rc),
            emit_conduct: inv(m.re),
            base_resist: m.rb,
            min_base_resist: m.rbm.unwrap_or(m.rb),
            irb: m.irb,
            tt_vbc_factor: if m.vtf != 0.0 {
                1.0 / (m.vtf * 1.44)
            } else {
                0.0
            },
            excess_phase_factor: (m.ptf / (180.0 / std::f64::consts::PI)) * m.tf,
            f2,
            f3,
            f6,
            f7,
            t_sat_cur,
            t_beta_f,
            t_beta_r,
            t_be_leak,
            t_bc_leak,
            t_be_cap,
            t_be_pot,
            t_bc_cap,
            t_bc_pot,
            t_dep_cap,
            t_f1,
            t_f4,
            t_f5,
            t_vcrit,
            vt,
            nf: m.nf,
            nr: m.nr,
            ne: m.ne,
            nc: m.nc,
            tf: m.tf,
            tr: m.tr,
            xtf: m.xtf,
            itf: m.itf,
            xcjc: m.xcjc,
            cjs: m.cjs,
            vjs: m.vjs,
            mjs: m.mjs,
            mje: m.mje,
            mjc: m.mjc,
            kf: m.kf,
            af: m.af,
            notes,
        }
    }
}

// State offsets (`bjtdefs.h`).
const VBE: usize = 0;
const VBC: usize = 1;
const CC: usize = 2;
const CB: usize = 3;
const GPI: usize = 4;
const GMU: usize = 5;
const GM: usize = 6;
const GO: usize = 7;
const QBE: usize = 8;
const CQBE: usize = 9;
const QBC: usize = 10;
const CQBC: usize = 11;
const QCS: usize = 12;
const CQCS: usize = 13;
const QBX: usize = 14;
const CQBX: usize = 15;
const GX: usize = 16;
const CEXBC: usize = 17;
const GEQCB: usize = 18;
const GCCS: usize = 19;
const GEQBX: usize = 20;
pub(crate) const BJT_NUM_STATES: usize = 21;

pub(crate) struct BjtI {
    col: Var,
    base: Var,
    emit: Var,
    subst: Var,
    col_p: Var,
    base_p: Var,
    emit_p: Var,
    t: BjtTemp,
    area: f64,
    off: bool,
    ic_vbe: f64,
    ic_vce: f64,
    st: usize,
    s: [Slot; 23],
    d: BjtDistoCoeffs,
}

/// Taylor coefficients of the BJT nonlinearities at the operating
/// point (`bjtdset.c`, the `here->ic_x2 …` fields): the collector
/// current in (vbe, vbc, delayed vbe), the base current and the
/// B-E charge in (vbe, vbc), the base-resistance current in (vbe,
/// vbc, vbb), and the one-variable depletion charges.
#[derive(Clone, Copy, Debug, Default)]
struct BjtDistoCoeffs {
    ic: DfCoeffs,
    ib: DfCoeffs,
    ibb: DfCoeffs,
    qbe: DfCoeffs,
    capbc2: f64,
    capbc3: f64,
    capbx2: f64,
    capbx3: f64,
    capsc2: f64,
    capsc3: f64,
}

/// Second-order Taylor coefficients get the device type (`0.5·type·
/// d2_p2`, `type·d2_pq`), third-order ones do not (`d3_p3/6`,
/// `0.5·d3_p2q`, `d3_pqr`) — `bjtdset.c`'s store block, shared by
/// the MOS setups (`mos*dset.c`).
pub(crate) fn taylor_coeffs(d: &Dderivs, typ: f64) -> DfCoeffs {
    DfCoeffs {
        cxx: 0.5 * typ * d.d2_p2,
        cyy: 0.5 * typ * d.d2_q2,
        czz: 0.5 * typ * d.d2_r2,
        cxy: typ * d.d2_pq,
        cyz: typ * d.d2_qr,
        cxz: typ * d.d2_pr,
        cxxx: d.d3_p3 / 6.,
        cyyy: d.d3_q3 / 6.,
        czzz: d.d3_r3 / 6.,
        cxxy: 0.5 * d.d3_p2q,
        cxxz: 0.5 * d.d3_p2r,
        cxyy: 0.5 * d.d3_pq2,
        cyyz: 0.5 * d.d3_q2r,
        cxzz: 0.5 * d.d3_pr2,
        cyzz: 0.5 * d.d3_qr2,
        cxyz: d.d3_pqr,
    }
}

/// Keep only the (x, y) coefficients — the C stores no `z` terms for
/// `ib` and `qbe` and passes `0.0` in their place.
fn xy_only(c: DfCoeffs) -> DfCoeffs {
    DfCoeffs {
        cxx: c.cxx,
        cyy: c.cyy,
        cxy: c.cxy,
        cxxx: c.cxxx,
        cyyy: c.cyyy,
        cxxy: c.cxxy,
        cxyy: c.cxyy,
        ..DfCoeffs::default()
    }
}

/// `x·e^{−jθ}` for a kernel pair (`r·cosθ + i·sinθ`, `i·cosθ −
/// r·sinθ`): `bjtdisto.c`'s excess-phase delay.
fn delay(r: f64, i: f64, theta: f64) -> (f64, f64) {
    (
        r * theta.cos() + i * theta.sin(),
        i * theta.cos() - r * theta.sin(),
    )
}

/// The delayed-vbe kernels (`z` of the collector current): every
/// kernel of `x` rotated by its own frequency times `td` — ω1 for
/// H1(f1), ±ω2 for H1(±f2), 2ω1 for H2(f1,f1), ω1−ω2 for H2(f1,−f2).
fn delayed(mode: DistoMode, x: &Kern, td: f64, omega1: f64, omega2: f64) -> Kern {
    let mut z = *x;
    (z.r1h1, z.i1h1) = delay(x.r1h1, x.i1h1, omega1 * td);
    match mode {
        DistoMode::TwoF1 => {}
        DistoMode::ThrF1 => {
            (z.r2h11, z.i2h11) = delay(x.r2h11, x.i2h11, 2.0 * omega1 * td);
        }
        DistoMode::F1pF2 => {
            (z.r1h2, z.i1h2) = delay(x.r1h2, x.i1h2, omega2 * td);
        }
        DistoMode::F1mF2 => {
            (z.r1h2, z.i1h2) = delay(x.r1h2, x.i1h2, -omega2 * td);
        }
        DistoMode::TwoF1mF2 => {
            (z.r1h2, z.i1h2) = delay(x.r1h2, x.i1h2, -omega2 * td);
            (z.r2h11, z.i2h11) = delay(x.r2h11, x.i2h11, 2.0 * omega1 * td);
            (z.h2f1f2, z.ih2f1f2) = delay(x.h2f1f2, x.ih2f1f2, (omega1 - omega2) * td);
        }
    }
    z
}

// Slot order (matches `bjtsetup.c`'s TSTALLOC list).
const COL_COLP: usize = 0;
const BASE_BASEP: usize = 1;
const EMIT_EMITP: usize = 2;
const COLP_COL: usize = 3;
const COLP_BASEP: usize = 4;
const COLP_EMITP: usize = 5;
const BASEP_BASE: usize = 6;
const BASEP_COLP: usize = 7;
const BASEP_EMITP: usize = 8;
const EMITP_EMIT: usize = 9;
const EMITP_COLP: usize = 10;
const EMITP_BASEP: usize = 11;
const COL_COL: usize = 12;
const BASE_BASE: usize = 13;
const EMIT_EMIT: usize = 14;
const COLP_COLP: usize = 15;
const BASEP_BASEP: usize = 16;
const EMITP_EMITP: usize = 17;
const SUBST_SUBST: usize = 18;
const COLP_SUBST: usize = 19;
const SUBST_COLP: usize = 20;
const BASE_COLP: usize = 21;
const COLP_BASE: usize = 22;

impl BjtI {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        nodes: [Var; 4],  // c b e s
        primes: [Var; 3], // c' b' e'
        t: BjtTemp,
        area: f64,
        off: bool,
        ic: (f64, f64), // (vbe, vce)
        st: usize,
    ) -> Self {
        BjtI {
            col: nodes[0],
            base: nodes[1],
            emit: nodes[2],
            subst: nodes[3],
            col_p: primes[0],
            base_p: primes[1],
            emit_p: primes[2],
            t,
            area,
            off,
            ic_vbe: ic.0,
            ic_vce: ic.1,
            st,
            s: [TRASH; 23],
            d: BjtDistoCoeffs::default(),
        }
    }
}

impl crate::devices::Device for BjtI {
    /// `BJTconvTest`: predicted vs computed collector and base
    /// currents, evaluated at the previous solution (`CKTrhsOld`).
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.t.type_;
        let st = self.st;
        let vbe = typ * (ctx.sol[self.base_p] - ctx.sol[self.emit_p]);
        let vbc = typ * (ctx.sol[self.base_p] - ctx.sol[self.col_p]);
        let delvbe = vbe - ctx.states.get(0, st + VBE);
        let delvbc = vbc - ctx.states.get(0, st + VBC);
        let cc = ctx.states.get(0, st + CC);
        let cb = ctx.states.get(0, st + CB);
        let gm = ctx.states.get(0, st + GM);
        let go = ctx.states.get(0, st + GO);
        let gpi = ctx.states.get(0, st + GPI);
        let gmu = ctx.states.get(0, st + GMU);
        let cchat = cc + (gm + go) * delvbe - (go + gmu) * delvbc;
        let cbhat = cb + gpi * delvbe + gmu * delvbc;
        let tol = ctx.reltol * f64::max(cchat.abs(), cc.abs()) + ctx.abstol;
        if (cchat - cc).abs() > tol {
            return false;
        }
        let tol = ctx.reltol * f64::max(cbhat.abs(), cb.abs()) + ctx.abstol;
        (cbhat - cb).abs() <= tol
    }

    /// `BJTnoise` (ngspice): thermal on RC/RB/RE, shot on Ic and
    /// Ib, flicker on Ib, plus the total.
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(&["_rc", "_rb", "_re", "_ic", "_ib", "_1overf", ""])
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        const N_MINLOG: f64 = 1e-38;
        let st = self.st;
        let rc = ctx.thermal(self.col_p, self.col, self.t.col_conduct * self.area);
        let rb = ctx.thermal(self.base_p, self.base, ctx.states.get(0, st + GX));
        let re = ctx.thermal(self.emit_p, self.emit, self.t.emit_conduct * self.area);
        let ic = ctx.shot(self.col_p, self.emit_p, ctx.states.get(0, st + CC));
        let ib = ctx.shot(self.base_p, self.emit_p, ctx.states.get(0, st + CB));
        let cb = ctx.states.get(0, st + CB);
        let flicker = ctx.gain2(self.base_p, self.emit_p)
            * self.t.kf
            * f64::exp(self.t.af * f64::ln(f64::max(cb.abs(), N_MINLOG)))
            / ctx.freq;
        let total = rc + rb + re + ic + ib + flicker;
        vec![rc, rb, re, ic, ib, flicker, total]
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (c, b, e, s) = (self.col, self.base, self.emit, self.subst);
        let (cp, bp, ep) = (self.col_p, self.base_p, self.emit_p);
        self.s = [
            mb.slot(c, cp),
            mb.slot(b, bp),
            mb.slot(e, ep),
            mb.slot(cp, c),
            mb.slot(cp, bp),
            mb.slot(cp, ep),
            mb.slot(bp, b),
            mb.slot(bp, cp),
            mb.slot(bp, ep),
            mb.slot(ep, e),
            mb.slot(ep, cp),
            mb.slot(ep, bp),
            mb.slot(c, c),
            mb.slot(b, b),
            mb.slot(e, e),
            mb.slot(cp, cp),
            mb.slot(bp, bp),
            mb.slot(ep, ep),
            mb.slot(s, s),
            mb.slot(cp, s),
            mb.slot(s, cp),
            mb.slot(b, cp),
            mb.slot(cp, b),
        ];
    }

    #[allow(clippy::too_many_lines)]
    fn load(&mut self, ctx: &mut LoadCtx) {
        let t = &self.t;
        let typ = t.type_;
        let vt = t.vt;
        let st = self.st;

        let csat = t.t_sat_cur * self.area;
        let rbpr = t.min_base_resist / self.area;
        let rbpi = t.base_resist / self.area - rbpr;
        let gcpr = t.col_conduct * self.area;
        let gepr = t.emit_conduct * self.area;
        let oik = t.inv_roll_f / self.area;
        let c2 = t.t_be_leak * self.area;
        let vte = t.ne * vt;
        let oikr = t.inv_roll_r / self.area;
        let c4 = t.t_bc_leak * self.area;
        let vtc = t.nc * vt;
        let td = t.excess_phase_factor;
        let xjrb = t.irb * self.area;

        let mut gccs = 0.0;
        let mut geqbx = 0.0;
        let mut geqcb = 0.0;

        // Bias resolution (the C's initialization ladder).
        let mut icheck = true;
        let (mut vbe, mut vbc);
        let (vbx, vcs);
        match ctx.mode {
            InitMode::Smsig => {
                vbe = ctx.states.get(0, st + VBE);
                vbc = ctx.states.get(0, st + VBC);
                vbx = typ * (ctx.sol[self.base] - ctx.sol[self.col_p]);
                vcs = typ * (ctx.sol[self.subst] - ctx.sol[self.col_p]);
            }
            InitMode::Tran => {
                vbe = ctx.states.get(1, st + VBE);
                vbc = ctx.states.get(1, st + VBC);
                if ctx.kind == AnKind::Tran && ctx.uic {
                    vbx = typ * (self.ic_vbe - self.ic_vce);
                    vcs = 0.0;
                } else {
                    vbx = typ * (ctx.sol[self.base] - ctx.sol[self.col_p]);
                    vcs = typ * (ctx.sol[self.subst] - ctx.sol[self.col_p]);
                }
            }
            InitMode::Jct if ctx.kind == AnKind::TranOp && ctx.uic => {
                vbe = typ * self.ic_vbe;
                let vce = typ * self.ic_vce;
                vbc = vbe - vce;
                vbx = vbc;
                vcs = 0.0;
            }
            InitMode::Jct if !self.off => {
                vbe = t.t_vcrit;
                vbc = 0.0;
                // "ERROR: need to initialize VCS, VBX here" (sic).
                vcs = 0.0;
                vbx = 0.0;
            }
            // `INITJCT` with `off`, or `INITFIX` with `off`.
            InitMode::Jct => {
                vbe = 0.0;
                vbc = 0.0;
                vcs = 0.0;
                vbx = 0.0;
            }
            InitMode::Fix if self.off => {
                vbe = 0.0;
                vbc = 0.0;
                vcs = 0.0;
                vbx = 0.0;
            }
            _ => {
                if ctx.mode == InitMode::Pred {
                    let xfact = ctx.pred_xfact;
                    let s1 = |o: usize| ctx.states.get(1, st + o);
                    vbe = (1.0 + xfact) * s1(VBE) - xfact * ctx.states.get(2, st + VBE);
                    vbc = (1.0 + xfact) * s1(VBC) - xfact * ctx.states.get(2, st + VBC);
                    for o in [VBE, VBC, CC, CB, GPI, GMU, GM, GO, GX] {
                        let v = ctx.states.get(1, st + o);
                        ctx.states.set(0, st + o, v);
                    }
                } else {
                    vbe = typ * (ctx.sol[self.base_p] - ctx.sol[self.emit_p]);
                    vbc = typ * (ctx.sol[self.base_p] - ctx.sol[self.col_p]);
                }
                vbx = typ * (ctx.sol[self.base] - ctx.sol[self.col_p]);
                vcs = typ * (ctx.sol[self.subst] - ctx.sol[self.col_p]);
                // Limit the nonlinear branch voltages.
                let mut ichk1 = true;
                vbe = pnjlim(vbe, ctx.states.get(0, st + VBE), vt, t.t_vcrit, &mut icheck);
                vbc = pnjlim(vbc, ctx.states.get(0, st + VBC), vt, t.t_vcrit, &mut ichk1);
                if ichk1 {
                    icheck = true;
                }
            }
        }

        // DC currents and conductances, in the ngspice form: the
        // pure junction keeps its exponential with a C³-continuous
        // polynomial tail below −3·n·vt, and gmin rides on the
        // leakage branches — the 3f5 put gmin inside gbe/gbc, where
        // 1/βR amplifies it into gmu and the NEWCONV current test
        // can never settle on an `off` device
        // (`corpus/KNOWN_DIFFS.md`).
        let e = crate::consts::E;
        let vtn = vt * t.nf;
        let (cbe, gbe, mut cben, mut gben);
        if vbe >= -3.0 * vtn {
            let evbe = f64::exp(vbe / vtn);
            cbe = csat * (evbe - 1.0);
            gbe = csat * evbe / vtn;
        } else {
            let arg = 3.0 * vtn / (vbe * e);
            let arg = arg * arg * arg;
            cbe = -csat * (1.0 + arg);
            gbe = csat * 3.0 * arg / vbe;
        }
        if c2 == 0.0 {
            cben = 0.0;
            gben = 0.0;
        } else if vbe >= -3.0 * vte {
            let evben = f64::exp(vbe / vte);
            cben = c2 * (evben - 1.0);
            gben = c2 * evben / vte;
        } else {
            let arg = 3.0 * vte / (vbe * e);
            let arg = arg * arg * arg;
            cben = -c2 * (1.0 + arg);
            gben = c2 * 3.0 * arg / vbe;
        }
        gben += ctx.gmin;
        cben += ctx.gmin * vbe;
        let vtn = vt * t.nr;
        let (cbc, gbc, mut cbcn, mut gbcn);
        if vbc >= -3.0 * vtn {
            let evbc = f64::exp(vbc / vtn);
            cbc = csat * (evbc - 1.0);
            gbc = csat * evbc / vtn;
        } else {
            let arg = 3.0 * vtn / (vbc * e);
            let arg = arg * arg * arg;
            cbc = -csat * (1.0 + arg);
            gbc = csat * 3.0 * arg / vbc;
        }
        if c4 == 0.0 {
            cbcn = 0.0;
            gbcn = 0.0;
        } else if vbc >= -3.0 * vtc {
            let evbcn = f64::exp(vbc / vtc);
            cbcn = c4 * (evbcn - 1.0);
            gbcn = c4 * evbcn / vtc;
        } else {
            let arg = 3.0 * vtc / (vbc * e);
            let arg = arg * arg * arg;
            cbcn = -c4 * (1.0 + arg);
            gbcn = c4 * 3.0 * arg / vbc;
        }
        gbcn += ctx.gmin;
        cbcn += ctx.gmin * vbc;

        // Base charge (Early + high injection).
        let q1 = 1.0 / (1.0 - t.inv_early_f * vbc - t.inv_early_r * vbe);
        let (qb, dqbdve, dqbdvc);
        if oik == 0.0 && oikr == 0.0 {
            qb = q1;
            dqbdve = q1 * qb * t.inv_early_r;
            dqbdvc = q1 * qb * t.inv_early_f;
        } else {
            let q2 = oik * cbe + oikr * cbc;
            let arg = f64::max(0.0, 1.0 + 4.0 * q2);
            let mut sqarg = 1.0;
            if arg != 0.0 {
                sqarg = arg.sqrt();
            }
            qb = q1 * (1.0 + sqarg) / 2.0;
            dqbdve = q1 * (qb * t.inv_early_r + oik * gbe / sqarg);
            dqbdvc = q1 * (qb * t.inv_early_f + oikr * gbc / sqarg);
        }

        // Weil's excess-phase approximation (backward Euler).
        let mut cc = 0.0;
        let mut cex = cbe;
        let mut gex = gbe;
        if ctx.kind == AnKind::Tran && td != 0.0 {
            let arg1 = ctx.delta / td;
            let arg2 = 3.0 * arg1;
            let arg1 = arg2 * arg1;
            let denom = 1.0 + arg1 + arg2;
            let arg3 = arg1 / denom;
            if ctx.mode == InitMode::Tran {
                ctx.states.set(1, st + CEXBC, cbe / qb);
                let v = ctx.states.get(1, st + CEXBC);
                ctx.states.set(2, st + CEXBC, v);
            }
            cc = (ctx.states.get(1, st + CEXBC) * (1.0 + ctx.pred_xfact + arg2)
                - ctx.states.get(2, st + CEXBC) * ctx.pred_xfact)
                / denom;
            cex = cbe * arg3;
            gex = gbe * arg3;
            ctx.states.set(0, st + CEXBC, cc + cex / qb);
        }

        // DC incremental conductances.
        let mut cc = cc + (cex - cbc) / qb - cbc / t.t_beta_r - cbcn;
        let mut cb = cbe / t.t_beta_f + cben + cbc / t.t_beta_r + cbcn;
        let mut gx = rbpr + rbpi / qb;
        if xjrb != 0.0 {
            let arg1 = f64::max(cb / xjrb, 1e-9);
            let arg2 = (-1.0 + (1.0 + 14.59025 * arg1).sqrt()) / 2.4317 / arg1.sqrt();
            let arg1t = arg2.tan();
            gx = rbpr + 3.0 * rbpi * (arg1t - arg2) / arg2 / arg1t / arg1t;
        }
        if gx != 0.0 {
            gx = 1.0 / gx;
        }
        let mut gpi = gbe / t.t_beta_f + gben;
        let mut gmu = gbc / t.t_beta_r + gbcn;
        let go = (gbc + (cex - cbc) * dqbdvc / qb) / qb;
        let gm = (gex - (cex - cbc) * dqbdve / qb) / qb - go;

        // Charge storage.
        let smsig = ctx.mode == InitMode::Smsig;
        let tranop_uic = ctx.kind == AnKind::TranOp && ctx.uic;
        if ctx.kind == AnKind::Tran || tranop_uic || smsig {
            let tf = t.tf;
            let tr = t.tr;
            let czbe = t.t_be_cap * self.area;
            let pe = t.t_be_pot;
            let xme = t.mje;
            let cdis = t.xcjc;
            let ctot = t.t_bc_cap * self.area;
            let czbc = ctot * cdis;
            let czbx = ctot - czbc;
            let pc = t.t_bc_pot;
            let xmc = t.mjc;
            let fcpe = t.t_dep_cap;
            let czcs = t.cjs * self.area;
            let ps = t.vjs;
            let xms = t.mjs;
            let xtf = t.xtf;
            let ovtf = t.tt_vbc_factor;
            let xjtf = t.itf * self.area;

            let (mut cbe_q, mut gbe_q) = (cbe, gbe);
            if tf != 0.0 && vbe > 0.0 {
                let mut argtf = 0.0;
                let mut arg2 = 0.0;
                let mut arg3 = 0.0;
                if xtf != 0.0 {
                    argtf = xtf;
                    if ovtf != 0.0 {
                        argtf *= f64::exp(vbc * ovtf);
                    }
                    arg2 = argtf;
                    if xjtf != 0.0 {
                        let temp = cbe / (cbe + xjtf);
                        argtf = argtf * temp * temp;
                        arg2 = argtf * (3.0 - temp - temp);
                    }
                    arg3 = cbe * argtf * ovtf;
                }
                cbe_q = cbe * (1.0 + argtf) / qb;
                gbe_q = (gbe * (1.0 + arg2) - cbe_q * dqbdve) / qb;
                geqcb = tf * (arg3 - cbe_q * dqbdvc) / qb;
            }

            #[allow(clippy::needless_late_init)] // mirrors bjtload.c
            let capbe;
            if vbe < fcpe {
                let arg = 1.0 - vbe / pe;
                let sarg = f64::exp(-xme * f64::ln(arg));
                ctx.states.set(
                    0,
                    st + QBE,
                    tf * cbe_q + pe * czbe * (1.0 - arg * sarg) / (1.0 - xme),
                );
                capbe = tf * gbe_q + czbe * sarg;
            } else {
                let f1 = t.t_f1;
                let f2 = t.f2;
                let f3 = t.f3;
                let czbef2 = czbe / f2;
                ctx.states.set(
                    0,
                    st + QBE,
                    tf * cbe_q
                        + czbe * f1
                        + czbef2
                            * (f3 * (vbe - fcpe) + (xme / (pe + pe)) * (vbe * vbe - fcpe * fcpe)),
                );
                capbe = tf * gbe_q + czbef2 * (f3 + xme * vbe / pe);
            }
            let fcpc = t.t_f4;
            let f1 = t.t_f5;
            let f2 = t.f6;
            let f3 = t.f7;
            #[allow(clippy::needless_late_init)] // mirrors bjtload.c
            let capbc;
            if vbc < fcpc {
                let arg = 1.0 - vbc / pc;
                let sarg = f64::exp(-xmc * f64::ln(arg));
                ctx.states.set(
                    0,
                    st + QBC,
                    tr * cbc + pc * czbc * (1.0 - arg * sarg) / (1.0 - xmc),
                );
                capbc = tr * gbc + czbc * sarg;
            } else {
                let czbcf2 = czbc / f2;
                ctx.states.set(
                    0,
                    st + QBC,
                    tr * cbc
                        + czbc * f1
                        + czbcf2
                            * (f3 * (vbc - fcpc) + (xmc / (pc + pc)) * (vbc * vbc - fcpc * fcpc)),
                );
                capbc = tr * gbc + czbcf2 * (f3 + xmc * vbc / pc);
            }
            #[allow(clippy::needless_late_init)] // mirrors bjtload.c
            let capbx;
            if vbx < fcpc {
                let arg = 1.0 - vbx / pc;
                let sarg = f64::exp(-xmc * f64::ln(arg));
                ctx.states
                    .set(0, st + QBX, pc * czbx * (1.0 - arg * sarg) / (1.0 - xmc));
                capbx = czbx * sarg;
            } else {
                let czbxf2 = czbx / f2;
                ctx.states.set(
                    0,
                    st + QBX,
                    czbx * f1
                        + czbxf2
                            * (f3 * (vbx - fcpc) + (xmc / (pc + pc)) * (vbx * vbx - fcpc * fcpc)),
                );
                capbx = czbxf2 * (f3 + xmc * vbx / pc);
            }
            #[allow(clippy::needless_late_init)] // mirrors bjtload.c
            let capcs;
            if vcs < 0.0 {
                let arg = 1.0 - vcs / ps;
                let sarg = f64::exp(-xms * f64::ln(arg));
                ctx.states
                    .set(0, st + QCS, ps * czcs * (1.0 - arg * sarg) / (1.0 - xms));
                capcs = czcs * sarg;
            } else {
                ctx.states
                    .set(0, st + QCS, vcs * czcs * (1.0 + xms * vcs / (2.0 * ps)));
                capcs = czcs * (1.0 + xms * vcs / ps);
            }

            if !tranop_uic {
                if smsig {
                    // Store the small-signal parameters and stop
                    // (`continue` in the C).
                    ctx.states.set(0, st + CQBE, capbe);
                    ctx.states.set(0, st + CQBC, capbc);
                    ctx.states.set(0, st + CQCS, capcs);
                    ctx.states.set(0, st + CQBX, capbx);
                    ctx.states.set(0, st + CEXBC, geqcb);
                    return;
                }
                if ctx.mode == InitMode::Tran {
                    for o in [QBE, QBC, QBX, QCS] {
                        let v = ctx.states.get(0, st + o);
                        ctx.states.set(1, st + o, v);
                    }
                }
                let (geq, _ceq) = ctx.integrate(capbe, st + QBE);
                geqcb *= ctx.ag[0];
                gpi += geq;
                cb += ctx.states.get(0, st + CQBE);
                let (geq, _ceq) = ctx.integrate(capbc, st + QBC);
                gmu += geq;
                cb += ctx.states.get(0, st + CQBC);
                cc -= ctx.states.get(0, st + CQBC);
                if ctx.mode == InitMode::Tran {
                    for o in [CQBE, CQBC] {
                        let v = ctx.states.get(0, st + o);
                        ctx.states.set(1, st + o, v);
                    }
                }
            }

            // C-S and B-X junction companions (transient only).
            if ctx.kind == AnKind::Tran {
                let (g, _) = ctx.integrate(capcs, st + QCS);
                gccs = g;
                let (g, _) = ctx.integrate(capbx, st + QBX);
                geqbx = g;
                if ctx.mode == InitMode::Tran {
                    for o in [CQBX, CQCS] {
                        let v = ctx.states.get(0, st + o);
                        ctx.states.set(1, st + o, v);
                    }
                }
            }
        }

        // Convergence: limiting counts as non-convergence (NEWCONV).
        if !(ctx.mode == InitMode::Fix && self.off) && icheck {
            ctx.noncon = true;
        }

        ctx.states.set(0, st + VBE, vbe);
        ctx.states.set(0, st + VBC, vbc);
        ctx.states.set(0, st + CC, cc);
        ctx.states.set(0, st + CB, cb);
        ctx.states.set(0, st + GPI, gpi);
        ctx.states.set(0, st + GMU, gmu);
        ctx.states.set(0, st + GM, gm);
        ctx.states.set(0, st + GO, go);
        ctx.states.set(0, st + GX, gx);
        ctx.states.set(0, st + GEQCB, geqcb);
        ctx.states.set(0, st + GCCS, gccs);
        ctx.states.set(0, st + GEQBX, geqbx);

        // Load the RHS (Norton equivalents, with the NPN/PNP sign).
        let ceqcs = typ * (ctx.states.get(0, st + CQCS) - vcs * gccs);
        let ceqbx = typ * (ctx.states.get(0, st + CQBX) - vbx * geqbx);
        let ceqbe = typ * (cc + cb - vbe * (gm + go + gpi) + vbc * (go - geqcb));
        let ceqbc = typ * (-cc + vbe * (gm + go) - vbc * (gmu + go));

        ctx.rhs[self.base] += -ceqbx;
        ctx.rhs[self.col_p] += ceqcs + ceqbx + ceqbc;
        ctx.rhs[self.base_p] += -ceqbe - ceqbc;
        ctx.rhs[self.emit_p] += ceqbe;
        ctx.rhs[self.subst] += -ceqcs;

        // Load the Y matrix.
        ctx.add(self.s[COL_COL], gcpr);
        ctx.add(self.s[BASE_BASE], gx + geqbx);
        ctx.add(self.s[EMIT_EMIT], gepr);
        ctx.add(self.s[COLP_COLP], gmu + go + gcpr + gccs + geqbx);
        ctx.add(self.s[BASEP_BASEP], gx + gpi + gmu + geqcb);
        ctx.add(self.s[EMITP_EMITP], gpi + gepr + gm + go);
        ctx.add(self.s[COL_COLP], -gcpr);
        ctx.add(self.s[BASE_BASEP], -gx);
        ctx.add(self.s[EMIT_EMITP], -gepr);
        ctx.add(self.s[COLP_COL], -gcpr);
        ctx.add(self.s[COLP_BASEP], -gmu + gm);
        ctx.add(self.s[COLP_EMITP], -gm - go);
        ctx.add(self.s[BASEP_BASE], -gx);
        ctx.add(self.s[BASEP_COLP], -gmu - geqcb);
        ctx.add(self.s[BASEP_EMITP], -gpi);
        ctx.add(self.s[EMITP_EMIT], -gepr);
        ctx.add(self.s[EMITP_COLP], -go + geqcb);
        ctx.add(self.s[EMITP_BASEP], -gpi - gm - geqcb);
        ctx.add(self.s[SUBST_SUBST], gccs);
        ctx.add(self.s[COLP_SUBST], -gccs);
        ctx.add(self.s[SUBST_COLP], -gccs);
        ctx.add(self.s[BASE_COLP], -geqbx);
        ctx.add(self.s[COLP_BASE], -geqbx);
    }

    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        crate::integ::terr(self.st + QBE, t, time_step);
        crate::integ::terr(self.st + QBC, t, time_step);
        crate::integ::terr(self.st + QCS, t, time_step);
    }

    /// `BJTacLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let st = self.st;
        let gcpr = self.t.col_conduct * self.area;
        let gepr = self.t.emit_conduct * self.area;
        let gpi = ctx.states.get(0, st + GPI);
        let gmu = ctx.states.get(0, st + GMU);
        let mut gm = ctx.states.get(0, st + GM);
        let go = ctx.states.get(0, st + GO);
        let mut xgm = 0.0;
        let td = self.t.excess_phase_factor;
        if td != 0.0 {
            let arg = td * ctx.omega;
            gm += go;
            xgm = -gm * arg.sin();
            gm = gm * arg.cos() - go;
        }
        let gx = ctx.states.get(0, st + GX);
        let xcpi = ctx.states.get(0, st + CQBE) * ctx.omega;
        let xcmu = ctx.states.get(0, st + CQBC) * ctx.omega;
        let xcbx = ctx.states.get(0, st + CQBX) * ctx.omega;
        let xccs = ctx.states.get(0, st + CQCS) * ctx.omega;
        let xcmcb = ctx.states.get(0, st + CEXBC) * ctx.omega;

        ctx.add_g(self.s[COL_COL], gcpr);
        ctx.add_g(self.s[BASE_BASE], gx);
        ctx.add_b(self.s[BASE_BASE], xcbx);
        ctx.add_g(self.s[EMIT_EMIT], gepr);
        ctx.add_g(self.s[COLP_COLP], gmu + go + gcpr);
        ctx.add_b(self.s[COLP_COLP], xcmu + xccs + xcbx);
        ctx.add_g(self.s[BASEP_BASEP], gx + gpi + gmu);
        ctx.add_b(self.s[BASEP_BASEP], xcpi + xcmu + xcmcb);
        ctx.add_g(self.s[EMITP_EMITP], gpi + gepr + gm + go);
        ctx.add_b(self.s[EMITP_EMITP], xcpi + xgm);
        ctx.add_g(self.s[COL_COLP], -gcpr);
        ctx.add_g(self.s[BASE_BASEP], -gx);
        ctx.add_g(self.s[EMIT_EMITP], -gepr);
        ctx.add_g(self.s[COLP_COL], -gcpr);
        ctx.add_g(self.s[COLP_BASEP], -gmu + gm);
        ctx.add_b(self.s[COLP_BASEP], -xcmu + xgm);
        ctx.add_g(self.s[COLP_EMITP], -gm - go);
        ctx.add_b(self.s[COLP_EMITP], -xgm);
        ctx.add_g(self.s[BASEP_BASE], -gx);
        ctx.add_g(self.s[BASEP_COLP], -gmu);
        ctx.add_b(self.s[BASEP_COLP], -xcmu - xcmcb);
        ctx.add_g(self.s[BASEP_EMITP], -gpi);
        ctx.add_b(self.s[BASEP_EMITP], -xcpi);
        ctx.add_g(self.s[EMITP_EMIT], -gepr);
        ctx.add_g(self.s[EMITP_COLP], -go);
        ctx.add_b(self.s[EMITP_COLP], xcmcb);
        ctx.add_g(self.s[EMITP_BASEP], -gpi - gm);
        ctx.add_b(self.s[EMITP_BASEP], -xcpi - xgm - xcmcb);
        ctx.add_b(self.s[SUBST_SUBST], xccs);
        ctx.add_b(self.s[COLP_SUBST], -xccs);
        ctx.add_b(self.s[SUBST_COLP], -xccs);
        ctx.add_b(self.s[BASE_COLP], -xcbx);
        ctx.add_b(self.s[COLP_BASE], -xcbx);
    }

    /// `BJTdSetup` (`dev/bjt/bjtdset.c`): Taylor coefficients of the
    /// Gummel–Poon nonlinearities at the operating point in
    /// `CKTrhsOld`, carried through the model equations with the
    /// `Dderivs` algebra (p = vbe, q = vbc, r = delayed vbe for the
    /// collector current; r = vbb for the base-resistance current).
    /// Kept as the C has them: `vbc` is taken from the *external*
    /// base node (like `vbx`), the bias-dependent `rbb` multiplies
    /// an `ib` variable whose value is 0 (so `vbb = rbb·ib` loses
    /// its `ib·∂rbb` term), and `qbe` gets no `tf` charge unless
    /// `vbe > 0`.
    #[allow(clippy::too_many_lines)]
    fn disto_setup(&mut self, ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        let t = &self.t;
        let sol = ctx.sol;
        let gmin = ctx.gmin;
        let typ = t.type_;
        let area = self.area;
        let vt = t.vt;

        let d_p = Dderivs::var_p();
        let d_r = Dderivs::var_r();
        let zero = Dderivs::default();

        let csat = t.t_sat_cur * area;
        let rbpr = t.min_base_resist / area;
        let rbpi = t.base_resist / area - rbpr;
        let oik = t.inv_roll_f / area;
        let c2 = t.t_be_leak * area;
        let vte = t.ne * vt;
        let oikr = t.inv_roll_r / area;
        let c4 = t.t_bc_leak * area;
        let vtc = t.nc * vt;
        let xjrb = t.irb * area;

        let vbe = typ * (sol[self.base_p] - sol[self.emit_p]);
        let vbc = typ * (sol[self.base] - sol[self.col_p]);
        let vbx = typ * (sol[self.base] - sol[self.col_p]);
        let vsc = typ * (sol[self.subst] - sol[self.col_p]);
        let vbb = typ * (sol[self.base] - sol[self.base_p]);

        // Junction currents and their derivatives (derivatives, not
        // Taylor coefficients: no 1/2!, 1/3!).
        let vtn = vt * t.nf;
        let (cbe, gbe, gbe2, gbe3, cben, gben, gben2, gben3);
        if vbe > -5.0 * vtn {
            let evbe = f64::exp(vbe / vtn);
            cbe = csat * (evbe - 1.0) + gmin * vbe;
            gbe = csat * evbe / vtn + gmin;
            gbe2 = csat * evbe / vtn / vtn;
            gbe3 = gbe2 / vtn;
            if c2 == 0.0 {
                cben = 0.0;
                (gben, gben2, gben3) = (0.0, 0.0, 0.0);
            } else {
                let evben = f64::exp(vbe / vte);
                cben = c2 * (evben - 1.0);
                gben = c2 * evben / vte;
                gben2 = gben / vte;
                gben3 = gben2 / vte;
            }
        } else {
            gbe = -csat / vbe + gmin;
            (gbe2, gbe3, gben2, gben3) = (0.0, 0.0, 0.0, 0.0);
            cbe = gbe * vbe;
            gben = -c2 / vbe;
            cben = gben * vbe;
        }
        let vtn = vt * t.nr;
        let (cbc, gbc, gbc2, gbc3, cbcn, gbcn, gbcn2, gbcn3);
        if vbc > -5.0 * vtn {
            let evbc = f64::exp(vbc / vtn);
            cbc = csat * (evbc - 1.0) + gmin * vbc;
            gbc = csat * evbc / vtn + gmin;
            gbc2 = csat * evbc / vtn / vtn;
            gbc3 = gbc2 / vtn;
            if c4 == 0.0 {
                cbcn = 0.0;
                (gbcn, gbcn2, gbcn3) = (0.0, 0.0, 0.0);
            } else {
                let evbcn = f64::exp(vbc / vtc);
                cbcn = c4 * (evbcn - 1.0);
                gbcn = c4 * evbcn / vtc;
                gbcn2 = gbcn / vtc;
                gbcn3 = gbcn2 / vtc;
            }
        } else {
            gbc = -csat / vbc + gmin;
            (gbc2, gbc3) = (0.0, 0.0);
            cbc = gbc * vbc;
            gbcn = -c4 / vbc;
            (gbcn2, gbcn3) = (0.0, 0.0);
            cbcn = gbcn * vbc;
        }

        // Base charge: q1 in (vbe, vbc), then qb with roll-off.
        let d_dummy = Dderivs {
            value: 1.0 - t.inv_early_f * vbc - t.inv_early_r * vbe,
            d1_p: -t.inv_early_r,
            d1_q: -t.inv_early_f,
            ..zero
        };
        let d_q1 = d_dummy.inv();
        let d_qb = if oik == 0.0 && oikr == 0.0 {
            d_q1
        } else {
            let q2 = oik * cbe + oikr * cbc;
            let d_q2 = Dderivs {
                value: q2,
                d1_p: oik * gbe,
                d1_q: oikr * gbc,
                d2_p2: oik * gbe2,
                d2_q2: oikr * gbc2,
                d3_p3: oik * gbe3,
                d3_q3: oikr * gbc3,
                ..zero
            };
            let arg = f64::max(0.0, 1.0 + 4.0 * q2);
            let d_arg = if arg == 0.0 {
                zero
            } else {
                let mut a = d_q2.times(4.0);
                a.value += 1.0;
                a
            };
            let d_sqarg = if arg != 0.0 {
                d_arg.sqrt()
            } else {
                Dderivs::constant(1.0)
            };
            let mut d_dummy = d_sqarg;
            d_dummy.value += 1.0;
            d_q1.mult(&d_dummy).times(0.5)
        };

        // ic = (cbe(vbed) − cbc(vbc))/qb(vbe, vbc) − cbc/BR − cbcn.
        let d_dummy = Dderivs {
            value: cbe - cbc,
            d1_r: gbe,
            d2_r2: gbe2,
            d3_r3: gbe3,
            d1_q: -gbc,
            d2_q2: -gbc2,
            d3_q3: -gbc3,
            ..zero
        };
        let mut d_ic = d_dummy.div(&d_qb);
        d_ic.value -= cbc / t.t_beta_r + cbcn;
        d_ic.d1_q -= gbc / t.t_beta_r + gbcn;
        d_ic.d2_q2 -= gbc2 / t.t_beta_r + gbcn2;
        d_ic.d3_q3 -= gbc3 / t.t_beta_r + gbcn3;

        // Base spread resistance: irb = f(vbe, vbc, vbb).
        let mut cb = 0.0;
        let d_ibb = if !(rbpr == 0.0 && rbpi == 0.0) {
            cb = cbe / t.t_beta_f + cben + cbc / t.t_beta_r + cbcn;
            if cb != 0.0 {
                if xjrb != 0.0 && rbpi != 0.0 {
                    // p = ib from here on (q, r unused).
                    let d_dummy = Dderivs {
                        value: f64::max(cb / xjrb, 1e-9),
                        d1_p: 1.0 / xjrb,
                        ..zero
                    };
                    let d_dummy = d_dummy.sqrt().times(2.4317);
                    let mut d_dummy2 = Dderivs {
                        value: 1.0 + 14.59025 * f64::max(cb / xjrb, 1e-9),
                        d1_p: 14.59025 / xjrb,
                        ..zero
                    }
                    .sqrt();
                    d_dummy2.value -= 1.0;
                    let d_z = d_dummy2.div(&d_dummy);
                    let d_tanz = d_z.tan();
                    let d_dummy = d_z.times(-1.0).plus(&d_tanz);
                    let d_dummy2 = d_tanz.mult(&d_tanz).mult(&d_z);
                    let mut d_rbb = d_dummy.div(&d_dummy2).times(3.0 * rbpi);
                    d_rbb.value += rbpr;
                    let d_vbb = d_rbb.mult(&d_p);
                    // Power-series inversion to the conductance
                    // derivatives (left at 0 when `d1_p` is 0, where
                    // the C only prints a warning).
                    let (mut gbb1, mut gbb2, mut gbb3) = (0.0, 0.0, 0.0);
                    if d_vbb.d1_p != 0.0 {
                        gbb1 = 1.0 / d_vbb.d1_p;
                        gbb2 = -(d_vbb.d2_p2 * 0.5) * gbb1 * gbb1;
                        gbb3 = gbb1
                            * gbb1
                            * gbb1
                            * gbb1
                            * (-(d_vbb.d3_p3 / 6.0)
                                + 2.0 * (d_vbb.d2_p2 * 0.5) * (d_vbb.d2_p2 * 0.5) * gbb1);
                    }
                    Dderivs {
                        value: cb,
                        d1_r: gbb1,
                        d2_r2: 2.0 * gbb2,
                        d3_r3: 6.0 * gbb3,
                        ..zero
                    }
                } else {
                    // rbb = rbpr + rbpi/qb; ibb = vbb/rbb.
                    let mut d_rbb = zero;
                    if rbpi != 0.0 {
                        d_rbb = d_qb.inv().times(rbpi);
                    }
                    d_rbb.value += rbpr;
                    let d_ibb = Dderivs { value: vbb, ..d_r };
                    d_ibb.div(&d_rbb)
                }
            } else {
                let mut d_ibb = d_r;
                if rbpr != 0.0 {
                    d_ibb.d1_r = 1.0 / rbpr;
                }
                d_ibb
            }
        } else {
            zero
        };

        // ib = cbe/BF + cben + cbc/BR + cbcn.
        let d_ib = Dderivs {
            value: cb,
            d1_p: gbe / t.t_beta_f + gben,
            d2_p2: gbe2 / t.t_beta_f + gben2,
            d3_p3: gbe3 / t.t_beta_f + gben3,
            d1_q: gbc / t.t_beta_r + gbcn,
            d2_q2: gbc2 / t.t_beta_r + gbcn2,
            d3_q3: gbc3 / t.t_beta_r + gbcn3,
            ..zero
        };

        // Charge storage.
        let tf = t.tf;
        let czbe = t.t_be_cap * area;
        let pe = t.t_be_pot;
        let xme = t.mje;
        let cdis = t.xcjc;
        let ctot = t.t_bc_cap * area;
        let czbc = ctot * cdis;
        let czbx = ctot - czbc;
        let pc = t.t_bc_pot;
        let xmc = t.mjc;
        let fcpe = t.t_dep_cap;
        let czcs = t.cjs * area;
        let ps = t.vjs;
        let xms = t.mjs;
        let xtf = t.xtf;
        let ovtf = t.tt_vbc_factor;
        let xjtf = t.itf * area;
        let mut d_qbe = if tf != 0.0 && vbe > 0.0 {
            let d_cbe = Dderivs {
                value: cbe,
                d1_p: gbe,
                d2_p2: gbe2,
                d3_p3: gbe3,
                ..zero
            };
            let d_tff = if xtf != 0.0 {
                let d_dummy = if ovtf != 0.0 {
                    Dderivs {
                        value: vbc * ovtf,
                        d1_q: ovtf,
                        ..zero
                    }
                    .exp()
                } else {
                    Dderivs::constant(1.0)
                };
                let d_dummy2 = if xjtf != 0.0 {
                    let mut den = d_cbe;
                    den.value += xjtf;
                    let r = d_cbe.div(&den);
                    r.mult(&r)
                } else {
                    Dderivs::constant(1.0)
                };
                let mut d_tff = d_dummy.mult(&d_dummy2).times(tf * xtf);
                d_tff.value += tf;
                d_tff
            } else {
                Dderivs::constant(tf)
            };
            // qbe = tff/qb·cbe.
            d_tff.div(&d_qb).mult(&d_cbe)
        } else {
            zero
        };
        let (lcapbe1, lcapbe2, lcapbe3);
        if vbe < fcpe {
            let arg = 1.0 - vbe / pe;
            let sarg = f64::exp(-xme * f64::ln(arg));
            lcapbe1 = czbe * sarg;
            lcapbe2 = 0.5 * czbe * xme * sarg / (arg * pe);
            lcapbe3 = czbe * xme * (xme + 1.0) * sarg / (arg * arg * pe * pe * 6.0);
        } else {
            let czbef2 = czbe / t.f2;
            lcapbe1 = czbef2 * (t.f3 + xme * vbe / pe);
            lcapbe2 = 0.5 * xme * czbef2 / pe;
            lcapbe3 = 0.0;
        }
        d_qbe.d1_p += lcapbe1;
        d_qbe.d2_p2 += lcapbe2 * 2.;
        d_qbe.d3_p3 += lcapbe3 * 6.;

        let fcpc = t.t_f4;
        let dep = |v: f64, cz: f64| {
            if v < fcpc {
                let arg = 1.0 - v / pc;
                let sarg = f64::exp(-xmc * f64::ln(arg));
                (
                    0.5 * cz * xmc * sarg / (arg * pc),
                    cz * xmc * (xmc + 1.0) * sarg / (arg * arg * pc * pc * 6.0),
                )
            } else {
                let czf2 = cz / t.f6;
                (0.5 * xmc * czf2 / pc, 0.0)
            }
        };
        let (capbc2, capbc3) = dep(vbc, czbc);
        let (capbx2, capbx3) = dep(vbx, czbx);
        let (capsc2, capsc3) = if vsc < 0.0 {
            let arg = 1.0 - vsc / ps;
            let sarg = f64::exp(-xms * f64::ln(arg));
            (
                0.5 * czcs * xms * sarg / (arg * ps),
                czcs * xms * (xms + 1.0) * sarg / (arg * arg * ps * ps * 6.0),
            )
        } else {
            (czcs * 0.5 * xms / ps, 0.0)
        };

        self.d = BjtDistoCoeffs {
            ic: taylor_coeffs(&d_ic, typ),
            ib: xy_only(taylor_coeffs(&d_ib, typ)),
            ibb: taylor_coeffs(&d_ibb, typ),
            qbe: xy_only(taylor_coeffs(&d_qbe, typ)),
            capbc2,
            capbc3,
            capbx2,
            capbx3,
            capsc2,
            capsc3,
        };
        Ok(())
    }

    /// `BJTdisto` (`dev/bjt/bjtdisto.c`), the load modes: x = vbe,
    /// y = vbc (both on the internal nodes), z = vbe delayed by the
    /// excess phase `td` for `ic`, z = vbb for `irb`, then the
    /// depletion charges of B-X (z + y, as the C reuses its `z`),
    /// B'-C' and S-C'. The C's conjugation slips are kept: in
    /// `D_F1MF2` the vbb and vsc kernels of H1(−f2) are *not*
    /// conjugated (`D_2F1MF2` conjugates them).
    fn disto_load(&self, ctx: &mut DistoCtx) {
        let d = &self.d;
        let t = &self.t;
        let mode = ctx.mode;
        let omega = ctx.omega;
        let td = t.excess_phase_factor;
        let (b, bp, cp, ep, s) = (self.base, self.base_p, self.col_p, self.emit_p, self.subst);
        let none = Kern::default();

        let kx = ctx.h.kern(mode, bp, ep);
        let ky = ctx.h.kern(mode, bp, cp);
        let mut kz = if td != 0.0 {
            delayed(mode, &kx, td, ctx.omega1, ctx.omega2)
        } else {
            kx
        };

        let ic = df_term(mode, &d.ic, &kx, &ky, &kz);
        ctx.stamp(cp, ep, ic);
        let ib = df_term(mode, &d.ib, &kx, &ky, &none);
        ctx.stamp(bp, ep, ib);

        if !(t.min_base_resist == 0.0 && t.base_resist == t.min_base_resist) {
            kz = ctx.h.kern(mode, b, bp);
            if mode == DistoMode::F1mF2 {
                kz.i1h2 = -kz.i1h2;
            }
            let ibb = df_term(mode, &d.ibb, &kx, &ky, &kz);
            ctx.stamp(b, bp, ibb);
        }

        let qbe = jomega(omega, df_term(mode, &d.qbe, &kx, &ky, &none));
        ctx.stamp(bp, ep, qbe);

        let kbx = kz.add(&ky);
        let qbx = jomega(omega, d1_term(mode, d.capbx2, d.capbx3, &kbx));
        ctx.stamp(b, cp, qbx);

        let qbc = jomega(omega, d1_term(mode, d.capbc2, d.capbc3, &ky));
        ctx.stamp(bp, cp, qbc);

        let mut ksc = ctx.h.kern(mode, s, cp);
        if mode == DistoMode::F1mF2 {
            ksc.i1h2 = -ksc.i1h2;
        }
        let qsc = jomega(omega, d1_term(mode, d.capsc2, d.capsc3, &ksc));
        ctx.stamp(s, cp, qsc);
    }
}
