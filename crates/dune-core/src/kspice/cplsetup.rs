//! The CPL setup of ngspice-47 (`devices/cpl/cplsetup.c`): modal
//! decomposition of the coupled lines by Jacobi rotations, polynomial
//! fits of the modal matrices over a ladder of frequencies, and Padé
//! approximations of every entry of the admittance and propagation
//! matrices. "Copyright 1992 Regents of the University of California.
//! All rights reserved. Author: 1992 Charles Hough. Modified: 2004
//! Paolo Nenzi - (ng)spice integration" (the diagonalization `diag.c`
//! part: Shen Lin, 1991).
//!
//! The C keeps all of it in file-scope statics (`ZY`, `Sv`, `D`,
//! `Y5`, `Y5_1`, `Sv_1`, `R_m`…`C_m`, `length`, `TAU`, `A`,
//! `frequency`, `Si`, `Si_1`, `SiSv_1`, `Sip`, `Si_1p`, `Sv_1p`, `W`,
//! `IWI`, `IWV`, `SIV`, `At`, `Scaling_F`, `Scaling_F2`, the
//! diagonalizer's `dim` and `row`): here they are the fields of
//! [`Setup`], built fresh for every instance (docs/kspice-lines.md).

// A statement-by-statement port: the indexed loops mirror the C's.
#![allow(
    clippy::needless_range_loop,
    clippy::manual_memcpy,
    clippy::explicit_counter_loop
)]

/// `Left_deg` (`multi_line.h`): the degree of the fitted polynomials.
const DEG_O: usize = 7;
/// `MAX_DEG`.
const MAX_DEG: usize = 8;
/// `epsilon` of `Gaussian_Elimination2` (misc).
const EPSILON: f64 = 1.0e-88;
/// `epsi_mult` of `Gaussian_Elimination` (Padé).
const EPSI_MULT: f64 = 1e-28;
/// `epsi2` of `diag`: rotations go on while an off-diagonal entry of
/// the scaled matrix exceeds it.
const EPSI2: f64 = 1.0e-8;
/// Dune's bound on the Jacobi sweep (the C loops until convergence,
/// with no limit).
const MAX_ROTATIONS: usize = 100_000;

/// A Padé result of one matrix entry: `C_0` and the poles/residues
/// (`Single_Out`/`Mult_Out` entries; `poly[6]` is `rtv`: 2 for a
/// complex pair, 1 for three real poles).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Fit {
    pub c0: f64,
    pub poly: [f64; 7],
    /// The normalized polynomial `matrix_p_mult` leaves (the C's
    /// `Poly[k]` before `generate_out` overwrites it in place).
    pub poly_src: [f64; DEG_O + 1],
}

/// What `coupled()` leaves for `ReadCpL`.
pub(crate) struct Coupled {
    /// `TAU[i]`, seconds.
    pub tau: Vec<f64>,
    /// `SIV[i][j]`.
    pub siv: Vec<Vec<Fit>>,
    /// `IWI[i][j]` per mode `k`.
    pub iwi: Vec<Vec<Vec<Fit>>>,
    /// `IWV[i][j]` per mode `k`.
    pub iwv: Vec<Vec<Vec<Fit>>>,
}

type Mat = Vec<Vec<f64>>;

fn mat(n: usize, m: usize) -> Mat {
    vec![vec![0.0; m]; n]
}

/// The working set of one `coupled()` run (the C's statics).
struct Setup {
    dim: usize,
    zy: Mat,
    sv: Mat,
    d: Vec<f64>,
    y5: Mat,
    y5_1: Mat,
    sv_1: Mat,
    r_m: Mat,
    g_m: Mat,
    l_m: Mat,
    c_m: Mat,
    length: f64,
    tau: Vec<f64>,
    frequency: [f64; MAX_DEG],
    si: Mat,
    si_1: Mat,
    si_sv_1: Vec<Vec<Vec<f64>>>,
    sip: Vec<Vec<Vec<f64>>>,
    si_1p: Vec<Vec<Vec<f64>>>,
    sv_1p: Vec<Vec<Vec<f64>>>,
    w: Vec<Vec<f64>>,
    scaling_f: f64,
    scaling_f2: f64,
}

/// `coupled(dim)` for the symmetric per-unit-length matrices (full,
/// already mirrored) and `length`.
pub(crate) fn coupled(
    r_m: Mat,
    g_m: Mat,
    l_m: Mat,
    c_m: Mat,
    length: f64,
) -> Result<Coupled, String> {
    let dim = r_m.len();
    let poly = || vec![vec![vec![0.0; DEG_O + 1]; dim]; dim];
    let mut s = Setup {
        dim,
        zy: mat(dim, dim),
        sv: mat(dim, dim),
        d: vec![0.0; dim],
        y5: mat(dim, dim),
        y5_1: mat(dim, dim),
        sv_1: mat(dim, dim),
        r_m,
        g_m,
        l_m,
        c_m,
        length,
        tau: vec![0.0; dim],
        frequency: [0.0; MAX_DEG],
        si: mat(dim, dim),
        si_1: mat(dim, dim),
        si_sv_1: poly(),
        sip: poly(),
        si_1p: poly(),
        sv_1p: poly(),
        w: vec![vec![0.0; MAX_DEG]; dim],
        scaling_f: 1.0,
        scaling_f2: 1.0,
    };

    // y = 0: ZY = LC.
    s.loop_zy(0.0)?;
    s.eval_frequency()?;
    s.eval_si_si_1(0.0)?;
    s.store_si_sv_1(0);
    s.store(0);
    for i in 1..=DEG_O {
        let f = s.frequency[i];
        s.loop_zy(f)?;
        s.eval_si_si_1(f)?;
        s.store_si_sv_1(i);
        s.store(i);
    }
    poly_matrix(&mut s.sip, &s.frequency)?;
    poly_matrix(&mut s.si_1p, &s.frequency)?;
    poly_matrix(&mut s.sv_1p, &s.frequency)?;
    s.poly_w()?;
    let mut iwi = matrix_p_mult(&s.sip, &s.w, &s.si_1p, dim);
    let mut iwv = matrix_p_mult(&s.sip, &s.w, &s.sv_1p, dim);
    poly_matrix(&mut s.si_sv_1, &s.frequency)?;
    let siv = s.generate_out(&mut iwi, &mut iwv)?;
    Ok(Coupled {
        tau: s.tau,
        siv,
        iwi,
        iwv,
    })
}

impl Setup {
    /// `loop_ZY(dim, y)`: diagonalize `(sC+G)`, then the product
    /// `Y5·(sL+R)·Y5` in that basis; leaves the modal matrices in
    /// `Sv`, `Sv_1` and the eigenvalues in `D`.
    fn loop_zy(&mut self, y: f64) -> Result<(), String> {
        let n = self.dim;
        for i in 0..n {
            for j in 0..n {
                self.zy[i][j] = self.scaling_f * self.c_m[i][j] + self.g_m[i][j] * y;
            }
        }
        self.diag()?;
        let mut fmin = self.d[0];
        for i in 1..n {
            if self.d[i] < fmin {
                fmin = self.d[i];
            }
        }
        if fmin < 0.0 {
            return Err(
                "the capacitance matrix of the multiconductor system is not positive definite"
                    .into(),
            );
        }
        let fmin = fmin.sqrt();
        let fmin1 = 1.0 / fmin;
        for i in 0..n {
            self.d[i] = self.d[i].sqrt();
        }
        for i in 0..n {
            for j in 0..n {
                self.y5[i][j] = self.d[i] * self.sv[j][i];
                self.y5_1[i][j] = self.sv[j][i] / self.d[i];
            }
        }
        for i in 0..n {
            for j in 0..n {
                self.sv_1[i][j] = 0.0;
                for k in 0..n {
                    self.sv_1[i][j] += self.sv[i][k] * self.y5[k][j];
                }
            }
        }
        self.y5.clone_from(&self.sv_1);
        for i in 0..n {
            for j in 0..n {
                self.sv_1[i][j] = 0.0;
                for k in 0..n {
                    self.sv_1[i][j] += self.sv[i][k] * self.y5_1[k][j];
                }
            }
        }
        self.y5_1.clone_from(&self.sv_1);
        for i in 0..n {
            for j in 0..n {
                self.zy[i][j] = 0.0;
                for k in 0..n {
                    self.zy[i][j] +=
                        (self.scaling_f * self.l_m[i][k] + self.r_m[i][k] * y) * self.y5[k][j];
                }
            }
        }
        for i in 0..n {
            for j in 0..n {
                self.sv_1[i][j] = 0.0;
                for k in 0..n {
                    self.sv_1[i][j] += self.y5[i][k] * self.zy[k][j];
                }
            }
        }
        self.zy.clone_from(&self.sv_1);
        self.diag()?;
        for i in 0..n {
            for j in 0..n {
                self.sv_1[i][j] = 0.0;
                for k in 0..n {
                    self.sv_1[i][j] += self.sv[k][i] * self.y5[k][j];
                }
                self.sv_1[i][j] *= fmin1;
            }
        }
        for i in 0..n {
            for j in 0..n {
                self.zy[i][j] = 0.0;
                for k in 0..n {
                    self.zy[i][j] += self.y5_1[i][k] * self.sv[k][j];
                }
                self.zy[i][j] *= fmin;
            }
        }
        self.sv.clone_from(&self.zy);
        Ok(())
    }

    /// `eval_frequency(dim, deg_o)`: the frequency ladder `0, 8·len,
    /// 16·len, …` and the scaling by the smallest mode.
    fn eval_frequency(&mut self) -> Result<(), String> {
        let mut min = self.d[0];
        for i in 1..self.dim {
            if self.d[i] < min {
                min = self.d[i];
            }
        }
        if min <= 0.0 {
            return Err("a CPL mode frequency is not positive".into());
        }
        self.scaling_f2 = 1.0 / min;
        self.scaling_f = self.scaling_f2.sqrt();
        let min = self.length * 8.0;
        self.frequency[0] = 0.0;
        for i in 1..=DEG_O {
            self.frequency[i] = self.frequency[i - 1] + min;
        }
        for i in 0..self.dim {
            self.d[i] *= self.scaling_f2;
        }
        Ok(())
    }

    /// `eval_Si_Si_1(dim, y)`: `Si_1 = Sv_1·(yR + F·L)/sqrt(D)` and its
    /// inverse `Si`.
    fn eval_si_si_1(&mut self, y: f64) -> Result<(), String> {
        let n = self.dim;
        for i in 0..n {
            for j in 0..n {
                self.si_1[i][j] = 0.0;
                for k in 0..n {
                    self.si_1[i][j] +=
                        self.sv_1[i][k] * (y * self.r_m[k][j] + self.scaling_f * self.l_m[k][j]);
                }
            }
        }
        for i in 0..n {
            for j in 0..n {
                self.si_1[i][j] /= self.d[i].sqrt();
            }
        }
        // The C's static `A[MAX_DIM][2·MAX_DIM]`: rows `dim..2·dim`
        // are searched for pivots but never written, so they hold
        // zeros — unless an earlier, larger CPL left its rows there
        // (docs/kspice-lines.md, "Fidelidade"). Here they are zeros.
        let mut a = mat(2 * n, 2 * n + 1);
        for i in 0..n {
            for j in 0..n {
                a[i][j] = self.si_1[i][j];
            }
            a[i][i + n] = 1.0;
        }
        gaussian_elimination2(&mut a, n)?;
        for i in 0..n {
            for j in 0..n {
                self.si[i][j] = a[i][j + n];
            }
        }
        Ok(())
    }

    fn store(&mut self, ind: usize) {
        for i in 0..self.dim {
            for j in 0..self.dim {
                self.sip[i][j][ind] = self.si[i][j];
                self.si_1p[i][j][ind] = self.si_1[i][j];
                self.sv_1p[i][j][ind] = self.sv_1[i][j];
            }
            self.w[i][ind] = self.d[i];
        }
    }

    fn store_si_sv_1(&mut self, ind: usize) {
        for i in 0..self.dim {
            for j in 0..self.dim {
                let mut temp = 0.0;
                for k in 0..self.dim {
                    temp += self.si[i][k] * self.sv_1[k][j];
                }
                self.si_sv_1[i][j][ind] = temp;
            }
        }
    }

    /// `poly_W`: fit each mode's eigenvalue curve and turn it into a
    /// delay and an attenuation series (`approx_mode`, in place).
    fn poly_w(&mut self) -> Result<(), String> {
        for i in 0..self.dim {
            let cof = match_poly(DEG_O, &self.frequency, &self.w[i])?;
            self.w[i][..=DEG_O].copy_from_slice(&cof);
            let x = self.w[i].clone();
            self.tau[i] = approx_mode(&x, &mut self.w[i], self.length, self.scaling_f);
        }
        Ok(())
    }

    /// `generate_out`: the Padé fits of `SiSv_1` (admittance), `IWI`
    /// and `IWV` (propagation).
    fn generate_out(
        &mut self,
        iwi: &mut [Vec<Vec<Fit>>],
        iwv: &mut [Vec<Vec<Fit>>],
    ) -> Result<Vec<Vec<Fit>>, String> {
        let n = self.dim;
        let mut siv = vec![vec![Fit::default(); n]; n];
        for i in 0..n {
            for j in 0..n {
                let p = &mut self.si_sv_1[i][j];
                let c = p[0];
                siv[i][j].c0 = c;
                if c == 0.0 {
                    continue;
                }
                for v in p.iter_mut() {
                    *v /= c;
                }
                let a_b = if i == j {
                    (self.g_m[i][i] / self.r_m[i][i]).sqrt() / c
                } else {
                    0.0
                };
                siv[i][j].poly = pade_apx(a_b, p)?;
            }
        }
        for (out, sqrt_g_over_r) in [(&mut *iwi, false), (&mut *iwv, true)] {
            for i in 0..n {
                for j in 0..n {
                    for k in 0..n {
                        let fit = &mut out[i][j][k];
                        let c = fit.c0;
                        if c == 0.0 {
                            continue;
                        }
                        let a_b = if i == j && k == i {
                            let e = (-(self.g_m[i][i] * self.r_m[i][i]).sqrt() * self.length).exp();
                            if sqrt_g_over_r {
                                (self.g_m[i][i] / self.r_m[i][i]).sqrt() * e / c
                            } else {
                                e / c
                            }
                        } else {
                            0.0
                        };
                        let p: [f64; DEG_O + 1] = fit.poly_src;
                        fit.poly = pade_apx(a_b, &p)?;
                    }
                }
            }
        }
        Ok(siv)
    }

    /// `diag(dims)`: Jacobi rotations on the upper triangle of `ZY`
    /// (scaled by `2/(min+max)`), largest off-diagonal entry first;
    /// `Sv` accumulates the rotations, `D` gets the eigenvalues.
    fn diag(&mut self) -> Result<(), String> {
        let n = self.dim;
        let mut fmin = self.zy[0][0].abs();
        let mut fmax = fmin;
        for i in 0..n {
            for j in i..n {
                let v = self.zy[i][j].abs();
                if v > fmax {
                    fmax = v;
                } else if v < fmin {
                    fmin = v;
                }
            }
        }
        let fmin = 2.0 / (fmin + fmax);
        for i in 0..n {
            for j in i..n {
                self.zy[i][j] *= fmin;
            }
        }
        for i in 0..n {
            for j in 0..n {
                self.sv[i][j] = if i == j { 1.0 } else { 0.0 };
            }
        }
        // `ordering()`: the largest off-diagonal entry of every row but
        // the last, sorted by decreasing value (the C's linked list
        // `row`; `sort` inserts before the first smaller entry).
        let mut row: Vec<Maxe> = Vec::new();
        for i in 0..n.saturating_sub(1) {
            let (m, mv) = self.row_max(i);
            sort_insert(
                &mut row,
                Maxe {
                    row: i,
                    col: m,
                    value: mv,
                },
            );
        }
        let mut rotations = 0;
        while let Some(top) = row.first().copied() {
            if top.value.is_nan() || top.value <= EPSI2 {
                break;
            }
            rotations += 1;
            if rotations > MAX_ROTATIONS {
                return Err(format!(
                    "the CPL modal decomposition does not converge in {MAX_ROTATIONS} \
                     Jacobi rotations"
                ));
            }
            let (p, q) = (top.row, top.col);
            self.rotate(p, q);
            // `reordering(p, q)`.
            let (m, mv) = self.row_max(p);
            delete_row(&mut row, p);
            sort_insert(
                &mut row,
                Maxe {
                    row: p,
                    col: m,
                    value: mv,
                },
            );
            if q + 1 != n {
                let (m, mv) = self.row_max(q);
                delete_row(&mut row, q);
                sort_insert(
                    &mut row,
                    Maxe {
                        row: q,
                        col: m,
                        value: mv,
                    },
                );
            }
        }
        for i in 0..n {
            self.d[i] = self.zy[i][i] / fmin;
        }
        Ok(())
    }

    /// The column of row `p`'s largest off-diagonal entry right of the
    /// diagonal, compared as `(int)(|x|·1e7)` like the C.
    fn row_max(&self, p: usize) -> (usize, f64) {
        let n = self.dim;
        let mut m = p + 1;
        let mut mv = self.zy[p][m].abs();
        for j in (m + 1)..n {
            if ((self.zy[p][j].abs() * 1e7) as i32) > ((1e7 * mv) as i32) {
                mv = self.zy[p][j].abs();
                m = j;
            }
        }
        (m, mv)
    }

    /// `rotate(dim, p, q)`: one Jacobi rotation zeroing `ZY[p][q]`.
    fn rotate(&mut self, p: usize, q: usize) {
        let n = self.dim;
        let zy = &mut self.zy;
        let ld = -zy[p][q];
        let mu = 0.5 * (zy[p][p] - zy[q][q]);
        let ve = (ld * ld + mu * mu).sqrt();
        let co = ((ve + mu.abs()) / (2.0 * ve)).sqrt();
        // `SGN(mu)` is `copysign(1.0, mu)` (`macros.h`).
        let si = 1.0_f64.copysign(mu) * ld / (2.0 * ve * co);
        let mut t = vec![0.0; n];
        for j in (p + 1)..n {
            t[j] = zy[p][j];
        }
        for j in 0..p {
            t[j] = zy[j][p];
        }
        for j in (p + 1)..n {
            if j == q {
                continue;
            }
            if j > q {
                zy[p][j] = t[j] * co - zy[q][j] * si;
            } else {
                zy[p][j] = t[j] * co - zy[j][q] * si;
            }
        }
        for j in (q + 1)..n {
            if j == p {
                continue;
            }
            zy[q][j] = t[j] * si + zy[q][j] * co;
        }
        for j in 0..p {
            if j == q {
                continue;
            }
            zy[j][p] = t[j] * co - zy[j][q] * si;
        }
        for j in 0..q {
            if j == p {
                continue;
            }
            zy[j][q] = t[j] * si + zy[j][q] * co;
        }
        let tt = zy[p][p];
        zy[p][p] = tt * co * co + zy[q][q] * si * si - 2.0 * zy[p][q] * si * co;
        zy[q][q] = tt * si * si + zy[q][q] * co * co + 2.0 * zy[p][q] * si * co;
        zy[p][q] = 0.0;
        let mut r = vec![0.0; n];
        for j in 0..n {
            t[j] = self.sv[j][p];
            r[j] = self.sv[j][q];
        }
        for j in 0..n {
            self.sv[j][p] = t[j] * co - r[j] * si;
            self.sv[j][q] = t[j] * si + r[j] * co;
        }
    }
}

/// One entry of the diagonalizer's `row` list (`MAXE`).
#[derive(Clone, Copy, Debug)]
struct Maxe {
    row: usize,
    col: usize,
    value: f64,
}

/// `sort(list, val, r, c, e)`: insert before the first entry whose
/// value is smaller (`list->value < val`).
fn sort_insert(list: &mut Vec<Maxe>, e: Maxe) {
    let at = list
        .iter()
        .position(|x| x.value < e.value)
        .unwrap_or(list.len());
    list.insert(at, e);
}

/// `delete_1(&row, rc)`.
fn delete_row(list: &mut Vec<Maxe>, rc: usize) {
    if let Some(k) = list.iter().position(|x| x.row == rc) {
        list.remove(k);
    }
}

/// `Gaussian_Elimination2(dims, -1)`: invert the left block of
/// `[A | I]`, pivot rows searched over `2·dims` rows as in the C.
fn gaussian_elimination2(a: &mut Mat, dims: usize) -> Result<(), String> {
    let dim = 2 * dims;
    for i in 0..dims {
        let mut imax = i;
        let mut max = a[i][i].abs();
        for (j, row) in a.iter().enumerate().take(dim).skip(i + 1) {
            if row[i].abs() > max {
                imax = j;
                max = row[i].abs();
            }
        }
        if max < EPSILON {
            return Err(format!(
                "Gaussian elimination (misc) fails during setup of CPL (all matrix \
                 elements are less than {EPSILON:e}); check the model parameters"
            ));
        }
        if imax != i {
            a.swap(i, imax);
        }
        let f = 1.0 / a[i][i];
        a[i][i] = 1.0;
        for j in (i + 1)..=dim {
            a[i][j] *= f;
        }
        for j in 0..dims {
            if i == j {
                continue;
            }
            let f = a[j][i];
            a[j][i] = 0.0;
            for k in (i + 1)..=dim {
                let v = a[i][k];
                a[j][k] -= f * v;
            }
        }
    }
    Ok(())
}

/// `polint`: Neville's interpolation at `x` of the points `(xa, ya)`
/// (1-based in the C; 0-based slices here), returning `y`.
fn polint(xa: &[f64], ya: &[f64], x: f64) -> Result<f64, String> {
    let n = xa.len();
    // 1-based work arrays, as in the C.
    let mut c = vec![0.0; n + 1];
    let mut d = vec![0.0; n + 1];
    let xa1 = |i: usize| xa[i - 1];
    let mut ns: usize = 1;
    let mut dif = (x - xa1(1)).abs();
    for i in 1..=n {
        let dift = (x - xa1(i)).abs();
        if dift < dif {
            ns = i;
            dif = dift;
        }
        c[i] = ya[i - 1];
        d[i] = ya[i - 1];
    }
    let mut y = ya[ns - 1];
    // `ns--` (it may reach 0, an index the C never reads then).
    let mut ns = ns as isize - 1;
    for m in 1..n {
        for i in 1..=(n - m) {
            let ho = xa1(i) - x;
            let hp = xa1(i + m) - x;
            let w = c[i + 1] - d[i];
            let den = ho - hp;
            if den == 0.0 {
                return Err(
                    "(Error) in routine POLINT during CPL setup (zero line length?)".into(),
                );
            }
            let den = w / den;
            d[i] = hp * den;
            c[i] = ho * den;
        }
        let dy = if 2 * ns < (n - m) as isize {
            c[(ns + 1) as usize]
        } else {
            let v = d[ns.max(0) as usize];
            ns -= 1;
            v
        };
        y += dy;
    }
    Ok(y)
}

/// `match(n, cof, xa, ya)`: the coefficients of the polynomial through
/// the `n + 1` points, by repeated extrapolation to 0 and deflation.
fn match_poly(n: usize, xa: &[f64], ya: &[f64]) -> Result<Vec<f64>, String> {
    let mut x: Vec<f64> = xa[..=n].to_vec();
    let mut y: Vec<f64> = ya[..=n].to_vec();
    let mut cof = vec![0.0; n + 1];
    for j in 0..=n {
        cof[j] = polint(&x[..=n - j], &y[..=n - j], 0.0)?;
        let mut xmin = 1.0e38;
        let mut k: isize = -1;
        for i in 0..=(n - j) {
            if x[i].abs() < xmin {
                xmin = x[i].abs();
                k = i as isize;
            }
            if x[i] != 0.0 {
                y[i] = (y[i] - cof[j]) / x[i];
            }
        }
        for i in ((k + 1) as usize)..=(n - j) {
            if i >= 1 {
                y[i - 1] = y[i];
                x[i - 1] = x[i];
            }
        }
    }
    Ok(cof)
}

/// `poly_matrix`: fit every entry in place.
fn poly_matrix(a: &mut [Vec<Vec<f64>>], frequency: &[f64; MAX_DEG]) -> Result<(), String> {
    for row in a.iter_mut() {
        for p in row.iter_mut() {
            let cof = match_poly(DEG_O, frequency, p)?;
            p.copy_from_slice(&cof);
        }
    }
    Ok(())
}

/// `mult_p`: `p3 = p1·p2` truncated to degree `d3`.
fn mult_p(p1: &[f64], p2: &[f64], d1: usize, d2: usize, d3: usize) -> Vec<f64> {
    let mut p3 = vec![0.0; d3 + 1];
    for i in 0..=d1 {
        let mut j = i;
        for k in 0..=d2 {
            if j > d3 {
                break;
            }
            p3[j] += p1[i] * p2[k];
            j += 1;
        }
    }
    p3
}

/// `matrix_p_mult(A, D, B, dim, deg_o, deg_o, X)`: per mode `k`,
/// `X[i][j]_k = A[i][k]·(D[k]·B[k][j])`, normalized to a unit
/// constant term with the constant kept in `C_0`.
fn matrix_p_mult(
    a: &[Vec<Vec<f64>>],
    d1: &[Vec<f64>],
    b: &[Vec<Vec<f64>>],
    dim: usize,
) -> Vec<Vec<Vec<Fit>>> {
    let mut t = vec![vec![Vec::new(); dim]; dim];
    for i in 0..dim {
        for j in 0..dim {
            t[i][j] = mult_p(&b[i][j], &d1[i], DEG_O, DEG_O, DEG_O);
        }
    }
    let mut x = vec![vec![vec![Fit::default(); dim]; dim]; dim];
    for i in 0..dim {
        for j in 0..dim {
            for k in 0..dim {
                let mut p = mult_p(&a[i][k], &t[k][j], DEG_O, DEG_O, DEG_O);
                let t1 = p[0];
                x[i][j][k].c0 = t1;
                if t1 != 0.0 {
                    p[0] = 1.0;
                    for v in p.iter_mut().skip(1) {
                        *v /= t1;
                    }
                }
                x[i][j][k].poly_src.copy_from_slice(&p);
            }
        }
    }
    x
}

/// `approx_mode(X, b, length)`: the delay of one mode and the
/// attenuation series of its propagation function (written into
/// `b[0..=5]`; `X` is the fitted eigenvalue polynomial).
fn approx_mode(x: &[f64], b: &mut [f64], length: f64, scaling_f: f64) -> f64 {
    let w0 = x[0];
    let w1 = x[1] / w0;
    let w2 = x[2] / w0;
    let w3 = x[3] / w0;
    let w4 = x[4] / w0;
    let w5 = x[5] / w0;
    let y1 = 0.5 * w1;
    let y2 = w2 - y1 * y1;
    let y3 = 3.0 * w3 - 3.0 * y1 * y2;
    let y4 = 12.0 * w4 - 3.0 * y2 * y2 - 4.0 * y1 * y3;
    let y5 = 60.0 * w5 - 5.0 * y1 * y4 - 10.0 * y2 * y3;
    let y6 = -10.0 * y3 * y3 - 15.0 * y2 * y4 - 6.0 * y1 * y5;
    let delay = w0.sqrt() * length / scaling_f;
    let atten = (-delay * y1).exp();
    let mut a = [0.0; 6];
    a[1] = y2 / 2.0;
    a[2] = y3 / 6.0;
    a[3] = y4 / 24.0;
    a[4] = y5 / 120.0;
    a[5] = y6 / 720.0;
    for v in a.iter_mut().skip(1) {
        *v *= -delay;
    }
    b[0] = 1.0;
    b[1] = a[1];
    for i in 2..=5 {
        b[i] = 0.0;
        for j in 1..=i {
            b[i] += j as f64 * a[j] * b[i - j];
        }
        b[i] /= i as f64;
    }
    for v in b.iter_mut().take(6) {
        *v *= atten;
    }
    delay
}

fn eval2(a: f64, b: f64, c: f64, x: f64) -> f64 {
    a * x * x + b * x + c
}

/// `get_c` of `cplsetup.c`.
fn get_c(q1: f64, q2: f64, q3: f64, p1: f64, p2: f64, a: f64, b: f64) -> (f64, f64) {
    let mut d =
        (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2) * (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2);
    d += (6.0 * a * b + 2.0 * p1 * b) * (6.0 * a * b + 2.0 * p1 * b);
    let mut n = -(q1 * (a * a - b * b) + q2 * a + q3) * (6.0 * a * b + 2.0 * p1 * b);
    n += (2.0 * q1 * a * b + q2 * b) * (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2);
    let ci = n / d;
    let mut n = (3.0 * (a * a - b * b) + 2.0 * p1 * a + p2) * (q1 * (a * a - b * b) + q2 * a + q3);
    n += (6.0 * a * b + 2.0 * p1 * b) * (2.0 * q1 * a * b + q2 * b);
    (n / d, ci)
}

/// `Pade_apx(a_b, b, …)`: the [3/3] Padé approximant of the series
/// `b` in `1/s` as `1 + Σ c_k/(s − x_k)`; returns
/// `[c1, c2, c3, x1, x2, x3, rtv]`.
fn pade_apx(a_b: f64, b: &[f64]) -> Result<[f64; 7], String> {
    let mut at = [
        [1.0 - a_b, b[1], b[2], -b[3]],
        [b[1], b[2], b[3], -b[4]],
        [b[2], b[3], b[4], -b[5]],
    ];
    gaussian_elimination(&mut at)?;
    let p3 = at[0][3];
    let p2 = at[1][3];
    let p1 = at[2][3];
    let q1 = p1 + b[1];
    let q2 = b[1] * p1 + p2 + b[2];
    let q3 = p3 * a_b;
    let (x1, x2, x3, img) = find_roots(p1, p2, p3);
    let c1 = eval2(q1 - p1, q2 - p2, q3 - p3, x1) / eval2(3.0, 2.0 * p1, p2, x1);
    if img {
        let (c2, c3) = get_c(q1 - p1, q2 - p2, q3 - p3, p1, p2, x2, x3);
        Ok([c1, c2, c3, x1, x2, x3, 2.0])
    } else {
        let c2 = eval2(q1 - p1, q2 - p2, q3 - p3, x2) / eval2(3.0, 2.0 * p1, p2, x2);
        let c3 = eval2(q1 - p1, q2 - p2, q3 - p3, x3) / eval2(3.0, 2.0 * p1, p2, x3);
        Ok([c1, c2, c3, x1, x2, x3, 1.0])
    }
}

/// `Gaussian_Elimination(3)` on `At`.
fn gaussian_elimination(a: &mut [[f64; 4]; 3]) -> Result<(), String> {
    let dim = 3;
    for i in 0..dim {
        let mut imax = i;
        let mut max = a[i][i].abs();
        for (j, row) in a.iter().enumerate().take(dim).skip(i + 1) {
            if row[i].abs() > max {
                imax = j;
                max = row[i].abs();
            }
        }
        if max < EPSI_MULT {
            return Err(format!(
                "Gaussian elimination (mult) fails during setup of CPL (all matrix \
                 elements are less than {EPSI_MULT:e}); check the model parameters"
            ));
        }
        if imax != i {
            a.swap(i, imax);
        }
        let f = 1.0 / a[i][i];
        a[i][i] = 1.0;
        for j in (i + 1)..=dim {
            a[i][j] *= f;
        }
        for j in 0..dim {
            if i == j {
                continue;
            }
            let f = a[j][i];
            a[j][i] = 0.0;
            for k in (i + 1)..=dim {
                let v = a[i][k];
                a[j][k] -= f * v;
            }
        }
    }
    Ok(())
}

/// `root3` of `cplsetup.c` (Horner form, unlike the TXL one).
fn root3(a1: f64, a2: f64, a3: f64, x: f64) -> f64 {
    let t1 = x * (x * (x + a1) + a2) + a3;
    let t2 = x * (2.0 * a1 + 3.0 * x) + a2;
    x - t1 / t2
}

/// `find_roots` of `cplsetup.c`: `(x1, x2, x3, complex)`; a complex
/// pair comes back as `x2 ± j·x3`.
fn find_roots(a1: f64, a2: f64, a3: f64) -> (f64, f64, f64, bool) {
    let q = (a1 * a1 - 3.0 * a2) / 9.0;
    let p = (2.0 * a1 * a1 * a1 - 9.0 * a1 * a2 + 27.0 * a3) / 54.0;
    let t = q * q * q - p * p;
    let mut x;
    if t >= 0.0 {
        let t = (p / (q * q.sqrt())).acos();
        x = -2.0 * q.sqrt() * (t / 3.0).cos() - a1 / 3.0;
    } else if p > 0.0 {
        let t = ((-t).sqrt() + p).powf(1.0 / 3.0);
        x = -(t + q / t) - a1 / 3.0;
    } else if p == 0.0 {
        x = -a1 / 3.0;
    } else {
        let t = ((-t).sqrt() - p).powf(1.0 / 3.0);
        x = (t + q / t) - a1 / 3.0;
    }
    let backup = x;
    let mut i = 0;
    let mut t = root3(a1, a2, a3, x);
    while (t - x).abs() > 5.0e-4 {
        i += 1;
        if i == 32 {
            x = backup;
            break;
        }
        x = t;
        t = root3(a1, a2, a3, x);
    }
    let x1 = x;
    // `div3`.
    let b1 = a1 + x;
    let b2 = -a3 / x;
    let t = b1 * b1 - 4.0 * b2;
    if t < 0.0 {
        (x1, -0.5 * b1, 0.5 * (-t).sqrt(), true)
    } else {
        let t = t.sqrt();
        let x2 = if b1 >= 0.0 {
            -0.5 * (b1 + t)
        } else {
            -0.5 * (b1 - t)
        };
        (x1, x2, b2 / x2, false)
    }
}
