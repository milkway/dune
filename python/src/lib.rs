//! Native core of the `spice` Python package (`spice._spice`).
//!
//! One entry point, [`simulate`]: parse a deck (from a string, with
//! `.include` served from an in-memory map like `dune-wasm`, or from a
//! file, with `.include` on disk), elaborate it, run every analysis
//! card through [`Circuit::run`] and hand the plots back as plain
//! Python objects. Columns move into NumPy arrays without a copy
//! (`PyArray1::from_vec` adopts the `Vec`'s buffer).
//!
//! `_pss` runs the periodic steady state of a deck directly
//! ([`Circuit::pss`], the call a `.pss` card makes).
//!
//! This module is private: the public, typed API is the pure-Python
//! layer in `spice/`. Errors in the deck or the run raise
//! `NativeError(kind, message, diagnostics)`, which that layer turns
//! into `spice.SpiceError` (kind `pss` carries a fourth argument, the
//! shooting history, and becomes `spice.PssConvergenceError`); bad `options` raise `ValueError` /
//! `TypeError` directly. A Rust panic never crosses the boundary — it
//! is caught and reported as kind `internal`.

mod pic;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use dune_core::consts::CTOK;
use dune_core::{
    AlterValue, Circuit, Compat, Data, MeasureError, Measurement, Method, Options, PSS_FREQ_PLOT,
    PSS_TIME_PLOT, PssParams, Session, Solution, SweepParam, SweepRun, VarType, measure_all,
};
use dune_netlist::ir::Netlist;
use dune_netlist::{Diagnostic, IncludeLoader, ParseOutcome, Severity, SourceMap};
use num_complex::Complex64;
use numpy::{PyArray1, PyReadonlyArray1};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString};

create_exception!(
    _spice,
    NativeError,
    PyException,
    "Raised as NativeError(kind, message, diagnostics); wrapped by spice.SpiceError."
);

/// File name a string deck is parsed under; `.include` paths resolve
/// relative to it, i.e. as given (same as `dune-wasm`).
const DECK_NAME: &str = "deck.cir";

/// Every `options` key accepted (the `.options` card names), in
/// `Options` field order.
const OPTION_NAMES: [&str; 32] = [
    "reltol",
    "abstol",
    "vntol",
    "gmin",
    "itl1",
    "itl2",
    "itl4",
    "gminsteps",
    "srcsteps",
    "temp",
    "tnom",
    "pivtol",
    "pivrel",
    "noopiter",
    "trtol",
    "chgtol",
    "method",
    "compat",
    "ltrabkrate",
    "ltrafit",
    "maxord",
    "nopage",
    "defl",
    "defw",
    "defad",
    "defas",
    "seed",
    "notrnoise",
    "devnoise",
    "devnt",
    "keepopinfo",
    "trytocompact",
];

/// A parser/options diagnostic with its position resolved.
#[derive(Clone)]
struct Diag {
    severity: &'static str,
    message: String,
    file: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
}

impl Diag {
    fn new(d: &Diagnostic, sources: &SourceMap) -> Self {
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        let (file, line, column) = match d.span {
            Some(span) => {
                let loc = sources.locate(span);
                (
                    Some(loc.file.display().to_string()),
                    Some(loc.line),
                    Some(loc.column),
                )
            }
            None => (None, None, None),
        };
        Diag {
            severity,
            message: d.message.clone(),
            file,
            line,
            column,
        }
    }
}

/// Why a run failed: `kind` is `io`, `parse`, `elaborate`,
/// `analysis`, `pss` or `internal`.
struct Failure {
    kind: &'static str,
    message: String,
    diags: Vec<Diag>,
    /// For kind `pss` (a `.pss` that did not converge): the shooting
    /// history.
    pss: Option<PssHistory>,
}

/// The history of a shooting that did not converge
/// ([`dune_core::Error::PssConvergence`]): the iterations run, the
/// residual of each and, for an autonomous oscillator, the last
/// period iterate.
struct PssHistory {
    iterations: usize,
    residuals: Vec<f64>,
    period: Option<f64>,
}

impl PssHistory {
    /// The history of `e`, when it is a `.pss` that did not converge.
    fn of(e: &dune_core::Error) -> Option<Self> {
        match e {
            dune_core::Error::PssConvergence {
                iterations,
                residuals,
                period,
            } => Some(PssHistory {
                iterations: *iterations,
                residuals: residuals.clone(),
                period: *period,
            }),
            _ => None,
        }
    }

    /// `{"iterations", "residuals", "period"}`: the extra argument of
    /// a `NativeError` of kind `pss`, and the fields of a failed sweep
    /// run of that kind.
    fn fill(&self, d: &Bound<'_, PyDict>) -> PyResult<()> {
        d.set_item("iterations", self.iterations)?;
        d.set_item("residuals", &self.residuals)?;
        d.set_item("period", self.period)
    }
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
            diags: Vec::new(),
            pss: None,
        }
    }

    /// An analysis that failed: kind `pss`, with the shooting history,
    /// for a `.pss` that did not converge; kind `analysis` otherwise.
    fn analysis(e: dune_core::Error) -> Self {
        match PssHistory::of(&e) {
            Some(h) => Failure {
                pss: Some(h),
                ..Failure::new("pss", e.to_string())
            },
            None => Failure::new("analysis", e.to_string()),
        }
    }
}

/// A finished run, still in Rust form.
struct Output {
    title: String,
    solutions: Vec<Solution>,
    measures: Vec<MeasRow>,
    notes: Vec<String>,
    diags: Vec<Diag>,
}

/// One measurement, as plain data (what `spice.Measurement` wraps).
struct MeasRow {
    name: String,
    analysis: &'static str,
    /// Index of the measured plot in the run (`None` for the direct
    /// `spice.measure` API).
    plot: Option<usize>,
    /// NaN when the measurement failed.
    value: f64,
    details: Vec<(&'static str, f64)>,
    error: Option<String>,
}

impl MeasRow {
    fn new(
        name: &str,
        analysis: &'static str,
        plot: Option<usize>,
        r: &Result<Measurement, MeasureError>,
    ) -> Self {
        let (value, details, error) = match r {
            Ok(m) => (m.value, m.detail.pairs(), None),
            Err(e) => (f64::NAN, Vec::new(), Some(e.describe())),
        };
        MeasRow {
            name: name.to_string(),
            analysis,
            plot,
            value,
            details,
            error,
        }
    }
}

/// The `.measure` analysis keyword of a plot (`tran`, `ac`, `dc`,
/// `sp`) and the plot name the engine measures it under, `None` for
/// plots that cannot be measured. The `.pss` plots measure as the
/// analyses they are shaped like: the period in time as a `.tran`, the
/// complex harmonics (on a frequency scale) as an `.ac`.
fn plot_analysis(plotname: &str) -> Option<(&'static str, &'static str)> {
    Some(match plotname {
        "Transient Analysis" => ("tran", "Transient Analysis"),
        "AC Analysis" => ("ac", "AC Analysis"),
        "SP Analysis" => ("sp", "SP Analysis"),
        "DC transfer characteristic" => ("dc", "DC transfer characteristic"),
        PSS_TIME_PLOT => ("tran", "Transient Analysis"),
        PSS_FREQ_PLOT => ("ac", "AC Analysis"),
        _ => return None,
    })
}

/// Where the deck comes from.
enum Source {
    Text {
        text: String,
        includes: BTreeMap<String, String>,
    },
    File(PathBuf),
}

/// `.include` from the in-memory map (a string deck never touches
/// the file system).
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

/// A parsed deck and its options (the deck's `.options`, then the
/// caller's overrides on top), ready to elaborate.
struct Prepared {
    netlist: Netlist,
    options: Options,
    diags: Vec<Diag>,
}

/// Parse the deck and settle its options.
fn prepare(source: &Source, overrides: &[Override]) -> Result<Prepared, Failure> {
    let parsed: ParseOutcome = match source {
        Source::Text { text, includes } => dune_netlist::parse_with_loader(
            Path::new(DECK_NAME),
            text.clone(),
            &mut MapLoader(includes),
        ),
        Source::File(path) => {
            dune_netlist::parse_file(path).map_err(|e| Failure::new("io", e.to_string()))?
        }
    };
    let mut diags: Vec<Diag> = parsed
        .diags
        .iter()
        .map(|d| Diag::new(d, &parsed.sources))
        .collect();
    if let Some(first) = parsed.diags.iter().find(|d| d.severity == Severity::Error) {
        return Err(Failure {
            kind: "parse",
            message: first.render(&parsed.sources),
            diags,
            pss: None,
        });
    }
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diags.extend(opt_diags.iter().map(|d| Diag::new(d, &parsed.sources)));
    for set in overrides {
        set(&mut options);
    }
    Ok(Prepared {
        netlist: parsed.netlist,
        options,
        diags,
    })
}

/// Parse, elaborate and run every analysis card (what `dune -b` runs).
fn run(source: &Source, overrides: &[Override]) -> Result<Output, Failure> {
    let Prepared {
        netlist,
        options,
        diags,
    } = prepare(source, overrides)?;
    let (circuit, mut notes) =
        Circuit::from_netlist(&netlist).map_err(|e| Failure::new("elaborate", e.to_string()))?;

    let mut solutions = Vec::new();
    for analysis in &netlist.analyses {
        let (sols, analysis_notes) = circuit.run(analysis, &options).map_err(Failure::analysis)?;
        notes.extend(analysis_notes);
        solutions.extend(sols);
    }
    if solutions.is_empty() {
        notes.push("no analyses in the deck; nothing to do".into());
    }
    // `.measure` cards: pure post-processing of the plots.
    let measures = deck_measures(&netlist.measures, &solutions);
    Ok(Output {
        title: netlist.title.clone(),
        solutions,
        measures,
        notes,
        diags,
    })
}

/// The deck's `.measure` cards evaluated on a run's plots.
fn deck_measures(cards: &[dune_netlist::ir::Measure], solutions: &[Solution]) -> Vec<MeasRow> {
    measure_all(cards, solutions)
        .iter()
        .map(|o| {
            let m = &cards[o.card];
            MeasRow::new(&m.name, m.analysis.keyword(), Some(o.plot), &o.result)
        })
        .collect()
}

/// A finished sweep, still in Rust form.
struct SweepOut {
    title: String,
    keys: Vec<String>,
    runs: Vec<SweepRun>,
    /// The deck's `.measure` cards, evaluated on every run.
    measures: Vec<dune_netlist::ir::Measure>,
    diags: Vec<Diag>,
}

/// Parse once, then run the deck per row of `rows` (issue #10).
fn run_sweep(
    source: &Source,
    overrides: &[Override],
    params: &[SweepParam],
    rows: &[Vec<f64>],
    threads: usize,
) -> Result<SweepOut, Failure> {
    let Prepared {
        netlist,
        options,
        diags,
    } = prepare(source, overrides)?;
    let runs = if threads == 1 {
        dune_core::sweep(&netlist, &options, params, rows)
    } else {
        dune_core::sweep_parallel(&netlist, &options, params, rows, threads)
    }
    .map_err(|e| Failure::new("sweep", e.to_string()))?;
    Ok(SweepOut {
        title: netlist.title.clone(),
        keys: params.iter().map(SweepParam::key).collect(),
        runs,
        measures: netlist.measures.clone(),
        diags,
    })
}

/// A finite float option value (bools refused: `True` is not a
/// tolerance).
fn float_value(key: &str, v: &Bound<'_, PyAny>) -> PyResult<f64> {
    if v.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "option {key} must be a number, not bool"
        )));
    }
    let x: f64 = v
        .extract()
        .map_err(|_| PyTypeError::new_err(format!("option {key} must be a number")))?;
    if !x.is_finite() {
        return Err(PyValueError::new_err(format!(
            "option {key} must be finite"
        )));
    }
    Ok(x)
}

/// A non-negative integer option value (same bound as `.options`).
fn count_value(key: &str, v: &Bound<'_, PyAny>) -> PyResult<usize> {
    if v.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "option {key} must be an int, not bool"
        )));
    }
    let n: i64 = v
        .extract()
        .map_err(|_| PyTypeError::new_err(format!("option {key} must be an int")))?;
    if !(0..1_000_000_000).contains(&n) {
        return Err(PyValueError::new_err(format!(
            "option {key} must be a non-negative count, got {n}"
        )));
    }
    Ok(n as usize)
}

/// A `seed` value: an int in `[1, 2^53)`, the card's range (Dune
/// never seeds from the clock, so there is no `"random"`).
fn seed_value(key: &str, v: &Bound<'_, PyAny>) -> PyResult<u64> {
    if v.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!(
            "option {key} must be an int, not bool"
        )));
    }
    let n: i64 = v
        .extract()
        .map_err(|_| PyTypeError::new_err(format!("option {key} must be an int")))?;
    if !(1..9_007_199_254_740_992).contains(&n) {
        return Err(PyValueError::new_err(format!(
            "option {key} must be a positive int below 2**53, got {n}"
        )));
    }
    Ok(n as u64)
}

/// A positive finite time step (`devnt`).
fn step_value(key: &str, v: &Bound<'_, PyAny>) -> PyResult<f64> {
    let x = float_value(key, v)?;
    if x <= 0.0 {
        return Err(PyValueError::new_err(format!(
            "option {key} must be a positive time step, got {x}"
        )));
    }
    Ok(x)
}

fn bool_value(key: &str, v: &Bound<'_, PyAny>) -> PyResult<bool> {
    if !v.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(format!("option {key} must be a bool")));
    }
    v.extract()
}

/// One validated `options` entry, applied after the deck's `.options`.
type Override = Box<dyn Fn(&mut Options) + Send + Sync>;

/// Validate the caller's `options` dict into setters. Keys are
/// case-insensitive, like `.options`; temperatures are in °C, like
/// the card.
fn parse_overrides(dict: &Bound<'_, PyDict>) -> PyResult<Vec<Override>> {
    macro_rules! set {
        ($field:ident, $value:expr) => {{
            let x = $value;
            Box::new(move |o: &mut Options| o.$field = x) as Override
        }};
    }
    let mut out = Vec::new();
    for (k, v) in dict.iter() {
        let key: String = k
            .cast::<PyString>()
            .map_err(|_| PyTypeError::new_err("option names must be str"))?
            .to_str()?
            .to_ascii_lowercase();
        let key = key.as_str();
        out.push(match key {
            "reltol" => set!(reltol, float_value(key, &v)?),
            "abstol" => set!(abstol, float_value(key, &v)?),
            "vntol" => set!(vntol, float_value(key, &v)?),
            "gmin" => set!(gmin, float_value(key, &v)?),
            "itl1" => set!(itl1, count_value(key, &v)?),
            "itl2" => set!(itl2, count_value(key, &v)?),
            "itl4" => set!(itl4, count_value(key, &v)?),
            "gminsteps" => set!(gminsteps, count_value(key, &v)?),
            "srcsteps" => set!(srcsteps, count_value(key, &v)?),
            "temp" => set!(temp, float_value(key, &v)? + CTOK),
            "tnom" => set!(tnom, float_value(key, &v)? + CTOK),
            "pivtol" => set!(pivtol, float_value(key, &v)?),
            "pivrel" => set!(pivrel, float_value(key, &v)?),
            "noopiter" => set!(noopiter, bool_value(key, &v)?),
            "trtol" => set!(trtol, float_value(key, &v)?),
            "chgtol" => set!(chgtol, float_value(key, &v)?),
            "method" => {
                let s: String = v
                    .extract()
                    .map_err(|_| PyTypeError::new_err("option method must be a str"))?;
                let m = match s.to_ascii_lowercase().as_str() {
                    "trap" | "trapezoidal" => Method::Trapezoidal,
                    "gear" => Method::Gear,
                    "trbdf2" => Method::TrBdf2,
                    _ => {
                        return Err(PyValueError::new_err(format!(
                            "option method must be 'trap', 'gear' or 'trbdf2', got {s:?}"
                        )));
                    }
                };
                Box::new(move |o: &mut Options| o.method = Some(m)) as Override
            }
            "compat" => {
                let s: String = v
                    .extract()
                    .map_err(|_| PyTypeError::new_err("option compat must be a str"))?;
                let c = match s.to_ascii_lowercase().as_str() {
                    "dune" => Compat::Dune,
                    "spice3" => Compat::Spice3,
                    _ => {
                        return Err(PyValueError::new_err(format!(
                            "option compat must be 'dune' or 'spice3', got {s:?}"
                        )));
                    }
                };
                Box::new(move |o: &mut Options| o.compat = c) as Override
            }
            "ltrafit" => set!(ltra_fit, bool_value(key, &v)?),
            "ltrabkrate" => {
                let v = count_value(key, &v)?;
                Box::new(move |o: &mut Options| o.ltra_break_rate = Some(v)) as Override
            }
            "maxord" => set!(maxord, count_value(key, &v)?),
            "nopage" => set!(nopage, bool_value(key, &v)?),
            "defl" => set!(defl, float_value(key, &v)?),
            "defw" => set!(defw, float_value(key, &v)?),
            "defad" => set!(defad, float_value(key, &v)?),
            "defas" => set!(defas, float_value(key, &v)?),
            "seed" => set!(seed, seed_value(key, &v)?),
            "notrnoise" => set!(notrnoise, bool_value(key, &v)?),
            "devnoise" => set!(dev_noise, bool_value(key, &v)?),
            "devnt" => set!(dev_noise_step, Some(step_value(key, &v)?)),
            "keepopinfo" => set!(keepopinfo, bool_value(key, &v)?),
            "trytocompact" => set!(trytocompact, bool_value(key, &v)?),
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown option '{other}'; valid options: {}",
                    OPTION_NAMES.join(", ")
                )));
            }
        });
    }
    Ok(out)
}

/// A `tmax_divisor` argument: `None`, or a finite number >= 1 (bools
/// refused). It is an argument, not an `options` key: it scales the
/// TMAX of every `.tran` card, a parameter of the analysis card
/// rather than an engine option (`Options::tmax_divisor`).
fn divisor_value(v: Option<&Bound<'_, PyAny>>) -> PyResult<Option<f64>> {
    let Some(v) = v.filter(|v| !v.is_none()) else {
        return Ok(None);
    };
    if v.is_instance_of::<PyBool>() {
        return Err(PyTypeError::new_err(
            "tmax_divisor must be a number, not bool",
        ));
    }
    let d: f64 = v
        .extract()
        .map_err(|_| PyTypeError::new_err("tmax_divisor must be a number"))?;
    if !(d >= 1.0 && d.is_finite()) {
        return Err(PyValueError::new_err(format!(
            "tmax_divisor must be a finite number >= 1, got {d}"
        )));
    }
    Ok(Some(d))
}

/// The caller's `options` and `tmax_divisor` as setters, validated
/// before any parsing work and applied over the deck's `.options`.
fn overrides_of(
    options: Option<&Bound<'_, PyDict>>,
    tmax_divisor: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<Override>> {
    let mut out = match options {
        Some(d) => parse_overrides(d)?,
        None => Vec::new(),
    };
    if let Some(d) = divisor_value(tmax_divisor)? {
        out.push(Box::new(move |o: &mut Options| o.tmax_divisor = Some(d)));
    }
    Ok(out)
}

fn diag_list<'py>(py: Python<'py>, diags: &[Diag]) -> PyResult<Bound<'py, PyList>> {
    let list = PyList::empty(py);
    for d in diags {
        let item = PyDict::new(py);
        item.set_item("severity", d.severity)?;
        item.set_item("message", &d.message)?;
        item.set_item("file", &d.file)?;
        item.set_item("line", d.line)?;
        item.set_item("column", d.column)?;
        list.append(item)?;
    }
    Ok(list)
}

/// One plot as a dict; the columns move into NumPy without a copy.
fn plot_dict<'py>(py: Python<'py>, title: &str, s: Solution) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("title", title)?;
    d.set_item("plotname", &s.plotname)?;
    let vars: Vec<(String, String)> = s
        .variables
        .iter()
        .map(|(n, t)| (n.clone(), t.name().to_string()))
        .collect();
    d.set_item("variables", vars)?;
    d.set_item("scale", s.scale)?;
    let columns = PyList::empty(py);
    let complex = match s.data {
        Data::Real(cols) => {
            for c in cols {
                columns.append(PyArray1::<f64>::from_vec(py, c))?;
            }
            false
        }
        Data::Complex(cols) => {
            for c in cols {
                columns.append(PyArray1::<Complex64>::from_vec(py, c))?;
            }
            true
        }
    };
    d.set_item("complex", complex)?;
    d.set_item("columns", columns)?;
    Ok(d)
}

/// The deck from exactly one of `text` and `path`.
fn source(
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
) -> PyResult<Source> {
    match (text, path) {
        (Some(text), None) => Ok(Source::Text {
            text,
            includes: includes.unwrap_or_default(),
        }),
        (None, Some(path)) => {
            if includes.is_some() {
                return Err(PyValueError::new_err(
                    "includes applies to a netlist string; a deck file resolves .include on disk",
                ));
            }
            Ok(Source::File(path))
        }
        _ => Err(PyValueError::new_err("give exactly one of text and path")),
    }
}

fn meas_dict<'py>(py: Python<'py>, m: &MeasRow) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("name", &m.name)?;
    d.set_item("analysis", m.analysis)?;
    d.set_item("plot", m.plot)?;
    d.set_item("value", m.value)?;
    let details = PyDict::new(py);
    for (k, v) in &m.details {
        details.set_item(*k, *v)?;
    }
    d.set_item("details", details)?;
    d.set_item("error", &m.error)?;
    Ok(d)
}

/// `_measure(plotname, variables, scale, columns, spec, name) -> dict`.
///
/// Rebuild one plot from its parts (the columns are copied out of
/// NumPy) and evaluate the measurement `spec` — a `.measure` body
/// such as `"trig v(1) val=0.5 rise=1 targ v(2) val=0.5 rise=1"` —
/// on it. A malformed `spec` raises `NativeError("measure", …)`; a
/// measurement that does not happen comes back with `error` set.
#[pyfunction]
#[pyo3(signature = (plotname, variables, scale, columns, spec, name))]
fn _measure<'py>(
    py: Python<'py>,
    plotname: String,
    variables: Vec<(String, String)>,
    scale: Option<usize>,
    columns: Vec<Bound<'py, PyAny>>,
    spec: String,
    name: String,
) -> PyResult<Bound<'py, PyDict>> {
    let fail = |msg: String| NativeError::new_err(("measure", msg, PyList::empty(py).unbind()));
    let Some((analysis, measured_as)) = plot_analysis(&plotname) else {
        return Err(fail(format!(
            "plot {plotname:?} cannot be measured: measure limited to tran, dc, sp, ac or pss analysis"
        )));
    };
    if columns.len() != variables.len() {
        return Err(PyValueError::new_err("one column per variable is needed"));
    }
    let mut real = Vec::new();
    let mut complex = Vec::new();
    for c in &columns {
        if let Ok(a) = c.extract::<PyReadonlyArray1<'py, f64>>() {
            real.push(a.as_array().iter().copied().collect::<Vec<f64>>());
        } else if let Ok(a) = c.extract::<PyReadonlyArray1<'py, Complex64>>() {
            complex.push(a.as_array().iter().copied().collect::<Vec<Complex64>>());
        } else {
            return Err(PyTypeError::new_err(
                "columns must be 1-D float64 or complex128 arrays",
            ));
        }
    }
    let data = match (real.is_empty(), complex.is_empty()) {
        (_, true) => Data::Real(real),
        (true, false) => Data::Complex(complex),
        _ => return Err(PyTypeError::new_err("a plot is all real or all complex")),
    };
    let sol = Solution {
        plotname: measured_as.to_string(),
        variables: variables
            .into_iter()
            .map(|(n, t)| (n, VarType::from_name(&t)))
            .collect(),
        data,
        scale,
    };
    let func = match dune_netlist::parse_measure_body(&spec) {
        Ok((f, _warnings)) => f,
        Err(e) => {
            return Err(fail(format!(
                "{} (at column {} of {spec:?})",
                e.message,
                e.start + 1
            )));
        }
    };
    let result = py.detach(|| {
        catch_unwind(AssertUnwindSafe(|| sol.measure(&func)))
            .map_err(|_| "internal error in the measurement (please report)".to_string())
    });
    match result {
        Ok(r) => meas_dict(py, &MeasRow::new(&name, analysis, None, &r)),
        Err(msg) => Err(NativeError::new_err((
            "internal",
            msg,
            PyList::empty(py).unbind(),
        ))),
    }
}

/// `_simulate(text, path, includes, options, tmax_divisor) -> dict`.
///
/// Exactly one of `text` (a deck; `.include` from `includes`) and
/// `path` (a deck file; `.include` on disk) is given. Returns
/// `{"title", "plots": [{"title", "plotname", "variables": [(name,
/// type)], "scale": int | None, "complex": bool, "columns":
/// [ndarray]}], "measurements": [dict], "notes": [str], "diagnostics":
/// [dict]}`. The GIL is
/// released while the simulation runs.
#[pyfunction]
#[pyo3(signature = (text, path, includes, options, tmax_divisor=None))]
fn _simulate<'py>(
    py: Python<'py>,
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
    options: Option<Bound<'py, PyDict>>,
    tmax_divisor: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyDict>> {
    let source = source(text, path, includes)?;
    // Validated before any parsing work; applied over the deck's
    // `.options` inside `run`.
    let overrides = overrides_of(options.as_ref(), tmax_divisor.as_ref())?;
    let outcome = py.detach(|| guarded(|| run(&source, &overrides)));

    outcome_dict(py, outcome)
}

/// A finished `.pss`, still in Rust form.
struct PssOut {
    /// The time-domain and frequency-domain plots, with the run's
    /// notes and diagnostics.
    output: Output,
    period: f64,
    t0: f64,
    residuals: Vec<f64>,
    periods: Vec<f64>,
}

/// Parse and elaborate the deck, then run [`Circuit::pss`] with `p`
/// (the deck's own analysis cards are not run).
fn run_pss(source: &Source, overrides: &[Override], p: &PssParams) -> Result<PssOut, Failure> {
    let Prepared {
        netlist,
        options,
        diags,
    } = prepare(source, overrides)?;
    let (circuit, mut notes) =
        Circuit::from_netlist(&netlist).map_err(|e| Failure::new("elaborate", e.to_string()))?;
    let pss = circuit.pss(&options, p).map_err(Failure::analysis)?;
    notes.extend(pss.notes);
    Ok(PssOut {
        output: Output {
            title: netlist.title.clone(),
            solutions: vec![pss.time, pss.spectrum],
            measures: Vec::new(),
            notes,
            diags,
        },
        period: pss.period,
        t0: pss.t0,
        residuals: pss.residuals,
        periods: pss.periods,
    })
}

/// `_pss(text, path, includes, options, tmax_divisor, freq, tstab,
/// osc_node, points, harmonics, max_iter, steady_coeff, uic) -> dict`.
///
/// The deck as in `_simulate`, its analysis cards ignored; the
/// periodic steady state per [`PssParams`] (`freq` 0 takes the
/// fundamental from the sources). Returns `_simulate`'s dict with the
/// two `.pss` plots (time, then spectrum) plus `"period"`, `"t0"`,
/// `"residuals"` and `"periods"`. A shooting that does not converge
/// raises `NativeError("pss", message, diagnostics, {"iterations",
/// "residuals", "period"})`. The GIL is released while it runs.
#[pyfunction]
#[pyo3(signature = (text, path, includes, options, tmax_divisor, freq, tstab, osc_node, points, harmonics, max_iter, steady_coeff, uic))]
#[allow(clippy::too_many_arguments)]
fn _pss<'py>(
    py: Python<'py>,
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
    options: Option<Bound<'py, PyDict>>,
    tmax_divisor: Option<Bound<'py, PyAny>>,
    freq: f64,
    tstab: f64,
    osc_node: Option<String>,
    points: usize,
    harmonics: usize,
    max_iter: usize,
    steady_coeff: f64,
    uic: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let source = source(text, path, includes)?;
    let overrides = overrides_of(options.as_ref(), tmax_divisor.as_ref())?;
    let params = PssParams {
        freq: (freq != 0.0).then_some(freq),
        tstab,
        osc_node,
        points,
        harmonics,
        max_iter,
        steady_coeff,
        uic,
    };
    let outcome = py.detach(|| guarded(|| run_pss(&source, &overrides, &params)));
    let out = outcome.map_err(|f| native_error(py, f))?;
    let result = outcome_dict(py, Ok(out.output))?;
    result.set_item("period", out.period)?;
    result.set_item("t0", out.t0)?;
    result.set_item("residuals", out.residuals)?;
    result.set_item("periods", out.periods)?;
    Ok(result)
}

/// `NativeError(kind, message, diagnostics)`, with a fourth argument
/// `{"iterations", "residuals", "period"}` for kind `pss`.
fn native_error(py: Python<'_>, f: Failure) -> PyErr {
    let diags = match diag_list(py, &f.diags) {
        Ok(d) => d.unbind(),
        Err(e) => return e,
    };
    match &f.pss {
        Some(h) => {
            let extra = PyDict::new(py);
            match h.fill(&extra) {
                Ok(()) => NativeError::new_err((f.kind, f.message, diags, extra.unbind())),
                Err(e) => e,
            }
        }
        None => NativeError::new_err((f.kind, f.message, diags)),
    }
}

/// A run's outcome as `_simulate`'s result dict, or a `NativeError`.
fn outcome_dict<'py>(
    py: Python<'py>,
    outcome: Result<Output, Failure>,
) -> PyResult<Bound<'py, PyDict>> {
    match outcome {
        Ok(out) => {
            let result = PyDict::new(py);
            result.set_item("title", &out.title)?;
            let plots = PyList::empty(py);
            for s in out.solutions {
                plots.append(plot_dict(py, &out.title, s)?)?;
            }
            result.set_item("plots", plots)?;
            let measures = PyList::empty(py);
            for m in &out.measures {
                measures.append(meas_dict(py, m)?)?;
            }
            result.set_item("measurements", measures)?;
            result.set_item("notes", out.notes)?;
            result.set_item("diagnostics", diag_list(py, &out.diags)?)?;
            Ok(result)
        }
        Err(f) => Err(native_error(py, f)),
    }
}

/// What every run's result of a session carries besides its plots.
struct SessionBase {
    title: String,
    /// Elaboration notes (each result starts with them).
    notes: Vec<String>,
    diags: Vec<Diag>,
    /// The deck's `.measure` cards, evaluated on every run.
    measures: Vec<dune_netlist::ir::Measure>,
}

impl SessionBase {
    fn finish(&self, solutions: Vec<Solution>, analysis_notes: Vec<String>) -> Output {
        let mut notes = self.notes.clone();
        notes.extend(analysis_notes);
        if solutions.is_empty() {
            notes.push("no analyses in the deck; nothing to do".into());
        }
        Output {
            title: self.title.clone(),
            measures: deck_measures(&self.measures, &solutions),
            solutions,
            notes,
            diags: self.diags.clone(),
        }
    }
}

/// `_Session(text, path, includes, options, tmax_divisor)`: a deck parsed and
/// elaborated once, altered and re-run (issue #12). The public
/// wrapper is `spice.Session`; failures raise `NativeError` with the
/// kinds of `_simulate`, plus `alter` for a refused alteration.
#[pyclass(name = "_Session", module = "spice._spice")]
struct NativeSession {
    session: Session,
    base: SessionBase,
}

fn alter_failure(py: Python<'_>, e: dune_core::Error) -> PyErr {
    native_error(py, Failure::new("alter", e.to_string()))
}

#[pymethods]
impl NativeSession {
    #[new]
    #[pyo3(signature = (text, path, includes, options, tmax_divisor=None))]
    fn new(
        py: Python<'_>,
        text: Option<String>,
        path: Option<PathBuf>,
        includes: Option<BTreeMap<String, String>>,
        options: Option<Bound<'_, PyDict>>,
        tmax_divisor: Option<Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let source = source(text, path, includes)?;
        let overrides = overrides_of(options.as_ref(), tmax_divisor.as_ref())?;
        let opened = py.detach(|| {
            guarded(|| {
                let Prepared {
                    netlist,
                    options,
                    diags,
                } = prepare(&source, &overrides)?;
                let (circuit, notes) = Circuit::from_netlist(&netlist)
                    .map_err(|e| Failure::new("elaborate", e.to_string()))?;
                Ok(NativeSession {
                    session: Session::new(circuit, options, netlist.analyses.clone()),
                    base: SessionBase {
                        title: netlist.title.clone(),
                        notes,
                        diags,
                        measures: netlist.measures.clone(),
                    },
                })
            })
        });
        opened.map_err(|f| native_error(py, f))
    }

    /// `alter(refdes, param, value)`: `value` a float or a sequence of
    /// floats (a source function's arguments).
    fn alter(
        &mut self,
        py: Python<'_>,
        refdes: &str,
        param: &str,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let v = if value.is_instance_of::<PyBool>() {
            return Err(PyTypeError::new_err("value must be a number, not bool"));
        } else if let Ok(x) = value.extract::<f64>() {
            AlterValue::Num(x)
        } else if let Ok(v) = value.extract::<Vec<f64>>() {
            AlterValue::List(v)
        } else {
            return Err(PyTypeError::new_err(
                "value must be a number or a sequence of numbers",
            ));
        };
        self.session
            .alter(refdes, param, v)
            .map_err(|e| alter_failure(py, e))
    }

    /// `param(refdes, param)`: a float, a list of floats or `None`.
    fn param<'py>(
        &self,
        py: Python<'py>,
        refdes: &str,
        param: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        match self
            .session
            .param(refdes, param)
            .map_err(|e| alter_failure(py, e))?
        {
            Some(AlterValue::Num(x)) => Ok(x.into_pyobject(py)?.into_any()),
            Some(AlterValue::List(v)) => Ok(PyList::new(py, v)?.into_any()),
            None => Ok(py.None().into_bound(py)),
        }
    }

    fn alterable(&self, py: Python<'_>, refdes: &str) -> PyResult<Vec<&'static str>> {
        self.session
            .circuit()
            .alterable(refdes)
            .map(|p| p.to_vec())
            .map_err(|e| alter_failure(py, e))
    }

    fn set_temp(&mut self, py: Python<'_>, celsius: f64) -> PyResult<()> {
        self.session
            .set_temp(celsius)
            .map_err(|e| alter_failure(py, e))
    }

    fn temp(&self) -> f64 {
        self.session.temp()
    }

    fn set_warm_start(&mut self, on: bool) {
        self.session.set_warm_start(on);
    }

    /// `set_tmax_divisor(d)`: `None` or a finite number >= 1, for the
    /// next runs (the circuit is kept).
    #[pyo3(signature = (d=None))]
    fn set_tmax_divisor(&mut self, d: Option<Bound<'_, PyAny>>) -> PyResult<()> {
        self.session.options_mut().tmax_divisor = divisor_value(d.as_ref())?;
        Ok(())
    }

    fn tmax_divisor(&self) -> Option<f64> {
        self.session.options().tmax_divisor
    }

    fn warm_start(&self) -> bool {
        self.session.warm_start()
    }

    /// `(hits, misses)`.
    fn warm_stats(&self) -> (u64, u64) {
        let s = self.session.warm_stats();
        (s.hits, s.misses)
    }

    /// Re-run every analysis card: `_simulate`'s dict (measurements
    /// included) for the current circuit. The GIL is released while
    /// it runs.
    fn run<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let NativeSession { session, base } = self;
        let outcome = py.detach(|| {
            guarded(|| {
                let (solutions, notes) = session.run().map_err(Failure::analysis)?;
                Ok(base.finish(solutions, notes))
            })
        });
        outcome_dict(py, outcome)
    }
}

/// `_sweep(text, path, includes, options, params, rows, threads,
/// tmax_divisor) -> dict`.
///
/// The deck as in `_simulate`; `params` are the swept quantities (see
/// `dune_core::SweepParam`), `rows` one list of values per run,
/// `threads` 1 for the sequential path, 0 for one per core. Returns
/// `{"title", "params": [canonical key], "diagnostics": [dict],
/// "runs": [{"ok": True, "plots", "measurements", "notes"} | {"ok": False, "kind",
/// "message"}]}` — a failed run of kind `pss` also has `"iterations"`,
/// `"residuals"` and `"period"`. Bad parameter names or rows raise `ValueError`.
/// The GIL is released while the runs execute.
#[pyfunction]
#[pyo3(signature = (text, path, includes, options, params, rows, threads, tmax_divisor=None))]
#[allow(clippy::too_many_arguments)]
fn _sweep<'py>(
    py: Python<'py>,
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
    options: Option<Bound<'py, PyDict>>,
    params: Vec<String>,
    rows: Vec<Vec<f64>>,
    threads: usize,
    tmax_divisor: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyDict>> {
    let source = source(text, path, includes)?;
    let overrides = overrides_of(options.as_ref(), tmax_divisor.as_ref())?;
    let params = params
        .iter()
        .map(|p| SweepParam::parse(p).map_err(|e| PyValueError::new_err(e.to_string())))
        .collect::<PyResult<Vec<_>>>()?;
    let outcome = py.detach(|| guarded(|| run_sweep(&source, &overrides, &params, &rows, threads)));
    let out = match outcome {
        Ok(out) => out,
        Err(f) if f.kind == "sweep" => return Err(PyValueError::new_err(f.message)),
        Err(f) => {
            return Err(NativeError::new_err((
                f.kind,
                f.message,
                diag_list(py, &f.diags)?.unbind(),
            )));
        }
    };
    let result = PyDict::new(py);
    result.set_item("title", &out.title)?;
    result.set_item("params", out.keys)?;
    result.set_item("diagnostics", diag_list(py, &out.diags)?)?;
    let runs = PyList::empty(py);
    for run in out.runs {
        let d = PyDict::new(py);
        match run.outcome {
            Ok(o) => {
                d.set_item("ok", true)?;
                let measures = PyList::empty(py);
                for r in measure_all(&out.measures, &o.solutions) {
                    let m = &out.measures[r.card];
                    let row = MeasRow::new(&m.name, m.analysis.keyword(), Some(r.plot), &r.result);
                    measures.append(meas_dict(py, &row)?)?;
                }
                d.set_item("measurements", measures)?;
                let plots = PyList::empty(py);
                for s in o.solutions {
                    plots.append(plot_dict(py, &out.title, s)?)?;
                }
                d.set_item("plots", plots)?;
                d.set_item("notes", o.notes)?;
            }
            Err(e) => {
                d.set_item("ok", false)?;
                let history = PssHistory::of(&e);
                let kind = match e {
                    dune_core::Error::Build(_)
                    | dune_core::Error::Elab(_)
                    | dune_core::Error::Unsupported { .. } => "elaborate",
                    _ if history.is_some() => "pss",
                    _ => "analysis",
                };
                d.set_item("kind", kind)?;
                d.set_item("message", e.to_string())?;
                if let Some(h) = &history {
                    h.fill(&d)?;
                }
            }
        }
        runs.append(d)?;
    }
    result.set_item("runs", runs)?;
    Ok(result)
}

// --- the figure path (issue #21) -----------------------------------

fn pic_diag_dict<'py>(py: Python<'py>, d: &pic::PicDiag) -> PyResult<Bound<'py, PyDict>> {
    let o = PyDict::new(py);
    o.set_item("severity", d.severity)?;
    o.set_item("kind", d.kind)?;
    o.set_item("message", &d.message)?;
    o.set_item("file", &d.file)?;
    o.set_item("line", d.line)?;
    o.set_item("column", d.column)?;
    o.set_item("end_column", d.end_column)?;
    Ok(o)
}

fn pic_diag_list<'py>(py: Python<'py>, diags: &[pic::PicDiag]) -> PyResult<Bound<'py, PyList>> {
    let l = PyList::empty(py);
    for d in diags {
        l.append(pic_diag_dict(py, d)?)?;
    }
    Ok(l)
}

fn range_dict<'py>(py: Python<'py>, r: &dune_pic::PicRange) -> PyResult<Bound<'py, PyDict>> {
    let o = PyDict::new(py);
    o.set_item("line", r.line)?;
    o.set_item("col", r.col)?;
    o.set_item("end_line", r.end_line)?;
    o.set_item("end_col", r.end_col)?;
    Ok(o)
}

/// The overlay map as plain dicts (the `dune-wasm` `OverlayMapJson`
/// shape); `spice/pic.py` makes the dataclasses.
fn pic_map_dict<'py>(py: Python<'py>, map: &dune_pic::OverlayMap) -> PyResult<Bound<'py, PyDict>> {
    let elements = PyList::empty(py);
    for e in &map.elements {
        let o = PyDict::new(py);
        o.set_item("refdes", &e.refdes)?;
        o.set_item("shapes", (e.shapes.start, e.shapes.end))?;
        let pins = PyList::empty(py);
        for p in &e.pins {
            let d = PyDict::new(py);
            d.set_item("name", &p.name)?;
            d.set_item("net", &p.net)?;
            d.set_item("x", p.x)?;
            d.set_item("y", p.y)?;
            pins.append(d)?;
        }
        o.set_item("pins", pins)?;
        match &e.value {
            Some(v) => {
                let d = PyDict::new(py);
                d.set_item("span", range_dict(py, &v.span)?)?;
                d.set_item("text", &v.text)?;
                o.set_item("value", d)?;
            }
            None => o.set_item("value", py.None())?,
        }
        let params = PyList::empty(py);
        for p in &e.params {
            let d = PyDict::new(py);
            d.set_item("name", &p.name)?;
            d.set_item("span", range_dict(py, &p.span)?)?;
            d.set_item("text", &p.text)?;
            params.append(d)?;
        }
        o.set_item("params", params)?;
        o.set_item("principal", &e.principal)?;
        elements.append(o)?;
    }
    let nets = PyList::empty(py);
    for n in &map.nets {
        let d = PyDict::new(py);
        d.set_item("name", &n.name)?;
        d.set_item("shapes", n.shapes.clone())?;
        nets.append(d)?;
    }
    let c = &map.canvas;
    let canvas = PyDict::new(py);
    canvas.set_item("w", c.w)?;
    canvas.set_item("h", c.h)?;
    canvas.set_item("origin", (c.origin[0], c.origin[1]))?;
    canvas.set_item("rescale", c.rescale)?;
    let out = PyDict::new(py);
    out.set_item("elements", elements)?;
    out.set_item("nets", nets)?;
    out.set_item("canvas", canvas)?;
    Ok(out)
}

/// `NativeError(kind, message, diagnostics)` of a figure that gave no
/// netlist or drawing; the diagnostics are figure diagnostics.
fn pic_error(py: Python<'_>, f: pic::PicFailure) -> PyErr {
    match pic_diag_list(py, &f.diagnostics) {
        Ok(d) => NativeError::new_err((f.kind, f.message, d.unbind())),
        Err(e) => e,
    }
}

/// Run `f`, turning a panic (a bug) into kind `internal`.
fn pic_guarded<T>(f: impl FnOnce() -> Result<T, pic::PicFailure>) -> Result<T, pic::PicFailure> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(pic::PicFailure {
            kind: "internal",
            message: "internal error in the figure extractor (please report)".into(),
            diagnostics: Vec::new(),
        })
    })
}

/// `_pic_extract(text, path, includes, snap) -> dict`: the extraction
/// of a figure given as text (with `includes`) or as a file (its
/// `.include` files read next to it). Returns `{netlist, source,
/// diagnostics, includes, map}`; a figure with no netlist raises
/// `NativeError` (kind `pic`, `io`, `options` or `internal`).
#[pyfunction]
#[pyo3(signature = (text, path, includes, snap=None))]
fn _pic_extract<'py>(
    py: Python<'py>,
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
    snap: Option<f64>,
) -> PyResult<Bound<'py, PyDict>> {
    let includes = includes.unwrap_or_default();
    let source = match (&text, &path) {
        (Some(t), None) => pic::PicSource::Text {
            text: t,
            includes: &includes,
        },
        (None, Some(p)) => pic::PicSource::Path(p),
        _ => return Err(PyValueError::new_err("give exactly one of text and path")),
    };
    let out = py
        .detach(|| pic_guarded(|| pic::extract(source, snap)))
        .map_err(|f| pic_error(py, f))?;
    let d = PyDict::new(py);
    d.set_item("netlist", &out.netlist)?;
    d.set_item("source", &out.source)?;
    d.set_item("diagnostics", pic_diag_list(py, &out.diagnostics)?)?;
    d.set_item("includes", &out.includes)?;
    d.set_item("map", pic_map_dict(py, &out.map)?)?;
    Ok(d)
}

/// `_pic_render(text, path) -> str`: the figure's SVG (typeset
/// `$...$` labels with the `texlabels` feature).
#[pyfunction]
#[pyo3(signature = (text, path))]
fn _pic_render(py: Python<'_>, text: Option<String>, path: Option<PathBuf>) -> PyResult<String> {
    let empty = BTreeMap::new();
    let source = match (&text, &path) {
        (Some(t), None) => pic::PicSource::Text {
            text: t,
            includes: &empty,
        },
        (None, Some(p)) => pic::PicSource::Path(p),
        _ => return Err(PyValueError::new_err("give exactly one of text and path")),
    };
    py.detach(|| pic_guarded(|| pic::render(source)))
        .map_err(|f| pic_error(py, f))
}

/// Run `f`, turning a panic (an engine bug) into kind `internal`.
fn guarded<T>(f: impl FnOnce() -> Result<T, Failure>) -> Result<T, Failure> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
        let what = p
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        Err(Failure::new(
            "internal",
            format!("internal error in the simulator (please report): {what}"),
        ))
    })
}

#[pymodule]
fn _spice(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("OPTION_NAMES", OPTION_NAMES.to_vec())?;
    m.add("NativeError", m.py().get_type::<NativeError>())?;
    m.add_function(wrap_pyfunction!(_simulate, m)?)?;
    m.add_class::<NativeSession>()?;
    m.add_function(wrap_pyfunction!(_sweep, m)?)?;
    m.add_function(wrap_pyfunction!(_measure, m)?)?;
    m.add_function(wrap_pyfunction!(_pss, m)?)?;
    m.add("TEXLABELS", pic::TEXLABELS)?;
    m.add_function(wrap_pyfunction!(_pic_extract, m)?)?;
    m.add_function(wrap_pyfunction!(_pic_render, m)?)?;
    Ok(())
}
