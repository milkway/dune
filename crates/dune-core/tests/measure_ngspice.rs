//! Dune's measurement layer against ngspice-47's, on ngspice's own
//! data: `corpus/measure/golden/<deck>.raw` is the plot ngspice
//! computed for `corpus/measure/<deck>.cir`, `<deck>.meas` its
//! `.meas` printout at 15 digits (`tools/measure-golden.sh`). Both
//! measure the identical samples, so any difference is the
//! measurement code — the bound is a few ulps of the printed value
//! (`TOL`), from operation order in the interpolations and sums.

use std::collections::HashMap;
use std::path::Path;

use dune_core::{MeasDetail, Solution, measure_all};
use dune_raw::Rawfile;

/// Relative bound: the C and Rust evaluate the same formulas, but
/// e.g. the 3/8-rule sums or `slope·x + yint` interpolations may
/// round differently in the last place, and the printout carries 16
/// significant digits.
/// Observed: ≤ 6e-16 on every deck.
const TOL: f64 = 1e-14;

/// Deliberate departures (`corpus/KNOWN_DIFFS.md`): ngspice fails
/// these, Dune measures them — `(deck, measure, Dune's value)`.
const KNOWN: &[(&str, &str, f64)] = &[
    // A crossing between the first two window points.
    ("meas_edge", "first", 0.06),
];

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/measure"))
}

/// ngspice's printed numbers per measure (value first, then the
/// extras in print order), and the names that failed.
fn golden(name: &str) -> (HashMap<String, Vec<f64>>, Vec<String>) {
    let text = std::fs::read_to_string(root().join("golden").join(format!("{name}.meas"))).unwrap();
    let mut ok = HashMap::new();
    let mut failed = Vec::new();
    for line in text.lines() {
        if let Some(n) = line.strip_prefix("FAILED ") {
            failed.push(n.to_string());
            continue;
        }
        let (n, rest) = line.split_once('=').unwrap();
        let nums: Vec<f64> = rest
            .split_whitespace()
            .filter_map(|w| w.parse().ok())
            .collect();
        ok.insert(n.trim().to_string(), nums);
    }
    (ok, failed)
}

fn ngspice_plot(name: &str) -> Solution {
    let raw = Rawfile::read_from(root().join("golden").join(format!("{name}.raw"))).unwrap();
    let p = raw.plots.into_iter().next().unwrap();
    Solution {
        plotname: p.name,
        variables: p.variables.into_iter().map(|v| (v.name, v.vtype)).collect(),
        data: p.data,
        scale: Some(0),
    }
}

fn numbers(value: f64, d: MeasDetail) -> Vec<f64> {
    match d {
        MeasDetail::None => vec![value],
        MeasDetail::Delay { trig, targ } => vec![value, targ, trig],
        MeasDetail::At { at } => vec![value, at],
        MeasDetail::With { with } => vec![value, with],
        MeasDetail::Window { from, to } => vec![value, from, to],
    }
}

fn check(deck: &str) {
    let parsed = dune_netlist::parse_file(&root().join(format!("{deck}.cir"))).unwrap();
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let measures = &parsed.netlist.measures;
    let sol = ngspice_plot(deck);
    let (want, failed) = golden(deck);
    let outcomes = measure_all(measures, std::slice::from_ref(&sol));
    assert_eq!(outcomes.len(), measures.len());
    let mut worst = 0.0f64;
    for o in &outcomes {
        let name = &measures[o.card].name;
        if let Some(&(_, _, v)) = KNOWN.iter().find(|k| k.0 == deck && k.1 == name) {
            let m = o
                .result
                .as_ref()
                .unwrap_or_else(|e| panic!("{deck}/{name}: {e}"));
            assert!((m.value - v).abs() <= TOL * v.abs(), "{deck}/{name}: {m:?}");
            assert!(
                failed.contains(name),
                "{deck}/{name}: ngspice no longer fails it"
            );
            continue;
        }
        if failed.contains(name) {
            assert!(
                o.result.is_err(),
                "{deck}/{name}: ngspice fails it, Dune gave {:?}",
                o.result
            );
            continue;
        }
        let m = o
            .result
            .as_ref()
            .unwrap_or_else(|e| panic!("{deck}/{name}: {e}"));
        let got = numbers(m.value, m.detail);
        let exp = want
            .get(name)
            .unwrap_or_else(|| panic!("{deck}/{name}: not in golden"));
        assert_eq!(got.len(), exp.len(), "{deck}/{name}: {got:?} vs {exp:?}");
        for (g, e) in got.iter().zip(exp) {
            let scale = g.abs().max(e.abs());
            let err = (g - e).abs();
            // The printout rounds to 16 digits; values that are zero
            // up to rounding noise (a crossing at 0 V, a 1e-16 V
            // minimum) compare absolutely against the deck's scale.
            let ok = err <= TOL * scale || err <= 1e-15;
            assert!(
                ok,
                "{deck}/{name}: Dune {g:e} vs ngspice {e:e} (rel {:.1e})",
                err / scale
            );
            if scale > 1e-12 {
                worst = worst.max(err / scale);
            }
        }
    }
    eprintln!(
        "{deck}: {} measures, worst relative difference {worst:.1e}",
        outcomes.len()
    );
}

#[test]
fn tran_rc_pulse_train() {
    check("meas_rc");
}

#[test]
fn tran_rlc_ringing() {
    check("meas_rlc");
}

#[test]
fn ac_two_pole() {
    check("meas_ac");
}

#[test]
fn dc_diode_clamp() {
    check("meas_dc");
}

#[test]
fn dc_backwards_sweep() {
    check("meas_dcrev");
}

#[test]
fn dc_first_segment_and_flat() {
    check("meas_edge");
}
