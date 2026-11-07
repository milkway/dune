//! The parallel device load (issue #9, lever 1): `load_threads > 1`
//! must reproduce the sequential load bit for bit — every plot of
//! every analysis card of every corpus deck, and a synthetic BSIM3
//! inverter chain through `.op`, `.dc`, `.ac` and `.tran`.
//!
//! Three thread counts: 2 and 3 split the device list into multi-device
//! chunks; 64 gives every device of a corpus deck its own chunk, so
//! each worker's state window is exactly one device's — a load that
//! touched another device's states would panic there.
#![cfg(feature = "parallel")]

use std::path::{Path, PathBuf};

use dune_core::{Circuit, Data, Options, Solution};
use dune_netlist::ir::Netlist;

const THREADS: [usize; 3] = [2, 3, 64];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

fn options(nl: &Netlist, threads: usize) -> Options {
    let mut o = Options::default();
    o.apply(&nl.options, &mut Vec::new());
    o.load_threads = threads;
    o
}

/// Every analysis card in order, as `dune -b` runs them; a failing
/// card is kept as its error text (it must fail the same way).
fn run(nl: &Netlist, threads: usize) -> Vec<Result<Vec<Solution>, String>> {
    let (ckt, _) = Circuit::from_netlist(nl).expect("elaborates");
    let opts = options(nl, threads);
    nl.analyses
        .iter()
        .map(|a| ckt.run(a, &opts).map(|(s, _)| s).map_err(|e| e.to_string()))
        .collect()
}

/// Bitwise equality of two plots (NaN-safe, -0.0 ≠ 0.0).
fn assert_bits(a: &Solution, b: &Solution, what: &str) {
    assert_eq!(a.plotname, b.plotname, "{what}");
    assert_eq!(a.variables, b.variables, "{what}");
    assert_eq!(a.scale, b.scale, "{what}");
    match (&a.data, &b.data) {
        (Data::Real(x), Data::Real(y)) => {
            assert_eq!(x.len(), y.len(), "{what}: {} points", a.plotname);
            for (cx, cy) in x.iter().zip(y) {
                let bx: Vec<u64> = cx.iter().map(|v| v.to_bits()).collect();
                let by: Vec<u64> = cy.iter().map(|v| v.to_bits()).collect();
                assert_eq!(bx, by, "{what}: {}", a.plotname);
            }
        }
        (Data::Complex(x), Data::Complex(y)) => {
            assert_eq!(x.len(), y.len(), "{what}: {} points", a.plotname);
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

fn assert_same(nl: &Netlist, what: &str) -> usize {
    let seq = run(nl, 1);
    for t in THREADS {
        let par = run(nl, t);
        assert_eq!(seq.len(), par.len());
        for (k, (s, p)) in seq.iter().zip(&par).enumerate() {
            let tag = format!("{what} card {k} threads {t}");
            match (s, p) {
                (Ok(s), Ok(p)) => {
                    assert_eq!(s.len(), p.len(), "{tag}");
                    for (x, y) in s.iter().zip(p) {
                        assert_bits(x, y, &tag);
                    }
                }
                (Err(s), Err(p)) => assert_eq!(s, p, "{tag}"),
                _ => panic!("{tag}: {s:?} vs {p:?}"),
            }
        }
    }
    seq.len()
}

fn decks(dir: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(corpus().join(dir))
        .expect("corpus dir")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "cir"))
        .collect();
    v.sort();
    v
}

/// Decks too slow to run four times: `ltra_3`'s breakpoint cascade
/// (KNOWN_DIFFS) costs minutes per run; its devices (diodes, LTRA)
/// are covered by `ltra_1`/`ltra_2`.
const SLOW: [&str; 1] = ["ltra_3.cir"];

fn check_dir(dir: &str) {
    let mut cards = 0;
    for path in decks(dir) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if SLOW.contains(&name.as_str()) {
            continue;
        }
        let parsed = dune_netlist::parse_file(&path).expect("reads");
        if parsed
            .diags
            .iter()
            .any(|d| d.severity == dune_netlist::Severity::Error)
            || Circuit::from_netlist(&parsed.netlist).is_err()
        {
            continue;
        }
        cards += assert_same(&parsed.netlist, &format!("{dir}/{name}"));
    }
    assert!(cards > 0, "{dir}: no card ran");
}

#[test]
fn berkeley_examples_bit_identical() {
    check_dir("spice3f5/examples");
}

#[test]
fn regress_bit_identical() {
    check_dir("regress");
}

#[test]
fn basic_pss_measure_bit_identical() {
    check_dir("basic");
    check_dir("pss");
    check_dir("measure");
}

/// A chain of `n` BSIM3 inverters (the `bsim3_inv` cards), with a
/// mutual-inductor pair in the middle of the device list so the plan
/// has in-place devices between parallel runs.
fn chain(n: usize, analyses: &str) -> Netlist {
    let models = std::fs::read_to_string(corpus().join("regress/bsim3_inv.cir")).unwrap();
    let models = &models[models.find(".model n1").unwrap()..models.find(".end").unwrap()];
    let mut d = String::from("bsim3 inverter chain\nvdd vdd 0 dc 3.3\n");
    d.push_str("vin in0 0 dc 0 pulse(0 3.3 1n 0.5n 0.5n 4n 10n) ac 1\n");
    for k in 0..n {
        let (a, b) = (format!("in{k}"), format!("in{}", k + 1));
        d.push_str(&format!(
            "mn{k} {b} {a} 0 0 n1 l=0.35u w=2u ad=2p as=2p pd=6u ps=6u\n\
             mp{k} {b} {a} vdd vdd p1 l=0.35u w=5u ad=5p as=5p pd=12u ps=12u\n\
             c{k} {b} 0 30f\n"
        ));
        if k == n / 2 {
            d.push_str(&format!(
                "l1 {b} t1 1u\nrt1 t1 0 1k\nl2 t2 0 1u\nrt2 t2 0 1k\nk12 l1 l2 0.5\n"
            ));
        }
    }
    d.push_str(analyses);
    d.push_str(models);
    d.push_str(".end\n");
    let parsed = dune_netlist::parse_str("chain.cir", &d);
    assert!(
        parsed
            .diags
            .iter()
            .all(|d| d.severity != dune_netlist::Severity::Error),
        "{:?}",
        parsed.diags
    );
    parsed.netlist
}

#[test]
fn bsim3_chain_four_analyses_bit_identical() {
    let nl = chain(
        12,
        ".op\n.dc vin 0 3.3 0.1\n.ac dec 5 1meg 1g\n.tran 0.1n 12n\n",
    );
    assert_eq!(assert_same(&nl, "chain"), 4);
}
