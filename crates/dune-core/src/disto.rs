//! `.disto` — small-signal distortion analysis (`ckt/distoan.c`,
//! `ckt/cktdisto.c`, `ckt/dkerproc.c`, `ckt/dloadfns.c`,
//! `ni/niditer.c`; Roychowdhury 1988).
//!
//! A Volterra-series method around the DC operating point: the
//! first-order kernels `H1(f1)`, `H1(f2)` are ordinary AC solves
//! driven by the sources' `DISTOF1`/`DISTOF2` inputs; each
//! higher-order kernel is one more complex solve of the same
//! linearized MNA system, at the combination frequency, driven by
//! the device nonlinearities evaluated on the lower-order kernels
//! (`DEVdisto`: Taylor coefficients of every nonlinear branch at the
//! operating point, contracted with the kernels by the `D1n*`/`D1i*`
//! multinomial helpers below). `DkerProc` then rescales the kernels
//! to sinusoid amplitudes.
//!
//! Without `F2OVERF1` the sweep yields the 2nd and 3rd harmonics of
//! f1; with it, the intermodulation products at f1+f2, f1−f2 and
//! 2f1−f2 (f2 held fixed at `F2OVERF1·FSTART`, as the C does to stay
//! compatible with hbsim).

use crate::ac::{AcSweep, ac_columns};
use crate::circuit::Circuit;
use crate::engine::{AcStepType, Engine};
use crate::error::Error;
use crate::matrix::Var;
use crate::options::Options;
use crate::solution::Solution;
use dune_raw::Data;

/// Result of a `.disto` analysis: two plots (`DISTORTION - 2nd
/// harmonic`, `DISTORTION - 3rd harmonic`) without `F2OVERF1`,
/// three (`DISTORTION - IM: f1+f2`, `… f1-f2`, `… 2f1-f2`) with it.
#[derive(Clone, Debug)]
pub struct Disto {
    pub solutions: Vec<Solution>,
    pub notes: Vec<String>,
}

/// The `CKTdisto` modes that load device nonlinearities
/// (`distodef.h`: `D_TWOF1` … `D_2F1MF2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DistoMode {
    /// `D_TWOF1`: second-order kernel at (f1, f1).
    TwoF1,
    /// `D_THRF1`: third-order kernel at (f1, f1, f1).
    ThrF1,
    /// `D_F1PF2`: second-order kernel at (f1, f2).
    F1pF2,
    /// `D_F1MF2`: second-order kernel at (f1, −f2).
    F1mF2,
    /// `D_2F1MF2`: third-order kernel at (f1, f1, −f2).
    TwoF1mF2,
}

impl DistoMode {
    /// `DkerProc`: kernel → sinusoid-amplitude factor.
    pub(crate) fn ker_factor(self) -> f64 {
        match self {
            DistoMode::TwoF1 | DistoMode::ThrF1 => 2.0,
            DistoMode::F1pF2 | DistoMode::F1mF2 => 4.0,
            DistoMode::TwoF1mF2 => 6.0,
        }
    }
}

/// Which source input `CKTdisto` stamps (`D_RHSF1` / `D_RHSF2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DistoInput {
    F1,
    F2,
}

/// The operating-point environment of `DEVdisto(D_SETUP)`
/// (`CKTrhsOld` holds the converged DC solution).
pub(crate) struct DistoSetupCtx<'a> {
    pub sol: &'a [f64],
    /// `CKTgmin`.
    pub gmin: f64,
}

/// The kernels computed so far at the current f1 (`job->r1H1ptr`
/// …): node-indexed real/imaginary parts, `[0]` = ground = 0. A
/// kernel not yet computed at this point is an empty slice.
pub(crate) struct DistoKernels<'a> {
    pub r1h1: &'a [f64],
    pub i1h1: &'a [f64],
    pub r2h11: &'a [f64],
    pub i2h11: &'a [f64],
    pub r1h2: &'a [f64],
    pub i1h2: &'a [f64],
    pub r2h1m2: &'a [f64],
    pub i2h1m2: &'a [f64],
}

/// What `DEVdisto` sees in the load modes: the mode, `CKTomega` (the
/// combination frequency being solved), the kernels and the RHS it
/// accumulates into (`CKTrhs`/`CKTirhs`).
pub(crate) struct DistoCtx<'a> {
    pub mode: DistoMode,
    pub omega: f64,
    /// `job->Domega1`: 2π·f1 at this point (the BJT's excess-phase
    /// delay rotates the kernels by it).
    pub omega1: f64,
    /// `job->Domega2`: 2π·f2 (0 without `F2OVERF1`).
    pub omega2: f64,
    pub h: DistoKernels<'a>,
    pub rhs_re: &'a mut [f64],
    pub rhs_im: &'a mut [f64],
}

impl DistoKernels<'_> {
    /// Branch value `x = v(a) − v(b)` of a kernel pair.
    pub(crate) fn branch(re: &[f64], im: &[f64], a: Var, b: Var) -> (f64, f64) {
        (re[a] - re[b], im[a] - im[b])
    }

    /// The kernels of branch `v(a) − v(b)` that the devices' `disto`
    /// routines read in `mode` (`r1h1x = r1H1[a] − r1H1[b]` …), in
    /// the `DpassStr` layout: `h2` is H1(f2) for `D_F1PF2` and the
    /// conjugate H1(−f2) for `D_F1MF2`/`D_2F1MF2` (the C's
    /// `i1hm2x = −(…)`); `h11` is H2(f1,f1) for `D_THRF1`/`D_2F1MF2`;
    /// `h12` is H2(f1,−f2) for `D_2F1MF2`. Kernels a mode does not
    /// read stay zero.
    pub(crate) fn kern(&self, mode: DistoMode, a: Var, b: Var) -> Kern {
        let mut k = Kern::default();
        (k.r1h1, k.i1h1) = Self::branch(self.r1h1, self.i1h1, a, b);
        match mode {
            DistoMode::TwoF1 => {}
            DistoMode::ThrF1 => {
                (k.r2h11, k.i2h11) = Self::branch(self.r2h11, self.i2h11, a, b);
            }
            DistoMode::F1pF2 => {
                (k.r1h2, k.i1h2) = Self::branch(self.r1h2, self.i1h2, a, b);
            }
            DistoMode::F1mF2 => {
                let (r, i) = Self::branch(self.r1h2, self.i1h2, a, b);
                (k.r1h2, k.i1h2) = (r, -i);
            }
            DistoMode::TwoF1mF2 => {
                let (r, i) = Self::branch(self.r1h2, self.i1h2, a, b);
                (k.r1h2, k.i1h2) = (r, -i);
                (k.r2h11, k.i2h11) = Self::branch(self.r2h11, self.i2h11, a, b);
                (k.h2f1f2, k.ih2f1f2) = Self::branch(self.r2h1m2, self.i2h1m2, a, b);
            }
        }
        k
    }
}

/// The lower-order kernels of one controlling variable, named as in
/// `DpassStr` (`r1h1x`, `i1h1x`, `r1h2x`, …, `h2f1f2x`, `ih2f1f2x`)
/// without the variable suffix. What `h2`/`h11`/`h12` hold depends
/// on the mode (see [`DistoKernels::kern`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Kern {
    pub r1h1: f64,
    pub i1h1: f64,
    pub r1h2: f64,
    pub i1h2: f64,
    pub r2h11: f64,
    pub i2h11: f64,
    pub h2f1f2: f64,
    pub ih2f1f2: f64,
}

impl Kern {
    /// Field-wise sum (the C's `r1h1z = r1h1z + r1h1y` …).
    pub(crate) fn add(&self, o: &Kern) -> Kern {
        Kern {
            r1h1: self.r1h1 + o.r1h1,
            i1h1: self.i1h1 + o.i1h1,
            r1h2: self.r1h2 + o.r1h2,
            i1h2: self.i1h2 + o.i1h2,
            r2h11: self.r2h11 + o.r2h11,
            i2h11: self.i2h11 + o.i2h11,
            h2f1f2: self.h2f1f2 + o.h2f1f2,
            ih2f1f2: self.ih2f1f2 + o.ih2f1f2,
        }
    }

    /// Field-wise difference (the MOS loads' `r1h1x − r1h1z` …).
    pub(crate) fn sub(&self, o: &Kern) -> Kern {
        Kern {
            r1h1: self.r1h1 - o.r1h1,
            i1h1: self.i1h1 - o.i1h1,
            r1h2: self.r1h2 - o.r1h2,
            i1h2: self.i1h2 - o.i1h2,
            r2h11: self.r2h11 - o.r2h11,
            i2h11: self.i2h11 - o.i2h11,
            h2f1f2: self.h2f1f2 - o.h2f1f2,
            ih2f1f2: self.ih2f1f2 - o.ih2f1f2,
        }
    }
}

/// The 16 Taylor coefficients of a three-variable nonlinearity, as
/// the first half of `DpassStr` (`cxx` … `cxyz`; second order
/// `cxx, cyy, czz, cxy, cyz, cxz`, third order `cxxx … cxyz`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DfCoeffs {
    pub cxx: f64,
    pub cyy: f64,
    pub czz: f64,
    pub cxy: f64,
    pub cyz: f64,
    pub cxz: f64,
    pub cxxx: f64,
    pub cyyy: f64,
    pub czzz: f64,
    pub cxxy: f64,
    pub cxxz: f64,
    pub cxyy: f64,
    pub cyyz: f64,
    pub cxzz: f64,
    pub cyzz: f64,
    pub cxyz: f64,
}

/// `DFn2F1`.
pub(crate) fn dfn2f1(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    s2v2f1(c.cxx, x.r1h1, x.i1h1, x.r1h1, x.i1h1)
        + s2v2f1(c.cyy, y.r1h1, y.i1h1, y.r1h1, y.i1h1)
        + s2v2f1(c.czz, z.r1h1, z.i1h1, z.r1h1, z.i1h1)
        + s2v2f1(c.cxy, x.r1h1, x.i1h1, y.r1h1, y.i1h1)
        + s2v2f1(c.cyz, y.r1h1, y.i1h1, z.r1h1, z.i1h1)
        + s2v2f1(c.cxz, x.r1h1, x.i1h1, z.r1h1, z.i1h1)
}

/// `DFi2F1`.
pub(crate) fn dfi2f1(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    s2i2f1(c.cxx, x.r1h1, x.i1h1, x.r1h1, x.i1h1)
        + s2i2f1(c.cyy, y.r1h1, y.i1h1, y.r1h1, y.i1h1)
        + s2i2f1(c.czz, z.r1h1, z.i1h1, z.r1h1, z.i1h1)
        + s2i2f1(c.cxy, x.r1h1, x.i1h1, y.r1h1, y.i1h1)
        + s2i2f1(c.cyz, y.r1h1, y.i1h1, z.r1h1, z.i1h1)
        + s2i2f1(c.cxz, x.r1h1, x.i1h1, z.r1h1, z.i1h1)
}

/// `S2v3F1`/`S2i3F1` of variables `a`, `b`.
fn s23(cab: f64, a: &Kern, b: &Kern, imag: bool) -> f64 {
    let f = if imag { s2i3f1 } else { s2v3f1 };
    f(
        cab, a.r1h1, a.i1h1, b.r1h1, b.i1h1, a.r2h11, a.i2h11, b.r2h11, b.i2h11,
    )
}

/// `S3v3F1`/`S3i3F1` of variables `a`, `b`, `c`.
fn s33(cabc: f64, a: &Kern, b: &Kern, c: &Kern, imag: bool) -> f64 {
    let f = if imag { s3i3f1 } else { s3v3f1 };
    f(cabc, a.r1h1, a.i1h1, b.r1h1, b.i1h1, c.r1h1, c.i1h1)
}

/// `DFn3F1` (`imag = false`) / `DFi3F1` (`imag = true`), the C's
/// partial sums kept in order.
fn df3f1(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern, imag: bool) -> f64 {
    if !imag {
        let mut temp = s23(c.cxx, x, x, false) + s23(c.cyy, y, y, false) + s23(c.czz, z, z, false);
        temp += s23(c.cxy, x, y, false)
            + s23(c.cyz, y, z, false)
            + s23(c.cxz, x, z, false)
            + s33(c.cxxx, x, x, x, false);
        temp += s33(c.cyyy, y, y, y, false)
            + s33(c.czzz, z, z, z, false)
            + s33(c.cxxy, x, x, y, false)
            + s33(c.cxxz, x, x, z, false)
            + s33(c.cxyy, x, y, y, false);
        temp += s33(c.cyyz, y, y, z, false)
            + s33(c.cxzz, x, z, z, false)
            + s33(c.cyzz, y, z, z, false)
            + s33(c.cxyz, x, y, z, false);
        temp
    } else {
        let mut temp = s23(c.cxx, x, x, true)
            + s23(c.cyy, y, y, true)
            + s23(c.czz, z, z, true)
            + s23(c.cxy, x, y, true);
        temp += s23(c.cyz, y, z, true)
            + s23(c.cxz, x, z, true)
            + s33(c.cxxx, x, x, x, true)
            + s33(c.cyyy, y, y, y, true);
        temp += s33(c.czzz, z, z, z, true)
            + s33(c.cxxy, x, x, y, true)
            + s33(c.cxxz, x, x, z, true)
            + s33(c.cxyy, x, y, y, true);
        temp += s33(c.cyyz, y, y, z, true)
            + s33(c.cxzz, x, z, z, true)
            + s33(c.cyzz, y, z, z, true)
            + s33(c.cxyz, x, y, z, true);
        temp
    }
}

/// `DFn3F1`.
pub(crate) fn dfn3f1(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    df3f1(c, x, y, z, false)
}

/// `DFi3F1`.
pub(crate) fn dfi3f1(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    df3f1(c, x, y, z, true)
}

/// `S2vF12`/`S2iF12` of variables `a`, `b`.
fn s2f12(cab: f64, a: &Kern, b: &Kern, imag: bool) -> f64 {
    let f = if imag { s2if12 } else { s2vf12 };
    f(
        cab, a.r1h1, a.i1h1, b.r1h1, b.i1h1, a.r1h2, a.i1h2, b.r1h2, b.i1h2,
    )
}

/// `DFnF12` (`imag = false`) / `DFiF12`, halved like the C.
fn dff12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern, imag: bool) -> f64 {
    let mut temp = s2f12(c.cxx, x, x, imag) + s2f12(c.cyy, y, y, imag) + s2f12(c.czz, z, z, imag);
    temp += s2f12(c.cxy, x, y, imag) + s2f12(c.cyz, y, z, imag) + s2f12(c.cxz, x, z, imag);
    0.5 * temp
}

/// `DFnF12`.
pub(crate) fn dfnf12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    dff12(c, x, y, z, false)
}

/// `DFiF12`.
pub(crate) fn dfif12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    dff12(c, x, y, z, true)
}

/// `S2v2F12`/`S2i2F12` of variables `a`, `b`.
fn s22f12(cab: f64, a: &Kern, b: &Kern, imag: bool) -> f64 {
    let f = if imag { s2i2f12 } else { s2v2f12 };
    f(
        cab, a.r1h1, a.i1h1, b.r1h1, b.i1h1, a.r1h2, a.i1h2, b.r1h2, b.i1h2, a.r2h11, a.i2h11,
        b.r2h11, b.i2h11, a.h2f1f2, a.ih2f1f2, b.h2f1f2, b.ih2f1f2,
    )
}

/// `S3v2F12`/`S3i2F12` of variables `a`, `b`, `c`.
fn s32f12(cabc: f64, a: &Kern, b: &Kern, c: &Kern, imag: bool) -> f64 {
    let f = if imag { s3i2f12 } else { s3v2f12 };
    f(
        cabc, a.r1h1, a.i1h1, b.r1h1, b.i1h1, c.r1h1, c.i1h1, a.r1h2, a.i1h2, b.r1h2, b.i1h2,
        c.r1h2, c.i1h2,
    )
}

/// `DFn2F12` (`imag = false`) / `DFi2F12` (the C's `DpassStr`
/// functions), the partial sums in the C's order, divided by 3.
fn df2f12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern, imag: bool) -> f64 {
    let i = imag;
    let mut temp = s22f12(c.cxx, x, x, i);
    temp += s22f12(c.cyy, y, y, i);
    temp += s22f12(c.czz, z, z, i);
    temp += s22f12(c.cxy, x, y, i);
    temp += s22f12(c.cyz, y, z, i);
    temp += s22f12(c.cxz, x, z, i);
    if !imag {
        temp += s32f12(c.cxxx, x, x, x, i) + s32f12(c.cyyy, y, y, y, i);
        temp += s32f12(c.czzz, z, z, z, i) + s32f12(c.cxxy, x, x, y, i);
        temp += s32f12(c.cxxz, x, x, z, i) + s32f12(c.cxyy, x, y, y, i);
        temp += s32f12(c.cyyz, y, y, z, i) + s32f12(c.cxzz, x, z, z, i);
        temp += s32f12(c.cyzz, y, z, z, i) + s32f12(c.cxyz, x, y, z, i);
    } else {
        temp += s32f12(c.cxxx, x, x, x, i);
        temp += s32f12(c.cyyy, y, y, y, i) + s32f12(c.czzz, z, z, z, i);
        temp += s32f12(c.cxxy, x, x, y, i) + s32f12(c.cxxz, x, x, z, i);
        temp += s32f12(c.cxyy, x, y, y, i) + s32f12(c.cyyz, y, y, z, i);
        temp += s32f12(c.cxzz, x, z, z, i);
        temp += s32f12(c.cyzz, y, z, z, i) + s32f12(c.cxyz, x, y, z, i);
    }
    temp / 3.
}

/// `DFn2F12`.
pub(crate) fn dfn2f12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    df2f12(c, x, y, z, false)
}

/// `DFi2F12`.
pub(crate) fn dfi2f12(c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> f64 {
    df2f12(c, x, y, z, true)
}

/// The `(temp, itemp)` a device's `disto` routine computes for a
/// three-variable memoryless nonlinearity in `mode`: `DFn2F1`/
/// `DFi2F1`, `DFn3F1`/`DFi3F1`, `DFnF12`/`DFiF12` (both IM
/// second-order modes — the caller's kernels already hold H1(f2) or
/// its conjugate) or `DFn2F12`/`DFi2F12`.
pub(crate) fn df_term(mode: DistoMode, c: &DfCoeffs, x: &Kern, y: &Kern, z: &Kern) -> (f64, f64) {
    match mode {
        DistoMode::TwoF1 => (dfn2f1(c, x, y, z), dfi2f1(c, x, y, z)),
        DistoMode::ThrF1 => (dfn3f1(c, x, y, z), dfi3f1(c, x, y, z)),
        DistoMode::F1pF2 | DistoMode::F1mF2 => (dfnf12(c, x, y, z), dfif12(c, x, y, z)),
        DistoMode::TwoF1mF2 => (dfn2f12(c, x, y, z), dfi2f12(c, x, y, z)),
    }
}

/// The one-variable counterpart of [`df_term`]: `D1n2F1`/`D1i2F1`,
/// `D1n3F1`/`D1i3F1`, `D1nF12`/`D1iF12`, `D1n2F12`/`D1i2F12` with
/// second- and third-order coefficients `cxx`, `cxxx`.
pub(crate) fn d1_term(mode: DistoMode, cxx: f64, cxxx: f64, x: &Kern) -> (f64, f64) {
    match mode {
        DistoMode::TwoF1 => (d1n2f1(cxx, x.r1h1, x.i1h1), d1i2f1(cxx, x.r1h1, x.i1h1)),
        DistoMode::ThrF1 => (
            d1n3f1(cxx, cxxx, x.r1h1, x.i1h1, x.r2h11, x.i2h11),
            d1i3f1(cxx, cxxx, x.r1h1, x.i1h1, x.r2h11, x.i2h11),
        ),
        DistoMode::F1pF2 | DistoMode::F1mF2 => (
            d1nf12(cxx, x.r1h1, x.i1h1, x.r1h2, x.i1h2),
            d1if12(cxx, x.r1h1, x.i1h1, x.r1h2, x.i1h2),
        ),
        DistoMode::TwoF1mF2 => (
            d1n2f12(
                cxx, cxxx, x.r1h1, x.i1h1, x.r1h2, x.i1h2, x.r2h11, x.i2h11, x.h2f1f2, x.ih2f1f2,
            ),
            d1i2f12(
                cxx, cxxx, x.r1h1, x.i1h1, x.r1h2, x.i1h2, x.r2h11, x.i2h11, x.h2f1f2, x.ih2f1f2,
            ),
        ),
    }
}

/// A charge term's current: `jω` times the charge's `(temp, itemp)`
/// — the C's `temp = −ω·D…i(…)`, `itemp = ω·D…n(…)`.
pub(crate) fn jomega(omega: f64, (n, i): (f64, f64)) -> (f64, f64) {
    (-omega * i, omega * n)
}

impl DistoCtx<'_> {
    /// Stamp a nonlinear branch current flowing from `p` to `n`
    /// (`CKTrhs[p] −= temp; CKTrhs[n] += temp`, same for `CKTirhs`).
    pub(crate) fn stamp(&mut self, p: Var, n: Var, (temp, itemp): (f64, f64)) {
        self.rhs_re[p] -= temp;
        self.rhs_im[p] -= itemp;
        self.rhs_re[n] += temp;
        self.rhs_im[n] += itemp;
    }
}

// ---------------------------------------------------------------
// `ckt/dloadfns.c`: the multinomial sub-functions (`S…`) and the
// one-variable load functions (`D1…`). Names follow the C's
// renaming convention (`2v` = two-variable term `xy`, `2F12` = two
// F1 plus F2, `v` = real part, `i` = imaginary part), lowercased.
// ---------------------------------------------------------------

/// `S2v2F1`.
pub(crate) fn s2v2f1(cxy: f64, r1h1x: f64, i1h1x: f64, r1h1y: f64, i1h1y: f64) -> f64 {
    cxy * (r1h1x * r1h1y - i1h1x * i1h1y)
}

/// `S2i2F1`.
pub(crate) fn s2i2f1(cxy: f64, r1h1x: f64, i1h1x: f64, r1h1y: f64, i1h1y: f64) -> f64 {
    cxy * (r1h1x * i1h1y + i1h1x * r1h1y)
}

/// `S2v3F1`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2v3f1(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r2h11x: f64,
    i2h11x: f64,
    r2h11y: f64,
    i2h11y: f64,
) -> f64 {
    cxy * (r1h1x * r2h11y - i1h1x * i2h11y + r1h1y * r2h11x - i1h1y * i2h11x)
}

/// `S2i3F1`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2i3f1(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r2h11x: f64,
    i2h11x: f64,
    r2h11y: f64,
    i2h11y: f64,
) -> f64 {
    cxy * (r1h1x * i2h11y + i1h1x * r2h11y + r1h1y * i2h11x + i1h1y * r2h11x)
}

/// `S2vF12`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2vf12(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
) -> f64 {
    cxy * (r1h1x * r1h2y - i1h1x * i1h2y + r1h1y * r1h2x - i1h1y * i1h2x)
}

/// `S2iF12`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2if12(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
) -> f64 {
    cxy * (r1h1x * i1h2y + i1h1x * r1h2y + r1h1y * i1h2x + i1h1y * r1h2x)
}

/// `S2v2F12` (17 arguments in the C).
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2v2f12(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
    r2h11x: f64,
    i2h11x: f64,
    r2h11y: f64,
    i2h11y: f64,
    h2f1f2x: f64,
    ih2f1f2x: f64,
    h2f1f2y: f64,
    ih2f1f2y: f64,
) -> f64 {
    cxy * (2.0 * (r1h1x * h2f1f2y - i1h1x * ih2f1f2y + r1h1y * h2f1f2x - i1h1y * ih2f1f2x)
        + r1h2x * r2h11y
        - i1h2x * i2h11y
        + r1h2y * r2h11x
        - i1h2y * i2h11x)
}

/// `S2i2F12` (17 arguments in the C).
#[allow(clippy::too_many_arguments)]
pub(crate) fn s2i2f12(
    cxy: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
    r2h11x: f64,
    i2h11x: f64,
    r2h11y: f64,
    i2h11y: f64,
    h2f1f2x: f64,
    ih2f1f2x: f64,
    h2f1f2y: f64,
    ih2f1f2y: f64,
) -> f64 {
    cxy * (2.0 * (r1h1x * ih2f1f2y + i1h1x * h2f1f2y + r1h1y * ih2f1f2x + i1h1y * h2f1f2x)
        + r1h2x * i2h11y
        + i1h2x * r2h11y
        + r1h2y * i2h11x
        + i1h2y * r2h11x)
}

/// `S3v3F1`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s3v3f1(
    cxyz: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h1z: f64,
    i1h1z: f64,
) -> f64 {
    cxyz * ((r1h1x * r1h1y - i1h1x * i1h1y) * r1h1z - (i1h1x * r1h1y + r1h1x * i1h1y) * i1h1z)
}

/// `S3i3F1`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn s3i3f1(
    cxyz: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h1z: f64,
    i1h1z: f64,
) -> f64 {
    cxyz * ((r1h1x * r1h1y - i1h1x * i1h1y) * i1h1z + (i1h1x * r1h1y + r1h1x * i1h1y) * r1h1z)
}

/// `S3v2F12` (13 arguments in the C).
#[allow(clippy::too_many_arguments)]
pub(crate) fn s3v2f12(
    cxyz: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h1z: f64,
    i1h1z: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
    r1h2z: f64,
    i1h2z: f64,
) -> f64 {
    cxyz * ((r1h1x * r1h1y - i1h1x * i1h1y) * r1h2z - (i1h1x * r1h1y + r1h1x * i1h1y) * i1h2z
        + (r1h1x * r1h1z - i1h1x * i1h1z) * r1h2y
        - (i1h1x * r1h1z + r1h1x * i1h1z) * i1h2y
        + (r1h1z * r1h1y - i1h1z * i1h1y) * r1h2x
        - (i1h1z * r1h1y + r1h1z * i1h1y) * i1h2x)
}

/// `S3i2F12` (13 arguments in the C).
#[allow(clippy::too_many_arguments)]
pub(crate) fn s3i2f12(
    cxyz: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h1y: f64,
    i1h1y: f64,
    r1h1z: f64,
    i1h1z: f64,
    r1h2x: f64,
    i1h2x: f64,
    r1h2y: f64,
    i1h2y: f64,
    r1h2z: f64,
    i1h2z: f64,
) -> f64 {
    cxyz * ((r1h1x * r1h1y - i1h1x * i1h1y) * i1h2z
        + (i1h1x * r1h1y + r1h1x * i1h1y) * r1h2z
        + (r1h1x * r1h1z - i1h1x * i1h1z) * i1h2y
        + (i1h1x * r1h1z + r1h1x * i1h1z) * r1h2y
        + (r1h1z * r1h1y - i1h1z * i1h1y) * i1h2x
        + (i1h1z * r1h1y + r1h1z * i1h1y) * r1h2x)
}

/// `D1n2F1`: real part of `cxx·H1(f1)²`.
pub(crate) fn d1n2f1(cxx: f64, r1h1x: f64, i1h1x: f64) -> f64 {
    s2v2f1(cxx, r1h1x, i1h1x, r1h1x, i1h1x)
}

/// `D1i2F1`: imaginary part of `cxx·H1(f1)²`.
pub(crate) fn d1i2f1(cxx: f64, r1h1x: f64, i1h1x: f64) -> f64 {
    s2i2f1(cxx, r1h1x, i1h1x, r1h1x, i1h1x)
}

/// `D1n3F1`: real part of `2·cxx·H1·H2(f1,f1) + cxxx·H1³`.
pub(crate) fn d1n3f1(cxx: f64, cxxx: f64, r1h1x: f64, i1h1x: f64, r2h11x: f64, i2h11x: f64) -> f64 {
    s2v3f1(
        cxx, r1h1x, i1h1x, r1h1x, i1h1x, r2h11x, i2h11x, r2h11x, i2h11x,
    ) + s3v3f1(cxxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h1x, i1h1x)
}

/// `D1i3F1`.
pub(crate) fn d1i3f1(cxx: f64, cxxx: f64, r1h1x: f64, i1h1x: f64, r2h11x: f64, i2h11x: f64) -> f64 {
    s2i3f1(
        cxx, r1h1x, i1h1x, r1h1x, i1h1x, r2h11x, i2h11x, r2h11x, i2h11x,
    ) + s3i3f1(cxxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h1x, i1h1x)
}

/// `D1nF12`: real part of `cxx·H1(f1)·H1(f2)`.
pub(crate) fn d1nf12(cxx: f64, r1h1x: f64, i1h1x: f64, r1h2x: f64, i1h2x: f64) -> f64 {
    let temp = s2vf12(cxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x);
    0.5 * temp
}

/// `D1iF12`.
pub(crate) fn d1if12(cxx: f64, r1h1x: f64, i1h1x: f64, r1h2x: f64, i1h2x: f64) -> f64 {
    let temp = s2if12(cxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x);
    0.5 * temp
}

/// `D1n2F12`: real part of the third-order kernel at (f1, f1, f2)
/// (divided by 3: "otherwise we get 3*kernel").
#[allow(clippy::too_many_arguments)]
pub(crate) fn d1n2f12(
    cxx: f64,
    cxxx: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h2x: f64,
    i1h2x: f64,
    r2h11x: f64,
    i2h11x: f64,
    h2f1f2x: f64,
    ih2f1f2x: f64,
) -> f64 {
    let temp = s2v2f12(
        cxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x, r2h11x, i2h11x, r2h11x,
        i2h11x, h2f1f2x, ih2f1f2x, h2f1f2x, ih2f1f2x,
    ) + s3v2f12(
        cxxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x, r1h2x, i1h2x,
    );
    temp / 3.
}

/// `D1i2F12`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn d1i2f12(
    cxx: f64,
    cxxx: f64,
    r1h1x: f64,
    i1h1x: f64,
    r1h2x: f64,
    i1h2x: f64,
    r2h11x: f64,
    i2h11x: f64,
    h2f1f2x: f64,
    ih2f1f2x: f64,
) -> f64 {
    let temp = s2i2f12(
        cxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x, r2h11x, i2h11x, r2h11x,
        i2h11x, h2f1f2x, ih2f1f2x, h2f1f2x, ih2f1f2x,
    ) + s3i2f12(
        cxxx, r1h1x, i1h1x, r1h1x, i1h1x, r1h1x, i1h1x, r1h2x, i1h2x, r1h2x, i1h2x, r1h2x, i1h2x,
    );
    temp / 3.
}

impl Circuit {
    /// Run `.disto DEC|OCT|LIN NP FSTART FSTOP <F2OVERF1>`
    /// (`DISTOan`). The sources' `DISTOF1`/`DISTOF2` values drive
    /// the first-order kernels; their `AC` values play no part.
    pub fn disto(
        &self,
        opts: &Options,
        sweep: AcSweep,
        points: usize,
        fstart: f64,
        fstop: f64,
        f2overf1: Option<f64>,
    ) -> Result<Disto, Error> {
        // `DsetParm`: "Frequency of 0 is invalid".
        let ok = fstart > 0.0 && fstart.is_finite() && fstop > 0.0 && fstop.is_finite();
        if !ok {
            return Err(Error::Build(".disto frequencies are inconsistent".into()));
        }
        if f2overf1.is_some_and(|r| !r.is_finite()) {
            return Err(Error::Build(".disto F2OVERF1 is not finite".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        let step_type = match sweep {
            AcSweep::Decade => AcStepType::Decade,
            AcSweep::Octave => AcStepType::Octave,
            AcSweep::Linear => AcStepType::Linear,
        };
        let plots = engine.disto(opts, step_type, points, fstart, fstop, f2overf1)?;
        let solutions = plots
            .into_iter()
            .map(|(plotname, rows)| {
                let (variables, columns) = ac_columns(&engine.vars, &rows);
                Solution {
                    plotname: plotname.to_string(),
                    variables,
                    data: Data::Complex(columns),
                    scale: Some(0),
                }
            })
            .collect();
        Ok(Disto { solutions, notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex64;

    fn c(re: f64, im: f64) -> Complex64 {
        Complex64::new(re, im)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * (1.0 + a.abs().max(b.abs()))
    }

    /// The real/imaginary helper pairs are the parts of plain
    /// complex products: `D1·2F1 = c·x²`, `D1·F12 = c·x·y`, `D1·3F1
    /// = 2c₂·x·H2 + c₃·x³`, `D1·2F12 = (2c₂(2x·H2(1,2) + y·H2(1,1))
    /// + 3c₃·x²·y)/3`.
    #[test]
    fn dloadfns_are_complex_multinomials() {
        let (cxx, cxxx) = (0.37, -1.9);
        let x = c(0.8, -0.3);
        let y = c(-0.2, 0.65);
        let h11 = c(1.1, 0.4);
        let h12 = c(-0.7, 0.25);

        let v = cxx * x * x;
        assert!(close(d1n2f1(cxx, x.re, x.im), v.re));
        assert!(close(d1i2f1(cxx, x.re, x.im), v.im));

        let v = 0.5 * cxx * (2.0 * x * y);
        assert!(close(d1nf12(cxx, x.re, x.im, y.re, y.im), v.re));
        assert!(close(d1if12(cxx, x.re, x.im, y.re, y.im), v.im));

        let v = cxx * 2.0 * x * h11 + cxxx * x * x * x;
        assert!(close(d1n3f1(cxx, cxxx, x.re, x.im, h11.re, h11.im), v.re));
        assert!(close(d1i3f1(cxx, cxxx, x.re, x.im, h11.re, h11.im), v.im));

        let v = (cxx * (2.0 * (2.0 * x * h12) + 2.0 * y * h11) + cxxx * 3.0 * x * x * y) / 3.0;
        let n = d1n2f12(
            cxx, cxxx, x.re, x.im, y.re, y.im, h11.re, h11.im, h12.re, h12.im,
        );
        let i = d1i2f12(
            cxx, cxxx, x.re, x.im, y.re, y.im, h11.re, h11.im, h12.re, h12.im,
        );
        assert!(close(n, v.re), "{n} vs {}", v.re);
        assert!(close(i, v.im), "{i} vs {}", v.im);
    }

    /// `S3v2F12`/`S3i2F12` are the symmetric three-variable product
    /// `x·y·w + x·z·v + z·y·u` (u, v, w the f2 kernels of x, y, z).
    #[test]
    fn three_variable_f12_term_is_symmetric_product() {
        let (x, y, z) = (c(0.3, 0.1), c(-0.5, 0.2), c(0.7, -0.4));
        let (u, v, w) = (c(0.2, 0.9), c(-0.1, -0.3), c(0.6, 0.05));
        let want = 1.7 * (x * y * w + x * z * v + z * y * u);
        let args = (
            x.re, x.im, y.re, y.im, z.re, z.im, u.re, u.im, v.re, v.im, w.re, w.im,
        );
        let re = s3v2f12(
            1.7, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7, args.8, args.9,
            args.10, args.11,
        );
        let im = s3i2f12(
            1.7, args.0, args.1, args.2, args.3, args.4, args.5, args.6, args.7, args.8, args.9,
            args.10, args.11,
        );
        assert!(close(re, want.re) && close(im, want.im));
    }

    fn kern(h1: Complex64, h2: Complex64, h11: Complex64, h12: Complex64) -> Kern {
        Kern {
            r1h1: h1.re,
            i1h1: h1.im,
            r1h2: h2.re,
            i1h2: h2.im,
            r2h11: h11.re,
            i2h11: h11.im,
            h2f1f2: h12.re,
            ih2f1f2: h12.im,
        }
    }

    /// The three-variable load functions (`DFn*`/`DFi*`) are the
    /// parts of the multinomial expansions of `Σ c_ab·a·b +
    /// Σ c_abc·a·b·c` over the kernels: `2F1` = Σ c_ab x_a x_b,
    /// `3F1` = Σ c_ab (x_a h_b + x_b h_a) + Σ c_abc x_a x_b x_c,
    /// `F12` = ½ Σ c_ab (x_a u_b + x_b u_a), `2F12` = ⅓ [Σ c_ab
    /// (2(x_a w_b + x_b w_a) + u_a h_b + u_b h_a) + Σ c_abc (x_a x_b
    /// u_c + x_a x_c u_b + x_c x_b u_a)] with u the f2 kernel, h =
    /// H2(f1,f1), w = H2(f1,−f2) — each coefficient checked alone.
    #[test]
    fn df_loads_are_three_variable_multinomials() {
        let x = [c(0.8, -0.3), c(-0.2, 0.65), c(0.45, 0.1)];
        let u = [c(0.3, 0.2), c(-0.6, -0.15), c(0.05, 0.9)];
        let h = [c(1.1, 0.4), c(-0.7, 0.25), c(0.2, -0.8)];
        let w = [c(-0.4, 0.3), c(0.9, 0.1), c(-0.25, -0.55)];
        let k: Vec<Kern> = (0..3).map(|i| kern(x[i], u[i], h[i], w[i])).collect();

        // (coefficient setter, index tuple)
        type Set = fn(&mut DfCoeffs, f64);
        let second: [(Set, usize, usize); 6] = [
            (|c, v| c.cxx = v, 0, 0),
            (|c, v| c.cyy = v, 1, 1),
            (|c, v| c.czz = v, 2, 2),
            (|c, v| c.cxy = v, 0, 1),
            (|c, v| c.cyz = v, 1, 2),
            (|c, v| c.cxz = v, 0, 2),
        ];
        let third: [(Set, usize, usize, usize); 10] = [
            (|c, v| c.cxxx = v, 0, 0, 0),
            (|c, v| c.cyyy = v, 1, 1, 1),
            (|c, v| c.czzz = v, 2, 2, 2),
            (|c, v| c.cxxy = v, 0, 0, 1),
            (|c, v| c.cxxz = v, 0, 0, 2),
            (|c, v| c.cxyy = v, 0, 1, 1),
            (|c, v| c.cyyz = v, 1, 1, 2),
            (|c, v| c.cxzz = v, 0, 2, 2),
            (|c, v| c.cyzz = v, 1, 2, 2),
            (|c, v| c.cxyz = v, 0, 1, 2),
        ];
        let cv = 0.73;
        let check = |mode: DistoMode, co: &DfCoeffs, want: Complex64| {
            let (n, i) = df_term(mode, co, &k[0], &k[1], &k[2]);
            assert!(
                close(n, want.re) && close(i, want.im),
                "{mode:?}: ({n}, {i}) vs {want}"
            );
        };
        for (set, a, b) in second {
            let mut co = DfCoeffs::default();
            set(&mut co, cv);
            check(DistoMode::TwoF1, &co, cv * x[a] * x[b]);
            check(DistoMode::ThrF1, &co, cv * (x[a] * h[b] + x[b] * h[a]));
            check(
                DistoMode::F1pF2,
                &co,
                0.5 * cv * (x[a] * u[b] + x[b] * u[a]),
            );
            let v = cv * (2.0 * (x[a] * w[b] + x[b] * w[a]) + u[a] * h[b] + u[b] * h[a]) / 3.0;
            check(DistoMode::TwoF1mF2, &co, v);
        }
        for (set, a, b, d) in third {
            let mut co = DfCoeffs::default();
            set(&mut co, cv);
            check(DistoMode::TwoF1, &co, c(0.0, 0.0));
            check(DistoMode::ThrF1, &co, cv * x[a] * x[b] * x[d]);
            check(DistoMode::F1mF2, &co, c(0.0, 0.0));
            let v = cv * (x[a] * x[b] * u[d] + x[a] * x[d] * u[b] + x[d] * x[b] * u[a]) / 3.0;
            check(DistoMode::TwoF1mF2, &co, v);
        }
    }

    /// The one-variable dispatcher is the `D1*` family, and `jω×`
    /// is a complex multiplication by `jω`.
    #[test]
    fn d1_term_and_jomega() {
        let k = kern(c(0.8, -0.3), c(0.3, 0.2), c(1.1, 0.4), c(-0.4, 0.3));
        let (n, i) = d1_term(DistoMode::ThrF1, 0.37, -1.9, &k);
        assert_eq!(n, d1n3f1(0.37, -1.9, k.r1h1, k.i1h1, k.r2h11, k.i2h11));
        assert_eq!(i, d1i3f1(0.37, -1.9, k.r1h1, k.i1h1, k.r2h11, k.i2h11));
        let z = c(0.3, -0.7);
        let (re, im) = jomega(2.5, (z.re, z.im));
        let want = c(0.0, 2.5) * z;
        assert!(close(re, want.re) && close(im, want.im));
    }

    fn divider(distof2: Option<(f64, f64)>) -> Circuit {
        use crate::circuit::{Resistor, Vsource};
        let mut ckt = Circuit::new("divider");
        ckt.add(Vsource {
            name: "v1".into(),
            nodes: ["1".into(), "0".into()],
            dc: Some(1.0),
            wave: None,
            ac: None,
            distof1: Some((0.1, 0.0)),
            distof2,
            port: None,
        })
        .unwrap();
        for (name, a, b) in [("r1", "1", "2"), ("r2", "2", "0")] {
            ckt.add(Resistor {
                name: name.into(),
                nodes: [a.into(), b.into()],
                value: 1e3,
            })
            .unwrap();
        }
        ckt
    }

    /// A linear circuit has no distortion: every higher-order kernel
    /// is identically zero, and the plots follow `DISTOan`'s naming.
    #[test]
    fn linear_circuit_has_zero_kernels() {
        let opts = Options::default();
        let d = divider(None)
            .disto(&opts, AcSweep::Decade, 2, 1e3, 1e4, None)
            .unwrap();
        let names: Vec<&str> = d.solutions.iter().map(|s| s.plotname.as_str()).collect();
        assert_eq!(
            names,
            ["DISTORTION - 2nd harmonic", "DISTORTION - 3rd harmonic"]
        );
        for sol in &d.solutions {
            assert_eq!(sol.num_points(), 3);
            for v in sol.column_complex("v(2)").unwrap() {
                assert_eq!(*v, Complex64::new(0.0, 0.0));
            }
        }
        let d = divider(Some((0.1, 0.0)))
            .disto(&opts, AcSweep::Linear, 1, 1e3, 2e3, Some(0.9))
            .unwrap();
        assert_eq!(d.solutions.len(), 3);
        assert_eq!(d.solutions[2].plotname, "DISTORTION - IM: 2f1-f2");
    }

    /// `E_NOF2SRC`: F2OVERF1 given but no source has a DISTOF2 input.
    #[test]
    fn f2_wanted_without_f2_source_fails() {
        let err = divider(None)
            .disto(&Options::default(), AcSweep::Decade, 1, 1e3, 1e3, Some(0.5))
            .unwrap_err();
        assert!(err.to_string().contains("f2"), "{err}");
    }

    #[test]
    fn dkerproc_factors() {
        assert_eq!(DistoMode::TwoF1.ker_factor(), 2.0);
        assert_eq!(DistoMode::ThrF1.ker_factor(), 2.0);
        assert_eq!(DistoMode::F1pF2.ker_factor(), 4.0);
        assert_eq!(DistoMode::F1mF2.ker_factor(), 4.0);
        assert_eq!(DistoMode::TwoF1mF2.ker_factor(), 6.0);
    }
}
