//! Time-dependent source functions (`dev/vsrc/vsrcload.c`,
//! `vsrcacct.c`; the I-source mirrors them). Arguments are stored as
//! given on the card; the defaults that depend on TSTEP and TSTOP
//! are resolved at evaluation time, exactly like the C macros.

use crate::matrix::AcceptCtx;

/// A transient source function: `PULSE SIN EXP PWL SFFM`, raw args.
#[derive(Clone, Debug)]
pub enum Wave {
    Pulse(Vec<f64>),
    Sin(Vec<f64>),
    Exp(Vec<f64>),
    Pwl(Vec<f64>),
    Sffm(Vec<f64>),
}

fn arg(c: &[f64], i: usize) -> Option<f64> {
    c.get(i).copied()
}

/// An argument that falls back to a default when missing **or zero**
/// (most of the C's macros test `&& coeffs[i] != 0`).
fn arg_nz(c: &[f64], i: usize, default: f64) -> f64 {
    match c.get(i) {
        Some(&v) if v != 0.0 => v,
        _ => default,
    }
}

impl Wave {
    /// Source value at `time` (`VSRCload`'s transient switch).
    pub(crate) fn value(&self, time: f64, step: f64, final_time: f64) -> f64 {
        match self {
            Wave::Pulse(c) => {
                let v1 = arg(c, 0).unwrap_or(0.0);
                let v2 = arg(c, 1).unwrap_or(0.0);
                let td = arg(c, 2).unwrap_or(0.0);
                let tr = arg_nz(c, 3, step);
                let tf = arg_nz(c, 4, step);
                let pw = arg_nz(c, 5, final_time);
                let per = arg_nz(c, 6, final_time);
                let mut time = time - td;
                if time > per {
                    // Repeating signal - figure out where we are in
                    // the period.
                    let basetime = per * (time / per).floor();
                    time -= basetime;
                }
                if time <= 0.0 || time >= tr + pw + tf {
                    v1
                } else if time >= tr && time <= tr + pw {
                    v2
                } else if time > 0.0 && time < tr {
                    v1 + (v2 - v1) * time / tr
                } else {
                    v2 + (v1 - v2) * (time - (tr + pw)) / tf
                }
            }
            Wave::Sin(c) => {
                let vo = arg(c, 0).unwrap_or(0.0);
                let va = arg(c, 1).unwrap_or(0.0);
                let freq = arg_nz(c, 2, 1.0 / final_time);
                let td = arg(c, 3).unwrap_or(0.0);
                let theta = arg(c, 4).unwrap_or(0.0);
                let time = time - td;
                if time <= 0.0 {
                    vo
                } else {
                    vo + va
                        * (freq * time * 2.0 * std::f64::consts::PI).sin()
                        * (-(time * theta)).exp()
                }
            }
            Wave::Exp(c) => {
                let v1 = arg(c, 0).unwrap_or(0.0);
                let v2 = arg(c, 1).unwrap_or(0.0);
                let td1 = arg(c, 2).unwrap_or(step);
                let tau1 = arg_nz(c, 3, step);
                let td2 = arg_nz(c, 4, td1 + step);
                let tau2 = arg_nz(c, 5, step);
                if time <= td1 {
                    v1
                } else if time <= td2 {
                    v1 + (v2 - v1) * (1.0 - (-(time - td1) / tau1).exp())
                } else {
                    v1 + (v2 - v1) * (1.0 - (-(time - td1) / tau1).exp())
                        + (v1 - v2) * (1.0 - (-(time - td2) / tau2).exp())
                }
            }
            Wave::Sffm(c) => {
                let vo = arg(c, 0).unwrap_or(0.0);
                let va = arg(c, 1).unwrap_or(0.0);
                let fc = arg_nz(c, 2, 1.0 / final_time);
                let mdi = arg(c, 3).unwrap_or(0.0);
                let fs = arg_nz(c, 4, 1.0 / final_time);
                // The C spells pi 3.141592654 here (and only here).
                #[allow(clippy::approx_constant)]
                const PI: f64 = 3.141_592_654;
                vo + va * ((2.0 * PI * fc * time) + mdi * (2.0 * PI * fs * time).sin()).sin()
            }
            Wave::Pwl(c) => {
                let pairs = c.len() / 2;
                if pairs == 0 {
                    return 0.0;
                }
                if time < c[0] {
                    return c[1];
                }
                for i in 0..pairs.saturating_sub(1) {
                    let (t0, v0) = (c[2 * i], c[2 * i + 1]);
                    let (t1, v1) = (c[2 * i + 2], c[2 * i + 3]);
                    if t0 == time {
                        return v0;
                    }
                    if t0 < time && t1 > time {
                        return v0 + (time - t0) / (t1 - t0) * (v1 - v0);
                    }
                }
                c[2 * pairs - 1]
            }
        }
    }

    /// The waveform's corners inside `(t0, t1)`: every PULSE corner
    /// (`TD + k·PER + {0, TR, TR+PW, TR+PW+TF}`, defaults as in
    /// [`Wave::value`]) and every PWL vertex time — the breakpoints
    /// [`Wave::accept`] would have planted by `t0` had the transient
    /// started at 0. At most 4 corners per period are listed, and a
    /// pulse with more than 10 000 periods in the window lists none
    /// (the step control then meets its corners unaided).
    pub(crate) fn corners(&self, t0: f64, t1: f64, step: f64, final_time: f64, out: &mut Vec<f64>) {
        let inside = |t: f64| t > t0 && t < t1;
        match self {
            Wave::Pulse(c) => {
                let td = arg(c, 2).unwrap_or(0.0);
                let tr = arg_nz(c, 3, step);
                let tf = arg_nz(c, 4, step);
                let pw = arg_nz(c, 5, final_time);
                let per = arg_nz(c, 6, final_time);
                let offsets = [0.0, tr, tr + pw, tr + pw + tf];
                if !(per > 0.0 && per.is_finite()) {
                    return;
                }
                let k0 = ((t0 - td) / per).floor() - 1.0;
                let k1 = ((t1 - td) / per).ceil() + 1.0;
                if !(k0.is_finite() && k1.is_finite()) || k1 - k0 > 10_000.0 {
                    return;
                }
                let mut k = k0.max(0.0);
                while k <= k1 {
                    for o in offsets {
                        let t = td + k * per + o;
                        if inside(t) {
                            out.push(t);
                        }
                    }
                    k += 1.0;
                }
            }
            Wave::Pwl(c) => {
                // Every even index is a PWL corner time.
                for &t in c.iter().step_by(2) {
                    if inside(t) {
                        out.push(t);
                    }
                }
            }
            _ => {}
        }
    }

    /// Breakpoint generation at an accepted timepoint
    /// (`VSRCaccept`); only PULSE and PWL make breakpoints in the
    /// 3f5 ("no breakpoints (yet)" for the rest).
    pub(crate) fn accept(&self, ctx: &mut AcceptCtx) {
        match self {
            Wave::Pulse(c) => {
                let td = arg(c, 2).unwrap_or(0.0);
                let tr = arg_nz(c, 3, ctx.step);
                let tf = arg_nz(c, 4, ctx.step);
                let pw = arg_nz(c, 5, ctx.final_time);
                let per = arg_nz(c, 6, ctx.final_time);
                let timetol = 1e-7 * pw;
                let same = |a: f64, b: f64| (a - b).abs() <= timetol;

                let mut time = ctx.time - td;
                let mut basetime = 0.0;
                if time >= per {
                    basetime = per * (time / per).floor();
                    time -= basetime;
                }
                if !ctx.brk {
                    return;
                }
                if time <= 0.0 || time >= tr + pw + tf {
                    if same(time, 0.0) {
                        ctx.set_break(basetime + tr + td);
                    } else if same(tr + pw + tf, time) {
                        ctx.set_break(basetime + per + td);
                    } else if time == -td {
                        ctx.set_break(basetime + td);
                    } else if same(per, time) {
                        ctx.set_break(basetime + td + tr + per);
                    }
                } else if time >= tr && time <= tr + pw {
                    if same(time, tr) {
                        ctx.set_break(basetime + td + tr + pw);
                    } else if same(tr + pw, time) {
                        ctx.set_break(basetime + td + tr + pw + tf);
                    }
                } else if time > 0.0 && time < tr {
                    if same(time, 0.0) {
                        ctx.set_break(basetime + td + tr);
                    } else if same(time, tr) {
                        ctx.set_break(basetime + td + tr + pw);
                    }
                } else {
                    // tr + pw < time < tr + pw + tf
                    if same(time, tr + pw) {
                        ctx.set_break(basetime + td + tr + pw + tf);
                    } else if same(time, tr + pw + tf) {
                        ctx.set_break(basetime + td + per);
                    }
                }
            }
            Wave::Pwl(c) => {
                let pairs = c.len() / 2;
                if pairs == 0 || !ctx.brk {
                    return;
                }
                if ctx.time < c[0] {
                    ctx.set_break(c[0]);
                    return;
                }
                for i in 0..pairs.saturating_sub(1) {
                    if c[2 * i] == ctx.time {
                        ctx.set_break(c[2 * i + 2]);
                        return;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_shape_and_defaults() {
        // pulse(0 1) with tstep 0.1, tstop 7 (rc.cir): TR=TF=0.1,
        // PW=PER=7.
        let w = Wave::Pulse(vec![0.0, 1.0]);
        assert_eq!(w.value(0.0, 0.1, 7.0), 0.0);
        assert!((w.value(0.05, 0.1, 7.0) - 0.5).abs() < 1e-12);
        assert_eq!(w.value(0.1, 0.1, 7.0), 1.0);
        assert_eq!(w.value(5.0, 0.1, 7.0), 1.0);
    }

    #[test]
    fn pulse_full_period() {
        let w = Wave::Pulse(vec![-1.0, 1.0, 1.0, 0.5, 0.5, 2.0, 5.0]);
        assert_eq!(w.value(0.0, 0.1, 100.0), -1.0); // before TD
        assert_eq!(w.value(1.25, 0.1, 100.0), 0.0); // mid rise
        assert_eq!(w.value(2.0, 0.1, 100.0), 1.0); // on
        assert_eq!(w.value(3.75, 0.1, 100.0), 0.0); // mid fall
        assert_eq!(w.value(5.0, 0.1, 100.0), -1.0); // off
        // Second period.
        assert_eq!(w.value(7.0, 0.1, 100.0), 1.0);
    }

    #[test]
    fn pwl_segments() {
        let w = Wave::Pwl(vec![0.0, -7.0, 10e-9, -7.0, 11e-9, -3.0]);
        assert_eq!(w.value(-1.0, 0.0, 1.0), -7.0);
        assert_eq!(w.value(5e-9, 0.0, 1.0), -7.0);
        assert!((w.value(10.5e-9, 0.0, 1.0) - (-5.0)).abs() < 1e-9);
        assert_eq!(w.value(2e-8, 0.0, 1.0), -3.0); // past the end
    }

    #[test]
    fn sin_and_exp() {
        let w = Wave::Sin(vec![1.0, 2.0, 1.0]); // 1 Hz around 1 V
        assert_eq!(w.value(0.0, 0.0, 1.0), 1.0);
        assert!((w.value(0.25, 0.0, 1.0) - 3.0).abs() < 1e-9);
        let w = Wave::Exp(vec![0.0, 1.0, 0.1, 0.2, 0.5, 0.3]);
        assert_eq!(w.value(0.05, 0.0, 1.0), 0.0);
        let v = w.value(0.3, 0.0, 1.0);
        assert!((v - (1.0 - (-0.2f64 / 0.2).exp())).abs() < 1e-12);
    }
}
