//! Device instances and their DC stamps. One type per device
//! (CLAUDE.md, Princípios 3), stamps copied from the 3f5 `*load.c`
//! files named on each type.

use crate::matrix::{LoadCtx, MatrixBuilder, Slot, TRASH, Var};

/// A device instance bound to matrix positions.
pub(crate) trait Device {
    /// Reserve matrix positions (the `TSTALLOC` calls in `*setup.c`).
    fn alloc(&mut self, mb: &mut MatrixBuilder);
    /// Load the DC/operating-point stamps for one Newton iteration.
    fn load_dc(&mut self, ctx: &mut LoadCtx);
    /// Overwrite the DC value (a `.dc` sweep writing `VSRCdcValue` /
    /// `ISRCdcValue`, `dctrcurv.c`). No-op for everything else.
    fn set_dc(&mut self, _value: f64) {}
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

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.g);
        ctx.add(self.slots[1], self.g);
        ctx.add(self.slots[2], -self.g);
        ctx.add(self.slots[3], -self.g);
    }
}

/// A capacitor is an open circuit at the operating point
/// (`CAPload` only stamps in transient/AC modes).
pub(crate) struct CapacitorI;

impl Device for CapacitorI {
    fn alloc(&mut self, _: &mut MatrixBuilder) {}
    fn load_dc(&mut self, _: &mut LoadCtx) {}
}

/// Mutual inductance has no DC effect (`MUTload` stamps only the
/// time-derivative terms).
pub(crate) struct MutualI;

impl Device for MutualI {
    fn alloc(&mut self, _: &mut MatrixBuilder) {}
    fn load_dc(&mut self, _: &mut LoadCtx) {}
}

/// Zero-volt branch between two nodes: the DC view of an inductor
/// (`INDload` under `MODEDC`) and the skeleton of a voltage source.
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

/// `VSRCload` (`dev/vsrc/vsrcload.c`): ideal voltage source; the DC
/// value is scaled by `CKTsrcFact` during source stepping.
pub(crate) struct VsourceI {
    pub dc: f64,
    branch: BranchStamp,
}

impl VsourceI {
    pub fn new(dc: f64, pos: Var, neg: Var, br: Var) -> Self {
        VsourceI {
            dc,
            branch: BranchStamp::new(pos, neg, br),
        }
    }
}

impl Device for VsourceI {
    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.branch.alloc(mb);
    }

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        ctx.rhs[self.branch.br] += ctx.srcfact * self.dc;
    }

    fn set_dc(&mut self, value: f64) {
        self.dc = value;
    }
}

/// Inductor at DC: a zero-volt source (`INDload`, `MODEDC` branch).
pub(crate) struct InductorI {
    branch: BranchStamp,
}

impl InductorI {
    pub fn new(pos: Var, neg: Var, br: Var) -> Self {
        InductorI {
            branch: BranchStamp::new(pos, neg, br),
        }
    }
}

impl Device for InductorI {
    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.branch.alloc(mb);
    }

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
    }
}

/// `ISRCload` (`dev/isrc/isrcload.c`): current flows through the
/// source from the positive to the negative node.
pub(crate) struct IsourceI {
    pub dc: f64,
    pub pos: Var,
    pub neg: Var,
}

impl Device for IsourceI {
    fn alloc(&mut self, _: &mut MatrixBuilder) {}

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        let v = ctx.srcfact * self.dc;
        ctx.rhs[self.pos] -= v;
        ctx.rhs[self.neg] += v;
    }

    fn set_dc(&mut self, value: f64) {
        self.dc = value;
    }
}

/// `VCVSload` (`dev/vcvs/vcvsload.c`): E source,
/// `v(pos) - v(neg) = gain * (v(cpos) - v(cneg))`.
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

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        ctx.add(self.cslots[0], -self.gain);
        ctx.add(self.cslots[1], self.gain);
    }
}

/// `VCCSload` (`dev/vccs/vccsload.c`): G source,
/// `i = gain * (v(cpos) - v(cneg))` out of `pos`.
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

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.gain);
        ctx.add(self.slots[1], -self.gain);
        ctx.add(self.slots[2], -self.gain);
        ctx.add(self.slots[3], self.gain);
    }
}

/// `CCCSload` (`dev/cccs/cccsload.c`): F source,
/// `i = gain * i(ctrl)`.
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

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        ctx.add(self.slots[0], self.gain);
        ctx.add(self.slots[1], -self.gain);
    }
}

/// `CCVSload` (`dev/ccvs/ccvsload.c`): H source,
/// `v(pos) - v(neg) = coeff * i(ctrl)`.
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

    fn load_dc(&mut self, ctx: &mut LoadCtx) {
        self.branch.load(ctx);
        ctx.add(self.cslot, -self.coeff);
    }
}
