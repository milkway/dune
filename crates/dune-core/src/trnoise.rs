//! Transient noise sources: ngspice-47's `TRNOISE` and `TRRANDOM`
//! (`dev/vsrc/vsrcload.c`, `vsrcacct.c`, `vsrcpar.c`,
//! `frontend/trannoise/1-f-code.c`; the I source mirrors them). The
//! 3f5 has neither, so ngspice's grammar and semantics are the
//! contract; the random streams are Dune's own (docs/dune-core.md
//! §Ruído transiente, corpus/KNOWN_DIFFS.md).
//!
//! **What ngspice does, and Dune with it:**
//!
//! - `TRNOISE(NA NT NALPHA NAMP RTSAM RTSCAPT RTSEMT)`: a sample
//!   sequence `x₀ = 0, x₁, x₂, …` on the grid `k·NT`, the source
//!   value linear between neighbouring samples (`V1 + (V2−V1)·frac`
//!   in `VSRCload` — not a staircase). `xₖ = NA·gₖ + (yₖ − y₀)` with
//!   `gₖ` standard gaussian and `y` the `1/f^NALPHA` sequence of
//!   Kasdin (Proc. IEEE 83(5), 1995): white noise of rms `NAMP`
//!   filtered by `hₖ = hₖ₋₁·(α/2 + k − 1)/k`, generated once for
//!   `⌊TSTOP/NT⌋ + 10` samples. A breakpoint sits on every grid point
//!   (`VSRCaccept`), so the integrator always steps onto the corners;
//!   TMAX is not touched (ngspice does not cap it either). `NT = 0`
//!   means no white/1-f noise. Random telegraph noise adds `RTSAM`
//!   while a trap is filled: capture after an exponential time of
//!   mean `RTSCAPT`, emission after one of mean `RTSEMT`, each edge a
//!   breakpoint; it is active only for `RTSAM > 0`.
//! - `TRRANDOM(TYPE TS TD PARAM1 PARAM2)`: `PARAM2` up to `TD`, then a
//!   fresh draw held on every interval `(TD + (k−1)·TS, TD + k·TS]`,
//!   breakpoints on the interval ends.
//! - Both add the source's `DC` value when one is given, and are 0 at
//!   t = 0 (TRRANDOM: `PARAM2`), so the operating point is noiseless.
//!
//! **What Dune does differently** (all in corpus/KNOWN_DIFFS.md):
//!
//! - The generator. ngspice draws every source from ONE global
//!   combined Tausworthe/LCG stream seeded through `rand()`, so a
//!   source's realization depends on the platform's libc, on the
//!   other sources and on the order the simulator happens to ask for
//!   values. Dune's generator is counter-based: every random number
//!   is a pure function of `(seed, source name, stream, index)`
//!   ([`Stream::at`]), the same bits on every platform (integer
//!   arithmetic plus the pure-Rust `libm` — no platform libm), and a
//!   source's noise does not change when other sources are added,
//!   removed or reordered, nor when the step control rejects and
//!   retries a timepoint.
//! - The 1/f filter is applied as a LINEAR convolution (zero-padded
//!   FFT of twice the length, Kasdin's prescription) where ngspice's
//!   FFT product is circular, which wraps the tail of the long-memory
//!   filter onto the first samples.
//! - Invalid arguments are errors where ngspice runs on garbage
//!   (`TRRANDOM` type outside 1–4 is 0 there, `TS ≤ 0` loops on one
//!   breakpoint, RTS with a zero time never leaves `t`).

use num_complex::Complex64;

use crate::circuit::{Circuit, Spec};
use crate::engine::MAX_OUTPUT_VALUES;
use crate::error::Error;
use crate::matrix::AcceptCtx;
use crate::options::{Options, TRAN_STEPS_PER_TMAX};
use crate::srcfn::Wave;

/// Version of the noise streams. Bumped whenever a change alters the
/// bits a given `(seed, source, deck)` produces; the pinned values in
/// this module's tests and the `corpus/regress/golden` probes are the
/// version-1 streams.
pub(crate) const STREAM_VERSION: u32 = 1;

/// SplitMix64's Weyl increment (Steele, Lea & Flood, "Fast splittable
/// pseudorandom number generators", OOPSLA 2014; ⌊2⁶⁴/φ⌋, odd).
const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// SplitMix64's output function (the `mix64variant13` constants of
/// Steele et al. 2014 as used by Vigna's `splitmix64.c`), a bijection
/// of `u64` with full avalanche.
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a 64 of the ASCII-lowercased source name (spice names are
/// case-insensitive): offset basis and prime of the FNV spec.
fn fnv1a(name: &str) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b.to_ascii_lowercase());
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// The key of stream `sub` of source `name` under `seed`.
pub(crate) fn stream_key(seed: u64, name: &str, sub: u64) -> u64 {
    let k = mix64(seed ^ GAMMA);
    let k = mix64(k ^ fnv1a(name));
    mix64(k.wrapping_add(sub.wrapping_mul(GAMMA)))
}

/// Stream ids under a source key.
const SUB_WHITE: u64 = 1;
const SUB_ONEF: u64 = 2;
const SUB_RTS: u64 = 3;
const SUB_RANDOM: u64 = 4;

/// A SplitMix64 stream: state advanced by [`GAMMA`], output
/// [`mix64`]. Passes BigCrush (Steele et al. 2014), 64-bit state,
/// trivially portable.
#[derive(Clone, Debug)]
pub(crate) struct Stream(u64);

impl Stream {
    /// The stream of item `index` under `key`: seeded by the
    /// `index`-th SplitMix64 output of `key`, so item `index` never
    /// depends on how many numbers other items consumed.
    pub(crate) fn at(key: u64, index: u64) -> Stream {
        Stream(mix64(
            key.wrapping_add(index.wrapping_add(1).wrapping_mul(GAMMA)),
        ))
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(GAMMA);
        mix64(self.0)
    }

    /// Uniform on `[0, 1)`, 53 bits.
    fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Uniform on `(0, 1]`, 53 bits (a safe `ln` argument).
    fn uniform_pos(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// Standard gaussian, Marsaglia's polar method (one of the pair is
    /// used, so each item costs a fixed-distribution number of draws).
    pub(crate) fn gauss(&mut self) -> f64 {
        loop {
            let u = 2.0 * self.uniform() - 1.0;
            let v = 2.0 * self.uniform() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * libm::sqrt(-2.0 * libm::log(s) / s);
            }
        }
    }

    /// Exponential of mean `mean` (`exprand`, `randnumb.c`).
    fn exprand(&mut self, mean: f64) -> f64 {
        -libm::log(self.uniform_pos()) * mean
    }

    /// Poisson of mean `lambda`: the number of unit-rate arrivals in
    /// `[0, lambda]`. ngspice's `poisson` inverts the CDF from
    /// `exp(−λ)` and stops at 1000; Dune keeps the cap and 0 for
    /// `λ ≤ 0`.
    fn poisson(&mut self, lambda: f64) -> u32 {
        if lambda.is_nan() || lambda <= 0.0 {
            return 0;
        }
        let mut k = 0;
        let mut t = 0.0;
        while k < 1000 {
            t += -libm::log(self.uniform_pos());
            if t > lambda {
                break;
            }
            k += 1;
        }
        k
    }
}

/// The realization of a source's noise function (built with the
/// source, evaluated by its load and accept hooks).
#[derive(Debug)]
pub(crate) enum NoiseWave {
    TrNoise(TrNoise),
    TrRandom(TrRandom),
}

/// `TRNOISE` parameters and caches.
#[derive(Debug)]
pub(crate) struct TrNoise {
    na: f64,
    ts: f64,
    alpha: f64,
    namp: f64,
    rts_am: f64,
    rts_capt: f64,
    rts_emt: f64,
    white: u64,
    onef_key: u64,
    /// `(final_time, yₖ − y₀)`: the 1/f sequence for one run length.
    onef: Option<(f64, Vec<f64>)>,
    /// Random-telegraph edges `cap₀ em₀ cap₁ em₁ …`, grown on demand
    /// from `rts_stream`.
    rts_edges: Vec<f64>,
    rts_stream: Stream,
}

/// `TRRANDOM` parameters.
#[derive(Debug)]
pub(crate) struct TrRandom {
    kind: u32,
    ts: f64,
    td: f64,
    p1: f64,
    p2: f64,
    key: u64,
}

/// Most 1/f samples one `TRNOISE` may need (`TSTOP/NT`): the FFT
/// then holds 2²⁴ complex values (256 MiB). ngspice has no limit.
pub(crate) const MAX_ONEF_SAMPLES: f64 = 8_388_608.0;

/// Relative slack on grid-index rounding: a timepoint within this
/// fraction of a step of a grid point counts as on it.
pub(crate) const GRID_TOL: f64 = 1e-9;

impl NoiseWave {
    /// The realization of `wave` for source `name` under
    /// `opts.seed`; `None` for a deterministic wave. Errors on
    /// arguments ngspice would run on garbage with. Under
    /// `opts.notrnoise` a `TRNOISE` keeps its validation and then
    /// loses every noise parameter (`trnoise_state_gen`: `NA = TS =
    /// NALPHA = NAMP = RTSAM = RTSCAPT = RTSEMT = 0`).
    pub(crate) fn from_wave(
        wave: Option<&Wave>,
        opts: &Options,
        name: &str,
    ) -> Result<Option<NoiseWave>, Error> {
        let seed = opts.seed;
        let bad = |m: String| Err(Error::Build(format!("source `{name}`: {m}")));
        match wave {
            Some(Wave::TrNoise(c)) => {
                if c.len() < 2 || c.len() > 7 {
                    return bad(format!("TRNOISE takes 2 to 7 arguments, got {}", c.len()));
                }
                if c.iter().any(|v| !v.is_finite()) {
                    return bad("TRNOISE arguments must be finite".into());
                }
                // `VSRCparam`: NAMP only with NALPHA ≠ 0, the RTS
                // times only with RTSAM ≠ 0.
                let get = |i: usize| c.get(i).copied().unwrap_or(0.0);
                let (na, ts, alpha) = (get(0), get(1), get(2));
                let namp = if alpha != 0.0 { get(3) } else { 0.0 };
                let rts_am = get(4);
                let (capt, emt) = if rts_am != 0.0 {
                    (get(5), get(6))
                } else {
                    (0.0, 0.0)
                };
                if ts < 0.0 {
                    return bad("TRNOISE time step NT must not be negative".into());
                }
                if rts_am > 0.0 && !(capt > 0.0 && emt > 0.0) {
                    return bad(
                        "TRNOISE random telegraph noise needs RTSCAPT > 0 and RTSEMT > 0".into(),
                    );
                }
                let (na, ts, alpha, namp, rts_am, capt, emt) = if opts.notrnoise {
                    (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
                } else {
                    (na, ts, alpha, namp, rts_am, capt, emt)
                };
                Ok(Some(NoiseWave::TrNoise(TrNoise {
                    na,
                    ts,
                    alpha,
                    namp,
                    rts_am,
                    rts_capt: capt,
                    rts_emt: emt,
                    white: stream_key(seed, name, SUB_WHITE),
                    onef_key: stream_key(seed, name, SUB_ONEF),
                    onef: None,
                    rts_edges: Vec::new(),
                    rts_stream: Stream::at(stream_key(seed, name, SUB_RTS), 0),
                })))
            }
            Some(Wave::TrRandom(c)) => {
                if c.len() < 2 || c.len() > 5 {
                    return bad(format!("TRRANDOM takes 2 to 5 arguments, got {}", c.len()));
                }
                if c.iter().any(|v| !v.is_finite()) {
                    return bad("TRRANDOM arguments must be finite".into());
                }
                // `(int)coeffs[0]`: truncation.
                let kind = c[0].trunc();
                if !(1.0..=4.0).contains(&kind) {
                    return bad(format!(
                        "TRRANDOM type must be 1 (uniform), 2 (gaussian), 3 (exponential) \
                         or 4 (poisson), got {}",
                        c[0]
                    ));
                }
                let ts = c[1];
                if ts.is_nan() || ts <= 0.0 {
                    return bad("TRRANDOM time step TS must be positive".into());
                }
                let td = c.get(2).copied().unwrap_or(0.0);
                if td < 0.0 {
                    return bad("TRRANDOM delay TD must not be negative".into());
                }
                Ok(Some(NoiseWave::TrRandom(TrRandom {
                    kind: kind as u32,
                    ts,
                    td,
                    p1: c.get(3).copied().unwrap_or(1.0),
                    p2: c.get(4).copied().unwrap_or(0.0),
                    key: stream_key(seed, name, SUB_RANDOM),
                })))
            }
            _ => Ok(None),
        }
    }

    /// The noise part of the source value at `time` (the `DC` value is
    /// the caller's).
    pub(crate) fn value(&mut self, time: f64, final_time: f64) -> f64 {
        match self {
            NoiseWave::TrNoise(n) => n.value(time, final_time),
            NoiseWave::TrRandom(r) => r.value(time),
        }
    }

    /// `VSRCaccept`: the next grid point / RTS edge as a breakpoint.
    pub(crate) fn accept(&mut self, ctx: &mut AcceptCtx) {
        match self {
            NoiseWave::TrNoise(n) => n.accept(ctx),
            NoiseWave::TrRandom(r) => r.accept(ctx),
        }
    }

    /// The densest spacing of the breakpoints this source plants (the
    /// grid step, or half the mean RTS cycle), for the step budget;
    /// `None` when it plants none.
    pub(crate) fn break_spacing(&self) -> Option<f64> {
        match self {
            NoiseWave::TrNoise(n) => {
                let grid = n.grid().then_some(n.ts);
                let rts = n.rts().then_some(0.5 * (n.rts_capt + n.rts_emt));
                match (grid, rts) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                }
            }
            NoiseWave::TrRandom(r) => Some(r.ts),
        }
    }

    /// Whether the realization is identically zero (a `TRNOISE` with
    /// no white, 1/f or RTS part — e.g. under `notrnoise`).
    pub(crate) fn is_silent(&self) -> bool {
        match self {
            NoiseWave::TrNoise(n) => !n.grid() && !n.rts(),
            NoiseWave::TrRandom(_) => false,
        }
    }

    /// The 1/f samples a run to `final_time` will generate (0 without
    /// 1/f noise).
    pub(crate) fn onef_samples(&self, final_time: f64) -> f64 {
        match self {
            NoiseWave::TrNoise(n) if n.has_onef() => (final_time / n.ts).floor() + 10.0,
            _ => 0.0,
        }
    }
}

impl TrNoise {
    fn has_onef(&self) -> bool {
        self.ts > 0.0 && self.alpha > 0.0 && self.namp > 0.0
    }

    /// Whether the white/1-f grid carries anything (ngspice plants its
    /// breakpoints for any `NT > 0`; a grid of zeros is skipped here).
    fn grid(&self) -> bool {
        self.ts > 0.0 && (self.na != 0.0 || self.has_onef())
    }

    fn rts(&self) -> bool {
        self.rts_am > 0.0
    }

    /// Sample `xₖ` (`trnoise_state_gen`: `x₀ = 0`, then
    /// `NA·g + oneof[k] − oneof[0]`).
    fn sample(&mut self, k: u64, final_time: f64) -> f64 {
        if k == 0 {
            return 0.0;
        }
        let mut v = 0.0;
        if self.na != 0.0 {
            v = self.na * Stream::at(self.white, k).gauss();
        }
        if self.has_onef() {
            v += self.onef_at(k, final_time);
        }
        v
    }

    fn onef_at(&mut self, k: u64, final_time: f64) -> f64 {
        if self.onef.as_ref().is_none_or(|(f, _)| *f != final_time) {
            let n = ((final_time / self.ts).floor() + 10.0).clamp(10.0, MAX_ONEF_SAMPLES) as usize;
            let y = kasdin(n, self.namp, self.alpha, self.onef_key);
            let y0 = y[0];
            self.onef = Some((final_time, y.iter().map(|v| v - y0).collect()));
        }
        let y = &self.onef.as_ref().expect("built above").1;
        // Past the table (a run longer than the one it was built
        // for cannot happen; clamp rather than panic).
        y[(k as usize).min(y.len() - 1)]
    }

    fn value(&mut self, time: f64, final_time: f64) -> f64 {
        let mut v = 0.0;
        if self.grid() && time > 0.0 {
            let x = time / self.ts;
            let n1 = x.floor();
            let k = n1 as u64;
            let v1 = self.sample(k, final_time);
            let v2 = self.sample(k + 1, final_time);
            v = v1 + (v2 - v1) * (x - n1);
        }
        if self.rts() && time > 0.0 && self.rts_high(time) {
            v += self.rts_am;
        }
        v
    }

    /// Grow the RTS edge list past `t`.
    fn rts_extend(&mut self, t: f64) {
        while self.rts_edges.last().is_none_or(|&e| e <= t) {
            let last = self.rts_edges.last().copied().unwrap_or(0.0);
            let mean = if self.rts_edges.len().is_multiple_of(2) {
                self.rts_capt
            } else {
                self.rts_emt
            };
            self.rts_edges.push(last + self.rts_stream.exprand(mean));
        }
    }

    /// Trap filled at `t`: `capᵢ ≤ t ≤ emᵢ` (`time >= RTScapTime` in
    /// `VSRCload`, the emission applied at the accept after `emᵢ`).
    fn rts_high(&mut self, t: f64) -> bool {
        self.rts_extend(t);
        let c = self.rts_edges.partition_point(|&e| e < t);
        c % 2 == 1 || self.rts_edges[c] == t
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        let t = ctx.time;
        let mb = ctx.min_break;
        if self.grid() {
            let k = ((t + mb) / self.ts + GRID_TOL).floor();
            let next = (k + 1.0) * self.ts;
            if next < ctx.final_time {
                ctx.set_break(next);
            }
        }
        if self.rts() {
            self.rts_extend(t + mb);
            let i = self.rts_edges.partition_point(|&e| e <= t + mb);
            let next = self.rts_edges[i];
            if next < ctx.final_time {
                ctx.set_break(next);
            }
        }
    }
}

impl TrRandom {
    /// The draw index of `t`: `None` up to `TD`, else `k ≥ 1` with
    /// `t ∈ (TD + (k−1)·TS, TD + k·TS]`.
    fn index(&self, t: f64) -> Option<u64> {
        if t <= self.td {
            return None;
        }
        let x = (t - self.td) / self.ts;
        Some((x - GRID_TOL).ceil().max(1.0) as u64)
    }

    fn draw(&self, k: u64) -> f64 {
        let mut s = Stream::at(self.key, k);
        let (p1, p2) = (self.p1, self.p2);
        match self.kind {
            // `drand()` ∈ [−1, 1).
            1 => p1 * (2.0 * s.uniform() - 1.0) + p2,
            2 => p1 * s.gauss() + p2,
            3 => s.exprand(p1) + p2,
            _ => f64::from(s.poisson(p1)) + p2,
        }
    }

    fn value(&self, time: f64) -> f64 {
        match self.index(time) {
            None => self.p2,
            Some(k) => self.draw(k),
        }
    }

    fn accept(&mut self, ctx: &mut AcceptCtx) {
        let t = ctx.time;
        let mb = ctx.min_break;
        let next = if t < self.td - mb {
            self.td
        } else {
            let k = ((t - self.td + mb) / self.ts + GRID_TOL).floor();
            self.td + (k + 1.0) * self.ts
        };
        if next > t && next < ctx.final_time {
            ctx.set_break(next);
        }
    }
}

/// Kasdin's `1/f^α` sequence of `n` samples: white noise of rms `q`
/// through the filter `h₀ = 1, hₖ = hₖ₋₁·(α/2 + k − 1)/k` (`f_alpha`
/// of `1-f-code.c`), as a linear convolution by zero-padded FFT.
pub(crate) fn kasdin(n: usize, q: f64, alpha: f64, key: u64) -> Vec<f64> {
    let m = (2 * n).next_power_of_two();
    let ha = alpha / 2.0;
    let mut hb = vec![Complex64::new(0.0, 0.0); m];
    let mut wb = vec![Complex64::new(0.0, 0.0); m];
    let mut s = Stream::at(key, 0);
    let mut h = 1.0;
    for i in 0..n {
        if i > 0 {
            h *= (ha + (i - 1) as f64) / i as f64;
        }
        hb[i].re = h;
        wb[i].re = q * s.gauss();
    }
    fft(&mut hb, false);
    fft(&mut wb, false);
    for (a, b) in hb.iter_mut().zip(&wb) {
        *a *= *b;
    }
    fft(&mut hb, true);
    hb.truncate(n);
    hb.into_iter().map(|c| c.re).collect()
}

/// In-place radix-2 FFT (`buf.len()` a power of two); the inverse is
/// scaled by `1/n`. Twiddles from the pure-Rust `libm`, so the result
/// is the same on every platform.
pub(crate) fn fft(buf: &mut [Complex64], inverse: bool) {
    let n = buf.len();
    debug_assert!(n.is_power_of_two());
    if n < 2 {
        return;
    }
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            buf.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let tw: Vec<Complex64> = (0..n / 2)
        .map(|j| {
            let a = 2.0 * std::f64::consts::PI * j as f64 / n as f64;
            Complex64::new(libm::cos(a), sign * libm::sin(a))
        })
        .collect();
    let mut len = 2;
    while len <= n {
        let stride = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let w = tw[k * stride];
                let a = buf[start + k];
                let b = buf[start + k + len / 2] * w;
                buf[start + k] = a + b;
                buf[start + k + len / 2] = a - b;
            }
        }
        len <<= 1;
    }
    if inverse {
        let s = 1.0 / n as f64;
        for v in buf.iter_mut() {
            *v *= s;
        }
    }
}

impl Circuit {
    /// The options a `.tran` with transient-noise sources runs under:
    /// `None` when there are none (the run is untouched). Every grid
    /// point is a breakpoint, and each breakpoint costs the step
    /// control about four steps (`DCtran` restarts at a tenth of the
    /// gap), so an automatic step budget sized by TMAX alone would
    /// stop a deck whose `NT` is far below TMAX; the budget becomes
    /// [`TRAN_STEPS_PER_TMAX`] per grid point when that is more. A
    /// run whose grid alone would exceed the output memory guard, or
    /// whose 1/f table exceeds [`MAX_ONEF_SAMPLES`], is refused. The
    /// note names the seed and stream version, so a printed result
    /// says how to reproduce it.
    pub(crate) fn noise_tran_options(
        &self,
        opts: &Options,
        tstep: f64,
        tstop: f64,
        tstart: f64,
        tmax: Option<f64>,
    ) -> Result<(Option<Options>, Option<String>), Error> {
        let mut spacing = f64::INFINITY;
        let mut sources = 0;
        let mut silenced = 0;
        for spec in &self.specs {
            let (name, wave) = match spec {
                Spec::Vsource(v) => (&v.name, &v.wave),
                Spec::Isource(i) => (&i.name, &i.wave),
                _ => continue,
            };
            let Some(n) = NoiseWave::from_wave(wave.as_ref(), opts, name)? else {
                continue;
            };
            if n.is_silent() {
                if opts.notrnoise && matches!(wave, Some(Wave::TrNoise(_))) {
                    silenced += 1;
                }
                continue;
            }
            sources += 1;
            if n.onef_samples(tstop) > MAX_ONEF_SAMPLES {
                return Err(Error::Build(format!(
                    "source `{name}`: TRNOISE 1/f noise needs more than {MAX_ONEF_SAMPLES} \
                     samples (TSTOP/NT); refusing"
                )));
            }
            if let Some(s) = n.break_spacing() {
                spacing = spacing.min(s);
            }
        }
        let silenced_note =
            (silenced > 0).then(|| format!("notrnoise: {silenced} TRNOISE source(s) silenced"));
        // Dune's device noise (`devnoise.rs`) plants a breakpoint on
        // every point of its own grid.
        if opts.dev_noise {
            let nt = opts.dev_noise_step.unwrap_or(tstep);
            if !(nt > 0.0 && nt.is_finite()) {
                return Err(Error::Build("devnt must be a positive time step".into()));
            }
            spacing = spacing.min(nt);
        }
        if sources == 0 && !opts.dev_noise {
            return Ok((None, silenced_note));
        }
        let mut note = format!(
            "transient noise: {sources} source(s), seed {} (stream version {STREAM_VERSION})",
            opts.seed
        );
        if let Some(s) = &silenced_note {
            note = format!("{note}; {s}");
        }
        let note = if sources == 0 {
            silenced_note
        } else {
            Some(note)
        };
        if !spacing.is_finite() {
            return Ok((None, note));
        }
        let breaks = (tstop / spacing).ceil();
        if !(breaks.is_finite() && 2.0 * breaks <= MAX_OUTPUT_VALUES as f64) {
            return Err(Error::Build(
                "transient noise grid (TSTOP/NT breakpoints) produces too many points; refusing"
                    .into(),
            ));
        }
        if opts.max_tran_steps.is_some() {
            return Ok((None, note));
        }
        // `Engine::tran`'s effective TMAX.
        let mut max_step = match tmax {
            Some(m) if m > 0.0 => m,
            _ => f64::min(tstep, (tstop - tstart) / 50.0),
        };
        if let Some(d) = opts.tmax_divisor {
            max_step /= d;
        }
        let auto = opts.tran_step_budget(tstop, max_step);
        let mut o = opts.clone();
        o.max_tran_steps = Some(auto.max(TRAN_STEPS_PER_TMAX.saturating_mul(breaks as usize)));
        Ok((Some(o), note))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(seed: u64) -> Options {
        Options {
            seed,
            ..Options::default()
        }
    }

    fn trnoise(args: &[f64], seed: u64, name: &str) -> NoiseWave {
        NoiseWave::from_wave(Some(&Wave::TrNoise(args.to_vec())), &seeded(seed), name)
            .unwrap()
            .unwrap()
    }

    fn trrandom(args: &[f64], seed: u64) -> TrRandom {
        match NoiseWave::from_wave(Some(&Wave::TrRandom(args.to_vec())), &seeded(seed), "v1") {
            Ok(Some(NoiseWave::TrRandom(r))) => r,
            other => panic!("{other:?}"),
        }
    }

    fn mean_var(x: &[f64]) -> (f64, f64) {
        let n = x.len() as f64;
        let m = x.iter().sum::<f64>() / n;
        let v = x.iter().map(|a| (a - m) * (a - m)).sum::<f64>() / (n - 1.0);
        (m, v)
    }

    /// The version-1 streams, pinned bit for bit: any change to the
    /// generator, the key derivation or the gaussian transform shows
    /// here (bump [`STREAM_VERSION`] and the goldens with it).
    #[test]
    fn streams_are_pinned() {
        assert_eq!(STREAM_VERSION, 1);
        // SplitMix64 from 0: Vigna's reference first output.
        let mut s = Stream(0);
        assert_eq!(s.next_u64(), 0xE220_A839_7B1D_CDAF);
        let key = stream_key(1, "vnoise", SUB_WHITE);
        let g: Vec<u64> = (1..=4)
            .map(|k| Stream::at(key, k).gauss().to_bits())
            .collect();
        assert_eq!(g, PINNED_GAUSS, "{g:#018x?}");
    }

    const PINNED_GAUSS: [u64; 4] = [
        0xbffb_3db5_266d_15be,
        0xbfe0_3ae8_cda0_4bbc,
        0xbffa_c941_5bff_467a,
        0xbfeb_24fd_9656_ea93,
    ];

    #[test]
    fn names_are_case_insensitive_and_streams_independent() {
        assert_eq!(stream_key(7, "VN1", 1), stream_key(7, "vn1", 1));
        assert_ne!(stream_key(7, "vn1", 1), stream_key(7, "vn2", 1));
        assert_ne!(stream_key(7, "vn1", 1), stream_key(8, "vn1", 1));
        assert_ne!(stream_key(7, "vn1", 1), stream_key(7, "vn1", 2));
    }

    /// 10⁶ white samples: mean and variance against `0, NA²` at 5σ of
    /// their sampling errors (`NA/√N` and `NA²·√(2/N)`).
    #[test]
    fn white_samples_have_variance_na2() {
        let na = 2.5e-3;
        let mut w = trnoise(&[na, 1e-9], 1, "vn");
        let n = 1_000_000u64;
        let NoiseWave::TrNoise(t) = &mut w else {
            unreachable!()
        };
        let x: Vec<f64> = (1..=n).map(|k| t.sample(k, 1.0)).collect();
        let (m, v) = mean_var(&x);
        let nf = n as f64;
        assert!(m.abs() < 5.0 * na / nf.sqrt(), "mean {m}");
        assert!(
            (v / (na * na) - 1.0).abs() < 5.0 * (2.0 / nf).sqrt(),
            "var {v}"
        );
        // Kurtosis of a gaussian is 3 (sampling σ ≈ √(24/N)).
        let k4 = x.iter().map(|a| (a - m).powi(4)).sum::<f64>() / nf / (v * v);
        assert!((k4 - 3.0).abs() < 5.0 * (24.0 / nf).sqrt(), "kurtosis {k4}");
        // Lag-1 correlation ≈ 0 (σ 1/√N).
        let r1 = x.windows(2).map(|p| (p[0] - m) * (p[1] - m)).sum::<f64>() / nf / v;
        assert!(r1.abs() < 5.0 / nf.sqrt(), "lag-1 {r1}");
    }

    #[test]
    fn trnoise_is_linear_between_samples_and_zero_at_t0() {
        let ts = 1e-6;
        let mut w = trnoise(&[1.0, ts], 3, "v");
        assert_eq!(w.value(0.0, 1e-3), 0.0);
        let (x3, x4) = match &mut w {
            NoiseWave::TrNoise(t) => (t.sample(3, 1e-3), t.sample(4, 1e-3)),
            NoiseWave::TrRandom(_) => unreachable!(),
        };
        let mid = w.value(3.25 * ts, 1e-3);
        assert!((mid - (x3 + 0.25 * (x4 - x3))).abs() < 1e-12);
        // Same seed, same name: same bits; another seed differs.
        let mut w2 = trnoise(&[1.0, ts], 3, "v");
        assert_eq!(w2.value(3.25 * ts, 1e-3).to_bits(), mid.to_bits());
        let mut w3 = trnoise(&[1.0, ts], 4, "v");
        assert_ne!(w3.value(3.25 * ts, 1e-3), mid);
    }

    /// Kasdin's 1/f^α: the log-log slope of the averaged periodogram
    /// over two decades well inside the band (f ∈ [10⁻³, 10⁻¹]
    /// cycles/sample, where the discrete law `(2 sin πf)^−α` is within
    /// 2% of `f^−α`) is −α to ±0.08 (16 realizations of 2¹⁶).
    #[test]
    fn one_over_f_slope() {
        for alpha in [0.5, 1.0, 1.5] {
            let n = 1 << 16;
            let nb = 24;
            let (f_lo, f_hi) = (1e-3_f64, 1e-1_f64);
            let mut acc = vec![(0.0, 0usize); nb];
            for r in 0..16u64 {
                let y = kasdin(n, 1.0, alpha, stream_key(r + 1, "vf", SUB_ONEF));
                // Hann window, mean removed.
                let m = y.iter().sum::<f64>() / n as f64;
                let mut b: Vec<Complex64> = y
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let w =
                            0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
                        Complex64::new((v - m) * w, 0.0)
                    })
                    .collect();
                fft(&mut b, false);
                for (i, c) in b.iter().enumerate().take(n / 2).skip(1) {
                    let f = i as f64 / n as f64;
                    if f < f_lo || f >= f_hi {
                        continue;
                    }
                    let bin = ((f / f_lo).ln() / (f_hi / f_lo).ln() * nb as f64) as usize;
                    acc[bin].0 += c.norm_sqr();
                    acc[bin].1 += 1;
                }
            }
            let pts: Vec<(f64, f64)> = acc
                .iter()
                .enumerate()
                .filter(|(_, a)| a.1 > 0)
                .map(|(i, a)| {
                    let f = f_lo * (f_hi / f_lo).powf((i as f64 + 0.5) / nb as f64);
                    (f.ln(), (a.0 / a.1 as f64).ln())
                })
                .collect();
            let np = pts.len() as f64;
            let mx = pts.iter().map(|p| p.0).sum::<f64>() / np;
            let my = pts.iter().map(|p| p.1).sum::<f64>() / np;
            let slope = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum::<f64>()
                / pts.iter().map(|p| (p.0 - mx).powi(2)).sum::<f64>();
            println!("alpha {alpha}: periodogram slope {slope:.4}");
            assert!((slope + alpha).abs() < 0.08, "alpha {alpha}: slope {slope}");
        }
    }

    /// The linear convolution equals the direct sum `Σ hⱼ wₖ₋ⱼ`.
    #[test]
    fn kasdin_is_a_linear_convolution() {
        let (n, alpha, q) = (50, 1.0, 0.7);
        let key = stream_key(5, "v", SUB_ONEF);
        let y = kasdin(n, q, alpha, key);
        let mut s = Stream::at(key, 0);
        let w: Vec<f64> = (0..n).map(|_| q * s.gauss()).collect();
        let mut h = vec![1.0; n];
        for i in 1..n {
            h[i] = h[i - 1] * (alpha / 2.0 + (i - 1) as f64) / i as f64;
        }
        for k in 0..n {
            let direct: f64 = (0..=k).map(|j| h[j] * w[k - j]).sum();
            assert!((y[k] - direct).abs() < 1e-12, "{k}: {} vs {direct}", y[k]);
        }
    }

    /// RTS: high a fraction `EMT/(CAPT+EMT)` of the time, mean dwell
    /// times `CAPT` (low) and `EMT` (high) — 2·10⁴ cycles, 5σ.
    #[test]
    fn rts_dwell_times() {
        let (capt, emt) = (2e-6, 5e-6);
        let w = trnoise(&[0.0, 0.0, 0.0, 0.0, 1e-3, capt, emt], 9, "vr");
        let NoiseWave::TrNoise(mut t) = w else {
            unreachable!()
        };
        let cycles = 20_000;
        let horizon = cycles as f64 * (capt + emt);
        t.rts_extend(horizon);
        let e = t.rts_edges.clone();
        let lows: Vec<f64> = (0..cycles)
            .map(|i| e[2 * i] - if i == 0 { 0.0 } else { e[2 * i - 1] })
            .collect();
        let highs: Vec<f64> = (0..cycles).map(|i| e[2 * i + 1] - e[2 * i]).collect();
        let (ml, _) = mean_var(&lows);
        let (mh, _) = mean_var(&highs);
        let tol = 5.0 / (cycles as f64).sqrt(); // exponential: σ = mean
        assert!((ml / capt - 1.0).abs() < tol, "low {ml}");
        assert!((mh / emt - 1.0).abs() < tol, "high {mh}");
        // The value follows the edges.
        let mid_high = 0.5 * (e[0] + e[1]);
        let mid_low = 0.5 * (e[1] + e[2]);
        assert_eq!(t.value(mid_high, horizon), 1e-3);
        assert_eq!(t.value(mid_low, horizon), 0.0);
        assert_eq!(t.value(e[0], horizon), 1e-3);
        assert_eq!(t.value(e[1], horizon), 1e-3);
    }

    /// TRRANDOM moments (10⁵ draws, 5σ): uniform `[−P1, P1) + P2`
    /// (variance P1²/3), gaussian, exponential (mean P1), poisson
    /// (mean = variance = λ).
    #[test]
    fn trrandom_moments() {
        let n = 100_000u64;
        let nf = n as f64;
        let check = |args: &[f64], mean: f64, var: f64| {
            let r = trrandom(args, 11);
            let x: Vec<f64> = (1..=n).map(|k| r.draw(k)).collect();
            let (m, v) = mean_var(&x);
            assert!(
                (m - mean).abs() < 5.0 * (var / nf).sqrt(),
                "{args:?}: mean {m}"
            );
            // Variance error: σ² ·√((κ−1)/N), κ ≤ 9 for these.
            assert!(
                (v / var - 1.0).abs() < 5.0 * (8.0 / nf).sqrt(),
                "{args:?}: var {v}"
            );
        };
        check(&[1.0, 1e-3, 0.0, 2.0, 0.5], 0.5, 4.0 / 3.0);
        check(&[2.0, 1e-3, 0.0, 0.3, 1.0], 1.0, 0.09);
        check(&[3.0, 1e-3, 0.0, 2.0, -1.0], 1.0, 4.0);
        check(&[4.0, 1e-3, 0.0, 3.5, 0.0], 3.5, 3.5);
        // Uniform draws stay in range.
        let r = trrandom(&[1.0, 1e-3, 0.0, 2.0, 0.5], 1);
        assert!((1..=10_000).all(|k| (-1.5..2.5).contains(&r.draw(k))));
    }

    #[test]
    fn trrandom_holds_on_intervals() {
        let r = trrandom(&[2.0, 1e-3, 5e-3, 1.0, 0.25], 2);
        assert_eq!(r.value(0.0), 0.25);
        assert_eq!(r.value(5e-3), 0.25);
        let a = r.value(5e-3 + 1e-9);
        assert_eq!(r.value(5.5e-3), a);
        assert_eq!(r.value(6e-3), a);
        assert_ne!(r.value(6e-3 + 1e-9), a);
        assert_eq!(r.index(6e-3 * (1.0 + 1e-13)), Some(1));
    }

    #[test]
    fn invalid_arguments_are_errors() {
        let e = |w: Wave| {
            NoiseWave::from_wave(Some(&w), &seeded(1), "v")
                .unwrap_err()
                .to_string()
        };
        assert!(e(Wave::TrNoise(vec![1.0])).contains("2 to 7"));
        assert!(e(Wave::TrNoise(vec![1.0, -1.0])).contains("negative"));
        assert!(e(Wave::TrNoise(vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0])).contains("RTSCAPT"));
        assert!(e(Wave::TrRandom(vec![5.0, 1e-3])).contains("type"));
        assert!(e(Wave::TrRandom(vec![1.0, 0.0])).contains("positive"));
        assert!(e(Wave::TrRandom(vec![1.0, 1.0, -1.0])).contains("delay"));
        assert!(
            NoiseWave::from_wave(Some(&Wave::Sin(vec![])), &seeded(1), "v")
                .unwrap()
                .is_none()
        );
    }
}
