//! The simulation engine: `NIiter` (`ni/niiter.c`), `NIconvTest`
//! (`ni/niconv.c`), the gmin/source-stepping driver `CKTop`
//! (`ckt/cktop.c`), the DC sweep `DCtrCurv` (`ckt/dctrcurv.c`) and
//! the transient loop `DCtran` (`ckt/dctran.c`) with its breakpoint
//! table (`cktsetbk.c`, `cktclrbk.c`) and LTE step control
//! (`ckttrunc.c`).

use std::collections::HashMap;

use crate::circuit::{Built, VarKind, Vars};
use crate::devices::Device;
use crate::error::Error;
use crate::integ::{Method, TerrCtx, com_cof};
use crate::linalg::{LinalgError, LinearSolver};
use crate::matrix::{AcCtx, AcceptCtx, AnKind, InitMode, LoadCtx, MatrixBuilder, Var};
use crate::options::Options;
use crate::state::States;
use num_complex::Complex64;

/// Memory guard for sweep/transient outputs: `points × vars` of f64.
const MAX_OUTPUT_VALUES: usize = 50_000_000;

pub(crate) struct Engine {
    pub vars: Vars,
    devices: Vec<Box<dyn Device>>,
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
}

impl Engine {
    pub fn new(built: Built) -> Self {
        let Built {
            vars,
            mut devices,
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
        }
    }

    pub fn set_kind(&mut self, kind: AnKind, uic: bool) {
        self.kind = kind;
        self.uic = uic;
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
            srcfact,
            noncon: false,
            states: &mut self.states,
            ag: self.ag,
            order: self.order,
            method: self.method,
            pred_xfact,
            delta: self.delta,
        };
        for d in &mut self.devices {
            d.load(&mut ctx);
        }
        for d in &mut self.devices {
            d.load_late(&mut ctx);
        }
        let noncon = ctx.noncon;

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
    fn ni_iter(
        &mut self,
        opts: &Options,
        start: InitMode,
        maxiter: usize,
        diag_gmin: f64,
        srcfact: f64,
    ) -> Result<(), Error> {
        // The UIC transient operating point: one load, no solve
        // (`niiter.c` top).
        if self.kind == AnKind::TranOp && self.uic {
            std::mem::swap(&mut self.sol, &mut self.rhs);
            self.load_all(start, opts, srcfact);
            return Ok(());
        }

        let mut mode = start;
        let mut iterno = 0usize;
        let mut ipass = false;
        let has_nodesets =
            !self.nodesets.is_empty() && matches!(self.kind, AnKind::DcOp | AnKind::DcSweep);

        loop {
            let mut noncon = self.load_all(mode, opts, srcfact);

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
                        return Ok(());
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

    /// `NIconvTest`.
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
        true
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

    /// `CKTop`.
    pub fn op(&mut self, opts: &Options) -> Result<(), Error> {
        let first = InitMode::Jct;
        let direct = if !opts.noopiter {
            self.ni_iter(opts, first, opts.itl1, 0.0, 1.0)
        } else {
            Err(Error::IterLim)
        };
        if direct.is_ok() {
            return Ok(());
        }
        let mut last = direct;

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
            last = self.ni_iter(opts, mode, opts.itl1, 0.0, 1.0);
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

    /// `DCtrCurv`: sweep one or two sources, the first innermost.
    pub fn dc(
        &mut self,
        opts: &Options,
        sweeps: &[SweepRef],
    ) -> Result<Vec<(f64, Vec<f64>)>, Error> {
        assert!(!sweeps.is_empty() && sweeps.len() <= 2);
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
    fn accept(&mut self, brk: bool) -> Result<(), Error> {
        let mut requests = Vec::new();
        let mut ctx = AcceptCtx {
            time: self.time,
            brk,
            step: self.step,
            final_time: self.final_time,
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
            self.accept(brk)?;
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
                    delta /= 10.0;
                }
                delta = f64::max(delta, 2.0 * delmin);
            } else if self.time + delta >= self.breaks[0] {
                save_delta = delta;
                delta = self.breaks[0] - self.time;
                brk = true;
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
            AcStepType::Decade => f64::exp(f64::ln(10.0) / n as f64),
            AcStepType::Octave => f64::exp(f64::ln(2.0) / n as f64),
            AcStepType::Linear => {
                // The C: delta = (stop-start)/(n-1) only when
                // n-1 > 1, else HUGE (a 2-point linear sweep yields
                // one point; quirk kept).
                if n - 1 > 1 {
                    (fstop - fstart) / (n - 1) as f64
                } else {
                    f64::MAX
                }
            }
        };

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
