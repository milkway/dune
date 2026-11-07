//! Switches — the voltage-controlled `S` (`dev/sw/`) and the
//! current-controlled `W` (`dev/csw/`) of the 3f5, in the form
//! ngspice-47 runs them (`swload.c`/`cswload.c` "Modified: 2001 Jon
//! Engelbert", plus `swtrunc.c`). The switch is a conductance `GON`
//! or `GOFF` between its two nodes; which one is a *state variable*
//! (`SWswitchstate`), decided anew in every load from the control
//! value read off the previous Newton iterate (`CKTrhsOld`) and kept
//! across the `INITF` ladder and from one timepoint to the next.
//!
//! ngspice widens the 3f5's on/off state to four values — off and
//! on outside the hysteresis window (`REALLY_OFF = 0`,
//! `REALLY_ON = 1`) and inside it (`HYST_OFF = 2`, `HYST_ON = 3`) —
//! keeps the sign of `VH`/`IH` (the 3f5 takes `|VH|`; a non-positive
//! hysteresis runs a separate state machine) and adds a timestep
//! limit for the voltage switch as the control approaches the
//! threshold (`SWtrunc`). The two loads are not quite the same code:
//! inside the window `CSWload`'s `INITFLOAT` keeps the state of the
//! last *accepted* point (`CKTstate1`) where `SWload` keeps the
//! current iterate's (`CKTstate0`), and `CSWload` never records its
//! control value, so `CSWtrunc` is inert. Both are ported as ngspice
//! has them (corpus/KNOWN_DIFFS.md).

use crate::integ::TerrCtx;
use crate::matrix::{AcCtx, InitMode, LoadCtx, MatrixBuilder, NoiseCtx, PzCtx, Slot, TRASH, Var};

/// `SW_NUM_STATES`/`CSW_NUM_STATES` (ngspice): switch state, control
/// value.
pub(crate) const SW_NUM_STATES: usize = 2;
const SWITCHSTATE: usize = 0;
const CTRLVALUE: usize = 1;

/// `REALLY_OFF`, `REALLY_ON`, `HYST_OFF`, `HYST_ON` (ngspice
/// `swload.c`), stored as doubles in the state vector like the C.
const REALLY_OFF: f64 = 0.0;
const REALLY_ON: f64 = 1.0;
const HYST_OFF: f64 = 2.0;
const HYST_ON: f64 = 3.0;

/// `.model … sw(…)` parameters, 3f5 names (`sw.c` model table) and
/// the defaults of `SWsetup`.
#[derive(Clone, Debug, Default)]
pub struct SwModel {
    /// `VT`: threshold voltage, default 0.
    pub vt: f64,
    /// `VH`: hysteresis voltage, default 0. Kept signed (ngspice; the
    /// 3f5 stores `|VH|`).
    pub vh: f64,
    /// `RON`: closed resistance, default 1 Ω (`SW_ON_CONDUCTANCE`).
    pub ron: Option<f64>,
    /// `ROFF`: open resistance, default `1/GMIN`
    /// (`SW_OFF_CONDUCTANCE` is `CKTgmin`).
    pub roff: Option<f64>,
}

/// `.model … csw(…)` parameters, 3f5 names (`csw.c` model table) and
/// the defaults of `CSWsetup`.
#[derive(Clone, Debug, Default)]
pub struct CswModel {
    /// `IT`: threshold current, default 0.
    pub it: f64,
    /// `IH`: hysteresis current, default 0. Kept signed (ngspice; the
    /// 3f5 stores `|IH|`).
    pub ih: f64,
    /// `RON`: closed resistance, default 1 Ω.
    pub ron: Option<f64>,
    /// `ROFF`: open resistance, default `1/GMIN`.
    pub roff: Option<f64>,
}

/// What a switch watches.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Control {
    /// `S`: the voltage between two nodes.
    Voltage { pos: Var, neg: Var },
    /// `W`: the branch current of a voltage source.
    Current { branch: Var },
}

/// The model-level constants a switch instance needs after
/// `SWsetup`/`CSWsetup` defaulting.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SwitchParams {
    pub thresh: f64,
    pub hyst: f64,
    pub gon: f64,
    pub goff: f64,
}

impl SwitchParams {
    /// `SWsetup`/`CSWsetup` defaults: `GON = 1`, `GOFF = gmin`.
    pub fn new(thresh: f64, hyst: f64, ron: Option<f64>, roff: Option<f64>, gmin: f64) -> Self {
        SwitchParams {
            thresh,
            hyst,
            gon: ron.map_or(1.0, |r| 1.0 / r),
            goff: roff.map_or(gmin, |r| 1.0 / r),
        }
    }
}

/// `SWinstance`/`CSWinstance`.
pub(crate) struct SwitchInst {
    p: SwitchParams,
    pos: Var,
    neg: Var,
    ctrl: Control,
    /// `SWzero_stateGiven`: the `ON` hint (`OFF` or nothing is false).
    on: bool,
    st: usize,
    slots: [Slot; 4],
}

/// Which state an `INITJCT`/`INITFIX` load picks: the hint, refined
/// to "really" on/off when the control is already past the window
/// on the hinted side (both `SWload` and `CSWload`; note the `>= 0`
/// here against the `> 0` of the later modes).
fn init_state(p: &SwitchParams, on: bool, c: f64) -> f64 {
    if on {
        if (p.hyst >= 0.0 && c > p.thresh + p.hyst) || (p.hyst < 0.0 && c > p.thresh - p.hyst) {
            REALLY_ON
        } else {
            HYST_ON
        }
    } else if (p.hyst >= 0.0 && c < p.thresh - p.hyst) || (p.hyst < 0.0 && c < p.thresh + p.hyst) {
        REALLY_OFF
    } else {
        HYST_OFF
    }
}

/// The `INITFLOAT` decision. `keep` is what a positive-hysteresis
/// switch holds inside the window (`SWload`: this iterate's state0;
/// `CSWload`: the last accepted point's state1); `prev` is state1.
fn float_state(p: &SwitchParams, c: f64, keep: f64, prev: f64) -> f64 {
    if p.hyst > 0.0 {
        if c > p.thresh + p.hyst {
            REALLY_ON
        } else if c < p.thresh - p.hyst {
            REALLY_OFF
        } else {
            keep
        }
    } else if c > p.thresh - p.hyst {
        REALLY_ON
    } else if c < p.thresh + p.hyst {
        REALLY_OFF
    } else if prev == HYST_OFF || prev == HYST_ON {
        prev
    } else if prev == REALLY_ON {
        HYST_OFF
    } else if prev == REALLY_OFF {
        HYST_ON
    } else {
        // `internalerror("bad value for previous state")` only
        // prints; the C's `current_state` keeps its -1 initializer.
        -1.0
    }
}

/// The `INITTRAN`/`INITPRED` decision from the last accepted state
/// `prev`. Inside a non-positive window the two loads differ:
/// `SWload` flips to the opposite *really* state (and falls back to
/// 0), `CSWload` repeats its `INITFLOAT` table.
fn tran_state(p: &SwitchParams, c: f64, prev: f64, current_ctrl: bool) -> f64 {
    if p.hyst > 0.0 {
        if c > p.thresh + p.hyst {
            REALLY_ON
        } else if c < p.thresh - p.hyst {
            REALLY_OFF
        } else {
            prev
        }
    } else if c > p.thresh - p.hyst {
        REALLY_ON
    } else if c < p.thresh + p.hyst {
        REALLY_OFF
    } else if prev == HYST_ON || prev == HYST_OFF {
        prev
    } else if current_ctrl {
        if prev == REALLY_ON {
            HYST_OFF
        } else if prev == REALLY_OFF {
            HYST_ON
        } else {
            -1.0
        }
    } else if prev == REALLY_ON {
        REALLY_OFF
    } else if prev == REALLY_OFF {
        REALLY_ON
    } else {
        0.0
    }
}

/// `g_now`: on for `REALLY_ON`/`HYST_ON` in the loads.
fn is_on(state: f64) -> bool {
    state == REALLY_ON || state == HYST_ON
}

impl SwitchInst {
    pub fn new(p: SwitchParams, pos: Var, neg: Var, ctrl: Control, on: bool, st: usize) -> Self {
        SwitchInst {
            p,
            pos,
            neg,
            ctrl,
            on,
            st,
            slots: [TRASH; 4],
        }
    }

    fn is_current(&self) -> bool {
        matches!(self.ctrl, Control::Current { .. })
    }

    fn ctrl_value(&self, sol: &[f64]) -> f64 {
        match self.ctrl {
            Control::Voltage { pos, neg } => sol[pos] - sol[neg],
            Control::Current { branch } => sol[branch],
        }
    }

    /// The conductance the small-signal loads use: `(int)` of state0,
    /// so any nonzero state — `HYST_OFF` included — is on
    /// (`SWacLoad`, `SWpzLoad`, `SWnoise`).
    fn g_small_signal(&self, state0: f64) -> f64 {
        if state0 as i32 != 0 {
            self.p.gon
        } else {
            self.p.goff
        }
    }

    fn stamp(&self, g: f64, mut add: impl FnMut(Slot, f64)) {
        add(self.slots[0], g);
        add(self.slots[1], -g);
        add(self.slots[2], -g);
        add(self.slots[3], g);
    }
}

impl crate::devices::Device for SwitchInst {
    fn par_load(&self) -> bool {
        true
    }

    // `DEVdisto` is NULL for both switches (`switf.h`/`cswitf.h`).
    fn disto_setup(&mut self, _ctx: &crate::disto::DistoSetupCtx) -> Result<(), &'static str> {
        Ok(())
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.pos),
            mb.slot(self.pos, self.neg),
            mb.slot(self.neg, self.pos),
            mb.slot(self.neg, self.neg),
        ];
    }

    /// `SWload`/`CSWload` (ngspice).
    fn load(&mut self, ctx: &mut LoadCtx) {
        let st = self.st + SWITCHSTATE;
        let old = ctx.states.get(0, st);
        let prev = ctx.states.get(1, st);
        let c = self.ctrl_value(ctx.sol);
        let current = self.is_current();

        let state = match ctx.mode {
            InitMode::Jct | InitMode::Fix => init_state(&self.p, self.on, c),
            InitMode::Smsig => prev,
            InitMode::Float => {
                let keep = if current { prev } else { old };
                let s = float_state(&self.p, c, keep, prev);
                if s != old {
                    // "ensure one more iteration"
                    ctx.noncon = true;
                }
                s
            }
            InitMode::Tran | InitMode::Pred => tran_state(&self.p, c, prev, current),
        };

        ctx.states.set(0, st, state);
        if !current {
            // `CSWload` has this store commented out ("FIXME"), which
            // leaves `CSWtrunc` without data.
            ctx.states.set(0, self.st + CTRLVALUE, c);
        }

        let g = if is_on(state) {
            self.p.gon
        } else {
            self.p.goff
        };
        self.stamp(g, |s, v| ctx.add(s, v));
    }

    /// `SWtrunc` (ngspice; the 3f5 switch has no `DEVtrunc`): when
    /// the control moves towards the edge of the window that would
    /// flip the switch, cap the step so it covers at most 3/4 of the
    /// remaining distance plus 50 mV, extrapolating the last step's
    /// rate. Note the test is `state0 == 0`: `HYST_OFF` is handled
    /// as if on. `CSWtrunc` reads a control value `CSWload` never
    /// stores, so it never limits; it is not ported.
    fn trunc(&self, t: &TerrCtx, time_step: &mut f64) {
        if self.is_current() {
            return;
        }
        let ctrl = self.st + CTRLVALUE;
        let v0 = t.states.get(0, ctrl);
        let last_change = v0 - t.states.get(1, ctrl);
        if t.states.get(0, self.st + SWITCHSTATE) == 0.0 {
            let r = self.p.thresh + self.p.hyst;
            if v0 < r && last_change > 0.0 {
                let max_change = (r - v0) * 0.75 + 0.05;
                let max_step = max_change / last_change * t.delta_old[0];
                if *time_step > max_step {
                    *time_step = max_step;
                }
            }
        } else {
            let r = self.p.thresh - self.p.hyst;
            if v0 > r && last_change < 0.0 {
                let max_change = (r - v0) * 0.75 - 0.05;
                let max_step = max_change / last_change * t.delta_old[0];
                if *time_step > max_step {
                    *time_step = max_step;
                }
            }
        }
    }

    /// `SWacLoad`/`CSWacLoad`: the state left by the `INITSMSIG`
    /// load (which in ngspice copies state1 — zero after a bare
    /// operating point, so AC sees the switch open).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let g = self.g_small_signal(ctx.states.get(0, self.st + SWITCHSTATE));
        self.stamp(g, |s, v| ctx.add_g(s, v));
    }

    /// `SWpzLoad`/`CSWpzLoad`: as the AC load.
    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        let g = self.g_small_signal(ctx.states.get(0, self.st + SWITCHSTATE));
        self.stamp(g, |s, v| ctx.add_g(s, v));
        Ok(())
    }

    /// `SWnoise`/`CSWnoise`: one thermal generator of the
    /// small-signal conductance (`onoise_s1`).
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(&[""])
    }

    fn noise_dens(&self, ctx: &NoiseCtx) -> Vec<f64> {
        let g = self.g_small_signal(ctx.states.get(0, self.st + SWITCHSTATE));
        vec![ctx.thermal(self.pos, self.neg, g)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(hyst: f64) -> SwitchParams {
        SwitchParams::new(2.5, hyst, Some(10.0), Some(1e6), 1e-12)
    }

    #[test]
    fn defaults_follow_swsetup() {
        let d = SwitchParams::new(0.0, 0.0, None, None, 1e-12);
        assert_eq!(d.gon, 1.0);
        assert_eq!(d.goff, 1e-12);
        let q = p(1.0);
        assert_eq!(q.gon, 0.1);
        assert_eq!(q.goff, 1e-6);
    }

    #[test]
    fn init_honours_the_hint() {
        let q = p(1.0);
        // Hint on: really on only past vt+vh, else on-in-window, and
        // even below the window the init modes keep it on.
        assert_eq!(init_state(&q, true, 4.0), REALLY_ON);
        assert_eq!(init_state(&q, true, 2.5), HYST_ON);
        assert_eq!(init_state(&q, true, 0.0), HYST_ON);
        // No hint / off: off whatever the control says.
        assert_eq!(init_state(&q, false, 0.0), REALLY_OFF);
        assert_eq!(init_state(&q, false, 2.5), HYST_OFF);
        assert_eq!(init_state(&q, false, 4.0), HYST_OFF);
        assert!(is_on(HYST_ON) && is_on(REALLY_ON));
        assert!(!is_on(HYST_OFF) && !is_on(REALLY_OFF));
    }

    #[test]
    fn hysteresis_rising_and_falling() {
        let q = p(1.0);
        // Rising from off: holds off through the window, flips past
        // vt+vh = 3.5.
        let mut s = REALLY_OFF;
        let mut trace = Vec::new();
        for v in [0.0, 1.0, 2.0, 3.0, 3.4, 3.6, 4.5, 5.0] {
            s = float_state(&q, v, s, s);
            trace.push(is_on(s));
        }
        assert_eq!(trace, [false, false, false, false, false, true, true, true]);
        // Falling from on: holds on down to vt-vh = 1.5.
        let mut trace = Vec::new();
        for v in [5.0, 3.0, 2.0, 1.6, 1.4, 0.0] {
            s = float_state(&q, v, s, s);
            trace.push(is_on(s));
        }
        assert_eq!(trace, [true, true, true, true, false, false]);
        // The transient predictor step decides from the last
        // accepted state the same way.
        assert_eq!(tran_state(&q, 2.5, REALLY_ON, false), REALLY_ON);
        assert_eq!(tran_state(&q, 2.5, REALLY_OFF, false), REALLY_OFF);
        assert_eq!(tran_state(&q, 3.6, REALLY_OFF, true), REALLY_ON);
        assert_eq!(tran_state(&q, 1.4, HYST_ON, true), REALLY_OFF);
    }

    #[test]
    fn in_window_hint_survives_float_for_sw_not_for_csw() {
        let q = p(1.0);
        // SWload keeps state0 (the hint the init modes stored);
        // CSWload keeps state1, which is zero in an operating point.
        let after_fix = init_state(&q, true, 2.5);
        assert_eq!(float_state(&q, 2.5, after_fix, 0.0), HYST_ON);
        assert_eq!(float_state(&q, 2.5, 0.0, 0.0), REALLY_OFF);
    }

    #[test]
    fn non_positive_hysteresis_machine() {
        // vh = -1: on above 3.5, off below 1.5, inside the window the
        // state is the opposite of where the control came from.
        let q = p(-1.0);
        assert_eq!(float_state(&q, 4.0, 0.0, REALLY_OFF), REALLY_ON);
        assert_eq!(float_state(&q, 1.0, 0.0, REALLY_ON), REALLY_OFF);
        assert_eq!(float_state(&q, 2.5, 0.0, REALLY_ON), HYST_OFF);
        assert_eq!(float_state(&q, 2.5, 0.0, REALLY_OFF), HYST_ON);
        assert_eq!(float_state(&q, 2.5, 0.0, HYST_ON), HYST_ON);
        assert_eq!(tran_state(&q, 2.5, REALLY_ON, false), REALLY_OFF);
        assert_eq!(tran_state(&q, 2.5, REALLY_ON, true), HYST_OFF);
        // vh = 0 takes the same branch: exactly at vt the previous
        // state decides.
        let z = p(0.0);
        assert_eq!(float_state(&z, 2.6, 0.0, REALLY_OFF), REALLY_ON);
        assert_eq!(float_state(&z, 2.4, 0.0, REALLY_ON), REALLY_OFF);
        assert_eq!(float_state(&z, 2.5, 0.0, REALLY_OFF), HYST_ON);
    }

    #[test]
    fn small_signal_state_is_the_int_cast() {
        let sw = SwitchInst::new(p(1.0), 1, 2, Control::Voltage { pos: 3, neg: 0 }, false, 0);
        assert_eq!(sw.g_small_signal(REALLY_OFF), 1e-6);
        assert_eq!(sw.g_small_signal(REALLY_ON), 0.1);
        assert_eq!(sw.g_small_signal(HYST_OFF), 0.1);
        assert_eq!(sw.g_small_signal(HYST_ON), 0.1);
    }
}
