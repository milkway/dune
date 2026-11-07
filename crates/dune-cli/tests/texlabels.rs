//! `$...$` figure labels (rpic `texlabels`, issue #7): the `dune`
//! binary registers rpic-render's RaTeX math renderer when built with
//! the `texlabels` feature (default), so `dune pic --svg` typesets
//! them; the pure rpic-core path (this test process, which registers
//! nothing — the same code the wasm build runs) keeps them literal.
//! Neither changes the netlist, and no corpus figure's SVG changes.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A one-transistor figure whose display labels are math.
const FIGURE: &str = r#".PS
spice = 1
texlabels = 1
B: (0.3,1.2)
Vcc: vsource((3.9,2.6), (3.9,0.3), 5, "$V_{CC}$")
Vb: vsource((0.3,1.2), (0.3,0.3), 0.8, "$G_1$")
Rb: resistor((0.3,1.2), (1.72,1.2), 10k)
Rc: resistor((2.36,2.6), (2.36,1.62), 1k)
Q1: npn((2.2,1.2), qn, "$T_1$")
line from Rc.P to Vcc.P
line from Q1.E to (Q1.E.x,0.3)
line from Vb.N to Vcc.N
dot at (Q1.E.x,0.3)
ground((1.2,0.3))
.model qn npn(bf=100)
.op
.PE
"#;

fn dune() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dune"))
}

/// `dune pic --svg` on `figure`: (SVG, netlist on stdout).
fn cli_svg(figure: &Path, dir: &Path) -> (String, String) {
    let svg = dir.join("out.svg");
    let out = dune()
        .arg("pic")
        .arg(figure)
        .arg("--svg")
        .arg(&svg)
        .output()
        .expect("run dune");
    assert!(
        out.status.success(),
        "dune pic --svg {}: {}",
        figure.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    (
        std::fs::read_to_string(&svg).expect("svg written"),
        String::from_utf8(out.stdout).expect("utf-8 netlist"),
    )
}

fn write_figure(dir: &Path) -> PathBuf {
    let fig = dir.join("mathlabels.pic");
    std::fs::write(&fig, FIGURE).unwrap();
    fig
}

/// The literal label text as the SVG escapes it (no `<>&` here).
const LITERALS: [&str; 3] = ["$T_1$", "$G_1$", "$V_{CC}$"];

#[test]
fn pure_rpic_core_keeps_math_labels_literal() {
    let svg = dune_pic::render_svg(FIGURE, &dune_pic::ExtractOptions::default()).unwrap();
    for lit in LITERALS {
        assert!(svg.contains(lit), "`{lit}` missing from the literal SVG");
    }
    // The only `<svg` is the root: no typeset fragment.
    assert_eq!(svg.matches("<svg").count(), 1);
}

#[cfg(feature = "texlabels")]
#[test]
fn cli_typesets_math_labels() {
    let dir = tempfile::tempdir().unwrap();
    let (svg, _) = cli_svg(&write_figure(dir.path()), dir.path());
    for lit in LITERALS {
        assert!(!svg.contains(lit), "`{lit}` leaked as literal text");
    }
    assert!(
        !svg.contains("T_1") && !svg.contains("V_{CC}"),
        "raw TeX leaked"
    );
    // One nested, glyph-path fragment per math label.
    assert_eq!(svg.matches("<svg x=\"").count(), 3, "{svg}");
    assert!(svg.contains("<path"), "typeset glyphs are paths");
}

#[cfg(not(feature = "texlabels"))]
#[test]
fn cli_without_feature_keeps_math_labels_literal() {
    let dir = tempfile::tempdir().unwrap();
    let (svg, _) = cli_svg(&write_figure(dir.path()), dir.path());
    let pure = dune_pic::render_svg(FIGURE, &dune_pic::ExtractOptions::default()).unwrap();
    assert_eq!(svg, pure);
}

/// Typesetting is presentation only: the netlist (values come raw from
/// the call site; the display guard compares the drawn text) is the
/// same with and without the renderer.
#[test]
fn math_labels_leave_the_netlist_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (_, netlist) = cli_svg(&write_figure(dir.path()), dir.path());
    let ex = dune_pic::extract(FIGURE, &dune_pic::ExtractOptions::default()).unwrap();
    assert!(ex.diagnostics.is_empty(), "{:?}", ex.diagnostics);
    assert_eq!(netlist, ex.netlist);
    assert!(
        netlist.contains("Q1 ") && netlist.contains(" qn"),
        "{netlist}"
    );
    assert!(!netlist.contains('$'), "{netlist}");
}

/// Registering the renderer changes nothing for figures that do not
/// opt into `texlabels`: every corpus figure's CLI SVG is byte-equal
/// to the pure rpic-core one (and so to what the wasm build draws).
#[test]
fn corpus_figures_svg_unchanged_by_the_renderer() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/pic");
    let dir = tempfile::tempdir().unwrap();
    let mut seen = 0;
    for entry in std::fs::read_dir(&corpus).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "pic") || path.ends_with("spice.pic") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        if src.contains("texlabels") {
            continue; // opted in: typesetting is the point there
        }
        let (svg, _) = cli_svg(&path, dir.path());
        let pure = dune_pic::render_svg(&src, &dune_pic::ExtractOptions::default()).unwrap();
        assert!(svg == pure, "{}: CLI SVG differs", path.display());
        seen += 1;
    }
    assert!(seen >= 14, "only {seen} corpus figures");
}
