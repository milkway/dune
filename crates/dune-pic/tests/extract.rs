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
Mid: node((1,1))");
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
A: node((0,1))
R1: resistor((3,1),(3,0),1k)
A: node((3,1))
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
In: node((0,1)); Top: node((0,1))
Zero: node((0,0)); ground((0,0))");
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
    find("node `Zero` is on the ground net");
    // The losing name is located at its call site (line 6, column 18).
    let sp = find("`In`, `Top`").span.clone().unwrap();
    assert_eq!((sp.line, sp.col), (6, 18));
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
    assert_eq!((sp.file.as_deref(), sp.line, sp.col), (None, 4, 1));
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
K1: resistor((0,1),(1,1),5)
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
    // W3 finds the second `R1:` — the duplicate's own call site.
    assert_eq!(dup.span.as_ref().unwrap().line, 5);
    assert_eq!(
        dup.to_string(),
        "5:1: error: refdes `R1` already used by `R1`; skipped"
    );
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
    let e = ex("Vdd: node((0,3))
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
fn opamp_pins_follow_subckt_order() {
    let e = ex("X1: opamp((2,1), OA)
In: node(X1.Inp); Fb: node(X1.Inm); Out: node(X1.Out)
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
    assert_eq!(cards(&e), ["R9 n1 0 2k"]);
}
