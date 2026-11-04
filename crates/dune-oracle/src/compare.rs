//! Rawfile comparison with the SPICE tolerances (PLAN.md §2.4, D-0006).
//!
//! Two values agree when `|a - b| <= floor + reltol * max(|a|, |b|)`,
//! where `floor` is `vntol` for voltage vectors and `abstol` for
//! everything else — the same split the 3f5 Newton convergence test
//! makes between node voltages and branch currents (`NIconvTest`,
//! `src/lib/ckt/niconv.c`).
//!
//! Phase-0 scope: plots are paired in order and must have the same
//! `Plotname`; variables are aligned by name (case-insensitive, like
//! the 3f5 front end's `cieq`); columns must have identical lengths.
//! Interpolating `.tran` onto the oracle's time grid and comparing
//! `.ac` as dB magnitude + phase are Phase-2 work.

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

impl Tolerances {
    /// Absolute floor for a vector of the given type.
    fn floor(&self, vtype: &VarType) -> f64 {
        match vtype {
            VarType::Voltage => self.vntol,
            _ => self.abstol,
        }
    }

    /// Allowed absolute deviation between `a` and `b` for a vector
    /// with absolute floor `floor`.
    fn allowance(&self, a: f64, b: f64, floor: f64) -> f64 {
        floor + self.reltol * a.abs().max(b.abs())
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
}

impl PlotReport {
    pub fn pass(&self) -> bool {
        self.problems.is_empty() && self.worst_ratio <= 1.0
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
    let mut r = PlotReport {
        name: oracle.name.clone(),
        nvars: oracle.variables.len(),
        npoints: oracle.num_points(),
        max_abs_err: 0.0,
        worst_ratio: 0.0,
        worst_var: None,
        problems: Vec::new(),
        notes: Vec::new(),
    };

    for (i, var) in oracle.variables.iter().enumerate() {
        let Some(j) = candidate.variable_index(&var.name) else {
            r.problems.push(format!("variable `{}` missing", var.name));
            continue;
        };
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
        // 0.5 µV apart at ~0 V: inside vntol, far outside abstol.
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
        let bad = raw(plot(vec![v("v(1)")], Data::Real(vec![vec![100.2]])));
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
}
