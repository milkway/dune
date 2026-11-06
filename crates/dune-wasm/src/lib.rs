//! WebAssembly build of the Dune simulator (PLAN.md §4), the engine
//! behind the Studio's "Simulate" panel.
//!
//! The JS surface is one function, `simulate(netlist, opts?)`: it
//! parses the deck with `dune-netlist`, builds it with
//! [`Circuit::from_netlist`], runs every analysis card in deck order
//! (exactly what `dune -b` runs) and returns the plots as a JS object
//! of shape [`RawfileJson`]. On failure it throws a JS `Error` whose
//! `message` is the engine's or parser's message, with `kind` and
//! `diagnostics` properties as in [`SimError`].
//!
//! Native builds (the `rlib`) expose the same path as
//! [`simulate_json`]/[`run`], which is what the tests exercise.
//!
//! The deck is hostile input: nothing here panics on a malformed
//! deck (the parser's no-panic contract carries over; every engine
//! failure is an `Err`). A long-running deck, however, blocks the
//! calling thread until it finishes — call it from a Web Worker.

mod schema;

pub use schema::{
    Columns, DiagnosticJson, PlotJson, RawfileJson, SimError, SimOptions, VariableJson,
};

use std::collections::BTreeMap;
use std::path::Path;

use dune_core::{Circuit, Options, Solution};
use dune_netlist::ir::{Analysis, OutVar, PzInput, PzWhich, Variation};
use dune_netlist::{IncludeLoader, Severity};
use wasm_bindgen::prelude::*;

/// File name the deck is parsed under; `.include` paths resolve
/// relative to it (i.e. as given).
pub const DECK_NAME: &str = "deck.cir";

/// Run a deck and return the result as JSON text ([`RawfileJson`]).
/// `opts_json` is a [`SimOptions`] object, or `None` for defaults.
pub fn simulate_json(netlist: &str, opts_json: Option<&str>) -> Result<String, SimError> {
    let opts = match opts_json {
        Some(s) => {
            let bad = |e: serde_json::Error| SimError::new("options", format!("bad options: {e}"));
            // An object only: serde would also accept a JSON array
            // as a (positional) struct.
            let v: serde_json::Value = serde_json::from_str(s).map_err(bad)?;
            if !v.is_object() {
                return Err(SimError::new("options", "bad options: expected an object"));
            }
            serde_json::from_value::<SimOptions>(v).map_err(bad)?
        }
        None => SimOptions::default(),
    };
    let raw = run(netlist, &opts)?;
    serde_json::to_string(&raw).map_err(|e| SimError::new("analysis", e.to_string()))
}

/// `.include` from the in-memory map of [`SimOptions::includes`].
struct MapLoader<'a>(&'a BTreeMap<String, String>);

impl IncludeLoader for MapLoader<'_> {
    fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
        let key = path.to_string_lossy();
        let text = self
            .0
            .get(key.as_ref())
            .ok_or_else(|| "not among the supplied includes".to_string())?;
        if text.len() as u64 > limit {
            return Err(format!(
                "larger than the deck's remaining {limit}-byte budget"
            ));
        }
        Ok(text.clone())
    }
}

/// Parse, elaborate and run every analysis of `netlist`.
///
/// The dispatch mirrors `crates/dune-cli/src/main.rs` card for card
/// (same engine calls, same plot order, title and empty date as its
/// rawfile); keep the two in step.
pub fn run(netlist: &str, opts: &SimOptions) -> Result<RawfileJson, SimError> {
    let parsed = dune_netlist::parse_with_loader(
        Path::new(DECK_NAME),
        netlist.to_string(),
        &mut MapLoader(&opts.includes),
    );
    let mut diagnostics: Vec<DiagnosticJson> = parsed
        .diags
        .iter()
        .map(|d| DiagnosticJson::from_diag(d, &parsed.sources))
        .collect();
    if let Some(first) = parsed.diags.iter().find(|d| d.severity == Severity::Error) {
        return Err(SimError {
            kind: "parse".into(),
            message: first.render(&parsed.sources),
            diagnostics,
        });
    }

    let (circuit, mut notes) = Circuit::from_netlist(&parsed.netlist)
        .map_err(|e| SimError::new("elaborate", e.to_string()))?;
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(
        opt_diags
            .iter()
            .map(|d| DiagnosticJson::from_diag(d, &parsed.sources)),
    );

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &parsed.netlist.analyses {
        run_analysis(&circuit, &options, analysis, &mut solutions, &mut notes)
            .map_err(|e| SimError::new("analysis", e))?;
    }
    if solutions.is_empty() {
        notes.push("no analyses in the deck; nothing to do".into());
    }

    let title = &parsed.netlist.title;
    Ok(RawfileJson {
        plots: solutions
            .iter()
            .map(|s| PlotJson::from(&s.to_plot(title, "")))
            .collect(),
        notes,
        diagnostics,
    })
}

fn sweep(v: &Variation) -> dune_core::AcSweep {
    match v {
        Variation::Dec => dune_core::AcSweep::Decade,
        Variation::Oct => dune_core::AcSweep::Octave,
        Variation::Lin => dune_core::AcSweep::Linear,
    }
}

/// One analysis card, as `dune-cli` runs it.
fn run_analysis(
    circuit: &Circuit,
    options: &Options,
    analysis: &Analysis,
    solutions: &mut Vec<Solution>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    let e = |e: dune_core::Error| e.to_string();
    match analysis {
        Analysis::Op { .. } => {
            let op = circuit.op(options).map_err(e)?;
            notes.extend(op.notes);
            solutions.push(op.solution);
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
            let dc = circuit.dc(options, &sweeps).map_err(e)?;
            notes.extend(dc.notes);
            solutions.push(dc.solution);
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
                .tran(options, *tstep, *tstop, tstart.unwrap_or(0.0), *tmax, *uic)
                .map_err(e)?;
            notes.extend(tr.notes);
            solutions.push(tr.solution);
        }
        Analysis::Ac {
            variation,
            points,
            fstart,
            fstop,
            ..
        } => {
            let ac = circuit
                .ac(options, sweep(variation), *points as usize, *fstart, *fstop)
                .map_err(e)?;
            notes.extend(ac.notes);
            solutions.push(ac.solution);
        }
        Analysis::Pz {
            nodes,
            input,
            which,
            ..
        } => {
            let input = match input {
                PzInput::Current => dune_core::PzInput::Current,
                PzInput::Voltage => dune_core::PzInput::Voltage,
            };
            let which = match which {
                PzWhich::Poles => dune_core::PzWhich::Poles,
                PzWhich::Zeros => dune_core::PzWhich::Zeros,
                PzWhich::Both => dune_core::PzWhich::Both,
            };
            let pz = circuit
                .pz(
                    options,
                    [
                        nodes[0].as_str(),
                        nodes[1].as_str(),
                        nodes[2].as_str(),
                        nodes[3].as_str(),
                    ],
                    input,
                    which,
                )
                .map_err(e)?;
            notes.extend(pz.notes);
            solutions.push(pz.solution);
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
            let no = circuit
                .noise(
                    options,
                    output,
                    output_ref.as_deref(),
                    src,
                    sweep(variation),
                    *points as usize,
                    *fstart,
                    *fstop,
                    pts_per_summary.map(|p| p as usize),
                )
                .map_err(e)?;
            notes.extend(no.notes);
            solutions.push(no.density);
            if let Some(integ) = no.integrated {
                solutions.push(integ);
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
            let di = circuit
                .disto(
                    options,
                    sweep(variation),
                    *points as usize,
                    *fstart,
                    *fstop,
                    *f2overf1,
                )
                .map_err(e)?;
            notes.extend(di.notes);
            solutions.extend(di.solutions);
        }
        Analysis::Tf { outvar, insrc, .. } => {
            let tf = circuit
                .tf(options, &output_var(outvar)?, insrc)
                .map_err(e)?;
            notes.extend(tf.notes);
            solutions.push(tf.solution);
        }
        Analysis::Sens {
            outvar,
            filters,
            ac,
            ..
        } => {
            let ac = ac.map(|(variation, points, fstart, fstop)| dune_core::SensAc {
                sweep: sweep(&variation),
                points: points as usize,
                fstart,
                fstop,
            });
            let sens = circuit
                .sens(options, &output_var(outvar)?, filters, ac)
                .map_err(e)?;
            notes.extend(sens.notes);
            solutions.push(sens.solution);
        }
    }
    Ok(())
}

/// The `.tf`/`.sens` output variable in the engine's form.
fn output_var(v: &OutVar) -> Result<dune_core::OutputVar, String> {
    match v {
        OutVar::Voltage {
            node, reference, ..
        } => Ok(dune_core::OutputVar::Voltage {
            node: node.clone(),
            reference: reference.clone(),
        }),
        OutVar::Current { vsrc, .. } => Ok(dune_core::OutputVar::Current { vsrc: vsrc.clone() }),
        OutVar::Raw(r) => Err(format!("`{r}` is not an output variable")),
    }
}

/// `simulate(netlist, opts?)` — run a SPICE3 deck.
///
/// `opts` is `undefined`/`null` or a [`SimOptions`] object. Returns a
/// plain JS object of shape [`RawfileJson`]; throws an `Error` with
/// `kind` and `diagnostics` properties ([`SimError`]) on failure.
#[wasm_bindgen]
pub fn simulate(netlist: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = if opts.is_undefined() || opts.is_null() {
        None
    } else {
        let s = js_sys::JSON::stringify(&opts)
            .map_err(|_| js_error(&SimError::new("options", "opts is not JSON-serializable")))?;
        Some(String::from(s))
    };
    let out = simulate_json(netlist, opts_json.as_deref()).map_err(|e| js_error(&e))?;
    js_sys::JSON::parse(&out)
}

/// A JS `Error` carrying `kind` and `diagnostics`.
fn js_error(e: &SimError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    let _ = js_sys::Reflect::set(&err, &"kind".into(), &e.kind.as_str().into());
    let diags = serde_json::to_string(&e.diagnostics)
        .ok()
        .and_then(|s| js_sys::JSON::parse(&s).ok())
        .unwrap_or_else(|| js_sys::Array::new().into());
    let _ = js_sys::Reflect::set(&err, &"diagnostics".into(), &diags);
    err.into()
}

#[cfg(test)]
mod tests;
