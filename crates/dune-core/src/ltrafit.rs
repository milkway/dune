//! Recursive convolution for the LTRA lossy line (issue #8): the
//! impulse responses of [`crate::ltra`] as sums of decaying real
//! exponentials, built once per transient run, so the history part of
//! each convolution advances as a first-order recursion per accepted
//! point instead of the `LTRArlcCoeffsSetup`/`LTRArcCoeffsSetup` walk
//! over the whole history on every timepoint.
//!
//! # The representation (branch-cut quadrature)
//!
//! The kernels have closed-form Laplace transforms whose only
//! singularity is a branch cut on the negative real axis (`ltraacld.c`
//! loads exactly these: `Y0(s)` and `e^{−λ(s)·l}`). With the delay
//! `e^{−sT}` factored out, deforming the Bromwich contour around the
//! cut gives every delay-free kernel as an exact superposition of real
//! decaying exponentials,
//!
//! ```text
//! h(τ) = −(1/π) ∫ Im H(−x + i0) · e^{−xτ} dx ,
//! ```
//!
//! and a quadrature rule on that integral is a pole–residue model
//! `Σ_j r_j·e^{−x_j τ}` — rational in `s`, with real negative poles,
//! stable by construction (no pole flipping, no least squares). It is
//! the exact partial-fraction expansion the generic vector fitting
//! (Gustavsen–Semlyen) would approximate iteratively from samples.
//!
//! - **RLC** (`α = β = R/2L`, cut on `[−2α, 0]`): with
//!   `x = 2α·sin²θ`, `θ ∈ [0, π/2]`,
//!   `h1'(τ) = −(4α/π)∫ sin²θ·e^{−xτ} dθ`,
//!   `h2(T+σ) = (4α/π)∫ e^{−x(T+σ)}·sin(αT·sin2θ)·sinθ·cosθ dθ`,
//!   `h3'(T+σ) = −(4α/π)∫ e^{−x(T+σ)}·sin²θ·cos(αT·sin2θ) dθ`
//!   — smooth integrands; panels graded dyadically towards `θ = 0`
//!   (the slow rates that carry the `τ^{−3/2}` tail) down to the run
//!   horizon, Gauss–Legendre on each. The fit is valid uniformly from
//!   lag 0.
//! - **RC** (`a = RCl²`, cut on `(−∞, 0]`): with `x = y²`,
//!   `h1(τ) = −(2√(C/R)/π)∫ y²·e^{−y²τ} dy` (the regular part of the
//!   `√(sC/R)` admittance), `h2(τ) = (2/π)∫ y·sin(√a·y)·e^{−y²τ} dy`,
//!   `h3(τ) = −(2√(C/R)/π)∫ y²·cos(√a·y)·e^{−y²τ} dy`. The truncated
//!   `y` range makes the sum valid only for lags `τ ≥ W`
//!   ([`RC_WINDOW_FRACTION`]`·a`); younger lags stay with the exact
//!   coefficients (a hybrid window).
//!
//! The quadrature order is raised until the measured error against
//! the closed-form responses (the C's `LTRArlcH2Func` & co., Bessel
//! terms in exponentially scaled form) meets [`FIT_TOL`].

use libm::{cos, erfc, exp, sin};
use std::f64::consts::PI;

/// Error budget of the fit, per kernel: both the worst pointwise error
/// over the lag range (relative to the kernel's peak) and the L1 error
/// (relative to `∫|h|` — the bound on the convolution error per unit
/// of input) must stay below it.
pub(crate) const FIT_TOL: f64 = 1e-6;

/// The exact-coefficient window of RC lines, as a fraction of the
/// diffusion time `a = RCl²`: lags younger than `W = a/8` keep the
/// closed-form coefficients of `LTRArcCoeffsSetup` (where `h1` is
/// singular and `h2`/`h3` switch on), older ones go to the recursion.
pub(crate) const RC_WINDOW_FRACTION: f64 = 1.0 / 8.0;

/// `Y²·W`: the Gaussian factor `e^{−y²τ}` at the top of the RC `y`
/// range is `e^{−28} ≈ 7e-13` at the window lag.
const RC_GAUSS_CUT: f64 = 28.0;

/// Quadrature orders tried per panel, in order.
const ORDERS: [usize; 7] = [6, 8, 12, 16, 24, 32, 48];

/// Gauss–Legendre nodes and weights on `[-1, 1]` (Newton on `P_m`).
fn gauss_legendre(m: usize) -> (Vec<f64>, Vec<f64>) {
    let mut x = vec![0.0; m];
    let mut w = vec![0.0; m];
    for i in 0..m.div_ceil(2) {
        let mut z = cos(PI * (i as f64 + 0.75) / (m as f64 + 0.5));
        let mut dp = 1.0;
        for _ in 0..100 {
            let (mut p0, mut p1) = (1.0, z);
            for k in 2..=m {
                let p2 = ((2 * k - 1) as f64 * z * p1 - (k - 1) as f64 * p0) / k as f64;
                p0 = p1;
                p1 = p2;
            }
            let pm = if m == 1 { z } else { p1 };
            let pm1 = if m == 1 { 1.0 } else { p0 };
            dp = m as f64 * (z * pm - pm1) / (z * z - 1.0);
            let dz = pm / dp;
            z -= dz;
            if dz.abs() < 1e-16 {
                break;
            }
        }
        x[i] = -z;
        x[m - 1 - i] = z;
        let wi = 2.0 / ((1.0 - z * z) * dp * dp);
        w[i] = wi;
        w[m - 1 - i] = wi;
    }
    (x, w)
}

/// Nodes and weights of the composite rule: `m` Gauss points on each
/// `[breaks[k], breaks[k+1]]`.
fn composite(breaks: &[f64], m: usize) -> (Vec<f64>, Vec<f64>) {
    let (gx, gw) = gauss_legendre(m);
    let mut nodes = Vec::with_capacity(m * breaks.len());
    let mut weights = Vec::with_capacity(m * breaks.len());
    for p in breaks.windows(2) {
        let (lo, hi) = (p[0], p[1]);
        let half = 0.5 * (hi - lo);
        let mid = 0.5 * (hi + lo);
        for (x, w) in gx.iter().zip(&gw) {
            nodes.push(mid + half * x);
            weights.push(half * w);
        }
    }
    (nodes, weights)
}

/// Split every panel wider than `wmax` into equal parts.
fn subdivide(breaks: &[f64], wmax: f64) -> Vec<f64> {
    let mut out = vec![breaks[0]];
    for p in breaks.windows(2) {
        let n = ((p[1] - p[0]) / wmax).ceil().clamp(1.0, 4096.0) as usize;
        for k in 1..=n {
            out.push(p[0] + (p[1] - p[0]) * k as f64 / n as f64);
        }
    }
    out
}

// ---------------------------------------------------------------------
// Exponentially scaled modified Bessel functions to full precision
// (power series up to x = 20, Hankel's asymptotic series beyond). The
// C's Numerical Recipes fits are good to ~1e-7 and overflow `e^x` past
// x ≈ 709; the recursive mode is measured against these instead, and
// its exact first segments use them too.
// ---------------------------------------------------------------------

/// Switch from the power series to the asymptotic one.
const BESSEL_SERIES_MAX: f64 = 20.0;

/// `e^{−x}·Σ_k (x/2)^{2k}/(k!·(k+ν)!)` for ν = 0, 1 (i.e. `e^{−x}I0(x)`
/// and `e^{−x}I1(x)/(x/2)`).
fn bessel_series(x: f64, nu: usize) -> f64 {
    let q = 0.25 * x * x;
    let mut term = 1.0;
    let mut sum = term;
    for k in 1..200 {
        let kf = k as f64;
        term *= q / (kf * (kf + nu as f64));
        sum += term;
        if term < 1e-17 * sum {
            break;
        }
    }
    exp(-x) * sum
}

/// Hankel: `e^{−x}I_ν(x) ≈ (2πx)^{−1/2}·Σ_k (−1)^k a_k(ν)/x^k`,
/// `a_k = Π_{m≤k}(4ν² − (2m−1)²)/(k!·8^k)`, summed to the smallest
/// term (x > 20: below 1e-17 relative).
fn bessel_asym(x: f64, nu: usize) -> f64 {
    let mu = 4.0 * (nu * nu) as f64;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..60 {
        let m = (2 * k - 1) as f64;
        let next = -term * (mu - m * m) / (k as f64 * 8.0 * x);
        if next.abs() >= term.abs() {
            break;
        }
        term = next;
        sum += term;
        if term.abs() < 1e-17 * sum.abs() {
            break;
        }
    }
    sum / (2.0 * PI * x).sqrt()
}

/// `e^{−x}·I0(x)`, `x ≥ 0`.
pub(crate) fn i0e(x: f64) -> f64 {
    let x = x.abs();
    if x <= BESSEL_SERIES_MAX {
        bessel_series(x, 0)
    } else {
        bessel_asym(x, 0)
    }
}

/// `e^{−x}·I1(x)/x`, `x ≥ 0` (1/2 at 0).
pub(crate) fn i1e_over_x(x: f64) -> f64 {
    let x = x.abs();
    if x <= BESSEL_SERIES_MAX {
        0.5 * bessel_series(x, 1)
    } else {
        bessel_asym(x, 1) / x
    }
}

/// `e^{−x}·I1(x)`, `x ≥ 0`.
pub(crate) fn i1e(x: f64) -> f64 {
    let x = x.abs();
    if x <= BESSEL_SERIES_MAX {
        0.5 * x * bessel_series(x, 1)
    } else {
        bessel_asym(x, 1)
    }
}

// ---------------------------------------------------------------------
// Closed-form responses (the references the fit is measured against).
// ---------------------------------------------------------------------

/// The delay-free kernels of one line regime.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Kernels {
    /// `α`, `T`: `h1'(τ)`, `h2(T+σ)`, `h3'(T+σ)`.
    Rlc { alpha: f64, td: f64 },
    /// `C/R`, `RCl²`: `h1(τ)` (regular part), `h2(τ)`, `h3(τ)`.
    Rc { cbyr: f64, rclsqr: f64 },
}

impl Kernels {
    /// Kernel `k` (0, 1, 2) at delay-free lag `s ≥ 0` — `ltramisc.c`'s
    /// `LTRArlcH1dashFunc`/`LTRArlcH2Func`/`LTRArlcH3dashFunc` with the
    /// exponentials folded into scaled Bessel functions, and the
    /// derivatives of the RC twice-integrated responses.
    pub fn exact(&self, k: usize, s: f64) -> f64 {
        match *self {
            Kernels::Rlc { alpha, td } => match k {
                0 => alpha * (i1e(alpha * s) - i0e(alpha * s)),
                _ => {
                    let t = td + s;
                    let z = alpha * (s * (2.0 * td + s)).sqrt();
                    let damp = exp(-(alpha * t - z));
                    if k == 1 {
                        alpha * alpha * td * damp * i1e_over_x(z)
                    } else {
                        alpha * damp * (alpha * t * i1e_over_x(z) - i0e(z))
                    }
                }
            },
            Kernels::Rc { cbyr, rclsqr } => {
                if s <= 0.0 {
                    return 0.0;
                }
                let c = 1.0 / (2.0 * PI.sqrt() * s * s.sqrt());
                let g = exp(-rclsqr / (4.0 * s));
                match k {
                    0 => -cbyr.sqrt() * c,
                    1 => rclsqr.sqrt() * c * g,
                    _ => -cbyr.sqrt() * c * (1.0 - rclsqr / (2.0 * s)) * g,
                }
            }
        }
    }
}

/// Measured error of one kernel's fit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct KernelErr {
    /// `max|h_fit − h| / max|h|` over the lags checked.
    pub max_rel: f64,
    /// `∫|h_fit − h| / ∫|h|` over the lags checked.
    pub l1_rel: f64,
}

/// A line's exponential-sum model: shared poles `−rates[j]`, one
/// residue list per kernel.
#[derive(Clone, Debug)]
pub(crate) struct ExpFit {
    pub kernels: Kernels,
    /// `x_j > 0`: the poles are `s = −x_j`.
    pub rates: Vec<f64>,
    /// Residues of kernels 0, 1, 2 (`h(τ) ≈ Σ r_j e^{−x_j τ}`).
    pub res: [Vec<f64>; 3],
    /// Lags below which the exact coefficients are used (0 for RLC).
    pub window: f64,
    /// The largest lag the fit was checked to (the run's TSTOP).
    pub horizon: f64,
    /// Per-panel quadrature order and panel count.
    pub order: usize,
    pub panels: usize,
    /// Error on the setup grid, per kernel.
    pub err: [KernelErr; 3],
}

impl ExpFit {
    /// `Σ r_j e^{−x_j τ}` for kernel `k`.
    pub fn eval(&self, k: usize, tau: f64) -> f64 {
        self.rates
            .iter()
            .zip(&self.res[k])
            .map(|(x, r)| r * exp(-x * tau))
            .sum()
    }

    /// Worst error of kernel `k` against the closed form over `lags`
    /// (pointwise, relative to the largest `|h|` among them).
    pub fn err_at(&self, k: usize, lags: impl Iterator<Item = f64>) -> f64 {
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for s in lags {
            let e = self.kernels.exact(k, s);
            num = num.max((self.eval(k, s) - e).abs());
            den = den.max(e.abs());
        }
        if den > 0.0 { num / den } else { num }
    }

    /// Check grid over `[window, horizon]`: geometric towards the
    /// lower end (RC's singular `h1`, the fast RLC start) plus uniform.
    fn check_grid(&self) -> Vec<f64> {
        let lo = self.window;
        let hi = self.horizon;
        let mut g = Vec::with_capacity(1201);
        let first = if lo > 0.0 { lo } else { hi * 1e-9 };
        if lo == 0.0 {
            g.push(0.0);
        }
        let n = 600;
        for k in 0..=n {
            g.push(first * (hi / first).powf(k as f64 / n as f64));
        }
        for k in 1..n {
            g.push(lo + (hi - lo) * k as f64 / n as f64);
        }
        g.sort_by(f64::total_cmp);
        g.dedup();
        g
    }

    fn measure(&mut self) {
        let grid = self.check_grid();
        for k in 0..3 {
            let (mut num, mut den) = (0.0f64, 0.0f64);
            let (mut l1e, mut l1h) = (0.0, 0.0);
            let mut prev: Option<(f64, f64, f64)> = None;
            for &s in &grid {
                let e = self.kernels.exact(k, s);
                let d = (self.eval(k, s) - e).abs();
                num = num.max(d);
                den = den.max(e.abs());
                if let Some((s0, d0, e0)) = prev {
                    l1e += 0.5 * (d + d0) * (s - s0);
                    l1h += 0.5 * (e.abs() + e0.abs()) * (s - s0);
                }
                prev = Some((s, d, e));
            }
            self.err[k] = KernelErr {
                max_rel: if den > 0.0 { num / den } else { num },
                l1_rel: if l1h > 0.0 { l1e / l1h } else { l1e },
            };
        }
    }

    fn worst(&self) -> f64 {
        self.err
            .iter()
            .map(|e| e.max_rel.max(e.l1_rel))
            .fold(0.0, f64::max)
    }
}

/// The RLC model for the run horizon `horizon` (the largest lag).
pub(crate) fn fit_rlc(alpha: f64, td: f64, horizon: f64) -> ExpFit {
    let kernels = Kernels::Rlc { alpha, td };
    // Dyadic panels towards θ = 0 until the slowest Gaussian width at
    // the horizon, e^{−2α·τmax·θ²}, is resolved.
    let theta_c = 1.0 / (2.0 * alpha * horizon).max(1e-300).sqrt();
    let mut breaks = vec![PI / 2.0];
    let mut th = PI / 2.0;
    while th > theta_c && breaks.len() < 80 {
        th *= 0.5;
        breaks.push(th);
    }
    if breaks.len() < 3 {
        breaks = vec![PI / 2.0, PI / 4.0, PI / 8.0];
    }
    breaks.push(0.0);
    breaks.reverse();
    // The propagation phase αT·sin2θ: keep it to ~1 rad per panel.
    let wmax = (PI / 8.0).min(1.0 / (alpha * td).max(1e-300));
    let breaks = subdivide(&breaks, wmax);
    let build = |m: usize| -> ExpFit {
        let (th, w) = composite(&breaks, m);
        let c = 4.0 * alpha / PI;
        let mut rates = Vec::with_capacity(th.len());
        let mut res = [Vec::new(), Vec::new(), Vec::new()];
        for (t, w) in th.iter().zip(&w) {
            let (s, co) = (sin(*t), cos(*t));
            let x = 2.0 * alpha * s * s;
            let ph = alpha * td * 2.0 * s * co;
            let d = exp(-x * td);
            rates.push(x);
            res[0].push(-c * w * s * s);
            res[1].push(c * w * d * sin(ph) * s * co);
            res[2].push(-c * w * d * s * s * cos(ph));
        }
        let mut f = ExpFit {
            kernels,
            rates,
            res,
            window: 0.0,
            horizon,
            order: m,
            panels: breaks.len() - 1,
            err: Default::default(),
        };
        f.measure();
        f
    };
    adapt(build)
}

/// The RC model for lags in `[window, horizon]`.
pub(crate) fn fit_rc(cbyr: f64, rclsqr: f64, window: f64, horizon: f64) -> ExpFit {
    let kernels = Kernels::Rc { cbyr, rclsqr };
    let ytop = (RC_GAUSS_CUT / window).sqrt();
    let ymin = 0.25 / horizon.sqrt();
    // Uniform panels over the oscillating top (√a·y), dyadic below.
    let wu = (2.0 / rclsqr.sqrt()).min(ytop / 4.0);
    let mut breaks = vec![ytop];
    let mut y = ytop;
    while y > wu {
        y = (y - wu).max(wu);
        breaks.push(y);
        if y == wu {
            break;
        }
    }
    while y > ymin && breaks.len() < 200 {
        y *= 0.5;
        breaks.push(y);
    }
    breaks.push(0.0);
    breaks.reverse();
    breaks.dedup();
    let sa = rclsqr.sqrt();
    let sc = cbyr.sqrt();
    let build = |m: usize| -> ExpFit {
        let (ys, w) = composite(&breaks, m);
        let mut rates = Vec::with_capacity(ys.len());
        let mut res = [Vec::new(), Vec::new(), Vec::new()];
        for (y, w) in ys.iter().zip(&w) {
            rates.push(y * y);
            res[0].push(-2.0 * sc / PI * w * y * y);
            res[1].push(2.0 / PI * w * y * sin(sa * y));
            res[2].push(-2.0 * sc / PI * w * y * y * cos(sa * y));
        }
        let mut f = ExpFit {
            kernels,
            rates,
            res,
            window,
            horizon,
            order: m,
            panels: breaks.len() - 1,
            err: Default::default(),
        };
        f.measure();
        f
    };
    adapt(build)
}

/// Raise the per-panel order until [`FIT_TOL`] holds (or the orders
/// run out, keeping the best).
fn adapt(build: impl Fn(usize) -> ExpFit) -> ExpFit {
    let mut best: Option<ExpFit> = None;
    for m in ORDERS {
        let f = build(m);
        let ok = f.worst() <= FIT_TOL;
        if best.as_ref().is_none_or(|b| f.worst() < b.worst()) {
            best = Some(f);
        }
        if ok {
            break;
        }
    }
    best.expect("at least one order is tried")
}

// ---------------------------------------------------------------------
// The recursion.
// ---------------------------------------------------------------------

/// Weights of one linear segment of length `h` for a pole of rate `x`:
/// `S(t+h) = e^{−z}·S(t) + h·(a·d(t+h) + b·d(t))`, `z = x·h`, with
/// `a = ∫₀¹ e^{−zu}(1−u)du`, `b = ∫₀¹ u·e^{−zu}du` (series below
/// `z = 0.5`, where the closed forms cancel). Returns `(e^{−z}, a, b)`.
pub(crate) fn segment(z: f64) -> (f64, f64, f64) {
    if z < 0.5 {
        // Σ (−z)^k/k! · {1/((k+1)(k+2)), 1/(k+2)}
        let (mut a, mut b) = (0.0, 0.0);
        let mut t = 1.0; // (−z)^k / k!
        for k in 0..18 {
            let kf = k as f64;
            a += t / ((kf + 1.0) * (kf + 2.0));
            b += t / (kf + 2.0);
            t *= -z / (kf + 1.0);
        }
        (exp(-z), a, b)
    } else {
        let e = exp(-z);
        let phi1 = (1.0 - e) / z;
        let psi = (1.0 - e * (1.0 + z)) / (z * z);
        (e, phi1 - psi, psi)
    }
}

/// `∫_0^τ h` and `∫_0^τ∫_0^u h` of the RC kernels in closed form
/// (`H'`, `H''`; `H''` is `LTRArcCoeffsSetup`'s, `H'` its derivative),
/// for the exact window. Kernel 0: `H'' = √(4·(C/R)·τ/π)`,
/// `H' = √((C/R)/(πτ))`; kernel 1: `H' = erfc(√(a/4τ))`; kernel 2:
/// `H' = √((C/R)/(πτ))·e^{−a/4τ}`.
pub(crate) fn rc_integrals(k: usize, cbyr: f64, rclsqr: f64, tau: f64) -> (f64, f64) {
    if tau <= 0.0 {
        return (0.0, 0.0);
    }
    let temp = rclsqr / (4.0 * tau);
    let e2 = if temp >= 100.0 {
        0.0
    } else {
        erfc(temp.sqrt())
    };
    let e3 = exp(-temp);
    match k {
        0 => ((cbyr / (PI * tau)).sqrt(), (4.0 * cbyr * tau / PI).sqrt()),
        1 => (
            e2,
            (tau + rclsqr * 0.5) * e2 - (tau * rclsqr / PI).sqrt() * e3,
        ),
        _ => (
            (cbyr / (PI * tau)).sqrt() * e3,
            cbyr.sqrt() * (2.0 * (tau / PI).sqrt() * e3 - rclsqr.sqrt() * e2),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_legendre_integrates_polynomials() {
        for m in [1, 2, 5, 8, 13, 48] {
            let (x, w) = gauss_legendre(m);
            let s: f64 = w.iter().sum();
            assert!((s - 2.0).abs() < 1e-14, "m={m} {s}");
            // ∫ x^(2m−2) = 2/(2m−1)
            let p = 2 * m - 2;
            let q: f64 = x.iter().zip(&w).map(|(x, w)| w * x.powi(p as i32)).sum();
            assert!((q - 2.0 / (p as f64 + 1.0)).abs() < 1e-13, "m={m} {q}");
        }
    }

    #[test]
    fn segment_weights_match_quadrature() {
        for &z in &[0.0, 1e-9, 0.1, 0.49, 0.5, 2.0, 40.0] {
            let (e, a, b) = segment(z);
            let (x, w) = gauss_legendre(40);
            let (mut qa, mut qb) = (0.0, 0.0);
            for (x, w) in x.iter().zip(&w) {
                let u = 0.5 * (x + 1.0);
                qa += 0.5 * w * (-z * u).exp() * (1.0 - u);
                qb += 0.5 * w * (-z * u).exp() * u;
            }
            assert!((e - (-z).exp()).abs() < 1e-16);
            assert!((a - qa).abs() < 1e-15, "z={z} {a} {qa}");
            assert!((b - qb).abs() < 1e-15, "z={z} {b} {qb}");
        }
    }

    #[test]
    fn scaled_bessel_are_accurate() {
        // Reference values (Abramowitz & Stegun, 16 digits).
        let refs = [
            (1.0f64, 1.266_065_877_752_008_4, 0.565_159_103_992_485_1),
            (5.0, 27.239_871_823_604_442, 24.335_642_142_450_524),
        ];
        for (x, i0, i1) in refs {
            let e = x.exp();
            assert!((i0e(x) * e - i0).abs() < 1e-14 * i0, "{x}");
            assert!((i1e(x) * e - i1).abs() < 1e-14 * i1, "{x}");
            assert!((i1e_over_x(x) * x * e - i1).abs() < 1e-14 * i1, "{x}");
        }
        // Series and asymptotic meet at the switch.
        let x = BESSEL_SERIES_MAX;
        for nu in [0, 1] {
            let s = if nu == 0 {
                bessel_series(x, 0)
            } else {
                0.5 * x * bessel_series(x, 1)
            };
            let a = bessel_asym(x, nu);
            assert!((s - a).abs() < 1e-14 * s, "nu={nu} {s} {a}");
        }
        // And the C's fits are within their own ~1e-7.
        for &x in &[0.3, 3.0, 3.75, 5.0, 40.0] {
            let (r0, r1) = super::super::ltra::tests_bess(x);
            assert!((i0e(x) * x.exp() - r0).abs() <= 3e-7 * r0, "{x}");
            assert!((i1e(x) * x.exp() - r1).abs() <= 3e-7 * r1, "{x}");
        }
    }

    #[test]
    fn rlc_fit_meets_the_budget_on_long_horizons() {
        // ltra_3's lines, the 60 ns run and a 1000/α one.
        let lines: [(f64, f64, f64, f64); 4] = [
            (0.2, 9.13e-9, 3.65e-12, 2.0),
            (0.2, 4.729_339_990_88e-9, 7.250_000_003_73e-12, 20.0),
            (0.2, 1.353_065_998_18e-8, 3.650_000_007_46e-12, 20.0),
            (12.45, 8.792e-9, 0.468e-12, 16.0),
        ];
        for &(r, l, c, len) in &lines {
            let alpha = 0.5 * r / l;
            let td = (l * c).sqrt() * len;
            for &h in &[60e-9, 1000.0 / alpha] {
                let f = fit_rlc(alpha, td, h);
                assert!(f.worst() <= FIT_TOL, "{r} {l} {h}: {:?}", f.err);
                // The DC gains are the C's LTRAintH*: −1, 1 − e^{−αT},
                // −e^{−αT} (the fit integrates to them up to the tail
                // beyond the horizon).
                let g =
                    |k: usize| -> f64 { f.rates.iter().zip(&f.res[k]).map(|(x, r)| r / x).sum() };
                let att = (-alpha * td).exp();
                assert!((g(0) + 1.0).abs() < 1e-6, "{}", g(0));
                assert!((g(1) - (1.0 - att)).abs() < 1e-6 * (1.0 - att) + 1e-9);
                assert!((g(2) + att).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn rc_fit_meets_the_budget() {
        let (r, c, len) = (0.2, 3.65e-12, 24.0);
        let (cbyr, a) = (c / r, r * c * len * len);
        let f = fit_rc(cbyr, a, RC_WINDOW_FRACTION * a, 60e-9);
        assert!(f.worst() <= FIT_TOL, "{:?} poles {}", f.err, f.rates.len());
    }

    #[test]
    fn rc_integrals_differentiate_consistently() {
        let (cbyr, a) = (3.65e-12 / 0.2, 0.2 * 3.65e-12 * 576.0);
        for k in 0..3 {
            for &t in &[a / 20.0, a, 30.0 * a] {
                let h = t * 1e-5;
                let d = (rc_integrals(k, cbyr, a, t + h).1 - rc_integrals(k, cbyr, a, t - h).1)
                    / (2.0 * h);
                let p = rc_integrals(k, cbyr, a, t).0;
                assert!((d - p).abs() <= 1e-6 * p.abs().max(1e-30), "k={k} {d} {p}");
                let dd = (rc_integrals(k, cbyr, a, t + h).0 - rc_integrals(k, cbyr, a, t - h).0)
                    / (2.0 * h);
                let e = Kernels::Rc { cbyr, rclsqr: a }.exact(k, t);
                assert!(
                    (dd - e).abs() <= 1e-5 * e.abs().max(1e-30),
                    "k={k} {dd} {e}"
                );
            }
        }
    }
}
