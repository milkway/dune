//! `CPL` (ngspice-47 `devices/cpl/`): the coupled multiconductor
//! lines of KSPICE, `PXXX N1 … Nk GND1 M1 … Mk GND2 MNAME
//! <LENGTH VALUE>` with `.model MNAME cpl R= L= G= C= LENGTH=`, the
//! matrices given as their upper triangles row by row.
//!
//! Translated from `cplsetup.c` (`CPLsetup`, `ReadCpL`; the numerical
//! setup is in `cplsetup.rs`) and `cplload.c` (`CPLload`). "Copyright
//! 1992 Regents of the University of California. All rights reserved.
//! Author: 1992 Charles Hough." The C has no AC, pole-zero, noise or
//! distortion load; Dune refuses those analyses for a CPL.

// A statement-by-statement port: the indexed loops mirror the C's.
#![allow(clippy::needless_range_loop)]

use std::collections::VecDeque;

use super::cplsetup::{Fit, coupled};
use super::{SetBuf, Term, exp_c, mult_c, ps};
use crate::devices::Device;
use crate::matrix::{AcceptCtx, LoadCtx, MatrixBuilder, Slot, Var};

/// Most coupled conductors (`MAX_CP_TX_LINES`, `swec.h`: every
/// per-line array of the C has this size).
pub const MAX_CPL_LINES: usize = 8;

/// `.model MNAME cpl (...)` (`CPLmPTable`): the per-unit-length
/// matrices as their upper triangles, row by row (`R11 R12 … R1k R22
/// … Rkk`), and the default length. `CPLsetup` refuses a model
/// without any of the five.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CplModel {
    /// `R`: series resistance (Ω/m).
    pub r: Option<Vec<f64>>,
    /// `L`: series inductance (H/m).
    pub l: Option<Vec<f64>>,
    /// `G`: shunt conductance (S/m).
    pub g: Option<Vec<f64>>,
    /// `C`: shunt capacitance (F/m).
    pub c: Option<Vec<f64>>,
    /// `LENGTH`: the length the instance `LENGTH` overrides.
    pub length: Option<f64>,
}

/// `TMS` of `swec.h`: one Padé fit as three partial-fraction terms
/// (`if_img`: terms 1 and 2 hold the real and imaginary parts of a
/// conjugate pair).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Tms {
    pub if_img: bool,
    pub aten: f64,
    pub tm: [Term; 3],
}

impl Tms {
    /// `ReadCpL`'s conversion of a `generate_out` fit, `None` when its
    /// constant term is zero (the C leaves the pointer NULL). Returns
    /// the fit and its `h*C` sum.
    fn from_fit(f: &Fit) -> Option<(Tms, f64)> {
        if f.c0 == 0.0 {
            return None;
        }
        let d = f.c0;
        let mut t = Tms {
            aten: d,
            // `(int)(Poly[6] - 1.0)`: 2 → complex pair, 1 → real.
            if_img: (f.poly[6] - 1.0) as i32 != 0,
            ..Tms::default()
        };
        for k in 0..3 {
            t.tm[k].c = f.poly[k] * d;
            t.tm[k].x = f.poly[3 + k];
        }
        let sum = if t.if_img {
            t.tm[0].c + 2.0 * t.tm[1].c
        } else {
            let mut s = 0.0;
            for k in 0..3 {
                s += t.tm[k].c;
            }
            s
        };
        Some((t, sum))
    }
}

/// `CPLine` of `swec.h` for `n` conductors; the instance keeps two
/// (committed and trial). Fits absent in the setup (`NULL` in the C)
/// are `None`.
#[derive(Clone, Debug)]
pub(crate) struct CpLine {
    pub ext: bool,
    pub ratio: Vec<f64>,
    /// Modal delays, picoseconds.
    pub taul: Vec<f64>,
    pub h1t: Vec<Option<Tms>>,
    pub h2t: Vec<Option<Tms>>,
    pub h3t: Vec<Option<Tms>>,
    pub h1c: Vec<f64>,
    pub h2c: Vec<f64>,
    pub h3c: Vec<f64>,
    pub h1e: Vec<[f64; 3]>,
    pub head: u64,
}

/// `copy_cp(new, old)`: every coefficient and state, then the head
/// catch-up (absent fits of `new` are created from `old`'s, as the C
/// allocates them; `old` has them all by then).
fn copy_cp(new: &mut CpLine, old: &CpLine) {
    new.ext = old.ext;
    new.ratio.clone_from(&old.ratio);
    new.taul.clone_from(&old.taul);
    new.h1t.clone_from(&old.h1t);
    new.h2t.clone_from(&old.h2t);
    new.h3t.clone_from(&old.h3t);
    new.h1c.clone_from(&old.h1c);
    new.h2c.clone_from(&old.h2c);
    new.h3c.clone_from(&old.h3c);
    new.h1e.clone_from(&old.h1e);
    if new.head < old.head {
        new.head = old.head;
    }
}

/// The setup result of one `P` instance (`CPLsetup` + `ReadCpL`).
#[derive(Clone, Debug)]
pub(crate) struct CplParams {
    pub n: usize,
    pub line: CpLine,
    /// `Rm[(m,m)]·length` per conductor (the clamped model values), the
    /// DC series resistances.
    pub g_dc: Vec<f64>,
}

impl CplParams {
    /// `CPLsetup` + `ReadCpL` for an instance of `model` with `n`
    /// conductors and the instance length `len`.
    pub fn new(
        name: &str,
        model: &CplModel,
        n: usize,
        len: Option<f64>,
    ) -> Result<CplParams, String> {
        if !(1..=MAX_CPL_LINES).contains(&n) {
            return Err(format!(
                "`{name}`: a CPL line has 1 to {MAX_CPL_LINES} conductors, got {n}"
            ));
        }
        let need = |v: &Option<Vec<f64>>, what: &str| {
            v.clone()
                .ok_or_else(|| format!("`{name}`: lossy line {what}"))
        };
        let mut rm = need(&model.r, "series resistance not given")?;
        let gm = need(&model.g, "parallel conductance not given")?;
        let lm = need(&model.l, "series inductance not given")?;
        let cm = need(&model.c, "parallel capacitance not given")?;
        let mlen = model
            .length
            .ok_or_else(|| format!("`{name}`: lossy line length must be given"))?;
        let length = len.unwrap_or(mlen);
        let tri = n * (n + 1) / 2;
        for (v, what) in [(&rm, "R"), (&gm, "G"), (&lm, "L"), (&cm, "C")] {
            if v.len() < tri {
                return Err(format!(
                    "`{name}`: CPL model {what} has {} values, {n} conductors need the {tri} \
                     of the upper triangle",
                    v.len()
                ));
            }
            if v.iter().take(tri).any(|x| !x.is_finite()) {
                return Err(format!("`{name}`: CPL model {what} must be finite"));
            }
        }
        if !length.is_finite() {
            return Err(format!("`{name}`: CPL length must be finite"));
        }
        // `ReadCpL`: mirror the triangles; `Rm` is clamped to 1e-4 in
        // the model itself (every entry, off-diagonal included).
        let mut r_m = vec![vec![0.0; n]; n];
        let mut g_m = r_m.clone();
        let mut l_m = r_m.clone();
        let mut c_m = r_m.clone();
        let mut counter = 0;
        for i in 0..n {
            for j in 0..n {
                if i > j {
                    r_m[i][j] = r_m[j][i];
                    g_m[i][j] = g_m[j][i];
                    c_m[i][j] = c_m[j][i];
                    l_m[i][j] = l_m[j][i];
                } else {
                    let f = rm[counter];
                    rm[counter] = if f > 1.0e-4 { f } else { 1.0e-4 };
                    r_m[i][j] = rm[counter];
                    g_m[i][j] = gm[counter];
                    l_m[i][j] = lm[counter];
                    c_m[i][j] = cm[counter];
                    counter += 1;
                }
            }
        }
        let co = coupled(r_m, g_m, l_m, c_m, length).map_err(|e| format!("`{name}`: {e}"))?;
        let mut line = CpLine {
            ext: false,
            ratio: vec![0.0; n],
            taul: vec![0.0; n],
            h1t: vec![None; n * n],
            h2t: vec![None; n * n * n],
            h3t: vec![None; n * n * n],
            h1c: vec![0.0; n * n],
            h2c: vec![0.0; n * n * n],
            h3c: vec![0.0; n * n * n],
            h1e: vec![[0.0; 3]; n * n],
            head: 0,
        };
        for i in 0..n {
            line.taul[i] = co.tau[i] * 1.0e+12;
            for j in 0..n {
                if let Some((t, s)) = Tms::from_fit(&co.siv[i][j]) {
                    line.h1t[i * n + j] = Some(t);
                    line.h1c[i * n + j] = s;
                }
                for k in 0..n {
                    let at = (i * n + j) * n + k;
                    if let Some((t, s)) = Tms::from_fit(&co.iwi[i][j][k]) {
                        line.h2t[at] = Some(t);
                        line.h2c[at] = s;
                    }
                    if let Some((t, s)) = Tms::from_fit(&co.iwv[i][j][k]) {
                        line.h3t[at] = Some(t);
                        line.h3c[at] = s;
                    }
                }
            }
        }
        let finite = line.taul.iter().all(|t| t.is_finite())
            && line
                .h1t
                .iter()
                .chain(&line.h2t)
                .chain(&line.h3t)
                .flatten()
                .all(|t| {
                    t.aten.is_finite() && t.tm.iter().all(|m| m.c.is_finite() && m.x.is_finite())
                });
        if !finite {
            return Err(format!(
                "`{name}`: the CPL Padé fit of this model is not finite"
            ));
        }
        if line.taul.iter().any(|t| t.is_nan() || *t <= 0.0) {
            return Err(format!("`{name}`: a CPL modal delay is not positive"));
        }
        // `resindex` walks the diagonal of the upper triangle.
        let mut g_dc = Vec::with_capacity(n);
        let mut resindex = 0;
        for m in 0..n {
            g_dc.push(rm[resindex] * length);
            resindex += n - m;
        }
        Ok(CplParams { n, line, g_dc })
    }
}

/// One accepted sample (`VI_list`).
#[derive(Clone, Debug, Default)]
struct Vi {
    time: i32,
    v_i: Vec<f64>,
    v_o: Vec<f64>,
    i_i: Vec<f64>,
    i_o: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
struct History {
    q: VecDeque<Vi>,
    base: u64,
}

impl History {
    fn get(&self, k: u64) -> Option<&Vi> {
        k.checked_sub(self.base)
            .and_then(|o| usize::try_from(o).ok())
            .and_then(|o| self.q.get(o))
    }

    fn prune(&mut self, k: u64) {
        while self.base < k && !self.q.is_empty() {
            self.q.pop_front();
            self.base += 1;
        }
    }
}

/// `get_pvs_vi`'s `errordetect` (and the NULL dereferences next to
/// it): the C prints this and exits.
const HISTORY_FAULT: &str = "CPL: your maximum time step is too large for tau \
                             (decrease the max time step in the .tran card)";

/// `CPLload`'s dc-setup check: the C prints this and exits.
const FORBIDDEN: &str = "CPL: forbidden combination of model parameters (a coupling term \
                         of the line fit vanishes)";

/// Delayed port values, `[mode][conductor]` (`v1_i … i2_o`).
struct Pvs {
    v1_i: Vec<f64>,
    v2_i: Vec<f64>,
    i1_i: Vec<f64>,
    i2_i: Vec<f64>,
    v1_o: Vec<f64>,
    v2_o: Vec<f64>,
    i1_o: Vec<f64>,
    i2_o: Vec<f64>,
}

/// What a provisional sample changed (see `TxlI::provisional`).
struct CplSnap {
    cp: CpLine,
    len: u64,
    nodes: [Vec<f64>; 4],
}

/// A `P` instance (`CPLinstance` with its two `CPLine`s).
pub(crate) struct CplI {
    n: usize,
    pos: Vec<Var>,
    neg: Vec<Var>,
    ibr1: Vec<Var>,
    ibr2: Vec<Var>,
    proto: CpLine,
    cp: CpLine,
    cp2: CpLine,
    g_dc: Vec<f64>,
    hist: History,
    dc_given: bool,
    dc1: Vec<f64>,
    dc2: Vec<f64>,
    /// The SWEC nodes (`in_node[k]->V/dv`, `out_node[k]->V/dv`).
    in_v: Vec<f64>,
    in_dv: Vec<f64>,
    out_v: Vec<f64>,
    out_dv: Vec<f64>,
    max_step: Option<f64>,
    /// A fault the C exits on (see [`Device::fatal`]).
    fatal: Option<&'static str>,
    /// Accepted since the last sample (see `TxlI::accepted`).
    accepted: bool,
    /// The state before a TR-BDF2 stage sample (see
    /// `TxlI::provisional`).
    provisional: Option<Box<CplSnap>>,
    s: CplSlots,
    buf: SetBuf,
}

/// The `TSTALLOC` pointers of `CPLsetup`: per conductor, then per
/// conductor pair (`[m·n + p]`).
#[derive(Clone, Default)]
struct CplSlots {
    ibr1_ibr1: Vec<Slot>,
    ibr2_ibr2: Vec<Slot>,
    pos_ibr1: Vec<Slot>,
    neg_ibr2: Vec<Slot>,
    pos_pos: Vec<Slot>,
    neg_neg: Vec<Slot>,
    neg_pos: Vec<Slot>,
    pos_neg: Vec<Slot>,
    ibr1_pos: Vec<Slot>,
    ibr2_neg: Vec<Slot>,
    ibr1_neg: Vec<Slot>,
    ibr2_pos: Vec<Slot>,
    ibr1_ibr2: Vec<Slot>,
    ibr2_ibr1: Vec<Slot>,
}

impl CplI {
    pub fn new(p: CplParams, pos: Vec<Var>, neg: Vec<Var>, ibr1: Vec<Var>, ibr2: Vec<Var>) -> Self {
        let n = p.n;
        CplI {
            n,
            pos,
            neg,
            ibr1,
            ibr2,
            cp: p.line.clone(),
            cp2: p.line.clone(),
            proto: p.line,
            g_dc: p.g_dc,
            hist: History::default(),
            dc_given: false,
            dc1: vec![0.0; n],
            dc2: vec![0.0; n],
            in_v: vec![0.0; n],
            in_dv: vec![0.0; n],
            out_v: vec![0.0; n],
            out_dv: vec![0.0; n],
            max_step: None,
            fatal: None,
            accepted: false,
            provisional: None,
            s: CplSlots::default(),
            buf: SetBuf::default(),
        }
    }

    /// Back to the state after `CPLsetup` (see `TxlI::restart`).
    fn restart(&mut self) {
        self.cp = self.proto.clone();
        self.cp2 = self.proto.clone();
        self.hist = History::default();
        self.dc_given = false;
        for v in [
            &mut self.dc1,
            &mut self.dc2,
            &mut self.in_v,
            &mut self.in_dv,
            &mut self.out_v,
            &mut self.out_dv,
        ] {
            v.iter_mut().for_each(|x| *x = 0.0);
        }
        self.max_step = None;
        self.fatal = None;
        self.accepted = false;
        self.provisional = None;
    }

    /// `add_new_vi`.
    fn add_new_vi(&mut self, time: i32, sol: &[f64]) {
        let pick = |v: &[Var]| v.iter().map(|&k| sol[k]).collect::<Vec<f64>>();
        let vi = Vi {
            time,
            v_i: pick(&self.pos),
            v_o: pick(&self.neg),
            i_i: pick(&self.ibr1),
            i_o: pick(&self.ibr2),
        };
        self.hist.q.push_back(vi);
    }

    /// The dc setup of `CPLload`.
    fn dc_setup(&mut self, sol: &[f64]) -> Result<(), &'static str> {
        let n = self.n;
        let cp = &mut self.cp;
        if cp
            .h1t
            .iter()
            .chain(&cp.h2t)
            .chain(&cp.h3t)
            .any(Option::is_none)
        {
            return Err(FORBIDDEN);
        }
        for i in 0..n {
            self.dc1[i] = sol[self.pos[i]];
            self.in_v[i] = self.dc1[i];
            self.dc2[i] = sol[self.neg[i]];
            self.out_v[i] = self.dc2[i];
        }
        self.dc_given = true;
        let (dc1, dc2) = (&self.dc1, &self.dc2);
        let init = |tms: &mut Tms, j: usize| {
            if tms.if_img {
                let t0 = tms.tm[0];
                tms.tm[0].cnv_i = -dc1[j] * t0.c / t0.x;
                tms.tm[0].cnv_o = -dc2[j] * t0.c / t0.x;
                let (a, b) = div_c(tms.tm[1].c, tms.tm[2].c, tms.tm[1].x, tms.tm[2].x);
                tms.tm[1].cnv_i = -dc1[j] * a;
                tms.tm[1].cnv_o = -dc2[j] * a;
                tms.tm[2].cnv_i = -dc1[j] * b;
                tms.tm[2].cnv_o = -dc2[j] * b;
            } else {
                for k in 0..3 {
                    let t = tms.tm[k];
                    tms.tm[k].cnv_i = -dc1[j] * t.c / t.x;
                    tms.tm[k].cnv_o = -dc2[j] * t.c / t.x;
                }
            }
        };
        for i in 0..n {
            for j in 0..n {
                if let Some(t) = cp.h1t[i * n + j].as_mut() {
                    init(t, j);
                }
                for l in 0..n {
                    if let Some(t) = cp.h2t[(i * n + j) * n + l].as_mut() {
                        for k in 0..3 {
                            t.tm[k].cnv_i = 0.0;
                            t.tm[k].cnv_o = 0.0;
                        }
                    }
                }
                for l in 0..n {
                    if let Some(t) = cp.h3t[(i * n + j) * n + l].as_mut() {
                        init(t, j);
                    }
                }
            }
        }
        let vi = Vi {
            time: 0,
            v_i: self.dc1.clone(),
            v_o: self.dc2.clone(),
            i_i: vec![0.0; n],
            i_o: vec![0.0; n],
        };
        self.hist = History::default();
        self.hist.q.push_back(vi);
        self.cp.head = 0;
        self.cp2.head = 0;
        Ok(())
    }

    /// The first instance loop of `CPLload` (see `TxlI::advance`).
    fn advance(&mut self, mut time: i32, sol: &[f64]) -> i32 {
        if let Some(tail) = self.hist.q.back()
            && time < tail.time
            && let Some(snap) = self.provisional.take()
        {
            self.rollback(*snap);
        }
        let Some(before) = self.hist.q.back().cloned() else {
            return time;
        };
        if before.time > time {
            time = before.time;
        }
        if time > before.time {
            if self.accepted {
                self.provisional = None;
            } else {
                self.provisional = Some(Box::new(CplSnap {
                    cp: self.cp.clone(),
                    len: self.hist.base + self.hist.q.len() as u64,
                    nodes: [
                        self.in_v.clone(),
                        self.in_dv.clone(),
                        self.out_v.clone(),
                        self.out_dv.clone(),
                    ],
                }));
            }
            self.accepted = false;
            copy_cp(&mut self.cp, &self.cp2);
            if self.provisional.is_none() {
                self.hist.prune(self.cp.head);
            }
            self.add_new_vi(time, sol);
            let delta = (time - before.time) as f64;
            let tail = self.hist.q.back().cloned().expect("just pushed");
            for m in 0..self.n {
                let v = before.v_i[m];
                self.in_v[m] = tail.v_i[m];
                self.in_dv[m] = (self.in_v[m] - v) / delta;
            }
            for m in 0..self.n {
                let v = before.v_o[m];
                self.out_v[m] = tail.v_o[m];
                self.out_dv[m] = (self.out_v[m] - v) / delta;
            }
            self.update_cnv(delta);
            if self.cp.ext {
                self.update_delayed_cnv(&tail, delta);
            }
        }
        time
    }

    /// Undo a provisional sample (see `TxlI::rollback`).
    fn rollback(&mut self, snap: CplSnap) {
        self.cp = snap.cp;
        while self.hist.base + self.hist.q.len() as u64 > snap.len {
            self.hist.q.pop_back();
        }
        self.cp2.head = self.cp.head;
        [self.in_v, self.in_dv, self.out_v, self.out_dv] = snap.nodes;
        // The sample before it came after the last accept.
        self.accepted = false;
    }

    /// `update_cnv`: advance the `h1` convolutions over the accepted
    /// step `h` (picoseconds). As in the TXL, the slopes are scaled by
    /// each real term's `c/x` cumulatively (docs/kspice-lines.md).
    fn update_cnv(&mut self, h: f64) {
        let n = self.n;
        for j in 0..n {
            for k in 0..n {
                let ai = self.in_v[k];
                let ao = self.out_v[k];
                let mut bi = self.in_dv[k];
                let mut bo = self.out_dv[k];
                let h1e = self.cp.h1e[j * n + k];
                let Some(tms) = self.cp.h1t[j * n + k].as_mut() else {
                    continue;
                };
                if tms.if_img {
                    let e = h1e[0];
                    let t = tms.tm[0].c / tms.tm[0].x;
                    update_cnv_a(tms, h, ai, ao, ai - bi * h, ao - bo * h, h1e[1], h1e[2]);
                    bi *= t;
                    bo *= t;
                    let tm = &mut tms.tm[0];
                    tm.cnv_i = (tm.cnv_i - bi * h) * e + (e - 1.0) * (ai * t + 1.0e+12 * bi / tm.x);
                    tm.cnv_o = (tm.cnv_o - bo * h) * e + (e - 1.0) * (ao * t + 1.0e+12 * bo / tm.x);
                } else {
                    for i in 0..3 {
                        let tm = &mut tms.tm[i];
                        let e = h1e[i];
                        let t = tm.c / tm.x;
                        bi *= t;
                        bo *= t;
                        tm.cnv_i =
                            (tm.cnv_i - bi * h) * e + (e - 1.0) * (ai * t + 1.0e+12 * bi / tm.x);
                        tm.cnv_o =
                            (tm.cnv_o - bo * h) * e + (e - 1.0) * (ao * t + 1.0e+12 * bo / tm.x);
                    }
                }
            }
        }
    }

    /// `update_delayed_cnv`.
    fn update_delayed_cnv(&mut self, vi: &Vi, h: f64) {
        let n = self.n;
        let h = h * 0.5e-12;
        for k in 0..n {
            let r = self.cp.ratio[k];
            if r <= 0.0 {
                continue;
            }
            for i in 0..n {
                for j in 0..n {
                    let at = (i * n + j) * n + k;
                    let Some(tms) = self.cp.h3t[at].as_mut() else {
                        continue;
                    };
                    let f = h * r * vi.v_i[j];
                    for t in &mut tms.tm {
                        t.cnv_i += f * t.c;
                    }
                    let f = h * r * vi.v_o[j];
                    for t in &mut tms.tm {
                        t.cnv_o += f * t.c;
                    }
                    // The C does not check this one for NULL (every fit
                    // exists past the dc setup).
                    if let Some(tms) = self.cp.h2t[at].as_mut() {
                        let f = h * r * vi.i_i[j];
                        for t in &mut tms.tm {
                            t.cnv_i += f * t.c;
                        }
                        let f = h * r * vi.i_o[j];
                        for t in &mut tms.tm {
                            t.cnv_o += f * t.c;
                        }
                    }
                }
            }
        }
    }

    /// The second instance loop of `CPLload` outside the DC modes.
    fn companion(
        &mut self,
        time: i32,
        time2: i32,
        h: f64,
        sol: &[f64],
    ) -> Result<(), &'static str> {
        let n = self.n;
        let h1 = 0.5 * h;
        if !self.dc_given {
            self.dc_setup(sol)?;
        }
        self.buf.clear();
        for m in 0..n {
            self.buf.set(self.s.ibr1_ibr1[m], -1.0);
            self.buf.set(self.s.ibr2_ibr2[m], -1.0);
        }
        for m in 0..n {
            self.buf.set(self.s.pos_ibr1[m], 1.0);
            self.buf.set(self.s.neg_ibr2[m], 1.0);
        }
        for m in 0..n {
            for p in 0..n {
                let t = self.cp.h1t[m * n + p].as_ref().ok_or(FORBIDDEN)?;
                let v = t.aten + h1 * self.cp.h1c[m * n + p];
                self.buf.set(self.s.ibr1_pos[m * n + p], v);
                self.buf.set(self.s.ibr2_neg[m * n + p], v);
            }
        }
        copy_cp(&mut self.cp2, &self.cp);
        let (ff, gg, ratio) = self.right_consts(time, time2, h, h1)?;
        for i in 0..n {
            self.buf.set_rhs(self.ibr1[i], ff[i]);
            self.buf.set_rhs(self.ibr2[i], gg[i]);
        }
        if let Some(ratio) = ratio {
            self.cp2.ext = true;
            for q in 0..n {
                self.cp.ratio[q] = ratio[q];
                if ratio[q] > 0.0 {
                    for m in 0..n {
                        for p in 0..n {
                            let at = (m * n + p) * n + q;
                            if let Some(t) = &self.cp.h3t[at] {
                                let f = ratio[q] * (h1 * self.cp.h3c[at] + t.aten);
                                self.buf.set(self.s.ibr1_neg[m * n + p], -f);
                                self.buf.set(self.s.ibr2_pos[m * n + p], -f);
                            }
                            if let Some(t) = &self.cp.h2t[at] {
                                let f = ratio[q] * (h1 * self.cp.h2c[at] + t.aten);
                                self.buf.set(self.s.ibr1_ibr2[m * n + p], -f);
                                self.buf.set(self.s.ibr2_ibr1[m * n + p], -f);
                            }
                        }
                    }
                }
            }
        } else {
            self.cp.ext = false;
        }
        Ok(())
    }

    /// `get_pvs_vi` on the trial line: the port values one modal delay
    /// back at `t1` and `t2`; `Some(ratio)` is `ext` with `ratio[]`.
    #[allow(clippy::type_complexity)]
    fn get_pvs(&mut self, t1: i32, t2: i32) -> Result<(Pvs, Option<Vec<f64>>), &'static str> {
        let n = self.n;
        let cp = &mut self.cp2;
        let hist = &self.hist;
        let z = || vec![0.0; n * n];
        let mut p = Pvs {
            v1_i: z(),
            v2_i: z(),
            i1_i: z(),
            i2_i: z(),
            v1_o: z(),
            v2_o: z(),
            i1_o: z(),
            i2_o: z(),
        };
        let mut ratio = vec![0.0; n];
        let mut ext = false;
        let mut ta = vec![0.0; n];
        let mut tb = vec![0.0; n];
        let mut mini: isize = -1;
        let mut minta = 123456789.0;
        for i in 0..n {
            ta[i] = t1 as f64 - cp.taul[i];
            tb[i] = t2 as f64 - cp.taul[i];
            if ta[i] < minta {
                minta = ta[i];
                mini = i as isize;
            }
        }
        let at = |k: u64| hist.get(k);
        for i in 0..n {
            ratio[i] = 0.0;
            if tb[i] <= 0.0 {
                for j in 0..n {
                    let k = i * n + j;
                    p.i1_i[k] = 0.0;
                    p.i2_i[k] = 0.0;
                    p.i1_o[k] = 0.0;
                    p.i2_o[k] = 0.0;
                    p.v1_i[k] = self.dc1[j];
                    p.v2_i[k] = self.dc1[j];
                    p.v1_o[k] = self.dc2[j];
                    p.v2_o[k] = self.dc2[j];
                }
                continue;
            }
            let mut vi1k = cp.head;
            let mut vi1 = at(vi1k).ok_or(HISTORY_FAULT)?;
            let mut vik = vi1k + 1;
            if ta[i] <= 0.0 {
                for j in 0..n {
                    let k = i * n + j;
                    p.i1_i[k] = 0.0;
                    p.i1_o[k] = 0.0;
                    p.v1_i[k] = self.dc1[j];
                    p.v1_o[k] = self.dc2[j];
                }
            } else {
                let mut vi = at(vik).ok_or(HISTORY_FAULT)?;
                while (vi.time as f64) < ta[i] {
                    vi1 = vi;
                    vi1k = vik;
                    vik += 1;
                    vi = at(vik).ok_or(HISTORY_FAULT)?;
                }
                let f = (ta[i] - vi1.time as f64) / (vi.time - vi1.time) as f64;
                for j in 0..n {
                    let k = i * n + j;
                    p.v1_i[k] = vi1.v_i[j] + f * (vi.v_i[j] - vi1.v_i[j]);
                    p.v1_o[k] = vi1.v_o[j] + f * (vi.v_o[j] - vi1.v_o[j]);
                    p.i1_i[k] = vi1.i_i[j] + f * (vi.i_i[j] - vi1.i_i[j]);
                    p.i1_o[k] = vi1.i_o[j] + f * (vi.i_o[j] - vi1.i_o[j]);
                }
                if i as isize == mini {
                    cp.head = vi1k;
                }
            }
            if tb[i] > t1 as f64 {
                ext = true;
                let r = (tb[i] - t1 as f64) / (t2 - t1) as f64;
                ratio[i] = r;
                // The last sample (`for (; vi->next; vi = vi->next)`), or
                // `vi1` when it is the only one.
                let vi = match at(vik) {
                    Some(_) => hist.q.back().ok_or(HISTORY_FAULT)?,
                    None => vi1,
                };
                let f = 1.0 - r;
                for j in 0..n {
                    let k = i * n + j;
                    p.v2_i[k] = vi.v_i[j] * f;
                    p.v2_o[k] = vi.v_o[j] * f;
                    p.i2_i[k] = vi.i_i[j] * f;
                    p.i2_o[k] = vi.i_o[j] * f;
                }
            } else {
                let mut vi = at(vik).ok_or(HISTORY_FAULT)?;
                while (vi.time as f64) < tb[i] {
                    vi1 = vi;
                    vik += 1;
                    vi = at(vik).ok_or(HISTORY_FAULT)?;
                }
                let f = (tb[i] - vi1.time as f64) / (vi.time - vi1.time) as f64;
                for j in 0..n {
                    let k = i * n + j;
                    p.v2_i[k] = vi1.v_i[j] + f * (vi.v_i[j] - vi1.v_i[j]);
                    p.v2_o[k] = vi1.v_o[j] + f * (vi.v_o[j] - vi1.v_o[j]);
                    p.i2_i[k] = vi1.i_i[j] + f * (vi.i_i[j] - vi1.i_i[j]);
                    p.i2_o[k] = vi1.i_o[j] + f * (vi.i_o[j] - vi1.i_o[j]);
                }
            }
        }
        Ok((p, ext.then_some(ratio)))
    }

    /// `right_consts` on the trial line.
    #[allow(clippy::type_complexity)]
    fn right_consts(
        &mut self,
        t: i32,
        time: i32,
        h: f64,
        h1: f64,
    ) -> Result<(Vec<f64>, Vec<f64>, Option<Vec<f64>>), &'static str> {
        let n = self.n;
        let mut ff = vec![0.0; n];
        let mut gg = vec![0.0; n];
        {
            let cp = &mut self.cp2;
            for j in 0..n {
                ff[j] = 0.0;
                gg[j] = 0.0;
                for k in 0..n {
                    let Some(tms) = cp.h1t[j * n + k].as_ref() else {
                        continue;
                    };
                    if tms.if_img {
                        let e = (tms.tm[0].x * h).exp();
                        let (er, ei) = exp_c(tms.tm[1].x, tms.tm[2].x, h);
                        cp.h1e[j * n + k] = [e, er, ei];
                        let ff1 = tms.tm[0].c * e * h1;
                        ff[j] -= tms.tm[0].cnv_i * e;
                        gg[j] -= tms.tm[0].cnv_o * e;
                        ff[j] -= ff1 * self.in_v[k];
                        gg[j] -= ff1 * self.out_v[k];
                        let (a1, _b1) = mult_c(tms.tm[1].c, tms.tm[2].c, er, ei);
                        let (a, _b) = mult_c(tms.tm[1].cnv_i, tms.tm[2].cnv_i, er, ei);
                        ff[j] -= 2.0 * (a1 * h1 * self.in_v[k] + a);
                        let (a, _b) = mult_c(tms.tm[1].cnv_o, tms.tm[2].cnv_o, er, ei);
                        gg[j] -= 2.0 * (a1 * h1 * self.out_v[k] + a);
                    } else {
                        let mut ff1 = 0.0;
                        for i in 0..3 {
                            let e = (tms.tm[i].x * h).exp();
                            cp.h1e[j * n + k][i] = e;
                            ff1 -= tms.tm[i].c * e;
                            ff[j] -= tms.tm[i].cnv_i * e;
                            gg[j] -= tms.tm[i].cnv_o * e;
                        }
                        ff[j] += ff1 * h1 * self.in_v[k];
                        gg[j] += ff1 * h1 * self.out_v[k];
                    }
                }
            }
        }
        let (p, ratio) = self.get_pvs(t, time)?;
        let cp = &mut self.cp2;
        for j in 0..n {
            // Node voltages, per mode.
            for k in 0..n {
                for l in 0..n {
                    let lk = l * n + k;
                    let Some(tms) = cp.h3t[(j * n + k) * n + l].as_mut() else {
                        continue;
                    };
                    let (df, dg) = advance_tms(
                        tms,
                        h,
                        h1,
                        (p.v1_i[lk], p.v2_i[lk]),
                        (p.v1_o[lk], p.v2_o[lk]),
                    );
                    ff[j] += df;
                    gg[j] += dg;
                }
            }
            // Line currents, per mode.
            for k in 0..n {
                for l in 0..n {
                    let lk = l * n + k;
                    let Some(tms) = cp.h2t[(j * n + k) * n + l].as_mut() else {
                        continue;
                    };
                    let (df, dg) = advance_tms(
                        tms,
                        h,
                        h1,
                        (p.i1_i[lk], p.i2_i[lk]),
                        (p.i1_o[lk], p.i2_o[lk]),
                    );
                    ff[j] += df;
                    gg[j] += dg;
                }
            }
        }
        Ok((ff, gg, ratio))
    }

    fn dc_stamps(&self, ctx: &mut LoadCtx) {
        let n = self.n;
        for m in 0..n {
            ctx.add(self.s.pos_ibr1[m], 1.0);
            ctx.add(self.s.neg_ibr2[m], 1.0);
            ctx.add(self.s.ibr1_ibr1[m], 1.0);
            ctx.add(self.s.ibr1_ibr2[m * n + m], 1.0);
            ctx.add(self.s.ibr2_pos[m * n + m], 1.0);
            ctx.add(self.s.ibr2_neg[m * n + m], -1.0);
            ctx.add(self.s.ibr2_ibr1[m * n + m], -self.g_dc[m]);
        }
    }
}

/// One fit of `right_consts`' delayed-source loops: advance its
/// convolutions with the delayed values `(x1, x2)` at both ends and
/// return the contributions to `ff` (output end) and `gg` (input
/// end).
fn advance_tms(tms: &mut Tms, h: f64, h1: f64, xi: (f64, f64), xo: (f64, f64)) -> (f64, f64) {
    let (x1_i, x2_i) = xi;
    let (x1_o, x2_o) = xo;
    if tms.if_img {
        let (er, ei) = exp_c(tms.tm[1].x, tms.tm[2].x, h);
        let a2 = h1 * tms.tm[1].c;
        let b2 = h1 * tms.tm[2].c;
        let (a, b) = mult_c(tms.tm[1].cnv_i, tms.tm[2].cnv_i, er, ei);
        let (a1, b1) = mult_c(a2, b2, x1_i * er + x2_i, x1_i * ei);
        tms.tm[1].cnv_i = a + a1;
        tms.tm[2].cnv_i = b + b1;
        let (a, b) = mult_c(tms.tm[1].cnv_o, tms.tm[2].cnv_o, er, ei);
        let (a1, b1) = mult_c(a2, b2, x1_o * er + x2_o, x1_o * ei);
        tms.tm[1].cnv_o = a + a1;
        tms.tm[2].cnv_o = b + b1;
        let tm = &mut tms.tm[0];
        let e = (tm.x * h).exp();
        tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (x1_i * e + x2_i);
        tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (x1_o * e + x2_o);
        let df = tms.aten * x2_o + tms.tm[0].cnv_o + 2.0 * tms.tm[1].cnv_o;
        let dg = tms.aten * x2_i + tms.tm[0].cnv_i + 2.0 * tms.tm[1].cnv_i;
        (df, dg)
    } else {
        let mut df = 0.0;
        let mut dg = 0.0;
        for tm in &mut tms.tm {
            let e = (tm.x * h).exp();
            tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (x1_i * e + x2_i);
            tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (x1_o * e + x2_o);
            df += tm.cnv_o;
            dg += tm.cnv_i;
        }
        df += tms.aten * x2_o;
        dg += tms.aten * x2_i;
        (df, dg)
    }
}

/// `update_cnv_a`: the complex pair of an `h1` fit over the accepted
/// step.
#[allow(clippy::too_many_arguments)]
fn update_cnv_a(tms: &mut Tms, h: f64, ai: f64, ao: f64, bi: f64, bo: f64, er: f64, ei: f64) {
    let h = h * 0.5e-12;
    let (a1, b1) = mult_c(tms.tm[1].c, tms.tm[2].c, er, ei);
    let (a, b) = mult_c(tms.tm[1].cnv_i, tms.tm[2].cnv_i, er, ei);
    tms.tm[1].cnv_i = a + h * (a1 * bi + ai * tms.tm[1].c);
    tms.tm[2].cnv_i = b + h * (b1 * bi + ai * tms.tm[2].c);
    let (a, b) = mult_c(tms.tm[1].cnv_o, tms.tm[2].cnv_o, er, ei);
    tms.tm[1].cnv_o = a + h * (a1 * bo + ao * tms.tm[1].c);
    tms.tm[2].cnv_o = b + h * (b1 * bo + ao * tms.tm[2].c);
}

/// `divC` of `cplload.c`.
fn div_c(ar: f64, ai: f64, br: f64, bi: f64) -> (f64, f64) {
    let t = br * br + bi * bi;
    ((ar * br + ai * bi) / t, (ai * br - ar * bi) / t)
}

impl Device for CplI {
    fn par_load(&self) -> bool {
        true
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let n = self.n;
        let mut s = CplSlots::default();
        for m in 0..n {
            let (p, q, b1, b2) = (self.pos[m], self.neg[m], self.ibr1[m], self.ibr2[m]);
            s.ibr1_ibr1.push(mb.slot(b1, b1));
            s.ibr2_ibr2.push(mb.slot(b2, b2));
            s.pos_ibr1.push(mb.slot(p, b1));
            s.neg_ibr2.push(mb.slot(q, b2));
            s.pos_pos.push(mb.slot(p, p));
            s.neg_neg.push(mb.slot(q, q));
            s.neg_pos.push(mb.slot(q, p));
            s.pos_neg.push(mb.slot(p, q));
            for k in 0..n {
                s.ibr1_pos.push(mb.slot(b1, self.pos[k]));
                s.ibr2_neg.push(mb.slot(b2, self.neg[k]));
                s.ibr1_neg.push(mb.slot(b1, self.neg[k]));
                s.ibr2_pos.push(mb.slot(b2, self.pos[k]));
                s.ibr1_ibr2.push(mb.slot(b1, self.ibr2[k]));
                s.ibr2_ibr1.push(mb.slot(b2, self.ibr1[k]));
            }
        }
        self.s = s;
    }

    /// `CPLload`.
    fn load(&mut self, ctx: &mut LoadCtx) {
        let h = ctx.delta;
        let time2 = ps(ctx.time);
        let hf = h * 1e12;
        let time = ps(ctx.time - ctx.delta);
        let cond1 = ctx.is_dc();
        let gmin = 0.1 * ctx.gmin;
        for m in 0..self.n {
            ctx.add(self.s.pos_pos[m], gmin);
            ctx.add(self.s.neg_neg[m], gmin);
            ctx.add(self.s.neg_pos[m], gmin);
            ctx.add(self.s.pos_neg[m], gmin);
        }
        if cond1 {
            self.restart();
            self.dc_stamps(ctx);
            return;
        }
        let time = self.advance(time, ctx.sol);
        let mut mintaul = 123456789.0;
        for &t in &self.cp.taul {
            if mintaul > t {
                mintaul = t;
            }
        }
        if mintaul < hf {
            let m = 0.9 * mintaul * 1e-12;
            self.max_step = Some(self.max_step.map_or(m, |o: f64| o.min(m)));
        }
        if let Err(e) = self.companion(time, time2, h, ctx.sol) {
            ctx.fault = Some(e);
            self.fatal = Some(e);
            return;
        }
        for &(s, v) in &self.buf.m {
            ctx.add(s, v);
        }
        for &(r, v) in &self.buf.r {
            ctx.add_rhs(r, v);
        }
    }

    fn max_step(&self) -> Option<f64> {
        self.max_step
    }

    /// Not in the C: see `TxlI::accept`.
    fn accept(&mut self, _ctx: &mut AcceptCtx) {
        self.accepted = true;
        if self.provisional.take().is_some() {
            self.hist.prune(self.cp.head);
        }
    }

    fn fatal(&self) -> Option<&'static str> {
        self.fatal
    }

    fn small_signal(&self) -> Result<(), &'static str> {
        Err("small-signal analysis of a CPL line (ngspice-47's CPL has no AC load)")
    }

    #[cfg(test)]
    fn history_len(&self) -> Option<usize> {
        Some(self.hist.q.len())
    }
}
