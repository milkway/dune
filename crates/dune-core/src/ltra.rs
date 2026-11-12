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
//! `.options trytocompact` (`ltraacct.c`) is ported: after an accepted
//! point where the last three samples of *every* line's port
//! variables are straight (`LtraI::straight_tail`), the engine drops
//! the middle sample of every line (`LtraI::drop_middle`) — the C's
//! shared `CKTtimePoints` list, kept per instance here in lockstep.
//! Not ported: `LTRAlteCalculate`: its LTE control needs
//! `fullcontrol`/`halfcontrol`, which the 3f5 (and ngspice) parameter
//! tables never accept, so it is unreachable.
//!
//! Dune's opt-in recursive mode (`Options::ltra_fit`, issue #8)
//! replaces the RLC/RC coefficient walk with the exponential sums of
//! [`crate::ltrafit`], advanced once per accepted point; off, every
//! line of the faithful path runs unchanged.

use crate::devices::Device;
use crate::disto::DistoSetupCtx;
use crate::integ::TerrCtx;
use crate::ltrafit::{
    ExpFit, RC_WINDOW_FRACTION, fit_rc, fit_rlc, i0e, i1e, i1e_over_x, rc_integrals, segment,
};
use crate::matrix::{
    AcCtx, AcceptCtx, InitMode, LoadCtx, MatrixBuilder, PeriodicPast, Slot, TRASH, Var,
};

// Every transcendental in this file goes through the pure-Rust `libm`
// crate (fdlibm lineage — what `f64::exp` already lowers to on wasm32),
// not `f64::exp`/`cos`/…, which call the platform C library (Apple's
// libm and glibc disagree in the last ulp). The line's own kernels
// (`e^{−βt}·I0/I1`, erfc) then compute the same bits on every target;
// `sqrt` and the arithmetic are IEEE-exact already. This does NOT make
// the ringing decks (ltra_1/ltra_3) platform-independent: their
// trajectory forks on ulps from anywhere (diode/BJT `exp`, the LU), and
// once the dioload string rings period-2, `accept`'s slope test plants
// a breakpoint at `t+T` on nearly every step, which forces tiny steps
// at `t+T`, which ring and plant more — a cascade faithful to
// ltraacct.c that never ends inside the output guard.
use libm::{atan, cos, exp, expm1, sin};

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
    /// (linear under `.options trytocompact`).
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
    /// The instance name, for notes.
    pub name: String,
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
    /// `LTRAstLineReltol`/`LTRAstLineAbstol` (`COMPACTREL`/
    /// `COMPACTABS`, the circuit tolerances when 0): the straight-line
    /// checks of the safe step and of `trytocompact`.
    st_line_reltol: f64,
    st_line_abstol: f64,
    /// Whether the model card gave an interpolation flag (`LTRAsetup`
    /// only picks the default when none was given).
    interp_given: bool,
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
            name: name.to_string(),
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
            st_line_reltol,
            st_line_abstol,
            interp_given: m.interp.is_some(),
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
                p.attenuation = exp(-p.beta * p.td);
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
                let em = exp(-x);
                let ep = exp(x); // "LTRA warning: may overflow!"
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

    /// The Dune profile's interpolation (D-0052): a model without an
    /// interpolation flag takes linear interpolation instead of the
    /// 3f5's quadratic. Through a corner on the uneven grid the step
    /// control leaves there, the parabola overshoots the delayed
    /// wave; measured against refined runs at the decks' own step,
    /// `ltra_1` is 5.1 rulers off with it and 2.4 with the linear
    /// rule, `ltra_3` 7.3 and 2.9. A flag on the model card wins.
    pub fn default_linear(&mut self) {
        if !self.interp_given {
            self.interp = LtraInterp::Linear;
        }
    }

    /// `.options trytocompact` in `LTRAsetup` (`ltraset.c:55-72`, the
    /// same in the 3f5): a model without an interpolation flag takes
    /// linear interpolation instead of quadratic, with the C's warning.
    pub fn try_to_compact(&mut self) {
        if !self.interp_given {
            self.interp = LtraInterp::Linear;
            self.notes.push(format!(
                "{}: using linear interpolation because trytocompact option specified",
                self.name
            ));
        }
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
        (exp(ax) / ax.sqrt())
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
        a * (exp(ax) / ax.sqrt())
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
        a * (exp(ax) / (ax * ax.sqrt()))
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
    alpha * alpha * t * exp(exparg) * bess_i1_x_over_x(besselarg)
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
    let expbeta_t = exp(-beta * t);

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
        let expterm = exp(-beta * hilimit1);
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
    let mut expterm = exp(exparg);

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
            expterm = exp(exparg);
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

/// `erfc`, the C library's (fdlibm lineage, via `libm` like every
/// transcendental here — see the note at the top of the file).
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
    let mut temp3 = exp(-temp);
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
            temp3 = exp(-temp);
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
    /// Dune's opt-in recursive convolution (`Options::ltra_fit`,
    /// issue #8); off = the C's coefficient walk, untouched.
    fit_on: bool,
    /// Dune's floor on the slope rule (D-0052; with the breakpoint
    /// hygiene): a slope change plants a breakpoint only when the
    /// wave moved by more than `RELTOL` of the largest either wave
    /// has been.
    slope_floor: bool,
    /// Dune's step limit from the interpolation of the delayed waves
    /// ([`LtraI::interp_error`], D-0055).
    interp_control: bool,
    /// Whether the slope rule fired at the last accepted point: the
    /// point before it is a corner of an incident wave.
    corner: bool,
    /// The largest magnitude each incident wave has had (the scale
    /// of [`LtraI::slope_floor`]).
    wave_peak: [f64; 2],
    /// The recursive-mode model and states of the current run (built
    /// at the operating-point accept, when TSTOP is known).
    fit: Option<FitState>,
    /// Why the recursive mode does not apply to this line, if it was
    /// asked for (LC/RG have no history convolution).
    fit_skip: Option<&'static str>,
    /// `.pss` (issue #20): the history of this run is the periodic
    /// past of a shooting interval (see [`LtraI::seed_periodic`]).
    periodic: bool,
    /// Why the periodic past could not be set up, reported at the
    /// first load.
    periodic_fault: Option<&'static str>,
}

/// The recursive model's horizon in a periodic run, in periods: the
/// infinite periodic past is summed in closed form, and lags beyond
/// the horizon (where the fit stops tracking the `τ^{−3/2}` tails) see
/// a zero-mean deviation, so what they miss cancels over each period.
const PERIODIC_HORIZON_PERIODS: f64 = 1000.0;

/// Wave blocks of a state snapshot.
const V1: usize = 0;
const V2: usize = 1;
const I1: usize = 2;
const I2: usize = 3;

/// The recursive-mode state of one line (issue #8).
///
/// For every pole `−x_j` of the [`ExpFit`] and every port wave `w`
/// (`v1`, `v2`, `i1`, `i2`), `S_j[w](t_k) = ∫₀^∞ e^{−x_j σ}·d_w(t_k − σ)dσ`
/// with `d_w = w − w_init` piecewise linear on the accepted points
/// (and `d_w(t_0) = 0`, the C's convention of leaving index 0 out of
/// the sums). It advances once per accepted point, in `accept` — never
/// per Newton iteration or per TR-BDF2 stage. A load at time `t`
/// needs it at the newest point (`h1'`, RC kernels) and at the point
/// just older than `t − T` (`h2`, `h3'`), so a snapshot per accepted
/// point is kept from the oldest one a later load can still reach.
struct FitState {
    fit: ExpFit,
    /// Snapshots, `4·N` values per accepted point from `base` on.
    snaps: Vec<f64>,
    base: usize,
    /// Scratch for `e^{−x_j·lag}`.
    ew: Vec<f64>,
}

impl FitState {
    fn new(fit: ExpFit) -> Self {
        let n = fit.rates.len();
        FitState {
            fit,
            snaps: vec![0.0; 4 * n],
            base: 0,
            ew: vec![0.0; n],
        }
    }

    fn n(&self) -> usize {
        self.fit.rates.len()
    }

    /// The snapshot of history point `k`; `None` if pruned (a bug).
    fn snap(&self, k: usize) -> Option<&[f64]> {
        let n4 = 4 * self.n();
        let off = k.checked_sub(self.base)? * n4;
        self.snaps.get(off..off + n4)
    }

    /// `e^{−x_j·lag}` into the scratch.
    fn exps(&mut self, lag: f64) {
        for (e, x) in self.ew.iter_mut().zip(&self.fit.rates) {
            *e = exp(-x * lag);
        }
    }

    /// `Σ_j r_j[kernel]·e^{−x_j·lag}·S_j[wave]` at snapshot `k`, with
    /// the exponentials already in the scratch.
    fn tail(&self, kernel: usize, k: usize, wave: usize) -> Option<f64> {
        let n = self.n();
        let s = &self.snap(k)?[wave * n..(wave + 1) * n];
        Some(
            self.fit.res[kernel]
                .iter()
                .zip(&self.ew)
                .zip(s)
                .map(|((r, e), s)| r * e * s)
                .sum(),
        )
    }

    /// `.pss`: the states at a period boundary under the infinite
    /// periodic past. With `P` the states one period of the deviation
    /// builds from zero (`segs` = that period's segments), periodicity
    /// `S = e^{−xT}·S + P` gives `S = P/(1 − e^{−xT})` exactly — the
    /// geometric sum over every earlier period. Replaces the snapshots
    /// by that single one.
    fn periodic_start(&mut self, period: f64, segs: &[(f64, [f64; 4], [f64; 4])]) {
        let n = self.n();
        let mut s = vec![0.0; 4 * n];
        for &(h, dn, dp) in segs {
            for j in 0..n {
                let (e, a, b) = segment(self.fit.rates[j] * h);
                for w in 0..4 {
                    let v = &mut s[w * n + j];
                    *v = e * *v + h * (a * dn[w] + b * dp[w]);
                }
            }
        }
        for j in 0..n {
            let g = -expm1(-self.fit.rates[j] * period);
            for w in 0..4 {
                s[w * n + j] /= g;
            }
        }
        self.snaps = s;
        self.base = 0;
    }

    /// Advance one accepted segment of length `h`: `dn`/`dp` are the
    /// four wave deviations at the new and the previous point.
    fn advance(&mut self, h: f64, dn: [f64; 4], dp: [f64; 4]) {
        let n = self.n();
        let start = self.snaps.len() - 4 * n;
        self.snaps.extend_from_within(start..);
        let new = start + 4 * n;
        for j in 0..n {
            let (e, a, b) = segment(self.fit.rates[j] * h);
            for w in 0..4 {
                let s = &mut self.snaps[new + w * n + j];
                *s = e * *s + h * (a * dn[w] + b * dp[w]);
            }
        }
    }

    /// Drop the snapshots older than history point `keep`, in chunks.
    fn prune(&mut self, keep: usize) {
        let dead = keep.saturating_sub(self.base);
        if dead >= 1024 {
            self.snaps.drain(..dead * 4 * self.n());
            self.base += dead;
        }
    }
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
            fit_on: false,
            slope_floor: false,
            corner: false,
            interp_control: false,
            wave_peak: [0.0; 2],
            fit: None,
            fit_skip: None,
            periodic: false,
            periodic_fault: None,
        }
    }

    /// Let the interpolation error of the delayed waves limit the
    /// step (the Dune profile).
    pub fn with_interp_control(mut self, on: bool) -> Self {
        self.interp_control = on;
        self
    }

    /// Turn on the floor of the slope rule (the Dune profile).
    pub fn with_slope_floor(mut self, on: bool) -> Self {
        self.slope_floor = on;
        self
    }

    /// Opt into the recursive convolution (`Options::ltra_fit`).
    pub fn with_fit(mut self, on: bool) -> Self {
        self.fit_on = on;
        self
    }

    /// Build the recursive model for a run to `final_time` (at the
    /// operating-point accept). Lines without a history convolution,
    /// or whose kernels live entirely inside the exact window, keep
    /// the exact path and say why.
    fn start_fit(&mut self, final_time: f64) {
        self.build_fit(final_time.max(f64::MIN_POSITIVE));
    }

    /// The recursive model for lags up to `horizon` (see `start_fit`).
    fn build_fit(&mut self, horizon: f64) {
        self.fit = None;
        self.fit_skip = None;
        let p = &self.p;
        match p.case {
            LtraCase::Lc | LtraCase::Rg => {
                self.fit_skip = Some("no history convolution in this regime");
            }
            LtraCase::Rlc if !(p.alpha > 0.0 && p.alpha.is_finite()) => {
                self.fit_skip = Some("needs R/L > 0");
            }
            LtraCase::Rlc => {
                self.fit = Some(FitState::new(fit_rlc(p.alpha, p.td, horizon)));
            }
            LtraCase::Rc => {
                let w = RC_WINDOW_FRACTION * p.rclsqr;
                if !(w > 0.0 && w.is_finite() && p.cbyr > 0.0 && p.cbyr.is_finite()) {
                    self.fit_skip = Some("needs R, C > 0");
                } else if w >= horizon {
                    self.fit_skip =
                        Some("the diffusion time exceeds the run: exact window covers it");
                } else {
                    self.fit = Some(FitState::new(fit_rc(p.cbyr, p.rclsqr, w, horizon)));
                }
            }
        }
    }

    /// The deviation `w − w_init` of history point `k` (0 at `k = 0`,
    /// the operating point, outside a periodic run).
    fn dev(&self, wave: usize, k: usize) -> f64 {
        if k == 0 && !self.periodic {
            return 0.0;
        }
        match wave {
            V1 => self.v1[k] - self.init_v1,
            V2 => self.v2[k] - self.init_v2,
            I1 => self.i1[k] - self.init_i1,
            _ => self.i2[k] - self.init_i2,
        }
    }

    /// `LTRArlcCoeffsSetup`'s `auxindex` by bisection: the newest
    /// point with `t − t_i ≥ T`, one older if the lag is exactly `T`.
    fn aux_index(&self, time: f64) -> usize {
        let td = self.p.td;
        if td == 0.0 {
            return self.times.len() - 1;
        }
        if time - td <= 0.0 {
            return 0;
        }
        let n = self.times.partition_point(|&t| time - t >= td);
        let Some(found) = n.checked_sub(1) else {
            return 0;
        };
        if time - self.times[found] == td {
            found.saturating_sub(1)
        } else {
            found
        }
    }

    /// The oldest history point whose snapshot a load after the newest
    /// accepted point can still need.
    fn oldest_needed(&self) -> usize {
        let n = self.times.len() - 1;
        let t = self.times[n];
        let k = match self.p.case {
            LtraCase::Rc => {
                let w = self.fit.as_ref().map_or(0.0, |f| f.fit.window);
                self.times
                    .partition_point(|&ti| t - ti >= w)
                    .saturating_sub(1)
            }
            _ => self.aux_index(t),
        };
        k.saturating_sub(1).min(n)
    }

    /// Recursive mode, LC/RLC: the first coefficients (for the matrix)
    /// and the history inputs of `LTRAload` in one pass. `h1'` is the
    /// exact first segment (lags `[0, Δ]`, the C's closed forms) plus
    /// the recursion for older lags; `h2`/`h3'` likewise at `t − T`,
    /// whose first segment keeps the C's linear-in-lag rule.
    fn fit_inputs_lc_rlc(&mut self, time: f64, del: Option<Delayed>) -> Result<(), &'static str> {
        const PRUNED: &str = "LTRA: recursive history state pruned";
        let n = self.times.len() - 1;
        let (mut in1, mut in2) = (0.0, 0.0);
        if self.p.case == LtraCase::Rlc {
            let (alpha, beta, td, admit) = (self.p.alpha, self.p.beta, self.p.td, self.p.admit);
            let fs = self.fit.as_mut().ok_or(PRUNED)?;
            // h1': H''(Δ)/Δ (LTRArlcH1dashTwiceIntFunc, scaled Bessel)
            // and the x_n share H'(Δ) − H''(Δ)/Δ, H'(Δ) = e^{−βΔ}I0(βΔ) − 1.
            let dlt = time - self.times[n];
            let bd = beta * dlt;
            let h1_first = if dlt == 0.0 {
                0.0
            } else {
                (i1e(bd) + i0e(bd)) - 1.0
            };
            let c_n = (i0e(bd) - 1.0) - h1_first;
            fs.exps(dlt);
            let t1 = fs.tail(0, n, V1).ok_or(PRUNED)?;
            let t2 = fs.tail(0, n, V2).ok_or(PRUNED)?;
            self.co.h1_first = h1_first;
            let p = &self.p;
            let mut d1 = c_n * self.dev(V1, n) + t1;
            let mut d2 = c_n * self.dev(V2, n) + t2;
            d1 += self.init_v1 * p.int_h1dash - self.init_v1 * h1_first;
            d2 += self.init_v2 * p.int_h1dash - self.init_v2 * h1_first;
            in1 -= d1 * admit;
            in2 -= d2 * admit;

            let (mut e1, mut e2, mut f1, mut f2) = (0.0, 0.0, 0.0, 0.0);
            if let Some(dl) = del {
                let aux = self.aux_index(time);
                let hi = time - self.times[aux];
                let dp = hi - td;
                let h2lo = rlc_h2(td, td, alpha, beta);
                let z = if hi > td {
                    alpha * (hi * hi - td * td).sqrt()
                } else {
                    0.0
                };
                let damp = exp(-(beta * hi - z));
                let h2hi = if hi < td {
                    0.0
                } else {
                    alpha * alpha * td * damp * i1e_over_x(z)
                };
                let h2_first = twiceintlinfunc(td, hi, td, h2lo, h2hi, td, hi) / dp;
                let c2 = intlinfunc(td, hi, h2lo, h2hi, td, hi) - h2_first;
                let h3hi = if hi <= td || beta == 0.0 {
                    0.0
                } else {
                    damp * i0e(z) - exp(-beta * td)
                };
                let h3_first = intlinfunc(td, hi, 0.0, h3hi, td, hi) / dp;
                let c3 = h3hi - h3_first;
                let fs = self.fit.as_mut().ok_or(PRUNED)?;
                fs.exps(dp);
                let (s2a, s2b) = (
                    fs.tail(1, aux, I2).ok_or(PRUNED)?,
                    fs.tail(1, aux, I1).ok_or(PRUNED)?,
                );
                let (s3a, s3b) = (
                    fs.tail(2, aux, V2).ok_or(PRUNED)?,
                    fs.tail(2, aux, V1).ok_or(PRUNED)?,
                );
                e1 = (dl.i2 - self.init_i2) * h2_first + c2 * self.dev(I2, aux) + s2a;
                e2 = (dl.i1 - self.init_i1) * h2_first + c2 * self.dev(I1, aux) + s2b;
                f1 = (dl.v2 - self.init_v2) * h3_first + c3 * self.dev(V2, aux) + s3a;
                f2 = (dl.v1 - self.init_v1) * h3_first + c3 * self.dev(V1, aux) + s3b;
            }
            let p = &self.p;
            e1 += self.init_i2 * p.int_h2;
            e2 += self.init_i1 * p.int_h2;
            in1 += e1;
            in2 += e2;
            f1 += self.init_v2 * p.int_h3dash;
            f2 += self.init_v1 * p.int_h3dash;
            in1 += admit * f1;
            in2 += admit * f2;
        }
        let p = &self.p;
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
        Ok(())
    }

    /// Recursive mode, RC: the exact coefficients of
    /// `LTRArcCoeffsSetup` over the window of lags `< W` (from the
    /// newest point `b` at least `W` old), the recursion beyond it.
    fn fit_inputs_rc(&mut self, time: f64) -> Result<(), &'static str> {
        const PRUNED: &str = "LTRA: recursive history state pruned";
        let n = self.times.len() - 1;
        let (cbyr, rclsqr) = (self.p.cbyr, self.p.rclsqr);
        let w = self.fit.as_ref().ok_or(PRUNED)?.fit.window;
        let b = self
            .times
            .partition_point(|&t| time - t >= w)
            .saturating_sub(1)
            .min(n);
        let lb = time - self.times[b];
        let fs = self.fit.as_mut().ok_or(PRUNED)?;
        fs.exps(lb);
        // sums[k][port]: kernel k against (v1|i2|v2) for port 1 and
        // (v2|i1|v1) for port 2, like `inputs_rc`.
        let waves = [[V1, V2], [I2, I1], [V2, V1]];
        let mut tails = [[0.0; 2]; 3];
        for k in 0..3 {
            for q in 0..2 {
                tails[k][q] = fs.tail(k, b, waves[k][q]).ok_or(PRUNED)?;
            }
        }
        let mut firsts = [0.0; 3];
        let mut sums = tails;
        for k in 0..3 {
            // H''(L_i) for i = n (first segment), walking older.
            let ln = time - self.times[n];
            let mut hi_val = rc_integrals(k, cbyr, rclsqr, ln).1;
            let mut dd_new = hi_val / ln; // DD of the segment [n, now]
            firsts[k] = dd_new;
            for i in (b + 1..=n).rev() {
                let lag = time - self.times[i - 1];
                let v = rc_integrals(k, cbyr, rclsqr, lag).1;
                let dd_old = (v - hi_val) / (self.times[i] - self.times[i - 1]);
                let c = dd_old - dd_new;
                for q in 0..2 {
                    sums[k][q] += c * self.dev(waves[k][q], i);
                }
                hi_val = v;
                dd_new = dd_old;
            }
            // The boundary point b: its share of lags [L_{b+1}, L_b].
            let c_b = rc_integrals(k, cbyr, rclsqr, lb).0 - dd_new;
            for q in 0..2 {
                sums[k][q] += c_b * self.dev(waves[k][q], b);
            }
        }
        self.co.h1_first = firsts[0];
        self.co.h2_first = firsts[1];
        self.co.h3_first = firsts[2];
        let p = &self.p;
        let init = |wave: usize| match wave {
            V1 => self.init_v1,
            V2 => self.init_v2,
            I1 => self.init_i1,
            _ => self.init_i2,
        };
        let ints = [p.int_h1dash, p.int_h2, p.int_h3dash];
        let mut inp = [0.0; 2];
        for q in 0..2 {
            for k in 0..3 {
                let x0 = init(waves[k][q]);
                let d = sums[k][q] + x0 * ints[k] - x0 * firsts[k];
                if k == 0 {
                    inp[q] -= d;
                } else {
                    inp[q] += d;
                }
            }
        }
        self.input1 = inp[0];
        self.input2 = inp[1];
        Ok(())
    }

    /// Advance the recursive states over the newest accepted segment
    /// and drop snapshots no later load can reach.
    fn fit_accept(&mut self) {
        let k = self.times.len() - 1;
        let h = self.times[k] - self.times[k - 1];
        let dn = [V1, V2, I1, I2].map(|w| self.dev(w, k));
        let dp = [V1, V2, I1, I2].map(|w| self.dev(w, k - 1));
        let keep = self.oldest_needed();
        if let Some(fs) = self.fit.as_mut() {
            fs.advance(h, dn, dp);
            fs.prune(keep);
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
        Some(self.interp_delayed(i as usize, target))
    }

    /// [`Self::delayed`] by bisection (recursive mode): the same
    /// bracketing point — the newest one strictly before `time − T`,
    /// never the newest of all — in O(log n) instead of the C's
    /// backward scan.
    fn delayed_bisect(&self, time: f64) -> Option<Delayed> {
        let target = time - self.p.td;
        let time_index = self.times.len() - 1;
        let below = self.times.partition_point(|&t| t < target);
        if below == 0 {
            return None;
        }
        let i = (below - 1).min(time_index.checked_sub(1)?);
        Some(self.interp_delayed(i, target))
    }

    /// The interpolation of `LTRAload` around `times[isaved]`,
    /// `times[isaved + 1]` (and `times[isaved − 1]` for quadratic).
    fn interp_delayed(&self, isaved: usize, target: f64) -> Delayed {
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
        Delayed {
            v1: one(&self.v1),
            i1: one(&self.i1),
            v2: one(&self.v2),
            i2: one(&self.i2),
        }
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
    fn par_load(&self) -> bool {
        true
    }

    /// `LTRAinfo` has no `DEVdisto`: the line is linear.
    fn disto_setup(&mut self, _ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        Ok(())
    }

    /// The recursive mode's report (issue #8): the model and its
    /// error against the closed-form responses, on the setup grid and
    /// at the lags the last timepoint's convolution actually spanned.
    fn tran_notes(&self) -> Vec<String> {
        if !self.fit_on {
            return Vec::new();
        }
        let name = &self.p.name;
        let Some(fs) = &self.fit else {
            let why = self.fit_skip.unwrap_or("not started");
            return vec![format!(
                "`{name}`: ltrafit not applied ({why}); exact convolution"
            )];
        };
        let f = &fs.fit;
        let (labels, delayed) = match self.p.case {
            LtraCase::Rc => (["h1", "h2", "h3"], false),
            _ => (["h1'", "h2", "h3'"], true),
        };
        let mut hist = [0.0; 3];
        if let Some(&te) = self.times.last() {
            let n = self.times.len() - 1;
            let w = f.window;
            let lags0 = self.times[..n].iter().map(|t| te - t).filter(|l| *l >= w);
            hist[0] = f.err_at(0, lags0);
            for (k, h) in hist.iter_mut().enumerate().skip(1) {
                *h = if delayed {
                    let td = self.p.td;
                    let lags = self.times.iter().map(|t| te - td - t).filter(|l| *l > 0.0);
                    f.err_at(k, lags)
                } else {
                    f.err_at(
                        k,
                        self.times[..n].iter().map(|t| te - t).filter(|l| *l >= w),
                    )
                };
            }
        }
        let errs: Vec<String> = (0..3)
            .map(|k| {
                format!(
                    "{} {:.1e}/{:.1e}/{:.1e}",
                    labels[k], f.err[k].max_rel, f.err[k].l1_rel, hist[k]
                )
            })
            .collect();
        let regime = match self.p.case {
            LtraCase::Rc => format!(
                "RC, RCl²={:.3e}s, exact window {:.3e}s",
                self.p.rclsqr, f.window
            ),
            _ => format!("RLC, T={:.3e}s, R/2L={:.3e}/s", self.p.td, self.p.alpha),
        };
        vec![format!(
            "`{name}`: ltrafit {regime}: {} real poles ({} panels x {} Gauss points); \
             kernel error max/L1 on [{:.3e}s, {:.3e}s] / max at the {} history lags: {}",
            f.rates.len(),
            f.panels,
            f.order,
            f.window,
            f.horizon,
            self.times.len(),
            errs.join(", ")
        )]
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
            ctx.add_rhs(self.br1, self.input1);
            ctx.add_rhs(self.br2, self.input2);
            return;
        }
        if self.times.is_empty() {
            // The operating point is always accepted (and recorded)
            // before the first transient load.
            ctx.fault = Some("LTRA: transient load before the operating point was stored");
            return;
        }
        if self.periodic {
            if let Some(f) = self.periodic_fault {
                ctx.fault = Some(f);
                return;
            }
            if self.fit.is_none() && matches!(self.p.case, LtraCase::Rlc | LtraCase::Rc) {
                // The exact walk sums a history that starts at an
                // operating point; the periodic past has none.
                ctx.fault = Some(
                    "LTRA: a lossy (RLC/RC) line in .pss needs the recursive convolution \
                     (.options ltrafit)",
                );
                return;
            }
        }
        let first = matches!(ctx.mode, InitMode::Tran | InitMode::Pred);
        let time_index = self.times.len() - 1;
        // `.pss`: the periodic past reaches back past every delay.
        let tdover = matches!(self.p.case, LtraCase::Lc | LtraCase::Rlc)
            && (ctx.time > self.p.td || self.periodic);
        let mut del = None;
        if first && self.fit.is_some() {
            // Recursive mode (issue #8): coefficients and inputs at once.
            if tdover {
                match self.delayed_bisect(ctx.time) {
                    Some(d) => del = Some(d),
                    None => {
                        ctx.fault = Some("LTRA: cannot find delayed timepoint");
                        return;
                    }
                }
            }
        } else if first {
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
            ctx.add_rhs(self.br1, self.input1);
            ctx.add_rhs(self.br2, self.input2);
            return;
        }

        // First timepoint after zero: record the operating point.
        if ctx.mode == InitMode::Tran && !ctx.uic && !self.periodic {
            self.init_v1 = ctx.sol[self.pos1] - ctx.sol[self.neg1];
            self.init_v2 = ctx.sol[self.pos2] - ctx.sol[self.neg2];
            self.init_i1 = ctx.sol[self.br1];
            self.init_i2 = ctx.sol[self.br2];
        }

        // Recursive mode: the first coefficients the stamps below use
        // come out of the same pass as the history inputs.
        if first && self.fit.is_some() {
            let r = match self.p.case {
                LtraCase::Lc | LtraCase::Rlc => self.fit_inputs_lc_rlc(ctx.time, del),
                LtraCase::Rc => self.fit_inputs_rc(ctx.time),
                LtraCase::Rg => Ok(()),
            };
            if let Err(e) = r {
                ctx.fault = Some(e);
                return;
            }
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

        if first && self.fit.is_none() {
            match self.p.case {
                LtraCase::Lc | LtraCase::Rlc => self.inputs_lc_rlc(del),
                LtraCase::Rc => self.inputs_rc(),
                LtraCase::Rg => {}
            }
        }

        ctx.add_rhs(self.br1, self.input1);
        ctx.add_rhs(self.br2, self.input2);
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
        if self.interp_control
            && matches!(p.case, LtraCase::Rlc | LtraCase::Lc)
            && let Some(err) = self.interp_error(t)
        {
            // The error goes as the square of the step: the step that
            // meets the tolerance is `h·√(tol/err)`.
            let peak = f64::max(self.wave_peak[0], self.wave_peak[1]);
            let tol = t.trtol * (t.reltol * peak + t.abstol);
            let h = t.time - self.times[self.times.len() - 1];
            if err > 0.0 && h > 0.0 {
                *time_step = time_step.min(h * (tol / err).sqrt());
            }
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
                let theta = 0.5 * atan(p.resist / (w * p.induct));
                let zmag = (p.resist * p.resist + w * w * p.induct * p.induct).sqrt();
                let mag = (w * p.capac / zmag).sqrt();
                let y0_r = mag * cos(theta);
                let y0_i = mag * sin(theta);
                let theta = std::f64::consts::FRAC_PI_2 - theta;
                let mag = mag * zmag;
                (y0_r, y0_i, mag * cos(theta), mag * sin(theta))
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
        let explambda_r = exp(exparg_r) * cos(exparg_i);
        let explambda_i = exp(exparg_r) * sin(exparg_i);
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

    fn compact_ready(&self) -> Option<bool> {
        self.straight_tail()
    }

    fn compact(&mut self) {
        self.drop_middle();
    }

    fn history_len(&self) -> Option<usize> {
        Some(self.times.len())
    }

    /// `LTRAaccept`: store the accepted point in the history and, when
    /// the slope of an incident wave `(v + Z0·i)·e^{−βT}` changes,
    /// plant a breakpoint one delay after the *previous* point (where
    /// the change happened).
    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if ctx.first {
            // MODEINITTRAN: the operating point starts the history.
            self.wave_peak = [0.0; 2];
            self.times.clear();
            self.v1.clear();
            self.i1.clear();
            self.v2.clear();
            self.i2.clear();
            self.periodic = false;
            self.periodic_fault = None;
            if let Some(past) = ctx.periodic {
                self.seed_periodic(past, ctx);
                return;
            }
        }
        let sol = ctx.sol;
        self.times.push(ctx.time);
        self.v1.push(sol[self.pos1] - sol[self.neg1]);
        self.v2.push(sol[self.pos2] - sol[self.neg2]);
        self.i1.push(sol[self.br1]);
        self.i2.push(sol[self.br2]);

        let idx = self.times.len() - 1;
        if self.fit_on {
            if ctx.first {
                self.start_fit(ctx.final_time);
            } else if idx > 0 && self.fit.is_some() {
                self.fit_accept();
            }
        }
        let p = &self.p;
        let wave = |v: f64, i: f64| ((v + i * p.imped) * p.attenuation).abs();
        self.wave_peak[0] = self.wave_peak[0].max(wave(self.v1[idx], self.i1[idx]));
        self.wave_peak[1] = self.wave_peak[1].max(wave(self.v2[idx], self.i2[idx]));
        if idx == 0 {
            return;
        }
        self.corner = self.slope_changed(idx, ctx.reltol, ctx.abstol);
        if self.corner {
            let t = self.times[idx - 1] + self.p.td;
            ctx.set_slope_break(t);
        }
    }
}

impl LtraI {
    /// The error the delayed waves will carry when this point is
    /// read back one delay later (Dune's, D-0055): `v(t − T)` and
    /// `i(t − T)` are interpolated between history points, and
    /// between two points `h` apart a straight line misses a wave of
    /// second derivative `w″` by `h²·|w″|/8`. `w″` is the second
    /// divided difference of each incident wave over the last two
    /// accepted points and the one just solved. Nothing in the 3f5
    /// bounds this: `LTRAtrunc` only keeps the step under the delay,
    /// and its `LTRAlteCalculate` (the convolutions' truncation
    /// error; unreachable from a card) was ported and measured — on
    /// the corpus it sits six orders below its tolerance and changes
    /// no deck. `None` before there are two accepted points.
    fn interp_error(&self, t: &TerrCtx) -> Option<f64> {
        let p = &self.p;
        let idx = self.times.len().checked_sub(1)?;
        if idx < 1 {
            return None;
        }
        let wave = |v: f64, i: f64| (v + i * p.imped) * p.attenuation;
        let (t2, t1, t0) = (self.times[idx - 1], self.times[idx], t.time);
        let h = t0 - t1;
        let second = |a: f64, b: f64, c: f64| ((c - b) / h - (b - a) / (t1 - t2)) / (t0 - t2);
        let d1 = second(
            wave(self.v1[idx - 1], self.i1[idx - 1]),
            wave(self.v1[idx], self.i1[idx]),
            wave(t.sol[self.pos1] - t.sol[self.neg1], t.sol[self.br1]),
        );
        let d2 = second(
            wave(self.v2[idx - 1], self.i2[idx - 1]),
            wave(self.v2[idx], self.i2[idx]),
            wave(t.sol[self.pos2] - t.sol[self.neg2], t.sol[self.br2]),
        );
        let err = 0.125 * h * h * f64::max(d1.abs(), d2.abs());
        err.is_finite().then_some(err)
    }

    /// `trytocompact`'s test in `LTRAaccept` (`ltraacct.c:84-127`):
    /// whether the last three samples of `v1`, `v2`, `i1` and `i2`
    /// each lie on a straight line in time within the model's
    /// `COMPACTREL`/`COMPACTABS` (`LTRAstraightLineCheck`). `None` when
    /// the line takes no part (fewer than three samples — the C's
    /// `CKTtimeIndex >= 2` —, the recursive mode, or a `.pss` run).
    fn straight_tail(&self) -> Option<bool> {
        let n = self.times.len();
        if n < 3 || self.fit_on || self.periodic {
            return None;
        }
        // Dune's guard (D-0055, with the slope floor): the middle
        // sample is a corner the slope rule has just found. The C's
        // test is on areas, and one step after a corner — a
        // picosecond, on a refined run — the corner passes it; the
        // history then holds a ramp from the last kept point to the
        // newest, the corner slides along with every step, and the
        // slope rule fires at each of them (the breakpoint cascade of
        // `regress/ltra_compact` at TMAX/40).
        if self.slope_floor && self.corner {
            return Some(false);
        }
        let (t1, t2, t3) = (self.times[n - 3], self.times[n - 2], self.times[n - 1]);
        let (rt, at) = (self.p.st_line_reltol, self.p.st_line_abstol);
        let straight =
            |y: &[f64]| straight_line_check(t1, y[n - 3], t2, y[n - 2], t3, y[n - 1], rt, at);
        Some(straight(&self.v1) && straight(&self.v2) && straight(&self.i1) && straight(&self.i2))
    }

    /// The compaction itself (`ltraacct.c:262-285`): the newest sample
    /// overwrites the middle one, which leaves the history.
    fn drop_middle(&mut self) {
        let n = self.times.len();
        debug_assert!(n >= 3);
        for h in [
            &mut self.times,
            &mut self.v1,
            &mut self.v2,
            &mut self.i1,
            &mut self.i2,
        ] {
            h.swap_remove(n - 2);
        }
    }

    /// `LTRAaccept`'s test at history point `idx ≥ 1`: whether the
    /// slope of an incident wave changed enough (and the wave is not
    /// a straight line within tolerance) to want a breakpoint one
    /// delay after point `idx − 1`.
    fn slope_changed(&self, idx: usize, reltol: f64, abstol: f64) -> bool {
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
        // The C's own test of "not a straight line" compares the
        // spread of the three values with 50·RELTOL of their *mean*:
        // around zero that is 50·ABSTOL, and the residue of a settled
        // edge passes it at every point. The floor asks the spread to
        // be visible at the scale of the line's signals (the larger
        // of the two waves: the backward wave of a matched line is
        // all residue).
        let peak = f64::max(self.wave_peak[0], self.wave_peak[1]);
        let check = |a: f64, b: f64, c: f64| {
            let spread = f64::max(f64::max(a, b), c) - f64::min(f64::min(a, b), c);
            spread >= (50.0 * (reltol / 3.0 * (a + b + c) + abstol)).abs()
                && (!self.slope_floor || spread >= reltol * peak)
        };
        let test1 = (d1 - d2).abs() >= p.reltol * f64::max(d1.abs(), d2.abs()) + p.abstol
            && check(v1, v2, v3);
        let test2 = (d3 - d4).abs() >= p.reltol * f64::max(d3.abs(), d4.abs()) + p.abstol
            && check(v4, v5, v6);
        test1 || test2
    }

    /// `.pss` (issue #20): the history of a shooting interval starting
    /// at `ctx.time = t₀` is the periodic past. The port histories get
    /// whole periods of the orbit back to the oldest lag a load can
    /// reach (one delay; the exact window of an RC fit), then the
    /// shooting state at `t₀`. The deviations are taken from the
    /// orbit's mean (not the operating point: there is none), so the
    /// constant part is the closed-form `∫h` term as always and the
    /// recursive states see a zero-mean periodic input, which
    /// [`FitState::periodic_start`] sums over the infinite past in
    /// closed form. The breakpoints the slope rule would have planted
    /// one delay after the past's slope changes are planted where they
    /// fall inside the interval.
    fn seed_periodic(&mut self, past: &PeriodicPast, ctx: &mut AcceptCtx) {
        self.periodic = true;
        let t0 = ctx.time;
        let period = past.period;
        let mut reach = self.p.td;
        if self.fit_on && self.p.case == LtraCase::Rc {
            reach = reach.max(RC_WINDOW_FRACTION * self.p.rclsqr);
        }
        // A sliver more than the reach, so a delay of exactly k periods
        // still finds a point before its target (quadratic interpolation).
        let from = t0 - reach - 1e-6 * period;
        let Some(samples) = past.samples(t0, from, ctx.sol) else {
            self.periodic_fault = Some("LTRA: the delay spans too many periods for .pss");
            return;
        };
        let (p1, n1, p2, n2, b1, b2) = (
            self.pos1, self.neg1, self.pos2, self.neg2, self.br1, self.br2,
        );
        for (t, x) in samples.into_iter().chain([(t0, ctx.sol)]) {
            self.times.push(t);
            self.v1.push(x[p1] - x[n1]);
            self.v2.push(x[p2] - x[n2]);
            self.i1.push(x[b1]);
            self.i2.push(x[b2]);
        }
        // One period is points 0..=per of the first copy.
        let per = past.phase.len() + 1;
        let mean = |w: &[f64]| {
            let mut acc = 0.0;
            for k in 1..=per {
                acc += 0.5 * (w[k] + w[k - 1]) * (self.times[k] - self.times[k - 1]);
            }
            acc / (self.times[per] - self.times[0])
        };
        self.init_v1 = mean(&self.v1);
        self.init_v2 = mean(&self.v2);
        self.init_i1 = mean(&self.i1);
        self.init_i2 = mean(&self.i2);
        if matches!(self.p.case, LtraCase::Rlc | LtraCase::Rc) {
            // The DC pin. With G = 0 the characteristic form loses the
            // line's DC drop: `Y0(0) = 0`, so at s = 0 both branch
            // equations reduce to `i1 + i2 = 0`, and the drop
            // `v1 − v2 = R·LEN·i1` lives only in the `√s` order — in
            // the time domain, the `τ^{−3/2}` tails integrated over a
            // history that starts at an operating point (whose DC
            // stamps are that resistor). A past that has always been
            // periodic carries no such start: its mean drop is a
            // neutral mode the orbit just copies (measured: the line's
            // apparent DC resistance drifting a fixed amount per
            // iteration, the period mismatch never closing at tight
            // tolerances). The past is pinned to the exact DC
            // relation instead: its mean voltages are shifted
            // symmetrically, the mean current kept, so that the mean
            // drop is `R·LEN` times the mean current. A fixed point of
            // the iteration then has the past equal to the orbit, so
            // the orbit itself meets the DC relation — the shift is
            // only nonzero while iterating.
            let i = 0.5 * (self.init_i1 - self.init_i2);
            let drop = self.p.resist * self.p.length * i;
            let avg = 0.5 * (self.init_v1 + self.init_v2);
            let (s1, s2) = (
                avg + 0.5 * drop - self.init_v1,
                avg - 0.5 * drop - self.init_v2,
            );
            let last = self.times.len() - 1;
            for k in 0..last {
                self.v1[k] += s1;
                self.v2[k] += s2;
            }
            self.init_v1 += s1;
            self.init_v2 += s2;
        }
        if self.fit_on {
            self.build_fit(ctx.final_time.max(PERIODIC_HORIZON_PERIODS * period));
            let seg = |k: usize| {
                (
                    self.times[k] - self.times[k - 1],
                    [V1, V2, I1, I2].map(|w| self.dev(w, k)),
                    [V1, V2, I1, I2].map(|w| self.dev(w, k - 1)),
                )
            };
            let first: Vec<_> = (1..=per).map(seg).collect();
            let rest: Vec<_> = (1..self.times.len()).map(seg).collect();
            let keep = self.oldest_needed();
            let span = self.times[per] - self.times[0];
            if let Some(fs) = self.fit.as_mut() {
                fs.periodic_start(span, &first);
                for (h, dn, dp) in rest {
                    fs.advance(h, dn, dp);
                }
                fs.prune(keep);
            }
        }
        for idx in 1..self.times.len() {
            let t = self.times[idx - 1] + self.p.td;
            if t > t0 + ctx.min_break
                && t < ctx.final_time - ctx.min_break
                && self.slope_changed(idx, ctx.reltol, ctx.abstol)
            {
                ctx.set_slope_break(t);
            }
        }
    }
}

/// The C's unscaled Bessel fits, for the scaled ones' tests.
#[cfg(test)]
pub(crate) fn tests_bess(x: f64) -> (f64, f64) {
    (bess_i0(x), bess_i1(x))
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
            port: None,
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
