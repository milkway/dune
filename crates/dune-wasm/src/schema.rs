//! The JSON shapes `simulate` returns and accepts.
//!
//! Every type here is plain data with `serde` derives; the field
//! names are the JSON keys. The plot shape mirrors
//! [`dune_raw::Plot`] field for field, so a `RawfileJson` carries
//! exactly what `dune -b deck.cir -r out.raw` would write.

use std::collections::BTreeMap;

use dune_netlist::{Diagnostic, Severity, SourceMap};
use dune_pic::{OverlayMap, PicDiagKind, PicDiagnostic, PicSeverity, PicSpan};
use dune_raw::{Data, Plot};
use serde::{Deserialize, Serialize};

use crate::overrides::Overrides;

/// Result of [`crate::simulate_json`]: every plot of the run plus the
/// messages the run produced along the way.
///
/// ```json
/// {
///   "plots": [ PlotJson, ... ],
///   "measurements": [ MeasurementJson, ... ],
///   "fourier": [ FourierJson, ... ],
///   "notes": [ "string", ... ],
///   "diagnostics": [ DiagnosticJson, ... ],
///   "pss": [ PssJson, ... ]
/// }
/// ```
///
/// * `plots` — one per analysis result, in deck order (a `.noise`
///   with a summary contributes two, a `.disto` up to four — the same
///   plots, in the same order, as the CLI's rawfile).
/// * `measurements` — the deck's `.measure` cards evaluated on the
///   run's plots ([`MeasurementJson`]): each card against every plot
///   of its analysis, in plot order then card order (what `dune -b`
///   prints). Empty when the deck has no `.measure` card.
/// * `fourier` — the deck's `.four` cards evaluated on the run's last
///   transient plot ([`FourierJson`]): one row per harmonic of each
///   vector, in card order then vector order (what `dune -b` prints).
///   Empty when the deck has no `.four` card.
/// * `notes` — informational messages from elaboration and from each
///   analysis (`note: …` lines on the CLI's stderr), in order.
/// * `diagnostics` — the non-fatal parser and `.options`
///   diagnostics (warnings); errors abort the run instead.
/// * `pss` — one entry per `.pss` card of the deck, in deck order
///   ([`PssJson`]): the numbers of the shooting that the two plots
///   alone do not carry. Empty when the deck has no `.pss` card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawfileJson {
    pub plots: Vec<PlotJson>,
    #[serde(default)]
    pub measurements: Vec<MeasurementJson>,
    #[serde(default)]
    pub fourier: Vec<FourierJson>,
    pub notes: Vec<String>,
    pub diagnostics: Vec<DiagnosticJson>,
    #[serde(default)]
    pub pss: Vec<PssJson>,
}

/// The periodic steady state of one `.pss` card: what Python's
/// `spice.PssResult` and R's `spice_pss` add to the two plots.
///
/// ```json
/// { "plot": 0, "period": 1.9936609e-4, "t0": 4.0e-3,
///   "residuals": [4.9e-3, 1.6e-7], "periods": [1.99366e-4, 1.9936609e-4] }
/// ```
///
/// * `plot` — index in `plots` of the card's time-domain plot; its
///   spectrum is `plot + 1`.
/// * `period` — the period `T` (s): the drive's for a driven circuit,
///   the shooting's result for an autonomous oscillator (where `T` is
///   a Newton unknown, D-0035).
/// * `t0` — start of the reported period.
/// * `residuals` — the period mismatch `max_i |x_i(t0+T) − x_i(t0)| /
///   tol_i` of each shooting iteration (the last one ≤ STEADYCOEFF);
///   `null` for an iteration whose transient failed and was
///   backtracked (an infinite residual).
/// * `periods` — the period of each iteration, aligned with
///   `residuals`: constant for a driven circuit, the Newton iterate of
///   an autonomous one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PssJson {
    pub plot: usize,
    pub period: f64,
    pub t0: f64,
    pub residuals: Vec<Option<f64>>,
    pub periods: Vec<f64>,
}

impl PssJson {
    /// The entry of `pss`, whose time plot is `plots[plot]`.
    pub fn new(pss: &dune_core::Pss, plot: usize) -> Self {
        PssJson {
            plot,
            period: pss.period,
            t0: pss.t0,
            residuals: finite_all(&pss.residuals),
            periods: pss.periods.clone(),
        }
    }
}

/// The shooting history of a `.pss` that did not converge: the extra
/// properties of a [`SimError`] of kind `pss`.
///
/// * `iterations` — the shooting iterations run;
/// * `residuals` — the mismatch of each, in units of the tolerance
///   (as in [`PssJson::residuals`]; `null` for an infinite one);
/// * `period` — the last period iterate of an autonomous oscillator,
///   `null` for a driven circuit (whose period is fixed).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PssFailureJson {
    pub iterations: usize,
    pub residuals: Vec<Option<f64>>,
    pub period: Option<f64>,
}

/// Non-finite values as `null`, as in `columns`.
fn finite_all(v: &[f64]) -> Vec<Option<f64>> {
    v.iter().copied().map(finite).collect()
}

/// One `.measure` result: the flat row Python's `Result.measurements`
/// (`spice.Measurement`) and R's `res$measurements` carry (D-0029).
///
/// ```json
/// { "name": "trise", "analysis": "tran", "plot": 0,
///   "value": 2.1972e-9,
///   "details": { "trig": 1.105e-10, "targ": 2.3077e-9 } }
/// { "name": "never", "analysis": "tran", "plot": 0,
///   "value": null, "details": {}, "error": "out of interval (WHEN)" }
/// ```
///
/// * `name` — the result name, lowercased as on the card.
/// * `analysis` — `tran`, `ac`, `dc` or `sp` (the card's first word).
/// * `plot` — index of the measured plot in `plots`.
/// * `value` — the result (a time for `WHEN` and `TRIG/TARG`, a value
///   for `FIND`/`MIN`/`AVG`…, the scale value for `MIN_AT`/`MAX_AT`).
///   Always present: `null` when the measurement failed (Python's
///   `nan`) and, as in `columns`, for a non-finite result.
/// * `details` — the extra numbers ngspice prints beside it, by its
///   labels and in its order: `trig`/`targ`, `at`, `with`, `from`/`to`.
///   Always present; `{}` for `WHEN`, `FIND`, `DERIV` and failures.
/// * `error` — why the measurement failed (`out of interval (WHEN)`,
///   an unknown vector…). **Absent** on success (Python's `None`): a
///   row is a failure exactly when it has the key.
///
/// A failed measurement is a row, never a thrown error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeasurementJson {
    pub name: String,
    pub analysis: String,
    pub plot: usize,
    pub value: Option<f64>,
    #[serde(with = "ordered_map")]
    pub details: Vec<(String, Option<f64>)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl MeasurementJson {
    /// The row of one card evaluated on plot `plot`.
    pub(crate) fn from_outcome(
        card: &dune_netlist::ir::Measure,
        plot: usize,
        r: &Result<dune_core::measure::Measurement, dune_core::measure::MeasureError>,
    ) -> Self {
        let (value, details, error) = match r {
            Ok(m) => (
                finite(m.value),
                m.detail
                    .pairs()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), finite(v)))
                    .collect(),
                None,
            ),
            Err(e) => (None, Vec::new(), Some(e.describe())),
        };
        MeasurementJson {
            name: card.name.clone(),
            analysis: card.analysis.keyword().to_string(),
            plot,
            value,
            details,
            error,
        }
    }
}

/// One harmonic of one `.four` vector: the flat row Python's
/// `Result.fourier` and R's `res$fourier` carry (D-0061).
///
/// ```json
/// { "card": 0, "vector": "v(out)", "harmonic": 1, "frequency": 1000,
///   "magnitude": 0.846, "phase": -32.15, "norm_magnitude": 1,
///   "norm_phase": 0, "thd": 0.000637, "gridsize": 200,
///   "polydegree": 1, "nperiods": 1 }
/// { "card": 0, "vector": "v(7)", "harmonic": null, "frequency": null,
///   "magnitude": null, "phase": null, "norm_magnitude": null,
///   "norm_phase": null, "thd": null, "gridsize": null,
///   "polydegree": null, "nperiods": null,
///   "error": "no such vector v(7)" }
/// ```
///
/// * `card` — position of the card among the deck's `.four` cards
///   (0 for the first).
/// * `vector` — the vector as ngspice names it (`v(out)`, `v(a,b)`,
///   `i(v1)`; plot names for `all`).
/// * `harmonic` — 0 for DC, then 1 to `nfreqs − 1`.
/// * `frequency` — `harmonic · f` (Hz).
/// * `magnitude` — the harmonic's amplitude (the signed mean for DC).
/// * `phase` — degrees, referred to a sine (0 for DC).
/// * `norm_magnitude`, `norm_phase` — over the fundamental's magnitude,
///   minus its phase (not wrapped; 0 for DC).
/// * `thd` — the vector's total harmonic distortion (%), repeated on
///   each of its rows.
/// * `gridsize`, `polydegree`, `nperiods` — the points transformed,
///   the interpolation degree and the periods (the deck's `.options`
///   `fourgridsize·nperiods`, `polydegree`, `nperiods`).
///
/// The numbers are `null` when not finite. A vector without a table
/// (no transient plot, an unknown vector, a run shorter than the
/// periods) is **one** row with every field after `vector` `null` and
/// `error` set; `error` is **absent** on success, as in
/// [`MeasurementJson`]. A failure is a row, never a thrown error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FourierJson {
    pub card: usize,
    pub vector: String,
    pub harmonic: Option<usize>,
    pub frequency: Option<f64>,
    pub magnitude: Option<f64>,
    pub phase: Option<f64>,
    pub norm_magnitude: Option<f64>,
    pub norm_phase: Option<f64>,
    pub thd: Option<f64>,
    pub gridsize: Option<usize>,
    pub polydegree: Option<usize>,
    pub nperiods: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FourierJson {
    /// The rows of one vector of card `card` (its position among the
    /// `.four` cards).
    pub(crate) fn rows(
        card: usize,
        vector: &str,
        r: &Result<dune_core::Fourier, dune_core::FourError>,
    ) -> Vec<Self> {
        match r {
            Ok(f) => f
                .harmonics
                .iter()
                .enumerate()
                .map(|(k, h)| FourierJson {
                    card,
                    vector: vector.to_string(),
                    harmonic: Some(k),
                    frequency: finite(h.freq),
                    magnitude: finite(h.mag),
                    phase: finite(h.phase),
                    norm_magnitude: finite(h.norm_mag),
                    norm_phase: finite(h.norm_phase),
                    thd: finite(f.thd),
                    gridsize: Some(f.gridsize),
                    polydegree: Some(f.polydegree),
                    nperiods: Some(f.nperiods),
                    error: None,
                })
                .collect(),
            Err(e) => vec![FourierJson {
                card,
                vector: vector.to_string(),
                harmonic: None,
                frequency: None,
                magnitude: None,
                phase: None,
                norm_magnitude: None,
                norm_phase: None,
                thd: None,
                gridsize: None,
                polydegree: None,
                nperiods: None,
                error: Some(e.to_string()),
            }],
        }
    }
}

/// `(key, value)` pairs as a JSON object, in their order (serde_json's
/// own map would sort the keys: `targ` before `trig`).
mod ordered_map {
    use serde::de::{MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        pairs: &[(String, Option<f64>)],
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(pairs.len()))?;
        for (k, v) in pairs {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Vec<(String, Option<f64>)>, D::Error> {
        struct Pairs;
        impl<'de> Visitor<'de> for Pairs {
            type Value = Vec<(String, Option<f64>)>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of numbers")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(entry) = a.next_entry()? {
                    out.push(entry);
                }
                Ok(out)
            }
        }
        d.deserialize_map(Pairs)
    }
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
///
/// Diagnostics of a pic figure (`extract_pic`/`simulate_pic`, and the
/// `pic` errors) carry two more keys, absent everywhere else:
///
/// ```json
/// { "severity": "warning", "kind": "dangling-pin", "message": "...",
///   "file": null, "line": 4, "column": 5, "end_column": 26 }
/// ```
///
/// * `kind` — what the diagnostic is about: `dangling-pin`,
///   `floating-net`, `missing-value`, `duplicate-refdes`,
///   `unknown-element`, `missing-pin`, `unlabeled-element`,
///   `node-name-conflict`, `prelude` (extraction), `compile` (rpic
///   rejected the figure) or `not-marked` (no `spice = 1`).
/// * `file` — `null` with a `line` is the user's figure itself;
///   `"spice.pic"` is the dune prelude, `"circuits"` rpic's std
///   library. `line`/`column`/`end_column` (1-based, end exclusive,
///   in characters as rpic counts them) are `null` when unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticJson {
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
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
/// * `options` — engine options applied after the deck's `.options`
///   cards, with the card's keys and units (`{"trtol": 1, "method":
///   "gear", "temp": 50}`, temperatures in °C). A number is taken
///   exactly; a string is read as the card's text would be (`"1u"`);
///   `true` is the bare flag, `false` clears it. An invalid entry is
///   a warning in the result's `diagnostics` (message prefixed
///   `opts.options:`), as an invalid `.options` card is — not a
///   throw.
/// * `tmaxDivisor` — a finite number ≥ 1: every `.tran` runs with its
///   effective maximum step (the card's TMAX, else
///   `min(TSTEP, span/50)`) divided by it, exactly as if the card
///   carried `TMAX/d` ([`dune_core::Options::tmax_divisor`]). Not an
///   `options` key: TMAX is a parameter of the analysis card. Out of
///   range is an `options` error.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SimOptions {
    pub includes: BTreeMap<String, String>,
    pub options: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "tmaxDivisor")]
    pub tmax_divisor: Option<f64>,
}

impl SimOptions {
    /// The engine overrides of the run.
    pub fn overrides(&self) -> Overrides {
        Overrides {
            options: self.options.clone(),
            tmax_divisor: self.tmax_divisor,
        }
    }
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
/// `analysis` (an analysis failed: no convergence, timestep too
/// small, singular matrix, …) or `pss` (a `.pss` card whose shooting
/// did not converge in its iterations; the error also carries the
/// history as `iterations`, `residuals` and `period`,
/// [`PssFailureJson`]).
///
/// The pic entry points (`extract_pic`, `simulate_pic`) add:
///
/// * `pic` — the figure produced no deck: rpic rejected it
///   (`diagnostics` = one `compile` entry with its position), it is
///   not marked `spice = 1` (one `not-marked` entry), or — in
///   `simulate_pic` only — extraction reported errors (`diagnostics`
///   = all extraction diagnostics, and the error's `pic` property
///   holds the [`PicJson`], netlist included).
/// * `internal` — a bug: the Rust side panicked. Caught on native
///   builds only; in wasm a panic traps (`WebAssembly.RuntimeError`)
///   and the instance must be discarded.
///
/// A `simulate_pic` failure after extraction (`parse`, `elaborate`,
/// `analysis`) also carries the extraction as the error's `pic`
/// property, so the extracted netlist can be shown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimError {
    pub kind: String,
    pub message: String,
    pub diagnostics: Vec<DiagnosticJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pic: Option<Box<PicJson>>,
    /// Kind `pss` only: the shooting history (flat on the JS error).
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub pss: Option<Box<PssFailureJson>>,
}

impl SimError {
    pub(crate) fn new(kind: &str, message: impl Into<String>) -> Self {
        SimError {
            kind: kind.to_string(),
            message: message.into(),
            diagnostics: Vec::new(),
            pic: None,
            pss: None,
        }
    }

    /// An engine failure: kind `pss`, with the shooting history, for a
    /// `.pss` that did not converge; kind `analysis` otherwise.
    pub(crate) fn analysis(e: dune_core::Error) -> Self {
        let message = e.to_string();
        match e {
            dune_core::Error::PssConvergence {
                iterations,
                residuals,
                period,
            } => SimError {
                pss: Some(Box::new(PssFailureJson {
                    iterations,
                    residuals: finite_all(&residuals),
                    period,
                })),
                ..SimError::new("pss", message)
            },
            _ => SimError::new("analysis", message),
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
            kind: None,
            message: d.message.clone(),
            file,
            line,
            column,
            end_column: None,
        }
    }

    pub(crate) fn from_pic(d: &PicDiagnostic) -> Self {
        let severity = match d.severity {
            PicSeverity::Error => "error",
            PicSeverity::Warning => "warning",
        };
        let kind = match d.kind {
            PicDiagKind::DanglingPin => "dangling-pin",
            PicDiagKind::FloatingNet => "floating-net",
            PicDiagKind::MissingValue => "missing-value",
            PicDiagKind::DuplicateRefdes => "duplicate-refdes",
            PicDiagKind::UnknownElement => "unknown-element",
            PicDiagKind::MissingPin => "missing-pin",
            PicDiagKind::UnlabeledElement => "unlabeled-element",
            PicDiagKind::NodeNameConflict => "node-name-conflict",
            PicDiagKind::Prelude => "prelude",
        };
        Self::pic_located(severity, kind, d.message.clone(), d.span.as_ref())
    }

    pub(crate) fn pic_located(
        severity: &str,
        kind: &str,
        message: String,
        span: Option<&PicSpan>,
    ) -> Self {
        DiagnosticJson {
            severity: severity.to_string(),
            kind: Some(kind.to_string()),
            message,
            file: span.and_then(|s| s.file.clone()),
            line: span.map(|s| s.line),
            column: span.map(|s| s.col),
            end_column: span.map(|s| s.end_col),
        }
    }
}

/// The extraction of a pic figure: what `extract_pic` returns, and the
/// `pic` key of `simulate_pic`'s result.
///
/// ```json
/// {
///   "netlist": "* netlist extracted by dune-pic\nR1 in out 1k\n...\n.end\n",
///   "diagnostics": [ DiagnosticJson, ... ],
///   "map": OverlayMapJson
/// }
/// ```
///
/// * `netlist` — the SPICE3 deck, title line first, ending in `.end`.
/// * `diagnostics` — the extraction diagnostics, warnings and errors,
///   each with its pic `kind` (see [`DiagnosticJson`]). An `error`
///   means an element was skipped or its card is incomplete: the
///   netlist is still returned by `extract_pic`, but `simulate_pic`
///   refuses to run it (kind `pic`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PicJson {
    pub netlist: String,
    pub diagnostics: Vec<DiagnosticJson>,
    pub map: OverlayMapJson,
}

/// Map from netlist names back to the shapes of the compiled figure
/// (mirrors [`dune_pic::OverlayMap`]), for drawing results over it.
///
/// ```json
/// {
///   "elements": [ { "refdes": "R1", "shapes": { "start": 12, "end": 19 },
///                   "pins": [ { "name": "P", "net": "in", "x": 0, "y": 1.5 },
///                             { "name": "N", "net": "out", "x": 2.5, "y": 1.5 } ] }, ... ],
///   "nets": [ { "name": "0", "shapes": [ 30, 31, 32 ] }, { "name": "out", "shapes": [ 27 ] }, ... ],
///   "canvas": { "w": 3.38, "h": 2.20, "origin": [0.62, 0.35], "rescale": 1 }
/// }
/// ```
///
/// Each element also carries `value`, `params` and `principal` (see
/// [`ElementOverlayJson`]), for editors that write a value back into
/// the figure source.
///
/// Shape numbers are indices into the drawing rpic compiles from the
/// figure **with the dune prelude** (`spice.pic` after rpic's circuit
/// library): number `N` is the SVG group `<g id="sN">`. The Studio must
/// therefore render the same prelude-prefixed source for the ids to
/// line up.
///
/// * `elements` — one per emitted card, in card order; `shapes` is
///   the element block's half-open range: ids `s{start}` …
///   `s{end-1}`; `pins` are its terminals in card order (none for a
///   `K`): the prelude pin `name` (`P`, `N`, `C`, `B`, `E`, `D`, `G`,
///   `S`, …), the `net` it is on (a `name` of `nets`) and the terminal
///   point `x`, `y` in pic inches — the frame of the drawing's shapes
///   and of rpic's scene (y up), which the scene maps to SVG pixels.
///   Pins are drawn inside the element, so a net whose elements touch
///   pin to pin has no `shapes` of its own; its pins are where to probe
///   it.
/// * `nets` — named nets in order of first appearance, ground (`0`)
///   first when present; `shapes` are the net's wires, junction dots
///   and ground symbols, sorted (named places draw nothing, so a net
///   that is only pins has an empty list). `name` is the node name in
///   the netlist, i.e. the `x` of the `v(x)` result vectors.
/// * `canvas` — the frame of `render_pic`'s SVG
///   ([`dune_pic::CanvasOverlay`]): a pin `(x, y)` is at SVG user units
///   `sx = (x + origin[0]) · 96`, `sy = (h − (y + origin[1])) · 96`,
///   exactly (`viewBox` = `0 0 w·96 h·96`; `rescale` is informational,
///   the pins already carry it).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OverlayMapJson {
    pub elements: Vec<ElementOverlayJson>,
    pub nets: Vec<NetOverlayJson>,
    #[serde(default)]
    pub canvas: CanvasJson,
}

/// [`OverlayMapJson::canvas`] (mirrors [`dune_pic::CanvasOverlay`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanvasJson {
    pub w: f64,
    pub h: f64,
    pub origin: [f64; 2],
    pub rescale: f64,
}

impl Default for CanvasJson {
    fn default() -> Self {
        let c = dune_pic::CanvasOverlay::default();
        CanvasJson {
            w: c.w,
            h: c.h,
            origin: c.origin,
            rescale: c.rescale,
        }
    }
}

/// One element of [`OverlayMapJson`] (mirrors
/// [`dune_pic::ElementOverlay`]).
///
/// * `value` — `{ "span": SourceRangeJson, "text": "1k" }`: where the
///   value/model argument is in the user's source, `null` when there is
///   no trustworthy one (no value argument, a `K`, an inline block, a
///   wrapper macro's call). A display text never moves it.
/// * `params` — `[{ "name": "acmag", "span": …, "text": "1" }, …]`:
///   the alterable parameters spelled inside `value`, named as
///   `alter(refdes, param, …)` takes them; a scalar covers its number
///   only, a transient function (`pulse`, `sin`, …) its whole spelling.
/// * `principal` — the parameter `alter(refdes, value)` changes
///   (`resistance`, `capacitance`, `inductance`, `dc`, `gain`, `area`),
///   `null` for letters without one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementOverlayJson {
    pub refdes: String,
    pub shapes: ShapeRangeJson,
    pub pins: Vec<PinOverlayJson>,
    #[serde(default)]
    pub value: Option<ValueSpanJson>,
    #[serde(default)]
    pub params: Vec<ParamSpanJson>,
    #[serde(default)]
    pub principal: Option<String>,
}

/// A range of the user's figure source (mirrors
/// [`dune_pic::PicRange`]): 1-based, end exclusive, columns in chars
/// (Unicode code points, not UTF-16 units).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRangeJson {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// [`ElementOverlayJson::value`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueSpanJson {
    pub span: SourceRangeJson,
    pub text: String,
}

/// One entry of [`ElementOverlayJson::params`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamSpanJson {
    pub name: String,
    pub span: SourceRangeJson,
    pub text: String,
}

impl From<&dune_pic::PicRange> for SourceRangeJson {
    fn from(r: &dune_pic::PicRange) -> Self {
        SourceRangeJson {
            line: r.line,
            col: r.col,
            end_line: r.end_line,
            end_col: r.end_col,
        }
    }
}

/// One terminal of an [`ElementOverlayJson`] (mirrors
/// [`dune_pic::PinOverlay`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PinOverlayJson {
    pub name: String,
    pub net: String,
    pub x: f64,
    pub y: f64,
}

/// A half-open range of shape numbers, `{ "start": a, "end": b }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShapeRangeJson {
    pub start: usize,
    pub end: usize,
}

/// One net of [`OverlayMapJson`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetOverlayJson {
    pub name: String,
    pub shapes: Vec<usize>,
}

impl From<&OverlayMap> for OverlayMapJson {
    fn from(m: &OverlayMap) -> Self {
        OverlayMapJson {
            elements: m
                .elements
                .iter()
                .map(|e| ElementOverlayJson {
                    refdes: e.refdes.clone(),
                    shapes: ShapeRangeJson {
                        start: e.shapes.start,
                        end: e.shapes.end,
                    },
                    pins: e
                        .pins
                        .iter()
                        .map(|p| PinOverlayJson {
                            name: p.name.clone(),
                            net: p.net.clone(),
                            x: p.x,
                            y: p.y,
                        })
                        .collect(),
                    value: e.value.as_ref().map(|v| ValueSpanJson {
                        span: (&v.span).into(),
                        text: v.text.clone(),
                    }),
                    params: e
                        .params
                        .iter()
                        .map(|p| ParamSpanJson {
                            name: p.name.clone(),
                            span: (&p.span).into(),
                            text: p.text.clone(),
                        })
                        .collect(),
                    principal: e.principal.clone(),
                })
                .collect(),
            nets: m
                .nets
                .iter()
                .map(|n| NetOverlayJson {
                    name: n.name.clone(),
                    shapes: n.shapes.clone(),
                })
                .collect(),
            canvas: CanvasJson {
                w: m.canvas.w,
                h: m.canvas.h,
                origin: m.canvas.origin,
                rescale: m.canvas.rescale,
            },
        }
    }
}

/// Result of `simulate_pic`: a [`RawfileJson`] (its keys at the top
/// level) plus the extraction under `pic`.
///
/// ```json
/// { "plots": [...], "measurements": [...], "fourier": [...], "notes": [...],
///   "diagnostics": [...], "pic": PicJson }
/// ```
///
/// The top-level `diagnostics` are the netlist parser's and
/// `.options`' (file `deck.cir`, the extracted deck); the figure's are
/// in `pic.diagnostics`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PicRunJson {
    #[serde(flatten)]
    pub raw: RawfileJson,
    pub pic: PicJson,
}

/// Options of `extract_pic`. Unknown keys are an error.
///
/// ```json
/// { "snap": 0.001 }
/// ```
///
/// * `snap` — distance (pic inches) under which two terminals are the
///   same point; a positive finite number, default `0.001`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ExtractOptionsJson {
    pub snap: Option<f64>,
}

/// Options of `simulate_pic`: those of `extract_pic` and of
/// `simulate`. Unknown keys are an error.
///
/// ```json
/// { "snap": 0.001, "includes": { "models.lib": "..." },
///   "options": { "trtol": 1 }, "tmaxDivisor": 10 }
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PicSimOptions {
    pub snap: Option<f64>,
    pub includes: BTreeMap<String, String>,
    pub options: serde_json::Map<String, serde_json::Value>,
    #[serde(rename = "tmaxDivisor")]
    pub tmax_divisor: Option<f64>,
}

impl PicSimOptions {
    /// The engine overrides of the run.
    pub fn overrides(&self) -> Overrides {
        Overrides {
            options: self.options.clone(),
            tmax_divisor: self.tmax_divisor,
        }
    }
}

/// Options of `render_pic`: none yet, so `{}` (or no argument).
/// Unknown keys are an error.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct RenderOptionsJson {}

fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

impl From<&Plot> for PlotJson {
    fn from(p: &Plot) -> Self {
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
            columns,
            ..PlotJson::header(p)
        }
    }
}

impl PlotJson {
    /// Everything of the plot but its data: `columns` is empty (`[]`
    /// on the wire); `npoints` and `variables` still describe the data.
    /// The typed-array entry points fill `columns` in afterwards.
    pub fn header(p: &Plot) -> Self {
        let complex = p.is_complex();
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
            columns: Columns::Real(Vec::new()),
        }
    }
}
