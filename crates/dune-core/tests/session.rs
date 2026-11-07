//! `Session` (issue #12): alter + re-run must be the fresh run of the
//! altered deck bit for bit (warm start off), and the opt-in warm
//! start must land on the same operating point to the Newton
//! tolerances.

use dune_core::{AlterValue, Circuit, Data, Options, Session, Solution};
use dune_netlist::ir;

/// Parse, elaborate, run every card: what `dune -b` does.
fn fresh(deck: &str) -> Vec<Solution> {
    let parsed = dune_netlist::parse_str("deck.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut o = Options::default();
    o.apply(&parsed.netlist.options, &mut Vec::new());
    let mut out = Vec::new();
    for a in &parsed.netlist.analyses {
        out.extend(c.run(a, &o).unwrap().0);
    }
    out
}

fn session(deck: &str) -> Session {
    let parsed = dune_netlist::parse_str("deck.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    Session::from_netlist(&parsed.netlist).unwrap().0
}

/// The value the deck parser (`INPevaluate`, which is not correctly
/// rounded) gives `text`: what an altered card would carry.
fn val(text: &str) -> f64 {
    let p = dune_netlist::parse_str("v.cir", &format!("v\nr1 1 0 {text}\n.end\n"));
    match &p.netlist.elements[0].device {
        ir::Device::Resistor { value, .. } => value.unwrap(),
        d => panic!("{d:?}"),
    }
}

/// Same plots, same names, same bits.
fn assert_bits(got: &[Solution], want: &[Solution], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: plot count");
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.plotname, w.plotname, "{what}");
        assert_eq!(g.variables, w.variables, "{what}: {}", w.plotname);
        assert_eq!(g.scale, w.scale, "{what}: {}", w.plotname);
        let bits = |d: &Data| -> Vec<Vec<u64>> {
            match d {
                Data::Real(c) => c
                    .iter()
                    .map(|c| c.iter().map(|x| x.to_bits()).collect())
                    .collect(),
                Data::Complex(c) => c
                    .iter()
                    .map(|c| {
                        c.iter()
                            .flat_map(|z| [z.re.to_bits(), z.im.to_bits()])
                            .collect()
                    })
                    .collect(),
            }
        };
        assert!(
            bits(&g.data) == bits(&w.data),
            "{what}: {} differs from the fresh run",
            w.plotname
        );
    }
}

fn plotnames(s: &[Solution]) -> Vec<&str> {
    s.iter().map(|p| p.plotname.as_str()).collect()
}

const RLC: &str = "rlc with every analysis
vin 1 0 dc 1 ac 1 pulse(0 1 1n 1n 1n 5u 10u)
r1 1 2 1k
l1 2 3 1m
c1 3 0 1n
r2 3 0 10k
d1 3 0 dmod
.model dmod d is=1e-14 cjo=1p
.op
.dc vin 0 2 0.5
.ac dec 10 1k 10meg
.tran 0.1u 20u
.end
";

#[test]
fn rlc_values_match_a_fresh_run_of_the_altered_deck() {
    let mut s = session(RLC);
    // The first run is the fresh run.
    assert_bits(&s.run().unwrap().0, &fresh(RLC), "unaltered");

    s.alter("R1", "", val("2.2k")).unwrap();
    let deck = RLC.replace("r1 1 2 1k", "r1 1 2 2.2k");
    let got = s.run().unwrap().0;
    assert_eq!(
        plotnames(&got),
        [
            "Operating Point",
            "DC transfer characteristic",
            "AC Analysis",
            "Transient Analysis"
        ]
    );
    assert_bits(&got, &fresh(&deck), "R");

    s.alter("c1", "capacitance", val("2.2n")).unwrap();
    let deck = deck.replace("c1 3 0 1n", "c1 3 0 2.2n");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "R+C");

    s.alter("l1", "l", val("0.47m")).unwrap();
    let deck = deck.replace("l1 2 3 1m", "l1 2 3 0.47m");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "R+C+L");

    // Back to the original values: the original bits.
    s.alter("r1", "r", val("1k")).unwrap();
    s.alter("c1", "", val("1n")).unwrap();
    s.alter("l1", "inductance", val("1m")).unwrap();
    assert_bits(&s.run().unwrap().0, &fresh(RLC), "restored");
}

#[test]
fn source_levels_and_waves_match_a_fresh_run() {
    let mut s = session(RLC);
    s.alter("vin", "dc", val("1.5")).unwrap();
    s.alter("vin", "acmag", val("2")).unwrap();
    s.alter("vin", "acphase", val("45")).unwrap();
    s.alter(
        "vin",
        "pulse",
        vec![
            val("0"),
            val("2"),
            val("2n"),
            val("0.5n"),
            val("3n"),
            val("4u"),
            val("8u"),
        ],
    )
    .unwrap();
    let deck = RLC.replace(
        "vin 1 0 dc 1 ac 1 pulse(0 1 1n 1n 1n 5u 10u)",
        "vin 1 0 dc 1.5 ac 2 45 pulse(0 2 2n 0.5n 3n 4u 8u)",
    );
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "V source");

    // Another function kind replaces the pulse.
    s.alter("vin", "sin", vec![val("0.5"), val("1"), val("100k")])
        .unwrap();
    let deck = deck.replace("pulse(0 2 2n 0.5n 3n 4u 8u)", "sin(0.5 1 100k)");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "sin");
}

const ISRC: &str = "current source and controlled sources
i1 0 1 dc 1m ac 1
r1 1 0 1k
e1 2 0 1 0 2
r2 2 0 1k
.op
.ac lin 3 1k 3k
.end
";

#[test]
fn current_source_and_gain_match_a_fresh_run() {
    let mut s = session(ISRC);
    s.alter("i1", "", val("2m")).unwrap();
    s.alter("i1", "acphase", val("90")).unwrap();
    s.alter("e1", "gain", val("3")).unwrap();
    let deck = ISRC
        .replace("i1 0 1 dc 1m ac 1", "i1 0 1 dc 2m ac 1 90")
        .replace("e1 2 0 1 0 2", "e1 2 0 1 0 3");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "I + E");
}

const MOSAMP: &str = "mos1 common-source amp
vdd 1 0 dc 5
vin 2 0 dc 1.2 ac 1 sin(1.2 0.1 1meg)
rd 1 3 10k
m1 3 2 0 0 nm l=2u w=10u ad=20p as=20p
cl 3 0 0.1p
.model nm nmos level=1 vto=0.7 kp=50u lambda=0.02 cgso=1n cgdo=1n cbd=10f cbs=10f
.op
.dc vin 0 3 0.25
.ac dec 5 1k 100meg
.tran 20n 3u
.end
";

#[test]
fn mos_width_length_and_temp_match_a_fresh_run() {
    let mut s = session(MOSAMP);
    s.alter("m1", "w", val("20u")).unwrap();
    s.alter("m1", "L", val("1.5u")).unwrap();
    let deck = MOSAMP.replace("l=2u w=10u", "l=1.5u w=20u");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "MOS W/L");

    s.alter("m1", "temp", val("85")).unwrap();
    let deck = deck.replace("l=1.5u w=20u", "l=1.5u w=20u temp=85");
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "MOS instance temp");
}

#[test]
fn circuit_temperature_matches_a_fresh_run() {
    for deck in [RLC, MOSAMP] {
        let mut s = session(deck);
        s.set_temp(val("85")).unwrap();
        assert!((s.temp() - 85.0).abs() < 1e-12);
        let altered = deck.replace(".end\n", ".options temp=85\n.end\n");
        assert_bits(&s.run().unwrap().0, &fresh(&altered), "TEMP");
    }
}

#[test]
fn bsim3_width_matches_a_fresh_run() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/regress/bsim3_inv.cir"
    );
    let deck = std::fs::read_to_string(path).unwrap();
    let mut s = session(&deck);
    assert_bits(&s.run().unwrap().0, &fresh(&deck), "bsim3 unaltered");
    s.alter("mn1", "w", val("3u")).unwrap();
    s.alter("rl", "", val("4.7k")).unwrap();
    let altered = deck
        .replace("mn1 3 2 0 0 n1 l=0.35u w=2u", "mn1 3 2 0 0 n1 l=0.35u w=3u")
        .replace("rl 1 7 5k", "rl 1 7 4.7k");
    assert_ne!(altered, deck);
    assert_bits(&s.run().unwrap().0, &fresh(&altered), "bsim3 W");
}

#[test]
fn param_reads_back_what_alter_wrote() {
    let mut s = session(RLC);
    assert_eq!(s.param("r1", "").unwrap(), Some(AlterValue::Num(1e3)));
    assert_eq!(
        s.param("vin", "acphase").unwrap(),
        Some(AlterValue::Num(0.0))
    );
    assert_eq!(s.param("vin", "sin").unwrap(), None);
    assert!(matches!(
        s.param("vin", "pulse").unwrap(),
        Some(AlterValue::List(a)) if a.len() == 7
    ));
    s.alter("d1", "temp", 50.0).unwrap();
    assert_eq!(s.param("d1", "temp").unwrap(), Some(AlterValue::Num(50.0)));
    let m = session(MOSAMP);
    assert_eq!(m.param("m1", "temp").unwrap(), None);
    assert_eq!(
        m.circuit().alterable("m1").unwrap(),
        ["w", "l", "temp"].as_slice()
    );
}

#[test]
fn bad_alterations_are_errors_and_change_nothing() {
    let mut s = session(RLC);
    let before = s.run().unwrap().0;
    for (r, p, v) in [
        ("rx", "", AlterValue::Num(1.0)),
        ("r1", "w", AlterValue::Num(1.0)),
        ("r1", "", AlterValue::Num(0.0)),
        ("r1", "", AlterValue::Num(f64::NAN)),
        ("r1", "", AlterValue::List(vec![1.0])),
        ("vin", "pulse", AlterValue::Num(1.0)),
        ("vin", "pwl", AlterValue::List(vec![0.0, f64::INFINITY])),
        ("vin", "dc", AlterValue::Num(f64::INFINITY)),
    ] {
        let e = s.alter(r, p, v.clone()).unwrap_err();
        assert!(
            matches!(e, dune_core::Error::Build(_)),
            "{r} {p} {v:?}: {e}"
        );
    }
    let msg = s.alter("r1", "w", 1.0).unwrap_err().to_string();
    assert!(msg.contains("valid: resistance"), "{msg}");
    let mut m = session(MOSAMP);
    assert!(m.alter("m1", "w", -1e-6).is_err());
    assert!(s.set_temp(f64::NAN).is_err());
    assert_bits(&s.run().unwrap().0, &before, "after rejected alters");
}

#[test]
fn a_cloned_session_is_independent() {
    let mut a = session(RLC);
    let mut b = a.clone();
    b.alter("r1", "", 5e3).unwrap();
    assert_bits(&a.run().unwrap().0, &fresh(RLC), "original");
    assert_ne!(
        a.param("r1", "").unwrap(),
        b.param("r1", "").unwrap(),
        "clone shares specs"
    );
}

// ---- warm start -------------------------------------------------------

/// Every real vector of `got` within the 2×SPICE ruler (D-0009) of
/// `want`: `2·(reltol·max(|a|,|b|) + floor)`, `vntol` for voltages and
/// `abstol` for currents. Complex columns componentwise.
fn assert_within_newton_tol(got: &[Solution], want: &[Solution], o: &Options, what: &str) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g.variables, w.variables);
        let floor = |name: &str| {
            if name.starts_with("i(") {
                o.abstol
            } else {
                o.vntol
            }
        };
        let close = |a: f64, b: f64, name: &str| {
            (a - b).abs() <= 2.0 * (o.reltol * a.abs().max(b.abs()) + floor(name))
        };
        for (k, (name, _)) in w.variables.iter().enumerate() {
            match (&g.data, &w.data) {
                (Data::Real(gc), Data::Real(wc)) => {
                    for (a, b) in gc[k].iter().zip(&wc[k]) {
                        assert!(close(*a, *b, name), "{what}: {name} {a} vs {b}");
                    }
                }
                (Data::Complex(gc), Data::Complex(wc)) => {
                    for (a, b) in gc[k].iter().zip(&wc[k]) {
                        assert!(
                            close(a.re, b.re, name) && close(a.im, b.im, name),
                            "{what}: {name} {a} vs {b}"
                        );
                    }
                }
                _ => panic!("{what}: data kinds differ"),
            }
        }
    }
}

const OP_AC: &str = "
.op
.ac dec 5 1k 100meg
.end
";

#[test]
fn warm_start_changes_the_cost_not_the_operating_point() {
    // (deck, element, param, values): the op-based cards of nonlinear
    // decks, altered in steps a slider would make.
    let mosamp2 = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/spice3f5/examples/mosamp2.cir"
    ))
    .unwrap();
    let mosamp_op = MOSAMP
        .replace(".dc vin 0 3 0.25\n", "")
        .replace(".tran 20n 3u\n", "");
    let rlc_op = RLC
        .replace(".dc vin 0 2 0.5\n", "")
        .replace(".tran 0.1u 20u\n", "");
    let mosamp2_op = cut_cards(&mosamp2) + OP_AC;
    let cases: [(&str, &str, &str, &[f64]); 4] = [
        (&rlc_op, "r1", "", &[1.1e3, 1.3e3, 2e3, 900.0]),
        (&mosamp_op, "m1", "w", &[11e-6, 12e-6, 15e-6, 8e-6]),
        (&mosamp_op, "vin", "dc", &[1.25, 1.3, 1.0, 2.0]),
        (&mosamp2_op, "m7", "w", &[490e-6, 500e-6, 470e-6]),
    ];
    for (deck, el, p, values) in cases {
        let mut cold = session(deck);
        let mut warm = session(deck);
        warm.set_warm_start(true);
        // The first card of the first run has nothing to start
        // from (the fresh bits); the later cards continue from it.
        let (w0, c0) = (warm.run().unwrap().0, cold.run().unwrap().0);
        assert_bits(&w0[..1], &c0[..1], "first op");
        assert_within_newton_tol(&w0, &c0, cold.options(), "first run");
        for &x in values {
            cold.alter(el, p, x).unwrap();
            warm.alter(el, p, x).unwrap();
            let want = cold.run().unwrap().0;
            let got = warm.run().unwrap().0;
            assert_within_newton_tol(&got, &want, cold.options(), &format!("{el} {p}={x}"));
        }
        let st = warm.warm_stats();
        assert!(st.hits >= values.len() as u64, "{el}: {st:?}");
        assert_eq!(cold.warm_stats().hits, 0);
    }
}

/// `deck` without its analysis cards and `.end`.
fn cut_cards(deck: &str) -> String {
    deck.lines()
        .filter(|l| {
            let l = l.trim_start().to_ascii_lowercase();
            ![
                ".op", ".ac", ".dc", ".tran", ".end", ".print", ".plot", ".width",
            ]
            .iter()
            .any(|c| l.split_whitespace().next() == Some(*c))
        })
        .map(|l| format!("{l}\n"))
        .collect()
}

#[test]
fn a_warm_miss_reruns_cold_and_gives_the_fresh_bits() {
    // ITL1 = 2 leaves the warm Newton one convergence check: a jump
    // from 1 V to 100 V across a diode cannot pass it, while the
    // cold path falls through to gmin stepping (ITL2) as usual.
    let deck = "diode clamp
v1 1 0 dc 1
r1 1 2 100
d1 2 0 dmod
.model dmod d is=1e-14 n=1
.options itl1=2
.op
.end
";
    let mut s = session(deck);
    s.set_warm_start(true);
    s.run().unwrap();
    s.alter("v1", "dc", 100.0).unwrap();
    let got = s.run().unwrap().0;
    assert_eq!(s.warm_stats().misses, 1, "{:?}", s.warm_stats());
    assert_bits(&got, &fresh(&deck.replace("dc 1\n", "dc 100\n")), "miss");
}

#[test]
fn nodesets_and_transients_are_never_warm_started() {
    let deck = "nodeset
v1 1 0 dc 1
r1 1 2 1k
d1 2 0 dmod
.model dmod d
.nodeset v(2)=0.6
.op
.end
";
    let mut s = session(deck);
    s.set_warm_start(true);
    s.run().unwrap();
    s.alter("r1", "", 2e3).unwrap();
    assert_bits(
        &s.run().unwrap().0,
        &fresh(&deck.replace("r1 1 2 1k", "r1 1 2 2k")),
        "nodeset",
    );
    let mut t = session(
        &RLC.replace(".op\n", "")
            .replace(".ac dec 10 1k 10meg\n", ""),
    );
    t.set_warm_start(true);
    t.run().unwrap();
    t.alter("r1", "", 2e3).unwrap();
    t.run().unwrap();
    assert_eq!(s.warm_stats().hits + t.warm_stats().hits, 0);
}

#[test]
fn subcircuit_elements_alter_by_their_expanded_name() {
    let deck = "subckt divider
v1 1 0 dc 2
x1 1 2 half
r9 2 0 1meg
.subckt half a b
r1 a b 1k
r2 b 0 1k
.ends
.op
.end
";
    let mut s = session(deck);
    s.alter("r.x1.r2", "", val("3k")).unwrap();
    let altered = deck.replace("r2 b 0 1k", "r2 b 0 3k");
    assert_bits(&s.run().unwrap().0, &fresh(&altered), "subckt");
}
