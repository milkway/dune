//! Rawfile comparison with the SPICE tolerances (PLAN.md §2.4, D-0006).
//!
//! Two values agree when `|a - b| <= 2 * (floor + reltol * max(|a|,
//! |b|))`, where `floor` is `vntol` for voltage vectors and `abstol`
//! for everything else — the same split the 3f5 Newton convergence
//! test makes between node voltages and branch currents
//! (`NIconvTest`, `src/lib/ckt/niconv.c`). The factor 2 is the
//! triangle inequality, not slack: oracle and candidate are two
//! *independent* Newton solvers, each stopping within one SPICE
//! tolerance of the true solution, so their mutual distance is
//! bounded by twice that tolerance. Anything beyond it is a real
//! disagreement.
//!
//! Plots are paired in order and must have the same `Plotname`;
//! variables are aligned by name (case-insensitive, like the 3f5
//! front end's `cieq`).
//!
//! A transient plot (scale variable `time`) is compared by linear
//! interpolation of the candidate onto the oracle's time grid
//! (PLAN.md §2.4), with a different ruler (D-0010): the step control
//! (`ckttrunc.c`/`cktterr.c`) accepts any step whose estimated local
//! truncation error stays within `trtol · (reltol · signal scale)`,
//! so two independent integrators legitimately differ by up to twice
//! that budget — pointwise `vntol` would reject correct waveforms
//! whose time grids merely disagree (measured on `rc.cir`: both
//! engines sit ~1e-3 from the analytic response, on opposite sides).
//! The transient allowance is therefore
//! `2 · (trtol · reltol · S + floor)` with `S` the vector's waveform
//! scale; the maximum and RMS errors are reported per worst vector.
//!
//! Two guards close holes of those rulers: the sweep scale (the
//! frequency or `.dc` sweep vector) is compared as a grid, at
//! rounding level, and reported as a structural problem when it
//! differs ([`compare_scale`]); and a transient plot may neither
//! exceed `POINTWISE_K` rulers pointwise (the time tube hides a
//! whole-step slip of an edge) nor pass only thanks to the global
//! time shift.
//!
//! Comparing `.ac` as dB magnitude + phase is still pending.

use dune_raw::{Data, Plot, Rawfile, VarType};

/// SPICE comparison tolerances. Defaults are the 3f5 `.options`
/// defaults (`spice.txt`): `reltol=1e-3`, `abstol=1e-12`, `vntol=1e-6`.
#[derive(Clone, Copy, Debug)]
pub struct Tolerances {
    pub reltol: f64,
    pub abstol: f64,
    pub vntol: f64,
}

impl Default for Tolerances {
    fn default() -> Self {
        Tolerances {
            reltol: 1e-3,
            abstol: 1e-12,
            vntol: 1e-6,
        }
    }
}

/// `TRTOL`, the truncation-error overestimation factor of the step
/// control (3f5 default 7): the transient comparison budget.
const TRTOL: f64 = 7.0;

/// The transient pointwise guard (see `compare_tran`): the raw
/// interpolated error, without the time tube, may reach this many
/// LTE rulers `2·(TRTOL·reltol·S + floor)` — `16·0.014·S ≈ 0.22·S`,
/// a fifth of the waveform scale. Measured on the corpus (78 decks):
/// every passing transient stays at ≤ 0.27 rulers pointwise (60×
/// under the limit), while the `tra_uic` regression from before the
/// UIC TSTEP breakpoint fix (an edge one oracle step late, 0.68 V
/// pointwise, invisible to the tube) sits at ~48 rulers. The XFAIL
/// decks (schmitt 52, ltra_3 43, mosamp2 6 pointwise) stay under
/// their caps once divided by the limit.
const POINTWISE_K: f64 = 16.0;

impl Tolerances {
    /// Absolute floor for a vector of the given type.
    fn floor(&self, vtype: &VarType) -> f64 {
        match vtype {
            VarType::Voltage => self.vntol,
            _ => self.abstol,
        }
    }

    /// Allowed absolute deviation between `a` and `b` for a vector
    /// with absolute floor `floor`: twice the SPICE allowance, since
    /// both sides converged independently (see the module docs).
    fn allowance(&self, a: f64, b: f64, floor: f64) -> f64 {
        2.0 * (floor + self.reltol * a.abs().max(b.abs()))
    }
}

/// Comparison outcome for one pair of plots.
#[derive(Debug)]
pub struct PlotReport {
    pub name: String,
    pub nvars: usize,
    pub npoints: usize,
    /// Largest absolute deviation seen (over real and imaginary parts).
    pub max_abs_err: f64,
    /// Largest `|a-b| / allowance` ratio; `<= 1.0` means within
    /// tolerance everywhere.
    pub worst_ratio: f64,
    /// Variable where `worst_ratio` occurred.
    pub worst_var: Option<String>,
    /// Structural problems (missing variable, length mismatch, …).
    pub problems: Vec<String>,
    /// Non-fatal observations (extra variables in the candidate, …).
    pub notes: Vec<String>,
    /// A transient plot that is within the ruler only after a global
    /// time shift (D-0011). Never a PASS: every passing deck of the
    /// corpus passes unshifted, and a shift that rescues a plot is
    /// the signature of a grid slip (see `compare_tran`); the shifted
    /// margin still measures XFAIL entries against their caps.
    pub shift_needed: bool,
}

impl PlotReport {
    pub fn pass(&self) -> bool {
        self.problems.is_empty() && self.worst_ratio <= 1.0 && !self.shift_needed
    }
}

/// Comparison outcome for a whole rawfile pair.
#[derive(Debug)]
pub struct Report {
    pub plots: Vec<PlotReport>,
    /// File-level structural problems (plot count or name mismatch).
    pub problems: Vec<String>,
}

impl Report {
    /// Strict verdict, ignoring XFAIL entries (used by tests; the
    /// CLI applies the XFAIL registry in its reporting instead).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn pass(&self) -> bool {
        self.problems.is_empty() && self.plots.iter().all(PlotReport::pass)
    }
}

/// Compare `candidate` against `oracle`. The oracle drives: every
/// oracle variable must exist in the candidate and agree point by
/// point; extra candidate variables are noted but allowed (Dune may
/// save more vectors than the golden run did).
pub fn compare_rawfiles(oracle: &Rawfile, candidate: &Rawfile, tol: &Tolerances) -> Report {
    let mut report = Report {
        plots: Vec::new(),
        problems: Vec::new(),
    };
    if oracle.plots.len() != candidate.plots.len() {
        report.problems.push(format!(
            "plot count differs: oracle has {}, candidate has {}",
            oracle.plots.len(),
            candidate.plots.len()
        ));
    }
    for (a, b) in oracle.plots.iter().zip(&candidate.plots) {
        if !a.name.eq_ignore_ascii_case(&b.name) {
            report.problems.push(format!(
                "plot name differs: oracle `{}` vs candidate `{}`",
                a.name, b.name
            ));
            continue;
        }
        report.plots.push(compare_plots(a, b, tol));
    }
    report
}

fn compare_plots(oracle: &Plot, candidate: &Plot, tol: &Tolerances) -> PlotReport {
    let is_tran = oracle
        .variables
        .first()
        .is_some_and(|v| v.name.eq_ignore_ascii_case("time"));
    if is_tran {
        return compare_tran(oracle, candidate, tol);
    }
    let mut r = PlotReport {
        name: oracle.name.clone(),
        nvars: oracle.variables.len(),
        npoints: oracle.num_points(),
        max_abs_err: 0.0,
        worst_ratio: 0.0,
        worst_var: None,
        problems: Vec::new(),
        notes: Vec::new(),
        shift_needed: false,
    };

    let scale = is_scale(oracle);
    for (i, var) in oracle.variables.iter().enumerate() {
        let Some(j) = candidate.variable_index(&var.name) else {
            r.problems.push(format!("variable `{}` missing", var.name));
            continue;
        };
        if i == 0 && scale {
            compare_scale(&var.name, &oracle.data, &candidate.data, j, &mut r);
            continue;
        }
        let floor = tol.floor(&var.vtype);
        match (&oracle.data, &candidate.data) {
            (Data::Real(a), Data::Real(b)) => {
                compare_column(&var.name, &a[i], &b[j], floor, tol, &mut r);
            }
            (Data::Complex(a), Data::Complex(b)) => {
                let (ra, ia): (Vec<f64>, Vec<f64>) = a[i].iter().map(|z| (z.re, z.im)).unzip();
                let (rb, ib): (Vec<f64>, Vec<f64>) = b[j].iter().map(|z| (z.re, z.im)).unzip();
                compare_column(&var.name, &ra, &rb, floor, tol, &mut r);
                compare_column(&var.name, &ia, &ib, floor, tol, &mut r);
            }
            _ => {
                r.problems
                    .push("real/complex flag differs between the files".into());
                break;
            }
        }
    }

    for var in &candidate.variables {
        if oracle.variable_index(&var.name).is_none() {
            r.notes
                .push(format!("extra variable `{}` in candidate", var.name));
        }
    }
    r
}

/// Relative ruler of the scale column (see [`compare_scale`]).
const SCALE_RTOL: f64 = 1e-9;

/// Whether a (non-transient) plot's first vector is a sweep scale:
/// the frequency of `.ac`/`.noise`/`.disto`/`.sp`, or a `.dc` sweep
/// vector (`v-sweep`, `i-sweep`, ngspice's `res-sweep`/`temp-sweep`).
/// Operating-point plots have no scale — their first vector is just
/// the first node.
fn is_scale(p: &Plot) -> bool {
    p.variables.first().is_some_and(|v| {
        let n = v.name.to_ascii_lowercase();
        matches!(v.vtype, VarType::Frequency)
            || n == "frequency"
            || ["v-sweep", "i-sweep", "res-sweep", "temp-sweep"]
                .iter()
                .any(|s| n == *s || n == format!("v({s})") || n == format!("i({s})"))
    })
}

/// The scale is a grid, not a solution: both programs compute it by
/// the same arithmetic from the card (`acan.c`'s `freq *= step`,
/// `dctrcurv.c`'s `+= step`), with no Newton tolerance to share. A
/// grid that differs beyond accumulated rounding (`SCALE_RTOL`,
/// relative to the point and to the largest point of the column) is
/// a structural problem — it misplaces every vector — and is
/// reported as such, not as a variable error.
fn compare_scale(name: &str, oracle: &Data, candidate: &Data, j: usize, r: &mut PlotReport) {
    let col = |d: &Data, k: usize| -> Option<Vec<f64>> {
        match d {
            Data::Real(c) => c.get(k).cloned(),
            Data::Complex(c) => c.get(k).map(|v| v.iter().map(|z| z.re).collect()),
        }
    };
    let (Some(a), Some(b)) = (col(oracle, 0), col(candidate, j)) else {
        r.problems.push(format!("scale `{name}` unreadable"));
        return;
    };
    if a.len() != b.len() {
        r.problems.push(format!(
            "scale `{name}` length differs: oracle {} vs candidate {}",
            a.len(),
            b.len()
        ));
        return;
    }
    let colmax = a.iter().chain(&b).fold(0.0f64, |m, v| m.max(v.abs()));
    for (k, (x, y)) in a.iter().zip(&b).enumerate() {
        if x == y {
            continue;
        }
        let err = (x - y).abs();
        let allow = SCALE_RTOL * (x.abs().max(y.abs()) + colmax);
        if err.is_nan() || err > allow {
            r.problems.push(format!(
                "scale `{name}` differs at point {k}: oracle {x:.9e} vs candidate {y:.9e} \
                 (grid mismatch)"
            ));
            return;
        }
    }
}

fn compare_column(
    name: &str,
    a: &[f64],
    b: &[f64],
    floor: f64,
    tol: &Tolerances,
    r: &mut PlotReport,
) {
    if a.len() != b.len() {
        r.problems.push(format!(
            "`{name}` length differs: oracle {} vs candidate {} \
             (interpolation onto the oracle grid is Phase-2 work)",
            a.len(),
            b.len()
        ));
        return;
    }
    for (x, y) in a.iter().zip(b) {
        // Bitwise agreement, equal infinities, or NaN on both sides:
        // no disagreement.
        if x == y || (x.is_nan() && y.is_nan()) {
            continue;
        }
        let err = (x - y).abs();
        if !err.is_finite() {
            r.problems
                .push(format!("`{name}` has non-finite disagreement"));
            return;
        }
        let ratio = err / tol.allowance(*x, *y, floor);
        if err > r.max_abs_err {
            r.max_abs_err = err;
        }
        if ratio > r.worst_ratio {
            r.worst_ratio = ratio;
            r.worst_var = Some(name.to_string());
        }
    }
}

/// Transient comparison: interpolate the candidate onto the oracle
/// grid, judge against the LTE budget (module docs).
fn compare_tran(oracle: &Plot, candidate: &Plot, tol: &Tolerances) -> PlotReport {
    let mut r = PlotReport {
        name: oracle.name.clone(),
        nvars: oracle.variables.len(),
        npoints: oracle.num_points(),
        max_abs_err: 0.0,
        worst_ratio: 0.0,
        worst_var: None,
        problems: Vec::new(),
        notes: Vec::new(),
        shift_needed: false,
    };
    let (Data::Real(oc), Data::Real(cc)) = (&oracle.data, &candidate.data) else {
        r.problems.push("transient plots must be real".into());
        return r;
    };
    let Some(ct_idx) = candidate.variable_index("time") else {
        r.problems.push("candidate has no time vector".into());
        return r;
    };
    let ot = &oc[0];
    let ct = &cc[ct_idx];
    if ct.windows(2).any(|w| w[1] < w[0]) {
        r.problems.push("candidate time is not monotonic".into());
        return r;
    }
    // Both runs must cover the same window (a candidate that died
    // early shows up here, not as clamped interpolation).
    let t_end_o = ot.last().copied().unwrap_or(0.0);
    let t_end_c = ct.last().copied().unwrap_or(0.0);
    if (t_end_o - t_end_c).abs() > 2.0 * tol.reltol * t_end_o.abs() + 1e-15 {
        r.problems.push(format!(
            "time span differs: oracle ends at {t_end_o:.6e}, candidate at {t_end_c:.6e}"
        ));
    }

    // One global time shift per plot (D-0011): self-timed events
    // (regenerative edges) inherit the integrators' accumulated phase
    // error, so the candidate may be compared at `t + delta` for a
    // single delta bounded by the two-sided LTE phase budget over the
    // run. delta = 0 is tried first and kept when it passes.
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (i, var) in oracle.variables.iter().enumerate().skip(1) {
        match candidate.variable_index(&var.name) {
            Some(j) => pairs.push((i, j)),
            None => r.problems.push(format!("variable `{}` missing", var.name)),
        }
    }
    let delta_max = 2.0 * TRTOL * tol.reltol * t_end_o.abs();
    // (gating worst_ratio, worst oracle var index, plot max err,
    // worst rms, worst non-gating ratio + var). Branch currents are
    // derived differential quantities (i(vin) = (v1-v2)/R with
    // |v1-v2| possibly a tiny fraction of the node scales): their
    // own-scale allowance cannot be reconstructed from the rawfile,
    // so in transient plots they are compared and reported but do
    // not gate the verdict (D-0011); node voltages gate.
    let eval = |delta: f64| -> (f64, usize, f64, f64, f64, usize, f64, usize) {
        let mut worst = (
            0.0f64,
            usize::MAX,
            0.0f64,
            0.0f64,
            0.0f64,
            usize::MAX,
            0.0f64,
            usize::MAX,
        );
        let mut plot_max_err = 0.0f64;
        for &(i, j) in &pairs {
            let a = &oc[i];
            let b = &cc[j];
            let scale = a.iter().chain(b.iter()).fold(0.0f64, |m, v| m.max(v.abs()));
            let floor = tol.floor(&oracle.variables[i].vtype);
            let allow = 2.0 * (TRTOL * tol.reltol * scale + floor);
            let mut max_tube = 0.0f64;
            let mut max_point = 0.0f64;
            let mut sum_sq = 0.0f64;
            for (k, t) in ot.iter().enumerate() {
                let ts = t + delta;
                let err = (a[k] - interp(ct, b, ts)).abs();
                if !err.is_finite() {
                    return (
                        f64::INFINITY,
                        i,
                        f64::INFINITY,
                        f64::INFINITY,
                        0.0,
                        usize::MAX,
                        f64::INFINITY,
                        i,
                    );
                }
                plot_max_err = plot_max_err.max(err);
                max_point = max_point.max(err);
                sum_sq += err * err;
                // Time tube: within the oracle's own local sampling
                // interval an edge cannot be located better than one
                // oracle step.
                let tau = oracle_tau(ot, k);
                max_tube = max_tube.max(tube_err(ct, b, ts, a[k], tau));
            }
            let tube_ratio = if max_tube == 0.0 {
                0.0
            } else {
                max_tube / allow
            };
            // The pointwise guard: the tube forgives an edge placed
            // anywhere within one oracle step, which also forgives a
            // whole-step slip of a deterministic edge; bound the raw
            // pointwise error at POINTWISE_K rulers.
            let ratio = tube_ratio.max(max_point / allow / POINTWISE_K);
            let gating = matches!(oracle.variables[i].vtype, VarType::Voltage);
            if gating && ratio > worst.0 {
                worst.0 = ratio;
                worst.1 = i;
                worst.3 = (sum_sq / ot.len().max(1) as f64).sqrt();
            }
            if !gating && ratio > worst.4 {
                worst.4 = ratio;
                worst.5 = i;
            }
            if gating && max_point / allow > worst.6 {
                worst.6 = max_point / allow;
                worst.7 = i;
            }
        }
        worst.2 = plot_max_err;
        worst
    };

    let mut best_delta = 0.0f64;
    let mut best = eval(0.0);
    if best.0 > 1.0 && delta_max > 0.0 {
        let mut candidates: Vec<f64> = (-40..=40).map(|k| delta_max * k as f64 / 40.0).collect();
        let mut step = delta_max / 40.0;
        for _ in 0..2 {
            for &d in &candidates {
                let e = eval(d);
                if e.0 < best.0 {
                    best = e;
                    best_delta = d;
                }
            }
            candidates = (-10..=10)
                .map(|k| best_delta + step * k as f64 / 10.0)
                .collect();
            step /= 10.0;
        }
    }

    let (ratio, worst_i, max_err, rms, ng_ratio, ng_i, pw_ratio, pw_i) = best;
    if best_delta != 0.0 {
        r.shift_needed = true;
        r.notes.push(format!(
            "within the ruler only after a time shift of {best_delta:+.3e}s: not a PASS \
             (a grid slip, or a self-timed XFAIL deck)"
        ));
    }
    if pw_i != usize::MAX {
        r.notes.push(format!(
            "pointwise guard: worst `{}` at {pw_ratio:.3} rulers (limit {POINTWISE_K})",
            oracle.variables[pw_i].name
        ));
    }
    r.worst_ratio = ratio;
    r.max_abs_err = max_err;
    if worst_i != usize::MAX {
        r.worst_var = Some(oracle.variables[worst_i].name.clone());
    }
    if let Some(w) = &r.worst_var {
        r.notes.push(format!(
            "tran ruler (D-0010/11): worst `{w}` pointwise max {max_err:.3e}, \
             rms {rms:.3e}, time shift {best_delta:+.3e}s (budget {delta_max:.1e})",
        ));
    }
    if ng_i != usize::MAX && ng_ratio > 1.0 {
        r.notes.push(format!(
            "non-gating `{}` at {ng_ratio:.1}x its own-scale ruler \
             (derived current; see D-0011)",
            oracle.variables[ng_i].name
        ));
    }
    for var in &candidate.variables {
        if oracle.variable_index(&var.name).is_none() {
            r.notes
                .push(format!("extra variable `{}` in candidate", var.name));
        }
    }
    r
}

/// The oracle's local sampling interval around point `k`.
fn oracle_tau(ot: &[f64], k: usize) -> f64 {
    let before = if k > 0 { ot[k] - ot[k - 1] } else { 0.0 };
    let after = if k + 1 < ot.len() {
        ot[k + 1] - ot[k]
    } else {
        0.0
    };
    f64::max(before, after)
}

/// Minimum |candidate(t') − a| over `t' ∈ [t−τ, t+τ]`, the candidate
/// being piecewise linear over `(ct, b)`.
fn tube_err(ct: &[f64], b: &[f64], t: f64, a: f64, tau: f64) -> f64 {
    let lo = t - tau;
    let hi = t + tau;
    let dl = interp(ct, b, lo) - a;
    let dh = interp(ct, b, hi) - a;
    let mut mn = f64::min(dl.abs(), dh.abs());
    let mut prev = dl;
    let start = ct.partition_point(|x| *x < lo);
    let end = ct.partition_point(|x| *x <= hi);
    for d0 in &b[start..end] {
        let d = d0 - a;
        if d == 0.0 || (d > 0.0) != (prev > 0.0) {
            return 0.0; // the candidate crosses a inside the window
        }
        mn = mn.min(d.abs());
        prev = d;
    }
    if dh != 0.0 && (dh > 0.0) != (prev > 0.0) {
        return 0.0;
    }
    mn
}

/// Linear interpolation of `(xs, ys)` at `x`, clamped at the ends.
fn interp(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    match xs.binary_search_by(|v| v.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Less)) {
        Ok(i) => ys[i],
        Err(0) => ys.first().copied().unwrap_or(0.0),
        Err(i) if i >= xs.len() => ys.last().copied().unwrap_or(0.0),
        Err(i) => {
            let (x0, x1) = (xs[i - 1], xs[i]);
            let (y0, y1) = (ys[i - 1], ys[i]);
            if x1 == x0 {
                y0
            } else {
                y0 + (x - x0) / (x1 - x0) * (y1 - y0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_raw::{VarType, Variable};
    use num_complex::Complex64;

    fn plot(vars: Vec<Variable>, data: Data) -> Plot {
        Plot {
            title: "t".into(),
            date: "d".into(),
            name: "Operating Point".into(),
            padded: true,
            commands: vec![],
            options: vec![],
            dimensions: None,
            variables: vars,
            data,
        }
    }

    fn raw(p: Plot) -> Rawfile {
        Rawfile {
            plots: vec![p],
            warnings: vec![],
        }
    }

    fn v(name: &str) -> Variable {
        Variable::new(name, VarType::Voltage)
    }

    #[test]
    fn identical_files_pass() {
        let a = raw(plot(
            vec![v("v(1)"), Variable::new("i(vin)", VarType::Current)],
            Data::Real(vec![vec![1.0, 2.0], vec![1e-3, -1e-3]]),
        ));
        let rep = compare_rawfiles(&a, &a.clone(), &Tolerances::default());
        assert!(rep.pass(), "{rep:?}");
        assert_eq!(rep.plots[0].max_abs_err, 0.0);
    }

    #[test]
    fn voltage_uses_vntol_floor() {
        let t = Tolerances::default();
        // 0.5 µV apart at ~0 V: inside the vntol allowance, far
        // outside the abstol one.
        let a = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![0.0]])));
        let b = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![5e-7]])));
        assert!(compare_rawfiles(&a, &b, &t).pass());
        // Same gap on a current vector must fail (abstol floor).
        let a = raw(plot(
            vec![Variable::new("i(vin)", VarType::Current)],
            Data::Real(vec![vec![0.0]]),
        ));
        let b = raw(plot(
            vec![Variable::new("i(vin)", VarType::Current)],
            Data::Real(vec![vec![5e-7]]),
        ));
        assert!(!compare_rawfiles(&a, &b, &t).pass());
    }

    #[test]
    fn relative_term_scales_with_magnitude() {
        let t = Tolerances::default();
        let a = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![100.0]])));
        let ok = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![100.05]])));
        // 3e-3 relative: beyond even the doubled allowance.
        let bad = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![100.3]])));
        assert!(compare_rawfiles(&a, &ok, &t).pass());
        assert!(!compare_rawfiles(&a, &bad, &t).pass());
    }

    #[test]
    fn missing_variable_fails_and_extra_is_a_note() {
        let t = Tolerances::default();
        let oracle = raw(plot(
            vec![v("v(1)"), v("v(2)")],
            Data::Real(vec![vec![1.0], vec![2.0]]),
        ));
        let cand = raw(plot(
            vec![v("v(1)"), v("v(3)")],
            Data::Real(vec![vec![1.0], vec![3.0]]),
        ));
        let rep = compare_rawfiles(&oracle, &cand, &t);
        assert!(!rep.pass());
        assert!(rep.plots[0].problems[0].contains("v(2)"));
        assert!(rep.plots[0].notes[0].contains("v(3)"));
    }

    #[test]
    fn variable_names_align_case_insensitively() {
        let t = Tolerances::default();
        let a = raw(plot(vec![v("V(OUT)")], Data::Real(vec![vec![1.0]])));
        let b = raw(plot(vec![v("v(out)")], Data::Real(vec![vec![1.0]])));
        assert!(compare_rawfiles(&a, &b, &t).pass());
    }

    #[test]
    fn complex_columns_compare_both_parts() {
        let t = Tolerances::default();
        let a = raw(plot(
            vec![v("v(1)")],
            Data::Complex(vec![vec![Complex64::new(1.0, -1.0)]]),
        ));
        let ok = raw(plot(
            vec![v("v(1)")],
            Data::Complex(vec![vec![Complex64::new(1.0005, -1.0005)]]),
        ));
        let bad = raw(plot(
            vec![v("v(1)")],
            Data::Complex(vec![vec![Complex64::new(1.0, -1.1)]]),
        ));
        assert!(compare_rawfiles(&a, &ok, &t).pass());
        assert!(!compare_rawfiles(&a, &bad, &t).pass());
    }

    #[test]
    fn length_mismatch_is_structural() {
        let t = Tolerances::default();
        let a = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![1.0, 2.0]])));
        let b = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![1.0]])));
        let rep = compare_rawfiles(&a, &b, &t);
        assert!(!rep.pass());
        assert!(rep.plots[0].problems[0].contains("length"));
    }

    #[test]
    fn nan_on_one_side_only_fails() {
        let t = Tolerances::default();
        let a = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![f64::NAN]])));
        let b = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![0.0]])));
        assert!(!compare_rawfiles(&a, &b, &t).pass());
        // NaN on both sides at the same point: no disagreement.
        assert!(compare_rawfiles(&a, &a.clone(), &t).pass());
    }

    fn tran(times: Vec<f64>, vals: Vec<f64>) -> Rawfile {
        let mut p = plot(
            vec![Variable::new("time", VarType::Time), v("v(1)")],
            Data::Real(vec![times, vals]),
        );
        p.name = "Transient Analysis".into();
        raw(p)
    }

    /// A unit step at `edge`, sampled on `0, dt, 2dt, … 10`.
    fn step(edge: f64, dt: f64) -> Rawfile {
        let n = (10.0 / dt).round() as usize;
        let t: Vec<f64> = (0..=n).map(|k| k as f64 * dt).collect();
        let y = t
            .iter()
            .map(|&x| if x >= edge { 1.0 } else { 0.0 })
            .collect();
        tran(t, y)
    }

    #[test]
    fn scale_grid_mismatch_is_structural() {
        let t = Tolerances::default();
        let ac = |f2: f64| {
            let mut p = plot(
                vec![Variable::new("frequency", VarType::Frequency), v("v(1)")],
                Data::Complex(vec![
                    vec![Complex64::new(1e3, 0.0), Complex64::new(f2, 0.0)],
                    vec![Complex64::new(1.0, 0.0); 2],
                ]),
            );
            p.name = "AC Analysis".into();
            raw(p)
        };
        assert!(compare_rawfiles(&ac(1e4), &ac(1e4), &t).pass());
        // 1e-4 relative: inside the variable ruler, a wrong grid.
        let rep = compare_rawfiles(&ac(1e4), &ac(1.0001e4), &t);
        assert!(!rep.pass());
        assert!(rep.plots[0].problems[0].contains("scale"), "{rep:?}");
    }

    #[test]
    fn tran_whole_step_slip_fails_the_pointwise_guard() {
        let t = Tolerances::default();
        let oracle = step(5.0, 1.0);
        assert!(compare_rawfiles(&oracle, &step(5.0, 0.1), &t).pass());
        // Edge 0.9 late: inside the ±1-step tube, beyond the shift
        // budget (2·7·1e-3·10 = 0.14), 71 rulers pointwise.
        let rep = compare_rawfiles(&oracle, &step(5.9, 0.1), &t);
        assert!(!rep.pass(), "{rep:?}");
    }

    #[test]
    fn tran_passing_only_after_a_shift_is_not_a_pass() {
        let t = Tolerances::default();
        let rep = compare_rawfiles(&step(5.0, 1.0), &step(5.1, 0.01), &t);
        assert!(rep.plots[0].shift_needed, "{rep:?}");
        assert!(rep.plots[0].worst_ratio <= 1.0);
        assert!(!rep.pass());
    }
}
