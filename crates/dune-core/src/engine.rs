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
    AcCtx, AcceptCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, NoiseCtx, PzCtx, Slot, Var,
};
use crate::options::Options;
use crate::state::States;
use num_complex::Complex64;

/// Memory guard for sweep/transient outputs: `points × vars` of f64.
const MAX_OUTPUT_VALUES: usize = 50_000_000;

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
    method: Method,

    // Breakpoint table (always ≥ 2 entries, sorted).
    breaks: Vec<f64>,
    min_break: f64,
    /// A device load fault from the last `load_all` (see
    /// [`LoadCtx::fault`]).
    fault: Option<&'static str>,
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
            breaks: vec![0.0, 0.0],
            min_break: 0.0,
            fault: None,
        }
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
        let mut ctx = LoadCtx {
            values: &mut self.values,
            rhs: &mut self.rhs,
            sol: &self.sol,
            mode,
            kind: self.kind,
            uic: self.uic,
            time: self.time,
            step: self.step,
            final_time: self.final_time,
            gmin: opts.gmin,
            reltol: opts.reltol,
            abstol: opts.abstol,
            srcfact,
            noncon: false,
            states: &mut self.states,
            ag: self.ag,
            order: self.order,
            method: self.method,
            pred_xfact,
            delta: self.delta,
            fault: None,
        };
        for d in &mut self.devices {
            d.load(&mut ctx);
        }
        for d in &mut self.devices {
            d.load_late(&mut ctx);
        }
        let noncon = ctx.noncon;
        self.fault = ctx.fault;

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
        // The UIC transient operating point: one load, no solve
        // (`niiter.c` top).
        if self.kind == AnKind::TranOp && self.uic {
            std::mem::swap(&mut self.sol, &mut self.rhs);
            self.load_all(start, opts, srcfact);
            return Ok(0);
        }

        let mut mode = start;
        let mut iterno = 0usize;
        let mut ipass = false;
        let has_nodesets =
            !self.nodesets.is_empty() && matches!(self.kind, AnKind::DcOp | AnKind::DcSweep);

        loop {
            let mut noncon = self.load_all(mode, opts, srcfact);
            if let Some(f) = self.fault.take() {
                return Err(Error::Build(f.into()));
            }

            self.solver
                .factor(&self.values[1..], diag_gmin)
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

            match mode {
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
                InitMode::Jct => mode = InitMode::Fix,
                InitMode::Fix => {
                    if !noncon {
                        mode = InitMode::Float;
                    }
                    ipass = true;
                }
                InitMode::Tran | InitMode::Pred | InitMode::Smsig => mode = InitMode::Float,
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

    /// `CKTop`. Direct Newton, then gmin stepping — ngspice's
    /// dynamic (Gillespie) algorithm when `gminsteps == 1` (the
    /// ngspice default), the 3f5 fixed ladder when `gminsteps > 1`
    /// — then source stepping.
    pub fn op(&mut self, opts: &Options) -> Result<(), Error> {
        let first = InitMode::Jct;
        let direct = if !opts.noopiter {
            self.ni_iter(opts, first, opts.itl1, 0.0, 1.0).map(|_| ())
        } else {
            Err(Error::IterLim)
        };
        if direct.is_ok() {
            return Ok(());
        }
        let mut last = direct;

        if opts.gminsteps == 1 {
            last = self.dynamic_gmin(opts);
            if last.is_ok() {
                return Ok(());
            }
        }

        if opts.gminsteps > 1 {
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
                return Ok(());
            }
        }

        if opts.srcsteps > 1 {
            let mut mode = first;
            for i in 0..=opts.srcsteps {
                let srcfact = i as f64 / opts.srcsteps as f64;
                let r = self.ni_iter(opts, mode, opts.itl1, 0.0, srcfact);
                mode = InitMode::Float;
                r?; // "source stepping failed"
            }
            return Ok(()); // "Source stepping completed"
        }

        last
    }

    /// ngspice's dynamic gmin stepping (`cktop.c::dynamic_gmin`,
    /// Alan Gillespie): start from a zeroed solution with a large
    /// diagonal gmin and shrink it adaptively — the factor grows
    /// when Newton converges fast, shrinks on failure, and the
    /// state rolls back to the last good step. The 3f5 has only the
    /// fixed ladder; several classic decks (schmitt, mosamp2) need
    /// this once the NEWCONV device tests are enforced
    /// (`corpus/KNOWN_DIFFS.md`).
    fn dynamic_gmin(&mut self, opts: &Options) -> Result<(), Error> {
        /// `CKTgminFactor` (ngspice default).
        const GMIN_FACTOR: f64 = 10.0;
        self.sol.fill(0.0);
        self.states.zero0();
        let mut factor = GMIN_FACTOR;
        let mut old_gmin = 1e-2;
        let mut diag_gmin = old_gmin / factor;
        let gtarget = opts.gmin; // gshunt defaults to 0
        let mut saved_sol: Vec<f64> = vec![0.0; self.sol.len()];
        let mut saved_states = self.states.snapshot0();
        let mut mode = InitMode::Jct;
        loop {
            match self.ni_iter(opts, mode, opts.itl2, diag_gmin, 1.0) {
                Ok(iters) => {
                    mode = InitMode::Float;
                    if diag_gmin <= gtarget {
                        break;
                    }
                    saved_sol.copy_from_slice(&self.sol);
                    saved_states = self.states.snapshot0();
                    if iters <= opts.itl2 / 4 {
                        factor *= factor.sqrt();
                        if factor > GMIN_FACTOR {
                            factor = GMIN_FACTOR;
                        }
                    }
                    if iters > 3 * opts.itl2 / 4 {
                        factor = factor.sqrt();
                    }
                    old_gmin = diag_gmin;
                    if diag_gmin < factor * gtarget {
                        diag_gmin = gtarget;
                    } else {
                        diag_gmin /= factor;
                    }
                }
                // Any failure (the C only looks at the convergence
                // flag): back off the factor and roll back.
                Err(_) => {
                    if factor < 1.00005 {
                        return Err(Error::IterLim);
                    }
                    factor = factor.sqrt().sqrt();
                    diag_gmin = old_gmin / factor;
                    self.sol.copy_from_slice(&saved_sol);
                    self.states.restore0(&saved_states);
                }
            }
        }
        // `CKTdiagGmin = CKTgshunt` (0), then the full-precision
        // Newton at `iterlim`.
        self.ni_iter(opts, InitMode::Float, opts.itl1, 0.0, 1.0)
            .map(|_| ())
    }

    /// `DCtrCurv`: sweep one or two sources, the first innermost.
    pub fn dc(
        &mut self,
        opts: &Options,
        sweeps: &[SweepRef],
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
        for (s, v) in sweeps.iter().zip(&cur) {
            self.devices[s.dev_index].set_dc(*v);
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
                self.devices[sweeps[i].dev_index].set_dc(cur[i]);
                continue;
            }
            while i > 0 {
                i -= 1;
                cur[i] = sweeps[i].start;
                self.devices[sweeps[i].dev_index].set_dc(cur[i]);
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
            self.devices[sweeps[0].dev_index].set_dc(cur[0]);
        }
        Ok(rows)
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
            order: self.order,
            method: self.method,
            delta: self.delta,
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

    /// `CKTaccept`: device hooks, then merge breakpoint requests.
    fn accept(&mut self, brk: bool, first: bool, opts: &Options) -> Result<(), Error> {
        let mut requests = Vec::new();
        let mut ctx = AcceptCtx {
            time: self.time,
            first,
            sol: &self.sol,
            reltol: opts.reltol,
            abstol: opts.abstol,
            brk,
            step: self.step,
            final_time: self.final_time,
            delta_old: &self.delta_old,
            min_break: self.min_break,
            requests: &mut requests,
        };
        for d in &mut self.devices {
            d.accept(&mut ctx);
        }
        for t in requests {
            self.set_break(t)?;
        }
        Ok(())
    }

    /// `DCtran`: the transient loop. Returns `(time, solution)` rows
    /// for every accepted timepoint at or after `tstart`.
    pub fn tran(&mut self, opts: &Options, p: &TranParams) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        // Default TMAX: the 3f5 used only (tstop-tstart)/50; ngspice
        // restored the SPICE2 default min(tstep, span/50), which is
        // what bounds its point density — the oracle's grid
        // (corpus/KNOWN_DIFFS.md).
        let max_step = match p.tmax {
            Some(m) if m > 0.0 => m,
            _ => f64::min(p.tstep, (p.tstop - p.tstart) / 50.0),
        };
        self.method = opts.method;
        self.maxord = match self.method {
            Method::Trapezoidal => 2,
            Method::Gear => opts.maxord.clamp(1, 6),
        };
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
        let delmin = 1e-9 * max_step;
        let mut delta = f64::min(p.tstop / 50.0, p.tstep) / 10.0;

        // Transient operating point.
        self.set_kind(AnKind::TranOp, p.uic);
        if p.uic {
            self.ni_iter(opts, InitMode::Jct, opts.itl1, 0.0, 1.0)?;
        } else {
            self.op(opts)?;
        }

        // Switch to transient proper (`MODETRAN | MODEINITTRAN`).
        self.set_kind(AnKind::Tran, p.uic);
        self.order = 1;
        self.ag = [0.0; 7];
        self.states.copy01();
        self.delta_old = [max_step; 8];
        self.delta = delta;
        let mut save_delta = p.tstop / 50.0;
        let mut firsttime = true;
        let mut brk = true; // CKTbreak
        let mut rows: Vec<(f64, Vec<f64>)> = Vec::new();

        'next_time: loop {
            // Accept the computed point: device hooks plant
            // breakpoints, stale ones retire.
            self.accept(brk, firsttime, opts)?;
            if self.time > self.breaks[0] {
                self.clr_break();
            }
            brk = false;
            // Under UIC ngspice omits the t=0 point (the 3f5 dumped
            // the untouched zero vector there — KNOWN_DIFFS.md).
            let skip = p.uic && self.time == 0.0;
            if self.time >= p.tstart && !skip {
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
                    if p.uic {
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

            loop {
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
        };
        self.devices[d].noise_dens(&ctx)
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
                values: &mut values,
                rhs: &mut rhs,
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
                states: &mut engine.states,
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

/// One resolved `.dc` sweep level.
pub(crate) struct SweepRef {
    pub dev_index: usize,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

/// `.tran` parameters after validation.
pub(crate) struct TranParams {
    pub tstep: f64,
    pub tstop: f64,
    pub tstart: f64,
    pub tmax: Option<f64>,
    pub uic: bool,
}
