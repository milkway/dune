//! MNA bookkeeping: unknowns are 1-based like the C (`CKTrhs[0]` is
//! the ground/trash position), and devices hold *slots* into a value
//! array instead of raw pointers (the C's `TSTALLOC` pointers into
//! the sparse matrix).

use std::collections::HashMap;

use crate::linalg::Pattern;

/// Index of an unknown: 0 is ground (writes vanish), 1..=n are real.
pub(crate) type Var = usize;

/// Index into the value array; slot 0 is the trash position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot(pub(crate) usize);

pub(crate) const TRASH: Slot = Slot(0);

/// Collects the sparsity pattern while devices allocate their
/// positions, then freezes into a [`Pattern`].
pub(crate) struct MatrixBuilder {
    n: usize,
    map: HashMap<(Var, Var), Slot>,
    entries: Vec<(usize, usize)>,
    /// Slots by matrix row (1-based), for the `.nodeset` row zeroing.
    row_slots: Vec<Vec<Slot>>,
}

impl MatrixBuilder {
    pub fn new(n: usize) -> Self {
        MatrixBuilder {
            n,
            map: HashMap::new(),
            entries: Vec::new(),
            row_slots: vec![Vec::new(); n + 1],
        }
    }

    /// Slot for position (r, c); ground row/column goes to trash.
    pub fn slot(&mut self, r: Var, c: Var) -> Slot {
        if r == 0 || c == 0 {
            return TRASH;
        }
        debug_assert!(r <= self.n && c <= self.n);
        if let Some(s) = self.map.get(&(r, c)) {
            return *s;
        }
        self.entries.push((r - 1, c - 1));
        let s = Slot(self.entries.len()); // slot 0 is trash
        self.map.insert((r, c), s);
        self.row_slots[r].push(s);
        s
    }

    pub fn pattern(&self) -> Pattern {
        Pattern {
            n: self.n,
            entries: self.entries.clone(),
        }
    }

    /// Number of value positions (slots), trash included.
    pub fn num_slots(&self) -> usize {
        self.entries.len() + 1
    }

    pub fn slots_in_row(&self, r: Var) -> &[Slot] {
        &self.row_slots[r]
    }

    /// All (row, slot) pairs of column `c` (the pole-zero column
    /// surgery needs them: `SMPcZeroCol`/`SMPcAddCol`).
    pub fn rows_in_col(&self, c: Var) -> Vec<(Var, Slot)> {
        let mut v: Vec<(Var, Slot)> = self
            .map
            .iter()
            .filter(|((_, cc), _)| *cc == c)
            .map(|((r, _), s)| (*r, *s))
            .collect();
        v.sort_unstable_by_key(|(r, _)| *r);
        v
    }
}

/// Analysis-kind bits of `CKTmode`: the C's `MODEDCOP`,
/// `MODETRANOP`, `MODEDCTRANCURVE` and `MODETRAN` (the composite
/// `MODEDC` mask is [`LoadCtx::is_dc`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnKind {
    DcOp,
    TranOp,
    DcSweep,
    Tran,
}

/// Everything a device sees while loading for one Newton iteration
/// (the `CKTload` environment).
pub(crate) struct LoadCtx<'a> {
    /// Matrix values, slot-indexed; `[0]` is trash.
    pub values: &'a mut [f64],
    /// Right-hand side, var-indexed; `[0]` is trash.
    pub rhs: &'a mut [f64],
    /// Previous solution (`CKTrhsOld`), var-indexed, `[0] == 0`.
    pub sol: &'a [f64],
    pub mode: InitMode,
    pub kind: AnKind,
    /// `MODEUIC`.
    pub uic: bool,
    /// `CKTtime` (0 in DC modes).
    pub time: f64,
    /// `CKTstep` — the user's TSTEP, which source-function defaults
    /// depend on (0 outside transient jobs).
    pub step: f64,
    /// `CKTfinalTime` (0 outside transient jobs).
    pub final_time: f64,
    pub gmin: f64,
    /// `CKTsrcFact`, the source-stepping scale.
    pub srcfact: f64,
    /// Set by devices that limited (`CKTnoncon`).
    pub noncon: bool,
    /// Device state banks.
    pub states: &'a mut crate::state::States,
    /// Integration coefficients (`CKTag`), order and method; only
    /// meaningful when `kind == Tran`.
    pub ag: [f64; 7],
    pub order: usize,
    pub method: crate::integ::Method,
    /// `CKTdelta / CKTdeltaOld[1]`, for `DEVpred`.
    pub pred_xfact: f64,
    /// `CKTdelta` (0 outside transient).
    pub delta: f64,
    /// Set by a device whose load cannot proceed (the C's `E_INTERN`
    /// returns from `DEVload`); the Newton call then fails, which the
    /// transient loop treats like non-convergence.
    pub fault: Option<&'static str>,
}

/// The `INITF` state the Newton loop drives (`MODEINITJCT`,
/// `MODEINITFIX`, `MODEINITFLOAT`, `MODEINITTRAN`, `MODEINITPRED`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitMode {
    Jct,
    Fix,
    Float,
    /// First timepoint after the transient operating point.
    Tran,
    /// First Newton iteration of every later timepoint.
    Pred,
    /// Small-signal parameter storage pass before `.ac`
    /// (`MODEINITSMSIG`).
    Smsig,
}

impl LoadCtx<'_> {
    pub fn add(&mut self, s: Slot, v: f64) {
        self.values[s.0] += v;
    }

    /// The C's `MODEDC` composite mask.
    pub fn is_dc(&self) -> bool {
        matches!(self.kind, AnKind::DcOp | AnKind::TranOp | AnKind::DcSweep)
    }

    /// `NIintegrate` against the current coefficients.
    pub fn integrate(&mut self, cap: f64, q: usize) -> (f64, f64) {
        crate::integ::integrate(self.states, &self.ag, self.order, self.method, cap, q)
    }

    /// `DEVpred` (`devsup.c`): extrapolate a state linearly from the
    /// two previous timepoints.
    pub fn pred(&self, off: usize) -> f64 {
        (1.0 + self.pred_xfact) * self.states.get(1, off)
            - self.pred_xfact * self.states.get(2, off)
    }
}

/// The pole-zero load environment (`CKTpzLoad` → `dev/*pzld.c`):
/// the complex MNA matrix at an arbitrary complex frequency `s`,
/// split into parallel real/imaginary slot arrays like [`AcCtx`].
pub(crate) struct PzCtx<'a> {
    pub g: &'a mut [f64],
    pub b: &'a mut [f64],
    /// The complex frequency under trial.
    pub s: num_complex::Complex64,
    /// Small-signal parameters from the `MODEINITSMSIG` pass.
    pub states: &'a crate::state::States,
}

impl PzCtx<'_> {
    pub fn add_g(&mut self, sl: Slot, v: f64) {
        self.g[sl.0] += v;
    }

    /// Stamp `s·c` — a capacitance (or inductance on a branch row)
    /// evaluated at the trial frequency.
    pub fn add_cap(&mut self, sl: Slot, c: f64) {
        self.g[sl.0] += self.s.re * c;
        self.b[sl.0] += self.s.im * c;
    }
}

/// What a device's noise evaluation sees (`CKTnoise` → `*noise.c`):
/// the adjoint AC solution (transfer from each internal source to
/// the output) and the shared per-frequency data.
pub(crate) struct NoiseCtx<'a> {
    /// Adjoint solution, real and imaginary parts, var-indexed.
    pub re: &'a [f64],
    pub im: &'a [f64],
    pub freq: f64,
    /// `CKTtemp`, kelvin.
    pub temp: f64,
    pub states: &'a crate::state::States,
}

impl NoiseCtx<'_> {
    /// `|V(n1) − V(n2)|²` of the adjoint solution (the squared
    /// transfer gain in `NevalSrc`).
    pub fn gain2(&self, n1: Var, n2: Var) -> f64 {
        let r = self.re[n1] - self.re[n2];
        let i = self.im[n1] - self.im[n2];
        r * r + i * i
    }

    /// `NevalSrc(THERMNOISE)`: `4kT·g` times the squared gain.
    pub fn thermal(&self, n1: Var, n2: Var, g: f64) -> f64 {
        self.gain2(n1, n2) * 4.0 * crate::consts::BOLTZ * self.temp * g
    }

    /// `NevalSrc(SHOTNOISE)`: `2q|I|` times the squared gain.
    pub fn shot(&self, n1: Var, n2: Var, current: f64) -> f64 {
        self.gain2(n1, n2) * 2.0 * crate::consts::CHARGE * current.abs()
    }
}

/// What a device's convergence test sees (`CKTconvTest` →
/// `dev/*conv.c`, active under NEWCONV): the fresh solution
/// (`CKTrhs`), the previous one (`CKTrhsOld`) and the state banks
/// the last load wrote.
pub(crate) struct ConvCtx<'a> {
    /// New solution, var-indexed (`CKTrhs` after the solve).
    pub rhs: &'a [f64],
    /// Previous solution (`CKTrhsOld`).
    pub sol: &'a [f64],
    pub states: &'a crate::state::States,
    pub reltol: f64,
    pub abstol: f64,
}

/// The AC load environment (`CKTacLoad`): the complex matrix
/// `G + jB` over the same sparsity pattern, split into parallel
/// real/imaginary slot arrays, plus the complex RHS.
pub(crate) struct AcCtx<'a> {
    pub g: &'a mut [f64],
    pub b: &'a mut [f64],
    pub rhs_re: &'a mut [f64],
    pub rhs_im: &'a mut [f64],
    /// `CKTomega = 2πf`.
    pub omega: f64,
    /// Small-signal parameters stored by the `MODEINITSMSIG` pass
    /// (`CKTstate0`, read-only here).
    pub states: &'a crate::state::States,
}

impl AcCtx<'_> {
    pub fn add_g(&mut self, s: Slot, v: f64) {
        self.g[s.0] += v;
    }

    pub fn add_b(&mut self, s: Slot, v: f64) {
        self.b[s.0] += v;
    }
}

/// What a device's `accept` hook sees at an accepted timepoint
/// (`CKTaccept`); breakpoint requests are merged by the engine's
/// `CKTsetBreak` port.
pub(crate) struct AcceptCtx<'a> {
    pub time: f64,
    /// `MODEINITTRAN`: the accept of the transient operating point.
    pub first: bool,
    /// The accepted solution (`CKTrhsOld`), var-indexed.
    pub sol: &'a [f64],
    /// `CKTreltol`, `CKTabstol`.
    pub reltol: f64,
    pub abstol: f64,
    /// `CKTbreak`: this timepoint sits on a breakpoint.
    pub brk: bool,
    pub step: f64,
    pub final_time: f64,
    pub requests: &'a mut Vec<f64>,
}

impl AcceptCtx<'_> {
    pub fn set_break(&mut self, t: f64) {
        self.requests.push(t);
    }
}
