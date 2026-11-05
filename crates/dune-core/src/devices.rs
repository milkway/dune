//! Device instances and their stamps. One type per device (CLAUDE.md
//! Princípios 3); stamps and state handling follow the 3f5 `*load.c`
//! / `*trunc.c` / `*acct.c` files named on each type.

use crate::integ::TerrCtx;
use crate::matrix::{
    AcCtx, AcceptCtx, AnKind, ConvCtx, InitMode, LoadCtx, MatrixBuilder, PzCtx, Slot, TRASH, Var,
};
use crate::srcfn::Wave;

/// A device instance bound to matrix positions and state slots.
pub(crate) trait Device {
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
    /// Timestep limiting from local truncation error (`*trunc.c`).
    fn trunc(&self, _t: &TerrCtx, _time_step: &mut f64) {}
    /// AC small-signal stamps (`*acld.c`). Default: no AC presence.
    fn load_ac(&mut self, _ctx: &mut AcCtx) {}
    /// Accepted-timepoint hook; sources plant breakpoints here
    /// (`*acct.c`).
    fn accept(&mut self, _ctx: &mut AcceptCtx) {}
    /// Overwrite the DC value (`.dc` sweep). No-op elsewhere.
    fn set_dc(&mut self, _value: f64) {}
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
}

/// `RESload` (`dev/res/resload.c`): conductance between two nodes.
pub(crate) struct ResistorI {
    pub g: f64,
    pub pos: Var,
    pub neg: Var,
    slots: [Slot; 4],
}

impl ResistorI {
    pub fn new(g: f64, pos: Var, neg: Var) -> Self {
        ResistorI {
            g,
            pos,
            neg,
            slots: [TRASH; 4],
        }
    }
}

impl Device for ResistorI {
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

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.slots[0], self.g);
        ctx.add_g(self.slots[1], self.g);
        ctx.add_g(self.slots[2], -self.g);
        ctx.add_g(self.slots[3], -self.g);
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.slots[0], self.g);
        ctx.add_g(self.slots[1], self.g);
        ctx.add_g(self.slots[2], -self.g);
        ctx.add_g(self.slots[3], -self.g);
        Ok(())
    }
}

/// `CAPload` (`dev/cap/capload.c`): open at DC, integrated charge in
/// transient. States: charge, companion current.
pub(crate) struct CapacitorI {
    capac: f64,
    ic: f64,
    pos: Var,
    neg: Var,
    slots: [Slot; 4],
    q: usize,
}

impl CapacitorI {
    pub fn new(capac: f64, ic: f64, pos: Var, neg: Var, q: usize) -> Self {
        CapacitorI {
            capac,
            ic,
            pos,
            neg,
            slots: [TRASH; 4],
            q,
        }
    }
}

impl Device for CapacitorI {
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
            ctx.rhs[self.pos] -= ceq;
            ctx.rhs[self.neg] += ceq;
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
        ctx.rhs[self.br] += veq;
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
    pub br1: Var,
    pub br2: Var,
    pub flux1: usize,
    pub flux2: usize,
    slots: [Slot; 2],
}

impl Device for MutualI {
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

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_cap(self.slots[0], -self.factor);
        ctx.add_cap(self.slots[1], -self.factor);
        Ok(())
    }
}

impl MutualI {
    pub fn new(factor: f64, br1: Var, br2: Var, flux1: usize, flux2: usize) -> Self {
        MutualI {
            factor,
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
fn source_value(ctx: &LoadCtx, dc: Option<f64>, wave: &Option<Wave>) -> f64 {
    if matches!(ctx.kind, AnKind::DcOp | AnKind::DcSweep) && dc.is_some() {
        return ctx.srcfact * dc.unwrap_or(0.0);
    }
    let v = match wave {
        Some(w) => {
            let time = if ctx.is_dc() { 0.0 } else { ctx.time };
            w.value(time, ctx.step, ctx.final_time)
        }
        None => dc.unwrap_or(0.0),
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
    /// `VSRCacReal`/`VSRCacImag` (`vsrctemp.c`: phase in degrees).
    ac_re: f64,
    ac_im: f64,
    branch: BranchStamp,
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
            ac_re,
            ac_im,
            branch: BranchStamp::new(pos, neg, br),
        }
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
    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.branch.alloc(mb);
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        let v = source_value(ctx, self.dc, &self.wave);
        ctx.rhs[self.branch.br] += v;
    }

    /// `VSRCacLoad`.
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        ctx.rhs_re[self.branch.br] += self.ac_re;
        ctx.rhs_im[self.branch.br] += self.ac_im;
    }

    /// `VSRCpzLoad`: the branch incidence, no excitation (the PZ
    /// drive comes from the column surgery).
    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        ctx.add_g(self.branch.slots[0], 1.0);
        ctx.add_g(self.branch.slots[1], 1.0);
        ctx.add_g(self.branch.slots[2], -1.0);
        ctx.add_g(self.branch.slots[3], -1.0);
        Ok(())
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if let Some(w) = &self.wave {
            w.accept(ctx);
        }
    }

    fn set_dc(&mut self, value: f64) {
        self.dc = Some(value);
    }
}

/// `ISRCload` (`dev/isrc/isrcload.c`).
pub(crate) struct IsourceI {
    pub dc: Option<f64>,
    pub wave: Option<Wave>,
    ac_re: f64,
    ac_im: f64,
    pub pos: Var,
    pub neg: Var,
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
            ac_re,
            ac_im,
            pos,
            neg,
        }
    }
}

impl Device for IsourceI {
    fn alloc(&mut self, _: &mut MatrixBuilder) {}

    fn load(&mut self, ctx: &mut LoadCtx) {
        let v = source_value(ctx, self.dc, &self.wave);
        ctx.rhs[self.pos] -= v;
        ctx.rhs[self.neg] += v;
    }

    /// `ISRCacLoad`. The 3f5 adds the AC stimulus with the polarity
    /// opposite to its own DC load (a long-known sign slip); ngspice
    /// fixed it, and the oracle confirmed the flip (180° on a
    /// current-driven deck). Dune follows ngspice
    /// (corpus/KNOWN_DIFFS.md).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        ctx.rhs_re[self.pos] -= self.ac_re;
        ctx.rhs_re[self.neg] += self.ac_re;
        ctx.rhs_im[self.pos] -= self.ac_im;
        ctx.rhs_im[self.neg] += self.ac_im;
    }

    /// `ISRCpzLoad`: an ideal current source is open-circuit.
    fn load_pz(&mut self, _ctx: &mut PzCtx) -> Result<(), &'static str> {
        Ok(())
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        if let Some(w) = &self.wave {
            w.accept(ctx);
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
}
