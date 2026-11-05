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

    /// `SMPcaSolve`: solve `Aᵀ x = b` from the matrix stored by the
    /// last [`LinearSolver::factor_complex`] (a fresh LU of the
    /// transpose; noise matrices are corpus-sized).
    fn solve_complex_transposed_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError> {
        debug_assert_eq!(rhs_re.len(), self.n);
        let at: Mat<c64> = Mat::from_fn(self.n, self.n, |i, j| self.cmat[(j, i)]);
        let lu = at.partial_piv_lu();
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

    /// Complex determinant with a dedicated small LU: partial
    /// pivoting on `|re|+|im|`, the running product kept normalized
    /// to `max(|re|,|im|) ∈ (0.5, 1]` with a base-2 exponent, like
    /// the 3f5's `SMPcDProd`/`C_NORM`. Pole-zero matrices are
    /// corpus-sized, so O(n³) dense is fine here.
    fn det_complex(&mut self, re: &[f64], im: &[f64]) -> (num_complex::Complex64, i32) {
        use num_complex::Complex64;
        debug_assert_eq!(re.len(), self.entries.len());
        let n = self.n;
        if n == 0 {
            return (Complex64::new(1.0, 0.0), 0);
        }
        let mut a = vec![Complex64::new(0.0, 0.0); n * n];
        for (k, (r, c)) in self.entries.iter().enumerate() {
            a[r * n + c] += Complex64::new(re[k], im[k]);
        }
        let mut det = Complex64::new(1.0, 0.0);
        let mut exp = 0i32;
        let mut sign = 1.0f64;
        for col in 0..n {
            // Pivot: largest |re|+|im| on or below the diagonal.
            let mut piv = col;
            let mut best = a[col * n + col].re.abs() + a[col * n + col].im.abs();
            for r in col + 1..n {
                let m = a[r * n + col].re.abs() + a[r * n + col].im.abs();
                if m > best {
                    best = m;
                    piv = r;
                }
            }
            if best == 0.0 {
                return (Complex64::new(0.0, 0.0), 0);
            }
            if piv != col {
                for c in 0..n {
                    a.swap(piv * n + c, col * n + c);
                }
                sign = -sign;
            }
            let p = a[col * n + col];
            det *= p;
            if det.re == 0.0 && det.im == 0.0 {
                return (Complex64::new(0.0, 0.0), 0);
            }
            // `C_NORM`
            while det.re.abs() > 1.0 || det.im.abs() > 1.0 {
                exp += 1;
                det.re /= 2.0;
                det.im /= 2.0;
            }
            while det.re.abs() <= 0.5 && det.im.abs() <= 0.5 {
                exp -= 1;
                det.re *= 2.0;
                det.im *= 2.0;
            }
            for r in col + 1..n {
                let f = a[r * n + col] / p;
                if f.re == 0.0 && f.im == 0.0 {
                    continue;
                }
                for c in col + 1..n {
                    let v = a[col * n + c];
                    a[r * n + c] -= f * v;
                }
            }
        }
        if !det.re.is_finite() || !det.im.is_finite() {
            return (Complex64::new(0.0, 0.0), 0);
        }
        (det * sign, exp)
    }
}
