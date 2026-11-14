//! A build without the `bsim` feature (D-0060): a deck that
//! instantiates a BSIM3 model is refused with a message naming the
//! missing feature, and an unused BSIM3 card is harmless.
#![cfg(not(feature = "bsim"))]

use dune_core::Circuit;

fn deck(name: &str) -> String {
    let path = format!("{}/../../corpus/regress/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn bsim3_instance_names_the_missing_feature() {
    let parsed = dune_netlist::parse_str("bsim3_iv.cir", &deck("bsim3_iv.cir"));
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let err = match Circuit::from_netlist(&parsed.netlist) {
        Ok(_) => panic!("a BSIM3 deck elaborated without the `bsim` feature"),
        Err(e) => e.to_string(),
    };
    assert!(
        err.contains("BSIM3") && err.contains("not available in this build"),
        "{err}"
    );
}

#[test]
fn unused_bsim3_card_is_harmless() {
    let text = "unused bsim3 card\nv1 1 0 1\nr1 1 0 1k\n.model n1 nmos level=8\n.op\n.end\n";
    let parsed = dune_netlist::parse_str("unused.cir", text);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    assert!(Circuit::from_netlist(&parsed.netlist).is_ok());
}
