//! `.pss` of autonomous oscillators (issue #20): the period is a
//! Newton unknown. References, per deck of `corpus/pss/osc_*.cir`:
//!
//! - the period of a long `.tran` once the start-up has died out,
//!   measured like a frequency counter (mean spacing of the rising
//!   mid-range crossings of OSCNODE over the saved window):
//!   ngspice-47's (`corpus/pss/golden/`, `tools/pss-golden.sh`) and
//!   Dune's own on the same card;
//! - ngspice-47's own `.pss` (a free-running oscillator search — its
//!   home ground) where the deck carries `*pss-ngspice-card:`;
//! - for the LC deck, Groszkowski's theorem (exact for a tank loaded
//!   by memoryless nonlinearities): `ω²/ω₀² = Σ|V_k|² / Σ k²|V_k|²`.
//!
//! The ruler for a period is the trapezoidal phase error per period,
//! `(2π/N)²/12` relative with `N` steps per period, summed over the
//! two computations compared. The measured numbers are printed
//! (`cargo test --test pss_osc -- --nocapture`) and recorded in
//! docs/dune-core.md.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use dune_core::{Circuit, Error, Options, Pss, PssParams, Solution};
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/pss")).to_path_buf()
}

fn deck_text(deck: &str) -> String {
    std::fs::read_to_string(root().join(format!("{deck}.cir"))).unwrap()
}

fn load(deck: &str) -> (Circuit, Options, Vec<Analysis>) {
    let parsed = dune_netlist::parse_file(&root().join(format!("{deck}.cir"))).unwrap();
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    // The references here are ngspice transients and Dune's own on
    // the same cards: the SPICE3 profile (trapezoidal `.tran`).
    let mut opts = Options::spice3();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    (c, opts, parsed.netlist.analyses.clone())
}

fn params(an: &[Analysis]) -> PssParams {
    let Some(Analysis::Pss {
        freq,
        tstab,
        osc_node,
        points,
        harmonics,
        uic,
        ..
    }) = an.iter().find(|a| matches!(a, Analysis::Pss { .. }))
    else {
        panic!("no .pss card");
    };
    PssParams {
        freq: (*freq != 0.0).then_some(*freq),
        tstab: *tstab,
        osc_node: Some(osc_node.clone()),
        points: points.map_or(128, |p| p as usize),
        harmonics: harmonics.map_or(10, |h| h as usize),
        uic: *uic,
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

/// The frequency counter: mean spacing of the rising crossings of the
/// mid-range, and the largest relative deviation of one interval.
fn counter(t: &[f64], v: &[f64]) -> (f64, f64, usize) {
    let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let level = 0.5 * (lo + hi);
    let c: Vec<f64> = (1..t.len())
        .filter(|&j| v[j - 1] < level && v[j] >= level)
        .map(|j| t[j - 1] + (t[j] - t[j - 1]) * (level - v[j - 1]) / (v[j] - v[j - 1]))
        .collect();
    assert!(c.len() >= 3, "{} crossings", c.len());
    let mean = (c[c.len() - 1] - c[0]) / (c.len() - 1) as f64;
    let spread = c
        .windows(2)
        .map(|w| ((w[1] - w[0]) / mean - 1.0).abs())
        .fold(0.0, f64::max);
    (mean, spread, c.len() - 1)
}

/// The deck's `*tran-golden:` card: (tstep, tstop, tstart, uic).
fn tran_card(deck: &str) -> (f64, f64, f64, bool) {
    let text = deck_text(deck);
    let line = text
        .lines()
        .find_map(|l| l.strip_prefix("*tran-golden:"))
        .expect("tran-golden card");
    let parsed = dune_netlist::parse_str("card.cir", &format!("t\n{line}\n.end\n"));
    match parsed.netlist.analyses.first() {
        Some(Analysis::Tran {
            tstep,
            tstop,
            tstart,
            uic,
            ..
        }) => (*tstep, *tstop, tstart.unwrap_or(0.0), *uic),
        other => panic!("{other:?}"),
    }
}

/// Steps per period of a `.tran` whose TMAX is the default
/// `min(TSTEP, (TSTOP − TSTART)/50)` (the LTE may take smaller ones).
fn tran_steps(card: (f64, f64, f64, bool), period: f64) -> f64 {
    period / f64::min(card.0, (card.1 - card.2) / 50.0)
}

/// Trapezoidal phase error per period, relative, at `n` steps.
fn trap_phase(n: f64) -> f64 {
    (2.0 * PI / n).powi(2) / 12.0
}

/// ngspice-47's long transient: its period by the counter.
fn golden_period(deck: &str, var: &str) -> (f64, f64, usize) {
    let raw = Rawfile::read_from(root().join("golden").join(format!("{deck}.raw"))).unwrap();
    let p = &raw.plots[0];
    let dune_raw::Data::Real(cols) = &p.data else {
        panic!("real tran")
    };
    let j = p.variables.iter().position(|v| v.name == var).unwrap();
    counter(&cols[0], &cols[j])
}

/// ngspice-47's own `.pss`: the fundamental it converged on (row 1 of
/// its frequency-domain plot).
fn ngspice_pss_freq(deck: &str) -> Option<f64> {
    let path = root().join("golden").join(format!("{deck}.pss.raw"));
    let raw = Rawfile::read_from(path).ok()?;
    let dune_raw::Data::Real(cols) = &raw.plots[0].data else {
        panic!("ngspice writes magnitudes")
    };
    Some(cols[0][1])
}

/// Dune's long transient on the golden's card (optionally with TMAX
/// and tightened tolerances): its period by the counter.
fn dune_period(deck: &str, var: &str, tmax: Option<f64>, tight: bool) -> (f64, f64, usize) {
    let (c, mut opts, _) = load(deck);
    if tight {
        tighten(&mut opts);
    }
    let (tstep, tstop, tstart, uic) = tran_card(deck);
    let tr = c.tran(&opts, tstep, tstop, tstart, tmax, uic).unwrap();
    counter(col(&tr.solution, "time"), col(&tr.solution, var))
}

/// RELTOL and VNTOL a thousand times tighter. ABSTOL stays at the
/// default 1e-12: the Wien bridge's op-amp output current
/// `(v(int) − v(out)) / 75 Ω` inherits the rounding of
/// `v(int) = 200k · (v(p) − v(m))`, about `200k · ε / 75 ≈ 4e-13 A`,
/// and where that current crosses zero ABSTOL alone guards the
/// Newton test. Below that floor convergence depends on the linear
/// solver landing on a bit-exact fixed point (the dense LU did at
/// 1e-14; the sparse LU's elimination order does not).
fn tighten(o: &mut Options) {
    o.reltol = 1e-6;
    o.vntol = 1e-9;
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b
}

/// The comparisons every oscillator gets; returns the default PSS.
fn check(deck: &str, var: &str, max_iter: usize) -> Pss {
    let pss = run(deck, |_, _| {});
    let t = pss.period;
    println!(
        "{deck}: T = {t:.9e} s ({:.6} Hz); [residual, T] {:?}",
        1.0 / t,
        pss.residuals.iter().zip(&pss.periods).collect::<Vec<_>>()
    );
    assert!(pss.residuals.len() <= max_iter, "{:?}", pss.residuals);
    assert_eq!(pss.periods.last(), Some(&t));
    // The plot really is one period of OSCNODE.
    let v = col(&pss.time, var);
    let ends = (v[v.len() - 1] - v[0]).abs();
    let swing = v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - v.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(ends < 1e-2 * swing, "{ends} of {swing}");

    let card = tran_card(deck);
    let n_pss = (col(&pss.time, "time").len() - 1) as f64;
    let ruler = trap_phase(n_pss) + trap_phase(tran_steps(card, t));
    let (tg, sg, kg) = golden_period(deck, var);
    println!(
        "{deck} vs ngspice .tran ({kg} periods, spread {sg:.1e}): T {tg:.9e}, rel {:.2e} \
         (ruler {ruler:.2e})",
        rel(t, tg)
    );
    assert!(rel(t, tg) < ruler, "{} > {ruler}", rel(t, tg));
    let (td, sd, kd) = dune_period(deck, var, None, false);
    println!(
        "{deck} vs Dune .tran same card ({kd} periods, spread {sd:.1e}): T {td:.9e}, rel {:.2e}; \
         Dune vs ngspice .tran rel {:.2e}",
        rel(t, td),
        rel(td, tg)
    );
    assert!(rel(t, td) < ruler, "{} > {ruler}", rel(t, td));
    // On the PSS's own TMAX (T/POINTS), Dune's .tran and the shooting
    // integrate the same way: what is left is far below the ruler.
    let (tf, _, _) = dune_period(deck, var, Some(t / n_pss), false);
    println!(
        "{deck} vs Dune .tran at TMAX = T/{n_pss}: T {tf:.9e}, rel {:.2e}",
        rel(t, tf)
    );
    assert!(rel(t, tf) < trap_phase(n_pss), "{}", rel(t, tf));
    if let Some(f) = ngspice_pss_freq(deck) {
        println!(
            "{deck} vs ngspice .pss: f {f:.6} Hz, rel to its own .tran {:.2e}, to Dune .pss {:.2e}",
            rel(1.0 / f, tg),
            rel(1.0 / f, t)
        );
    }
    pss
}

#[test]
fn lc_oscillator_against_transients_and_groszkowski() {
    check("osc_vdp", "v(1)", 3);
    // The diodes clamp hard: Groszkowski's sum needs the harmonics up
    // to high order, and the integrator's phase error must sit below
    // the 1e-4 pull being checked — 1024 points, 200 harmonics.
    let pss = run("osc_vdp", |o, p| {
        tighten(o);
        p.points = 1024;
        p.harmonics = 200;
    });
    // Groszkowski: every element but L and C is memoryless, so the
    // reactive powers balance, Σ k·Q_k = 0 → ω²/ω₀² = Σ|V_k|²/Σk²|V_k|².
    let vk = pss.spectrum.column_complex("v(1)").unwrap();
    let (num, den) = vk
        .iter()
        .enumerate()
        .skip(1)
        .fold((0.0, 0.0), |(a, b), (k, v)| {
            (a + v.norm_sqr(), b + (k * k) as f64 * v.norm_sqr())
        });
    let t0 = 2.0 * PI * (1e-3f64 * 1e-6).sqrt();
    let grosz = t0 * (den / num).sqrt();
    println!(
        "osc_vdp: T0 = 2π√(LC) = {t0:.9e}, T/T0 − 1 = {:.3e}; Groszkowski {grosz:.9e}, rel {:.2e}",
        pss.period / t0 - 1.0,
        rel(pss.period, grosz)
    );
    // The harmonics pull the period 3.4e-3 above the linear tank's;
    // Groszkowski accounts for it to the integrator's phase error.
    assert!(pss.period / t0 - 1.0 > 1e-3);
    assert!(rel(pss.period, grosz) < 2.0 * trap_phase(1024.0));
}

#[test]
fn ring_oscillator_against_transients() {
    check("osc_ring", "v(1)", 3);
}

#[test]
fn phase_shift_oscillator_against_transients() {
    check("osc_phase", "v(c)", 3);
}

#[test]
fn wien_bridge_against_transients() {
    check("osc_wien", "v(out)", 3);
}

#[test]
fn colpitts_against_transients_and_ngspice_pss() {
    let pss = check("osc_colpitts", "v(3)", 4);
    // ngspice's own example, its .pss converges here.
    let f = ngspice_pss_freq("osc_colpitts").unwrap();
    let (tg, _, _) = golden_period("osc_colpitts", "v(3)");
    let ruler = 2.0 * trap_phase(256.0) + trap_phase(tran_steps(tran_card("osc_colpitts"), tg));
    assert!(rel(pss.period, 1.0 / f) < ruler);
}

/// Tightened tolerances and 1024 points: the PSS period and Dune's
/// long transient on the same TMAX converge onto each other.
#[test]
fn tightened_periods_converge() {
    for (deck, var) in [("osc_vdp", "v(1)"), ("osc_wien", "v(out)")] {
        let pss = run(deck, |o, p| {
            tighten(o);
            p.points = 1024;
        });
        let (td, _, _) = dune_period(deck, var, Some(pss.period / 1024.0), true);
        let r = rel(pss.period, td);
        println!(
            "{deck} tight: T {:.9e}, Dune .tran TMAX=T/1024 tight {td:.9e}, rel {r:.2e}; \
             residuals {:?}",
            pss.period, pss.residuals
        );
        assert!(r < 2.0 * trap_phase(1024.0), "{r}");
    }
}

#[test]
fn a_frequency_guess_seeds_the_period() {
    let free = run("osc_vdp", |_, _| {});
    // A stabilization too short to measure the period (1.2 periods,
    // one crossing in its window): FREQ's guess seeds T, and Newton
    // walks it in from 10% off on either side.
    for g in [0.9, 1.1] {
        let guessed = run("osc_vdp", |_, p| {
            p.freq = Some(1.0 / (g * free.period));
            p.tstab = 1.2 * free.period;
        });
        println!(
            "osc_vdp from a guess {g}·T: T {:.9e} vs {:.9e} (rel {:.2e}), [residual, T] {:?}",
            guessed.period,
            free.period,
            rel(guessed.period, free.period),
            guessed
                .residuals
                .iter()
                .zip(&guessed.periods)
                .collect::<Vec<_>>()
        );
        assert!(guessed.notes.iter().any(|n| n.contains("guess")));
        assert!(guessed.residuals.len() <= 6);
        // Two converged orbits differ by their discretization (other
        // start, other grid): the trapezoidal phase error at 256.
        assert!(rel(guessed.period, free.period) < 2.0 * trap_phase(256.0));
    }
    // With crossings to count, the measured period wins over the guess.
    let measured = run("osc_phase", |_, p| p.freq = Some(1.0 / (1.1 * 1.9157e-3)));
    assert!(
        measured
            .notes
            .iter()
            .any(|n| n.contains("FREQ's guess was"))
    );
    assert!(measured.residuals.len() <= 3);
}

#[test]
fn a_quiet_circuit_is_a_clear_error() {
    // No periodic source and nothing that oscillates: an RC at rest.
    let parsed = dune_netlist::parse_str("rc.cir", "rc\nv1 1 0 1\nr1 1 2 1k\nc1 2 0 1u\n.end\n");
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let p = PssParams {
        tstab: 10e-3,
        osc_node: Some("2".into()),
        ..PssParams::default()
    };
    match c.pss(&Options::default(), &p) {
        Err(Error::Build(m)) => assert!(m.contains("does not oscillate"), "{m}"),
        other => panic!("{other:?}"),
    }
    let p = PssParams {
        osc_node: None,
        ..p
    };
    assert!(
        matches!(c.pss(&Options::default(), &p), Err(Error::Build(m)) if m.contains("OSCNODE"))
    );
}

#[test]
fn too_few_iterations_reports_the_last_period() {
    let (c, opts, an) = load("osc_colpitts");
    let p = PssParams {
        max_iter: 1,
        ..params(&an)
    };
    match c.pss(&opts, &p) {
        Err(Error::PssConvergence {
            iterations,
            residuals,
            period,
        }) => {
            assert_eq!(iterations, 1);
            assert_eq!(residuals.len(), 1);
            let t = period.expect("autonomous: the last period");
            assert!(rel(t, 2.99e-7) < 0.05, "{t}");
        }
        other => panic!("{other:?}"),
    }
}

/// The Dune profile shoots with TR-BDF2 (D-0047) after a trapezoidal
/// start-up. Every oscillator converges under it — `osc_colpitts`
/// never starts when the start-up itself runs TR-BDF2 on its
/// TSTAB/1000 grid, wider than the period — and at the deck's 256
/// points its period is closer than the trapezoidal one to the
/// period of a 2048-point shooting (measured: about half the error).
#[test]
fn the_dune_profile_shoots_with_trbdf2() {
    let dune = |o: &mut Options| o.compat = dune_core::Compat::Dune;
    for deck in [
        "osc_phase",
        "osc_ring",
        "osc_vdp",
        "osc_wien",
        "osc_colpitts",
    ] {
        let trap = run(deck, |_, _| {}).period;
        let trbdf2 = run(deck, |o, _| dune(o)).period;
        let fine = run(deck, |_, p| p.points = 2048).period;
        let (et, eb) = (rel(trap, fine), rel(trbdf2, fine));
        println!("{deck}: T {fine:e}; trap {et:.2e}, trbdf2 {eb:.2e}");
        assert!(eb < 0.75 * et, "{deck}: trap {et:e}, trbdf2 {eb:e}");
    }
    // The line oscillator: converges, on the same period.
    let trap = run("osc_tline", |_, _| {});
    let trbdf2 = run("osc_tline", |o, _| dune(o));
    let d = rel(trbdf2.period, trap.period);
    println!(
        "osc_tline: trap {} iterations, trbdf2 {}; periods differ by {d:.2e}",
        trap.residuals.len(),
        trbdf2.residuals.len()
    );
    assert!(d < 1e-4, "{d:e}");

    // An explicit method shoots with it; the start-up stays
    // trapezoidal, so the Colpitts starts under TR-BDF2 and Gear too.
    let colpitts = run("osc_colpitts", |o, _| dune(o)).period;
    for method in [dune_core::Method::TrBdf2, dune_core::Method::Gear] {
        let t = run("osc_colpitts", |o, _| o.method = Some(method)).period;
        println!(
            "osc_colpitts {method:?}: {:.2e} from the default",
            rel(t, colpitts)
        );
        assert!(rel(t, colpitts) < 1e-3, "{method:?}: {t:e}");
    }
}
