//! WebAssembly build of the Dune simulator (PLAN.md §4), the engine
//! behind the Studio's "Simulate" panel.
//!
//! The JS surface is three functions:
//!
//! * `simulate(netlist, opts?)` parses the deck with `dune-netlist`,
//!   builds it with [`Circuit::from_netlist`], runs every analysis card
//!   in deck order (exactly what `dune -b` runs) and returns the plots
//!   as a JS object of shape [`RawfileJson`];
//! * `extract_pic(src, opts?)` extracts the SPICE deck of an rpic
//!   figure with [`dune_pic::extract`] (embedded prelude) and returns
//!   it with its diagnostics and overlay map ([`PicJson`]);
//! * `simulate_pic(src, opts?)` does both: extraction, then the deck
//!   through `simulate`'s pipeline ([`PicRunJson`]).
//!
//! On failure each throws a JS `Error` whose `message` is the
//! engine's, parser's or extractor's message, with `kind` and
//! `diagnostics` properties (and `pic`, for the pic entry points) as
//! in [`SimError`]. Rendering the figure is not here: the Studio has
//! rpic's own wasm for the SVG.
//!
//! Native builds (the `rlib`) expose the same paths as
//! [`simulate_json`]/[`run`], [`extract_pic_json`]/[`extract`] and
//! [`simulate_pic_json`]/[`run_pic`], which is what the tests exercise.
//!
//! The deck and the figure are hostile input: nothing here panics on
//! malformed input (the parser's no-panic contract carries over; every
//! engine failure is an `Err`). A long-running deck, however, blocks
//! the calling thread until it finishes — call it from a Web Worker.

mod schema;

pub use schema::{
    Columns, DiagnosticJson, ElementOverlayJson, ExtractOptionsJson, NetOverlayJson,
    OverlayMapJson, PicJson, PicRunJson, PicSimOptions, PlotJson, RawfileJson, ShapeRangeJson,
    SimError, SimOptions, VariableJson,
};

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use dune_core::{Circuit, Options, Solution};
use dune_netlist::{IncludeLoader, Severity};
use dune_pic::{ExtractOptions, PicError};
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

/// File name the deck is parsed under; `.include` paths resolve
/// relative to it (i.e. as given).
pub const DECK_NAME: &str = "deck.cir";

/// Run a deck and return the result as JSON text ([`RawfileJson`]).
/// `opts_json` is a [`SimOptions`] object, or `None` for defaults.
pub fn simulate_json(netlist: &str, opts_json: Option<&str>) -> Result<String, SimError> {
    let opts: SimOptions = parse_options(opts_json)?;
    let raw = run(netlist, &opts)?;
    serde_json::to_string(&raw).map_err(|e| SimError::new("analysis", e.to_string()))
}

/// Extract a figure's deck and return it as JSON text ([`PicJson`]).
/// `opts_json` is an [`ExtractOptionsJson`] object, or `None`.
pub fn extract_pic_json(src: &str, opts_json: Option<&str>) -> Result<String, SimError> {
    guarded(|| {
        let opts: ExtractOptionsJson = parse_options(opts_json)?;
        let pic = extract(src, opts.snap)?;
        serde_json::to_string(&pic).map_err(|e| SimError::new("internal", e.to_string()))
    })
}

/// Extract a figure's deck and run it; JSON text ([`PicRunJson`]).
/// `opts_json` is a [`PicSimOptions`] object, or `None`.
pub fn simulate_pic_json(src: &str, opts_json: Option<&str>) -> Result<String, SimError> {
    guarded(|| {
        let opts: PicSimOptions = parse_options(opts_json)?;
        let out = run_pic(src, &opts)?;
        serde_json::to_string(&out).map_err(|e| SimError::new("internal", e.to_string()))
    })
}

/// Parse an `opts` object (`None` = defaults) into `T`.
fn parse_options<T: DeserializeOwned + Default>(opts_json: Option<&str>) -> Result<T, SimError> {
    let Some(s) = opts_json else {
        return Ok(T::default());
    };
    let bad = |e: serde_json::Error| SimError::new("options", format!("bad options: {e}"));
    // An object only: serde would also accept a JSON array as a
    // (positional) struct.
    let v: serde_json::Value = serde_json::from_str(s).map_err(bad)?;
    if !v.is_object() {
        return Err(SimError::new("options", "bad options: expected an object"));
    }
    serde_json::from_value::<T>(v).map_err(bad)
}

/// Run `f`, turning a panic into an `internal` error. Effective on
/// native builds; wasm32 aborts on panic (the instance traps).
fn guarded<T>(f: impl FnOnce() -> Result<T, SimError>) -> Result<T, SimError> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        Err(SimError::new(
            "internal",
            format!("internal error (please report): {what}"),
        ))
    })
}

/// Extract the deck of a pic figure with the embedded prelude.
///
/// `snap` (pic inches) must be positive and finite when given. A
/// figure rpic rejects, or one not marked `spice = 1`, is a `pic`
/// error; extraction diagnostics, errors included, come back in the
/// [`PicJson`].
pub fn extract(src: &str, snap: Option<f64>) -> Result<PicJson, SimError> {
    let mut opts = ExtractOptions::default();
    if let Some(snap) = snap {
        if !(snap.is_finite() && snap > 0.0) {
            return Err(SimError::new(
                "options",
                format!("bad options: snap must be a positive finite number, got {snap}"),
            ));
        }
        opts.snap = snap;
    }
    let ex = dune_pic::extract(src, &opts).map_err(|e| {
        let (kind, span) = match &e {
            PicError::Compile { span, .. } => ("compile", span.as_ref()),
            PicError::NotMarked => ("not-marked", None),
        };
        let message = match &e {
            PicError::Compile { message, .. } => message.clone(),
            PicError::NotMarked => e.to_string(),
        };
        let shown = match span {
            Some(s) => format!("{s}: {message}"),
            None => message.clone(),
        };
        SimError {
            kind: "pic".into(),
            message: shown,
            diagnostics: vec![DiagnosticJson::pic_located("error", kind, message, span)],
            pic: None,
        }
    })?;
    Ok(PicJson {
        netlist: ex.netlist,
        diagnostics: ex
            .diagnostics
            .iter()
            .map(DiagnosticJson::from_pic)
            .collect(),
        map: OverlayMapJson::from(&ex.map),
    })
}

/// Extract a figure's deck and run every analysis in it.
///
/// Extraction errors (any diagnostic of severity error) stop here with
/// a `pic` error; otherwise the deck goes through [`run`] and any
/// failure there carries the extraction in [`SimError::pic`].
pub fn run_pic(src: &str, opts: &PicSimOptions) -> Result<PicRunJson, SimError> {
    let pic = extract(src, opts.snap)?;
    // dune_pic diagnostics map 1:1, so the severity string decides.
    let first_error = pic
        .diagnostics
        .iter()
        .find(|d| d.severity == "error")
        .map(|d| match d.line {
            Some(line) => format!("{}:{}: {}", line, d.column.unwrap_or(1), d.message),
            None => d.message.clone(),
        });
    if let Some(message) = first_error {
        return Err(SimError {
            kind: "pic".into(),
            message,
            diagnostics: pic.diagnostics.clone(),
            pic: Some(Box::new(pic)),
        });
    }
    match run_includes(&pic.netlist, &opts.includes) {
        Ok(raw) => Ok(PicRunJson { raw, pic }),
        Err(mut e) => {
            e.pic = Some(Box::new(pic));
            Err(e)
        }
    }
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
    run_includes(netlist, &opts.includes)
}

fn run_includes(
    netlist: &str,
    includes: &BTreeMap<String, String>,
) -> Result<RawfileJson, SimError> {
    let parsed = dune_netlist::parse_with_loader(
        Path::new(DECK_NAME),
        netlist.to_string(),
        &mut MapLoader(includes),
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
            pic: None,
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
    let opts_json = opts_text(&opts)?;
    let out = simulate_json(netlist, opts_json.as_deref()).map_err(|e| js_error(&e))?;
    js_sys::JSON::parse(&out)
}

/// `extract_pic(src, opts?)` — the SPICE deck of an rpic figure.
///
/// `opts` is `undefined`/`null` or an [`ExtractOptionsJson`] object
/// (`{ snap }`). Returns a plain JS object of shape [`PicJson`]
/// (`{ netlist, diagnostics, map }`); extraction diagnostics, errors
/// included, are in the result. Throws an `Error` of kind `pic` when
/// there is no deck at all (rpic rejected the figure, or it is not
/// marked `spice = 1`), or `options`.
/// The pic prelude dune-pic compiles ahead of the figure. The Studio
/// must render the SVG from exactly `prelude() + "\n" + src` (with
/// rpic's circuits library on) for the overlay's `sN` shape ids to
/// line up with the map.
#[wasm_bindgen]
pub fn prelude() -> String {
    dune_pic::PRELUDE.to_string()
}

#[wasm_bindgen]
pub fn extract_pic(src: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    let out = extract_pic_json(src, opts_json.as_deref()).map_err(|e| js_error(&e))?;
    js_sys::JSON::parse(&out)
}

/// `simulate_pic(src, opts?)` — extract an rpic figure's deck and run
/// it.
///
/// `opts` is `undefined`/`null` or a [`PicSimOptions`] object
/// (`{ snap, includes }`). Returns a plain JS object of shape
/// [`PicRunJson`]: `simulate`'s result plus the extraction under
/// `pic`. Throws as `extract_pic` does, also with kind `pic` when the
/// extraction has errors, and as `simulate` does for the deck; the
/// thrown error then carries the extraction as its `pic` property.
#[wasm_bindgen]
pub fn simulate_pic(src: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    let out = simulate_pic_json(src, opts_json.as_deref()).map_err(|e| js_error(&e))?;
    js_sys::JSON::parse(&out)
}

/// `opts` as JSON text; `None` for `undefined`/`null`.
fn opts_text(opts: &JsValue) -> Result<Option<String>, JsValue> {
    if opts.is_undefined() || opts.is_null() {
        return Ok(None);
    }
    let s = js_sys::JSON::stringify(opts)
        .map_err(|_| js_error(&SimError::new("options", "opts is not JSON-serializable")))?;
    Ok(Some(String::from(s)))
}

/// Serde value → JS value, through JSON text.
fn to_js<T: serde::Serialize>(v: &T) -> Option<JsValue> {
    serde_json::to_string(v)
        .ok()
        .and_then(|s| js_sys::JSON::parse(&s).ok())
}

/// A JS `Error` carrying `kind`, `diagnostics` and, when present, `pic`.
fn js_error(e: &SimError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    let _ = js_sys::Reflect::set(&err, &"kind".into(), &e.kind.as_str().into());
    let diags = to_js(&e.diagnostics).unwrap_or_else(|| js_sys::Array::new().into());
    let _ = js_sys::Reflect::set(&err, &"diagnostics".into(), &diags);
    if let Some(pic) = e.pic.as_deref().and_then(to_js) {
        let _ = js_sys::Reflect::set(&err, &"pic".into(), &pic);
    }
    err.into()
}

#[cfg(test)]
mod tests;
