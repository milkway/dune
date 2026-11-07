//! The overlay map's write-back spans (issue #17): an editor replaces
//! `ElementOverlay::value` / `params` text in the source and
//! re-extracts; exactly that value changes on the card.

use std::path::PathBuf;

use dune_pic::{ElementOverlay, ExtractOptions, Extraction, PicRange, extract};

fn corpus(name: &str) -> String {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/pic")
        .join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn ex(src: &str) -> Extraction {
    extract(src, &ExtractOptions::default()).unwrap_or_else(|e| panic!("{e}\n{src}"))
}

fn fig(body: &str) -> String {
    format!(".PS\nspice = 1\n{body}\n.PE\n")
}

fn elem<'a>(e: &'a Extraction, refdes: &str) -> &'a ElementOverlay {
    e.map
        .elements
        .iter()
        .find(|x| x.refdes == refdes)
        .unwrap_or_else(|| panic!("no {refdes}"))
}

/// Byte offset of 1-based `(line, col)` (col in chars) in `src`.
fn offset(src: &str, line: u32, col: u32) -> usize {
    let mut start = 0;
    for _ in 1..line {
        start += src[start..].find('\n').expect("line in range") + 1;
    }
    let rest = &src[start..];
    start
        + rest
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(rest.len()))
            .nth(col as usize - 1)
            .expect("col in range")
}

/// The source text in `r`.
fn slice<'a>(src: &'a str, r: &PicRange) -> &'a str {
    &src[offset(src, r.line, r.col)..offset(src, r.end_line, r.end_col)]
}

/// `src` with the text in `r` replaced by `new` — the editor's write.
fn write(src: &str, r: &PicRange, new: &str) -> String {
    let (a, b) = (
        offset(src, r.line, r.col),
        offset(src, r.end_line, r.end_col),
    );
    format!("{}{new}{}", &src[..a], &src[b..])
}

/// The netlist lines that differ between two extractions.
fn diff(a: &Extraction, b: &Extraction) -> Vec<(String, String)> {
    let (la, lb): (Vec<&str>, Vec<&str>) =
        (a.netlist.lines().collect(), b.netlist.lines().collect());
    assert_eq!(la.len(), lb.len(), "{}\n---\n{}", a.netlist, b.netlist);
    la.iter()
        .zip(&lb)
        .filter(|(x, y)| x != y)
        .map(|(x, y)| (x.to_string(), y.to_string()))
        .collect()
}

/// Rewrite `refdes`'s param `name` (or its whole value with `None`)
/// to `new`, re-extract, and return the one card that changed.
fn rewrite(src: &str, refdes: &str, name: Option<&str>, new: &str) -> (String, String) {
    let before = ex(src);
    let e = elem(&before, refdes);
    let (r, text) = match name {
        None => {
            let v = e.value.as_ref().expect("value span");
            (&v.span, &v.text)
        }
        Some(n) => {
            let p = e
                .params
                .iter()
                .find(|p| p.name == n)
                .unwrap_or_else(|| panic!("{refdes}: no param {n}: {:?}", e.params));
            (&p.span, &p.text)
        }
    };
    assert_eq!(slice(src, r), text, "span text");
    let edited = write(src, r, new);
    let after = ex(&edited);
    let d = diff(&before, &after);
    assert_eq!(d.len(), 1, "{d:?}\n{edited}");
    d.into_iter().next().unwrap()
}

#[test]
fn rc_value_spans_and_principals() {
    let src = corpus("rc.pic");
    let e = ex(&src);
    let r1 = elem(&e, "R1");
    let v = r1.value.as_ref().unwrap();
    assert_eq!(v.text, "1k");
    // `R1: resistor(Vin, Vout, 1k)` is line 6 of rc.pic.
    assert_eq!(
        v.span,
        PicRange {
            line: 6,
            col: 25,
            end_line: 6,
            end_col: 27
        }
    );
    assert_eq!(r1.principal.as_deref(), Some("resistance"));
    assert_eq!(r1.params.len(), 1);
    assert_eq!(
        (r1.params[0].name.as_str(), r1.params[0].text.as_str()),
        ("resistance", "1k")
    );
    assert_eq!(elem(&e, "C1").principal.as_deref(), Some("capacitance"));
    // V1 has a display ("G₁") after its value: the span is the value's.
    let v1 = elem(&e, "V1");
    assert_eq!(
        v1.value.as_ref().unwrap().text,
        "PULSE(0 5 0 1u 1u 10m 20m)"
    );
    assert_eq!(v1.principal.as_deref(), Some("dc"));
    let names: Vec<&str> = v1.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["pulse"]);
}

#[test]
fn writing_a_value_back_changes_only_that_value() {
    let src = corpus("rc.pic");
    assert_eq!(
        rewrite(&src, "R1", None, "2.2k"),
        ("R1 Vin Vout 1k".into(), "R1 Vin Vout 2.2k".into())
    );
    assert_eq!(
        rewrite(&src, "C1", Some("capacitance"), "470n"),
        ("C1 Vout 0 1u".into(), "C1 Vout 0 470n".into())
    );
    // PULSE with a display after it.
    assert_eq!(
        rewrite(&src, "V1", Some("pulse"), "PULSE(0 3.3 0 1n 1n 5m 10m)"),
        (
            "V1 Vin 0 PULSE(0 5 0 1u 1u 10m 20m)".into(),
            "V1 Vin 0 PULSE(0 3.3 0 1n 1n 5m 10m)".into()
        )
    );
}

#[test]
fn source_with_dc_and_ac_fields() {
    let src = fig("V1: vsource((0,1),(0,0), DC 5 AC 1 45, \"G₁\")
R1: resistor((0,1),(0,0), 1k); ground((0,0))
.ac dec 10 1 1meg");
    let e = ex(&src);
    let v1 = elem(&e, "V1");
    let got: Vec<(&str, &str)> = v1
        .params
        .iter()
        .map(|p| (p.name.as_str(), p.text.as_str()))
        .collect();
    assert_eq!(got, [("dc", "5"), ("acmag", "1"), ("acphase", "45")]);
    assert_eq!(v1.value.as_ref().unwrap().text, "DC 5 AC 1 45");
    let card = |s: &str| ("V1 n1 0 DC 5 AC 1 45".to_string(), s.to_string());
    assert_eq!(
        rewrite(&src, "V1", Some("acmag"), "1.5"),
        card("V1 n1 0 DC 5 AC 1.5 45")
    );
    assert_eq!(
        rewrite(&src, "V1", Some("acphase"), "-90"),
        card("V1 n1 0 DC 5 AC 1 -90")
    );
    assert_eq!(
        rewrite(&src, "V1", Some("dc"), "12"),
        card("V1 n1 0 DC 12 AC 1 45")
    );
    assert_eq!(
        rewrite(&src, "V1", None, "SIN(0 1 1k)"),
        card("V1 n1 0 SIN(0 1 1k)")
    );
}

#[test]
fn wrappers_inline_blocks_and_coupling_have_no_span() {
    let e = ex(&fig("define rr { R7: resistor($2, $1, 10k) }
define pass { R8: resistor($1, $2, $3) }
rr((0,0), (1,0), 5)
pass((1,0), (2,0), 2k)
L1: inductor((2,0),(3,0), 1m); L2: inductor((3,0),(4,0), 1m)
K1: coupling(L1, L2, 0.9)
ground((0,0)); ground((4,0))"));
    // Drawn fallback (the wrapper's 3rd argument is not the value).
    let r7 = elem(&e, "R7");
    assert!(r7.value.is_none() && r7.params.is_empty());
    assert_eq!(r7.principal.as_deref(), Some("resistance"));
    // A wrapper whose argument does reach the card: the value is
    // read, but no span is offered (the call is not the element's).
    assert!(e.netlist.contains("R8 n1 n2 2k"), "{}", e.netlist);
    assert!(elem(&e, "R8").value.is_none());
    let k1 = elem(&e, "K1");
    assert!(k1.value.is_none() && k1.principal.is_none());
    assert!(elem(&e, "L1").value.is_some());

    // An inline element block: no call at all.
    let e = ex(&fig(
        "R1: [ \"5k\" at (0.5,0.2); P: circle invis rad 0 at (0,0); \
         N: circle invis rad 0 at (1,0); line from (0,0) to (1,0) ]
ground((0,0)); ground((1,0))",
    ));
    assert!(e.netlist.contains(" 5k\n"), "{}", e.netlist);
    assert!(elem(&e, "R1").value.is_none());
}

#[test]
fn multi_line_arguments() {
    let src = fig("R9: resistor((2,0), (3,0), 1.5e3 \\
  )
V1: vsource((3,0), (4,0), PULSE(0 1e-3 \\
  0 1e-9))
ground((2,0)); ground((4,0))");
    let e = ex(&src);
    // A value whose raw argument ends in a continuation: the trimmed
    // range is one line.
    let r9 = elem(&e, "R9").value.as_ref().unwrap();
    assert_eq!(r9.text, "1.5e3");
    assert_eq!((r9.span.line, r9.span.end_line), (3, 3));
    // A value split over lines: the span covers both, verbatim.
    let v1 = elem(&e, "V1");
    let v = v1.value.as_ref().unwrap();
    assert_eq!(v.text, "PULSE(0 1e-3 \\\n  0 1e-9)");
    assert_eq!(
        v.span,
        PicRange {
            line: 5,
            col: 27,
            end_line: 6,
            end_col: 10
        }
    );
    assert_eq!(slice(&src, &v.span), v.text);
    assert_eq!(v1.params[0].name, "pulse");
    assert_eq!(v1.params[0].span, v.span);
    assert_eq!(
        rewrite(&src, "V1", Some("pulse"), "PULSE(0 2 0 1n)"),
        (
            "V1 n1 0 PULSE(0 0.001 0 0.000000001)".into(),
            "V1 n1 0 PULSE(0 2 0 1n)".into()
        )
    );
    assert_eq!(
        rewrite(&src, "R9", None, "330"),
        ("R9 0 n1 1.5e3".into(), "R9 0 n1 330".into())
    );
}

#[test]
fn columns_count_chars_after_multibyte_text() {
    // A display with multibyte chars earlier on the same line: the
    // value's column counts chars, not bytes.
    let src = fig(
        "V1: vsource((0,1),(0,0), 1, \"Ωµ₁\"); R1: resistor((0,1),(1,1), 4.7k, \"R₁Ω\")
C1: capacitor((1,1),(1,0), \"10n\"); ground((0,0)); ground((1,0))",
    );
    let e = ex(&src);
    let r1 = elem(&e, "R1").value.as_ref().unwrap();
    let line = src.lines().nth(2).unwrap();
    let chars: Vec<char> = line.chars().collect();
    let want = chars
        .windows(4)
        .position(|w| w.iter().collect::<String>() == "4.7k")
        .unwrap() as u32
        + 1;
    assert!(
        line.find("4.7k").unwrap() as u32 + 1 > want,
        "bytes ≠ chars here"
    );
    assert_eq!(
        (r1.span.line, r1.span.col, r1.span.end_col),
        (3, want, want + 4)
    );
    assert_eq!(slice(&src, &r1.span), "4.7k");
    assert_eq!(
        rewrite(&src, "R1", Some("resistance"), "10k"),
        ("R1 n1 n2 4.7k".into(), "R1 n1 n2 10k".into())
    );
    // A quoted value: the span is inside the quotes.
    let c1 = elem(&e, "C1").value.as_ref().unwrap();
    assert_eq!(c1.text, "10n");
    assert_eq!(slice(&src, &c1.span), "10n");
    assert_eq!(
        rewrite(&src, "C1", None, "22n"),
        ("C1 n2 0 10n".into(), "C1 n2 0 22n".into())
    );
}

#[test]
fn other_letters_name_their_params() {
    let src = fig("Q1: npn((3,1), QX 2, \"T₁\")
M1: nmos((5,1), NM W=1e-6 L=1u)
line from Q1.C to (3.16,2); line from M1.D to (5.16,2)
line from Q1.B to (2,1) then to (2,2); line from (2,2) to (5.16,2)
line from M1.G to (4.5,0)
line from Q1.E to (3.16,0); line from M1.S to (5.16,0)
V1: vsource((8,2),(8,0), 1)
H1: ccvs((8,2),(9,2), V1 100)
line from (5.16,2) to (8,2); line from (2,0) to (8,0); ground((2,0)); ground((9,2))
.model QX NPN
.model NM NMOS");
    let e = ex(&src);
    let names = |r: &str| {
        elem(&e, r)
            .params
            .iter()
            .map(|p| format!("{}={}", p.name, p.text))
            .collect::<Vec<_>>()
    };
    assert_eq!(names("Q1"), ["area=2"]);
    assert_eq!(names("M1"), ["w=1e-6", "l=1u"]);
    assert_eq!(names("H1"), ["gain=100"]);
    assert_eq!(elem(&e, "M1").principal, None);
    assert_eq!(elem(&e, "H1").principal.as_deref(), Some("gain"));
    assert_eq!(elem(&e, "Q1").value.as_ref().unwrap().text, "QX 2");
    let (_, after) = rewrite(&src, "M1", Some("w"), "2u");
    assert!(after.ends_with("NM W=2u L=1u"), "{after}");
}
