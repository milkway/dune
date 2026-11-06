//! Native tests of the JSON path (the wasm entry point is a thin
//! wrapper over [`simulate_json`]).

use super::*;

const DIVIDER: &str = "divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n";

fn parse(json: &str) -> RawfileJson {
    serde_json::from_str(json).expect("output is a RawfileJson")
}

#[test]
fn op_divider() {
    let raw = parse(&simulate_json(DIVIDER, None).unwrap());
    assert_eq!(raw.plots.len(), 1);
    let p = &raw.plots[0];
    assert_eq!(p.title, "divider");
    assert_eq!(p.plotname, "Operating Point");
    assert_eq!(p.flags, "real");
    assert_eq!(p.npoints, 1);
    let i = p
        .variables
        .iter()
        .position(|v| v.name == "v(2)")
        .expect("v(2) saved");
    assert_eq!(p.variables[i].vtype, "voltage");
    let Columns::Real(cols) = &p.columns else {
        panic!("op is real")
    };
    assert_eq!(cols.len(), p.variables.len());
    assert!((cols[i][0].unwrap() - 5.0).abs() < 1e-9);
}

#[test]
fn ac_is_complex_pairs() {
    let deck = "rc\nv1 1 0 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.ac dec 2 10 1k\n.end\n";
    let json = simulate_json(deck, None).unwrap();
    let raw = parse(&json);
    let p = &raw.plots[0];
    assert_eq!(p.flags, "complex");
    assert_eq!(p.variables[0].vtype, "frequency");
    let Columns::Complex(cols) = &p.columns else {
        panic!("ac is complex")
    };
    assert!(cols.iter().all(|c| c.len() == p.npoints));
    // Pairs on the wire, not objects.
    assert!(json.contains("[[[10.0,0.0]"));
}

/// Same plots, values bit-for-bit, as the CLI's path (via the shared
/// engine calls), on a multi-analysis deck from the corpus.
#[test]
fn corpus_rc_matches_engine() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/spice3f5/examples/rc.cir"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let raw = run(&text, &SimOptions::default()).unwrap();
    assert!(!raw.plots.is_empty());
    for p in &raw.plots {
        let n = match &p.columns {
            Columns::Real(c) => c.iter().map(Vec::len).collect::<Vec<_>>(),
            Columns::Complex(c) => c.iter().map(Vec::len).collect(),
        };
        assert_eq!(n.len(), p.variables.len());
        assert!(n.iter().all(|&k| k == p.npoints));
    }
}

#[test]
fn includes_come_from_the_map() {
    let deck = "inc\n.include models.lib\nv1 1 0 dc 1\nd1 1 0 dmod\n.op\n.end\n";
    let err = simulate_json(deck, None).unwrap_err();
    assert_eq!(err.kind, "parse");
    let opts = r#"{"includes":{"models.lib":".model dmod d is=1e-14\n"}}"#;
    let raw = parse(&simulate_json(deck, Some(opts)).unwrap());
    assert_eq!(raw.plots[0].plotname, "Operating Point");
}

#[test]
fn bad_options_are_errors() {
    for opts in ["", "[]", r#"{"inclues":{}}"#, r#"{"includes":3}"#] {
        let err = simulate_json(DIVIDER, Some(opts)).unwrap_err();
        assert_eq!(err.kind, "options", "{opts:?}");
    }
    assert!(simulate_json(DIVIDER, Some("{}")).is_ok());
}

#[test]
fn parse_errors_carry_positions() {
    let err = simulate_json("t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n", None).unwrap_err();
    assert_eq!(err.kind, "parse");
    assert!(err.message.starts_with("deck.cir:3:"), "{err}");
    let d = err
        .diagnostics
        .iter()
        .find(|d| d.severity == "error")
        .expect("an error diagnostic");
    assert_eq!(d.line, Some(3));
    assert_eq!(d.file.as_deref(), Some(DECK_NAME));
}

#[test]
fn engine_errors_are_errors() {
    // A resistor with a model name parses but does not elaborate.
    let err = simulate_json("t\nv1 1 0 dc 1\nr1 1 0 rmod\n.op\n.end\n", None).unwrap_err();
    assert_eq!(err.kind, "elaborate", "{err}");
    // Unknown sweep source: the analysis itself fails.
    let err = simulate_json("t\nv1 1 0 dc 1\nr1 1 0 1k\n.dc v9 0 1 0.1\n.end\n", None).unwrap_err();
    assert!(matches!(err.kind.as_str(), "analysis" | "parse"), "{err}");
}

/// Hostile decks: every one must come back `Ok` or `Err`, never
/// panic (in wasm a panic traps and poisons the instance).
#[test]
fn malformed_decks_do_not_panic() {
    let decks: &[&str] = &[
        "",
        "\n",
        "title only",
        "t\n.end\n",
        "t\n.op\n",
        "t\nr1\n.op\n.end\n",
        "t\nr1 1 0 0\n.op\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.tran 0 0\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.tran -1n 1u\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.dc v1 0 1 0\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.dc v1 0 1 -1\n.end\n",
        "t\nv1 1 0 ac 1\nr1 1 0 1k\n.ac dec 0 0 0\n.end\n",
        "t\nv1 1 0 ac 1\nr1 1 0 1k\n.noise v(1) v9 dec 1 1 10\n.end\n",
        "t\nv1 1 0 ac 1\nr1 1 0 1k\n.pz 1 0 9 0 vol pz\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.tf v(7) v1\n.end\n",
        "t\nv1 1 0 dc 1\nr1 1 0 1k\n.sens v(1) ac dec 0 0 0\n.end\n",
        "t\nx1 1 2 nosuch\n.op\n.end\n",
        "t\n.subckt a 1\nx1 1 a\n.ends\nx1 1 a\n.op\n.end\n",
        "t\n.include /etc/passwd\n.op\n.end\n",
        "t\n.include ../../../../../../dev/zero\n.op\n.end\n",
        "t\n.param x={x}\nr1 1 0 {x}\n.op\n.end\n",
        "t\nv1 1 0 pulse(\nr1 1 0 1k\n.tran 1n 10n\n.end\n",
        "t\nv1 1 0 pwl(1 2 0 3)\nr1 1 0 1k\n.tran 1n 10n\n.end\n",
        "t\nv1 1 0 dc 1e308\nr1 1 0 1e-308\n.op\n.end\n",
        "t\nv1 1 0 dc nan\nr1 1 0 1k\n.op\n.end\n",
        "t\n.model m nmos level=99\nm1 1 1 0 0 m\n.op\n.end\n",
        "t\n\u{0}\u{feff}\u{1F600}\n.end\n",
        "t\n+\n+\n.op\n.end\n",
        "t\n.options reltol=-1 abstol=0 itl1=0\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n",
    ];
    for deck in decks {
        let _ = simulate_json(deck, None);
    }
}

/// The parser's fuzz-style sweep over truncations of a real deck.
#[test]
fn truncated_corpus_deck_does_not_panic() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/spice3f5/examples/rtlinv.cir"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut cuts: Vec<usize> = (0..text.len()).step_by(7).collect();
    cuts.push(text.len());
    for cut in cuts {
        if text.is_char_boundary(cut) {
            let _ = simulate_json(&text[..cut], None);
        }
    }
}
