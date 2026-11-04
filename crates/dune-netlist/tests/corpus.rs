//! The whole corpus must parse without errors (warnings are
//! reported but allowed — the decks are historical).

use std::path::{Path, PathBuf};

fn corpus_cirs(sub: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .join(sub);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("cir")) {
                out.push(p);
            }
        }
    }
    out.sort();
    assert!(!out.is_empty(), "no circuits under {}", dir.display());
    out
}

#[test]
fn parse_the_3f5_examples() {
    let mut failures = Vec::new();
    for path in corpus_cirs("spice3f5/examples") {
        let outcome = dune_netlist::parse_file(&path).expect("readable file");
        for d in &outcome.diags {
            let line = d.render(&outcome.sources);
            if d.severity == dune_netlist::Severity::Error {
                failures.push(line);
            } else {
                eprintln!("note: {line}");
            }
        }
        // Every deck in this corpus has elements and a title.
        assert!(
            !outcome.netlist.elements.is_empty(),
            "{}: no elements parsed",
            path.display()
        );
    }
    assert!(
        failures.is_empty(),
        "parse errors in the corpus:\n{}",
        failures.join("\n")
    );
}
