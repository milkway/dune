//! The accuracy ruler (`--accuracy`): how far a deck's transient, run
//! at the step the deck asks for, is from the converged answer.
//!
//! The golden check (`--ci`) measures *fidelity*: Dune against one
//! ngspice run at the same settings. It cannot tell an accurate run
//! from an inaccurate one — a deck whose default step is too coarse
//! passes when both engines are wrong in the same way (`csw_rly`),
//! and fails when they are wrong in different ways (the XFAIL decks).
//! This ruler measures the other thing. For every transient deck:
//!
//! - the **candidate** is the deck as written;
//! - the **reference** is the same deck with `TMAX` divided by
//!   [`REFINE`], run by Dune itself. That a refined Dune run is the
//!   refined ngspice run is what the golden check establishes, and
//!   the converged companions of the XFAIL decks gate it directly
//!   (`regress/*_fine`, `*_gear`); the survey behind this ruler
//!   measured it on the whole corpus (docs/dune-oracle.md);
//! - the reference is trusted only if it has itself converged: the
//!   run at half the refinement must agree with it within
//!   [`CONVERGED`] of the ruler. A deck where it has not is not
//!   measurable this way (trapezoidal ringing that the step does not
//!   damp, switch events) and must be listed as such in
//!   `corpus/ACCURACY`;
//! - the **margin** is the comparator's worst ratio with the
//!   candidate's own grid driving: the error at the points the
//!   engine computed, the reference interpolated between its (40
//!   times denser) points.
//!
//! `corpus/ACCURACY` caps the margin of the decks that are not within
//! [`DEFAULT_CAP`]; a deck over its cap fails the run, so a change to
//! the integration or to the step control shows up here as numbers.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use dune_core::{Circuit, Options};
use dune_netlist::Severity;
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;

use crate::compare::{Tolerances, compare_rawfiles};
use crate::corpus::Corpus;
use crate::dune_run;

/// `TMAX` divisor of the reference run.
const REFINE: f64 = 40.0;
/// The reference counts as converged when the run at `REFINE / 2`
/// is within this fraction of the ruler of it.
const CONVERGED: f64 = 0.2;
/// Cap of a deck `corpus/ACCURACY` does not list.
const DEFAULT_CAP: f64 = 0.2;

/// One `corpus/ACCURACY` line.
enum Entry {
    /// `stem|<cap>|reason`: the deck's margin may reach `cap`.
    Cap(f64),
    /// `stem|unmeasured|reason`: no converged reference; not run.
    Unmeasured,
}

fn read_registry(root: &Path) -> Result<HashMap<String, Entry>> {
    let path = root.join("ACCURACY");
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut f = line.splitn(3, '|');
        let (Some(stem), Some(what), Some(_reason)) = (f.next(), f.next(), f.next()) else {
            bail!("corpus/ACCURACY: bad line `{line}` (want stem|cap-or-unmeasured|reason)");
        };
        let entry = if what == "unmeasured" {
            Entry::Unmeasured
        } else {
            Entry::Cap(
                what.parse()
                    .ok()
                    .filter(|c: &f64| c.is_finite() && *c > 0.0)
                    .with_context(|| format!("corpus/ACCURACY: bad cap in `{line}`"))?,
            )
        };
        if map.insert(stem.to_string(), entry).is_some() {
            bail!("corpus/ACCURACY: `{stem}` listed twice");
        }
    }
    Ok(map)
}

/// The deck's transient analyses, with `TMAX` divided by `refine`
/// (`None`: as written). `TMAX` defaults as the engine does:
/// `min(TSTEP, (TSTOP − TSTART)/50)`.
fn run_tran(path: &Path, refine: Option<f64>) -> Result<Rawfile, String> {
    let parsed = dune_netlist::parse_file(path).map_err(|e| e.to_string())?;
    if let Some(d) = parsed.diags.iter().find(|d| d.severity == Severity::Error) {
        return Err(d.render(&parsed.sources));
    }
    let (circuit, _notes) = Circuit::from_netlist(&parsed.netlist).map_err(|e| e.to_string())?;
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let mut plots = Vec::new();
    for analysis in &parsed.netlist.analyses {
        let Analysis::Tran {
            tstep,
            tstop,
            tstart,
            tmax,
            uic,
            span,
        } = analysis
        else {
            continue;
        };
        let base = tmax.unwrap_or_else(|| f64::min(*tstep, (tstop - tstart.unwrap_or(0.0)) / 50.0));
        let card = Analysis::Tran {
            tstep: *tstep,
            tstop: *tstop,
            tstart: *tstart,
            tmax: refine.map(|r| base / r).or(*tmax),
            uic: *uic,
            span: *span,
        };
        let (solutions, _notes) = circuit.run(&card, &options).map_err(|e| e.to_string())?;
        plots.extend(
            solutions
                .iter()
                .map(|s| s.to_plot(&parsed.netlist.title, "")),
        );
    }
    Ok(Rawfile {
        plots,
        warnings: Vec::new(),
    })
}

/// The comparator's worst ratio over the plots, `driver`'s grid
/// driving; `Err` on a structural mismatch.
fn margin(driver: &Rawfile, other: &Rawfile) -> Result<f64, String> {
    let report = compare_rawfiles(driver, other, &Tolerances::default());
    if let Some(p) = report.problems.first() {
        return Err(p.clone());
    }
    let mut worst = 0.0f64;
    for plot in &report.plots {
        if let Some(p) = plot.problems.first() {
            return Err(p.clone());
        }
        worst = worst.max(plot.worst_ratio);
    }
    Ok(worst)
}

/// What the ruler found for one deck.
enum Measured {
    Margin(f64),
    /// The reference disagrees with the half-refined run by this much.
    Unconverged(f64),
    Failed(String),
}

fn measure(path: &Path) -> Measured {
    let run = |refine| run_tran(path, refine);
    let (cand, half, reference) = match (run(None), run(Some(REFINE / 2.0)), run(Some(REFINE))) {
        (Ok(c), Ok(h), Ok(r)) => (c, h, r),
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return Measured::Failed(e),
    };
    match margin(&half, &reference) {
        Err(e) => return Measured::Failed(e),
        Ok(conv) if conv > CONVERGED => return Measured::Unconverged(conv),
        Ok(_) => {}
    }
    match margin(&cand, &reference) {
        Ok(m) => Measured::Margin(m),
        Err(e) => Measured::Failed(e),
    }
}

/// `dune-oracle --accuracy [NAME..]`: measure every transient deck
/// (or the named ones) and hold each to its cap.
pub fn run(args: &[String]) -> Result<bool> {
    let corpus = Corpus::load()?;
    let registry = read_registry(&corpus.root)?;
    let mut ok = true;
    let (mut measured, mut sum) = (0usize, 0.0f64);
    println!("{:<14} {:>8} {:>6}  result", "circuit", "margin", "cap");
    for c in &corpus.circuits {
        if !args.is_empty() && !args.iter().any(|a| a.trim_end_matches(".cir") == c.stem) {
            continue;
        }
        if c.skip.is_some() || !c.analyses.iter().any(|a| a == ".tran") || !dune_run::supported(c) {
            continue;
        }
        let entry = registry.get(&c.stem);
        if matches!(entry, Some(Entry::Unmeasured)) {
            println!(
                "{:<14} {:>8} {:>6}  UNMEASURED (corpus/ACCURACY)",
                c.stem, "-", "-"
            );
            continue;
        }
        let cap = match entry {
            Some(Entry::Cap(cap)) => *cap,
            _ => DEFAULT_CAP,
        };
        match measure(&c.path) {
            Measured::Margin(m) => {
                measured += 1;
                sum += m;
                let verdict = if m > cap {
                    ok = false;
                    "FAIL: over its cap"
                } else if entry.is_some() && m <= DEFAULT_CAP {
                    "ok (its corpus/ACCURACY entry can go)"
                } else if entry.is_some() && m < 0.5 * cap {
                    "ok (the cap can come down)"
                } else {
                    "ok"
                };
                println!("{:<14} {m:>8.3} {cap:>6.2}  {verdict}", c.stem);
            }
            Measured::Unconverged(conv) => {
                ok = false;
                println!(
                    "{:<14} {:>8} {cap:>6.2}  FAIL: the reference has not converged \
                     (TMAX/{} vs TMAX/{REFINE}: {conv:.3}); list the deck as \
                     `unmeasured` in corpus/ACCURACY",
                    c.stem,
                    "-",
                    REFINE / 2.0
                );
            }
            Measured::Failed(e) => {
                ok = false;
                println!("{:<14} {:>8} {cap:>6.2}  FAIL: {e}", c.stem, "-");
            }
        }
    }
    for stem in registry.keys() {
        if !corpus.circuits.iter().any(|c| &c.stem == stem) {
            ok = false;
            println!("{stem:<14} FAIL: listed in corpus/ACCURACY but not in the corpus");
        }
    }
    println!("\n{measured} transient deck(s) measured, margins sum to {sum:.3}");
    Ok(ok)
}
