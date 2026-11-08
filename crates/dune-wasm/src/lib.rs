//! WebAssembly build of the Dune simulator (PLAN.md §4), the engine
//! behind the Studio's "Simulate" panel.
//!
//! The JS surface:
//!
//! * `simulate(netlist, opts?)` parses the deck with `dune-netlist`,
//!   builds it with [`Circuit::from_netlist`], runs every analysis card
//!   in deck order (exactly what `dune -b` runs) and returns the plots,
//!   with the deck's `.measure` results evaluated on them
//!   ([`MeasurementJson`]), as a JS object of shape [`RawfileJson`];
//! * `extract_pic(src, opts?)` extracts the SPICE deck of an rpic
//!   figure with [`dune_pic::extract`] (embedded prelude) and returns
//!   it with its diagnostics and overlay map ([`PicJson`]);
//! * `simulate_pic(src, opts?)` does both: extraction, then the deck
//!   through `simulate`'s pipeline ([`PicRunJson`]);
//! * `render_pic(src, opts?)` renders the figure to SVG with
//!   [`dune_pic::render_svg`] — the same rpic compile (prelude
//!   included) whose shape ids the overlay map refers to; it does not
//!   require `spice = 1`;
//! * `simulate_typed`/`simulate_pic_typed` are `simulate`/`simulate_pic`
//!   with each plot's `columns` as `Float64Array`s instead of JSON
//!   arrays (see [`simulate_typed`]);
//! * `prelude()` is the embedded pic prelude.
//!
//! Every run takes the engine overrides of [`SimOptions`] (issue #16):
//! `options`, `.options` keys applied after the deck's cards, and
//! `tmaxDivisor`, which divides each `.tran`'s maximum step
//! ([`Overrides`]); a `Session` swaps both between runs.
//!
//! On failure each throws a JS `Error` whose `message` is the
//! engine's, parser's or extractor's message, with `kind` and
//! `diagnostics` properties (and `pic`, for the pic entry points;
//! `iterations`/`residuals`/`period` for a `.pss` that did not
//! converge, kind `pss`) as in [`SimError`].
//!
//! Native builds (the `rlib`) expose the same paths as
//! [`simulate_json`]/[`run`], [`extract_pic_json`]/[`extract`],
//! [`simulate_pic_json`]/[`run_pic`] and [`render_pic_svg`], plus
//! [`run_data`]/[`run_pic_data`] and [`column_buffers`] behind the
//! typed variants, which is what the tests exercise.
//!
//! The deck and the figure are hostile input: nothing here panics on
//! malformed input (the parser's no-panic contract carries over; every
//! engine failure is an `Err`). A long-running deck, however, blocks
//! the calling thread until it finishes — call it from a Web Worker.

mod overrides;
mod schema;
mod session;

pub use schema::{
    CanvasJson, Columns, DiagnosticJson, ElementOverlayJson, ExtractOptionsJson, MeasurementJson,
    NetOverlayJson, OverlayMapJson, ParamSpanJson, PicJson, PicRunJson, PicSimOptions, PlotJson,
    PssFailureJson, PssJson, RawfileJson, RenderOptionsJson, ShapeRangeJson, SimError, SimOptions,
    SourceRangeJson, ValueSpanJson, VariableJson,
};

pub use overrides::Overrides;
pub use session::{JsSession, SimSession};

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use dune_core::{Circuit, Options, PssParams, Solution};
use dune_netlist::{IncludeLoader, Severity};
use dune_pic::{ExtractOptions, PicError};
use dune_raw::{Data, Plot};
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

/// Render a figure to SVG text with the embedded prelude
/// ([`dune_pic::render_svg`]): the drawing the overlay map of
/// [`extract`] indexes (`<g id="sN">`). `opts_json` is a
/// [`RenderOptionsJson`] object (currently empty), or `None`.
///
/// Any figure rpic compiles renders — `spice = 1` is not required. A
/// figure rpic rejects is a `pic` error with one `compile` diagnostic,
/// as in [`extract`].
pub fn render_pic_svg(src: &str, opts_json: Option<&str>) -> Result<String, SimError> {
    guarded(|| {
        let _opts: RenderOptionsJson = parse_options(opts_json)?;
        dune_pic::render_svg(src, &ExtractOptions::default()).map_err(pic_error)
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
    let ex = dune_pic::extract(src, &opts).map_err(pic_error)?;
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

/// A figure that produced no deck (or no drawing): a `pic` error with
/// one `compile` or `not-marked` diagnostic.
fn pic_error(e: PicError) -> SimError {
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
        pss: None,
    }
}

/// Extract a figure's deck and run every analysis in it.
///
/// Extraction errors (any diagnostic of severity error) stop here with
/// a `pic` error; otherwise the deck goes through [`run`] and any
/// failure there carries the extraction in [`SimError::pic`].
pub fn run_pic(src: &str, opts: &PicSimOptions) -> Result<PicRunJson, SimError> {
    let (data, pic) = run_pic_data(src, opts)?;
    Ok(PicRunJson {
        raw: data.to_json(),
        pic,
    })
}

/// [`run_pic`] with the plots kept in engine form ([`RunData`]).
pub fn run_pic_data(src: &str, opts: &PicSimOptions) -> Result<(RunData, PicJson), SimError> {
    opts.overrides().check()?;
    let pic = extract_runnable(src, opts.snap)?;
    match run_includes(&pic.netlist, &opts.includes, &opts.overrides()) {
        Ok(data) => Ok((data, pic)),
        Err(mut e) => {
            e.pic = Some(Box::new(pic));
            Err(e)
        }
    }
}

/// [`extract`], with extraction errors (any diagnostic of severity
/// error) turned into a `pic` error carrying the extraction.
pub(crate) fn extract_runnable(src: &str, snap: Option<f64>) -> Result<PicJson, SimError> {
    let pic = extract(src, snap)?;
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
            pss: None,
        });
    }
    Ok(pic)
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
    Ok(run_data(netlist, opts)?.to_json())
}

/// [`run`] with the plots kept in engine form ([`RunData`]).
pub fn run_data(netlist: &str, opts: &SimOptions) -> Result<RunData, SimError> {
    let overrides = opts.overrides();
    overrides.check()?;
    run_includes(netlist, &opts.includes, &overrides)
}

/// The outcome of a run before its plots become JSON: what the typed
/// entry points read their columns from.
#[derive(Clone, Debug)]
pub struct RunData {
    /// One per analysis result, in deck order (as [`RawfileJson::plots`]).
    pub plots: Vec<Plot>,
    /// The deck's `.measure` results (as [`RawfileJson::measurements`]).
    pub measurements: Vec<MeasurementJson>,
    pub notes: Vec<String>,
    pub diagnostics: Vec<DiagnosticJson>,
    /// One per `.pss` card (as [`RawfileJson::pss`]).
    pub pss: Vec<PssJson>,
}

impl RunData {
    /// The full JSON result (what `simulate` returns).
    pub fn to_json(&self) -> RawfileJson {
        RawfileJson {
            plots: self.plots.iter().map(PlotJson::from).collect(),
            measurements: self.measurements.clone(),
            notes: self.notes.clone(),
            diagnostics: self.diagnostics.clone(),
            pss: self.pss.clone(),
        }
    }

    /// The JSON result without data: every plot's `columns` is an empty
    /// array (metadata only; `npoints` still counts the points).
    pub fn header_json(&self) -> RawfileJson {
        RawfileJson {
            plots: self.plots.iter().map(PlotJson::header).collect(),
            measurements: self.measurements.clone(),
            notes: self.notes.clone(),
            diagnostics: self.diagnostics.clone(),
            pss: self.pss.clone(),
        }
    }
}

/// A plot's data as flat `f64` buffers, one per variable in
/// `variables` order: the column itself in a real plot (borrowed), the
/// interleaved `re, im, re, im, …` (length `2·npoints`) in a complex
/// one. Non-finite values stay as they are (NaN, ±inf).
pub fn column_buffers(p: &Plot) -> Vec<Cow<'_, [f64]>> {
    match &p.data {
        Data::Real(cols) => cols.iter().map(|c| Cow::Borrowed(c.as_slice())).collect(),
        Data::Complex(cols) => cols
            .iter()
            .map(|c| Cow::Owned(c.iter().flat_map(|z| [z.re, z.im]).collect()))
            .collect(),
    }
}

fn run_includes(
    netlist: &str,
    includes: &BTreeMap<String, String>,
    overrides: &Overrides,
) -> Result<RunData, SimError> {
    let mut prepared = prepare(netlist, includes)?;
    let (options, override_diags) = overrides.apply(&prepared.options);
    prepared.override_diagnostics = override_diags;
    let mut out = Outcome::new(prepared.notes.clone());
    for analysis in &prepared.analyses {
        let card = match pss_params(analysis) {
            Some(p) => prepared
                .circuit
                .pss(&options, &p)
                .map(|p| Card::Pss(Box::new(p))),
            None => prepared.circuit.run(analysis, &options).map(Card::Plain),
        };
        out.push(card.map_err(SimError::analysis)?);
    }
    Ok(prepared.finish(out))
}

/// What one analysis card produced: a `.pss` keeps its engine result
/// (period, history) besides the plots.
pub(crate) enum Card {
    Plain((Vec<Solution>, Vec<String>)),
    Pss(Box<dune_core::Pss>),
}

/// The cards of a run so far, in deck order.
pub(crate) struct Outcome {
    solutions: Vec<Solution>,
    notes: Vec<String>,
    pss: Vec<PssJson>,
}

impl Outcome {
    /// An empty run whose notes start with `notes` (the elaboration's).
    pub fn new(notes: Vec<String>) -> Self {
        Outcome {
            solutions: Vec::new(),
            notes,
            pss: Vec::new(),
        }
    }

    pub fn push(&mut self, card: Card) {
        match card {
            Card::Plain((sols, notes)) => {
                self.notes.extend(notes);
                self.solutions.extend(sols);
            }
            Card::Pss(pss) => {
                self.pss.push(PssJson::new(&pss, self.solutions.len()));
                self.notes.extend(pss.notes);
                self.solutions.push(pss.time);
                self.solutions.push(pss.spectrum);
            }
        }
    }
}

/// The [`PssParams`] a `.pss` card asks for — the mapping of
/// [`Circuit::run`] (`dune_core::run`), repeated here because `run`
/// returns only the plots and the period, `t0` and the shooting
/// history travel in [`RawfileJson::pss`]. `None` for any other card.
/// `pss_cards_run_as_the_engine_runs_them` (tests) pins the two
/// mappings together on every `.pss` deck of the corpus.
pub(crate) fn pss_params(analysis: &dune_netlist::ir::Analysis) -> Option<PssParams> {
    let dune_netlist::ir::Analysis::Pss {
        freq,
        tstab,
        osc_node,
        points,
        harmonics,
        sc_iter,
        steady_coeff,
        uic,
        ..
    } = analysis
    else {
        return None;
    };
    let d = PssParams::default();
    // ngspice reads the counts as IF_INTEGER (truncated).
    let count = |v: Option<f64>, default: usize| match v {
        Some(v) if v.is_finite() && v >= 0.0 => v.trunc().min(1e9) as usize,
        Some(_) => 0,
        None => default,
    };
    Some(PssParams {
        freq: (*freq != 0.0).then_some(*freq),
        tstab: *tstab,
        osc_node: Some(osc_node.clone()),
        points: count(*points, d.points),
        harmonics: count(*harmonics, d.harmonics),
        max_iter: count(*sc_iter, d.max_iter),
        steady_coeff: steady_coeff.unwrap_or(d.steady_coeff),
        uic: *uic,
    })
}

/// A deck parsed and elaborated, ready to run: what a fresh run and a
/// [`session::SimSession`] share up to the first analysis.
pub(crate) struct Prepared {
    pub circuit: Circuit,
    /// The deck's options (its `.options` cards; no overrides).
    pub options: Options,
    pub analyses: Vec<dune_netlist::ir::Analysis>,
    /// The deck's `.measure` cards, evaluated on every run.
    pub measures: Vec<dune_netlist::ir::Measure>,
    pub title: String,
    /// Elaboration notes (each run's result starts with them).
    pub notes: Vec<String>,
    /// Parser and `.options` warnings.
    pub diagnostics: Vec<DiagnosticJson>,
    /// Warnings of the run's `SimOptions.options` (after the deck's).
    pub override_diagnostics: Vec<DiagnosticJson>,
}

impl Prepared {
    /// A run's [`RunData`] from its plots and notes (the elaboration
    /// notes first), with the deck's `.measure` cards evaluated on it.
    pub fn finish(&self, out: Outcome) -> RunData {
        let Outcome {
            solutions,
            mut notes,
            pss,
        } = out;
        if solutions.is_empty() {
            notes.push("no analyses in the deck; nothing to do".into());
        }
        let measurements = dune_core::measure::measure_all(&self.measures, &solutions)
            .iter()
            .map(|o| MeasurementJson::from_outcome(&self.measures[o.card], o.plot, &o.result))
            .collect();
        RunData {
            measurements,
            plots: solutions
                .iter()
                .map(|s| s.to_plot(&self.title, ""))
                .collect(),
            notes,
            diagnostics: self
                .diagnostics
                .iter()
                .chain(&self.override_diagnostics)
                .cloned()
                .collect(),
            pss,
        }
    }
}

/// Parse and elaborate `netlist` (`.include` from `includes`).
pub(crate) fn prepare(
    netlist: &str,
    includes: &BTreeMap<String, String>,
) -> Result<Prepared, SimError> {
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
            pss: None,
        });
    }

    let (circuit, notes) = Circuit::from_netlist(&parsed.netlist)
        .map_err(|e| SimError::new("elaborate", e.to_string()))?;
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(
        opt_diags
            .iter()
            .map(|d| DiagnosticJson::from_diag(d, &parsed.sources)),
    );
    Ok(Prepared {
        circuit,
        options,
        analyses: parsed.netlist.analyses.clone(),
        measures: parsed.netlist.measures.clone(),
        title: parsed.netlist.title.clone(),
        notes,
        diagnostics,
        override_diagnostics: Vec::new(),
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

/// `prelude()` — the pic prelude dune-pic compiles ahead of the figure.
///
/// Kept for embedders that render with their own rpic: the SVG must
/// come from exactly `prelude() + "\n" + src` (with rpic's circuits
/// library on, and — before rpic 0.13 — the `.`-lines blanked) for the
/// overlay's `sN` shape ids to line up with the map. `render_pic` does
/// that with dune's own rpic and supersedes it.
#[wasm_bindgen]
pub fn prelude() -> String {
    dune_pic::PRELUDE.to_string()
}

/// `extract_pic(src, opts?)` — the SPICE deck of an rpic figure.
///
/// `opts` is `undefined`/`null` or an [`ExtractOptionsJson`] object
/// (`{ snap }`). Returns a plain JS object of shape [`PicJson`]
/// (`{ netlist, diagnostics, map }`); extraction diagnostics, errors
/// included, are in the result. Throws an `Error` of kind `pic` when
/// there is no deck at all (rpic rejected the figure, or it is not
/// marked `spice = 1`), or `options`.
#[wasm_bindgen]
pub fn extract_pic(src: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    let out = extract_pic_json(src, opts_json.as_deref()).map_err(|e| js_error(&e))?;
    js_sys::JSON::parse(&out)
}

/// `render_pic(src, opts?)` — the SVG of an rpic figure.
///
/// Compiles `prelude + src` with dune's own rpic (circuits library on)
/// — the very compile [`extract_pic`]'s overlay map indexes, so its
/// `<g id="sN">` groups line up with the map whatever rpic version the
/// embedder has. Does not require `spice = 1`; the figure's `.`-lines
/// (SPICE directives) draw nothing. `opts` is `undefined`/`null` or
/// `{}` ([`RenderOptionsJson`]). Throws an `Error` of kind `pic` (one
/// `compile` diagnostic) when rpic rejects the figure, or `options`.
#[wasm_bindgen]
pub fn render_pic(src: &str, opts: JsValue) -> Result<String, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    render_pic_svg(src, opts_json.as_deref()).map_err(|e| js_error(&e))
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

/// `simulate_typed(netlist, opts?)` — `simulate` with typed-array
/// columns.
///
/// Same arguments, errors and result object as `simulate`, except that
/// each plot's `columns` is a JS `Array` of `Float64Array`s, one per
/// variable: the values themselves in a `real` plot (`npoints` long),
/// interleaved `re, im, re, im, …` in a `complex` one (`2·npoints`
/// long). NaN and ±inf stay as they are (the JSON variant writes
/// `null`).
///
/// Each array is a **copy** into a JS-owned `ArrayBuffer` (one per
/// column), not a view of wasm memory — a view would be detached by
/// the next memory growth. They survive the instance, are
/// structured-cloneable and can be listed as transferables in
/// `postMessage`, which moves them without copying.
#[wasm_bindgen]
pub fn simulate_typed(netlist: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    let data = parse_options::<SimOptions>(opts_json.as_deref())
        .and_then(|o| run_data(netlist, &o))
        .map_err(|e| js_error(&e))?;
    let header = serde_json::to_string(&data.header_json())
        .map_err(|e| js_error(&SimError::new("internal", e.to_string())))?;
    typed_result(&header, &data.plots)
}

/// `simulate_pic_typed(src, opts?)` — `simulate_pic` with the plots'
/// `columns` as `Float64Array`s, as in [`simulate_typed`] (`pic` is
/// unchanged).
#[wasm_bindgen]
pub fn simulate_pic_typed(src: &str, opts: JsValue) -> Result<JsValue, JsValue> {
    #[cfg(feature = "console-panic")]
    console_error_panic_hook::set_once();
    let opts_json = opts_text(&opts)?;
    let (data, pic) = guarded(|| {
        let opts: PicSimOptions = parse_options(opts_json.as_deref())?;
        run_pic_data(src, &opts)
    })
    .map_err(|e| js_error(&e))?;
    let header = serde_json::to_string(&PicRunJson {
        raw: data.header_json(),
        pic,
    })
    .map_err(|e| js_error(&SimError::new("internal", e.to_string())))?;
    typed_result(&header, &data.plots)
}

/// Parse the data-less JSON `header` and fill each plot's `columns`
/// with the typed arrays of the matching engine plot.
fn typed_result(header: &str, plots: &[Plot]) -> Result<JsValue, JsValue> {
    let obj = js_sys::JSON::parse(header)?;
    let js_plots: js_sys::Array = js_sys::Reflect::get(&obj, &"plots".into())?.unchecked_into();
    let key: JsValue = "columns".into();
    for (i, p) in plots.iter().enumerate() {
        let cols = js_sys::Array::new();
        for buf in column_buffers(p) {
            // `From<&[f64]>` copies into a fresh JS-owned buffer.
            cols.push(&js_sys::Float64Array::from(buf.as_ref()));
        }
        js_sys::Reflect::set(&js_plots.get(i as u32), &key, &cols)?;
    }
    Ok(obj)
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

/// A JS `Error` carrying `kind`, `diagnostics` and, when present, `pic`
/// and (kind `pss`) `iterations`, `residuals`, `period`.
fn js_error(e: &SimError) -> JsValue {
    let err = js_sys::Error::new(&e.message);
    let _ = js_sys::Reflect::set(&err, &"kind".into(), &e.kind.as_str().into());
    let diags = to_js(&e.diagnostics).unwrap_or_else(|| js_sys::Array::new().into());
    let _ = js_sys::Reflect::set(&err, &"diagnostics".into(), &diags);
    if let Some(pic) = e.pic.as_deref().and_then(to_js) {
        let _ = js_sys::Reflect::set(&err, &"pic".into(), &pic);
    }
    if let Some(p) = &e.pss {
        let _ = js_sys::Reflect::set(&err, &"iterations".into(), &(p.iterations as f64).into());
        let residuals = to_js(&p.residuals).unwrap_or_else(|| js_sys::Array::new().into());
        let _ = js_sys::Reflect::set(&err, &"residuals".into(), &residuals);
        let period = p.period.map_or(JsValue::NULL, JsValue::from);
        let _ = js_sys::Reflect::set(&err, &"period".into(), &period);
    }
    err.into()
}

#[cfg(test)]
mod tests;
