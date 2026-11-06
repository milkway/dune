//! The JSON shapes `simulate` returns and accepts.
//!
//! Every type here is plain data with `serde` derives; the field
//! names are the JSON keys. The plot shape mirrors
//! [`dune_raw::Plot`] field for field, so a `RawfileJson` carries
//! exactly what `dune -b deck.cir -r out.raw` would write.

use std::collections::BTreeMap;

use dune_netlist::{Diagnostic, Severity, SourceMap};
use dune_raw::{Data, Plot};
use serde::{Deserialize, Serialize};

/// Result of [`crate::simulate_json`]: every plot of the run plus the
/// messages the run produced along the way.
///
/// ```json
/// {
///   "plots": [ PlotJson, ... ],
///   "notes": [ "string", ... ],
///   "diagnostics": [ DiagnosticJson, ... ]
/// }
/// ```
///
/// * `plots` — one per analysis result, in deck order (a `.noise`
///   with a summary contributes two, a `.disto` up to four — the same
///   plots, in the same order, as the CLI's rawfile).
/// * `notes` — informational messages from elaboration and from each
///   analysis (`note: …` lines on the CLI's stderr), in order.
/// * `diagnostics` — the non-fatal parser and `.options`
///   diagnostics (warnings); errors abort the run instead.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawfileJson {
    pub plots: Vec<PlotJson>,
    pub notes: Vec<String>,
    pub diagnostics: Vec<DiagnosticJson>,
}

/// One plot, mirroring [`dune_raw::Plot`] (the 3f5 `struct plot`).
///
/// ```json
/// {
///   "title": "deck title line",
///   "date": "",
///   "plotname": "Transient Analysis",
///   "flags": "real",
///   "commands": [],
///   "options": [],
///   "dimensions": null,
///   "variables": [ { "name": "time", "type": "time" }, ... ],
///   "npoints": 123,
///   "columns": [ [0.0, 1e-9, ...], ... ]
/// }
/// ```
///
/// * `flags` — the rawfile `Flags:` line verbatim: `real` or
///   `complex`, followed by ` unpadded` when the plot is not padded.
/// * `dimensions` — `Dimensions:` of a multidimensional plot (e.g. a
///   nested `.dc`), or `null`.
/// * `variables` — the `Variables:` table in order; `type` is the 3f5
///   type name (`time`, `frequency`, `voltage`, `current`, `pole`,
///   `onoise-spectrum`, …). The first variable is the scale.
/// * `columns` — one array per variable, in `variables` order, each
///   `npoints` long. In a `real` plot the entries are numbers; in a
///   `complex` plot each entry is a `[re, im]` pair (real vectors in a
///   complex plot carry a zero imaginary part, as in the rawfile).
///   Non-finite values (NaN, ±inf) are written as `null`, the only
///   JSON for them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlotJson {
    pub title: String,
    pub date: String,
    pub plotname: String,
    pub flags: String,
    pub commands: Vec<String>,
    pub options: Vec<String>,
    pub dimensions: Option<Vec<usize>>,
    pub variables: Vec<VariableJson>,
    pub npoints: usize,
    pub columns: Columns,
}

/// One `Variables:` entry: name and 3f5 type name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VariableJson {
    pub name: String,
    #[serde(rename = "type")]
    pub vtype: String,
}

/// Plot data, one column per variable. Serialized untagged: plain
/// numbers for real plots, `[re, im]` pairs for complex ones (the
/// plot's `flags` says which).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Columns {
    Real(Vec<Vec<Option<f64>>>),
    Complex(Vec<Vec<[Option<f64>; 2]>>),
}

/// A parser/options diagnostic, with its position resolved.
///
/// ```json
/// { "severity": "warning", "message": "...",
///   "file": "deck.cir", "line": 3, "column": 1 }
/// ```
///
/// `file`, `line` and `column` (1-based; the column counts bytes) are
/// `null` for deck-level diagnostics that have no span.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticJson {
    pub severity: String,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Run options (the `opts` argument of `simulate`). Every field is
/// optional; unknown keys are an error, so a typo does not pass
/// silently.
///
/// ```json
/// { "includes": { "models.lib": "* text of the file\n.model ..." } }
/// ```
///
/// * `includes` — the files a deck may `.include`, by the path the
///   card names (resolved relative to the including file, the deck
///   itself being `deck.cir` at the root). There is no file system in
///   the browser: an `.include` not in this map is a parse error.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SimOptions {
    pub includes: BTreeMap<String, String>,
}

/// Why a run failed, as carried by the thrown JS error.
///
/// ```json
/// { "kind": "parse", "message": "...", "diagnostics": [ DiagnosticJson, ... ] }
/// ```
///
/// `kind` is one of `options` (bad `opts`), `parse` (the deck has
/// errors; `diagnostics` lists all of them, warnings included),
/// `elaborate` (the deck parsed but does not build into a circuit)
/// or `analysis` (an analysis failed: no convergence, timestep too
/// small, singular matrix, …).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimError {
    pub kind: String,
    pub message: String,
    pub diagnostics: Vec<DiagnosticJson>,
}

impl SimError {
    pub(crate) fn new(kind: &str, message: impl Into<String>) -> Self {
        SimError {
            kind: kind.to_string(),
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }
}

impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for SimError {}

impl DiagnosticJson {
    pub(crate) fn from_diag(d: &Diagnostic, sources: &SourceMap) -> Self {
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
        DiagnosticJson {
            severity: severity.to_string(),
            message: d.message.clone(),
            file,
            line,
            column,
        }
    }
}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

impl From<&Plot> for PlotJson {
    fn from(p: &Plot) -> Self {
        let complex = p.is_complex();
        let columns = match &p.data {
            Data::Real(cols) => Columns::Real(
                cols.iter()
                    .map(|c| c.iter().map(|&x| finite(x)).collect())
                    .collect(),
            ),
            Data::Complex(cols) => Columns::Complex(
                cols.iter()
                    .map(|c| c.iter().map(|z| [finite(z.re), finite(z.im)]).collect())
                    .collect(),
            ),
        };
        PlotJson {
            title: p.title.clone(),
            date: p.date.clone(),
            plotname: p.name.clone(),
            flags: format!(
                "{}{}",
                if complex { "complex" } else { "real" },
                if p.padded { "" } else { " unpadded" }
            ),
            commands: p.commands.clone(),
            options: p.options.clone(),
            dimensions: p.dimensions.clone(),
            variables: p
                .variables
                .iter()
                .map(|v| VariableJson {
                    name: v.name.clone(),
                    vtype: v.vtype.name().to_string(),
                })
                .collect(),
            npoints: p.num_points(),
            columns,
        }
    }
}
