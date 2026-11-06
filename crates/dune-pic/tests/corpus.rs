//! The `corpus/pic` figures: extraction text against the hand-written
//! goldens in `corpus/pic/golden/`, the extracted netlist through the
//! normal `dune-netlist` parser, and a simulation through
//! `Circuit::run` checked against closed forms.

use std::path::PathBuf;

use dune_core::{Circuit, Options, Solution};
use dune_pic::{ExtractOptions, extract};

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/pic")
        .join(rel)
}

/// Whitespace-normalized lines (runs of blanks collapse, blank lines
/// drop).
fn normalize(s: &str) -> Vec<String> {
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Extract `name.pic`, compare with `golden/name.cir`, parse, run
/// every analysis.
fn check(name: &str) -> Vec<Solution> {
    let src = std::fs::read_to_string(corpus(&format!("{name}.pic"))).unwrap();
    let ex = extract(&src, &ExtractOptions::default()).unwrap();
    assert!(
        ex.diagnostics.is_empty(),
        "{name}: unexpected diagnostics: {:#?}",
        ex.diagnostics
    );
    let golden = std::fs::read_to_string(corpus(&format!("golden/{name}.cir"))).unwrap();
    assert_eq!(
        normalize(&ex.netlist),
        normalize(&golden),
        "{name}: extraction differs from golden\n{}",
        ex.netlist
    );

    let parsed = dune_netlist::parse_str(&format!("{name}.cir"), &ex.netlist);
    assert!(parsed.is_clean(), "{name}: {:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let mut sols = Vec::new();
    for a in &parsed.netlist.analyses {
        sols.extend(ckt.run(a, &options).unwrap().0);
    }
    sols
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name).unwrap_or_else(|| {
        panic!(
            "no `{name}` in {}: {:?}",
            s.plotname,
            s.variables.iter().map(|v| &v.0).collect::<Vec<_>>()
        )
    })
}

/// Linear interpolation of column `y` at abscissa `t`.
fn at(t_col: &[f64], y: &[f64], t: f64) -> f64 {
    let k = t_col.partition_point(|&x| x < t).clamp(1, t_col.len() - 1);
    let (t0, t1) = (t_col[k - 1], t_col[k]);
    y[k - 1] + (y[k] - y[k - 1]) * (t - t0) / (t1 - t0)
}

#[test]
fn rc_step_response_is_exponential() {
    let sols = check("rc");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "Transient Analysis");
    let t = col(s, "time");
    let v = col(s, "v(vout)");
    // 5 (1 - exp(-t/tau)), tau = 1 ms; the 1 us rise shifts it by 0.5 us.
    for &tt in &[0.5e-3, 1e-3, 2e-3, 4e-3] {
        let want = 5.0 * (1.0 - (-(tt - 0.5e-6) / 1e-3f64).exp());
        let got = at(t, v, tt);
        assert!(
            (got - want).abs() < 5e-3,
            "v(vout)({tt}) = {got}, want {want}"
        );
    }
    // The input node follows the source.
    assert!((at(t, col(s, "v(vin)"), 2e-3) - 5.0).abs() < 1e-9);
}

#[test]
fn rlc_ac_matches_transfer_function() {
    let sols = check("rlc");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "AC Analysis");
    let f = s.column_complex("frequency").unwrap();
    let v = s.column_complex("v(vout)").unwrap();
    assert_eq!(f.len(), 3 * 20 + 1);
    let (r, l, c) = (20.0, 10e-3, 1e-6);
    let mut peak: (f64, f64) = (0.0, 0.0);
    for (fk, vk) in f.iter().zip(v) {
        let w = 2.0 * std::f64::consts::PI * fk.re;
        // H = 1 / (1 - w^2 LC + j w RC)
        let h = num_complex::Complex64::new(1.0 - w * w * l * c, w * r * c).inv();
        assert!(
            (vk - h).norm() <= 1e-6 * h.norm().max(1e-3),
            "f={} {vk} vs {h}",
            fk.re
        );
        if vk.norm() > peak.1 {
            peak = (fk.re, vk.norm());
        }
    }
    // Q = 5 at f0 = 1.59 kHz (the grid point nearest f0 sits ~1% off).
    assert!((peak.0 - 1591.5).abs() < 60.0, "peak at {} Hz", peak.0);
    assert!(peak.1 > 4.5 && peak.1 < 5.1, "peak |H| = {}", peak.1);
}

#[test]
fn rect_charges_the_reservoir() {
    let sols = check("rect");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    let t = col(s, "time");
    let v = col(s, "v(vout)");
    let peak = v.iter().cloned().fold(f64::MIN, f64::max);
    // One diode drop (plus RS) under the 10 V crest.
    assert!(peak > 9.0 && peak < 9.6, "peak {peak}");
    // Ripple: the load discharges ~ I/C * T = 9.3 mA / 100 uF * 16.7 ms.
    let min_late = t
        .iter()
        .zip(v)
        .filter(|(tt, _)| **tt > 20e-3)
        .map(|(_, vv)| *vv)
        .fold(f64::MAX, f64::min);
    assert!(
        min_late > 7.2 && min_late < peak - 0.5,
        "ripple trough {min_late}"
    );
    // The diode never conducts backwards: v(vout) <= source + small.
    assert!(v.iter().all(|&x| x > -1e-3), "negative output");
}

#[test]
fn bjtamp_biases_and_amplifies() {
    let sols = check("bjtamp");
    assert_eq!(sols.len(), 2);
    let op = &sols[0];
    assert_eq!(op.plotname, "Operating Point");
    let vb = col(op, "v(vb)")[0];
    let ve = col(op, "v(n1)")[0];
    let vc = col(op, "v(vout)")[0];
    // Hand analysis: Vth = 2.105 V, Rth = 8.25k, IB ~ 13 uA, IC ~ 1.3 mA.
    assert!((vb - ve - 0.68).abs() < 0.08, "vbe = {}", vb - ve);
    assert!((ve - 1.33).abs() < 0.1, "ve = {ve}");
    assert!((vc - 5.8).abs() < 0.5, "vc = {vc}");
    assert!((col(op, "v(vcc)")[0] - 12.0).abs() < 1e-9);

    let tr = &sols[1];
    assert_eq!(tr.plotname, "Transient Analysis");
    let t = col(tr, "time");
    let vout = col(tr, "v(vout)");
    // Last full cycle (2..3 ms): peak-to-peak ~ 2 * 10 mV * R3/(R4 + re).
    let late: Vec<f64> = t
        .iter()
        .zip(vout)
        .filter(|(tt, _)| **tt >= 2e-3)
        .map(|(_, v)| *v)
        .collect();
    let pp = late.iter().cloned().fold(f64::MIN, f64::max)
        - late.iter().cloned().fold(f64::MAX, f64::min);
    let gain = pp / 20e-3;
    assert!(gain > 4.0 && gain < 4.7, "|gain| = {gain}");
    // Inverting: at the input crest (2.25 ms) the output is below bias.
    assert!(at(t, vout, 2.25e-3) < vc, "not inverting");
}
