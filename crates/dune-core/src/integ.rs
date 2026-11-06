//! Numerical integration support: corrector coefficients
//! (`ni/nicomcof.c`), the companion-model evaluation
//! (`ni/niinteg.c::NIintegrate`) and the local truncation error
//! timestep estimate (`ckt/cktterr.c::CKTterr`).

use crate::state::States;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Trapezoidal,
    Gear,
}

/// `NIcomCof`: fill `ag[0..=order]` for the current step sizes.
/// `delta_old[0]` is the step being attempted.
// The literal `0.5 / (1.0 - 0.5)` and the index-style loops keep the
// C's shape ("should have 'xmu' instead of .5 eventually").
#[allow(clippy::eq_op, clippy::needless_range_loop)]
pub(crate) fn com_cof(
    method: Method,
    order: usize,
    delta: f64,
    delta_old: &[f64; 8],
) -> Result<[f64; 7], crate::error::Error> {
    let mut ag = [0.0f64; 7];
    match method {
        Method::Trapezoidal => match order {
            1 => {
                ag[0] = 1.0 / delta;
                ag[1] = -1.0 / delta;
            }
            2 => {
                // xmu = 0.5 (pure trapezoidal).
                ag[0] = 1.0 / delta / (1.0 - 0.5);
                ag[1] = 0.5 / (1.0 - 0.5);
            }
            _ => {
                return Err(crate::error::Error::Build(
                    "illegal integration order".into(),
                ));
            }
        },
        Method::Gear => {
            if order == 0 || order > 6 {
                return Err(crate::error::Error::Build(
                    "illegal integration order".into(),
                ));
            }
            // Solve for the BDF coefficients on the (possibly
            // non-uniform) grid, exactly like the C's little LU.
            let mut mat = [[0.0f64; 8]; 8];
            ag[1] = -1.0 / delta;
            for e in mat[0].iter_mut().take(order + 1) {
                *e = 1.0;
            }
            for row in mat.iter_mut().take(order + 1).skip(1) {
                row[0] = 0.0;
            }
            let mut arg = 0.0;
            for i in 1..=order {
                arg += delta_old[i - 1];
                let mut arg1 = 1.0;
                for j in 1..=order {
                    arg1 *= arg / delta;
                    mat[j][i] = arg1;
                }
            }
            // LU decompose.
            for i in 1..=order {
                for j in i + 1..=order {
                    mat[j][i] /= mat[i][i];
                    for k in i + 1..=order {
                        mat[j][k] -= mat[j][i] * mat[i][k];
                    }
                }
            }
            // Forward substitution.
            for i in 1..=order {
                for j in i + 1..=order {
                    ag[j] -= mat[j][i] * ag[i];
                }
            }
            // Backward substitution.
            ag[order] /= mat[order][order];
            for i in (0..order).rev() {
                for j in i + 1..=order {
                    ag[i] -= mat[i][j] * ag[j];
                }
                ag[i] /= mat[i][i];
            }
        }
    }
    Ok(ag)
}

/// `NIintegrate`: given the charge state at `q` (its companion
/// current lives at `q + 1`), update `state0[q+1]` and return
/// `(geq, ceq)`.
pub(crate) fn integrate(
    st: &mut States,
    ag: &[f64; 7],
    order: usize,
    method: Method,
    cap: f64,
    q: usize,
) -> (f64, f64) {
    let c = q + 1;
    match method {
        Method::Trapezoidal => match order {
            1 => {
                let v = ag[0] * st.get(0, q) + ag[1] * st.get(1, q);
                st.set(0, c, v);
            }
            _ => {
                let v = -st.get(1, c) * ag[1] + ag[0] * (st.get(0, q) - st.get(1, q));
                st.set(0, c, v);
            }
        },
        Method::Gear => {
            let mut v = 0.0;
            for (bank, a) in ag.iter().enumerate().take(order + 1).skip(1) {
                v += a * st.get(bank, q);
            }
            v += ag[0] * st.get(0, q);
            st.set(0, c, v);
        }
    }
    let ceq = st.get(0, c) - ag[0] * st.get(0, q);
    let geq = ag[0] * cap;
    (geq, ceq)
}

/// Inputs `CKTterr` needs from the circuit.
pub(crate) struct TerrCtx<'a> {
    pub states: &'a States,
    /// The just-converged solution (`CKTrhsOld`) and `CKTtime`, for
    /// devices that estimate from their own history (`LTRAtrunc`).
    pub sol: &'a [f64],
    pub time: f64,
    pub delta_old: &'a [f64; 8],
    pub order: usize,
    pub method: Method,
    pub delta: f64,
    pub reltol: f64,
    pub abstol: f64,
    pub chgtol: f64,
    pub trtol: f64,
}

/// `CKTterr`: shrink `time_step` to what the divided-difference LTE
/// estimate of the charge at `q` allows.
pub(crate) fn terr(q: usize, ctx: &TerrCtx, time_step: &mut f64) {
    const GEAR_COEFF: [f64; 6] = [
        0.5,
        0.2222222222,
        0.1363636364,
        0.096,
        0.07299270073,
        0.05830903790,
    ];
    const TRAP_COEFF: [f64; 2] = [0.5, 0.08333333333];

    let c = q + 1;
    let st = ctx.states;
    let volttol = ctx.abstol + ctx.reltol * f64::max(st.get(0, c).abs(), st.get(1, c).abs());
    let chargetol = f64::max(st.get(0, q).abs(), st.get(1, q).abs());
    let chargetol = ctx.reltol * f64::max(chargetol, ctx.chgtol) / ctx.delta;
    let tol = f64::max(volttol, chargetol);

    // Divided differences of the charge history.
    let mut diff = [0.0f64; 8];
    let mut deltmp = [0.0f64; 8];
    for i in (0..=ctx.order + 1).rev() {
        diff[i] = st.get(i, q);
    }
    deltmp[..=ctx.order].copy_from_slice(&ctx.delta_old[..=ctx.order]);
    let mut j = ctx.order;
    loop {
        for i in 0..=j {
            diff[i] = (diff[i] - diff[i + 1]) / deltmp[i];
        }
        if j == 0 {
            break;
        }
        j -= 1;
        for i in 0..=j {
            deltmp[i] = deltmp[i + 1] + ctx.delta_old[i];
        }
    }

    let factor = match ctx.method {
        Method::Gear => GEAR_COEFF[ctx.order - 1],
        Method::Trapezoidal => TRAP_COEFF[ctx.order - 1],
    };
    let mut del = ctx.trtol * tol / f64::max(ctx.abstol, factor * diff[0].abs());
    if ctx.order == 2 {
        del = del.sqrt();
    } else if ctx.order > 2 {
        del = (del.ln() / ctx.order as f64).exp();
    }
    *time_step = time_step.min(del);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trap_coefficients() {
        let d = [1e-3; 8];
        let ag = com_cof(Method::Trapezoidal, 1, 1e-3, &d).unwrap();
        assert_eq!(ag[0], 1e3);
        assert_eq!(ag[1], -1e3);
        let ag = com_cof(Method::Trapezoidal, 2, 1e-3, &d).unwrap();
        assert_eq!(ag[0], 2e3);
        assert_eq!(ag[1], 1.0);
    }

    #[test]
    fn gear_order_1_and_2_match_known_bdf() {
        let d = [1e-3; 8];
        // BDF1 on a uniform grid: ag = [1/h, -1/h].
        let ag = com_cof(Method::Gear, 1, 1e-3, &d).unwrap();
        assert!((ag[0] - 1e3).abs() < 1e-6);
        assert!((ag[1] + 1e3).abs() < 1e-6);
        // BDF2 uniform: [3/(2h), -2/h, 1/(2h)].
        let ag = com_cof(Method::Gear, 2, 1e-3, &d).unwrap();
        assert!((ag[0] - 1.5e3).abs() < 1e-6, "{}", ag[0]);
        assert!((ag[1] + 2e3).abs() < 1e-6);
        assert!((ag[2] - 0.5e3).abs() < 1e-6);
    }

    #[test]
    fn trap_integrate_rc_step() {
        // One backward-Euler step of a 1 F capacitor from 0 V to 1 V
        // in 1 s: i = C dv/dt = 1 A; geq = 1, ceq = i - ag0*q = -0.
        let mut st = States::new(2);
        let d = [1.0; 8];
        let ag = com_cof(Method::Trapezoidal, 1, 1.0, &d).unwrap();
        st.set(1, 0, 0.0); // old charge
        st.set(0, 0, 1.0); // new charge = C*v
        let (geq, ceq) = integrate(&mut st, &ag, 1, Method::Trapezoidal, 1.0, 0);
        assert_eq!(geq, 1.0);
        assert_eq!(st.get(0, 1), 1.0); // current
        assert_eq!(ceq, 0.0);
    }
}
