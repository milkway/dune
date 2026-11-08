//! Device noise in the time domain (issue #20 item 3; Dune's own —
//! neither the 3f5 nor ngspice-47 has it, ngspice only the
//! independent `TRNOISE`/`TRRANDOM` sources).
//!
//! Under `.options devnoise` every generator the `.noise` analysis
//! knows becomes a stochastic current source in `.tran`, between the
//! same two nodes: thermal (`4kT·g`) and shot (`2q|I|`) as white
//! noise, flicker (`coef/f^EF`, e.g. `KF·|I|^AF/f`) as Kasdin's
//! `1/f^α` sequence (`trnoise::kasdin`, α = EF). The generators are
//! not described twice: each device's `noise_dens` — the code `.noise`
//! runs — is called with an all-zero adjoint and the recording sinks
//! of [`NoiseCtx`], which hand back `(n1, n2, density)` for every
//! thermal/shot generator and `(n1, n2, coef, EF)` for every flicker
//! one, in the device's own fixed order.
//!
//! **Discretization.** One grid `k·NT` for all generators (`NT` =
//! `.options devnt`, default TSTEP), a breakpoint on every grid point
//! (like `TRNOISE`), the current linear between grid samples. A white
//! generator of one-sided density `S` has samples `σ·gₖ` with
//! `σ² = S/(2·NT)` (its sampled sequence then has the one-sided
//! density `2σ²NT = S` up to `1/(2NT)`; the linear interpolation
//! shapes it by `sinc⁴(f·NT)`, flat below the grid rate). A flicker
//! generator `coef/f^α` scales a unit Kasdin sequence `yₖ − y₀` by
//! `√Q`, `Q = coef·(2π·NT)^α/(2·NT)` (Kasdin's one-sided density
//! `2·NT·Q/(2 sin πf·NT)^α ≈ coef/f^α`; for α = 1, `Q = π·coef`).
//! Sample 0 is 0, so the operating point and `t = 0` are noiseless.
//!
//! **Bias dependence (the approximation).** Densities follow the
//! circuit: they are re-evaluated at every grid point from the
//! accepted solution there, and sample `k+1` takes the amplitude of
//! grid point `k` — the current stays continuous, and the noise of an
//! interval is set by the bias at its start (a lag of one `NT`; a
//! shot generator of a current that moves by `ΔI` over one `NT` is off
//! by `ΔI/2I` in variance there). For a long-memory flicker generator
//! the modulated-stationary form `√Q(t)·y(t)` is itself the model.
//!
//! **Streams.** The counted PRNG of D-0033: generator `j` of device
//! `name` draws from `stream_key(seed, name, SUB_DEV_WHITE + j)` (or
//! `SUB_DEV_FLICKER + j`), sample `k` from `Stream::at(key, k)`. The
//! same seed gives the same bits; adding, removing or reordering other
//! devices or sources changes nothing; step rejection changes
//! nothing.

use std::cell::RefCell;

use crate::devices::Device;
use crate::error::Error;
use crate::matrix::{FlickerGen, NoiseCtx, NoiseGen, Var};
use crate::state::States;
use crate::trnoise::{GRID_TOL, MAX_ONEF_SAMPLES, Stream, kasdin, stream_key};

/// Stream ids of device generators under a device-name key (far from
/// the source streams 1–4 of `trnoise.rs`).
const SUB_DEV_WHITE: u64 = 0x100;
const SUB_DEV_FLICKER: u64 = 0x1_0000;

enum Kind {
    /// Thermal or shot: white samples.
    White,
    /// Power-law flicker of exponent `ef`, with its unit Kasdin
    /// sequence `yₖ − y₀` (built at the first grid point).
    Flicker { ef: f64, y: Vec<f64> },
}

struct Gen {
    n1: Var,
    n2: Var,
    kind: Kind,
    key: u64,
    /// Amplitude of sample `k` (from grid point `k − 1`) and of sample
    /// `k + 1` (from grid point `k`), `k` = [`DevNoise::k`].
    amp_prev: f64,
    amp_cur: f64,
}

impl Gen {
    fn unit(&self, k: u64) -> f64 {
        if k == 0 {
            return 0.0;
        }
        match &self.kind {
            Kind::White => Stream::at(self.key, k).gauss(),
            Kind::Flicker { y, .. } => y[(k as usize).min(y.len() - 1)],
        }
    }

    /// The amplitude for density `d` (white: A²/Hz; flicker: the
    /// coefficient at 1 Hz). A negative density (a negative
    /// resistor's `4kT·g`) is taken by magnitude.
    fn amp(&self, d: f64, nt: f64) -> f64 {
        let d = d.abs();
        if !d.is_finite() {
            return 0.0;
        }
        match &self.kind {
            Kind::White => (d / (2.0 * nt)).sqrt(),
            Kind::Flicker { ef, .. } => {
                (d * (2.0 * std::f64::consts::PI * nt).powf(*ef) / (2.0 * nt)).sqrt()
            }
        }
    }
}

/// The transient device-noise injector of one `.tran` run.
pub(crate) struct DevNoise {
    nt: f64,
    seed: u64,
    temp: f64,
    final_time: f64,
    /// Noisy devices (index, first generator, generator count);
    /// empty until the first grid point.
    devs: Vec<(usize, usize, usize)>,
    gens: Vec<Gen>,
    /// Grid interval of the last accepted point; `None` before t = 0.
    k: Option<u64>,
    zeros: Vec<f64>,
    notes: Vec<String>,
}

/// One device's generators: `(white, flicker)` records.
fn record(
    d: &dyn Device,
    zeros: &[f64],
    temp: f64,
    states: &States,
) -> (Vec<NoiseGen>, Vec<FlickerGen>) {
    let white = RefCell::new(Vec::new());
    let flicker = RefCell::new(Vec::new());
    let ctx = NoiseCtx {
        re: zeros,
        im: zeros,
        freq: 1.0,
        temp,
        states,
        sp_sink: Some(&white),
        flicker_sink: Some(&flicker),
    };
    d.noise_dens(&ctx);
    (white.into_inner(), flicker.into_inner())
}

impl DevNoise {
    pub(crate) fn new(nt: f64, seed: u64, temp: f64, final_time: f64, nvars: usize) -> DevNoise {
        DevNoise {
            nt,
            seed,
            temp,
            final_time,
            devs: Vec::new(),
            gens: Vec::new(),
            k: None,
            zeros: vec![0.0; nvars],
            notes: Vec::new(),
        }
    }

    /// The generators of every noisy device, from the states at
    /// t = 0 (the order a device records is fixed by its code, not
    /// by its bias).
    fn collect(
        &mut self,
        devices: &[Box<dyn Device>],
        names: &[String],
        states: &States,
    ) -> Result<(), Error> {
        let n_onef = (self.final_time / self.nt).floor() + 10.0;
        let mut unsupported: Vec<String> = Vec::new();
        for (i, d) in devices.iter().enumerate() {
            if d.noise_suffixes().is_none() {
                continue;
            }
            let (white, flicker) = record(d.as_ref(), &self.zeros, self.temp, states);
            let first = self.gens.len();
            let name = &names[i];
            for (j, &(n1, n2, _)) in white.iter().enumerate() {
                self.gens.push(Gen {
                    n1,
                    n2,
                    kind: Kind::White,
                    key: stream_key(self.seed, name, SUB_DEV_WHITE + j as u64),
                    amp_prev: 0.0,
                    amp_cur: 0.0,
                });
            }
            for (j, f) in flicker.iter().enumerate() {
                match *f {
                    FlickerGen::PowerLaw { n1, n2, ef, .. } => {
                        if n_onef > MAX_ONEF_SAMPLES {
                            return Err(Error::Build(format!(
                                "device `{name}`: transient flicker noise needs more than \
                                 {MAX_ONEF_SAMPLES} samples (TSTOP/DEVNT); refusing"
                            )));
                        }
                        let key = stream_key(self.seed, name, SUB_DEV_FLICKER + j as u64);
                        let y = kasdin(n_onef as usize, 1.0, ef, key);
                        let y0 = y[0];
                        self.gens.push(Gen {
                            n1,
                            n2,
                            kind: Kind::Flicker {
                                ef,
                                y: y.iter().map(|v| v - y0).collect(),
                            },
                            key,
                            amp_prev: 0.0,
                            amp_cur: 0.0,
                        });
                    }
                    FlickerGen::Unsupported(what) => unsupported.push(format!("`{name}` ({what})")),
                }
            }
            self.devs.push((i, first, self.gens.len() - first));
        }
        let live = self.gens.iter().filter(|g| g.n1 != g.n2).count();
        self.notes.push(format!(
            "device noise: {live} generator(s) on {} device(s), devnt {:e} s, seed {} \
             (stream version {})",
            self.devs.len(),
            self.nt,
            self.seed,
            crate::trnoise::STREAM_VERSION
        ));
        if !unsupported.is_empty() {
            self.notes.push(format!(
                "device noise: no transient form for {}; left out",
                unsupported.join(", ")
            ));
        }
        Ok(())
    }

    /// The densities at the accepted point, as the amplitudes of the
    /// next sample.
    fn refresh(&mut self, devices: &[Box<dyn Device>], states: &States) {
        for &(i, first, count) in &self.devs {
            let (white, flicker) = record(devices[i].as_ref(), &self.zeros, self.temp, states);
            let dens = white
                .iter()
                .map(|&(_, _, d)| d)
                .chain(flicker.iter().filter_map(|f| match *f {
                    FlickerGen::PowerLaw { coef, .. } => Some(coef),
                    FlickerGen::Unsupported(_) => None,
                }));
            let mut n = 0;
            for (g, d) in self.gens[first..first + count].iter_mut().zip(dens) {
                g.amp_cur = g.amp(d, self.nt);
                n += 1;
            }
            debug_assert_eq!(n, count, "device {i} changed its generator count");
        }
    }

    /// `CKTaccept` for the injector: on a grid point, the next
    /// sample's amplitude comes from this point's bias, and the next
    /// grid point becomes a breakpoint (pushed into `requests`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn accept(
        &mut self,
        time: f64,
        min_break: f64,
        devices: &[Box<dyn Device>],
        names: &[String],
        states: &States,
        requests: &mut Vec<(f64, bool)>,
    ) -> Result<(), Error> {
        let x = time / self.nt;
        let kk = (x + GRID_TOL).floor().max(0.0) as u64;
        match self.k {
            None => {
                self.collect(devices, names, states)?;
                self.refresh(devices, states);
                for g in &mut self.gens {
                    g.amp_prev = g.amp_cur;
                }
                self.k = Some(kk);
            }
            Some(k) if kk > k => {
                // Landed on (or, defensively, passed) a grid point.
                for g in &mut self.gens {
                    g.amp_prev = g.amp_cur;
                }
                self.refresh(devices, states);
                self.k = Some(kk);
            }
            Some(_) => {}
        }
        let kb = ((time + min_break) / self.nt + GRID_TOL).floor();
        let next = (kb + 1.0) * self.nt;
        if next < self.final_time {
            requests.push((next, false));
        }
        Ok(())
    }

    /// Add the generator currents at `time` into the right-hand side
    /// (a current `i` from `n1` to `n2` through the generator).
    pub(crate) fn stamp(&self, time: f64, rhs: &mut [f64]) {
        let Some(k) = self.k else {
            return;
        };
        let frac = (time / self.nt - k as f64).clamp(0.0, 1.0);
        for g in &self.gens {
            if g.n1 == g.n2 || (g.amp_prev == 0.0 && g.amp_cur == 0.0) {
                continue;
            }
            let x0 = g.amp_prev * g.unit(k);
            let x1 = g.amp_cur * g.unit(k + 1);
            let i = x0 + (x1 - x0) * frac;
            rhs[g.n1] -= i;
            rhs[g.n2] += i;
        }
    }

    pub(crate) fn notes(&self) -> &[String] {
        &self.notes
    }
}
