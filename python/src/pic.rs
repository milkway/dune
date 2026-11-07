//! The figure path (issue #21): an rpic figure → SPICE3 netlist,
//! overlay map and SVG, through `dune-pic`'s public API only (as the
//! CLI uses it). Plain Rust types, so `cargo test` exercises it
//! without a Python interpreter; `lib.rs` converts to Python objects.
//! (Same logic as the R package's `r/src/rust/src/pic.rs`; keep the
//! two in step — mirror rule.)

use std::collections::BTreeMap;
use std::path::Path;

use dune_netlist::IncludeLoader;
use dune_pic::{
    ExtractOptions, OverlayMap, PicDiagKind, PicDiagnostic, PicError, PicSeverity, PicSpan,
};

/// Where the figure comes from.
pub enum PicSource<'a> {
    /// A `.pic` file: its `.include` cards resolve on disk, relative to
    /// the figure (as `dune pic --run` does).
    Path(&'a Path),
    /// Figure text: `.include` is served from `includes` only.
    Text {
        text: &'a str,
        includes: &'a BTreeMap<String, String>,
    },
}

/// A figure diagnostic with its span flattened (the `dune-wasm`
/// `DiagnosticJson` of a pic diagnostic: same `kind` names).
#[derive(Debug, Clone, PartialEq)]
pub struct PicDiag {
    pub severity: &'static str,
    pub kind: &'static str,
    pub message: String,
    /// `None` = the user's figure; `spice.pic` (the prelude) or
    /// `circuits` (rpic's std library) otherwise.
    pub file: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub end_column: Option<u32>,
}

impl PicDiag {
    fn located(
        severity: &'static str,
        kind: &'static str,
        message: String,
        span: Option<&PicSpan>,
    ) -> Self {
        PicDiag {
            severity,
            kind,
            message,
            file: span.and_then(|s| s.file.clone()),
            line: span.map(|s| s.line),
            column: span.map(|s| s.col),
            end_column: span.map(|s| s.end_col),
        }
    }

    fn from_pic(d: &PicDiagnostic) -> Self {
        let severity = match d.severity {
            PicSeverity::Error => "error",
            PicSeverity::Warning => "warning",
        };
        Self::located(
            severity,
            diag_kind(d.kind),
            d.message.clone(),
            d.span.as_ref(),
        )
    }
}

/// The `dune-wasm` names of the extraction diagnostic kinds.
pub fn diag_kind(k: PicDiagKind) -> &'static str {
    match k {
        PicDiagKind::DanglingPin => "dangling-pin",
        PicDiagKind::FloatingNet => "floating-net",
        PicDiagKind::MissingValue => "missing-value",
        PicDiagKind::DuplicateRefdes => "duplicate-refdes",
        PicDiagKind::UnknownElement => "unknown-element",
        PicDiagKind::MissingPin => "missing-pin",
        PicDiagKind::UnlabeledElement => "unlabeled-element",
        PicDiagKind::NodeNameConflict => "node-name-conflict",
        PicDiagKind::Prelude => "prelude",
    }
}

/// Why a figure gave no netlist (or no drawing): kind `io` (the
/// figure file cannot be read), `options` (a bad `snap`) or `pic` (rpic
/// rejected the figure, or it is not marked `spice = 1`), with one
/// diagnostic of kind `compile` / `not-marked` for `pic`.
#[derive(Debug, Clone, PartialEq)]
pub struct PicFailure {
    pub kind: &'static str,
    pub message: String,
    pub diagnostics: Vec<PicDiag>,
}

impl PicFailure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        PicFailure {
            kind,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// `name` labels the figure in the message (its path, or nothing
    /// for figure text).
    fn pic(e: PicError, name: Option<&str>) -> Self {
        let (kind, span, message) = match &e {
            PicError::Compile { message, span } => ("compile", span.as_ref(), message.clone()),
            PicError::NotMarked => ("not-marked", None, e.to_string()),
        };
        let at = match (name, span) {
            (Some(n), Some(s)) if s.file.is_none() => format!("{n}:{s}: "),
            (Some(n), None) => format!("{n}: "),
            (_, Some(s)) => format!("{s}: "),
            (None, None) => String::new(),
        };
        PicFailure {
            kind: "pic",
            message: format!("{at}{message}"),
            diagnostics: vec![PicDiag::located("error", kind, message, span)],
        }
    }
}

/// An extraction: the netlist, its diagnostics, the overlay map, the
/// figure text and the `.include` files its netlist reads.
pub struct PicOut {
    pub netlist: String,
    pub diagnostics: Vec<PicDiag>,
    pub map: OverlayMap,
    /// The figure source (what [`render`] draws).
    pub source: String,
    /// By the path its `.include` card names, resolved relative to the
    /// including file with the netlist at the root — the `includes` a
    /// text deck takes. For a figure file, the files read from disk
    /// next to it at extraction; for figure text, the caller's map.
    pub includes: BTreeMap<String, String>,
}

/// Records every file the netlist's `.include` cards read from disk,
/// keyed relative to the figure's directory.
struct RecordingLoader<'a> {
    base: &'a Path,
    seen: BTreeMap<String, String>,
}

impl IncludeLoader for RecordingLoader<'_> {
    fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
        let text = dune_netlist::deck::FsLoader.load(path, limit)?;
        let key = path.strip_prefix(self.base).unwrap_or(path);
        self.seen
            .insert(key.to_string_lossy().into_owned(), text.clone());
        Ok(text)
    }
}

/// The `.include` files `netlist` reads, as a deck in `dir` would.
/// A file that cannot be read is left out: running the netlist then
/// reports it as a parse error naming the card.
fn includes_near(netlist: &str, dir: &Path) -> BTreeMap<String, String> {
    let mut loader = RecordingLoader {
        base: dir,
        seen: BTreeMap::new(),
    };
    let _ =
        dune_netlist::parse_with_loader(&dir.join("deck.cir"), netlist.to_string(), &mut loader);
    loader.seen
}

fn options(snap: Option<f64>) -> Result<ExtractOptions<'static>, PicFailure> {
    let mut opts = ExtractOptions::default();
    if let Some(snap) = snap {
        if !(snap.is_finite() && snap > 0.0) {
            return Err(PicFailure::new(
                "options",
                format!("snap must be a positive finite number, got {snap}"),
            ));
        }
        opts.snap = snap;
    }
    Ok(opts)
}

fn read(path: &Path) -> Result<String, PicFailure> {
    std::fs::read_to_string(path)
        .map_err(|e| PicFailure::new("io", format!("cannot read {}: {e}", path.display())))
}

/// Extract the netlist of a figure (`dune_pic::extract`, embedded
/// prelude). Extraction diagnostics, errors included, come back in
/// [`PicOut::diagnostics`]; only a figure that yields no netlist fails.
pub fn extract(source: PicSource<'_>, snap: Option<f64>) -> Result<PicOut, PicFailure> {
    let opts = options(snap)?;
    let (text, name) = match &source {
        PicSource::Path(p) => (read(p)?, Some(p.display().to_string())),
        PicSource::Text { text, .. } => (text.to_string(), None),
    };
    let ex = dune_pic::extract(&text, &opts).map_err(|e| PicFailure::pic(e, name.as_deref()))?;
    let includes = match source {
        PicSource::Path(p) => includes_near(&ex.netlist, p.parent().unwrap_or(Path::new(""))),
        PicSource::Text { includes, .. } => includes.clone(),
    };
    Ok(PicOut {
        diagnostics: ex.diagnostics.iter().map(PicDiag::from_pic).collect(),
        netlist: ex.netlist,
        map: ex.map,
        source: text,
        includes,
    })
}

/// Render a figure to SVG (`dune_pic::render_svg`): the drawing whose
/// shape ids the overlay map refers to. `spice = 1` is not required.
/// With the `texlabels` feature (default) rpic's RaTeX math renderer
/// is registered first, so `$...$` labels of a `texlabels = 1` figure
/// come out typeset, as in `dune pic --svg`.
pub fn render(source: PicSource<'_>) -> Result<String, PicFailure> {
    register_math();
    let (text, name) = match &source {
        PicSource::Path(p) => (read(p)?, Some(p.display().to_string())),
        PicSource::Text { text, .. } => (text.to_string(), None),
    };
    dune_pic::render_svg(&text, &ExtractOptions::default())
        .map_err(|e| PicFailure::pic(e, name.as_deref()))
}

/// rpic's math hook is process-wide and first-call-wins (a
/// `OnceLock`): registering on every render is a no-op after the first.
/// The `rpic-core` pin must equal dune-pic's, or this would set the
/// hook of a second copy nobody reads.
fn register_math() {
    #[cfg(feature = "texlabels")]
    rpic_core::set_math_renderer(rpic_render::math::render_math);
}

/// Whether `$...$` labels are typeset (the `texlabels` feature).
pub const TEXLABELS: bool = cfg!(feature = "texlabels");

#[cfg(test)]
mod tests {
    use super::*;

    fn corpus(rel: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../corpus/pic")
            .join(rel)
    }

    #[test]
    fn figure_file_reads_its_includes() {
        let out = extract(PicSource::Path(&corpus("opamp.pic")), None).unwrap();
        assert!(out.diagnostics.is_empty());
        assert_eq!(
            out.includes.keys().collect::<Vec<_>>(),
            vec!["opamp_ideal.sub"]
        );
        assert!(out.includes["opamp_ideal.sub"].contains(".subckt"));
    }

    #[test]
    fn compile_error_is_a_located_pic_failure() {
        let empty = BTreeMap::new();
        let src = ".PS\nspice = 1\nR1: resistor((0,0), (1,0), 1k\n.PE\n";
        let e = extract(
            PicSource::Text {
                text: src,
                includes: &empty,
            },
            None,
        )
        .err()
        .unwrap();
        assert_eq!(e.kind, "pic");
        assert_eq!(e.diagnostics[0].kind, "compile");
        assert!(e.diagnostics[0].line.is_some());
    }

    #[test]
    fn unmarked_figure_is_not_marked() {
        let empty = BTreeMap::new();
        let e = extract(
            PicSource::Text {
                text: ".PS\nbox\n.PE\n",
                includes: &empty,
            },
            None,
        )
        .err()
        .unwrap();
        assert_eq!((e.kind, e.diagnostics[0].kind), ("pic", "not-marked"));
        // ...but it renders.
        let svg = render(PicSource::Text {
            text: ".PS\nbox\n.PE\n",
            includes: &empty,
        })
        .unwrap();
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn bad_snap_is_an_options_failure() {
        let empty = BTreeMap::new();
        let src = PicSource::Text {
            text: "",
            includes: &empty,
        };
        assert_eq!(extract(src, Some(-1.0)).err().unwrap().kind, "options");
    }
}
