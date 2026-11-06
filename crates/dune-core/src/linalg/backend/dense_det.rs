//! Backend-neutral complex determinant for the pole-zero search
//! (`SMPcLUfac` + `SMPcDProd`), shared by every [`super`] backend so
//! the `.pz` root search sees bit-identical determinants whatever LU
//! the build uses. Pole-zero matrices are corpus-sized, so O(n³)
//! dense is fine here; the xingo-sparse backend uses it because that
//! crate has no complex scalar (its `Real` is `f64` only, and the
//! real 2n embedding it solves complex systems with only yields
//! `|det|²`, losing the phase the Muller iteration needs).

use num_complex::Complex64;

/// Determinant of `G + jB` (`re`/`im` parallel to `entries`, the
/// 0-based pattern of an `n`×`n` matrix; duplicates are summed) as a
/// base-2 mantissa/exponent pair. Partial pivoting on `|re|+|im|`;
/// the running product is kept normalized to
/// `max(|re|,|im|) ∈ (0.5, 1]` like the 3f5's `C_NORM`. A singular
/// matrix returns a zero mantissa.
pub(crate) fn det_complex(
    n: usize,
    entries: &[(usize, usize)],
    re: &[f64],
    im: &[f64],
) -> (Complex64, i32) {
    if n == 0 {
        return (Complex64::new(1.0, 0.0), 0);
    }
    let mut a = vec![Complex64::new(0.0, 0.0); n * n];
    for (k, (r, c)) in entries.iter().enumerate() {
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
