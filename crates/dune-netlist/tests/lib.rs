//! `.lib`/`.include` against real files (fixtures under
//! `tests/fixtures/lib`), and the parser bugs found while measuring
//! the open PDKs (`docs/pdk-parser.md` §8).

use std::path::{Path, PathBuf};

use dune_netlist::ir::*;
use dune_netlist::{ParseOutcome, Severity, parse_file, parse_str};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lib")
}

/// Errors as `file-name:line: message`, the file name relative to the
/// fixtures directory, so tests can check spans land in the right file.
fn errors(out: &ParseOutcome) -> Vec<String> {
    out.diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| match d.span {
            Some(span) => {
                let loc = out.sources.locate(span);
                let name = loc
                    .file
                    .strip_prefix(fixtures())
                    .unwrap_or(&loc.file)
                    .display()
                    .to_string();
                format!("{name}:{}: {}", loc.line, d.message)
            }
            None => format!("?: {}", d.message),
        })
        .collect()
}

/// Parse `text` as if it were a deck living in the fixtures directory.
fn parse_here(text: &str) -> ParseOutcome {
    let name = fixtures().join("deck.cir");
    parse_str(name.to_str().unwrap(), text)
}

fn value(nl: &Netlist, name: &str) -> f64 {
    let el = nl.elements.iter().find(|e| e.name == name).unwrap();
    match el.device {
        Device::Resistor { value: Some(v), .. } => v,
        Device::Source { dc: Some(v), .. } => v,
        ref d => panic!("{name}: {d:?}"),
    }
}

#[test]
fn sections_nested_libs_and_includes_from_disk() {
    let out = parse_file(&fixtures().join("top.cir")).unwrap();
    assert_eq!(errors(&out), Vec::<String>::new());
    assert!(out.diags.is_empty(), "{:?}", out.diags);
    let nl = &out.netlist;
    assert_eq!(value(nl, "v1"), 2.0); // section `typ`, not `ff`
    assert_eq!(value(nl, "r1"), 1e3); // corners.lib `tt`, relative to models/
    assert_eq!(value(nl, "r2"), 7e3); // `.params` from ../sub/shared.inc
    // `outside` is never read: only section bodies are spliced.
    assert!(
        !errors(&parse_here(
            "t\n.lib models/main.lib typ\nr9 1 0 {outside}\n.end\n"
        ))
        .is_empty()
    );
}

#[test]
fn errors_inside_a_section_point_into_the_library() {
    let out = parse_here("t\n.lib models/main.lib broken\nv1 1 0 1\n.end\n");
    let e = errors(&out);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].starts_with("models/main.lib:13: "), "{e:?}");
}

#[test]
fn reader_errors_are_spanned_in_the_including_file() {
    for (line, want) in [
        (
            ".lib models/main.lib nosuch",
            "deck.cir:2: section `nosuch` not found",
        ),
        (".lib models nosuch", "deck.cir:2: cannot open library file"),
        (".include models", "deck.cir:2: cannot open .include file"),
        (".lib 'models/main.lib", "deck.cir:2: missing closing quote"),
        (".include cycle/a.inc", "cycle/b.inc:2: .include cycle"),
    ] {
        let out = parse_here(&format!("t\n{line}\nv1 1 0 1\n.end\n"));
        let e = errors(&out);
        assert_eq!(e.len(), 1, "{line}: {e:?}");
        assert!(e[0].starts_with(want), "{line}: {e:?}");
    }
    // A directory is refused by the loader, not read.
    let out = parse_here("t\n.lib models s\n.end\n");
    assert!(
        errors(&out)[0].contains("not a regular file"),
        "{:?}",
        errors(&out)
    );
}

#[cfg(unix)]
#[test]
fn unreadable_library_is_a_diagnostic() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("dune-netlist-lib-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("locked.lib");
    std::fs::write(&lib, ".lib s\nr1 1 0 1\n.endl\n").unwrap();
    std::fs::set_permissions(&lib, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable = std::fs::read(&lib).is_ok(); // running as root
    let deck = dir.join("deck.cir");
    let out = parse_str(deck.to_str().unwrap(), "t\n.lib locked.lib s\n.end\n");
    std::fs::set_permissions(&lib, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    if !readable {
        let e = errors(&out);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("cannot open library file"), "{e:?}");
    }
}

#[test]
fn quoted_include_opens() {
    // Was: the quotes stayed in the path (`…/"sub/extra.inc"`).
    for q in ["\"sub/shared.inc\"", "'sub/shared.inc'", "sub/shared.inc"] {
        let out = parse_here(&format!("t\n.include {q}\nr1 1 0 {{kshared}}\n.end\n"));
        assert_eq!(errors(&out), Vec::<String>::new(), "{q}");
        // `{7}` reaches the device as 7.000000000000001, like
        // ngspice-47's `%.15e` text substitution (probe c7 q_inst_7).
        assert_eq!(value(&out.netlist, "r1"), 7f64.next_up());
    }
}

#[test]
fn power_is_left_associative_like_ngspice() {
    // ngspice-47: 64 (probe D_POW); it was 512 here.
    let out = parse_str(
        "t.cir",
        "t\n.param p={2^3^2}\nv1 1 0 {p}\nv2 2 0 {-2^2}\n.end\n",
    );
    assert_eq!(value(&out.netlist, "v1"), 64.0);
    assert_eq!(value(&out.netlist, "v2"), -4.0);
}

#[test]
fn x_line_parameters_are_a_precise_error() {
    let out = parse_str(
        "t.cir",
        "t\n.subckt inv a b\nr1 a b 1\n.ends\nx1 1 2 inv w=0.15 l=1u\n.end\n",
    );
    let e: Vec<_> = out.diags.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("subcircuit parameters on X lines"), "{e:?}");
    assert!(!e[0].contains("no such subcircuit"));
    assert_eq!(out.sources.snippet(out.diags[0].span.unwrap()), "w");
}

#[test]
fn non_finite_parameters_fail_where_used() {
    // Defined but unused, or only passed through other `.param`s and
    // made finite again: fine, as in ngspice-47.
    let ok = parse_str(
        "t.cir",
        "t\n.param z={1/0} y={z*2} w={1/z}\nv1 1 0 {w+1}\n.end\n",
    );
    assert!(ok.is_clean(), "{:?}", ok.diags);
    assert_eq!(value(&ok.netlist, "v1"), 1.0);
    // Reaching an element, a model or a control card: spanned error.
    for (card, snippet) in [
        ("v1 1 0 {z}", "{z}"),
        ("r1 1 0 {z*0}", "{z*0}"),
        (".model rm r rsh={z}", "{z}"),
        (".tran 1n {z}", "{z}"),
    ] {
        let out = parse_str("t.cir", &format!("t\n.param z={{1/0}}\n{card}\n.end\n"));
        let errs: Vec<_> = out
            .diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errs.len(), 1, "{card}: {:?}", out.diags);
        assert!(errs[0].message.contains("not a finite number"), "{card}");
        assert_eq!(
            out.sources.snippet(errs[0].span.unwrap()),
            snippet,
            "{card}"
        );
    }
}
