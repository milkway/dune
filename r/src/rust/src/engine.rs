//! The R-independent half of the bindings: deck → plots, options,
//! rawfile I/O. Plain Rust types only, so `cargo test` exercises it
//! without an R session; `lib.rs` converts to and from SEXPs.

use std::collections::BTreeMap;
use std::path::Path;

use dune_core::consts::CTOK;
use dune_core::{
    AlterValue, Circuit, MeasureError, Measurement, Method, Options, Session, Solution, SweepParam,
    measure_all,
};
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
#[derive(Clone)]
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
    /// The deck's `.measure` cards, each on every plot of its
    /// analysis (plot order, then card order).
    pub measures: Vec<MeasRow>,
    pub notes: Vec<String>,
    pub diagnostics: Vec<Diag>,
}

/// One measurement as plain data (a row of the R tibble).
pub struct MeasRow {
    pub name: String,
    pub analysis: &'static str,
    /// Index of the measured plot in the run; `None` for
    /// `spice_measure()`.
    pub plot: Option<usize>,
    /// NaN when the measurement failed.
    pub value: f64,
    /// ngspice's extra numbers by label (`trig targ at with from to`).
    pub details: Vec<(&'static str, f64)>,
    pub error: Option<String>,
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

    /// The value of one detail label, NaN when absent.
    pub fn detail(&self, label: &str) -> f64 {
        self.details
            .iter()
            .find(|(k, _)| *k == label)
            .map_or(f64::NAN, |(_, v)| *v)
    }
}

/// The `.measure` analysis keyword of a plot.
fn plot_analysis(plotname: &str) -> Option<&'static str> {
    Some(match plotname {
        "Transient Analysis" => "tran",
        "AC Analysis" => "ac",
        "SP Analysis" => "sp",
        "DC transfer characteristic" => "dc",
        _ => return None,
    })
}

/// `spice_measure()`: evaluate `(name, spec)` measurement bodies on
/// one plot (`scale` its 0-based scale index). Err is a message for
/// a malformed spec or an unmeasurable plot.
pub fn measure_plot(
    plot: Plot,
    scale: Option<usize>,
    specs: &[(String, String)],
) -> Result<Vec<MeasRow>, String> {
    let Some(analysis) = plot_analysis(&plot.name) else {
        return Err(format!(
            "plot \"{}\" cannot be measured: measure limited to tran, dc, sp, or ac analysis",
            plot.name
        ));
    };
    let mut funcs = Vec::with_capacity(specs.len());
    for (name, spec) in specs {
        let (f, _warnings) = dune_netlist::parse_measure_body(spec).map_err(|e| {
            format!(
                "`{name}`: {} (at column {} of \"{spec}\")",
                e.message,
                e.start + 1
            )
        })?;
        funcs.push(f);
    }
    let sol = Solution {
        plotname: plot.name,
        variables: plot
            .variables
            .into_iter()
            .map(|v| (v.name, v.vtype))
            .collect(),
        data: plot.data,
        scale,
    };
    Ok(specs
        .iter()
        .zip(&funcs)
        .map(|((name, _), f)| MeasRow::new(name, analysis, None, &sol.measure(f)))
        .collect())
}

/// The option names `apply_options` accepts, in `Options` field order
/// (the card's names, as in `spice.txt` §SIMULATOR VARIABLES).
pub const OPTION_NAMES: [&str; 25] = [
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
    "ltrafit",
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
                    OptValue::Str(s) if s.eq_ignore_ascii_case("trbdf2") => Method::TrBdf2,
                    OptValue::Str(s)
                        if s.eq_ignore_ascii_case("trap")
                            || s.eq_ignore_ascii_case("trapezoidal") =>
                    {
                        Method::Trapezoidal
                    }
                    _ => {
                        return Err(
                            "option `method` must be \"trap\", \"gear\" or \"trbdf2\"".into()
                        );
                    }
                }
            }
            "ltrafit" => opts.ltra_fit = flag(n, v)?,
            "ltrabkrate" => opts.ltra_break_rate = Some(count(n, v)?),
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

/// A parsed deck with its options settled (the deck's `.options`,
/// then `user` on top), ready to elaborate.
struct Prepared {
    netlist: dune_netlist::ir::Netlist,
    options: Options,
    diagnostics: Vec<Diag>,
}

fn prepare(source: Source<'_>, user: &[(String, OptValue)]) -> Result<Prepared, Failure> {
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

    let mut options = Options::default();
    let mut opt_diags = Vec::new();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(render_diags(&parsed, &opt_diags));
    // Cannot fail: validated above on a fresh `Options`.
    apply_options(&mut options, user).map_err(|m| Failure::new("options", m))?;
    Ok(Prepared {
        netlist: parsed.netlist,
        options,
        diagnostics,
    })
}

fn to_plots(title: &str, solutions: &[Solution]) -> Vec<(Plot, Option<usize>)> {
    solutions
        .iter()
        .map(|s| (s.to_plot(title, ""), s.scale))
        .collect()
}

/// Parse, elaborate and run every analysis card in deck order — what
/// `dune -b` runs — through [`Circuit::run`]. The deck's `.options`
/// apply first, then `user` on top.
pub fn run(source: Source<'_>, user: &[(String, OptValue)]) -> Result<RunOutput, Failure> {
    let Prepared {
        netlist,
        options,
        mut diagnostics,
    } = prepare(source, user)?;

    let (circuit, mut notes) = Circuit::from_netlist(&netlist).map_err(|e| Failure {
        kind: "elaborate",
        message: e.to_string(),
        diagnostics: std::mem::take(&mut diagnostics),
    })?;

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &netlist.analyses {
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

    let measures = deck_measures(&netlist.measures, &solutions);

    let title = netlist.title.clone();
    Ok(RunOutput {
        measures,
        plots: to_plots(&title, &solutions),
        title,
        notes,
        diagnostics,
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

/// A deck kept alive for `spice_alter()` / `spice_rerun()` (issue
/// #12): [`dune_core::Session`] plus what every run's result carries
/// (title, elaboration notes, parser warnings, `.measure` cards).
pub struct LiveSession {
    session: Session,
    title: String,
    notes: Vec<String>,
    diagnostics: Vec<Diag>,
    measures: Vec<dune_netlist::ir::Measure>,
}

/// A refused alteration (kind `alter`; the circuit is unchanged).
fn alter_failure(e: dune_core::Error) -> Failure {
    Failure::new("alter", e.to_string())
}

impl LiveSession {
    pub fn open(source: Source<'_>, user: &[(String, OptValue)]) -> Result<Self, Failure> {
        let Prepared {
            netlist,
            options,
            mut diagnostics,
        } = prepare(source, user)?;
        let (circuit, notes) = Circuit::from_netlist(&netlist).map_err(|e| Failure {
            kind: "elaborate",
            message: e.to_string(),
            diagnostics: std::mem::take(&mut diagnostics),
        })?;
        Ok(LiveSession {
            session: Session::new(circuit, options, netlist.analyses.clone()),
            title: netlist.title.clone(),
            notes,
            diagnostics,
            measures: netlist.measures.clone(),
        })
    }

    pub fn alter(&mut self, refdes: &str, param: &str, value: AlterValue) -> Result<(), Failure> {
        self.session
            .alter(refdes, param, value)
            .map_err(alter_failure)
    }

    pub fn param(&self, refdes: &str, param: &str) -> Result<Option<AlterValue>, Failure> {
        self.session.param(refdes, param).map_err(alter_failure)
    }

    pub fn alterable(&self, refdes: &str) -> Result<&'static [&'static str], Failure> {
        self.session
            .circuit()
            .alterable(refdes)
            .map_err(alter_failure)
    }

    pub fn set_temp(&mut self, celsius: f64) -> Result<(), Failure> {
        self.session.set_temp(celsius).map_err(alter_failure)
    }

    pub fn temp(&self) -> f64 {
        self.session.temp()
    }

    pub fn set_warm_start(&mut self, on: bool) {
        self.session.set_warm_start(on);
    }

    pub fn warm_start(&self) -> bool {
        self.session.warm_start()
    }

    /// `(hits, misses)`.
    pub fn warm_stats(&self) -> (u64, u64) {
        let s = self.session.warm_stats();
        (s.hits, s.misses)
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Re-run every analysis card: [`run`]'s output for the altered
    /// deck, the `.measure` cards included.
    pub fn run(&mut self) -> Result<RunOutput, Failure> {
        let (solutions, analysis_notes) = self.session.run().map_err(|e| Failure {
            kind: "analysis",
            message: e.to_string(),
            diagnostics: self.diagnostics.clone(),
        })?;
        let mut notes = self.notes.clone();
        notes.extend(analysis_notes);
        if solutions.is_empty() {
            notes.push("no analyses in the deck; nothing to do".into());
        }
        Ok(RunOutput {
            measures: deck_measures(&self.measures, &solutions),
            plots: to_plots(&self.title, &solutions),
            title: self.title.clone(),
            notes,
            diagnostics: self.diagnostics.clone(),
        })
    }
}

/// One run of a sweep: its plots and notes, or why it failed.
pub enum SweepRunOutput {
    Ok {
        plots: Vec<(Plot, Option<usize>)>,
        /// The deck's `.measure` cards on this run's plots.
        measures: Vec<MeasRow>,
        notes: Vec<String>,
    },
    Failed {
        kind: &'static str,
        message: String,
    },
}

/// A whole sweep (issue #10).
pub struct SweepOutput {
    pub title: String,
    /// The swept parameters in canonical form (`r1`, `@m1[w]`, `temp`).
    pub params: Vec<String>,
    pub runs: Vec<SweepRunOutput>,
    pub diagnostics: Vec<Diag>,
}

/// Parse once, then run the deck per row of `rows` through
/// `dune_core::sweep` (`threads == 1`) or `sweep_parallel` (any other
/// count; 0 = one per core). An unusable parameter or row is kind
/// `sweep`; a run's own failure stays in its [`SweepRunOutput`].
pub fn sweep(
    source: Source<'_>,
    user: &[(String, OptValue)],
    params: &[String],
    rows: &[Vec<f64>],
    threads: usize,
) -> Result<SweepOutput, Failure> {
    let params = params
        .iter()
        .map(|p| SweepParam::parse(p))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| Failure::new("sweep", e.to_string()))?;
    let Prepared {
        netlist,
        options,
        diagnostics,
    } = prepare(source, user)?;
    let runs = if threads == 1 {
        dune_core::sweep(&netlist, &options, &params, rows)
    } else {
        dune_core::sweep_parallel(&netlist, &options, &params, rows, threads)
    }
    .map_err(|e| Failure {
        kind: "sweep",
        message: e.to_string(),
        diagnostics: Vec::new(),
    })?;
    let title = netlist.title.clone();
    let runs = runs
        .into_iter()
        .map(|r| match r.outcome {
            Ok(o) => SweepRunOutput::Ok {
                plots: to_plots(&title, &o.solutions),
                measures: measure_all(&netlist.measures, &o.solutions)
                    .iter()
                    .map(|m| {
                        let c = &netlist.measures[m.card];
                        MeasRow::new(&c.name, c.analysis.keyword(), Some(m.plot), &m.result)
                    })
                    .collect(),
                notes: o.notes,
            },
            Err(e) => SweepRunOutput::Failed {
                kind: match e {
                    dune_core::Error::Build(_)
                    | dune_core::Error::Elab(_)
                    | dune_core::Error::Unsupported { .. } => "elaborate",
                    _ => "analysis",
                },
                message: e.to_string(),
            },
        })
        .collect();
    Ok(SweepOutput {
        title,
        params: params.iter().map(SweepParam::key).collect(),
        runs,
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
    fn a_live_session_reruns_as_the_altered_deck() {
        let inc = BTreeMap::new();
        let deck = "divider\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n";
        let src = || Source::Text {
            text: deck,
            includes: &inc,
        };
        let mut s = LiveSession::open(src(), &[]).ok().unwrap();
        let v2 = |o: &RunOutput| o.plots[0].0.real_values("v(2)").unwrap()[0];
        s.alter("r2", "", AlterValue::Num(3e3)).ok().unwrap();
        let altered = deck.replace("r2 2 0 1k", "r2 2 0 3k");
        let fresh = run(
            Source::Text {
                text: &altered,
                includes: &inc,
            },
            &[],
        )
        .ok()
        .unwrap();
        assert_eq!(v2(&s.run().ok().unwrap()).to_bits(), v2(&fresh).to_bits());
        let Err(f) = s.alter("r2", "", AlterValue::Num(0.0)) else {
            panic!("zero resistance");
        };
        assert_eq!(f.kind, "alter");
        assert_eq!(s.alterable("r1").ok().unwrap(), ["resistance"].as_slice());
        assert_eq!(s.title(), "divider");
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
    fn sweep_runs_every_row() {
        let inc = BTreeMap::new();
        let src = || Source::Text {
            text: DIVIDER,
            includes: &inc,
        };
        let rows = vec![vec![1e3], vec![3e3]];
        let out = sweep(src(), &[], &["R2".into()], &rows, 1).ok().unwrap();
        assert_eq!(out.params, ["r2"]);
        let v2: Vec<f64> = out
            .runs
            .iter()
            .map(|r| match r {
                SweepRunOutput::Ok { plots, .. } => plots[0].0.real_values("v(2)").unwrap()[0],
                SweepRunOutput::Failed { .. } => panic!("run failed"),
            })
            .collect();
        assert!((v2[0] - 1.0).abs() < 1e-9 && (v2[1] - 1.5).abs() < 1e-9);
        let Err(f) = sweep(src(), &[], &["r9".into()], &[vec![1.0]], 1) else {
            panic!("expected a sweep error");
        };
        assert_eq!(f.kind, "sweep");
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
