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
//!   [`REFINE`] (and by twice that, up to [`REFINE_MAX`], while it
//!   has not converged), run by Dune itself. That a refined Dune run is the
//!   refined ngspice run is what the golden check establishes, and
//!   the converged companions of the XFAIL decks gate it directly
//!   (`regress/*_fine`, `*_gear`); the survey behind this ruler
//!   measured it on the whole corpus (docs/dune-oracle.md);
//! - the reference is trusted only if it has itself converged: the
//!   run at half the refinement must agree with it within
//!   [`CONVERGED`] of the ruler, the two compared at the candidate's
//!   time resolution — or, failing that, the margin must be the
//!   same within [`STABLE`] against either of them;
//! - both comparisons give an event its relative accuracy in time:
//!   a jump two runs place within `RELTOL·t` of each other is the
//!   same answer ([`Ruler`]). A deck where it has not is not
//!   measurable this way (trapezoidal ringing that the step does not
//!   damp, a breakpoint cascade) and must be listed as such in
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

use crate::compare::{Report, Ruler, Tolerances, compare_transients};
use crate::corpus::Corpus;
use crate::dune_run;

/// `TMAX` divisor of the first reference run.
const REFINE: f64 = 40.0;
/// The reference is refined by twos up to this divisor while it has
/// not converged (`ltra_2`'s 326 lumped nodes need 80).
const REFINE_MAX: f64 = 160.0;
/// The reference counts as converged when the run at `REFINE / 2`
/// is within this fraction of the ruler of it.
const CONVERGED: f64 = 0.2;
/// ...or when the margin moves by no more than this fraction of
/// itself between the two refinements as reference.
const STABLE: f64 = 0.1;
/// Cap of a deck `corpus/ACCURACY` does not list.
const DEFAULT_CAP: f64 = 0.2;

/// One `corpus/ACCURACY` line.
enum Entry {
    /// `stem|<cap>|reason`: the deck's margin may reach `cap`.
    Cap(f64),
    /// `stem|unmeasured|reason`: no converged reference; not run.
    Unmeasured,
    /// `stem|slow <cap>|reason`: measured, and held to `cap`, only
    /// when the deck is named on the command line — its reference
    /// takes minutes (`ltra_3` at TMAX/80: 7).
    Slow(f64),
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
            bail!(
                "corpus/ACCURACY: bad line `{line}` (want stem|cap, `slow cap` or unmeasured|reason)"
            );
        };
        let cap = |text: &str| {
            text.parse()
                .ok()
                .filter(|c: &f64| c.is_finite() && *c > 0.0)
                .with_context(|| format!("corpus/ACCURACY: bad cap in `{line}`"))
        };
        let entry = if what == "unmeasured" {
            Entry::Unmeasured
        } else if let Some(text) = what.strip_prefix("slow ") {
            Entry::Slow(cap(text)?)
        } else {
            Entry::Cap(cap(what)?)
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
    let saved = saved_vectors(&parsed.netlist);
    for plot in &mut plots {
        keep_vectors(plot, |name| match &saved {
            Some(saved) => saved.iter().any(|s| s == name),
            None => top_level(name),
        });
    }
    Ok(Rawfile {
        plots,
        warnings: Vec::new(),
    })
}

/// The vectors a deck asks for with `.save` (lower case, `v(n)` and
/// `i(vsrc)`); `None` when it has no `.save` or one of them is not a
/// plain node voltage or source current (`all`, a differential
/// `v(a,b)`): the deck is then measured on its top level
/// ([`top_level`]).
///
/// A deck that names its outputs is measured on them. Dune writes
/// every node whatever `.save` says, and the internal nodes of
/// `ltra_1`'s diode loads snap in under a picosecond: two refinements
/// never agree there, and the deck could not be measured at all.
fn saved_vectors(netlist: &dune_netlist::ir::Netlist) -> Option<Vec<String>> {
    use dune_netlist::ir::{OutVar, OutputKind};
    let mut names = Vec::new();
    for out in netlist
        .outputs
        .iter()
        .filter(|o| o.kind == OutputKind::Save)
    {
        for var in &out.vars {
            names.push(match var {
                OutVar::Voltage {
                    node,
                    reference: None,
                    suffix: None,
                } => format!("v({})", node.to_ascii_lowercase()),
                OutVar::Current { vsrc, suffix: None } => {
                    format!("i({})", vsrc.to_ascii_lowercase())
                }
                _ => return None,
            });
        }
    }
    (!names.is_empty()).then_some(names)
}

/// Whether a vector belongs to the deck's top level: not a node
/// inside a subcircuit instance (`v(x3.4)`) nor a device's internal
/// node (`v(o1#i1)`).
///
/// A deck that saves nothing in particular is measured on these. A
/// subcircuit's internal nodes are how the block is built, not what
/// the deck is about, and they are where a model is least well
/// behaved: the diode loads of `ltra_2`/`ltra_3` (`TT`, no `CJO`)
/// jump tens of millivolts in under a picosecond inside `x3`, which
/// no run places twice at the same femtosecond — on every node, the
/// decks measured 1.26 and 3.32 rulers, all of it there. A flat deck
/// has no such nodes and is measured as before.
fn top_level(name: &str) -> bool {
    !name.contains(['.', '#'])
}

/// Drop every vector of `plot` but its scale and the ones `keep`
/// accepts (by lower-case name).
fn keep_vectors(plot: &mut dune_raw::Plot, keep: impl Fn(&str) -> bool) {
    let wanted: Vec<bool> = plot
        .variables
        .iter()
        .enumerate()
        .map(|(i, v)| i == 0 || keep(&v.name.to_ascii_lowercase()))
        .collect();
    let mut it = wanted.iter();
    plot.variables.retain(|_| *it.next().unwrap());
    if let dune_raw::Data::Real(cols) = &mut plot.data {
        let mut it = wanted.iter();
        cols.retain(|_| *it.next().unwrap());
    }
}

/// The comparator's worst ratio over the plots, `driver`'s grid
/// driving; `Err` on a structural mismatch.
fn margin(driver: &Rawfile, other: &Rawfile) -> Result<f64, String> {
    worst(compare_transients(
        driver,
        other,
        &Ruler { lens: None },
        &Tolerances::default(),
    ))
}

/// How far the half-refined run is from the reference at the
/// candidate's resolution (`compare_transients_through`).
fn convergence(half: &Rawfile, reference: &Rawfile, cand: &Rawfile) -> Result<f64, String> {
    worst(compare_transients(
        half,
        reference,
        &Ruler { lens: Some(cand) },
        &Tolerances::default(),
    ))
}

fn worst(report: Report) -> Result<f64, String> {
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
    /// The margin, and the candidate's timepoints (what it cost).
    Margin(f64, usize),
    /// The reference disagrees with the half-refined run by this
    /// much, against a margin of that much, at this `TMAX` divisor.
    Unconverged(f64, f64, f64),
    Failed(String),
}

fn measure(path: &Path) -> Measured {
    let run = |refine| run_tran(path, refine);
    let (cand, mut half) = match (run(None), run(Some(REFINE / 2.0))) {
        (Ok(c), Ok(h)) => (c, h),
        (Err(e), _) | (_, Err(e)) => return Measured::Failed(e),
    };
    let points = cand.plots.iter().map(|p| p.num_points()).sum();
    // Refine until two successive references agree, or the margin
    // does not depend on which of them it is taken against.
    let mut refine = REFINE;
    loop {
        let reference = match run(Some(refine)) {
            Ok(r) => r,
            Err(e) => return Measured::Failed(e),
        };
        let conv = match convergence(&half, &reference, &cand) {
            Err(e) => return Measured::Failed(e),
            Ok(conv) => conv,
        };
        let m = match margin(&cand, &reference) {
            Ok(m) => m,
            Err(e) => return Measured::Failed(e),
        };
        if conv <= CONVERGED {
            return Measured::Margin(m, points);
        }
        // The two refinements differ somewhere (a snap leaves a spike
        // in one of them), but the measurement may still not depend
        // on which of them is the reference.
        match margin(&cand, &half) {
            Ok(mh) if (mh - m).abs() <= STABLE * m => return Measured::Margin(m, points),
            Ok(_) => {}
            Err(e) => return Measured::Failed(e),
        }
        if refine >= REFINE_MAX {
            return Measured::Unconverged(conv, m, refine);
        }
        half = reference;
        refine *= 2.0;
    }
}

/// `dune-oracle --accuracy [NAME..]`: measure every transient deck
/// (or the named ones) and hold each to its cap.
pub fn run(args: &[String]) -> Result<bool> {
    let corpus = Corpus::load()?;
    let registry = read_registry(&corpus.root)?;
    let mut ok = true;
    let (mut measured, mut sum, mut total) = (0usize, 0.0f64, 0usize);
    println!(
        "{:<14} {:>8} {:>6} {:>7}  result",
        "circuit", "margin", "cap", "points"
    );
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
        if matches!(entry, Some(Entry::Slow(_))) && args.is_empty() {
            println!(
                "{:<14} {:>8} {:>6} {:>7}  SLOW (corpus/ACCURACY; run it by name)",
                c.stem, "-", "-", "-"
            );
            continue;
        }
        let cap = match entry {
            Some(Entry::Cap(cap) | Entry::Slow(cap)) => *cap,
            _ => DEFAULT_CAP,
        };
        match measure(&c.path) {
            Measured::Margin(m, points) => {
                measured += 1;
                sum += m;
                total += points;
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
                println!("{:<14} {m:>8.3} {cap:>6.2} {points:>7}  {verdict}", c.stem);
            }
            Measured::Unconverged(conv, m, refine) => {
                ok = false;
                println!(
                    "{:<14} {:>8} {cap:>6.2}  FAIL: the reference has not converged \
                     (TMAX/{} vs TMAX/{refine}: {conv:.3}, against a margin of {m:.3}); list the deck as \
                     `unmeasured` in corpus/ACCURACY",
                    c.stem,
                    "-",
                    refine / 2.0
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
    println!(
        "\n{measured} transient deck(s) measured, margins sum to {sum:.3}, {total} timepoints"
    );
    Ok(ok)
}
