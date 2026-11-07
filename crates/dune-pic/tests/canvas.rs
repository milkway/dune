//! The overlay map's SVG frame (issue #19): `OverlayMap::canvas` maps
//! pins (pic inches, y up) onto the user units of `render_svg`'s SVG.

use std::path::PathBuf;

use dune_pic::{ExtractOptions, Extraction, extract, render_svg};

fn corpus(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/pic")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn ex(src: &str) -> Extraction {
    extract(src, &ExtractOptions::default()).unwrap_or_else(|e| panic!("{e}\n{src}"))
}

/// Every coordinate pair of the SVG's `<line>` endpoints and
/// `<polyline>`/`<polygon>` points (user units).
fn stroke_points(svg: &str) -> Vec<(f64, f64)> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let k = format!(" {name}=\"");
        let i = tag.find(&k)? + k.len();
        Some(tag[i..].split('"').next()?.to_string())
    };
    let num = |s: String| s.parse::<f64>().unwrap();
    let mut out = Vec::new();
    for tag in svg.split('<').skip(1) {
        if tag.starts_with("line ") {
            for (x, y) in [("x1", "y1"), ("x2", "y2")] {
                out.push((num(attr(tag, x).unwrap()), num(attr(tag, y).unwrap())));
            }
        } else if tag.starts_with("polyline ") || tag.starts_with("polygon ") {
            for p in attr(tag, "points").unwrap().split_whitespace() {
                let (x, y) = p.split_once(',').unwrap();
                out.push((num(x.into()), num(y.into())));
            }
        }
    }
    out
}

/// The `(w, h)` of the root `viewBox`.
fn view_box(svg: &str) -> (f64, f64) {
    let vb = svg
        .split("viewBox=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let n: Vec<f64> = vb.split_whitespace().map(|x| x.parse().unwrap()).collect();
    (n[2], n[3])
}

/// Each pin, mapped by the canvas, lands on a drawn stroke endpoint
/// (SVG numbers carry six decimals, so 1e-6 is the resolution).
fn check_frame(src: &str) {
    let e = ex(src);
    let svg = render_svg(src, &ExtractOptions::default()).unwrap();
    let c = e.map.canvas;
    let (vw, vh) = view_box(&svg);
    assert!(
        (vw - c.w * 96.0).abs() < 1e-6 && (vh - c.h * 96.0).abs() < 1e-6,
        "{c:?} vs {vw} {vh}"
    );
    let pts = stroke_points(&svg);
    let mut n = 0;
    for el in &e.map.elements {
        for p in &el.pins {
            let (sx, sy) = c.to_svg(p.x, p.y);
            let best = pts
                .iter()
                .map(|&(x, y)| (x - sx).abs().max((y - sy).abs()))
                .fold(f64::INFINITY, f64::min);
            assert!(
                best < 1e-6,
                "{}.{} at ({sx}, {sy}): nearest {best}",
                el.refdes,
                p.name
            );
            n += 1;
        }
    }
    assert!(n > 0);
}

#[test]
fn canvas_maps_pins_onto_the_svg_strokes() {
    check_frame(&corpus("rc.pic"));
    let c = ex(&corpus("rc.pic")).map.canvas;
    assert_eq!(c.rescale, 1.0);
    // Wrong signs are far off (the formula is `+ origin`).
    assert!(c.origin[0].abs() > 0.1 && c.origin[1].abs() > 0.1, "{c:?}");
}

#[test]
fn canvas_with_margins_rescale_and_a_fixed_page() {
    let body = "V1: vsource((0,1),(0,0), 1)
R1: resistor((0,1),(2,1), 1k)
C1: capacitor((2,1),(2,0), 1u)
line from (0,0) to (2,0); ground((1,0))";
    // Margins (all + per side) and a thick prelude.
    check_frame(&format!(
        ".PS\nspice = 1\nmargin = 0.3; leftmargin = 0.2; topmargin = 0.1\nlinethick = 2\n{body}\n.PE\n"
    ));
    // Whole-picture rescale: `.PS <width>` shrinks a 2-inch figure.
    let src = format!(".PS 1.3\nspice = 1\n{body}\n.PE\n");
    check_frame(&src);
    let c = ex(&src).map.canvas;
    assert!((c.rescale - 1.0).abs() > 0.05, "{c:?}");
    // A fixed page rectangle.
    check_frame(&format!(
        ".PS\nspice = 1\ncanvas from (-1,-1) to (4,3)\n{body}\n.PE\n"
    ));
}

#[test]
fn canvas_matches_rpic_scene_header() {
    // The scene's `canvas` (six decimals) is the same frame.
    let src = corpus("rc.pic");
    let mut full = dune_pic::PRELUDE.to_string();
    full.push_str(&src);
    let opts = rpic_core::CompileOptions {
        circuits: true,
        includes: rpic_core::IncludePolicy::Deny,
        ..Default::default()
    };
    let d = rpic_core::compile_with_diagnostics(&full, &opts).unwrap();
    let scene = rpic_core::scene_json(&d, &rpic_core::SceneOptions::default()).json;
    let field = |k: &str| -> f64 {
        let i = scene.find(&format!("\"{k}\":")).unwrap() + k.len() + 3;
        scene[i..]
            .split([',', '}'])
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    let origin: Vec<f64> = scene
        .split("\"origin\":[")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap()
        .split(',')
        .map(|x| x.parse().unwrap())
        .collect();
    let c = ex(&src).map.canvas;
    for (a, b) in [
        (c.w, field("w")),
        (c.h, field("h")),
        (c.origin[0], origin[0]),
        (c.origin[1], origin[1]),
        (c.rescale, field("rescale")),
    ] {
        assert!((a - b).abs() <= 5e-7, "{a} vs {b}\n{c:?}");
    }
}

#[test]
fn canvas_of_every_corpus_figure() {
    for f in [
        "bandpass", "bjtamp", "ccprobe", "cmosinv", "diffpair", "mos4bias", "opamp", "rc", "rect",
        "rlc", "rtlinv", "tline", "tuned", "vcamp",
    ] {
        let src = corpus(&format!("{f}.pic"));
        let e = ex(&src);
        let svg = render_svg(&src, &ExtractOptions::default()).unwrap();
        let (vw, vh) = view_box(&svg);
        let c = e.map.canvas;
        assert!(
            (vw - c.w * 96.0).abs() < 1e-6 && (vh - c.h * 96.0).abs() < 1e-6,
            "{f}"
        );
    }
}
