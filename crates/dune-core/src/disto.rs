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
    pub h: DistoKernels<'a>,
    pub rhs_re: &'a mut [f64],
    pub rhs_im: &'a mut [f64],
}

impl DistoKernels<'_> {
    /// Branch value `x = v(a) − v(b)` of a kernel pair.
    pub(crate) fn branch(re: &[f64], im: &[f64], a: Var, b: Var) -> (f64, f64) {
        (re[a] - re[b], im[a] - im[b])
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
