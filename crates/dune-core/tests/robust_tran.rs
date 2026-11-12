//! The opt-in robust transient options of issue #6: `method=trbdf2`
//! (TR-BDF2 composite) and `ltrabkrate=N` (LTRA breakpoint hygiene).
//! The faithful default is the oracle's business (`dune-oracle --ci`);
//! these check the robust modes against physics and closed forms.

use dune_core::{Circuit, Method, Options};

fn corpus(rel: &str) -> String {
    let path = format!("{}/../../corpus/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Parse a deck, apply its `.options`, return the circuit, options and
/// the first `.tran`.
fn load(name: &str, text: &str) -> (Circuit, Options, dune_netlist::ir::Analysis) {
    let parsed = dune_netlist::parse_str(name, text);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut diags = Vec::new();
    opts.apply(&parsed.netlist.options, &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
    let tran = parsed.netlist.analyses[0].clone();
    (ckt, opts, tran)
}

fn run(ckt: &Circuit, opts: &Options, a: &dune_netlist::ir::Analysis) -> dune_core::Solution {
    let (mut sols, _) = ckt.run(a, opts).unwrap();
    sols.remove(0)
}

/// Sign changes of successive differences: a period-2 ring flips on
/// every point.
fn alternations(v: &[f64]) -> usize {
    v.windows(3)
        .filter(|w| (w[2] - w[1]) * (w[1] - w[0]) < 0.0)
        .count()
}

#[test]
fn options_parse_the_robust_values() {
    let (_, opts, _) = load(
        "o.cir",
        "o\nr1 1 0 1\nv1 1 0 1\n.options method=trbdf2 ltrabkrate=10\n.tran 1n 10n\n.end\n",
    );
    assert_eq!(opts.method, Some(Method::TrBdf2));
    assert_eq!(opts.ltra_break_rate, Some(10));
    let d = Options::default();
    assert_eq!(d.method, None);
    assert_eq!(d.ltra_break_rate, None);
}

/// The defaults profile (D-0046): a `.tran` without `method=`
/// integrates with TR-BDF2 under Dune's profile and with the
/// trapezoidal rule under `compat=spice3`; the `.pss` shooting
/// follows (D-0047); an explicit method wins in both, wherever it
/// sits on the card. The start-up of an autonomous `.pss` is always
/// trapezoidal.
#[test]
fn the_profile_picks_the_default_method() {
    use dune_core::Compat;
    let deck = |opt: &str| format!("o\nr1 1 0 1\nv1 1 0 1\n{opt}\n.tran 1n 10n\n.end\n");
    let opts = |opt: &str| load("o.cir", &deck(opt)).1;

    let d = opts("");
    assert_eq!(d.compat, Compat::Dune);
    assert_eq!(d.tran_method(), Method::TrBdf2);
    assert_eq!(d.pss_method(), Method::TrBdf2);
    assert_eq!(d.pss_stab_method(), Method::Trapezoidal);

    let s = opts(".options compat=spice3");
    assert_eq!((s.compat, s.method), (Compat::Spice3, None));
    assert_eq!(s.tran_method(), Method::Trapezoidal);
    assert_eq!(s.pss_method(), Method::Trapezoidal);
    assert_eq!(s.pss_stab_method(), Method::Trapezoidal);
    assert_eq!(Options::spice3().tran_method(), Method::Trapezoidal);

    for card in [
        ".options compat=spice3 method=gear",
        ".options method=gear compat=spice3",
    ] {
        let g = opts(card);
        assert_eq!(g.tran_method(), Method::Gear, "{card}");
        assert_eq!(g.pss_method(), Method::Gear, "{card}");
        assert_eq!(g.pss_stab_method(), Method::Trapezoidal, "{card}");
    }
    assert_eq!(opts(".options compat=dune").tran_method(), Method::TrBdf2);
    // An unknown profile is a warning and changes nothing.
    let parsed = dune_netlist::parse_str("o.cir", &deck(".options compat=hspice"));
    let (mut o, mut diags) = (Options::default(), Vec::new());
    o.apply(&parsed.netlist.options, &mut diags);
    assert_eq!((o.compat, diags.len()), (Compat::Dune, 1));
}

/// `corpus/regress/trbdf2_ring.cir`: the dioload diode string of
/// ltra_3. The trapezoidal rule rings period-2 on the string's inner
/// nodes (≈0.7 of all points alternate); TR-BDF2 does not.
#[test]
fn trbdf2_does_not_ring_where_trap_does() {
    let (ckt, opts, tran) = load("ring.cir", &corpus("regress/trbdf2_ring.cir"));
    assert_eq!(opts.method, Some(Method::TrBdf2));
    let tb = run(&ckt, &opts, &tran);
    let trap = run(
        &ckt,
        &Options {
            method: Some(Method::Trapezoidal),
            ..opts.clone()
        },
        &tran,
    );
    for node in ["v(2)", "v(3)"] {
        let (a, n) = (tb.column(node).unwrap(), trap.column(node).unwrap());
        let (alt_tb, alt_trap) = (alternations(a), alternations(n));
        assert!(
            alt_trap * 2 > n.len(),
            "trap {node}: {alt_trap}/{}",
            n.len()
        );
        assert!(alt_tb * 10 < a.len(), "trbdf2 {node}: {alt_tb}/{}", a.len());
    }
}

/// `rc.cir` (3f5 example): a 1 Ω / 1 F RC driven by a 0→1 V ramp over
/// 0.1 s, closed form known. TR-BDF2's error must not exceed the
/// trapezoidal rule's at the same TMAX.
#[test]
fn trbdf2_error_does_not_exceed_trap_on_rc() {
    let (ckt, opts, tran) = load("rc.cir", &corpus("spice3f5/examples/rc.cir"));
    let exact = |t: f64| {
        let tr = 0.1;
        if t <= tr {
            (t - (1.0 - (-t).exp())) / tr
        } else {
            1.0 - (tr.exp() - 1.0) / tr * (-t).exp()
        }
    };
    let err = |method| {
        let sol = run(
            &ckt,
            &Options {
                method: Some(method),
                ..opts.clone()
            },
            &tran,
        );
        let t = sol.column("time").unwrap();
        let v = sol.column("v(2)").unwrap();
        t.iter()
            .zip(v)
            .map(|(&t, &v)| (v - exact(t)).abs())
            .fold(0.0, f64::max)
    };
    let (e_trap, e_tb) = (err(Method::Trapezoidal), err(Method::TrBdf2));
    assert!(e_trap > 1e-5 && e_trap < 1e-3, "{e_trap}");
    assert!(e_tb <= e_trap, "trbdf2 {e_tb} vs trap {e_trap}");
}

/// The breakpoint hygiene leaves a deck without an echo cascade
/// untouched: `regress/ltra_rc.cir` (slope breakpoints at every pulse
/// edge) is bit-identical with `ltrabkrate` on and off. (Measured too:
/// ltra_1, ltra_2, ltra_3 on arm64 and ltra_rg — all bit-identical.)
#[test]
fn ltra_hygiene_is_a_no_op_without_a_cascade() {
    let (ckt, opts, tran) = load("ltra_rc.cir", &corpus("regress/ltra_rc.cir"));
    // On by default under the Dune profile (D-0047), off with 0.
    assert_eq!(opts.ltra_slope_rate(), Some(dune_core::LTRA_BREAK_RATE));
    assert_eq!(Options::spice3().ltra_slope_rate(), None);
    let with = |rate| Options {
        ltra_break_rate: Some(rate),
        ..opts.clone()
    };
    assert_eq!(with(0).ltra_slope_rate(), None);
    assert_eq!(with(4).ltra_slope_rate(), Some(4));
    let off = run(&ckt, &with(0), &tran);
    let on = run(&ckt, &opts, &tran);
    assert_eq!(off.data, on.data);
}

/// A voltage switch (closes at 3.05 V, opens at 2.05 V) and a current
/// switch (closes at 3.05 mA, opens at 2.05 mA) on a 0 → 5 → 0
/// triangle of 1 ms: the crossings are at 0.305 ms and 0.795 ms. `.tran 20u`
/// gives a 20 µs grid that holds neither.
const SWITCH_DECK: &str = "switch events
vc 1 0 pwl(0 0 0.5m 5 1m 0)
rc 1 0 1k
vs 1 9 0
rs 9 0 1k
vdd 2 0 10
s1 2 3 1 0 smod
r1 3 0 1k
w1 2 4 vs wmod
r2 4 0 1k
.model smod sw vt=2.55 vh=0.5 ron=1 roff=1e6
.model wmod csw it=2.55m ih=0.5m ron=1 roff=1e6
.tran 20u 1m
.end
";

/// The times at which column `name` crosses 5 V between two rows, as
/// `(last row before, first row after)`.
fn flips(sol: &dune_core::Solution, name: &str) -> Vec<(f64, f64)> {
    let dune_raw::Data::Real(cols) = &sol.data else {
        panic!("real data");
    };
    let k = sol.variables.iter().position(|(n, _)| n == name).unwrap();
    (1..cols[0].len())
        .filter(|&i| (cols[k][i] > 5.0) != (cols[k][i - 1] > 5.0))
        .map(|i| (cols[0][i - 1], cols[0][i]))
        .collect()
}

/// Event location (D-0047): under the Dune profile a switch flip is
/// bracketed by two accepted points `RELTOL·TMAX` apart that hold the
/// instant its control crossed the threshold; under `compat=spice3`
/// it lands on whatever timepoint first sees the control past it.
#[test]
fn switch_flips_are_located_under_the_dune_profile() {
    let (ckt, opts, tran) = load("sw.cir", SWITCH_DECK);
    let sol = run(&ckt, &opts, &tran);
    let tol = 1e-3 * 20e-6;
    for name in ["v(3)", "v(4)"] {
        let f = flips(&sol, name);
        assert_eq!(f.len(), 2, "{name}: {f:?}");
        for ((before, after), at) in f.into_iter().zip([0.305e-3, 0.795e-3]) {
            assert!(
                before < at && at < after,
                "{name}: {before:e} {at:e} {after:e}"
            );
            assert!(
                after - before <= tol * (1.0 + 1e-9),
                "{name}: {:e}",
                after - before
            );
        }
    }

    let spice3 = Options::spice3();
    let sol = run(&ckt, &spice3, &tran);
    for name in ["v(3)", "v(4)"] {
        let f = flips(&sol, name);
        assert_eq!(f.len(), 2, "{name}: {f:?}");
        assert!(
            f.iter().all(|(before, after)| after - before > 100.0 * tol),
            "{name}: {f:?}"
        );
    }
}

/// The located flip does not depend on the grid: the same deck at a
/// tenth of the step puts its flips within both resolutions of the
/// same instants, where the unlocated runs differ by a whole step.
#[test]
fn located_flips_do_not_move_with_the_step() {
    let (ckt, opts, tran) = load("sw.cir", SWITCH_DECK);
    let fine_deck = SWITCH_DECK.replace(".tran 20u 1m", ".tran 20u 1m 0 2u");
    let (_, _, fine) = load("sw.cir", &fine_deck);
    let (a, b) = (run(&ckt, &opts, &tran), run(&ckt, &opts, &fine));
    for name in ["v(3)", "v(4)"] {
        for (x, y) in flips(&a, name).into_iter().zip(flips(&b, name)) {
            assert!((x.1 - y.1).abs() <= 1e-3 * 20e-6, "{name}: {x:?} {y:?}");
        }
    }
}

/// A switch that controls itself: a relaxation oscillator (10 V
/// through 10 kΩ into 100 nF, a 100 Ω switch across the capacitor
/// closing at 7 V and opening at 3 V). The flip discharges the node
/// the switch watches, so the step that flips ends with the control
/// back below the level — the crossing is extrapolated, not
/// interpolated. Period, closed form: `τc·ln(7/3) + τd·ln((7−v∞)/
/// (3−v∞))` with the discharge's Thevenin `v∞`, `τd`. Located, the
/// run is within 5e-4 of it and every cycle is the same; on the grid
/// (`compat=spice3`) it is off by more than 1 %.
#[test]
fn a_self_controlled_switch_is_located() {
    let deck = "relaxation oscillator
vdd 1 0 10
r1 1 2 10k
c1 2 0 100n ic=0
s1 2 0 2 0 sdis
.model sdis sw vt=5 vh=2 ron=100 roff=1e9
.tran 50u 20m uic
.end
";
    let (ckt, opts, tran) = load("relax.cir", deck);
    let (rp, c): (f64, f64) = (1e4 * 100.0 / 1.01e4, 100e-9);
    let vinf: f64 = 10.0 * 100.0 / 1.01e4;
    let exact = 1e4 * c * (7.0f64 / 3.0).ln() + rp * c * ((7.0 - vinf) / (3.0 - vinf)).ln();
    let periods = |o: &Options| {
        let sol = run(&ckt, o, &tran);
        let dune_raw::Data::Real(cols) = &sol.data else {
            panic!("real data");
        };
        let k = sol.variables.iter().position(|(n, _)| n == "v(2)").unwrap();
        let (t, v) = (&cols[0], &cols[k]);
        let peaks: Vec<f64> = (1..t.len() - 1)
            .filter(|&i| v[i] >= v[i - 1] && v[i] > v[i + 1] && v[i] > 6.0)
            .map(|i| t[i])
            .collect();
        peaks.windows(2).map(|w| w[1] - w[0]).collect::<Vec<f64>>()
    };
    let located = periods(&opts);
    assert!(located.len() >= 20, "{}", located.len());
    for p in &located {
        assert!((p - exact).abs() < 5e-4 * exact, "{p:e} vs {exact:e}");
    }
    let grid = periods(&Options::spice3());
    let mean = grid.iter().sum::<f64>() / grid.len() as f64;
    assert!((mean - exact).abs() > 1e-2 * exact, "{mean:e} vs {exact:e}");
}

/// Event location for the B source (D-0051): `corpus/regress/
/// bsrc_step.cir`. Under the Dune profile each jump of a `u()` or
/// `sgn()` is bracketed by two accepted points `RELTOL·TMAX` apart
/// around the instant its argument crossed zero; a `sgn()` times a
/// factor that vanishes there (b3) is not an event and costs no
/// points; under `compat=spice3` the jumps land on the grid.
#[test]
fn b_source_jumps_are_located() {
    let (ckt, opts, tran) = load("bsrc_step.cir", &corpus("regress/bsrc_step.cir"));
    let tol = 1e-3 * 20e-6;
    let crossings = |sol: &dune_core::Solution, name: &str, level: f64| {
        let dune_raw::Data::Real(cols) = &sol.data else {
            panic!("real data");
        };
        let k = sol.variables.iter().position(|(n, _)| n == name).unwrap();
        (1..cols[0].len())
            .filter(|&i| (cols[k][i] > level) != (cols[k][i - 1] > level))
            .map(|i| (cols[0][i - 1], cols[0][i]))
            .collect::<Vec<_>>()
    };
    let sol = run(&ckt, &opts, &tran);
    for (name, level, at) in [
        ("v(2)", 2.5, [0.305e-3, 0.695e-3]),
        ("v(4)", 0.0, [0.205e-3, 0.795e-3]),
    ] {
        let f = crossings(&sol, name, level);
        assert_eq!(f.len(), 2, "{name}: {f:?}");
        for ((before, after), at) in f.into_iter().zip(at) {
            assert!(
                before < at && at < after,
                "{name}: {before:e} {at:e} {after:e}"
            );
            assert!(
                after - before <= tol * (1.0 + 1e-9),
                "{name}: {:e}",
                after - before
            );
        }
    }
    let grid = run(&ckt, &Options::spice3(), &tran);
    for (name, level) in [("v(2)", 2.5), ("v(4)", 0.0)] {
        let f = crossings(&grid, name, level);
        assert!(f.iter().all(|(b, a)| a - b > 100.0 * tol), "{name}: {f:?}");
    }

    // Without b1 and b2 nothing jumps: the same grid with the event
    // location on and off.
    let quiet = corpus("regress/bsrc_step.cir")
        .lines()
        .filter(|l| !l.starts_with("b1") && !l.starts_with("b2"))
        .collect::<Vec<_>>()
        .join("\n");
    let (ckt, opts, tran) = load("quiet.cir", &quiet);
    let on = run(&ckt, &opts, &tran);
    let off = run(
        &ckt,
        &Options {
            compat: dune_core::Compat::Spice3,
            method: Some(Method::TrBdf2),
            trtol: opts.trtol / dune_core::SPICE_TRTOL,
            ..opts.clone()
        },
        &tran,
    );
    assert_eq!(on.data, off.data);
}

/// A B source that feeds its own argument: a Schmitt trigger made of
/// one `u()` with positive feedback (trips up at 3.5 V, down at
/// 2.5 V) around an RC of 1 ms — a relaxation oscillator of period
/// `τ·(ln(2.5/1.5) + ln(3.5/2.5))`. The jump pushes the argument
/// further, so the crossing is extrapolated from its rate before the
/// step. Located, every period is within 5e-4 of the closed form; on
/// the grid (`compat=spice3`) the mean period is off by more than
/// 1 %.
#[test]
fn a_b_source_with_feedback_is_located() {
    let deck = "b-source schmitt oscillator
b1 3 0 v=5*u(v(2)-3.5+0.2*v(3))
r3 3 0 1k
b2 4 0 v=5-v(3)
r1 4 2 10k
c1 2 0 100n ic=0
.tran 50u 20m uic
.end
";
    let (ckt, opts, tran) = load("bschmitt.cir", deck);
    let exact = 1e-3 * ((2.5f64 / 1.5).ln() + (3.5f64 / 2.5).ln());
    let periods = |o: &Options| {
        let sol = run(&ckt, o, &tran);
        let dune_raw::Data::Real(cols) = &sol.data else {
            panic!("real data");
        };
        let k = sol.variables.iter().position(|(n, _)| n == "v(3)").unwrap();
        let (t, v) = (&cols[0], &cols[k]);
        let ups: Vec<f64> = (1..t.len())
            .filter(|&i| v[i] > 2.5 && v[i - 1] <= 2.5)
            .map(|i| t[i])
            .collect();
        ups.windows(2).map(|w| w[1] - w[0]).collect::<Vec<f64>>()
    };
    let located = periods(&opts);
    assert!(located.len() >= 20, "{}", located.len());
    for p in &located {
        assert!((p - exact).abs() < 5e-4 * exact, "{p:e} vs {exact:e}");
    }
    let grid = periods(&Options::spice3());
    let mean = grid.iter().sum::<f64>() / grid.len() as f64;
    assert!((mean - exact).abs() > 1e-2 * exact, "{mean:e} vs {exact:e}");
}

/// A lossless LTRA line (Z0 = 50 Ω, T = 4.38 ns) between a 10 Ω
/// source and a 1 kΩ load: the 1 ns ramp of the source bounces for
/// 60 ns, and every waveform is a sum of delayed copies of it —
/// `v(3) = Σ (1+Γl)(ΓlΓs)^k·V⁺(t − (2k+1)T)`.
///
/// The 3f5's slope rule almost never plants the breakpoint where a
/// corner arrives (D-0056): the corner falls between timepoints, the
/// interpolation rounds it, and the rounding compounds at every
/// reflection. Measured: 1.0 % of the swing under `compat=spice3`,
/// 0.16 % under the Dune profile.
#[test]
fn ltra_reflections_keep_their_corners() {
    let (l, c, len, rs, rl) = (9.13e-9f64, 3.65e-12f64, 24.0f64, 10.0f64, 1000.0f64);
    let deck = format!(
        "reflections\nv1 1 0 pwl(0 0 1n 0 2n 1 100n 1)\nrs 1 2 {rs}\no1 2 0 3 0 ll\n\
         rl 3 0 {rl}\n.model ll ltra l={l} c={c} len={len}\n.tran 0.1n 60n\n.end\n"
    );
    let (z0, td) = ((l / c).sqrt(), len * (l * c).sqrt());
    let (gs, gl) = ((rs - z0) / (rs + z0), (rl - z0) / (rl + z0));
    let vplus = |t: f64| (t - 1e-9).clamp(0.0, 1e-9) / 1e-9 * z0 / (z0 + rs);
    let v3 = |t: f64| -> f64 {
        (0..12)
            .map(|k| (1.0 + gl) * (gl * gs).powi(k) * vplus(t - (2 * k + 1) as f64 * td))
            .sum()
    };
    let (ckt, opts, tran) = load("refl.cir", &deck);
    let worst = |o: &Options| {
        let sol = run(&ckt, o, &tran);
        let (t, v) = (sol.column("time").unwrap(), sol.column("v(3)").unwrap());
        t.iter()
            .zip(v)
            .map(|(t, v)| (v - v3(*t)).abs())
            .fold(0.0f64, f64::max)
    };
    let (dune, spice3) = (worst(&opts), worst(&Options::spice3()));
    assert!(dune < 3e-3, "{dune:e}");
    assert!(spice3 > 8e-3, "{spice3:e}");
}

/// `trytocompact` under the Dune profile (D-0056): a sample leaves
/// the history only while the chord from the last one kept passes by
/// every sample dropped since. The compacted run of
/// `regress/ltra_compact` refined 20 times stays within 2e-3 V of the
/// same run without the option (`COMPACTREL` = 1e-3 of a 1 V swing,
/// at the line and again at the load; measured 1.0e-3); the 3f5's
/// area test drifts (measured 1.7e-2 V, with a breakpoint cascade).
#[test]
fn compaction_does_not_drift_on_a_fine_step() {
    let text = corpus("regress/ltra_compact.cir")
        .replace(".tran 0.2ns 100ns", ".tran 0.2ns 100ns 0 1e-11");
    let (ckt, opts, tran) = load("ltra_compact.cir", &text);
    assert!(opts.trytocompact);
    let plain = Options {
        trytocompact: false,
        ..opts.clone()
    };
    let (on, off) = (run(&ckt, &opts, &tran), run(&ckt, &plain, &tran));
    for node in ["v(2)", "v(3)"] {
        let (ta, ya) = (on.column("time").unwrap(), on.column(node).unwrap());
        let (tb, yb) = (off.column("time").unwrap(), off.column(node).unwrap());
        let mut k = 0;
        let mut w = 0.0f64;
        for (t, y) in ta.iter().zip(ya) {
            while k + 2 < tb.len() && tb[k + 1] < *t {
                k += 1;
            }
            let f = ((t - tb[k]) / (tb[k + 1] - tb[k])).clamp(0.0, 1.0);
            w = w.max((y - (yb[k] + f * (yb[k + 1] - yb[k]))).abs());
        }
        assert!(w < 2e-3, "{node}: {w:e}");
    }
    // No cascade: about one point per maximum step.
    assert!(on.column("time").unwrap().len() < 12_000);
}
