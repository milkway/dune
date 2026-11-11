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

// ---- pic figures (extract_pic / simulate_pic) ----

fn corpus_pic(name: &str) -> String {
    let path = format!("{}/../../corpus/pic/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

fn pic_fig(body: &str) -> String {
    format!(".PS\nspice = 1\n{body}\n.PE\n")
}

const DANGLING: &str = "V1: vsource((0,1),(0,0), 1)
R1: resistor((0,1),(1,1),1k)
ground((0,0))
.op";

#[test]
fn extract_pic_rc_shape() {
    let json = extract_pic_json(&corpus_pic("rc.pic"), None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["diagnostics", "map", "netlist"]);
    let pic: PicJson = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(pic.netlist, corpus_pic("golden/rc.cir"));
    assert!(pic.diagnostics.is_empty(), "{:?}", pic.diagnostics);
    let refdes: Vec<&str> = pic.map.elements.iter().map(|e| e.refdes.as_str()).collect();
    assert_eq!(refdes, ["V1", "R1", "C1"]);
    for e in &pic.map.elements {
        assert!(e.shapes.start < e.shapes.end, "{e:?}");
    }
    let nets: Vec<&str> = pic.map.nets.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(nets[0], "0");
    assert!(nets.contains(&"Vin") && nets.contains(&"Vout"), "{nets:?}");
    // Wire format: `shapes: {start, end}` on elements, a list on nets.
    assert!(v["map"]["elements"][0]["shapes"]["start"].is_u64());
    assert!(v["map"]["nets"][0]["shapes"].is_array());
    // Ground net draws its wire and ground symbol.
    assert!(!pic.map.nets[0].shapes.is_empty());
    // Write-back spans (issue #17) and the SVG frame (issue #19).
    let r1 = &v["map"]["elements"][1];
    assert_eq!(r1["value"]["text"], "1k");
    assert_eq!(
        r1["value"]["span"],
        serde_json::json!({"line": 6, "col": 25, "end_line": 6, "end_col": 27})
    );
    assert_eq!(r1["principal"], "resistance");
    assert_eq!(r1["params"][0]["name"], "resistance");
    let v1 = &v["map"]["elements"][0];
    assert_eq!(v1["params"][0]["name"], "pulse");
    assert_eq!(v1["params"][0]["text"], "PULSE(0 5 0 1u 1u 10m 20m)");
    let keys: Vec<&str> = v["map"]["canvas"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["h", "origin", "rescale", "w"]);
    assert_eq!(v["map"]["canvas"]["origin"].as_array().unwrap().len(), 2);
    // A pin through the documented formula lands on the SVG's numbers.
    let c = &pic.map.canvas;
    let p = &pic.map.elements[1].pins[0];
    let (sx, sy) = (
        (p.x + c.origin[0]) * 96.0,
        (c.h - (p.y + c.origin[1])) * 96.0,
    );
    let svg = render_pic_svg(&corpus_pic("rc.pic"), None).unwrap();
    let pt = format!("{},{}", fmt6(sx), fmt6(sy));
    assert!(svg.contains(&pt), "{pt}");
    // No value span: a `K`, a wrapper.
    let src = "spice = 1\nL1: inductor((0,0),(1,0),1m); L2: inductor((1,0),(2,0),1m)\n\
               K1: coupling(L1, L2, 0.5); ground((0,0)); ground((2,0))\n";
    let k: serde_json::Value = serde_json::from_str(&extract_pic_json(src, None).unwrap()).unwrap();
    assert!(k["map"]["elements"][2]["value"].is_null());
    assert!(k["map"]["elements"][2]["principal"].is_null());
    assert_eq!(k["map"]["elements"][2]["params"], serde_json::json!([]));
}

/// rpic's SVG number format (six decimals, no trailing zeros).
fn fmt6(x: f64) -> String {
    let mut s = format!("{:.6}", (x * 1e6).round() / 1e6);
    while s.contains('.') && (s.ends_with('0') || s.ends_with('.')) {
        s.pop();
    }
    s
}

/// rc.pic end to end: extract, run the `.tran`, check the RC charge
/// (tau = 1 ms) against the closed form.
#[test]
fn simulate_pic_rc_tran() {
    let json = simulate_pic_json(&corpus_pic("rc.pic"), None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    for k in ["plots", "notes", "diagnostics", "pic"] {
        assert!(v.get(k).is_some(), "missing {k}");
    }
    let out: PicRunJson = serde_json::from_value(v).unwrap();
    assert_eq!(out.pic.netlist, corpus_pic("golden/rc.cir"));
    let p = &out.raw.plots[0];
    assert_eq!(p.plotname, "Transient Analysis");
    let Columns::Real(cols) = &p.columns else {
        panic!("tran is real")
    };
    let iv = p
        .variables
        .iter()
        .position(|x| x.name.eq_ignore_ascii_case("v(vout)"))
        .unwrap_or_else(|| panic!("v(vout) in {:?}", p.variables));
    let t = &cols[0];
    let vout = &cols[iv];
    let last = p.npoints - 1;
    assert!((t[last].unwrap() - 5e-3).abs() < 1e-12);
    for k in 0..p.npoints {
        let (tk, vk) = (t[k].unwrap(), vout[k].unwrap());
        assert!((-1e-6..=5.0 + 1e-6).contains(&vk), "v(vout)={vk} at t={tk}");
        // Ideal step response, ignoring the 1 µs rise: tolerance
        // covers the rise and the tran's LTE.
        if tk > 1e-4 {
            let ideal = 5.0 * (1.0 - (-(tk - 0.5e-6) / 1e-3).exp());
            assert!((vk - ideal).abs() < 0.02, "t={tk}: {vk} vs {ideal}");
        }
    }
    assert!((vout[last].unwrap() - 4.966).abs() < 0.01);
}

#[test]
fn every_corpus_figure_simulates() {
    for name in ["rc.pic", "rlc.pic", "rect.pic", "bjtamp.pic"] {
        let out = run_pic(&corpus_pic(name), &PicSimOptions::default())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!out.raw.plots.is_empty(), "{name}");
        let golden = corpus_pic(&format!("golden/{}", name.replace(".pic", ".cir")));
        assert_eq!(out.pic.netlist, golden, "{name}");
    }
}

#[test]
fn unmarked_figure_is_a_pic_error() {
    let src = ".PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n";
    for err in [
        extract_pic_json(src, None).unwrap_err(),
        simulate_pic_json(src, None).unwrap_err(),
    ] {
        assert_eq!(err.kind, "pic", "{err}");
        assert!(err.message.contains("spice = 1"), "{err}");
        assert_eq!(err.diagnostics.len(), 1);
        let d = &err.diagnostics[0];
        assert_eq!(d.kind.as_deref(), Some("not-marked"));
        assert_eq!(d.severity, "error");
        assert_eq!(d.line, None);
        assert!(err.pic.is_none());
    }
}

#[test]
fn compile_error_is_a_pic_error_with_position() {
    let err = extract_pic_json("spice = 1\n\nline from (0,0) to\n", None).unwrap_err();
    assert_eq!(err.kind, "pic", "{err}");
    let d = &err.diagnostics[0];
    assert_eq!(d.kind.as_deref(), Some("compile"));
    assert_eq!((d.file.as_deref(), d.line), (None, Some(3)), "{d:?}");
    assert!(err.message.starts_with("3:"), "{err}");
}

#[test]
fn dangling_pin_is_a_warning_in_the_result() {
    let src = pic_fig(DANGLING);
    let pic: PicJson = serde_json::from_str(&extract_pic_json(&src, None).unwrap()).unwrap();
    let d = pic
        .diagnostics
        .iter()
        .find(|d| d.kind.as_deref() == Some("dangling-pin"))
        .unwrap_or_else(|| panic!("{:?}", pic.diagnostics));
    assert_eq!(d.severity, "warning");
    assert!(d.message.contains("R1.N"), "{}", d.message);
    // R1's call site in the figure: line 4 (after `.PS`, `spice = 1`,
    // V1), column 5.
    assert_eq!(
        (d.file.as_deref(), d.line, d.column),
        (None, Some(4), Some(5))
    );
    assert!(d.end_column.unwrap() > 5);
    // Warnings do not stop simulate_pic; they ride along under `pic`.
    let out = run_pic(&src, &PicSimOptions::default()).unwrap();
    assert_eq!(out.raw.plots[0].plotname, "Operating Point");
    assert!(
        out.pic
            .diagnostics
            .iter()
            .any(|d| d.kind.as_deref() == Some("dangling-pin"))
    );
}

#[test]
fn extraction_errors_stop_simulate_pic() {
    // R1 lacks its value: an error diagnostic, still a netlist.
    let src = pic_fig("V1: vsource((0,1),(0,0), 1)\nR1: resistor((0,1),(0,0))\nground((0,0))\n.op");
    let pic: PicJson = serde_json::from_str(&extract_pic_json(&src, None).unwrap()).unwrap();
    assert!(pic.diagnostics.iter().any(|d| d.severity == "error"));
    let err = run_pic(&src, &PicSimOptions::default()).unwrap_err();
    assert_eq!(err.kind, "pic", "{err}");
    assert!(
        err.diagnostics
            .iter()
            .any(|d| d.kind.as_deref() == Some("missing-value"))
    );
    assert_eq!(err.pic.as_ref().unwrap().netlist, pic.netlist);
}

#[test]
fn deck_failures_after_extraction_carry_the_extraction() {
    // A `.dc` over a source the figure does not have.
    let src = pic_fig(&DANGLING.replace(".op", ".dc v9 0 1 0.1"));
    let err = run_pic(&src, &PicSimOptions::default()).unwrap_err();
    assert!(matches!(err.kind.as_str(), "analysis" | "parse"), "{err}");
    assert!(err.pic.as_ref().unwrap().netlist.contains("R1 "));
}

#[test]
fn pic_options() {
    let src = pic_fig(DANGLING);
    assert!(extract_pic_json(&src, Some(r#"{"snap":0.01}"#)).is_ok());
    assert!(simulate_pic_json(&src, Some(r#"{"snap":0.01,"includes":{}}"#)).is_ok());
    for opts in [
        "[]",
        r#"{"snap":0}"#,
        r#"{"snap":-1}"#,
        r#"{"snap":"x"}"#,
        r#"{"includes":{}}"#, // extract_pic takes no includes
    ] {
        let err = extract_pic_json(&src, Some(opts)).unwrap_err();
        assert_eq!(err.kind, "options", "{opts}: {err}");
    }
    let err = simulate_pic_json(&src, Some(r#"{"snapp":1}"#)).unwrap_err();
    assert_eq!(err.kind, "options");
    // Includes reach the extracted deck.
    let src = pic_fig(
        "V1: vsource((0,1),(0,0), 1)\nD1: diode((0,1),(0,0), dmod)\nground((0,0))\n.include m.lib\n.op",
    );
    assert_eq!(simulate_pic_json(&src, None).unwrap_err().kind, "parse");
    let opts = r#"{"includes":{"m.lib":".model dmod d is=1e-14\n"}}"#;
    assert!(simulate_pic_json(&src, Some(opts)).is_ok());
}

/// The netlist-only output keeps its exact keys: pic additions to the
/// shared types serialize only when set.
#[test]
fn simulate_wire_format_unchanged() {
    let err = simulate_json("t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n", None).unwrap_err();
    let d = serde_json::to_value(&err.diagnostics[0]).unwrap();
    let keys: Vec<&str> = d.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["column", "file", "line", "message", "severity"]);
    assert!(serde_json::to_value(&err).unwrap().get("pic").is_none());
}

#[test]
fn hostile_figures_do_not_panic() {
    let figures: &[&str] = &[
        "",
        "spice = 1",
        ".PS\nspice = 1\n.PE\n",
        ".PS\nspice = 1\n[\n.PE\n",
        ".PS\nspice = 1\nR1: resistor((0,0),(0,0),1k)\n.PE\n",
        ".PS\nspice = 1\nR1: resistor((0,0),(1e308,1e308),1k)\n.PE\n",
        ".PS\nspice = 1\nX1: opamp((0,0))\n.PE\n",
        ".PS\nspice = 1\nR1: resistor((0,0),(1,0),1k)\nR1: resistor((0,0),(1,0),1k)\n.PE\n",
        ".PS\nspice = 1\ncopy \"/etc/passwd\"\n.PE\n",
        ".PS\nspice = 1\n.tran 0 0\n+ x\n.PE\n",
        ".PS\nspice = 1\nfor i = 1 to 1e9 do { }\n.PE\n",
        "\u{0}\u{feff}\u{1F600}",
    ];
    for f in figures {
        let r = simulate_pic_json(f, None);
        if let Err(e) = &r {
            assert_ne!(e.kind, "internal", "{f:?}: {e}");
        }
    }
    let text = corpus_pic("bjtamp.pic");
    for cut in (0..text.len()).step_by(11) {
        if text.is_char_boundary(cut) {
            let r = extract_pic_json(&text[..cut], None);
            if let Err(e) = &r {
                assert_ne!(e.kind, "internal", "cut {cut}: {e}");
            }
        }
    }
}

#[test]
fn render_pic_marked_figure_has_the_map_ids() {
    let src = corpus_pic("rc.pic");
    let svg = render_pic_svg(&src, None).unwrap();
    assert!(svg.starts_with("<svg") || svg.contains("<svg"), "{svg:.80}");
    let pic = extract(&src, None).unwrap();
    for e in &pic.map.elements {
        for n in e.shapes.start..e.shapes.end {
            assert!(svg.contains(&format!("id=\"s{n}\"")), "{} s{n}", e.refdes);
        }
    }
    for n in pic.map.nets.iter().flat_map(|n| &n.shapes) {
        assert!(svg.contains(&format!("id=\"s{n}\"")), "net s{n}");
    }
    // Same as dune_pic's own render (no extra wrapping here).
    assert_eq!(
        svg,
        dune_pic::render_svg(&src, &ExtractOptions::default()).unwrap()
    );
}

#[test]
fn render_pic_does_not_need_spice_1() {
    let svg = render_pic_svg(".PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n", None).unwrap();
    assert!(svg.contains("<svg"), "{svg:.80}");
    assert!(render_pic_svg("box \"plain pic\"\n", Some("{}")).is_ok());
}

#[test]
fn render_pic_errors() {
    let err = render_pic_svg("spice = 1\n\nline from (0,0) to\n", None).unwrap_err();
    assert_eq!(err.kind, "pic", "{err}");
    let d = &err.diagnostics[0];
    assert_eq!(d.kind.as_deref(), Some("compile"));
    assert_eq!((d.file.as_deref(), d.line), (None, Some(3)), "{d:?}");
    for bad in [r#"{"snap":1}"#, "[]", "nope"] {
        let err = render_pic_svg(".PS\n.PE\n", Some(bad)).unwrap_err();
        assert_eq!(err.kind, "options", "{bad}: {err}");
    }
}

#[test]
fn hostile_figures_do_not_panic_render() {
    for f in [
        "",
        "[\n",
        "\u{0}\u{feff}\u{1F600}",
        "for i = 1 to 1e9 do { }\n",
        "copy \"/etc/passwd\"\n",
    ] {
        if let Err(e) = render_pic_svg(f, None) {
            assert_ne!(e.kind, "internal", "{f:?}: {e}");
        }
    }
}

/// The typed variants' data: real columns verbatim, complex ones
/// interleaved, the JSON header with empty `columns`.
#[test]
fn typed_buffers_match_json_columns() {
    let deck = "rc\nv1 1 0 dc 1 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.op\n.ac dec 2 10 1k\n.end\n";
    let data = run_data(deck, &SimOptions::default()).unwrap();
    let json = data.to_json();
    assert_eq!(json, run(deck, &SimOptions::default()).unwrap());
    let header = data.header_json();
    assert_eq!(header.plots.len(), 2);
    for ((p, pj), ph) in data.plots.iter().zip(&json.plots).zip(&header.plots) {
        assert_eq!(ph.columns, Columns::Real(Vec::new()));
        assert_eq!(
            PlotJson {
                columns: pj.columns.clone(),
                ..ph.clone()
            },
            *pj
        );
        let bufs = column_buffers(p);
        assert_eq!(bufs.len(), pj.variables.len());
        match &pj.columns {
            Columns::Real(cols) => {
                for (b, c) in bufs.iter().zip(cols) {
                    assert_eq!(b.len(), pj.npoints);
                    assert!(b.iter().zip(c).all(|(x, y)| Some(*x) == *y));
                }
            }
            Columns::Complex(cols) => {
                for (b, c) in bufs.iter().zip(cols) {
                    assert_eq!(b.len(), 2 * pj.npoints);
                    for (k, [re, im]) in c.iter().enumerate() {
                        assert_eq!((Some(b[2 * k]), Some(b[2 * k + 1])), (*re, *im));
                    }
                }
            }
        }
    }
    // The header is valid wire JSON with `"columns":[]`.
    let text = serde_json::to_string(&header).unwrap();
    assert!(text.contains("\"columns\":[]"), "{text}");
}

#[test]
fn typed_pic_run_matches_json_run() {
    let src = corpus_pic("rc.pic");
    let opts = PicSimOptions::default();
    let (data, pic) = run_pic_data(&src, &opts).unwrap();
    let full = run_pic(&src, &opts).unwrap();
    assert_eq!(pic, full.pic);
    assert_eq!(data.to_json(), full.raw);
    // Failures carry the extraction just like run_pic's.
    let err = run_pic_data(
        &pic_fig("V1: vsource((0,1),(0,0), 1)\nR1: resistor((0,1),(0,0))\nground((0,0))\n.op"),
        &opts,
    )
    .unwrap_err();
    assert_eq!(err.kind, "pic");
    assert!(err.pic.is_some());
}

// ---- Session (issue #12) -------------------------------------------

use dune_core::AlterValue;

const RC_AMP: &str = "session deck
vin 1 0 dc 1 ac 1 pulse(0 1 1u 1u 1u 4u 10u)
r1 1 2 1k
c1 2 0 1n
d1 2 0 dm
.model dm d is=1e-14
.options reltol=1e-4
.temp 27
.op
.ac dec 5 1k 1meg
.tran 0.2u 10u
.end
";

/// The session's result is `simulate`'s for the altered deck: the
/// same JSON, notes and warnings included.
#[test]
fn session_run_is_the_fresh_run_of_the_altered_deck() {
    let mut s = SimSession::open(RC_AMP, &SimOptions::default()).unwrap();
    assert_eq!(
        s.run().unwrap().to_json(),
        run(RC_AMP, &SimOptions::default()).unwrap()
    );
    // `.temp` is obsolete: the warning rides along on every run.
    assert!(!s.run().unwrap().diagnostics.is_empty());

    s.alter("R1", "", AlterValue::Num(2048.0)).unwrap();
    s.alter(
        "vin",
        "pulse",
        AlterValue::List(vec![0.0, 2.0, 1e-6, 1e-6, 1e-6, 4e-6, 10.0 * 1e-6]),
    )
    .unwrap();
    s.set_temp(50.0).unwrap();
    let altered = RC_AMP
        .replace("r1 1 2 1k", "r1 1 2 2048")
        .replace("pulse(0 1 1u", "pulse(0 2 1u")
        .replace(".options reltol=1e-4", ".options reltol=1e-4 temp=50");
    // The card's doubles: `INPevaluate` reads `10u` as 10 * 1e-6
    // (not 1e-5; two roundings), `1u` as 1e-6.
    let got = s.run().unwrap().to_json();
    let want = run(&altered, &SimOptions::default()).unwrap();
    assert_eq!(got, want);
}

#[test]
fn session_errors_and_reads() {
    let mut s = SimSession::open(RC_AMP, &SimOptions::default()).unwrap();
    let e = s.alter("r9", "", AlterValue::Num(1.0)).unwrap_err();
    assert_eq!(e.kind, "alter");
    let e = s.alter("r1", "", AlterValue::Num(0.0)).unwrap_err();
    assert_eq!(e.kind, "alter");
    assert_eq!(s.param("r1", "").unwrap(), Some(AlterValue::Num(1e3)));
    assert_eq!(s.alterable("c1").unwrap(), ["capacitance", "ic"].as_slice());
    assert!((s.temp() - 27.0).abs() < 1e-12);
    let e = SimSession::open("t\nr1 1 0\n.end\n", &SimOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.kind, "elaborate");
    let e = SimSession::open("t\nr1 1 0 1k\n.foo\n.end\n", &SimOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.kind, "parse");
}

#[test]
fn pic_session_alters_by_refdes() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/pic/rc.pic"
    ))
    .unwrap();
    let mut s = SimSession::open_pic(&src, &PicSimOptions::default()).unwrap();
    let (fresh, pic) = run_pic_data(&src, &PicSimOptions::default()).unwrap();
    assert_eq!(s.pic(), Some(&pic));
    assert_eq!(s.run().unwrap().to_json(), fresh.to_json());
    // The figure's refdes, as drawn.
    s.alter("R1", "resistance", AlterValue::Num(2e3)).unwrap();
    let altered = pic.netlist.replace("R1 Vin Vout 1k", "R1 Vin Vout 2k");
    assert_ne!(altered, pic.netlist, "{}", pic.netlist);
    assert_eq!(
        s.run().unwrap().to_json(),
        run(&altered, &SimOptions::default()).unwrap()
    );
    // A figure that does not extract is a pic error, as in simulate_pic.
    let e = SimSession::open_pic(
        ".PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n",
        &PicSimOptions::default(),
    )
    .err()
    .unwrap();
    assert_eq!(e.kind, "pic");
}

// ---- .measure results (issue #14) ----------------------------------

/// An RC step: v(out) = 1 − e^{−(t−1n)/τ}, so the 10–90 % rise time
/// is τ·ln 9 exactly (τ = 1 ns).
const RC_MEAS: &str = "rc measured
v1 in 0 pulse(0 1 1n 1p 1p 20n 40n)
r1 in out 1k
c1 out 0 1p
.tran 10p 15n
.meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1
.meas tran vmax max v(out)
.meas tran tw when v(out)=0.5
.meas tran never when v(out)=5
.meas ac nothing find v(out) at=1k
.end
";

fn meas<'a>(raw: &'a RawfileJson, name: &str) -> &'a MeasurementJson {
    raw.measurements
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("no measurement {name}"))
}

#[test]
fn measurements_are_flat_rows() {
    let raw = run(RC_MEAS, &SimOptions::default()).unwrap();
    // The `.meas ac` card has no ac plot to measure: no row.
    let names: Vec<&str> = raw.measurements.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["trise", "vmax", "tw", "never"]);
    let tau = 1e3 * 1e-12;
    let m = meas(&raw, "trise");
    assert_eq!((m.analysis.as_str(), m.plot, &m.error), ("tran", 0, &None));
    let v = m.value.unwrap();
    assert!((v / (tau * 9f64.ln()) - 1.0).abs() < 1e-3, "trise = {v}");
    let keys: Vec<&str> = m.details.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["trig", "targ"]);
    let (trig, targ) = (m.details[0].1.unwrap(), m.details[1].1.unwrap());
    assert_eq!(targ - trig, v);
    assert!(meas(&raw, "tw").details.is_empty());
    let never = meas(&raw, "never");
    assert_eq!(never.value, None);
    assert!(never.details.is_empty());
    assert_eq!(never.error.as_deref(), Some("out of interval (WHEN)"));
}

/// The wire shape: `value` and `details` always there (`null` / `{}`
/// on failure), `error` only on failure, `details` in ngspice order.
#[test]
fn measurement_wire_shape() {
    let text = simulate_json(RC_MEAS, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let rows = v["measurements"].as_array().unwrap();
    let keys = |r: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = r.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    assert_eq!(
        keys(&rows[0]),
        ["analysis", "details", "name", "plot", "value"]
    );
    assert_eq!(
        keys(&rows[3]),
        ["analysis", "details", "error", "name", "plot", "value"]
    );
    assert!(rows[3]["value"].is_null());
    assert_eq!(rows[3]["details"], serde_json::json!({}));
    // Key order of `details` survives on the wire (trig before targ).
    let i = text.find("\"trig\":").unwrap();
    let j = text.find("\"targ\":").unwrap();
    assert!(i < j, "{text}");
    // Round trip (values to 1 ulp-ish: serde_json's default float
    // parsing is not correctly rounded; JS's `JSON.parse` is).
    let back = parse(&text).measurements;
    let orig = run(RC_MEAS, &SimOptions::default()).unwrap().measurements;
    assert_eq!(back.len(), orig.len());
    for (a, b) in back.iter().zip(&orig) {
        assert_eq!(
            (&a.name, &a.error, a.details.len()),
            (&b.name, &b.error, b.details.len())
        );
        let close = |x: Option<f64>, y: Option<f64>| match (x, y) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-15 * y.abs(),
            (x, y) => x == y,
        };
        assert!(close(a.value, b.value), "{a:?} vs {b:?}");
    }
    // A deck without `.measure` cards: an empty array, never absent.
    let v: serde_json::Value =
        serde_json::from_str(&simulate_json(DIVIDER, None).unwrap()).unwrap();
    assert_eq!(v["measurements"], serde_json::json!([]));
}

/// The typed variants' header carries the same rows.
#[test]
fn typed_header_carries_measurements() {
    let data = run_data(RC_MEAS, &SimOptions::default()).unwrap();
    assert_eq!(data.header_json().measurements, data.to_json().measurements);
    assert_eq!(data.measurements.len(), 4);
    let text = serde_json::to_string(&data.header_json()).unwrap();
    assert!(
        text.contains("\"measurements\":[{\"name\":\"trise\""),
        "{text}"
    );
}

/// Every run of a session re-measures: R doubled → τ and trise doubled.
#[test]
fn session_run_remeasures() {
    let mut s = SimSession::open(RC_MEAS, &SimOptions::default()).unwrap();
    let first = s.run().unwrap();
    assert_eq!(
        first.to_json(),
        run(RC_MEAS, &SimOptions::default()).unwrap()
    );
    let t1 = meas(&first.to_json(), "trise").value.unwrap();
    s.alter("r1", "", AlterValue::Num(2e3)).unwrap();
    let t2 = meas(&s.run().unwrap().to_json(), "trise").value.unwrap();
    assert!((t2 / t1 - 2.0).abs() < 1e-3, "{t1} -> {t2}");
}

/// A figure's `.meas` line is measured; the rows sit at the top level
/// of the pic result, beside `pic`.
#[test]
fn pic_run_carries_measurements() {
    let src = corpus_pic("rc.pic").replace(
        ".tran 20u 5m\n",
        ".tran 20u 5m\n.meas tran trise trig v(vout) val=0.5 rise=1 targ v(vout) val=4.5 rise=1\n",
    );
    let text = simulate_pic_json(&src, None).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["pic"].is_object());
    let rows = v["measurements"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{text}");
    let t = rows[0]["value"].as_f64().unwrap();
    assert!((t / (1e-3 * 9f64.ln()) - 1.0).abs() < 1e-2, "trise = {t}");
}

// ---- engine overrides: `options` / `tmaxDivisor` (issue #16) ----

/// Every column of every plot, as bits.
fn run_bits(d: &RunData) -> Vec<Vec<u64>> {
    d.plots
        .iter()
        .flat_map(|p| {
            column_buffers(p)
                .into_iter()
                .map(|c| c.iter().map(|x| x.to_bits()).collect())
        })
        .collect()
}

/// The overrides on a figure are the deck-side edit, bit for bit:
/// `.options trtol=1` plus the `.tran` card's TMAX divided by 10.
#[test]
fn overrides_are_the_edited_deck_bit_for_bit() {
    let fine: PicSimOptions =
        serde_json::from_str(r#"{"options":{"trtol":1},"tmaxDivisor":10}"#).unwrap();
    for (fig, card, edited) in [
        // Default TMAX min(20u, 5m/50) = 20u -> 2u.
        (
            "rc.pic",
            ".tran 20u 5m",
            ".options trtol=1\n.tran 20u 5m 0 2u",
        ),
        // Default TMAX min(0.1m, 50m/50) = 0.1m -> 0.01m (a diode).
        (
            "rect.pic",
            ".tran 0.1m 50m",
            ".options trtol=1\n.tran 0.1m 50m 0 0.01m",
        ),
    ] {
        let src = corpus_pic(fig);
        let (api, pic) = run_pic_data(&src, &fine).unwrap();
        assert!(pic.netlist.contains(card), "{fig}: {}", pic.netlist);
        let deck = pic.netlist.replace(card, edited);
        let want = run_data(&deck, &SimOptions::default()).unwrap();
        assert_eq!(run_bits(&api), run_bits(&want), "{fig}");
        let plain = run_pic_data(&src, &PicSimOptions::default()).unwrap().0;
        assert!(
            run_bits(&api)[0].len() > run_bits(&plain)[0].len(),
            "{fig}: denser"
        );
        // Defaults unchanged: no overrides is the plain deck.
        assert_eq!(
            run_bits(&plain),
            run_bits(&run_data(&pic.netlist, &SimOptions::default()).unwrap())
        );
    }
}

#[test]
fn override_warnings_ride_in_diagnostics() {
    let opts = r#"{"options":{"trtol":"abc","bogus":1,"reltol":1e-4}}"#;
    let out: RawfileJson = parse(&simulate_json(DIVIDER, Some(opts)).unwrap());
    let msgs: Vec<&str> = out.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    assert!(msgs.iter().all(|m| m.starts_with("opts.options: ")));
    assert!(
        out.diagnostics
            .iter()
            .all(|d| d.severity == "warning" && d.line.is_none())
    );
    for bad in [
        r#"{"tmaxDivisor":0.5}"#,
        r#"{"tmaxDivisor":"x"}"#,
        r#"{"options":[1]}"#,
    ] {
        assert_eq!(
            simulate_json(DIVIDER, Some(bad)).unwrap_err().kind,
            "options",
            "{bad}"
        );
    }
    let e = simulate_pic_json(&corpus_pic("rc.pic"), Some(r#"{"tmaxDivisor":0}"#)).unwrap_err();
    assert_eq!(e.kind, "options");
}

/// The Studio's "fine steps" toggle: flipping the overrides on a live
/// session gives the fresh runs, both ways, without reopening.
#[test]
fn session_toggles_overrides_without_reopening() {
    let src = corpus_pic("rc.pic");
    let mut s = SimSession::open_pic(&src, &PicSimOptions::default()).unwrap();
    let plain = s.run().unwrap();
    let fine_opts: PicSimOptions =
        serde_json::from_str(r#"{"options":{"trtol":1},"tmaxDivisor":10}"#).unwrap();
    let fine = run_pic_data(&src, &fine_opts).unwrap().0;

    s.set_options(fine_opts.options.clone());
    s.set_tmax_divisor(Some(10.0)).unwrap();
    assert_eq!(run_bits(&s.run().unwrap()), run_bits(&fine));
    assert_eq!(s.tmax_divisor(), Some(10.0));
    assert_eq!(s.options(), &fine_opts.options);

    assert_eq!(s.set_tmax_divisor(Some(0.0)).unwrap_err().kind, "options");
    assert_eq!(
        s.tmax_divisor(),
        Some(10.0),
        "refused divisor changes nothing"
    );

    s.set_options(serde_json::Map::new());
    s.set_tmax_divisor(None).unwrap();
    assert_eq!(run_bits(&s.run().unwrap()), run_bits(&plain));

    // Opening with the overrides is the same as setting them.
    let mut opened = SimSession::open_pic(&src, &fine_opts).unwrap();
    assert_eq!(run_bits(&opened.run().unwrap()), run_bits(&fine));
}

#[test]
fn session_temp_survives_new_options() {
    let mut s = SimSession::open(RC_AMP, &SimOptions::default()).unwrap();
    s.set_temp(50.0).unwrap();
    let mut o = serde_json::Map::new();
    o.insert("trtol".into(), 2.into());
    o.insert("x".into(), serde_json::Value::Null);
    s.set_options(o);
    assert!((s.temp() - 50.0).abs() < 1e-12);
    let d = s.run().unwrap().diagnostics;
    assert_eq!(
        d.iter()
            .filter(|d| d.message.starts_with("opts.options"))
            .count(),
        1
    );
    s.set_options(serde_json::Map::new());
    let d = s.run().unwrap().diagnostics;
    assert_eq!(
        d.iter()
            .filter(|d| d.message.starts_with("opts.options"))
            .count(),
        0
    );
}

// ---- .pss (issue #13, D-0032) --------------------------------------

/// A `.pss` deck needs no API of its own here: `simulate` returns its
/// two plots on the normal path — the period real, like a `.tran`, the
/// harmonics as `[re, im]` pairs, like an `.ac` — and their values
/// are those of a direct `Circuit::pss` call.
#[test]
fn pss_plots_travel_like_tran_and_ac() {
    let deck = "rc pss\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.pss 1k 5m 2 128 10\n.end\n";
    let json = simulate_json(deck, None).unwrap();
    let raw = parse(&json);
    assert_eq!(raw.plots.len(), 2);
    let (t, f) = (&raw.plots[0], &raw.plots[1]);
    assert_eq!(t.plotname, dune_core::PSS_TIME_PLOT);
    assert_eq!(t.flags, "real");
    assert_eq!(t.variables[0].vtype, "time");
    assert_eq!(t.npoints, 129);
    assert_eq!(f.plotname, dune_core::PSS_FREQ_PLOT);
    assert_eq!(f.flags, "complex");
    assert_eq!(f.variables[0].vtype, "frequency");
    assert_eq!(f.npoints, 10);
    assert!(json.contains("[[[0.0,0.0],[1000.0,0.0]"), "frequency pairs");
    assert!(raw.notes.iter().any(|n| n.starts_with("pss: ")));

    // The engine's own call, as spice.pss / spice_pss make it.
    let parsed = dune_netlist::parse_str("deck.cir", deck);
    let (circuit, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    opts.apply(&parsed.netlist.options, &mut Vec::new());
    let pss = circuit
        .pss(
            &opts,
            &dune_core::PssParams {
                freq: Some(1e3),
                tstab: 5e-3,
                osc_node: Some("2".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let typed = run_data(deck, &SimOptions::default()).unwrap();
    for (got, want) in typed.plots.iter().zip([&pss.time, &pss.spectrum]) {
        assert_eq!(*got, want.to_plot("rc pss", ""));
    }
    let i = f.variables.iter().position(|v| v.name == "v(2)").unwrap();
    let Columns::Complex(cols) = &f.columns else {
        panic!("the spectrum is complex")
    };
    let [re, im] = cols[i][1];
    let h1 = pss.spectrum.column_complex("v(2)").unwrap()[1];
    assert_eq!((re, im), (Some(h1.re), Some(h1.im)));
}

/// A shooting that does not converge is kind `pss`, with the history
/// as structured fields (flat beside `kind` in the JSON, as on the JS
/// error): a driven circuit has no period iterate (`null`).
#[test]
fn pss_convergence_failure_is_kind_pss() {
    let deck = "rc pss\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.pss 1k 5m 2 128 10 1\n.end\n";
    let err = simulate_json(deck, None).unwrap_err();
    assert_eq!(err.kind, "pss");
    assert!(
        err.message.contains("did not converge in 1 shooting"),
        "{}",
        err.message
    );
    let h = *err.pss.clone().expect("the shooting history");
    assert_eq!(h.iterations, 1);
    assert_eq!(h.residuals.len(), 1);
    assert!(h.residuals[0].unwrap() > 1.0);
    assert_eq!(h.period, None);
    let v = serde_json::to_value(&err).unwrap();
    assert_eq!(v["iterations"], 1);
    assert_eq!(v["residuals"].as_array().unwrap().len(), 1);
    assert!(v["period"].is_null() && v.get("period").is_some());

    // The same failure from a Session.
    let mut s = SimSession::open(deck, &SimOptions::default()).unwrap();
    assert_eq!(s.run().unwrap_err(), err);

    // Every other engine failure keeps kind `analysis` and no history.
    let e = simulate_json("rc pss\nr1 1 0 1k\n.pss 1k 5m 1\n.end\n", None).unwrap_err();
    assert_eq!(e.kind, "analysis");
    assert!(e.pss.is_none());
    assert!(
        serde_json::to_value(&e)
            .unwrap()
            .get("iterations")
            .is_none()
    );
}

fn corpus_pss(name: &str) -> String {
    let path = format!("{}/../../corpus/pss/{name}.cir", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap()
}

/// An autonomous oscillator cut short reports its last period iterate.
#[test]
fn autonomous_pss_failure_carries_the_period() {
    let deck = corpus_pss("osc_vdp").replace(".pss 0 4m 1 256 10 uic", ".pss 0 4m 1 256 10 1 uic");
    let err = simulate_json(&deck, None).unwrap_err();
    assert_eq!(err.kind, "pss");
    let h = *err.pss.unwrap();
    assert_eq!((h.iterations, h.residuals.len()), (1, 1));
    let t = h.period.expect("an autonomous shooting's period");
    // The frequency counter's estimate: near 2π√(LC).
    let t_lc = 2.0 * std::f64::consts::PI * (1e-3f64 * 1e-6).sqrt();
    assert!((t / t_lc - 1.0).abs() < 0.05, "{t}");
}

/// The `.pss` entry of a run: the period, `t0` and the history of the
/// card's own engine call, pointing at its two plots.
#[test]
fn pss_entry_of_an_autonomous_oscillator() {
    let raw = run(&corpus_pss("osc_vdp"), &SimOptions::default()).unwrap();
    assert_eq!(raw.pss.len(), 1);
    let p = &raw.pss[0];
    assert_eq!(p.plot, 0);
    assert_eq!(raw.plots[p.plot].plotname, dune_core::PSS_TIME_PLOT);
    assert_eq!(raw.plots[p.plot + 1].plotname, dune_core::PSS_FREQ_PLOT);
    assert_eq!(p.periods.len(), p.residuals.len());
    assert!(p.residuals.len() >= 2);
    assert_eq!(*p.periods.last().unwrap(), p.period);
    assert!(p.residuals.last().unwrap().unwrap() <= 1.0);
    // The Newton iterate moved T (an autonomous circuit's unknown).
    assert_ne!(p.periods[0], p.period);
    // T = 199.36138 µs with the default TR-BDF2 shooting (D-0047;
    // 199.36609 µs with the trapezoidal rule, the 2048-point
    // shooting gives 199.35692 µs — docs/dune-core.md).
    assert!((p.period / 199.36138e-6 - 1.0).abs() < 1e-6, "{}", p.period);
    let Columns::Real(cols) = &raw.plots[0].columns else {
        panic!("the period is real")
    };
    assert_eq!(cols[0][0], Some(p.t0));
    // A deck without `.pss` has an empty list, serialized all the same.
    let json = simulate_json("t\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n", None).unwrap();
    assert!(json.contains("\"pss\":[]"), "{json}");
}

/// `.pss` cards go through the engine's one card path
/// (`Circuit::run_card`): every `.pss` deck of the corpus gives the
/// plots and notes of `Circuit::run`, one `pss` entry per card with
/// `run_card`'s period and `t0`, and a Session the same result as a
/// fresh run. (The card→`PssParams` mapping itself is pinned in
/// `dune-core`'s `tests/run.rs`.)
#[test]
fn pss_cards_run_as_the_engine_runs_them() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/pss");
    let mut decks: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "cir"))
        .collect();
    decks.sort();
    assert!(decks.len() >= 9);
    for path in decks {
        let text = std::fs::read_to_string(&path).unwrap();
        let data = run_data(&text, &SimOptions::default()).unwrap();
        let parsed = dune_netlist::parse_str("deck.cir", &text);
        let (circuit, notes) = Circuit::from_netlist(&parsed.netlist).unwrap();
        let mut opts = Options::default();
        opts.apply(&parsed.netlist.options, &mut Vec::new());
        let mut want_notes = notes;
        let mut want = Vec::new();
        let mut periods = Vec::new();
        for a in &parsed.netlist.analyses {
            let card = circuit.run_card(a, &opts).unwrap();
            if let dune_core::CardOutput::Pss(p) = &card {
                periods.push((p.period, p.t0));
            }
            let (sols, n) = card.into_plots();
            want.extend(sols.iter().map(|s| s.to_plot(&parsed.netlist.title, "")));
            want_notes.extend(n);
        }
        assert_eq!(data.plots, want, "{}", path.display());
        assert_eq!(data.notes, want_notes, "{}", path.display());
        assert!(!periods.is_empty(), "{}", path.display());
        let got: Vec<_> = data.pss.iter().map(|p| (p.period, p.t0)).collect();
        assert_eq!(got, periods, "{}", path.display());
        let mut s = SimSession::open(&text, &SimOptions::default()).unwrap();
        assert_eq!(
            s.run().unwrap().to_json(),
            data.to_json(),
            "{}",
            path.display()
        );
    }
}
