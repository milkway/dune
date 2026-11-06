//! The R-independent half of the bindings: deck → plots, options,
//! rawfile I/O. Plain Rust types only, so `cargo test` exercises it
//! without an R session; `lib.rs` converts to and from SEXPs.

use std::collections::BTreeMap;
use std::path::Path;

use dune_core::consts::CTOK;
use dune_core::{Circuit, Method, Options, Solution};
use dune_netlist::{IncludeLoader, ParseOutcome, Severity};
use dune_raw::{Data, Plot, Rawfile};

/// Name a text deck is parsed under (as in `dune-wasm`): `.include`
/// paths resolve relative to it, i.e. exactly as written.
pub const DECK_NAME: &str = "deck.cir";

/// Where the deck comes from.
pub enum Source<'a> {
    /// A file on disk; `.include` resolves on the file system,
    /// relative to it.
    Path(&'a Path),
    /// Netlist text; `.include` is served from the map only.
    Text {
        text: &'a str,
        includes: &'a BTreeMap<String, String>,
    },
}

/// One user override of a simulator option, already typed by the R
/// side: a number, a logical or a string (exactly one is set).
pub enum OptValue {
    Num(f64),
    Bool(bool),
    Str(String),
}

/// A diagnostic, rendered with its `file:line:col` span.
pub struct Diag {
    pub severity: &'static str,
    pub message: String,
}

/// A failure, with the stage it happened at (the same `kind`s as the
/// `dune-wasm` `SimError`: `options|parse|elaborate|analysis`, plus
/// `io` for an unreadable root deck).
pub struct Failure {
    pub kind: &'static str,
    pub message: String,
    pub diagnostics: Vec<Diag>,
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }
}

/// Everything a run produced.
pub struct RunOutput {
    pub title: String,
    /// `(plot, scale index)` in rawfile order; the scale index is the
    /// `Solution`'s (`None` for the operating point, `.tf`, `.pz`…),
    /// which a rawfile cannot carry.
    pub plots: Vec<(Plot, Option<usize>)>,
    pub notes: Vec<String>,
    pub diagnostics: Vec<Diag>,
}

/// The option names `apply_options` accepts, in `Options` field order
/// (the card's names, as in `spice.txt` §SIMULATOR VARIABLES).
pub const OPTION_NAMES: [&str; 23] = [
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
    "maxord",
    "nopage",
    "defl",
    "defw",
    "defad",
    "defas",
];

/// `.include` from an in-memory map (the `dune-wasm` `MapLoader`).
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

/// Override `opts` field by field. Unknown names and ill-typed values
/// are errors here (unlike a deck's `.options`, which only warns, as
/// the 3f5 does): a typo in R code should not pass silently.
pub fn apply_options(opts: &mut Options, user: &[(String, OptValue)]) -> Result<(), String> {
    fn num(name: &str, v: &OptValue) -> Result<f64, String> {
        match v {
            OptValue::Num(x) if x.is_finite() => Ok(*x),
            _ => Err(format!("option `{name}` needs a finite number")),
        }
    }
    fn count(name: &str, v: &OptValue) -> Result<usize, String> {
        match v {
            OptValue::Num(x) if x.is_finite() && *x >= 0.0 && *x < 1e9 && x.fract() == 0.0 => {
                Ok(*x as usize)
            }
            _ => Err(format!("option `{name}` needs a non-negative whole number")),
        }
    }
    fn flag(name: &str, v: &OptValue) -> Result<bool, String> {
        match v {
            OptValue::Bool(b) => Ok(*b),
            _ => Err(format!("option `{name}` needs TRUE or FALSE")),
        }
    }
    for (name, v) in user {
        let n = name.as_str();
        match n {
            "reltol" => opts.reltol = num(n, v)?,
            "abstol" => opts.abstol = num(n, v)?,
            "vntol" => opts.vntol = num(n, v)?,
            "gmin" => opts.gmin = num(n, v)?,
            "itl1" => opts.itl1 = count(n, v)?,
            "itl2" => opts.itl2 = count(n, v)?,
            "itl4" => opts.itl4 = count(n, v)?,
            "gminsteps" => opts.gminsteps = count(n, v)?,
            "srcsteps" => opts.srcsteps = count(n, v)?,
            // °C on the card and here; kelvin inside, like the C.
            "temp" => opts.temp = num(n, v)? + CTOK,
            "tnom" => opts.tnom = num(n, v)? + CTOK,
            "pivtol" => opts.pivtol = num(n, v)?,
            "pivrel" => opts.pivrel = num(n, v)?,
            "noopiter" => opts.noopiter = flag(n, v)?,
            "trtol" => opts.trtol = num(n, v)?,
            "chgtol" => opts.chgtol = num(n, v)?,
            "method" => {
                opts.method = match v {
                    OptValue::Str(s) if s.eq_ignore_ascii_case("gear") => Method::Gear,
                    OptValue::Str(s)
                        if s.eq_ignore_ascii_case("trap")
                            || s.eq_ignore_ascii_case("trapezoidal") =>
                    {
                        Method::Trapezoidal
                    }
                    _ => return Err("option `method` must be \"trap\" or \"gear\"".into()),
                }
            }
            "maxord" => opts.maxord = count(n, v)?,
            "nopage" => opts.nopage = flag(n, v)?,
            "defl" => opts.defl = num(n, v)?,
            "defw" => opts.defw = num(n, v)?,
            "defad" => opts.defad = num(n, v)?,
            "defas" => opts.defas = num(n, v)?,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(())
}

fn render_diags(parsed: &ParseOutcome, diags: &[dune_netlist::Diagnostic]) -> Vec<Diag> {
    diags
        .iter()
        .map(|d| Diag {
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            message: d.render(&parsed.sources),
        })
        .collect()
}

/// Parse, elaborate and run every analysis card in deck order — what
/// `dune -b` runs — through [`Circuit::run`]. The deck's `.options`
/// apply first, then `user` on top.
pub fn run(source: Source<'_>, user: &[(String, OptValue)]) -> Result<RunOutput, Failure> {
    // Validate the overrides before any work.
    let mut options = Options::default();
    apply_options(&mut options, user).map_err(|m| Failure::new("options", m))?;

    let parsed = match source {
        Source::Path(p) => {
            dune_netlist::parse_file(p).map_err(|e| Failure::new("io", e.to_string()))?
        }
        Source::Text { text, includes } => dune_netlist::parse_with_loader(
            Path::new(DECK_NAME),
            text.to_string(),
            &mut MapLoader(includes),
        ),
    };
    let mut diagnostics = render_diags(&parsed, &parsed.diags);
    if let Some(first) = parsed.diags.iter().find(|d| d.severity == Severity::Error) {
        return Err(Failure {
            kind: "parse",
            message: first.render(&parsed.sources),
            diagnostics,
        });
    }

    let (circuit, mut notes) = Circuit::from_netlist(&parsed.netlist).map_err(|e| Failure {
        kind: "elaborate",
        message: e.to_string(),
        diagnostics: std::mem::take(&mut diagnostics),
    })?;

    let mut options = Options::default();
    let mut opt_diags = Vec::new();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(render_diags(&parsed, &opt_diags));
    // Cannot fail: validated above on a fresh `Options`.
    apply_options(&mut options, user).map_err(|m| Failure::new("options", m))?;

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &parsed.netlist.analyses {
        let (sols, analysis_notes) = circuit.run(analysis, &options).map_err(|e| Failure {
            kind: "analysis",
            message: e.to_string(),
            diagnostics: std::mem::take(&mut diagnostics),
        })?;
        notes.extend(analysis_notes);
        solutions.extend(sols);
    }
    if solutions.is_empty() {
        notes.push("no analyses in the deck; nothing to do".into());
    }

    let title = parsed.netlist.title.clone();
    Ok(RunOutput {
        plots: solutions
            .iter()
            .map(|s| (s.to_plot(&title, ""), s.scale))
            .collect(),
        title,
        notes,
        diagnostics,
    })
}

/// Read every plot of a rawfile (ASCII or binary).
pub fn read_raw(path: &Path) -> Result<Rawfile, String> {
    Rawfile::read_from(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write plots back to back, as `raw_write` does.
pub fn write_raw(path: &Path, plots: Vec<Plot>, binary: bool) -> Result<(), String> {
    for p in &plots {
        let n = p.data.num_points();
        let ok = match &p.data {
            Data::Real(c) => c.len() == p.variables.len() && c.iter().all(|v| v.len() == n),
            Data::Complex(c) => c.len() == p.variables.len() && c.iter().all(|v| v.len() == n),
        };
        if !ok {
            return Err(format!(
                "plot `{}`: every variable needs one column of the same length",
                p.name
            ));
        }
    }
    let raw = Rawfile {
        plots,
        warnings: Vec::new(),
    };
    let format = if binary {
        dune_raw::Format::Binary
    } else {
        dune_raw::Format::Ascii
    };
    std::fs::write(path, raw.to_bytes(format)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIVIDER: &str = "divider\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n";

    fn text(t: &str) -> Result<RunOutput, Failure> {
        let inc = BTreeMap::new();
        run(
            Source::Text {
                text: t,
                includes: &inc,
            },
            &[],
        )
    }

    #[test]
    fn op_of_a_divider() {
        let out = text(DIVIDER).ok().unwrap();
        assert_eq!(out.plots.len(), 1);
        let (plot, scale) = &out.plots[0];
        assert_eq!(*scale, None);
        assert!((plot.real_values("v(2)").unwrap()[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_error_carries_span() {
        let Err(f) = text("t\nv1 1 0 dc 1\nq1 1 1 0\n.op\n.end\n") else {
            panic!("expected a parse error");
        };
        assert_eq!(f.kind, "parse");
        assert!(f.message.starts_with("deck.cir:3:"), "{}", f.message);
    }

    #[test]
    fn unknown_option_is_an_error() {
        let inc = BTreeMap::new();
        let Err(f) = run(
            Source::Text {
                text: DIVIDER,
                includes: &inc,
            },
            &[("reltl".into(), OptValue::Num(1e-4))],
        ) else {
            panic!("expected an options error");
        };
        assert_eq!(f.kind, "options");
    }

    #[test]
    fn includes_come_from_the_map() {
        let mut inc = BTreeMap::new();
        inc.insert("r2.inc".to_string(), "r2 2 0 1k\n".to_string());
        let out = run(
            Source::Text {
                text: "t\nv1 1 0 dc 2\nr1 1 2 1k\n.include r2.inc\n.op\n.end\n",
                includes: &inc,
            },
            &[],
        )
        .ok()
        .unwrap();
        assert!((out.plots[0].0.real_values("v(2)").unwrap()[0] - 1.0).abs() < 1e-9);
    }
}
