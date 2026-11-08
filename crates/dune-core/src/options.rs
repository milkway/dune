//! Simulator options with the 3f5 defaults
//! (`lib/ckt/cktinit.c`, `lib/ckt/cktntask.c`; names from
//! `spice.txt` §SIMULATOR VARIABLES). Temperatures arrive in °C on
//! the card and live in kelvin here, like the C.

use dune_netlist::Diagnostic;
use dune_netlist::ir::{Param, ParamValue};

use crate::consts::{CTOK, REFTEMP};
use crate::integ::Method;

#[derive(Clone, Debug)]
pub struct Options {
    /// `RELTOL`, default 1e-3.
    pub reltol: f64,
    /// `ABSTOL`, default 1e-12 (amperes).
    pub abstol: f64,
    /// `VNTOL`, default 1e-6 (volts).
    pub vntol: f64,
    /// `GMIN`, default 1e-12.
    pub gmin: f64,
    /// `ITL1`: DC iteration limit, default 100.
    pub itl1: usize,
    /// `ITL2`: DC transfer-curve iteration limit, default 50.
    pub itl2: usize,
    /// `ITL4`: transient timepoint iteration limit, default 10
    /// (`CKTtranMaxIter`).
    pub itl4: usize,
    /// Gmin steps in `CKTop`. 1 (the ngspice default) selects the
    /// dynamic Gillespie stepping; n > 1 runs the 3f5 fixed ladder
    /// with n steps (`corpus/KNOWN_DIFFS.md`).
    pub gminsteps: usize,
    /// Source steps in `CKTop`, default 10 (`TSKnumSrcSteps`).
    pub srcsteps: usize,
    /// Operating temperature in kelvin (`TEMP`, default 27 °C).
    pub temp: f64,
    /// Nominal parameter-measurement temperature (`TNOM`, 27 °C).
    pub tnom: f64,
    /// `PIVTOL`, default 1e-13.
    pub pivtol: f64,
    /// `PIVREL`, default 1e-3.
    pub pivrel: f64,
    /// `NOOPITER`: go directly to gmin stepping.
    pub noopiter: bool,
    /// `TRTOL`: truncation-error overestimation factor, default 7.
    pub trtol: f64,
    /// `CHGTOL`: charge tolerance, default 1e-14.
    pub chgtol: f64,
    /// `METHOD`: `trap` (default), `gear`, or Dune's opt-in `trbdf2`
    /// ([`Method::TrBdf2`]; not a SPICE value).
    pub method: Method,
    /// `MAXORD` for Gear, default 2.
    pub maxord: usize,
    /// `NOPAGE`: suppress page breaks in printed output.
    pub nopage: bool,
    /// `DEFL`/`DEFW`: default MOS channel length/width, meters
    /// (`CKTdefaultMosL/W`, 1e-4).
    pub defl: f64,
    pub defw: f64,
    /// `DEFAD`/`DEFAS`: default MOS drain/source diffusion area, m²
    /// (`CKTdefaultMosAD/AS`, 0).
    pub defad: f64,
    pub defas: f64,
    /// Dune's transient step budget (not a SPICE option; no `.options`
    /// card sets it): the most internal timepoints — accepted or
    /// rejected — one `.tran` may attempt before it stops with
    /// [`Error::StepLimit`](crate::Error::StepLimit). The 3f5 and
    /// ngspice-47 both parse `ITL5` ("total transient iteration
    /// limit") and ignore it (`cktsopt.c`: `case OPT_ITL5: break;`),
    /// so neither bounds a run whose breakpoints cascade (D-0022).
    ///
    /// `None` (default): automatic, scaled to the deck —
    /// `max(TRAN_STEP_FLOOR, TRAN_STEPS_PER_TMAX · ⌈TSTOP/TMAX⌉)`
    /// with the effective TMAX, i.e. 10 000 steps or 50 per maximum
    /// step span, whichever is more (the corpus peaks at 13 per span,
    /// ltra_2; a fixed budget cannot work because a cascading LTRA
    /// deck costs O(n²) in its history length — D-0022). `Some(n)`:
    /// at most `n` (`Some(usize::MAX)` is unlimited).
    pub max_tran_steps: Option<usize>,
    /// Dune's opt-in LTRA breakpoint hygiene (`.options ltrabkrate=N`;
    /// not a SPICE option, issue #6). The slope rule of `LTRAaccept`
    /// plants a breakpoint one delay after every accepted point where
    /// an incident wave's slope changes; `CKTsetBreak` merges only
    /// within `5e-5·TMAX`, so a ringing load can feed it an echo
    /// cascade (D-0022). `Some(n)`, `n > 0`: a slope breakpoint that
    /// falls within `TMAX/n` of one already scheduled is dropped,
    /// which caps them at about `n` per TMAX span. `None` or
    /// `Some(0)` (default): faithful, every request scheduled.
    pub ltra_break_rate: Option<usize>,
    /// Dune's opt-in recursive convolution for LTRA lines
    /// (`.options ltrafit`; not a SPICE option, issue #8). The 3f5
    /// recomputes every line's convolution over the whole accepted
    /// history at each timepoint — O(n) per point, quadratic per run.
    /// `true`: the RLC and RC impulse responses become exponential
    /// sums (an exact branch-cut quadrature, error budget 1e-6 per
    /// kernel, reported in the run's notes) advanced once per accepted
    /// point, with the newest segment kept exact — O(1) per point.
    /// `false` (default): faithful.
    pub ltra_fit: bool,
    /// Dune's transient refinement knob (an API argument, not a SPICE
    /// option: no `.options` card sets it, issue #16). `Some(d)`,
    /// `d ≥ 1` and finite: every `.tran` runs with its effective
    /// maximum step divided by `d` — the 4th argument of the card
    /// when given, else the default `min(TSTEP, (TSTOP−TSTART)/50)`.
    /// Everything derived from TMAX (`delmin`, the breakpoint merge
    /// radius, the step budget) follows, so the run is the one the
    /// deck would give with that card's TMAX edited to `TMAX/d`.
    /// It is not a `.options` key because TMAX is a parameter of the
    /// analysis card, and the divisor applies to every `.tran` card of
    /// the deck whatever TMAX each one carries. Any other value makes
    /// `.tran` fail with [`Error::Build`](crate::Error::Build).
    /// `None` (default): the card's TMAX as is.
    pub tmax_divisor: Option<f64>,
    /// `SEED`: the seed of the transient-noise sources
    /// (`TRNOISE`/`TRRANDOM`, ngspice-47 `.options seed=N`, which sets
    /// its `rndseed`). Default 1 — ngspice's start-up `rndseed` too
    /// (`main.c`) —, so a deck without the option is reproducible.
    /// The same seed gives the same noise bits on every platform (see
    /// `docs/dune-core.md` §Ruído transiente); ngspice's
    /// `seed=random` (a clock seed) is refused with a warning.
    pub seed: u64,
    /// `NOTRNOISE` (ngspice-47's `notrnoise` variable, read by
    /// `trnoise_state_gen`): every `TRNOISE` source runs with all its
    /// noise parameters zeroed — just its `DC` value, no grid, no
    /// breakpoints —, so a deck compares with and without noise
    /// without editing it. As in ngspice, `TRRANDOM` is untouched
    /// (its state generator never reads the variable). Default false.
    pub notrnoise: bool,
    /// Worker threads for the per-iteration device load (issue #9,
    /// lever 1; an API argument, not a SPICE option). `1` (default):
    /// the sequential load, exactly the 3f5's `CKTload`. `0`: one
    /// thread per core. `n > 1`: the devices are split into
    /// contiguous runs evaluated on `n` threads, and their stamps are
    /// replayed into the matrix in device order, so every result is
    /// bit-identical to `1` whatever the count. It only pays on
    /// circuits with many expensive devices (MOS/BSIM); see
    /// `docs/perf.md` for the measured break-even. Needs the
    /// `parallel` feature (native builds); without it (and on
    /// `wasm32`) the value is ignored and the load is sequential.
    pub load_threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            reltol: 1e-3,
            abstol: 1e-12,
            vntol: 1e-6,
            gmin: 1e-12,
            itl1: 100,
            itl2: 50,
            itl4: 10,
            gminsteps: 1,
            srcsteps: 10,
            temp: REFTEMP,
            tnom: REFTEMP,
            pivtol: 1e-13,
            pivrel: 1e-3,
            noopiter: false,
            trtol: 7.0,
            chgtol: 1e-14,
            method: Method::Trapezoidal,
            maxord: 2,
            nopage: false,
            defl: 1e-4,
            defw: 1e-4,
            defad: 0.0,
            defas: 0.0,
            max_tran_steps: None,
            ltra_break_rate: None,
            ltra_fit: false,
            load_threads: 1,
            tmax_divisor: None,
            seed: 1,
            notrnoise: false,
        }
    }
}

/// Floor of the automatic transient step budget
/// ([`Options::max_tran_steps`] `= None`).
pub const TRAN_STEP_FLOOR: usize = 10_000;

/// Internal timepoints allowed per TMAX span of `[0, TSTOP]` by the
/// automatic transient step budget (3.8× the corpus maximum of
/// 13.0, `ltra_2`).
pub const TRAN_STEPS_PER_TMAX: usize = 50;

impl Options {
    /// The step budget of a `.tran` running to `tstop` with effective
    /// maximum step `max_step` (see [`Options::max_tran_steps`]).
    pub fn tran_step_budget(&self, tstop: f64, max_step: f64) -> usize {
        match self.max_tran_steps {
            Some(n) => n,
            None => {
                let spans = (tstop / max_step).ceil();
                let auto = if spans.is_finite() && spans >= 0.0 {
                    (spans as usize).saturating_mul(TRAN_STEPS_PER_TMAX)
                } else {
                    usize::MAX
                };
                auto.max(TRAN_STEP_FLOOR)
            }
        }
    }

    /// Apply `.options` parameters from the IR. Unknown or
    /// not-yet-implemented options produce a warning, never an error
    /// (the 3f5 front end also lets unknown variables through).
    pub fn apply(&mut self, params: &[Param], diags: &mut Vec<Diagnostic>) {
        for p in params {
            let num = match &p.value {
                ParamValue::Num(v) => Some(*v),
                _ => None,
            };
            macro_rules! num_opt {
                ($field:ident) => {
                    match num {
                        Some(v) => self.$field = v,
                        None => diags.push(Diagnostic::warning(
                            format!("option {} needs a numeric value", p.name),
                            p.span,
                        )),
                    }
                };
            }
            match p.name.as_str() {
                "reltol" => num_opt!(reltol),
                "abstol" => num_opt!(abstol),
                "vntol" => num_opt!(vntol),
                "gmin" => num_opt!(gmin),
                "pivtol" => num_opt!(pivtol),
                "pivrel" => num_opt!(pivrel),
                "itl1" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl1 = v;
                    }
                }
                "itl2" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl2 = v;
                    }
                }
                "itl4" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl4 = v;
                    }
                }
                "maxord" => {
                    if let Some(v) = count(num, p, diags) {
                        self.maxord = v;
                    }
                }
                "trtol" => num_opt!(trtol),
                "chgtol" => num_opt!(chgtol),
                "method" => match &p.value {
                    ParamValue::Str(s) if s == "gear" => self.method = Method::Gear,
                    ParamValue::Str(s) if s == "trap" || s == "trapezoidal" => {
                        self.method = Method::Trapezoidal;
                    }
                    // Dune's opt-in L-stable composite (issue #6).
                    ParamValue::Str(s) if s == "trbdf2" => self.method = Method::TrBdf2,
                    _ => diags.push(Diagnostic::warning(
                        "method must be trap, gear or trbdf2".to_string(),
                        p.span,
                    )),
                },
                "ltrabkrate" => {
                    if let Some(v) = count(num, p, diags) {
                        self.ltra_break_rate = Some(v);
                    }
                }
                // Dune's opt-in LTRA recursive convolution (issue #8):
                // a bare flag or `ltrafit=0|1`.
                "ltrafit" => self.ltra_fit = num.is_none_or(|v| v != 0.0),
                "gminsteps" => {
                    if let Some(v) = count(num, p, diags) {
                        self.gminsteps = v;
                    }
                }
                "srcsteps" | "itl6" => {
                    if let Some(v) = count(num, p, diags) {
                        self.srcsteps = v;
                    }
                }
                "temp" => match num {
                    Some(v) => self.temp = v + CTOK,
                    None => diags.push(Diagnostic::warning(
                        "option temp needs a value in °C".to_string(),
                        p.span,
                    )),
                },
                "tnom" => match num {
                    Some(v) => self.tnom = v + CTOK,
                    None => diags.push(Diagnostic::warning(
                        "option tnom needs a value in °C".to_string(),
                        p.span,
                    )),
                },
                // ngspice-47 `inp.c`: `option seed=val|random`; a
                // value that `atoi` makes non-positive is skipped with
                // a warning. Dune never seeds from the clock.
                "seed" => match num {
                    Some(v) if v >= 1.0 && v.is_finite() && v < 9.007_199_254_740_992e15 => {
                        self.seed = v as u64;
                    }
                    _ => diags.push(Diagnostic::warning(
                        format!(
                            "option seed needs a positive integer (seed=random is not \
                             supported: runs must be reproducible); keeping seed={}",
                            self.seed
                        ),
                        p.span,
                    )),
                },
                // ngspice-47 `set notrnoise` / `.options notrnoise`
                // (a bare flag; `notrnoise=0|1` too).
                "notrnoise" => self.notrnoise = num.is_none_or(|v| v != 0.0),
                "noopiter" => self.noopiter = true,
                "nopage" => self.nopage = true,
                // `cktsopt.c` has a copy-paste bug that makes `defas`
                // set the drain area; ngspice fixed it and so do we
                // (`corpus/KNOWN_DIFFS.md`).
                "defl" => num_opt!(defl),
                "defw" => num_opt!(defw),
                "defad" => num_opt!(defad),
                "defas" => num_opt!(defas),
                // Recognized 3f5 options that only matter to later
                // phases (output formatting, statistics, devices).
                "itl3" | "itl5" | "acct" | "list" | "nomod" | "node" | "opts" | "bypass"
                | "badmos3" | "keepopinfo" => {}
                // `spiceif.c`'s `obsolete[]`: SPICE2 artifacts (lvlcod
                // generated machine code for the solver) that must
                // never do anything — the 3f5 words it "is obsolete".
                "limpts" | "limtim" | "lvlcod" => diags.push(Diagnostic::warning(
                    format!("option {} is an obsolete SPICE2 option; ignored", p.name),
                    p.span,
                )),
                other => diags.push(Diagnostic::warning(
                    format!("option {other} is not implemented yet; ignored"),
                    p.span,
                )),
            }
        }
    }
}

fn count(num: Option<f64>, p: &Param, diags: &mut Vec<Diagnostic>) -> Option<usize> {
    match num {
        Some(v) if v >= 0.0 && v.is_finite() && v < 1e9 => Some(v as usize),
        _ => {
            diags.push(Diagnostic::warning(
                format!("option {} needs a non-negative count", p.name),
                p.span,
            ));
            None
        }
    }
}
