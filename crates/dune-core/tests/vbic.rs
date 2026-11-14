//! The VBIC front end (`inpdomod.c` level dispatch, `INP2Q` node
//! count, `VBICparam`) and the instance independence Dune keeps where
//! ngspice-47's `VBICload` leaks `Vrth` (docs/vbic.md).

use dune_core::{Circuit, Options, Solution, Spec, Vbic, VbicModel, Vsource};

fn vsrc(name: &str, p: &str, dc: f64) -> Vsource {
    Vsource {
        name: name.into(),
        nodes: [p.into(), "0".into()],
        dc: Some(dc),
        wave: None,
        ac: None,
        distof1: None,
        distof2: None,
        port: None,
    }
}

fn elaborate(deck: &str) -> Result<Circuit, String> {
    let parsed = dune_netlist::parse_str("vbic.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    Circuit::from_netlist(&parsed.netlist)
        .map(|(c, _)| c)
        .map_err(|e| e.to_string())
}

fn refused(deck: &str) -> String {
    match elaborate(deck) {
        Ok(_) => panic!("deck elaborated: {deck}"),
        Err(e) => e,
    }
}

fn op(deck: &str) -> Solution {
    let parsed = dune_netlist::parse_str("vbic.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let opts = Options::default();
    let (mut s, _) = ckt.run(&parsed.netlist.analyses[0], &opts).unwrap();
    s.remove(0)
}

fn value(s: &Solution, name: &str) -> f64 {
    let i = s
        .variables
        .iter()
        .position(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name} in {:?}", s.variables));
    match &s.data {
        dune_raw::Data::Real(c) => c[i][0],
        dune_raw::Data::Complex(c) => c[i][0].re,
    }
}

#[test]
fn level_picks_the_bipolar_model() {
    // Five nodes on a VBIC card elaborate; the thermal node exists.
    let s = op("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 0 dt qv\n\
                .model qv npn level=4 rth=100\n.op\n.end\n");
    assert!(value(&s, "v(dt)") > 0.0);
    // Level 9 is VBIC too; the internal nodes carry the C's names.
    let s = op("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qv\n\
                .model qv pnp level=9 rcx=5\n.op\n.end\n");
    assert!(s.variables.iter().any(|(n, _)| n == "v(q1#collCI)"));
    assert!(!s.variables.iter().any(|(n, _)| n == "v(q1#collector)"));
}

#[test]
fn gummel_poon_refuses_a_fifth_node() {
    let err =
        refused("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 0 dt qg\n.model qg npn bf=100\n.op\n.end\n");
    assert!(err.contains("too many nodes"), "{err}");
}

#[test]
fn unsupported_levels_err_only_when_used() {
    // An unused HICUM card is harmless.
    assert!(elaborate("v\nv1 1 0 1\nr1 1 0 1k\n.model qh npn level=8\n.op\n.end\n").is_ok());
    let err = refused("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qh\n.model qh npn level=8\n.op\n.end\n");
    assert!(err.contains("HICUM"), "{err}");
    let err = refused("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qx\n.model qx npn level=5\n.op\n.end\n");
    assert!(err.contains("level 5"), "{err}");
}

#[test]
fn instance_parameters_are_checked() {
    let err = refused(
        "v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qv ic=0.7,2,3\n.model qv npn level=4\n.op\n.end\n",
    );
    assert!(err.contains("IC takes 1 or 2"), "{err}");
    let err =
        refused("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qv\n.model qv npn level=4 area=2\n.op\n.end\n");
    assert!(err.contains("instance parameter"), "{err}");
}

#[test]
fn positional_area_wins_over_the_keyword() {
    let a = op("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qv 3 area=2\n.model qv npn level=4\n.op\n.end\n");
    let b = op("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 qv area=3\n.model qv npn level=4\n.op\n.end\n");
    assert_eq!(value(&a, "i(vc)"), value(&b, "i(vc)"));
}

#[test]
fn named_thermal_node_without_self_heating_reads_zero() {
    let s = op("v\nvc c 0 2\nvb b 0 0.7\nq1 c b 0 0 dt qv\n\
                .model qv npn level=4 rth=100 selft=0\n.op\n.end\n");
    assert_eq!(value(&s, "v(dt)"), 0.0);
}

#[test]
fn instance_order_does_not_leak_the_thermal_rise() {
    // ngspice-47 loads q2 after q1 when q2's card comes first and runs
    // it at q1's temperature (KNOWN_DIFFS); Dune's loads are
    // independent of the order.
    let models = ".model nc npn (level=4 is=1e-16 ibei=1e-18 iben=1e-15 vef=50 ikf=50e-3)\n\
                  .model nh npn (level=4 is=1e-16 ibei=1e-18 iben=1e-15 vef=50 ikf=50e-3 \
                  rth=300 xis=3)\n";
    let cold = "vc2 c2 0 dc 2\nvb2 b2 0 dc 0.75\nq2 c2 b2 0 0 nc\n";
    let hot = "vc c 0 dc 5\nib 0 b dc 200u\nq1 c b 0 0 dt nh\n";
    let a = op(&format!("a\n{cold}{hot}{models}.op\n.end\n"));
    let b = op(&format!("b\n{hot}{cold}{models}.op\n.end\n"));
    let alone = op(&format!("c\n{cold}{models}.op\n.end\n"));
    let i = |s: &Solution| value(s, "i(vc2)");
    assert_eq!(i(&a), i(&b));
    assert!((i(&a) - i(&alone)).abs() <= 1e-9 * i(&alone).abs());
    assert!(value(&a, "v(dt)") > 20.0);
}

#[test]
fn programmatic_spec_runs() {
    let mut m = VbicModel::new(true);
    m.set("is", 1e-16).unwrap();
    m.set("rcx", 5.0).unwrap();
    let mut ckt = Circuit::default();
    ckt.add(vsrc("vc", "c", 2.0)).unwrap();
    ckt.add(vsrc("vb", "b", 0.7)).unwrap();
    ckt.add(Spec::from(Vbic {
        name: "q1".into(),
        nodes: ["c".into(), "b".into(), "0".into(), "0".into(), "0".into()],
        model: m,
        area: 1.0,
        m: 1.0,
        off: false,
        ic: (0.0, 0.0),
        temp: None,
        dtemp: 0.0,
    }))
    .unwrap();
    let s = ckt.op(&Options::default()).unwrap().solution;
    assert!(value(&s, "i(vc)") < 0.0);
    assert!(s.variables.iter().any(|(n, _)| n == "v(q1#collCI)"));
}
