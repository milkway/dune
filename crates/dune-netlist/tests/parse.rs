//! Driver-level tests: whole decks in, IR out.

use dune_netlist::ir::*;
use dune_netlist::{Severity, parse_str};

fn clean(deck: &str) -> Netlist {
    let out = parse_str("test.cir", deck);
    let errs: Vec<String> = out
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&out.sources))
        .collect();
    assert!(errs.is_empty(), "unexpected errors: {errs:#?}");
    out.netlist
}

fn errors(deck: &str) -> Vec<String> {
    let out = parse_str("test.cir", deck);
    out.diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn a_whole_small_deck() {
    let nl = clean(
        "basic rc circuit\n\
         .options reltol=1e-4 method=gear\n\
         vin 1 0 pulse(0 5 1ns 1ns 1ns 500ns 1us)\n\
         r1 1 2 1k\n\
         c1 2 0 1uf\n\
         .tran 10ns 1us\n\
         .print tran v(1) v(2) i(vin)\n\
         .ic v(2)=0\n\
         .end\n",
    );
    assert_eq!(nl.title, "basic rc circuit");
    assert_eq!(nl.elements.len(), 3);
    assert_eq!(nl.elements[1].name, "r1");
    assert!(matches!(
        nl.analyses[0],
        Analysis::Tran { tstep, tstop, uic: false, .. } if tstep == 1e-8 && tstop == 1e-6
    ));
    assert_eq!(nl.options.len(), 2);
    assert_eq!(nl.options[1].value, ParamValue::Str("gear".into()));
    assert_eq!(nl.outputs.len(), 1);
    assert_eq!(
        nl.outputs[0].vars[2],
        OutVar::Current {
            vsrc: "vin".into(),
            suffix: None
        }
    );
    assert_eq!(nl.node_inits[0].node, "2");
}

#[test]
fn subckt_expansion_follows_ngspice_naming() {
    let nl = clean(
        "two inverters\n\
         .subckt inv in out\n\
         rload vdd out 10k\n\
         mpd out in 0 0 mmod l=4u w=6u\n\
         .ends inv\n\
         .model mmod nmos level=1\n\
         x1 a b inv\n\
         x2 b c inv\n\
         vdd vdd 0 5\n\
         .end\n",
    );
    let names: Vec<&str> = nl.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["r.x1.rload", "m.x1.mpd", "r.x2.rload", "m.x2.mpd", "vdd"]
    );
    // Ports map to the caller's nodes; `vdd` is an internal node of
    // the subcircuit (not a port), so it gets the instance prefix;
    // ground stays 0.
    assert_eq!(nl.elements[0].nodes, ["x1.vdd", "b"]);
    assert_eq!(nl.elements[1].nodes, ["b", "a", "0", "0"]);
    // The global model is not renamed.
    match &nl.elements[1].device {
        Device::Mosfet { model, .. } => assert_eq!(model, "mmod"),
        d => panic!("{d:?}"),
    }
}

#[test]
fn nested_subckts_and_local_models() {
    let nl = clean(
        "nested\n\
         .subckt outer a b\n\
         .model dloc d is=1e-14\n\
         .subckt inner p q\n\
         r1 p q 1k\n\
         .ends inner\n\
         d1 a mid dloc\n\
         xi mid b inner\n\
         .ends outer\n\
         xo in out outer\n\
         .end\n",
    );
    let names: Vec<&str> = nl.elements.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["d.xo.d1", "r.xo.xi.r1"]);
    assert_eq!(nl.elements[0].nodes, ["in", "xo.mid"]);
    assert_eq!(nl.elements[1].nodes, ["xo.mid", "out"]);
    // The local model was instantiated under the instance path and
    // the element reference follows it.
    match &nl.elements[0].device {
        Device::Diode { model, .. } => assert_eq!(model, "xo.dloc"),
        d => panic!("{d:?}"),
    }
    assert!(nl.models.iter().any(|m| m.name == "xo.dloc"));
}

#[test]
fn device_references_are_renamed_too() {
    let nl = clean(
        "coupling in a subckt\n\
         .subckt pair a b\n\
         l1 a 0 1m\n\
         l2 b 0 1m\n\
         k1 l1 l2 0.98\n\
         vs a 0 0\n\
         f1 a b vs 2\n\
         .ends\n\
         x7 p q pair\n\
         .end\n",
    );
    let k = nl
        .elements
        .iter()
        .find(|e| e.name == "k.x7.k1")
        .expect("coupled inductors");
    assert!(matches!(
        &k.device,
        Device::Mutual { l1, l2, .. } if l1 == "l.x7.l1" && l2 == "l.x7.l2"
    ));
    let f = nl.elements.iter().find(|e| e.name == "f.x7.f1").unwrap();
    assert!(matches!(
        &f.device,
        Device::Cccs { vname, .. } if vname == "v.x7.vs"
    ));
}

#[test]
fn params_work_in_values() {
    let nl = clean(
        "parameterized\n\
         .param rload=10k two=2\n\
         .param rhalf={rload/two}\n\
         r1 1 0 {rload}\n\
         r2 1 0 {rhalf*two}\n\
         v1 1 0 dc {two}\n\
         .end\n",
    );
    assert!(matches!(
        nl.elements[0].device,
        Device::Resistor { value: Some(v), .. } if v == 1e4
    ));
    assert!(matches!(
        nl.elements[1].device,
        Device::Resistor { value: Some(v), .. } if v == 1e4
    ));
    assert!(matches!(
        nl.elements[2].device,
        Device::Source { dc: Some(d), .. } if d == 2.0
    ));
}

#[test]
fn control_block_is_kept_verbatim() {
    let nl = clean(
        "with control\n\
         r1 1 0 1k\n\
         .control\n\
         run\n\
         print v(1)\n\
         .endc\n\
         .end\n",
    );
    assert_eq!(nl.control, ["run", "print v(1)"]);
    assert_eq!(nl.elements.len(), 1);
}

#[test]
fn structural_errors_are_diagnosed() {
    assert!(
        errors("t\n.subckt broken a b\nr1 a b 1\n.end\n")
            .iter()
            .any(|e| e.contains("never closed"))
    );
    assert!(
        errors("t\nx1 a b nope\n.end\n")
            .iter()
            .any(|e| e.contains("no such subcircuit"))
    );
    assert!(
        errors("t\n.subckt s a b\nr1 a b 1\n.ends\nx1 a b c s\n.end\n")
            .iter()
            .any(|e| e.contains("2 ports but the call connects 3"))
    );
    assert!(
        errors("t\n.bogus 1 2\n.end\n")
            .iter()
            .any(|e| e.contains("unknown control card"))
    );
    assert!(
        errors("t\n.subckt s a\n.tran 1n 1u\n.ends\n.end\n")
            .iter()
            .any(|e| e.contains("not allowed inside .subckt"))
    );
}

#[test]
fn recursive_subckt_terminates_with_an_error() {
    let out = parse_str(
        "test.cir",
        "recursive\n\
         .subckt loop a b\n\
         xback a b loop\n\
         .ends\n\
         x1 p q loop\n\
         .end\n",
    );
    assert!(
        out.diags
            .iter()
            .any(|d| d.message.contains("nested deeper")),
        "{:?}",
        out.diags
    );
}

#[test]
fn analyses_parse() {
    let nl = clean(
        "all analyses\n\
         v1 1 0 ac 1\n\
         r1 1 0 50\n\
         .ac dec 10 1 10meg\n\
         .dc v1 0 5 0.1\n\
         .op\n\
         .pz 1 0 1 0 cur pz\n\
         .tf v(1) v1\n\
         .sens v(1)\n\
         .end\n",
    );
    assert_eq!(nl.analyses.len(), 6);
    assert!(matches!(
        &nl.analyses[0],
        Analysis::Ac { variation: Variation::Dec, points, fstop, .. }
            if *points == 10.0 && *fstop == 1e7
    ));
    assert!(matches!(
        &nl.analyses[1],
        Analysis::Dc { sweeps, .. } if sweeps.len() == 1 && sweeps[0].src == "v1"
    ));
    assert!(matches!(
        &nl.analyses[3],
        Analysis::Pz {
            which: PzWhich::Both,
            input: PzInput::Current,
            ..
        }
    ));
    assert!(matches!(
        &nl.analyses[4],
        Analysis::Tf { outvar: OutVar::Voltage { node, .. }, insrc, .. }
            if node == "1" && insrc == "v1"
    ));
}

#[test]
fn tf_and_sens_cards() {
    let nl = clean(
        "tf and sens\n\
         v1 1 0 ac 1\n\
         r1 1 2 1k\n\
         v2 2 0 0\n\
         .tf v( 2 , 1 ) v1\n\
         .tf i(v2) v1\n\
         .sens v(2) r* d1:is d1_m ac dec 5 10 1k\n\
         .sens i(v2) dc\n\
         .sens v(1,2)\n\
         .end\n",
    );
    assert_eq!(nl.analyses.len(), 5);
    assert!(matches!(
        &nl.analyses[0],
        Analysis::Tf { outvar: OutVar::Voltage { node, reference: Some(r), .. }, insrc, .. }
            if node == "2" && r == "1" && insrc == "v1"
    ));
    assert!(matches!(
        &nl.analyses[1],
        Analysis::Tf { outvar: OutVar::Current { vsrc, .. }, insrc, .. }
            if vsrc == "v2" && insrc == "v1"
    ));
    match &nl.analyses[2] {
        Analysis::Sens {
            outvar:
                OutVar::Voltage {
                    node,
                    reference: None,
                    ..
                },
            filters,
            ac: Some((Variation::Dec, points, fstart, fstop)),
            ..
        } => {
            assert_eq!(node, "2");
            assert_eq!(filters, &["r*", "d1:is", "d1_m"]);
            assert_eq!((*points, *fstart, *fstop), (5.0, 10.0, 1e3));
        }
        other => panic!("unexpected {other:?}"),
    }
    assert!(matches!(
        &nl.analyses[3],
        Analysis::Sens { outvar: OutVar::Current { vsrc, .. }, filters, ac: None, .. }
            if vsrc == "v2" && filters.is_empty()
    ));
    assert!(matches!(
        &nl.analyses[4],
        Analysis::Sens { outvar: OutVar::Voltage { reference: Some(r), .. }, ac: None, .. }
            if r == "2"
    ));
}

#[test]
fn output_variable_forms() {
    let nl = clean(
        "outputs\n\
         v1 1 0 ac 1\n\
         r1 1 2 1k\n\
         r2 2 0 1k\n\
         .ac dec 10 1 1k\n\
         .print ac vm(2) vp(2) vdb(2,1) i(v1)\n\
         .save all v(2)\n\
         .end\n",
    );
    let print = &nl.outputs[0];
    assert_eq!(
        print.vars[0],
        OutVar::Voltage {
            node: "2".into(),
            reference: None,
            suffix: Some(AcSuffix::Mag)
        }
    );
    assert_eq!(
        print.vars[2],
        OutVar::Voltage {
            node: "2".into(),
            reference: Some("1".into()),
            suffix: Some(AcSuffix::Db)
        }
    );
    let save = &nl.outputs[1];
    assert_eq!(save.vars[0], OutVar::Raw("all".into()));
}

#[test]
fn hostile_decks_do_not_hang_or_panic() {
    // Garbage bytes, overlong tokens, stray controls.
    let mut junk = String::from("junk deck\n");
    junk.push_str(&"+".repeat(2000));
    junk.push('\n');
    junk.push_str(&"r1 1 0 ".repeat(500));
    junk.push('\n');
    junk.push_str(".subckt\n.ends\n.ends\n.endc\n");
    junk.push_str("v1 1 0 pulse(\n");
    junk.push_str(".model\n.model m\n.model m d(\n");
    junk.push_str(".print\n.save\n.four\n.tf\n.noise\n.ac\n.dc srC\n");
    let out = parse_str("test.cir", &junk);
    assert!(!out.diags.is_empty());
}

#[test]
fn subckt_bomb_is_bounded() {
    // 8 levels of 8-way fan-out would flatten to 8^8 = 16M copies;
    // the expansion budget must cut it off with one error instead.
    let mut deck = String::from("bomb\n.subckt s0 a b\nr1 a b 1k\n.ends\n");
    for level in 1..=8 {
        deck.push_str(&format!(".subckt s{level} a b\n"));
        for i in 0..8 {
            deck.push_str(&format!("x{i} a b s{}\n", level - 1));
        }
        deck.push_str(".ends\n");
    }
    deck.push_str("xtop p q s8\n.end\n");
    let out = dune_netlist::parse_str("bomb.cir", &deck);
    assert!(
        out.diags
            .iter()
            .any(|d| d.message.contains("expansion exceeds")),
        "{:?}",
        out.diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(out.netlist.elements.len() <= 1 << 20);
}

#[test]
fn measure_cards_parse_with_continuations() {
    let nl = clean(
        "meas\n\
         v1 1 0 pulse(0 1 1n)\n\
         r1 1 2 1k\n\
         c1 2 0 1p\n\
         .tran 10p 10n\n\
         .meas tran tdelay trig v(1) val=0.5 rise=1\n\
         + targ v(2) val=0.5 rise=1\n\
         .measure AC f3db WHEN vdb(2)=-3\n\
         .meas dc vmax MAX v(2) from=0 to=1\n\
         .end\n",
    );
    assert_eq!(nl.measures.len(), 3);
    let m = &nl.measures[0];
    assert_eq!(m.analysis, MeasAnalysis::Tran);
    assert_eq!(m.name, "tdelay");
    assert_eq!(m.keyword, "trig");
    assert!(matches!(m.func, MeasFunc::Delay { .. }));
    assert_eq!(nl.measures[1].analysis, MeasAnalysis::Ac);
    assert!(matches!(nl.measures[1].func, MeasFunc::When(_)));
    assert!(matches!(
        nl.measures[2].func,
        MeasFunc::Stat {
            stat: MeasStat::Max,
            ..
        }
    ));
}

#[test]
fn malformed_measure_cards_are_errors_with_spans() {
    let deck = "meas\nr1 1 0 1k\n.op\n.meas tran x max v(1) bogus=1\n.meas op y max v(1)\n.meas tran\n.end\n";
    let out = parse_str("test.cir", deck);
    let errs: Vec<String> = out
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&out.sources))
        .collect();
    assert_eq!(errs.len(), 3, "{errs:#?}");
    // Points at the bad word, not the whole card.
    assert!(errs[0].starts_with("test.cir:4:23:"), "{}", errs[0]);
    assert!(errs[0].contains("no such parameter"), "{}", errs[0]);
    assert!(
        errs[1].contains("unrecognized analysis type `op`"),
        "{}",
        errs[1]
    );
    assert!(errs[2].contains("needs a result name"), "{}", errs[2]);
    assert!(out.netlist.measures.is_empty());
}
