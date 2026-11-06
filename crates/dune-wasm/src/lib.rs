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
/// Each card runs through [`Circuit::run`], the dispatch `dune-cli`
/// shares (same plot order, title and empty date as its rawfile).
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
        let (sols, analysis_notes) = circuit
            .run(analysis, &options)
            .map_err(|e| SimError::new("analysis", e.to_string()))?;
        notes.extend(analysis_notes);
        solutions.extend(sols);
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
