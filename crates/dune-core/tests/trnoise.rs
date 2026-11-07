//! Transient noise (issue #13, part 2): ngspice-47's `TRNOISE` and
//! `TRRANDOM` through whole circuits.
//!
//! - The `corpus/regress/trnoise_*.cir`/`trrandom.cir` probes against
//!   goldens written by Dune itself (`corpus/regress/golden/
//!   *.dune.raw`; ngspice's streams are different numbers, so there is
//!   no ngspice golden to compare sample by sample). Regenerate with
//!   `DUNE_BLESS=1 cargo test -p dune-core --test trnoise`.
//! - Reproducibility: same seed ⇒ same bits; another seed, or another
//!   source name, ⇒ another realization; adding a source leaves the
//!   others' noise alone.
//! - Statistics against closed forms: white noise over a resistor
//!   (variance `NA²` at the grid points, `2NA²/3` time-averaged over
//!   the linear interpolation, the gaussian ±1σ/±2σ mass), and white
//!   noise through an RC (the exact first-order-hold variance).
//! - `.pss` refuses a noisy deck.
//!
//! Numbers are printed (`-- --nocapture`) and recorded in
//! docs/dune-core.md §Ruído transiente.

use std::path::{Path, PathBuf};

use dune_core::{Circuit, Error, Options, PssParams, Solution};
use dune_raw::{Format, Rawfile};

fn corpus(rel: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus")).join(rel)
}

fn load_str(name: &str, text: &str) -> (Circuit, Options, Vec<dune_netlist::ir::Analysis>) {
    let parsed = dune_netlist::parse_str(name, text);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    assert!(d.is_empty(), "{d:?}");
    (c, opts, parsed.netlist.analyses.clone())
}

fn run_str(name: &str, text: &str, tweak: impl Fn(&mut Options)) -> (Solution, Vec<String>) {
    let (c, mut opts, an) = load_str(name, text);
    tweak(&mut opts);
    let (mut sols, notes) = c.run(&an[0], &opts).unwrap();
    (sols.remove(0), notes)
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name)
        .unwrap_or_else(|| panic!("no {name}: {:?}", s.variables))
}

/// The values of `v` at the grid points `k·ts`, k = 1…: the transient
/// steps onto every one of them (each is a breakpoint).
fn at_grid(s: &Solution, v: &str, ts: f64) -> Vec<f64> {
    let t = col(s, "time");
    let y = col(s, v);
    let mut out = Vec::new();
    let mut k = 1.0;
    for (i, &ti) in t.iter().enumerate() {
        if (ti - k * ts).abs() <= 1e-9 * ts {
            out.push(y[i]);
            k += 1.0;
        }
    }
    let expected = (t[t.len() - 1] / ts + 1e-9).floor();
    // The last grid point may coincide with TSTOP (planted by the
    // engine itself); every interior one must be present.
    assert!(
        out.len() as f64 >= expected - 1.0,
        "{v}: {} grid points of {expected}",
        out.len()
    );
    out
}

fn mean_var(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let m = x.iter().sum::<f64>() / n;
    (
        m,
        x.iter().map(|a| (a - m).powi(2)).sum::<f64>() / (n - 1.0),
    )
}

// ---------------------------------------------------------------------
// Probes against Dune's own goldens.

const PROBES: &[&str] = &["trnoise_white", "trnoise_1f_rts", "trrandom"];

#[test]
fn probes_match_their_dune_golden() {
    let bless = std::env::var_os("DUNE_BLESS").is_some();
    for stem in PROBES {
        let path = corpus(&format!("regress/{stem}.cir"));
        let text = std::fs::read_to_string(&path).unwrap();
        let (sol, notes) = run_str(stem, &text, |_| {});
        assert!(
            notes.iter().any(|n| n.contains("stream version 1")),
            "{notes:?}"
        );
        let golden = corpus(&format!("regress/golden/{stem}.dune.raw"));
        if bless {
            let raw = Rawfile {
                plots: vec![sol.to_plot(stem, "")],
                warnings: vec![],
            };
            std::fs::write(&golden, raw.to_bytes(Format::Binary)).unwrap();
            continue;
        }
        let raw = Rawfile::read_from(&golden)
            .unwrap_or_else(|e| panic!("{}: {e} (bless with DUNE_BLESS=1)", golden.display()));
        let plot = &raw.plots[0];
        assert_eq!(plot.num_points(), sol.num_points(), "{stem}: point count");
        for (name, _) in &sol.variables {
            let want = plot.real_values(name).unwrap();
            let got = col(&sol, name);
            // The noise streams are bit-exact on every platform (pinned
            // in the unit tests); the circuit solve around them may
            // differ in the last ulps between LU builds (SIMD), hence
            // a 1e-12 relative ruler instead of bit equality.
            let scale = want.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-30);
            for (i, (a, b)) in got.iter().zip(want).enumerate() {
                assert!(
                    (a - b).abs() <= 1e-12 * scale,
                    "{stem}: {name}[{i}] = {a:e}, golden {b:e}"
                );
            }
        }
    }
    assert!(!bless, "goldens blessed; rerun without DUNE_BLESS");
}

// ---------------------------------------------------------------------
// Reproducibility.

const WHITE: &str = "white noise
vn 1 0 trnoise(1 1u)
r1 1 0 1k
.tran 1u 200u
.end
";

#[test]
fn same_seed_same_bits_other_seed_other_noise() {
    let a = run_str("w", WHITE, |_| {}).0;
    let b = run_str("w", WHITE, |_| {}).0;
    let (va, vb) = (col(&a, "v(1)"), col(&b, "v(1)"));
    assert_eq!(va.len(), vb.len());
    assert!(va.iter().zip(vb).all(|(x, y)| x.to_bits() == y.to_bits()));
    // The default seed is 1.
    let one = run_str("w", WHITE, |o| o.seed = 1).0;
    assert!(
        col(&one, "v(1)")
            .iter()
            .zip(va)
            .all(|(x, y)| x.to_bits() == y.to_bits())
    );
    let c = run_str("w", WHITE, |o| o.seed = 2).0;
    let same = col(&c, "v(1)")
        .iter()
        .zip(va)
        .filter(|(x, y)| x == y)
        .count();
    assert_eq!(same, 1, "only t = 0 coincides"); // the operating point
    // `.options seed=2` on the card is the same as the field.
    let card = WHITE.replace(".end", ".options seed=2\n.end");
    let d = run_str("w", &card, |_| {}).0;
    assert!(
        col(&d, "v(1)")
            .iter()
            .zip(col(&c, "v(1)"))
            .all(|(x, y)| x.to_bits() == y.to_bits())
    );
}

#[test]
fn a_source_keeps_its_noise_when_others_join() {
    let two = WHITE.replace(
        "r1 1 0 1k",
        "r1 1 0 1k\nva 2 0 trnoise(1 0.3u)\nra 2 0 1k\ni9 0 3 trrandom(2 7u)\nr9 3 0 1",
    );
    let a = run_str("w", WHITE, |_| {}).0;
    let b = run_str("w2", &two, |_| {}).0;
    // Different time grids (more breakpoints); compare at v(1)'s own
    // grid points.
    // (A grid point that merges with an earlier breakpoint of the
    // other grid is stepped onto an ulp early: compare to 1e-12.)
    let (ga, gb) = (at_grid(&a, "v(1)", 1e-6), at_grid(&b, "v(1)", 1e-6));
    assert_eq!(ga.len(), gb.len());
    assert!(
        ga.iter().zip(&gb).all(|(x, y)| (x - y).abs() <= 1e-12),
        "{ga:?}\n{gb:?}"
    );
}

// ---------------------------------------------------------------------
// Statistics.

/// White noise on a resistor, 2·10⁵ samples: the grid values are the
/// gaussian samples themselves — variance `NA²` (σ_rel = √(2/N) =
/// 0.32%), mass within ±1σ/±2σ of 68.27%/95.45% (binomial σ ≈ 0.1%),
/// all at 5σ — and the time average of v² over the piecewise-linear
/// interpolation is `⅔·NA²` (E[((1−u)a + ub)²] = ⅔ for independent
/// unit samples; a staircase would give 1).
#[test]
fn white_noise_variance_and_histogram() {
    let na = 0.1;
    let ts = 1e-6;
    let n = 200_000.0;
    let deck = format!(
        "white\n.options seed=42\nvn 1 0 dc 0 trnoise({na} {ts})\nr1 1 0 1k\n.tran {ts} {}\n.end\n",
        n * ts
    );
    let (sol, _) = run_str("white", &deck, |_| {});
    let x = at_grid(&sol, "v(1)", ts);
    let nf = x.len() as f64;
    let (m, v) = mean_var(&x);
    let rel = v / (na * na) - 1.0;
    let within = |k: f64| x.iter().filter(|a| a.abs() <= k * na).count() as f64 / nf;
    let (p1, p2) = (within(1.0), within(2.0));
    // ⅔·NA² from the trapezoid of v² over the accepted points (exact
    // on each linear segment up to the quadratic term; the steps are
    // ≤ TS/2, so the trapezoid overestimates by ≤ (TS/2)²/6·|v″|…: use
    // Simpson-free exact integration of the square of a linear
    // segment instead).
    let t = col(&sol, "time");
    let y = col(&sol, "v(1)");
    let mut acc = 0.0;
    for i in 1..t.len() {
        let (a, b) = (y[i - 1], y[i]);
        acc += (t[i] - t[i - 1]) * (a * a + a * b + b * b) / 3.0;
    }
    let tavg = acc / t[t.len() - 1] / (na * na);
    println!(
        "white: N={nf} mean={m:.3e} var/NA²−1={rel:+.4e} P(±1σ)={p1:.5} P(±2σ)={p2:.5} \
         <v²>_t/NA²={tavg:.5}"
    );
    assert!(m.abs() < 5.0 * na / nf.sqrt());
    assert!(rel.abs() < 5.0 * (2.0 / nf).sqrt());
    assert!((p1 - 0.682_689).abs() < 5.0 * (0.6827 * 0.3173 / nf).sqrt());
    assert!((p2 - 0.954_500).abs() < 5.0 * (0.9545 * 0.0455 / nf).sqrt());
    // σ of the time average ≈ √(2/N)·⅔·(a bit), 5σ ≈ 1.1%.
    assert!((tavg - 2.0 / 3.0).abs() < 0.011, "{tavg}");
}

/// White noise through an RC low-pass, sampled at the grid points:
/// the input is linear between samples, so the exact discretization
/// is the first-order hold `y⁺ = a·y + b₀·x + b₁·x⁺` with
/// `a = e^(−h/τ)`, `b₁ = 1 − τ(1−a)/h`, `b₀ = 1 − a − b₁`, whose
/// stationary variance is `NA²·[b₁² + (a·b₁ + b₀)²/(1 − a²)]`
/// (→ `NA²·h/(2τ)` for `h ≪ τ`, the white-noise bandwidth form).
/// τ = 10·NT, 4·10⁵ samples; the sampling error of the variance of
/// this AR(1)-like sequence is `√(2(1+a²)/((1−a²)N))` = 0.71%, ruler
/// 5σ. The trapezoidal rule's own error at the ~TS/4 steps it takes
/// is ~1e-4 relative, below the ruler.
#[test]
fn rc_filtered_white_noise_matches_the_first_order_hold() {
    let na = 1.0;
    let ts = 1e-6;
    let (r, c) = (1e3, 1e-8);
    let tau = r * c;
    let n = 400_000.0;
    let deck = format!(
        "rc\n.options seed=5\nvn 1 0 trnoise({na} {ts})\nr1 1 2 {r}\nc1 2 0 {c}\n.tran {ts} {}\n.end\n",
        n * ts
    );
    let (sol, _) = run_str("rc", &deck, |_| {});
    let y = at_grid(&sol, "v(2)", ts);
    // Drop 50τ of start-up.
    let y = &y[500..];
    let (_, v) = mean_var(y);
    let a = (-ts / tau).exp();
    let b1 = 1.0 - tau * (1.0 - a) / ts;
    let b0 = 1.0 - a - b1;
    let exact = na * na * (b1 * b1 + (a * b1 + b0).powi(2) / (1.0 - a * a));
    let white = na * na * ts / (2.0 * tau);
    let sigma = (2.0 * (1.0 + a * a) / ((1.0 - a * a) * y.len() as f64)).sqrt();
    println!(
        "rc: var={v:.6e} exact(FOH)={exact:.6e} ({:+.3e}) NA²·NT/2τ={white:.6e} σ_rel={sigma:.3e}",
        v / exact - 1.0
    );
    assert!((v / exact - 1.0).abs() < 5.0 * sigma);
}

/// `I` sources too, and `DC` adds: an `I` TRNOISE into 1 Ω reads
/// minus the source value (current flows + → − through the source).
#[test]
fn current_source_noise_adds_to_dc() {
    let deck = "i\ni1 0 1 dc 2 trnoise(0.5 1u)\nr1 1 0 1\n.tran 1u 50u\n.end\n";
    let (sol, _) = run_str("i", deck, |_| {});
    let v = col(&sol, "v(1)");
    assert_eq!(v[0], 2.0);
    let x = at_grid(&sol, "v(1)", 1e-6);
    let (m, var) = mean_var(&x);
    assert!(
        (m - 2.0).abs() < 0.5 && var > 0.05 && var < 1.0,
        "{m} {var}"
    );
}

#[test]
fn pss_refuses_transient_noise() {
    let deck = "pss\nvs 1 0 sin(0 1 1k)\nvn 1 2 trnoise(1m 1u)\nr1 2 0 1k\n.tran 1u 1m\n.end\n";
    let (c, opts, _) = load_str("pss", deck);
    let err = c
        .pss(
            &opts,
            &PssParams {
                freq: Some(1e3),
                ..PssParams::default()
            },
        )
        .unwrap_err();
    match err {
        Error::Unsupported { name, what } => {
            assert_eq!(name, "vn");
            assert!(what.contains("TRNOISE/TRRANDOM"), "{what}");
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn invalid_noise_arguments_fail_the_build() {
    let deck = "bad\nv1 1 0 trrandom(7 1u)\nr1 1 0 1\n.tran 1u 10u\n.end\n";
    let (c, opts, an) = load_str("bad", deck);
    let err = c.run(&an[0], &opts).unwrap_err();
    assert!(err.to_string().contains("TRRANDOM type"), "{err}");
}
