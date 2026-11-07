//! Extraction rules and diagnostics on small inline figures.

use dune_pic::{
    ExtractOptions, Extraction, PicDiagKind, PicError, PicSeverity, extract, render_svg,
};

fn ex(body: &str) -> Extraction {
    let src = format!(".PS\nspice = 1\n{body}\n.PE\n");
    extract(&src, &ExtractOptions::default()).unwrap_or_else(|e| panic!("{e}\n{src}"))
}

/// Element cards (no title, no directives, no `.end`).
fn cards(e: &Extraction) -> Vec<String> {
    e.netlist
        .lines()
        .skip(1)
        .filter(|l| !l.starts_with('.'))
        .map(str::to_string)
        .collect()
}

fn kinds(e: &Extraction) -> Vec<PicDiagKind> {
    e.diagnostics.iter().map(|d| d.kind).collect()
}

#[test]
fn refuses_figures_without_spice_marker() {
    let err = extract(
        ".PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n",
        &Default::default(),
    );
    assert_eq!(err.unwrap_err(), PicError::NotMarked);
    let err = extract(".PS\nspice = 0\n.PE\n", &Default::default());
    assert_eq!(err.unwrap_err(), PicError::NotMarked);
}

#[test]
fn compile_errors_point_into_the_user_source() {
    let err = extract("spice = 1\n\nline from (0,0) to\n", &Default::default()).unwrap_err();
    let PicError::Compile { span, .. } = err else {
        panic!("{err:?}")
    };
    let span = span.expect("span");
    assert_eq!(span.file, None);
    assert_eq!(span.line, 3, "{span:?}");
}

#[test]
fn divider_with_ground_and_names() {
    let e = ex("V1: vsource((0,1), (0,0), DC 10)
R1: resistor((0,1), (1,1), 1k)
R2: resistor((1,1), (1,0), 1k)
line from (0,0) to (1,0)
ground((0.5,0))
Mid: (1,1)");
    assert!(e.diagnostics.is_empty(), "{:?}", e.diagnostics);
    assert_eq!(cards(&e), ["V1 n1 0 DC 10", "R1 n1 Mid 1k", "R2 Mid 0 1k"]);
    assert!(e.netlist.ends_with(".end\n"));
}

#[test]
fn crossing_wires_need_a_dot_t_junctions_do_not() {
    // A horizontal wire R1-R2 and a vertical wire R3-R4 cross at (1,1).
    let crossing =
        "R1: resistor((-1,1),(0,1),1); line from (0,1) to (2,1); R2: resistor((2,1),(3,1),1)
R3: resistor((1,-1),(1,0),1); line from (1,0) to (1,2); R4: resistor((1,2),(1,3),1)";
    // Node `k` (1-based) of the card of `refdes`.
    let node = |e: &Extraction, refdes: &str, k: usize| {
        cards(e)
            .iter()
            .find(|c| c.starts_with(&format!("{refdes} ")))
            .unwrap()
            .split(' ')
            .nth(k)
            .unwrap()
            .to_string()
    };
    // No terminal at the crossing: two nets.
    let e = ex(crossing);
    assert_eq!(node(&e, "R1", 2), node(&e, "R2", 1));
    assert_eq!(node(&e, "R3", 2), node(&e, "R4", 1));
    assert_ne!(node(&e, "R1", 2), node(&e, "R3", 2), "{}", e.netlist);
    // A dot at the crossing is a terminal on both wires: one net.
    let e = ex(&format!("{crossing}\ndot at (1,1)"));
    assert_eq!(node(&e, "R1", 2), node(&e, "R3", 2), "{}", e.netlist);

    // T-junction: a wire ending on another wire's interior joins it.
    let e = ex("V1: vsource((0,1),(0,0), 1)
line from (0,1) to (2,1)
R1: resistor((1,1),(1,0),1k)
line from (0,0) to (1,0); ground((0.5,0))");
    assert_eq!(
        cards(&e),
        ["V1 n1 0 1", "R1 n1 0 1k"],
        "{:?}",
        e.diagnostics
    );
}

#[test]
fn polyline_vertices_are_terminals() {
    // R1.N touches a bend of the polyline, not an end.
    let e = ex("V1: vsource((0,1),(0,0), 1)
line from (0,1) to (1,1) then to (1,2) then to (3,2)
R1: resistor((1,2),(1,3),1k)
R2: resistor((3,2),(3,0),1k)
line from (0,0) to (3,0); ground((1,0))
line from R1.N to (0,3) then to (0,2)
R3: resistor((0,2),(-1,2),1)
line from R3.N to (-1,0) then to (0,0)");
    assert!(e.diagnostics.is_empty(), "{:?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[0], "V1 n1 0 1");
    assert_eq!(c[1], "R1 n1 n2 1k");
    assert_eq!(c[2], "R2 n1 0 1k");
    assert_eq!(c[3], "R3 n2 0 1");
}

#[test]
fn same_name_on_two_nets_is_one_node() {
    let e = ex("V1: vsource((0,1),(0,0), 1)
A: (0,1)
R1: resistor((3,1),(3,0),1k)
A: (3,1)
ground((0,0)); ground((3,0))");
    assert_eq!(cards(&e), ["V1 A 0 1", "R1 A 0 1k"]);
    assert!(e.diagnostics.is_empty(), "{:?}", e.diagnostics);
    let a = e.map.nets.iter().filter(|n| n.name == "A").count();
    assert_eq!(a, 1);
}

#[test]
fn node_name_conflicts() {
    let e = ex("V1: vsource((0,1),(0,0), 1)
R1: resistor((0,1),(1,0),1k)
line from (0,0) to (1,0)
In: (0,1); Top: (0,1)
Zero: (0,0); ground((0,0))");
    assert_eq!(cards(&e), ["V1 In 0 1", "R1 In 0 1k"]);
    let k = kinds(&e);
    assert_eq!(
        k,
        [PicDiagKind::NodeNameConflict, PicDiagKind::NodeNameConflict]
    );
    let find = |needle: &str| {
        e.diagnostics
            .iter()
            .find(|d| d.message.contains(needle))
            .unwrap_or_else(|| panic!("{needle}: {:?}", e.diagnostics))
    };
    find("place `Zero` is on the ground net");
    // The losing name is located at its place (line 6, column 12).
    let sp = find("`In`, `Top`").span.clone().unwrap();
    assert_eq!((sp.file, sp.line, sp.col, sp.end_col), (None, 6, 12, 15));
}

#[test]
fn dangling_pin_and_floating_wire() {
    let e = ex("V1: vsource((0,1),(0,0), 1)
R1: resistor((0,1),(1,1),1k)
ground((0,0))
line from (2,2) to (3,2)");
    let k = kinds(&e);
    assert!(k.contains(&PicDiagKind::DanglingPin), "{:?}", e.diagnostics);
    assert!(k.contains(&PicDiagKind::FloatingNet), "{:?}", e.diagnostics);
    let dangling = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::DanglingPin)
        .unwrap();
    assert!(dangling.message.contains("R1.N"), "{}", dangling.message);
    assert_eq!(dangling.severity, PicSeverity::Warning);
    let sp = dangling.span.as_ref().unwrap();
    // The element's call site (`resistor(…)` on R1's line).
    assert_eq!((sp.file.as_deref(), sp.line, sp.col), (None, 4, 5));
    let floating = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::FloatingNet)
        .unwrap();
    assert_eq!(floating.span.as_ref().unwrap().line, 6);
    assert!(e.is_clean()); // warnings only
}

#[test]
fn element_errors() {
    let e = ex("V1: vsource((0,1),(0,0))
R1: resistor((0,1),(1,1))
R1: resistor((0,1),(1,1),5)
Y1: resistor((0,1),(1,1),5)
Q1: resistor((0,1),(1,1),5)
resistor((0,1),(1,1),7k)
line from (1,1) to (0,0); ground((0,0))");
    let mut k = kinds(&e);
    k.sort_by_key(|k| format!("{k:?}"));
    assert_eq!(
        k,
        [
            PicDiagKind::DuplicateRefdes,
            PicDiagKind::MissingPin,
            PicDiagKind::MissingValue,
            PicDiagKind::MissingValue,
            PicDiagKind::UnknownElement,
            PicDiagKind::UnlabeledElement,
        ],
        "{:#?}",
        e.diagnostics
    );
    let sev = |kind, needle: &str| {
        e.diagnostics
            .iter()
            .find(|d| d.kind == kind && d.message.contains(needle))
            .unwrap_or_else(|| panic!("{kind:?} {needle}: {:#?}", e.diagnostics))
            .severity
    };
    assert_eq!(sev(PicDiagKind::MissingValue, "V1"), PicSeverity::Warning);
    assert_eq!(sev(PicDiagKind::MissingValue, "R1"), PicSeverity::Error);
    assert!(!e.is_clean());
    // Only V1 and R1 survive.
    assert_eq!(cards(&e), ["V1 n1 0", "R1 n1 0"]);
    let dup = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::DuplicateRefdes)
        .unwrap();
    // BlockInfo.span: the duplicate's own call site in the user's
    // figure — not the prelude `define` its shapes point into.
    let sp = dup.span.as_ref().unwrap();
    assert_eq!((sp.file.as_deref(), sp.line, sp.col), (None, 5, 5));
    assert_eq!(sp.end_col, 5 + "resistor((0,1),(1,1),5)".len() as u32);
    assert_eq!(
        dup.to_string(),
        "5:5: error: refdes `R1` already used by `R1`; skipped"
    );
    // Every element diagnostic is located in the user's source.
    for d in &e.diagnostics {
        let sp = d.span.as_ref().unwrap_or_else(|| panic!("{d}"));
        assert_eq!(sp.file, None, "{d}");
    }
}

#[test]
fn directives_pass_through_in_order() {
    let e = ex(".model DX D(IS=1e-14
+ RS=2)
V1: vsource((0,1),(0,0), DC 1)
D1: diode((0,1),(0,0), DX)
ground((0,0))
.op
.end");
    assert_eq!(
        e.netlist,
        "* netlist extracted by dune-pic\nV1 n1 0 DC 1\nD1 n1 0 DX\n\
         .model DX D(IS=1e-14\n+ RS=2)\n.op\n.end\n"
    );
    let parsed = dune_netlist::parse_str("d.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
}

#[test]
fn continuation_lines_fold_into_their_directive() {
    // rpic folds column-1 `+` lines into the passthrough entry above;
    // each is trimmed at both ends (CRLF too), and the lines after them keep
    // their numbers (the dangling R2 is reported on line 7: `.PS`,
    // `spice = 1`, then the body).
    let src = ".PS\r\nspice = 1\r\n.model DX D(IS=1e-14  \r\n+ RS=2 \r\n+   N=1.5)\r\n\
               V1: vsource((0,1),(0,0), DC 1); D1: diode((0,1),(0,0), DX)\r\n\
               R2: resistor((3,1),(3,0), 1k); ground((0,0)); ground((3,0))\r\n.op\r\n.PE\r\n";
    let e = extract(src, &ExtractOptions::default()).unwrap();
    assert_eq!(
        e.netlist,
        "* netlist extracted by dune-pic\nV1 n1 0 DC 1\nD1 n1 0 DX\nR2 n2 0 1k\n\
         .model DX D(IS=1e-14\n+ RS=2\n+   N=1.5)\n.op\n.end\n"
    );
    let d = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::DanglingPin)
        .unwrap_or_else(|| panic!("{:?}", e.diagnostics));
    assert_eq!(
        d.span.as_ref().map(|s| (s.file.clone(), s.line)),
        Some((None, 7))
    );
}

#[test]
fn snap_tolerance_is_an_option() {
    let src = ".PS\nspice = 1
V1: vsource((0,1),(0,0), 1)
R1: resistor((0.002,1),(0.002,0),1k)
ground((0,0)); line from (0,0) to (0.002,0)
.PE\n";
    let tight = extract(src, &ExtractOptions::default()).unwrap();
    assert!(cards(&tight)[1].starts_with("R1 n2 0"), "{}", tight.netlist);
    let loose = extract(
        src,
        &ExtractOptions {
            snap: 0.01,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cards(&loose), ["V1 n1 0 1", "R1 n1 0 1k"]);
}

/// Every D-0019 element, wired into one deck that `dune-netlist`
/// parses cleanly (op-amp aside: an X card needs a `.subckt` body,
/// which a figure cannot carry yet — see the pin-order test below).
#[test]
fn all_elements_extract_to_a_clean_deck() {
    let e = ex("Vdd: (0,3)
V1: vsource((0,3),(0,0), DC 5)
line from (0,3) to (8,3); line from (0,0) to (8,0); ground((4.5,0))
VC: battery((10,1.5),(10,0), 1)
R4: resistor((10,1.5),(11,1.5),1k); line from R4.N to (11,0)
ground((10,0)); ground((11,0))
R1: resistor((1,3),(1,2),1k)
C1: capacitor((1,2),(1,0),1n)
L1: inductor((2,3),(2,2),1u)
D1: diode((2,0),(2,2),DM)
Q1: npn((3,1.5), QM)
line from Q1.C to (3.16,3); R3: resistor(Q1.B, (2.52,0), 100k); line from Q1.E to (3.16,0)
M1: nmos((4,1.5), NM W=10u L=1u)
line from M1.D to (4.16,3); line from M1.G to (3.6,0); line from M1.S to (4.16,0)
M2: pmos((5,1.5), PM)
line from M2.S to (5.16,3); line from M2.G to (4.6,0); line from M2.D to (5.16,0)
J1: njfet((6,1.5), JM)
line from J1.D to (6.16,3); line from J1.G to (5.6,0); line from J1.S to (6.16,0)
S1: switch((7,3),(7,2), SM)
line from S1.Cp to (1,2); line from S1.Cn to (7.4,0)
R2: resistor((7,2),(7,0),1k)
W1: cswitch((8,3),(8,2), VC WM)
I1: isource((8,2),(8,0), 1m)
.model DM D
.model QM NPN
.model NM NMOS
.model PM PMOS
.model JM NJF
.model SM SW
.model WM CSW
.op");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    let card = |p: &str| c.iter().find(|l| l.starts_with(p)).unwrap().clone();
    let base = card("R3 ").split(' ').nth(1).unwrap().to_string();
    assert_eq!(card("Q1 "), format!("Q1 Vdd {base} 0 QM"));
    assert_eq!(card("M1 "), "M1 Vdd 0 0 0 NM W=10u L=1u");
    // pmos: source (and body) on top.
    assert_eq!(card("M2 "), "M2 0 0 Vdd Vdd PM");
    assert_eq!(card("J1 "), "J1 Vdd 0 0 JM");
    let c1p = card("C1 ").split(' ').nth(1).unwrap().to_string();
    assert!(
        card("S1 ").ends_with(&format!(" {c1p} 0 SM")),
        "{}",
        card("S1 ")
    );
    assert!(card("W1 ").ends_with(" VC WM"));
    let parsed = dune_netlist::parse_str("all.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}\n{}", parsed.diags, e.netlist);
    let (ckt, _) = dune_core::Circuit::from_netlist(&parsed.netlist).unwrap();
    let sols = ckt
        .run(&parsed.netlist.analyses[0], &dune_core::Options::default())
        .unwrap()
        .0;
    assert_eq!(sols[0].plotname, "Operating Point");
}

#[test]
fn transmission_lines_emit_four_node_cards() {
    // Left to right: returns below; top to bottom: returns to the
    // left (right of the drawing direction, i.e. -x when going down).
    let e = ex("A: (0,2); B: (2,2); C: (3,2); D: (3,0)
T1: tline(A, B, Z0=50 TD=1n F=1meg)
O1: ltraline(C, D, LMOD)
Ra: T1.N1; Rb: T1.N2; Rc: O1.N1; Rd: O1.N2
line from T1.N1 to (T1.N1.x,1); line from T1.N2 to (T1.N2.x,1)
line from O1.N1 to (2.5,O1.N1.y); line from O1.N2 to (2.5,O1.N2.y)
.model LMOD LTRA(R=0 L=250n C=100p LEN=1)
.tran 10p 5n");
    assert_eq!(
        cards(&e),
        ["T1 A Ra B Rb Z0=50 TD=1n F=1meg", "O1 C Rc D Rd LMOD"]
    );
    let parsed = dune_netlist::parse_str("tl.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}\n{}", parsed.diags, e.netlist);
}

#[test]
fn transmission_line_return_pins_sit_right_of_the_direction() {
    // Return pins 0.4 in to the right of P1 -> P2, 0.12 in inside the
    // ends of the 0.8 in body: a 2 in line from (0,0) to (2,0) has them
    // at x = 0.6 + 0.12 and 1.4 - 0.12, y = -0.4.
    let e = ex("T1: tline((0,0),(2,0),Z0=50 TD=1n)
Na: (0.72,-0.4); Nb: (1.28,-0.4)
line from (0,0) to (0,-1); line from (2,0) to (2,-1)
line from Na to (0.72,-1); line from Nb to (1.28,-1)");
    assert_eq!(cards(&e), ["T1 n1 Na n2 Nb Z0=50 TD=1n"]);
}

#[test]
fn opamp_pins_follow_subckt_order() {
    let e = ex("X1: opamp((2,1), OA)
In: X1.Inp; Fb: X1.Inm; Out: X1.Out
line from X1.Inp to (1,0.5); line from X1.Inm to (1,1.5); line from X1.Out to (3,1)");
    assert_eq!(cards(&e), ["X1 In Fb Out OA"]);
}

#[test]
fn overlay_map_points_at_drawn_shapes() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/pic/rc.pic"),
    )
    .unwrap();
    let e = extract(&src, &Default::default()).unwrap();
    let names: Vec<&str> = e.map.elements.iter().map(|x| x.refdes.as_str()).collect();
    assert_eq!(names, ["V1", "R1", "C1"]);
    let nets: Vec<&str> = e.map.nets.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(nets, ["0", "Vin", "Vout"]);
    // Element ranges are disjoint and ordered; ids render as sN..sM.
    let r = &e.map.elements;
    assert!(r[0].shapes.end <= r[1].shapes.start && r[1].shapes.end <= r[2].shapes.start);
    assert_eq!(
        r[0].ids(),
        format!("s{}..s{}", r[0].shapes.start, r[0].shapes.end - 1)
    );
    // The ground net holds the bottom wire and the ground symbol.
    assert!(e.map.nets[0].shapes.len() >= 2, "{:?}", e.map.nets[0]);
    // The ids exist in the rendered SVG.
    let svg = render_svg(&src, &Default::default()).unwrap();
    for net in &e.map.nets {
        for id in &net.shapes {
            assert!(svg.contains(&format!("id=\"s{id}\"")), "s{id} missing");
        }
    }
}

/// `(name, net, x, y)` of each pin of element `refdes`.
fn pins_of(e: &Extraction, refdes: &str) -> Vec<(String, String, f64, f64)> {
    let el = e.map.elements.iter().find(|x| x.refdes == refdes).unwrap();
    el.pins
        .iter()
        .map(|p| (p.name.clone(), p.net.clone(), p.x, p.y))
        .collect()
}

#[test]
fn overlay_pins_name_nets_without_wires() {
    // Elements snapped pin to pin: not one wire, so the nets between
    // them draw nothing — the pins are their probe targets.
    let e = ex("Vin: (0,1)
V1: vsource(Vin, (0,0), 1)
R1: resistor(Vin, (1.5,1), 1k)
R2: resistor((1.5,1), (1.5,0), 2k)
Q1: npn((2.5,1), QX)
ground((0,0)); ground((1.5,0))");
    assert_eq!(
        cards(&e),
        ["V1 Vin 0 1", "R1 Vin n1 1k", "R2 n1 0 2k", "Q1 n3 n2 n4 QX"]
    );
    let near = |got: &[(String, String, f64, f64)], want: &[(&str, &str, f64, f64)]| {
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, w) in got.iter().zip(want) {
            assert_eq!((g.0.as_str(), g.1.as_str()), (w.0, w.1), "{got:?}");
            assert!(
                (g.2 - w.2).abs() < 1e-9 && (g.3 - w.3).abs() < 1e-9,
                "{g:?} vs {w:?}"
            );
        }
    };
    near(
        &pins_of(&e, "V1"),
        &[("P", "Vin", 0.0, 1.0), ("N", "0", 0.0, 0.0)],
    );
    near(
        &pins_of(&e, "R1"),
        &[("P", "Vin", 0.0, 1.0), ("N", "n1", 1.5, 1.0)],
    );
    near(
        &pins_of(&e, "R2"),
        &[("P", "n1", 1.5, 1.0), ("N", "0", 1.5, 0.0)],
    );
    // `n1` has no shapes of its own; the two pins on it coincide.
    let n1 = e.map.nets.iter().find(|n| n.name == "n1").unwrap();
    assert!(n1.shapes.is_empty(), "{n1:?}");
    // A centered device's pins sit off its anchor, in card order.
    let q = pins_of(&e, "Q1");
    let names: Vec<(&str, &str)> = q.iter().map(|p| (p.0.as_str(), p.1.as_str())).collect();
    assert_eq!(names, [("C", "n3"), ("B", "n2"), ("E", "n4")]);
    assert!(q.iter().all(|p| (p.2, p.3) != (2.5, 1.0)), "{q:?}");
    // Every pin's net is a net of the map.
    for el in &e.map.elements {
        for p in &el.pins {
            assert!(e.map.nets.iter().any(|n| n.name == p.net), "{p:?}");
        }
    }
}

#[test]
fn copy_circuits_is_flagged() {
    let e = ex("copy \"circuits\"\nline from (0,0) to (1,0)");
    assert!(
        kinds(&e).contains(&PicDiagKind::Prelude),
        "{:?}",
        e.diagnostics
    );
}

#[test]
fn custom_prelude_replaces_the_embedded_one() {
    let prelude = "define res { [ __resistor($1,$2); \"$3\" at ($1+$2)/2; \
        P: circle invis rad 0 at $1; N: circle invis rad 0 at $2 ] with .P at $1 }\n\
        define ground { [ __ground($1); Gnd: circle invis rad 0 at $1 ] with .Gnd at $1 }\n";
    let src = ".PS\nspice = 1\nA: (0,0); B: (0,1)\nR9: res(B, A, 2k)\nground(A)\n.PE\n";
    let e = extract(
        src,
        &ExtractOptions {
            prelude: Some(prelude),
            ..Default::default()
        },
    )
    .unwrap();
    // `A` and `B` are places, so they name nets (A loses to ground).
    assert_eq!(cards(&e), ["R9 B 0 2k"]);
    assert_eq!(
        kinds(&e)[0],
        PicDiagKind::NodeNameConflict,
        "{:?}",
        e.diagnostics
    );
}

#[test]
fn values_pass_through_raw() {
    // pic would print these floats as 0.000000001 and 1500; the card
    // carries them as written.
    let e = ex(
        "V1: vsource((0,1),(0,0), PULSE(0 1e-3 0 1e-9 1e-9 5e-6 1e-5))
R1: resistor((0,1),(1,1), 1e-9)
C1: capacitor((1,1),(1,0), 1.5e3)
Q1: npn((3,1), \"QX\")
line from Q1.C to (1,1); line from Q1.B to (0,1) + (0,-0.5) then to (0,1)
line from Q1.E to (1,0)
line from (0,0) to (1,0); ground((0,0))",
    );
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    assert_eq!(
        cards(&e),
        [
            "V1 n1 0 PULSE(0 1e-3 0 1e-9 1e-9 5e-6 1e-5)",
            "R1 n1 n2 1e-9",
            "C1 n2 0 1.5e3",
            "Q1 n2 n1 0 QX",
        ]
    );
}

#[test]
fn value_falls_back_to_the_drawing() {
    // The outermost call is the wrapper `rr`, whose 3rd argument is not
    // R7's value: the drawn text decides. A call split over lines still
    // reads each argument raw (`MacroCall.raw_args`); only an argument
    // that is itself split keeps pic's normalized text.
    let e = ex("define rr { R7: resistor($2, $1, 10k) }
rr((0,0), (1,0), 5)
R8: resistor((1,0), \\
  (2,0), 2.5e3)
R9: resistor((2,0), (3,0), 1.5e3 \\
  )
V1: vsource((3,0), (4,0), PULSE(0 1e-3 \\
  0 1e-9))
ground((0,0)); ground((4,0))");
    assert_eq!(
        cards(&e),
        [
            "R7 n1 0 10k",
            "R8 n1 n2 2.5e3",
            "R9 n2 n3 1.5e3",
            "V1 n3 0 PULSE(0 0.001 0 0.000000001)"
        ]
    );
}

#[test]
fn places_name_nets_they_touch() {
    let e = ex("V1: vsource((0,1),(0,0), 1)
line from (0,1) to (2,1)
R1: resistor((2,1),(2,0),1k)
line from (0,0) to (2,0); ground((0,0))
Mid: (1,1)
Nowhere: (5,5)
R1: (2,1)");
    // `Mid` sits on the wire's interior; `R1` is an element's name.
    assert_eq!(cards(&e), ["V1 Mid 0 1", "R1 Mid 0 1k"]);
    let msg = |kind| {
        e.diagnostics
            .iter()
            .filter(|d| d.kind == kind)
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        msg(PicDiagKind::FloatingNet),
        ["8:1: warning: place `Nowhere` names no net: no wire or pin is there"]
    );
    assert_eq!(
        msg(PicDiagKind::NodeNameConflict),
        ["9:1: warning: place `R1` has the name of element `R1`; it names no net"]
    );
}

#[test]
fn directives_are_column_one_lines() {
    // An indented `.` line is pic, as in dpic: here, a syntax error.
    let err = extract(".PS\nspice = 1\n  .op\n.PE\n", &Default::default()).unwrap_err();
    assert!(matches!(err, PicError::Compile { .. }), "{err:?}");
}

/// Controlled sources (D-0026): E/G are four-point with the gain as
/// the 5th argument; F/H are two-point with `Vname gain` as the value.
#[test]
fn controlled_sources_emit_card_order() {
    let e = ex("V1: vsource((0,2),(0,0), DC 1)
R1: resistor((0,2),(1,2),1k); V0: vsource((1,2),(1,0), 0)
E1: vcvs((3,2),(3,0),(2.5,1.2),(2.5,0.8), 1e1)
line from (1,2) to (2.1,2) then to (2.1,1.2) then to E1.Cp
line from E1.Cn to (2.1,0.8) then to (2.1,0)
R2: resistor((3.8,2),(3.8,0),1k); line from E1.P to R2.P
G1: vccs((5,2),(5,0),(4.5,1.2),(4.5,0.8), \"2m\")
line from R2.P to (4.2,2) then to (4.2,1.2) then to G1.Cp
line from G1.Cn to (4.2,0.8) then to (4.2,0)
F1: cccs((6,2),(6,0), V0 3)
H1: ccvs((7,2),(7,0), V0 1e3)
R3: resistor((5.5,2),(5.5,0),1k); line from G1.P to R3.P then to F1.P then to H1.P
line from (0,0) to (7,0); ground((0,0))
.op");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    assert_eq!(
        cards(&e),
        [
            "V1 n1 0 DC 1",
            "R1 n1 n2 1k",
            "V0 n2 0 0",
            "E1 n3 0 n2 0 1e1",
            "R2 n3 0 1k",
            "G1 n4 0 n3 0 2m",
            "F1 n4 0 V0 3",
            "H1 n4 0 V0 1e3",
            "R3 n4 0 1k",
        ]
    );
    let parsed = dune_netlist::parse_str("dep.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
}

#[test]
fn controlled_source_references_are_checked() {
    let e = ex("V1: vsource((0,1),(0,0), 1); R1: resistor((0,1),(1,1),1k)
L1: inductor((1,1),(1,0),1u); line from (0,0) to (9,0); ground((0,0))
F1: cccs((2,1),(2,0), R1 2)
H1: ccvs((3,1),(3,0), V9 2)
F2: cccs((4,1),(4,0), V1 2)
E1: resistor((5,1),(5,0), 2)
K1: coupling(L1, R1, 0.5)
K2: resistor((6,1),(6,0), 0.5)
line from (2,1) to (4,1)");
    let msgs: Vec<String> = e
        .diagnostics
        .iter()
        .filter(|d| d.severity == PicSeverity::Error)
        .map(|d| format!("{:?} {}", d.kind, d.message))
        .collect();
    let has = |needle: &str| {
        assert!(
            msgs.iter().any(|m| m.contains(needle)),
            "{needle}: {msgs:#?}"
        )
    };
    has(
        "MissingValue `F1` needs a controlling voltage source (`Vname gain`): `R1` is not a `V` element",
    );
    has(
        "MissingValue `H1` needs a controlling voltage source (`Vname gain`): no element `V9` in the figure",
    );
    has("MissingValue `K1` needs two inductors (`L1, L2, k`): `R1` is not a `L` element");
    has("MissingPin `E1`: a `E` element needs pins P, N, Cp, Cn");
    has("MissingPin `K2`: a `K` element is drawn with `coupling(L1, L2, k)`");
    assert_eq!(msgs.len(), 5, "{msgs:#?}");
    // F2 is fine; the bad cards still go out (the netlist is wrong, as
    // for a missing value), the mis-drawn ones are skipped.
    let c = cards(&e);
    assert!(c.contains(&"F2 n3 0 V1 2".to_string()), "{c:?}");
    assert!(c.contains(&"K1 L1 R1 0.5".to_string()), "{c:?}");
    assert!(!c.iter().any(|l| l.starts_with("E1") || l.starts_with("K2")));
    // Located at the user's call.
    let f1 = e
        .diagnostics
        .iter()
        .find(|d| d.message.starts_with("`F1`"))
        .unwrap();
    let sp = f1.span.as_ref().unwrap();
    assert_eq!((sp.file.as_deref(), sp.line, sp.col), (None, 5, 5));
}

#[test]
fn coupling_emits_k_and_simulates() {
    // An ideal-ish transformer: k = 0.99, 1:4 inductance = 1:2 turns.
    let e = ex("V1: acsource((0,1),(0,0), AC 1)
R1: resistor((0,1),(1,1),1)
L1: inductor((1,1),(1,0),1m)
L2: inductor((2,1),(2,0),4m)
K1: coupling(L1, L2, 0.99)
R2: resistor((3,1),(3,0),1Meg); line from L2.P to R2.P
line from (0,0) to (3,0); ground((0,0))
Out: (3,1)
.ac lin 1 10k 10k");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    assert!(e.netlist.contains("\nK1 L1 L2 0.99\n"), "{}", e.netlist);
    // The coupling block is in the overlay map, like any element.
    assert!(e.map.elements.iter().any(|m| m.refdes == "K1"));
    let parsed = dune_netlist::parse_str("k.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = dune_core::Circuit::from_netlist(&parsed.netlist).unwrap();
    let s = &ckt
        .run(&parsed.netlist.analyses[0], &dune_core::Options::default())
        .unwrap()
        .0[0];
    // Open secondary: v2/v1 = k sqrt(L2/L1) = 1.98 (wL1 = 63 ohm >> R1).
    let v = s.column_complex("v(out)").unwrap()[0].norm();
    assert!((v - 1.98).abs() < 0.01, "|v(out)| = {v}");
}

#[test]
fn four_terminal_transistors() {
    let e = ex("Vdd: (0,3); Vsub: (6,-1)
V1: vsource((0,3),(0,0), DC 5)
line from (0,3) to (5,3); line from (0,0) to (5,0); ground((0,0))
V2: vsource((6,0),(6,-1), DC 2)
line from (5,0) to (6,0)
M1: nmos4((1,1.5), NM W=10u L=1u)
line from M1.D to (1.16,3); line from M1.G to (0.6,3); line from M1.S to (1.16,0)
line from M1.B to (1.6,-1) then to Vsub
M2: pmos4((2.5,1.5), PM)
line from M2.S to (2.66,3); line from M2.G to (2.1,0); line from M2.D to (2.66,0)
line from M2.B to (3.2,3)
R1: resistor((4,3),(4,2.5),10k)
Q1: npn4((4,1.5), QM)
line from R1.N to Q1.C; line from Q1.B to (3.52,3); line from Q1.E to (4.16,0)
line from Q1.S to (4.5,0)
.model NM NMOS
.model PM PMOS
.model QM NPN(CJS=1p)
.op");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    let card = |p: &str| c.iter().find(|l| l.starts_with(p)).unwrap().clone();
    // Bulk on its own net, not the source.
    assert_eq!(card("M1 "), "M1 Vdd Vdd 0 Vsub NM W=10u L=1u");
    // pmos4: source on top, bulk to Vdd.
    assert_eq!(card("M2 "), "M2 0 0 Vdd Vdd PM");
    // The substrate is the 4th node, before the model (INP2Q).
    let coll = card("R1 ").split(' ').nth(2).unwrap().to_string();
    assert_eq!(card("Q1 "), format!("Q1 {coll} Vdd 0 0 QM"));
    let parsed = dune_netlist::parse_str("m4.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}\n{}", parsed.diags, e.netlist);
    let (ckt, _) = dune_core::Circuit::from_netlist(&parsed.netlist).unwrap();
    let s = &ckt
        .run(&parsed.netlist.analyses[0], &dune_core::Options::default())
        .unwrap()
        .0[0];
    assert_eq!(s.plotname, "Operating Point");
    assert!((s.column("v(vsub)").unwrap()[0] + 2.0).abs() < 1e-12);
}

#[test]
fn substrate_named_like_the_model_is_flagged() {
    // `Q1 c b e QM QM` would read the substrate as the model.
    let e = ex("QM: (4.5,0.5)
Q1: npn4((4,1.5), QM)
line from Q1.S to (4.5,0.5); R1: resistor((4.5,0.5),(4.5,0),1); ground((4.5,0))
line from Q1.C to (4.16,2); R2: resistor((4.16,2),(4.16,2.5),1); ground((4.16,2.5))
line from Q1.B to (3.52,0); line from Q1.E to (4.16,0); ground((3.52,0)); ground((4.16,0))");
    let d = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::NodeNameConflict)
        .unwrap_or_else(|| panic!("{:#?}", e.diagnostics));
    assert_eq!(d.severity, PicSeverity::Warning);
    assert!(d.message.contains("substrate net `QM`"), "{}", d.message);
}

/// The text the figure draws (all `<text>` contents of the SVG).
fn drawn_texts(body: &str) -> Vec<String> {
    let src = format!(".PS\nspice = 1\n{body}\n.PE\n");
    let svg = render_svg(&src, &ExtractOptions::default()).unwrap();
    svg.split("<text")
        .skip(1)
        .filter_map(|t| {
            let t = &t[t.find('>')? + 1..];
            Some(t[..t.find('<')?].to_string())
        })
        .collect()
}

#[test]
fn display_text_is_drawn_and_the_value_emitted() {
    let body = "V1: vsource((0,1),(0,0), PULSE(0 5 1e-9 1n 1n 5n 10n), \"Vin\")
R1: resistor((0,1),(1,1), 1e3, \"Rload\")
C1: capacitor((1,1),(1,0), 1p, \"\")
D1: diode((1,1),(2,1), DX, \"D\"); I1: isource((2,1),(2,0), 1m, \"Ib\")
line from (0,0) to (2,0); ground((0,0))
.model DX D";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    assert_eq!(
        cards(&e),
        [
            "V1 n1 0 PULSE(0 5 1e-9 1n 1n 5n 10n)",
            "R1 n1 n2 1e3",
            "C1 n2 0 1p",
            "D1 n2 n3 DX",
            "I1 n3 0 1m",
        ]
    );
    // The drawing shows the displays (and the value when the display
    // is empty), never the long stimulus.
    let texts = drawn_texts(body);
    for t in ["Vin", "Rload", "1p", "D", "Ib"] {
        assert!(texts.iter().any(|x| x == t), "{t}: {texts:?}");
    }
    assert!(
        !texts
            .iter()
            .any(|x| x.contains("PULSE") || x == "1e3" || x == "1000")
    );
}

#[test]
fn display_on_four_point_sources_is_the_sixth_argument() {
    let body = "V1: vsource((0,2),(0,0), DC 1); line from (0,2) to (2.1,2) then to (2.1,1.2)
E1: vcvs((3,2),(3,0),(2.5,1.2),(2.5,0.8), 1e1, \"A\")
line from (2.1,1.2) to E1.Cp; line from E1.Cn to (2.1,0.8) then to (2.1,0)
G1: vccs((4,2),(4,0),(3.5,1.2),(3.5,0.8), 2m, \"gm\")
line from E1.P to (3.3,2) then to (3.3,1.2) then to G1.Cp
line from G1.Cn to (3.3,0.8) then to (3.3,0)
R1: resistor((5,2),(5,0), 1k); line from G1.P to R1.P
line from (0,0) to (5,0); ground((0,0))";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[1], "E1 n2 0 n1 0 1e1");
    assert_eq!(c[2], "G1 n3 0 n2 0 2m");
    let texts = drawn_texts(body);
    assert!(texts.iter().any(|x| x == "A") && texts.iter().any(|x| x == "gm"));
    assert!(!texts.iter().any(|x| x == "1e1" || x == "10" || x == "2m"));
}

#[test]
fn wrapper_arguments_never_pass_for_a_display() {
    // A wrapper with one more argument than the value slot: its 4th
    // argument matches the drawn text, but it is not a display (the
    // call is not the prelude's): the drawing decides, as before.
    let e = ex("define rr { R7: resistor($1, $2, $4) }
rr((0,0), (1,0), junk, 10k)
ground((0,0)); ground((1,0))");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    assert_eq!(cards(&e), ["R7 0 0 10k"]);

    // A wrapper that passes a display: the drawing is not the value
    // and the call is not the element's, so there is no value to
    // trust — an error, not the display (nor the wrapper's argument
    // that happens to equal it) on the card.
    for call in ["vv((0,1), (0,0), Vin)", "vv((0,1), (0,0), \"Vin\")"] {
        let e = ex(&format!(
            "define vv {{ V1: vsource($1, $2, DC 1, $3) }}
{call}
R1: resistor((0,1),(0,0),1k); ground((0,0))"
        ));
        let d = e
            .diagnostics
            .iter()
            .find(|d| d.kind == PicDiagKind::MissingValue)
            .unwrap_or_else(|| panic!("{call}: {:#?}", e.diagnostics));
        assert_eq!(d.severity, PicSeverity::Error);
        assert!(d.message.contains("display text"), "{}", d.message);
        assert_eq!(e.diagnostics.len(), 1, "{:#?}", e.diagnostics);
        assert_eq!(cards(&e)[0], "V1 n1 0", "{call}");
    }
}

#[test]
fn display_on_centered_devices() {
    let body = "Q1: npn((3,1), QX AREA=2, \"T₁\")
M1: nmos((5,1), NM W=1e-6 L=1u, \"T₂\")
J1: njfet((7,1), JM, \"T₃\")
line from Q1.C to (3.16,2); line from M1.D to (5.16,2); line from J1.D to (7.16,2)
line from Q1.B to (2,1) then to (2,2); line from (2,2) to (7.16,2)
line from M1.G to (4.5,0); line from J1.G to (6.5,0)
line from Q1.E to (3.16,0); line from M1.S to (5.16,0); line from J1.S to (7.16,0)
V1: vsource((8,2),(8,0), DC 1, \"V₁\")
line from (7.16,2) to (8,2); line from (2,0) to (8,0); ground((2,0))
.model QX NPN
.model NM NMOS
.model JM NJF
.op";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    // The model stays on the card, raw (`1e-6` would print as
    // 0.000001), even with a multibyte display after it on the line.
    assert_eq!(c[0], "Q1 n1 n1 0 QX AREA=2");
    assert_eq!(c[1], "M1 n1 0 0 0 NM W=1e-6 L=1u");
    assert_eq!(c[2], "J1 n1 0 0 JM");
    assert_eq!(c[3], "V1 n1 0 DC 1");
    let texts = drawn_texts(body);
    for t in ["T₁", "T₂", "T₃", "V₁"] {
        assert!(texts.iter().any(|x| x == t), "{t}: {texts:?}");
    }
    assert!(!texts.iter().any(|x| x.contains("QX") || x.contains("NM")));
    let parsed = dune_netlist::parse_str("c.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
}

#[test]
fn display_with_spaces_matches_after_normalization() {
    // pic collapses nothing inside a string, but the guard compares
    // word-normalized text: a display with runs of blanks still
    // matches what was drawn, and the value is the raw argument.
    let body = "V1: vsource((0,1),(0,0), SIN(0  1e-3   1k), \"G₁  (1 kHz)\")
M1: nmos((2,1), NM  W=2u, \" T  4 \")
line from M1.D to (2.16,1.5) then to (0,1.5) then to (0,1)
line from M1.G to (1.6,0); line from M1.S to (2.16,0); line from (0,0) to (2.16,0)
ground((0,0))
.model NM NMOS";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[0], "V1 n1 0 SIN(0  1e-3   1k)");
    assert_eq!(c[1], "M1 n1 0 0 0 NM  W=2u");
    let texts = drawn_texts(body);
    assert!(texts.iter().any(|x| x.contains("(1 kHz)")), "{texts:?}");
}

#[test]
fn display_through_a_wrapper_on_a_centered_device_is_refused() {
    let e = ex("define mytr { Q1: npn($1, QX, $2) }
mytr((3,1), \"T₁\")
line from Q1.C to (3.16,2); line from Q1.B to (2,1) then to (2,2) then to (3.16,2)
line from Q1.E to (3.16,0); ground((3.16,0))
V1: vsource((4,2),(4,0), 1); line from (3.16,2) to (4,2); line from (4,0) to (3.16,0)");
    let d = e
        .diagnostics
        .iter()
        .find(|d| d.kind == PicDiagKind::MissingValue)
        .unwrap_or_else(|| panic!("{:#?}", e.diagnostics));
    assert_eq!(d.severity, PicSeverity::Error);
    assert!(cards(&e)[0].starts_with("Q1 "));
    assert!(!cards(&e)[0].contains("T₁"));
}

#[test]
fn generic_subckt_pins_follow_card_order() {
    // subckt2 .. subckt7: N points, then the name; pins P1 .. PN.
    let e = ex("X1: subckt2((0,1), (1.2,1), TWOPORT)
X2: subckt5((2,1), (2.3,0.6), (2.6,0.6), (2.9,1), (2.45,1.4), FIVE)
X3: subckt7((4,1), (4,0.6), (4.4,0.6), (4.8,0.6), (5.2,0.6), (5.2,1), (4.6,1.4), SEVEN)
In: X1.P1
line from X1.P2 to X2.P1
V2: vsource((0,1), (0,0), 2); line from (0,0) to (5.2,0); ground((0,0))
line from X2.P2 to (2.3,0); line from X2.P3 to (2.6,0)
line from X2.P4 to X3.P1; line from X2.P5 to (2.45,1.8) then to (4.6,1.8) then to X3.P7
line from X3.P2 to (4,0); line from X3.P3 to (4.4,0); line from X3.P4 to (4.8,0)
line from X3.P5 to (5.2,0); line from X3.P6 to (5.6,1)
V1: vsource((5.6,1), (5.6,0), 1); line from (5.2,0) to (5.6,0)");
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[0], "X1 In n1 TWOPORT");
    assert_eq!(c[1], "X2 n1 0 0 n2 n3 FIVE");
    assert_eq!(c[2], "X3 n2 0 0 0 0 n4 n3 SEVEN");
    let names: Vec<String> = pins_of(&e, "X3").into_iter().map(|p| p.0).collect();
    assert_eq!(names, ["P1", "P2", "P3", "P4", "P5", "P6", "P7"]);
    // The pins sit on the user's points.
    let p = pins_of(&e, "X2");
    assert_eq!((p[4].2, p[4].3), (2.45, 1.4));
    assert!(e.map.elements[0].principal.is_none());
}

#[test]
fn generic_subckt_and_opamp_coexist() {
    let body = "X1: opamp((2,1), OA)
X2: subckt3((3,1.5), (3,0.5), (4,1), BUF, \"B₁\")
line from X1.Out to (2.6,1) then to (2.6,1.5) then to X2.P1
line from X1.Inp to (1,0.5); line from X1.Inm to (1,1.5)
line from X2.P2 to (3,0); line from X2.P3 to (4.5,1)
line from (1,0.5) to (1,0) then to (3,0); ground((1,0))
V1: vsource((1,1.5), (0.5,1.5), 1); R1: resistor((4.5,1), (4.5,0), 1k)
line from (0.5,1.5) to (0.5,0) then to (1,0); line from (3,0) to (4.5,0)";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[0], "X1 0 n1 n2 OA");
    // The display is drawn, the subcircuit name stays on the card.
    assert_eq!(c[1], "X2 n2 0 n3 BUF");
    let texts = drawn_texts(body);
    assert!(texts.iter().any(|t| t == "B₁"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "BUF"), "{texts:?}");
    // Pin numbers are drawn inside the box.
    for n in ["1", "2", "3"] {
        assert!(texts.iter().any(|t| t == n), "{n}: {texts:?}");
    }
    let names: Vec<String> = pins_of(&e, "X1").into_iter().map(|p| p.0).collect();
    assert_eq!(names, ["Inp", "Inm", "Out"]);
}

#[test]
fn subckt_box_spans_the_pins() {
    // Two pins one beside the other: the box grows to 0.6 x 0.4 about
    // their middle, the pins stay on its outline.
    let svg = render_svg(
        ".PS\nspice = 1\nX1: subckt2((0,0), (1,0), TWO)\n.PE\n",
        &ExtractOptions::default(),
    )
    .unwrap();
    assert!(svg.contains(">TWO<"), "{svg}");
    let e = ex("X1: subckt2((0,0), (0,0.1), TWO)");
    let p = pins_of(&e, "X1");
    assert_eq!((p[0].2, p[0].3, p[1].2, p[1].3), (0.0, 0.0, 0.0, 0.1));
}

#[test]
fn mesfet_is_a_z_card_with_jfet_pins() {
    let body = "Z1: nmes((2,1), ZN 2, \"T₁\")
Z2: pmes((4,1), ZP)
line from Z1.D to (2.16,2) then to (4.16,2); line from Z2.D to (4.16,2)
line from Z1.G to (1.6,0); line from Z2.G to (3.6,0)
line from Z1.S to (2.16,0); line from Z2.S to (4.16,0)
V1: vsource((5,2), (5,0), 3); line from (4.16,2) to (5,2)
line from (1.6,0) to (5,0); ground((1.6,0))
.model ZN NMF
.model ZP PMF";
    let e = ex(body);
    assert!(e.diagnostics.is_empty(), "{:#?}", e.diagnostics);
    let c = cards(&e);
    assert_eq!(c[0], "Z1 n1 0 0 ZN 2");
    assert_eq!(c[1], "Z2 n1 0 0 ZP");
    let names: Vec<String> = pins_of(&e, "Z1").into_iter().map(|p| p.0).collect();
    assert_eq!(names, ["D", "G", "S"]);
    assert_eq!(e.map.elements[0].principal.as_deref(), Some("area"));
    let texts = drawn_texts(body);
    assert!(texts.iter().any(|t| t == "T₁") && texts.iter().any(|t| t == "ZP"));
    let parsed = dune_netlist::parse_str("z.cir", &e.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
}
