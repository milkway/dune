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

    /// Factor the complex matrix `G + jB` over the same pattern
    /// (`SMPcLUfac`); `re`/`im` are parallel to the pattern entries.
    fn factor_complex(&mut self, re: &[f64], im: &[f64]) -> Result<(), LinalgError>;

    /// Solve the complex system in place (`SMPcSolve`).
    fn solve_complex_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError>;

    /// Solve the **transposed** complex system in place
    /// (`SMPcaSolve`, the noise adjoint). Requires a prior
    /// [`LinearSolver::factor_complex`].
    fn solve_complex_transposed_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError>;

    /// Determinant of the complex matrix `G + jB` as a base-2
    /// mantissa/exponent pair (`SMPcLUfac` + `SMPcDProd`): the
    /// mantissa is normalized so `max(|re|,|im|) ∈ (0.5, 1]` like
    /// the 3f5's `C_NORM`. A singular matrix returns a zero
    /// mantissa — the pole-zero search treats that as a root.
    fn det_complex(&mut self, re: &[f64], im: &[f64]) -> (num_complex::Complex64, i32);
}

/// The default backend for this build.
pub fn default_solver() -> Box<dyn LinearSolver> {
    #[cfg(feature = "solver-faer")]
    {
        Box::new(backend::faer_dense::FaerDense::default())
    }
    #[cfg(not(feature = "solver-faer"))]
    {
        compile_error!(
            "dune-core needs a linear solver backend (enable the `solver-faer` feature)"
        );
    }
}
