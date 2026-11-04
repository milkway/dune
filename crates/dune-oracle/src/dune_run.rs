//! Running Dune itself as the candidate side of a comparison
//! (library call, no subprocess). Phase 1: decks whose only analysis
//! is `.op`.

use std::path::Path;

use dune_core::{Circuit, Options};
use dune_netlist::Severity;
use dune_netlist::ir::Analysis;
use dune_raw::Rawfile;

use crate::corpus::Circuit as CorpusCircuit;

/// Can Dune simulate everything this deck asks for?
pub fn supported(c: &CorpusCircuit) -> bool {
    !c.analyses.is_empty() && c.analyses.iter().all(|a| a == ".op")
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
