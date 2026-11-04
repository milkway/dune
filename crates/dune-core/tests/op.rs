//! Operating-point tests. The reference numbers come from
//! ngspice-47 (`ngspice -b -D filetype=ascii -r`), the oracle
//! (D-0006); agreement is checked with the SPICE tolerances.

use dune_core::{Circuit, Options, Resistor, Vsource};

const RELTOL: f64 = 1e-3;
const VNTOL: f64 = 1e-6;
const ABSTOL: f64 = 1e-12;

fn assert_close(sol: &dune_core::Solution, name: &str, expect: f64, floor: f64) {
    let got = sol.column(name).unwrap_or_else(|| panic!("{name} missing"))[0];
    let tol = floor + RELTOL * f64::max(got.abs(), expect.abs());
    assert!(
        (got - expect).abs() <= tol,
        "{name}: got {got:.15e}, ngspice says {expect:.15e}"
    );
}

#[test]
fn voltage_divider() {
    let mut ckt = Circuit::new("divider");
    ckt.add(Vsource {
        name: "v1".into(),
        nodes: ["1".into(), "0".into()],
        dc: Some(5.0),
        wave: None,
    })
    .unwrap();
    ckt.add(Resistor {
        name: "r1".into(),
        nodes: ["1".into(), "2".into()],
        value: 1e3,
    })
    .unwrap();
    ckt.add(Resistor {
        name: "r2".into(),
        nodes: ["2".into(), "0".into()],
        value: 2e3,
    })
    .unwrap();
    let op = ckt.op(&Options::default()).unwrap();
    assert_close(&op.solution, "v(2)", 10.0 / 3.0, VNTOL);
    assert_close(&op.solution, "i(v1)", -5.0 / 3e3, ABSTOL);
}

/// The probe deck exercises every Phase-1 device: divider, inductor
/// short, diode with RS (internal node), VCVS, CCCS. Reference run:
/// ngspice-47, 2026-11-03.
const PROBE: &str = "\
op probe: divider, dependent sources, diode, inductor
v1 1 0 dc 5
r1 1 2 1k
r2 2 0 2k
l1 2 3 1m
rl 3 0 1meg
d1 2 4 dmod
rd 4 0 100
.model dmod d(is=1e-14 rs=2)
e1 5 0 2 0 2.0
re 5 0 10k
f1 0 6 v1 3
rf 6 0 50
.op
.end
";

#[test]
fn probe_deck_matches_ngspice() {
    let parsed = dune_netlist::parse_str("probe.cir", PROBE);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, notes) = Circuit::from_netlist(&parsed.netlist).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    let op = ckt.op(&Options::default()).unwrap();

    for (name, expect) in [
        ("v(1)", 5.0),
        ("v(2)", 1.038158296765296),
        ("v(3)", 1.038158296765296),
        ("v(4)", 3.441724396555416e-1),
        ("v(5)", 2.076316593530591),
        ("v(6)", -5.942762554852057e-1),
    ] {
        assert_close(&op.solution, name, expect, VNTOL);
    }
    for (name, expect) in [
        ("i(v1)", -3.961841703234705e-3),
        ("i(l1)", 1.038158296763745e-6),
        ("i(e1)", -2.076316593530591e-4),
    ] {
        assert_close(&op.solution, name, expect, ABSTOL);
    }
    // ngspice's op plot has exactly these nine vectors: the diode's
    // internal node stays internal.
    assert_eq!(op.solution.variables.len(), 9);
    assert_eq!(op.solution.plotname, "Operating Point");
}

#[test]
fn ccvs_and_vccs_and_isource() {
    // i1 pushes 1 mA into node 1 (through 1k -> v(1) = -1V?); check
    // the sign convention against a hand calculation instead:
    // i1 0->1 of 1mA, r1 1-0 1k: v(1) = 1 V.
    let deck = "\
h and g sources
i1 0 1 dc 1m
r1 1 0 1k
vs 2 0 dc 1
rs 2 0 1k
h1 3 0 vs 2k
rh 3 0 1k
g1 0 4 1 0 2m
rg 4 0 1k
.op
.end
";
    let parsed = dune_netlist::parse_str("hg.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let op = ckt.op(&Options::default()).unwrap();
    // v(1) = 1mA * 1k = 1 V.
    assert_close(&op.solution, "v(1)", 1.0, VNTOL);
    // i(vs) = -1/1k; h1 = 2k * i(vs) = -2 V.
    assert_close(&op.solution, "i(vs)", -1e-3, ABSTOL);
    assert_close(&op.solution, "v(3)", -2.0, VNTOL);
    // g1 drives 2m * v(1) into node 4 over 1k: v(4) = 2 V.
    assert_close(&op.solution, "v(4)", 2.0, VNTOL);
}

#[test]
fn floating_node_is_a_clean_singular_error() {
    let mut ckt = Circuit::new("floating");
    ckt.add(Resistor {
        name: "r1".into(),
        nodes: ["a".into(), "b".into()],
        value: 1e3,
    })
    .unwrap();
    let err = ckt.op(&Options::default()).unwrap_err();
    assert!(matches!(err, dune_core::Error::Singular(_)), "{err}");
}

#[test]
fn nodeset_converges_to_the_same_point() {
    let deck = "\
divider with nodeset
v1 1 0 dc 5
r1 1 2 1k
r2 2 0 2k
.nodeset v(2)=1.0
.op
.end
";
    let parsed = dune_netlist::parse_str("ns.cir", deck);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let op = ckt.op(&Options::default()).unwrap();
    assert_close(&op.solution, "v(2)", 10.0 / 3.0, VNTOL);
}

/// Regression: a junction returning to ground read the trash that
/// grounded loads dump into rhs[0] as a ground "voltage" (the C
/// zeroes `*CKTrhsOld` after every solve, niiter.c). ngspice-47:
/// i(v1) = -12.93551714646209 A for this deck.
#[test]
fn grounded_diode_reads_ground_as_zero() {
    let deck = "\
min grounded diode
v1 1 0 dc 0.9
d1 1 0 dmod
.model dmod d(is=1e-14)
.op
.end
";
    let parsed = dune_netlist::parse_str("min.cir", deck);
    let (ckt, _) = dune_core::Circuit::from_netlist(&parsed.netlist).unwrap();
    let op = ckt.op(&Options::default()).unwrap();
    assert_close(&op.solution, "v(1)", 0.9, VNTOL);
    assert_close(&op.solution, "i(v1)", -12.93551714646209, ABSTOL);
}
