//! Dense LU backend over `faer` (partial pivoting). Corpus-sized
//! circuits are tiny; sparsity (faer sparse or the xingo-sparse
//! circuit profile) is Phase-3 work with its own benchmark (P-3).

use faer::linalg::solvers::{PartialPivLu, Solve};
use faer::{Mat, MatMut, c64};

use super::super::{LinalgError, LinearSolver, Pattern};

pub struct FaerDense {
    n: usize,
    entries: Vec<(usize, usize)>,
    /// Diagonal positions present in the pattern (gmin landing spots,
    /// mirroring `LoadGmin`: only existing diagonals get gmin).
    diags: Vec<usize>,
    mat: Mat<f64>,
    lu: Option<PartialPivLu<f64>>,
    cmat: Mat<c64>,
    clu: Option<PartialPivLu<c64>>,
}

impl Default for FaerDense {
    fn default() -> Self {
        FaerDense {
            n: 0,
            entries: Vec::new(),
            diags: Vec::new(),
            mat: Mat::new(),
            lu: None,
            cmat: Mat::new(),
            clu: None,
        }
    }
}

impl LinearSolver for FaerDense {
    fn analyze(&mut self, pattern: &Pattern) {
        self.n = pattern.n;
        self.entries = pattern.entries.clone();
        self.diags = pattern
            .entries
            .iter()
            .enumerate()
            .filter_map(|(k, (r, c))| (r == c).then_some(k))
            .map(|k| self.entries[k].0)
            .collect();
        self.mat = Mat::zeros(self.n, self.n);
        self.lu = None;
        self.cmat = Mat::zeros(self.n, self.n);
        self.clu = None;
    }

    fn factor(&mut self, values: &[f64], diag_gmin: f64) -> Result<(), LinalgError> {
        debug_assert_eq!(values.len(), self.entries.len());
        self.mat.fill(0.0);
        for ((r, c), v) in self.entries.iter().zip(values) {
            self.mat[(*r, *c)] += v;
        }
        if diag_gmin != 0.0 {
            for &d in &self.diags {
                self.mat[(d, d)] += diag_gmin;
            }
        }
        // A structurally empty row or column is the classic "no DC
        // path to ground" / floating-node singularity; report it
        // before LU turns it into NaNs.
        for i in 0..self.n {
            let row_zero = (0..self.n).all(|j| self.mat[(i, j)] == 0.0);
            let col_zero = (0..self.n).all(|j| self.mat[(j, i)] == 0.0);
            if row_zero || col_zero {
                return Err(LinalgError::Singular { row: i });
            }
        }
        self.lu = Some(self.mat.partial_piv_lu());
        Ok(())
    }

    fn solve_in_place(&mut self, rhs: &mut [f64]) -> Result<(), LinalgError> {
        let lu = self.lu.as_ref().expect("factor before solve");
        debug_assert_eq!(rhs.len(), self.n);
        let m = MatMut::from_column_major_slice_mut(rhs, self.n, 1);
        lu.solve_in_place(m);
        if let Some(row) = rhs.iter().position(|v| !v.is_finite()) {
            return Err(LinalgError::Singular { row });
        }
        Ok(())
    }

    fn factor_complex(&mut self, re: &[f64], im: &[f64]) -> Result<(), LinalgError> {
        debug_assert_eq!(re.len(), self.entries.len());
        self.cmat.fill(c64::new(0.0, 0.0));
        for (k, (r, c)) in self.entries.iter().enumerate() {
            self.cmat[(*r, *c)] += c64::new(re[k], im[k]);
        }
        for i in 0..self.n {
            let row_zero = (0..self.n).all(|j| self.cmat[(i, j)] == c64::new(0.0, 0.0));
            let col_zero = (0..self.n).all(|j| self.cmat[(j, i)] == c64::new(0.0, 0.0));
            if row_zero || col_zero {
                return Err(LinalgError::Singular { row: i });
            }
        }
        self.clu = Some(self.cmat.partial_piv_lu());
        Ok(())
    }

    fn solve_complex_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError> {
        let lu = self.clu.as_ref().expect("factor before solve");
        debug_assert_eq!(rhs_re.len(), self.n);
        let mut rhs: Vec<c64> = rhs_re
            .iter()
            .zip(rhs_im.iter())
            .map(|(r, i)| c64::new(*r, *i))
            .collect();
        let m = MatMut::from_column_major_slice_mut(&mut rhs, self.n, 1);
        lu.solve_in_place(m);
        for (k, v) in rhs.iter().enumerate() {
            if !v.re.is_finite() || !v.im.is_finite() {
                return Err(LinalgError::Singular { row: k });
            }
            rhs_re[k] = v.re;
            rhs_im[k] = v.im;
        }
        Ok(())
    }
}
