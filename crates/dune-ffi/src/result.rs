//! The safe half of the crate: running a deck and holding the
//! outcome in C-ready form. Nothing here is `unsafe`; the boundary
//! functions in `lib.rs` only translate pointers to and from it.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use dune_core::{Circuit, Data, Options, Solution};
use dune_netlist::{Diagnostic, IncludeLoader, ParseOutcome, Severity, SourceMap};

/// File name a text deck is parsed under; `.include` paths resolve
/// relative to it, i.e. as given (same as `dune-wasm` and Python).
pub(crate) const DECK_NAME: &str = "deck.cir";

/// Why a run failed (`DUNE_ERROR_KIND_NONE` on success).
///
/// The kinds are those of the other bindings: `parse`, `elaborate`,
/// `analysis` and `internal` as in `dune-wasm`/Python, `io` as in
/// Python/R (the deck file of `dune_simulate_file` is unreadable) and
/// `input` as in R (an argument the caller passed is unusable: a NULL
/// pointer, text that is not UTF-8, an include map with a NULL
/// entry).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuneErrorKind {
    /// The run succeeded.
    None = 0,
    /// An argument of the call is unusable (NULL, not UTF-8, ...).
    Input = 1,
    /// The deck file could not be read.
    Io = 2,
    /// The deck has a syntax error (see the diagnostics).
    Parse = 3,
    /// The deck parsed but does not describe a buildable circuit.
    Elaborate = 4,
    /// An analysis failed (no convergence, timestep too small, ...).
    Analysis = 5,
    /// A bug in the simulator (a caught Rust panic); please report.
    Internal = 6,
}

/// Severity of a diagnostic.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DuneSeverity {
    Error = 0,
    Warning = 1,
}

/// A parser/options diagnostic with its position resolved.
pub(crate) struct Diag {
    pub severity: DuneSeverity,
    pub message: CString,
    /// `None` (and line/column 0) when the diagnostic has no span.
    pub file: Option<CString>,
    pub line: u32,
    pub column: u32,
}

/// One plot, with its strings already NUL-terminated.
pub(crate) struct Plot {
    pub name: CString,
    pub title: CString,
    pub var_names: Vec<CString>,
    pub var_types: Vec<CString>,
    pub solution: Solution,
}

/// What `dune_simulate*` hands out behind `DuneResult*`: immutable
/// after construction, so every borrowed pointer into it stays valid
/// until `dune_result_free`.
pub struct DuneResult {
    pub(crate) error_kind: DuneErrorKind,
    pub(crate) error_message: Option<CString>,
    pub(crate) notes: Vec<CString>,
    pub(crate) diagnostics: Vec<Diag>,
    pub(crate) plots: Vec<Plot>,
}

/// A run's failure before it becomes a [`DuneResult`].
pub(crate) struct Failure {
    pub kind: DuneErrorKind,
    pub message: String,
    pub diags: Vec<Diag>,
}

impl Failure {
    pub fn new(kind: DuneErrorKind, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
            diags: Vec::new(),
        }
    }
}

/// Where the deck comes from.
pub(crate) enum Source {
    Text {
        text: String,
        includes: BTreeMap<String, String>,
    },
    File(PathBuf),
}

/// A C string from Rust text. Interior NULs (possible: the deck is
/// arbitrary bytes and messages quote it) become U+FFFD so the C
/// side sees the whole message rather than a silent truncation.
pub(crate) fn c_string(s: &str) -> CString {
    CString::new(s.replace('\0', "\u{FFFD}")).unwrap_or_default()
}

impl Diag {
    fn new(d: &Diagnostic, sources: &SourceMap) -> Self {
        let severity = match d.severity {
            Severity::Error => DuneSeverity::Error,
            Severity::Warning => DuneSeverity::Warning,
        };
        let (file, line, column) = match d.span {
            Some(span) => {
                let loc = sources.locate(span);
                (
                    Some(c_string(&loc.file.display().to_string())),
                    loc.line,
                    loc.column,
                )
            }
            None => (None, 0, 0),
        };
        Diag {
            severity,
            message: c_string(&d.message),
            file,
            line,
            column,
        }
    }
}

/// `.include` from the caller's in-memory map (a text deck never
/// touches the file system).
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

/// Parse, elaborate and run every analysis card in deck order (what
/// `dune -b` runs), each through [`Circuit::run`].
fn run(source: &Source) -> Result<DuneResult, Failure> {
    let parsed: ParseOutcome = match source {
        Source::Text { text, includes } => dune_netlist::parse_with_loader(
            Path::new(DECK_NAME),
            text.clone(),
            &mut MapLoader(includes),
        ),
        Source::File(path) => dune_netlist::parse_file(path)
            .map_err(|e| Failure::new(DuneErrorKind::Io, e.to_string()))?,
    };
    let mut diagnostics: Vec<Diag> = parsed
        .diags
        .iter()
        .map(|d| Diag::new(d, &parsed.sources))
        .collect();
    if let Some(first) = parsed.diags.iter().find(|d| d.severity == Severity::Error) {
        return Err(Failure {
            kind: DuneErrorKind::Parse,
            message: first.render(&parsed.sources),
            diags: diagnostics,
        });
    }

    // Warnings found so far ride along with a later failure.
    let fail = |kind, e: dune_core::Error, diags| Failure {
        kind,
        message: e.to_string(),
        diags,
    };
    let (circuit, mut notes) = match Circuit::from_netlist(&parsed.netlist) {
        Ok(c) => c,
        Err(e) => return Err(fail(DuneErrorKind::Elaborate, e, diagnostics)),
    };
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(opt_diags.iter().map(|d| Diag::new(d, &parsed.sources)));

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &parsed.netlist.analyses {
        let (sols, analysis_notes) = match circuit.run(analysis, &options) {
            Ok(r) => r,
            Err(e) => return Err(fail(DuneErrorKind::Analysis, e, diagnostics)),
        };
        notes.extend(analysis_notes);
        solutions.extend(sols);
    }
    if solutions.is_empty() {
        notes.push("no analyses in the deck; nothing to do".into());
    }

    let title = c_string(&parsed.netlist.title);
    Ok(DuneResult {
        error_kind: DuneErrorKind::None,
        error_message: None,
        notes: notes.iter().map(|n| c_string(n)).collect(),
        diagnostics,
        plots: solutions
            .into_iter()
            .map(|s| Plot {
                name: c_string(&s.plotname),
                title: title.clone(),
                var_names: s.variables.iter().map(|(n, _)| c_string(n)).collect(),
                var_types: s
                    .variables
                    .iter()
                    .map(|(_, t)| c_string(t.name()))
                    .collect(),
                solution: s,
            })
            .collect(),
    })
}

impl DuneResult {
    /// A failed run: no plots, no notes; the diagnostics found so far.
    pub(crate) fn failure(f: Failure) -> Self {
        DuneResult {
            error_kind: f.kind,
            error_message: Some(c_string(&f.message)),
            notes: Vec::new(),
            diagnostics: f.diags,
            plots: Vec::new(),
        }
    }

    /// Run `source`; a panic (an engine bug) becomes kind `internal`
    /// and never unwinds into the caller's C frames.
    pub(crate) fn simulate(source: Result<Source, Failure>) -> Self {
        Self::guarded(|| run(&source?))
    }

    /// Run `f`, turning a panic into kind `internal`.
    pub(crate) fn guarded(f: impl FnOnce() -> Result<DuneResult, Failure>) -> Self {
        let outcome = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|p| {
            let what = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(Failure::new(
                DuneErrorKind::Internal,
                format!("internal error in the simulator (please report): {what}"),
            ))
        });
        outcome.unwrap_or_else(DuneResult::failure)
    }

    pub(crate) fn plot(&self, plot: usize) -> Option<&Plot> {
        self.plots.get(plot)
    }
}

impl Plot {
    pub(crate) fn is_complex(&self) -> bool {
        matches!(self.solution.data, Data::Complex(_))
    }

    /// The real column `var`, `None` when out of range or complex.
    pub(crate) fn column_real(&self, var: usize) -> Option<&[f64]> {
        match &self.solution.data {
            Data::Real(cols) => cols.get(var).map(Vec::as_slice),
            Data::Complex(_) => None,
        }
    }

    /// The complex column `var` as a pointer to interleaved `re, im`
    /// doubles and their count (twice the number of points); `None`
    /// when out of range or real.
    ///
    /// `num_complex::Complex<f64>` is `#[repr(C)] { re, im }`, so a
    /// `[Complex64]` of length n is laid out exactly as `[f64; 2n]`
    /// (checked by a test); the pointer cast needs no `unsafe` here,
    /// only the caller's read does.
    pub(crate) fn column_complex(&self, var: usize) -> Option<(*const f64, usize)> {
        match &self.solution.data {
            Data::Complex(cols) => cols
                .get(var)
                .map(|c| (c.as_ptr().cast::<f64>(), 2 * c.len())),
            Data::Real(_) => None,
        }
    }
}
