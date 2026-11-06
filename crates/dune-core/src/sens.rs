//! `.sens` — DC and AC small-signal sensitivity (`ckt/cktsens.c`,
//! `ckt/cktsgen.c`).
//!
//! This is the perturbation method of the 3f5's `sens_sens` (the
//! Meikle adjoint `SENS2` was never compiled into the 3f5 front end;
//! ngspice-47 ships the same `cktsens.c`). After the operating
//! point — and, for AC, the small-signal solve at each frequency —
//! every parameter `p` of every device is visited in turn
//! (`sgen_next`): the device alone is loaded into an empty matrix
//! and RHS, `p` is moved by `δ = 1e-6·p` (`1e-6` when `p == 0`), the
//! device is loaded again, `p` is restored, and with `ΔY`, `ΔI` the
//! difference of the two loads one more solve against the factored
//! system gives
//!
//! `∂x/∂p ≈ Y⁻¹·(ΔI − ΔY·E) / δ`,
//!
//! of which the output (`V(N1,N2)` or a branch current) is kept.
//! Vector names follow `cktsens.c`: `<inst>:<kw>` for a model
//! parameter, `<inst>` for the principal instance parameter,
//! `<inst>_<kw>` for the others; the parameter set is ngspice-47's
//! (which lists zero-valued parameters the 3f5 skipped), restricted
//! to what each Dune device models (see `Device::sens_params`).
//! ngspice's optional filters (glob patterns on those names) select
//! a subset.

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::ac::AcSweep;
use crate::circuit::Circuit;
use crate::engine::Engine;
use crate::error::Error;
use crate::matrix::{AnKind, InitMode};
use crate::options::Options;
use crate::solution::Solution;
use crate::tf::{OutputVar, resolve_output};

/// `Sens_Delta`, `Sens_Abs_Delta` (`cktsens.c`).
const SENS_DELTA: f64 = 1e-6;
const SENS_ABS_DELTA: f64 = 1e-6;

/// The AC form of `.sens`: `AC DEC|OCT|LIN N FSTART FSTOP`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SensAc {
    pub sweep: AcSweep,
    /// Points per decade/octave, or the number of points of a `LIN`
    /// sweep (`count_steps`).
    pub points: usize,
    pub fstart: f64,
    pub fstop: f64,
}

/// Result of a `.sens` analysis: one real point (DC) or one complex
/// point per frequency (AC), one vector per perturbed parameter.
#[derive(Clone, Debug)]
pub struct Sens {
    /// The `Sensitivity Analysis` plot: `v(<name>)` vectors (real for
    /// DC; complex after a `frequency` scale for AC).
    pub solution: Solution,
    /// Setup remarks (diode temperature-adjust notes, …).
    pub notes: Vec<String>,
}

/// `scan` (ngspice `cktsens.c`): glob match with `*` and `?`.
///
/// Iterative two-pointer form, linear in `filter.len() + name.len()`:
/// on a mismatch it retries from the last `*`, one name byte later.
/// The C recurses per `*`, which a hostile card (`a*a*a*a*…` against
/// a long name) turns exponential — a deck must never hang (CLAUDE.md
/// princípio 4). Same accepted language.
fn glob(filter: &[u8], name: &[u8]) -> bool {
    let (mut f, mut n) = (0, 0);
    let mut star: Option<(usize, usize)> = None; // (filter pos after '*', name pos it matched)
    while n < name.len() {
        if f < filter.len() && (filter[f] == name[n] || filter[f] == b'?') {
            f += 1;
            n += 1;
        } else if f < filter.len() && filter[f] == b'*' {
            star = Some((f + 1, n));
            f += 1;
        } else if let Some((sf, sn)) = star {
            f = sf;
            n = sn + 1;
            star = Some((sf, sn + 1));
        } else {
            return false;
        }
    }
    while f < filter.len() && filter[f] == b'*' {
        f += 1;
    }
    f == filter.len()
}

/// Position of a device type in ngspice's `DEVices[]` (`dev.c`),
/// which fixes the order `sgen_next` walks the types in.
fn dev_order(name: &str) -> usize {
    match name.as_bytes().first().map(u8::to_ascii_lowercase) {
        Some(b'c') => 0,
        Some(b'f') => 1,
        Some(b'h') => 2,
        Some(b'd') => 3,
        Some(b'l') => 4,
        Some(b'k') => 5,
        Some(b'i') => 6,
        Some(b'r') => 7,
        Some(b'g') => 8,
        Some(b'e') => 9,
        Some(b'v') => 10,
        _ => 11,
    }
}

/// `count_steps` (`cktsens.c`): number of frequencies and the step.
/// The octave count divides `ln(high/low)` by `M_LOG2E` (so it
/// multiplies by `ln 2` instead of dividing) — kept, as in the C.
fn count_steps(ac: &SensAc) -> (usize, f64) {
    let steps = ac.points.max(1) as f64;
    let (mut low, mut high) = (ac.fstart, ac.fstop);
    let (n, s) = match ac.sweep {
        AcSweep::Linear => (steps, (high - low) / steps),
        AcSweep::Decade => {
            if low <= 0.0 {
                low = 1e-3;
            }
            if high <= low {
                high = 10.0 * low;
            }
            (
                (steps * (high / low).log10() + 1.01).trunc(),
                10f64.powf(1.0 / steps),
            )
        }
        AcSweep::Octave => {
            if low <= 0.0 {
                low = 1e-3;
            }
            if high <= low {
                high = 2.0 * low;
            }
            (
                (steps * (high / low).ln() / std::f64::consts::LOG2_E + 1.01).trunc(),
                2f64.powf(1.0 / steps),
            )
        }
    };
    let n = if n.is_finite() && n >= 1.0 {
        n.min(1e6) as usize
    } else {
        1
    };
    (n, s)
}

impl Circuit {
    /// Run `.sens OUTVAR [FILTER…] [AC DEC|OCT|LIN N FSTART FSTOP]`
    /// (`filters` empty: every parameter).
    pub fn sens(
        &self,
        opts: &Options,
        output: &OutputVar,
        filters: &[String],
        ac: Option<SensAc>,
    ) -> Result<Sens, Error> {
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        engine.set_kind(AnKind::DcOp, false);
        engine.op(opts)?;
        let out = resolve_output(&engine, output, ".sens")?;

        // The parameter walk, in `sgen_next` order: device types in
        // `DEVices[]` order, instances last-to-first within a type
        // (ngspice prepends each new instance to its model's list).
        let params = engine.sens_params(ac.is_some())?;
        let mut devs: Vec<usize> = (0..params.len()).collect();
        devs.sort_by_key(|&d| (dev_order(&engine.names[d]), std::cmp::Reverse(d)));
        let filters: Vec<String> = filters.iter().map(|f| f.to_ascii_lowercase()).collect();
        let mut walk: Vec<(usize, &'static str, f64, String)> = Vec::new();
        for &d in &devs {
            for p in &params[d] {
                let name = format!("{}{}", engine.names[d], p.suffix);
                if filters.is_empty() || filters.iter().any(|f| glob(f.as_bytes(), name.as_bytes()))
                {
                    walk.push((d, p.suffix, p.value, name));
                }
            }
        }
        if walk.is_empty() {
            return Err(Error::Build(
                ".sens: no parameter matches the filters".into(),
            ));
        }
        let delta_of = |value: f64| {
            if value != 0.0 {
                value * SENS_DELTA
            } else {
                SENS_ABS_DELTA
            }
        };

        let mut variables: Vec<(String, VarType)> = Vec::new();
        let solution = match ac {
            None => {
                let mut cols = Vec::with_capacity(walk.len());
                for (d, suffix, value, name) in &walk {
                    let delta = delta_of(*value);
                    let w = engine.sens_dc_column(opts, *d, suffix, *value, delta)?;
                    cols.push(vec![out.read(&w)]);
                    variables.push((format!("v({name})"), VarType::Voltage));
                }
                Solution {
                    plotname: "Sensitivity Analysis".to_string(),
                    variables,
                    data: Data::Real(cols),
                    scale: None,
                }
            }
            Some(ac) => {
                engine.load_all_public(InitMode::Smsig, opts);
                let (nfreqs, step) = count_steps(&ac);
                if nfreqs.saturating_mul(walk.len() + 1) > 50_000_000 {
                    return Err(Error::Build(
                        ".sens produces too many points; refusing".into(),
                    ));
                }
                variables.push(("frequency".to_string(), VarType::Frequency));
                let mut cols: Vec<Vec<Complex64>> = vec![Vec::new(); walk.len() + 1];
                let mut freq = ac.fstart;
                for _ in 0..nfreqs {
                    let omega = 2.0 * std::f64::consts::PI * freq;
                    let (er, ei) = engine.sens_ac_solution(omega)?;
                    cols[0].push(Complex64::new(freq, 0.0));
                    for (k, (d, suffix, value, _)) in walk.iter().enumerate() {
                        let delta = delta_of(*value);
                        let (wr, wi) =
                            engine.sens_ac_column(omega, *d, suffix, *value, delta, (&er, &ei))?;
                        cols[k + 1].push(Complex64::new(out.read(&wr), out.read(&wi)));
                    }
                    // `inc_freq` compares the step type against
                    // `LINEAR` (3) instead of `SENS_LINEAR` (15), so
                    // every sweep — linear ones included — multiplies.
                    freq *= step;
                }
                for (_, _, _, name) in &walk {
                    variables.push((format!("v({name})"), VarType::Voltage));
                }
                Solution {
                    plotname: "Sensitivity Analysis".to_string(),
                    variables,
                    data: Data::Complex(cols),
                    scale: Some(0),
                }
            }
        };
        Ok(Sens { solution, notes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_like_ngspice_scan() {
        assert!(glob(b"r*", b"r1_m"));
        assert!(glob(b"*", b"anything"));
        assert!(glob(b"d1:is", b"d1:is"));
        assert!(!glob(b"d1:is", b"d1:isr"));
        assert!(glob(b"?1", b"r1"));
        assert!(glob(b"r*_m", b"r12_m"));
        assert!(!glob(b"r*_m", b"r12_scale"));
        assert!(!glob(b"r1", b"r1_m"));
        assert!(glob(b"a*b*c", b"axxbyyc"));
        assert!(glob(b"**", b"x"));
        assert!(glob(b"a**b", b"ab"));
        // Hostile pattern: the recursive form is exponential here;
        // the iterative one must answer instantly.
        let name = [b'a'; 4096];
        let mut hostile = Vec::new();
        for _ in 0..64 {
            hostile.extend_from_slice(b"a*");
        }
        hostile.push(b'b');
        assert!(!glob(&hostile, &name));
        hostile.pop();
        hostile.push(b'a');
        assert!(glob(&hostile, &name));
    }

    #[test]
    fn count_steps_matches_the_c() {
        let dec = SensAc {
            sweep: AcSweep::Decade,
            points: 2,
            fstart: 100.0,
            fstop: 10e3,
        };
        let (n, s) = count_steps(&dec);
        assert_eq!(n, 5);
        assert!((s - 10f64.sqrt()).abs() < 1e-15);
        let lin = SensAc {
            sweep: AcSweep::Linear,
            points: 2,
            fstart: 100.0,
            fstop: 200.0,
        };
        assert_eq!(count_steps(&lin), (2, 50.0));
        let oct = SensAc {
            sweep: AcSweep::Octave,
            points: 1,
            fstart: 1.0,
            fstop: 8.0,
        };
        // ln(8)·ln(2) + 1.01 → 2, not log2(8) + 1 = 4.
        assert_eq!(count_steps(&oct).0, 2);
    }
}
