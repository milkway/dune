//! The simulation engine: `NIiter` (`ni/niiter.c`), `NIconvTest`
//! (`ni/niconv.c`), the gmin/source-stepping driver `CKTop`
//! (`ckt/cktop.c`), the DC sweep `DCtrCurv` (`ckt/dctrcurv.c`) and
//! the transient loop `DCtran` (`ckt/dctran.c`) with its breakpoint
//! table (`cktsetbk.c`, `cktclrbk.c`) and LTE step control
//! (`ckttrunc.c`).

use std::collections::HashMap;

use crate::circuit::{Built, VarKind, Vars};
use crate::devices::{Device, SensParam};
use crate::disto::{DistoCtx, DistoInput, DistoKernels, DistoMode, DistoSetupCtx};
use crate::error::Error;
use crate::integ::{Method, TerrCtx, com_cof};
use crate::linalg::{LinalgError, LinearSolver};
use crate::matrix::{
    AcCtx, AcceptCtx, AnKind, InitMode, LoadCtx, LoadEnv, MatrixBuilder, NoiseCtx, PzCtx, Slot,
    StampSink, Var,
};
use crate::measure::almost_equal_ulps;
use crate::options::Options;
use crate::state::States;
use num_complex::Complex64;

/// Memory guard for sweep/transient outputs: `points × vars` of f64.
pub(crate) const MAX_OUTPUT_VALUES: usize = 50_000_000;

/// Refuse a frequency sweep whose PLANNED point count already blows
/// [`MAX_OUTPUT_VALUES`], before any work. The per-iteration checks
/// inside the loops stay (they also cover unplanned growth), but a
/// hostile card (`.ac lin 1e30 1 2`) must cost nothing — on wasm an
/// out-of-memory or minutes-long refusal cannot be interrupted.
pub(crate) fn sweep_budget(
    step_type: AcStepType,
    n: usize,
    fstart: f64,
    fstop: f64,
    nvars: usize,
    what: &str,
) -> Result<(), Error> {
    let intervals = match step_type {
        AcStepType::Linear => 1.0,
        AcStepType::Decade => (fstop / fstart).log10().abs().ceil() + 1.0,
        AcStepType::Octave => (fstop / fstart).log2().abs().ceil() + 1.0,
    };
    let planned = n as f64 * intervals * nvars as f64;
    if planned.is_finite() && planned <= MAX_OUTPUT_VALUES as f64 {
        Ok(())
    } else {
        Err(Error::Build(format!(
            "{what} sweep produces too many points; refusing"
        )))
    }
}

/// `CKTgminFactor` (ngspice-47 default, `cktntask.c`: `TSKgminFactor
/// = 10`).
const GMIN_FACTOR: f64 = 10.0;
/// `CKTgshunt` (ngspice-47 default 0; not a 3f5 option).
const GSHUNT: f64 = 0.0;
/// The ngspice-47 default `optran 1 1 1 100n 10u 0` (frontend/init.c:
/// 77-93): the transient operating point steps at most 100 ns …
const OPTRAN_STEP: f64 = 100e-9;
/// … up to 10 µs, with no supply ramp.
const OPTRAN_FINAL: f64 = 10e-6;

/// ngspice-47's `NIiter` floor (`niiter.c:37-39`: "some convergence
/// issues that get resolved by increasing max iter"). The Dune's own
/// `NIiter` keeps the 3f5 limits (`itl1`/`itl2`/`itl4` as given); the
/// floor applies only inside the ngspice-only fallback degrees of
/// `CKTop` (dynamic and true gmin, Gillespie source stepping,
/// `OPtran`), whose step adaptation was tuned against it.
fn ng_iter(maxiter: usize) -> usize {
    maxiter.max(100)
}

/// The degree of the `CKTop` fallback chain that closed the last cold
/// operating point (tests and statistics only; not API).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpStep {
    Direct,
    DynamicGmin,
    TrueGmin,
    Spice3Gmin,
    GillespieSrc,
    Spice3Src,
    OpTran,
}

/// The adaptive walk of ngspice's `dynamic_gmin` (`cktop.c:188-249`)
/// and `new_gmin` (`cktop.c:376-441`): the gmin to try next, starting
/// at `1e-2/CKTgminFactor`. A converged step divides the gmin by the
/// factor — grown ×√factor (capped at `CKTgminFactor`) after a fast
/// step (≤ ¼ of `itl2` iterations), shrunk to `max(√factor,
/// slow_floor)` after a slow one (> ¾) —, landing exactly on the
/// target when the next division would pass it; a failed step backs
/// the factor off to its 4th root and retries from the last good
/// gmin, giving up once the factor is below 1.00005. `slow_floor` is
/// the one literal difference of the two walks: 1.00005 (diagonal)
/// and 3 (true gmin).
#[derive(Clone, Debug)]
pub(crate) struct GminWalk {
    factor: f64,
    old_gmin: f64,
    /// The gmin of the step about to be tried.
    pub gmin: f64,
    gtarget: f64,
    slow_floor: f64,
}

impl GminWalk {
    pub fn new(gtarget: f64, slow_floor: f64) -> Self {
        let factor = GMIN_FACTOR;
        let old_gmin = 1e-2;
        GminWalk {
            factor,
            old_gmin,
            gmin: old_gmin / factor,
            gtarget,
            slow_floor,
        }
    }

    /// A step converged in `iters` iterations: `true` when it was at
    /// the target (the walk is done), else the next gmin is set.
    pub fn converged(&mut self, iters: usize, itl2: usize) -> bool {
        if self.gmin <= self.gtarget {
            return true;
        }
        if iters <= itl2 / 4 {
            self.factor *= self.factor.sqrt();
            if self.factor > GMIN_FACTOR {
                self.factor = GMIN_FACTOR;
            }
        }
        if iters > 3 * itl2 / 4 {
            self.factor = f64::max(self.factor.sqrt(), self.slow_floor);
        }
        self.old_gmin = self.gmin;
        if self.gmin < self.factor * self.gtarget {
            self.factor = self.gmin / self.gtarget;
            self.gmin = self.gtarget;
        } else {
            self.gmin /= self.factor;
        }
        false
    }

    /// A step failed: `true` when the walk gives up, else the retry
    /// gmin is set.
    pub fn failed(&mut self) -> bool {
        if self.factor < 1.00005 {
            return true;
        }
        self.factor = self.factor.sqrt().sqrt();
        self.gmin = self.old_gmin / self.factor;
        false
    }
}

/// The source ramp of ngspice's `gillespie_src` (`cktop.c:553-641`):
/// `srcFact` climbs from the converged all-zero solution by `raise`
/// (initially 0.001), which grows ×1.5 after a fast step (≤ ¼ of
/// `itl2` iterations) and halves after a slow one (> ¾). A failure
/// divides `raise` by 10 (capped at 0.01) and re-solves at the last
/// converged factor; the ramp gives up when a failed step was within
/// 1e-8 of it or `raise` drops below 1e-7, and is done when a solve
/// at `srcFact = 1` converged.
#[derive(Clone, Debug)]
pub(crate) struct SrcRamp {
    raise: f64,
    /// The last converged `srcFact`.
    pub conv_fact: f64,
    /// The `srcFact` of the step about to be tried.
    pub src_fact: f64,
}

impl SrcRamp {
    pub fn new() -> Self {
        SrcRamp {
            raise: 0.001,
            conv_fact: 0.0,
            src_fact: 0.001,
        }
    }

    /// The C's `do … while ((raise >= 1e-7) && (ConvFact < 1))`.
    pub fn finished(&self) -> bool {
        !(self.raise >= 1e-7 && self.conv_fact < 1.0)
    }

    pub fn converged(&mut self, iters: usize, itl2: usize) {
        self.conv_fact = self.src_fact;
        self.src_fact = self.conv_fact + self.raise;
        if iters <= itl2 / 4 {
            self.raise *= 1.5;
        }
        if iters > 3 * itl2 / 4 {
            self.raise *= 0.5;
        }
        self.src_fact = self.src_fact.min(1.0);
    }

    /// `true` when the ramp gives up at once (the C's `break`).
    pub fn failed(&mut self) -> bool {
        if self.src_fact - self.conv_fact < 1e-8 {
            return true;
        }
        self.raise /= 10.0;
        if self.raise > 0.01 {
            self.raise = 0.01;
        }
        self.src_fact = self.conv_fact;
        false
    }
}

/// Whether the sorted breakpoint table has an entry within `r` of `t`
/// (the LTRA breakpoint hygiene's dedupe test).
fn within_radius(breaks: &[f64], t: f64, r: f64) -> bool {
    let i = breaks.partition_point(|&b| b < t);
    let after = breaks.get(i).is_some_and(|&b| b - t <= r);
    let before = i > 0 && t - breaks[i - 1] <= r;
    after || before
}

/// The positions Sparse 1.3's `LoadGmin` (`spsmp.c:460`) adds the
/// diagonal gmin to: `Matrix->Diag[]` after `spMNA_Preorder`
/// (`sputils.c`), which the 3f5 and ngspice-47 (Sparse is its default
/// solver; KLU only under `.options klu`) run once on the first loaded
/// matrix. The preorder fills every structurally zero diagonal that
/// has a *symmetric twin* — a pair `(r, j)`, `(j, r)` both of
/// magnitude exactly 1, i.e. the ±1 incidences of a voltage-source
/// branch — by swapping columns `j` and `r`: lone twins first, then
/// the first column with several, repeated until none is left. The
/// swapped columns' diagonal becomes the twins themselves, so under
/// gmin stepping a voltage source's incidence reads `1 + gmin` (the
/// source's node sits at `V/(1+gmin)`) and that node gets no shunt to
/// ground — what the oracle does, measured on schmitt's path. Every
/// other diagonal present gets the gmin as before.
fn gmin_slots(mb: &MatrixBuilder, values: &[f64]) -> Vec<Slot> {
    let entries = mb.entries();
    let n = mb.pattern().n;
    // Internal column j (1-based) holds the elements of an external
    // column, sorted by row as Sparse links them: (row, slot).
    let mut cols: Vec<Vec<(usize, Slot)>> = vec![Vec::new(); n + 1];
    for (k, &(r, c)) in entries.iter().enumerate() {
        cols[c + 1].push((r + 1, Slot(k + 1)));
    }
    for col in &mut cols {
        col.sort_unstable_by_key(|e| e.0);
    }
    let mut diag: Vec<Option<Slot>> = vec![None; n + 1];
    for (j, col) in cols.iter().enumerate().skip(1) {
        diag[j] = col.iter().find(|e| e.0 == j).map(|e| e.1);
    }
    let one = |s: Slot| values[s.0].abs() == 1.0;
    // `CountTwins`: (count capped at 2, first pair as (row, twin slots)).
    let count_twins = |cols: &Vec<Vec<(usize, Slot)>>, j: usize| {
        let mut twins = 0;
        let mut first = None;
        for &(row, s1) in &cols[j] {
            if !one(s1) {
                continue;
            }
            if let Some(&(_, s2)) = cols[row].iter().find(|e| e.0 == j)
                && one(s2)
            {
                twins += 1;
                if twins >= 2 {
                    return (twins, first);
                }
                first = Some((row, s1, s2));
            }
        }
        (twins, first)
    };
    // `SwapCols`: Diag[j] = twin at row j, Diag[row] = twin at row `row`.
    let swap = |cols: &mut Vec<Vec<(usize, Slot)>>,
                diag: &mut Vec<Option<Slot>>,
                j: usize,
                (row, s1, s2): (usize, Slot, Slot)| {
        cols.swap(j, row);
        diag[j] = Some(s2);
        diag[row] = Some(s1);
    };
    let mut start_at = 1;
    loop {
        let mut another = false;
        let mut swapped = false;
        // The C's `for (J = StartAt; …)` reads StartAt once; a new
        // StartAt is for the next pass.
        let from = start_at;
        for j in from..=n {
            if diag[j].is_none() {
                match count_twins(&cols, j) {
                    (1, Some(t)) => {
                        swap(&mut cols, &mut diag, j, t);
                        swapped = true;
                    }
                    (t, _) if t > 1 && !another => {
                        another = true;
                        start_at = j;
                    }
                    _ => {}
                }
            }
        }
        if another {
            let mut j = start_at;
            while !swapped && j <= n {
                if diag[j].is_none()
                    && let (_, Some(t)) = count_twins(&cols, j)
                {
                    swap(&mut cols, &mut diag, j, t);
                    swapped = true;
                }
                j += 1;
            }
        }
        if !another {
            break;
        }
    }
    diag.into_iter().flatten().collect()
}

pub(crate) struct Engine {
    pub vars: Vars,
    devices: Vec<Box<dyn Device>>,
    /// Element name per device index.
    pub names: Vec<String>,
    mb: MatrixBuilder,
    solver: Box<dyn LinearSolver>,
    values: Vec<f64>,
    /// `CKTrhs`: overwritten by each solve with the new solution.
    rhs: Vec<f64>,
    /// `CKTrhsOld`: the previous solution; the final solution after
    /// a successful solve.
    pub sol: Vec<f64>,
    states: States,
    nodesets: Vec<(Var, f64)>,
    nodeset_diag: Vec<crate::matrix::Slot>,
    /// `.ic` entries, forced during the transient operating point
    /// (`cktload.c` under `MODETRANOP && !UIC`).
    ics: Vec<(Var, f64)>,
    ic_diag: Vec<crate::matrix::Slot>,
    /// V/I source name → (device index, is Vsource), for `.dc`.
    pub sources: HashMap<String, (usize, bool)>,

    // Analysis mode (the C's CKTmode bits + time bookkeeping).
    kind: AnKind,
    uic: bool,
    time: f64,
    step: f64,
    final_time: f64,
    delta: f64,
    delta_old: [f64; 8],
    ag: [f64; 7],
    order: usize,
    maxord: usize,
    /// The method devices integrate with: under TR-BDF2 the current
    /// stage's (trapezoidal, then gear).
    method: Method,
    /// `method=trbdf2` (issue #6): each step is two stages, and the
    /// state banks/`delta_old` advance per stage.
    trbdf2: bool,
    /// Under TR-BDF2, the accepted-step history (`[0]` = the step
    /// being taken) that devices with their own per-accepted-point
    /// history read; `delta_old` then holds the stage history.
    step_old: [f64; 8],
    /// Set while a TR-BDF2 composite step is LTE-checked: `CKTterr`
    /// then runs its composite estimate.
    composite: bool,

    // Breakpoint table (always ≥ 2 entries, sorted).
    breaks: Vec<f64>,
    min_break: f64,
    /// The opt-in LTRA breakpoint hygiene (`Options::ltra_break_rate`):
    /// the merge radius for slope-rule breakpoints, `TMAX/N`, or
    /// `None` (faithful: every request goes through `CKTsetBreak`).
    slope_break_radius: Option<f64>,
    /// A device load fault from the last `load_all` (see
    /// [`LoadCtx::fault`]).
    fault: Option<&'static str>,
    /// The session warm-start channel (empty outside a
    /// [`Session`](crate::Session) with warm starts on).
    warm: crate::session::WarmSlot,
    /// `.pss` with transmission lines: the periodic past the lines
    /// take as history at the first accept of a shooting interval.
    periodic: Option<crate::matrix::PeriodicPast>,
    /// Per device, its window of state offsets (`Built::state_ranges`).
    #[cfg(feature = "parallel")]
    state_ranges: Vec<(usize, usize)>,
    /// The parallel load (`Options::load_threads != 1`), built at the
    /// first load that asks for it.
    #[cfg(feature = "parallel")]
    par: Option<crate::parload::ParLoad>,
    /// The transient device-noise injector (`.options devnoise`;
    /// set by `Circuit::tran` only).
    dev_noise: Option<crate::devnoise::DevNoise>,
    /// The degree of `CKTop` that closed the last cold operating point.
    pub(crate) op_step: Option<OpStep>,
    /// Where `LoadGmin` puts the diagonal gmin: the diagonal of the
    /// matrix *after* Sparse's `spMNA_Preorder` column swaps, fixed at
    /// the first load like the C's `NIDIDPREORDER` ([`gmin_slots`]).
    gmin_slots: Option<Vec<Slot>>,
    /// `CKTdelmin`/`CKTminBreak` as `OPtran` sees them: 0 for `.op` and
    /// `.dc` (the task defaults), `.tran`'s during its operating point.
    op_delmin: f64,
    op_min_break: f64,
}

impl Engine {
    pub fn new(built: Built) -> Self {
        let Built {
            vars,
            mut devices,
            names,
            nodesets,
            ics,
            sources,
            num_states,
            warm,
            #[cfg(feature = "parallel")]
            state_ranges,
            ..
        } = built;
        let n = vars.len() - 1;
        let mut mb = MatrixBuilder::new(n);
        for d in &mut devices {
            d.alloc(&mut mb);
        }
        let nodeset_diag = nodesets.iter().map(|&(v, _)| mb.slot(v, v)).collect();
        let ic_diag = ics.iter().map(|&(v, _)| mb.slot(v, v)).collect();
        let mut solver = crate::linalg::default_solver();
        solver.analyze(&mb.pattern());
        let num_slots = mb.num_slots();
        Engine {
            vars,
            devices,
            names,
            mb,
            solver,
            values: vec![0.0; num_slots],
            rhs: vec![0.0; n + 1],
            sol: vec![0.0; n + 1],
            states: States::new(num_states.max(1)),
            nodesets,
            nodeset_diag,
            ics,
            ic_diag,
            sources,
            kind: AnKind::DcOp,
            uic: false,
            time: 0.0,
            step: 0.0,
            final_time: 0.0,
            delta: 0.0,
            delta_old: [0.0; 8],
            ag: [0.0; 7],
            order: 1,
            maxord: 2,
            method: Method::Trapezoidal,
            trbdf2: false,
            step_old: [0.0; 8],
            composite: false,
            breaks: vec![0.0, 0.0],
            min_break: 0.0,
            slope_break_radius: None,
            fault: None,
            warm,
            periodic: None,
            #[cfg(feature = "parallel")]
            state_ranges,
            #[cfg(feature = "parallel")]
            par: None,
            dev_noise: None,
            op_step: None,
            gmin_slots: None,
            op_delmin: 0.0,
            op_min_break: 0.0,
        }
    }

    /// Run the next `.tran` with transient device noise.
    pub fn set_dev_noise(&mut self, dn: crate::devnoise::DevNoise) {
        self.dev_noise = Some(dn);
    }

    /// The device-noise injector's remarks (empty without one).
    pub fn dev_noise_notes(&self) -> Vec<String> {
        self.dev_noise
            .as_ref()
            .map(|d| d.notes().to_vec())
            .unwrap_or_default()
    }

    pub fn set_kind(&mut self, kind: AnKind, uic: bool) {
        self.kind = kind;
        self.uic = uic;
    }

    /// One load pass for callers outside this module (the pole-zero
    /// analysis runs `MODEINITSMSIG` through it).
    pub fn load_all_public(&mut self, mode: InitMode, opts: &Options) {
        self.load_all(mode, opts, 1.0);
    }

    fn load_all(&mut self, mode: InitMode, opts: &Options, srcfact: f64) -> bool {
        self.values.fill(0.0);
        self.rhs.fill(0.0);
        let pred_xfact = if self.delta_old[1] != 0.0 {
            self.delta / self.delta_old[1]
        } else {
            0.0
        };
        let sol = &self.sol;
        let (kind, uic, time, step, final_time) =
            (self.kind, self.uic, self.time, self.step, self.final_time);
        let (ag, order, method, delta) = (self.ag, self.order, self.method, self.delta);
        let env = LoadEnv {
            sol,
            mode,
            kind,
            uic,
            time,
            step,
            final_time,
            gmin: opts.gmin,
            reltol: opts.reltol,
            abstol: opts.abstol,
            srcfact,
            ag,
            order,
            method,
            pred_xfact,
            delta,
        };

        // The parallel load (issue #9): same stamps, same order of
        // summation — see `parload`.
        #[cfg(feature = "parallel")]
        let par_done = if opts.load_threads != 1 {
            if self.par.as_ref().map(|p| p.requested()) != Some(opts.load_threads) {
                self.par = crate::parload::ParLoad::new(
                    opts.load_threads,
                    &self.devices,
                    &self.state_ranges,
                );
            }
            self.par.as_mut().map(|par| {
                crate::parload::load(
                    par,
                    &mut self.devices,
                    &self.state_ranges,
                    &mut self.states,
                    &mut self.values,
                    &mut self.rhs,
                    &env,
                )
            })
        } else {
            None
        };
        #[cfg(not(feature = "parallel"))]
        let par_done: Option<(bool, Option<&'static str>)> = None;

        let (mut noncon, mut fault) = match par_done {
            Some(done) => done,
            None => {
                let sink = StampSink::Direct {
                    values: &mut self.values,
                    rhs: &mut self.rhs,
                };
                let mut ctx = env.ctx(sink, self.states.view());
                for d in &mut self.devices {
                    d.load(&mut ctx);
                }
                (ctx.noncon, ctx.fault)
            }
        };
        {
            let sink = StampSink::Direct {
                values: &mut self.values,
                rhs: &mut self.rhs,
            };
            let mut ctx = env.ctx(sink, self.states.view());
            ctx.noncon = noncon;
            ctx.fault = fault;
            for d in &mut self.devices {
                d.load_late(&mut ctx);
            }
            noncon = ctx.noncon;
            fault = ctx.fault;
        }
        self.fault = fault;
        if self.kind == AnKind::Tran
            && let Some(dn) = &self.dev_noise
        {
            dn.stamp(self.time, &mut self.rhs);
        }

        // Nodesets hold during INITJCT/INITFIX of DC solutions;
        // `.ic` nodes hold throughout the transient operating point
        // (`cktload.c`; row replacement, see docs/dune-core.md).
        let is_dc = matches!(self.kind, AnKind::DcOp | AnKind::TranOp | AnKind::DcSweep);
        if is_dc {
            if matches!(mode, InitMode::Jct | InitMode::Fix) {
                for (k, &(var, value)) in self.nodesets.iter().enumerate() {
                    for s in self.mb.slots_in_row(var) {
                        self.values[s.0] = 0.0;
                    }
                    self.values[self.nodeset_diag[k].0] = 1.0;
                    self.rhs[var] = value;
                }
            }
            if self.kind == AnKind::TranOp && !self.uic {
                for (k, &(var, value)) in self.ics.iter().enumerate() {
                    for s in self.mb.slots_in_row(var) {
                        self.values[s.0] = 0.0;
                    }
                    self.values[self.ic_diag[k].0] = 1.0;
                    self.rhs[var] = value;
                }
            }
        }
        noncon
    }

    /// One `NIiter` call. `start` is the `INITF` state to begin in.
    /// Returns the iteration count (the dynamic gmin stepping
    /// adapts its factor on it).
    fn ni_iter(
        &mut self,
        opts: &Options,
        start: InitMode,
        maxiter: usize,
        diag_gmin: f64,
        srcfact: f64,
    ) -> Result<usize, Error> {
        let mut mode = start;
        self.ni_iter_mode(opts, &mut mode, maxiter, diag_gmin, srcfact)
    }

    /// [`Engine::ni_iter`] with the C's `CKTmode` INITF bits in
    /// `mode`: on return they are where `NIiter` left them — the
    /// fallback degrees of `CKTop` retry a failed solve in the mode
    /// it stopped in, not from `INITJCT`.
    fn ni_iter_mode(
        &mut self,
        opts: &Options,
        mode: &mut InitMode,
        maxiter: usize,
        diag_gmin: f64,
        srcfact: f64,
    ) -> Result<usize, Error> {
        // The UIC transient operating point: one load, no solve
        // (`niiter.c` top).
        if self.kind == AnKind::TranOp && self.uic {
            std::mem::swap(&mut self.sol, &mut self.rhs);
            self.load_all(*mode, opts, srcfact);
            return Ok(0);
        }

        let mut iterno = 0usize;
        let mut ipass = false;
        let has_nodesets =
            !self.nodesets.is_empty() && matches!(self.kind, AnKind::DcOp | AnKind::DcSweep);

        loop {
            let mut noncon = self.load_all(*mode, opts, srcfact);
            if let Some(f) = self.fault.take() {
                return Err(Error::Build(f.into()));
            }

            // `SMPpreOrder` once, on the first loaded matrix
            // (`niiter.c`: `NIDIDPREORDER`), then `LoadGmin` on the
            // preordered diagonal.
            if self.gmin_slots.is_none() {
                self.gmin_slots = Some(gmin_slots(&self.mb, &self.values));
            }
            if diag_gmin != 0.0 {
                for s in self.gmin_slots.as_deref().unwrap_or_default() {
                    self.values[s.0] += diag_gmin;
                }
            }
            self.solver
                .factor(&self.values[1..], 0.0)
                .map_err(|e| self.map_singular(e))?;
            self.solver
                .solve_in_place(&mut self.rhs[1..])
                .map_err(|e| self.map_singular(e))?;
            // The C zeroes the ground positions right after the
            // solve (`*CKTrhs = 0; *CKTrhsOld = 0;` in niiter.c).
            self.rhs[0] = 0.0;
            self.sol[0] = 0.0;

            iterno += 1;
            if iterno > maxiter {
                return Err(Error::IterLim);
            }
            if !noncon && iterno != 1 {
                noncon = !self.converged(opts);
            } else {
                noncon = true;
            }

            match *mode {
                InitMode::Float => {
                    if has_nodesets {
                        if ipass {
                            noncon = true;
                        }
                        ipass = false;
                    }
                    if !noncon {
                        std::mem::swap(&mut self.sol, &mut self.rhs);
                        return Ok(iterno);
                    }
                }
                InitMode::Jct => *mode = InitMode::Fix,
                InitMode::Fix => {
                    if !noncon {
                        *mode = InitMode::Float;
                    }
                    ipass = true;
                }
                InitMode::Tran | InitMode::Pred | InitMode::Smsig => *mode = InitMode::Float,
            }
            std::mem::swap(&mut self.sol, &mut self.rhs);
        }
    }

    /// `NIconvTest`: the node/branch deltas, then the per-device
    /// current tests (`CKTconvTest`, active under NEWCONV).
    fn converged(&self, opts: &Options) -> bool {
        for i in 1..self.vars.len() {
            let new = self.rhs[i];
            let old = self.sol[i];
            let floor = match self.vars.kinds[i] {
                VarKind::Branch => opts.abstol,
                _ => opts.vntol,
            };
            let tol = opts.reltol * f64::max(new.abs(), old.abs()) + floor;
            if (new - old).abs() > tol {
                return false;
            }
        }
        let ctx = crate::matrix::ConvCtx {
            rhs: &self.rhs,
            sol: &self.sol,
            states: &self.states,
            reltol: opts.reltol,
            abstol: opts.abstol,
            vntol: opts.vntol,
            gmin: opts.gmin,
        };
        self.devices.iter().all(|d| d.conv_test(&ctx))
    }

    fn map_singular(&self, e: LinalgError) -> Error {
        let LinalgError::Singular { row } = e;
        let name = self
            .vars
            .names
            .get(row + 1)
            .cloned()
            .unwrap_or_else(|| "?".into());
        Error::Singular(name)
    }

    /// `CKTop`, behind the session warm start: a standalone
    /// operating point (`MODEDCOP`) of a [`Session`](crate::Session)
    /// with warm starts on first tries one `NIiter` in `INITFLOAT`
    /// from the previous operating point (solution and state bank
    /// 0) — the continuation `DCtrCurv` uses between sweep points. A
    /// miss returns at once with the slot marked, and the session
    /// re-runs the whole analysis cold, so the cold result is the
    /// fresh-run one bit for bit. Every operating point solved here
    /// (warm or cold) refreshes the slot.
    pub fn op(&mut self, opts: &Options) -> Result<(), Error> {
        let warm_ok = self.kind == AnKind::DcOp && self.nodesets.is_empty();
        if warm_ok && let Some((sol, st0)) = self.warm.guess(self.sol.len(), self.states.len0()) {
            self.sol.copy_from_slice(&sol);
            self.states.restore0(&st0);
            return match self.ni_iter(opts, InitMode::Float, opts.itl1, 0.0, 1.0) {
                Ok(_) => {
                    self.warm.hit(&self.sol, &self.states.snapshot0());
                    Ok(())
                }
                Err(e) => {
                    self.warm.miss();
                    Err(e)
                }
            };
        }
        self.op_cold(opts)?;
        if warm_ok {
            self.warm.store(&self.sol, &self.states.snapshot0());
        }
        Ok(())
    }

    /// `CKTop` (ngspice-47 `spicelib/analysis/cktop.c:26-116`): the
    /// operating-point fallback chain, each degree tried only when
    /// every earlier one failed, in the C's order and with its
    /// defaults (docs/dune-core.md §Cadeia do op):
    ///
    /// 1. direct Newton at `itl1` (skipped under `noopiter`);
    /// 2. gmin stepping — `gminsteps == 1` (default): Gillespie's
    ///    dynamic diagonal gmin, then, if it fails, the "true gmin"
    ///    stepping of the devices' own `gmin` (`new_gmin`);
    ///    `gminsteps > 1`: the 3f5 fixed ladder; 0: none;
    /// 3. source stepping — `srcsteps == 1` (default): Gillespie's
    ///    adaptive ramp (`gillespie_src`); `> 1`: the 3f5 fixed
    ///    ladder; 0: none;
    /// 4. the transient operating point (`OPtran`, optran.c), on by
    ///    default in ngspice-47 (`optran 1 1 1 100n 10u 0`, set by
    ///    `cp_init`, frontend/init.c:77-93).
    fn op_cold(&mut self, opts: &Options) -> Result<(), Error> {
        self.op_step = None;
        // `ckt->CKTmode = firstmode`; NIiter moves the INITF bits and
        // a failed call leaves them where it stopped.
        let mut mode = InitMode::Jct;
        let direct = if !opts.noopiter {
            self.ni_iter_mode(opts, &mut mode, opts.itl1, 0.0, 1.0)
                .map(|_| ())
        } else {
            Err(Error::IterLim)
        };
        if direct.is_ok() {
            self.op_step = Some(OpStep::Direct);
            return Ok(());
        }
        let mut last = direct;

        if opts.gminsteps == 1 {
            last = self.dynamic_gmin(opts);
            if last.is_ok() {
                self.op_step = Some(OpStep::DynamicGmin);
                return Ok(());
            }
            // Without the frontend variable `dyngmin` (unset by
            // default) the C follows with `new_gmin` (cktop.c:63-69).
            last = self.true_gmin(opts);
            if last.is_ok() {
                self.op_step = Some(OpStep::TrueGmin);
                return Ok(());
            }
        }

        if opts.gminsteps > 1 {
            let first = InitMode::Jct;
            let mut diag_gmin = opts.gmin;
            for _ in 0..opts.gminsteps {
                diag_gmin *= 10.0;
            }
            let mut mode = first;
            for _ in 0..=opts.gminsteps {
                if self.ni_iter(opts, mode, opts.itl1, diag_gmin, 1.0).is_err() {
                    break;
                }
                diag_gmin /= 10.0;
                mode = InitMode::Float;
            }
            last = self.ni_iter(opts, mode, opts.itl1, 0.0, 1.0).map(|_| ());
            if last.is_ok() {
                self.op_step = Some(OpStep::Spice3Gmin);
                return Ok(());
            }
        }

        if opts.srcsteps == 1 {
            last = self.gillespie_src(opts);
            if last.is_ok() {
                self.op_step = Some(OpStep::GillespieSrc);
                return Ok(());
            }
        } else if opts.srcsteps > 1 {
            let mut mode = InitMode::Jct;
            let mut ladder = Ok(());
            for i in 0..=opts.srcsteps {
                let srcfact = i as f64 / opts.srcsteps as f64;
                let r = self.ni_iter(opts, mode, opts.itl1, 0.0, srcfact);
                mode = InitMode::Float;
                if let Err(e) = r {
                    ladder = Err(e); // "source stepping failed"
                    break;
                }
            }
            if ladder.is_ok() {
                self.op_step = Some(OpStep::Spice3Src);
                return Ok(()); // "Source stepping completed"
            }
            last = ladder;
        }

        // cktop.c:94-103. A failed transient op reports its own cause
        // in the C; here the earlier degree's error stands.
        if self.op_tran(opts).is_ok() {
            self.op_step = Some(OpStep::OpTran);
            return Ok(());
        }
        last
    }

    /// ngspice's dynamic gmin stepping (`cktop.c:161-274`, Alan
    /// Gillespie): from a zeroed solution and state, walk the diagonal
    /// gmin down from 1e-3 to `max(gmin, gshunt)` ([`GminWalk`]),
    /// rolling back to the last good step on failure. Whether the walk
    /// reaches the target or gives up, the C ends with a full Newton at
    /// `CKTdiagGmin = CKTgshunt` (0) from wherever it stopped. The 3f5
    /// has only the fixed ladder; several classic decks (schmitt,
    /// mosamp2) need this once the NEWCONV device tests are enforced
    /// (`corpus/KNOWN_DIFFS.md`).
    fn dynamic_gmin(&mut self, opts: &Options) -> Result<(), Error> {
        let mut walk = GminWalk::new(f64::max(opts.gmin, GSHUNT), 1.00005);
        let mut mode = self.gmin_walk(opts, &mut walk, false);
        self.ni_iter_mode(opts, &mut mode, ng_iter(opts.itl1), GSHUNT, 1.0)
            .map(|_| ())
    }

    /// ngspice's "true gmin stepping" (`new_gmin`, `cktop.c:349-466`):
    /// the same walk on the devices' own `CKTgmin` (the conductance
    /// every junction loads in parallel) with the diagonal gmin at
    /// `CKTgshunt`; a slow step floors the factor at 3 instead of
    /// 1.00005. `CKTgmin` is restored to `max(gmin, gshunt)` for the
    /// final Newton.
    fn true_gmin(&mut self, opts: &Options) -> Result<(), Error> {
        let mut walk = GminWalk::new(f64::max(opts.gmin, GSHUNT), 3.0);
        let mut mode = self.gmin_walk(opts, &mut walk, true);
        self.ni_iter_mode(opts, &mut mode, ng_iter(opts.itl1), GSHUNT, 1.0)
            .map(|_| ())
    }

    /// The loop shared by [`Engine::dynamic_gmin`] and
    /// [`Engine::true_gmin`]: each step's gmin goes to `LoadGmin`
    /// (diagonal walk) or replaces the devices' `gmin` with the
    /// diagonal at `CKTgshunt` (`device`, the true walk). Returns the
    /// INITF mode the walk ended in.
    fn gmin_walk(&mut self, opts: &Options, walk: &mut GminWalk, device: bool) -> InitMode {
        let mut o = opts.clone();
        let mut mode = InitMode::Jct; // `CKTmode = firstmode`
        self.sol.fill(0.0);
        self.states.zero0();
        let maxit = ng_iter(opts.itl2);
        let mut saved_sol: Vec<f64> = vec![0.0; self.sol.len()];
        let mut saved_states = self.states.snapshot0();
        loop {
            let diag = if device {
                o.gmin = walk.gmin;
                GSHUNT
            } else {
                walk.gmin
            };
            match self.ni_iter_mode(&o, &mut mode, maxit, diag, 1.0) {
                Ok(iters) => {
                    mode = InitMode::Float; // `CKTmode = continuemode`
                    if walk.converged(iters, opts.itl2) {
                        break; // reached the target
                    }
                    saved_sol.copy_from_slice(&self.sol);
                    saved_states = self.states.snapshot0();
                }
                // Any failure (the C only looks at the return code):
                // back off and roll back, in the mode NIiter left.
                Err(_) => {
                    if walk.failed() {
                        break; // "Last gmin step failed"
                    }
                    self.sol.copy_from_slice(&saved_sol);
                    self.states.restore0(&saved_states);
                }
            }
        }
        mode
    }

    /// Gillespie's source stepping (`gillespie_src`,
    /// `cktop.c:480-658`): solve with every source at zero — with a
    /// fixed 10-step diagonal-gmin ladder from `gmin·1e10` if the
    /// plain solve fails —, then raise `srcFact` from 0 by the
    /// adaptive increment of [`SrcRamp`]. Succeeds when a solve at
    /// `srcFact = 1` converges (no extra Newton after it).
    fn gillespie_src(&mut self, opts: &Options) -> Result<(), Error> {
        let mut mode = InitMode::Jct;
        let maxit = ng_iter(opts.itl2);
        self.sol.fill(0.0);
        self.states.zero0();
        // "First, try a straight solution with all sources at zero".
        let mut r = self.ni_iter_mode(opts, &mut mode, maxit, GSHUNT, 0.0);
        if r.is_err() {
            // "try gmin stepping as well for the first solution"
            // (`gshunt <= 0` starts from `gmin`).
            let mut diag_gmin = opts.gmin;
            for _ in 0..10 {
                diag_gmin *= 10.0;
            }
            for _ in 0..=10 {
                r = self.ni_iter_mode(opts, &mut mode, maxit, diag_gmin, 0.0);
                if r.is_err() {
                    break; // "gmin step failed"
                }
                diag_gmin /= 10.0;
                mode = InitMode::Float;
            }
        }
        if let Err(e) = r {
            return Err(e); // "source stepping failed"
        }
        let mut last = Error::IterLim;
        let mut ramp = SrcRamp::new();
        let mut saved_sol = self.sol.clone();
        let mut saved_states = self.states.snapshot0();
        while !ramp.finished() {
            let r = self.ni_iter_mode(opts, &mut mode, maxit, GSHUNT, ramp.src_fact);
            mode = InitMode::Float;
            match r {
                Ok(iters) => {
                    ramp.converged(iters, opts.itl2);
                    saved_sol.copy_from_slice(&self.sol);
                    saved_states = self.states.snapshot0();
                }
                Err(e) => {
                    last = e;
                    if ramp.failed() {
                        break;
                    }
                    self.sol.copy_from_slice(&saved_sol);
                    self.states.restore0(&saved_states);
                }
            }
        }
        if ramp.conv_fact != 1.0 {
            Err(last) // "source stepping failed"
        } else {
            Ok(()) // "Source stepping completed"
        }
    }

    /// `OPtran` (ngspice-47 `spicelib/analysis/optran.c:290-852`, the
    /// XSPICE build the oracle runs): when every other degree has
    /// failed, march a private transient of the circuit from the
    /// state the last attempt left, `0 → 10 µs` with `TSTEP = TMAX =
    /// 100 ns` and trapezoidal/LTE control as `DCtran`, and take the
    /// final point as the operating point. Faithful to the C in what
    /// matters for the result:
    ///
    /// - the circuit time `CKTtime` is never advanced (only the local
    ///   `optime`), so sources hold their transient value at t = 0
    ///   and plant no breakpoints; the only breakpoints are 0 and the
    ///   final time (`opbreaks`);
    /// - first step `min(10 µs/100, 100 ns)/10`, cut ÷10 again at the
    ///   start; a failed Newton (any cause) divides the step by 8;
    ///   `NIiter` gets its ngspice floor of 100 iterations;
    /// - `delmin`/`minBreak` are the ones the calling analysis has
    ///   (0 for `.op`/`.dc`; `.tran`'s for its operating point), and
    ///   no `MAX(delta, 2·delmin)` (XSPICE branch).
    ///
    /// Device accept hooks are not called (they only plant source
    /// breakpoints in `CKTbreaks`, which `OPtran` ignores, or record
    /// line history); circuits with transmission lines skip this
    /// degree (their history would be built on a frozen `CKTtime`).
    fn op_tran(&mut self, opts: &Options) -> Result<(), Error> {
        if self.names.iter().any(|n| {
            let c = n.as_bytes().first().map(u8::to_ascii_lowercase);
            matches!(c, Some(b'o') | Some(b't'))
        }) {
            return Err(Error::IterLim);
        }
        let saved_kind = self.kind;
        let saved_step = self.step;
        let saved_trbdf2 = self.trbdf2;
        let saved_method = self.method;
        let saved_maxord = self.maxord;
        let dev_noise = self.dev_noise.take();
        self.set_tran_method(opts);
        self.trbdf2 = false;
        let r = self.op_tran_loop(opts);
        self.kind = saved_kind;
        self.step = saved_step;
        self.trbdf2 = saved_trbdf2;
        self.method = saved_method;
        self.maxord = saved_maxord;
        self.dev_noise = dev_noise;
        self.ag = [0.0; 7];
        r
    }

    fn op_tran_loop(&mut self, opts: &Options) -> Result<(), Error> {
        let final_time = OPTRAN_FINAL;
        let max_step = OPTRAN_STEP;
        let (delmin, min_break) = (self.op_delmin, self.op_min_break);
        self.step = OPTRAN_STEP;
        let mut delta = f64::min(final_time / 100.0, self.step) / 10.0;
        let mut breaks = vec![0.0, final_time];
        let clr_break = |b: &mut Vec<f64>| {
            if b.len() > 2 {
                b.remove(0);
            } else {
                b[0] = b[1];
                b[1] = final_time;
            }
        };
        let mut firsttime = true;
        self.order = 1;
        self.delta_old = [max_step; 8];
        self.delta = delta;
        let mut save_delta = final_time / 50.0;
        self.set_kind(AnKind::Tran, false);
        self.ag = [0.0; 7];
        self.states.copy01();
        let mut optime = 0.0f64;

        'next_time: loop {
            if optime > breaks[0] {
                clr_break(&mut breaks);
            }
            if almost_equal_ulps(optime, final_time, 100) {
                return Ok(()); // "Transient op finished successfully"
            }

            // resume:
            delta = f64::min(delta, max_step);
            if almost_equal_ulps(optime, breaks[0], 100) || breaks[0] - optime <= delmin {
                self.order = 1;
                delta = f64::min(delta, 0.1 * f64::min(save_delta, breaks[1] - breaks[0]));
                if firsttime {
                    delta /= 10.0;
                }
            }
            // XSPICE: throw out breakpoints at or before now, then
            // force the next one.
            while almost_equal_ulps(breaks[0], optime, 100) || breaks[0] <= optime + min_break {
                clr_break(&mut breaks);
                if breaks[0] == breaks[1] && breaks[0] <= optime + min_break {
                    // Both entries spent (only possible at the end).
                    break;
                }
            }
            if optime + delta > breaks[0] {
                save_delta = delta;
                delta = breaks[0] - optime;
            }

            for i in (0..=5).rev() {
                self.delta_old[i + 1] = self.delta_old[i];
            }
            self.delta_old[0] = delta;
            self.states.rotate(self.maxord);

            loop {
                let olddelta = delta;
                optime += delta;
                self.delta_old[0] = delta;
                self.delta = delta;
                self.ag = com_cof(self.method, self.order, delta, &self.delta_old)?;
                let start = if firsttime {
                    InitMode::Tran
                } else {
                    InitMode::Pred
                };
                let converged = self.ni_iter(opts, start, ng_iter(opts.itl4), 0.0, 1.0);
                if firsttime {
                    self.states.copy1_to_23();
                }
                match converged {
                    Err(_) => {
                        optime -= delta;
                        delta /= 8.0;
                        self.order = 1;
                    }
                    Ok(_) => {
                        if firsttime {
                            firsttime = false;
                            continue 'next_time; // no LTE check on the first point
                        }
                        let mut new = delta;
                        self.trunc(opts, &mut new);
                        if new > 0.9 * delta {
                            if self.order == 1 && self.maxord > 1 {
                                new = delta;
                                self.order = 2;
                                self.trunc(opts, &mut new);
                                if new <= 1.05 * delta {
                                    self.order = 1;
                                }
                            }
                            delta = new;
                            continue 'next_time;
                        }
                        optime -= delta;
                        delta = new;
                    }
                }
                if delta <= delmin {
                    if olddelta > delmin {
                        delta = delmin;
                    } else {
                        return Err(Error::Timestep { time: optime }); // "Timestep too small"
                    }
                }
            }
        }
    }

    /// `DCtrCurv`: sweep one or two variables, the first innermost.
    /// A [`SweepTarget::Temp`] level calls `retemp` with the new
    /// temperature (°C) for a freshly built device list — the
    /// `CKTtemp` of ngspice's `TEMP_CODE` sweep.
    pub fn dc(
        &mut self,
        opts: &Options,
        sweeps: &[SweepRef],
        retemp: &mut dyn FnMut(f64) -> Result<Vec<Box<dyn Device>>, Error>,
    ) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        assert!(!sweeps.is_empty() && sweeps.len() <= 2);
        // Refuse before the first solve when the planned grid alone
        // blows the output cap (each point is a full Newton solve).
        let planned: f64 = sweeps
            .iter()
            .map(|s| ((s.stop - s.start) / s.step).abs() + 1.0)
            .product::<f64>()
            * self.vars.len() as f64;
        if !(planned.is_finite() && planned <= MAX_OUTPUT_VALUES as f64) {
            return Err(Error::Build(
                "dc sweep produces too many points; refusing".into(),
            ));
        }
        self.set_kind(AnKind::DcSweep, false);
        let sign = |x: f64| if x >= 0.0 { 1.0 } else { -1.0 };

        let mut cur: Vec<f64> = sweeps.iter().map(|s| s.start).collect();
        for level in 0..sweeps.len() {
            self.set_sweep(sweeps, &cur, level, false, retemp)?;
        }
        let mut rows: Vec<(f64, Vec<f64>)> = Vec::new();
        let mut start_mode = InitMode::Jct;
        let mut i = 0usize;
        loop {
            let s = &sweeps[i];
            // ngspice's guard (its dctrcurv.c): the accumulated
            // value may drift past the stop by rounding; the raw 3f5
            // comparison would then drop the final point
            // (corpus/KNOWN_DIFFS.md).
            if cur[i] * sign(s.step) - sign(s.step) * s.stop > f64::EPSILON * 1e3 {
                i += 1;
                start_mode = InitMode::Jct;
                if i >= sweeps.len() {
                    break;
                }
                cur[i] += sweeps[i].step;
                self.set_sweep(sweeps, &cur, i, true, retemp)?;
                continue;
            }
            while i > 0 {
                i -= 1;
                cur[i] = sweeps[i].start;
                self.set_sweep(sweeps, &cur, i, false, retemp)?;
            }
            if self.ni_iter(opts, start_mode, opts.itl2, 0.0, 1.0).is_err() {
                self.op(opts)?;
            }
            start_mode = InitMode::Float;
            // `DCtrCurv` rotates the state banks before every point,
            // so state1 there is the previous point's state0. We keep
            // state0 in place instead (the loads start in INITFLOAT
            // from it), but the switches read the previous point's
            // state from state1 (ngspice `cswload.c`), so mirror it.
            // No other device reads state1 in a DC mode.
            self.states.copy01();
            if (rows.len() + 1).saturating_mul(self.vars.len()) > MAX_OUTPUT_VALUES {
                return Err(Error::Build(
                    "dc sweep produces too many points; refusing".into(),
                ));
            }
            rows.push((cur[0], self.sol.clone()));
            cur[0] += sweeps[0].step;
            self.set_sweep(sweeps, &cur, 0, true, retemp)?;
        }
        Ok(rows)
    }

    /// Drive sweep level `level` to `cur[level]`. A temperature
    /// level swaps in devices rebuilt at the new temperature and
    /// re-applies the device levels' current values to them; on an
    /// increment (`stepped`) that already passed the stop it does
    /// nothing, like ngspice's `nextstep` ("discard Temp
    /// evaluation": the value is never solved at).
    fn set_sweep(
        &mut self,
        sweeps: &[SweepRef],
        cur: &[f64],
        level: usize,
        stepped: bool,
        retemp: &mut dyn FnMut(f64) -> Result<Vec<Box<dyn Device>>, Error>,
    ) -> Result<(), Error> {
        let s = &sweeps[level];
        match s.target {
            SweepTarget::Device(d) => self.devices[d].set_dc(cur[level]),
            SweepTarget::Temp => {
                let sign = if s.step >= 0.0 { 1.0 } else { -1.0 };
                if stepped && sign * (cur[level] - s.stop) > f64::EPSILON * 1e3 {
                    return Ok(());
                }
                let devices = retemp(cur[level])?;
                self.replace_devices(devices)?;
                for (s, v) in sweeps.iter().zip(cur) {
                    if let SweepTarget::Device(d) = s.target {
                        self.devices[d].set_dc(*v);
                    }
                }
            }
        }
        Ok(())
    }

    /// Swap in a device list built from the same circuit (same
    /// order, nodes and state offsets — only temperature-derived
    /// values differ): the `CKTtemp` pass. Matrix slots are
    /// re-derived on a fresh builder, which yields the identical
    /// numbering because allocation is deterministic.
    pub(crate) fn replace_devices(
        &mut self,
        mut devices: Vec<Box<dyn Device>>,
    ) -> Result<(), Error> {
        let mut mb = MatrixBuilder::new(self.vars.len() - 1);
        for d in &mut devices {
            d.alloc(&mut mb);
        }
        if devices.len() != self.devices.len() || mb.num_slots() > self.mb.num_slots() {
            return Err(Error::Build(
                "internal: rebuilt device list does not match the circuit".into(),
            ));
        }
        self.devices = devices;
        #[cfg(feature = "parallel")]
        {
            self.par = None;
        }
        Ok(())
    }

    /// `CKTsetBreak` (`cktsetbk.c`): sorted insert with `minBreak`
    /// merging.
    fn set_break(&mut self, time: f64) -> Result<(), Error> {
        if self.time > time {
            // "breakpoint in the past - HELP!" is E_INTERN in the C.
            return Err(Error::Build("breakpoint in the past".into()));
        }
        for i in 0..self.breaks.len() {
            if self.breaks[i] > time {
                if self.breaks[i] - time <= self.min_break {
                    // "very close together - take earlier point"
                    self.breaks[i] = time;
                    return Ok(());
                }
                if i > 0 && time - self.breaks[i - 1] <= self.min_break {
                    return Ok(());
                }
                self.breaks.insert(i, time);
                return Ok(());
            }
        }
        if time - self.breaks[self.breaks.len() - 1] <= self.min_break {
            return Ok(());
        }
        self.breaks.push(time);
        Ok(())
    }

    /// `CKTclrBreak`.
    fn clr_break(&mut self) {
        if self.breaks.len() > 2 {
            self.breaks.remove(0);
        } else {
            self.breaks[0] = self.breaks[1];
            self.breaks[1] = self.final_time;
        }
    }

    /// `CKTtrunc`: every device's LTE estimate; growth capped at 2×.
    fn trunc(&self, opts: &Options, time_step: &mut f64) {
        let mut timetemp = f64::MAX;
        let t = TerrCtx {
            states: &self.states,
            sol: &self.sol,
            time: self.time,
            delta_old: &self.delta_old,
            step_old: self.accepted_steps(),
            order: self.order,
            method: if self.composite {
                Method::TrBdf2
            } else {
                self.method
            },
            delta: if self.composite {
                self.step_old[0]
            } else {
                self.delta
            },
            reltol: opts.reltol,
            abstol: opts.abstol,
            chgtol: opts.chgtol,
            trtol: opts.trtol,
        };
        for d in &self.devices {
            d.trunc(&t, &mut timetemp);
        }
        *time_step = f64::min(2.0 * *time_step, timetemp);
    }

    /// The devices' remarks about the transient run just finished.
    pub fn tran_notes(&self) -> Vec<String> {
        self.devices.iter().flat_map(|d| d.tran_notes()).collect()
    }

    /// The accepted-step history devices see (`CKTdeltaOld` outside
    /// TR-BDF2).
    fn accepted_steps(&self) -> &[f64; 8] {
        if self.trbdf2 {
            &self.step_old
        } else {
            &self.delta_old
        }
    }

    /// `CKTaccept`: device hooks, then merge breakpoint requests.
    fn accept(&mut self, brk: bool, first: bool, opts: &Options) -> Result<(), Error> {
        let mut requests = Vec::new();
        let delta_old = if self.trbdf2 {
            &self.step_old
        } else {
            &self.delta_old
        };
        let mut ctx = AcceptCtx {
            time: self.time,
            first,
            sol: &self.sol,
            reltol: opts.reltol,
            abstol: opts.abstol,
            brk,
            step: self.step,
            final_time: self.final_time,
            delta_old,
            min_break: self.min_break,
            requests: &mut requests,
            periodic: if first { self.periodic.as_ref() } else { None },
        };
        for d in &mut self.devices {
            d.accept(&mut ctx);
        }
        if let Some(dn) = self.dev_noise.as_mut() {
            dn.accept(
                self.time,
                self.min_break,
                &self.devices,
                &self.names,
                &self.states,
                &mut requests,
            )?;
        }
        for (t, slope) in requests {
            if slope && self.near_break(t) {
                continue;
            }
            self.set_break(t)?;
        }
        Ok(())
    }

    /// The LTRA breakpoint hygiene (issue #6): whether a slope-rule
    /// request at `t` falls within the hygiene radius of a breakpoint
    /// already in the table, and is dropped. `CKTsetBreak` merges only
    /// within `CKTminBreak = 5e-5·TMAX` — a guard against coincident
    /// times, not a density limit: it lets 20 000 breakpoints into one
    /// TMAX span, which is what an LTRA echo cascade fills (D-0022).
    /// The radius `TMAX/N` caps slope breakpoints at about `N` per TMAX
    /// span. Always false with the hygiene off.
    fn near_break(&self, t: f64) -> bool {
        self.slope_break_radius
            .is_some_and(|r| within_radius(&self.breaks, t, r))
    }

    /// `DCtran`: the transient loop. Returns `(time, solution)` rows
    /// for every accepted timepoint at or after `tstart`.
    pub fn tran(&mut self, opts: &Options, p: &TranParams) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        // Default TMAX: the 3f5 used only (tstop-tstart)/50; ngspice
        // restored the SPICE2 default min(tstep, span/50), which is
        // what bounds its point density — the oracle's grid
        // (corpus/KNOWN_DIFFS.md).
        let mut max_step = match p.tmax {
            Some(m) if m > 0.0 => m,
            _ => f64::min(p.tstep, (p.tstop - p.tstart) / 50.0),
        };
        // Dune's refinement knob (issue #16; validated by
        // `Circuit::tran`): the run of the deck with TMAX/d on the card.
        if let Some(d) = opts.tmax_divisor {
            max_step /= d;
        }
        self.set_tran_method(opts);
        let planned = ((p.tstop - p.tstart) / p.tstep + 2.0) * self.vars.len() as f64;
        if !(planned.is_finite() && planned <= MAX_OUTPUT_VALUES as f64) {
            return Err(Error::Build(
                "transient produces too many points; refusing".into(),
            ));
        }
        self.step = p.tstep;
        self.final_time = p.tstop;
        self.time = 0.0;
        self.breaks = vec![0.0, p.tstop];
        self.min_break = 5e-5 * max_step;
        self.slope_break_radius = match opts.ltra_break_rate {
            Some(n) if n > 0 => Some(f64::max(max_step / n as f64, self.min_break)),
            _ => None,
        };
        let delmin = 1e-9 * max_step;
        let delta = f64::min(p.tstop / 50.0, p.tstep) / 10.0;

        // Transient operating point.
        self.set_kind(AnKind::TranOp, p.uic);
        if p.uic {
            self.ckt_ic();
            self.ni_iter(opts, InitMode::Jct, opts.itl1, 0.0, 1.0)?;
        } else {
            // `traninit.c`: `CKTdelmin = 1e-11·CKTmaxStep`; the XSPICE
            // `dctran.c:163` sets `CKTminBreak = 10·CKTdelmin` before
            // `CKTop` — what `OPtran` sees.
            self.op_delmin = 1e-11 * max_step;
            self.op_min_break = 10.0 * self.op_delmin;
            let r = self.op(opts);
            self.op_delmin = 0.0;
            self.op_min_break = 0.0;
            r?;
        }

        self.tran_loop(
            opts,
            &TranLoop {
                tstart: p.tstart,
                uic: p.uic,
                max_step,
                delmin,
                delta,
                save_delta: p.tstop / 50.0,
                step_budget: opts.tran_step_budget(p.tstop, max_step),
            },
            None,
        )
    }

    /// `CKTic` (`cktic.c`) under UIC: the right-hand side holds the
    /// `.nodeset` values, then the `.ic` ones (which win), zero
    /// elsewhere — the UIC operating point swaps it into the solution
    /// without solving — and every device takes the initial
    /// conditions it was not given from it (`DEVsetic`).
    fn ckt_ic(&mut self) {
        self.rhs.iter_mut().for_each(|v| *v = 0.0);
        for &(var, v) in &self.nodesets {
            self.rhs[var] = v;
        }
        for &(var, v) in &self.ics {
            self.rhs[var] = v;
        }
        for d in &mut self.devices {
            d.set_ic(&self.rhs);
        }
    }

    /// The integration method fields of `DCtran`'s setup.
    fn set_tran_method(&mut self, opts: &Options) {
        self.trbdf2 = opts.method == Method::TrBdf2;
        self.method = if self.trbdf2 {
            Method::Trapezoidal
        } else {
            opts.method
        };
        self.maxord = match opts.method {
            Method::Trapezoidal | Method::TrBdf2 => 2,
            Method::Gear => opts.maxord.clamp(1, 6),
        };
    }

    /// `DCtran` after its operating point: the timestep loop, from
    /// the engine's current time and solution to `final_time`. With
    /// `log`, every accepted step is recorded (end time, the order it
    /// was solved at, the `CKTbreak` flag its accept saw) so that
    /// [`Engine::tran_replay`] can retrace the same grid.
    fn tran_loop(
        &mut self,
        opts: &Options,
        lp: &TranLoop,
        mut log: Option<&mut Vec<StepRec>>,
    ) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        let max_step = lp.max_step;
        let delmin = lp.delmin;
        let mut delta = lp.delta;
        // Switch to transient proper (`MODETRAN | MODEINITTRAN`).
        self.set_kind(AnKind::Tran, lp.uic);
        self.order = 1;
        self.ag = [0.0; 7];
        self.states.copy01();
        self.delta_old = [max_step; 8];
        self.step_old = [max_step; 8];
        self.delta = delta;
        let mut save_delta = lp.save_delta;
        let mut firsttime = true;
        let mut brk = true; // CKTbreak
        let mut rows: Vec<(f64, Vec<f64>)> = Vec::new();
        let mut steps: usize = 0;
        let step_budget = lp.step_budget;
        // The order the point just computed was solved at (0 before
        // the first step).
        let mut solved_order = 0usize;

        'next_time: loop {
            if let Some(log) = log.as_deref_mut()
                && solved_order > 0
            {
                log.push(StepRec {
                    time: self.time,
                    order: solved_order,
                    brk,
                });
            }
            // Accept the computed point: device hooks plant
            // breakpoints, stale ones retire.
            self.accept(brk, firsttime, opts)?;
            if self.time > self.breaks[0] {
                self.clr_break();
            }
            brk = false;
            // Under UIC ngspice omits the t=0 point (the 3f5 dumped
            // the untouched zero vector there — KNOWN_DIFFS.md).
            let skip = lp.uic && self.time == 0.0;
            if self.time >= lp.tstart && !skip {
                if (rows.len() + 1).saturating_mul(self.vars.len()) > MAX_OUTPUT_VALUES {
                    return Err(Error::Build(
                        "transient produces too many points; refusing".into(),
                    ));
                }
                rows.push((self.time, self.sol.clone()));
            }
            if (self.time - self.final_time).abs() < self.min_break {
                return Ok(rows);
            }

            // "resume": pick the next delta.
            delta = f64::min(delta, max_step);
            if self.time == self.breaks[0] || self.breaks[0] - self.time <= delmin {
                // First timepoint after a breakpoint: cut the order
                // and limit the step by the breakpoint spacing.
                self.order = 1;
                delta = f64::min(
                    delta,
                    0.1 * f64::min(save_delta, self.breaks[1] - self.breaks[0]),
                );
                if firsttime {
                    // ngspice (not the 3f5): "set a breakpoint to
                    // reduce ringing of current in devices" under UIC.
                    if lp.uic {
                        self.set_break(self.step)?;
                    }
                    delta /= 10.0;
                }
                delta = f64::max(delta, 2.0 * delmin);
            } else if self.time + delta > self.breaks[0] {
                save_delta = delta;
                delta = self.breaks[0] - self.time;
                brk = true;
            } else if self.breaks[0] - self.time < 1.9 * delta {
                // ngspice (not the 3f5): when the step would stop
                // short of the next breakpoint by less than 0.9 of
                // itself, it splits the approach into two equal
                // steps instead of leaving a sliver. Measured on
                // ngspice-47 (boundary between 1.899 and 1.901 step
                // lengths); without it every pulse edge and LTRA
                // delay breakpoint is approached on a different grid
                // (corpus/KNOWN_DIFFS.md).
                delta = 0.5 * (self.breaks[0] - self.time);
            }

            for i in (0..=5).rev() {
                self.delta_old[i + 1] = self.delta_old[i];
            }
            self.delta_old[0] = delta;
            self.states.rotate(self.maxord);
            if self.trbdf2 {
                for i in (0..=5).rev() {
                    self.step_old[i + 1] = self.step_old[i];
                }
                self.step_old[0] = delta;
                let restart = self.tran_trbdf2_point(
                    opts,
                    &mut delta,
                    &mut firsttime,
                    delmin,
                    &mut steps,
                    step_budget,
                    false,
                )?;
                solved_order = if restart { 1 } else { 2 };
                continue 'next_time;
            }

            loop {
                // Dune's step budget (D-0022): every attempted
                // timepoint counts, accepted or not. Neither C
                // enforces ITL5, so a breakpoint cascade (ltra_3 on
                // x86_64: the LTRA slope test re-planting `t+T` each
                // step) would otherwise run for hours.
                steps += 1;
                if steps > step_budget {
                    return Err(Error::StepLimit {
                        steps: step_budget,
                        time: self.time,
                    });
                }
                let olddelta = delta;
                self.time += delta;
                self.delta_old[0] = delta;
                self.delta = delta;
                self.ag = com_cof(self.method, self.order, delta, &self.delta_old)?;

                let start = if firsttime {
                    InitMode::Tran
                } else {
                    InitMode::Pred
                };
                let converged = self.ni_iter(opts, start, opts.itl4, 0.0, 1.0);

                if let Err(e) = converged {
                    if matches!(e, Error::Singular(_)) {
                        return Err(e);
                    }
                    // Rejected for non-convergence.
                    self.time -= delta;
                    delta /= 8.0;
                    self.order = 1;
                } else {
                    solved_order = self.order;
                    if firsttime {
                        firsttime = false;
                        continue 'next_time; // no LTE check on the first point
                    }
                    let mut new = delta;
                    self.trunc(opts, &mut new);
                    if new > 0.9 * delta {
                        if self.order == 1 {
                            new = delta;
                            self.order = 2;
                            self.trunc(opts, &mut new);
                            if new <= 1.05 * delta {
                                self.order = 1;
                            }
                        }
                        delta = new;
                        continue 'next_time;
                    } else {
                        // Rejected for truncation error.
                        self.time -= delta;
                        delta = new;
                    }
                }
                if delta <= delmin {
                    if olddelta > delmin {
                        delta = delmin;
                    } else {
                        return Err(Error::Timestep { time: self.time });
                    }
                }
            }
        }
    }
}

impl Engine {
    /// The periodic past the transmission lines read as history in
    /// the next [`Engine::tran_period`]/[`Engine::tran_replay`].
    pub(crate) fn set_periodic_past(&mut self, past: Option<crate::matrix::PeriodicPast>) {
        self.periodic = past;
    }

    /// The device states the transient operating point would leave
    /// had it converged on `x0`: load passes in `MODETRANOP |
    /// MODEINITFLOAT` over the fixed solution `x0` (no solve), repeated
    /// until no device limits its junction voltages any more (the
    /// limiting walks state 0 onto `x0`'s voltages in a few passes).
    /// Charges, junction voltages and every other state-0 quantity
    /// are then functions of `x0` alone — the shooting state.
    fn seed_states(&mut self, opts: &Options, x0: &[f64]) -> Result<(), Error> {
        self.set_kind(AnKind::TranOp, false);
        self.sol.copy_from_slice(x0);
        self.sol[0] = 0.0;
        for _ in 0..SEED_PASSES {
            let noncon = self.load_all(InitMode::Float, opts, 1.0);
            if let Some(f) = self.fault.take() {
                return Err(Error::Build(f.into()));
            }
            if !noncon {
                return Ok(());
            }
        }
        Err(Error::Build(
            "pss: the device states do not settle on the shooting state".into(),
        ))
    }

    /// The setup [`Engine::tran_period`] and [`Engine::tran_replay`]
    /// share: `DCtran`'s fields for an interval `[t0, t0 + period]`,
    /// the breakpoint table seeded with the source corners inside it,
    /// and the device states seeded from `x0`.
    fn period_setup(&mut self, opts: &Options, run: &PeriodRun, x0: &[f64]) -> Result<(), Error> {
        self.set_tran_method(opts);
        let t1 = run.t0 + run.period;
        self.step = run.tstep;
        self.final_time = t1;
        self.time = run.t0;
        self.breaks = vec![run.t0, t1];
        self.min_break = 5e-5 * run.max_step;
        self.slope_break_radius = match opts.ltra_break_rate {
            Some(n) if n > 0 => Some(f64::max(run.max_step / n as f64, self.min_break)),
            _ => None,
        };
        let mut corners = Vec::new();
        for d in &self.devices {
            d.wave_corners(run.t0, t1, run.tstep, t1, &mut corners);
        }
        corners.sort_by(f64::total_cmp);
        for c in corners {
            if c - run.t0 > self.min_break && t1 - c > self.min_break {
                self.set_break(c)?;
            }
        }
        self.seed_states(opts, x0)
    }

    /// `DCtran` over one shooting interval, from the state `x0` at
    /// `run.t0` instead of an operating point: the full adaptive loop
    /// (LTE control, breakpoints, TR-BDF2, the step budget), every
    /// accepted step logged into `log`. Returns the rows from `t0` to
    /// `t0 + period`.
    pub fn tran_period(
        &mut self,
        opts: &Options,
        run: &PeriodRun,
        x0: &[f64],
        log: &mut Vec<StepRec>,
    ) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        self.period_setup(opts, run, x0)?;
        let max_step = run.max_step;
        self.tran_loop(
            opts,
            &TranLoop {
                tstart: run.t0,
                uic: false,
                max_step,
                delmin: 1e-9 * max_step,
                delta: f64::min(run.period / 50.0, run.tstep) / 10.0,
                save_delta: run.period / 50.0,
                step_budget: opts.tran_step_budget(run.period, max_step),
            },
            Some(log),
        )
    }

    /// The shooting interval retraced on a frozen grid: from `x0` at
    /// `run.t0`, every step of `log` (a [`Engine::tran_period`] log)
    /// taken at its time and order with no LTE check and no
    /// breakpoint-driven stepping (device accept hooks still run with
    /// the logged `CKTbreak` flag). The map `x0 ↦ x(t0 + period)` is
    /// then a smooth function of `x0`, which is what the shooting
    /// Jacobian differentiates; an adaptive run would add the noise of
    /// its step choices. A step whose Newton iteration fails is
    /// [`Error::IterLim`]. Returns the final solution.
    pub fn tran_replay(
        &mut self,
        opts: &Options,
        run: &PeriodRun,
        x0: &[f64],
        log: &[StepRec],
    ) -> Result<Vec<f64>, Error> {
        self.replay_loop(opts, run, x0, log, None)
    }

    /// [`Engine::tran_replay`] keeping every row: `(t0, x0)` and the
    /// solution at each logged step (the rows of the logged run, on its
    /// own grid).
    pub fn tran_replay_rows(
        &mut self,
        opts: &Options,
        run: &PeriodRun,
        x0: &[f64],
        log: &[StepRec],
    ) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        let mut rows = Vec::with_capacity(log.len() + 1);
        self.replay_loop(opts, run, x0, log, Some(&mut rows))?;
        Ok(rows)
    }

    fn replay_loop(
        &mut self,
        opts: &Options,
        run: &PeriodRun,
        x0: &[f64],
        log: &[StepRec],
        mut rows: Option<&mut Vec<(f64, Vec<f64>)>>,
    ) -> Result<Vec<f64>, Error> {
        self.period_setup(opts, run, x0)?;
        let max_step = run.max_step;
        self.set_kind(AnKind::Tran, false);
        self.order = 1;
        self.ag = [0.0; 7];
        self.states.copy01();
        self.delta_old = [max_step; 8];
        self.step_old = [max_step; 8];
        self.delta = f64::min(run.period / 50.0, run.tstep) / 10.0;
        let mut firsttime = true;
        self.accept(true, true, opts)?;
        if self.time > self.breaks[0] {
            self.clr_break();
        }
        if let Some(rows) = rows.as_deref_mut() {
            rows.push((self.time, self.sol.clone()));
        }
        for rec in log {
            let delta = rec.time - self.time;
            if !(delta > 0.0 && delta.is_finite()) {
                return Err(Error::Build("pss: replayed grid is not increasing".into()));
            }
            for i in (0..=5).rev() {
                self.delta_old[i + 1] = self.delta_old[i];
            }
            self.delta_old[0] = delta;
            self.states.rotate(self.maxord);
            self.order = rec.order;
            if self.trbdf2 {
                for i in (0..=5).rev() {
                    self.step_old[i + 1] = self.step_old[i];
                }
                self.step_old[0] = delta;
                let mut d = delta;
                let mut steps = 0usize;
                self.tran_trbdf2_point(
                    opts,
                    &mut d,
                    &mut firsttime,
                    0.0,
                    &mut steps,
                    usize::MAX,
                    true,
                )?;
            } else {
                self.time = rec.time;
                self.delta = delta;
                self.ag = com_cof(self.method, self.order, delta, &self.delta_old)?;
                let start = if firsttime {
                    InitMode::Tran
                } else {
                    InitMode::Pred
                };
                self.ni_iter(opts, start, opts.itl4, 0.0, 1.0)?;
                firsttime = false;
            }
            self.time = rec.time;
            self.accept(rec.brk, false, opts)?;
            if self.time > self.breaks[0] {
                self.clr_break();
            }
            if let Some(rows) = rows.as_deref_mut() {
                rows.push((self.time, self.sol.clone()));
            }
        }
        Ok(self.sol.clone())
    }

    /// One accepted timepoint under `method=trbdf2` (issue #6): the
    /// inner attempt loop of [`Engine::tran`]. On entry the step
    /// history and the state banks have advanced once (as for any
    /// method) and `delta` is the step `h` to try; on success the
    /// engine sits at `t + h` and `delta` holds the next step.
    ///
    /// Where the 3f5 cuts the order — the first point, the point after
    /// a breakpoint, a retry after non-convergence — the step is one
    /// backward-Euler step checked by `CKTterr` at order 1, exactly the
    /// faithful restart: it re-seeds the companion currents, which a
    /// source corner leaves stale. Every other step is the composite:
    /// a trapezoidal stage to `t + γh`, then a BDF2 stage on
    /// `{t, t + γh, t + h}` (`com_cof` already solves non-uniform
    /// BDF2), checked by [`crate::integ::terr`]'s composite estimate.
    /// Stage 2 rotates the banks and `delta_old` once more; a rejection
    /// (a stage not converging, or the LTE) undoes that rotation and
    /// restores the solution at `t`, so every retry starts from the
    /// accepted point. The first point skips the LTE check, like
    /// `DCtran`. Returns whether the accepted step was the order-1
    /// restart.
    ///
    /// `forced` is the grid replay of [`Engine::tran_replay`]: the
    /// step `h` (and the restart choice, through `order`) is taken as
    /// given — no LTE check, and a stage that does not converge is
    /// [`Error::IterLim`] instead of a retry.
    #[allow(clippy::too_many_arguments)]
    fn tran_trbdf2_point(
        &mut self,
        opts: &Options,
        delta: &mut f64,
        firsttime: &mut bool,
        delmin: f64,
        steps: &mut usize,
        step_budget: usize,
        forced: bool,
    ) -> Result<bool, Error> {
        let t0 = self.time;
        let sol0 = self.sol.clone();
        loop {
            *steps += 1;
            if *steps > step_budget {
                return Err(Error::StepLimit {
                    steps: step_budget,
                    time: self.time,
                });
            }
            let olddelta = *delta;
            let h = *delta;
            self.step_old[0] = h;
            let start = if *firsttime {
                InitMode::Tran
            } else {
                InitMode::Pred
            };
            let restart = *firsttime || self.order == 1;

            let mut saved_old = None;
            let ok = if restart {
                // The 3f5's order-1 restart: one backward-Euler step.
                self.time = t0 + h;
                self.delta_old[0] = h;
                self.delta = h;
                self.method = Method::Trapezoidal;
                self.ag = com_cof(Method::Trapezoidal, 1, h, &self.delta_old)?;
                self.newton(opts, start)?
            } else {
                // Stage 1: trapezoidal to t + γh.
                let d1 = crate::integ::TRBDF2_GAMMA * h;
                let t1 = t0 + d1;
                self.time = t1;
                self.delta_old[0] = d1;
                self.delta = d1;
                self.method = Method::Trapezoidal;
                self.ag = com_cof(Method::Trapezoidal, 2, d1, &self.delta_old)?;
                if self.newton(opts, start)? {
                    // Stage 2: BDF2 on {t, t + γh, t + h}.
                    saved_old = Some(self.delta_old);
                    for i in (0..=5).rev() {
                        self.delta_old[i + 1] = self.delta_old[i];
                    }
                    self.states.rotate(self.maxord);
                    self.time = t0 + h;
                    let d2 = self.time - t1;
                    self.delta_old[0] = d2;
                    self.delta = d2;
                    self.method = Method::Gear;
                    self.ag = com_cof(Method::Gear, 2, d2, &self.delta_old)?;
                    self.newton(opts, InitMode::Pred)?
                } else {
                    false
                }
            };

            if ok {
                if *firsttime {
                    *firsttime = false;
                    self.order = 2;
                    return Ok(restart);
                }
                if forced {
                    self.order = 2;
                    return Ok(restart);
                }
                let mut new = h;
                self.composite = !restart;
                self.trunc(opts, &mut new);
                self.composite = false;
                *delta = new;
                if new > 0.9 * h {
                    self.order = 2;
                    return Ok(restart);
                }
            } else if forced {
                return Err(Error::IterLim);
            }
            // Rejected: roll back to the accepted point.
            if let Some(old) = saved_old {
                self.states.unrotate(self.maxord);
                self.delta_old = old;
            }
            self.time = t0;
            self.sol.copy_from_slice(&sol0);
            if !ok {
                *delta /= 8.0;
                self.order = 1;
            }
            if *delta <= delmin {
                if olddelta > delmin {
                    *delta = delmin;
                } else {
                    return Err(Error::Timestep { time: self.time });
                }
            }
        }
    }

    /// One transient Newton solve: `Ok(false)` when it does not
    /// converge (the step is retried smaller); a singular matrix is
    /// fatal, as in `DCtran`.
    fn newton(&mut self, opts: &Options, start: InitMode) -> Result<bool, Error> {
        match self.ni_iter(opts, start, opts.itl4, 0.0, 1.0) {
            Ok(_) => Ok(true),
            Err(e @ Error::Singular(_)) => Err(e),
            Err(_) => Ok(false),
        }
    }

    /// `ACan` (`ckt/acan.c`): operating point, one `MODEINITSMSIG`
    /// load to store small-signal parameters, then one complex
    /// solve per frequency (`NIacIter`). Returns `(freq, solution)`
    /// rows.
    pub fn ac(
        &mut self,
        opts: &Options,
        step_type: AcStepType,
        number_steps: usize,
        fstart: f64,
        fstop: f64,
    ) -> Result<Vec<(f64, Vec<Complex64>)>, Error> {
        let n = number_steps.max(1);
        let freq_delta = match step_type {
            // ngspice-47 (`acan.c`), not the 3f5: under a decade of
            // span the 3f5's `10^(1/n)` stays (`stop == start` gives
            // 1, one point); over it the step is recomputed as
            // `exp(ln(stop/start)/floor(log10(stop/start)·n))` so
            // the last point lands exactly on FSTOP. The two agree
            // only on whole-decade spans (every older corpus deck;
            // gate: `corpus/regress/acdec.cir`).
            AcStepType::Decade => {
                if fstop / 10.0 < fstart {
                    if fstop == fstart {
                        1.0
                    } else {
                        f64::exp(f64::ln(10.0) / n as f64)
                    }
                } else {
                    let steps = f64::floor((fstop / fstart).log10().abs() * n as f64);
                    f64::exp(f64::ln(fstop / fstart) / steps)
                }
            }
            AcStepType::Octave => f64::exp(f64::ln(2.0) / n as f64),
            AcStepType::Linear => {
                // The C: delta = (stop-start)/(n-1) only when
                // n-1 > 1, else 0 (the McRoberts patch in ngspice;
                // a 2-point linear sweep yields one point either
                // way — the 3f5's HUGE did the same).
                if n - 1 > 1 {
                    (fstop - fstart) / (n - 1) as f64
                } else {
                    0.0
                }
            }
        };

        sweep_budget(step_type, n, fstart, fstop, self.vars.len(), "ac")?;

        // DC operating point, then the small-signal storage pass.
        self.set_kind(AnKind::DcOp, false);
        self.op(opts)?;
        self.load_all(InitMode::Smsig, opts, 1.0);

        let freq_tol = match step_type {
            AcStepType::Linear => freq_delta * opts.reltol,
            _ => freq_delta * fstop * opts.reltol,
        };

        let nvars = self.vars.len();
        let num_slots = self.values.len();
        let mut g = vec![0.0; num_slots];
        let mut b = vec![0.0; num_slots];
        let mut re = vec![0.0; nvars];
        let mut im = vec![0.0; nvars];
        let mut rows: Vec<(f64, Vec<Complex64>)> = Vec::new();
        let mut freq = fstart;
        while freq <= fstop + freq_tol {
            let omega = 2.0 * std::f64::consts::PI * freq;
            g.fill(0.0);
            b.fill(0.0);
            re.fill(0.0);
            im.fill(0.0);
            let mut ctx = AcCtx {
                g: &mut g,
                b: &mut b,
                rhs_re: &mut re,
                rhs_im: &mut im,
                omega,
                states: &self.states,
                sp: false,
            };
            for d in &mut self.devices {
                d.load_ac(&mut ctx);
            }
            self.solver
                .factor_complex(&g[1..], &b[1..])
                .map_err(|e| self.map_singular(e))?;
            self.solver
                .solve_complex_in_place(&mut re[1..], &mut im[1..])
                .map_err(|e| self.map_singular(e))?;
            re[0] = 0.0;
            im[0] = 0.0;
            if (rows.len() + 1).saturating_mul(nvars) > MAX_OUTPUT_VALUES {
                return Err(Error::Build(
                    "ac sweep produces too many points; refusing".into(),
                ));
            }
            rows.push((
                freq,
                re.iter()
                    .zip(&im)
                    .map(|(r, i)| Complex64::new(*r, *i))
                    .collect(),
            ));
            match step_type {
                AcStepType::Decade | AcStepType::Octave => {
                    freq *= freq_delta;
                    if freq_delta == 1.0 {
                        break;
                    }
                }
                AcStepType::Linear => {
                    freq += freq_delta;
                    if freq_delta == 0.0 {
                        break;
                    }
                }
            }
        }
        Ok(rows)
    }

    /// Devices with noise generators, in device order (`CKTnoise`).
    /// [`crate::devices::Device::noise_sep`] of device `d`.
    pub fn noise_sep(&self, d: usize) -> &'static str {
        self.devices[d].noise_sep()
    }

    pub fn noise_devices(&self) -> Vec<(usize, &'static [&'static str])> {
        self.devices
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.noise_suffixes().map(|sfx| (i, sfx)))
            .collect()
    }

    /// One noise frequency point: the ordinary AC solve for the
    /// output gain, then the adjoint (transposed) solve with a unit
    /// current at the output (`NIacIter` + `NInzIter`). Returns the
    /// squared gain and the adjoint solution.
    pub fn noise_point(
        &mut self,
        freq: f64,
        pos_out: Var,
        neg_out: Var,
    ) -> Result<(f64, Vec<f64>, Vec<f64>), Error> {
        let omega = 2.0 * std::f64::consts::PI * freq;
        let num_slots = self.values.len();
        let nvars = self.vars.len();
        let mut g = vec![0.0; num_slots];
        let mut b = vec![0.0; num_slots];
        let mut re = vec![0.0; nvars];
        let mut im = vec![0.0; nvars];
        {
            let mut ctx = AcCtx {
                g: &mut g,
                b: &mut b,
                rhs_re: &mut re,
                rhs_im: &mut im,
                omega,
                states: &self.states,
                sp: false,
            };
            for d in &mut self.devices {
                d.load_ac(&mut ctx);
            }
        }
        self.solver
            .factor_complex(&g[1..], &b[1..])
            .map_err(|e| self.map_singular(e))?;
        self.solver
            .solve_complex_in_place(&mut re[1..], &mut im[1..])
            .map_err(|e| self.map_singular(e))?;
        re[0] = 0.0;
        im[0] = 0.0;
        let gr = re[pos_out] - re[neg_out];
        let gi = im[pos_out] - im[neg_out];
        let gain2 = gr * gr + gi * gi;

        // Adjoint system: unit current excitation at the output.
        re.fill(0.0);
        im.fill(0.0);
        re[pos_out] = 1.0;
        re[neg_out] -= 1.0;
        re[0] = 0.0;
        im[0] = 0.0;
        self.solver
            .solve_complex_transposed_in_place(&mut re[1..], &mut im[1..])
            .map_err(|e| self.map_singular(e))?;
        re[0] = 0.0;
        im[0] = 0.0;
        Ok((gain2, re, im))
    }

    /// Spectral densities of device `d` against the adjoint
    /// solution.
    pub fn noise_dens_of(
        &self,
        d: usize,
        freq: f64,
        re: &[f64],
        im: &[f64],
        temp: f64,
    ) -> Vec<f64> {
        let ctx = NoiseCtx {
            re,
            im,
            freq,
            temp,
            states: &self.states,
            sp_sink: None,
            flicker_sink: None,
        };
        self.devices[d].noise_dens(&ctx)
    }

    /// The thermal and shot generators of every noisy device,
    /// `(n1, n2, density)` in device order (`CKTspnoise` walking
    /// `DEVnoise` under `DOING_SP`; `nevalsrc.c`). The adjoint
    /// solution handed to the devices is only read for flicker gains,
    /// which `.sp` drops.
    pub fn sp_noise_generators(&self, freq: f64, temp: f64) -> Vec<crate::matrix::NoiseGen> {
        let sink = std::cell::RefCell::new(Vec::new());
        let zeros = vec![0.0; self.vars.len()];
        let ctx = NoiseCtx {
            re: &zeros,
            im: &zeros,
            freq,
            temp,
            states: &self.states,
            sp_sink: Some(&sink),
            flicker_sink: None,
        };
        for d in &self.devices {
            if d.noise_suffixes().is_some() {
                d.noise_dens(&ctx);
            }
        }
        sink.into_inner()
    }

    /// `SPan`'s operating point (`span.c`): `CKTop`, then the
    /// `MODEINITSMSIG` load that stores the small-signal parameters.
    pub fn sp_prepare(&mut self, opts: &Options) -> Result<(), Error> {
        self.set_kind(AnKind::DcOp, false);
        self.op(opts)?;
        self.load_all(InitMode::Smsig, opts, 1.0);
        Ok(())
    }

    /// One `.sp` frequency (`span.c` inner loop): `NIspPreload` — the
    /// AC load under `MODESP` and one complex factorization — then
    /// one `NIspSolve` per port with that port's branch driven by 1 V
    /// (`VSRCspupdate`), and, when `adjoint` is set, the transposed
    /// solves with a unit current into each port's `+` node
    /// (`NInspIter`) on the same factors.
    ///
    /// The base RHS reproduces the C exactly: `span.c` backs up the
    /// loaded RHS with two `memcpy` into `rhswoPorts` (the second,
    /// meant for `irhswoPorts`, overwrites it with the imaginary
    /// part) and restores the never-written, zeroed `irhswoPorts` as
    /// the imaginary part. With every V source's stimulus off under
    /// `MODESP` only I-source `AC` values reach the load, and of
    /// those only the imaginary part survives, as a real excitation.
    ///
    /// Returns per port, in port order, the forward solution and
    /// (when asked) the adjoint solution, var-indexed.
    #[allow(clippy::type_complexity)]
    pub fn sp_point(
        &mut self,
        freq: f64,
        ports: &[crate::circuit::PortInfo],
        adjoint: bool,
    ) -> Result<(Vec<Vec<Complex64>>, Vec<Vec<Complex64>>), Error> {
        let omega = 2.0 * std::f64::consts::PI * freq;
        let num_slots = self.values.len();
        let nvars = self.vars.len();
        let mut g = vec![0.0; num_slots];
        let mut b = vec![0.0; num_slots];
        let mut load_re = vec![0.0; nvars];
        let mut load_im = vec![0.0; nvars];
        {
            let mut ctx = AcCtx {
                g: &mut g,
                b: &mut b,
                rhs_re: &mut load_re,
                rhs_im: &mut load_im,
                omega,
                states: &self.states,
                sp: true,
            };
            for d in &mut self.devices {
                d.load_ac(&mut ctx);
            }
        }
        self.solver
            .factor_complex(&g[1..], &b[1..])
            .map_err(|e| self.map_singular(e))?;
        // `rhswoPorts` ends up holding the loaded imaginary part.
        let base_re = load_im;

        let mut forward = Vec::with_capacity(ports.len());
        let mut re = vec![0.0; nvars];
        let mut im = vec![0.0; nvars];
        for p in ports {
            re.copy_from_slice(&base_re);
            im.fill(0.0);
            re[p.br] += 1.0;
            self.solver
                .solve_complex_in_place(&mut re[1..], &mut im[1..])
                .map_err(|e| self.map_singular(e))?;
            re[0] = 0.0;
            im[0] = 0.0;
            forward.push(
                re.iter()
                    .zip(&im)
                    .map(|(r, i)| Complex64::new(*r, *i))
                    .collect(),
            );
        }

        let mut adjoints = Vec::new();
        if adjoint {
            for p in ports {
                re.fill(0.0);
                im.fill(0.0);
                re[p.pos] = 1.0;
                re[p.neg] = -1.0;
                re[0] = 0.0;
                self.solver
                    .solve_complex_transposed_in_place(&mut re[1..], &mut im[1..])
                    .map_err(|e| self.map_singular(e))?;
                re[0] = 0.0;
                im[0] = 0.0;
                adjoints.push(
                    re.iter()
                        .zip(&im)
                        .map(|(r, i)| Complex64::new(*r, *i))
                        .collect(),
                );
            }
        }
        Ok((forward, adjoints))
    }

    /// `DISTOan` (`ckt/distoan.c`): operating point, the
    /// `MODEINITSMSIG` pass, `DEVdisto(D_SETUP)` for the Taylor
    /// coefficients, then per f1 the chain of complex solves
    /// (`CKTacLoad` for the matrix at the combination frequency,
    /// `CKTdisto` for the RHS, `NIdIter`). Returns one
    /// `(plot name, rows)` per output plot, each row already scaled
    /// by `DkerProc`.
    #[allow(clippy::too_many_lines, clippy::type_complexity)]
    pub fn disto(
        &mut self,
        opts: &Options,
        step_type: AcStepType,
        num_steps: usize,
        fstart: f64,
        fstop: f64,
        f2ovr_f1: Option<f64>,
    ) -> Result<Vec<(&'static str, Vec<(f64, Vec<Complex64>)>)>, Error> {
        // `DnumSteps` is an int; a zero step count would make the
        // decade multiplier infinite and the loop endless.
        let n = num_steps.max(1) as f64;
        sweep_budget(
            step_type,
            num_steps.max(1),
            fstart,
            fstop,
            self.vars.len(),
            "disto",
        )?;
        let (freq_delta, freq_tol) = match step_type {
            AcStepType::Decade => {
                let d = f64::exp(f64::ln(10.0) / n);
                (d, d * fstop * opts.reltol)
            }
            AcStepType::Octave => {
                let d = f64::exp(f64::ln(2.0) / n);
                (d, d * fstop * opts.reltol)
            }
            // The 3f5 divides by DnumSteps+1 here (unlike ACan).
            AcStepType::Linear => {
                let d = (fstop - fstart) / (n + 1.0);
                (d, d * opts.reltol)
            }
        };

        self.set_kind(AnKind::DcOp, false);
        self.op(opts)?;
        self.load_all(InitMode::Smsig, opts, 1.0);

        // `CKTdisto(D_SETUP)`: every device must know its
        // distortion coefficients (the C skips devices whose
        // `DEVdisto` is NULL; Dune's linear devices opt in, and a
        // nonlinear device without the port is refused).
        {
            let ctx = DistoSetupCtx {
                sol: &self.sol,
                gmin: opts.gmin,
            };
            for (k, d) in self.devices.iter_mut().enumerate() {
                d.disto_setup(&ctx).map_err(|what| Error::Unsupported {
                    name: self.names[k].clone(),
                    what: what.to_string(),
                })?;
            }
        }

        let f2wanted = f2ovr_f1.is_some();
        let f2ovr_f1 = f2ovr_f1.unwrap_or(0.0);
        let two_pi = 2.0 * std::f64::consts::PI;
        let mut freq = fstart;
        // "keeping f2 const to be compatible with hbsim": ω2 is set
        // once, from the start frequency.
        let omega2 = two_pi * freq * f2ovr_f1;

        let nvars = self.vars.len();
        let num_slots = self.values.len();
        let mut g = vec![0.0; num_slots];
        let mut b = vec![0.0; num_slots];
        let empty: [f64; 0] = [];

        let mut out: Vec<(DistoMode, Vec<(f64, Vec<Complex64>)>)> = if f2wanted {
            vec![
                (DistoMode::F1pF2, Vec::new()),
                (DistoMode::F1mF2, Vec::new()),
                (DistoMode::TwoF1mF2, Vec::new()),
            ]
        } else {
            vec![
                (DistoMode::TwoF1, Vec::new()),
                (DistoMode::ThrF1, Vec::new()),
            ]
        };

        while freq <= fstop + freq_tol {
            let omega1 = two_pi * freq;

            // H1(f1): sources' DISTOF1 inputs (`D_RHSF1`).
            self.disto_factor(omega1, &mut g, &mut b)?;
            let mut f2given = false;
            let (r1h1, i1h1) = {
                let mut re = vec![0.0; nvars];
                let mut im = vec![0.0; nvars];
                for d in &self.devices {
                    f2given |= d.disto_rhs(DistoInput::F1, &mut re, &mut im);
                }
                self.disto_solve(re, im)?
            };

            // H2(f1, f1) at 2·ω1 (`D_TWOF1`).
            let omega = 2.0 * omega1;
            let (r2h11, i2h11) = self.disto_kernel(
                DistoMode::TwoF1,
                omega,
                &mut g,
                &mut b,
                DistoKernels {
                    r1h1: &r1h1,
                    i1h1: &i1h1,
                    r2h11: &empty,
                    i2h11: &empty,
                    r1h2: &empty,
                    i1h2: &empty,
                    r2h1m2: &empty,
                    i2h1m2: &empty,
                },
                (omega1, omega2),
            )?;

            let mut results: Vec<(Vec<f64>, Vec<f64>)> = Vec::new();
            if !f2wanted {
                // H3(f1, f1, f1) at 3·ω1 (`D_THRF1`).
                let h3 = self.disto_kernel(
                    DistoMode::ThrF1,
                    3.0 * omega1,
                    &mut g,
                    &mut b,
                    DistoKernels {
                        r1h1: &r1h1,
                        i1h1: &i1h1,
                        r2h11: &r2h11,
                        i2h11: &i2h11,
                        r1h2: &empty,
                        i1h2: &empty,
                        r2h1m2: &empty,
                        i2h1m2: &empty,
                    },
                    (omega1, omega2),
                )?;
                results.push((r2h11, i2h11));
                results.push(h3);
            } else if f2given {
                // H1(f2): DISTOF2 inputs (`D_RHSF2`).
                self.disto_factor(omega2, &mut g, &mut b)?;
                let (r1h2, i1h2) = {
                    let mut re = vec![0.0; nvars];
                    let mut im = vec![0.0; nvars];
                    for d in &self.devices {
                        d.disto_rhs(DistoInput::F2, &mut re, &mut im);
                    }
                    self.disto_solve(re, im)?
                };
                // H2(f1, f2) at ω1+ω2 (`D_F1PF2`).
                let h12 = self.disto_kernel(
                    DistoMode::F1pF2,
                    omega1 + omega2,
                    &mut g,
                    &mut b,
                    DistoKernels {
                        r1h1: &r1h1,
                        i1h1: &i1h1,
                        r2h11: &r2h11,
                        i2h11: &i2h11,
                        r1h2: &r1h2,
                        i1h2: &i1h2,
                        r2h1m2: &empty,
                        i2h1m2: &empty,
                    },
                    (omega1, omega2),
                )?;
                // H2(f1, −f2) at ω1−ω2 (`D_F1MF2`).
                let (r2h1m2, i2h1m2) = self.disto_kernel(
                    DistoMode::F1mF2,
                    omega1 - omega2,
                    &mut g,
                    &mut b,
                    DistoKernels {
                        r1h1: &r1h1,
                        i1h1: &i1h1,
                        r2h11: &r2h11,
                        i2h11: &i2h11,
                        r1h2: &r1h2,
                        i1h2: &i1h2,
                        r2h1m2: &empty,
                        i2h1m2: &empty,
                    },
                    (omega1, omega2),
                )?;
                // H3(f1, f1, −f2) at 2ω1−ω2 (`D_2F1MF2`).
                let h3 = self.disto_kernel(
                    DistoMode::TwoF1mF2,
                    2.0 * omega1 - omega2,
                    &mut g,
                    &mut b,
                    DistoKernels {
                        r1h1: &r1h1,
                        i1h1: &i1h1,
                        r2h11: &r2h11,
                        i2h11: &i2h11,
                        r1h2: &r1h2,
                        i1h2: &i1h2,
                        r2h1m2: &r2h1m2,
                        i2h1m2: &i2h1m2,
                    },
                    (omega1, omega2),
                )?;
                results.push(h12);
                results.push((r2h1m2, i2h1m2));
                results.push(h3);
            } else {
                // `E_NOF2SRC`.
                return Err(Error::Build(
                    ".disto: no source with f2 distortion input".into(),
                ));
            }

            let total: usize = out.iter().map(|(_, r)| r.len() + 1).sum();
            if total.saturating_mul(nvars) > MAX_OUTPUT_VALUES {
                return Err(Error::Build(
                    "disto sweep produces too many points; refusing".into(),
                ));
            }
            // `DkerProc` + `CKTacDump`.
            for ((mode, rows), (re, im)) in out.iter_mut().zip(results) {
                let k = mode.ker_factor();
                let mut sol: Vec<Complex64> = re
                    .iter()
                    .zip(&im)
                    .map(|(r, i)| Complex64::new(r * k, i * k))
                    .collect();
                sol[0] = Complex64::new(0.0, 0.0);
                rows.push((freq, sol));
            }

            match step_type {
                AcStepType::Decade | AcStepType::Octave => {
                    freq *= freq_delta;
                    if freq_delta == 1.0 {
                        break;
                    }
                }
                AcStepType::Linear => {
                    freq += freq_delta;
                    if freq_delta == 0.0 {
                        break;
                    }
                }
            }
        }

        Ok(out
            .into_iter()
            .map(|(mode, rows)| {
                let name = match mode {
                    DistoMode::TwoF1 => "DISTORTION - 2nd harmonic",
                    DistoMode::ThrF1 => "DISTORTION - 3rd harmonic",
                    DistoMode::F1pF2 => "DISTORTION - IM: f1+f2",
                    DistoMode::F1mF2 => "DISTORTION - IM: f1-f2",
                    DistoMode::TwoF1mF2 => "DISTORTION - IM: 2f1-f2",
                };
                (name, rows)
            })
            .collect())
    }

    /// `CKTacLoad` at `omega` (matrix only — the RHS it would stamp
    /// is overwritten by `CKTdisto`) and the complex factorization
    /// `NIdIter` performs.
    fn disto_factor(&mut self, omega: f64, g: &mut [f64], b: &mut [f64]) -> Result<(), Error> {
        let nvars = self.vars.len();
        let mut scratch_re = vec![0.0; nvars];
        let mut scratch_im = vec![0.0; nvars];
        g.fill(0.0);
        b.fill(0.0);
        let mut ctx = AcCtx {
            g,
            b,
            rhs_re: &mut scratch_re,
            rhs_im: &mut scratch_im,
            omega,
            states: &self.states,
            sp: false,
        };
        for d in &mut self.devices {
            d.load_ac(&mut ctx);
        }
        self.solver
            .factor_complex(&g[1..], &b[1..])
            .map_err(|e| self.map_singular(e))
    }

    /// `NIdIter`'s solve on the current factorization; zeroes the
    /// ground entry like the C.
    fn disto_solve(
        &mut self,
        mut re: Vec<f64>,
        mut im: Vec<f64>,
    ) -> Result<(Vec<f64>, Vec<f64>), Error> {
        re[0] = 0.0;
        im[0] = 0.0;
        self.solver
            .solve_complex_in_place(&mut re[1..], &mut im[1..])
            .map_err(|e| self.map_singular(e))?;
        re[0] = 0.0;
        im[0] = 0.0;
        Ok((re, im))
    }

    /// One higher-order kernel: factor at the combination frequency,
    /// `CKTdisto(mode)` (zeroed RHS + every device's nonlinear
    /// load), solve.
    fn disto_kernel(
        &mut self,
        mode: DistoMode,
        omega: f64,
        g: &mut [f64],
        b: &mut [f64],
        h: DistoKernels,
        (omega1, omega2): (f64, f64),
    ) -> Result<(Vec<f64>, Vec<f64>), Error> {
        self.disto_factor(omega, g, b)?;
        let nvars = self.vars.len();
        let mut re = vec![0.0; nvars];
        let mut im = vec![0.0; nvars];
        {
            let mut ctx = DistoCtx {
                mode,
                omega,
                omega1,
                omega2,
                h,
                rhs_re: &mut re,
                rhs_im: &mut im,
            };
            for d in &self.devices {
                d.disto_load(&mut ctx);
            }
        }
        self.disto_solve(re, im)
    }

    /// `CKTpzSetup` (`ckt/cktpzset.c`): rebuild the matrix pattern
    /// for a pole or zero search — the transfer-function columns,
    /// the drive entries and the slots the column surgery needs.
    /// The engine's solver is re-analyzed; only `pz_det` may be
    /// used afterwards.
    pub fn pz_setup(
        &mut self,
        in_pos: Var,
        in_neg: Var,
        out_pos: Var,
        out_neg: Var,
        vol_input: bool,
        zeros: bool,
    ) -> PzSetup {
        let n = self.vars.len() - 1;
        let mut mb = MatrixBuilder::new(n);
        for d in &mut self.devices {
            d.alloc(&mut mb);
        }

        let mut input_pos = in_pos;
        let mut input_neg = in_neg;
        let (output_pos, output_neg) = if zeros {
            (out_pos, out_neg)
        } else if vol_input {
            // Vi/Ii in Y.
            (in_pos, in_neg)
        } else {
            // Denominator only.
            input_pos = 0;
            input_neg = 0;
            (0, 0)
        };

        let solution_col;
        let mut balance_col = 0;
        if output_pos != 0 {
            solution_col = output_pos;
            if output_neg != 0 {
                balance_col = output_neg;
            }
        } else {
            solution_col = output_neg;
            std::mem::swap(&mut input_pos, &mut input_neg);
        }

        let drive_p =
            (input_pos != 0 && solution_col != 0).then(|| mb.slot(input_pos, solution_col));
        let drive_n =
            (input_neg != 0 && solution_col != 0).then(|| mb.slot(input_neg, solution_col));

        // `SMPcAddCol` needs a slot in the solution column for every
        // row the balance column touches.
        let mut bal_pairs = Vec::new();
        if balance_col != 0 && solution_col != 0 {
            for (r, bs) in mb.rows_in_col(balance_col) {
                let ss = mb.slot(r, solution_col);
                bal_pairs.push((bs, ss));
            }
        }
        let sol_slots: Vec<Slot> = if solution_col != 0 {
            mb.rows_in_col(solution_col)
                .into_iter()
                .map(|(_, s)| s)
                .collect()
        } else {
            Vec::new()
        };

        self.solver.analyze(&mb.pattern());
        let num_slots = mb.num_slots();
        self.mb = mb;
        PzSetup {
            bal_pairs,
            sol_slots,
            drive_p,
            drive_n,
            g: vec![0.0; num_slots],
            b: vec![0.0; num_slots],
        }
    }

    /// `CKTpzLoad` + `SMPcLUfac` + `SMPcDProd`: the transfer
    /// function's determinant at the trial frequency, as a base-2
    /// mantissa/exponent pair (zero mantissa means singular — a
    /// root).
    pub fn pz_det(&mut self, setup: &mut PzSetup, s: Complex64) -> Result<(Complex64, i32), Error> {
        setup.g.fill(0.0);
        setup.b.fill(0.0);
        let mut ctx = PzCtx {
            g: &mut setup.g,
            b: &mut setup.b,
            s,
            states: &self.states,
        };
        for d in &mut self.devices {
            d.load_pz(&mut ctx)
                .map_err(|e| Error::Build(format!("pole-zero: {e}")))?;
        }
        // Column surgery: fold the balance column into the solution
        // column, zero the solution column, then set the drive.
        for &(bs, ss) in &setup.bal_pairs {
            setup.g[ss.0] += setup.g[bs.0];
            setup.b[ss.0] += setup.b[bs.0];
        }
        for sl in &setup.sol_slots {
            setup.g[sl.0] = 0.0;
            setup.b[sl.0] = 0.0;
        }
        if let Some(dp) = setup.drive_p {
            setup.g[dp.0] = 1.0;
            setup.b[dp.0] = 0.0;
        }
        if let Some(dn) = setup.drive_n {
            setup.g[dn.0] = -1.0;
            setup.b[dn.0] = 0.0;
        }
        Ok(self.solver.det_complex(&setup.g[1..], &setup.b[1..]))
    }
}

/// `.tf` and `.sens` (`ckt/tfanal.c`, `ckt/cktsens.c`): extra solves
/// against the factorization the operating point (or one AC point)
/// left in the solver, and the per-parameter perturbation loads of
/// the sensitivity analysis.
impl Engine {
    /// `SMPsolve` of `rhs` (var-indexed, `[0]` is ground) against the
    /// operating-point Jacobian still factored in the solver — the
    /// last Newton iteration's matrix, as `TFanal` uses it.
    pub fn solve_factored(&mut self, rhs: &mut [f64]) -> Result<(), Error> {
        rhs[0] = 0.0;
        self.solver
            .solve_in_place(&mut rhs[1..])
            .map_err(|e| self.map_singular(e))?;
        rhs[0] = 0.0;
        Ok(())
    }

    /// The sensitivity parameters of every device, in device order;
    /// a device without a sensitivity port fails the analysis (the C
    /// would perturb its parameters too, so skipping it silently
    /// would drop vectors).
    pub fn sens_params(&self, ac: bool) -> Result<Vec<Vec<SensParam>>, Error> {
        self.devices
            .iter()
            .zip(&self.names)
            .map(|(d, name)| {
                d.sens_params(ac).ok_or_else(|| Error::Unsupported {
                    name: name.clone(),
                    what: "sensitivity analysis of this device".into(),
                })
            })
            .collect()
    }

    /// `Y·x` restricted to the stamps in `values` (slot-indexed), the
    /// `SMPmultiply` of `delta_Y·E`: returns the var-indexed product.
    fn slot_product(&self, values: &[f64], x: &[f64]) -> Vec<f64> {
        let mut out = vec![0.0; x.len()];
        for (k, &(r, c)) in self.mb.entries().iter().enumerate() {
            let v = values[k + 1];
            if v != 0.0 {
                out[r + 1] += v * x[c + 1];
            }
        }
        out
    }

    /// One DC sensitivity column (`sens_sens`, `is_dc`): device `dev`
    /// loaded into an empty matrix/RHS at the operating point, its
    /// parameter `suffix` moved from `value` by `delta`, loaded again,
    /// restored; then `delta_E = Y⁻¹·(ΔI − ΔY·E) / delta`. The loads
    /// see `CKTrhsOld` — the iterate the last Jacobian was built at,
    /// which `op` leaves in `rhs` — and `E` is the converged solution.
    pub fn sens_dc_column(
        &mut self,
        opts: &Options,
        dev: usize,
        suffix: &str,
        value: f64,
        delta: f64,
    ) -> Result<Vec<f64>, Error> {
        let saved = self.states.snapshot0();
        let load_at = self.rhs.clone();
        let one = |engine: &mut Engine| {
            let mut values = vec![0.0; engine.values.len()];
            let mut rhs = vec![0.0; engine.rhs.len()];
            let mut ctx = LoadCtx {
                sink: StampSink::Direct {
                    values: &mut values,
                    rhs: &mut rhs,
                },
                sol: &load_at,
                mode: InitMode::Float,
                kind: AnKind::DcOp,
                uic: false,
                time: 0.0,
                step: 0.0,
                final_time: 0.0,
                gmin: opts.gmin,
                reltol: opts.reltol,
                abstol: opts.abstol,
                srcfact: 1.0,
                noncon: false,
                states: engine.states.view(),
                ag: [0.0; 7],
                order: 1,
                method: engine.method,
                pred_xfact: 0.0,
                delta: 0.0,
                fault: None,
            };
            engine.devices[dev].load(&mut ctx);
            engine.devices[dev].load_late(&mut ctx);
            match ctx.fault {
                Some(f) => Err(Error::Build(f.into())),
                None => Ok((values, rhs)),
            }
        };
        let (y0, i0) = one(self)?;
        self.devices[dev].sens_set(suffix, value + delta);
        let perturbed = one(self);
        self.devices[dev].sens_set(suffix, value);
        self.states.restore0(&saved);
        let (y1, i1) = perturbed?;

        let dy: Vec<f64> = y1.iter().zip(&y0).map(|(a, b)| a - b).collect();
        let dye = self.slot_product(&dy, &self.sol);
        let mut w: Vec<f64> = i1
            .iter()
            .zip(&i0)
            .zip(&dye)
            .map(|((a, b), p)| (a - b) - p)
            .collect();
        self.solve_factored(&mut w)?;
        for x in &mut w {
            *x /= delta;
        }
        Ok(w)
    }

    /// The AC solution at `omega` (`NIacIter` after the small-signal
    /// pass), leaving `G + jB` factored for the sensitivity solves.
    pub fn sens_ac_solution(&mut self, omega: f64) -> Result<(Vec<f64>, Vec<f64>), Error> {
        let num_slots = self.values.len();
        let nvars = self.vars.len();
        let mut g = vec![0.0; num_slots];
        let mut b = vec![0.0; num_slots];
        let mut re = vec![0.0; nvars];
        let mut im = vec![0.0; nvars];
        let mut ctx = AcCtx {
            g: &mut g,
            b: &mut b,
            rhs_re: &mut re,
            rhs_im: &mut im,
            omega,
            states: &self.states,
            sp: false,
        };
        for d in &mut self.devices {
            d.load_ac(&mut ctx);
        }
        self.solver
            .factor_complex(&g[1..], &b[1..])
            .map_err(|e| self.map_singular(e))?;
        self.solver
            .solve_complex_in_place(&mut re[1..], &mut im[1..])
            .map_err(|e| self.map_singular(e))?;
        re[0] = 0.0;
        im[0] = 0.0;
        Ok((re, im))
    }

    /// One AC sensitivity column: the [`Engine::sens_dc_column`]
    /// recipe on the `DEVacLoad` stamps (complex `ΔY`, `ΔI`) against
    /// the AC solution `e` and the factorization of
    /// [`Engine::sens_ac_solution`].
    #[allow(clippy::too_many_arguments)]
    pub fn sens_ac_column(
        &mut self,
        omega: f64,
        dev: usize,
        suffix: &str,
        value: f64,
        delta: f64,
        e: (&[f64], &[f64]),
    ) -> Result<(Vec<f64>, Vec<f64>), Error> {
        let one = |engine: &mut Engine| {
            let mut g = vec![0.0; engine.values.len()];
            let mut b = vec![0.0; engine.values.len()];
            let mut re = vec![0.0; engine.rhs.len()];
            let mut im = vec![0.0; engine.rhs.len()];
            let mut ctx = AcCtx {
                g: &mut g,
                b: &mut b,
                rhs_re: &mut re,
                rhs_im: &mut im,
                omega,
                states: &engine.states,
                sp: false,
            };
            engine.devices[dev].load_ac(&mut ctx);
            (g, b, re, im)
        };
        let (g0, b0, r0, i0) = one(self);
        self.devices[dev].sens_set(suffix, value + delta);
        let (g1, b1, r1, i1) = one(self);
        self.devices[dev].sens_set(suffix, value);

        let dg: Vec<f64> = g1.iter().zip(&g0).map(|(a, b)| a - b).collect();
        let db: Vec<f64> = b1.iter().zip(&b0).map(|(a, b)| a - b).collect();
        let (er, ei) = e;
        // (dG + j dB)(er + j ei)
        let gr = self.slot_product(&dg, er);
        let gi = self.slot_product(&dg, ei);
        let br = self.slot_product(&db, er);
        let bi = self.slot_product(&db, ei);
        let mut wr: Vec<f64> = (0..r1.len())
            .map(|k| (r1[k] - r0[k]) - (gr[k] - bi[k]))
            .collect();
        let mut wi: Vec<f64> = (0..i1.len())
            .map(|k| (i1[k] - i0[k]) - (gi[k] + br[k]))
            .collect();
        wr[0] = 0.0;
        wi[0] = 0.0;
        self.solver
            .solve_complex_in_place(&mut wr[1..], &mut wi[1..])
            .map_err(|e| self.map_singular(e))?;
        wr[0] = 0.0;
        wi[0] = 0.0;
        for x in wr.iter_mut().chain(wi.iter_mut()) {
            *x /= delta;
        }
        Ok((wr, wi))
    }
}

/// Matrix bookkeeping for one pole or zero search
/// (`CKTpzSetup`'s products).
pub(crate) struct PzSetup {
    /// (balance slot, solution slot) per shared row (`SMPcAddCol`).
    bal_pairs: Vec<(Slot, Slot)>,
    /// Every slot of the solution column (`SMPcZeroCol`).
    sol_slots: Vec<Slot>,
    drive_p: Option<Slot>,
    drive_n: Option<Slot>,
    g: Vec<f64>,
    b: Vec<f64>,
}

/// `.ac` sweep spacing (`DEC OCT LIN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcStepType {
    Decade,
    Octave,
    Linear,
}

/// What one `.dc` level drives.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SweepTarget {
    /// A V/I source's DC value or (ngspice) a resistor's
    /// resistance, through [`Device::set_dc`].
    Device(usize),
    /// The circuit temperature, °C (ngspice's `TEMP_CODE`).
    Temp,
}

/// One resolved `.dc` sweep level.
pub(crate) struct SweepRef {
    pub target: SweepTarget,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

/// The timestep loop's settings ([`Engine::tran_loop`]): the values
/// `DCtran` computes before its operating point.
pub(crate) struct TranLoop {
    /// Rows before this time are not returned.
    pub tstart: f64,
    pub uic: bool,
    pub max_step: f64,
    pub delmin: f64,
    /// The first step tried.
    pub delta: f64,
    /// `CKTsaveDelta`'s initial value.
    pub save_delta: f64,
    pub step_budget: usize,
}

/// One accepted step of a logged transient: the point's time, the
/// integration order it was solved at (under TR-BDF2, 1 for the
/// backward-Euler restart and 2 for the composite step) and the
/// `CKTbreak` flag its accept saw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepRec {
    pub time: f64,
    pub order: usize,
    pub brk: bool,
}

/// One shooting interval of `.pss`: a transient over
/// `[t0, t0 + period]` started from a given state.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PeriodRun {
    pub t0: f64,
    pub period: f64,
    /// The `TSTEP` sources see (default PULSE rise/fall times).
    pub tstep: f64,
    pub max_step: f64,
}

/// How many load passes [`Engine::seed_states`] allows the junction
/// limiting to walk the device states onto the given solution.
const SEED_PASSES: usize = 200;

/// `.tran` parameters after validation.
pub(crate) struct TranParams {
    pub tstep: f64,
    pub tstop: f64,
    pub tstart: f64,
    pub tmax: Option<f64>,
    pub uic: bool,
}

#[cfg(test)]
mod tests {
    use super::{AnKind, Engine, GminWalk, InitMode, OpStep, SrcRamp, within_radius};
    use crate::options::Options;

    fn engine_for(deck: &str) -> (Engine, Options) {
        let parsed = dune_netlist::parse_str("t.cir", deck);
        let (ckt, _) = crate::Circuit::from_netlist(&parsed.netlist).unwrap();
        let mut opts = Options::default();
        let mut diags = Vec::new();
        opts.apply(&parsed.netlist.options, &mut diags);
        let built = ckt.build(&opts).unwrap();
        (Engine::new(built), opts)
    }

    fn var(e: &Engine, name: &str) -> f64 {
        let i = e.vars.names.iter().position(|n| n == name).unwrap();
        e.sol[i]
    }

    fn close4(a: f64, b: f64) -> bool {
        ((a - b) / b).abs() < 1e-4
    }

    /// The diagonal walk on ngspice-47's schmitt.cir path (`set
    /// ngdebug` plus the iteration counts of an instrumented build):
    /// the 1e-3 step fails, then converged steps of 11, 2, 3, 4, 3 and
    /// 2 iterations.
    #[test]
    fn gmin_walk_retraces_ngspice_schmitt() {
        let mut w = GminWalk::new(1e-12, 1.00005);
        let want = [
            1.0e-3, 5.6234e-3, 2.3714e-3, 6.4938e-4, 9.3057e-5, 9.3057e-6, 9.3057e-7, 9.3057e-8,
            9.3057e-9, 9.3057e-10, 9.3057e-11, 9.3057e-12, 1.0e-12,
        ];
        let iters = [0, 11, 2, 3, 4, 3, 2, 2, 2, 2, 2, 2, 2];
        for (k, (&g, &it)) in want.iter().zip(&iters).enumerate() {
            assert!(close4(w.gmin, g), "step {k}: {} vs {g}", w.gmin);
            let done = if k == 0 {
                assert!(!w.failed());
                false
            } else {
                w.converged(it, 50)
            };
            assert_eq!(done, k == want.len() - 1, "step {k}");
        }
    }

    /// Every step failing (offbjt.cir in ngspice-47): the factor backs
    /// off to its 4th root until it is below 1.00005 — nine tries.
    #[test]
    fn gmin_walk_gives_up_like_ngspice() {
        let mut w = GminWalk::new(1e-12, 1.00005);
        let want = [
            1.0e-3, 5.6234e-3, 8.6596e-3, 9.6466e-3, 9.9105e-3, 9.9775e-3, 9.9944e-3, 9.9986e-3,
            9.9996e-3,
        ];
        for (k, &g) in want.iter().enumerate() {
            assert!(close4(w.gmin, g), "try {k}: {} vs {g}", w.gmin);
            assert_eq!(w.failed(), k == want.len() - 1, "try {k}");
        }
    }

    /// A slow step (> ¾·itl2 iterations) floors the factor at 1.00005
    /// for the diagonal walk and at 3 for the true one (cktop.c:223 vs
    /// cktop.c:412).
    #[test]
    fn gmin_walk_slow_floor() {
        let mut d = GminWalk::new(1e-12, 1.00005);
        let mut t = GminWalk::new(1e-12, 3.0);
        assert!(!d.converged(40, 50) && !t.converged(40, 50));
        assert!(close4(d.gmin, 1e-3 / 10f64.sqrt()));
        assert!(close4(t.gmin, 1e-3 / 10f64.sqrt()));
        for _ in 0..3 {
            d.converged(40, 50);
            t.converged(40, 50);
        }
        assert!(d.factor < 1.2, "{}", d.factor);
        assert_eq!(t.factor, 3.0);
    }

    /// Gillespie's ramp on opfall_src.cir: every step converges in 2
    /// iterations, and ngspice-47 prints "Supplies reduced to" these
    /// percentages.
    #[test]
    fn src_ramp_retraces_ngspice() {
        let want = [
            0.1, 0.2, 0.35, 0.575, 0.9125, 1.4188, 2.1781, 3.3172, 5.0258, 7.5887, 11.4330,
            17.1995, 25.8493, 38.8239, 58.2859, 87.4788, 100.0,
        ];
        let mut r = SrcRamp::new();
        for (k, &p) in want.iter().enumerate() {
            assert!(!r.finished(), "step {k}");
            assert!(
                close4(r.src_fact * 100.0, p),
                "step {k}: {}",
                r.src_fact * 100.0
            );
            r.converged(2, 50);
        }
        assert!(r.finished());
        assert_eq!(r.conv_fact, 1.0);
    }

    /// A failure re-solves at the last converged factor with the
    /// increment ÷10 (cktop.c:619-636), and a failure right at it ends
    /// the ramp.
    #[test]
    fn src_ramp_backs_off() {
        let mut r = SrcRamp::new();
        r.converged(30, 50); // neither fast nor slow: raise stays 0.001
        assert!(close4(r.src_fact, 0.002));
        assert!(!r.failed());
        assert!(close4(r.src_fact, 0.001));
        r.converged(30, 50);
        assert!(close4(r.src_fact, 0.0011));
        let mut r = SrcRamp::new();
        r.src_fact = r.conv_fact; // nothing to back off from
        assert!(r.failed());
    }

    /// `spMNA_Preorder` + `LoadGmin`: the voltage source's incidence
    /// twins take the diagonal gmin, so its node reads `V/(1+gmin)`
    /// (ngspice-47's schmitt trace: −1.6 V → −1.59105 V at 5.6234e-3).
    #[test]
    fn diag_gmin_lands_on_source_twins() {
        let (mut e, opts) = engine_for("t\nv1 1 0 -1.6\nr1 1 0 1k\n.op\n.end\n");
        e.ni_iter(&opts, InitMode::Jct, 100, 5.6234e-3, 1.0)
            .unwrap();
        assert!(
            (var(&e, "1") + 1.6 / 1.0056234).abs() < 1e-12,
            "{}",
            var(&e, "1")
        );
        e.ni_iter(&opts, InitMode::Jct, 100, 0.0, 1.0).unwrap();
        assert_eq!(var(&e, "1"), -1.6);
    }

    fn degree(deck: &str, kind: AnKind) -> (Engine, Option<OpStep>) {
        let (mut e, opts) = engine_for(deck);
        e.set_kind(kind, false);
        e.op(&opts).unwrap();
        let step = e.op_step;
        (e, step)
    }

    /// The probe decks close the operating point at the degree
    /// ngspice-47 closes them (`set ngdebug`): Gillespie source
    /// stepping for the off diode, the transient op for the off BJT
    /// behind a pulse in `.op`, and source stepping again for the same
    /// circuit's TRANOP (offbjt.cir: srcFact scales the pulse there).
    #[test]
    fn probes_close_at_the_ngspice_degree() {
        let src = include_str!("../../../corpus/regress/opfall_src.cir");
        let (e, step) = degree(src, AnKind::DcOp);
        assert_eq!(step, Some(OpStep::GillespieSrc));
        assert!((var(&e, "2") - 10.0).abs() < 1e-6);

        let tran = include_str!("../../../corpus/regress/opfall_tran.cir");
        let (e, step) = degree(tran, AnKind::DcOp);
        assert_eq!(step, Some(OpStep::OpTran));
        assert!((var(&e, "e") + 4.16e-10).abs() < 1e-12, "{}", var(&e, "e"));

        let off = include_str!("../../../corpus/regress/offbjt.cir");
        let (_, step) = degree(off, AnKind::TranOp);
        assert_eq!(step, Some(OpStep::GillespieSrc));

        // The floating off-diode chain: diagonal gmin fails at every
        // factor; which later degree closes it is decided by LU
        // roundoff in an INITFIX current test (docs/dune-core.md), so
        // only the answer is pinned here.
        let chain = include_str!("../../../corpus/regress/opfall_gmin.cir");
        let (e, step) = degree(chain, AnKind::DcOp);
        assert!(
            !matches!(step, Some(OpStep::Direct | OpStep::DynamicGmin)),
            "{step:?}"
        );
        assert!((var(&e, "3") - 1000.0).abs() < 1e-2, "{}", var(&e, "3"));
    }

    /// Without the defaults' degrees the same decks fail: the transient
    /// op is the only one closing opfall_tran (srcsteps/gminsteps 0
    /// leave just it, and it still closes).
    #[test]
    fn transient_op_alone_closes_the_off_bjt() {
        let tran = include_str!("../../../corpus/regress/opfall_tran.cir");
        let (mut e, mut opts) = engine_for(tran);
        opts.gminsteps = 0;
        opts.srcsteps = 0;
        e.op(&opts).unwrap();
        assert_eq!(e.op_step, Some(OpStep::OpTran));
    }

    #[test]
    fn hygiene_dedupe_checks_both_neighbours() {
        let breaks = [0.0, 1.0, 2.0, 10.0];
        assert!(within_radius(&breaks, 1.05, 0.1)); // after 1.0
        assert!(within_radius(&breaks, 1.95, 0.1)); // before 2.0
        assert!(within_radius(&breaks, 2.0, 0.0)); // coincident
        assert!(!within_radius(&breaks, 1.5, 0.1));
        assert!(!within_radius(&breaks, 10.2, 0.1)); // past the end
        assert!(within_radius(&breaks, 10.05, 0.1));
        assert!(!within_radius(&[], 1.0, 0.1));
    }
}
