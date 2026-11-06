//! Running Dune itself as the candidate side of a comparison
//! (library call, no subprocess). Phase 1: decks whose only analysis
//! is `.op`.

use std::path::Path;

use dune_core::{Circuit, Options};
use dune_netlist::Severity;
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;

use crate::corpus::Circuit as CorpusCircuit;

/// Can Dune simulate everything this deck asks for? Both the
/// analyses and the device letters must be implemented (PLAN.md §3;
/// `x` is fine because subcircuits expand to their contents, which
/// the scan also sees).
pub fn supported(c: &CorpusCircuit) -> bool {
    const ANALYSES: &[&str] = &[".op", ".dc", ".tran", ".ac", ".pz", ".noise", ".disto"];
    const DEVICES: &[char] = &[
        'r', 'c', 'l', 'k', 'v', 'i', 'e', 'f', 'g', 'h', 'd', 'q', 'j', 'z', 'm', 'o', 's', 'w',
        'x',
    ];
    // MOS levels implemented so far.
    const MOS_LEVELS: &[u32] = &[1, 2, 3, 4, 5, 6];
    !c.analyses.is_empty()
        && c.analyses.iter().all(|a| ANALYSES.contains(&a.as_str()))
        && c.devices.iter().all(|d| DEVICES.contains(d))
        && c.mos_levels.iter().all(|l| MOS_LEVELS.contains(l))
}

/// Parse, elaborate and run every analysis of the deck, returning a
/// rawfile-shaped result. Any diagnostic error or engine failure is
/// a candidate failure, reported as text.
pub fn run(path: &Path) -> Result<Rawfile, String> {
    let parsed = dune_netlist::parse_file(path).map_err(|e| e.to_string())?;
    let errors: Vec<String> = parsed
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&parsed.sources))
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let (circuit, _notes) = Circuit::from_netlist(&parsed.netlist).map_err(|e| e.to_string())?;
    let mut options = Options::default();
    let mut opt_diags = Vec::new();
    options.apply(&parsed.netlist.options, &mut opt_diags);

    let mut plots = Vec::new();
    for analysis in &parsed.netlist.analyses {
        match analysis {
            Analysis::Op { .. } => {
                let op = circuit.op(&options).map_err(|e| e.to_string())?;
                plots.push(op.solution.to_plot(&parsed.netlist.title, ""));
            }
            Analysis::Dc { sweeps, .. } => {
                let sweeps: Vec<dune_core::DcSweep> = sweeps
                    .iter()
                    .map(|s| dune_core::DcSweep {
                        src: s.src.clone(),
                        start: s.start,
                        stop: s.stop,
                        step: s.step,
                    })
                    .collect();
                let dc = circuit.dc(&options, &sweeps).map_err(|e| e.to_string())?;
                plots.push(dc.solution.to_plot(&parsed.netlist.title, ""));
            }
            Analysis::Tran {
                tstep,
                tstop,
                tstart,
                tmax,
                uic,
                ..
            } => {
                let tr = circuit
                    .tran(&options, *tstep, *tstop, tstart.unwrap_or(0.0), *tmax, *uic)
                    .map_err(|e| e.to_string())?;
                plots.push(tr.solution.to_plot(&parsed.netlist.title, ""));
            }
            Analysis::Ac {
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
                let ac = circuit
                    .ac(&options, sweep, *points as usize, *fstart, *fstop)
                    .map_err(|e| e.to_string())?;
                plots.push(ac.solution.to_plot(&parsed.netlist.title, ""));
            }
            Analysis::Pz {
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
                let pz = circuit
                    .pz(
                        &options,
                        [
                            nodes[0].as_str(),
                            nodes[1].as_str(),
                            nodes[2].as_str(),
                            nodes[3].as_str(),
                        ],
                        input,
                        which,
                    )
                    .map_err(|e| e.to_string())?;
                plots.push(pz.solution.to_plot(&parsed.netlist.title, ""));
            }
            Analysis::Noise {
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
                let no = circuit
                    .noise(
                        &options,
                        output,
                        output_ref.as_deref(),
                        src,
                        sweep,
                        *points as usize,
                        *fstart,
                        *fstop,
                        pts_per_summary.map(|p| p as usize),
                    )
                    .map_err(|e| e.to_string())?;
                plots.push(no.density.to_plot(&parsed.netlist.title, ""));
                if let Some(integ) = no.integrated {
                    plots.push(integ.to_plot(&parsed.netlist.title, ""));
                }
            }
            Analysis::Disto {
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
                let di = circuit
                    .disto(
                        &options,
                        sweep,
                        *points as usize,
                        *fstart,
                        *fstop,
                        *f2overf1,
                    )
                    .map_err(|e| e.to_string())?;
                for sol in di.solutions {
                    plots.push(sol.to_plot(&parsed.netlist.title, ""));
                }
            }
            other => return Err(format!("analysis {other:?} not supported yet")),
        }
    }
    if plots.is_empty() {
        return Err("deck has no analyses".into());
    }
    Ok(Rawfile {
        plots,
        warnings: Vec::new(),
    })
}
