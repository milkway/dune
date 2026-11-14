//! `.pss` — periodic steady state of a driven circuit by shooting
//! (issue #13, D-0028 track 4). No 3f5 contract: the analysis is new.
//! ngspice-47 has a `.pss` card (Perticaroli/Lannutti,
//! `analysis/dcpss.c`), but its algorithm is a free-running
//! oscillator search — it re-estimates the frequency from the
//! transient on every cycle and never solves `x(T) = x(0)`, so on a
//! driven circuit it drifts off the drive frequency (a 1 kHz RC
//! comes out at 750 Hz). Dune keeps ngspice's card and plot names
//! and replaces the method (docs/dune-core.md, "Regime permanente
//! periódico").
//!
//! **Method.** Shooting–Newton on the period map. With `Φ(x₀)` the
//! state a transient started from `x₀` at `t₀` reaches at `t₀ + T`,
//! the periodic steady state solves `F(x₀) = Φ(x₀) − x₀ = 0`;
//! Newton's step solves `(I − Φ′) δ = Φ(x₀) − x₀` with the monodromy
//! matrix `Φ′`. `Φ′` is never formed: GMRES needs only the products
//! `Φ′ v`, each one directional finite difference of two transients
//! retraced on the frozen grid of the current iterate (matrix-free
//! shooting, Telichevesky–Kundert–White 1995). A circuit with `k`
//! slow modes needs about `k + 1` products per Newton step, against
//! `n` for a dense finite-difference Jacobian; a linear circuit
//! converges in one Newton step.
//!
//! **Transient map.** Every shooting run is a fresh engine running
//! the full `DCtran` loop (LTE control, breakpoints, trap/Gear/
//! TR-BDF2, the step budget of D-0022) from the state `x₀` instead
//! of an operating point: the device states are seeded from `x₀`
//! ([`Engine::tran_period`]), so charges and junction voltages are
//! functions of `x₀` alone — the full MNA vector is the shooting
//! state, and its algebraic components fall out of the Jacobian on
//! their own (their columns of `Φ′` vanish).
//!
//! **Autonomous oscillators** (no periodic source, issue #20): the
//! period `T` is a Newton unknown, the phase pinned by holding
//! OSCNODE at its value on a rising mid-range crossing — see
//! `Circuit::pss_autonomous` and docs/dune-core.md.
//!
//! **Transmission lines** (issue #20): a line's state is its history,
//! and in the periodic regime the history is the orbit itself,
//! `x(t − τ) = x(t − τ + kT)`. The lines read the previous orbit as
//! their past (`matrix::PeriodicPast`), a fixed point iterated (Picard) on a
//! frozen step grid while Newton solves `x₀` — see
//! `Shooter::lines_excited`. Lossy LTRA lines need `.options
//! ltrafit` (their infinite periodic past is summed in closed form by
//! the recursive model) and have their DC drop pinned (`ltra.rs`).

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::circuit::{Circuit, Spec, VarKind, saved_node};
use crate::engine::{Engine, MAX_OUTPUT_VALUES, PeriodRun, StepRec, TranParams};
use crate::error::Error;
use crate::integ::Method;
use crate::matrix::PeriodicPast;
use crate::options::Options;
use crate::solution::Solution;
use crate::srcfn::Wave;

/// Plot name of the period in the time domain (ngspice-47's).
pub const PSS_TIME_PLOT: &str = "Time Domain Periodic Steady State Analysis";
/// Plot name of the harmonics (ngspice-47's).
pub const PSS_FREQ_PLOT: &str = "Frequency Domain Periodic Steady State Analysis";

/// The `.pss` request (see [`Circuit::pss`]).
#[derive(Clone, Debug, PartialEq)]
pub struct PssParams {
    /// Fundamental frequency in Hz; `None` takes it from the
    /// periodic sources (the greatest common divisor of their
    /// frequencies). Without periodic sources (an autonomous
    /// oscillator) it is a guess of the period's inverse, and `None`
    /// estimates it from the stabilization transient.
    pub freq: Option<f64>,
    /// Stabilization time: a plain transient runs from 0 to the first
    /// period boundary at or after `tstab` (at least one period)
    /// before the shooting starts there.
    pub tstab: f64,
    /// ngspice's oscillation node. A driven circuit only checks and
    /// reports it; an autonomous one (no periodic source) needs it:
    /// its phase condition holds this node at its value on a rising
    /// mid-range crossing, and its crossings estimate the period.
    pub osc_node: Option<String>,
    /// Samples per period in the time-domain plot (the plot has
    /// `points + 1` rows, both ends included, like ngspice's).
    pub points: usize,
    /// Rows of the frequency-domain plot: DC plus `harmonics − 1`
    /// harmonics (ngspice's count).
    pub harmonics: usize,
    /// Most shooting iterations (adaptive period transients).
    pub max_iter: usize,
    /// Scale of the convergence tolerance: converged when every
    /// unknown has `|x(t₀+T) − x(t₀)| ≤ steady_coeff · (RELTOL ·
    /// peak + VNTOL|ABSTOL)`, `peak` its largest magnitude over the
    /// period.
    pub steady_coeff: f64,
    /// Run the stabilization transient with `UIC`.
    pub uic: bool,
}

impl Default for PssParams {
    fn default() -> Self {
        PssParams {
            freq: None,
            tstab: 0.0,
            osc_node: None,
            points: 128,
            harmonics: 10,
            max_iter: 50,
            steady_coeff: 1.0,
            uic: false,
        }
    }
}

/// Result of a periodic steady-state analysis.
#[derive(Clone, Debug)]
pub struct Pss {
    /// One period sampled at `points + 1` equispaced times from `t₀`
    /// to `t₀ + T` ([`PSS_TIME_PLOT`], scale `time`; node voltages and
    /// branch currents as in `.tran`).
    pub time: Solution,
    /// The complex harmonics `X_k`, `k = 0 … harmonics − 1`, at
    /// `k·f` ([`PSS_FREQ_PLOT`], scale `frequency`): `X_0` the mean,
    /// and `x(t) = X_0 + Σ Re(X_k e^{jkωt})` — `.ac`'s phasor
    /// convention, phases referred to `t = 0`. ngspice writes only
    /// the magnitudes; `mag()` of these columns is that plot.
    pub spectrum: Solution,
    /// The period `T` (s): the drive's for a driven circuit, the
    /// shooting's result for an autonomous oscillator.
    pub period: f64,
    /// Start of the reported period (a multiple of `T`).
    pub t0: f64,
    /// The residual `max_i |x_i(t₀+T) − x_i(t₀)| / tol_i` of each
    /// shooting iteration (converged when ≤ 1).
    pub residuals: Vec<f64>,
    /// The period of each shooting iteration, aligned with
    /// `residuals` (constant for a driven circuit; the Newton iterate
    /// `T` of an autonomous one).
    pub periods: Vec<f64>,
    pub notes: Vec<String>,
}

/// The perturbation of the directional differences, in units of the
/// convergence tolerance (the scaled unknowns are `x_i / tol_i`).
const FD_EPS: f64 = 1e-2;
/// The replays of the Jacobian products tighten the Newton
/// tolerances of every step by this factor: a frozen-grid replay is
/// smooth in `x₀` only up to its Newton error, which the difference
/// would amplify by `1/ε`.
const REPLAY_TIGHTEN: f64 = 1e-3;
/// Most halvings of a Newton step whose residual did not decrease.
const MAX_BACKTRACK: usize = 4;

impl Circuit {
    /// Run `.pss`: the periodic steady state of the circuit driven at
    /// the fundamental `p.freq` (or the one its sources share), by
    /// shooting–Newton with matrix-free GMRES on the period map (see
    /// the module docs). Fails with [`Error::PssConvergence`] when
    /// `p.max_iter` shooting iterations do not bring the period
    /// mismatch within tolerance.
    pub fn pss(&self, opts: &Options, p: &PssParams) -> Result<Pss, Error> {
        // The shooting's own default method (`Options::pss_method`),
        // pinned so that every transient below integrates with it.
        let stab_method = opts.pss_stab_method();
        let opts = &Options {
            method: Some(opts.pss_method()),
            ..opts.clone()
        };
        let bad = |m: &str| Err(Error::Build(format!(".pss {m}")));
        if !(p.tstab.is_finite() && p.tstab >= 0.0) {
            return bad("stabilization time must be finite and not negative");
        }
        if p.points < 4 || p.points > 10_000_000 {
            return bad("needs 4 to 10 000 000 points per period");
        }
        if p.harmonics < 1 || p.harmonics > p.points / 2 {
            return bad("harmonics must be between 1 and points/2");
        }
        if p.max_iter < 1 {
            return bad("needs at least one shooting iteration");
        }
        if !(p.steady_coeff.is_finite() && p.steady_coeff > 0.0) {
            return bad("steady coefficient must be positive");
        }
        if opts.dev_noise {
            // Issue #22: the same reason as the TRNOISE/TRRANDOM
            // sources below. The option used to be ignored here
            // (only `.tran` installs the generators), so the result
            // looked as if it had taken the noise into account.
            return Err(Error::Build(
                ".pss: `.options devnoise` is on, and transient device noise is a random \
                 stimulus, which has no periodic steady state; turn it off for the shooting"
                    .into(),
            ));
        }
        for spec in &self.specs {
            let noisy = match spec {
                Spec::Vsource(v) => &v.wave,
                Spec::Isource(i) => &i.wave,
                _ => &None,
            };
            if matches!(noisy, Some(Wave::TrNoise(_) | Wave::TrRandom(_))) {
                // Issue #13, part 2: a random stimulus has no periodic
                // steady state; shooting would chase a moving target.
                return Err(Error::Unsupported {
                    name: spec.name().to_string(),
                    what: "a transient-noise source (TRNOISE/TRRANDOM) in .pss — a random \
                           stimulus has no periodic steady state"
                        .into(),
                });
            }
            if matches!(spec, Spec::Txl(_) | Spec::Cpl(_)) {
                // The KSPICE lines keep their history from an operating
                // point and have no periodic-past port (ngspice-47 has
                // no `.pss` model for them either).
                return Err(Error::Unsupported {
                    name: spec.name().to_string(),
                    what: "a KSPICE (Y/P) line in .pss".into(),
                });
            }
            if let Spec::Ltra(l) = spec
                && !opts.ltra_fit
            {
                // Issue #20: the periodic past of a lossy line is
                // infinite; only the recursive model sums it.
                let lp = crate::ltra::LtraParams::new(&l.name, &l.model, opts.reltol, opts.abstol)
                    .map_err(Error::Build)?;
                if matches!(
                    lp.case,
                    crate::ltra::LtraCase::Rlc | crate::ltra::LtraCase::Rc
                ) {
                    return Err(Error::Build(format!(
                        ".pss: the lossy (RLC/RC) line `{}` needs `.options ltrafit`: in the \
                         periodic regime its history is the orbit repeated over the whole past, \
                         which the exact convolution (a walk over a history that starts at an \
                         operating point) cannot hold, and the recursive model sums in closed \
                         form",
                        l.name
                    )));
                }
            }
        }
        let mut notes = Vec::new();
        let sources = self.source_frequencies()?;
        if sources.is_empty() {
            // No periodic stimulus: a free-running oscillator, its
            // period an unknown of the shooting (see `pss_autonomous`).
            return self.pss_autonomous(opts, p, stab_method);
        }
        let freq = match p.freq {
            Some(f) => {
                if !(f.is_finite() && f > 0.0) {
                    return bad("frequency must be positive");
                }
                for (name, fs) in &sources {
                    let k = fs / f;
                    if (k - k.round()).abs() > 1e-6 * k.max(1.0) || k.round() < 1.0 {
                        notes.push(format!(
                            "pss: source `{name}` ({fs:e} Hz) is not a harmonic of the \
                             fundamental {f:e} Hz; the circuit is not {:e}-s periodic",
                            1.0 / f
                        ));
                    }
                }
                f
            }
            None => harvest(&sources)?,
        };
        let period = 1.0 / freq;
        let tstep = period / p.points as f64;
        let max_step = f64::min(tstep, period / 50.0);
        let n_stab = (p.tstab / period - 1e-9).ceil().max(1.0);
        if n_stab > 1e9 {
            return bad("stabilization spans too many periods");
        }
        let t0 = n_stab * period;
        let run = PeriodRun {
            t0,
            period,
            tstep,
            max_step,
        };

        // Stabilization: a plain transient to t0.
        let built = self.build(opts)?;
        notes.extend(built.notes.iter().cloned());
        let mut engine = Engine::new(built);
        // Work guards (hostile cards must cost nothing, CLAUDE.md
        // princípio 4): a period transient keeps about `points` rows,
        // the stabilization takes `n_stab · points` steps, the DFT
        // `harmonics · points` products per unknown.
        let nvars = engine.vars.len() as f64;
        let pts = p.points as f64;
        if (pts + 1.0) * nvars > MAX_OUTPUT_VALUES as f64
            || n_stab * pts > 1e8
            || p.harmonics as f64 * pts * nvars > 1e9
        {
            return bad("asks for too many points or periods; refusing");
        }
        if let Some(node) = &p.osc_node {
            let lc = node.to_ascii_lowercase();
            if !engine.vars.names.contains(&lc) {
                return Err(Error::Build(format!(".pss: no node `{node}` for oscnode")));
            }
            notes.push(format!(
                "pss: oscnode `{lc}` is for autonomous circuits; the driven shooting does not use it"
            ));
        }
        let ports = self.line_ports(&engine.vars.names);
        let lines = !ports.is_empty();
        // With lines, the last stabilization period is the first guess
        // of the periodic past (only the rows kept change).
        let stab = engine.tran(
            opts,
            &TranParams {
                tstep,
                tstop: t0,
                tstart: if lines { t0 - period } else { t0 },
                tmax: Some(max_step),
                uic: p.uic,
            },
        )?;
        let names = engine.vars.names.clone();
        let kinds = engine.vars.kinds.clone();
        let mut x = engine.sol.clone();
        drop(engine);

        let mut shooter = Shooter {
            circuit: self,
            opts,
            tight: tightened(opts),
            run,
            kinds: &kinds,
            adaptive: 0,
            replays: 0,
            krylov: Vec::new(),
            past: if lines {
                Some(
                    Past::from_rows(&stab, None, t0 - period, period, max_step)
                        .ok_or_else(|| Error::Build(".pss: no stabilization period".into()))?,
                )
            } else {
                None
            },
            ports,
            history: Vec::new(),
            refresh: Vec::new(),
        };
        let mut residuals = Vec::new();
        let converged = if lines {
            shooter.lines_excited(&mut x, p, &mut residuals)?
        } else {
            // The last Newton step taken: (base point, step, base residual, fraction).
            let mut last: Option<(Vec<f64>, Vec<f64>, f64, f64)> = None;
            let min_frac = 0.5f64.powi(MAX_BACKTRACK as i32);
            loop {
                // A transient that fails from a Newton iterate (an
                // overshoot into a region the step control cannot cross)
                // counts as an infinite residual and is backtracked.
                let (mut period_run, rnorm) = match shooter.adaptive(&x) {
                    Ok(r) => {
                        let n = r.rnorm;
                        (Some(r), n)
                    }
                    Err(e) => match &last {
                        Some((_, _, _, frac)) if *frac > min_frac => (None, f64::INFINITY),
                        _ => return Err(e),
                    },
                };
                residuals.push(rnorm);
                if rnorm <= p.steady_coeff
                    && let Some(r) = period_run.take()
                {
                    break r;
                }
                if residuals.len() >= p.max_iter {
                    return Err(Error::PssConvergence {
                        iterations: residuals.len(),
                        residuals,
                        period: None,
                    });
                }
                // Backtrack a Newton step that did not reduce the residual.
                if let Some((base, step, base_norm, frac)) = &mut last
                    && rnorm > *base_norm
                    && *frac > min_frac
                {
                    *frac *= 0.5;
                    for i in 0..x.len() {
                        x[i] = base[i] + *frac * step[i];
                    }
                    continue;
                }
                let Some(period_run) = period_run else {
                    unreachable!("a failed run is always backtracked")
                };
                let step = shooter.newton_step(&x, &period_run)?;
                let base = x.clone();
                for i in 1..x.len() {
                    x[i] += step[i];
                }
                last = Some((base, step, rnorm, 1.0));
            }
        };

        notes.push(format!(
            "pss: period {period:e} s (fundamental {freq:e} Hz{}); shooting over \
             [{t0:e}, {:e}] s after {n_stab} stabilization period(s)",
            if p.freq.is_some() {
                String::new()
            } else {
                ", from the sources".to_string()
            },
            t0 + period
        ));
        notes.push(shooter.summary(&residuals));
        notes.extend(shooter.lines_note());

        let (time, spectrum) = outputs(&converged, &names, &kinds, p, t0, period);
        let periods = vec![period; residuals.len()];
        Ok(Pss {
            time,
            spectrum,
            period,
            t0,
            residuals,
            periods,
            notes,
        })
    }

    /// The port unknowns of the transmission lines (issue #20) — the
    /// four terminals and the two port-current equations of every
    /// T/O line, ground left out —, whose orbit must reproduce the
    /// periodic past the lines read.
    fn line_ports(&self, names: &[String]) -> Vec<usize> {
        let find = |n: &str| names.iter().position(|v| v.eq_ignore_ascii_case(n));
        let mut out = Vec::new();
        for spec in &self.specs {
            let (name, nodes) = match spec {
                Spec::Ltra(l) => (&l.name, &l.nodes),
                Spec::Tra(t) => (&t.name, &t.nodes),
                _ => continue,
            };
            let branches = [format!("{name}#i1"), format!("{name}#i2")];
            for n in nodes
                .iter()
                .map(String::as_str)
                .chain(branches.iter().map(String::as_str))
            {
                if let Some(i) = find(n)
                    && i != 0
                    && !out.contains(&i)
                {
                    out.push(i);
                }
            }
        }
        out
    }

    /// The frequencies the independent sources repeat at: SIN and
    /// SFFM carriers/modulations, PULSE `1/PER`. Waveforms that settle
    /// to a constant (EXP, PWL, a PULSE without PER) set none. A SIN
    /// or SFFM without an explicit frequency (its default is
    /// `1/TSTOP`) or a damped SIN has no period and is an error.
    fn source_frequencies(&self) -> Result<Vec<(String, f64)>, Error> {
        let mut out = Vec::new();
        for spec in &self.specs {
            let (name, wave) = match spec {
                Spec::Vsource(v) => (&v.name, &v.wave),
                Spec::Isource(i) => (&i.name, &i.wave),
                _ => continue,
            };
            let Some(wave) = wave else { continue };
            let given = |c: &[f64], i: usize| c.get(i).copied().filter(|&v| v != 0.0);
            let no_period = |what: &str| {
                Err(Error::Build(format!(
                    ".pss: source `{name}`: {what} has no fixed period"
                )))
            };
            match wave {
                Wave::Sin(c) => {
                    if given(c, 1).is_none() {
                        continue;
                    }
                    if given(c, 4).is_some() {
                        return no_period("a damped SIN");
                    }
                    match given(c, 2) {
                        Some(f) => out.push((name.clone(), f.abs())),
                        None => return no_period("a SIN without a frequency"),
                    }
                }
                Wave::Sffm(c) => {
                    if given(c, 1).is_none() {
                        continue;
                    }
                    match given(c, 2) {
                        Some(f) => out.push((name.clone(), f.abs())),
                        None => return no_period("an SFFM without a carrier frequency"),
                    }
                    if given(c, 3).is_some() {
                        match given(c, 4) {
                            Some(f) => out.push((name.clone(), f.abs())),
                            None => return no_period("an SFFM without a signal frequency"),
                        }
                    }
                }
                Wave::Pulse(c) => {
                    let v1 = c.first().copied().unwrap_or(0.0);
                    let v2 = c.get(1).copied().unwrap_or(0.0);
                    if v1 != v2
                        && let Some(per) = given(c, 6)
                    {
                        out.push((name.clone(), 1.0 / per.abs()));
                    }
                }
                Wave::Exp(_) | Wave::Pwl(_) => {}
                // Refused by `Circuit::pss` before the harvest.
                Wave::TrNoise(_) | Wave::TrRandom(_) => {}
            }
        }
        Ok(out)
    }
}

impl Circuit {
    /// `.pss` of an autonomous oscillator (no periodic source): the
    /// period is an unknown. Shooting–Newton on the augmented system
    /// `[Φ_T(x₀) − x₀ = 0; x₀[osc] = v_ref]`, the phase condition
    /// fixing `OSCNODE` at its value on a rising crossing of its
    /// mid-range (see [`osc_start`]); the bordered Jacobian
    /// `[I − Φ′, −∂Φ/∂T]` (osc column dropped) is solved by the same
    /// matrix-free GMRES, `∂Φ/∂T` by a replay on the frozen grid
    /// stretched to `T + δT`.
    fn pss_autonomous(
        &self,
        opts: &Options,
        p: &PssParams,
        stab_method: Method,
    ) -> Result<Pss, Error> {
        let bad = |m: &str| Err(Error::Build(format!(".pss {m}")));
        let guess = match p.freq {
            Some(f) if !(f.is_finite() && f > 0.0) => return bad("frequency must be positive"),
            Some(f) => Some(1.0 / f),
            None => None,
        };
        let Some(node) = &p.osc_node else {
            return bad(
                "of an autonomous circuit (no periodic source) needs the oscillation node \
                 OSCNODE",
            );
        };
        if p.tstab <= 0.0 {
            return bad(
                "of an autonomous circuit needs a stabilization time TSTAB: the oscillation \
                 must start and be measured before the shooting",
            );
        }
        let built = self.build(opts)?;
        let mut notes: Vec<String> = built.notes.clone();
        let mut engine = Engine::new(built);
        let lc = node.to_ascii_lowercase();
        let Some(osc) = engine.vars.names.iter().position(|n| *n == lc) else {
            return Err(Error::Build(format!(".pss: no node `{node}` for oscnode")));
        };
        if osc == 0 || engine.vars.kinds[osc] != VarKind::Node {
            return Err(Error::Build(format!(
                ".pss: oscnode `{node}` must be a circuit node"
            )));
        }
        // Stabilization grid: with a guess, the driven one; without,
        // TMAX = TSTAB/STAB_POINTS (the LTE control refines it).
        let (tstep, max_step) = match guess {
            Some(t) => (t / p.points as f64, f64::min(t / p.points as f64, t / 50.0)),
            None => (p.tstab / STAB_POINTS, p.tstab / STAB_POINTS),
        };
        let nvars = engine.vars.len() as f64;
        let pts = p.points as f64;
        if (pts + 1.0) * nvars > MAX_OUTPUT_VALUES as f64
            || p.tstab / tstep > 1e8
            || p.harmonics as f64 * pts * nvars > 1e9
        {
            return bad("asks for too many points or periods; refusing");
        }
        // The start-up runs on its own method
        // (`Options::pss_stab_method`): an oscillation has to grow
        // out of the kick on a grid that does not know its period.
        let stab_opts = Options {
            method: Some(stab_method),
            ..opts.clone()
        };
        let rows = engine.tran(
            &stab_opts,
            &TranParams {
                tstep,
                tstop: p.tstab,
                tstart: p.tstab * (1.0 - STAB_WINDOW),
                tmax: Some(max_step),
                uic: p.uic,
            },
        )?;
        let names = engine.vars.names.clone();
        let kinds = engine.vars.kinds.clone();
        let ports = self.line_ports(&names);
        drop(engine);
        // Lines: a continuation from the stabilization (the refine
        // run, and the shooting) reads the last period before it as
        // its periodic past.
        let no_past = || {
            Error::Build(
                ".pss: the stabilization window holds less than one period before the \
                 shooting start; lengthen TSTAB"
                    .into(),
            )
        };
        let past_before = |rows: &[(f64, Vec<f64>)], at: f64, t: f64, max_step: f64| {
            if ports.is_empty() {
                Ok(None)
            } else {
                Past::from_rows(rows, None, at - t, t, max_step)
                    .map(Some)
                    .ok_or_else(no_past)
            }
        };
        let mut start = osc_start(&rows, osc, &lc, guess, opts)?;
        let mut start_rows = rows;
        if guess.is_none() {
            // The stabilization grid knew no period: refine the
            // estimate (and the start) on a short continuation at
            // the shooting's own grid, T/POINTS, from where it ended.
            let (t_end, x_end) = start_rows.last().expect("the stabilization has rows");
            let t = start.period;
            let tstep = t / p.points as f64;
            let refine = PeriodRun {
                t0: *t_end,
                period: REFINE_PERIODS * t,
                tstep,
                max_step: f64::min(tstep, t / 50.0),
            };
            let past = past_before(&start_rows, *t_end, t, max_step)?;
            let mut engine = Engine::new(self.build(opts)?);
            engine.set_periodic_past(past.as_ref().map(|p| p.at(t)));
            let fine = engine.tran_period(opts, &refine, x_end, &mut Vec::new())?;
            notes.push(start.note);
            start = osc_start(&fine, osc, &lc, None, opts)?;
            start_rows = fine;
            start.note = format!(
                "{} (refined over {REFINE_PERIODS} periods continued from TSTAB at T/POINTS)",
                start.note
            );
        }
        notes.push(start.note.clone());
        let t0 = start.t0;
        let mut x = start.x0;
        let mut period = start.period;

        let run_for = |t: f64| PeriodRun {
            t0,
            period: t,
            tstep: t / p.points as f64,
            max_step: f64::min(t / p.points as f64, t / 50.0),
        };
        let first_past = past_before(&start_rows, t0, period, run_for(period).max_step)?;
        drop(start_rows);
        let mut shooter = Shooter {
            circuit: self,
            opts,
            tight: tightened(opts),
            run: run_for(period),
            kinds: &kinds,
            adaptive: 0,
            replays: 0,
            krylov: Vec::new(),
            past: first_past,
            ports: ports.clone(),
            history: Vec::new(),
            refresh: Vec::new(),
        };
        let mut residuals = Vec::new();
        let mut periods = Vec::new();
        let mut last: Option<OscStep> = None;
        let min_frac = 0.5f64.powi(MAX_BACKTRACK as i32);
        let fail = |residuals: Vec<f64>, period: f64| Error::PssConvergence {
            iterations: residuals.len(),
            residuals,
            period: Some(period),
        };
        let converged = if shooter.past.is_some() {
            let (r, dt) = shooter.lines_autonomous(
                &mut x,
                &mut period,
                osc,
                p,
                &run_for,
                &mut residuals,
                &mut periods,
            )?;
            last = Some(OscStep {
                x: Vec::new(),
                t: period,
                dx: Vec::new(),
                dt,
                base_norm: 0.0,
                frac: 1.0,
            });
            r
        } else {
            loop {
                shooter.run = run_for(period);
                let (mut period_run, rnorm) = match shooter.adaptive(&x) {
                    Ok(r) => {
                        let n = r.rnorm;
                        (Some(r), n)
                    }
                    Err(e) => match &last {
                        Some(l) if l.frac > min_frac => (None, f64::INFINITY),
                        Some(_) => {
                            residuals.push(f64::INFINITY);
                            periods.push(period);
                            return Err(fail(residuals, period));
                        }
                        None => return Err(e),
                    },
                };
                residuals.push(rnorm);
                periods.push(period);
                // At least one Newton step: an estimate already within
                // the tolerance still gets its T corrected (quadratically)
                // — the mismatch criterion alone admits a period error of
                // about tol/|dx/dt| — and the step's δT is reported.
                if rnorm <= p.steady_coeff
                    && last.is_some()
                    && let Some(r) = period_run.take()
                {
                    break r;
                }
                if residuals.len() >= p.max_iter {
                    return Err(fail(residuals, period));
                }
                if let Some(l) = &mut last
                    && rnorm > l.base_norm
                    && l.frac > min_frac
                {
                    l.frac *= 0.5;
                    for ((xi, bx), sx) in x.iter_mut().zip(&l.x).zip(&l.dx) {
                        *xi = bx + l.frac * sx;
                    }
                    period = l.t + l.frac * l.dt;
                    continue;
                }
                let Some(period_run) = period_run else {
                    unreachable!("a failed run is always backtracked")
                };
                let (mut sx, mut st) = shooter.newton_step_osc(&x, period, osc, &period_run)?;
                // A Newton step never moves T by more than MAX_DT of itself
                // (far from the orbit the linearized phase drift is not
                // to be trusted); the whole step is scaled.
                if st.abs() > MAX_DT * period {
                    let k = MAX_DT * period / st.abs();
                    sx.iter_mut().for_each(|v| *v *= k);
                    st *= k;
                }
                let base = x.clone();
                for i in 1..x.len() {
                    x[i] += sx[i];
                }
                let bt = period;
                period += st;
                last = Some(OscStep {
                    x: base,
                    t: bt,
                    dx: sx,
                    dt: st,
                    base_norm: rnorm,
                    frac: 1.0,
                });
            }
        };

        // The trivial periodic solution — the equilibrium — is
        // periodic for every T; a converged "orbit" with no swing at
        // OSCNODE is that, not an oscillation.
        let (lo, hi) = converged
            .rows
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), r| {
                (a.min(r.1[osc]), b.max(r.1[osc]))
            });
        if hi - lo <= 10.0 * converged.tol[osc] {
            return Err(Error::Build(format!(
                ".pss: the shooting settled on the equilibrium (v({lc}) swings {:e} V); \
                 the circuit does not oscillate",
                hi - lo
            )));
        }
        let hist: Vec<String> = residuals
            .iter()
            .zip(&periods)
            .map(|(r, t)| format!("[{r:.3e}, {t:.9e}]"))
            .collect();
        let dt = last.as_ref().map_or(0.0, |l| l.dt * l.frac);
        notes.push(format!(
            "pss: autonomous oscillator: period {period:e} s (frequency {:e} Hz) is the \
             shooting's result (last Newton correction {dt:.3e} s); [residual/tolerance, T] \
             per iteration: {}; phase condition v({lc}) = {:e} V at t0 = {t0:e} s",
            1.0 / period,
            hist.join(", "),
            x[osc]
        ));
        notes.push(shooter.summary(&residuals));
        notes.extend(shooter.lines_note());
        let (time, spectrum) = outputs(&converged, &names, &kinds, p, t0, period);
        Ok(Pss {
            time,
            spectrum,
            period,
            t0,
            residuals,
            periods,
            notes,
        })
    }
}

/// Default stabilization grid of an autonomous `.pss` without a
/// frequency guess: TMAX = TSTAB / this.
const STAB_POINTS: f64 = 1000.0;
/// The fraction of TSTAB (its end) the stabilization keeps and
/// measures the oscillation on.
const STAB_WINDOW: f64 = 0.5;
/// Rising crossings the period estimate averages over (at most).
const EST_PERIODS: usize = 4;
/// Length, in estimated periods, of the continuation that refines a
/// period estimated on the stabilization grid.
const REFINE_PERIODS: f64 = 5.5;
/// Grids the shooting with transmission lines converges on: the
/// first adaptive run's, then one refreshed from the converged orbit.
const GRID_PASSES: usize = 2;
/// Largest relative change of `T` in one Newton step.
const MAX_DT: f64 = 0.2;

/// The last Newton step of an autonomous shooting, for backtracking.
struct OscStep {
    /// The base point `(x, T)` …
    x: Vec<f64>,
    t: f64,
    /// … the full step from it …
    dx: Vec<f64>,
    dt: f64,
    /// … the residual at the base, and the fraction of the step taken.
    base_norm: f64,
    frac: f64,
}

/// Where the autonomous shooting starts.
#[derive(Debug)]
struct OscStart {
    /// The rising mid-range crossing of OSCNODE the period starts at.
    t0: f64,
    /// The state there (cubic interpolation of the stabilization).
    x0: Vec<f64>,
    /// The initial period: the guess, or the crossing estimate.
    period: f64,
    note: String,
}

/// Rising crossings of `level` by unknown `k` over `rows`, linearly
/// interpolated.
fn rising_crossings(rows: &[(f64, Vec<f64>)], k: usize, level: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for w in rows.windows(2) {
        let (a, b) = (w[0].1[k], w[1].1[k]);
        if a < level && b >= level {
            out.push(w[0].0 + (w[1].0 - w[0].0) * (level - a) / (b - a));
        }
    }
    out
}

/// The start of the autonomous shooting from the stabilization rows
/// (the last `STAB_WINDOW` of TSTAB). The level is the mid-range
/// `(max + min)/2` of OSCNODE — over the last `EST_PERIODS` periods
/// once a period is known, so a still-growing amplitude does not bias
/// it; the start `t0` is the last rising crossing of that level,
/// where `|dv/dt|` is near its largest (the transversality the phase
/// condition needs). Without a guess the period is the mean spacing
/// of the last `EST_PERIODS` + 1 rising crossings — a frequency
/// counter's estimate, exact for any waveform shape once the orbit is
/// settled, and its spread is reported.
fn osc_start(
    rows: &[(f64, Vec<f64>)],
    osc: usize,
    name: &str,
    guess: Option<f64>,
    opts: &Options,
) -> Result<OscStart, Error> {
    let span = |from: f64| {
        rows.iter()
            .filter(|r| r.0 >= from)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), r| {
                (a.min(r.1[osc]), b.max(r.1[osc]))
            })
    };
    let (Some(first), Some(end)) = (rows.first(), rows.last()) else {
        return Err(Error::Build(".pss: empty stabilization".into()));
    };
    let (t_first, t_end) = (first.0, end.0);
    let (lo, hi) = span(t_first);
    let floor = 10.0 * (opts.reltol * lo.abs().max(hi.abs()) + opts.vntol);
    let quiet = || {
        Err(Error::Build(format!(
            ".pss: the circuit does not oscillate at the oscillation node within TSTAB \
             ({:e} V of swing over the last {:.0}% of it); kick it out of its equilibrium \
             (an initial condition with UIC, or a supply ramp by PWL/PULSE) or lengthen \
             TSTAB",
            hi - lo,
            100.0 * STAB_WINDOW
        )))
    };
    if (hi - lo).is_nan() || hi - lo <= floor {
        return quiet();
    }
    // First pass: the mid-range of the whole window.
    let c = rising_crossings(rows, osc, 0.5 * (lo + hi));
    let estimate = |c: &[f64]| -> Option<(f64, f64)> {
        if c.len() < 3 {
            return None;
        }
        let k = EST_PERIODS.min(c.len() - 1);
        let tail = &c[c.len() - 1 - k..];
        let mean = (tail[k] - tail[0]) / k as f64;
        let spread = tail
            .windows(2)
            .map(|w| ((w[1] - w[0]) / mean - 1.0).abs())
            .fold(0.0, f64::max);
        Some((mean, spread))
    };
    let rough = match guess {
        Some(t) => t,
        None => match estimate(&c) {
            Some((t, _)) => t,
            None => {
                return Err(Error::Build(format!(
                    ".pss: {} rising crossing(s) of the oscillation node's mid-range in the \
                     last {:.0}% of TSTAB; estimating the period needs 3 — lengthen TSTAB or \
                     give FREQ",
                    c.len(),
                    100.0 * STAB_WINDOW
                )));
            }
        },
    };
    // Second pass: the mid-range of the last few periods.
    let (lo2, hi2) = span(f64::max(t_first, t_end - EST_PERIODS as f64 * rough));
    if (hi2 - lo2).is_nan() || hi2 - lo2 <= floor {
        return quiet();
    }
    let level = 0.5 * (lo2 + hi2);
    let c = rising_crossings(rows, osc, level);
    let Some(&t0) = c.last() else {
        return quiet();
    };
    // A measured period beats a guess: FREQ sets the stabilization
    // grid, and seeds T only when the window has too few crossings.
    let guessed = guess.map_or_else(String::new, |g| format!("; FREQ's guess was {g:e} s"));
    let (period, how) = match (estimate(&c), guess) {
        (Some((t, spread)), _) => {
            let k = EST_PERIODS.min(c.len() - 1);
            (
                t,
                format!(
                    "estimate {t:e} s from the last {k} periods between rising crossings \
                     (spread {spread:.1e}){guessed}"
                ),
            )
        }
        (None, Some(g)) => (
            g,
            format!("guess {g:e} s from FREQ (too few crossings to measure one)"),
        ),
        (None, None) => (rough, format!("estimate {rough:e} s (first pass)")),
    };
    let all = [0, rows.len() - 1];
    let x0 = interp(rows, &all, t0);
    Ok(OscStart {
        t0,
        x0,
        period,
        note: format!(
            "pss: no periodic source — autonomous oscillator on v({}); initial period {how}; \
             mid-range level {level:e} V, swing {:e} V; shooting starts at its last rising \
             crossing, t0 = {t0:e} s",
            name,
            hi2 - lo2
        ),
    })
}

/// The fundamental shared by the source frequencies: their greatest
/// common divisor, to 1e-9 relative, with at most a million
/// harmonics up to the highest.
fn harvest(sources: &[(String, f64)]) -> Result<f64, Error> {
    let Some(fmax) = sources.iter().map(|s| s.1).reduce(f64::max) else {
        return Err(Error::Build(
            ".pss: no periodic source; give the fundamental frequency".into(),
        ));
    };
    let tol = 1e-9 * fmax;
    let gcd = |a: f64, b: f64| {
        let (mut a, mut b) = (a.max(b), a.min(b));
        while b > tol {
            let r = (a - b * (a / b).round()).abs();
            a = b;
            b = if r <= tol { 0.0 } else { r };
        }
        a
    };
    let f = sources.iter().map(|s| s.1).fold(0.0, gcd);
    if f.is_nan() || f <= 0.0 || fmax / f > 1e6 {
        return Err(Error::Build(
            ".pss: the source frequencies share no fundamental; give it".into(),
        ));
    }
    Ok(f)
}

/// The Newton tolerances of the Jacobian replays.
fn tightened(opts: &Options) -> Options {
    let mut o = opts.clone();
    o.reltol *= REPLAY_TIGHTEN;
    o.vntol *= REPLAY_TIGHTEN;
    o.abstol *= REPLAY_TIGHTEN;
    o
}

/// One adaptive period transient and what the shooting reads off it.
struct PeriodOut {
    rows: Vec<(f64, Vec<f64>)>,
    log: Vec<StepRec>,
    /// `x(t₀ + T) − x(t₀)`.
    resid: Vec<f64>,
    /// Per-unknown tolerance (index 0, ground, is 1).
    tol: Vec<f64>,
    /// The period mismatch `max_i |x_i(t₀+T) − x_i(t₀)| / tol_i`.
    rnorm: f64,
    /// Lines (issue #20): how far the orbit is from the periodic past
    /// it read, `max |x_i(t) − past_i(t)| / tol_i` over the lines' port
    /// unknowns (0 without lines).
    hnorm: f64,
    /// The period of the run.
    period: f64,
}

impl PeriodOut {
    /// The residual reported per iteration: the period mismatch, and
    /// the lines' history mismatch when there are lines.
    fn total(&self) -> f64 {
        self.rnorm.max(self.hnorm)
    }
}

/// Aitken's extrapolation of the autonomous line iteration. Each
/// pass of `lines_autonomous` is a fixed-point iteration on
/// `(x₀, T, orbit)` whose slowest mode is the nearly neutral phase
/// mode of the history (shifting the past in time): the residual
/// falls by a constant factor `ρ` per replay (0.85 on `osc_tline`).
/// Once three successive residuals show the same `ρ`, the iterate is
/// moved to the limit of that geometric sequence, `z + ρ/(1−ρ)·Δz`,
/// and the estimate starts over.
#[derive(Default)]
struct Aitken {
    /// The last iterates.
    seen: Vec<Iterate>,
}

/// One iterate of the autonomous line iteration.
struct Iterate {
    residual: f64,
    x: Vec<f64>,
    period: f64,
    /// The orbit's values, row by row.
    orbit: Vec<Vec<f64>>,
}

impl Aitken {
    /// Two successive ratios must agree this closely.
    const AGREE: f64 = 0.05;

    /// Record the iterate just computed (`cur` is the orbit of
    /// `x`/`period` before the Newton step that produced them) and,
    /// when the residuals are geometric, extrapolate all three in
    /// place.
    fn push(&mut self, x: &mut [f64], period: &mut f64, cur: &mut PeriodOut, t0: f64) -> bool {
        let values: Vec<Vec<f64>> = cur.rows.iter().map(|(_, v)| v.clone()).collect();
        if self
            .seen
            .last()
            .is_some_and(|l| l.orbit.len() != values.len())
        {
            self.seen.clear();
        }
        self.seen.push(Iterate {
            residual: cur.total(),
            x: x.to_vec(),
            period: *period,
            orbit: values,
        });
        let n = self.seen.len();
        if n < 3 {
            return false;
        }
        let (a, b, c) = (&self.seen[n - 3], &self.seen[n - 2], &self.seen[n - 1]);
        let (r1, r2) = (b.residual / a.residual, c.residual / b.residual);
        if !(0.3..0.98).contains(&r2) || (r1 - r2).abs() > Self::AGREE * r2 {
            return false;
        }
        let f = r2 / (1.0 - r2);
        for (xi, (ci, bi)) in x.iter_mut().zip(c.x.iter().zip(&b.x)) {
            *xi += f * (ci - bi);
        }
        let old = cur.period;
        *period += f * (c.period - b.period);
        // The orbit moves with it; its times stretch to its period
        // the way the grid does.
        let new = old + f * (c.period - b.period);
        for ((t, v), (cv, bv)) in cur.rows.iter_mut().zip(c.orbit.iter().zip(&b.orbit)) {
            *t = t0 + (*t - t0) * new / old;
            for (vi, (ci, bi)) in v.iter_mut().zip(cv.iter().zip(bv)) {
                *vi += f * (ci - bi);
            }
        }
        for rec in &mut cur.log {
            rec.time = t0 + (rec.time - t0) * new / old;
        }
        cur.period = new;
        self.seen.clear();
        true
    }
}

/// The periodic past of the transmission lines (issue #20) — the
/// device-facing orbit — and the breakpoint rows to interpolate it on
/// when checking the next orbit against it.
struct Past {
    orbit: PeriodicPast,
    /// Breakpoint indices into the closed period (0 = the start
    /// boundary, 1… the interior samples, `m + 1` the end boundary;
    /// only the ends for a past from the stabilization, which has no
    /// step log).
    breaks: Vec<usize>,
}

impl Past {
    /// The past from `rows` over `[start, start + period]`: its
    /// interior samples (boundaries are the shooting state). Samples
    /// within `CKTminBreak` of a boundary are left out, as an accept
    /// there would be. `log` (a shooting run's) marks the breakpoint
    /// rows (`rows[j]` is `log[j − 1]`'s point). `None` when `rows` do
    /// not reach back to within a step (`max_step`) of `start`.
    fn from_rows(
        rows: &[(f64, Vec<f64>)],
        log: Option<&[StepRec]>,
        start: f64,
        period: f64,
        max_step: f64,
    ) -> Option<Past> {
        let mb = 5e-5 * max_step;
        // The first row may land up to a step after the boundary (the
        // boundary sample is the shooting state anyway).
        if rows.first().is_none_or(|r| r.0 > start + max_step + mb) {
            return None;
        }
        let mut phase = Vec::new();
        let mut kept = Vec::new();
        let mut breaks = vec![0usize];
        for (j, (t, x)) in rows.iter().enumerate() {
            if *t - start <= mb || start + period - *t <= mb {
                continue;
            }
            phase.push((*t - start) / period);
            kept.push(x.clone());
            if let Some(log) = log
                && j >= 1
                && log.get(j - 1).is_some_and(|r| r.brk)
            {
                breaks.push(phase.len());
            }
        }
        breaks.push(phase.len() + 1);
        Some(Past {
            orbit: PeriodicPast {
                period,
                phase,
                rows: kept,
            },
            breaks,
        })
    }

    /// The device-facing past for a run of period `period`.
    fn at(&self, period: f64) -> PeriodicPast {
        PeriodicPast {
            period,
            ..self.orbit.clone()
        }
    }

    /// `max_i |x_i(t) − past_i(t)| / tol_i` over the lines' `ports`
    /// at the interior rows of a run from `x` at `t0` on the past's own
    /// grid ([`interp`] reproduces a sample exactly at its time).
    fn mismatch(
        &self,
        rows: &[(f64, Vec<f64>)],
        x: &[f64],
        t0: f64,
        period: f64,
        ports: &[usize],
        tol: &[f64],
    ) -> f64 {
        let mut closed: Vec<(f64, Vec<f64>)> = Vec::with_capacity(self.orbit.phase.len() + 2);
        closed.push((0.0, x.to_vec()));
        for (u, r) in self.orbit.phase.iter().zip(&self.orbit.rows) {
            closed.push((u * period, r.clone()));
        }
        closed.push((period, x.to_vec()));
        let mut worst = 0.0f64;
        let n = rows.len();
        for (t, sol) in rows.iter().take(n.saturating_sub(1)).skip(1) {
            let v = interp(&closed, &self.breaks, t - t0);
            for &i in ports {
                worst = worst.max((sol[i] - v[i]).abs() / tol[i]);
            }
        }
        worst
    }
}

/// A frozen step grid (an adaptive period run's log) and the run it
/// was taken on, replayable at any period by uniform stretching.
struct Grid {
    run: PeriodRun,
    log: Vec<StepRec>,
}

impl Grid {
    fn of(run: PeriodRun, log: &[StepRec]) -> Grid {
        Grid {
            run,
            log: log.to_vec(),
        }
    }

    /// The run and log at period `t`: every time from `t₀` scaled by
    /// `t/T` (TSTEP and TMAX too), the end pinned to `t₀ + t`.
    fn at(&self, t: f64) -> (PeriodRun, Vec<StepRec>) {
        stretch(&self.run, &self.log, t)
    }
}

/// `run`/`log` stretched to period `t` (see [`Grid::at`]); unchanged
/// at the run's own period.
fn stretch(run: &PeriodRun, log: &[StepRec], t: f64) -> (PeriodRun, Vec<StepRec>) {
    if t == run.period {
        return (*run, log.to_vec());
    }
    let k = t / run.period;
    let stretched = PeriodRun {
        t0: run.t0,
        period: t,
        tstep: run.tstep * k,
        max_step: run.max_step * k,
    };
    let mut log: Vec<StepRec> = log
        .iter()
        .map(|r| StepRec {
            time: run.t0 + (r.time - run.t0) * k,
            ..*r
        })
        .collect();
    if let Some(end) = log.last_mut() {
        end.time = run.t0 + t;
    }
    (stretched, log)
}

struct Shooter<'a> {
    circuit: &'a Circuit,
    opts: &'a Options,
    tight: Options,
    run: PeriodRun,
    kinds: &'a [VarKind],
    adaptive: usize,
    replays: usize,
    /// GMRES dimension of each Newton step.
    krylov: Vec<usize>,
    /// Transmission lines (issue #20): the periodic past they read
    /// (`None` without lines), their port unknowns, and the history
    /// mismatch of every run.
    past: Option<Past>,
    ports: Vec<usize>,
    history: Vec<f64>,
    /// The period mismatch of each grid-refresh run.
    refresh: Vec<f64>,
}

/// A fresh engine for one shooting run, with the lines' periodic past
/// (if any) at the run's period.
fn period_engine(
    circuit: &Circuit,
    opts: &Options,
    past: Option<&Past>,
    period: f64,
) -> Result<Engine, Error> {
    let mut e = Engine::new(circuit.build(opts)?);
    e.set_periodic_past(past.map(|p| p.at(period)));
    Ok(e)
}

impl Shooter<'_> {
    fn engine(&self) -> Result<Engine, Error> {
        period_engine(self.circuit, self.opts, self.past.as_ref(), self.run.period)
    }

    /// The lines' note: the history mismatch per frozen-grid
    /// iteration and the period mismatch of the grid refresh.
    fn lines_note(&self) -> Option<String> {
        self.past.as_ref()?;
        let fmt = |v: &[f64]| -> String {
            if v.is_empty() {
                "-".to_string()
            } else {
                v.iter()
                    .map(|r| format!("{r:.3e}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        Some(format!(
            "pss: transmission lines read the orbit as their history (x(t − τ) = \
             x(t − τ + kT)): Picard on it, Newton on x0, on a frozen grid (the first \
             guess is the stabilization's last period); history mismatch per replay over {} \
             line port unknown(s): {}; grid refreshed once from the converged orbit, the \
             refresh run's period mismatch (the cross-grid discretization of the lines, not \
             iterated on): {}",
            self.ports.len(),
            fmt(&self.history),
            fmt(&self.refresh)
        ))
    }

    /// The adaptive transient over the period from `x`.
    fn adaptive(&mut self, x: &[f64]) -> Result<PeriodOut, Error> {
        self.adaptive += 1;
        let mut log = Vec::new();
        let rows = self
            .engine()?
            .tran_period(self.opts, &self.run, x, &mut log)?;
        Ok(self.measure(rows, log, x))
    }

    /// What the shooting reads off a period run from `x`: the period
    /// mismatch and the per-unknown tolerances.
    fn measure(&self, rows: Vec<(f64, Vec<f64>)>, log: Vec<StepRec>, x: &[f64]) -> PeriodOut {
        let n = x.len();
        let last = &rows.last().expect("the period has its end point").1;
        let mut peak = vec![0.0f64; n];
        for (_, sol) in &rows {
            for i in 1..n {
                peak[i] = peak[i].max(sol[i].abs());
            }
        }
        let mut tol = vec![1.0; n];
        let mut resid = vec![0.0; n];
        let mut rnorm = 0.0f64;
        for i in 1..n {
            let floor = match self.kinds[i] {
                VarKind::Branch => self.opts.abstol,
                _ => self.opts.vntol,
            };
            tol[i] = self.opts.reltol * peak[i] + floor;
            resid[i] = last[i] - x[i];
            rnorm = rnorm.max(resid[i].abs() / tol[i]);
        }
        PeriodOut {
            rows,
            log,
            resid,
            tol,
            rnorm,
            hnorm: 0.0,
            period: self.run.period,
        }
    }

    /// Lines (issue #20): one replay of the interval from `x` on the
    /// frozen `grid` (an adaptive run's step log, stretched to the
    /// current period), the lines reading the orbit `cur` as their
    /// periodic past — the Picard half. The replay's rows sit at the
    /// times of the past's samples, so the mismatch between the orbit
    /// and the history it read is measured exactly. That past stays
    /// the shooter's for the Newton half.
    fn replay_on(&mut self, x: &[f64], grid: &Grid, cur: &PeriodOut) -> Result<PeriodOut, Error> {
        let (t0, max_step) = (self.run.t0, self.run.max_step);
        let past = Past::from_rows(&cur.rows, Some(&cur.log), t0, cur.period, max_step)
            .expect("a period run starts at t0");
        let (run, log) = grid.at(self.run.period);
        self.replays += 1;
        let rows = period_engine(self.circuit, self.opts, Some(&past), run.period)?
            .tran_replay_rows(self.opts, &run, x, &log)?;
        let mut r = self.measure(rows, log, x);
        r.hnorm = past.mismatch(&r.rows, x, t0, run.period, &self.ports, &r.tol);
        self.history.push(r.hnorm);
        self.past = Some(past);
        Ok(r)
    }

    /// The driven shooting with transmission lines (issue #20). The
    /// lines' state is their history, which in the periodic regime is
    /// the orbit itself, `x(t − τ) = x(t − τ + kT)`; the shooting
    /// unknown stays `x₀` and the history is a fixed point solved
    /// alongside. An adaptive run (its past: the stabilization's last
    /// period) fixes the grid; on that frozen grid each iteration
    /// replays the interval with the previous orbit as the past
    /// (Picard) and, while the period mismatch exceeds the tolerance,
    /// takes a Newton step on `x₀` with that past frozen (the excited
    /// shooting's GMRES). Converged on the grid when both the period
    /// mismatch and the history mismatch are within tolerance. The
    /// grid is then refreshed once — an adaptive run from the result
    /// picks one for the converged orbit — and the iteration converges
    /// again on it; that orbit is the result.
    ///
    /// The frozen grid is what lets the history converge at all.
    /// Across grids the lines are not reproducible to the SPICE
    /// tolerance: the slope rule plants echo breakpoints on the *sign*
    /// of a slope (so two runs place a delayed corner up to a step
    /// apart — tens of tolerances at the corner), and the LTRA
    /// convolution's error is set by its grid, not by RELTOL (it has no
    /// LTE control). The refresh run's period mismatch is that
    /// cross-grid discretization, and is reported, not iterated on.
    fn lines_excited(
        &mut self,
        x: &mut [f64],
        p: &PssParams,
        residuals: &mut Vec<f64>,
    ) -> Result<PeriodOut, Error> {
        let fail = |residuals: &[f64]| Error::PssConvergence {
            iterations: residuals.len(),
            residuals: residuals.to_vec(),
            period: None,
        };
        let mut seed = self.adaptive(x)?;
        residuals.push(seed.rnorm);
        for pass in 0..GRID_PASSES {
            let grid = Grid::of(self.run, &seed.log);
            let mut cur = seed;
            let done = loop {
                if residuals.len() >= p.max_iter {
                    return Err(fail(residuals));
                }
                let r = self.replay_on(x, &grid, &cur)?;
                residuals.push(r.total());
                if r.total() <= p.steady_coeff {
                    break r;
                }
                if r.rnorm > p.steady_coeff {
                    let step = self.newton_step(x, &r)?;
                    for i in 1..x.len() {
                        x[i] += step[i];
                    }
                }
                cur = r;
            };
            if pass + 1 == GRID_PASSES {
                return Ok(done);
            }
            self.past = Some(self.past_of(&done));
            seed = self.adaptive(x)?;
            residuals.push(seed.rnorm);
            self.refresh.push(seed.rnorm);
        }
        unreachable!("the last pass returns")
    }

    /// The autonomous shooting with transmission lines: the scheme of
    /// [`Shooter::lines_excited`] with the Newton step of
    /// [`Shooter::newton_step_osc`] (`x₀` and `T`; the frozen grid and
    /// the past stretch with `T`, both being functions of phase), at
    /// least one Newton step (D-0035). Returns the orbit and the last
    /// period correction.
    #[allow(clippy::too_many_arguments)]
    fn lines_autonomous(
        &mut self,
        x: &mut [f64],
        period: &mut f64,
        osc: usize,
        p: &PssParams,
        run_for: &dyn Fn(f64) -> PeriodRun,
        residuals: &mut Vec<f64>,
        periods: &mut Vec<f64>,
    ) -> Result<(PeriodOut, f64), Error> {
        let fail = |residuals: &[f64], period: f64| Error::PssConvergence {
            iterations: residuals.len(),
            residuals: residuals.to_vec(),
            period: Some(period),
        };
        self.run = run_for(*period);
        let mut seed = self.adaptive(x)?;
        residuals.push(seed.rnorm);
        periods.push(*period);
        let mut dt = 0.0;
        let mut stepped = false;
        for pass in 0..GRID_PASSES {
            let grid = Grid::of(self.run, &seed.log);
            let mut cur = seed;
            let mut aitken = Aitken::default();
            // The replay after an extrapolation only measures it.
            let mut settle = false;
            let done = loop {
                if residuals.len() >= p.max_iter {
                    return Err(fail(residuals, *period));
                }
                self.run = run_for(*period);
                let r = self.replay_on(x, &grid, &cur)?;
                residuals.push(r.total());
                periods.push(*period);
                if r.total() <= p.steady_coeff && stepped {
                    break r;
                }
                if (r.rnorm > p.steady_coeff && !settle) || !stepped {
                    let (mut sx, mut st) = self.newton_step_osc(x, *period, osc, &r)?;
                    if st.abs() > MAX_DT * *period {
                        let k = MAX_DT * *period / st.abs();
                        sx.iter_mut().for_each(|v| *v *= k);
                        st *= k;
                    }
                    for i in 1..x.len() {
                        x[i] += sx[i];
                    }
                    *period += st;
                    dt = st;
                    stepped = true;
                }
                cur = r;
                settle = aitken.push(x, period, &mut cur, self.run.t0);
            };
            if pass + 1 == GRID_PASSES {
                return Ok((done, dt));
            }
            self.run = run_for(*period);
            self.past = Some(self.past_of(&done));
            seed = self.adaptive(x)?;
            residuals.push(seed.rnorm);
            periods.push(*period);
            self.refresh.push(seed.rnorm);
        }
        unreachable!("the last pass returns")
    }

    /// The periodic past a converged orbit leaves for the next run.
    fn past_of(&self, done: &PeriodOut) -> Past {
        Past::from_rows(
            &done.rows,
            Some(&done.log),
            self.run.t0,
            done.period,
            self.run.max_step,
        )
        .expect("a period run starts at t0")
    }

    /// The Newton step `δ` of `(I − Φ′) δ = Φ(x) − x`, by GMRES on the
    /// scaled unknowns `x_i / tol_i` with `Φ′ v` from frozen-grid
    /// replays. The replays first run at tightened Newton tolerances;
    /// if one of them fails, the step is redone at the deck's.
    fn newton_step(&mut self, x: &[f64], at: &PeriodOut) -> Result<Vec<f64>, Error> {
        match self.newton_step_with(x, at, true) {
            Ok(s) => Ok(s),
            Err(Error::IterLim) => self.newton_step_with(x, at, false),
            Err(e) => Err(e),
        }
    }

    fn newton_step_with(
        &mut self,
        x: &[f64],
        at: &PeriodOut,
        tight: bool,
    ) -> Result<Vec<f64>, Error> {
        let opts = if tight { &self.tight } else { self.opts };
        let (circuit, build_opts, run) = (self.circuit, self.opts, self.run);
        let past = self.past.as_ref();
        let n = x.len();
        let d = &at.tol;
        let replays = std::cell::Cell::new(0usize);
        let replay = |x0: &[f64]| -> Result<Vec<f64>, Error> {
            replays.set(replays.get() + 1);
            period_engine(circuit, build_opts, past, run.period)?
                .tran_replay(opts, &run, x0, &at.log)
        };
        let base = replay(x)?;
        // Scaled residual, unknowns 1..n.
        let b: Vec<f64> = (1..n).map(|i| at.resid[i] / d[i]).collect();
        let bnorm = norm(&b);
        let lin_tol = f64::min(1e-2, 1e-4 * bnorm);
        let mut apply = |v: &[f64]| -> Result<Vec<f64>, Error> {
            let mut xp = x.to_vec();
            for i in 1..n {
                xp[i] += FD_EPS * d[i] * v[i - 1];
            }
            let y = replay(&xp)?;
            Ok((1..n)
                .map(|i| v[i - 1] - (y[i] - base[i]) / (FD_EPS * d[i]))
                .collect())
        };
        let solved = gmres(&b, n - 1, lin_tol, &mut apply);
        self.replays += replays.get();
        let (ds, dim) = solved?;
        self.krylov.push(dim);
        let mut step = vec![0.0; n];
        for i in 1..n {
            step[i] = ds[i - 1] * d[i];
        }
        Ok(step)
    }

    /// The Newton step `(δx, δT)` of an autonomous oscillator: the
    /// bordered system `(I − Φ′) δx − (∂Φ/∂T) δT = Φ_T(x) − x` with
    /// `δx[osc] = 0` (the phase condition fixes OSCNODE), square
    /// again because `T` takes the place of `x[osc]` among the
    /// unknowns. GMRES on the scaled unknowns (`x_i / tol_i`,
    /// `T / (RELTOL·T)`); one replay per product perturbs `x` and `T`
    /// together, `T` by stretching the frozen grid uniformly — the
    /// replay is then smooth in `T` as it is in `x` (every step, and
    /// every coefficient of the integration formulas, scales with it).
    fn newton_step_osc(
        &mut self,
        x: &[f64],
        period: f64,
        osc: usize,
        at: &PeriodOut,
    ) -> Result<(Vec<f64>, f64), Error> {
        match self.newton_step_osc_with(x, period, osc, at, true) {
            Ok(s) => Ok(s),
            Err(Error::IterLim) => self.newton_step_osc_with(x, period, osc, at, false),
            Err(e) => Err(e),
        }
    }

    fn newton_step_osc_with(
        &mut self,
        x: &[f64],
        period: f64,
        osc: usize,
        at: &PeriodOut,
        tight: bool,
    ) -> Result<(Vec<f64>, f64), Error> {
        let opts = if tight { &self.tight } else { self.opts };
        let (circuit, build_opts, run) = (self.circuit, self.opts, self.run);
        let n = x.len();
        let d = &at.tol;
        let s_t = self.opts.reltol * period;
        // Unknown i sits in slot i − 1 and T in OSCNODE's slot, so
        // the operator stays identity plus low rank (the identity's
        // osc entry is replaced by the −∂Φ/∂T column), which is what
        // makes GMRES converge in a few products.
        let slot = |i: usize| i - 1;
        let t_slot = osc - 1;
        let replays = std::cell::Cell::new(0usize);
        let past = self.past.as_ref();
        let replay = |x0: &[f64], t: f64| -> Result<Vec<f64>, Error> {
            replays.set(replays.get() + 1);
            // Lines: the past stretches with the period (it is the
            // orbit as a function of phase).
            let mut engine = period_engine(circuit, build_opts, past, t)?;
            if t == period {
                return engine.tran_replay(opts, &run, x0, &at.log);
            }
            let k = t / period;
            let stretched = PeriodRun {
                t0: run.t0,
                period: t,
                tstep: run.tstep * k,
                max_step: run.max_step * k,
            };
            let mut log: Vec<StepRec> = at
                .log
                .iter()
                .map(|r| StepRec {
                    time: run.t0 + (r.time - run.t0) * k,
                    ..*r
                })
                .collect();
            if let Some(end) = log.last_mut() {
                end.time = run.t0 + t;
            }
            engine.tran_replay(opts, &stretched, x0, &log)
        };
        let base = replay(x, period)?;
        let b: Vec<f64> = (1..n).map(|i| at.resid[i] / d[i]).collect();
        let bnorm = norm(&b);
        let lin_tol = f64::min(1e-2, 1e-4 * bnorm);
        let mut apply = |u: &[f64]| -> Result<Vec<f64>, Error> {
            let mut xp = x.to_vec();
            for i in 1..n {
                if i != osc {
                    xp[i] += FD_EPS * d[i] * u[slot(i)];
                }
            }
            let y = replay(&xp, period + FD_EPS * s_t * u[t_slot])?;
            Ok((1..n)
                .map(|i| {
                    let ui = if i == osc { 0.0 } else { u[slot(i)] };
                    ui - (y[i] - base[i]) / (FD_EPS * d[i])
                })
                .collect())
        };
        let solved = gmres(&b, n - 1, lin_tol, &mut apply);
        self.replays += replays.get();
        let (us, dim) = solved?;
        self.krylov.push(dim);
        let mut step = vec![0.0; n];
        for i in 1..n {
            if i != osc {
                step[i] = us[slot(i)] * d[i];
            }
        }
        Ok((step, us[t_slot] * s_t))
    }

    fn summary(&self, residuals: &[f64]) -> String {
        let hist: Vec<String> = residuals.iter().map(|r| format!("{r:.3e}")).collect();
        format!(
            "pss: {} shooting iteration(s), residual/tolerance per iteration: {}; \
             {} adaptive and {} replayed period transients (GMRES dimensions {:?})",
            residuals.len(),
            hist.join(", "),
            self.adaptive,
            self.replays,
            self.krylov
        )
    }
}

/// A matrix-free operator `v ↦ A v` (one product may run transients).
type LinearMap<'a> = dyn FnMut(&[f64]) -> Result<Vec<f64>, Error> + 'a;

fn norm(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Restart-free GMRES (Saad–Schultz, modified Gram–Schmidt with one
/// reorthogonalization, Givens rotations) for `A y = b` from `y = 0`,
/// at most `m` iterations, stopping at residual `≤ tol`. Returns the
/// solution and the Krylov dimension used.
fn gmres(b: &[f64], m: usize, tol: f64, apply: &mut LinearMap) -> Result<(Vec<f64>, usize), Error> {
    let n = b.len();
    let beta = norm(b);
    if beta == 0.0 || m == 0 {
        return Ok((vec![0.0; n], 0));
    }
    let mut v: Vec<Vec<f64>> = vec![b.iter().map(|x| x / beta).collect()];
    let mut h: Vec<Vec<f64>> = Vec::new(); // columns, each of length j + 2
    let mut cs: Vec<f64> = Vec::new();
    let mut sn: Vec<f64> = Vec::new();
    let mut g = vec![beta];
    let mut k = 0;
    while k < m {
        let mut w = apply(&v[k])?;
        let mut col = vec![0.0; k + 2];
        for _pass in 0..2 {
            for (i, vi) in v.iter().enumerate() {
                let c: f64 = w.iter().zip(vi).map(|(a, b)| a * b).sum();
                col[i] += c;
                for (wj, vj) in w.iter_mut().zip(vi) {
                    *wj -= c * vj;
                }
            }
        }
        let hn = norm(&w);
        col[k + 1] = hn;
        for i in 0..k {
            let t = cs[i] * col[i] + sn[i] * col[i + 1];
            col[i + 1] = -sn[i] * col[i] + cs[i] * col[i + 1];
            col[i] = t;
        }
        let r = col[k].hypot(col[k + 1]);
        let (c, s) = if r == 0.0 {
            (1.0, 0.0)
        } else {
            (col[k] / r, col[k + 1] / r)
        };
        col[k] = r;
        col[k + 1] = 0.0;
        cs.push(c);
        sn.push(s);
        g.push(-s * g[k]);
        g[k] *= c;
        h.push(col);
        k += 1;
        let breakdown = hn <= 1e-14 * beta;
        if g[k].abs() <= tol || breakdown {
            break;
        }
        v.push(w.iter().map(|x| x / hn).collect());
    }
    // Back substitution on the k×k triangle.
    let mut y = vec![0.0; k];
    for i in (0..k).rev() {
        let mut s = g[i];
        for j in i + 1..k {
            s -= h[j][i] * y[j];
        }
        y[i] = if h[i][i] != 0.0 { s / h[i][i] } else { 0.0 };
    }
    let mut x = vec![0.0; n];
    for (j, yj) in y.iter().enumerate() {
        for (xi, vi) in x.iter_mut().zip(&v[j]) {
            *xi += yj * vi;
        }
    }
    Ok((x, k))
}

/// The time-domain and frequency-domain plots of the converged period.
fn outputs(
    per: &PeriodOut,
    names: &[String],
    kinds: &[VarKind],
    p: &PssParams,
    t0: f64,
    period: f64,
) -> (Solution, Solution) {
    let n_pts = p.points;
    let times: Vec<f64> = (0..=n_pts)
        .map(|k| t0 + period * k as f64 / n_pts as f64)
        .collect();
    // The rows at breakpoints (and both ends): a stencil never
    // straddles a source corner.
    let rows = &per.rows;
    let mut breaks = vec![0usize];
    breaks.extend((1..rows.len()).filter(|&j| per.log[j - 1].brk));
    if *breaks.last().unwrap_or(&0) != rows.len() - 1 {
        breaks.push(rows.len() - 1);
    }

    let mut variables = vec![("time".to_string(), VarType::Time)];
    let mut sel: Vec<usize> = Vec::new();
    for i in 1..names.len() {
        match kinds[i] {
            VarKind::Node if saved_node(&names[i]) => {
                variables.push((format!("v({})", names[i]), VarType::Voltage));
                sel.push(i);
            }
            VarKind::Branch => {
                variables.push((format!("i({})", names[i]), VarType::Current));
                sel.push(i);
            }
            _ => {}
        }
    }
    let mut cols: Vec<Vec<f64>> = vec![times.clone()];
    cols.extend(sel.iter().map(|_| Vec::with_capacity(times.len())));
    for (k, &t) in times.iter().enumerate() {
        let vals: Vec<f64> = if k == n_pts {
            rows[rows.len() - 1].1.clone()
        } else if k == 0 {
            rows[0].1.clone()
        } else {
            interp(rows, &breaks, t)
        };
        for (c, &i) in sel.iter().enumerate() {
            cols[c + 1].push(vals[i]);
        }
    }

    // DFT of the first `points` samples (the last one closes the period).
    let h = p.harmonics;
    let nn = n_pts as f64;
    let mut fvars = vec![("frequency".to_string(), VarType::Frequency)];
    fvars.extend(variables[1..].iter().cloned());
    let mut fcols: Vec<Vec<Complex64>> = vec![
        (0..h)
            .map(|k| Complex64::new(k as f64 / period, 0.0))
            .collect(),
    ];
    for col in &cols[1..] {
        let samples = &col[..n_pts];
        let mut out = Vec::with_capacity(h);
        for k in 0..h {
            let mut acc = Complex64::new(0.0, 0.0);
            for (m, &s) in samples.iter().enumerate() {
                // Reduce k·m mod N before the angle: exact arguments.
                let km = (k * m) % n_pts;
                let ang = -2.0 * std::f64::consts::PI * km as f64 / nn;
                acc += s * Complex64::new(ang.cos(), ang.sin());
            }
            out.push(if k == 0 { acc / nn } else { acc * (2.0 / nn) });
        }
        fcols.push(out);
    }
    (
        Solution {
            plotname: PSS_TIME_PLOT.to_string(),
            variables,
            data: Data::Real(cols),
            scale: Some(0),
        },
        Solution {
            plotname: PSS_FREQ_PLOT.to_string(),
            variables: fvars,
            data: Data::Complex(fcols),
            scale: Some(0),
        },
    )
}

/// The solution at `t` by cubic Lagrange interpolation over the four
/// accepted points around it, kept inside the breakpoint segment
/// (lower order when the segment has fewer points; `breaks` are the
/// sorted breakpoint rows, both ends included).
fn interp(rows: &[(f64, Vec<f64>)], breaks: &[usize], t: f64) -> Vec<f64> {
    // Rows j and j+1 bracket t.
    let j = rows.partition_point(|r| r.0 <= t).clamp(1, rows.len() - 1) - 1;
    let b = breaks.partition_point(|&b| b <= j);
    let lo = breaks[b - 1];
    let hi = breaks.get(b).copied().unwrap_or(rows.len() - 1).max(j + 1);
    let want = 4usize.min(hi - lo + 1);
    let mut a = j.saturating_sub(1).max(lo);
    if a + want - 1 > hi {
        a = hi + 1 - want;
    }
    let n = rows[0].1.len();
    let mut out = vec![0.0; n];
    for p in a..a + want {
        let mut w = 1.0;
        for q in a..a + want {
            if q != p {
                w *= (t - rows[q].0) / (rows[p].0 - rows[q].0);
            }
        }
        for (o, v) in out.iter_mut().zip(&rows[p].1) {
            *o += w * v;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvest_takes_the_common_fundamental() {
        let s = |f: &[f64]| -> Vec<(String, f64)> { f.iter().map(|&f| ("v".into(), f)).collect() };
        assert!((harvest(&s(&[1e3])).unwrap() - 1e3).abs() < 1e-9);
        assert!((harvest(&s(&[1e3, 3e3])).unwrap() - 1e3).abs() < 1e-6);
        assert!((harvest(&s(&[2e3, 3e3])).unwrap() - 1e3).abs() < 1e-6);
        assert!((harvest(&s(&[50e3, 1.0 / 20e-6])).unwrap() - 50e3).abs() < 1e-3);
        assert!(harvest(&s(&[1.0, std::f64::consts::PI])).is_err());
        assert!(harvest(&[]).is_err());
    }

    #[test]
    fn gmres_solves_a_small_system() {
        // A = I - u wᵀ (a rank-one monodromy): converges in 2 steps.
        let u = [0.5, -0.2, 0.1];
        let w = [0.3, 0.4, -0.6];
        let mut apply = |v: &[f64]| -> Result<Vec<f64>, Error> {
            let d: f64 = w.iter().zip(v).map(|(a, b)| a * b).sum();
            Ok(v.iter().zip(&u).map(|(vi, ui)| vi - ui * d).collect())
        };
        let b = [1.0, 2.0, 3.0];
        let (y, k) = gmres(&b, 3, 1e-13, &mut apply).unwrap();
        assert!(k <= 2);
        let ay = apply(&y).unwrap();
        for (a, b) in ay.iter().zip(&b) {
            assert!((a - b).abs() < 1e-12);
        }
    }
}
