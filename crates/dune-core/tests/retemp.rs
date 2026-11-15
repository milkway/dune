//! Re-expansion of a netlist that reads `temper` (D-0075): every path
//! by which a circuit is simulated at another temperature than the
//! one its deck was evaluated at must give what the deck gives when
//! it is read at that temperature.

use dune_core::{Circuit, Data, Options, Session, Solution, SweepParam, sweep};
use dune_netlist::ir::{Analysis, Netlist};

fn parse(text: &str) -> Netlist {
    let out = dune_netlist::parse_str("deck.cir", text);
    let errors: Vec<_> = out
        .diags
        .iter()
        .filter(|d| d.severity == dune_netlist::Severity::Error)
        .map(|d| d.message.clone())
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    out.netlist
}

fn options(nl: &Netlist) -> Options {
    let mut o = Options::default();
    o.apply(&nl.options, &mut Vec::new());
    o
}

fn column(sol: &Solution, name: &str) -> Vec<f64> {
    let i = sol
        .variables
        .iter()
        .position(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name}"));
    match &sol.data {
        Data::Real(cols) => cols[i].clone(),
        Data::Complex(_) => panic!("complex"),
    }
}

/// A deck whose model, device value, behavioural R and subcircuit read
/// `temper`, plus a resistor with a plain `tc1` and a diode (which
/// follow the temperature by themselves).
fn deck(temp_card: &str, analysis: &str) -> String {
    format!(
        "t\n{temp_card}\n\
         .model rm r rsh={{10+temper*0.125}}\n\
         .model dm d is={{1e-14*(1+temper/50)}}\n\
         .subckt seg a b tc=1e-3 rt={{1+tc*(temper-27)}}\n\
         r1 a b {{100*rt}}\n\
         .ends\n\
         i1 0 1 1m\n\
         r1 1 2 rm l=2 w=1\n\
         d1 2 0 dm area={{1+temper/200}}\n\
         v2 3 0 1\n\
         r2 3 4 r={{500*(1+temper/100)+10*v(4)}} tc1=1e-3\n\
         x1 4 0 seg\n\
         r5 4 0 1k tc1=2e-3\n\
         {analysis}\n.end\n"
    )
}

const NODES: [&str; 4] = ["v(1)", "v(2)", "v(3)", "v(4)"];

/// The operating point of the deck read at `t` °C.
fn op_at(t: f64) -> Vec<f64> {
    let nl = parse(&deck(&format!(".temp {t}"), ".op"));
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let (sols, _) = ckt.run(&nl.analyses[0], &options(&nl)).unwrap();
    NODES.iter().map(|n| column(&sols[0], n)[0]).collect()
}

fn close(a: &[f64], b: &[f64]) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()) + 1e-12)
}

#[test]
fn a_dc_temp_sweep_re_expands_at_every_point() {
    let nl = parse(&deck("", ".dc temp -40 120 40"));
    assert!(nl.uses_temper());
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let (sols, _) = ckt.run(&nl.analyses[0], &options(&nl)).unwrap();
    let temps = column(&sols[0], "temp-sweep");
    assert_eq!(temps.len(), 5);
    for (k, &t) in temps.iter().enumerate() {
        let got: Vec<f64> = NODES.iter().map(|n| column(&sols[0], n)[k]).collect();
        let want = op_at(t);
        // The sweep solves from the previous point (and the diode's
        // junction from a prediction): agreement to the Newton
        // tolerances (RELTOL = 1e-3), not bit identity.
        assert!(
            got.iter()
                .zip(&want)
                .all(|(a, b)| (a - b).abs() <= 1e-3 * b.abs() + 1e-6),
            "{t} °C: {got:?} vs {want:?}"
        );
    }
    // The values do move with temper (the plain-deck sweep would not).
    assert!((column(&sols[0], "v(1)")[0] - column(&sols[0], "v(1)")[4]).abs() > 1e-3);
}

#[test]
fn an_options_override_re_expands() {
    let nl = parse(&deck("", ".op"));
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let mut o = options(&nl);
    o.temp = 85.0 + dune_core::consts::CTOK;
    let (sols, _) = ckt.run(&nl.analyses[0], &o).unwrap();
    let got: Vec<f64> = NODES.iter().map(|n| column(&sols[0], n)[0]).collect();
    // Bit for bit what the deck read at 85 °C gives.
    assert_eq!(got, op_at(85.0));
    // At the deck's own temperature the circuit is used as it is.
    let (sols, _) = ckt.run(&nl.analyses[0], &options(&nl)).unwrap();
    let got: Vec<f64> = NODES.iter().map(|n| column(&sols[0], n)[0]).collect();
    assert_eq!(got, op_at(27.0));
}

#[test]
fn a_session_re_expands_and_keeps_its_alterations() {
    let nl = parse(&deck("", ".op"));
    let (mut s, _) = Session::from_netlist(&nl).unwrap();
    s.set_temp(60.0).unwrap();
    let (p, _) = s.run().unwrap();
    let got: Vec<f64> = NODES.iter().map(|n| column(&p[0], n)[0]).collect();
    assert_eq!(got, op_at(60.0));
    // An alteration survives a later change of temperature.
    s.alter("v2", "dc", 2.0).unwrap();
    s.set_temp(-10.0).unwrap();
    let (p, _) = s.run().unwrap();
    let want = {
        let nl = parse(&deck(".temp -10", ".op").replace("v2 3 0 1", "v2 3 0 2"));
        let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
        let (sols, _) = ckt.run(&nl.analyses[0], &options(&nl)).unwrap();
        NODES
            .iter()
            .map(|n| column(&sols[0], n)[0])
            .collect::<Vec<_>>()
    };
    let got: Vec<f64> = NODES.iter().map(|n| column(&p[0], n)[0]).collect();
    assert_eq!(got, want);
}

#[test]
fn a_swept_temperature_re_expands() {
    let nl = parse(&deck("", ".op"));
    let runs = sweep(
        &nl,
        &options(&nl),
        &[SweepParam::Temp, SweepParam::parse("v2").unwrap()],
        &[vec![0.0, 1.0], vec![100.0, 1.0], vec![27.0, 1.0]],
    )
    .unwrap();
    for (run, t) in runs.iter().zip([0.0, 100.0, 27.0]) {
        let out = run.outcome.as_ref().unwrap();
        let got: Vec<f64> = NODES
            .iter()
            .map(|n| column(&out.solutions[0], n)[0])
            .collect();
        assert!(close(&got, &op_at(t)), "{t}: {got:?}");
    }
}

#[test]
fn a_deck_without_temper_is_not_re_expanded() {
    // The plain path: the netlist does not ask for re-expansion and
    // an options override only moves CKTtemp.
    let text = "t\nr1 1 0 {100+3}\ni1 0 1 1\n.op\n.end\n";
    let nl = parse(text);
    assert!(!nl.uses_temper());
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let mut o = options(&nl);
    o.temp += 50.0;
    let (sols, _) = ckt.run(&nl.analyses[0], &o).unwrap();
    // `{100+3}` reaches the card as ngspice's `%.15e` text gives it.
    assert_eq!(column(&sols[0], "v(1)")[0], 103.00000000000001);
}

#[test]
fn a_topology_that_follows_the_temperature_is_refused() {
    // A MOS drain resistance that vanishes at some temperature takes
    // away the internal drain node: the .dc temp sweep cannot keep its
    // unknowns, and says so.
    let text = "t\n.model nm nmos level=1 vto=0.6 kp=1e-4 rd={max(0, 50-temper)}\n\
                m1 1 1 0 0 nm w=2u l=1u\ni1 0 1 20u\n.dc temp 0 100 50\n.end\n";
    let nl = parse(text);
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let a: &Analysis = &nl.analyses[0];
    let e = ckt.run(a, &options(&nl)).unwrap_err().to_string();
    assert!(e.contains("topology"), "{e}");
}

#[test]
fn re_expanding_sweeps_are_bounded() {
    // Every point parses the deck again: a sweep that would do it more
    // than 10 000 times is refused before the first solve.
    let text = "t\nr1 1 0 {100+temper}\ni1 0 1 1\n.dc temp 0 20000 1\n.end\n";
    let nl = parse(text);
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let e = ckt
        .run(&nl.analyses[0], &options(&nl))
        .unwrap_err()
        .to_string();
    assert!(e.contains("re-expand"), "{e}");
    // The same sweep of a deck without temper only moves CKTtemp.
    let nl = parse(&text.replace("{100+temper}", "100"));
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    assert!(ckt.run(&nl.analyses[0], &options(&nl)).is_ok());
}

#[test]
fn a_seeded_temperature_sweep_keeps_its_draws() {
    // D-0075: the draws of a seeded deck do not change when it is
    // re-expanded at another temperature, so each point of the .dc
    // temp sweep equals the deck read (and drawn) at that temperature.
    let text = |temp: &str, an: &str| {
        format!(
            "t\n{temp}\n.options seed=5\n.param p=agauss(1k,100,1)\n\
             .subckt seg a b\nr1 a b {{p*(1+temper/1000)}}\n.ends\n\
             i1 0 1 1m\nx1 1 2 seg\nx2 2 0 seg\n{an}\n.end\n"
        )
    };
    let nl = parse(&text("", ".dc temp 0 100 50"));
    let (ckt, _) = Circuit::from_netlist(&nl).unwrap();
    let (sols, _) = ckt.run(&nl.analyses[0], &options(&nl)).unwrap();
    for (k, t) in [0.0, 50.0, 100.0].into_iter().enumerate() {
        let one = parse(&text(&format!(".temp {t}"), ".op"));
        let (c1, _) = Circuit::from_netlist(&one).unwrap();
        let (s1, _) = c1.run(&one.analyses[0], &options(&one)).unwrap();
        for n in ["v(1)", "v(2)"] {
            let (a, b) = (column(&sols[0], n)[k], column(&s1[0], n)[0]);
            // Linear circuit: the same draws give the same voltages.
            assert!((a - b).abs() <= 1e-12 * b.abs(), "{t} {n}: {a} vs {b}");
        }
    }
    // Re-expanding at the deck's own temperature is bit-identical.
    let op = parse(&text("", ".op"));
    let (again, _) = op.at_temperature(27.0);
    let run = |nl: &Netlist| {
        let (c, _) = Circuit::from_netlist(nl).unwrap();
        let (s, _) = c.run(&nl.analyses[0], &options(nl)).unwrap();
        NODES[..2]
            .iter()
            .map(|n| column(&s[0], n)[0].to_bits())
            .collect::<Vec<_>>()
    };
    assert_eq!(run(&op), run(&again));
}
