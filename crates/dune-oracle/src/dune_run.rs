//! Running Dune itself as the candidate side of a comparison
//! (library call, no subprocess). Phase 1: decks whose only analysis
//! is `.op`.

use std::path::Path;

use dune_core::{Circuit, Options};
use dune_netlist::Severity;
use dune_raw::Rawfile;

use crate::corpus::Circuit as CorpusCircuit;

/// Can Dune simulate everything this deck asks for? Both the
/// analyses and the device letters must be implemented (PLAN.md §3;
/// `x` is fine because subcircuits expand to their contents, which
/// the scan also sees).
pub fn supported(c: &CorpusCircuit) -> bool {
    const ANALYSES: &[&str] = &[
        ".op", ".dc", ".tran", ".ac", ".pz", ".noise", ".disto", ".tf", ".sens",
    ];
    const DEVICES: &[char] = &[
        'r', 'c', 'l', 'k', 'v', 'i', 'e', 'f', 'g', 'h', 'd', 'q', 'j', 'z', 'm', 'o', 's', 'w',
        't', 'x',
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
        let (solutions, _notes) = circuit.run(analysis, &options).map_err(|e| e.to_string())?;
        plots.extend(
            solutions
                .iter()
                .map(|s| s.to_plot(&parsed.netlist.title, "")),
        );
    }
    if plots.is_empty() {
        return Err("deck has no analyses".into());
    }
    Ok(Rawfile {
        plots,
        warnings: Vec::new(),
    })
}
