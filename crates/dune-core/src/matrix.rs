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
    pub gmin: f64,
    /// `CKTsrcFact`, the source-stepping scale.
    pub srcfact: f64,
    /// Set by devices that limited or failed their own convergence
    /// test (`CKTnoncon`).
    pub noncon: bool,
    pub reltol: f64,
    pub abstol: f64,
}

/// The `INITF` state the Newton loop drives (`MODEINITJCT`,
/// `MODEINITFIX`, `MODEINITFLOAT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitMode {
    Jct,
    Fix,
    Float,
}

impl LoadCtx<'_> {
    pub fn add(&mut self, s: Slot, v: f64) {
        self.values[s.0] += v;
    }
}
