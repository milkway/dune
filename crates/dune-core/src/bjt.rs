//! Bipolar junction transistor — the full 3f5 Gummel–Poon model
//! (`dev/bjt/bjtload.c`, `bjttemp.c`, `bjtsetup.c`, `bjtacld.c`,
//! `bjttrunc.c`). One junction pair with leakage diodes, base-width
//! modulation (Early), high-injection roll-off (`qb`), bias-dependent
//! base resistance (`irb`), four charge storages (B-E, B-C internal
//! and external split by `xcjc`, C-S), and Weil's excess-phase
//! approximation in transient.

use crate::consts::{BOLTZ, CHARGE, KOVERQ, REFTEMP, ROOT2};
use crate::diode::pnjlim;
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
        }
    }
}

impl crate::devices::Device for BjtI {
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

        // DC currents and conductances.
        let vtn = vt * t.nf;
        let (cbe, gbe, cben, gben);
        if vbe > -5.0 * vtn {
            let evbe = f64::exp(vbe / vtn);
            cbe = csat * (evbe - 1.0) + ctx.gmin * vbe;
            gbe = csat * evbe / vtn + ctx.gmin;
            if c2 == 0.0 {
                cben = 0.0;
                gben = 0.0;
            } else {
                let evben = f64::exp(vbe / vte);
                cben = c2 * (evben - 1.0);
                gben = c2 * evben / vte;
            }
        } else {
            gbe = -csat / vbe + ctx.gmin;
            cbe = gbe * vbe;
            gben = -c2 / vbe;
            cben = gben * vbe;
        }
        let vtn = vt * t.nr;
        let (cbc, gbc, cbcn, gbcn);
        if vbc > -5.0 * vtn {
            let evbc = f64::exp(vbc / vtn);
            cbc = csat * (evbc - 1.0) + ctx.gmin * vbc;
            gbc = csat * evbc / vtn + ctx.gmin;
            if c4 == 0.0 {
                cbcn = 0.0;
                gbcn = 0.0;
            } else {
                let evbcn = f64::exp(vbc / vtc);
                cbcn = c4 * (evbcn - 1.0);
                gbcn = c4 * evbcn / vtc;
            }
        } else {
            gbc = -csat / vbc + ctx.gmin;
            cbc = gbc * vbc;
            gbcn = -c4 / vbc;
            cbcn = gbcn * vbc;
        }

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
}
