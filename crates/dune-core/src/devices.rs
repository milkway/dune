//! Device instances and their stamps. One type per device (CLAUDE.md
//! Princípios 3); stamps and state handling follow the 3f5 `*load.c`
//! / `*trunc.c` / `*acct.c` files named on each type.

use crate::disto::{DistoCtx, DistoInput, DistoSetupCtx};
use crate::integ::TerrCtx;
use crate::matrix::{
    AcCtx, AcceptCtx, AnKind, ConvCtx, InitMode, LoadCtx, MatrixBuilder, NoiseCtx, PzCtx, Slot,
    TRASH, Var,
};
use crate::srcfn::Wave;
use crate::trnoise::NoiseWave;

/// A device instance bound to matrix positions and state slots.
pub(crate) trait Device: Send {
    /// Reserve matrix positions (`*setup.c`); state offsets are
    /// assigned at build time so coupled devices can reference each
    /// other's states.
    fn alloc(&mut self, mb: &mut MatrixBuilder);
    /// Load stamps for one Newton iteration (`*load.c`).
    fn load(&mut self, ctx: &mut LoadCtx);
    /// Second load phase; only the inductor uses it (the C loads all
    /// inductor fluxes, then mutuals, then integrates — see
    /// `indload.c`).
    fn load_late(&mut self, _ctx: &mut LoadCtx) {}
    /// Whether [`load`](Device::load) may run on a worker of the
    /// parallel load (issue #9): it must touch only its own fields,
    /// the state offsets it allocated at build time (its window —
    /// `Built::state_ranges`) and the stamp sink, and its stamps must
    /// not depend on the order it runs in relative to other devices
    /// beyond that. Opt-in per type after an audit of its load; a
    /// device that writes another device's states (the mutual
    /// inductor, `MUTload` adding into both inductors' flux) keeps
    /// the default and loads in place, in device order.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    fn par_load(&self) -> bool {
        false
    }
    /// Timestep limiting from local truncation error (`*trunc.c`).
    fn trunc(&self, _t: &TerrCtx, _time_step: &mut f64) {}
    /// AC small-signal stamps (`*acld.c`). Default: no AC presence.
    fn load_ac(&mut self, _ctx: &mut AcCtx) {}
    /// Accepted-timepoint hook; sources plant breakpoints here
    /// (`*acct.c`).
    fn accept(&mut self, _ctx: &mut AcceptCtx) {}
    /// The corner times of the device's transient waveform inside
    /// `(t0, t1)` (PULSE corners, PWL vertices), for a transient that
    /// starts at `t0 > 0` from a given state (the `.pss` shooting
    /// runs): `VSRCaccept` only plants the next corner when the
    /// current point sits on one, so the chain must be seeded. The
    /// default (every device but the V/I sources) has none.
    fn wave_corners(&self, _t0: f64, _t1: f64, _step: f64, _final_time: f64, _out: &mut Vec<f64>) {}
    /// Remarks about a finished transient run (Dune's opt-in modes
    /// report their error budgets here). Default: none.
    fn tran_notes(&self) -> Vec<String> {
        Vec::new()
    }
    /// Overwrite the DC value (`.dc` sweep). No-op elsewhere.
    fn set_dc(&mut self, _value: f64) {}
    /// `DEVsetic` (`*getic.c`), called by `CKTic` under UIC with the
    /// right-hand side holding the `.nodeset`/`.ic` node values (zero
    /// elsewhere): an initial condition not given on the instance is
    /// taken from the external nodes. Default: none (the devices
    /// whose `*itf.h` has no `DEVsetic`).
    fn set_ic(&mut self, _rhs: &[f64]) {}
    /// Which `IC=` components the card gave (the C's `*icXGiven`
    /// bits, set by `*param.c`), for [`Device::set_ic`]. Devices
    /// are built with every component given (a programmatic spec's
    /// tuple is taken as given); the netlist path narrows it.
    fn set_ic_given(&mut self, _given: IcGiven) {}
    /// Device convergence test (`*conv.c`, NEWCONV): called once
    /// the node voltages pass `NIconvTest`; returns false when the
    /// device's current disagrees with its own linearization.
    fn conv_test(&self, _ctx: &ConvCtx) -> bool {
        true
    }
    /// Complex-frequency stamps for pole-zero analysis
    /// (`dev/*pzld.c`). The default refuses, so a `.pz` deck with a
    /// device that lacks the port fails loudly (the C silently
    /// skips devices without `DEVpzLoad`, which would corrupt the
    /// determinant).
    fn load_pz(&mut self, _ctx: &mut PzCtx) -> Result<(), &'static str> {
        Err("this device has no pole-zero load yet")
    }
    /// Noise generators (`*noise.c`): the per-generator name
    /// suffixes, the last entry being the device total (empty
    /// suffix). `None` marks a noiseless device.
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        None
    }
    /// What joins `onoise`/`onoise_total` and the instance name in
    /// the per-generator vector names: `_` for the 3f5 devices
    /// (`"onoise_%s%s"`), `.` for ngspice's BSIM3 (`"onoise.%s%s"`).
    fn noise_sep(&self) -> &'static str {
        "_"
    }
    /// Spectral densities at the adjoint solution, one per suffix
    /// (the device computes its own total, like the C).
    fn noise_dens(&self, _ctx: &NoiseCtx) -> Vec<f64> {
        Vec::new()
    }
    /// `DEVdisto(D_SETUP)` (`dev/*dset.c`): evaluate the Taylor
    /// coefficients of the device nonlinearities at the operating
    /// point. Inherently linear devices (the C's `DEVdisto == NULL`)
    /// return `Ok`; the default refuses, so a `.disto` deck with a
    /// nonlinear device that lacks the port fails loudly instead of
    /// silently dropping its distortion.
    fn disto_setup(&mut self, _ctx: &DistoSetupCtx) -> Result<(), &'static str> {
        Err("distortion analysis for this device")
    }
    /// `DEVdisto` in the load modes (`D_TWOF1` … `D_2F1MF2`):
    /// accumulate the nonlinear excitation into the RHS. Linear
    /// devices contribute nothing.
    fn disto_load(&self, _ctx: &mut DistoCtx) {}
    /// `CKTdisto(D_RHSF1/D_RHSF2)` for independent sources: write
    /// the `DISTOF1`/`DISTOF2` input into the RHS; returns whether
    /// this source has a `DISTOF2` input (`Df2given`).
    fn disto_rhs(&self, _which: DistoInput, _re: &mut [f64], _im: &mut [f64]) -> bool {
        false
    }
    /// The parameters `.sens` perturbs (`cktsgen.c`'s walk over the
    /// device's `IFparm` tables, in their ngspice-47 form: model
    /// table first, then instance table; AC-only entries only when
    /// `ac`). `None` marks a device without a sensitivity port, which
    /// makes the analysis fail loudly.
    fn sens_params(&self, _ac: bool) -> Option<Vec<SensParam>> {
        None
    }
    /// `sens_setp` + `sens_temp`: set the parameter named by `suffix`
    /// (as listed by [`Device::sens_params`]) and redo the
    /// temperature-dependent derived values. Parameters with no
    /// effect on Dune's equations are no-ops.
    fn sens_set(&mut self, _suffix: &str, _value: f64) {}
}

/// The given-ness of an instance's `IC=` components: bit `i` is
/// component `i` of the device's `ic` tuple (BJT `VBE, VCE`; MOS
/// `VDS, VGS, VBS`; JFET/MES `VDS, VGS`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct IcGiven(pub u8);

impl IcGiven {
    /// Every component given (the programmatic default).
    pub(crate) const ALL: IcGiven = IcGiven(u8::MAX);
    /// No component given.
    pub(crate) const NONE: IcGiven = IcGiven(0);

    /// An `IC=` list of `n` values: the `*param.c` switch falls
    /// through from `case n` down to `case 1`, so the first `n`
    /// components are given.
    pub(crate) fn first(n: usize) -> IcGiven {
        IcGiven(((1u16 << n.min(8)) - 1) as u8)
    }

    pub(crate) fn with(self, i: usize) -> IcGiven {
        IcGiven(self.0 | (1 << i))
    }

    pub(crate) fn has(self, i: usize) -> bool {
        self.0 & (1 << i) != 0
    }
}

/// One `*getic.c` assignment: `if (!icGiven) ic = rhs[a] − rhs[b]`
/// (external nodes: "user specified, so use external nodes").
pub(crate) fn getic(given: IcGiven, i: usize, ic: &mut f64, rhs: &[f64], a: Var, b: Var) {
    if !given.has(i) {
        *ic = rhs[a] - rhs[b];
    }
}

/// One perturbable parameter of a device instance (`sgen`): the
/// sensitivity vector is named `<instance><suffix>` — `:kw` for a
/// model parameter, the bare instance name for the principal
/// (`IF_PRINCIPAL`) instance parameter, `_kw` for the others
/// (`cktsens.c`).
#[derive(Clone, Debug)]
pub(crate) struct SensParam {
    pub suffix: &'static str,
    /// The value `sens_getp` reads; the perturbation is
    /// `value·1e-6`, or `1e-6` when it is zero.
    pub value: f64,
}

/// Parameters with no effect on the equations of Dune's device
/// (temperature coefficients at `TEMP == TNOM`, geometry of a
/// resistor whose value is given, noise coefficients, …): listed
/// with value 0 so the vector exists, as in the ngspice output,
/// where they come out exactly zero.
fn sens_inert(suffixes: &[&'static str]) -> impl Iterator<Item = SensParam> {
    suffixes
        .iter()
        .map(|&suffix| SensParam { suffix, value: 0.0 })
}

fn sens_param(suffix: &'static str, value: f64) -> SensParam {
    SensParam { suffix, value }
}

/// The `disto_setup` of a device with no nonlinearity (`DEVdisto ==
/// NULL` in its `*itf.h`).
macro_rules! disto_linear {
    () => {
        fn disto_setup(&mut self, _ctx: &DistoSetupCtx) -> Result<(), &'static str> {
            Ok(())
        }
    };
}

/// `RESload` (`dev/res/resload.c`): conductance between two nodes.
pub(crate) struct ResistorI {
    pub g: f64,
    /// `RESacConduct`: the AC conductance (`ac=` resistance or `g`).
    g_ac: f64,
    /// Resistance, the `ac=` resistance, multiplier and scale the
    /// conductances derive from (ngspice's `RESupdate_conduct`:
    /// `g = m/(R·scale)`); only `.sens` moves them.
    r: f64,
    r_ac: Option<f64>,
    m: f64,
    scale: f64,
    /// `REStemp`'s `1 + TC1·ΔT + TC2·ΔT²` (1 for a plain resistor).
    factor: f64,
    pub pos: Var,
    pub neg: Var,
    slots: [Slot; 4],
}

impl ResistorI {
    /// A resistor of nominal resistance `r` at temperature factor
    /// `factor` (`REStemp`: `RESconduct = 1/(RESresist·factor)`).
    pub fn with_factor(r: f64, factor: f64, pos: Var, neg: Var) -> Self {
        let mut res = ResistorI {
            g: 0.0,
            g_ac: 0.0,
            r,
            r_ac: None,
            m: 1.0,
            scale: 1.0,
            factor,
            pos,
            neg,
            slots: [TRASH; 4],
        };
        res.update_conduct();
        res
    }

    /// `RESupdate_conduct` (ngspice `restemp.c`; the 3f5's
    /// `REStemp` without `m`/`scale`/`ac`).
    fn update_conduct(&mut self) {
        self.g = self.m / (self.r * self.factor * self.scale);
        self.g_ac = self.m / (self.r_ac.unwrap_or(self.r) * self.factor * self.scale);
    }
}

/// ngspice-47 `RESmPTable` entries `.sens` walks (`res.c`; the
/// `IF_REDUNDANT` aliases, `IF_NONSENSE` `defw`/`l`/`tnom` and the
/// integer `noisy` drop out of `set_param`). Plain resistors still
/// have a default model, so these are always listed.
const RES_MODEL_SENS: &[&str] = &[
    ":rsh", ":narrow", ":short", ":tc1", ":tc2", ":tce", ":kf", ":af", ":r", ":bv_max", ":lf",
    ":wf", ":ef",
];

impl Device for ResistorI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.pos),
            mb.slot(self.neg, self.neg),
            mb.slot(self.pos, self.neg),
            mb.slot(self.neg, self.pos),
        ];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.g);
        ctx.add(self.slots[1], self.g);
        ctx.add(self.slots[2], -self.g);
        ctx.add(self.slots[3], -self.g);
    }

    /// ngspice's `.dc RNAME …` (`dctrcurv.c`, `rcode`): the swept
    /// value is the nominal resistance (`RESresist`, `RESresGiven`),
    /// still divided by the temperature factor.
    fn set_dc(&mut self, value: f64) {
        self.r = value;
        self.update_conduct();
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.slots[0], self.g_ac);
        ctx.add_g(self.slots[1], self.g_ac);
        ctx.add_g(self.slots[2], -self.g_ac);
        ctx.add_g(self.slots[3], -self.g_ac);
    }

    /// `RESpTable`: the principal `resistance`, the AC-only `ac`,
    /// then `temp dtemp l w m tc tc2 tce bv_max scale`; only the
    /// resistance, `ac`, `m` and `scale` reach Dune's conductance.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        let mut v: Vec<SensParam> = sens_inert(RES_MODEL_SENS).collect();
        v.push(sens_param("", self.r));
        if ac {
            v.push(sens_param("_ac", self.r_ac.unwrap_or(self.r)));
        }
        v.extend(sens_inert(&["_temp", "_dtemp", "_l", "_w"]));
        v.push(sens_param("_m", self.m));
        v.extend(sens_inert(&["_tc", "_tc2", "_tce", "_bv_max"]));
        v.push(sens_param("_scale", self.scale));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        match suffix {
            "" => self.r = value,
            "_ac" => self.r_ac = Some(value),
            "_m" => self.m = value,
            "_scale" => self.scale = value,
            _ => return,
        }
        self.update_conduct();
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.slots[0], self.g);
        ctx.add_g(self.slots[1], self.g);
        ctx.add_g(self.slots[2], -self.g);
        ctx.add_g(self.slots[3], -self.g);
        Ok(())
    }

    /// `RESnoise` (ngspice): thermal + flicker + total. Plain
    /// resistors have no model card, so `fNcoef` is 0 and the
    /// flicker density is identically zero (the vector still
    /// exists, as in the golden rawfiles).
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(&["_thermal", "_1overf", ""])
    }

    fn noise_dens(&self, ctx: &NoiseCtx) -> Vec<f64> {
        let thermal = ctx.thermal(self.pos, self.neg, self.g);
        let flicker = 0.0;
        vec![thermal, flicker, thermal + flicker]
    }
}

/// `CAPload` (`dev/cap/capload.c`): open at DC, integrated charge in
/// transient. States: charge, companion current.
pub(crate) struct CapacitorI {
    capac: f64,
    /// Instance capacitance, multiplier and scale (`capac =
    /// C·m·scale`, ngspice `captemp.c` + `capload.c`); only `.sens`
    /// moves them.
    c: f64,
    m: f64,
    scale: f64,
    ic: f64,
    /// `CAPicGiven`: an `IC=` on the card, which `CAPgetic` keeps.
    ic_given: bool,
    pos: Var,
    neg: Var,
    slots: [Slot; 4],
    q: usize,
}

impl CapacitorI {
    pub fn new(capac: f64, ic: Option<f64>, pos: Var, neg: Var, q: usize) -> Self {
        CapacitorI {
            capac,
            c: capac,
            m: 1.0,
            scale: 1.0,
            ic: ic.unwrap_or(0.0),
            ic_given: ic.is_some(),
            pos,
            neg,
            slots: [TRASH; 4],
            q,
        }
    }
}

impl Device for CapacitorI {
    fn par_load(&self) -> bool {
        true
    }

    /// `CAPgetic` (`capgetic.c`).
    fn set_ic(&mut self, rhs: &[f64]) {
        if !self.ic_given {
            self.ic = rhs[self.pos] - rhs[self.neg];
        }
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.pos),
            mb.slot(self.neg, self.neg),
            mb.slot(self.pos, self.neg),
            mb.slot(self.neg, self.pos),
        ];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        // Pure DC sweeps/op: the capacitor is simply absent.
        if !(ctx.kind == AnKind::Tran || ctx.kind == AnKind::TranOp) {
            return;
        }
        let cond1 =
            (ctx.is_dc() && ctx.mode == InitMode::Jct) || (ctx.uic && ctx.mode == InitMode::Tran);
        let vcap = if cond1 {
            self.ic
        } else {
            ctx.sol[self.pos] - ctx.sol[self.neg]
        };
        if ctx.kind == AnKind::Tran {
            if ctx.mode == InitMode::Pred {
                let q1 = ctx.states.get(1, self.q);
                ctx.states.set(0, self.q, q1);
            } else {
                ctx.states.set(0, self.q, self.capac * vcap);
                if ctx.mode == InitMode::Tran {
                    let q0 = ctx.states.get(0, self.q);
                    ctx.states.set(1, self.q, q0);
                }
            }
            let (geq, ceq) = ctx.integrate(self.capac, self.q);
            if ctx.mode == InitMode::Tran {
                let c0 = ctx.states.get(0, self.q + 1);
                ctx.states.set(1, self.q + 1, c0);
            }
            ctx.add(self.slots[0], geq);
            ctx.add(self.slots[1], geq);
            ctx.add(self.slots[2], -geq);
            ctx.add(self.slots[3], -geq);
            ctx.add_rhs(self.pos, -ceq);
            ctx.add_rhs(self.neg, ceq);
        } else {
            // Transient operating point: just record the charge.
            ctx.states.set(0, self.q, self.capac * vcap);
        }
    }

    fn trunc(&self, t: &TerrCtx, time_step: &mut f64) {
        // `CAPtrunc` is one `CKTterr` call on the charge state.
        crate::integ::terr(self.q, t, time_step);
    }

    /// `CAPacLoad`: `+jωC`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let val = ctx.omega * self.capac;
        ctx.add_b(self.slots[0], val);
        ctx.add_b(self.slots[1], val);
        ctx.add_b(self.slots[2], -val);
        ctx.add_b(self.slots[3], -val);
    }

    /// `CAPmPTable`/`CAPpTable` (ngspice-47 `cap.c`): the capacitance,
    /// `ic`, `w`, `l` and the model's geometry entries are `IF_AC`,
    /// so a DC walk lists only `bv_max` and the temperature/scale
    /// entries — all without effect on an open circuit.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        let mut v: Vec<SensParam> = Vec::new();
        if ac {
            v.extend(sens_inert(&[
                ":cap", ":cj", ":cjsw", ":narrow", ":short", ":del", ":tc1", ":tc2", ":di",
                ":thick",
            ]));
        }
        v.extend(sens_inert(&[":bv_max"]));
        if ac {
            v.push(sens_param("", self.c));
            v.push(sens_param("_ic", self.ic));
        }
        v.extend(sens_inert(&["_temp", "_dtemp"]));
        if ac {
            v.extend(sens_inert(&["_w", "_l"]));
        }
        v.push(sens_param("_m", self.m));
        v.extend(sens_inert(&["_tc1", "_tc2", "_bv_max"]));
        v.push(sens_param("_scale", self.scale));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        match suffix {
            "" => self.c = value,
            "_m" => self.m = value,
            "_scale" => self.scale = value,
            _ => return,
        }
        self.capac = self.c * self.m * self.scale;
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_cap(self.slots[0], self.capac);
        ctx.add_cap(self.slots[1], self.capac);
        ctx.add_cap(self.slots[2], -self.capac);
        ctx.add_cap(self.slots[3], -self.capac);
        Ok(())
    }
}

/// `INDload` (`dev/ind/indload.c`): a branch equation whose flux is
/// integrated; DC collapses it to a 0 V source. States: flux, volt.
pub(crate) struct InductorI {
    induct: f64,
    /// Instance inductance, multiplier and scale (`induct =
    /// L·scale/m`, ngspice `indtemp.c` + `indload.c`); only `.sens`
    /// moves them.
    l: f64,
    m: f64,
    scale: f64,
    ic: f64,
    pos: Var,
    neg: Var,
    pub(crate) br: Var,
    slots: [Slot; 4],
    br_br: Slot,
    pub(crate) flux: usize,
}

impl InductorI {
    pub fn new(induct: f64, ic: f64, pos: Var, neg: Var, br: Var, flux: usize) -> Self {
        InductorI {
            induct,
            l: induct,
            m: 1.0,
            scale: 1.0,
            ic,
            pos,
            neg,
            br,
            slots: [TRASH; 4],
            br_br: TRASH,
            flux,
        }
    }
}

impl Device for InductorI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.br),
            mb.slot(self.br, self.pos),
            mb.slot(self.neg, self.br),
            mb.slot(self.br, self.neg),
        ];
        self.br_br = mb.slot(self.br, self.br);
    }

    /// Phase one: the flux from the previous iterate (mutual
    /// couplings add theirs on top before `load_late` integrates).
    fn load(&mut self, ctx: &mut LoadCtx) {
        if !(ctx.is_dc() || ctx.mode == InitMode::Pred) {
            let flux = if ctx.uic && ctx.mode == InitMode::Tran {
                self.induct * self.ic
            } else {
                self.induct * ctx.sol[self.br]
            };
            ctx.states.set(0, self.flux, flux);
        }
    }

    fn load_late(&mut self, ctx: &mut LoadCtx) {
        let (req, veq);
        if ctx.is_dc() {
            req = 0.0;
            veq = 0.0;
        } else {
            if ctx.mode == InitMode::Pred {
                let f1 = ctx.states.get(1, self.flux);
                ctx.states.set(0, self.flux, f1);
            } else if ctx.mode == InitMode::Tran {
                let f0 = ctx.states.get(0, self.flux);
                ctx.states.set(1, self.flux, f0);
            }
            let (r, v) = ctx.integrate(self.induct, self.flux);
            req = r;
            veq = v;
        }
        ctx.add_rhs(self.br, veq);
        if ctx.mode == InitMode::Tran {
            let v0 = ctx.states.get(0, self.flux + 1);
            ctx.states.set(1, self.flux + 1, v0);
        }
        ctx.add(self.slots[0], 1.0);
        ctx.add(self.slots[1], 1.0);
        ctx.add(self.slots[2], -1.0);
        ctx.add(self.slots[3], -1.0);
        ctx.add(self.br_br, -req);
    }

    fn trunc(&self, t: &TerrCtx, time_step: &mut f64) {
        // `INDtrunc`: `CKTterr` on the flux.
        crate::integ::terr(self.flux, t, time_step);
    }

    /// `INDacLoad`: the branch equation with `-jωL`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let val = ctx.omega * self.induct;
        ctx.add_g(self.slots[0], 1.0);
        ctx.add_g(self.slots[1], 1.0);
        ctx.add_g(self.slots[2], -1.0);
        ctx.add_g(self.slots[3], -1.0);
        ctx.add_b(self.br_br, -val);
    }

    /// `INDmPTable`/`INDpTable` (ngspice-47 `ind.c`): every model
    /// entry, the inductance and `ic` are `IF_AC`. The coupling
    /// factors of `K` cards are their own device (`MUTtemp`) and do
    /// not follow a perturbed inductance, as in the C.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        let mut v: Vec<SensParam> = Vec::new();
        if ac {
            v.extend(sens_inert(&[
                ":ind", ":tc1", ":tc2", ":csect", ":dia", ":length", ":nt", ":mu",
            ]));
            v.push(sens_param("", self.l));
            v.push(sens_param("_ic", self.ic));
        }
        v.extend(sens_inert(&["_temp", "_dtemp"]));
        v.push(sens_param("_m", self.m));
        v.extend(sens_inert(&["_tc1", "_tc2"]));
        v.push(sens_param("_scale", self.scale));
        v.extend(sens_inert(&["_nt"]));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        match suffix {
            "" => self.l = value,
            "_m" => self.m = value,
            "_scale" => self.scale = value,
            _ => return,
        }
        self.induct = self.l * self.scale / self.m;
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.slots[0], 1.0);
        ctx.add_g(self.slots[1], 1.0);
        ctx.add_g(self.slots[2], -1.0);
        ctx.add_g(self.slots[3], -1.0);
        ctx.add_cap(self.br_br, -self.induct);
        Ok(())
    }
}

/// `MUTload` (inside `indload.c`): adds `M·i(other)` to each coupled
/// flux and stamps `-M·ag0` between the branch equations.
pub(crate) struct MutualI {
    /// `MUTfactor = k·sqrt(L1·L2)`.
    pub factor: f64,
    /// The coupling `k` and `sqrt(|L1·L2|)` it multiplies (`.sens`).
    k: f64,
    sqrt_l1l2: f64,
    pub br1: Var,
    pub br2: Var,
    pub flux1: usize,
    pub flux2: usize,
    slots: [Slot; 2],
}

impl Device for MutualI {
    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [mb.slot(self.br1, self.br2), mb.slot(self.br2, self.br1)];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        if !(ctx.is_dc() || ctx.mode == InitMode::Pred) {
            ctx.states
                .add(0, self.flux1, self.factor * ctx.sol[self.br2]);
            ctx.states
                .add(0, self.flux2, self.factor * ctx.sol[self.br1]);
        }
        if !ctx.is_dc() {
            ctx.add(self.slots[0], -self.factor * ctx.ag[0]);
            ctx.add(self.slots[1], -self.factor * ctx.ag[0]);
        }
    }

    /// `MUTacLoad`: `-jωM` between the branch equations.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let val = ctx.omega * self.factor;
        ctx.add_b(self.slots[0], -val);
        ctx.add_b(self.slots[1], -val);
    }

    /// `MUTpTable`: the principal coupling `k` is `IF_AC`; nothing
    /// in a DC walk.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        Some(if ac {
            vec![sens_param("", self.k)]
        } else {
            Vec::new()
        })
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix.is_empty() {
            self.k = value;
            self.factor = self.k * self.sqrt_l1l2;
        }
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_cap(self.slots[0], -self.factor);
        ctx.add_cap(self.slots[1], -self.factor);
        Ok(())
    }
}

impl MutualI {
    /// `k` and `sqrt(|L1·L2|)`; `MUTfactor` is their product.
    pub fn new(k: f64, sqrt_l1l2: f64, br1: Var, br2: Var, flux1: usize, flux2: usize) -> Self {
        MutualI {
            factor: k * sqrt_l1l2,
            k,
            sqrt_l1l2,
            br1,
            br2,
            flux1,
            flux2,
            slots: [TRASH; 2],
        }
    }
}

/// Shared branch-equation stamps of V-like elements.
struct BranchStamp {
    pos: Var,
    neg: Var,
    br: Var,
    slots: [Slot; 4],
}

impl BranchStamp {
    fn new(pos: Var, neg: Var, br: Var) -> Self {
        BranchStamp {
            pos,
            neg,
            br,
            slots: [TRASH; 4],
        }
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.br),
            mb.slot(self.br, self.pos),
            mb.slot(self.neg, self.br),
            mb.slot(self.br, self.neg),
        ];
    }

    fn load(&self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], 1.0);
        ctx.add(self.slots[1], 1.0);
        ctx.add(self.slots[2], -1.0);
        ctx.add(self.slots[3], -1.0);
    }
}

/// Value of an independent source at this iteration (`VSRCload`,
/// `ISRCload`): the DC value under `.op`/`.dc` when it was given,
/// otherwise the transient function at `time` (0 under DC modes),
/// else DC. Under the transient operating point ngspice scales the
/// final value — whatever produced it — by `srcFact`, so source
/// stepping can zero transient-function sources too; the 3f5 left
/// those unscaled and its stepping could never converge a tranop
/// driven only by such sources (`corpus/KNOWN_DIFFS.md`).
///
/// A transient-noise function (`noise`, ngspice-47's `TRNOISE`/
/// `TRRANDOM`) is added to the DC value, as `VSRCload` does; it is
/// noiseless at t = 0 and hence in every DC mode.
fn source_value(
    ctx: &LoadCtx,
    dc: Option<f64>,
    wave: &Option<Wave>,
    noise: Option<&mut NoiseWave>,
) -> f64 {
    if matches!(ctx.kind, AnKind::DcOp | AnKind::DcSweep) && dc.is_some() {
        return ctx.srcfact * dc.unwrap_or(0.0);
    }
    let v = match (noise, wave) {
        (Some(n), _) => {
            let time = if ctx.is_dc() { 0.0 } else { ctx.time };
            n.value(time, ctx.final_time) + dc.unwrap_or(0.0)
        }
        (None, Some(w)) => {
            let time = if ctx.is_dc() { 0.0 } else { ctx.time };
            w.value(time, ctx.step, ctx.final_time)
        }
        (None, None) => dc.unwrap_or(0.0),
    };
    if ctx.kind == AnKind::TranOp {
        v * ctx.srcfact
    } else {
        v
    }
}

/// `VSRCload` (`dev/vsrc/vsrcload.c`).
pub(crate) struct VsourceI {
    pub dc: Option<f64>,
    pub wave: Option<Wave>,
    /// The realization of a `TRNOISE`/`TRRANDOM` wave.
    noise: Option<NoiseWave>,
    /// `AC <MAG <PHASE°>>` as given (`.sens` moves it).
    ac: Option<(f64, f64)>,
    /// `VSRCacReal`/`VSRCacImag` (`vsrctemp.c`: phase in degrees).
    ac_re: f64,
    ac_im: f64,
    branch: BranchStamp,
    /// `DISTOF1`/`DISTOF2 <MAG <PHASE°>>` (`VSRCdF1mag` …).
    disto: SourceDisto,
    /// RF port (ngspice-47 RFSPICE): the series reference
    /// conductance between `+` and the internal node, which the
    /// branch equation then drives instead of `+`.
    port: Option<PortStamp>,
}

/// The `Y0 = 1/Z0` stamp of an RF port between the external `+`
/// node and `<name>#res` (`VSRCposPosPtr` … `VSRCnegPosPtr` of
/// `vsrcset.c`, which despite their names sit on `res`).
struct PortStamp {
    pos: Var,
    res: Var,
    y0: f64,
    slots: [Slot; 4],
}

impl PortStamp {
    fn stamp(&self, mut add: impl FnMut(Slot, f64)) {
        add(self.slots[0], self.y0);
        add(self.slots[1], self.y0);
        add(self.slots[2], -self.y0);
        add(self.slots[3], -self.y0);
    }
}

impl VsourceI {
    pub fn new(
        dc: Option<f64>,
        wave: Option<Wave>,
        ac: Option<(f64, f64)>,
        pos: Var,
        neg: Var,
        br: Var,
    ) -> Self {
        let (ac_re, ac_im) = ac_parts(ac);
        VsourceI {
            dc,
            wave,
            noise: None,
            ac,
            ac_re,
            ac_im,
            branch: BranchStamp::new(pos, neg, br),
            disto: SourceDisto::default(),
            port: None,
        }
    }

    /// Attach the `DISTOF1`/`DISTOF2` inputs.
    pub fn with_disto(mut self, f1: Option<(f64, f64)>, f2: Option<(f64, f64)>) -> Self {
        self.disto = SourceDisto { f1, f2 };
        self
    }

    /// Attach the realization of the source's noise wave
    /// ([`NoiseWave::from_wave`]).
    pub fn with_noise(mut self, noise: Option<NoiseWave>) -> Self {
        self.noise = noise;
        self
    }

    /// Make the source an RF port (`VSRCsetup`/`VSRCtemp` under
    /// RFSPICE): the branch incidence moves from `+` to the internal
    /// node `res`, and `Y0 = 1/z0` joins `+` to `res` in every
    /// analysis (`VSRCload` and `VSRCacLoad` both stamp it).
    pub fn with_port(mut self, res: Var, z0: f64) -> Self {
        let pos = self.branch.pos;
        self.branch.pos = res;
        self.port = Some(PortStamp {
            pos,
            res,
            y0: 1.0 / z0,
            slots: [TRASH; 4],
        });
        self
    }
}

/// A source's distortion inputs, `(mag, phase°)` each.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SourceDisto {
    pub f1: Option<(f64, f64)>,
    pub f2: Option<(f64, f64)>,
}

impl SourceDisto {
    /// The `(0.5·mag·cos, 0.5·mag·sin)` phasor `CKTdisto` stamps for
    /// the requested input (`ckt/cktdisto.c`, `D_RHSF1/D_RHSF2`).
    fn input(&self, which: DistoInput) -> Option<(f64, f64)> {
        let (mag, phase) = match which {
            DistoInput::F1 => self.f1?,
            DistoInput::F2 => self.f2?,
        };
        let arg = std::f64::consts::PI * phase / 180.0;
        Some((0.5 * mag * arg.cos(), 0.5 * mag * arg.sin()))
    }
}

/// `(mag, phase°)` → rectangular (`vsrctemp.c`).
fn ac_parts(ac: Option<(f64, f64)>) -> (f64, f64) {
    match ac {
        Some((mag, phase)) => {
            let radians = phase * std::f64::consts::PI / 180.0;
            (mag * radians.cos(), mag * radians.sin())
        }
        None => (0.0, 0.0),
    }
}

impl Device for VsourceI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        if let Some(p) = &mut self.port {
            p.slots = [
                mb.slot(p.pos, p.pos),
                mb.slot(p.res, p.res),
                mb.slot(p.pos, p.res),
                mb.slot(p.res, p.pos),
            ];
        }
        self.branch.alloc(mb);
    }

    /// `CKTdisto` for `VSRC`: the input phasor on the branch
    /// equation (assigned, not added, as in the C).
    fn disto_rhs(&self, which: DistoInput, re: &mut [f64], im: &mut [f64]) -> bool {
        if let Some((r, i)) = self.disto.input(which) {
            re[self.branch.br] = r;
            im[self.branch.br] = i;
        }
        self.disto.f2.is_some()
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        if let Some(p) = &self.port {
            p.stamp(|sl, v| ctx.add(sl, v));
        }
        let v = source_value(ctx, self.dc, &self.wave, self.noise.as_mut());
        ctx.add_rhs(self.branch.br, v);
    }

    /// `VSRCacLoad` (ngspice-47, RFSPICE): under `.sp` every V
    /// source's AC stimulus is off (`MODESP`; the analysis drives
    /// the ports itself).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        if !ctx.sp {
            ctx.rhs_re[self.branch.br] += self.ac_re;
            ctx.rhs_im[self.branch.br] += self.ac_im;
        }
        if let Some(p) = &self.port {
            p.stamp(|sl, v| ctx.add_g(sl, v));
        }
    }

    /// `VSRCpzLoad`: the branch incidence, no excitation (the PZ
    /// drive comes from the column surgery). ngspice's `vsrcpzld.c`
    /// has no RFSPICE branch, so a port loads only its branch (on the
    /// internal node) and no reference conductance.
    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        Ok(())
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if let Some(n) = &mut self.noise {
            n.accept(ctx);
        } else if let Some(w) = &self.wave {
            w.accept(ctx);
        }
    }

    fn wave_corners(&self, t0: f64, t1: f64, step: f64, final_time: f64, out: &mut Vec<f64>) {
        if let Some(w) = &self.wave {
            w.corners(t0, t1, step, final_time, out);
        }
    }

    fn set_dc(&mut self, value: f64) {
        self.dc = Some(value);
    }

    /// `VSRCpTable` (ngspice-47 `vsrc.c`, RF ports compiled in): the
    /// principal `dc`, the AC-only `acmag`/`acphase`, then the port
    /// entries `z0 pwr freq phase`, inert without `portnum`. On a
    /// port `z0` is live (`sens_temp` reruns `VSRCtemp`, which
    /// recomputes `Y0`); `pwr freq phase` only shape ngspice's `PORT`
    /// time function, which Dune does not port, and stay inert.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        let mut v = vec![sens_param("", sens_dc_value(self.dc, &self.wave))];
        if ac {
            let (mag, phase) = self.ac.unwrap_or((0.0, 0.0));
            v.push(sens_param("_acmag", mag));
            v.push(sens_param("_acphase", phase));
        }
        match &self.port {
            Some(p) => v.push(sens_param("_z0", 1.0 / p.y0)),
            None => v.extend(sens_inert(&["_z0"])),
        }
        v.extend(sens_inert(&["_pwr", "_freq", "_phase"]));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_z0" {
            if let Some(p) = &mut self.port {
                p.y0 = 1.0 / value;
            }
            return;
        }
        if let Some(ac) = sens_set_source(suffix, value, &mut self.dc, &mut self.ac) {
            (self.ac_re, self.ac_im) = ac_parts(Some(ac));
        }
    }
}

/// The `dc` value `sens_getp` reads off an independent source: the
/// given DC value, else the transient function at t = 0 (what
/// `VSRCtemp`/`ISRCtemp` store when no DC value was given).
fn sens_dc_value(dc: Option<f64>, wave: &Option<Wave>) -> f64 {
    match (dc, wave) {
        (Some(v), _) => v,
        (None, Some(w)) => w.value(0.0, 0.0, 0.0),
        (None, None) => 0.0,
    }
}

/// `VSRCparam`/`ISRCparam` for the entries `.sens` moves: setting
/// `dc` makes it given (so it wins over a transient function in DC
/// modes, as in `VSRCload`); `acmag`/`acphase` make the AC stimulus
/// given. Returns the new `(mag, phase)` when the AC value changed.
fn sens_set_source(
    suffix: &str,
    value: f64,
    dc: &mut Option<f64>,
    ac: &mut Option<(f64, f64)>,
) -> Option<(f64, f64)> {
    let (mag, phase) = ac.unwrap_or((0.0, 0.0));
    let new = match suffix {
        "" => {
            *dc = Some(value);
            return None;
        }
        "_acmag" => (value, phase),
        "_acphase" => (mag, value),
        _ => return None,
    };
    *ac = Some(new);
    Some(new)
}

/// `ISRCload` (`dev/isrc/isrcload.c`).
pub(crate) struct IsourceI {
    pub dc: Option<f64>,
    pub wave: Option<Wave>,
    /// The realization of a `TRNOISE`/`TRRANDOM` wave.
    noise: Option<NoiseWave>,
    /// `AC <MAG <PHASE°>>` as given, and the parallel multiplier
    /// (`ISRCmValue`, 1 on cards; `.sens` moves both).
    ac: Option<(f64, f64)>,
    m: f64,
    ac_re: f64,
    ac_im: f64,
    pub pos: Var,
    pub neg: Var,
    disto: SourceDisto,
}

impl IsourceI {
    pub fn new(
        dc: Option<f64>,
        wave: Option<Wave>,
        ac: Option<(f64, f64)>,
        pos: Var,
        neg: Var,
    ) -> Self {
        let (ac_re, ac_im) = ac_parts(ac);
        IsourceI {
            dc,
            wave,
            noise: None,
            ac,
            m: 1.0,
            ac_re,
            ac_im,
            pos,
            neg,
            disto: SourceDisto::default(),
        }
    }

    /// Attach the `DISTOF1`/`DISTOF2` inputs.
    pub fn with_disto(mut self, f1: Option<(f64, f64)>, f2: Option<(f64, f64)>) -> Self {
        self.disto = SourceDisto { f1, f2 };
        self
    }

    /// Attach the realization of the source's noise wave
    /// ([`NoiseWave::from_wave`]).
    pub fn with_noise(mut self, noise: Option<NoiseWave>) -> Self {
        self.noise = noise;
        self
    }
}

impl Device for IsourceI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, _: &mut MatrixBuilder) {}

    /// `CKTdisto` for `ISRC` (assigned, not added, as in the C).
    /// The 3f5 stamps `−input` at `N+` and `+input` at `N−`;
    /// ngspice-47 stamps the opposite polarity — measured on the
    /// oracle as an exact sign flip of every odd-order kernel
    /// (3rd harmonic, 2f1−f2) of a current-driven deck, with the
    /// even orders unchanged. Dune follows ngspice
    /// (corpus/KNOWN_DIFFS.md), as for `ISRCacLoad`.
    fn disto_rhs(&self, which: DistoInput, re: &mut [f64], im: &mut [f64]) -> bool {
        if let Some((r, i)) = self.disto.input(which) {
            re[self.pos] = r;
            re[self.neg] = -r;
            im[self.pos] = i;
            im[self.neg] = -i;
        }
        self.disto.f2.is_some()
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        let v = self.m * source_value(ctx, self.dc, &self.wave, self.noise.as_mut());
        ctx.add_rhs(self.pos, -v);
        ctx.add_rhs(self.neg, v);
    }

    /// `ISRCacLoad`. The 3f5 adds the AC stimulus with the polarity
    /// opposite to its own DC load (a long-known sign slip); ngspice
    /// fixed it, and the oracle confirmed the flip (180° on a
    /// current-driven deck). Dune follows ngspice
    /// (corpus/KNOWN_DIFFS.md).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let (re, im) = (self.m * self.ac_re, self.m * self.ac_im);
        ctx.rhs_re[self.pos] -= re;
        ctx.rhs_re[self.neg] += re;
        ctx.rhs_im[self.pos] -= im;
        ctx.rhs_im[self.neg] += im;
    }

    /// `ISRCpTable` (ngspice-47 `isrc.c`): the principal `dc`, `m`,
    /// then the AC-only `acmag`/`acphase`; `ISRCload`/`ISRCacLoad`
    /// scale both values by `m`.
    fn sens_params(&self, ac: bool) -> Option<Vec<SensParam>> {
        let mut v = vec![
            sens_param("", sens_dc_value(self.dc, &self.wave)),
            sens_param("_m", self.m),
        ];
        if ac {
            let (mag, phase) = self.ac.unwrap_or((0.0, 0.0));
            v.push(sens_param("_acmag", mag));
            v.push(sens_param("_acphase", phase));
        }
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_m" {
            self.m = value;
        } else if let Some(ac) = sens_set_source(suffix, value, &mut self.dc, &mut self.ac) {
            (self.ac_re, self.ac_im) = ac_parts(Some(ac));
        }
    }

    /// `ISRCpzLoad`: an ideal current source is open-circuit.
    fn load_pz(&mut self, _ctx: &mut PzCtx) -> Result<(), &'static str> {
        Ok(())
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if let Some(n) = &mut self.noise {
            n.accept(ctx);
        } else if let Some(w) = &self.wave {
            w.accept(ctx);
        }
    }

    fn wave_corners(&self, t0: f64, t1: f64, step: f64, final_time: f64, out: &mut Vec<f64>) {
        if let Some(w) = &self.wave {
            w.corners(t0, t1, step, final_time, out);
        }
    }

    fn set_dc(&mut self, value: f64) {
        self.dc = Some(value);
    }
}

/// `VCVSload`: E source.
pub(crate) struct VcvsI {
    pub gain: f64,
    branch: BranchStamp,
    cpos: Var,
    cneg: Var,
    cslots: [Slot; 2],
}

impl VcvsI {
    pub fn new(gain: f64, n: [Var; 4], br: Var) -> Self {
        VcvsI {
            gain,
            branch: BranchStamp::new(n[0], n[1], br),
            cpos: n[2],
            cneg: n[3],
            cslots: [TRASH; 2],
        }
    }
}

impl Device for VcvsI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.branch.alloc(mb);
        self.cslots = [
            mb.slot(self.branch.br, self.cpos),
            mb.slot(self.branch.br, self.cneg),
        ];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        ctx.add(self.cslots[0], -self.gain);
        ctx.add(self.cslots[1], self.gain);
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        ctx.add_g(self.cslots[0], -self.gain);
        ctx.add_g(self.cslots[1], self.gain);
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        ctx.add_g(self.cslots[0], -self.gain);
        ctx.add_g(self.cslots[1], self.gain);
        Ok(())
    }

    /// `VCVSpTable`: `gain` (not principal).
    fn sens_params(&self, _ac: bool) -> Option<Vec<SensParam>> {
        Some(vec![sens_param("_gain", self.gain)])
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_gain" {
            self.gain = value;
        }
    }
}

/// `VCCSload`: G source.
pub(crate) struct VccsI {
    pub gain: f64,
    n: [Var; 4],
    slots: [Slot; 4],
}

impl VccsI {
    pub fn new(gain: f64, n: [Var; 4]) -> Self {
        VccsI {
            gain,
            n,
            slots: [TRASH; 4],
        }
    }
}

impl Device for VccsI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let [pos, neg, cpos, cneg] = self.n;
        self.slots = [
            mb.slot(pos, cpos),
            mb.slot(pos, cneg),
            mb.slot(neg, cpos),
            mb.slot(neg, cneg),
        ];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.gain);
        ctx.add(self.slots[1], -self.gain);
        ctx.add(self.slots[2], -self.gain);
        ctx.add(self.slots[3], self.gain);
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.slots[0], self.gain);
        ctx.add_g(self.slots[1], -self.gain);
        ctx.add_g(self.slots[2], -self.gain);
        ctx.add_g(self.slots[3], self.gain);
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.slots[0], self.gain);
        ctx.add_g(self.slots[1], -self.gain);
        ctx.add_g(self.slots[2], -self.gain);
        ctx.add_g(self.slots[3], self.gain);
        Ok(())
    }

    /// `VCCSpTable`: `gain` and `m`. ngspice folds `m` into the
    /// coefficient before the analyses, so moving it afterwards does
    /// nothing (the golden's `g*_m` vectors are exactly zero).
    fn sens_params(&self, _ac: bool) -> Option<Vec<SensParam>> {
        let mut v = vec![sens_param("_gain", self.gain)];
        v.extend(sens_inert(&["_m"]));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_gain" {
            self.gain = value;
        }
    }
}

/// `CCCSload`: F source.
pub(crate) struct CccsI {
    pub gain: f64,
    pos: Var,
    neg: Var,
    ctrl_br: Var,
    slots: [Slot; 2],
}

impl CccsI {
    pub fn new(gain: f64, pos: Var, neg: Var, ctrl_br: Var) -> Self {
        CccsI {
            gain,
            pos,
            neg,
            ctrl_br,
            slots: [TRASH; 2],
        }
    }
}

impl Device for CccsI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots = [
            mb.slot(self.pos, self.ctrl_br),
            mb.slot(self.neg, self.ctrl_br),
        ];
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.gain);
        ctx.add(self.slots[1], -self.gain);
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.slots[0], self.gain);
        ctx.add_g(self.slots[1], -self.gain);
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.slots[0], self.gain);
        ctx.add_g(self.slots[1], -self.gain);
        Ok(())
    }

    /// `CCCSpTable`: `gain` and an inert `m` (as for the VCCS).
    fn sens_params(&self, _ac: bool) -> Option<Vec<SensParam>> {
        let mut v = vec![sens_param("_gain", self.gain)];
        v.extend(sens_inert(&["_m"]));
        Some(v)
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_gain" {
            self.gain = value;
        }
    }
}

/// `CCVSload`: H source.
pub(crate) struct CcvsI {
    pub coeff: f64,
    branch: BranchStamp,
    ctrl_br: Var,
    cslot: Slot,
}

impl CcvsI {
    pub fn new(coeff: f64, pos: Var, neg: Var, br: Var, ctrl_br: Var) -> Self {
        CcvsI {
            coeff,
            branch: BranchStamp::new(pos, neg, br),
            ctrl_br,
            cslot: TRASH,
        }
    }
}

impl Device for CcvsI {
    fn par_load(&self) -> bool {
        true
    }

    disto_linear!();

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.branch.alloc(mb);
        self.cslot = mb.slot(self.branch.br, self.ctrl_br);
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        ctx.add(self.cslot, -self.coeff);
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        ctx.add_g(self.cslot, -self.coeff);
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        ctx.add_g(self.cslot, -self.coeff);
        Ok(())
    }

    /// `CCVSpTable`: `gain` (the transresistance).
    fn sens_params(&self, _ac: bool) -> Option<Vec<SensParam>> {
        Some(vec![sens_param("_gain", self.coeff)])
    }

    fn sens_set(&mut self, suffix: &str, value: f64) {
        if suffix == "_gain" {
            self.coeff = value;
        }
    }
}
