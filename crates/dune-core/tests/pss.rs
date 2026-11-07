//! `.pss` (issue #13) against closed forms and long transients.
//!
//! - `pss_rc`, `pss_rlc`: linear circuits driven by a sine — the
//!   period must be the forced response `|H| sin(ωt + arg H)` and the
//!   first harmonic `H·(−j)`.
//! - `pss_rect`, `pss_pulse`: nonlinear circuits — the period must be
//!   the last period of a long `.tran` once the start-up has decayed:
//!   ngspice-47's (`corpus/pss/golden/`, `tools/pss-golden.sh`) on
//!   the D-0010 ruler, and Dune's own on a tighter one.
//!
//! The measured numbers are printed (`cargo test --test pss --
//! --nocapture`) and recorded in docs/dune-core.md.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use dune_core::{Circuit, Error, Method, Options, Pss, PssParams, Solution};
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;
use num_complex::Complex64;

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/pss")).to_path_buf()
}

fn load(deck: &str) -> (Circuit, Options, Vec<Analysis>) {
    let parsed = dune_netlist::parse_file(&root().join(format!("{deck}.cir"))).unwrap();
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    (c, opts, parsed.netlist.analyses.clone())
}

/// The deck's `.pss` card as parameters.
fn params(an: &[Analysis]) -> PssParams {
    let Some(Analysis::Pss {
        freq,
        tstab,
        points,
        harmonics,
        ..
    }) = an.iter().find(|a| matches!(a, Analysis::Pss { .. }))
    else {
        panic!("no .pss card");
    };
    PssParams {
        freq: (*freq != 0.0).then_some(*freq),
        tstab: *tstab,
        points: points.map_or(128, |p| p as usize),
        harmonics: harmonics.map_or(10, |h| h as usize),
        ..PssParams::default()
    }
}

fn run(deck: &str, tweak: impl Fn(&mut Options, &mut PssParams)) -> Pss {
    let (c, mut opts, an) = load(deck);
    let mut p = params(&an);
    tweak(&mut opts, &mut p);
    c.pss(&opts, &p).unwrap()
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name).unwrap_or_else(|| panic!("no {name}"))
}

/// Max |PSS − A·|H| sin(ωt + arg H)| over the period, relative to the
/// amplitude, and the first harmonic's relative error.
fn sine_errors(pss: &Pss, out: &str, f: f64, h: Complex64) -> (f64, f64) {
    let t = col(&pss.time, "time");
    let v = col(&pss.time, out);
    let w = 2.0 * PI * f;
    let amp = h.norm();
    let mut worst = 0.0f64;
    for (&t, &v) in t.iter().zip(v) {
        let exact = amp * (w * t + h.arg()).sin();
        worst = worst.max((v - exact).abs() / amp);
    }
    // sin(ωt) = Re(−j e^{jωt}): the phasor of the response is −j·H.
    let x1 = pss.spectrum.column_complex(out).unwrap()[1];
    let want = Complex64::new(0.0, -1.0) * h;
    (worst, (x1 - want).norm() / want.norm())
}

fn tighten(o: &mut Options) {
    o.reltol = 1e-7;
    o.vntol = 1e-12;
    o.abstol = 1e-15;
}

fn rc_h(f: f64) -> Complex64 {
    let wrc = 2.0 * PI * f * 1e3 * 1e-6;
    Complex64::new(1.0, 0.0) / Complex64::new(1.0, wrc)
}

fn rlc_h(f: f64) -> Complex64 {
    let (r, l, c) = (10.0, 10e-3, 1e-6);
    let w = 2.0 * PI * f;
    Complex64::new(1.0, 0.0) / Complex64::new(1.0 - w * w * l * c, w * r * c)
}

#[test]
fn rc_matches_the_forced_response() {
    let h = rc_h(1e3);
    let pss = run("pss_rc", |_, _| {});
    let (e, e1) = sine_errors(&pss, "v(2)", 1e3, h);
    println!(
        "pss_rc default: max err {e:.2e} of amplitude, X1 {e1:.2e}, residuals {:?}",
        pss.residuals
    );
    assert!(pss.residuals.len() <= 3);
    // The period itself is exact; what is left is the transient's
    // own discretization at RELTOL 1e-3 / 128 points.
    assert!(e < 1e-3 && e1 < 1e-3, "{e} {e1}");
    // Tightened: the error follows the integrator, not the shooting.
    let pss = run("pss_rc", |o, p| {
        tighten(o);
        p.points = 1024;
    });
    let (e, e1) = sine_errors(&pss, "v(2)", 1e3, h);
    println!(
        "pss_rc tight: max err {e:.2e}, X1 {e1:.2e}, residuals {:?}",
        pss.residuals
    );
    // (ωh)²/12 = 3.1e-6 at h = T/1024: trapezoidal phase error.
    assert!(e < 5e-6 && e1 < 5e-6, "{e} {e1}");
    // DC is zero, the higher harmonics are numerical noise.
    let v = pss.spectrum.column_complex("v(2)").unwrap();
    assert!(
        v[0].norm() < 1e-6 && v[2].norm() < 1e-6,
        "{} {}",
        v[0],
        v[2]
    );
}

#[test]
fn rlc_matches_the_forced_response_from_far_off() {
    let h = rlc_h(1.5e3);
    let pss = run("pss_rlc", |_, _| {});
    let (e, e1) = sine_errors(&pss, "v(3)", 1.5e3, h);
    println!(
        "pss_rlc default: max err {e:.2e}, X1 {e1:.2e}, residuals {:?}",
        pss.residuals
    );
    // One period into a ring with τ = 2 ms the mismatch is hundreds
    // of tolerances; a linear map needs one Newton step.
    assert!(pss.residuals[0] > 100.0);
    assert_eq!(pss.residuals.len(), 2);
    assert!(e < 2e-3 && e1 < 2e-3, "{e} {e1}");
    let pss = run("pss_rlc", |o, p| {
        tighten(o);
        p.points = 1024;
    });
    let (e, e1) = sine_errors(&pss, "v(3)", 1.5e3, h);
    println!(
        "pss_rlc tight: max err {e:.2e}, X1 {e1:.2e}, residuals {:?}",
        pss.residuals
    );
    // The trapezoidal phase error, raised ~Q-fold near resonance.
    assert!(pss.residuals.len() <= 4);
    assert!(e < 1e-4 && e1 < 1e-4, "{e} {e1}");
}

#[test]
fn every_integration_method_shoots() {
    let h = rc_h(1e3);
    for (m, name) in [(Method::Gear, "gear"), (Method::TrBdf2, "trbdf2")] {
        let pss = run("pss_rc", |o, _| o.method = m);
        let (e, e1) = sine_errors(&pss, "v(2)", 1e3, h);
        println!(
            "pss_rc {name}: max err {e:.2e}, X1 {e1:.2e}, residuals {:?}",
            pss.residuals
        );
        assert!(pss.residuals.len() <= 3 && e < 2e-3, "{name}: {e}");
        let pss = run("pss_rect", |o, _| o.method = m);
        println!("pss_rect {name}: residuals {:?}", pss.residuals);
        assert!(pss.residuals.len() <= 4, "{name}");
    }
}

/// The golden's last period: (times shifted to start at 0, columns).
fn golden_tail(deck: &str, period: f64) -> Solution {
    let raw = Rawfile::read_from(root().join("golden").join(format!("{deck}.raw"))).unwrap();
    let p = raw.plots.into_iter().next().unwrap();
    let mut s = Solution {
        plotname: p.name,
        variables: p.variables.into_iter().map(|v| (v.name, v.vtype)).collect(),
        data: p.data,
        scale: Some(0),
    };
    let dune_raw::Data::Real(cols) = &mut s.data else {
        panic!("real tran")
    };
    let tend = *cols[0].last().unwrap();
    for t in &mut cols[0] {
        *t -= tend - period;
    }
    s
}

/// Linear interpolation of `(ts, ys)` at `t`.
fn lerp(ts: &[f64], ys: &[f64], t: f64) -> f64 {
    let j = ts.partition_point(|&x| x <= t).clamp(1, ts.len() - 1);
    let (t0, t1) = (ts[j - 1], ts[j]);
    ys[j - 1] + (ys[j] - ys[j - 1]) * (t - t0) / (t1 - t0)
}

/// Max |PSS − reference| over the period for `var`, against the
/// D-0010 ruler 2·TRTOL·RELTOL·S (S = the reference's peak).
fn tail_error(pss: &Pss, reference: &Solution, var: &str) -> (f64, f64) {
    let t = col(&pss.time, "time");
    let v = col(&pss.time, var);
    let rt = col(reference, "time");
    let rv = col(reference, var);
    let s = rv.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
    let mut worst = 0.0f64;
    for (&t, &v) in t.iter().zip(v) {
        worst = worst.max((v - lerp(rt, rv, t - pss.t0)).abs());
    }
    (worst, 2.0 * 7.0 * 1e-3 * s)
}

/// Dune's own long transient, last period shifted like the golden.
fn dune_tail(deck: &str, tstep: f64, periods: f64, pss: &Pss, tight: bool) -> Solution {
    let (c, mut opts, _) = load(deck);
    if tight {
        tighten(&mut opts);
    }
    let tstop = periods * pss.period;
    let tr = c
        .tran(&opts, tstep, tstop, tstop - 1.1 * pss.period, None, false)
        .unwrap();
    let mut s = tr.solution;
    let dune_raw::Data::Real(cols) = &mut s.data else {
        panic!()
    };
    for t in &mut cols[0] {
        *t -= tstop - pss.period;
    }
    s
}

#[test]
fn rectifier_matches_the_long_transient() {
    let pss = run("pss_rect", |_, _| {});
    println!("pss_rect: residuals {:?}", pss.residuals);
    assert!(pss.residuals.len() <= 3);
    let gold = golden_tail("pss_rect", pss.period);
    for var in ["v(2)", "v(1)"] {
        let (e, ruler) = tail_error(&pss, &gold, var);
        println!("pss_rect vs ngspice .tran tail {var}: {e:.3e} V (ruler {ruler:.3e})");
        assert!(e < ruler, "{var}: {e} > {ruler}");
    }
    // Dune's .tran on the PSS step (TMAX = T/256): the same
    // discretization, so what is left is the shooting tolerance.
    let own = dune_tail("pss_rect", pss.period / 256.0, 200.0, &pss, false);
    let (e, _) = tail_error(&pss, &own, "v(2)");
    println!("pss_rect vs Dune .tran tail v(2): {e:.3e} V");
    assert!(e < 2e-3, "{e}");
    // Both tightened: they converge onto each other.
    let pss = run("pss_rect", |o, p| {
        tighten(o);
        p.points = 1024;
    });
    println!("pss_rect tight: residuals {:?}", pss.residuals);
    let own = dune_tail("pss_rect", pss.period / 1024.0, 200.0, &pss, true);
    let (e, _) = tail_error(&pss, &own, "v(2)");
    println!("pss_rect tight vs Dune tight .tran tail v(2): {e:.3e} V");
    assert!(e < 1e-4, "{e}");
}

#[test]
fn pulse_train_matches_the_long_transient() {
    let pss = run("pss_pulse", |_, _| {});
    println!("pss_pulse: residuals {:?}", pss.residuals);
    assert!(pss.residuals.len() <= 3);
    let gold = golden_tail("pss_pulse", pss.period);
    for var in ["v(2)", "v(3)"] {
        let (e, ruler) = tail_error(&pss, &gold, var);
        println!("pss_pulse vs ngspice .tran tail {var}: {e:.3e} V (ruler {ruler:.3e})");
        assert!(e < ruler, "{var}: {e} > {ruler}");
    }
    let own = dune_tail("pss_pulse", pss.period / 200.0, 300.0, &pss, false);
    let (e, _) = tail_error(&pss, &own, "v(3)");
    println!("pss_pulse vs Dune .tran tail v(3): {e:.3e} V");
    assert!(e < 5e-3, "{e}");
}

#[test]
fn the_fundamental_comes_from_the_sources() {
    let pss = run("pss_rect", |_, p| p.freq = None);
    assert!((pss.period - 20e-6).abs() < 1e-15);
    assert!(pss.notes.iter().any(|n| n.contains("from the sources")));
}

#[test]
fn too_few_iterations_is_a_structured_error() {
    let (c, opts, an) = load("pss_rlc");
    let p = PssParams {
        max_iter: 1,
        ..params(&an)
    };
    match c.pss(&opts, &p) {
        Err(Error::PssConvergence {
            iterations,
            residuals,
        }) => {
            assert_eq!(iterations, 1);
            assert!(residuals[0] > 1.0);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_deck_card_runs_through_circuit_run() {
    let (c, opts, an) = load("pss_rc");
    let (plots, notes) = c.run(&an[0], &opts).unwrap();
    assert_eq!(plots.len(), 2);
    assert_eq!(plots[0].plotname, dune_core::PSS_TIME_PLOT);
    assert_eq!(plots[1].plotname, dune_core::PSS_FREQ_PLOT);
    assert_eq!(plots[0].num_points(), 129);
    assert_eq!(plots[1].num_points(), 10);
    assert!(notes.iter().any(|n| n.contains("oscnode")));
}

/// ngspice-47's own `.pss` lands on the drive for the rectifier
/// (`corpus/pss/golden/pss_rect.pss.raw`, 50.000002 kHz): its harmonic
/// magnitudes against Dune's.
#[test]
fn rectifier_harmonics_match_ngspice_pss() {
    let pss = run("pss_rect", |_, _| {});
    let raw = Rawfile::read_from(root().join("golden/pss_rect.pss.raw")).unwrap();
    let p = &raw.plots[0];
    let dune_raw::Data::Real(cols) = &p.data else {
        panic!("ngspice writes magnitudes")
    };
    for var in ["v(2)", "v(1)", "i(vin)"] {
        let j = p.variables.iter().position(|v| v.name == var).unwrap();
        let ours = pss.spectrum.column_complex(var).unwrap();
        let scale = col(&pss.time, var)
            .iter()
            .fold(0.0f64, |a, &b| a.max(b.abs()));
        let mut worst = 0.0f64;
        for (k, x) in ours.iter().enumerate() {
            // ngspice's DC row is the signed mean, the rest magnitudes.
            let mine = if k == 0 { x.re } else { x.norm() };
            worst = worst.max((mine - cols[j][k]).abs());
        }
        let ruler = 2.0 * 7.0 * 1e-3 * scale;
        println!(
            "pss_rect harmonics vs ngspice .pss {var}: max |Δ| {worst:.3e} \
             (ruler {ruler:.3e}); DC {:.6e} vs {:.6e}, H1 {:.6e} vs {:.6e}",
            ours[0].re,
            cols[j][0],
            ours[1].norm(),
            cols[j][1]
        );
        assert!(worst < ruler, "{var}: {worst} > {ruler}");
    }
}
