//! `Circuit::run` (the shared card dispatch) and the `.noise` output
//! contract.

use dune_core::{AcSweep, CardOutput, Circuit, Error, Options, OutputVar};

const DECK: &str = "run dispatch
v1 1 0 dc 1 ac 1
r1 1 2 1k
r2 2 0 3k
c1 2 0 1n
.op
.ac dec 5 1k 1meg
.noise v(2) v1 dec 5 1k 1meg 1
.tf v(2) v1
.end
";

fn circuit() -> (Circuit, dune_netlist::ir::Netlist) {
    let parsed = dune_netlist::parse_str("run.cir", DECK);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    (ckt, parsed.netlist)
}

#[test]
fn run_dispatches_every_card_in_order() {
    let (ckt, nl) = circuit();
    let opts = Options::default();
    let names: Vec<Vec<String>> = nl
        .analyses
        .iter()
        .map(|a| {
            let (sols, _notes) = ckt.run(a, &opts).unwrap();
            sols.into_iter().map(|s| s.plotname).collect()
        })
        .collect();
    assert_eq!(names.len(), 4);
    assert_eq!(names[0], ["Operating Point"]);
    assert_eq!(names[1], ["AC Analysis"]);
    // Density plus integrated totals.
    assert_eq!(names[2].len(), 2, "{:?}", names[2]);
    assert_eq!(names[3].len(), 1);
}

#[test]
fn noise_rejects_a_current_output() {
    let (ckt, _) = circuit();
    let err = ckt
        .noise(
            &Options::default(),
            &OutputVar::Current { vsrc: "v1".into() },
            "v1",
            AcSweep::Decade,
            5,
            1e3,
            1e6,
            None,
        )
        .unwrap_err();
    assert!(matches!(err, Error::Build(_)), "{err:?}");
    assert!(err.to_string().contains("voltage"), "{err}");
}

#[test]
fn raw_data_types_are_reexported() {
    let (ckt, _) = circuit();
    let op = ckt.op(&Options::default()).unwrap();
    let _: &dune_core::Data = &op.solution.data;
    let _: &[(String, dune_core::VarType)] = &op.solution.variables;
    assert!(matches!(op.solution.data, dune_core::Data::Real(_)));
}

/// Dune's transient step budget (`Options::max_tran_steps`, D-0022):
/// the 3f5 and ngspice ignore ITL5, so a breakpoint cascade would run
/// unbounded; the guard stops the run with a structured error. A
/// cascade is hard to build on purpose, so the budget is shrunk
/// instead: a pulse train needs far more than 50 timepoints.
#[test]
fn tran_step_budget_stops_the_run() {
    let deck = "step budget
v1 1 0 pulse(0 1 1n 1n 1n 5n 10n)
r1 1 2 1k
c1 2 0 1p
.tran 0.1n 100n
.end
";
    let parsed = dune_netlist::parse_str("budget.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let tran = &parsed.netlist.analyses[0];

    // The automatic budget is far above what the deck needs: 10 000
    // (floor) vs 50 per TMAX span (min(0.1n, 100n/50) → 1000 spans).
    let opts = Options::default();
    assert_eq!(opts.max_tran_steps, None);
    assert_eq!(opts.tran_step_budget(100e-9, 0.1e-9), 50_000);
    assert_eq!(opts.tran_step_budget(1e-9, 0.1e-9), 10_000);
    ckt.run(tran, &opts).unwrap();

    let opts = Options {
        max_tran_steps: Some(50),
        ..Options::default()
    };
    let err = ckt.run(tran, &opts).unwrap_err();
    match err {
        Error::StepLimit { steps, time } => {
            assert_eq!(steps, 50);
            assert!(time > 0.0 && time < 100e-9, "{time}");
        }
        other => panic!("expected StepLimit, got {other:?}"),
    }
    assert!(err_text(&ckt, tran, &opts).contains("internal steps"));

    // An explicit cap replaces the automatic one.
    let opts = Options {
        max_tran_steps: Some(usize::MAX),
        ..Options::default()
    };
    ckt.run(tran, &opts).unwrap();
}

fn err_text(ckt: &Circuit, a: &dune_netlist::ir::Analysis, opts: &Options) -> String {
    ckt.run(a, opts).unwrap_err().to_string()
}

/// `Circuit::run_card` is the one card→analysis path: `Circuit::run`
/// and `Session::run_analysis` are its plots, and a `.pss` card comes
/// back whole with the card's parameters (counts truncated as ngspice
/// reads IF_INTEGER, defaults for what the card omits). Pins what the
/// wasm build's duplicate `PssParams` mapping used to be checked
/// against (D-0036e).
#[test]
fn run_card_is_the_one_card_mapping() {
    let deck = "pss card
v1 1 0 sin(0 1 1k) ac 1
r1 1 2 1k
c1 2 0 1u
.options keepopinfo
.ac dec 2 100 10k
.pss 1k 5m 2 64.9 7.7
.pss 1k 5m 2
.end
";
    let parsed = dune_netlist::parse_str("card.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    opts.apply(&parsed.netlist.options, &mut Vec::new());
    let mut session = dune_core::Session::from_netlist(&parsed.netlist).unwrap().0;
    let mut pss = Vec::new();
    for a in &parsed.netlist.analyses {
        let card = ckt.run_card(a, &opts).unwrap();
        if let CardOutput::Pss(p) = &card {
            pss.push(p.clone());
        }
        let raw = |(sols, notes): (Vec<dune_core::Solution>, Vec<String>)| {
            let plots: Vec<_> = sols.iter().map(|s| s.to_plot("t", "")).collect();
            (plots, notes)
        };
        let session_card = raw(session.run_card(a).unwrap().into_plots());
        let plots = raw(card.into_plots());
        assert_eq!(plots, raw(ckt.run(a, &opts).unwrap()));
        assert_eq!(plots, session_card);
        assert_eq!(plots, raw(session.run_analysis(a).unwrap()));
    }
    // `.ac` stays a plain card, keepopinfo's plot included.
    let (ac, _) = ckt.run(&parsed.netlist.analyses[0], &opts).unwrap();
    assert_eq!(ac[0].plotname, "AC Operating Point");
    // `64.9 7.7` → 64 points (65 rows), 7 harmonics; then the
    // defaults (`PssParams::default`: 128, 10).
    assert_eq!(pss.len(), 2);
    let rows = |s: &dune_core::Solution| match &s.data {
        dune_raw::Data::Real(c) => c[0].len(),
        dune_raw::Data::Complex(c) => c[0].len(),
    };
    assert_eq!((rows(&pss[0].time), rows(&pss[0].spectrum)), (65, 7));
    assert_eq!((rows(&pss[1].time), rows(&pss[1].spectrum)), (129, 10));
    assert!((pss[0].period - 1e-3).abs() < 1e-15);
    assert_eq!(pss[0].residuals.len(), pss[0].periods.len());
    // A negative count is 0, which the analysis refuses (an error,
    // never a panic).
    let bad = dune_netlist::parse_str(
        "bad.cir",
        "bad\nv1 1 0 sin(0 1 1k)\nr1 1 0 1k\n.pss 1k 5m 1 -3 10\n.end\n",
    );
    let (ckt, _) = Circuit::from_netlist(&bad.netlist).unwrap();
    let e = ckt
        .run_card(&bad.netlist.analyses[0], &Options::default())
        .unwrap_err();
    assert!(e.to_string().contains("points"), "{e}");
}
