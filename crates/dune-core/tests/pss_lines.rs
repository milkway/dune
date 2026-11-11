//! `.pss` with transmission lines (issue #20): the lines' history in
//! the periodic regime is the orbit itself, `x(t − τ) = x(t − τ + kT)`.
//!
//! - `pss_tra`: two ideal lines (TD shorter and longer than the
//!   period, one into a diode clamp) driven by a pulse train;
//! - `pss_ltra`: lossy RLC and RC lines (recursive convolution,
//!   `.options ltrafit`) and a lossless LTRA behind a diode clamp.
//!
//! References: the last period of a long `.tran` — ngspice-47's
//! (`corpus/pss/golden/`, `tools/pss-golden.sh`) on the D-0010 ruler,
//! and Dune's own on the PSS step (TMAX = T/POINTS). The measured
//! numbers are printed (`cargo test --test pss_lines -- --nocapture`)
//! and recorded in docs/dune-core.md.

use std::path::{Path, PathBuf};

use dune_core::{Circuit, Error, Options, Pss, PssParams, Solution};
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/pss")).to_path_buf()
}

fn load_text(text: &str) -> (Circuit, Options, Vec<Analysis>) {
    let parsed = dune_netlist::parse_str("deck.cir", text);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    // The references here are ngspice transients and Dune's own on
    // the same cards: the SPICE3 profile (trapezoidal `.tran`).
    let mut opts = Options::spice3();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    (c, opts, parsed.netlist.analyses.clone())
}

fn deck(name: &str) -> String {
    std::fs::read_to_string(root().join(format!("{name}.cir"))).unwrap()
}

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

fn run_text(text: &str, tweak: impl Fn(&mut Options, &mut PssParams)) -> Pss {
    let (c, mut opts, an) = load_text(text);
    let mut p = params(&an);
    tweak(&mut opts, &mut p);
    c.pss(&opts, &p).unwrap()
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name).unwrap_or_else(|| panic!("no {name}"))
}

/// The golden's last period, times shifted to start at 0.
fn golden_tail(name: &str, period: f64) -> Solution {
    let raw = Rawfile::read_from(root().join("golden").join(format!("{name}.raw"))).unwrap();
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

/// `(max over the period of the distance from the reference's
/// ±1-step tube, D-0010 ruler 2·TRTOL·RELTOL·S)`: the tube (D-0011)
/// because the lines put sharp corners on the waveforms, which two
/// grids place up to a step apart.
fn tail_error(pss: &Pss, reference: &Solution, var: &str) -> (f64, f64) {
    let t = col(&pss.time, "time");
    let v = col(&pss.time, var);
    let rt = col(reference, "time");
    let rv = col(reference, var);
    let s = rv.iter().fold(0.0f64, |a, &b| a.max(b.abs()));
    let mut worst = 0.0f64;
    for (&t, &v) in t.iter().zip(v) {
        let tt = t - pss.t0;
        let j = rt.partition_point(|&x| x <= tt).clamp(1, rt.len() - 1);
        let (a, b) = (j.saturating_sub(2), (j + 1).min(rt.len() - 1));
        // The linear interpolant too: the reference's window may start
        // a sliver after the period does.
        let dt = rt[j] - rt[j - 1];
        let r = if dt > 0.0 {
            rv[j - 1] + (rv[j] - rv[j - 1]) * (tt - rt[j - 1]) / dt
        } else {
            rv[j]
        };
        let (lo, hi) = rv[a..=b]
            .iter()
            .fold((r, r), |(l, h), &x| (l.min(x), h.max(x)));
        worst = worst.max((lo - v).max(v - hi).max(0.0));
    }
    (worst, 2.0 * 7.0 * 1e-3 * s)
}

/// Max pointwise |PSS − reference| over the period (reference linearly
/// interpolated): the plain distance, reported next to the tube's.
fn plain_error(pss: &Pss, reference: &Solution, var: &str) -> f64 {
    let t = col(&pss.time, "time");
    let v = col(&pss.time, var);
    let rt = col(reference, "time");
    let rv = col(reference, var);
    let mut worst = 0.0f64;
    for (&t, &v) in t.iter().zip(v) {
        let tt = t - pss.t0;
        let j = rt.partition_point(|&x| x <= tt).clamp(1, rt.len() - 1);
        let dt = rt[j] - rt[j - 1];
        // A rawfile repeats the time of a breakpoint row now and then.
        let r = if dt > 0.0 {
            rv[j - 1] + (rv[j] - rv[j - 1]) * (tt - rt[j - 1]) / dt
        } else {
            rv[j]
        };
        worst = worst.max((v - r).abs());
    }
    worst
}

/// Dune's own long transient of `text`, last period shifted like the
/// golden.
fn dune_tail(text: &str, tmax: f64, periods: f64, pss: &Pss, fit: Option<bool>) -> Solution {
    let (c, mut opts, _) = load_text(text);
    if let Some(f) = fit {
        opts.ltra_fit = f;
    }
    let tstop = periods * pss.period;
    let tr = c
        .tran(
            &opts,
            tmax,
            tstop,
            tstop - 1.1 * pss.period,
            Some(tmax),
            false,
        )
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
fn ideal_lines_match_the_long_transient() {
    let text = deck("pss_tra");
    let pss = run_text(&text, |_, _| {});
    println!("pss_tra: residuals {:?}", pss.residuals);
    for n in &pss.notes {
        println!("  {n}");
    }
    assert!(pss.residuals.len() <= 12);
    let gold = golden_tail("pss_tra", pss.period);
    for var in ["v(2)", "v(3)", "v(4)", "v(5)"] {
        let (e, ruler) = tail_error(&pss, &gold, var);
        let plain = plain_error(&pss, &gold, var);
        println!(
            "pss_tra vs ngspice .tran tail {var}: tube {e:.3e} V, plain {plain:.3e} V \
             (ruler {ruler:.3e})"
        );
        assert!(e < ruler, "{var}: {e} > {ruler}");
    }
    let own = dune_tail(&text, pss.period / 256.0, 100.0, &pss, None);
    for var in ["v(3)", "v(5)"] {
        let (e, ruler) = tail_error(&pss, &own, var);
        let plain = plain_error(&pss, &own, var);
        println!(
            "pss_tra vs Dune .tran tail (TMAX = T/256) {var}: tube {e:.3e} V, plain {plain:.3e} V"
        );
        assert!(e < 0.25 * ruler, "{var}: {e}");
    }
}

#[test]
fn lossy_lines_match_the_long_transient() {
    let text = deck("pss_ltra");
    let pss = run_text(&text, |_, _| {});
    println!("pss_ltra: residuals {:?}", pss.residuals);
    for n in &pss.notes {
        println!("  {n}");
    }
    assert!(pss.residuals.len() <= 12);
    let gold = golden_tail("pss_ltra", pss.period);
    for var in ["v(2)", "v(3)", "v(4)", "v(5)", "v(6)", "v(7)"] {
        let (e, ruler) = tail_error(&pss, &gold, var);
        let plain = plain_error(&pss, &gold, var);
        println!(
            "pss_ltra vs ngspice .tran tail {var}: tube {e:.3e} V, plain {plain:.3e} V \
             (ruler {ruler:.3e})"
        );
        assert!(e < ruler, "{var}: {e} > {ruler}");
    }
    // Dune's .tran on the PSS step, with the recursive model and with
    // the exact convolution (the .pss needs the former; the two agree
    // within the fit's budget).
    for fit in [true, false] {
        let own = dune_tail(&text, pss.period / 256.0, 50.0, &pss, Some(fit));
        for var in ["v(3)", "v(5)", "v(7)"] {
            let (e, ruler) = tail_error(&pss, &own, var);
            let plain = plain_error(&pss, &own, var);
            println!(
                "pss_ltra vs Dune .tran tail (TMAX = T/256, ltrafit {fit}) {var}: tube \
                 {e:.3e} V, plain {plain:.3e} V"
            );
            // v(7), the clamp behind the lossless line, gets the full
            // ruler: its worst point is one sample at the arrival
            // corner of the edge (5.31 ns), where the PSS grid and the
            // transient's steps straddle the corner differently. Which
            // sample (v(6) or v(7)) carries it moves with the step
            // sequence; since the engine reports the C's iterate
            // (docs/dune-core.md §Qual iterado, 2026-11-08) it is
            // v(7), 8.6e-3 V against both the Dune and the ngspice
            // transients (it was v(6) at 7.8e-3 V against ngspice).
            let bound = if var == "v(7)" { ruler } else { 0.25 * ruler };
            assert!(e < bound, "{var}: {e}");
        }
    }
}

/// The period mean of `var` over `[t0, t1]` (trapezoidal on the rows).
fn mean_of(s: &Solution, var: &str, t0: f64, t1: f64) -> f64 {
    let t = col(s, "time");
    let v = col(s, var);
    let mut acc = 0.0;
    for j in 1..t.len() {
        let (a, b) = (t[j - 1].max(t0), t[j].min(t1));
        if b > a {
            let at = |x: f64| v[j - 1] + (v[j] - v[j - 1]) * (x - t[j - 1]) / (t[j] - t[j - 1]);
            acc += 0.5 * (at(a) + at(b)) * (b - a);
        }
    }
    acc / (t1 - t0)
}

/// The DC pin (docs/dune-core.md): the mean of a linear network's
/// periodic response is its DC gain times the drive's mean, exactly —
/// here `2.1 V · 100/(10 + R·LEN + 100)` at both lossy lines' far ends.
/// The PSS lands on it; a long transient of the LTRA convolution
/// (ngspice's or Dune's, exact or recursive) drifts away from it like
/// √t, its DC drop being a neutral mode that the discretization pushes.
#[test]
fn lossy_lines_hold_the_exact_dc_level() {
    let text = deck("pss_ltra");
    let pss = run_text(&text, |_, _| {});
    let dc = 2.1 * 100.0 / (10.0 + 0.2 * 24.0 + 100.0);
    let t = col(&pss.time, "time");
    let (a, b) = (t[0], *t.last().unwrap());
    let gold = golden_tail("pss_ltra", pss.period);
    for var in ["v(3)", "v(5)"] {
        let m = mean_of(&pss.time, var, a, b);
        let g = mean_of(&gold, var, 0.0, pss.period);
        println!(
            "pss_ltra mean {var}: exact DC {dc:.6} V; PSS {m:.6} ({:+.2e}); ngspice .tran \
             after 50 periods {g:.6} ({:+.2e})",
            m - dc,
            g - dc
        );
        assert!((m - dc).abs() < 2e-4, "{var}: {m} vs {dc}");
    }
    for periods in [25.0, 100.0, 400.0] {
        let own = dune_tail(&text, pss.period / 256.0, periods, &pss, None);
        let m = mean_of(&own, "v(3)", 0.0, pss.period);
        println!(
            "pss_ltra mean v(3), Dune .tran (ltrafit) after {periods} periods: {m:.6} ({:+.2e})",
            m - dc
        );
    }
}

#[test]
fn lossy_lines_without_ltrafit_are_refused_clearly() {
    let text = deck("pss_ltra").replace(".options ltrafit\n", "");
    let (c, opts, an) = load_text(&text);
    match c.pss(&opts, &params(&an)) {
        Err(Error::Build(m)) => {
            assert!(m.contains("`o1`") && m.contains(".options ltrafit"), "{m}");
        }
        other => panic!("{other:?}"),
    }
    // Lossless lines need no option.
    let lc = "lossless ltra\nv1 1 0 pulse(0 1 0 1n 1n 10n 40n)\nr1 1 2 30\n\
              o1 2 0 3 0 lc\nr2 3 0 200\n\
              .model lc ltra r=0 l=9.13e-9 c=3.65e-12 len=24\n\
              .pss 25meg 200n 3 128 5\n.end\n";
    let pss = run_text(lc, |_, _| {});
    assert!(pss.residuals.last().unwrap() <= &1.0);
    // ltrafit leaves a lossless line on the exact path: same bits.
    let fit = run_text(lc, |o, _| o.ltra_fit = true);
    assert_eq!(pss.time.data, fit.time.data);
}

/// The frequency counter of tests/pss_osc.rs: mean spacing of the
/// rising crossings of the mid-range.
fn counter(t: &[f64], v: &[f64]) -> f64 {
    let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let level = 0.5 * (lo + hi);
    let c: Vec<f64> = (1..t.len())
        .filter(|&j| v[j - 1] < level && v[j] >= level)
        .map(|j| t[j - 1] + (t[j] - t[j - 1]) * (level - v[j - 1]) / (v[j] - v[j - 1]))
        .collect();
    assert!(c.len() >= 3, "{} crossings", c.len());
    (c[c.len() - 1] - c[0]) / (c.len() - 1) as f64
}

/// An autonomous oscillator closed through a line: the period is a
/// Newton unknown and the line's history (the orbit) stretches with
/// it.
#[test]
fn delay_line_oscillator_period() {
    let text = deck("osc_tline");
    let (c, opts, an) = load_text(&text);
    let mut p = params(&an);
    p.osc_node = Some("3".into());
    let pss = c.pss(&opts, &p).unwrap();
    println!(
        "osc_tline: T = {:e} s, residuals {:?}",
        pss.period, pss.residuals
    );
    for n in &pss.notes {
        println!("  {n}");
    }
    let raw = Rawfile::read_from(root().join("golden/osc_tline.raw")).unwrap();
    let plot = &raw.plots[0];
    let dune_raw::Data::Real(cols) = &plot.data else {
        panic!()
    };
    let k = plot
        .variables
        .iter()
        .position(|v| v.name == "v(3)")
        .unwrap();
    let t_ng = counter(&cols[0], &cols[k]);
    // Dune's own .tran on the PSS step over the golden's window.
    let tm = pss.period / 256.0;
    let tr = c.tran(&opts, tm, 100e-9, 90e-9, Some(tm), false).unwrap();
    let t_dune = counter(col(&tr.solution, "time"), col(&tr.solution, "v(3)"));
    // Trapezoidal phase error per period, (2π/N)²/12, summed.
    let ph = |n: f64| (2.0 * std::f64::consts::PI / n).powi(2) / 12.0;
    let n_ng = pss.period / 2e-12;
    let e_ng = (pss.period / t_ng - 1.0).abs();
    let e_dune = (pss.period / t_dune - 1.0).abs();
    // Dune's .tran on the golden's card: the fidelity of the line itself.
    let tr = c.tran(&opts, 2e-12, 100e-9, 90e-9, None, false).unwrap();
    let t_card = counter(col(&tr.solution, "time"), col(&tr.solution, "v(3)"));
    p.points = 1024;
    let fine = c.pss(&opts, &p).unwrap();
    let e_fine = (fine.period / t_ng - 1.0).abs();
    println!(
        "osc_tline: T ngspice .tran {t_ng:e}, Dune .tran (same card) {t_card:e} ({:.2e}); \
         PSS 256 pts {:e}: {e_ng:.2e} from ngspice (phase ruler {:.2e}; the inverter's \
         edges and the line's interpolation are coarser than a sine at 256 points), \
         {e_dune:.2e} from Dune's .tran at T/256; PSS 1024 pts {:e}: {e_fine:.2e} from \
         ngspice (ruler {:.2e}), {} iterations",
        (t_card / t_ng - 1.0).abs(),
        pss.period,
        ph(256.0) + ph(n_ng),
        fine.period,
        ph(1024.0) + ph(n_ng),
        fine.residuals.len()
    );
    assert!((t_card / t_ng - 1.0).abs() < 1e-6);
    assert!(e_dune < 2.0 * ph(256.0), "{e_dune}");
    assert!(e_fine < ph(1024.0) + ph(n_ng), "{e_fine}");
}
