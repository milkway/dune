//! Third-order partial derivatives of a function of three variables
//! (`distodef.h`'s `Dderivs`, `dev/disto/*.c` — Roychowdhury 1989):
//! the bookkeeping the BJT and MOS2/MOS3 `.disto` setups use to
//! carry the Taylor coefficients of a nonlinear current through the
//! chain of elementary operations of the model equations.
//!
//! Every operation is a literal port of its C file (`EqualDeriv`,
//! `PlusDeriv`, `TimesDeriv`, `MultDeriv`, `DivDeriv`, `InvDeriv`,
//! `SqrtDeriv`, `PowDeriv`, `ExpDeriv`, `TanDeriv`, `AtanDeriv`,
//! `CosDeriv`, `CubeDeriv`), formula for formula and in the C's
//! order of evaluation, so that rounding matches; the C's
//! `new`/`old` aliasing is handled by returning a fresh value. The
//! variables are `p`, `q`, `r` as in the C; which voltages they
//! stand for is the caller's business.

/// `Dderivs`: the value and all partial derivatives up to third
/// order with respect to `p`, `q`, `r` (`d2_pq` = ∂²/∂p∂q, …;
/// *derivatives*, not Taylor coefficients — no 1/n! factors).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Dderivs {
    pub value: f64,
    pub d1_p: f64,
    pub d1_q: f64,
    pub d1_r: f64,
    pub d2_p2: f64,
    pub d2_q2: f64,
    pub d2_r2: f64,
    pub d2_pq: f64,
    pub d2_qr: f64,
    pub d2_pr: f64,
    pub d3_p3: f64,
    pub d3_q3: f64,
    pub d3_r3: f64,
    pub d3_p2r: f64,
    pub d3_p2q: f64,
    pub d3_q2r: f64,
    pub d3_pq2: f64,
    pub d3_pr2: f64,
    pub d3_qr2: f64,
    pub d3_pqr: f64,
}

impl Dderivs {
    /// The independent variable `p` at value 0 (the setups' `d_p`).
    pub fn var_p() -> Self {
        Dderivs {
            d1_p: 1.0,
            ..Dderivs::default()
        }
    }

    /// The independent variable `q` at value 0 (`d_q`).
    pub fn var_q() -> Self {
        Dderivs {
            d1_q: 1.0,
            ..Dderivs::default()
        }
    }

    /// The independent variable `r` at value 0 (`d_r`).
    pub fn var_r() -> Self {
        Dderivs {
            d1_r: 1.0,
            ..Dderivs::default()
        }
    }

    /// A constant (`EqualDeriv(&d, &d_zero); d.value = v`).
    pub fn constant(value: f64) -> Self {
        Dderivs {
            value,
            ..Dderivs::default()
        }
    }

    /// `PlusDeriv`: `self + o`.
    pub fn plus(&self, o: &Dderivs) -> Dderivs {
        Dderivs {
            value: self.value + o.value,
            d1_p: self.d1_p + o.d1_p,
            d1_q: self.d1_q + o.d1_q,
            d1_r: self.d1_r + o.d1_r,
            d2_p2: self.d2_p2 + o.d2_p2,
            d2_q2: self.d2_q2 + o.d2_q2,
            d2_r2: self.d2_r2 + o.d2_r2,
            d2_pq: self.d2_pq + o.d2_pq,
            d2_qr: self.d2_qr + o.d2_qr,
            d2_pr: self.d2_pr + o.d2_pr,
            d3_p3: self.d3_p3 + o.d3_p3,
            d3_q3: self.d3_q3 + o.d3_q3,
            d3_r3: self.d3_r3 + o.d3_r3,
            d3_p2r: self.d3_p2r + o.d3_p2r,
            d3_p2q: self.d3_p2q + o.d3_p2q,
            d3_q2r: self.d3_q2r + o.d3_q2r,
            d3_pq2: self.d3_pq2 + o.d3_pq2,
            d3_pr2: self.d3_pr2 + o.d3_pr2,
            d3_qr2: self.d3_qr2 + o.d3_qr2,
            d3_pqr: self.d3_pqr + o.d3_pqr,
        }
    }

    /// `TimesDeriv`: `k·self`.
    pub fn times(&self, k: f64) -> Dderivs {
        Dderivs {
            value: k * self.value,
            d1_p: k * self.d1_p,
            d1_q: k * self.d1_q,
            d1_r: k * self.d1_r,
            d2_p2: k * self.d2_p2,
            d2_q2: k * self.d2_q2,
            d2_r2: k * self.d2_r2,
            d2_pq: k * self.d2_pq,
            d2_qr: k * self.d2_qr,
            d2_pr: k * self.d2_pr,
            d3_p3: k * self.d3_p3,
            d3_q3: k * self.d3_q3,
            d3_r3: k * self.d3_r3,
            d3_p2r: k * self.d3_p2r,
            d3_p2q: k * self.d3_p2q,
            d3_q2r: k * self.d3_q2r,
            d3_pq2: k * self.d3_pq2,
            d3_pr2: k * self.d3_pr2,
            d3_qr2: k * self.d3_qr2,
            d3_pqr: k * self.d3_pqr,
        }
    }

    /// `MultDeriv`: `self·o` (Leibniz rule, the C's term order).
    #[rustfmt::skip]
    pub fn mult(&self, o: &Dderivs) -> Dderivs {
        let (t1, t2) = (self, o);
        Dderivs {
            value: t1.value * t2.value,
            d1_p: t1.d1_p*t2.value + t1.value*t2.d1_p,
            d1_q: t1.d1_q*t2.value + t1.value*t2.d1_q,
            d1_r: t1.d1_r*t2.value + t1.value*t2.d1_r,
            d2_p2: t1.d2_p2*t2.value + t1.d1_p*t2.d1_p + t1.d1_p*t2.d1_p + t1.value*t2.d2_p2,
            d2_q2: t1.d2_q2*t2.value + t1.d1_q*t2.d1_q + t1.d1_q*t2.d1_q + t1.value*t2.d2_q2,
            d2_r2: t1.d2_r2*t2.value + t1.d1_r*t2.d1_r + t1.d1_r*t2.d1_r + t1.value*t2.d2_r2,
            d2_pq: t1.d2_pq*t2.value + t1.d1_p*t2.d1_q + t1.d1_q*t2.d1_p + t1.value*t2.d2_pq,
            d2_qr: t1.d2_qr*t2.value + t1.d1_q*t2.d1_r + t1.d1_r*t2.d1_q + t1.value*t2.d2_qr,
            d2_pr: t1.d2_pr*t2.value + t1.d1_p*t2.d1_r + t1.d1_r*t2.d1_p + t1.value*t2.d2_pr,
            d3_p3: t1.d3_p3*t2.value + t1.d2_p2*t2.d1_p + t1.d2_p2*t2.d1_p
                + t2.d2_p2*t1.d1_p + t2.d2_p2*t1.d1_p + t1.d2_p2*t2.d1_p
                + t2.d2_p2*t1.d1_p + t1.value*t2.d3_p3,
            d3_q3: t1.d3_q3*t2.value + t1.d2_q2*t2.d1_q + t1.d2_q2*t2.d1_q
                + t2.d2_q2*t1.d1_q + t2.d2_q2*t1.d1_q + t1.d2_q2*t2.d1_q
                + t2.d2_q2*t1.d1_q + t1.value*t2.d3_q3,
            d3_r3: t1.d3_r3*t2.value + t1.d2_r2*t2.d1_r + t1.d2_r2*t2.d1_r
                + t2.d2_r2*t1.d1_r + t2.d2_r2*t1.d1_r + t1.d2_r2*t2.d1_r
                + t2.d2_r2*t1.d1_r + t1.value*t2.d3_r3,
            d3_p2r: t1.d3_p2r*t2.value + t1.d2_p2*t2.d1_r + t1.d2_pr*t2.d1_p
                + t2.d2_p2*t1.d1_r + t2.d2_pr*t1.d1_p + t1.d2_pr*t2.d1_p
                + t2.d2_pr*t1.d1_p + t1.value*t2.d3_p2r,
            d3_p2q: t1.d3_p2q*t2.value + t1.d2_p2*t2.d1_q + t1.d2_pq*t2.d1_p
                + t2.d2_p2*t1.d1_q + t2.d2_pq*t1.d1_p + t1.d2_pq*t2.d1_p
                + t2.d2_pq*t1.d1_p + t1.value*t2.d3_p2q,
            d3_q2r: t1.d3_q2r*t2.value + t1.d2_q2*t2.d1_r + t1.d2_qr*t2.d1_q
                + t2.d2_q2*t1.d1_r + t2.d2_qr*t1.d1_q + t1.d2_qr*t2.d1_q
                + t2.d2_qr*t1.d1_q + t1.value*t2.d3_q2r,
            d3_pq2: t1.d3_pq2*t2.value + t1.d2_q2*t2.d1_p + t1.d2_pq*t2.d1_q
                + t2.d2_q2*t1.d1_p + t2.d2_pq*t1.d1_q + t1.d2_pq*t2.d1_q
                + t2.d2_pq*t1.d1_q + t1.value*t2.d3_pq2,
            d3_pr2: t1.d3_pr2*t2.value + t1.d2_r2*t2.d1_p + t1.d2_pr*t2.d1_r
                + t2.d2_r2*t1.d1_p + t2.d2_pr*t1.d1_r + t1.d2_pr*t2.d1_r
                + t2.d2_pr*t1.d1_r + t1.value*t2.d3_pr2,
            d3_qr2: t1.d3_qr2*t2.value + t1.d2_r2*t2.d1_q + t1.d2_qr*t2.d1_r
                + t2.d2_r2*t1.d1_q + t2.d2_qr*t1.d1_r + t1.d2_qr*t2.d1_r
                + t2.d2_qr*t1.d1_r + t1.value*t2.d3_qr2,
            d3_pqr: t1.d3_pqr*t2.value + t1.d2_pq*t2.d1_r + t1.d2_pr*t2.d1_q
                + t2.d2_pq*t1.d1_r + t2.d2_qr*t1.d1_p + t1.d2_qr*t2.d1_p
                + t2.d2_pr*t1.d1_q + t1.value*t2.d3_pqr,
        }
    }

    /// `DivDeriv`: `self/den`. The higher orders reuse the lower
    /// ones of the result, as the C does.
    #[rustfmt::skip]
    pub fn div(&self, den: &Dderivs) -> Dderivs {
        let num = self;
        let mut n = Dderivs { value: num.value / den.value, ..Dderivs::default() };
        n.d1_p = (num.d1_p - num.value*den.d1_p/den.value)/den.value;
        n.d1_q = (num.d1_q - num.value*den.d1_q/den.value)/den.value;
        n.d1_r = (num.d1_r - num.value*den.d1_r/den.value)/den.value;
        n.d2_p2 = (num.d2_p2 - den.d1_p*n.d1_p - n.value*den.d2_p2 + den.d1_p*(n.value*den.d1_p - num.d1_p)/den.value)/den.value;
        n.d2_q2 = (num.d2_q2 - den.d1_q*n.d1_q - n.value*den.d2_q2 + den.d1_q*(n.value*den.d1_q - num.d1_q)/den.value)/den.value;
        n.d2_r2 = (num.d2_r2 - den.d1_r*n.d1_r - n.value*den.d2_r2 + den.d1_r*(n.value*den.d1_r - num.d1_r)/den.value)/den.value;
        n.d2_pq = (num.d2_pq - den.d1_q*n.d1_p - n.value*den.d2_pq + den.d1_p*(n.value*den.d1_q - num.d1_q)/den.value)/den.value;
        n.d2_qr = (num.d2_qr - den.d1_r*n.d1_q - n.value*den.d2_qr + den.d1_q*(n.value*den.d1_r - num.d1_r)/den.value)/den.value;
        n.d2_pr = (num.d2_pr - den.d1_r*n.d1_p - n.value*den.d2_pr + den.d1_p*(n.value*den.d1_r - num.d1_r)/den.value)/den.value;
        n.d3_p3 = (-den.d1_p*n.d2_p2 + num.d3_p3 - den.d2_p2*n.d1_p - den.d1_p*n.d2_p2 - n.d1_p*den.d2_p2 - n.value*den.d3_p3
            + (den.d1_p*(n.d1_p*den.d1_p + n.value*den.d2_p2 - num.d2_p2) + (n.value*den.d1_p - num.d1_p)*(den.d2_p2 - den.d1_p*den.d1_p/den.value))/den.value)/den.value;
        n.d3_q3 = (-den.d1_q*n.d2_q2 + num.d3_q3 - den.d2_q2*n.d1_q - den.d1_q*n.d2_q2 - n.d1_q*den.d2_q2 - n.value*den.d3_q3
            + (den.d1_q*(n.d1_q*den.d1_q + n.value*den.d2_q2 - num.d2_q2) + (n.value*den.d1_q - num.d1_q)*(den.d2_q2 - den.d1_q*den.d1_q/den.value))/den.value)/den.value;
        n.d3_r3 = (-den.d1_r*n.d2_r2 + num.d3_r3 - den.d2_r2*n.d1_r - den.d1_r*n.d2_r2 - n.d1_r*den.d2_r2 - n.value*den.d3_r3
            + (den.d1_r*(n.d1_r*den.d1_r + n.value*den.d2_r2 - num.d2_r2) + (n.value*den.d1_r - num.d1_r)*(den.d2_r2 - den.d1_r*den.d1_r/den.value))/den.value)/den.value;
        n.d3_p2r = (-den.d1_r*n.d2_p2 + num.d3_p2r - den.d2_pr*n.d1_p - den.d1_p*n.d2_pr - n.d1_r*den.d2_p2 - n.value*den.d3_p2r
            + (den.d1_p*(n.d1_r*den.d1_p + n.value*den.d2_pr - num.d2_pr) + (n.value*den.d1_p - num.d1_p)*(den.d2_pr - den.d1_p*den.d1_r/den.value))/den.value)/den.value;
        n.d3_p2q = (-den.d1_q*n.d2_p2 + num.d3_p2q - den.d2_pq*n.d1_p - den.d1_p*n.d2_pq - n.d1_q*den.d2_p2 - n.value*den.d3_p2q
            + (den.d1_p*(n.d1_q*den.d1_p + n.value*den.d2_pq - num.d2_pq) + (n.value*den.d1_p - num.d1_p)*(den.d2_pq - den.d1_p*den.d1_q/den.value))/den.value)/den.value;
        n.d3_q2r = (-den.d1_r*n.d2_q2 + num.d3_q2r - den.d2_qr*n.d1_q - den.d1_q*n.d2_qr - n.d1_r*den.d2_q2 - n.value*den.d3_q2r
            + (den.d1_q*(n.d1_r*den.d1_q + n.value*den.d2_qr - num.d2_qr) + (n.value*den.d1_q - num.d1_q)*(den.d2_qr - den.d1_q*den.d1_r/den.value))/den.value)/den.value;
        n.d3_pq2 = (-den.d1_p*n.d2_q2 + num.d3_pq2 - den.d2_pq*n.d1_q - den.d1_q*n.d2_pq - n.d1_p*den.d2_q2 - n.value*den.d3_pq2
            + (den.d1_q*(n.d1_p*den.d1_q + n.value*den.d2_pq - num.d2_pq) + (n.value*den.d1_q - num.d1_q)*(den.d2_pq - den.d1_q*den.d1_p/den.value))/den.value)/den.value;
        n.d3_pr2 = (-den.d1_p*n.d2_r2 + num.d3_pr2 - den.d2_pr*n.d1_r - den.d1_r*n.d2_pr - n.d1_p*den.d2_r2 - n.value*den.d3_pr2
            + (den.d1_r*(n.d1_p*den.d1_r + n.value*den.d2_pr - num.d2_pr) + (n.value*den.d1_r - num.d1_r)*(den.d2_pr - den.d1_r*den.d1_p/den.value))/den.value)/den.value;
        n.d3_qr2 = (-den.d1_q*n.d2_r2 + num.d3_qr2 - den.d2_qr*n.d1_r - den.d1_r*n.d2_qr - n.d1_q*den.d2_r2 - n.value*den.d3_qr2
            + (den.d1_r*(n.d1_q*den.d1_r + n.value*den.d2_qr - num.d2_qr) + (n.value*den.d1_r - num.d1_r)*(den.d2_qr - den.d1_r*den.d1_q/den.value))/den.value)/den.value;
        n.d3_pqr = (-den.d1_r*n.d2_pq + num.d3_pqr - den.d2_qr*n.d1_p - den.d1_q*n.d2_pr - n.d1_r*den.d2_pq - n.value*den.d3_pqr
            + (den.d1_p*(n.d1_r*den.d1_q + n.value*den.d2_qr - num.d2_qr) + (n.value*den.d1_q - num.d1_q)*(den.d2_pr - den.d1_p*den.d1_r/den.value))/den.value)/den.value;
        n
    }

    /// `InvDeriv`: `1/self`.
    #[rustfmt::skip]
    pub fn inv(&self) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: 1.0 / t.value, ..Dderivs::default() };
        n.d1_p = -n.value*n.value*t.d1_p;
        n.d1_q = -n.value*n.value*t.d1_q;
        n.d1_r = -n.value*n.value*t.d1_r;
        n.d2_p2 = -n.value*(2.0*n.d1_p*t.d1_p + n.value*t.d2_p2);
        n.d2_q2 = -n.value*(2.0*n.d1_q*t.d1_q + n.value*t.d2_q2);
        n.d2_r2 = -n.value*(2.0*n.d1_r*t.d1_r + n.value*t.d2_r2);
        n.d2_pq = -n.value*(2.0*n.d1_q*t.d1_p + n.value*t.d2_pq);
        n.d2_qr = -n.value*(2.0*n.d1_r*t.d1_q + n.value*t.d2_qr);
        n.d2_pr = -n.value*(2.0*n.d1_r*t.d1_p + n.value*t.d2_pr);
        n.d3_p3 = -(2.0*(t.d1_p*n.d1_p*n.d1_p + n.value*(n.d2_p2*t.d1_p + n.d1_p*t.d2_p2 + n.d1_p*t.d2_p2)) + n.value*n.value*t.d3_p3);
        n.d3_q3 = -(2.0*(t.d1_q*n.d1_q*n.d1_q + n.value*(n.d2_q2*t.d1_q + n.d1_q*t.d2_q2 + n.d1_q*t.d2_q2)) + n.value*n.value*t.d3_q3);
        n.d3_r3 = -(2.0*(t.d1_r*n.d1_r*n.d1_r + n.value*(n.d2_r2*t.d1_r + n.d1_r*t.d2_r2 + n.d1_r*t.d2_r2)) + n.value*n.value*t.d3_r3);
        n.d3_p2r = -(2.0*(t.d1_p*n.d1_p*n.d1_r + n.value*(n.d2_pr*t.d1_p + n.d1_p*t.d2_pr + n.d1_r*t.d2_p2)) + n.value*n.value*t.d3_p2r);
        n.d3_p2q = -(2.0*(t.d1_p*n.d1_p*n.d1_q + n.value*(n.d2_pq*t.d1_p + n.d1_p*t.d2_pq + n.d1_q*t.d2_p2)) + n.value*n.value*t.d3_p2q);
        n.d3_q2r = -(2.0*(t.d1_q*n.d1_q*n.d1_r + n.value*(n.d2_qr*t.d1_q + n.d1_q*t.d2_qr + n.d1_r*t.d2_q2)) + n.value*n.value*t.d3_q2r);
        n.d3_pq2 = -(2.0*(t.d1_q*n.d1_q*n.d1_p + n.value*(n.d2_pq*t.d1_q + n.d1_q*t.d2_pq + n.d1_p*t.d2_q2)) + n.value*n.value*t.d3_pq2);
        n.d3_pr2 = -(2.0*(t.d1_r*n.d1_r*n.d1_p + n.value*(n.d2_pr*t.d1_r + n.d1_r*t.d2_pr + n.d1_p*t.d2_r2)) + n.value*n.value*t.d3_pr2);
        n.d3_qr2 = -(2.0*(t.d1_r*n.d1_r*n.d1_q + n.value*(n.d2_qr*t.d1_r + n.d1_r*t.d2_qr + n.d1_q*t.d2_r2)) + n.value*n.value*t.d3_qr2);
        n.d3_pqr = -(2.0*(t.d1_p*n.d1_q*n.d1_r + n.value*(n.d2_qr*t.d1_p + n.d1_q*t.d2_pr + n.d1_r*t.d2_pq)) + n.value*n.value*t.d3_pqr);
        n
    }

    /// `SqrtDeriv`: `√self`; all derivatives zero at a zero
    /// argument (the C's guard).
    #[rustfmt::skip]
    pub fn sqrt(&self) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: t.value.sqrt(), ..Dderivs::default() };
        if t.value == 0.0 {
            return n;
        }
        n.d1_p = 0.5*t.d1_p/n.value;
        n.d1_q = 0.5*t.d1_q/n.value;
        n.d1_r = 0.5*t.d1_r/n.value;
        n.d2_p2 = 0.5/n.value*(t.d2_p2 - 0.5*t.d1_p*t.d1_p/t.value);
        n.d2_q2 = 0.5/n.value*(t.d2_q2 - 0.5*t.d1_q*t.d1_q/t.value);
        n.d2_r2 = 0.5/n.value*(t.d2_r2 - 0.5*t.d1_r*t.d1_r/t.value);
        n.d2_pq = 0.5/n.value*(t.d2_pq - 0.5*t.d1_p*t.d1_q/t.value);
        n.d2_qr = 0.5/n.value*(t.d2_qr - 0.5*t.d1_q*t.d1_r/t.value);
        n.d2_pr = 0.5/n.value*(t.d2_pr - 0.5*t.d1_p*t.d1_r/t.value);
        n.d3_p3 = 0.5*(t.d3_p3/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_p*t.d1_p*t.d1_p + t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2));
        n.d3_q3 = 0.5*(t.d3_q3/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_q*t.d1_q*t.d1_q + t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2));
        n.d3_r3 = 0.5*(t.d3_r3/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_r*t.d1_r*t.d1_r + t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2));
        n.d3_p2r = 0.5*(t.d3_p2r/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_p*t.d1_p*t.d1_r + t.d1_p*t.d2_pr + t.d1_p*t.d2_pr + t.d1_r*t.d2_p2));
        n.d3_p2q = 0.5*(t.d3_p2q/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_p*t.d1_p*t.d1_q + t.d1_p*t.d2_pq + t.d1_p*t.d2_pq + t.d1_q*t.d2_p2));
        n.d3_q2r = 0.5*(t.d3_q2r/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_q*t.d1_q*t.d1_r + t.d1_q*t.d2_qr + t.d1_q*t.d2_qr + t.d1_r*t.d2_q2));
        n.d3_pq2 = 0.5*(t.d3_pq2/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_q*t.d1_q*t.d1_p + t.d1_q*t.d2_pq + t.d1_q*t.d2_pq + t.d1_p*t.d2_q2));
        n.d3_pr2 = 0.5*(t.d3_pr2/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_r*t.d1_r*t.d1_p + t.d1_r*t.d2_pr + t.d1_r*t.d2_pr + t.d1_p*t.d2_r2));
        n.d3_qr2 = 0.5*(t.d3_qr2/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_r*t.d1_r*t.d1_q + t.d1_r*t.d2_qr + t.d1_r*t.d2_qr + t.d1_q*t.d2_r2));
        n.d3_pqr = 0.5*(t.d3_pqr/n.value - 0.5/(t.value*n.value)*(-1.5/t.value*t.d1_p*t.d1_q*t.d1_r + t.d1_p*t.d2_qr + t.d1_q*t.d2_pr + t.d1_r*t.d2_pq));
        n
    }

    /// `PowDeriv`: `self^emm`.
    #[rustfmt::skip]
    pub fn pow(&self, emm: f64) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: t.value.powf(emm), ..Dderivs::default() };
        let a = emm*n.value/t.value;
        n.d1_p = emm*n.value/t.value*t.d1_p;
        n.d1_q = emm*n.value/t.value*t.d1_q;
        n.d1_r = emm*n.value/t.value*t.d1_r;
        n.d2_p2 = a*((emm-1.0)/t.value*t.d1_p*t.d1_p + t.d2_p2);
        n.d2_q2 = a*((emm-1.0)/t.value*t.d1_q*t.d1_q + t.d2_q2);
        n.d2_r2 = a*((emm-1.0)/t.value*t.d1_r*t.d1_r + t.d2_r2);
        n.d2_pq = a*((emm-1.0)/t.value*t.d1_p*t.d1_q + t.d2_pq);
        n.d2_qr = a*((emm-1.0)/t.value*t.d1_q*t.d1_r + t.d2_qr);
        n.d2_pr = a*((emm-1.0)/t.value*t.d1_p*t.d1_r + t.d2_pr);
        let b = emm*(emm-1.0)*n.value/(t.value*t.value);
        n.d3_p3 = b*((emm-2.0)/t.value*t.d1_p*t.d1_p*t.d1_p + t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2) + a*t.d3_p3;
        n.d3_q3 = b*((emm-2.0)/t.value*t.d1_q*t.d1_q*t.d1_q + t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2) + a*t.d3_q3;
        n.d3_r3 = b*((emm-2.0)/t.value*t.d1_r*t.d1_r*t.d1_r + t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2) + a*t.d3_r3;
        n.d3_p2r = b*((emm-2.0)/t.value*t.d1_p*t.d1_p*t.d1_r + t.d1_p*t.d2_pr + t.d1_p*t.d2_pr + t.d1_r*t.d2_p2) + a*t.d3_p2r;
        n.d3_p2q = b*((emm-2.0)/t.value*t.d1_p*t.d1_p*t.d1_q + t.d1_p*t.d2_pq + t.d1_p*t.d2_pq + t.d1_q*t.d2_p2) + a*t.d3_p2q;
        n.d3_q2r = b*((emm-2.0)/t.value*t.d1_q*t.d1_q*t.d1_r + t.d1_q*t.d2_qr + t.d1_q*t.d2_qr + t.d1_r*t.d2_q2) + a*t.d3_q2r;
        n.d3_pq2 = b*((emm-2.0)/t.value*t.d1_q*t.d1_q*t.d1_p + t.d1_q*t.d2_pq + t.d1_q*t.d2_pq + t.d1_p*t.d2_q2) + a*t.d3_pq2;
        n.d3_pr2 = b*((emm-2.0)/t.value*t.d1_r*t.d1_r*t.d1_p + t.d1_r*t.d2_pr + t.d1_r*t.d2_pr + t.d1_p*t.d2_r2) + a*t.d3_pr2;
        n.d3_qr2 = b*((emm-2.0)/t.value*t.d1_r*t.d1_r*t.d1_q + t.d1_r*t.d2_qr + t.d1_r*t.d2_qr + t.d1_q*t.d2_r2) + a*t.d3_qr2;
        n.d3_pqr = b*((emm-2.0)/t.value*t.d1_p*t.d1_q*t.d1_r + t.d1_p*t.d2_qr + t.d1_q*t.d2_pr + t.d1_r*t.d2_pq) + a*t.d3_pqr;
        n
    }

    /// `ExpDeriv`: `exp(self)`.
    #[rustfmt::skip]
    pub fn exp(&self) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: t.value.exp(), ..Dderivs::default() };
        n.d1_p = n.value*t.d1_p;
        n.d1_q = n.value*t.d1_q;
        n.d1_r = n.value*t.d1_r;
        n.d2_p2 = n.value*t.d2_p2 + t.d1_p*n.d1_p;
        n.d2_q2 = n.value*t.d2_q2 + t.d1_q*n.d1_q;
        n.d2_r2 = n.value*t.d2_r2 + t.d1_r*n.d1_r;
        n.d2_pq = n.value*t.d2_pq + t.d1_p*n.d1_q;
        n.d2_qr = n.value*t.d2_qr + t.d1_q*n.d1_r;
        n.d2_pr = n.value*t.d2_pr + t.d1_p*n.d1_r;
        n.d3_p3 = n.value*t.d3_p3 + t.d2_p2*n.d1_p + t.d2_p2*n.d1_p + n.d2_p2*t.d1_p;
        n.d3_q3 = n.value*t.d3_q3 + t.d2_q2*n.d1_q + t.d2_q2*n.d1_q + n.d2_q2*t.d1_q;
        n.d3_r3 = n.value*t.d3_r3 + t.d2_r2*n.d1_r + t.d2_r2*n.d1_r + n.d2_r2*t.d1_r;
        n.d3_p2r = n.value*t.d3_p2r + t.d2_p2*n.d1_r + t.d2_pr*n.d1_p + n.d2_pr*t.d1_p;
        n.d3_p2q = n.value*t.d3_p2q + t.d2_p2*n.d1_q + t.d2_pq*n.d1_p + n.d2_pq*t.d1_p;
        n.d3_q2r = n.value*t.d3_q2r + t.d2_q2*n.d1_r + t.d2_qr*n.d1_q + n.d2_qr*t.d1_q;
        n.d3_pq2 = n.value*t.d3_pq2 + t.d2_q2*n.d1_p + t.d2_pq*n.d1_q + n.d2_pq*t.d1_q;
        n.d3_pr2 = n.value*t.d3_pr2 + t.d2_r2*n.d1_p + t.d2_pr*n.d1_r + n.d2_pr*t.d1_r;
        n.d3_qr2 = n.value*t.d3_qr2 + t.d2_r2*n.d1_q + t.d2_qr*n.d1_r + n.d2_qr*t.d1_r;
        n.d3_pqr = n.value*t.d3_pqr + t.d2_pq*n.d1_r + t.d2_pr*n.d1_q + n.d2_qr*t.d1_p;
        n
    }

    /// `TanDeriv`: `tan(self)`.
    #[rustfmt::skip]
    pub fn tan(&self) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: t.value.tan(), ..Dderivs::default() };
        let s = 1.0 + n.value*n.value;
        n.d1_p = s*t.d1_p;
        n.d1_q = s*t.d1_q;
        n.d1_r = s*t.d1_r;
        n.d2_p2 = s*t.d2_p2 + 2.0*n.value*t.d1_p*n.d1_p;
        n.d2_q2 = s*t.d2_q2 + 2.0*n.value*t.d1_q*n.d1_q;
        n.d2_r2 = s*t.d2_r2 + 2.0*n.value*t.d1_r*n.d1_r;
        n.d2_pq = s*t.d2_pq + 2.0*n.value*t.d1_p*n.d1_q;
        n.d2_qr = s*t.d2_qr + 2.0*n.value*t.d1_q*n.d1_r;
        n.d2_pr = s*t.d2_pr + 2.0*n.value*t.d1_p*n.d1_r;
        n.d3_p3 = s*t.d3_p3 + 2.0*(n.value*(t.d2_p2*n.d1_p + t.d2_p2*n.d1_p + n.d2_p2*t.d1_p) + t.d1_p*n.d1_p*n.d1_p);
        n.d3_q3 = s*t.d3_q3 + 2.0*(n.value*(t.d2_q2*n.d1_q + t.d2_q2*n.d1_q + n.d2_q2*t.d1_q) + t.d1_q*n.d1_q*n.d1_q);
        n.d3_r3 = s*t.d3_r3 + 2.0*(n.value*(t.d2_r2*n.d1_r + t.d2_r2*n.d1_r + n.d2_r2*t.d1_r) + t.d1_r*n.d1_r*n.d1_r);
        n.d3_p2r = s*t.d3_p2r + 2.0*(n.value*(t.d2_p2*n.d1_r + t.d2_pr*n.d1_p + n.d2_pr*t.d1_p) + t.d1_p*n.d1_p*n.d1_r);
        n.d3_p2q = s*t.d3_p2q + 2.0*(n.value*(t.d2_p2*n.d1_q + t.d2_pq*n.d1_p + n.d2_pq*t.d1_p) + t.d1_p*n.d1_p*n.d1_q);
        n.d3_q2r = s*t.d3_q2r + 2.0*(n.value*(t.d2_q2*n.d1_r + t.d2_qr*n.d1_q + n.d2_qr*t.d1_q) + t.d1_q*n.d1_q*n.d1_r);
        n.d3_pq2 = s*t.d3_pq2 + 2.0*(n.value*(t.d2_q2*n.d1_p + t.d2_pq*n.d1_q + n.d2_pq*t.d1_q) + t.d1_q*n.d1_q*n.d1_p);
        n.d3_pr2 = s*t.d3_pr2 + 2.0*(n.value*(t.d2_r2*n.d1_p + t.d2_pr*n.d1_r + n.d2_pr*t.d1_r) + t.d1_r*n.d1_r*n.d1_p);
        n.d3_qr2 = s*t.d3_qr2 + 2.0*(n.value*(t.d2_r2*n.d1_q + t.d2_qr*n.d1_r + n.d2_qr*t.d1_r) + t.d1_r*n.d1_r*n.d1_q);
        n.d3_pqr = s*t.d3_pqr + 2.0*(n.value*(t.d2_pq*n.d1_r + t.d2_pr*n.d1_q + n.d2_qr*t.d1_p) + t.d1_p*n.d1_q*n.d1_r);
        n
    }

    /// `AtanDeriv`: `atan(self)`.
    #[rustfmt::skip]
    pub fn atan(&self) -> Dderivs {
        let t = self;
        let mut n = Dderivs { value: t.value.atan(), ..Dderivs::default() };
        let s = 1.0 + t.value*t.value;
        n.d1_p = t.d1_p/s;
        n.d1_q = t.d1_q/s;
        n.d1_r = t.d1_r/s;
        n.d2_p2 = t.d2_p2/s - 2.0*t.value*n.d1_p*n.d1_p;
        n.d2_q2 = t.d2_q2/s - 2.0*t.value*n.d1_q*n.d1_q;
        n.d2_r2 = t.d2_r2/s - 2.0*t.value*n.d1_r*n.d1_r;
        n.d2_pq = t.d2_pq/s - 2.0*t.value*n.d1_p*n.d1_q;
        n.d2_qr = t.d2_qr/s - 2.0*t.value*n.d1_q*n.d1_r;
        n.d2_pr = t.d2_pr/s - 2.0*t.value*n.d1_p*n.d1_r;
        n.d3_p3 = (t.d3_p3 - t.d2_p2*n.d1_p*2.0*t.value)/s - 2.0*(n.d1_p*n.d1_p*t.d1_p + t.value*(n.d2_p2*n.d1_p + n.d2_p2*n.d1_p));
        n.d3_q3 = (t.d3_q3 - t.d2_q2*n.d1_q*2.0*t.value)/s - 2.0*(n.d1_q*n.d1_q*t.d1_q + t.value*(n.d2_q2*n.d1_q + n.d2_q2*n.d1_q));
        n.d3_r3 = (t.d3_r3 - t.d2_r2*n.d1_r*2.0*t.value)/s - 2.0*(n.d1_r*n.d1_r*t.d1_r + t.value*(n.d2_r2*n.d1_r + n.d2_r2*n.d1_r));
        n.d3_p2r = (t.d3_p2r - t.d2_p2*n.d1_r*2.0*t.value)/s - 2.0*(n.d1_p*n.d1_p*t.d1_r + t.value*(n.d2_pr*n.d1_p + n.d2_pr*n.d1_p));
        n.d3_p2q = (t.d3_p2q - t.d2_p2*n.d1_q*2.0*t.value)/s - 2.0*(n.d1_p*n.d1_p*t.d1_q + t.value*(n.d2_pq*n.d1_p + n.d2_pq*n.d1_p));
        n.d3_q2r = (t.d3_q2r - t.d2_q2*n.d1_r*2.0*t.value)/s - 2.0*(n.d1_q*n.d1_q*t.d1_r + t.value*(n.d2_qr*n.d1_q + n.d2_qr*n.d1_q));
        n.d3_pq2 = (t.d3_pq2 - t.d2_q2*n.d1_p*2.0*t.value)/s - 2.0*(n.d1_q*n.d1_q*t.d1_p + t.value*(n.d2_pq*n.d1_q + n.d2_pq*n.d1_q));
        n.d3_pr2 = (t.d3_pr2 - t.d2_r2*n.d1_p*2.0*t.value)/s - 2.0*(n.d1_r*n.d1_r*t.d1_p + t.value*(n.d2_pr*n.d1_r + n.d2_pr*n.d1_r));
        n.d3_qr2 = (t.d3_qr2 - t.d2_r2*n.d1_q*2.0*t.value)/s - 2.0*(n.d1_r*n.d1_r*t.d1_q + t.value*(n.d2_qr*n.d1_r + n.d2_qr*n.d1_r));
        n.d3_pqr = (t.d3_pqr - t.d2_pq*n.d1_r*2.0*t.value)/s - 2.0*(n.d1_p*n.d1_q*t.d1_r + t.value*(n.d2_pr*n.d1_q + n.d2_qr*n.d1_p));
        n
    }

    /// `CosDeriv`: `cos(self)`.
    #[rustfmt::skip]
    pub fn cos(&self) -> Dderivs {
        let t = self;
        let (s, c) = (t.value.sin(), t.value.cos());
        Dderivs {
            value: c,
            d1_p: -s*t.d1_p,
            d1_q: -s*t.d1_q,
            d1_r: -s*t.d1_r,
            d2_p2: -(c*t.d1_p*t.d1_p + s*t.d2_p2),
            d2_q2: -(c*t.d1_q*t.d1_q + s*t.d2_q2),
            d2_r2: -(c*t.d1_r*t.d1_r + s*t.d2_r2),
            d2_pq: -(c*t.d1_p*t.d1_q + s*t.d2_pq),
            d2_qr: -(c*t.d1_q*t.d1_r + s*t.d2_qr),
            d2_pr: -(c*t.d1_p*t.d1_r + s*t.d2_pr),
            d3_p3: -(s*(t.d3_p3 - t.d1_p*t.d1_p*t.d1_p) + c*(t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2 + t.d1_p*t.d2_p2)),
            d3_q3: -(s*(t.d3_q3 - t.d1_q*t.d1_q*t.d1_q) + c*(t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2 + t.d1_q*t.d2_q2)),
            d3_r3: -(s*(t.d3_r3 - t.d1_r*t.d1_r*t.d1_r) + c*(t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2 + t.d1_r*t.d2_r2)),
            d3_p2r: -(s*(t.d3_p2r - t.d1_r*t.d1_p*t.d1_p) + c*(t.d1_p*t.d2_pr + t.d1_p*t.d2_pr + t.d1_r*t.d2_p2)),
            d3_p2q: -(s*(t.d3_p2q - t.d1_q*t.d1_p*t.d1_p) + c*(t.d1_p*t.d2_pq + t.d1_p*t.d2_pq + t.d1_q*t.d2_p2)),
            d3_q2r: -(s*(t.d3_q2r - t.d1_r*t.d1_q*t.d1_q) + c*(t.d1_q*t.d2_qr + t.d1_q*t.d2_qr + t.d1_r*t.d2_q2)),
            d3_pq2: -(s*(t.d3_pq2 - t.d1_p*t.d1_q*t.d1_q) + c*(t.d1_q*t.d2_pq + t.d1_q*t.d2_pq + t.d1_p*t.d2_q2)),
            d3_pr2: -(s*(t.d3_pr2 - t.d1_p*t.d1_r*t.d1_r) + c*(t.d1_r*t.d2_pr + t.d1_r*t.d2_pr + t.d1_p*t.d2_r2)),
            d3_qr2: -(s*(t.d3_qr2 - t.d1_q*t.d1_r*t.d1_r) + c*(t.d1_r*t.d2_qr + t.d1_r*t.d2_qr + t.d1_q*t.d2_r2)),
            d3_pqr: -(s*(t.d3_pqr - t.d1_r*t.d1_p*t.d1_q) + c*(t.d1_q*t.d2_pr + t.d1_p*t.d2_qr + t.d1_r*t.d2_pq)),
        }
    }

    /// `CubeDeriv`: `self³`.
    #[rustfmt::skip]
    pub fn cube(&self) -> Dderivs {
        let t = self;
        let v = t.value;
        Dderivs {
            value: v*v*v,
            d1_p: 3.0*v*v*t.d1_p,
            d1_q: 3.0*v*v*t.d1_q,
            d1_r: 3.0*v*v*t.d1_r,
            d2_p2: 3.0*(2.0*v*t.d1_p*t.d1_p + v*v*t.d2_p2),
            d2_q2: 3.0*(2.0*v*t.d1_q*t.d1_q + v*v*t.d2_q2),
            d2_r2: 3.0*(2.0*v*t.d1_r*t.d1_r + v*v*t.d2_r2),
            d2_pq: 3.0*(2.0*v*t.d1_p*t.d1_q + v*v*t.d2_pq),
            d2_qr: 3.0*(2.0*v*t.d1_q*t.d1_r + v*v*t.d2_qr),
            d2_pr: 3.0*(2.0*v*t.d1_p*t.d1_r + v*v*t.d2_pr),
            d3_p3: 3.0*(2.0*(t.d1_p*t.d1_p*t.d1_p + v*(t.d2_p2*t.d1_p + t.d2_p2*t.d1_p + t.d2_p2*t.d1_p)) + v*v*t.d3_p3),
            d3_q3: 3.0*(2.0*(t.d1_q*t.d1_q*t.d1_q + v*(t.d2_q2*t.d1_q + t.d2_q2*t.d1_q + t.d2_q2*t.d1_q)) + v*v*t.d3_q3),
            d3_r3: 3.0*(2.0*(t.d1_r*t.d1_r*t.d1_r + v*(t.d2_r2*t.d1_r + t.d2_r2*t.d1_r + t.d2_r2*t.d1_r)) + v*v*t.d3_r3),
            d3_p2r: 3.0*(2.0*(t.d1_p*t.d1_p*t.d1_r + v*(t.d2_p2*t.d1_r + t.d2_pr*t.d1_p + t.d2_pr*t.d1_p)) + v*v*t.d3_p2r),
            d3_p2q: 3.0*(2.0*(t.d1_p*t.d1_p*t.d1_q + v*(t.d2_p2*t.d1_q + t.d2_pq*t.d1_p + t.d2_pq*t.d1_p)) + v*v*t.d3_p2q),
            d3_q2r: 3.0*(2.0*(t.d1_q*t.d1_q*t.d1_r + v*(t.d2_q2*t.d1_r + t.d2_qr*t.d1_q + t.d2_qr*t.d1_q)) + v*v*t.d3_q2r),
            d3_pq2: 3.0*(2.0*(t.d1_q*t.d1_q*t.d1_p + v*(t.d2_q2*t.d1_p + t.d2_pq*t.d1_q + t.d2_pq*t.d1_q)) + v*v*t.d3_pq2),
            d3_pr2: 3.0*(2.0*(t.d1_r*t.d1_r*t.d1_p + v*(t.d2_r2*t.d1_p + t.d2_pr*t.d1_r + t.d2_pr*t.d1_r)) + v*v*t.d3_pr2),
            d3_qr2: 3.0*(2.0*(t.d1_r*t.d1_r*t.d1_q + v*(t.d2_r2*t.d1_q + t.d2_qr*t.d1_r + t.d2_qr*t.d1_r)) + v*v*t.d3_qr2),
            d3_pqr: 3.0*(2.0*(t.d1_p*t.d1_q*t.d1_r + v*(t.d2_pq*t.d1_r + t.d2_qr*t.d1_p + t.d2_pr*t.d1_q)) + v*v*t.d3_pqr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A smooth test point: three affine-ish "voltages" with nonzero
    /// higher derivatives, so that every mixed term is exercised.
    fn seeds() -> (Dderivs, Dderivs, Dderivs) {
        let p = Dderivs::var_p();
        let q = Dderivs::var_q();
        let r = Dderivs::var_r();
        // a = 0.7 + 0.3p − 0.2q + 0.1r + 0.05·p·q
        let a = p
            .times(0.3)
            .plus(&q.times(-0.2))
            .plus(&r.times(0.1))
            .plus(&p.mult(&q).times(0.05))
            .plus(&Dderivs::constant(0.7));
        // b = 1.3 − 0.4p + 0.25q·r + 0.15r
        let b = p
            .times(-0.4)
            .plus(&q.mult(&r).times(0.25))
            .plus(&r.times(0.15))
            .plus(&Dderivs::constant(1.3));
        // c = 0.4 + 0.2p·r − 0.3q
        let c = p
            .mult(&r)
            .times(0.2)
            .plus(&q.times(-0.3))
            .plus(&Dderivs::constant(0.4));
        (a, b, c)
    }

    /// Evaluate the same composite on plain numbers.
    fn seeds_at(p: f64, q: f64, r: f64) -> (f64, f64, f64) {
        (
            0.7 + 0.3 * p - 0.2 * q + 0.1 * r + 0.05 * p * q,
            1.3 - 0.4 * p + 0.25 * q * r + 0.15 * r,
            0.4 + 0.2 * p * r - 0.3 * q,
        )
    }

    /// Central finite differences of `f` at the origin for every
    /// entry of `Dderivs`, compared with the propagated derivatives.
    fn check(d: &Dderivs, f: &dyn Fn(f64, f64, f64) -> f64, what: &str) {
        let h = 1e-3;
        let e = |i: usize| {
            let mut v = [0.0; 3];
            v[i] = 1.0;
            v
        };
        let at = |x: [f64; 3]| f(x[0], x[1], x[2]);
        let add =
            |a: [f64; 3], b: [f64; 3], s: f64| [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]];
        let z = [0.0; 3];
        let d1 = |i: usize| (at(add(z, e(i), h)) - at(add(z, e(i), -h))) / (2.0 * h);
        let d2 = |i: usize, j: usize| {
            let (ei, ej) = (e(i), e(j));
            (at(add(add(z, ei, h), ej, h))
                - at(add(add(z, ei, h), ej, -h))
                - at(add(add(z, ei, -h), ej, h))
                + at(add(add(z, ei, -h), ej, -h)))
                / (4.0 * h * h)
        };
        let h3 = 1e-2;
        let d3 = |i: usize, j: usize, k: usize| {
            let mut s = 0.0;
            for a in [-1.0, 1.0] {
                for b in [-1.0, 1.0] {
                    for c in [-1.0, 1.0] {
                        let x = add(add(add(z, e(i), a * h3), e(j), b * h3), e(k), c * h3);
                        s += a * b * c * at(x);
                    }
                }
            }
            s / (8.0 * h3 * h3 * h3)
        };
        let got = [
            d.value, d.d1_p, d.d1_q, d.d1_r, d.d2_p2, d.d2_q2, d.d2_r2, d.d2_pq, d.d2_qr, d.d2_pr,
            d.d3_p3, d.d3_q3, d.d3_r3, d.d3_p2r, d.d3_p2q, d.d3_q2r, d.d3_pq2, d.d3_pr2, d.d3_qr2,
            d.d3_pqr,
        ];
        let want = [
            at(z),
            d1(0),
            d1(1),
            d1(2),
            d2(0, 0),
            d2(1, 1),
            d2(2, 2),
            d2(0, 1),
            d2(1, 2),
            d2(0, 2),
            d3(0, 0, 0),
            d3(1, 1, 1),
            d3(2, 2, 2),
            d3(0, 0, 2),
            d3(0, 0, 1),
            d3(1, 1, 2),
            d3(0, 1, 1),
            d3(0, 2, 2),
            d3(1, 2, 2),
            d3(0, 1, 2),
        ];
        let names = [
            "value", "d1_p", "d1_q", "d1_r", "d2_p2", "d2_q2", "d2_r2", "d2_pq", "d2_qr", "d2_pr",
            "d3_p3", "d3_q3", "d3_r3", "d3_p2r", "d3_p2q", "d3_q2r", "d3_pq2", "d3_pr2", "d3_qr2",
            "d3_pqr",
        ];
        for k in 0..20 {
            let tol = 2e-3 * (1.0 + want[k].abs());
            assert!(
                (got[k] - want[k]).abs() <= tol,
                "{what}.{}: propagated {} vs finite-difference {}",
                names[k],
                got[k],
                want[k]
            );
        }
    }

    /// Every C derivative rule agrees with finite differences of the
    /// function it claims to differentiate (so the literal ports
    /// carry no transcription slip, and the C formulas are exact).
    #[test]
    fn rules_match_finite_differences() {
        let (a, b, c) = seeds();
        let fa = |p, q, r| seeds_at(p, q, r).0;
        let fb = |p, q, r| seeds_at(p, q, r).1;
        let fc = |p, q, r| seeds_at(p, q, r).2;
        check(&a, &fa, "seed a");
        check(&a.mult(&b), &|p, q, r| fa(p, q, r) * fb(p, q, r), "mult");
        check(&a.div(&b), &|p, q, r| fa(p, q, r) / fb(p, q, r), "div");
        check(&b.inv(), &|p, q, r| 1.0 / fb(p, q, r), "inv");
        check(&a.sqrt(), &|p, q, r| fa(p, q, r).sqrt(), "sqrt");
        check(&b.pow(0.37), &|p, q, r| fb(p, q, r).powf(0.37), "pow");
        check(&c.exp(), &|p, q, r| fc(p, q, r).exp(), "exp");
        check(&c.tan(), &|p, q, r| fc(p, q, r).tan(), "tan");
        check(&c.atan(), &|p, q, r| fc(p, q, r).atan(), "atan");
        check(&c.cos(), &|p, q, r| fc(p, q, r).cos(), "cos");
        check(&c.cube(), &|p, q, r| fc(p, q, r).powi(3), "cube");
        check(
            &a.mult(&c).div(&b.sqrt()).exp(),
            &|p, q, r| (fa(p, q, r) * fc(p, q, r) / fb(p, q, r).sqrt()).exp(),
            "composite",
        );
    }
}
