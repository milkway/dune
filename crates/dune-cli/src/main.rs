//! The `dune` batch-mode simulator, flag-compatible with `spice3 -b`:
//! `dune -b input.cir -r output.raw`. Phase 1 runs `.op`; the other
//! analyses arrive with their phases (PLAN.md §3).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use dune_core::{Circuit, Options};
use dune_netlist::Severity;
use dune_raw::{Format, Rawfile};

const USAGE: &str = "usage: dune -b INPUT.cir [-r OUTPUT.raw]";

fn main() -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut rawfile: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-b" => {} // batch is the only mode there is, so far
            "-r" => {
                rawfile = Some(PathBuf::from(args.next().context("-r needs a file name")?));
            }
            flag if flag.starts_with('-') => bail!("unknown flag `{flag}`\n{USAGE}"),
            _ => {
                if input.replace(PathBuf::from(&a)).is_some() {
                    bail!("more than one input file\n{USAGE}");
                }
            }
        }
    }
    let Some(input) = input else {
        bail!("no input file\n{USAGE}");
    };

    // Parse; diagnostics go to stderr, errors are fatal like the
    // 3f5's deck errors.
    let parsed = dune_netlist::parse_file(&input)?;
    let mut fatal = false;
    for d in &parsed.diags {
        eprintln!("{}", d.render(&parsed.sources));
        fatal |= d.severity == Severity::Error;
    }
    if fatal {
        bail!("deck has errors");
    }

    let (circuit, notes) = Circuit::from_netlist(&parsed.netlist)
        .map_err(|e| anyhow::anyhow!("{}: {e}", input.display()))?;
    for n in &notes {
        eprintln!("note: {n}");
    }
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    for d in &opt_diags {
        eprintln!("{}", d.render(&parsed.sources));
    }

    let mut plots = Vec::new();
    for analysis in &parsed.netlist.analyses {
        match analysis {
            dune_netlist::ir::Analysis::Op { .. } => {
                let op = circuit.op(&options)?;
                for n in &op.notes {
                    eprintln!("note: {n}");
                }
                plots.push(op.solution.to_plot(&parsed.netlist.title, ""));
            }
            dune_netlist::ir::Analysis::Dc { sweeps, .. } => {
                let sweeps: Vec<dune_core::DcSweep> = sweeps
                    .iter()
                    .map(|s| dune_core::DcSweep {
                        src: s.src.clone(),
                        start: s.start,
                        stop: s.stop,
                        step: s.step,
                    })
                    .collect();
                let dc = circuit.dc(&options, &sweeps)?;
                for n in &dc.notes {
                    eprintln!("note: {n}");
                }
                plots.push(dc.solution.to_plot(&parsed.netlist.title, ""));
            }
            dune_netlist::ir::Analysis::Tran {
                tstep,
                tstop,
                tstart,
                tmax,
                uic,
                ..
            } => {
                let tr =
                    circuit.tran(&options, *tstep, *tstop, tstart.unwrap_or(0.0), *tmax, *uic)?;
                for n in &tr.notes {
                    eprintln!("note: {n}");
                }
                plots.push(tr.solution.to_plot(&parsed.netlist.title, ""));
            }
            other => bail!("analysis {other:?} is not implemented yet (PLAN.md §3)"),
        }
    }
    if plots.is_empty() {
        eprintln!("note: no analyses in the deck; nothing to do");
        return Ok(());
    }

    let raw = Rawfile {
        plots,
        warnings: Vec::new(),
    };
    match rawfile {
        Some(path) => {
            let mut f = std::fs::File::create(&path)
                .with_context(|| format!("creating {}", path.display()))?;
            // Binary is the 3f5's default rawfile format.
            raw.write(&mut f, Format::Binary)?;
        }
        None => {
            // Plain value listing until the spice3 .print formats
            // land (PLAN Phase 1 leftover, see docs/dune-core.md).
            for plot in &raw.plots {
                println!("{}", plot.name);
                for (i, v) in plot.variables.iter().enumerate() {
                    if let dune_raw::Data::Real(cols) = &plot.data {
                        println!("{:<24} = {:.6e}", v.name, cols[i][0]);
                    }
                }
            }
        }
    }
    Ok(())
}
