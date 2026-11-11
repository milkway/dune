//! The R-independent half of the bindings: deck → plots, options,
//! rawfile I/O. Plain Rust types only, so `cargo test` exercises it
//! without an R session; `lib.rs` converts to and from SEXPs.

use std::collections::BTreeMap;
use std::path::Path;

use dune_core::consts::CTOK;
use dune_core::{
    AlterValue, Circuit, MeasureError, Measurement, Method, Options, PSS_FREQ_PLOT, PSS_TIME_PLOT,
    PssParams, Session, Solution, SweepParam, measure_all,
};
use dune_netlist::{IncludeLoader, ParseOutcome, Severity};
use dune_raw::{Data, Plot, Rawfile};

/// Name a text deck is parsed under (as in `dune-wasm`): `.include`
/// paths resolve relative to it, i.e. exactly as written.
pub const DECK_NAME: &str = "deck.cir";

/// Stack of the worker thread of [`on_worker`]: the R main thread's
/// is large, and the parser and the elaborator recurse.
const WORKER_STACK: usize = 64 << 20;

/// Run `f` — an engine computation — where its arithmetic gives the
/// same bits as on a sweep's worker threads.
///
/// On `x86_64-pc-windows-gnu` (Rtools) the C runtime's `exp` and
/// `pow` are x87 code whose rounding follows the thread's precision
/// control: the main thread runs with the 64-bit mantissa the CRT
/// start-up (and R) sets, a new thread with Windows' 53-bit default.
/// Measured on the GitHub Windows runner: 17% of `exp` and 46% of
/// `powf` results differ in the last bit between the two. A deck run
/// from the R thread would then disagree with the same deck run by
/// `spice_sweep(.threads = n)`, breaking the sweep's bit-for-bit
/// contract. So there every computation runs on a fresh thread — the
/// one mode the pool's workers have too. Elsewhere the math library
/// does not look at the x87 control word and `f` runs in place.
pub fn compute<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    #[cfg(all(windows, target_env = "gnu"))]
    {
        on_worker(f)
    }
    #[cfg(not(all(windows, target_env = "gnu")))]
    {
        f()
    }
}

/// `f` on a scoped thread of its own; a panic in it resumes here.
#[cfg_attr(not(all(windows, target_env = "gnu")), allow(dead_code))]
fn on_worker<T: Send>(f: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("spice-compute".into())
            .stack_size(WORKER_STACK)
            .spawn_scoped(scope, f);
        match worker {
            Ok(handle) => handle
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            Err(e) => panic!("cannot start the computation thread: {e}"),
        }
    })
}

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
/// `io` for an unreadable root deck and `pss` for a periodic steady
/// state that did not converge).
pub struct Failure {
    pub kind: &'static str,
    pub message: String,
    pub diagnostics: Vec<Diag>,
    /// For kind `pss`: the shooting history.
    pub pss: Option<PssHistory>,
}

/// The history of a shooting that did not converge
/// ([`dune_core::Error::PssConvergence`]): the iterations run, the
/// residual of each and, for an autonomous oscillator, the last
/// period iterate (`None` for a driven circuit).
#[derive(Clone, Debug, PartialEq)]
pub struct PssHistory {
    pub iterations: usize,
    pub residuals: Vec<f64>,
    pub period: Option<f64>,
}

impl PssHistory {
    /// The history of `e`, when it is a `.pss` that did not converge.
    pub fn of(e: &dune_core::Error) -> Option<Self> {
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
}

impl Failure {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
            diagnostics: Vec::new(),
            pss: None,
        }
    }

    /// An analysis that failed, with the diagnostics seen so far: kind
    /// `pss` with the shooting history for a `.pss` that did not
    /// converge, kind `analysis` otherwise.
    fn analysis(e: dune_core::Error, diagnostics: Vec<Diag>) -> Self {
        let pss = PssHistory::of(&e);
        Failure {
            kind: if pss.is_some() { "pss" } else { "analysis" },
            message: e.to_string(),
            diagnostics,
            pss,
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

/// The `.measure` analysis keyword of a plot and the plot name the
/// engine measures it under. The `.pss` plots measure as the analyses
/// they are shaped like: the period in time as a `.tran`, the complex
/// harmonics (on a frequency scale) as an `.ac`.
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

/// `spice_measure()`: evaluate `(name, spec)` measurement bodies on
/// one plot (`scale` its 0-based scale index). Err is a message for
/// a malformed spec or an unmeasurable plot.
pub fn measure_plot(
    plot: Plot,
    scale: Option<usize>,
    specs: &[(String, String)],
) -> Result<Vec<MeasRow>, String> {
    let Some((analysis, measured_as)) = plot_analysis(&plot.name) else {
        return Err(format!(
            "plot \"{}\" cannot be measured: measure limited to tran, dc, sp, ac or pss analysis",
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
        plotname: measured_as.to_string(),
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
pub const OPTION_NAMES: [&str; 31] = [
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
    "seed",
    "notrnoise",
    "devnoise",
    "devnt",
    "keepopinfo",
    "trytocompact",
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
    fn seed(name: &str, v: &OptValue) -> Result<u64, String> {
        match v {
            OptValue::Num(x) if *x >= 1.0 && *x < 9.007_199_254_740_992e15 && x.fract() == 0.0 => {
                Ok(*x as u64)
            }
            _ => Err(format!(
                "option `{name}` needs a whole number >= 1 (no \"random\": runs are reproducible)"
            )),
        }
    }
    fn step(name: &str, v: &OptValue) -> Result<f64, String> {
        match v {
            OptValue::Num(x) if x.is_finite() && *x > 0.0 => Ok(*x),
            _ => Err(format!("option `{name}` needs a positive time step")),
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
            "seed" => opts.seed = seed(n, v)?,
            "notrnoise" => opts.notrnoise = flag(n, v)?,
            "devnoise" => opts.dev_noise = flag(n, v)?,
            "devnt" => opts.dev_noise_step = Some(step(n, v)?),
            "keepopinfo" => opts.keepopinfo = flag(n, v)?,
            "trytocompact" => opts.trytocompact = flag(n, v)?,
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    Ok(())
}

/// Check a `tmax_divisor` argument: `None` or a finite number ≥ 1.
/// It is an argument of its own, not an `options` name: it scales the
/// TMAX of every `.tran` card, a parameter of the analysis card, not
/// an engine option ([`Options::tmax_divisor`]).
pub fn check_divisor(d: Option<f64>) -> Result<(), String> {
    match d {
        Some(d) if !(d >= 1.0 && d.is_finite()) => Err(format!(
            "`tmax_divisor` must be a finite number >= 1, got {d}"
        )),
        _ => Ok(()),
    }
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
/// then `user` and `tmax_divisor` on top), ready to elaborate.
struct Prepared {
    netlist: dune_netlist::ir::Netlist,
    options: Options,
    diagnostics: Vec<Diag>,
}

fn prepare(
    source: Source<'_>,
    user: &[(String, OptValue)],
    tmax_divisor: Option<f64>,
) -> Result<Prepared, Failure> {
    // Validate the overrides before any work.
    let mut options = Options::default();
    apply_options(&mut options, user).map_err(|m| Failure::new("options", m))?;
    check_divisor(tmax_divisor).map_err(|m| Failure::new("options", m))?;

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
            pss: None,
        });
    }

    let mut options = Options::default();
    let mut opt_diags = Vec::new();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    diagnostics.extend(render_diags(&parsed, &opt_diags));
    // Cannot fail: validated above on a fresh `Options`.
    apply_options(&mut options, user).map_err(|m| Failure::new("options", m))?;
    options.tmax_divisor = tmax_divisor;
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
/// apply first, then `user` and `tmax_divisor` on top.
pub fn run(
    source: Source<'_>,
    user: &[(String, OptValue)],
    tmax_divisor: Option<f64>,
) -> Result<RunOutput, Failure> {
    let Prepared {
        netlist,
        options,
        mut diagnostics,
    } = prepare(source, user, tmax_divisor)?;

    let (circuit, mut notes) = Circuit::from_netlist(&netlist).map_err(|e| Failure {
        diagnostics: std::mem::take(&mut diagnostics),
        ..Failure::new("elaborate", e.to_string())
    })?;

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &netlist.analyses {
        let (sols, analysis_notes) = circuit
            .run(analysis, &options)
            .map_err(|e| Failure::analysis(e, std::mem::take(&mut diagnostics)))?;
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

/// What [`pss`] produced: [`run`]'s output with the two `.pss` plots
/// (time, then spectrum), plus the period and the shooting history.
pub struct PssOutput {
    pub run: RunOutput,
    pub period: f64,
    pub t0: f64,
    pub residuals: Vec<f64>,
    /// The period of each shooting iteration, aligned with `residuals`.
    pub periods: Vec<f64>,
}

/// `spice_pss()`: parse and elaborate the deck (options as in [`run`]),
/// then run [`Circuit::pss`] with `p`. The deck's own analysis cards
/// are not run; a shooting that does not converge is kind `pss`.
pub fn pss(
    source: Source<'_>,
    user: &[(String, OptValue)],
    tmax_divisor: Option<f64>,
    p: &PssParams,
) -> Result<PssOutput, Failure> {
    let Prepared {
        netlist,
        options,
        mut diagnostics,
    } = prepare(source, user, tmax_divisor)?;
    let (circuit, mut notes) = Circuit::from_netlist(&netlist).map_err(|e| Failure {
        diagnostics: std::mem::take(&mut diagnostics),
        ..Failure::new("elaborate", e.to_string())
    })?;
    let pss = circuit
        .pss(&options, p)
        .map_err(|e| Failure::analysis(e, std::mem::take(&mut diagnostics)))?;
    notes.extend(pss.notes);
    let title = netlist.title.clone();
    Ok(PssOutput {
        run: RunOutput {
            measures: Vec::new(),
            plots: to_plots(&title, &[pss.time, pss.spectrum]),
            title,
            notes,
            diagnostics,
        },
        period: pss.period,
        t0: pss.t0,
        residuals: pss.residuals,
        periods: pss.periods,
    })
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
    pub fn open(
        source: Source<'_>,
        user: &[(String, OptValue)],
        tmax_divisor: Option<f64>,
    ) -> Result<Self, Failure> {
        let Prepared {
            netlist,
            options,
            mut diagnostics,
        } = prepare(source, user, tmax_divisor)?;
        let (circuit, notes) = Circuit::from_netlist(&netlist).map_err(|e| Failure {
            diagnostics: std::mem::take(&mut diagnostics),
            ..Failure::new("elaborate", e.to_string())
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

    /// Replace the `tmax_divisor` of the next runs (kind `options`
    /// when out of range; nothing changes then).
    pub fn set_tmax_divisor(&mut self, d: Option<f64>) -> Result<(), Failure> {
        check_divisor(d).map_err(|m| Failure::new("options", m))?;
        self.session.options_mut().tmax_divisor = d;
        Ok(())
    }

    pub fn tmax_divisor(&self) -> Option<f64> {
        self.session.options().tmax_divisor
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
        let (solutions, analysis_notes) = self
            .session
            .run()
            .map_err(|e| Failure::analysis(e, self.diagnostics.clone()))?;
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
        /// For kind `pss`: the shooting history.
        pss: Option<PssHistory>,
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
    tmax_divisor: Option<f64>,
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
    } = prepare(source, user, tmax_divisor)?;
    let runs = if threads == 1 {
        dune_core::sweep(&netlist, &options, &params, rows)
    } else {
        dune_core::sweep_parallel(&netlist, &options, &params, rows, threads)
    }
    .map_err(|e| Failure::new("sweep", e.to_string()))?;
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
                pss: PssHistory::of(&e),
                kind: match e {
                    dune_core::Error::Build(_)
                    | dune_core::Error::Elab(_)
                    | dune_core::Error::Unsupported { .. } => "elaborate",
                    dune_core::Error::PssConvergence { .. } => "pss",
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

    /// The worker-thread path (what Windows/Rtools always takes)
    /// returns what the in-place call does, and hands a panic back.
    #[test]
    fn a_computation_on_the_worker_thread_gives_the_same_plots() {
        let deck = "rc\nv1 1 0 pulse(0 1 0 1n 1n 1m 2m)\nr1 1 2 1k\nc1 2 0 1u\n\
                    .tran 10u 2m\n.end\n";
        let inc = BTreeMap::new();
        let go = || {
            let source = Source::Text {
                text: deck,
                includes: &inc,
            };
            let out = run(source, &[], None).ok().expect("runs");
            out.plots
                .iter()
                .flat_map(|(p, _)| match &p.data {
                    Data::Real(cols) => cols.iter().flatten().map(|v| v.to_bits()).collect(),
                    Data::Complex(_) => Vec::new(),
                })
                .collect::<Vec<u64>>()
        };
        let here = go();
        assert!(here.len() > 100);
        assert_eq!(here, on_worker(go));
        assert_eq!(here, compute(go));

        let caught = std::panic::catch_unwind(|| on_worker(|| panic!("boom")));
        assert!(caught.is_err());
    }

    const DIVIDER: &str = "divider\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n";

    fn text(t: &str) -> Result<RunOutput, Failure> {
        let inc = BTreeMap::new();
        run(
            Source::Text {
                text: t,
                includes: &inc,
            },
            &[],
            None,
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
        let mut s = LiveSession::open(src(), &[], None).ok().unwrap();
        let v2 = |o: &RunOutput| o.plots[0].0.real_values("v(2)").unwrap()[0];
        s.alter("r2", "", AlterValue::Num(3e3)).ok().unwrap();
        let altered = deck.replace("r2 2 0 1k", "r2 2 0 3k");
        let fresh = run(
            Source::Text {
                text: &altered,
                includes: &inc,
            },
            &[],
            None,
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
            None,
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
        let out = sweep(src(), &[], None, &["R2".into()], &rows, 1)
            .ok()
            .unwrap();
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
        let Err(f) = sweep(src(), &[], None, &["r9".into()], &[vec![1.0]], 1) else {
            panic!("expected a sweep error");
        };
        assert_eq!(f.kind, "sweep");
    }

    const RC: &str = "rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.end\n";

    fn rc_pss(max_iter: usize) -> Result<PssOutput, Failure> {
        let inc = BTreeMap::new();
        let p = PssParams {
            freq: Some(1e3),
            tstab: 5e-3,
            osc_node: Some("2".into()),
            max_iter,
            ..PssParams::default()
        };
        pss(
            Source::Text {
                text: RC,
                includes: &inc,
            },
            &[],
            None,
            &p,
        )
    }

    #[test]
    fn pss_is_the_card() {
        let out = rc_pss(50).ok().unwrap();
        let card = text(&RC.replace(".end", ".pss 1k 5m 2 128 10\n.end"))
            .ok()
            .unwrap();
        assert_eq!(out.run.notes, card.notes);
        assert_eq!(out.run.plots.len(), 2);
        for ((a, sa), (b, sb)) in out.run.plots.iter().zip(&card.plots) {
            assert_eq!((a.name.as_str(), sa), (b.name.as_str(), sb));
            assert_eq!(a, b);
        }
        assert_eq!(out.period, 1e-3);
        assert!(out.residuals.len() == 2 && out.residuals[1] <= 1.0);
        assert_eq!(out.periods, [1e-3; 2]);
    }

    #[test]
    fn pss_failure_carries_the_history() {
        let Err(f) = rc_pss(1) else {
            panic!("one iteration cannot converge");
        };
        assert_eq!(f.kind, "pss");
        let h = f.pss.unwrap();
        let r = h.residuals.clone();
        assert!(h.iterations == 1 && r.len() == 1 && r[0] > 1.0);
        assert_eq!(h.period, None);
        // The same through the card.
        let Err(card) = text(&RC.replace(".end", ".pss 1k 5m 2 128 10 1\n.end")) else {
            panic!("one iteration cannot converge");
        };
        assert_eq!(card.kind, "pss");
        assert_eq!(card.pss.unwrap(), h);
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
            None,
        )
        .ok()
        .unwrap();
        assert!((out.plots[0].0.real_values("v(2)").unwrap()[0] - 1.0).abs() < 1e-9);
    }
}
