//! Native core of the `spice` Python package (`spice._spice`).
//!
//! One entry point, [`simulate`]: parse a deck (from a string, with
//! `.include` served from an in-memory map like `dune-wasm`, or from a
//! file, with `.include` on disk), elaborate it, run every analysis
//! card through [`Circuit::run`] and hand the plots back as plain
//! Python objects. Columns move into NumPy arrays without a copy
//! (`PyArray1::from_vec` adopts the `Vec`'s buffer).
//!
//! This module is private: the public, typed API is the pure-Python
//! layer in `spice/`. Errors in the deck or the run raise
//! `NativeError(kind, message, diagnostics)`, which that layer turns
//! into `spice.SpiceError`; bad `options` raise `ValueError` /
//! `TypeError` directly. A Rust panic never crosses the boundary — it
//! is caught and reported as kind `internal`.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use dune_core::consts::CTOK;
use dune_core::{Circuit, Data, Method, Options, Solution, SweepParam, SweepRun};
use dune_netlist::ir::Netlist;
use dune_netlist::{Diagnostic, IncludeLoader, ParseOutcome, Severity, SourceMap};
use num_complex::Complex64;
use numpy::PyArray1;
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

/// Every `options` key accepted, in `Options` field order.
const OPTION_NAMES: [&str; 24] = [
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
    "ltrabkrate",
    "maxord",
    "nopage",
    "defl",
    "defw",
    "defad",
    "defas",
];

/// A parser/options diagnostic with its position resolved.
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
/// `analysis` or `internal`.
struct Failure {
    kind: &'static str,
    message: String,
    diags: Vec<Diag>,
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
            diags: Vec::new(),
        }
    }
}

/// A finished run, still in Rust form.
struct Output {
    title: String,
    solutions: Vec<Solution>,
    notes: Vec<String>,
    diags: Vec<Diag>,
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
        let (sols, analysis_notes) = circuit
            .run(analysis, &options)
            .map_err(|e| Failure::new("analysis", e.to_string()))?;
        notes.extend(analysis_notes);
        solutions.extend(sols);
    }
    if solutions.is_empty() {
        notes.push("no analyses in the deck; nothing to do".into());
    }
    Ok(Output {
        title: netlist.title.clone(),
        solutions,
        notes,
        diags,
    })
}

/// A finished sweep, still in Rust form.
struct SweepOut {
    title: String,
    keys: Vec<String>,
    runs: Vec<SweepRun>,
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
                Box::new(move |o: &mut Options| o.method = m) as Override
            }
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

/// `_simulate(text, path, includes, options) -> dict`.
///
/// Exactly one of `text` (a deck; `.include` from `includes`) and
/// `path` (a deck file; `.include` on disk) is given. Returns
/// `{"title", "plots": [{"title", "plotname", "variables": [(name,
/// type)], "scale": int | None, "complex": bool, "columns":
/// [ndarray]}], "notes": [str], "diagnostics": [dict]}`. The GIL is
/// released while the simulation runs.
#[pyfunction]
#[pyo3(signature = (text, path, includes, options))]
fn _simulate<'py>(
    py: Python<'py>,
    text: Option<String>,
    path: Option<PathBuf>,
    includes: Option<BTreeMap<String, String>>,
    options: Option<Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyDict>> {
    let source = source(text, path, includes)?;
    // Validated before any parsing work; applied over the deck's
    // `.options` inside `run`.
    let overrides = match &options {
        Some(d) => parse_overrides(d)?,
        None => Vec::new(),
    };
    let outcome = py.detach(|| guarded(|| run(&source, &overrides)));

    match outcome {
        Ok(out) => {
            let result = PyDict::new(py);
            result.set_item("title", &out.title)?;
            let plots = PyList::empty(py);
            for s in out.solutions {
                plots.append(plot_dict(py, &out.title, s)?)?;
            }
            result.set_item("plots", plots)?;
            result.set_item("notes", out.notes)?;
            result.set_item("diagnostics", diag_list(py, &out.diags)?)?;
            Ok(result)
        }
        Err(f) => Err(NativeError::new_err((
            f.kind,
            f.message,
            diag_list(py, &f.diags)?.unbind(),
        ))),
    }
}

/// `_sweep(text, path, includes, options, params, rows, threads) -> dict`.
///
/// The deck as in `_simulate`; `params` are the swept quantities (see
/// `dune_core::SweepParam`), `rows` one list of values per run,
/// `threads` 1 for the sequential path, 0 for one per core. Returns
/// `{"title", "params": [canonical key], "diagnostics": [dict],
/// "runs": [{"ok": True, "plots", "notes"} | {"ok": False, "kind",
/// "message"}]}`. Bad parameter names or rows raise `ValueError`.
/// The GIL is released while the runs execute.
#[pyfunction]
#[pyo3(signature = (text, path, includes, options, params, rows, threads))]
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
) -> PyResult<Bound<'py, PyDict>> {
    let source = source(text, path, includes)?;
    let overrides = match &options {
        Some(d) => parse_overrides(d)?,
        None => Vec::new(),
    };
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
                let plots = PyList::empty(py);
                for s in o.solutions {
                    plots.append(plot_dict(py, &out.title, s)?)?;
                }
                d.set_item("plots", plots)?;
                d.set_item("notes", o.notes)?;
            }
            Err(e) => {
                d.set_item("ok", false)?;
                let kind = match e {
                    dune_core::Error::Build(_)
                    | dune_core::Error::Elab(_)
                    | dune_core::Error::Unsupported { .. } => "elaborate",
                    _ => "analysis",
                };
                d.set_item("kind", kind)?;
                d.set_item("message", e.to_string())?;
            }
        }
        runs.append(d)?;
    }
    result.set_item("runs", runs)?;
    Ok(result)
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
    m.add_function(wrap_pyfunction!(_sweep, m)?)?;
    Ok(())
}
