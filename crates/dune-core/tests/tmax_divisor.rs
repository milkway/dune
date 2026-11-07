//! `Options::tmax_divisor` (issue #16): a run with the divisor is the
//! run of the deck whose `.tran` card carries `TMAX/d`, bit for bit.

use dune_core::{Circuit, Options, Solution};

fn run(text: &str, tweak: impl Fn(&mut Options)) -> Vec<Solution> {
    let parsed = dune_netlist::parse_str("deck.cir", text);
    assert!(parsed.diags.is_empty(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut diags = Vec::new();
    opts.apply(&parsed.netlist.options, &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
    tweak(&mut opts);
    let mut out = Vec::new();
    for a in &parsed.netlist.analyses {
        out.extend(ckt.run(a, &opts).unwrap().0);
    }
    out
}

/// The real columns of a run, as bits.
fn bits(sols: &[Solution]) -> Vec<Vec<u64>> {
    sols.iter()
        .flat_map(|s| match &s.data {
            dune_raw::Data::Real(cols) => cols
                .iter()
                .map(|c| c.iter().map(|x| x.to_bits()).collect())
                .collect::<Vec<_>>(),
            dune_raw::Data::Complex(_) => unreachable!(),
        })
        .collect()
}

const RC: &str = "rc\nV1 Vin 0 PULSE(0 5 0 1u 1u 10m 20m)\nR1 Vin Vout 1k\nC1 Vout 0 1u\n";
const RLC: &str = "rlc\nV1 in 0 PULSE(0 1 0 1n 1n 1 2)\nR1 in n1 20\nL1 n1 out 10m\nC1 out 0 1u\n";

#[test]
fn divisor_matches_the_edited_card_default_tmax() {
    // Default TMAX = min(20u, 5m/50) = 20u; /10 = 2u.
    let api = run(&format!("{RC}.tran 20u 5m\n.end\n"), |o| {
        o.trtol = 1.0;
        o.tmax_divisor = Some(10.0);
    });
    let deck = run(
        &format!("{RC}.options trtol=1\n.tran 20u 5m 0 2u\n.end\n"),
        |_| {},
    );
    let plain = run(&format!("{RC}.tran 20u 5m\n.end\n"), |_| {});
    assert_eq!(bits(&api), bits(&deck));
    assert!(api[0].column("time").unwrap().len() > plain[0].column("time").unwrap().len());
}

#[test]
fn divisor_matches_the_edited_card_explicit_tmax() {
    // Card TMAX 10u; /10 = 1u. The RLC rings at ~1.6 kHz.
    let api = run(&format!("{RLC}.tran 10u 5m 0 10u\n.end\n"), |o| {
        o.trtol = 1.0;
        o.tmax_divisor = Some(10.0);
    });
    let deck = run(
        &format!("{RLC}.options trtol=1\n.tran 10u 5m 0 1u\n.end\n"),
        |_| {},
    );
    assert_eq!(bits(&api), bits(&deck));
}

#[test]
fn divisor_one_and_none_agree() {
    let deck = format!("{RC}.tran 20u 5m\n.end\n");
    assert_eq!(
        bits(&run(&deck, |o| o.tmax_divisor = Some(1.0))),
        bits(&run(&deck, |_| {}))
    );
}

#[test]
fn divisor_out_of_range_is_refused() {
    let parsed = dune_netlist::parse_str("deck.cir", &format!("{RC}.tran 20u 5m\n.end\n"));
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    for d in [0.5, 0.0, -2.0, f64::NAN, f64::INFINITY] {
        let opts = Options {
            tmax_divisor: Some(d),
            ..Options::default()
        };
        let err = ckt.run(&parsed.netlist.analyses[0], &opts).unwrap_err();
        assert!(err.to_string().contains("tmax divisor"), "{d}: {err}");
    }
}
