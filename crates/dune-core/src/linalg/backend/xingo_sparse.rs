//! Sparse LU backend over `xingo-sparse` (D-0005's target backend;
//! feature `solver-xingo`, D-0013).
//!
//! What xingo-sparse provides and how it is used here:
//!
//! * **Real LU** — `Lu<f64>` (Markowitz with a relative column
//!   threshold after a singleton/triangular phase, Suhl & Suhl) fed
//!   through a [`ColumnSource`] that reads our own CSC arrays, so the
//!   MNA values never pass through `CscMatrix` (whose invariant
//!   forbids stored zeros, and MNA slots are often exactly zero).
//!   Solves are `ftran` (`A x = b`) and `btran` (`Aᵀ y = c`) in dense
//!   mode.
//! * **Pivot-sequence reuse.** The first [`LinearSolver::factor`]
//!   after [`LinearSolver::analyze`] is a full Markowitz
//!   factorization; every later one replays its pivot sequence
//!   (`LuFactor::refactorize_same_pattern`, xingo's Algorithm R: a
//!   left-looking elimination with no search), keeping the threshold
//!   test on every pivot. A refused pivot — the values drifted, as
//!   between the junction-initialized and the converged Newton
//!   iterates — falls back to a fresh Markowitz factorization inside
//!   the crate, whose sequence the next calls replay. This is the
//!   `SMPreorder` once / `SMPluFac` after scheme of `niiter.c`, with
//!   the reorder triggered by the pivot test instead of by a
//!   singular `SMPluFac`. On our side [`LinearSolver::analyze`] fixes
//!   the CSC structure, the slot → CSC position map and the diagonal
//!   positions for gmin, so a Newton iteration only scatters values.
//! * **No complex scalar.** `Real` is implemented for `f64` only and
//!   cannot be implemented for `Complex64` from here (orphan rule,
//!   and a complex field has no total order for the pivot test). The
//!   complex systems of `.ac`/`.noise`/`.pz`/`.disto` are solved on
//!   the real 2n embedding `[G −B; B G] [xr; xi] = [br; bi]`, whose
//!   pattern is fixed in `analyze` too. The transposed complex solve
//!   (`SMPcaSolve`) reuses that factorization: the transpose of the
//!   embedding is the embedding of `Aᴴ`, and `Aᵀ x = b` ⟺
//!   `Aᴴ x̄ = b̄`, so `btran` on conjugated data gives `x̄`.
//! * **Determinant**: the embedding only yields `|det A|²`, so
//!   [`LinearSolver::det_complex`] uses the backend-neutral dense
//!   routine shared with the faer backend (`super::dense_det`).
//!
//! Pivot options: `drop_tol = 0` (xingo's default `1e-12` is an
//! absolute drop that would erase gmin-sized conductances),
//! `abs_pivot_tol = 0` (only exact zeros are refused, like faer's
//! partial pivoting; the 3f5's `PivAbsTol` is 1e-13 but Sparse 1.3
//! still pivots on a sub-threshold entry rather than fail), and
//! `rel_pivot_tol = 1e-3`, the 3f5's `PivRelTol` (`CKTpivotRelTol`).

use xingo_sparse::{ColumnSource, Lu, LuError, LuFactor, LuOptions, SparseError, SparseVec};

use super::super::{LinalgError, LinearSolver, Pattern};

/// `xingo-sparse` options for MNA matrices (see the module docs).
fn circuit_options() -> LuOptions {
    LuOptions {
        abs_pivot_tol: 0.0,
        rel_pivot_tol: 1e-3,
        drop_tol: 0.0,
        ..LuOptions::default()
    }
}

/// A fixed CSC structure over a list of (row, col) positions, with
/// the position of every input entry (duplicates share one).
#[derive(Default)]
struct Structure {
    n: usize,
    col_ptr: Vec<usize>,
    row_idx: Vec<u32>,
    /// `pos[k]`: CSC position of input entry `k`.
    pos: Vec<usize>,
}

impl Structure {
    fn new(n: usize, entries: &[(usize, usize)]) -> Self {
        let mut order: Vec<usize> = (0..entries.len()).collect();
        order.sort_by_key(|&k| (entries[k].1, entries[k].0));
        let mut col_ptr = vec![0usize; n + 1];
        let mut row_idx: Vec<u32> = Vec::with_capacity(entries.len());
        let mut pos = vec![0usize; entries.len()];
        let mut last: Option<(usize, usize)> = None;
        for &k in &order {
            let (r, c) = entries[k];
            if last != Some((r, c)) {
                row_idx.push(u32::try_from(r).expect("matrix order exceeds u32"));
                col_ptr[c + 1] += 1;
                last = Some((r, c));
            }
            pos[k] = row_idx.len() - 1;
        }
        for j in 0..n {
            col_ptr[j + 1] += col_ptr[j];
        }
        Structure {
            n,
            col_ptr,
            row_idx,
            pos,
        }
    }

    fn nnz(&self) -> usize {
        self.row_idx.len()
    }
}

/// The matrix handed to the factorization: our CSC arrays plus the
/// current values; exact zeros are skipped (they carry no pivot and
/// no fill).
struct Source<'a> {
    s: &'a Structure,
    val: &'a [f64],
}

impl ColumnSource<f64> for Source<'_> {
    fn m(&self) -> usize {
        self.s.n
    }

    fn for_each_entry<F: FnMut(u32, f64)>(&self, k: usize, mut f: F) {
        let (a, b) = (self.s.col_ptr[k], self.s.col_ptr[k + 1]);
        for (&i, &v) in self.s.row_idx[a..b].iter().zip(&self.val[a..b]) {
            if v != 0.0 {
                f(i, v);
            }
        }
    }

    fn nnz_hint(&self) -> usize {
        self.s.nnz()
    }

    fn validate(&self) -> Result<(), SparseError> {
        Ok(())
    }
}

/// `SMPluFac`'s structural check: a row or column with no nonzero
/// value is the "no DC path to ground" singularity; report it (by
/// row/column index modulo `n`) before the LU does.
fn empty_line(s: &Structure, val: &[f64], n_report: usize) -> Option<usize> {
    let mut row_has = vec![false; s.n];
    for j in 0..s.n {
        let (a, b) = (s.col_ptr[j], s.col_ptr[j + 1]);
        let mut col_has = false;
        for (&i, &v) in s.row_idx[a..b].iter().zip(&val[a..b]) {
            if v != 0.0 {
                row_has[i as usize] = true;
                col_has = true;
            }
        }
        if !col_has {
            return Some(j % n_report);
        }
    }
    row_has.iter().position(|h| !h).map(|i| i % n_report)
}

fn map_lu_error(e: &LuError, n_report: usize) -> LinalgError {
    let row = match e {
        LuError::Singular { rows, cols, .. } => rows
            .first()
            .map(|r| r.0 as usize)
            .or_else(|| cols.first().copied())
            .unwrap_or(0),
        _ => 0,
    };
    LinalgError::Singular {
        row: row % n_report.max(1),
    }
}

pub struct XingoSparse {
    n: usize,
    entries: Vec<(usize, usize)>,
    /// Real n×n structure and its value buffer.
    real: Structure,
    val: Vec<f64>,
    /// CSC positions of the diagonals present in the pattern (gmin
    /// landing spots, `LoadGmin`).
    diag_pos: Vec<usize>,
    lu: Lu<f64>,
    lu_ok: bool,
    /// Complex 2n embedding: structure, values, and per pattern entry
    /// the positions of `G` (top-left), `−B` (top-right), `B`
    /// (bottom-left), `G` (bottom-right).
    cplx: Structure,
    cval: Vec<f64>,
    cpos: Vec<[usize; 4]>,
    clu: Lu<f64>,
    clu_ok: bool,
    work: SparseVec<f64>,
    cwork: SparseVec<f64>,
}

impl Default for XingoSparse {
    fn default() -> Self {
        XingoSparse {
            n: 0,
            entries: Vec::new(),
            real: Structure::default(),
            val: Vec::new(),
            diag_pos: Vec::new(),
            lu: Lu::new(circuit_options()),
            lu_ok: false,
            cplx: Structure::default(),
            cval: Vec::new(),
            cpos: Vec::new(),
            clu: Lu::new(circuit_options()),
            clu_ok: false,
            work: SparseVec::new(0),
            cwork: SparseVec::new(0),
        }
    }
}

impl XingoSparse {
    /// Dense-mode solve through `ftran`/`btran` of `lu`, overwriting
    /// `x`.
    fn solve_with(lu: &mut Lu<f64>, work: &mut SparseVec<f64>, x: &mut [f64], transposed: bool) {
        work.make_dense();
        work.values_mut().copy_from_slice(x);
        if transposed {
            lu.btran(work);
        } else {
            lu.ftran(work);
        }
        x.copy_from_slice(work.values());
    }

    fn complex_solve(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
        transposed: bool,
    ) -> Result<(), LinalgError> {
        // A solve after a failed factorization reports the failure
        // again instead of panicking (the faer backend would reuse a
        // stale LU; the engine propagates factor errors before any
        // solve, so neither path is expected).
        if !self.clu_ok {
            return Err(LinalgError::Singular { row: 0 });
        }
        let n = self.n;
        debug_assert_eq!(rhs_re.len(), n);
        if n == 0 {
            return Ok(());
        }
        let mut x = vec![0.0; 2 * n];
        x[..n].copy_from_slice(rhs_re);
        // Transposed: solve Aᴴ x̄ = b̄ with the embedding's transpose.
        let s = if transposed { -1.0 } else { 1.0 };
        for (xi, bi) in x[n..].iter_mut().zip(rhs_im.iter()) {
            *xi = s * bi;
        }
        Self::solve_with(&mut self.clu, &mut self.cwork, &mut x, transposed);
        for k in 0..n {
            let (r, i) = (x[k], s * x[n + k]);
            if !r.is_finite() || !i.is_finite() {
                return Err(LinalgError::Singular { row: k });
            }
            rhs_re[k] = r;
            rhs_im[k] = i;
        }
        Ok(())
    }
}

impl LinearSolver for XingoSparse {
    fn analyze(&mut self, pattern: &Pattern) {
        let n = pattern.n;
        self.n = n;
        self.entries = pattern.entries.clone();
        self.real = Structure::new(n, &pattern.entries);
        self.val = vec![0.0; self.real.nnz()];
        let mut diag_pos: Vec<usize> = pattern
            .entries
            .iter()
            .zip(&self.real.pos)
            .filter_map(|((r, c), &p)| (r == c).then_some(p))
            .collect();
        diag_pos.sort_unstable();
        diag_pos.dedup();
        self.diag_pos = diag_pos;

        let mut quad = Vec::with_capacity(4 * pattern.entries.len());
        for &(r, c) in &pattern.entries {
            quad.extend_from_slice(&[(r, c), (r, c + n), (r + n, c), (r + n, c + n)]);
        }
        self.cplx = Structure::new(2 * n, &quad);
        self.cval = vec![0.0; self.cplx.nnz()];
        self.cpos = self.cplx.pos.as_chunks::<4>().0.to_vec();

        self.lu = Lu::new(circuit_options());
        self.clu = Lu::new(circuit_options());
        self.lu_ok = false;
        self.clu_ok = false;
        self.work = SparseVec::new(n);
        self.cwork = SparseVec::new(2 * n);
    }

    fn factor(&mut self, values: &[f64], diag_gmin: f64) -> Result<(), LinalgError> {
        debug_assert_eq!(values.len(), self.entries.len());
        self.lu_ok = false;
        if self.n == 0 {
            return Ok(());
        }
        self.val.fill(0.0);
        for (&p, v) in self.real.pos.iter().zip(values) {
            self.val[p] += v;
        }
        if diag_gmin != 0.0 {
            for &p in &self.diag_pos {
                self.val[p] += diag_gmin;
            }
        }
        if let Some(row) = empty_line(&self.real, &self.val, self.n) {
            return Err(LinalgError::Singular { row });
        }
        let src = Source {
            s: &self.real,
            val: &self.val,
        };
        self.lu
            .refactorize_same_pattern(&src)
            .map_err(|e| map_lu_error(&e, self.n))?;
        self.lu_ok = true;
        Ok(())
    }

    fn solve_in_place(&mut self, rhs: &mut [f64]) -> Result<(), LinalgError> {
        debug_assert_eq!(rhs.len(), self.n);
        if self.n == 0 {
            return Ok(());
        }
        if !self.lu_ok {
            return Err(LinalgError::Singular { row: 0 });
        }
        Self::solve_with(&mut self.lu, &mut self.work, rhs, false);
        if let Some(row) = rhs.iter().position(|v| !v.is_finite()) {
            return Err(LinalgError::Singular { row });
        }
        Ok(())
    }

    fn factor_complex(&mut self, re: &[f64], im: &[f64]) -> Result<(), LinalgError> {
        debug_assert_eq!(re.len(), self.entries.len());
        self.clu_ok = false;
        if self.n == 0 {
            self.clu_ok = true;
            return Ok(());
        }
        self.cval.fill(0.0);
        for (k, q) in self.cpos.iter().enumerate() {
            self.cval[q[0]] += re[k];
            self.cval[q[1]] -= im[k];
            self.cval[q[2]] += im[k];
            self.cval[q[3]] += re[k];
        }
        // Structural check on the complex matrix itself (a line of
        // `G + jB` is empty iff both of its embedding lines are).
        self.val.fill(0.0);
        for (k, &p) in self.real.pos.iter().enumerate() {
            self.val[p] += re[k].abs() + im[k].abs();
        }
        if let Some(row) = empty_line(&self.real, &self.val, self.n) {
            return Err(LinalgError::Singular { row });
        }
        let src = Source {
            s: &self.cplx,
            val: &self.cval,
        };
        self.clu
            .refactorize_same_pattern(&src)
            .map_err(|e| map_lu_error(&e, self.n))?;
        self.clu_ok = true;
        Ok(())
    }

    fn solve_complex_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError> {
        self.complex_solve(rhs_re, rhs_im, false)
    }

    /// `SMPcaSolve` against the factorization of the last
    /// [`LinearSolver::factor_complex`] (no refactorization; see the
    /// module docs for the conjugation identity).
    fn solve_complex_transposed_in_place(
        &mut self,
        rhs_re: &mut [f64],
        rhs_im: &mut [f64],
    ) -> Result<(), LinalgError> {
        self.complex_solve(rhs_re, rhs_im, true)
    }

    fn det_complex(&mut self, re: &[f64], im: &[f64]) -> (num_complex::Complex64, i32) {
        debug_assert_eq!(re.len(), self.entries.len());
        super::dense_det::det_complex(self.n, &self.entries, re, im)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex64;

    /// A 4×4 MNA-shaped pattern: a resistive ladder, a voltage
    /// source branch (zero diagonal at row 3) and one asymmetric
    /// controlled-source entry.
    fn pattern() -> Pattern {
        Pattern {
            n: 4,
            entries: vec![
                (0, 0),
                (0, 1),
                (1, 0),
                (1, 1),
                (1, 2),
                (2, 1),
                (2, 2),
                (0, 3),
                (3, 0),
                (2, 0),
                (3, 3),
            ],
        }
    }

    fn values() -> Vec<f64> {
        vec![
            2e-3, -1e-3, -1e-3, 3e-3, -2e-3, -2e-3, 2e-3, 1.0, 1.0, 5e-4, 0.0,
        ]
    }

    fn dense(p: &Pattern, v: &[f64], gmin: f64) -> Vec<Vec<f64>> {
        let mut a = vec![vec![0.0; p.n]; p.n];
        for (&(r, c), x) in p.entries.iter().zip(v) {
            a[r][c] += x;
        }
        for &(r, c) in &p.entries {
            if r == c {
                a[r][r] += gmin;
            }
        }
        a
    }

    /// Determinant by cofactor expansion (test helper).
    fn dense_det(a: &[Vec<f64>]) -> f64 {
        fn det(a: &[Vec<f64>]) -> f64 {
            let n = a.len();
            if n == 1 {
                return a[0][0];
            }
            (0..n)
                .map(|j| {
                    let minor: Vec<Vec<f64>> = a[1..]
                        .iter()
                        .map(|r| {
                            r.iter()
                                .enumerate()
                                .filter_map(|(c, &x)| (c != j).then_some(x))
                                .collect()
                        })
                        .collect();
                    let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
                    sign * a[0][j] * det(&minor)
                })
                .sum()
        }
        det(a)
    }

    #[test]
    fn real_solve_with_gmin() {
        let p = pattern();
        let v = values();
        let mut s = XingoSparse::default();
        s.analyze(&p);
        for gmin in [0.0, 1e-3] {
            s.factor(&v, gmin).unwrap();
            let b = [1e-3, 0.0, 2e-3, 5.0];
            let mut x = b;
            s.solve_in_place(&mut x).unwrap();
            let a = dense(&p, &v, gmin);
            for i in 0..4 {
                let ax: f64 = (0..4).map(|j| a[i][j] * x[j]).sum();
                assert!((ax - b[i]).abs() < 1e-12, "row {i}: {ax} vs {}", b[i]);
            }
        }
    }

    /// A refactorization on drifting values replays the recorded
    /// pivot sequence; values that break a recorded pivot fall back
    /// to a fresh Markowitz factorization. Either way the solve is
    /// that of the new matrix.
    #[test]
    fn refactor_replays_the_pivot_sequence_or_falls_back() {
        let p = pattern();
        let mut s = XingoSparse::default();
        s.analyze(&p);
        let check = |s: &mut XingoSparse, v: &[f64]| {
            s.factor(v, 0.0).unwrap();
            let b = [1e-3, 0.0, 2e-3, 5.0];
            let mut x = b;
            s.solve_in_place(&mut x).unwrap();
            let a = dense(&p, v, 0.0);
            for i in 0..4 {
                let ax: f64 = (0..4).map(|j| a[i][j] * x[j]).sum();
                // Componentwise backward error: the shrunk entries
                // make some of these matrices ill-conditioned.
                let scale: f64 = b[i].abs() + (0..4).map(|j| (a[i][j] * x[j]).abs()).sum::<f64>();
                assert!(
                    (ax - b[i]).abs() <= 1e-12 * scale,
                    "row {i}: {ax} vs {}",
                    b[i]
                );
            }
        };
        let v = values();
        check(&mut s, &v);
        assert_eq!(s.lu.stats().symbolic_reuses, 0);
        // Same pattern, values 10% off: a replay.
        let drifted: Vec<f64> = v.iter().map(|x| 1.1 * x).collect();
        check(&mut s, &drifted);
        assert_eq!(s.lu.stats().symbolic_reuses, 1);
        // Every recorded pivot is some entry of the matrix; shrinking
        // each entry in turn far below the threshold of its column
        // must refuse at least one replay, and every solve stays right.
        let fallbacks = s.lu.stats().symbolic_fallbacks;
        for k in 0..v.len() {
            if v[k] == 0.0 {
                continue;
            }
            let mut w = v.clone();
            w[k] *= 1e-9;
            if dense_det(&dense(&p, &w, 0.0)).abs() > 1e-15 {
                check(&mut s, &w);
            }
            check(&mut s, &v);
        }
        assert!(s.lu.stats().symbolic_fallbacks > fallbacks);
    }

    #[test]
    fn complex_and_transposed_solves() {
        let p = pattern();
        let re = values();
        let im: Vec<f64> = (0..re.len()).map(|k| 1e-4 * (k as f64 + 1.0)).collect();
        let mut s = XingoSparse::default();
        s.analyze(&p);
        s.factor_complex(&re, &im).unwrap();
        let mut a = vec![vec![Complex64::new(0.0, 0.0); 4]; 4];
        for (k, &(r, c)) in p.entries.iter().enumerate() {
            a[r][c] += Complex64::new(re[k], im[k]);
        }
        let br = [1.0, -2.0, 0.5, 3.0];
        let bi = [0.0, 1.0, -1.0, 0.25];
        for transposed in [false, true] {
            let (mut xr, mut xi) = (br, bi);
            if transposed {
                s.solve_complex_transposed_in_place(&mut xr, &mut xi)
                    .unwrap();
            } else {
                s.solve_complex_in_place(&mut xr, &mut xi).unwrap();
            }
            for i in 0..4 {
                let ax: Complex64 = (0..4)
                    .map(|j| {
                        let aij = if transposed { a[j][i] } else { a[i][j] };
                        aij * Complex64::new(xr[j], xi[j])
                    })
                    .sum();
                let b = Complex64::new(br[i], bi[i]);
                assert!((ax - b).norm() < 1e-9, "transposed={transposed} row {i}");
            }
        }
    }

    #[test]
    fn empty_row_is_singular() {
        let p = Pattern {
            n: 2,
            entries: vec![(0, 0), (1, 1)],
        };
        let mut s = XingoSparse::default();
        s.analyze(&p);
        assert!(matches!(
            s.factor(&[1.0, 0.0], 0.0),
            Err(LinalgError::Singular { row: 1 })
        ));
        // gmin on the diagonal rescues it, exactly like LoadGmin.
        s.factor(&[1.0, 0.0], 1e-12).unwrap();
    }

    #[test]
    fn numerically_singular_is_reported() {
        let p = Pattern {
            n: 2,
            entries: vec![(0, 0), (0, 1), (1, 0), (1, 1)],
        };
        let mut s = XingoSparse::default();
        s.analyze(&p);
        assert!(s.factor(&[1.0, 2.0, 2.0, 4.0], 0.0).is_err());
    }
}
