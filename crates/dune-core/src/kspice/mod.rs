//! The KSPICE transmission lines of ngspice-47: the single lossy
//! line `TXL` (`Y` cards, `devices/txl/`) and the coupled
//! multiconductor lines `CPL` (`P` cards, `devices/cpl/`).
//!
//! Translated from the ngspice-47 sources, which carry
//! "Copyright 1992 Regents of the University of California. All
//! rights reserved. Author: 1992 Charles Hough" (CPL setup also
//! "Modified: 2004 Paolo Nenzi - (ng)spice integration"; the Jacobi
//! diagonalization and the SWEC data structures are by Shen Lin,
//! 1990-1991, same copyright), under the Modified BSD licence.
//!
//! Both devices model the line by Padé approximations of the
//! characteristic admittance and of the propagation function, fitted
//! at setup, and convolve the port history recursively at each
//! timepoint (docs/kspice-lines.md). The C keeps its working data in
//! file-scope statics, global node tables and free lists; here every
//! piece of it lives in the device instance or in a setup value
//! (docs/kspice-lines.md, "Estado global").

mod cpl;
mod cplsetup;
mod txl;

#[cfg(test)]
mod tests;

pub(crate) use cpl::{CplI, CplParams};
pub use cpl::{CplModel, MAX_CPL_LINES};
pub use txl::TxlModel;
pub(crate) use txl::{TxlI, TxlParams};

use crate::matrix::{Slot, Var};

/// One term `c / (s − x)` of a partial-fraction expansion, with its
/// recursive-convolution states at the line's input and output
/// (`TERM` of `swec.h`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Term {
    pub c: f64,
    pub x: f64,
    pub cnv_i: f64,
    pub cnv_o: f64,
}

/// `(int)(t * 1e12)`: the integer picoseconds both loads keep their
/// history in. The C conversion is undefined outside the `int` range;
/// Rust's saturates (what arm64 does in hardware) — a run past about
/// 2.147 ms pins every later time at `i32::MAX`
/// (docs/kspice-lines.md, "Fidelidade").
pub(crate) fn ps(t: f64) -> i32 {
    (t * 1e12) as i32
}

/// `expC` of `txlload.c`/`cplload.c`: `e^{ar·h}·(cos ai·h, sin ai·h)`.
pub(crate) fn exp_c(ar: f64, ai: f64, h: f64) -> (f64, f64) {
    let e = (ar * h).exp();
    let cs = (ai * h).cos();
    let si = (ai * h).sin();
    (e * cs, e * si)
}

/// `multC`: the complex product, in the C's operation order.
pub(crate) fn mult_c(ar: f64, ai: f64, br: f64, bi: f64) -> (f64, f64) {
    let tp = ar * br - ai * bi;
    let ci = ar * bi + ai * br;
    (tp, ci)
}

/// The stamps of one load of a KSPICE line. The C *assigns* most of
/// its matrix entries and both right-hand-side rows (`*ptr = v`) —
/// entries no other device touches, cleared by `CKTload` before every
/// load — so an assignment equals an addition into zero, except when
/// two assigned pointers alias one position (a line whose ends share
/// a node): then the later assignment wins. `set` keeps exactly that
/// rule; the buffer is added into the matrix once the load is done.
#[derive(Default)]
pub(crate) struct SetBuf {
    pub m: Vec<(Slot, f64)>,
    pub r: Vec<(Var, f64)>,
}

impl SetBuf {
    pub fn clear(&mut self) {
        self.m.clear();
        self.r.clear();
    }

    /// `*ptr = v`.
    pub fn set(&mut self, s: Slot, v: f64) {
        match self.m.iter_mut().find(|(k, _)| *k == s) {
            Some(e) => e.1 = v,
            None => self.m.push((s, v)),
        }
    }

    /// `*(ckt->CKTrhs + var) = v`.
    pub fn set_rhs(&mut self, var: Var, v: f64) {
        match self.r.iter_mut().find(|(k, _)| *k == var) {
            Some(e) => e.1 = v,
            None => self.r.push((var, v)),
        }
    }
}
