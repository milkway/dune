//! `Circuit::run` (the shared card dispatch) and the `.noise` output
//! contract.

use dune_core::{AcSweep, Circuit, Error, Options, OutputVar};

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
