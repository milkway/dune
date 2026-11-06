//! Lossy transmission line, Roychowdhury's convolution model (the 3f5
//! `LTRA` device: `dev/ltra/ltraset.c`, `ltratemp.c`, `ltraload.c`,
//! `ltraacct.c`, `ltratrun.c`, `ltraacld.c`, `ltramisc.c`).
//!
//! Each port obeys a branch equation (`LTRAbrEq1/2`, created with
//! `CKTmkVolt`, so the front end and the convergence test treat them
//! as *voltage* unknowns even though they carry the port currents):
//!
//! ```text
//! Y0·v1(t) − i1(t) = Y0·(h1' ∗ v1) + (h2 ∗ i2)(t−T) + Y0·(h3' ∗ v2)(t−T)
//!                    + e^{−βT}·(Y0·v2(t−T) + i2(t−T))
//! ```
//!
//! (and symmetrically for port 2), with `T = l·sqrt(LC)` the delay,
//! `Y0 = sqrt(C/L)`, `α = β = R/2L` (G = 0). The impulse responses
//! (`ltramisc.c`, tilde-functions of the paper):
//!
//! - `h1'(t) = α·e^{−βt}·(I1(αt) − I0(αt))`
//! - `h2(t)  = α²T·e^{−βt}·I1(α·√(t²−T²))/(α·√(t²−T²))`, `t ≥ T`
//! - `h3'(t) = α·e^{−βt}·(t/√(t²−T²)·I1(α√(t²−T²)) − I0(α√(t²−T²)))`
//!
//! are convolved with the piecewise-linear port histories: the
//! coefficient lists (`LTRArlcCoeffsSetup`, `LTRArcCoeffsSetup`) are
//! second differences of the analytic twice-integrated responses on
//! the accepted-timepoint grid (`CKTtimePoints`), recomputed on the
//! first Newton iteration of every timepoint. Delayed values come
//! from quadratic/linear interpolation in that grid. Four regimes:
//! LC (lossless), RLC, RC (`L = G = 0`, erfc kernels) and RG (a
//! frequency-independent attenuator stamped like DC).
//!
//! The line keeps its own copy of the accepted-timepoint list (the
//! C's global `CKTtimePoints`, which `DCtran` only maintains when an
//! LTRA instance exists); each instance stores its port histories.
//! Not ported: `.options trytocompact` history compaction (no corpus
//! deck uses it — the option is reported as ignored), and
//! `LTRAlteCalculate`: its LTE control needs `fullcontrol`/
//! `halfcontrol`, which the 3f5 (and ngspice) parameter tables never
//! accept, so it is unreachable.

use crate::devices::Device;
use crate::disto::DistoSetupCtx;
use crate::integ::TerrCtx;
use crate::matrix::{AcCtx, AcceptCtx, InitMode, LoadCtx, MatrixBuilder, Slot, TRASH, Var};

/// How the delayed port values `v(t−T)`, `i(t−T)` are interpolated
/// from the accepted timepoints (`LININTERP`/`QUADINTERP`/
/// `MIXEDINTERP`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LtraInterp {
    /// `LININTERP`: linear between the two bracketing points.
    Linear,
    /// `QUADINTERP` (the default): quadratic through three points.
    Quadratic,
    /// `MIXEDINTERP`: quadratic, falling back to linear when the
    /// quadratic value leaves the range of the three samples.
    Mixed,
}

/// `.model … ltra(…)` parameters, 3f5 names (`ltra.c` model table,
/// `ltrampar.c`; `spice.txt` §Lossy Transmission Lines). Per-unit-
/// length quantities are multiplied by `LEN`.
#[derive(Clone, Debug)]
pub struct LtraModel {
    /// `R`: resistance per unit length (Ω); unset = 0 with a warning.
    pub r: Option<f64>,
    /// `L`: inductance per unit length (H); unset = 0 with a warning.
    pub l: Option<f64>,
    /// `G`: conductance per unit length (S); unset = 0.
    pub g: Option<f64>,
    /// `C`: capacitance per unit length (F); unset = 0 with a
    /// warning.
    pub c: Option<f64>,
    /// `LEN`: length of the line; required.
    pub len: Option<f64>,
    /// `REL`: relative rate of change of the derivative for breakpoint
    /// setting, default 1. (An output-only parameter in the 3f5
    /// table; ngspice accepts it as input — corpus/KNOWN_DIFFS.md.)
    pub rel: Option<f64>,
    /// `ABS`: absolute rate of change of the derivative for
    /// breakpoint setting, default 1 (same 3f5/ngspice note).
    pub abs: Option<f64>,
    /// `STEPLIMIT` (true, the default) / `NOSTEPLIMIT` (false): limit
    /// the timestep to the line delay `T`. The last flag wins.
    pub steplimit: bool,
    /// `LININTERP`/`QUADINTERP`/`MIXEDINTERP`; `None` = quadratic
    /// (linear under `trytocompact`, which Dune does not implement).
    pub interp: Option<LtraInterp>,
    /// `TRUNCNR`: Newton iterations for the LTE step estimate (only
    /// reachable with LTE control, which the parameter table never
    /// enables; kept for completeness).
    pub truncnr: bool,
    /// `TRUNCDONTCUT`: do not limit the step by the impulse-response
    /// safe step (`LTRAmaxSafeStep`) of RLC lines.
    pub truncdontcut: bool,
    /// `COMPACTREL`: reltol for the straight-line checks; 0 = the
    /// circuit `RELTOL`.
    pub compactrel: f64,
    /// `COMPACTABS`: abstol for the straight-line checks; 0 = the
    /// circuit `ABSTOL`.
    pub compactabs: f64,
}

impl Default for LtraModel {
    fn default() -> Self {
        LtraModel {
            r: None,
            l: None,
            g: None,
            c: None,
            len: None,
            rel: None,
            abs: None,
            steplimit: true,
            interp: None,
            truncnr: false,
            truncdontcut: false,
            compactrel: 0.0,
            compactabs: 0.0,
        }
    }
}

/// `LTRAspecialCase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LtraCase {
    Lc,
    Rlc,
    Rc,
    Rg,
}

/// Model quantities after `LTRAsetup` + `LTRAtemp` (and the RG
/// constants `LTRAload` computes in DC mode, which depend on nothing
/// else).
#[derive(Clone, Debug)]
pub(crate) struct LtraParams {
    pub case: LtraCase,
    resist: f64,
    induct: f64,
    capac: f64,
    length: f64,
    /// `LTRAtd`, the propagation delay.
    pub td: f64,
    imped: f64,
    admit: f64,
    alpha: f64,
    beta: f64,
    /// `e^{−βT}`; 1 for LC, unset (0) for RC/RG like the C.
    attenuation: f64,
    cbyr: f64,
    rclsqr: f64,
    int_h1dash: f64,
    int_h2: f64,
    int_h3dash: f64,
    cosh_lroot_gr: f64,
    r_rs_lr_gr_or_g: f64,
    r_gs_lr_gr_or_r: f64,
    reltol: f64,
    abstol: f64,
    /// `LTRAchopReltol`: never settable (`choprel` is `#ifdef
    /// notdef` in the table), so always 0.
    chop_reltol: f64,
    step_limit: bool,
    interp: LtraInterp,
    trunc_dont_cut: bool,
    max_safe_step: f64,
    /// Setup warnings (`IFerror(ERR_WARNING, …)`).
    pub notes: Vec<String>,
}

impl LtraParams {
    /// `LTRAsetup` (validation, defaults, case classification) and
    /// `LTRAtemp` (derived constants). `reltol`/`abstol` are the
    /// circuit tolerances the straight-line checks default to.
    pub fn new(name: &str, m: &LtraModel, reltol: f64, abstol: f64) -> Result<Self, String> {
        let mut notes = Vec::new();
        for (key, v) in [
            ("r", m.r),
            ("l", m.l),
            ("g", m.g),
            ("c", m.c),
            ("len", m.len),
            ("rel", m.rel),
            ("abs", m.abs),
        ] {
            if let Some(v) = v
                && !v.is_finite()
            {
                return Err(format!(
                    "`{name}`: lossy line parameter {key} is not finite"
                ));
            }
        }
        let resist = match m.r {
            Some(v) => v,
            None => {
                notes.push(format!(
                    "`{name}`: lossy line series resistance not given, assumed zero"
                ));
                0.0
            }
        };
        let st_line_reltol = if m.compactrel == 0.0 {
            reltol
        } else {
            m.compactrel
        };
        let st_line_abstol = if m.compactabs == 0.0 {
            abstol
        } else {
            m.compactabs
        };
        let interp = m.interp.unwrap_or(LtraInterp::Quadratic);
        let conduct = m.g.unwrap_or(0.0);
        let induct = match m.l {
            Some(v) => v,
            None => {
                notes.push(format!(
                    "`{name}`: lossy line series inductance not given, assumed zero"
                ));
                0.0
            }
        };
        // The C reports a missing C as ERR_FATAL but carries on with
        // zero (no `return`), so it is a warning in effect.
        let capac = match m.c {
            Some(v) => v,
            None => {
                notes.push(format!(
                    "`{name}`: lossy line parallel capacitance not given, assumed zero"
                ));
                0.0
            }
        };
        let Some(length) = m.len else {
            return Err(format!("`{name}`: lossy line length must be given"));
        };

        // Classification, in the C's order (later tests win, errors
        // return at once).
        let (r, g, l, c) = (resist, conduct, induct, capac);
        let mut case = None;
        if r == 0.0 && g == 0.0 && c != 0.0 && l != 0.0 {
            case = Some(LtraCase::Lc);
        }
        if r != 0.0 && g == 0.0 && c != 0.0 && l != 0.0 {
            case = Some(LtraCase::Rlc);
        }
        if r != 0.0 && g == 0.0 && c != 0.0 && l == 0.0 {
            case = Some(LtraCase::Rc);
        }
        if r != 0.0 && g == 0.0 && c == 0.0 && l != 0.0 {
            return Err(format!("`{name}`: RL line not supported yet"));
        }
        if r != 0.0 && g != 0.0 && c == 0.0 && l == 0.0 {
            case = Some(LtraCase::Rg);
        }
        if g != 0.0 && (c != 0.0 || l != 0.0) {
            return Err(format!(
                "`{name}`: Nonzero G (except RG) line not supported yet"
            ));
        }
        let nonzero = [r, g, l, c].iter().filter(|v| **v != 0.0).count();
        let Some(case) = case.filter(|_| nonzero > 1) else {
            return Err(format!(
                "`{name}`: At least two of R,L,G,C must be specified and nonzero"
            ));
        };

        let mut p = LtraParams {
            case,
            resist,
            induct,
            capac,
            length,
            td: 0.0,
            imped: 0.0,
            admit: 0.0,
            alpha: 0.0,
            beta: 0.0,
            attenuation: 0.0,
            cbyr: 0.0,
            rclsqr: 0.0,
            int_h1dash: 0.0,
            int_h2: 0.0,
            int_h3dash: 0.0,
            cosh_lroot_gr: 0.0,
            r_rs_lr_gr_or_g: 0.0,
            r_gs_lr_gr_or_r: 0.0,
            reltol: m.rel.unwrap_or(1.0),
            abstol: m.abs.unwrap_or(1.0),
            chop_reltol: 0.0,
            step_limit: m.steplimit,
            interp,
            trunc_dont_cut: m.truncdontcut,
            max_safe_step: 0.0,
            notes,
        };

        // `LTRAtemp`.
        match case {
            LtraCase::Lc => {
                p.imped = (l / c).sqrt();
                p.admit = 1.0 / p.imped;
                p.td = (l * c).sqrt() * length;
                p.attenuation = 1.0;
            }
            LtraCase::Rlc => {
                p.imped = (l / c).sqrt();
                p.admit = 1.0 / p.imped;
                p.td = (l * c).sqrt() * length;
                p.alpha = 0.5 * (r / l);
                p.beta = p.alpha;
                p.attenuation = (-p.beta * p.td).exp();
                if p.alpha > 0.0 {
                    p.int_h1dash = -1.0;
                    p.int_h2 = 1.0 - p.attenuation;
                    p.int_h3dash = -p.attenuation;
                }
                // alpha == 0: all zero; alpha < 0: the C leaves them
                // at their calloc'd zero too.
                if !p.trunc_dont_cut {
                    p.max_safe_step =
                        max_safe_step(p.td, p.alpha, p.beta, st_line_reltol, st_line_abstol);
                }
            }
            LtraCase::Rc => {
                p.cbyr = c / r;
                p.rclsqr = r * c * length * length;
                p.int_h1dash = 0.0;
                p.int_h2 = 1.0;
                p.int_h3dash = 0.0;
            }
            LtraCase::Rg => {
                // `LTRAload` under MODEDC.
                let x = length * (r * g).sqrt();
                let em = (-x).exp();
                let ep = x.exp(); // "LTRA warning: may overflow!"
                p.cosh_lroot_gr = 0.5 * (ep + em);
                p.r_rs_lr_gr_or_g = if g <= 1.0e-10 {
                    // hack!
                    length * r
                } else {
                    0.5 * (ep - em) * (r / g).sqrt()
                };
                p.r_gs_lr_gr_or_r = if r <= 1.0e-10 {
                    // hack!
                    length * g
                } else {
                    0.5 * (ep - em) * (g / r).sqrt()
                };
            }
        }
        Ok(p)
    }
}

/// The `LTRAmaxSafeStep` bisection of `LTRAtemp`: shrink
/// `[T, 10T]` towards `T` until `h2` looks straight over it. The C
/// evaluates `h3'` alongside but passes the `h2` triple to
/// `LTRAstraightLineCheck` twice (so only `h2` decides); ported as
/// is. The function evaluations are pure, so the unused `h3'` values
/// are not computed here.
fn max_safe_step(td: f64, alpha: f64, beta: f64, reltol: f64, abstol: f64) -> f64 {
    let mut xbig = td + 9.0 * td; // "hack! ckt is not yet initialised..."
    let xsmall = td;
    let mut xmid = 0.5 * (xbig + xsmall);
    let y1small = rlc_h2(xsmall, td, alpha, beta);
    let maxiter = 50;
    let mut iters = 0;
    loop {
        iters += 1;
        let y1big = rlc_h2(xbig, td, alpha, beta);
        let y1mid = rlc_h2(xmid, td, alpha, beta);
        let one = straight_line_check(xbig, y1big, xmid, y1mid, xsmall, y1small, reltol, abstol);
        let done = one as u32 + one as u32;
        if done == 2 || iters > maxiter {
            break;
        }
        xbig = xmid;
        xmid = 0.5 * (xbig + xsmall);
    }
    xbig - td
}

// ---------------------------------------------------------------------
// ltramisc.c
// ---------------------------------------------------------------------

/// `LTRAquadInterp`: coefficients of the quadratic through
/// `(t1, t2, t3)` evaluated at `t`; `None` when two abscissae
/// coincide (the C returns 1 and leaves them unset).
fn quad_interp(t: f64, t1: f64, t2: f64, t3: f64) -> Option<(f64, f64, f64)> {
    if t == t1 {
        return Some((1.0, 0.0, 0.0));
    }
    if t == t2 {
        return Some((0.0, 1.0, 0.0));
    }
    if t == t3 {
        return Some((0.0, 0.0, 1.0));
    }
    if (t2 - t1) == 0.0 || (t3 - t2) == 0.0 || (t1 - t3) == 0.0 {
        return None;
    }
    let mut f1 = (t - t2) * (t - t3);
    let mut f2 = (t - t1) * (t - t3);
    let mut f3 = (t - t1) * (t - t2);
    f1 /= t1 - t2;
    f2 /= t2 - t1;
    f2 /= t2 - t3;
    f3 /= t2 - t3;
    f1 /= t1 - t3;
    f3 /= t1 - t3;
    Some((f1, f2, f3))
}

/// `LTRAlinInterp`.
fn lin_interp(t: f64, t1: f64, t2: f64) -> Option<(f64, f64)> {
    if t1 == t2 {
        return None;
    }
    if t == t1 {
        return Some((1.0, 0.0));
    }
    if t == t2 {
        return Some((0.0, 1.0));
    }
    let temp = (t - t1) / (t2 - t1);
    Some((1.0 - temp, temp))
}

/// `intlinfunc`: `∫_lo^hi h(τ)dτ` for `h` linear through
/// `(t1, lovalue)`, `(t2, hivalue)`.
fn intlinfunc(lolimit: f64, hilimit: f64, lovalue: f64, hivalue: f64, t1: f64, t2: f64) -> f64 {
    let width = t2 - t1;
    if width == 0.0 {
        return 0.0;
    }
    let m = (hivalue - lovalue) / width;
    (hilimit - lolimit) * lovalue
        + 0.5 * m * ((hilimit - t1) * (hilimit - t1) - (lolimit - t1) * (lolimit - t1))
}

/// `twiceintlinfunc`: `∫_lo^hi ∫_otherlo^τ h(τ')dτ'dτ`, `h` linear as
/// in [`intlinfunc`].
fn twiceintlinfunc(
    lolimit: f64,
    hilimit: f64,
    otherlolimit: f64,
    lovalue: f64,
    hivalue: f64,
    t1: f64,
    t2: f64,
) -> f64 {
    let width = t2 - t1;
    if width == 0.0 {
        return 0.0;
    }
    let m = (hivalue - lovalue) / width;
    let temp1 = hilimit - t1;
    let temp2 = lolimit - t1;
    let temp3 = otherlolimit - t1;
    let mut dummy = lovalue
        * ((hilimit - otherlolimit) * (hilimit - otherlolimit)
            - (lolimit - otherlolimit) * (lolimit - otherlolimit));
    dummy += m
        * ((temp1 * temp1 * temp1 - temp2 * temp2 * temp2) / 3.0
            - temp3 * temp3 * (hilimit - lolimit));
    dummy * 0.5
}

/// `bessI0` (Numerical Recipes polynomial fits).
fn bess_i0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        1.0 + y
            * (3.5156229
                + y * (3.0899424
                    + y * (1.2067492 + y * (0.2659732 + y * (0.360768e-1 + y * 0.45813e-2)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.39894228
                + y * (0.1328592e-1
                    + y * (0.225319e-2
                        + y * (-0.157565e-2
                            + y * (0.916281e-2
                                + y * (-0.2057706e-1
                                    + y * (0.2635537e-1
                                        + y * (-0.1647633e-1 + y * 0.392377e-2))))))))
    }
}

/// `bessI1` (Numerical Recipes).
fn bess_i1(x: f64) -> f64 {
    let ax = x.abs();
    let ans = if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        ax * (0.5
            + y * (0.87890594
                + y * (0.51498869
                    + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3))))))
    } else {
        let y = 3.75 / ax;
        let a = 0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2));
        let a = 0.39894228
            + y * (-0.3988024e-1
                + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * a))));
        a * (ax.exp() / ax.sqrt())
    };
    if x < 0.0 { -ans } else { ans }
}

/// `bessI1xOverX`: `I1(x)/x`, finite at 0.
fn bess_i1_x_over_x(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75) * (x / 3.75);
        0.5 + y
            * (0.87890594
                + y * (0.51498869
                    + y * (0.15084934 + y * (0.2658733e-1 + y * (0.301532e-2 + y * 0.32411e-3)))))
    } else {
        let y = 3.75 / ax;
        let a = 0.2282967e-1 + y * (-0.2895312e-1 + y * (0.1787654e-1 - y * 0.420059e-2));
        let a = 0.39894228
            + y * (-0.3988024e-1
                + y * (-0.362018e-2 + y * (0.163801e-2 + y * (-0.1031555e-1 + y * a))));
        a * (ax.exp() / (ax * ax.sqrt()))
    }
}

/// `LTRArlcH2Func`: `h2(t) = α²T·e^{−βt}·I1(α√(t²−T²))/(α√(t²−T²))`
/// for `t ≥ T`, else 0.
fn rlc_h2(time: f64, t: f64, alpha: f64, beta: f64) -> f64 {
    if alpha == 0.0 {
        return 0.0;
    }
    if time < t {
        return 0.0;
    }
    let besselarg = if time != t {
        alpha * (time * time - t * t).sqrt()
    } else {
        0.0
    };
    let exparg = -beta * time;
    alpha * alpha * t * exparg.exp() * bess_i1_x_over_x(besselarg)
}

/// `LTRAstraightLineCheck`: do the three points lie acceptably close
/// to a straight line? Compares the triangle they enclose with the
/// trapezoid areas under them (an integral-flavoured criterion).
#[allow(clippy::too_many_arguments)]
fn straight_line_check(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    reltol: f64,
    abstol: f64,
) -> bool {
    let quad1 = (y2.abs() + y1.abs()) * 0.5 * (x2 - x1).abs();
    let quad2 = (y3.abs() + y2.abs()) * 0.5 * (x3 - x2).abs();
    let quad3 = (y3.abs() + y1.abs()) * 0.5 * (x3 - x1).abs();
    let tr_area = (quad3 - quad1 - quad2).abs();
    let area = quad1 + quad2;
    area * reltol + abstol > tr_area
}

/// First coefficients and convolution lists for one timepoint
/// (`LTRAh*FirstCoeff`, `LTRAh*Coeffs`, `LTRAauxIndex`).
#[derive(Clone, Debug, Default)]
struct Coeffs {
    h1_first: f64,
    h2_first: f64,
    h3_first: f64,
    h1: Vec<f64>,
    h2: Vec<f64>,
    h3: Vec<f64>,
    aux_index: usize,
}

impl Coeffs {
    fn ensure(&mut self, n: usize) {
        if self.h1.len() < n {
            self.h1.resize(n, 0.0);
            self.h2.resize(n, 0.0);
            self.h3.resize(n, 0.0);
        }
    }
}

/// `LTRArlcCoeffsSetup`: the RLC coefficient lists for `curtime`,
/// assuming piecewise-linear port waveforms between the accepted
/// `timelist[0..=timeindex]`. `h1'` uses its closed-form twice
/// integral; `h2` and `h3'` (which start at `T`) integrate a linear
/// interpolation of the function between grid-aligned lags. The
/// chop tests use `<=` (the RC version uses `<`); with the always-0
/// chop reltol they only stop on an exactly zero coefficient.
///
/// The `i == auxindex` correction of the previous interval's
/// `[lolimit2, hilimit2]` is commented out in the C (`/* if (i ==
/// auxindex) { lolimit2 = T; … } */`); ported as is.
#[allow(clippy::too_many_arguments)]
fn rlc_coeffs(
    co: &mut Coeffs,
    t: f64,
    alpha: f64,
    beta: f64,
    curtime: f64,
    timelist: &[f64],
    timeindex: usize,
    reltol: f64,
) {
    co.ensure(timeindex + 1);

    let auxindex: usize = if t == 0.0 {
        timeindex
    } else if curtime - t <= 0.0 {
        0
    } else {
        let mut exact = false;
        let mut found: isize = -1;
        for i in (0..=timeindex).rev() {
            if curtime - timelist[i] == t {
                exact = true;
                found = i as isize;
                break;
            }
            if curtime - timelist[i] > t {
                found = i as isize;
                break;
            }
        }
        // "LTRAcoeffSetup: i <= 0: some mistake!" — cannot happen
        // (timelist[0] = 0 and curtime > T); clamp instead of
        // indexing before the list like the C would.
        let a = if exact { found - 1 } else { found };
        a.max(0) as usize
    };

    let alphasq_t = alpha * alpha * t;
    let expbeta_t = (-beta * t).exp();

    let (mut h2lovalue1, mut h2hivalue1, mut h2dummy1, mut h2relval) = (0.0, 0.0, 0.0, 0.0);
    let (mut h3hivalue1, mut h3dummy1, mut h3relval) = (0.0, 0.0, 0.0);

    // The first coefficient (the lag `curtime − T`).
    if auxindex != 0 {
        let lolimit1 = t;
        let hilimit1 = curtime - timelist[auxindex];
        let delta1 = hilimit1 - lolimit1;

        h2lovalue1 = rlc_h2(t, t, alpha, beta);
        let besselarg = if hilimit1 > t {
            alpha * (hilimit1 * hilimit1 - t * t).sqrt()
        } else {
            0.0
        };
        let expterm = (-beta * hilimit1).exp();
        let bessi1overxterm = bess_i1_x_over_x(besselarg);
        h2hivalue1 = if alpha == 0.0 || hilimit1 < t {
            0.0
        } else {
            alphasq_t * expterm * bessi1overxterm
        };
        h2dummy1 = twiceintlinfunc(
            lolimit1, hilimit1, lolimit1, h2lovalue1, h2hivalue1, lolimit1, hilimit1,
        ) / delta1;
        co.h2_first = h2dummy1;
        h2relval = (reltol * h2dummy1).abs();

        let h3lovalue1 = 0.0; // E3dash should be consistent with this
        let bessi0term = bess_i0(besselarg);
        h3hivalue1 = if hilimit1 <= t || beta == 0.0 {
            0.0
        } else {
            expterm * bessi0term - expbeta_t
        };
        h3dummy1 = intlinfunc(
            lolimit1, hilimit1, h3lovalue1, h3hivalue1, lolimit1, hilimit1,
        ) / delta1;
        co.h3_first = h3dummy1;
        h3relval = (h3dummy1 * reltol).abs();
    } else {
        co.h2_first = 0.0;
        co.h3_first = 0.0;
    }

    let mut lolimit1 = 0.0;
    let mut hilimit1 = curtime - timelist[timeindex];
    let mut delta1 = hilimit1 - lolimit1;
    let mut exparg = -beta * hilimit1;
    let mut expterm = exparg.exp();

    let h1twice = |hilimit1: f64, exparg: f64, expterm: f64| -> f64 {
        // LTRArlcH1dashTwiceIntFunc(hilimit1, beta), inlined.
        if beta == 0.0 {
            hilimit1
        } else if hilimit1 == 0.0 {
            0.0
        } else {
            (bess_i1(-exparg) + bess_i0(-exparg)) * hilimit1 * expterm - hilimit1
        }
    };
    let mut h1hivalue1 = h1twice(hilimit1, exparg, expterm);
    let mut h1dummy1 = h1hivalue1 / delta1;
    co.h1_first = h1dummy1;
    let h1relval = (h1dummy1 * reltol).abs();

    let (mut doh1, mut doh2, mut doh3) = (true, true, true);
    let (mut lolimit2, mut hilimit2) = (0.0, 0.0);

    for i in (1..=timeindex).rev() {
        if doh1 || doh2 || doh3 {
            lolimit2 = lolimit1;
            hilimit2 = hilimit1;
            lolimit1 = hilimit2;
            hilimit1 = curtime - timelist[i - 1];
            delta1 = timelist[i] - timelist[i - 1];
            exparg = -beta * hilimit1;
            expterm = exparg.exp();
        }

        if doh1 {
            let h1hivalue2 = h1hivalue1;
            let h1dummy2 = h1dummy1;
            let h1lovalue1 = h1hivalue2;
            h1hivalue1 = h1twice(hilimit1, exparg, expterm);
            h1dummy1 = (h1hivalue1 - h1lovalue1) / delta1;
            co.h1[i] = h1dummy1 - h1dummy2;
            if co.h1[i].abs() <= h1relval {
                doh1 = false;
            }
        } else {
            co.h1[i] = 0.0;
        }

        if i <= auxindex {
            let mut besselarg = 0.0;
            if doh2 || doh3 {
                besselarg = if hilimit1 > t {
                    alpha * (hilimit1 * hilimit1 - t * t).sqrt()
                } else {
                    0.0
                };
            }

            if doh2 {
                let h2lovalue2 = h2lovalue1;
                let h2hivalue2 = h2hivalue1;
                let h2dummy2 = h2dummy1;
                h2lovalue1 = h2hivalue2;
                let bessi1overxterm = bess_i1_x_over_x(besselarg);
                h2hivalue1 = if alpha == 0.0 || hilimit1 < t {
                    0.0
                } else {
                    alphasq_t * expterm * bessi1overxterm
                };
                h2dummy1 = twiceintlinfunc(
                    lolimit1, hilimit1, lolimit1, h2lovalue1, h2hivalue1, lolimit1, hilimit1,
                ) / delta1;
                co.h2[i] = h2dummy1 - h2dummy2
                    + intlinfunc(
                        lolimit2, hilimit2, h2lovalue2, h2hivalue2, lolimit2, hilimit2,
                    );
                if co.h2[i].abs() <= h2relval {
                    doh2 = false;
                }
            } else {
                co.h2[i] = 0.0;
            }

            if doh3 {
                let h3hivalue2 = h3hivalue1;
                let h3dummy2 = h3dummy1;
                let h3lovalue1 = h3hivalue2;
                let bessi0term = bess_i0(besselarg);
                h3hivalue1 = if hilimit1 <= t || beta == 0.0 {
                    0.0
                } else {
                    expterm * bessi0term - expbeta_t
                };
                h3dummy1 = intlinfunc(
                    lolimit1, hilimit1, h3lovalue1, h3hivalue1, lolimit1, hilimit1,
                ) / delta1;
                co.h3[i] = h3dummy1 - h3dummy2;
                if co.h3[i].abs() <= h3relval {
                    doh3 = false;
                }
            } else {
                co.h3[i] = 0.0;
            }
        }
    }
    co.aux_index = auxindex;
}

/// `erfc`, the C library's (fdlibm lineage, via `libm`).
fn erfc(x: f64) -> f64 {
    libm::erfc(x)
}

/// `LTRArcCoeffsSetup`: the RC (`L = G = 0`) coefficient lists, from
/// the closed-form twice-integrated responses
/// `H1''(t) = √(4·(C/R)·t/π)`,
/// `H2''(t) = (t + RCl²/2)·erfc(√(RCl²/4t)) − √(t·RCl²/π)·e^{−RCl²/4t}`,
/// `H3''(t) = √(C/R)·(2√(t/π)·e^{−RCl²/4t} − √(RCl²)·erfc(√(RCl²/4t)))`.
/// Chop tests use `<` (never true with the 0 chop reltol).
fn rc_coeffs(
    co: &mut Coeffs,
    cbyr: f64,
    rclsqr: f64,
    curtime: f64,
    timelist: &[f64],
    timeindex: usize,
    reltol: f64,
) {
    use std::f64::consts::PI;
    co.ensure(timeindex + 1);
    let auxindex = timeindex;

    let mut delta1 = curtime - timelist[auxindex];
    let mut hilimit1 = delta1;

    let mut h1hivalue1 = (4.0 * cbyr * hilimit1 / PI).sqrt();
    let mut h1dummy1 = h1hivalue1 / delta1;
    co.h1_first = h1dummy1;
    let h1relval = (h1dummy1 * reltol).abs();

    let mut temp = rclsqr / (4.0 * hilimit1);
    let mut temp2 = if temp >= 100.0 {
        0.0
    } else {
        erfc(temp.sqrt())
    };
    let mut temp3 = (-temp).exp();
    let temp4 = rclsqr.sqrt();
    let temp5 = cbyr.sqrt();

    let mut h2hivalue1 = if hilimit1 != 0.0 {
        (hilimit1 + rclsqr * 0.5) * temp2 - (hilimit1 * rclsqr / PI).sqrt() * temp3
    } else {
        0.0
    };
    let mut h2dummy1 = h2hivalue1 / delta1;
    co.h2_first = h2dummy1;
    let h2relval = (h2dummy1 * reltol).abs();

    let h3of = |hilimit1: f64, temp2: f64, temp3: f64| -> f64 {
        if hilimit1 != 0.0 {
            let temp = 2.0 * (hilimit1 / PI).sqrt() * temp3 - temp4 * temp2;
            temp5 * temp
        } else {
            0.0
        }
    };
    let mut h3hivalue1 = h3of(hilimit1, temp2, temp3);
    let mut h3dummy1 = h3hivalue1 / delta1;
    co.h3_first = h3dummy1;
    let h3relval = (h3dummy1 * reltol).abs();

    let (mut doh1, mut doh2, mut doh3) = (true, true, true);
    for i in (1..=auxindex).rev() {
        delta1 = timelist[i] - timelist[i - 1];
        hilimit1 = curtime - timelist[i - 1];

        if doh1 {
            let h1hivalue2 = h1hivalue1;
            let h1dummy2 = h1dummy1;
            let h1lovalue1 = h1hivalue2;
            h1hivalue1 = (4.0 * cbyr * hilimit1 / PI).sqrt();
            h1dummy1 = (h1hivalue1 - h1lovalue1) / delta1;
            co.h1[i] = h1dummy1 - h1dummy2;
            if co.h1[i].abs() < h1relval {
                doh1 = false;
            }
        } else {
            co.h1[i] = 0.0;
        }

        if doh2 || doh3 {
            temp = rclsqr / (4.0 * hilimit1);
            temp2 = if temp >= 100.0 {
                0.0
            } else {
                erfc(temp.sqrt())
            };
            temp3 = (-temp).exp();
        }

        if doh2 {
            let h2hivalue2 = h2hivalue1;
            let h2dummy2 = h2dummy1;
            let h2lovalue1 = h2hivalue2;
            h2hivalue1 = if hilimit1 != 0.0 {
                (hilimit1 + rclsqr * 0.5) * temp2 - (hilimit1 * rclsqr / PI).sqrt() * temp3
            } else {
                0.0
            };
            h2dummy1 = (h2hivalue1 - h2lovalue1) / delta1;
            co.h2[i] = h2dummy1 - h2dummy2;
            if co.h2[i].abs() < h2relval {
                doh2 = false;
            }
        } else {
            co.h2[i] = 0.0;
        }

        if doh3 {
            let h3hivalue2 = h3hivalue1;
            let h3dummy2 = h3dummy1;
            let h3lovalue1 = h3hivalue2;
            h3hivalue1 = h3of(hilimit1, temp2, temp3);
            h3dummy1 = (h3hivalue1 - h3lovalue1) / delta1;
            co.h3[i] = h3dummy1 - h3dummy2;
            if co.h3[i].abs() < h3relval {
                doh3 = false;
            }
        } else {
            co.h3[i] = 0.0;
        }
    }
    co.aux_index = auxindex;
}

// ---------------------------------------------------------------------
// The instance
// ---------------------------------------------------------------------

/// Matrix positions of `LTRAsetup` (`TSTALLOC` list, in order).
#[derive(Clone, Copy)]
struct Slots {
    ibr1_pos1: Slot,
    ibr1_neg1: Slot,
    ibr1_pos2: Slot,
    ibr1_neg2: Slot,
    ibr1_ibr1: Slot,
    ibr1_ibr2: Slot,
    ibr2_pos1: Slot,
    ibr2_neg1: Slot,
    ibr2_pos2: Slot,
    ibr2_neg2: Slot,
    ibr2_ibr1: Slot,
    ibr2_ibr2: Slot,
    pos1_ibr1: Slot,
    neg1_ibr1: Slot,
    pos2_ibr2: Slot,
    neg2_ibr2: Slot,
}

const NO_SLOTS: Slots = Slots {
    ibr1_pos1: TRASH,
    ibr1_neg1: TRASH,
    ibr1_pos2: TRASH,
    ibr1_neg2: TRASH,
    ibr1_ibr1: TRASH,
    ibr1_ibr2: TRASH,
    ibr2_pos1: TRASH,
    ibr2_neg1: TRASH,
    ibr2_pos2: TRASH,
    ibr2_neg2: TRASH,
    ibr2_ibr1: TRASH,
    ibr2_ibr2: TRASH,
    pos1_ibr1: TRASH,
    neg1_ibr1: TRASH,
    pos2_ibr2: TRASH,
    neg2_ibr2: TRASH,
};

/// Delayed port values `v1(t−T)`, `i1(t−T)`, `v2(t−T)`, `i2(t−T)`.
#[derive(Clone, Copy, Default)]
struct Delayed {
    v1: f64,
    i1: f64,
    v2: f64,
    i2: f64,
}

/// One `O` line.
pub(crate) struct LtraI {
    p: LtraParams,
    pos1: Var,
    neg1: Var,
    pos2: Var,
    neg2: Var,
    br1: Var,
    br2: Var,
    slots: Slots,
    /// `LTRAinput1/2`: the history part of each branch equation.
    input1: f64,
    input2: f64,
    /// `LTRAinitVolt1/2`, `LTRAinitCur1/2`: the operating point.
    init_v1: f64,
    init_i1: f64,
    init_v2: f64,
    init_i2: f64,
    /// `CKTtimePoints[0..=CKTtimeIndex]` and the port histories
    /// `LTRAv1/i1/v2/i2` on it.
    times: Vec<f64>,
    v1: Vec<f64>,
    i1: Vec<f64>,
    v2: Vec<f64>,
    i2: Vec<f64>,
    co: Coeffs,
    /// `CKTgmin` of the last DC load, for the RG stamps in `.ac`.
    gmin: f64,
}

impl LtraI {
    /// `nodes` = (P1+, P1−, P2+, P2−); `br` = the `#i1`/`#i2`
    /// equations.
    pub fn new(p: LtraParams, nodes: [Var; 4], br: [Var; 2]) -> Self {
        LtraI {
            p,
            pos1: nodes[0],
            neg1: nodes[1],
            pos2: nodes[2],
            neg2: nodes[3],
            br1: br[0],
            br2: br[1],
            slots: NO_SLOTS,
            input1: 0.0,
            input2: 0.0,
            init_v1: 0.0,
            init_i1: 0.0,
            init_v2: 0.0,
            init_i2: 0.0,
            times: Vec::new(),
            v1: Vec::new(),
            i1: Vec::new(),
            v2: Vec::new(),
            i2: Vec::new(),
            co: Coeffs::default(),
            gmin: 0.0,
        }
    }

    /// The DC / RG stamps of `LTRAload` (`MODEDC`, or the RG case at
    /// any time).
    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        let s = self.slots;
        let p = &self.p;
        match p.case {
            LtraCase::Rg => {
                ctx.add(s.ibr1_pos1, 1.0);
                ctx.add(s.ibr1_neg1, -1.0);
                ctx.add(s.ibr1_pos2, -p.cosh_lroot_gr);
                ctx.add(s.ibr1_neg2, p.cosh_lroot_gr);
                ctx.add(s.ibr1_ibr2, (1.0 + ctx.gmin) * p.r_rs_lr_gr_or_g);

                ctx.add(s.ibr2_ibr2, p.cosh_lroot_gr);
                ctx.add(s.ibr2_pos2, -(1.0 + ctx.gmin) * p.r_gs_lr_gr_or_r);
                ctx.add(s.ibr2_neg2, (1.0 + ctx.gmin) * p.r_gs_lr_gr_or_r);
                ctx.add(s.ibr2_ibr1, 1.0);

                ctx.add(s.pos1_ibr1, 1.0);
                ctx.add(s.neg1_ibr1, -1.0);
                ctx.add(s.pos2_ibr2, 1.0);
                ctx.add(s.neg2_ibr2, -1.0);
            }
            LtraCase::Lc | LtraCase::Rlc | LtraCase::Rc => {
                // "load a simple resistor": i1 + i2 = 0 and
                // v(P1+) − v(P2+) = R·len·i1 — the C ignores the
                // negative nodes here (ngspice too).
                ctx.add(s.pos1_ibr1, 1.0);
                ctx.add(s.neg1_ibr1, -1.0);
                ctx.add(s.pos2_ibr2, 1.0);
                ctx.add(s.neg2_ibr2, -1.0);

                ctx.add(s.ibr1_ibr1, 1.0);
                ctx.add(s.ibr1_ibr2, 1.0);
                ctx.add(s.ibr2_pos1, 1.0);
                ctx.add(s.ibr2_pos2, -1.0);
                ctx.add(s.ibr2_ibr1, -p.resist * p.length);
            }
        }
        self.input1 = 0.0;
        self.input2 = 0.0;
    }

    /// The delayed-timepoint search and interpolation of `LTRAload`
    /// (LC/RLC, `tdover`). `None` is the C's E_INTERN "cannot find
    /// delayed timepoint" (the step is then cut like a failed
    /// Newton solve).
    fn delayed(&self, time: f64) -> Option<Delayed> {
        let td = self.p.td;
        let time_index = self.times.len() - 1;
        let target = time - td;
        let mut i: isize = time_index as isize;
        while i >= 0 {
            if self.times[i as usize] < target {
                break;
            }
            i -= 1;
        }
        if i == time_index as isize {
            i -= 1;
        }
        if i < 0 {
            return None;
        }
        let isaved = i as usize;
        let t2 = self.times[isaved];
        let t3 = self.times[isaved + 1];
        let interp = self.p.interp;
        let quad_ok = isaved != 0 && matches!(interp, LtraInterp::Quadratic | LtraInterp::Mixed);
        let mut qf = (0.0, 0.0, 0.0);
        if quad_ok {
            let t1 = self.times[isaved - 1];
            qf = quad_interp(target, t1, t2, t3).unwrap_or((0.0, 0.0, 0.0));
        }
        let mut lf = (0.0, 0.0);
        if isaved == 0 || matches!(interp, LtraInterp::Mixed | LtraInterp::Linear) {
            lf = lin_interp(target, t2, t3).unwrap_or((0.0, 0.0));
        }
        let one = |h: &[f64]| -> f64 {
            let mut d = 0.0;
            let (mut max, mut min) = (0.0, 0.0);
            if quad_ok {
                let (a, b, c) = (h[isaved - 1], h[isaved], h[isaved + 1]);
                d = a * qf.0 + b * qf.1 + c * qf.2;
                max = f64::max(f64::max(a, b), c);
                min = f64::min(f64::min(a, b), c);
            }
            if interp == LtraInterp::Linear || isaved == 0 || (quad_ok && (d > max || d < min)) {
                // QUADINTERP keeps the out-of-range quadratic value
                // (the C only prints a debug warning).
                if !(quad_ok && interp == LtraInterp::Quadratic) {
                    d = h[isaved] * lf.0 + h[isaved + 1] * lf.1;
                }
            }
            d
        };
        Some(Delayed {
            v1: one(&self.v1),
            i1: one(&self.i1),
            v2: one(&self.v2),
            i2: one(&self.i2),
        })
    }

    /// The `MODEINITPRED | MODEINITTRAN` part of `LTRAload` for
    /// LC/RLC: the history inputs of both branch equations.
    fn inputs_lc_rlc(&mut self, del: Option<Delayed>) {
        let p = &self.p;
        let time_index = self.times.len() - 1;
        let (mut in1, mut in2) = (0.0, 0.0);
        if p.case == LtraCase::Rlc {
            let co = &self.co;
            // Convolution of h1' with v1 and v2.
            let (mut d1, mut d2) = (0.0, 0.0);
            for i in (1..=time_index).rev() {
                if co.h1[i] != 0.0 {
                    d1 += co.h1[i] * (self.v1[i] - self.init_v1);
                    d2 += co.h1[i] * (self.v2[i] - self.init_v2);
                }
            }
            d1 += self.init_v1 * p.int_h1dash;
            d2 += self.init_v2 * p.int_h1dash;
            d1 -= self.init_v1 * co.h1_first;
            d2 -= self.init_v2 * co.h1_first;
            in1 -= d1 * p.admit;
            in2 -= d2 * p.admit;

            // Convolution of h2 with i2 and i1.
            let (mut d1, mut d2) = (0.0, 0.0);
            if let Some(dl) = del {
                d1 = (dl.i2 - self.init_i2) * co.h2_first;
                d2 = (dl.i1 - self.init_i1) * co.h2_first;
                for i in (1..=co.aux_index).rev() {
                    if co.h2[i] != 0.0 {
                        d1 += co.h2[i] * (self.i2[i] - self.init_i2);
                        d2 += co.h2[i] * (self.i1[i] - self.init_i1);
                    }
                }
            }
            d1 += self.init_i2 * p.int_h2;
            d2 += self.init_i1 * p.int_h2;
            in1 += d1;
            in2 += d2;

            // Convolution of h3' with v2 and v1.
            let (mut d1, mut d2) = (0.0, 0.0);
            if let Some(dl) = del {
                d1 = (dl.v2 - self.init_v2) * co.h3_first;
                d2 = (dl.v1 - self.init_v1) * co.h3_first;
                for i in (1..=co.aux_index).rev() {
                    if co.h3[i] != 0.0 {
                        d1 += co.h3[i] * (self.v2[i] - self.init_v2);
                        d2 += co.h3[i] * (self.v1[i] - self.init_v1);
                    }
                }
            }
            d1 += self.init_v2 * p.int_h3dash;
            d2 += self.init_v1 * p.int_h3dash;
            in1 += p.admit * d1;
            in2 += p.admit * d2;
        }
        // The lossless-like parts (RLC falls through into LC).
        match del {
            None => {
                in1 += p.attenuation * (self.init_v2 * p.admit + self.init_i2);
                in2 += p.attenuation * (self.init_v1 * p.admit + self.init_i1);
            }
            Some(dl) => {
                in1 += p.attenuation * (dl.v2 * p.admit + dl.i2);
                in2 += p.attenuation * (dl.v1 * p.admit + dl.i1);
            }
        }
        self.input1 = in1;
        self.input2 = in2;
    }

    /// The RC inputs of `LTRAload`.
    fn inputs_rc(&mut self) {
        let p = &self.p;
        let co = &self.co;
        let time_index = self.times.len() - 1;
        let (mut in1, mut in2) = (0.0, 0.0);

        let (mut d1, mut d2) = (0.0, 0.0);
        for i in (1..=time_index).rev() {
            if co.h1[i] != 0.0 {
                d1 += co.h1[i] * (self.v1[i] - self.init_v1);
                d2 += co.h1[i] * (self.v2[i] - self.init_v2);
            }
        }
        d1 += self.init_v1 * p.int_h1dash;
        d2 += self.init_v2 * p.int_h1dash;
        d1 -= self.init_v1 * co.h1_first;
        d2 -= self.init_v2 * co.h1_first;
        in1 -= d1;
        in2 -= d2;

        let (mut d1, mut d2) = (0.0, 0.0);
        for i in (1..=time_index).rev() {
            if co.h2[i] != 0.0 {
                d1 += co.h2[i] * (self.i2[i] - self.init_i2);
                d2 += co.h2[i] * (self.i1[i] - self.init_i1);
            }
        }
        d1 += self.init_i2 * p.int_h2;
        d2 += self.init_i1 * p.int_h2;
        d1 -= self.init_i2 * co.h2_first;
        d2 -= self.init_i1 * co.h2_first;
        in1 += d1;
        in2 += d2;

        let (mut d1, mut d2) = (0.0, 0.0);
        for i in (1..=time_index).rev() {
            if co.h3[i] != 0.0 {
                d1 += co.h3[i] * (self.v2[i] - self.init_v2);
                d2 += co.h3[i] * (self.v1[i] - self.init_v1);
            }
        }
        d1 += self.init_v2 * p.int_h3dash;
        d2 += self.init_v1 * p.int_h3dash;
        d1 -= self.init_v2 * co.h3_first;
        d2 -= self.init_v1 * co.h3_first;
        in1 += d1;
        in2 += d2;

        self.input1 = in1;
        self.input2 = in2;
    }
}

impl Device for LtraI {
    /// `LTRAinfo` has no `DEVdisto`: the line is linear.
    fn disto_setup(&mut self, _ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        Ok(())
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (p1, n1, p2, n2, b1, b2) = (
            self.pos1, self.neg1, self.pos2, self.neg2, self.br1, self.br2,
        );
        self.slots = Slots {
            ibr1_pos1: mb.slot(b1, p1),
            ibr1_neg1: mb.slot(b1, n1),
            ibr1_pos2: mb.slot(b1, p2),
            ibr1_neg2: mb.slot(b1, n2),
            ibr1_ibr1: mb.slot(b1, b1),
            ibr1_ibr2: mb.slot(b1, b2),
            ibr2_pos1: mb.slot(b2, p1),
            ibr2_neg1: mb.slot(b2, n1),
            ibr2_pos2: mb.slot(b2, p2),
            ibr2_neg2: mb.slot(b2, n2),
            ibr2_ibr1: mb.slot(b2, b1),
            ibr2_ibr2: mb.slot(b2, b2),
            pos1_ibr1: mb.slot(p1, b1),
            neg1_ibr1: mb.slot(n1, b1),
            pos2_ibr2: mb.slot(p2, b2),
            neg2_ibr2: mb.slot(n2, b2),
        };
        // "so that SMPpreOrder does not screw up on occasion — for
        // example, when one end of the lossy line is hanging".
        mb.slot(p1, p1);
        mb.slot(n1, n1);
        mb.slot(p2, p2);
        mb.slot(n2, n2);
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        if ctx.is_dc() {
            self.gmin = ctx.gmin;
            self.load_dc(ctx);
            ctx.rhs[self.br1] += self.input1;
            ctx.rhs[self.br2] += self.input2;
            return;
        }
        if self.times.is_empty() {
            // The operating point is always accepted (and recorded)
            // before the first transient load.
            ctx.fault = Some("LTRA: transient load before the operating point was stored");
            return;
        }
        let first = matches!(ctx.mode, InitMode::Tran | InitMode::Pred);
        let time_index = self.times.len() - 1;
        let tdover = matches!(self.p.case, LtraCase::Lc | LtraCase::Rlc) && ctx.time > self.p.td;
        let mut del = None;
        if first {
            match self.p.case {
                LtraCase::Rlc => rlc_coeffs(
                    &mut self.co,
                    self.p.td,
                    self.p.alpha,
                    self.p.beta,
                    ctx.time,
                    &self.times,
                    time_index,
                    self.p.chop_reltol,
                ),
                LtraCase::Rc => rc_coeffs(
                    &mut self.co,
                    self.p.cbyr,
                    self.p.rclsqr,
                    ctx.time,
                    &self.times,
                    time_index,
                    self.p.chop_reltol,
                ),
                LtraCase::Lc | LtraCase::Rg => {}
            }
            if tdover {
                match self.delayed(ctx.time) {
                    Some(d) => del = Some(d),
                    None => {
                        ctx.fault = Some("LTRA: cannot find delayed timepoint");
                        return;
                    }
                }
            }
        }

        if self.p.case == LtraCase::Rg {
            self.load_dc(ctx);
            ctx.rhs[self.br1] += self.input1;
            ctx.rhs[self.br2] += self.input2;
            return;
        }

        // First timepoint after zero: record the operating point.
        if ctx.mode == InitMode::Tran && !ctx.uic {
            self.init_v1 = ctx.sol[self.pos1] - ctx.sol[self.neg1];
            self.init_v2 = ctx.sol[self.pos2] - ctx.sol[self.neg2];
            self.init_i1 = ctx.sol[self.br1];
            self.init_i2 = ctx.sol[self.br2];
        }

        // Matrix loading, every call.
        let s = self.slots;
        let p = &self.p;
        match p.case {
            LtraCase::Rlc | LtraCase::Lc => {
                if p.case == LtraCase::Rlc {
                    let d = p.admit * self.co.h1_first;
                    ctx.add(s.ibr1_pos1, d);
                    ctx.add(s.ibr1_neg1, -d);
                    ctx.add(s.ibr2_pos2, d);
                    ctx.add(s.ibr2_neg2, -d);
                }
                ctx.add(s.ibr1_pos1, p.admit);
                ctx.add(s.ibr1_neg1, -p.admit);
                ctx.add(s.ibr1_ibr1, -1.0);
                ctx.add(s.pos1_ibr1, 1.0);
                ctx.add(s.neg1_ibr1, -1.0);

                ctx.add(s.ibr2_pos2, p.admit);
                ctx.add(s.ibr2_neg2, -p.admit);
                ctx.add(s.ibr2_ibr2, -1.0);
                ctx.add(s.pos2_ibr2, 1.0);
                ctx.add(s.neg2_ibr2, -1.0);
            }
            LtraCase::Rc => {
                ctx.add(s.ibr1_ibr1, -1.0);
                ctx.add(s.pos1_ibr1, 1.0);
                ctx.add(s.neg1_ibr1, -1.0);

                ctx.add(s.ibr2_ibr2, -1.0);
                ctx.add(s.pos2_ibr2, 1.0);
                ctx.add(s.neg2_ibr2, -1.0);

                let d = self.co.h1_first;
                ctx.add(s.ibr1_pos1, d);
                ctx.add(s.ibr1_neg1, -d);
                ctx.add(s.ibr2_pos2, d);
                ctx.add(s.ibr2_neg2, -d);

                let d = self.co.h2_first;
                ctx.add(s.ibr1_ibr2, -d);
                ctx.add(s.ibr2_ibr1, -d);

                let d = self.co.h3_first;
                ctx.add(s.ibr1_pos2, -d);
                ctx.add(s.ibr1_neg2, d);
                ctx.add(s.ibr2_pos1, -d);
                ctx.add(s.ibr2_neg1, d);
            }
            LtraCase::Rg => {} // stamped by `load_dc` above
        }

        if first {
            match self.p.case {
                LtraCase::Lc | LtraCase::Rlc => self.inputs_lc_rlc(del),
                LtraCase::Rc => self.inputs_rc(),
                LtraCase::Rg => {}
            }
        }

        ctx.rhs[self.br1] += self.input1;
        ctx.rhs[self.br2] += self.input2;
    }

    /// `LTRAtrunc`: the delay ceiling (`STEPLIMIT`, or a derivative
    /// change of the incident waves under `NOSTEPLIMIT`) and the RLC
    /// impulse-response safe step. The LTE branch needs
    /// `fullcontrol`/`halfcontrol`, never settable, so it is absent.
    fn trunc(&self, t: &TerrCtx, time_step: &mut f64) {
        let p = &self.p;
        if matches!(p.case, LtraCase::Lc | LtraCase::Rlc) {
            if p.step_limit {
                *time_step = time_step.min(p.td);
            } else if self.times.len() >= 2 {
                let idx = self.times.len() - 1;
                let sol = t.sol;
                let a = p.admit;
                let att = p.attenuation;
                let i1 = ((sol[self.pos2] - sol[self.neg2]) * a + sol[self.br2]) * att;
                let i2 = (self.v2[idx] * a + self.i2[idx]) * att;
                let i3 = (self.v2[idx - 1] * a + self.i2[idx - 1]) * att;
                let i4 = ((sol[self.pos1] - sol[self.neg1]) * a + sol[self.br1]) * att;
                let i5 = (self.v1[idx] * a + self.i1[idx]) * att;
                let i6 = (self.v1[idx - 1] * a + self.i1[idx - 1]) * att;
                let h_now = t.time - self.times[idx];
                let h_old = self.times[idx] - self.times[idx - 1];
                let d1 = (i1 - i2) / h_now;
                let d2 = (i2 - i3) / h_old;
                let d3 = (i4 - i5) / h_now;
                let d4 = (i5 - i6) / h_old;
                if (d1 - d2).abs() >= p.reltol * f64::max(d1.abs(), d2.abs()) + p.abstol
                    || (d3 - d4).abs() >= p.reltol * f64::max(d3.abs(), d4.abs()) + p.abstol
                {
                    // "derivative changing - need to schedule after
                    // delay"
                    *time_step = time_step.min(p.td);
                }
            }
        }
        if p.case == LtraCase::Rlc && !p.trunc_dont_cut {
            *time_step = time_step.min(p.max_safe_step);
        }
    }

    /// `LTRAacLoad`: `Y0(s)·V1 − I1 = e^{−λ(s)·l}·(Y0(s)·V2 + I2)` and
    /// its mirror, `Y0 = √((sC+G)/(sL+R))`, `λ = √((sC+G)(sL+R))`.
    /// RG is loaded exactly as in DC.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let p = &self.p;
        let s = self.slots;
        let w = ctx.omega;
        let (y0_r, y0_i, lambda_r, lambda_i) = match p.case {
            LtraCase::Lc => (p.admit, 0.0, 0.0, p.td * w),
            LtraCase::Rlc => {
                let theta = 0.5 * (p.resist / (w * p.induct)).atan();
                let zmag = (p.resist * p.resist + w * w * p.induct * p.induct).sqrt();
                let mag = (w * p.capac / zmag).sqrt();
                let y0_r = mag * theta.cos();
                let y0_i = mag * theta.sin();
                let theta = std::f64::consts::FRAC_PI_2 - theta;
                let mag = mag * zmag;
                (y0_r, y0_i, mag * theta.cos(), mag * theta.sin())
            }
            LtraCase::Rc => {
                let y = (0.5 * w * p.cbyr).sqrt();
                let l = (0.5 * w * p.resist * p.capac).sqrt();
                (y, y, l, l)
            }
            LtraCase::Rg => {
                let g1 = (1.0 + self.gmin) * p.r_rs_lr_gr_or_g;
                let g2 = (1.0 + self.gmin) * p.r_gs_lr_gr_or_r;
                ctx.add_g(s.ibr1_pos1, 1.0);
                ctx.add_g(s.ibr1_neg1, -1.0);
                ctx.add_g(s.ibr1_pos2, -p.cosh_lroot_gr);
                ctx.add_g(s.ibr1_neg2, p.cosh_lroot_gr);
                ctx.add_g(s.ibr1_ibr2, g1);
                ctx.add_g(s.ibr2_ibr2, p.cosh_lroot_gr);
                ctx.add_g(s.ibr2_pos2, -g2);
                ctx.add_g(s.ibr2_neg2, g2);
                ctx.add_g(s.ibr2_ibr1, 1.0);
                ctx.add_g(s.pos1_ibr1, 1.0);
                ctx.add_g(s.neg1_ibr1, -1.0);
                ctx.add_g(s.pos2_ibr2, 1.0);
                ctx.add_g(s.neg2_ibr2, -1.0);
                return;
            }
        };
        let exparg_r = -lambda_r * p.length;
        let exparg_i = -lambda_i * p.length;
        let explambda_r = exparg_r.exp() * exparg_i.cos();
        let explambda_i = exparg_r.exp() * exparg_i.sin();
        let y0exp_r = y0_r * explambda_r - y0_i * explambda_i;
        let y0exp_i = y0_r * explambda_i + y0_i * explambda_r;

        let both = |ctx: &mut AcCtx, sl: Slot, re: f64, im: f64| {
            ctx.add_g(sl, re);
            ctx.add_b(sl, im);
        };
        both(ctx, s.ibr1_pos1, y0_r, y0_i);
        both(ctx, s.ibr1_neg1, -y0_r, -y0_i);
        ctx.add_g(s.ibr1_ibr1, -1.0);
        both(ctx, s.ibr1_pos2, -y0exp_r, -y0exp_i);
        both(ctx, s.ibr1_neg2, y0exp_r, y0exp_i);
        both(ctx, s.ibr1_ibr2, -explambda_r, -explambda_i);

        both(ctx, s.ibr2_pos2, y0_r, y0_i);
        both(ctx, s.ibr2_neg2, -y0_r, -y0_i);
        ctx.add_g(s.ibr2_ibr2, -1.0);
        both(ctx, s.ibr2_pos1, -y0exp_r, -y0exp_i);
        both(ctx, s.ibr2_neg1, y0exp_r, y0exp_i);
        both(ctx, s.ibr2_ibr1, -explambda_r, -explambda_i);

        ctx.add_g(s.pos1_ibr1, 1.0);
        ctx.add_g(s.neg1_ibr1, -1.0);
        ctx.add_g(s.pos2_ibr2, 1.0);
        ctx.add_g(s.neg2_ibr2, -1.0);
    }

    /// `LTRAaccept`: store the accepted point in the history and, when
    /// the slope of an incident wave `(v + Z0·i)·e^{−βT}` changes,
    /// plant a breakpoint one delay after the *previous* point (where
    /// the change happened).
    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if ctx.first {
            // MODEINITTRAN: the operating point starts the history.
            self.times.clear();
            self.v1.clear();
            self.i1.clear();
            self.v2.clear();
            self.i2.clear();
        }
        let sol = ctx.sol;
        self.times.push(ctx.time);
        self.v1.push(sol[self.pos1] - sol[self.neg1]);
        self.v2.push(sol[self.pos2] - sol[self.neg2]);
        self.i1.push(sol[self.br1]);
        self.i2.push(sol[self.br2]);

        let idx = self.times.len() - 1;
        if idx == 0 {
            return;
        }
        let p = &self.p;
        let wave1 = |k: usize| (self.v1[k] + self.i1[k] * p.imped) * p.attenuation;
        let wave2 = |k: usize| (self.v2[k] + self.i2[k] * p.imped) * p.attenuation;
        let v1 = wave1(idx);
        let v2 = wave1(idx - 1);
        let v3 = if idx < 2 { v2 } else { wave1(idx - 2) };
        let v4 = wave2(idx);
        let v5 = wave2(idx - 1);
        let v6 = if idx < 2 { v5 } else { wave2(idx - 2) };
        let h1 = self.times[idx] - self.times[idx - 1];
        let d1 = (v1 - v2) / h1;
        let d3 = (v4 - v5) / h1;
        // At the first step the C divides the zero `v2 − v3` by a
        // difference reaching before the list (`CKTtimePoints[-1]`);
        // the quotient is zero.
        let (d2, d4) = if idx < 2 {
            (0.0, 0.0)
        } else {
            let h2 = self.times[idx - 1] - self.times[idx - 2];
            ((v2 - v3) / h2, (v5 - v6) / h2)
        };
        let check = |a: f64, b: f64, c: f64| {
            f64::max(f64::max(a, b), c) - f64::min(f64::min(a, b), c)
                >= (50.0 * (ctx.reltol / 3.0 * (a + b + c) + ctx.abstol)).abs()
        };
        let test1 = (d1 - d2).abs() >= p.reltol * f64::max(d1.abs(), d2.abs()) + p.abstol
            && check(v1, v2, v3);
        let test2 = (d3 - d4).abs() >= p.reltol * f64::max(d3.abs(), d4.abs()) + p.abstol
            && check(v4, v5, v6);
        if test1 || test2 {
            let t = self.times[idx - 1] + p.td;
            ctx.set_break(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * f64::max(a.abs(), b.abs())
    }

    /// `LTRArlcH1dashFunc` (test reference only; the load uses its
    /// closed-form twice integral).
    fn rlc_h1dash(time: f64, alpha: f64, beta: f64) -> f64 {
        if alpha == 0.0 {
            return 0.0;
        }
        (bess_i1(alpha * time) - bess_i0(alpha * time)) * alpha * (-beta * time).exp()
    }

    /// `LTRArlcH3dashFunc`.
    fn rlc_h3dash(time: f64, t: f64, alpha: f64, beta: f64) -> f64 {
        if alpha == 0.0 || time < t {
            return 0.0;
        }
        let besselarg = if time != t {
            alpha * (time * time - t * t).sqrt()
        } else {
            0.0
        };
        (alpha * time * bess_i1_x_over_x(besselarg) - bess_i0(besselarg))
            * alpha
            * (-beta * time).exp()
    }

    /// `LTRArlcH1dashTwiceIntFunc`.
    fn rlc_h1dash_twice_int(time: f64, beta: f64) -> f64 {
        if beta == 0.0 {
            return time;
        }
        let arg = beta * time;
        if arg == 0.0 {
            return 0.0;
        }
        (bess_i1(arg) + bess_i0(arg)) * time * (-arg).exp() - time
    }

    /// `LTRArlcH3dashIntFunc`.
    fn rlc_h3dash_int(time: f64, t: f64, beta: f64) -> f64 {
        if time <= t || beta == 0.0 {
            return 0.0;
        }
        (-beta * time).exp() * bess_i0(beta * (time * time - t * t).sqrt()) - (-beta * t).exp()
    }

    #[test]
    fn bessel_fits_match_reference_values() {
        // Abramowitz & Stegun table values; NR fits are good to ~1e-7.
        assert!(close(bess_i0(1.0), 1.266_065_877_752_008, 2e-7));
        assert!(close(bess_i1(1.0), 0.565_159_103_992_485, 2e-7));
        assert!(close(bess_i0(5.0), 27.239_871_823_604_44, 2e-7));
        assert!(close(bess_i1(5.0), 24.335_642_142_450_52, 2e-7));
        assert!(close(bess_i1(-5.0), -24.335_642_142_450_52, 2e-7));
        assert!(close(
            bess_i1_x_over_x(5.0),
            24.335_642_142_450_52 / 5.0,
            2e-7
        ));
        assert_eq!(bess_i1_x_over_x(0.0), 0.5);
        assert!(close(bess_i1_x_over_x(2.0), bess_i1(2.0) / 2.0, 1e-15));
    }

    #[test]
    fn linear_integrals_are_exact() {
        // h(τ) = 1 + τ through (0, 1) and (2, 3).
        assert!(close(intlinfunc(0.0, 2.0, 1.0, 3.0, 0.0, 2.0), 4.0, 1e-15));
        assert!(close(intlinfunc(0.5, 1.5, 1.0, 3.0, 0.0, 2.0), 2.0, 1e-15));
        // ∫_0^2 ∫_0^τ (1 + τ') dτ' dτ = ∫_0^2 (τ + τ²/2) dτ = 2 + 4/3.
        let v = twiceintlinfunc(0.0, 2.0, 0.0, 1.0, 3.0, 0.0, 2.0);
        assert!(close(v, 2.0 + 4.0 / 3.0, 1e-14), "{v}");
        // Inner lower limit 1: ∫_1^2 ∫_1^τ (1+τ') = ∫_1^2 (τ−1)(3+τ)/2
        // = 7/6.
        let v = twiceintlinfunc(1.0, 2.0, 1.0, 1.0, 3.0, 0.0, 2.0);
        assert!(close(v, 7.0 / 6.0, 1e-14), "{v}");
    }

    #[test]
    fn interpolation_reproduces_polynomials() {
        let (c1, c2, c3) = quad_interp(1.7, 0.0, 1.0, 3.0).unwrap();
        let f = |t: f64| 2.0 - t + 0.5 * t * t;
        let q = c1 * f(0.0) + c2 * f(1.0) + c3 * f(3.0);
        assert!(close(q, f(1.7), 1e-14));
        assert_eq!(quad_interp(1.0, 0.0, 1.0, 3.0), Some((0.0, 1.0, 0.0)));
        assert_eq!(quad_interp(0.5, 1.0, 1.0, 3.0), None);
        let (l1, l2) = lin_interp(0.25, 0.0, 1.0).unwrap();
        assert_eq!((l1, l2), (0.75, 0.25));
    }

    #[test]
    fn closed_form_integrals_differentiate_to_the_responses() {
        // α = β (G = 0): H1''' = h1', d/dt H3int = h3'.
        let (alpha, t) = (1.1e7, 4.38e-9);
        for &time in &[1e-9, 5e-9, 2e-8] {
            let h = time * 1e-4;
            let f = |x: f64| rlc_h1dash_twice_int(x, alpha);
            let second = (f(time + h) - 2.0 * f(time) + f(time - h)) / (h * h);
            assert!(
                close(second, rlc_h1dash(time, alpha, alpha), 1e-4),
                "{second} vs {}",
                rlc_h1dash(time, alpha, alpha)
            );
            if time > t {
                let g = |x: f64| rlc_h3dash_int(x, t, alpha);
                let first = (g(time + h) - g(time - h)) / (2.0 * h);
                assert!(close(first, rlc_h3dash(time, t, alpha, alpha), 1e-4));
            }
        }
    }

    #[test]
    fn rlc_coefficients_telescope_to_the_last_divided_difference() {
        // With chopping off, h1First + Σ h1[i] is the divided
        // difference of H1'' over the oldest interval.
        let (alpha, td) = (1.1e7, 4.38e-9);
        let times: Vec<f64> = (0..40)
            .map(|k| k as f64 * 0.1e-9 + (k * k) as f64 * 1e-12)
            .collect();
        let n = times.len() - 1;
        let cur = times[n] + 0.07e-9;
        let mut co = Coeffs::default();
        rlc_coeffs(&mut co, td, alpha, alpha, cur, &times, n, 0.0);
        let sum: f64 = co.h1_first + co.h1[1..=n].iter().sum::<f64>();
        let f = |x: f64| rlc_h1dash_twice_int(x, alpha);
        let last = (f(cur - times[0]) - f(cur - times[1])) / (times[1] - times[0]);
        assert!(close(sum, last, 1e-9), "{sum} vs {last}");
        // The first coefficient is H1''(Δ)/Δ.
        let d = cur - times[n];
        assert!(close(co.h1_first, f(d) / d, 1e-15));
        // The lag-T search: the point just older than cur − T.
        assert!(cur - times[co.aux_index] > td);
        assert!(cur - times[co.aux_index + 1] <= td);
        assert!(co.h2_first > 0.0);
    }

    #[test]
    fn rc_coefficients_telescope() {
        let (cbyr, rclsqr) = (3.65e-12 / 0.2, 0.2 * 3.65e-12 * 24.0 * 24.0);
        let times: Vec<f64> = (0..30).map(|k| k as f64 * 0.1e-9).collect();
        let n = times.len() - 1;
        let cur = times[n] + 0.05e-9;
        let mut co = Coeffs::default();
        rc_coeffs(&mut co, cbyr, rclsqr, cur, &times, n, 0.0);
        let f = |x: f64| (4.0 * cbyr * x / std::f64::consts::PI).sqrt();
        let sum: f64 = co.h1_first + co.h1[1..=n].iter().sum::<f64>();
        let last = (f(cur - times[0]) - f(cur - times[1])) / (times[1] - times[0]);
        assert!(close(sum, last, 1e-9));
    }

    /// A matched lossless line: the far end is the near end delayed by
    /// `T`, and the near end sees `Z0` (a 2:1 divider with the source
    /// resistance) until the delay is over and forever after.
    #[test]
    fn matched_lc_line_delays_by_td() {
        use crate::circuit::{Circuit, Ltra, Resistor, Vsource};
        let (l, c, len) = (250e-9, 100e-12, 1.0); // Z0 = 50 Ω, T = 5 ns
        let mut ckt = Circuit::new("matched line");
        ckt.add(Vsource {
            name: "v1".into(),
            nodes: ["1".into(), "0".into()],
            dc: None,
            wave: Some(crate::srcfn::Wave::Pulse(vec![
                0.0, 1.0, 1e-9, 1e-9, 1e-9, 100e-9, 200e-9,
            ])),
            ac: None,
            distof1: None,
            distof2: None,
        })
        .unwrap();
        for (name, a, b) in [("rs", "1", "2"), ("rl", "3", "0")] {
            ckt.add(Resistor {
                name: name.into(),
                nodes: [a.into(), b.into()],
                value: 50.0,
            })
            .unwrap();
        }
        ckt.add(Ltra {
            name: "o1".into(),
            nodes: ["2".into(), "0".into(), "3".into(), "0".into()],
            model: LtraModel {
                r: Some(0.0),
                l: Some(l),
                c: Some(c),
                len: Some(len),
                ..LtraModel::default()
            },
        })
        .unwrap();
        let opts = crate::options::Options::default();
        let tr = ckt.tran(&opts, 0.1e-9, 20e-9, 0.0, None, false).unwrap();
        let t = tr.solution.column("time").unwrap();
        let v2 = tr.solution.column("v(2)").unwrap();
        let v3 = tr.solution.column("v(3)").unwrap();
        assert!(tr.solution.column("v(o1#i1)").is_some());
        let at = |col: &[f64], when: f64| {
            let k = t.iter().position(|&x| x >= when).unwrap();
            let f = (when - t[k - 1]) / (t[k] - t[k - 1]);
            col[k - 1] + f * (col[k] - col[k - 1])
        };
        // Before the wave arrives the far end is quiet.
        assert!(at(v3, 5.5e-9).abs() < 1e-6, "{}", at(v3, 5.5e-9));
        // Mid-ramp: v3(t) = v2(t − 5 ns), both half the source.
        assert!((at(v2, 1.5e-9) - 0.25).abs() < 2e-3, "{}", at(v2, 1.5e-9));
        assert!((at(v3, 6.5e-9) - 0.25).abs() < 2e-3, "{}", at(v3, 6.5e-9));
        assert!((at(v3, 12e-9) - 0.5).abs() < 1e-6);
        assert!((at(v2, 12e-9) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn setup_classifies_and_rejects_like_ltraset() {
        let base = LtraModel {
            r: Some(0.2),
            l: Some(9.13e-9),
            c: Some(3.65e-12),
            len: Some(24.0),
            ..LtraModel::default()
        };
        let p = LtraParams::new("o1", &base, 1e-3, 1e-12).unwrap();
        assert_eq!(p.case, LtraCase::Rlc);
        assert!(close(p.td, (9.13e-9f64 * 3.65e-12).sqrt() * 24.0, 1e-15));
        assert!(p.max_safe_step > 0.0 && p.max_safe_step <= 9.0 * p.td);
        assert!(p.notes.is_empty());

        let lc = LtraModel {
            r: Some(0.0),
            ..base.clone()
        };
        assert_eq!(
            LtraParams::new("o1", &lc, 1e-3, 1e-12).unwrap().case,
            LtraCase::Lc
        );
        let rc = LtraModel {
            l: Some(0.0),
            ..base.clone()
        };
        assert_eq!(
            LtraParams::new("o1", &rc, 1e-3, 1e-12).unwrap().case,
            LtraCase::Rc
        );
        let rl = LtraModel {
            c: Some(0.0),
            ..base.clone()
        };
        assert!(
            LtraParams::new("o1", &rl, 1e-3, 1e-12)
                .unwrap_err()
                .contains("RL line")
        );
        let rlcg = LtraModel {
            g: Some(1e-3),
            ..base.clone()
        };
        assert!(
            LtraParams::new("o1", &rlcg, 1e-3, 1e-12)
                .unwrap_err()
                .contains("Nonzero G")
        );
        let rg = LtraModel {
            g: Some(1e-3),
            l: Some(0.0),
            c: Some(0.0),
            ..base.clone()
        };
        let p = LtraParams::new("o1", &rg, 1e-3, 1e-12).unwrap();
        assert_eq!(p.case, LtraCase::Rg);
        assert!(p.cosh_lroot_gr > 1.0);
        let nolen = LtraModel {
            len: None,
            ..base.clone()
        };
        assert!(
            LtraParams::new("o1", &nolen, 1e-3, 1e-12)
                .unwrap_err()
                .contains("length")
        );
        let one = LtraModel {
            r: None,
            l: None,
            ..base.clone()
        };
        let e = LtraParams::new("o1", &one, 1e-3, 1e-12).unwrap_err();
        assert!(e.contains("At least two"), "{e}");
        let nan = LtraModel {
            r: Some(f64::NAN),
            ..base
        };
        assert!(LtraParams::new("o1", &nan, 1e-3, 1e-12).is_err());
    }
}
