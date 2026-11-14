//! `TXL` (ngspice-47 `devices/txl/`): the single lossy transmission
//! line of KSPICE, `YXXX N1 GND1 N2 GND2 MNAME <LEN VALUE>` with
//! `.model MNAME txl R= L= G= C= LENGTH=`.
//!
//! Translated from `txlsetup.c` (setup and the Padé fit: `ReadTxL`,
//! `main_pade`, `y_pade`, `exp_pade`, `pade`, `get_h3`,
//! `update_h1C_c`, the root finders) and `txlload.c` (`TXLload`, which
//! ngspice also installs as `DEVacLoad`). "Copyright 1992 Regents of
//! the University of California. All rights reserved. Author: 1992
//! Charles Hough." `TXLaccept` exists in the C but is not wired into
//! `TXLinfo` (`DEVaccept = NULL`), so it is not ported.

// A statement-by-statement port: the indexed loops mirror the C's.
#![allow(clippy::needless_range_loop)]

use std::collections::VecDeque;

use super::{SetBuf, Term, exp_c, mult_c, ps};
use crate::devices::Device;
use crate::matrix::{AcCtx, AcceptCtx, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};

/// `.model MNAME txl (...)`: the per-unit-length line constants and
/// the default length (`TXLmPTable`). `TXLsetup` refuses a model
/// without any of the five.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TxlModel {
    /// `R`: series resistance per unit length (Ω/m).
    pub r: Option<f64>,
    /// `L`: series inductance per unit length (H/m).
    pub l: Option<f64>,
    /// `G`: shunt conductance per unit length (S/m).
    pub g: Option<f64>,
    /// `C`: shunt capacitance per unit length (F/m).
    pub c: Option<f64>,
    /// `LENGTH`: the line length the instance `LEN` overrides.
    pub length: Option<f64>,
}

/// `TXLine` of `swec.h`: the coefficients and convolution states of
/// one line. The instance keeps two (`txline`, `txline2`): the
/// committed one and the trial one each Newton iteration rebuilds
/// from it (docs/kspice-lines.md, "Estado do TXL").
#[derive(Clone, Debug, Default)]
pub(crate) struct TxLine {
    /// 1 if the line is lossless (`R/L < 5e5` and `G < 1e-2`).
    pub lsl: bool,
    /// Set when the step is longer than the delay left the last load.
    pub ext: bool,
    pub ratio: f64,
    /// The delay, picoseconds.
    pub taul: f64,
    pub sqt_cdl: f64,
    pub h2_aten: f64,
    pub h3_aten: f64,
    pub h1c: f64,
    pub h1e: [f64; 3],
    pub if_img: bool,
    pub h1: [Term; 3],
    pub h2: [Term; 3],
    pub h3: [Term; 6],
    /// `vi_head`: absolute index into the instance history.
    pub head: u64,
}

/// `copy_tx(new, old)`: every coefficient and state, then `new`'s
/// history head catches up with `old`'s (the C frees the passed
/// samples; the instance drops them once neither line needs them).
fn copy_tx(new: &mut TxLine, old: &TxLine) {
    new.lsl = old.lsl;
    new.ext = old.ext;
    new.ratio = old.ratio;
    new.taul = old.taul;
    new.sqt_cdl = old.sqt_cdl;
    new.h2_aten = old.h2_aten;
    new.h3_aten = old.h3_aten;
    new.h1c = old.h1c;
    new.h1e = old.h1e;
    new.h1 = old.h1;
    new.h2 = old.h2;
    new.h3 = old.h3;
    new.if_img = old.if_img;
    if new.head < old.head {
        new.head = old.head;
    }
}

/// The setup result of one `Y` instance (`ReadTxL`): the line
/// coefficients and what the DC load needs.
#[derive(Clone, Debug)]
pub(crate) struct TxlParams {
    pub line: TxLine,
    /// `model->R · length`, the DC series resistance.
    pub g_dc: f64,
}

const EPSI: f64 = 1.0e-16;
const EPSI2: f64 = 1.0e-28;

impl TxlParams {
    /// `TXLsetup` + `ReadTxL` for an instance of `model` with the
    /// instance `LEN` `len`.
    pub fn new(name: &str, model: &TxlModel, len: Option<f64>) -> Result<TxlParams, String> {
        let need =
            |v: Option<f64>, what: &str| v.ok_or_else(|| format!("`{name}`: lossy line {what}"));
        let r = need(model.r, "series resistance not given")?;
        let g = need(model.g, "parallel conductance not given")?;
        let l = need(model.l, "series inductance not given")?;
        let c = need(model.c, "parallel capacitance not given")?;
        let mlen = need(model.length, "length must be given")?;
        let len = len.unwrap_or(mlen);
        for (v, what) in [(r, "R"), (l, "L"), (g, "G"), (c, "C"), (len, "length")] {
            if !v.is_finite() {
                return Err(format!("`{name}`: TXL {what} must be finite"));
            }
        }
        let mut t = TxLine::default();
        // `ReadTxL`: `L = MAX(L, 1e-12)`.
        let ll = 1e-12;
        let lm = if l > ll { l } else { ll };
        if len == 0.0 {
            // "(Error) transmission line of zero length", then
            // `controlled_exit`.
            return Err(format!("`{name}`: transmission line of zero length"));
        }
        if r / lm < 5.0e5 && g < 1.0e-2 {
            t.lsl = true;
            t.taul = (c * lm).sqrt() * len * 1.0e12;
            t.sqt_cdl = (c / lm).sqrt();
            t.h3_aten = t.sqt_cdl;
            t.h2_aten = 1.0;
            t.h1c = 0.0;
        }
        if !t.lsl {
            main_pade(r, lm, g, c, len, &mut t).map_err(|e| format!("`{name}`: {e}"))?;
        }
        if !line_finite(&t) {
            return Err(format!(
                "`{name}`: the TXL Padé fit of R={r:e} L={lm:e} G={g:e} C={c:e} \
                 length={len:e} is not finite"
            ));
        }
        if t.taul.is_nan() || t.taul <= 0.0 {
            return Err(format!(
                "`{name}`: TXL delay {:e} s is not positive",
                t.taul * 1e-12
            ));
        }
        Ok(TxlParams {
            line: t,
            g_dc: r * len,
        })
    }
}

fn line_finite(t: &TxLine) -> bool {
    let terms = t.h1.iter().chain(&t.h2).chain(&t.h3);
    [t.taul, t.sqt_cdl, t.h2_aten, t.h3_aten, t.h1c]
        .iter()
        .all(|v| v.is_finite())
        && terms
            .into_iter()
            .all(|m| m.c.is_finite() && m.x.is_finite())
}

/// `main_pade`.
fn main_pade(r: f64, l: f64, g: f64, c: f64, len: f64, h: &mut TxLine) -> Result<(), String> {
    y_pade(r, l, g, c, h)?;
    h.if_img = exp_pade(r, l, g, c, len, h)?;
    get_h3(h);
    h.taul *= 1.0e12;
    update_h1c_c(h);
    Ok(())
}

/// `div_C` of `txlsetup.c` (operation order of the C).
fn div_c(ar: f64, ai: f64, br: f64, bi: f64) -> (f64, f64) {
    let mut cr = ar * br + ai * bi;
    let mut ci = -ar * bi + ai * br;
    cr /= br * br + bi * bi;
    ci /= br * br + bi * bi;
    (cr, ci)
}

/// `get_h3`: the partial fractions of `h1·h2` (the voltage-to-current
/// propagation term) from those of `h1` and `h2`.
fn get_h3(h: &mut TxLine) {
    h.h3_aten = h.h2_aten * h.sqt_cdl;
    let xx1 = h.h1[0].x;
    let xx2 = h.h1[1].x;
    let xx3 = h.h1[2].x;
    let xx4 = h.h2[0].x;
    let xx5 = h.h2[1].x;
    let xx6 = h.h2[2].x;
    for (k, x) in [xx1, xx2, xx3, xx4, xx5, xx6].into_iter().enumerate() {
        h.h3[k].x = x;
    }
    let cc1 = h.h1[0].c;
    let cc2 = h.h1[1].c;
    let cc3 = h.h1[2].c;
    let cc4 = h.h2[0].c;
    let cc5 = h.h2[1].c;
    let cc6 = h.h2[2].c;
    if h.if_img {
        let d = |xx: f64| xx * xx - 2.0 * xx5 * xx + xx5 * xx5 + xx6 * xx6;
        h.h3[0].c =
            cc1 + cc1 * (cc4 / (xx1 - xx4) + 2.0 * (cc5 * xx1 - xx6 * cc6 - xx5 * cc5) / d(xx1));
        h.h3[1].c =
            cc2 + cc2 * (cc4 / (xx2 - xx4) + 2.0 * (cc5 * xx2 - xx6 * cc6 - xx5 * cc5) / d(xx2));
        h.h3[2].c =
            cc3 + cc3 * (cc4 / (xx3 - xx4) + 2.0 * (cc5 * xx3 - xx6 * cc6 - xx5 * cc5) / d(xx3));
        h.h3[3].c = cc4 + cc4 * (cc1 / (xx4 - xx1) + cc2 / (xx4 - xx2) + cc3 / (xx4 - xx3));
        h.h3[4].c = cc5;
        h.h3[5].c = cc6;
        for (cc, xx) in [(cc1, xx1), (cc2, xx2), (cc3, xx3)] {
            let (r, i) = div_c(cc5, cc6, xx5 - xx, xx6);
            h.h3[4].c += r * cc;
            h.h3[5].c += i * cc;
        }
    } else {
        h.h3[0].c = cc1 + cc1 * (cc4 / (xx1 - xx4) + cc5 / (xx1 - xx5) + cc6 / (xx1 - xx6));
        h.h3[1].c = cc2 + cc2 * (cc4 / (xx2 - xx4) + cc5 / (xx2 - xx5) + cc6 / (xx2 - xx6));
        h.h3[2].c = cc3 + cc3 * (cc4 / (xx3 - xx4) + cc5 / (xx3 - xx5) + cc6 / (xx3 - xx6));
        h.h3[3].c = cc4 + cc4 * (cc1 / (xx4 - xx1) + cc2 / (xx4 - xx2) + cc3 / (xx4 - xx3));
        h.h3[4].c = cc5 + cc5 * (cc1 / (xx5 - xx1) + cc2 / (xx5 - xx2) + cc3 / (xx5 - xx3));
        h.h3[5].c = cc6 + cc6 * (cc1 / (xx6 - xx1) + cc2 / (xx6 - xx2) + cc3 / (xx6 - xx3));
    }
}

/// `update_h1C_c`: scale the residues by their attenuations.
fn update_h1c_c(h: &mut TxLine) {
    let mut d = 0.0;
    for t in &mut h.h1 {
        t.c *= h.sqt_cdl;
        d += t.c;
    }
    h.h1c = d;
    for t in &mut h.h2 {
        t.c *= h.h2_aten;
    }
    for t in &mut h.h3 {
        t.c *= h.h3_aten;
    }
}

fn eval2(a: f64, b: f64, c: f64, x: f64) -> f64 {
    a * x * x + b * x + c
}

/// `y_pade`: the Padé approximation of the characteristic admittance
/// `Y(s)/sqrt(C/L)` around `s = ∞` (`y.c`).
fn y_pade(r: f64, l: f64, g: f64, c: f64, h: &mut TxLine) -> Result<(), String> {
    let sqt_cdl = (c / l).sqrt();
    let rdl = r / l;
    let gdc = g / c;
    let [b1, b2, b3, b4, b5] = mac(gdc, rdl);
    let mut a = [
        [1.0 - (gdc / rdl).sqrt(), b1, b2, -b3],
        [b1, b2, b3, -b4],
        [b2, b3, b4, -b5],
    ];
    gaussian_elimination(&mut a, EPSI)?;
    let p3 = a[0][3];
    let p2 = a[1][3];
    let p1 = a[2][3];
    let q1 = p1 + b1;
    let q2 = b1 * p1 + p2 + b2;
    let q3 = p3 * (gdc / rdl).sqrt();
    let (x1, x2, x3) = find_roots(p1, p2, p3)?;
    let c1 = eval2(q1 - p1, q2 - p2, q3 - p3, x1) / eval2(3.0, 2.0 * p1, p2, x1);
    let c2 = eval2(q1 - p1, q2 - p2, q3 - p3, x2) / eval2(3.0, 2.0 * p1, p2, x2);
    let c3 = eval2(q1 - p1, q2 - p2, q3 - p3, x3) / eval2(3.0, 2.0 * p1, p2, x3);
    h.sqt_cdl = sqt_cdl;
    h.h1[0].c = c1;
    h.h1[1].c = c2;
    h.h1[2].c = c3;
    h.h1[0].x = x1;
    h.h1[1].x = x2;
    h.h1[2].x = x3;
    Ok(())
}

/// `Gaussian_Elimination1`/`Gaussian_Elimination2` (identical but for
/// the pivot threshold): Gauss-Jordan with partial pivoting on a 3×4
/// augmented matrix. The C prints a message and exits on a pivot
/// below `eps`; Dune refuses the instance.
fn gaussian_elimination(a: &mut [[f64; 4]; 3], eps: f64) -> Result<(), String> {
    let dim = 3;
    for i in 0..dim {
        let mut imax = i;
        let mut max = a[i][i].abs();
        for (j, row) in a.iter().enumerate().take(dim).skip(i + 1) {
            if row[i].abs() > max {
                imax = j;
                max = row[i].abs();
            }
        }
        // `if (max < epsi)`: a NaN pivot passes it in the C too.
        if max < eps {
            return Err(format!(
                "Gaussian elimination fails during setup of TXL (all matrix elements \
                 are less than {eps:e}); check the model parameters"
            ));
        }
        if imax != i {
            for k in i..=dim {
                let t = a[i][k];
                a[i][k] = a[imax][k];
                a[imax][k] = t;
            }
        }
        let f = 1.0 / a[i][i];
        a[i][i] = 1.0;
        for j in (i + 1)..=dim {
            a[i][j] *= f;
        }
        for j in 0..dim {
            if i == j {
                continue;
            }
            let f = a[j][i];
            a[j][i] = 0.0;
            for k in (i + 1)..=dim {
                a[j][k] -= f * a[i][k];
            }
        }
    }
    Ok(())
}

/// `root3` of `txlsetup.c`: one Newton step on the monic cubic.
fn root3(a1: f64, a2: f64, a3: f64, x: f64) -> f64 {
    let t1 = x * x * x + a1 * x * x + a2 * x + a3;
    let t2 = 3.0 * x * x + 2.0 * a1 * x + a2;
    x - t1 / t2
}

/// `div3`: deflate the cubic by its root `x`.
fn div3(a1: f64, a3: f64, x: f64) -> (f64, f64) {
    (a1 + x, -a3 / x)
}

/// The real root of `x³ + a1x² + a2x + a3` by the trigonometric /
/// Cardano formula, polished by up to 31 Newton steps (the shared
/// head of `find_roots` and `exp_find_roots`).
fn first_root(a1: f64, a2: f64, a3: f64) -> f64 {
    let q = (a1 * a1 - 3.0 * a2) / 9.0;
    let p = (2.0 * a1 * a1 * a1 - 9.0 * a1 * a2 + 27.0 * a3) / 54.0;
    let t = q * q * q - p * p;
    let mut x;
    if t >= 0.0 {
        let t = (p / (q * q.sqrt())).acos();
        x = -2.0 * q.sqrt() * (t / 3.0).cos() - a1 / 3.0;
    } else if p > 0.0 {
        let t = ((-t).sqrt() + p).powf(1.0 / 3.0);
        x = -(t + q / t) - a1 / 3.0;
    } else if p == 0.0 {
        x = -a1 / 3.0;
    } else {
        let t = ((-t).sqrt() - p).powf(1.0 / 3.0);
        x = (t + q / t) - a1 / 3.0;
    }
    let backup = x;
    let mut i = 0;
    let mut t = root3(a1, a2, a3, x);
    while (t - x).abs() > 5.0e-4 {
        i += 1;
        if i == 32 {
            x = backup;
            break;
        }
        x = t;
        t = root3(a1, a2, a3, x);
    }
    x
}

/// `find_roots` of `y.c`: the three real poles of the admittance fit;
/// a complex pair is fatal in the C ("Two Imaginary Roots in
/// Characteristic Admittance", `controlled_exit`).
fn find_roots(a1: f64, a2: f64, a3: f64) -> Result<(f64, f64, f64), String> {
    let x = first_root(a1, a2, a3);
    let (a1, a2) = div3(a1, a3, x);
    let mut t = a1 * a1 - 4.0 * a2;
    if t < 0.0 {
        return Err("two imaginary roots in the TXL characteristic admittance".into());
    }
    t *= 1.0e-18;
    t = t.sqrt() * 1.0e9;
    let x2 = if a1 >= 0.0 {
        -0.5 * (a1 + t)
    } else {
        -0.5 * (a1 - t)
    };
    Ok((x, x2, a2 / x2))
}

/// `exp_find_roots` of `exp.c`: the poles of the propagation fit; a
/// complex pair is allowed (returned as `(re, im)` in `x2`, `x3` with
/// the flag set — the C's static `ifImg`, which it also prints).
fn exp_find_roots(a1: f64, a2: f64, a3: f64) -> (f64, f64, f64, bool) {
    let x = first_root(a1, a2, a3);
    let (a1, a2) = div3(a1, a3, x);
    let mut t = a1 * a1 - 4.0 * a2;
    if t < 0.0 {
        let x3 = 0.5 * (-t).sqrt();
        let x2 = -0.5 * a1;
        (x, x2, x3, true)
    } else {
        t *= 1.0e-16;
        t = t.sqrt() * 1.0e8;
        let x2 = if a1 >= 0.0 {
            -0.5 * (a1 + t)
        } else {
            -0.5 * (a1 - t)
        };
        (x, x2, a2 / x2, false)
    }
}

/// `mac`: the Maclaurin coefficients of `sqrt((1+az)/(1+bz))`.
fn mac(a: f64, b: f64) -> [f64; 5] {
    let y1 = 0.5 * (a - b);
    let b1 = y1;
    let y2 = 0.5 * (3.0 * b * b - 2.0 * a * b - a * a) * y1 / (a - b);
    let y3 = ((3.0 * b * b + a * a) * y1 * y1 + 0.5 * (3.0 * b * b - 2.0 * a * b - a * a) * y2)
        / (a - b);
    let y4 = ((3.0 * b * b - 3.0 * a * a) * y1 * y1 * y1
        + (9.0 * b * b + 3.0 * a * a) * y1 * y2
        + 0.5 * (3.0 * b * b - 2.0 * a * b - a * a) * y3)
        / (a - b);
    let y5 = (12.0 * a * a * y1 * y1 * y1 * y1
        + y1 * y1 * y2 * (18.0 * b * b - 18.0 * a * a)
        + (9.0 * b * b + 3.0 * a * a) * (y2 * y2 + y1 * y3)
        + (3.0 * b * b + a * a) * y1 * y3
        + 0.5 * (3.0 * b * b - 2.0 * a * b - a * a) * y4)
        / (a - b);
    [b1, y2 / 2.0, y3 / 6.0, y4 / 24.0, y5 / 120.0]
}

/// `get_c` of `exp.c`: the complex residue at the pole `a + jb`.
fn get_c(q1: f64, q2: f64, q3: f64, p1: f64, p2: f64, a: f64, b: f64) -> (f64, f64) {
    let mut d =
        (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2) * (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2);
    d += (6.0 * a * b + 2.0 * p1 * b) * (6.0 * a * b + 2.0 * p1 * b);
    let mut n = -(q1 * (a * a - b * b) + q2 * a + q3) * (6.0 * a * b + 2.0 * p1 * b);
    n += (2.0 * q1 * a * b + q2 * b) * (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2);
    let ci = n / d;
    let mut n = (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2) * (q1 * (a * a - b * b) + q2 * a + q3);
    n += (6.0 * a * b + 2.0 * p1 * b) * (2.0 * q1 * a * b + q2 * b);
    (n / d, ci)
}

/// `exp_pade` + `pade`: the Padé approximation of the propagation
/// function `e^{-l·sqrt((R+sL)(G+sC))}` with its delay `τ·l` taken
/// out (`exp.c`). Returns `ifImg`. The C's file-scope statics
/// (`a0…a5`, `ep*`, `eq*`, `ec*`, `ex*`, `RG`, `tau`, `ifImg`, `AA`)
/// are locals here.
fn exp_pade(r: f64, l: f64, g: f64, c: f64, len: f64, h: &mut TxLine) -> Result<bool, String> {
    let tau = (l * c).sqrt();
    let rdl = r / l;
    let gdc = g / c;
    let rg = r * g;
    let (a, b, t) = (rdl, gdc, tau);
    let y1 = 0.5 * (a + b);
    let y2 = a * b - y1 * y1;
    let y3 = -3.0 * y1 * y2;
    let y4 = -3.0 * y2 * y2 - 4.0 * y1 * y3;
    let y5 = -5.0 * y1 * y4 - 10.0 * y2 * y3;
    let y6 = -10.0 * y3 * y3 - 15.0 * y2 * y4 - 6.0 * y1 * y5;
    let mut a0 = y1 * t;
    let mut a1 = y2 * t * t / 2.0;
    let mut a2 = y3 * t * t * t / 6.0;
    let mut a3 = y4 * t * t * t * t / 24.0;
    let mut a4 = y5 * t * t * t * t * t / 120.0;
    let mut a5 = y6 * t * t * t * t * t * t / 720.0;
    a0 *= len;
    a1 *= len;
    a2 *= len;
    a3 *= len;
    a4 *= len;
    a5 *= len;

    // `pade(l)`.
    let av = [0.0, -a1, -a2, -a3, -a4, -a5];
    let mut bv = [0.0; 6];
    bv[0] = 1.0;
    bv[1] = av[1];
    for i in 2..=5 {
        bv[i] = 0.0;
        for j in 1..=i {
            bv[i] += j as f64 * av[j] * bv[i - j];
        }
        bv[i] /= i as f64;
    }
    let mut aa = [
        [1.0 - (a0 - len * rg.sqrt()).exp(), bv[1], bv[2], -bv[3]],
        [bv[1], bv[2], bv[3], -bv[4]],
        [bv[2], bv[3], bv[4], -bv[5]],
    ];
    gaussian_elimination(&mut aa, EPSI2)?;
    let mut ep3 = aa[0][3];
    let mut ep2 = aa[1][3];
    let mut ep1 = aa[2][3];
    let mut eq1 = ep1 + bv[1];
    let mut eq2 = bv[1] * ep1 + ep2 + bv[2];
    let mut eq3 = ep3 * (a0 - len * rg.sqrt()).exp();
    ep3 /= tau * tau * tau;
    ep2 /= tau * tau;
    ep1 /= tau;
    eq3 /= tau * tau * tau;
    eq2 /= tau * tau;
    eq1 /= tau;
    let (ex1, ex2, ex3, if_img) = exp_find_roots(ep1, ep2, ep3);
    let ec1 = eval2(eq1 - ep1, eq2 - ep2, eq3 - ep3, ex1) / eval2(3.0, 2.0 * ep1, ep2, ex1);
    let (ec2, ec3) = if if_img {
        get_c(eq1 - ep1, eq2 - ep2, eq3 - ep3, ep1, ep2, ex2, ex3)
    } else {
        (
            eval2(eq1 - ep1, eq2 - ep2, eq3 - ep3, ex2) / eval2(3.0, 2.0 * ep1, ep2, ex2),
            eval2(eq1 - ep1, eq2 - ep2, eq3 - ep3, ex3) / eval2(3.0, 2.0 * ep1, ep2, ex3),
        )
    };

    h.taul = tau * len;
    h.h2_aten = (-a0).exp();
    h.h2[0].c = ec1;
    h.h2[1].c = ec2;
    h.h2[2].c = ec3;
    h.h2[0].x = ex1;
    h.h2[1].x = ex2;
    h.h2[2].x = ex3;
    Ok(if_img)
}

/// One accepted sample of the port history (`VI_list_txl`).
#[derive(Clone, Copy, Debug, Default)]
struct Vi {
    time: i32,
    v_i: f64,
    v_o: f64,
    i_i: f64,
    i_o: f64,
}

/// The port history, `vi_head … vi_tail` of both lines: one queue
/// with absolute indices (the C's shared linked list and its free
/// list).
#[derive(Clone, Debug, Default)]
struct History {
    q: VecDeque<Vi>,
    /// Absolute index of `q[0]`.
    base: u64,
}

impl History {
    fn get(&self, k: u64) -> Option<&Vi> {
        k.checked_sub(self.base)
            .and_then(|o| usize::try_from(o).ok())
            .and_then(|o| self.q.get(o))
    }

    fn tail(&self) -> Option<&Vi> {
        self.q.back()
    }

    fn tail_index(&self) -> u64 {
        self.base + self.q.len() as u64 - 1
    }

    /// Drop the samples before absolute index `k`.
    fn prune(&mut self, k: u64) {
        while self.base < k && !self.q.is_empty() {
            self.q.pop_front();
            self.base += 1;
        }
    }
}

/// The delayed port values `get_pvs_vi_txl` returns.
#[derive(Default)]
struct Pvs {
    v1_i: f64,
    v2_i: f64,
    i1_i: f64,
    i2_i: f64,
    v1_o: f64,
    v2_o: f64,
    i1_o: f64,
    i2_o: f64,
}

/// What the C dereferences past the end of its history (a NULL
/// `vi->next`): it crashes; Dune faults the load.
const HISTORY_FAULT: &str = "TXL: the line history ran out (time step too large for the \
                             line delay)";

/// What a provisional sample changed (see `TxlI::provisional`).
struct TxlSnap {
    tx: TxLine,
    /// The history's end (absolute index) before the sample.
    len: u64,
    nodes: [f64; 4],
}

/// A `Y` instance (`TXLinstance` with its two `TXLine`s).
pub(crate) struct TxlI {
    pos: Var,
    neg: Var,
    ibr1: Var,
    ibr2: Var,
    /// The setup coefficients, to restart from at every DC load.
    proto: TxLine,
    /// `txline` (committed) and `txline2` (trial).
    tx: TxLine,
    tx2: TxLine,
    g_dc: f64,
    hist: History,
    /// `TXLdcGiven`.
    dc_given: bool,
    dc1: f64,
    dc2: f64,
    /// The SWEC nodes of the two ends (`in_node->V/dv`,
    /// `out_node->V/dv`), shared by both lines.
    in_v: f64,
    in_dv: f64,
    out_v: f64,
    out_dv: f64,
    /// `CKTrhsOld` at the small-signal pass: what the dc setup of the
    /// first AC load reads (`(pos, neg, ibr1, ibr2)`).
    smsig_sol: [f64; 4],
    /// `CKTmaxStep = 0.9·taul` once a step outran the delay.
    max_step: Option<f64>,
    /// A fault the C exits on (see [`Device::fatal`]).
    fatal: Option<&'static str>,
    /// Whether the engine accepted a timepoint since the last sample
    /// was appended (`Device::accept`).
    accepted: bool,
    /// The state before a sample appended with no accept in between —
    /// a TR-BDF2 stage point (Dune's default profile): restored if
    /// the step is then rejected (see [`TxlI::advance`]).
    provisional: Option<Box<TxlSnap>>,
    /// `CKTgmin` of the run, for the AC load.
    gmin: f64,
    s: TxlSlots,
    buf: SetBuf,
}

#[derive(Clone, Copy)]
struct TxlSlots {
    pos_pos: Slot,
    pos_neg: Slot,
    neg_pos: Slot,
    neg_neg: Slot,
    ibr1_pos: Slot,
    ibr2_neg: Slot,
    neg_ibr2: Slot,
    pos_ibr1: Slot,
    ibr1_ibr1: Slot,
    ibr2_ibr2: Slot,
    ibr1_neg: Slot,
    ibr2_pos: Slot,
    ibr1_ibr2: Slot,
    ibr2_ibr1: Slot,
}

impl TxlI {
    /// Nodes `pos` (`N1`), `neg` (`N2`); branch equations `ibr` (the
    /// C's `branch1`, `branch2`).
    pub fn new(p: TxlParams, pos: Var, neg: Var, ibr: [Var; 2], gmin: f64) -> Self {
        TxlI {
            pos,
            neg,
            ibr1: ibr[0],
            ibr2: ibr[1],
            tx: p.line.clone(),
            tx2: p.line.clone(),
            proto: p.line,
            g_dc: p.g_dc,
            hist: History::default(),
            dc_given: false,
            dc1: 0.0,
            dc2: 0.0,
            in_v: 0.0,
            in_dv: 0.0,
            out_v: 0.0,
            out_dv: 0.0,
            smsig_sol: [0.0; 4],
            max_step: None,
            fatal: None,
            accepted: false,
            provisional: None,
            gmin,
            s: TxlSlots {
                pos_pos: TRASH,
                pos_neg: TRASH,
                neg_pos: TRASH,
                neg_neg: TRASH,
                ibr1_pos: TRASH,
                ibr2_neg: TRASH,
                neg_ibr2: TRASH,
                pos_ibr1: TRASH,
                ibr1_ibr1: TRASH,
                ibr2_ibr2: TRASH,
                ibr1_neg: TRASH,
                ibr2_pos: TRASH,
                ibr1_ibr2: TRASH,
                ibr2_ibr1: TRASH,
            },
            buf: SetBuf::default(),
        }
    }

    /// Back to the state after `TXLsetup`: no history, no dc setup.
    /// Called at every DC load — the C's `TXLdcGiven` is cleared only
    /// by `TXLunsetup`, but every Dune analysis loads its operating
    /// point before the first non-DC load.
    fn restart(&mut self) {
        self.tx = self.proto.clone();
        self.tx2 = self.proto.clone();
        self.hist = History::default();
        self.dc_given = false;
        self.dc1 = 0.0;
        self.dc2 = 0.0;
        self.in_v = 0.0;
        self.in_dv = 0.0;
        self.out_v = 0.0;
        self.out_dv = 0.0;
        self.max_step = None;
        self.fatal = None;
        self.accepted = false;
        self.provisional = None;
    }

    /// `add_new_vi_txl`.
    fn add_new_vi(&mut self, time: i32, sol: &[f64]) {
        self.hist.q.push_back(Vi {
            time,
            v_i: sol[self.pos],
            v_o: sol[self.neg],
            i_i: sol[self.ibr1],
            i_o: sol[self.ibr2],
        });
    }

    /// The dc setup of `TXLload` (`TXLdcGiven == 0`): the history
    /// starts with the operating point at t = 0 and the convolutions
    /// with its steady state. `x` is `(pos, neg, ibr1, ibr2)` of
    /// `CKTrhsOld`.
    fn dc_setup(&mut self, x: [f64; 4]) {
        let tx = &mut self.tx;
        self.dc1 = x[0];
        self.in_v = self.dc1;
        self.dc2 = x[1];
        self.out_v = self.dc2;
        self.dc_given = true;
        let vi = Vi {
            time: 0,
            i_i: x[2],
            i_o: x[3],
            v_i: self.dc1,
            v_o: self.dc2,
        };
        for t in &mut tx.h1 {
            t.cnv_i = -self.dc1 * t.c / t.x;
            t.cnv_o = -self.dc2 * t.c / t.x;
        }
        for t in &mut tx.h2 {
            t.cnv_i = 0.0;
            t.cnv_o = 0.0;
        }
        for t in &mut tx.h3 {
            t.cnv_i = -self.dc1 * t.c / t.x;
            t.cnv_o = -self.dc2 * t.c / t.x;
        }
        self.hist = History::default();
        self.hist.q.push_back(vi);
        let head = self.hist.tail_index();
        self.tx.head = head;
        self.tx2.head = head;
    }

    /// The first instance loop of `TXLload`: at the first load of a
    /// new timepoint (`time` past the last sample) the trial line is
    /// committed, the accepted solution appended to the history and
    /// the convolutions advanced over the accepted step. Returns the
    /// `time` the second loop uses (the C bumps its shared `time` to
    /// the last sample when rounding left it behind).
    fn advance(&mut self, mut time: i32, sol: &[f64]) -> i32 {
        if let Some(tail) = self.hist.tail()
            && time < tail.time
            && let Some(snap) = self.provisional.take()
        {
            // The step past a provisional sample was rejected.
            self.rollback(*snap);
        }
        let Some(&before) = self.hist.tail() else {
            return time;
        };
        if time < before.time {
            time = before.time;
        }
        if time > before.time {
            if self.accepted {
                self.provisional = None;
            } else {
                self.provisional = Some(Box::new(self.snapshot()));
            }
            self.accepted = false;
            copy_tx(&mut self.tx, &self.tx2);
            if self.provisional.is_none() {
                self.hist.prune(self.tx.head);
            }
            self.add_new_vi(time, sol);
            let delta = (time - before.time) as f64;
            let tail = *self.hist.tail().expect("just pushed");
            let v = before.v_i;
            self.in_v = tail.v_i;
            self.in_dv = (self.in_v - v) / delta;
            let v = before.v_o;
            self.out_v = tail.v_o;
            self.out_dv = (self.out_v - v) / delta;
            if self.tx.lsl {
                return time;
            }
            update_cnv_txl(
                &mut self.tx,
                self.in_v,
                self.out_v,
                self.in_dv,
                self.out_dv,
                delta,
            );
            if self.tx.ext {
                let tail = *self.hist.tail().expect("just pushed");
                update_delayed_cnv_txl(&mut self.tx, &tail, delta);
            }
        }
        time
    }

    fn snapshot(&self) -> TxlSnap {
        TxlSnap {
            tx: self.tx.clone(),
            len: self.hist.base + self.hist.q.len() as u64,
            nodes: [self.in_v, self.in_dv, self.out_v, self.out_dv],
        }
    }

    /// Undo a provisional sample: the committed line, the history and
    /// the node values as they were before it.
    fn rollback(&mut self, snap: TxlSnap) {
        self.tx = snap.tx;
        while self.hist.base + self.hist.q.len() as u64 > snap.len {
            self.hist.q.pop_back();
        }
        self.tx2.head = self.tx.head;
        [self.in_v, self.in_dv, self.out_v, self.out_dv] = snap.nodes;
        // The sample before it came after the last accept.
        self.accepted = false;
    }

    /// The second instance loop of `TXLload` outside the DC modes:
    /// the companion stamps and the trial convolution. `time` is the
    /// previous timepoint, `time2` this one (picoseconds).
    fn companion(
        &mut self,
        time: i32,
        time2: i32,
        h: f64,
        dc_x: [f64; 4],
    ) -> Result<(), &'static str> {
        let h1 = 0.5 * h;
        if !self.dc_given {
            self.dc_setup(dc_x);
        }
        let s = self.s;
        let tx = &self.tx;
        self.buf.clear();
        self.buf.set(s.ibr1_ibr1, -1.0);
        self.buf.set(s.ibr2_ibr2, -1.0);
        self.buf.set(s.pos_ibr1, 1.0);
        self.buf.set(s.neg_ibr2, 1.0);
        self.buf.set(s.ibr1_pos, tx.sqt_cdl + h1 * tx.h1c);
        self.buf.set(s.ibr2_neg, tx.sqt_cdl + h1 * tx.h1c);

        copy_tx(&mut self.tx2, &self.tx);
        let r = right_consts_txl(
            &mut self.tx2,
            &self.hist,
            time,
            time2,
            h,
            h1,
            (self.in_v, self.out_v),
            (self.dc1, self.dc2),
        )?;
        self.buf.set_rhs(self.ibr1, r.ff);
        self.buf.set_rhs(self.ibr2, r.gg);
        let tx = &mut self.tx;
        if let Some(ratio) = r.ratio {
            if tx.lsl {
                let f = ratio * tx.h3_aten;
                self.buf.set(s.ibr1_neg, -f);
                self.buf.set(s.ibr2_pos, -f);
                let f = ratio * tx.h2_aten;
                self.buf.set(s.ibr1_ibr2, -f);
                self.buf.set(s.ibr2_ibr1, -f);
            } else {
                tx.ext = true;
                tx.ratio = ratio;
                if ratio > 0.0 {
                    let f = ratio
                        * (h1
                            * (tx.h3[0].c
                                + tx.h3[1].c
                                + tx.h3[2].c
                                + tx.h3[3].c
                                + tx.h3[4].c
                                + tx.h3[5].c)
                            + tx.h3_aten);
                    self.buf.set(s.ibr1_neg, -f);
                    self.buf.set(s.ibr2_pos, -f);
                    let f = ratio * (h1 * (tx.h2[0].c + tx.h2[1].c + tx.h2[2].c) + tx.h2_aten);
                    self.buf.set(s.ibr1_ibr2, -f);
                    self.buf.set(s.ibr2_ibr1, -f);
                }
            }
        } else {
            tx.ext = false;
        }
        Ok(())
    }

    /// The DC stamps of `TXLload` (`MODEDC`): `R·len` between the
    /// ends, `G` ignored.
    fn dc_stamps(&self, ctx: &mut LoadCtx) {
        let s = self.s;
        ctx.add(s.pos_ibr1, 1.0);
        ctx.add(s.neg_ibr2, 1.0);
        ctx.add(s.ibr1_ibr1, 1.0);
        ctx.add(s.ibr1_ibr2, 1.0);
        ctx.add(s.ibr2_pos, 1.0);
        ctx.add(s.ibr2_neg, -1.0);
        ctx.add(s.ibr2_ibr1, -self.g_dc);
    }

    fn gmin_slots(&self) -> [Slot; 4] {
        [
            self.s.pos_pos,
            self.s.neg_neg,
            self.s.neg_pos,
            self.s.pos_neg,
        ]
    }

    /// The `TXLload` step limit: `CKTmaxStep = 0.9·taul` when the step
    /// (picoseconds) exceeds the delay of a lossy line.
    fn check_step(&mut self, hf: f64) {
        if !self.tx.lsl && hf > self.tx.taul {
            let m = 0.9 * self.tx.taul * 1e-12;
            self.max_step = Some(self.max_step.map_or(m, |o| o.min(m)));
        }
    }
}

/// `update_cnv_txl`: advance the `h1` convolutions over the accepted
/// step `h` (picoseconds) with the node voltages and slopes. The C
/// multiplies the slopes `bi`, `bo` by each term's `c/x` cumulatively
/// across the three terms (ported as is; docs/kspice-lines.md).
fn update_cnv_txl(tx: &mut TxLine, ai: f64, ao: f64, mut bi: f64, mut bo: f64, h: f64) {
    for i in 0..3 {
        let e = tx.h1e[i];
        let tm = &mut tx.h1[i];
        let t = tm.c / tm.x;
        bi *= t;
        bo *= t;
        tm.cnv_i = (tm.cnv_i - bi * h) * e + (e - 1.0) * (ai * t + 1.0e+12 * bi / tm.x);
        tm.cnv_o = (tm.cnv_o - bo * h) * e + (e - 1.0) * (ao * t + 1.0e+12 * bo / tm.x);
    }
}

/// `update_delayed_cnv_txl`: the part of the step that fell inside
/// the delay at the last load (`ratio`), added with the new sample.
fn update_delayed_cnv_txl(tx: &mut TxLine, vi: &Vi, h: f64) {
    let h = h * 0.5e-12;
    let ratio1 = tx.ratio;
    if ratio1 > 0.0 {
        let f = h * ratio1 * vi.v_i;
        for t in &mut tx.h3 {
            t.cnv_i += f * t.c;
        }
        let f = h * ratio1 * vi.v_o;
        for t in &mut tx.h3 {
            t.cnv_o += f * t.c;
        }
        let f = h * ratio1 * vi.i_i;
        for t in &mut tx.h2 {
            t.cnv_i += f * t.c;
        }
        let f = h * ratio1 * vi.i_o;
        for t in &mut tx.h2 {
            t.cnv_o += f * t.c;
        }
    }
}

/// `get_pvs_vi_txl`: the port values one delay ago at the previous
/// timepoint `t1` and at this one `t2`. `Ok(Some(ratio))` is the C's
/// `ext` with `ratio[0]`: the delayed point `t2 − τ` lies past `t1`,
/// so its value is unknown yet and is split off by `ratio`.
fn get_pvs(
    t1: i32,
    t2: i32,
    tx: &mut TxLine,
    hist: &History,
    dc: (f64, f64),
) -> Result<(Pvs, Option<f64>), &'static str> {
    let mut p = Pvs::default();
    let ta = t1 as f64 - tx.taul;
    let tb = t2 as f64 - tx.taul;
    let (dc1, dc2) = dc;
    if tb <= 0.0 {
        p.v1_i = dc1;
        p.v2_i = dc1;
        p.v2_o = dc2;
        p.v1_o = dc2;
        return Ok((p, None));
    }
    let at = |k: u64| hist.get(k).copied();
    let mut vi1k = tx.head;
    let mut vik = tx.head + 1;
    let mut vi1 = at(vi1k).ok_or(HISTORY_FAULT)?;
    if ta <= 0.0 {
        p.i1_i = 0.0;
        p.i1_o = 0.0;
        p.v1_i = dc1;
        p.v1_o = dc2;
    } else {
        let mut vi = at(vik).ok_or(HISTORY_FAULT)?;
        while (vi.time as f64) < ta {
            vi1 = vi;
            vi1k = vik;
            vik += 1;
            vi = at(vik).ok_or(HISTORY_FAULT)?;
        }
        let f = (ta - vi1.time as f64) / (vi.time - vi1.time) as f64;
        p.v1_i = vi1.v_i + f * (vi.v_i - vi1.v_i);
        p.v1_o = vi1.v_o + f * (vi.v_o - vi1.v_o);
        p.i1_i = vi1.i_i + f * (vi.i_i - vi1.i_i);
        p.i1_o = vi1.i_o + f * (vi.i_o - vi1.i_o);
        tx.head = vi1k;
    }
    if tb > t1 as f64 {
        let ratio = (tb - t1 as f64) / (t2 - t1) as f64;
        // `for (; vi->time != t1; vi = vi->next)`, or `vi1` when the
        // history had a single sample.
        let vi = match at(vik) {
            Some(mut v) => {
                while v.time != t1 {
                    vik += 1;
                    v = at(vik).ok_or(HISTORY_FAULT)?;
                }
                v
            }
            None => vi1,
        };
        let f = 1.0 - ratio;
        p.v2_i = vi.v_i * f;
        p.v2_o = vi.v_o * f;
        p.i2_i = vi.i_i * f;
        p.i2_o = vi.i_o * f;
        Ok((p, Some(ratio)))
    } else {
        let mut vi = at(vik).ok_or(HISTORY_FAULT)?;
        while (vi.time as f64) < tb {
            vi1 = vi;
            vik += 1;
            vi = at(vik).ok_or(HISTORY_FAULT)?;
        }
        let f = (tb - vi1.time as f64) / (vi.time - vi1.time) as f64;
        p.v2_i = vi1.v_i + f * (vi.v_i - vi1.v_i);
        p.v2_o = vi1.v_o + f * (vi.v_o - vi1.v_o);
        p.i2_i = vi1.i_i + f * (vi.i_i - vi1.i_i);
        p.i2_o = vi1.i_o + f * (vi.i_o - vi1.i_o);
        Ok((p, None))
    }
}

/// What `right_consts_txl` leaves: the two right-hand sides and the
/// `ext` ratio.
struct Rhs {
    ff: f64,
    gg: f64,
    ratio: Option<f64>,
}

/// `right_consts_txl` on the trial line: the history sources of both
/// branch equations, advancing the trial convolutions over `h`.
#[allow(clippy::too_many_arguments)]
fn right_consts_txl(
    tx: &mut TxLine,
    hist: &History,
    t: i32,
    time: i32,
    h: f64,
    h1: f64,
    v: (f64, f64),
    dc: (f64, f64),
) -> Result<Rhs, &'static str> {
    let (in_v, out_v) = v;
    let mut ff = 0.0;
    let mut gg = 0.0;
    if !tx.lsl {
        let mut ff1 = 0.0;
        for i in 0..3 {
            let e = (tx.h1[i].x * h).exp();
            tx.h1e[i] = e;
            ff1 -= tx.h1[i].c * e;
            ff -= tx.h1[i].cnv_i * e;
            gg -= tx.h1[i].cnv_o * e;
        }
        ff += ff1 * h1 * in_v;
        gg += ff1 * h1 * out_v;
    }
    let (p, ratio) = get_pvs(t, time, tx, hist, dc)?;
    if tx.lsl {
        ff = tx.h3_aten * p.v2_o + tx.h2_aten * p.i2_o;
        gg = tx.h3_aten * p.v2_i + tx.h2_aten * p.i2_i;
    } else if tx.if_img {
        for i in 0..4 {
            let tm = &mut tx.h3[i];
            let e = (tm.x * h).exp();
            tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (p.v1_i * e + p.v2_i);
            tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (p.v1_o * e + p.v2_o);
        }
        let (er, ei) = exp_c(tx.h3[4].x, tx.h3[5].x, h);
        let a2 = h1 * tx.h3[4].c;
        let b2 = h1 * tx.h3[5].c;
        let (a, b) = mult_c(tx.h3[4].cnv_i, tx.h3[5].cnv_i, er, ei);
        let (a1, b1) = mult_c(a2, b2, p.v1_i * er + p.v2_i, p.v1_i * ei);
        tx.h3[4].cnv_i = a + a1;
        tx.h3[5].cnv_i = b + b1;
        let (a, b) = mult_c(tx.h3[4].cnv_o, tx.h3[5].cnv_o, er, ei);
        let (a1, b1) = mult_c(a2, b2, p.v1_o * er + p.v2_o, p.v1_o * ei);
        tx.h3[4].cnv_o = a + a1;
        tx.h3[5].cnv_o = b + b1;

        ff += tx.h3_aten * p.v2_o;
        gg += tx.h3_aten * p.v2_i;
        for i in 0..5 {
            ff += tx.h3[i].cnv_o;
            gg += tx.h3[i].cnv_i;
        }
        // The C adds term 4 a second time (2·Re of the pair; term 5
        // holds the imaginary part).
        ff += tx.h3[4].cnv_o;
        gg += tx.h3[4].cnv_i;

        {
            let tm = &mut tx.h2[0];
            let e = (tm.x * h).exp();
            tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (p.i1_i * e + p.i2_i);
            tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (p.i1_o * e + p.i2_o);
        }
        let (er, ei) = exp_c(tx.h2[1].x, tx.h2[2].x, h);
        let a2 = h1 * tx.h2[1].c;
        let b2 = h1 * tx.h2[2].c;
        let (a, b) = mult_c(tx.h2[1].cnv_i, tx.h2[2].cnv_i, er, ei);
        let (a1, b1) = mult_c(a2, b2, p.i1_i * er + p.i2_i, p.i1_i * ei);
        tx.h2[1].cnv_i = a + a1;
        tx.h2[2].cnv_i = b + b1;
        let (a, b) = mult_c(tx.h2[1].cnv_o, tx.h2[2].cnv_o, er, ei);
        let (a1, b1) = mult_c(a2, b2, p.i1_o * er + p.i2_o, p.i1_o * ei);
        tx.h2[1].cnv_o = a + a1;
        tx.h2[2].cnv_o = b + b1;

        ff += tx.h2_aten * p.i2_o + tx.h2[0].cnv_o + 2.0 * tx.h2[1].cnv_o;
        gg += tx.h2_aten * p.i2_i + tx.h2[0].cnv_i + 2.0 * tx.h2[1].cnv_i;
    } else {
        for tm in &mut tx.h3 {
            let e = (tm.x * h).exp();
            tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (p.v1_i * e + p.v2_i);
            tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (p.v1_o * e + p.v2_o);
        }
        ff += tx.h3_aten * p.v2_o;
        gg += tx.h3_aten * p.v2_i;
        for tm in &tx.h3 {
            ff += tm.cnv_o;
            gg += tm.cnv_i;
        }
        for tm in &mut tx.h2 {
            let e = (tm.x * h).exp();
            tm.cnv_i = tm.cnv_i * e + h1 * tm.c * (p.i1_i * e + p.i2_i);
            tm.cnv_o = tm.cnv_o * e + h1 * tm.c * (p.i1_o * e + p.i2_o);
        }
        ff += tx.h2_aten * p.i2_o;
        gg += tx.h2_aten * p.i2_i;
        for tm in &tx.h2 {
            ff += tm.cnv_o;
            gg += tm.cnv_i;
        }
    }
    Ok(Rhs { ff, gg, ratio })
}

impl Device for TxlI {
    fn par_load(&self) -> bool {
        true
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        // `TXLsetup`'s `TSTALLOC` order.
        let (p, n, b1, b2) = (self.pos, self.neg, self.ibr1, self.ibr2);
        self.s = TxlSlots {
            pos_pos: mb.slot(p, p),
            pos_neg: mb.slot(p, n),
            neg_pos: mb.slot(n, p),
            neg_neg: mb.slot(n, n),
            ibr1_pos: mb.slot(b1, p),
            ibr2_neg: mb.slot(b2, n),
            neg_ibr2: mb.slot(n, b2),
            pos_ibr1: mb.slot(p, b1),
            ibr1_ibr1: mb.slot(b1, b1),
            ibr2_ibr2: mb.slot(b2, b2),
            ibr1_neg: mb.slot(b1, n),
            ibr2_pos: mb.slot(b2, p),
            ibr1_ibr2: mb.slot(b1, b2),
            ibr2_ibr1: mb.slot(b2, b1),
        };
    }

    /// `TXLload`.
    fn load(&mut self, ctx: &mut LoadCtx) {
        let h = ctx.delta;
        let time2 = ps(ctx.time);
        let hf = h * 1e12;
        let time = ps(ctx.time - ctx.delta);
        let cond1 = ctx.is_dc();
        let gmin = 0.1 * ctx.gmin;
        for s in self.gmin_slots() {
            ctx.add(s, gmin);
        }
        if cond1 {
            self.restart();
            if ctx.mode == InitMode::Smsig {
                self.smsig_sol = [
                    ctx.sol[self.pos],
                    ctx.sol[self.neg],
                    ctx.sol[self.ibr1],
                    ctx.sol[self.ibr2],
                ];
            }
            self.dc_stamps(ctx);
            return;
        }
        let time = self.advance(time, ctx.sol);
        self.check_step(hf);
        let x = [
            ctx.sol[self.pos],
            ctx.sol[self.neg],
            ctx.sol[self.ibr1],
            ctx.sol[self.ibr2],
        ];
        if let Err(e) = self.companion(time, time2, h, x) {
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

    /// `TXLload` installed as `DEVacLoad`: the transient companion at
    /// `CKTtime = CKTdelta = 0` (what an AC run leaves them at),
    /// stamped into the real part of the matrix, its history sources
    /// into the real right-hand side — with a DC bias on the line
    /// the AC solution carries that constant excitation, as in
    /// ngspice (docs/kspice-lines.md).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let gmin = 0.1 * self.gmin;
        for s in self.gmin_slots() {
            ctx.add_g(s, gmin);
        }
        // The first loop of `TXLload` does nothing at t = 0: the
        // history is empty or ends at t = 0 already.
        let x = self.smsig_sol;
        if self.companion(0, 0, 0.0, x).is_err() {
            // Unreachable at t = 0: the history is never read.
            return;
        }
        for &(s, v) in &self.buf.m {
            ctx.add_g(s, v);
        }
        for &(r, v) in &self.buf.r {
            ctx.rhs_re[r] += v;
        }
    }

    fn max_step(&self) -> Option<f64> {
        self.max_step
    }

    /// Not in the C (`DEVaccept = NULL`): marks that the engine
    /// accepted a timepoint, so the next sample the load appends is a
    /// real one. Under the trapezoid and Gear every sample is; only a
    /// TR-BDF2 step (Dune's profile) has an intermediate stage point,
    /// which the lazy accept of `TXLload` appends without an accept.
    fn accept(&mut self, _ctx: &mut AcceptCtx) {
        self.accepted = true;
        if self.provisional.take().is_some() {
            self.hist.prune(self.tx.head);
        }
    }

    fn fatal(&self) -> Option<&'static str> {
        self.fatal
    }

    #[cfg(test)]
    fn history_len(&self) -> Option<usize> {
        Some(self.hist.q.len())
    }
}
