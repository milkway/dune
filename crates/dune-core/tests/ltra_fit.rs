//! The opt-in LTRA recursive convolution of issue #8
//! (`.options ltrafit`). The faithful default is the oracle's business
//! (`dune-oracle --ci`, which also runs `corpus/regress/ltrafit.cir`
//! against ngspice-47); these gate the fit error the run reports and
//! the agreement with the faithful coefficient walk.

use dune_core::{Circuit, Method, Options};

fn corpus(rel: &str) -> String {
    let path = format!("{}/../../corpus/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

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

/// The fit errors a note reports: every `a/b/c` triple after a kernel
/// label (setup-grid max, setup-grid L1, max at the history lags).
/// Worst `|a − b|` of one vector, `b` linearly interpolated onto `a`'s
/// timepoints (the two runs' grids differ in the last bits).
fn worst_diff(a: &dune_core::Solution, b: &dune_core::Solution, node: &str) -> f64 {
    let (ta, tb) = (a.column("time").unwrap(), b.column("time").unwrap());
    let (ya, yb) = (a.column(node).unwrap(), b.column(node).unwrap());
    let mut k = 0;
    let mut w = 0.0f64;
    for (t, y) in ta.iter().zip(ya) {
        while k + 2 < tb.len() && tb[k + 1] < *t {
            k += 1;
        }
        let f = ((t - tb[k]) / (tb[k + 1] - tb[k])).clamp(0.0, 1.0);
        let v = yb[k] + f * (yb[k + 1] - yb[k]);
        w = w.max((y - v).abs());
    }
    w
}

fn reported_errors(note: &str) -> Vec<f64> {
    let tail = note.split("history lags:").nth(1).expect("error list");
    tail.split(',')
        .flat_map(|k| k.split_whitespace().nth(1).unwrap().split('/'))
        .map(|v| v.parse::<f64>().unwrap())
        .collect()
}

#[test]
fn option_parses_as_flag_or_value() {
    let deck = |o: &str| format!("o\nr1 1 0 1\nv1 1 0 1\n.options {o}\n.tran 1n 10n\n.end\n");
    assert!(load("a", &deck("ltrafit")).1.ltra_fit);
    assert!(load("b", &deck("ltrafit=1")).1.ltra_fit);
    assert!(!load("c", &deck("ltrafit=0")).1.ltra_fit);
    assert!(!Options::default().ltra_fit);
}

/// `corpus/regress/ltrafit.cir`: an RLC and an RC line, 3545 accepted
/// points. Every kernel's reported error stays inside the 1e-6 budget,
/// and the waveforms agree with the faithful walk far inside the
/// SPICE tolerances (the remaining difference is the C's linear
/// interpolation of `h2`/`h3'` between history lags, which the
/// recursion integrates exactly).
#[test]
fn recursive_mode_meets_its_budget_and_tracks_the_faithful_walk() {
    let (ckt, opts, tran) = load("ltrafit.cir", &corpus("regress/ltrafit.cir"));
    assert!(opts.ltra_fit);
    let (on, notes) = ckt.run(&tran, &opts).unwrap();
    let fits: Vec<&String> = notes.iter().filter(|n| n.contains("ltrafit")).collect();
    assert_eq!(fits.len(), 2, "{notes:?}");
    for n in &fits {
        let errs = reported_errors(n);
        assert_eq!(errs.len(), 9, "{n}");
        assert!(errs.iter().all(|e| *e <= 1e-6), "{n}");
    }
    let off_opts = Options {
        ltra_fit: false,
        ..opts.clone()
    };
    let (off, off_notes) = ckt.run(&tran, &off_opts).unwrap();
    assert!(off_notes.iter().all(|n| !n.contains("ltrafit")));
    let worst = |node: &str| worst_diff(&on[0], &off[0], node);
    // RLC line ends: 4 V swing, reltol·4 V = 4e-3; measured 9e-5.
    assert!(worst("v(2)") < 2e-4, "{}", worst("v(2)"));
    assert!(worst("v(3)") < 2e-4, "{}", worst("v(3)"));
    // RC line: the windowed hybrid, measured 2e-7.
    assert!(worst("v(4)") < 1e-6, "{}", worst("v(4)"));
    assert!(worst("v(5)") < 1e-6, "{}", worst("v(5)"));
}

/// Under TR-BDF2 the recursion advances only at accepted points (the
/// γ-stage never enters the history): the recursive and the faithful
/// runs agree as closely as under trap.
#[test]
fn recursive_mode_under_trbdf2() {
    let (ckt, opts, tran) = load("ltrafit.cir", &corpus("regress/ltrafit.cir"));
    let tb = Options {
        method: Some(Method::TrBdf2),
        ..opts.clone()
    };
    let (on, _) = ckt.run(&tran, &tb).unwrap();
    let (off, _) = ckt
        .run(
            &tran,
            &Options {
                ltra_fit: false,
                ..tb.clone()
            },
        )
        .unwrap();
    let (trap, _) = ckt
        .run(
            &tran,
            &Options {
                method: Some(Method::Trapezoidal),
                ..opts.clone()
            },
        )
        .unwrap();
    assert_ne!(on[0].data, trap[0].data, "trbdf2 must change this deck");
    for node in ["v(2)", "v(3)", "v(4)", "v(5)"] {
        let w = worst_diff(&on[0], &off[0], node);
        assert!(w < 2e-4, "{node}: {w}");
    }
}

/// Lines without a history convolution (LC, RG) keep the exact path
/// and say so; the run is unchanged.
#[test]
fn regimes_without_history_are_untouched() {
    let (ckt, mut opts, tran) = load("ltra_rg.cir", &corpus("regress/ltra_rg.cir"));
    let (off, _) = ckt.run(&tran, &opts).unwrap();
    opts.ltra_fit = true;
    let (on, notes) = ckt.run(&tran, &opts).unwrap();
    assert_eq!(off[0].data, on[0].data);
    assert!(
        notes.iter().any(|n| n.contains("ltrafit not applied")),
        "{notes:?}"
    );
}
