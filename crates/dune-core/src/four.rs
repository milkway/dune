//! `.four`: Fourier analysis (harmonics and THD) of a transient
//! vector — a pure post-processing layer, like [`crate::measure`]:
//! samples in, a harmonic table out, the simulation never touched.
//!
//! Translated from ngspice-47 `src/frontend/fourier.c` (`fourier`,
//! `CKTfour`) and `src/maths/poly/{interpolate,polyfit,polyeval}.c`
//! (`ft_interpolate`, `ft_polyfit`, `ft_peval`) — Copyright 1990
//! Regents of the University of California, author 1985 Wayne A.
//! Christopher, U. C. Berkeley CAD Group (Modified BSD). The SPICE3f5
//! original is `src/lib/fte/fourier.c`; where the two differ ngspice
//! is followed (`nperiods`, `hypot`, the header line). The batch
//! wiring (`.four` executed after the run on the newest transient
//! plot) is `frontend/dotcards.c::ft_cktcoms`.
//!
//! The algorithm, per output vector `v(t)` with fundamental `f`:
//!
//! 1. take the last `P = nperiods` periods, `[t_max − P/f, t_max]`;
//! 2. resample them on `N = fourgridsize·P` uniform points
//!    `t_i = t_0 + i·(t_max − t_0)/N`, `i = 0..N−1`, by piecewise
//!    polynomial interpolation of degree `polydegree` (`0` = no
//!    resampling: the raw samples are used as if uniform);
//! 3. for `k = 0..nfreqs−1`: `s_k = Σ v_i sin(2πkPi/N)`,
//!    `c_k = Σ v_i cos(2πkPi/N)`; DC `= c_0/N` (signed);
//!    `|V_k| = hypot(2s_k/N, 2c_k/N)`,
//!    `φ_k = atan2(2c_k/N, 2s_k/N)·180/π` (degrees, sine reference);
//!    normalised `|V_k|/|V_1|` and `φ_k − φ_1`;
//!    `THD = 100·sqrt(Σ_{k≥2} (|V_k|/|V_1|)²)` percent.
//!
//! Deliberate departures (each in `corpus/KNOWN_DIFFS.md`): the grid
//! size is not compounded by `nperiods` once per vector (an ngspice
//! slip: the second vector of a card gets `fourgridsize·P²` points);
//! entries that would read out of bounds in the C (degenerate scales)
//! are errors; oversized grids are refused instead of running for
//! hours. Notes in `docs/dune-four.md`.

use dune_netlist::Diagnostic;
use dune_netlist::ir::{Output, OutputKind, Param, ParamValue};

use crate::select::{ColData, OutExpr};
use crate::solution::Solution;

/// Default `nfreqs`: harmonics computed, DC included (`fourier.c`).
pub const DEF_NFREQS: usize = 10;
/// Default `nperiods`: periods transformed (ngspice; the 3f5 has 1).
pub const DEF_NPERIODS: usize = 1;
/// Default `polydegree`: linear interpolation onto the grid.
pub const DEF_POLYDEGREE: usize = 1;
/// Default `fourgridsize` (`DEF_FOURGRIDSIZE`): points per period.
pub const DEF_FOURGRIDSIZE: usize = 200;

/// Most samples a transform may use (`fourgridsize·nperiods`, or the
/// vector length with `polydegree=0`): a hostile `.options` must not
/// allocate gigabytes.
pub const MAX_FOUR_POINTS: usize = 10_000_000;
/// Most `sin`/`cos` pairs a transform may evaluate (points ×
/// harmonics), about ten seconds of work.
pub const MAX_FOUR_WORK: u128 = 1_000_000_000;

/// `EDGE_FACTOR` of `interpolate.c`: a vertical edge (two samples at
/// the same abscissa) is turned into a slope by pushing the earlier
/// point back by this fraction of the previous spacing.
const EDGE_FACTOR: f64 = 1e-3;

/// The front-end variables that steer `.four` (ngspice reads them
/// with `cp_getvar`, which also sees the deck's `.options`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FourParams {
    /// `nfreqs`: harmonics computed, DC included (default 10).
    pub nfreqs: usize,
    /// `nperiods`: periods transformed (default 1).
    pub nperiods: usize,
    /// `polydegree`: interpolation degree onto the uniform grid;
    /// `0` transforms the raw samples (default 1).
    pub polydegree: usize,
    /// `fourgridsize`: grid points per period (default 200).
    pub gridsize: usize,
}

impl Default for FourParams {
    fn default() -> Self {
        FourParams {
            nfreqs: DEF_NFREQS,
            nperiods: DEF_NPERIODS,
            polydegree: DEF_POLYDEGREE,
            gridsize: DEF_FOURGRIDSIZE,
        }
    }
}

impl FourParams {
    /// Read `nfreqs`, `nperiods`, `polydegree` and `fourgridsize` from
    /// a deck's `.options`, as ngspice's `cp_getvar` does: the
    /// **first** occurrence wins (the circuit variable list is
    /// searched from its head), a real value is truncated toward zero
    /// (`(int) va_real`), and a value out of range (`nfreqs`,
    /// `nperiods`, `fourgridsize` < 1, `polydegree` < 0) or a bare
    /// flag falls back to the default — with a warning here, silently
    /// in ngspice. Other options are ignored (see
    /// [`crate::Options::apply`]).
    pub fn from_options(params: &[Param], diags: &mut Vec<Diagnostic>) -> FourParams {
        let mut out = FourParams::default();
        let mut seen = [false; 4];
        for p in params {
            let (slot, min) = match p.name.as_str() {
                "nfreqs" => (0, 1),
                "nperiods" => (1, 1),
                "polydegree" => (2, 0),
                "fourgridsize" => (3, 1),
                _ => continue,
            };
            if seen[slot] {
                continue;
            }
            seen[slot] = true;
            let value = match p.value {
                ParamValue::Num(v) if v.is_finite() && v.trunc().abs() < 2_147_483_648.0 => {
                    Some(v.trunc() as i64)
                }
                _ => None,
            };
            match value {
                Some(v) if v >= min => {
                    let v = v as usize;
                    match slot {
                        0 => out.nfreqs = v,
                        1 => out.nperiods = v,
                        2 => out.polydegree = v,
                        _ => out.gridsize = v,
                    }
                }
                _ => diags.push(Diagnostic::warning(
                    format!(
                        "option {} needs an integer >= {min}; using the default",
                        p.name
                    ),
                    p.span,
                )),
            }
        }
        out
    }
}

/// One row of the harmonic table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Harmonic {
    /// `k·f` (0 for DC).
    pub freq: f64,
    /// Amplitude `hypot(2s_k/N, 2c_k/N)`; for DC the signed mean
    /// `c_0/N`.
    pub mag: f64,
    /// Phase in degrees, `atan2(c_k, s_k)` (a sine at zero phase
    /// reads 0); 0 for DC.
    pub phase: f64,
    /// `mag / mag₁` (0 for DC).
    pub norm_mag: f64,
    /// `phase − phase₁`, degrees, not wrapped (0 for DC).
    pub norm_phase: f64,
}

/// The Fourier analysis of one vector.
#[derive(Clone, Debug, PartialEq)]
pub struct Fourier {
    /// Fundamental frequency (Hz).
    pub fund: f64,
    /// Total harmonic distortion, percent:
    /// `100·sqrt(Σ_{k=2}^{nfreqs−1} (mag_k/mag_1)²)`.
    pub thd: f64,
    /// Samples transformed: `fourgridsize·nperiods`, or the vector
    /// length when `polydegree = 0` (ngspice's `Gridsize:`).
    pub gridsize: usize,
    /// Interpolation degree used.
    pub polydegree: usize,
    /// Periods transformed.
    pub nperiods: usize,
    /// `nfreqs` rows, DC first.
    pub harmonics: Vec<Harmonic>,
}

/// Why a vector has no Fourier table.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FourError {
    /// The fundamental is not a positive finite number.
    #[error("bad fundamental freq {0}")]
    BadFrequency(f64),
    /// No `.tran` plot to analyse.
    #[error("no transient data available for fourier analysis")]
    NoTransient,
    /// The output variable is not in the plot.
    #[error("no such vector {0}")]
    UnknownVector(String),
    /// Time and data lengths differ (`fourier.c`).
    #[error("lengths don't match: {0}, {1}")]
    LengthMismatch(usize, usize),
    /// The run is shorter than the requested periods.
    #[error("({nperiods} * wavelength) longer than time span")]
    SpanTooShort { nperiods: usize },
    /// `ft_interpolate` gave up (its message).
    #[error("can't interpolate: {0}")]
    Interpolate(String),
    /// The grid or the work exceeds [`MAX_FOUR_POINTS`] /
    /// [`MAX_FOUR_WORK`].
    #[error("fourier grid too large ({points} points × {nfreqs} harmonics)")]
    TooLarge { points: usize, nfreqs: usize },
}

/// Fourier analysis of `values` sampled at `time` (a transient
/// vector and its scale): `fourier.c::fourier` for one vector, then
/// `CKTfour`. See the module docs for the equations.
pub fn fourier(
    time: &[f64],
    values: &[f64],
    fund: f64,
    params: &FourParams,
) -> Result<Fourier, FourError> {
    if !(fund.is_finite() && fund > 0.0) {
        return Err(FourError::BadFrequency(fund));
    }
    if values.len() != time.len() {
        return Err(FourError::LengthMismatch(values.len(), time.len()));
    }
    let nfreqs = params.nfreqs.max(1);
    let nperiods = params.nperiods.max(1);
    let too_large = |points: usize| FourError::TooLarge { points, nfreqs };
    let (data, gridsize) = if params.polydegree > 0 {
        let n = params
            .gridsize
            .max(1)
            .checked_mul(nperiods)
            .filter(|&n| n <= MAX_FOUR_POINTS)
            .ok_or_else(|| too_large(params.gridsize.saturating_mul(nperiods)))?;
        if n as u128 * nfreqs as u128 > MAX_FOUR_WORK {
            return Err(too_large(n));
        }
        // ft_minmax(time): HUGE / -HUGE seeds.
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &t in time {
            if t < lo {
                lo = t;
            }
            if t > hi {
                hi = t;
            }
        }
        let d = nperiods as f64 / fund; // the wavelength(s)
        if hi - lo < d || (hi - lo).is_nan() {
            return Err(FourError::SpanTooShort { nperiods });
        } else if hi - lo > d {
            lo = hi - d;
        }
        let d = (hi - lo) / n as f64;
        let grid: Vec<f64> = (0..n).map(|i| lo + i as f64 * d).collect();
        let data =
            interpolate(values, time, &grid, params.polydegree).map_err(FourError::Interpolate)?;
        (data, n)
    } else {
        let n = values.len();
        if n > MAX_FOUR_POINTS || n as u128 * nfreqs as u128 > MAX_FOUR_WORK {
            return Err(too_large(n));
        }
        (values.to_vec(), n)
    };
    let (thd, harmonics) = ckt_four(&data, nfreqs, nperiods, fund);
    Ok(Fourier {
        fund,
        thd,
        gridsize,
        polydegree: params.polydegree,
        nperiods,
        harmonics,
    })
}

/// `CKTfour`: the DFT of `nperiods` periods sampled uniformly in
/// `value`, at the first `nfreqs` harmonics of `fund`; returns the THD
/// (percent) and the table. Operation order is the C's.
fn ckt_four(value: &[f64], nfreqs: usize, nperiods: usize, fund: f64) -> (f64, Vec<Harmonic>) {
    use std::f64::consts::PI;
    let ndata = value.len();
    let mut mag = vec![0.0f64; nfreqs];
    let mut phase = vec![0.0f64; nfreqs];
    for (i, &v) in value.iter().enumerate() {
        for j in 0..nfreqs {
            let arg = j as f64 * 2.0 * PI * nperiods as f64 * i as f64 / ndata as f64;
            mag[j] += v * arg.sin();
            phase[j] += v * arg.cos();
        }
    }
    let mut rows = vec![
        Harmonic {
            freq: 0.0,
            mag: 0.0,
            phase: 0.0,
            norm_mag: 0.0,
            norm_phase: 0.0,
        };
        nfreqs
    ];
    rows[0].mag = phase[0] / ndata as f64;
    let mut thd = 0.0;
    for i in 1..nfreqs {
        let tmp = mag[i] * 2.0 / ndata as f64;
        phase[i] *= 2.0 / ndata as f64;
        mag[i] = tmp.hypot(phase[i]);
        phase[i] = phase[i].atan2(tmp) * 180.0 / PI;
        let r = &mut rows[i];
        r.freq = i as f64 * fund;
        r.mag = mag[i];
        r.phase = phase[i];
        r.norm_mag = mag[i] / mag[1];
        r.norm_phase = phase[i] - phase[1];
        if i > 1 {
            thd += r.norm_mag * r.norm_mag;
        }
    }
    (100.0 * thd.sqrt(), rows)
}

/// `ft_peval`: Horner evaluation of `coeffs[0..=degree]` at `x`.
fn peval(x: f64, coeffs: &[f64], degree: usize) -> f64 {
    let mut y = coeffs[degree];
    for i in (0..degree).rev() {
        y *= x;
        y += coeffs[i];
    }
    y
}

/// `ft_polyfit`: the degree-`degree` polynomial through
/// `(x[i], y[i])`, `i = 0..=degree`, into `result[0..=degree]`, by
/// Gauss–Jordan with partial pivoting; `false` when singular or when
/// the fit misses a point by more than 1e-3 absolute or relative
/// (`ABS_TOL`/`REL_TOL`). Degree 1 is the closed-form line.
fn polyfit(x: &[f64], y: &[f64], result: &mut [f64], degree: usize) -> bool {
    if degree == 1 {
        result[0] = (x[1] * y[0] - x[0] * y[1]) / (x[1] - x[0]);
        result[1] = (y[1] - y[0]) / (x[1] - x[0]);
        return true;
    }
    const ABS_TOL: f64 = 0.001;
    const REL_TOL: f64 = 0.001;
    let n = degree + 1;
    for r in result.iter_mut().take(n) {
        *r = 0.0;
    }
    let mut mat1 = vec![0.0f64; n * n];
    let mut mat2: Vec<f64> = y[..n].to_vec();
    let mut l = 0;
    for &xi in x.iter().take(n) {
        let mut d = 1.0;
        for _ in 0..n {
            mat1[l] = d;
            d *= xi;
            l += 1;
        }
    }
    for i in 0..n {
        // The C seeds `largest` with the signed diagonal.
        let mut largest = mat1[i * n + i];
        let mut lindex = i;
        for j in i..n {
            if mat1[j * n + i].abs() > largest {
                largest = mat1[j * n + i].abs();
                lindex = j;
            }
        }
        if lindex != i {
            for k in 0..n {
                mat1.swap(i * n + k, lindex * n + k);
            }
            mat2.swap(i, lindex);
        }
        if mat1[i * n + i] == 0.0 {
            return false;
        }
        for j in i + 1..n {
            let d = mat1[j * n + i] / mat1[i * n + i];
            for k in 0..n {
                mat1[j * n + k] -= d * mat1[i * n + k];
            }
            mat2[j] -= d * mat2[i];
        }
    }
    for i in (1..n).rev() {
        for j in (0..i).rev() {
            let d = mat1[j * n + i] / mat1[i * n + i];
            for k in 0..n {
                mat1[j * n + k] -= d * mat1[i * n + k];
            }
            mat2[j] -= d * mat2[i];
        }
    }
    for i in 0..n {
        result[i] = mat2[i] / mat1[i * n + i];
    }
    for i in 0..n {
        let d = peval(x[i], result, degree);
        let err = (d - y[i]).abs();
        if err > ABS_TOL || err / (if d.abs() > ABS_TOL { d.abs() } else { ABS_TOL }) > REL_TOL {
            return false;
        }
    }
    true
}

/// `putinterval`: evaluate `poly` at the new-scale points after
/// `last` up to the last one not beyond `oval`; returns that index.
fn putinterval(
    poly: &[f64],
    degree: usize,
    nvec: &mut [f64],
    last: isize,
    nscale: &[f64],
    oval: f64,
    sign: f64,
) -> isize {
    let nlen = nscale.len() as isize;
    let mut end = last + 1;
    while end < nlen {
        if nscale[end as usize] * sign > oval * sign {
            break;
        }
        end += 1;
    }
    end -= 1;
    for i in (last + 1)..=end {
        nvec[i as usize] = peval(nscale[i as usize], poly, degree);
    }
    end
}

/// `ft_interpolate`: resample `data` (on `oscale`) onto `nscale` with
/// sliding polynomials of degree `degree`, lowering the degree where a
/// fit fails, and turning vertical edges into steep slopes. A read
/// the C would make out of bounds (degenerate scales) is an error
/// here.
fn interpolate(
    data: &[f64],
    oscale: &[f64],
    nscale: &[f64],
    degree: usize,
) -> Result<Vec<f64>, String> {
    let olen = oscale.len();
    let nlen = nscale.len();
    if olen < 2 || nlen < 2 {
        return Err("lengths too small to interpolate".into());
    }
    if degree < 1 || degree > olen {
        return Err(format!("degree is {degree}, can't interpolate"));
    }
    let oob = || "bad scale, can't interpolate".to_string();
    let o = |k: usize| oscale.get(k).copied().ok_or_else(oob);
    let dat = |k: usize| data.get(k).copied().ok_or_else(oob);

    let mut sign = 1.0;
    for i in 0..olen - 1 {
        if oscale[i + 1] < oscale[i] {
            sign = -1.0;
            break;
        } else if oscale[i + 1] > oscale[i] {
            sign = 1.0;
            break;
        }
    }

    let mut ndata = vec![0.0f64; nlen];
    let mut result = vec![0.0f64; degree + 1];
    let mut xdata = vec![0.0f64; degree + 1];
    let mut ydata = vec![0.0f64; degree + 1];

    // Initial load, skipping irrelevant points and fudging vertical
    // edges.
    let mut i = 0usize;
    let mut l = 0usize;
    let middle = degree.div_ceil(2); // (degree + 1) / 2
    if sign > 0.0 {
        while l < olen - degree && o(l + middle)? < nscale[0] {
            l += 1;
        }
    } else {
        while l < olen - degree && o(l + middle)? > nscale[0] {
            l += 1;
        }
    }
    ydata[0] = dat(l)?;
    xdata[0] = o(l)?;
    loop {
        if o(l + 1)? == o(l)? {
            if i == 0 {
                l += 1;
                ydata[0] = dat(l)?; // ignore the first point
            } else {
                let diff = xdata[i] - xdata[i - 1];
                xdata[i] -= sign * diff * EDGE_FACTOR;
            }
        }
        i += 1;
        l += 1;
        xdata[i] = o(l)?;
        ydata[i] = dat(l)?;
        if !(i < degree && l < olen - 1) {
            break;
        }
    }
    if i < degree {
        return Err("too few points to calculate polynomial".into());
    }

    let mut off = 0usize;
    let mut tdegree = degree;
    while !polyfit(&xdata[off..], &ydata[off..], &mut result, tdegree) {
        tdegree -= 1;
        if tdegree == 0 {
            return Err("internal error".into());
        }
        if tdegree % 2 == 1 {
            off += 1; // drop the left point
        }
    }
    let mut lastone = putinterval(
        &result,
        tdegree,
        &mut ndata,
        -1,
        nscale,
        xdata[middle],
        sign,
    );

    l += 1;
    while l < olen && lastone < nlen as isize - 1 {
        let out = xdata[0];
        for k in 0..degree {
            xdata[k] = xdata[k + 1];
            ydata[k] = ydata[k + 1];
        }
        let i = degree;
        ydata[i] = data[l];
        xdata[i] = oscale[l];
        // Vertical edge.
        if oscale[l] == xdata[i - 1] {
            let diff = if degree == 1 {
                xdata[0] - out
            } else {
                xdata[i - 1] - xdata[i - 2]
            };
            xdata[i - 1] -= sign * diff * EDGE_FACTOR;
        }
        // Skip input points until the next output point is framed.
        if l < olen - degree {
            let next = nscale[(lastone + 1) as usize];
            if (sign > 0.0 && xdata[middle] < next) || (sign < 0.0 && xdata[middle] > next) {
                l += 1;
                continue;
            }
        }
        let mut off = 0usize;
        let mut tdegree = degree;
        while !polyfit(&xdata[off..], &ydata[off..], &mut result, tdegree) {
            tdegree -= 1;
            if tdegree == 0 {
                return Err("internal error".into());
            }
            if (degree - tdegree) & 1 == 0 {
                off += 1; // drop the left point after the right one
            }
        }
        lastone = putinterval(
            &result,
            tdegree,
            &mut ndata,
            lastone,
            nscale,
            xdata[middle],
            sign,
        );
        l += 1;
    }
    // The C evaluates the tail with the full `degree` even when the
    // last fit was reduced (stale high coefficients included).
    lastone = putinterval(
        &result,
        degree,
        &mut ndata,
        lastone,
        nscale,
        oscale[olen - 1],
        sign,
    );
    if lastone < nlen as isize - 1 {
        ndata[nlen - 1] = data[olen - 1];
    }
    Ok(ndata)
}

impl Solution {
    /// Fourier analysis of one vector of this transient plot (pure:
    /// the solution is only read) — what one name of a `.four FREQ
    /// OV…` card does. `var` is any `.print`-style expression
    /// (`v(2)`, `v(a,b)`, `i(vin)`); [`OutExpr::All`] is rejected
    /// here (it is a list, see [`four_all`]).
    pub fn fourier(
        &self,
        fund: f64,
        var: &OutExpr,
        params: &FourParams,
    ) -> Result<Fourier, FourError> {
        if !(fund.is_finite() && fund > 0.0) {
            return Err(FourError::BadFrequency(fund));
        }
        if self.plotname != "Transient Analysis" {
            return Err(FourError::NoTransient);
        }
        let label = var.label();
        if matches!(var, OutExpr::All) {
            return Err(FourError::UnknownVector(label));
        }
        let sel = self
            .select(std::slice::from_ref(var))
            .map_err(|_| FourError::UnknownVector(label.clone()))?;
        let time = match sel.scale.map(|c| c.data) {
            Some(ColData::Real(t)) => t,
            _ => return Err(FourError::NoTransient),
        };
        let values = match sel.columns.into_iter().next().map(|c| c.data) {
            Some(ColData::Real(v)) => v,
            Some(ColData::Complex(v)) => v.iter().map(|z| z.re).collect(),
            None => return Err(FourError::UnknownVector(label)),
        };
        fourier(&time, &values, fund, params)
    }
}

/// One output vector of one `.four` card.
#[derive(Clone, Debug)]
pub struct FourOutcome {
    /// Index of the card in [`dune_netlist::ir::Netlist::outputs`].
    pub card: usize,
    /// The vector's name as ngspice prints it (`v(2)`, `v(a,b)`,
    /// `i(vin)`; plot names for `all`).
    pub var: String,
    pub result: Result<Fourier, FourError>,
}

/// Evaluate a deck's `.four` cards (the `OutputKind::Four` entries of
/// `outputs`), in card order then name order, on the **last**
/// transient plot of `solutions` (ngspice's `setcplot("tran")` takes
/// the newest). Without a transient plot every name fails with
/// [`FourError::NoTransient`]. `all` expands to every vector of the
/// plot, its scale included (as `ft_evaluate` does).
pub fn four_all(
    outputs: &[Output],
    solutions: &[Solution],
    params: &FourParams,
) -> Vec<FourOutcome> {
    let tran = solutions
        .iter()
        .rev()
        .find(|s| s.plotname == "Transient Analysis");
    let mut out = Vec::new();
    for (card, o) in outputs.iter().enumerate() {
        let OutputKind::Four { freq } = o.kind else {
            continue;
        };
        for v in &o.vars {
            let expr = OutExpr::from_ir(v);
            let exprs: Vec<OutExpr> = match (&expr, tran) {
                (OutExpr::All, Some(sol)) => sol
                    .variables
                    .iter()
                    .map(|(n, _)| OutExpr::Name(n.clone()))
                    .collect(),
                _ => vec![expr],
            };
            for e in exprs {
                let result = match tran {
                    Some(sol) => sol.fourier(freq, &e, params),
                    None => Err(FourError::NoTransient),
                };
                out.push(FourOutcome {
                    card,
                    var: e.label(),
                    result,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// Uniform samples of one period of `f(t)` plus the endpoint.
    fn sampled(f: impl Fn(f64) -> f64, period: f64, n: usize) -> (Vec<f64>, Vec<f64>) {
        let t: Vec<f64> = (0..=n).map(|i| i as f64 * period / n as f64).collect();
        let v = t.iter().map(|&t| f(t)).collect();
        (t, v)
    }

    #[test]
    fn pure_sine_has_unit_fundamental_and_no_distortion() {
        // Samples exactly on the grid: linear interpolation is exact
        // there, so the DFT sees the sine itself.
        let (t, v) = sampled(|t| 2.0 * (2.0 * PI * 1e3 * t).sin(), 1e-3, 200);
        let f = fourier(&t, &v, 1e3, &FourParams::default()).unwrap();
        assert_eq!(f.harmonics.len(), 10);
        assert!(close(f.harmonics[1].mag, 2.0, 1e-12));
        assert!(close(f.harmonics[1].phase, 0.0, 1e-9));
        assert!(close(f.harmonics[0].mag, 0.0, 1e-12));
        assert!(f.thd < 1e-10, "thd {}", f.thd);
        assert_eq!(f.harmonics[3].freq, 3e3);
    }

    #[test]
    fn cosine_reads_ninety_degrees_and_dc_is_signed() {
        let (t, v) = sampled(|t| -0.5 + (2.0 * PI * 50.0 * t).cos(), 0.02, 400);
        let p = FourParams {
            gridsize: 400,
            ..FourParams::default()
        };
        let f = fourier(&t, &v, 50.0, &p).unwrap();
        assert!(close(f.harmonics[0].mag, -0.5, 1e-12));
        assert!(close(f.harmonics[1].phase, 90.0, 1e-9));
    }

    #[test]
    fn square_wave_thd_matches_the_odd_harmonic_series() {
        // ±1 square wave on the grid: b_k = 4/(πk) for odd k (up to
        // the grid's aliasing). THD over k = 2..9 =
        // sqrt(Σ_{k=3,5,7,9} 1/k²) — within the DFT's aliasing error.
        let n = 2000;
        let (t, v) = sampled(|t| if t < 0.5 { 1.0 } else { -1.0 }, 1.0, n);
        let p = FourParams {
            gridsize: n,
            polydegree: 0,
            ..FourParams::default()
        };
        // Raw samples, n + 1 of them (the endpoint repeats the start):
        // drop the endpoint so exactly one period is transformed.
        let f = fourier(&t[..n], &v[..n], 1.0, &p).unwrap();
        let expect = 100.0 * (1.0f64 / 9.0 + 1.0 / 25.0 + 1.0 / 49.0 + 1.0 / 81.0).sqrt();
        assert!(close(f.thd, expect, 0.05), "{} vs {expect}", f.thd);
        assert!(close(f.harmonics[1].mag, 4.0 / PI, 1e-3));
        assert!(f.harmonics[2].mag < 1e-3);
    }

    #[test]
    fn only_the_last_periods_are_transformed() {
        // A transient that settles: the first period is garbage.
        let (t, v) = sampled(
            |t| {
                if t < 1e-3 {
                    5.0
                } else {
                    (2.0 * PI * 1e3 * t).sin()
                }
            },
            2e-3,
            400,
        );
        let f = fourier(&t, &v, 1e3, &FourParams::default()).unwrap();
        assert!(close(f.harmonics[1].mag, 1.0, 1e-9));
        assert!(close(f.harmonics[0].mag, 0.0, 1e-9));
        let two = FourParams {
            nperiods: 2,
            ..FourParams::default()
        };
        let f2 = fourier(&t, &v, 1e3, &two).unwrap();
        assert_eq!(f2.gridsize, 400);
        assert!(f2.harmonics[0].mag > 2.0);
    }

    #[test]
    fn span_too_short_and_bad_frequency_are_errors() {
        let (t, v) = sampled(|t| t, 1e-3, 10);
        assert_eq!(
            fourier(&t, &v, 100.0, &FourParams::default()),
            Err(FourError::SpanTooShort { nperiods: 1 })
        );
        assert_eq!(
            fourier(&t, &v, -1.0, &FourParams::default()),
            Err(FourError::BadFrequency(-1.0))
        );
        assert!(matches!(
            fourier(&t, &v, f64::NAN, &FourParams::default()),
            Err(FourError::BadFrequency(_))
        ));
        assert_eq!(
            fourier(&t[..3], &v, 1e3, &FourParams::default()),
            Err(FourError::LengthMismatch(11, 3))
        );
        assert!(matches!(
            fourier(&[], &[], 1e3, &FourParams::default()),
            Err(FourError::SpanTooShort { .. })
        ));
    }

    #[test]
    fn hostile_sizes_are_refused_not_run() {
        let (t, v) = sampled(|t| t, 1e-3, 10);
        let p = FourParams {
            gridsize: usize::MAX,
            nperiods: 3,
            ..FourParams::default()
        };
        assert!(matches!(
            fourier(&t, &v, 1e3, &p),
            Err(FourError::TooLarge { .. })
        ));
        let p = FourParams {
            nfreqs: 1 << 30,
            ..FourParams::default()
        };
        assert!(matches!(
            fourier(&t, &v, 1e3, &p),
            Err(FourError::TooLarge { .. })
        ));
        // A huge fundamental collapses the window to a point: odd but
        // finite, never a panic.
        let f = fourier(&t, &v, 1e300, &FourParams::default()).unwrap();
        assert!(f.harmonics[0].mag.is_finite());
    }

    #[test]
    fn one_harmonic_has_no_thd_and_no_panic() {
        let (t, v) = sampled(|t| t, 1e-3, 10);
        let p = FourParams {
            nfreqs: 1,
            ..FourParams::default()
        };
        let f = fourier(&t, &v, 1e3, &p).unwrap();
        assert_eq!(f.harmonics.len(), 1);
        assert_eq!(f.thd, 0.0);
    }

    #[test]
    fn linear_interpolation_hits_samples_and_midpoints() {
        let x = [0.0, 1.0, 2.0, 4.0];
        let y = [0.0, 10.0, 20.0, 0.0];
        let n = [0.0, 0.5, 1.5, 3.0, 4.0];
        let r = interpolate(&y, &x, &n, 1).unwrap();
        assert_eq!(r, vec![0.0, 5.0, 15.0, 10.0, 0.0]);
    }

    #[test]
    fn vertical_edges_become_steep_slopes() {
        // A step stored as two samples at the same time (a breakpoint).
        let x = [0.0, 1.0, 1.0, 2.0];
        let y = [0.0, 0.0, 1.0, 1.0];
        let r = interpolate(&y, &x, &[0.5, 0.9999, 1.5], 1).unwrap();
        assert_eq!(r[0], 0.0);
        assert_eq!(r[2], 1.0);
        assert!(r[1] < 1.0);
    }

    #[test]
    fn quadratic_interpolation_is_exact_on_parabolas() {
        let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.1).collect();
        let y: Vec<f64> = x.iter().map(|x| 3.0 * x * x - x + 0.5).collect();
        let n: Vec<f64> = (0..37).map(|i| 0.05 * i as f64).collect();
        let r = interpolate(&y, &x, &n, 2).unwrap();
        for (xn, yn) in n.iter().zip(&r) {
            assert!(close(*yn, 3.0 * xn * xn - xn + 0.5, 1e-9), "{xn}: {yn}");
        }
    }

    #[test]
    fn degenerate_scales_are_errors_not_panics() {
        assert!(interpolate(&[1.0], &[0.0], &[0.0, 1.0], 1).is_err());
        assert!(interpolate(&[1.0, 2.0], &[0.0, 1.0], &[0.0, 1.0], 3).is_err());
        // All abscissae equal: the C reads past the end; here it
        // must simply return (error or values), never panic.
        let _ = interpolate(&[1.0, 2.0, 3.0], &[1.0, 1.0, 1.0], &[0.0, 1.0], 1);
        let _ = interpolate(&[1.0, 2.0], &[1.0, 1.0], &[2.0, 3.0], 1);
        let _ = interpolate(&[1.0, 2.0, 3.0], &[2.0, 1.0, 0.0], &[1.5, 0.5], 2);
    }

    #[test]
    fn options_first_occurrence_wins_and_bad_values_warn() {
        let parsed = dune_netlist::parse_str(
            "opts.cir",
            "opts\n.options nfreqs=3.7\n.options nfreqs=5 polydegree=0 fourgridsize=0 \
             nperiods reltol=1e-4\nr1 1 0 1\n.end\n",
        );
        let mut diags = Vec::new();
        let fp = FourParams::from_options(&parsed.netlist.options, &mut diags);
        assert_eq!(
            fp,
            FourParams {
                nfreqs: 3,
                nperiods: 1,
                polydegree: 0,
                gridsize: 200,
            }
        );
        assert_eq!(diags.len(), 2);
    }
}
