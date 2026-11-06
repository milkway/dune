//! The `dune` batch-mode simulator, flag-compatible with `spice3 -b`:
//! `dune -b input.cir [-r output.raw]`. With `-r` the results go to
//! a binary SPICE3 rawfile and `.print` cards are skipped, like the
//! 3f5; without it, `.print` cards render in the spice3 column
//! format (`print.rs`). `.op`, `.dc` and `.tran` run; the others
//! arrive with their phases (PLAN.md §3).

#![forbid(unsafe_code)]

mod print;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use dune_core::{Circuit, Options, OutExpr, Solution};
use dune_netlist::Severity;
use dune_netlist::ir::{Output, OutputKind};
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

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &parsed.netlist.analyses {
        match analysis {
            dune_netlist::ir::Analysis::Op { .. } => {
                let op = circuit.op(&options)?;
                for n in &op.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(op.solution);
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
                solutions.push(dc.solution);
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
                solutions.push(tr.solution);
            }
            dune_netlist::ir::Analysis::Ac {
                variation,
                points,
                fstart,
                fstop,
                ..
            } => {
                let sweep = match variation {
                    dune_netlist::ir::Variation::Dec => dune_core::AcSweep::Decade,
                    dune_netlist::ir::Variation::Oct => dune_core::AcSweep::Octave,
                    dune_netlist::ir::Variation::Lin => dune_core::AcSweep::Linear,
                };
                let ac = circuit.ac(&options, sweep, *points as usize, *fstart, *fstop)?;
                for n in &ac.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(ac.solution);
            }
            dune_netlist::ir::Analysis::Pz {
                nodes,
                input,
                which,
                ..
            } => {
                let input = match input {
                    dune_netlist::ir::PzInput::Current => dune_core::PzInput::Current,
                    dune_netlist::ir::PzInput::Voltage => dune_core::PzInput::Voltage,
                };
                let which = match which {
                    dune_netlist::ir::PzWhich::Poles => dune_core::PzWhich::Poles,
                    dune_netlist::ir::PzWhich::Zeros => dune_core::PzWhich::Zeros,
                    dune_netlist::ir::PzWhich::Both => dune_core::PzWhich::Both,
                };
                let pz = circuit.pz(
                    &options,
                    [
                        nodes[0].as_str(),
                        nodes[1].as_str(),
                        nodes[2].as_str(),
                        nodes[3].as_str(),
                    ],
                    input,
                    which,
                )?;
                for n in &pz.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(pz.solution);
            }
            dune_netlist::ir::Analysis::Noise {
                output,
                output_ref,
                src,
                variation,
                points,
                fstart,
                fstop,
                pts_per_summary,
                ..
            } => {
                let sweep = match variation {
                    dune_netlist::ir::Variation::Dec => dune_core::AcSweep::Decade,
                    dune_netlist::ir::Variation::Oct => dune_core::AcSweep::Octave,
                    dune_netlist::ir::Variation::Lin => dune_core::AcSweep::Linear,
                };
                let no = circuit.noise(
                    &options,
                    output,
                    output_ref.as_deref(),
                    src,
                    sweep,
                    *points as usize,
                    *fstart,
                    *fstop,
                    pts_per_summary.map(|p| p as usize),
                )?;
                for n in &no.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(no.density);
                if let Some(integ) = no.integrated {
                    solutions.push(integ);
                }
            }
            dune_netlist::ir::Analysis::Disto {
                variation,
                points,
                fstart,
                fstop,
                f2overf1,
                ..
            } => {
                let sweep = match variation {
                    dune_netlist::ir::Variation::Dec => dune_core::AcSweep::Decade,
                    dune_netlist::ir::Variation::Oct => dune_core::AcSweep::Octave,
                    dune_netlist::ir::Variation::Lin => dune_core::AcSweep::Linear,
                };
                let di = circuit.disto(
                    &options,
                    sweep,
                    *points as usize,
                    *fstart,
                    *fstop,
                    *f2overf1,
                )?;
                for n in &di.notes {
                    eprintln!("note: {n}");
                }
                solutions.extend(di.solutions);
            }
            dune_netlist::ir::Analysis::Tf { outvar, insrc, .. } => {
                let tf = circuit.tf(&options, &output_var(outvar)?, insrc)?;
                for n in &tf.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(tf.solution);
            }
            dune_netlist::ir::Analysis::Sens {
                outvar,
                filters,
                ac,
                ..
            } => {
                let ac = ac.map(|(variation, points, fstart, fstop)| dune_core::SensAc {
                    sweep: match variation {
                        dune_netlist::ir::Variation::Dec => dune_core::AcSweep::Decade,
                        dune_netlist::ir::Variation::Oct => dune_core::AcSweep::Octave,
                        dune_netlist::ir::Variation::Lin => dune_core::AcSweep::Linear,
                    },
                    points: points as usize,
                    fstart,
                    fstop,
                });
                let sens = circuit.sens(&options, &output_var(outvar)?, filters, ac)?;
                for n in &sens.notes {
                    eprintln!("note: {n}");
                }
                solutions.push(sens.solution);
            }
        }
    }
    if solutions.is_empty() {
        eprintln!("note: no analyses in the deck; nothing to do");
        return Ok(());
    }

    match rawfile {
        Some(path) => {
            // Rawfile mode: like `spice3 -b -r`, everything goes to
            // the rawfile and `.print` cards are not rendered.
            let raw = Rawfile {
                plots: solutions
                    .iter()
                    .map(|s| s.to_plot(&parsed.netlist.title, ""))
                    .collect(),
                warnings: Vec::new(),
            };
            let mut f = std::fs::File::create(&path)
                .with_context(|| format!("creating {}", path.display()))?;
            // Binary is the 3f5's default rawfile format.
            raw.write(&mut f, Format::Binary)?;
        }
        None => {
            run_prints(&parsed.netlist, &solutions, &options)?;
        }
    }
    Ok(())
}

/// Render the deck's `.print` cards in the spice3 column format;
/// fall back to a bare value listing when the deck has none (so an
/// `.op`-only deck still shows its numbers).
fn run_prints(
    nl: &dune_netlist::ir::Netlist,
    solutions: &[Solution],
    options: &Options,
) -> Result<()> {
    let page = print::PageSetup {
        width: nl.width_out.unwrap_or(print::DEF_WIDTH),
        height: print::DEF_HEIGHT,
        nobreak: options.nopage,
    };
    let mut printed = false;
    for output in &nl.outputs {
        let Output {
            kind,
            analysis,
            vars,
            ..
        } = output;
        match kind {
            OutputKind::Print => {}
            OutputKind::Plot => {
                eprintln!("note: .plot (ascii graphs) is not implemented yet; use .print");
                continue;
            }
            // `.save` narrows what gets saved; Dune currently saves
            // everything. `.four` waits for its analysis.
            _ => continue,
        }
        let plotname = match analysis.as_deref() {
            Some("tran") => "Transient Analysis",
            Some("dc") => "DC transfer characteristic",
            Some("ac") => "AC Analysis",
            Some(other) => {
                eprintln!("note: .print {other}: analysis not implemented yet");
                continue;
            }
            None => continue,
        };
        let Some(solution) = solutions.iter().find(|s| s.plotname == plotname) else {
            eprintln!(
                "note: .print {}: no such analysis in this run",
                analysis.as_deref().unwrap_or("")
            );
            continue;
        };
        let exprs: Vec<OutExpr> = vars.iter().map(OutExpr::from_ir).collect();
        match solution.select(&exprs) {
            Ok(sel) => {
                let mut out = String::new();
                print::print_columns(&mut out, &nl.title, &solution.plotname, "", &sel, &page);
                print!("{out}");
                printed = true;
            }
            Err(e) => eprintln!("warning: .print: {e}"),
        }
    }
    if !printed {
        for s in solutions {
            if s.num_points() == 1 {
                println!("{}", s.plotname);
                for (name, _) in &s.variables {
                    if let Some(col) = s.column(name) {
                        println!("{:<24} = {}", name, dune_raw::fmt_e(col[0], 6));
                    }
                }
            }
        }
    }
    Ok(())
}

/// The `.tf`/`.sens` output variable in the engine's form.
fn output_var(v: &dune_netlist::ir::OutVar) -> Result<dune_core::OutputVar> {
    use dune_netlist::ir::OutVar;
    match v {
        OutVar::Voltage {
            node, reference, ..
        } => Ok(dune_core::OutputVar::Voltage {
            node: node.clone(),
            reference: reference.clone(),
        }),
        OutVar::Current { vsrc, .. } => Ok(dune_core::OutputVar::Current { vsrc: vsrc.clone() }),
        OutVar::Raw(r) => bail!("`{r}` is not an output variable"),
    }
}
