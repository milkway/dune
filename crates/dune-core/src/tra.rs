//! Ideal (lossless) transmission line, the 3f5 `TRA` device
//! (`dev/tra/trasetup.c`, `tratemp.c`, `traload.c`, `traacct.c`,
//! `tratrunc.c`, `traacld.c`, `traparam.c`).
//!
//! Method of characteristics: each port is a resistor `Z0` from the
//! positive terminal to an internal node `int`, then a voltage source
//! from `int` to the negative terminal whose value is the wave
//! arriving from the other end one delay `TD` earlier:
//!
//! ```text
//! v(int1) − v(N2) = v2(t−TD) + Z0·i2(t−TD)      (branch #i1)
//! v(int2) − v(N4) = v1(t−TD) + Z0·i1(t−TD)      (branch #i2)
//! ```
//!
//! with `i1`/`i2` the currents entering the ports at `N1`/`N3`. Four
//! extra unknowns per line, made with `CKTmkVolt` in this order:
//! `<name>#i1`, `<name>#i2`, `<name>#int1`, `<name>#int2` (all
//! voltage-typed, all written to the rawfile).
//!
//! - DC (`MODEDC`): the sources read the *current* opposite port,
//!   `v(int1) − v(N2) = v2 + (1−GMIN)·Z0·i2` and its mirror — a short
//!   between the ports up to the `(1−GMIN)` factor.
//! - Transient: the excitations `TRAinput1/2` are formed on the first
//!   Newton iteration of every timepoint (`MODEINITPRED`) by
//!   three-point Lagrange interpolation in the instance's own table
//!   of accepted waves (`TRAdelays`); `MODEINITTRAN` seeds the table
//!   with the operating point (or the `IC=` values under UIC) at
//!   `−2TD`, `−TD`, `0`.
//! - AC: `V(int1) − V(N2) = e^{−jωTD}·(V2 + Z0·I2)` and mirror.
//!
//! Timestep control: `TRAaccept` appends each accepted point that is
//! more than `CKTminBreak` past the last one, drops entries older
//! than `t − TD` (keeping two), and when the slope of either wave
//! changes (`|d1−d2| ≥ rel·max(|d1|,|d2|) + abs`, slopes over
//! `CKTdeltaOld[0..2]`) plants a breakpoint one delay after the
//! previous point; `TRAtrunc` caps the step so the line does not
//! step past it. `rel`/`abs` are output-only in the parameter table
//! (`tra.c`), so both stay at their `TRAsetup` default of 1.
//!
//! ngspice-47 differences followed here: `TRAaccept` plants the
//! breakpoint only when it lies in the future (the 3f5 called
//! `CKTsetBreak` unconditionally, which fails with "breakpoint in the
//! past" whenever the step exceeds `TD`).

use crate::devices::Device;
use crate::disto::DistoSetupCtx;
use crate::integ::TerrCtx;
use crate::matrix::{
    AcCtx, AcceptCtx, InitMode, LoadCtx, MatrixBuilder, PeriodicPast, Slot, TRASH, Var,
};

/// `TRAsetup` defaults for the output-only breakpoint tolerances
/// (`TRAreltol`, `TRAabstol`).
const TRA_RELTOL: f64 = 1.0;
const TRA_ABSTOL: f64 = 1.0;

/// Resolved instance parameters (`TRAsetup` + `TRAtemp`).
#[derive(Clone, Debug)]
pub(crate) struct TraParams {
    /// `TRAimped`.
    pub z0: f64,
    /// `TRAconduct = 1/Z0`.
    pub g: f64,
    /// `TRAtd`: given, or `NL/F`.
    pub td: f64,
    /// `TRAinitVolt1`, `TRAinitCur1`, `TRAinitVolt2`, `TRAinitCur2`.
    pub ic: [f64; 4],
}

impl TraParams {
    /// `TRAtemp`: `TD = NL/F` unless given (`NL` defaults to 0.25,
    /// `F` to 1 GHz in `TRAsetup`). The C accepts any value; a zero,
    /// negative or non-finite `Z0`/`TD` would only produce NaNs or an
    /// empty delay table, so it is rejected here.
    pub fn new(
        name: &str,
        z0: f64,
        td: Option<f64>,
        f: Option<f64>,
        nl: Option<f64>,
        ic: [f64; 4],
    ) -> Result<Self, String> {
        if !(z0.is_finite() && z0 != 0.0) {
            return Err(format!("{name}: transmission line z0 must be nonzero"));
        }
        let td = match td {
            Some(td) => td,
            None => nl.unwrap_or(0.25) / f.unwrap_or(1e9),
        };
        if !(td.is_finite() && td > 0.0) {
            return Err(format!(
                "{name}: transmission line delay must be positive (TD = {td:e})"
            ));
        }
        if ic.iter().any(|v| !v.is_finite()) {
            return Err(format!("{name}: non-finite initial condition"));
        }
        Ok(TraParams {
            z0,
            g: 1.0 / z0,
            td,
            ic,
        })
    }
}

/// Matrix positions of `TRAsetup` (`TSTALLOC` list).
#[derive(Clone, Copy)]
struct Slots {
    ibr1_ibr2: Slot,
    ibr1_int1: Slot,
    ibr1_neg1: Slot,
    ibr1_neg2: Slot,
    ibr1_pos2: Slot,
    ibr2_ibr1: Slot,
    ibr2_int2: Slot,
    ibr2_neg1: Slot,
    ibr2_neg2: Slot,
    ibr2_pos1: Slot,
    int1_ibr1: Slot,
    int1_int1: Slot,
    int1_pos1: Slot,
    int2_ibr2: Slot,
    int2_int2: Slot,
    int2_pos2: Slot,
    neg1_ibr1: Slot,
    neg2_ibr2: Slot,
    pos1_int1: Slot,
    pos1_pos1: Slot,
    pos2_int2: Slot,
    pos2_pos2: Slot,
}

const NO_SLOTS: Slots = Slots {
    ibr1_ibr2: TRASH,
    ibr1_int1: TRASH,
    ibr1_neg1: TRASH,
    ibr1_neg2: TRASH,
    ibr1_pos2: TRASH,
    ibr2_ibr1: TRASH,
    ibr2_int2: TRASH,
    ibr2_neg1: TRASH,
    ibr2_neg2: TRASH,
    ibr2_pos1: TRASH,
    int1_ibr1: TRASH,
    int1_int1: TRASH,
    int1_pos1: TRASH,
    int2_ibr2: TRASH,
    int2_int2: TRASH,
    int2_pos2: TRASH,
    neg1_ibr1: TRASH,
    neg2_ibr2: TRASH,
    pos1_int1: TRASH,
    pos1_pos1: TRASH,
    pos2_int2: TRASH,
    pos2_pos2: TRASH,
};

/// One `TRAdelays` triple: an accepted time and the two waves
/// leaving the line at that time — `input1` (from port 2,
/// `v2 + Z0·i2`, what port 1 sees `TD` later) and `input2` (from
/// port 1).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Entry {
    t: f64,
    in1: f64,
    in2: f64,
}

/// The Lagrange weights of `TRAload`'s `MODEINITPRED` branch for the
/// value at `x` through `(t1, t2, t3)`, in the C's operation order.
/// `None` when two abscissae coincide (the C then `continue`s past
/// the instance, leaving its RHS unloaded for that iteration).
fn lagrange3(x: f64, t1: f64, t2: f64, t3: f64) -> Option<(f64, f64, f64)> {
    if (t2 - t1) == 0.0 || (t3 - t2) == 0.0 {
        return None;
    }
    let mut f1 = (x - t2) * (x - t3);
    let mut f2 = (x - t1) * (x - t3);
    let mut f3 = (x - t1) * (x - t2);
    f1 /= t1 - t2;
    f2 /= t2 - t1;
    f2 /= t2 - t3;
    f3 /= t2 - t3;
    if (t3 - t1) == 0.0 {
        f1 = 0.0;
        f2 = 0.0;
    } else {
        f1 /= t1 - t3;
        f3 /= t1 - t3;
    }
    Some((f1, f2, f3))
}

/// The slope-change test shared by `TRAaccept` and `TRAtrunc`.
fn slope_changed(d1: f64, d2: f64) -> bool {
    (d1 - d2).abs() >= TRA_RELTOL * f64::max(d1.abs(), d2.abs()) + TRA_ABSTOL
}

/// One `T` line.
pub(crate) struct TraI {
    p: TraParams,
    pos1: Var,
    neg1: Var,
    pos2: Var,
    neg2: Var,
    int1: Var,
    int2: Var,
    br1: Var,
    br2: Var,
    slots: Slots,
    /// `TRAinput1/2`.
    input1: f64,
    input2: f64,
    /// `TRAdelays[0..=TRAsizeDelay]`; empty until the first
    /// transient load (the C's freshly allocated, zeroed table).
    delays: Vec<Entry>,
    /// `.pss` (issue #20): the table was seeded from the periodic past
    /// at the first accept, so `MODEINITTRAN` reads it like any later
    /// timepoint instead of seeding the operating point.
    periodic: bool,
}

impl TraI {
    /// `nodes` = (P1+, P1−, P2+, P2−); `br` = `#i1`/`#i2`; `int` =
    /// `#int1`/`#int2`.
    pub fn new(p: TraParams, nodes: [Var; 4], br: [Var; 2], int: [Var; 2]) -> Self {
        TraI {
            p,
            pos1: nodes[0],
            neg1: nodes[1],
            pos2: nodes[2],
            neg2: nodes[3],
            int1: int[0],
            int2: int[1],
            br1: br[0],
            br2: br[1],
            slots: NO_SLOTS,
            input1: 0.0,
            input2: 0.0,
            delays: Vec::new(),
            periodic: false,
        }
    }

    /// The two outgoing waves at a solution: (`v2 + Z0·i2`,
    /// `v1 + Z0·i1`).
    fn waves(&self, sol: &[f64]) -> (f64, f64) {
        let z0 = self.p.z0;
        (
            (sol[self.pos2] - sol[self.neg2]) + sol[self.br2] * z0,
            (sol[self.pos1] - sol[self.neg1]) + sol[self.br1] * z0,
        )
    }

    /// `MODEINITPRED`: interpolate the waves at `time − TD`. `None`
    /// is the C's `continue` on coincident abscissae.
    fn predict(&self, time: f64) -> Option<(f64, f64)> {
        let size = self.delays.len().checked_sub(1)?;
        if size < 2 {
            return None;
        }
        let target = time - self.p.td;
        let mut i = 2;
        while i < size && self.delays[i].t <= target {
            i += 1;
        }
        let (a, b, c) = (self.delays[i - 2], self.delays[i - 1], self.delays[i]);
        let (f1, f2, f3) = lagrange3(target, a.t, b.t, c.t)?;
        Some((
            f1 * a.in1 + f2 * b.in1 + f3 * c.in1,
            f1 * a.in2 + f2 * b.in2 + f3 * c.in2,
        ))
    }

    /// `.pss` (issue #20): the delay table of a shooting interval
    /// starting at `ctx.time = t₀` is the periodic past — the orbit's
    /// waves over `[t₀ − TD, t₀)` (whole periods back from `t₀`, so
    /// the oldest entry is at or before `t₀ − TD`) and the shooting
    /// state's at `t₀`. The breakpoints `TRAaccept` would have
    /// planted one delay after every slope change of that past are
    /// planted too, where they fall inside the interval.
    fn seed_periodic(&mut self, past: &PeriodicPast, ctx: &mut AcceptCtx) {
        let t0 = ctx.time;
        let td = self.p.td;
        // Without the samples (a delay of more periods than the guard
        // takes) the table stays empty; `.pss` refuses such lines first.
        self.delays.clear();
        self.periodic = true;
        let Some(samples) = past.samples(t0, t0 - td, ctx.sol) else {
            return; // the empty table faults at the first load
        };
        for (t, x) in samples.into_iter().chain([(t0, ctx.sol)]) {
            let (in1, in2) = self.waves(x);
            self.delays.push(Entry { t, in1, in2 });
        }
        for w in self.delays.windows(3) {
            let (e2, e1, e0) = (w[0], w[1], w[2]);
            let (h0, h1) = (e0.t - e1.t, e1.t - e2.t);
            let d1 = (e0.in1 - e1.in1) / h0;
            let d2 = (e1.in1 - e2.in1) / h1;
            let d3 = (e0.in2 - e1.in2) / h0;
            let d4 = (e1.in2 - e2.in2) / h1;
            let when = e1.t + td;
            if (slope_changed(d1, d2) || slope_changed(d3, d4))
                && when > t0 + ctx.min_break
                && when < ctx.final_time - ctx.min_break
            {
                ctx.set_break(when);
            }
        }
    }
}

impl Device for TraI {
    fn par_load(&self) -> bool {
        true
    }

    /// `TRAinfo` has no `DEVdisto`: the line is linear.
    fn disto_setup(&mut self, _ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        Ok(())
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let (p1, n1, p2, n2) = (self.pos1, self.neg1, self.pos2, self.neg2);
        let (i1, i2, b1, b2) = (self.int1, self.int2, self.br1, self.br2);
        self.slots = Slots {
            ibr1_ibr2: mb.slot(b1, b2),
            ibr1_int1: mb.slot(b1, i1),
            ibr1_neg1: mb.slot(b1, n1),
            ibr1_neg2: mb.slot(b1, n2),
            ibr1_pos2: mb.slot(b1, p2),
            ibr2_ibr1: mb.slot(b2, b1),
            ibr2_int2: mb.slot(b2, i2),
            ibr2_neg1: mb.slot(b2, n1),
            ibr2_neg2: mb.slot(b2, n2),
            ibr2_pos1: mb.slot(b2, p1),
            int1_ibr1: mb.slot(i1, b1),
            int1_int1: mb.slot(i1, i1),
            int1_pos1: mb.slot(i1, p1),
            int2_ibr2: mb.slot(i2, b2),
            int2_int2: mb.slot(i2, i2),
            int2_pos2: mb.slot(i2, p2),
            neg1_ibr1: mb.slot(n1, b1),
            neg2_ibr2: mb.slot(n2, b2),
            pos1_int1: mb.slot(p1, i1),
            pos1_pos1: mb.slot(p1, p1),
            pos2_int2: mb.slot(p2, i2),
            pos2_pos2: mb.slot(p2, p2),
        };
    }

    /// `TRAload`.
    fn load(&mut self, ctx: &mut LoadCtx) {
        let s = self.slots;
        let g = self.p.g;
        ctx.add(s.pos1_pos1, g);
        ctx.add(s.pos1_int1, -g);
        ctx.add(s.neg1_ibr1, -1.0);
        ctx.add(s.pos2_pos2, g);
        ctx.add(s.neg2_ibr2, -1.0);
        ctx.add(s.int1_pos1, -g);
        ctx.add(s.int1_int1, g);
        ctx.add(s.int1_ibr1, 1.0);
        ctx.add(s.int2_int2, g);
        ctx.add(s.int2_ibr2, 1.0);
        ctx.add(s.ibr1_neg1, -1.0);
        ctx.add(s.ibr1_int1, 1.0);
        ctx.add(s.ibr2_neg2, -1.0);
        ctx.add(s.ibr2_int2, 1.0);
        ctx.add(s.pos2_int2, -g);
        ctx.add(s.int2_pos2, -g);

        if ctx.is_dc() {
            let zg = (1.0 - ctx.gmin) * self.p.z0;
            ctx.add(s.ibr1_pos2, -1.0);
            ctx.add(s.ibr1_neg2, 1.0);
            ctx.add(s.ibr1_ibr2, -zg);
            ctx.add(s.ibr2_pos1, -1.0);
            ctx.add(s.ibr2_neg1, 1.0);
            ctx.add(s.ibr2_ibr1, -zg);
            return;
        }

        match ctx.mode {
            // `.pss`: the table holds the periodic past already.
            InitMode::Tran | InitMode::Pred if self.periodic => match self.predict(ctx.time) {
                Some((a, b)) => {
                    self.input1 = a;
                    self.input2 = b;
                }
                None if self.delays.is_empty() => {
                    ctx.fault = Some("TRA: the delay spans too many periods for .pss");
                    return;
                }
                None => return,
            },
            InitMode::Tran => {
                if ctx.uic {
                    let [v1, i1, v2, i2] = self.p.ic;
                    self.input1 = v2 + i2 * self.p.z0;
                    self.input2 = v1 + i1 * self.p.z0;
                } else {
                    (self.input1, self.input2) = self.waves(ctx.sol);
                }
                let td = self.p.td;
                let e = |t| Entry {
                    t,
                    in1: self.input1,
                    in2: self.input2,
                };
                self.delays = vec![e(-2.0 * td), e(-td), e(0.0)];
            }
            InitMode::Pred => match self.predict(ctx.time) {
                Some((a, b)) => {
                    self.input1 = a;
                    self.input2 = b;
                }
                // The C's `continue`: no RHS for this instance.
                None => return,
            },
            _ => {}
        }
        ctx.add_rhs(self.br1, self.input1);
        ctx.add_rhs(self.br2, self.input2);
    }

    /// `TRAtrunc`: when a wave's slope changes, do not step past the
    /// last stored point plus `TD`. The slopes divide by
    /// `CKTdeltaOld[1]`/`[2]` although the newest difference spans
    /// the step being checked (`CKTdeltaOld[0]`) — kept from the C.
    /// (`step_old`: the accepted steps, which the table follows.)
    fn trunc(&self, t: &TerrCtx, time_step: &mut f64) {
        let Some(size) = self.delays.len().checked_sub(1) else {
            return;
        };
        if size < 1 {
            return;
        }
        let (v1, v4) = self.waves(t.sol);
        let (last, prev) = (self.delays[size], self.delays[size - 1]);
        let d1 = (v1 - last.in1) / t.step_old[1];
        let d2 = (last.in1 - prev.in1) / t.step_old[2];
        let d3 = (v4 - last.in2) / t.step_old[1];
        let d4 = (last.in2 - prev.in2) / t.step_old[2];
        if slope_changed(d1, d2) || slope_changed(d3, d4) {
            let tmp = last.t + self.p.td - t.time;
            *time_step = time_step.min(tmp);
        }
    }

    /// `TRAacLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let s = self.slots;
        let g = self.p.g;
        let z0 = self.p.z0;
        let real = (-ctx.omega * self.p.td).cos();
        let imag = (-ctx.omega * self.p.td).sin();
        ctx.add_g(s.pos1_pos1, g);
        ctx.add_g(s.pos1_int1, -g);
        ctx.add_g(s.neg1_ibr1, -1.0);
        ctx.add_g(s.pos2_pos2, g);
        ctx.add_g(s.neg2_ibr2, -1.0);
        ctx.add_g(s.int1_pos1, -g);
        ctx.add_g(s.int1_int1, g);
        ctx.add_g(s.int1_ibr1, 1.0);
        ctx.add_g(s.int2_int2, g);
        ctx.add_g(s.int2_ibr2, 1.0);
        ctx.add_g(s.ibr1_neg1, -1.0);
        ctx.add_g(s.ibr1_pos2, -real);
        ctx.add_b(s.ibr1_pos2, -imag);
        ctx.add_g(s.ibr1_neg2, real);
        ctx.add_b(s.ibr1_neg2, imag);
        ctx.add_g(s.ibr1_int1, 1.0);
        ctx.add_g(s.ibr1_ibr2, -real * z0);
        ctx.add_b(s.ibr1_ibr2, -imag * z0);
        ctx.add_g(s.ibr2_pos1, -real);
        ctx.add_b(s.ibr2_pos1, -imag);
        ctx.add_g(s.ibr2_neg1, real);
        ctx.add_b(s.ibr2_neg1, imag);
        ctx.add_g(s.ibr2_neg2, -1.0);
        ctx.add_g(s.ibr2_int2, 1.0);
        ctx.add_g(s.ibr2_ibr1, -real * z0);
        ctx.add_b(s.ibr2_ibr1, -imag * z0);
        ctx.add_g(s.pos2_int2, -g);
        ctx.add_g(s.int2_pos2, -g);
    }

    /// `TRAaccept` in its ngspice-47 form.
    fn accept(&mut self, ctx: &mut AcceptCtx) {
        // The accept of the transient operating point runs before
        // the first `MODEINITTRAN` load: the C table is still the
        // zeroed allocation (`TRAsizeDelay == 0`), on which neither
        // the shift (`0 − TD > 0`) nor the append (`0 − 0 >
        // CKTminBreak`) fires.
        if ctx.first {
            self.periodic = false;
            if let Some(past) = ctx.periodic {
                self.seed_periodic(past, ctx);
            }
            return;
        }
        if self.delays.len() < 3 {
            return;
        }
        let td = self.p.td;
        // Shift: keep two entries at or before `t − TD`.
        if ctx.time - td > self.delays[2].t {
            let size = self.delays.len() - 1;
            let mut i = 2;
            while i < size && ctx.time - td > self.delays[i].t {
                i += 1;
            }
            self.delays.drain(0..i - 2);
        }
        let size = self.delays.len() - 1;
        if ctx.time - self.delays[size].t > ctx.min_break {
            let (in1, in2) = self.waves(ctx.sol);
            self.delays.push(Entry {
                t: ctx.time,
                in1,
                in2,
            });
            let size = size + 1;
            let (e0, e1, e2) = (
                self.delays[size],
                self.delays[size - 1],
                self.delays[size - 2],
            );
            let d1 = (e0.in1 - e1.in1) / ctx.delta_old[0];
            let d2 = (e1.in1 - e2.in1) / ctx.delta_old[1];
            let d3 = (e0.in2 - e1.in2) / ctx.delta_old[0];
            let d4 = (e1.in2 - e2.in2) / ctx.delta_old[1];
            if slope_changed(d1, d2) || slope_changed(d3, d4) {
                // "Derivative changed - need to schedule after
                // delay"; ngspice only when it lies ahead.
                let when = e1.t + td;
                if when > ctx.time {
                    ctx.set_break(when);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(td: f64) -> TraParams {
        TraParams::new("t1", 50.0, Some(td), None, None, [0.0; 4]).unwrap()
    }

    #[test]
    fn lagrange_weights_reproduce_quadratics() {
        let q = |x: f64| 3.0 * x * x - 2.0 * x + 0.5;
        let (t1, t2, t3) = (-1.5, 0.25, 2.0);
        for &x in &[-1.0, 0.0, 0.7, 1.9, 3.5] {
            let (f1, f2, f3) = lagrange3(x, t1, t2, t3).unwrap();
            let v = f1 * q(t1) + f2 * q(t2) + f3 * q(t3);
            assert!((v - q(x)).abs() < 1e-12, "x={x}: {v} vs {}", q(x));
        }
        assert!(lagrange3(0.0, 1.0, 1.0, 2.0).is_none());
        assert!(lagrange3(0.0, 1.0, 2.0, 2.0).is_none());
    }

    /// The delayed wave is read back from the table at `t − TD`:
    /// exact on the stored samples and quadratic in between.
    #[test]
    fn prediction_reads_the_table_one_delay_back() {
        let td = 1e-9;
        let mut tra = TraI::new(params(td), [1, 0, 2, 0], [3, 4], [5, 6]);
        let wave = |t: f64| 1e18 * t * t + 2e9 * t + 1.0;
        tra.delays = (0..8)
            .map(|k| {
                let t = (k as f64 - 2.0) * 0.4e-9;
                Entry {
                    t,
                    in1: wave(t),
                    in2: -wave(t),
                }
            })
            .collect();
        for &t in &[1.0e-9, 1.3e-9, 1.8e-9, 2.6e-9] {
            let (a, b) = tra.predict(t).unwrap();
            let want = wave(t - td);
            assert!((a - want).abs() < 1e-9 * want.abs(), "t={t}: {a} vs {want}");
            assert!((b + want).abs() < 1e-9 * want.abs());
        }
        // The operating-point seed (three equal samples) predicts the
        // constant.
        tra.delays = vec![
            Entry {
                t: -2.0 * td,
                in1: 0.7,
                in2: 0.2,
            },
            Entry {
                t: -td,
                in1: 0.7,
                in2: 0.2,
            },
            Entry {
                t: 0.0,
                in1: 0.7,
                in2: 0.2,
            },
        ];
        let (a, b) = tra.predict(0.3e-9).unwrap();
        assert!((a - 0.7).abs() < 1e-15 && (b - 0.2).abs() < 1e-15);
    }

    fn matched_line(wave: Option<crate::srcfn::Wave>) -> crate::circuit::Circuit {
        use crate::circuit::{Circuit, Resistor, Tra, Vsource};
        let mut ckt = Circuit::new("matched line");
        ckt.add(Vsource {
            name: "v1".into(),
            nodes: ["1".into(), "0".into()],
            dc: wave.is_none().then_some(1.0),
            wave,
            ac: None,
            distof1: None,
            distof2: None,
            port: None,
        })
        .unwrap();
        for (name, a, b) in [("rs", "1", "2"), ("rl", "3", "0")] {
            ckt.add(Resistor {
                name: name.into(),
                nodes: [a.into(), b.into()],
                value: 50.0,
            })
            .unwrap();
        }
        ckt.add(Tra {
            name: "t1".into(),
            nodes: ["2".into(), "0".into(), "3".into(), "0".into()],
            z0: 50.0,
            td: Some(5e-9),
            f: None,
            nl: None,
            ic: [0.0; 4],
        })
        .unwrap();
        ckt
    }

    /// The `MODEDC` stamps: across the line `v1 = v2` and `i1 = −i2`
    /// up to the `(1 − GMIN)` factor, so a 1 V source behind 50 Ω
    /// into a 50 Ω load reads 0.5 V at both ends.
    #[test]
    fn dc_form_is_a_short_between_the_ports() {
        let ckt = matched_line(None);
        let op = ckt.op(&crate::options::Options::default()).unwrap();
        let get = |n: &str| op.solution.column(n).unwrap()[0];
        assert!((get("v(2)") - 0.5).abs() < 1e-9, "{}", get("v(2)"));
        assert!((get("v(3)") - 0.5).abs() < 1e-9, "{}", get("v(3)"));
        // Port currents enter at N1/N3: 10 mA in at port 1, out at 2.
        assert!((get("v(t1#i1)") - 0.01).abs() < 1e-12);
        assert!((get("v(t1#i1)") + get("v(t1#i2)")).abs() < 1e-12);
        // int1 sits Z0·i1 below port 1.
        assert!(get("v(t1#int1)").abs() < 1e-9, "{}", get("v(t1#int1)"));
    }

    #[test]
    fn matched_line_delays_by_td() {
        let ckt = matched_line(Some(crate::srcfn::Wave::Pulse(vec![
            0.0, 1.0, 1e-9, 1e-9, 1e-9, 100e-9, 200e-9,
        ])));
        let opts = crate::options::Options::default();
        let tr = ckt.tran(&opts, 0.1e-9, 20e-9, 0.0, None, false).unwrap();
        let t = tr.solution.column("time").unwrap();
        let v2 = tr.solution.column("v(2)").unwrap();
        let v3 = tr.solution.column("v(3)").unwrap();
        let at = |col: &[f64], when: f64| {
            let k = t.iter().position(|&x| x >= when).unwrap();
            let f = (when - t[k - 1]) / (t[k] - t[k - 1]);
            col[k - 1] + f * (col[k] - col[k - 1])
        };
        assert!(at(v3, 5.5e-9).abs() < 1e-9, "{}", at(v3, 5.5e-9));
        assert!((at(v2, 1.5e-9) - 0.25).abs() < 2e-3, "{}", at(v2, 1.5e-9));
        assert!((at(v3, 6.5e-9) - 0.25).abs() < 2e-3, "{}", at(v3, 6.5e-9));
        assert!((at(v3, 12e-9) - 0.5).abs() < 1e-9);
        assert!((at(v2, 12e-9) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn delay_from_frequency_and_normalized_length() {
        let p = TraParams::new("t", 50.0, None, Some(100e6), Some(0.5), [0.0; 4]).unwrap();
        assert!((p.td - 5e-9).abs() < 1e-21);
        let p = TraParams::new("t", 50.0, None, None, None, [0.0; 4]).unwrap();
        assert!((p.td - 0.25e-9).abs() < 1e-21);
        assert!(TraParams::new("t", 0.0, Some(1e-9), None, None, [0.0; 4]).is_err());
        assert!(TraParams::new("t", 50.0, Some(0.0), None, None, [0.0; 4]).is_err());
    }
}
