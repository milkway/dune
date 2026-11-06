//! Numerical oracle harness (PLAN.md §2.4, D-0006): runs the pinned
//! ngspice in batch mode and compares SPICE3 rawfiles with the SPICE
//! tolerances (`reltol=1e-3`, `abstol=1e-12`, `vntol=1e-6`).
//!
//! The candidate side is Dune itself (library call) for every deck
//! whose analyses the engine implements (Phase 1: `.op`). For the
//! rest, `<circuit>` runs ngspice afresh against the golden
//! (environment drift check) and `--ci` checks the golden against
//! itself (reader + comparator sanity; CI has no ngspice).

#![forbid(unsafe_code)]

mod compare;
mod corpus;
mod dune_run;
mod ngspice;

use std::path::Path;

use anyhow::{Context, Result, bail};
use dune_raw::Rawfile;

use compare::{Report, Tolerances, compare_rawfiles};
use corpus::{Circuit, Corpus};

const USAGE: &str = "\
usage:
  dune-oracle --list                 inventory the corpus
  dune-oracle --ci                   check every golden rawfile (no ngspice)
  dune-oracle --gen-golden [NAME..]  run ngspice, write corpus/golden/
  dune-oracle --compare A.raw B.raw  compare two rawfiles
  dune-oracle NAME..                 run ngspice afresh, compare vs golden";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ok = match args.first().map(String::as_str) {
        Some("--list") => list()?,
        Some("--ci") => ci()?,
        Some("--gen-golden") => gen_golden(&args[1..])?,
        Some("--compare") => compare_files(&args[1..])?,
        Some(flag) if flag.starts_with('-') => bail!("unknown flag `{flag}`\n{USAGE}"),
        Some(_) => run_circuits(&args)?,
        None => bail!("missing arguments\n{USAGE}"),
    };
    if !ok {
        std::process::exit(1);
    }
    Ok(())
}

fn list() -> Result<bool> {
    let corpus = Corpus::load()?;
    println!(
        "{:<28} {:<20} {:<14} {:<7} note",
        "circuit", "analyses", "devices", "golden"
    );
    for c in &corpus.circuits {
        let golden = if corpus.golden_path(c).is_file() {
            "yes"
        } else {
            "-"
        };
        let note = c.skip.as_deref().map(|r| format!("SKIP: {r}"));
        println!(
            "{:<28} {:<20} {:<14} {:<7} {}",
            c.rel,
            c.analyses.join(" "),
            c.devices.iter().collect::<String>(),
            golden,
            note.unwrap_or_default()
        );
    }
    println!(
        "\n{} circuits under {}",
        corpus.circuits.len(),
        corpus.root.display()
    );
    Ok(true)
}

/// CI mode, no ngspice install needed: decks Dune can simulate run
/// through the engine and compare against the golden; the rest get a
/// golden-vs-golden sanity check until their analyses exist.
fn ci() -> Result<bool> {
    let corpus = Corpus::load()?;
    let tol = Tolerances::default();
    let mut all_ok = true;
    let mut via_dune = 0usize;
    let mut via_self = 0usize;
    print_result_header();
    for c in &corpus.circuits {
        if let Some(reason) = &c.skip {
            print_skip(&c.stem, reason);
            continue;
        }
        let golden_path = corpus.golden_path(c);
        let golden = match load_raw(&golden_path) {
            Err(e) => {
                println!("{:<14} {e:#}", c.stem);
                println!("{:<14} FAIL (run --gen-golden and commit the result)", "");
                all_ok = false;
                continue;
            }
            Ok(g) => g,
        };
        if dune_run::supported(c) {
            via_dune += 1;
            match dune_run::run(&c.path) {
                Ok(candidate) => {
                    let report = compare_rawfiles(&golden, &candidate, &tol);
                    all_ok &= print_report(&c.stem, "dune", &report, &corpus.xfails);
                }
                Err(e) => all_ok &= print_dune_failure(&c.stem, &e, &corpus.xfails),
            }
        } else {
            via_self += 1;
            let report = compare_rawfiles(&golden, &golden, &tol);
            all_ok &= print_report(&c.stem, "self", &report, &corpus.xfails);
        }
    }
    println!(
        "\n{via_dune} circuit(s) simulated by dune, {via_self} still \
         golden-vs-golden (analyses pending per PLAN.md §3)"
    );
    Ok(all_ok)
}

/// Compare the named circuits against their goldens: through the
/// engine when Dune supports the deck, through a fresh ngspice run
/// otherwise.
fn run_circuits(args: &[String]) -> Result<bool> {
    let corpus = Corpus::load()?;
    let scratch = scratch_dir()?;
    let tol = Tolerances::default();
    let mut all_ok = true;
    let mut version_checked = false;
    print_result_header();
    for arg in args {
        let Some(c) = corpus.find(arg) else {
            bail!("`{arg}` is not in the corpus (see --list)");
        };
        if let Some(reason) = &c.skip {
            print_skip(&c.stem, reason);
            continue;
        }
        let golden = load_raw(&corpus.golden_path(c))
            .with_context(|| format!("no usable golden for `{}`; run --gen-golden", c.stem))?;
        if dune_run::supported(c) {
            match dune_run::run(&c.path) {
                Ok(candidate) => {
                    let report = compare_rawfiles(&golden, &candidate, &tol);
                    all_ok &= print_report(&c.stem, "dune", &report, &corpus.xfails);
                }
                Err(e) => all_ok &= print_dune_failure(&c.stem, &e, &corpus.xfails),
            }
        } else {
            if !version_checked {
                ngspice::check_version(&corpus.root)?;
                version_checked = true;
            }
            let fresh_path = scratch.path().join(format!("{}.raw", c.stem));
            ngspice::run(&c.path, &fresh_path)?;
            let fresh = load_raw(&fresh_path)?;
            let report = compare_rawfiles(&golden, &fresh, &tol);
            all_ok &= print_report(&c.stem, "ngspice", &report, &corpus.xfails);
        }
    }
    Ok(all_ok)
}

/// Run ngspice over the corpus (or the named subset) and write
/// `corpus/golden/<stem>.raw`.
fn gen_golden(args: &[String]) -> Result<bool> {
    let corpus = Corpus::load()?;
    let pin = ngspice::check_version(&corpus.root)?;
    let golden_dir = corpus.root.join("golden");
    std::fs::create_dir_all(&golden_dir)?;
    let scratch = scratch_dir()?;

    let selected: Vec<&Circuit> = if args.is_empty() {
        corpus.circuits.iter().collect()
    } else {
        args.iter()
            .map(|a| {
                corpus
                    .find(a)
                    .ok_or_else(|| anyhow::anyhow!("`{a}` is not in the corpus (see --list)"))
            })
            .collect::<Result<_>>()?
    };

    let mut all_ok = true;
    println!("{:<14} {:<6} {:<8} note", "circuit", "plots", "points");
    for c in selected {
        if let Some(reason) = &c.skip {
            print_skip(&c.stem, reason);
            continue;
        }
        let tmp = scratch.path().join(format!("{}.raw", c.stem));
        if let Err(e) = ngspice::run(&c.path, &tmp) {
            println!("{:<14} {e:#}", c.stem);
            all_ok = false;
            continue;
        }
        match load_raw(&tmp) {
            Err(e) => {
                println!(
                    "{:<14} {:<6} {:<8} FAIL: {e:#} (no rawfile output? add to corpus/SKIP)",
                    c.stem, "-", "-"
                );
                all_ok = false;
            }
            Ok(raw) => {
                // Deduplicate: ngspice repeats its `Command:` header
                // once per plot (see corpus/KNOWN_DIFFS.md).
                let mut seen = Vec::new();
                for w in &raw.warnings {
                    if !seen.contains(&w) {
                        println!("{:<14} warning: {w}", c.stem);
                        seen.push(w);
                    }
                }
                std::fs::copy(&tmp, corpus.golden_path(c))?;
                let points: usize = raw.plots.iter().map(dune_raw::Plot::num_points).sum();
                println!(
                    "{:<14} {:<6} {:<8} written",
                    c.stem,
                    raw.plots.len(),
                    points
                );
            }
        }
    }
    println!(
        "\ngolden generated with {pin} into {}",
        golden_dir.display()
    );
    Ok(all_ok)
}

fn compare_files(args: &[String]) -> Result<bool> {
    let [a, b] = args else {
        bail!("--compare takes exactly two rawfile paths\n{USAGE}");
    };
    let oracle = load_raw(Path::new(a))?;
    let candidate = load_raw(Path::new(b))?;
    let report = compare_rawfiles(&oracle, &candidate, &Tolerances::default());
    print_result_header();
    let ok = print_report(a, "file", &report, &[]);
    Ok(ok)
}

fn load_raw(path: &Path) -> Result<Rawfile> {
    let raw = Rawfile::read_from(path).with_context(|| format!("reading {}", path.display()))?;
    if raw.plots.is_empty() {
        bail!("{} contains no plots", path.display());
    }
    Ok(raw)
}

/// Private scratch directory for ngspice output: random name, created
/// atomically with owner-only permissions, removed on drop (a
/// predictable `/tmp` name would be open to symlink squatting).
fn scratch_dir() -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix("dune-oracle-")
        .tempdir()
        .context("creating scratch directory")
}

fn print_result_header() {
    println!(
        "{:<14} {:<26} {:<8} {:>4} {:>7} {:>10} {:>8}  result",
        "circuit", "plot", "cand", "vars", "pts", "max|err|", "margin"
    );
}

fn print_skip(name: &str, reason: &str) {
    println!("{name:<14} SKIP: {reason}");
}

/// Report a failed Dune run; returns whether it is acceptable — only
/// when a `corpus/XFAIL` entry for the deck names the failure in its
/// `accept-error=` flag (the other branch of a bistable deck, e.g.
/// the transient step budget; D-0022).
fn print_dune_failure(name: &str, err: &str, xfails: &[corpus::Xfail]) -> bool {
    println!("{name:<14} dune failed: {err}");
    match xfails
        .iter()
        .find(|x| x.stem == name && x.accepts_error(err))
    {
        Some(x) => {
            println!(
                "{:<14}   xfail: accepted failure ({}) of a bistable deck: {}",
                "",
                x.accept_error.as_deref().unwrap_or_default(),
                x.reason
            );
            println!("{name:<14} XFAIL (accepted error)");
            true
        }
        None => false,
    }
}

/// Print one row per compared plot; returns whether everything passed.
/// `margin` is the worst `|a-b| / (floor + reltol*max(|a|,|b|))` ratio:
/// `<= 1.0` is within tolerance.
fn print_report(name: &str, cand: &str, report: &Report, xfails: &[corpus::Xfail]) -> bool {
    let mut all_ok = true;
    for p in &report.plots {
        let xfail = xfails
            .iter()
            .find(|x| x.stem == name && p.name.eq_ignore_ascii_case(&x.plot));
        let verdict = match (&xfail, p.pass()) {
            (_, true) => "PASS",
            // An expected failure within its recorded margin cap is
            // reported but does not fail the run (corpus/XFAIL).
            (Some(x), false) if p.problems.is_empty() && p.worst_ratio <= x.max_margin => "XFAIL",
            _ => "FAIL",
        };
        let worst = p
            .worst_var
            .as_deref()
            .map(|v| format!(" worst: {v}"))
            .unwrap_or_default();
        println!(
            "{:<14} {:<26} {:<8} {:>4} {:>7} {:>10.3e} {:>8.3}  {verdict}{worst}",
            name, p.name, cand, p.nvars, p.npoints, p.max_abs_err, p.worst_ratio
        );
        for problem in &p.problems {
            println!("{:<14}   problem: {problem}", "");
        }
        for note in &p.notes {
            println!("{:<14}   note: {note}", "");
        }
        match (&xfail, verdict) {
            (Some(x), "XFAIL") => {
                println!("{:<14}   xfail: {} (cap {})", "", x.reason, x.max_margin);
            }
            // A bistable deck passes on one branch of its fork and
            // fails within the cap on the other (D-0022): both are
            // expected, so a PASS does not ask for removal.
            (Some(x), "PASS") if x.bistable => {
                println!(
                    "{:<14}   note: bistable entry in corpus/XFAIL, passing on this branch",
                    ""
                );
            }
            (Some(_), "PASS") => {
                println!(
                    "{:<14}   note: listed in corpus/XFAIL but passing; remove the entry",
                    ""
                );
            }
            _ => {}
        }
        if verdict == "FAIL" {
            all_ok = false;
        }
    }
    for problem in &report.problems {
        println!("{name:<14} problem: {problem}");
        all_ok = false;
    }
    all_ok
}
