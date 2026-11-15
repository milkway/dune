//! `dune_core::sweep` (issue #10): a sweep of a corpus deck must
//! reproduce N individual runs of the edited deck bit for bit, and
//! `sweep_parallel` must reproduce `sweep` bit for bit.

use std::path::{Path, PathBuf};

use dune_core::{Circuit, Data, Options, Solution, SweepParam, SweepRun, sweep};
use dune_netlist::ir::{Device, Netlist, ParamValue};

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/spice3f5/examples")
}

fn parse(text: &str) -> Netlist {
    let out = dune_netlist::parse_str("deck.cir", text);
    assert!(
        out.diags
            .iter()
            .all(|d| d.severity != dune_netlist::Severity::Error),
        "deck does not parse"
    );
    out.netlist
}

fn options(nl: &Netlist) -> Options {
    let mut o = Options::default();
    o.apply(&nl.options, &mut Vec::new());
    o
}

/// What `dune -b` runs: elaborate, every analysis card in order.
fn single(nl: &Netlist) -> Vec<Solution> {
    let (ckt, _) = Circuit::from_netlist(nl).expect("elaborates");
    let opts = options(nl);
    nl.analyses
        .iter()
        .flat_map(|a| ckt.run(a, &opts).expect("runs").0)
        .collect()
}

/// Bitwise equality of two plots (NaN-safe, -0.0 ≠ 0.0).
fn assert_bits(a: &Solution, b: &Solution, what: &str) {
    assert_eq!(a.plotname, b.plotname, "{what}");
    assert_eq!(a.variables, b.variables, "{what}");
    assert_eq!(a.scale, b.scale, "{what}");
    match (&a.data, &b.data) {
        (Data::Real(x), Data::Real(y)) => {
            assert_eq!(x.len(), y.len(), "{what}");
            for (cx, cy) in x.iter().zip(y) {
                let bx: Vec<u64> = cx.iter().map(|v| v.to_bits()).collect();
                let by: Vec<u64> = cy.iter().map(|v| v.to_bits()).collect();
                assert_eq!(bx, by, "{what}: {}", a.plotname);
            }
        }
        (Data::Complex(x), Data::Complex(y)) => {
            assert_eq!(x.len(), y.len(), "{what}");
            for (cx, cy) in x.iter().zip(y) {
                let bx: Vec<(u64, u64)> = cx
                    .iter()
                    .map(|z| (z.re.to_bits(), z.im.to_bits()))
                    .collect();
                let by: Vec<(u64, u64)> = cy
                    .iter()
                    .map(|z| (z.re.to_bits(), z.im.to_bits()))
                    .collect();
                assert_eq!(bx, by, "{what}: {}", a.plotname);
            }
        }
        _ => panic!("{what}: real vs complex"),
    }
}

fn assert_runs_bits(a: &[SweepRun], b: &[SweepRun]) {
    assert_eq!(a.len(), b.len());
    for (r, (x, y)) in a.iter().zip(b).enumerate() {
        assert_eq!(x.values, y.values);
        let (xs, ys) = (x.outcome.as_ref().unwrap(), y.outcome.as_ref().unwrap());
        assert_eq!(xs.solutions.len(), ys.solutions.len());
        for (p, q) in xs.solutions.iter().zip(&ys.solutions) {
            assert_bits(p, q, &format!("run {r}"));
        }
    }
}

fn element<'a>(nl: &'a Netlist, name: &str) -> &'a Device {
    &nl.elements.iter().find(|e| e.name == name).unwrap().device
}

/// diffpair.cir with its commented .tf/.dc/.ac switched on (four
/// analyses: tf, dc, ac, tran).
fn diffpair() -> String {
    std::fs::read_to_string(examples().join("diffpair.cir"))
        .unwrap()
        .lines()
        .map(|l| match l {
            "*.tf v(5) vin" | "*.dc vin -0.25 0.25 0.005" | "*.ac dec 10 1 10ghz" => &l[1..],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The deck as a user would edit it by hand for one run.
fn diffpair_edited(rc1: &str, vcc: &str, acmag: &str, temp: &str) -> String {
    diffpair()
        .replace("rc1 4 8 10k", &format!("rc1 4 8 {rc1}"))
        .replace("vcc 8 0 12", &format!("vcc 8 0 {vcc}"))
        .replace(
            "sin(0 0.1 5meg) ac 1",
            &format!("sin(0 0.1 5meg) ac {acmag}"),
        )
        .replace(".tran 5ns", &format!(".options temp={temp}\n.tran 5ns"))
}

#[test]
fn sweep_reproduces_individual_runs_bit_for_bit() {
    let base = parse(&diffpair());
    let params: Vec<SweepParam> = ["rc1", "vcc", "@vin[acmag]", "temp"]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    let cases = [
        ("8k", "12", "1", "27"),
        ("12.5k", "10", "0.5", "-40"),
        ("10k", "15", "2", "85"),
        ("9.1k", "11.7", "1.3", "0.5"),
    ];
    let mut rows = Vec::new();
    let mut expected = Vec::new();
    for (rc1, vcc, acmag, temp) in cases {
        let nl = parse(&diffpair_edited(rc1, vcc, acmag, temp));
        // The sweep values are the f64s the edited deck parses to,
        // so the comparison does not depend on how INPevaluate rounds.
        let Device::Resistor { value: Some(r), .. } = element(&nl, "rc1") else {
            panic!()
        };
        let Device::Source { dc: Some(v), .. } = element(&nl, "vcc") else {
            panic!()
        };
        let Device::Source { ac: Some(ac), .. } = element(&nl, "vin") else {
            panic!()
        };
        let t = options(&nl).temp - dune_core::consts::CTOK;
        rows.push(vec![*r, *v, ac.mag, t]);
        expected.push(single(&nl));
    }
    // temp round-trips through °C exactly for these values.
    for (row, (.., temp)) in rows.iter().zip(cases) {
        assert_eq!(row[3] + dune_core::consts::CTOK, {
            let nl = parse(&format!("t\n.options temp={temp}\n.end\n"));
            options(&nl).temp
        });
    }

    let runs = sweep(&base, &options(&base), &params, &rows).unwrap();
    assert_eq!(runs.len(), cases.len());
    for (r, (run, want)) in runs.iter().zip(&expected).enumerate() {
        assert_eq!(run.values, rows[r]);
        let got = &run.outcome.as_ref().unwrap().solutions;
        assert_eq!(got.len(), 4, "tf, dc, ac, tran");
        for (g, w) in got.iter().zip(want) {
            assert_bits(g, w, &format!("run {r}"));
        }
    }
    // Deterministic: a second sweep gives the same bits.
    assert_runs_bits(
        &runs,
        &sweep(&base, &options(&base), &params, &rows).unwrap(),
    );
    // The values really changed something.
    let v5 = |r: usize| {
        runs[r].outcome.as_ref().unwrap().solutions[3]
            .column("v(5)")
            .unwrap()[0]
    };
    assert_ne!(v5(0).to_bits(), v5(1).to_bits());
}

#[test]
fn mosfet_geometry_sweep_matches_edited_decks() {
    let text = std::fs::read_to_string(examples().join("mosmem.cir")).unwrap();
    let base = parse(&text);
    let params: Vec<SweepParam> = vec!["@m1[w]".parse().unwrap(), "@M3[L]".parse().unwrap()];
    let mut rows = Vec::new();
    let mut expected = Vec::new();
    for (w, l) in [("250u", "5u"), ("200u", "4u"), ("300u", "6.5u")] {
        let edited = text
            .replace(
                "m1 3 1 0 0 mod w=250u l=5u",
                &format!("m1 3 1 0 0 mod w={w} l=5u"),
            )
            .replace(
                "m3 9 9 3 0 mod w=5u   l=5u",
                &format!("m3 9 9 3 0 mod w=5u l={l}"),
            );
        let nl = parse(&edited);
        let num = |dev: &Device, key: &str| match dev {
            Device::Mosfet { params, .. } => params
                .iter()
                .rev()
                .find_map(|p| match (&p.name, &p.value) {
                    (n, ParamValue::Num(v)) if n == key => Some(*v),
                    _ => None,
                })
                .unwrap(),
            _ => panic!(),
        };
        rows.push(vec![
            num(element(&nl, "m1"), "w"),
            num(element(&nl, "m3"), "l"),
        ]);
        expected.push(single(&nl));
    }
    let runs = sweep(&base, &options(&base), &params, &rows).unwrap();
    for (r, (run, want)) in runs.iter().zip(&expected).enumerate() {
        for (g, w) in run.outcome.as_ref().unwrap().solutions.iter().zip(want) {
            assert_bits(g, w, &format!("mosmem run {r}"));
        }
    }
}

#[test]
fn sweep_refuses_bad_parameters_before_running() {
    let base = parse(&diffpair());
    let opts = options(&base);
    let err = |names: &[&str], rows: Vec<Vec<f64>>| {
        let params: Vec<SweepParam> = names.iter().map(|s| s.parse().unwrap()).collect();
        sweep(&base, &opts, &params, &rows).unwrap_err().to_string()
    };
    assert!(err(&["r9"], vec![vec![1.0]]).contains("no element `r9`"));
    assert!(err(&["q1"], vec![vec![1.0]]).contains("no primary value"));
    assert!(err(&["@rc1[w]"], vec![vec![1.0]]).contains("no sweepable parameter `w`"));
    assert!(err(&["rc1", "@rc1[r]"], vec![vec![1.0, 2.0]]).contains("given twice"));
    assert!(err(&["rc1"], vec![vec![1.0, 2.0]]).contains("2 values for 1"));
    assert!(err(&["rc1"], vec![vec![f64::NAN]]).contains("not finite"));
}

#[test]
fn a_failing_run_does_not_stop_the_others() {
    // A zero resistor is refused at elaboration; the other runs go on.
    let base = parse("div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n");
    let p = vec!["r2".parse().unwrap()];
    let runs = sweep(
        &base,
        &options(&base),
        &p,
        &[vec![1e3], vec![0.0], vec![3e3]],
    )
    .unwrap();
    assert!(runs[0].outcome.is_ok() && runs[2].outcome.is_ok());
    assert!(runs[1].outcome.is_err());
    let v2 = runs[2].outcome.as_ref().unwrap().solutions[0]
        .column("v(2)")
        .unwrap()[0];
    assert!((v2 - 1.5).abs() < 1e-12);
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_sweep_is_bit_identical_to_sequential() {
    let base = parse(&diffpair());
    let params: Vec<SweepParam> = vec!["rc1".parse().unwrap(), "temp".parse().unwrap()];
    let rows: Vec<Vec<f64>> = (0..12)
        .map(|i| vec![8e3 + 500.0 * i as f64, -20.0 + 10.0 * i as f64])
        .collect();
    let opts = options(&base);
    let seq = sweep(&base, &opts, &params, &rows).unwrap();
    for threads in [0, 1, 3, 8] {
        let par = dune_core::sweep_parallel(&base, &opts, &params, &rows, threads).unwrap();
        assert_runs_bits(&seq, &par);
    }
}

/// A seeded deck (D-0075) whose draws reach a model, an instance, a
/// subcircuit and a value that reads `temper`.
fn seeded_deck() -> String {
    "seeded mismatch\n.options seed=21\n.param p=agauss(1k,100,3) q=aunif(1,0.05)\n\
     .model dm d is={1e-14*q}\n.subckt seg a b\nr1 a b {p}\n.ends\n\
     v1 1 0 2\nx1 1 2 seg\nx2 2 3 seg\nd1 3 0 dm area={q}\nr9 3 0 {p+temper}\n\
     .op\n.dc v1 0 2 0.5\n.end\n"
        .to_string()
}

#[test]
fn a_seeded_deck_is_reproducible() {
    // Same seed, same bits; another seed (with_seed), other numbers.
    let a = single(&parse(&seeded_deck()));
    let b = single(&parse(&seeded_deck()));
    for (x, y) in a.iter().zip(&b) {
        assert_bits(x, y, "same seed");
    }
    let (other, _) = parse(&seeded_deck()).with_seed(22);
    let c = single(&other);
    assert_ne!(
        format!("{:?}", a[0].data),
        format!("{:?}", c[0].data),
        "another seed draws other numbers"
    );
    // Monte Carlo by seed: run i = with_seed(base + i); with_seed(21)
    // is the deck itself.
    let (same, _) = parse(&seeded_deck()).with_seed(21);
    for (x, y) in a.iter().zip(&single(&same)) {
        assert_bits(x, y, "with_seed(21)");
    }
}

#[cfg(feature = "parallel")]
#[test]
fn a_seeded_sweep_is_thread_count_independent() {
    let base = parse(&seeded_deck());
    let params: Vec<SweepParam> = vec!["v1".parse().unwrap(), "temp".parse().unwrap()];
    let rows: Vec<Vec<f64>> = (0..9)
        .map(|i| vec![1.0 + 0.1 * i as f64, -40.0 + 20.0 * i as f64])
        .collect();
    let opts = options(&base);
    let seq = sweep(&base, &opts, &params, &rows).unwrap();
    for threads in [1, 2, 3] {
        let par = dune_core::sweep_parallel(&base, &opts, &params, &rows, threads).unwrap();
        assert_runs_bits(&seq, &par);
    }
}

/// `seeded_deck` with its seed, `v1` and temperature written in.
fn seeded_edited(seed: u64, v1: &str, temp: &str) -> String {
    seeded_deck()
        .replace(
            ".options seed=21",
            &format!(".options seed={seed} temp={temp}"),
        )
        .replace("v1 1 0 2\n", &format!("v1 1 0 {v1}\n"))
}

#[test]
fn a_seed_sweep_reproduces_the_seeded_decks_bit_for_bit() {
    // `seed` composes with a value and the temperature: every run is
    // the deck with `.options seed=S temp=T` and the value written in.
    let base = parse(&seeded_deck());
    assert!(base.uses_random() && base.uses_temper());
    let params: Vec<SweepParam> = ["seed", "v1", "temp"]
        .iter()
        .map(|s| s.parse().unwrap())
        .collect();
    let rows = vec![
        vec![21.0, 2.0, 27.0],
        vec![22.0, 2.0, 27.0],
        vec![7.0, 1.5, -10.0],
        vec![4_294_967_295.0, 1.0, 80.0],
    ];
    let runs = sweep(&base, &options(&base), &params, &rows).unwrap();
    let texts = [
        seeded_edited(21, "2", "27"),
        seeded_edited(22, "2", "27"),
        seeded_edited(7, "1.5", "-10"),
        seeded_edited(4_294_967_295, "1", "80"),
    ];
    for (r, (run, text)) in runs.iter().zip(&texts).enumerate() {
        let out = run.outcome.as_ref().expect("run succeeds");
        assert!(out.notes.iter().all(|n| !n.contains("`seed` is swept")));
        let want = single(&parse(text));
        assert_eq!(out.solutions.len(), want.len());
        for (a, b) in out.solutions.iter().zip(&want) {
            assert_bits(a, b, &format!("seed run {r}"));
        }
    }
    // Seed 21 is the deck's own; 22 draws other numbers.
    let v = |r: usize| {
        runs[r].outcome.as_ref().unwrap().solutions[0]
            .column("v(3)")
            .unwrap()[0]
    };
    assert_ne!(v(0).to_bits(), v(1).to_bits());
}

#[test]
fn a_seed_sweep_matches_with_seed_and_wins_over_the_callers_seed() {
    // The caller's options say seed 5; the swept seed wins, and each
    // run is `with_seed` of the deck.
    let base = parse(&seeded_deck());
    let mut opts = options(&base);
    opts.seed = 5;
    let seeds: Vec<Vec<f64>> = (100..104).map(|s| vec![s as f64]).collect();
    let runs = sweep(&base, &opts, &[SweepParam::Seed], &seeds).unwrap();
    for (run, s) in runs.iter().zip(100u64..) {
        let (nl, diags) = base.with_seed(s);
        assert!(
            diags
                .iter()
                .all(|d| d.severity != dune_netlist::Severity::Error)
        );
        let want = single(&nl);
        for (a, b) in run.outcome.as_ref().unwrap().solutions.iter().zip(&want) {
            assert_bits(a, b, &format!("seed {s}"));
        }
    }
}

#[test]
fn a_seed_sweep_refuses_what_is_not_a_seed() {
    let base = parse(&seeded_deck());
    let opts = options(&base);
    let err = |names: &[&str], v: f64| {
        let params: Vec<SweepParam> = names.iter().map(|s| s.parse().unwrap()).collect();
        let row = vec![v; params.len()];
        sweep(&base, &opts, &params, &[vec![3.0; params.len()], row])
            .unwrap_err()
            .to_string()
    };
    for bad in [0.0, -3.0, 1.5, 4_294_967_296.0, 1e300] {
        let e = err(&["seed"], bad);
        assert!(
            e.contains("sweep run 1") && e.contains("not a seed"),
            "{bad}: {e}"
        );
    }
    assert!(err(&["seed"], f64::INFINITY).contains("not finite"));
    assert!(err(&["seed", "SEED"], 3.0).contains("`seed` given twice"));
}

#[test]
fn a_seed_sweep_without_random_functions_says_what_the_seed_changes() {
    // No random function, no transient noise: the runs are the same
    // circuit, and each one says so.
    let quiet = parse("div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n");
    assert!(!quiet.uses_random());
    let runs = sweep(
        &quiet,
        &options(&quiet),
        &[SweepParam::Seed],
        &[vec![1.0], vec![2.0]],
    )
    .unwrap();
    let a = runs[0].outcome.as_ref().unwrap();
    let b = runs[1].outcome.as_ref().unwrap();
    assert!(
        a.notes
            .iter()
            .any(|n| n.contains("every run is the same circuit"))
    );
    assert_bits(&a.solutions[0], &b.solutions[0], "seed-independent");
    // A TRNOISE source: the seed changes the noise only, and each run
    // is the deck with `.options seed=S`.
    let noisy_text = |opt: &str| {
        format!("noisy\n{opt}v1 1 0 dc 0 trnoise(1m 1n 0 0)\nr1 1 0 1k\n.tran 1n 20n\n.end\n")
    };
    let noisy = parse(&noisy_text(""));
    let runs = sweep(
        &noisy,
        &options(&noisy),
        &[SweepParam::Seed],
        &[vec![1.0], vec![2.0]],
    )
    .unwrap();
    let a = runs[0].outcome.as_ref().unwrap();
    let b = runs[1].outcome.as_ref().unwrap();
    assert!(
        a.notes
            .iter()
            .any(|n| n.contains("only changes its transient noise"))
    );
    assert_ne!(
        format!("{:?}", a.solutions[0].data),
        format!("{:?}", b.solutions[0].data),
        "another seed, other noise"
    );
    let want = single(&parse(&noisy_text(".options seed=2\n")));
    assert_bits(&b.solutions[0], &want[0], "seed 2 = .options seed=2");
}

#[cfg(feature = "parallel")]
#[test]
fn a_seed_sweep_is_thread_count_independent() {
    let base = parse(&seeded_deck());
    let params: Vec<SweepParam> = vec!["seed".parse().unwrap(), "temp".parse().unwrap()];
    let rows: Vec<Vec<f64>> = (0..10)
        .map(|i| vec![1000.0 + i as f64, -40.0 + 15.0 * i as f64])
        .collect();
    let opts = options(&base);
    let seq = sweep(&base, &opts, &params, &rows).unwrap();
    for threads in [0, 1, 3, 8] {
        let par = dune_core::sweep_parallel(&base, &opts, &params, &rows, threads).unwrap();
        assert_runs_bits(&seq, &par);
    }
}
