//! Linear solver abstraction (D-0005). Devices never see the backend;
//! nothing outside `linalg::backend` may call `faer` (or, later,
//! `xingo-sparse`) directly.
//!
//! The split mirrors the 3f5's SMP layer: `analyze` fixes the
//! sparsity pattern (`SMPpreOrder`), `factor` refactors with new
//! values and the diagonal gmin of `CKTop`'s gmin stepping
//! (`SMPluFac` + `LoadGmin`), `solve_in_place` back-substitutes
//! (`SMPsolve`).

pub mod backend;

/// Matrix sparsity pattern: `n`×`n`, ground already removed
/// (0-based). `entries` are unique positions; values passed to
/// [`LinearSolver::factor`] are parallel to it.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub n: usize,
    pub entries: Vec<(usize, usize)>,
}

#[derive(Debug, thiserror::Error)]
pub enum LinalgError {
    /// Structural or numerical singularity; `row` is the 0-based
    /// matrix row (caller maps it back to a node name).
    #[error("singular matrix at row {row}")]
    Singular { row: usize },
}

pub trait LinearSolver {
    /// Fix the pattern for subsequent factorizations.
    fn analyze(&mut self, pattern: &Pattern);

    /// Factor with `values` (parallel to the pattern's entries).
    /// `diag_gmin` is added to every diagonal position present in
    /// the pattern, exactly like the 3f5's `LoadGmin`.
    fn factor(&mut self, values: &[f64], diag_gmin: f64) -> Result<(), LinalgError>;

    /// Solve `A x = rhs`, overwriting `rhs` with `x`.
    fn solve_in_place(&mut self, rhs: &mut [f64]) -> Result<(), LinalgError>;
}

/// The default backend for this build.
pub fn default_solver() -> Box<dyn LinearSolver> {
    #[cfg(feature = "faer")]
    {
        Box::new(backend::faer_dense::FaerDense::default())
    }
    #[cfg(not(feature = "faer"))]
    {
        compile_error!("dune-core needs a linear solver backend (enable the `faer` feature)");
    }
}
