//! Numerical integration support: corrector coefficients
//! (`ni/nicomcof.c`), the companion-model evaluation
//! (`ni/niinteg.c::NIintegrate`) and the local truncation error
//! timestep estimate (`ckt/cktterr.c::CKTterr`), plus the opt-in
//! TR-BDF2 composite (not in any SPICE; issue #6).

use crate::state::States;

/// The `.options method=` integration method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// `trap` (default): the 3f5's trapezoidal rule, order 1 after
    /// breakpoints, 2 otherwise.
    Trapezoidal,
    /// `gear`: variable-order BDF up to `MAXORD`.
    Gear,
    /// `trbdf2`: Dune's opt-in TR-BDF2 composite (Bank et al., IEEE
    /// Trans. ED 32(10), 1985; Hosea & Shampine, Appl. Numer. Math.
    /// 20, 1996). Each step `h` is a trapezoidal stage to
    /// `t + γh`, `γ = 2 − √2`, then a BDF2 stage on
    /// `{t, t + γh, t + h}`: one-step, second order and L-stable, so
    /// stiff modes that the trapezoidal rule rings (`−1` amplification
    /// as `h/τ → ∞`) are damped. Not a SPICE method: the 3f5 contract
    /// (D-0002) holds only for `trap`/`gear`.
    TrBdf2,
}

/// `γ = 2 − √2` of TR-BDF2: the trapezoidal stage covers `γ·h`. With
/// this value both stages share the Jacobian coefficient
/// `2/(γh) = (2 − γ)/((1 − γ)h)`.
pub(crate) const TRBDF2_GAMMA: f64 = 2.0 - std::f64::consts::SQRT_2;

/// The TR-BDF2 local-error constant `k = (−3γ² + 4γ − 2)/(12(2 − γ))`
/// (`≈ −0.0404`): the step's charge error is `|k|·h³·|q'''|`, against
/// the trapezoidal rule's `h³·|q'''|/12` (Bank et al. 1985).
pub(crate) const TRBDF2_LTE_K: f64 = {
    let g = TRBDF2_GAMMA;
    (-3.0 * g * g + 4.0 * g - 2.0) / (12.0 * (2.0 - g))
};

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
        // The engine runs TR-BDF2 as a trapezoidal stage and a gear
        // stage and asks for those coefficients; the composite has
        // none of its own.
        Method::TrBdf2 => {
            return Err(crate::error::Error::Build(
                "TR-BDF2 has no single-step coefficients".into(),
            ));
        }
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
    st: &mut crate::state::StateView,
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
        // Devices see the stage method (the engine never hands them
        // `TrBdf2`); the gear sum is the stage that closes the step.
        Method::Gear | Method::TrBdf2 => {
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
    /// Step history aligned with the state banks: `[0]` spans bank 1
    /// → bank 0. Under TR-BDF2 the banks hold the stages, so `[0]` is
    /// the BDF2 stage and `[1]` the trapezoidal stage of the step
    /// being checked.
    pub delta_old: &'a [f64; 8],
    /// Accepted-step history (`[0]` is the step being checked), for
    /// devices that keep their own per-accepted-point history
    /// (`TRAtrunc`). Equal to `delta_old` except under TR-BDF2.
    pub step_old: &'a [f64; 8],
    pub order: usize,
    pub method: Method,
    /// `CKTdelta`; under TR-BDF2 the whole step `h`.
    pub delta: f64,
    pub reltol: f64,
    pub abstol: f64,
    pub chgtol: f64,
    pub trtol: f64,
}

/// `CKTterr`: shrink `time_step` to what the divided-difference LTE
/// estimate of the charge at `q` allows.
pub(crate) fn terr(q: usize, ctx: &TerrCtx, time_step: &mut f64) {
    if ctx.method == Method::TrBdf2 {
        terr_trbdf2(q, ctx, time_step);
        return;
    }
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
        // `TrBdf2` returned above.
        Method::Trapezoidal | Method::TrBdf2 => TRAP_COEFF[ctx.order - 1],
    };
    let mut del = ctx.trtol * tol / f64::max(ctx.abstol, factor * diff[0].abs());
    if ctx.order == 2 {
        del = del.sqrt();
    } else if ctx.order > 2 {
        del = (del.ln() / ctx.order as f64).exp();
    }
    *time_step = time_step.min(del);
}

/// The TR-BDF2 counterpart of [`terr`] for a composite step, in
/// `CKTterr`'s own form: same tolerances, same `trtol`, the `√` of a
/// second-order method, and `CKTterr`'s scale — at order 2 it compares
/// one sixth of the true local error (`TRAP_COEFF[1]·q[·,·,·,·]` is
/// `(h³q'''/12)/6` over `h³`, likewise gear-2's `2/9`), and so does
/// this: at equal derivatives and tolerance the composite is held to
/// the trapezoidal rule's true error.
///
/// The estimate is Bank et al.'s (1985): from the companion currents
/// `q'` at the three stage points (banks 2, 1, 0 = `t, t+γh, t+h`) the
/// step's charge error is `2|k|·h³·|q'[·,·,·]|`, with `q'[·,·,·] ≈
/// q'''/2` their second divided difference, so the measure is
/// `|k|·|q'[·,·,·]|/3`. It reads no history before the step. (The
/// order-1 restart steps the engine takes after breakpoints are plain
/// backward Euler and go through `CKTterr` itself.)
fn terr_trbdf2(q: usize, ctx: &TerrCtx, time_step: &mut f64) {
    let c = q + 1;
    let st = ctx.states;
    let (h2, h1) = (ctx.delta_old[0], ctx.delta_old[1]);
    let h = h1 + h2;
    let volttol = ctx.abstol + ctx.reltol * f64::max(st.get(0, c).abs(), st.get(2, c).abs());
    let chargetol = f64::max(st.get(0, q).abs(), st.get(2, q).abs());
    let chargetol = ctx.reltol * f64::max(chargetol, ctx.chgtol) / h;
    let tol = f64::max(volttol, chargetol);

    let (g0, gg, g1) = (st.get(2, c), st.get(1, c), st.get(0, c));
    let dd2 = ((g1 - gg) / h2 - (gg - g0) / h1) / h;
    let measure = TRBDF2_LTE_K.abs() * dd2.abs() / 3.0;
    let del = (ctx.trtol * tol / f64::max(ctx.abstol, measure)).sqrt();
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
        let (geq, ceq) = integrate(&mut st.view(), &ag, 1, Method::Trapezoidal, 1.0, 0);
        assert_eq!(geq, 1.0);
        assert_eq!(st.get(0, 1), 1.0); // current
        assert_eq!(ceq, 0.0);
    }

    /// One TR-BDF2 step of `q' = −λq` from `q = 1` through the engine's
    /// own pieces (`com_cof` per stage, `integrate`, bank rotation):
    /// each stage solves `ag0·q + ceq = −λq` for the new charge.
    fn trbdf2_decay(lambda: f64, h: f64) -> f64 {
        let mut st = States::new(2);
        let mut d = [h; 8];
        // Accepted point: q = 1, q' = −λ (consistent derivative).
        st.set(0, 0, 1.0);
        st.set(0, 1, -lambda);
        let solve = |st: &mut States, ag: &[f64; 7], order, method| {
            st.set(0, 0, 0.0);
            let (_, ceq) = integrate(&mut st.view(), ag, order, method, 1.0, 0);
            let q = -ceq / (ag[0] + lambda);
            st.set(0, 0, q);
            integrate(&mut st.view(), ag, order, method, 1.0, 0);
        };
        let d1 = TRBDF2_GAMMA * h;
        st.rotate(2);
        d[0] = d1;
        let ag = com_cof(Method::Trapezoidal, 2, d1, &d).unwrap();
        solve(&mut st, &ag, 2, Method::Trapezoidal);
        st.rotate(2);
        d.copy_within(0..7, 1);
        d[0] = h - d1;
        let ag = com_cof(Method::Gear, 2, h - d1, &d).unwrap();
        solve(&mut st, &ag, 2, Method::Gear);
        st.get(0, 0)
    }

    #[test]
    fn trbdf2_stages_share_the_jacobian_coefficient() {
        // γ = 2 − √2: 2/(γh) == (2 − γ)/((1 − γ)h), and com_cof's
        // non-uniform BDF2 is the TR-BDF2 second stage.
        let h = 1e-9;
        let d1 = TRBDF2_GAMMA * h;
        let mut d = [h; 8];
        d[0] = d1;
        let tr = com_cof(Method::Trapezoidal, 2, d1, &d).unwrap();
        d.copy_within(0..7, 1);
        d[0] = h - d1;
        let bdf = com_cof(Method::Gear, 2, h - d1, &d).unwrap();
        let rel = (tr[0] - bdf[0]).abs() / tr[0];
        assert!(rel < 1e-12, "{} vs {}", tr[0], bdf[0]);
        // Second stage: (1 − γ)/(2 − γ)·h·q'₁ = q₁ − q_γ/(γ(2−γ)) +
        // (1−γ)²/(γ(2−γ))·q₀.
        let g = TRBDF2_GAMMA;
        let w = (1.0 - g) / (2.0 - g) * h;
        assert!((bdf[0] * w - 1.0).abs() < 1e-12);
        assert!((bdf[1] * w + 1.0 / (g * (2.0 - g))).abs() < 1e-12);
        assert!((bdf[2] * w - (1.0 - g).powi(2) / (g * (2.0 - g))).abs() < 1e-12);
        assert!(com_cof(Method::TrBdf2, 2, h, &d).is_err());
    }

    #[test]
    fn trbdf2_is_l_stable_where_trap_rings() {
        // Stiff mode, h/τ = 1e6: trap's amplification → −1 (the
        // ringing of issue #6), TR-BDF2's → 0.
        let (lambda, h) = (1e15, 1e-9);
        let trap = (1.0 - lambda * h / 2.0) / (1.0 + lambda * h / 2.0);
        assert!(trap < -0.99999);
        let r = trbdf2_decay(lambda, h);
        assert!(r.abs() < 1e-5, "{r}");
        // Non-stiff: second order, local error |k|·(λh)³.
        let (lambda, h) = (1.0, 1e-2);
        let err = (trbdf2_decay(lambda, h) - (-lambda * h).exp()).abs();
        let pred = TRBDF2_LTE_K.abs() * (lambda * h).powi(3);
        assert!((err / pred - 1.0).abs() < 0.05, "{err} vs {pred}");
    }

    #[test]
    fn trbdf2_lte_estimate_is_exact_on_a_cubic() {
        // q = t³ on {t0, t0+γh, t0+h}: q'[·,·,·] = q'''/2 = 3 exactly;
        // the composite's measure is |k|·3/3 and del = √(trtol·tol/m).
        let (t0, h) = (0.3, 1e-2);
        let d1 = TRBDF2_GAMMA * h;
        let mut st = States::new(2);
        for (bank, t) in [(2, t0), (1, t0 + d1), (0, t0 + h)] {
            st.set(bank, 0, t * t * t);
            st.set(bank, 1, 3.0 * t * t);
        }
        let mut d = [h; 8];
        d[0] = h - d1;
        d[1] = d1;
        let ctx = TerrCtx {
            states: &st,
            sol: &[],
            time: t0 + h,
            delta_old: &d,
            step_old: &[h; 8],
            order: 2,
            method: Method::TrBdf2,
            delta: h,
            reltol: 1e-3,
            abstol: 1e-12,
            chgtol: 1e-14,
            trtol: 7.0,
        };
        let mut del = f64::MAX;
        terr(0, &ctx, &mut del);
        let tq = f64::max((t0 + h).powi(3), t0.powi(3));
        let ti = 3.0 * f64::max((t0 + h).powi(2), t0.powi(2));
        let tol = f64::max(1e-12 + 1e-3 * ti, 1e-3 * tq / h);
        let want = (7.0 * tol / (TRBDF2_LTE_K.abs() * 3.0 / 3.0)).sqrt();
        assert!((del / want - 1.0).abs() < 1e-6, "{del} vs {want}");
    }
}
