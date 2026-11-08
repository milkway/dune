//! Device noise in the time domain (`.options devnoise`, issue #20
//! item 3): the generators `.noise` knows as stochastic current
//! sources in `.tran`. Neither the 3f5 nor ngspice-47 has this, so
//! the rulers are physics and statistics, at 5σ of the sampling
//! error (seeds fixed):
//!
//! - a lone resistor: the grid samples are white with the one-sided
//!   density `4kTR` (variance `4kTR·Δf` in any band `Δf`);
//! - an RC: the total variance of the capacitor voltage is `kT/C`
//!   (the exact first-order-hold value for the grid, → kT/C as
//!   NT/τ → 0);
//! - a diode fed by an ideal current source: shot noise `2qI`
//!   through `r_d²`, and a `KF·I/f` flicker generator's slope and
//!   level (Kasdin's discrete law);
//! - BJT and MOSFET amplifiers: the output periodogram in a band
//!   against the `.noise` analysis of the same deck;
//! - reproducibility: same seed ⇒ same bits; another seed ⇒ other
//!   noise; devices joining elsewhere change nothing.
//! - the `corpus/regress/devnoise_*.cir` probes against goldens
//!   written by Dune itself (regenerate with
//!   `DUNE_BLESS=1 cargo test -p dune-core --test devnoise`).
//!
//! Numbers are printed (`-- --nocapture`) and recorded in
//! docs/dune-core.md §Ruído de dispositivo no tempo.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use dune_core::consts::{BOLTZ, CHARGE};
use dune_core::{Circuit, Options, Solution};
use dune_raw::{Format, Rawfile};
use num_complex::Complex64;

const T: f64 = 300.15;

/// `DEVNOISE_LONG=k` multiplies the sample counts of the statistics
/// tests by `k` (the longer runs quoted in docs/dune-core.md).
fn long() -> usize {
    std::env::var("DEVNOISE_LONG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

fn corpus(rel: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus")).join(rel)
}

fn load(text: &str) -> (Circuit, Options, Vec<dune_netlist::ir::Analysis>) {
    let parsed = dune_netlist::parse_str("devnoise", text);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    assert!(d.is_empty(), "{d:?}");
    (c, opts, parsed.netlist.analyses.clone())
}

/// Run analysis `i` of the deck.
fn run(text: &str, i: usize, tweak: impl Fn(&mut Options)) -> (Vec<Solution>, Vec<String>) {
    let (c, mut opts, an) = load(text);
    tweak(&mut opts);
    c.run(&an[i], &opts).unwrap()
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name)
        .unwrap_or_else(|| panic!("no {name}: {:?}", s.variables))
}

/// The values of `v` at the grid points `k·nt`, k = 1… (every one is
/// a breakpoint, so the transient steps onto it).
fn at_grid(s: &Solution, v: &str, nt: f64) -> Vec<f64> {
    let t = col(s, "time");
    let y = col(s, v);
    let mut out = Vec::new();
    let mut k = 1.0;
    for (i, &ti) in t.iter().enumerate() {
        if (ti - k * nt).abs() <= 1e-9 * nt {
            out.push(y[i]);
            k += 1.0;
        }
    }
    let expected = (t[t.len() - 1] / nt + 1e-9).floor();
    assert!(
        out.len() as f64 >= expected - 1.0,
        "{v}: {} grid points of {expected}",
        out.len()
    );
    out
}

fn mean_var(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let m = x.iter().sum::<f64>() / n;
    let v = x.iter().map(|a| (a - m) * (a - m)).sum::<f64>() / (n - 1.0);
    (m, v)
}

fn fft(buf: &mut [Complex64]) {
    let n = buf.len();
    assert!(n.is_power_of_two());
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            buf.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let w = Complex64::from_polar(1.0, -2.0 * PI / len as f64);
        for start in (0..n).step_by(len) {
            let mut wk = Complex64::new(1.0, 0.0);
            for k in 0..len / 2 {
                let a = buf[start + k];
                let b = buf[start + k + len / 2] * wk;
                buf[start + k] = a + b;
                buf[start + k + len / 2] = a - b;
                wk *= w;
            }
        }
        len <<= 1;
    }
}

/// Welch periodogram, one-sided (units²/Hz): non-overlapping Hann
/// segments of `seg` samples at spacing `dt`, mean removed per
/// segment. Returns `(frequency, per-segment densities)` per bin.
fn welch(x: &[f64], seg: usize, dt: f64) -> (Vec<f64>, Vec<Vec<f64>>) {
    let w: Vec<f64> = (0..seg)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / seg as f64).cos())
        .collect();
    let s2: f64 = w.iter().map(|v| v * v).sum();
    let nseg = x.len() / seg;
    let freqs: Vec<f64> = (1..seg / 2).map(|i| i as f64 / (seg as f64 * dt)).collect();
    let mut per = vec![Vec::with_capacity(nseg); freqs.len()];
    for s in 0..nseg {
        let chunk = &x[s * seg..(s + 1) * seg];
        let m = chunk.iter().sum::<f64>() / seg as f64;
        let mut b: Vec<Complex64> = chunk
            .iter()
            .zip(&w)
            .map(|(v, wi)| Complex64::new((v - m) * wi, 0.0))
            .collect();
        fft(&mut b);
        for (i, p) in per.iter_mut().enumerate() {
            p.push(2.0 * b[i + 1].norm_sqr() * dt / s2);
        }
    }
    (freqs, per)
}

/// The band average of `ratio(f) · P(f)` over `[f_lo, f_hi)`, with its
/// sampling σ measured from the scatter of the per-segment band
/// means (no assumption about bin correlations).
fn band(
    freqs: &[f64],
    per: &[Vec<f64>],
    f_lo: f64,
    f_hi: f64,
    ratio: impl Fn(f64) -> f64,
) -> (f64, f64) {
    let nseg = per[0].len();
    let mut seg_means = vec![0.0; nseg];
    let mut nb = 0;
    for (i, &f) in freqs.iter().enumerate() {
        if f < f_lo || f >= f_hi {
            continue;
        }
        let r = ratio(f);
        for s in 0..nseg {
            seg_means[s] += r * per[i][s];
        }
        nb += 1;
    }
    assert!(nb > 0);
    for m in &mut seg_means {
        *m /= nb as f64;
    }
    let (m, v) = mean_var(&seg_means);
    (m, (v / nseg as f64).sqrt())
}

// ---------------------------------------------------------------------
// Thermal noise.

/// A lone 10 kΩ: its node voltage is `−i·R`, so the grid samples are
/// the generator's own — white, one-sided density `4kTR` (variance
/// `2kTR/NT` = `4kTR·Δf` over the grid bandwidth `Δf = 1/(2NT)`). The
/// periodogram over 90% of the band and the variance in a
/// measurement band `[0.1, 0.2]/NT` both at 5σ.
#[test]
fn resistor_thermal_density_is_4ktr() {
    let (r, nt) = (10e3, 1e-9);
    let n = 1 << 17;
    let deck = format!(
        "r\n.options devnoise seed=11\nr1 1 0 {r}\n.tran {nt} {}\n.end\n",
        n as f64 * nt
    );
    let (sols, notes) = run(&deck, 0, |_| {});
    assert!(
        notes
            .iter()
            .any(|s| s.contains("device noise: 1 generator(s)")),
        "{notes:?}"
    );
    let x = at_grid(&sols[0], "v(1)", nt);
    let s_th = 4.0 * BOLTZ * T * r;
    let (_, var) = mean_var(&x);
    let var_ref = s_th / (2.0 * nt);
    let sig_var = (2.0 / x.len() as f64).sqrt();
    let (freqs, per) = welch(&x, 1024, nt);
    let (p, sp) = band(&freqs, &per, 0.02 / nt, 0.47 / nt, |_| 1.0);
    // Variance in the band [0.1, 0.2]/NT: ∫P df over the bins.
    let df = 1.0 / (1024.0 * nt);
    let (p_band, sp_band) = band(&freqs, &per, 0.1 / nt, 0.2 / nt, |_| 1.0);
    let nbins = freqs
        .iter()
        .filter(|&&f| (0.1 / nt..0.2 / nt).contains(&f))
        .count() as f64;
    let var_band = p_band * nbins * df;
    let var_band_ref = s_th * nbins * df;
    println!(
        "R: var/(2kTR/NT) = {:.5} (σ {sig_var:.4}); P/4kTR = {:.5} (σ {:.4}); \
         band var/(4kTRΔf) = {:.5} (σ {:.4})",
        var / var_ref,
        p / s_th,
        sp / s_th,
        var_band / var_band_ref,
        sp_band / s_th
    );
    assert!((var / var_ref - 1.0).abs() < 5.0 * sig_var);
    assert!((p / s_th - 1.0).abs() < 5.0 * sp / s_th);
    assert!((var_band / var_band_ref - 1.0).abs() < 5.0 * sp_band / s_th);
}

/// The classic: a noisy R across C. The capacitor voltage's total
/// variance is `kT/C` whatever R is. On the grid the current is
/// linear between samples, so the exact discrete value is the
/// first-order hold `NA²·[b₁² + (a·b₁ + b₀)²/(1 − a²)]` with the
/// Norton→Thévenin `NA² = σᵢ²R² = 2kTR/NT` (`a = e^(−NT/τ)`,
/// `b₁ = 1 − τ(1−a)/NT`, `b₀ = 1 − a − b₁`), which tends to
/// `NA²·NT/(2τ) = kT/C`; at τ = 50·NT it is 0.993·kT/C. 4·10⁵ grid
/// samples: σ_rel = √(2(1+a²)/((1−a²)N)) = 1.6%; both the FOH value
/// and kT/C itself within 5σ.
#[test]
fn rc_total_noise_is_kt_over_c() {
    let (r, c) = (1e3, 1e-9);
    let tau = r * c;
    let nt = tau / 50.0;
    let n = 400_000.0 * long() as f64;
    let deck = format!(
        "ktc\n.options devnoise seed=3\nr1 1 0 {r}\nc1 1 0 {c}\n.tran {nt} {}\n.end\n",
        n * nt
    );
    let (sols, _) = run(&deck, 0, |_| {});
    let y = at_grid(&sols[0], "v(1)", nt);
    let y = &y[500..]; // 10τ of start-up
    let (_, v) = mean_var(y);
    let ktc = BOLTZ * T / c;
    let a = (-nt / tau).exp();
    let b1 = 1.0 - tau * (1.0 - a) / nt;
    let b0 = 1.0 - a - b1;
    let na2 = 2.0 * BOLTZ * T * r / nt;
    let foh = na2 * (b1 * b1 + (a * b1 + b0).powi(2) / (1.0 - a * a));
    let sigma = (2.0 * (1.0 + a * a) / ((1.0 - a * a) * y.len() as f64)).sqrt();
    println!(
        "kT/C: var = {v:.5e} V², kT/C = {ktc:.5e} (ratio {:.5}), FOH = {foh:.5e} \
         (ratio {:.5}, FOH/kTC {:.5}), σ_rel {sigma:.4}, rms {:.4} µV",
        v / ktc,
        v / foh,
        foh / ktc,
        v.sqrt() * 1e6
    );
    assert!((v / foh - 1.0).abs() < 5.0 * sigma);
    assert!((v / ktc - 1.0).abs() < 5.0 * sigma);
}

// ---------------------------------------------------------------------
// Shot and flicker noise.

/// The diode's small-signal resistance at the deck's bias, from the
/// op: `r_d = 1/(I_S/V_T·e^(V/V_T) + GMIN)`.
fn diode_rd(c: &Circuit, opts: &Options, is: f64) -> (f64, f64) {
    let op = c.op(opts).unwrap();
    let vd = col(&op.solution, "v(1)")[0];
    let vt = BOLTZ * T / CHARGE;
    let id = is * ((vd / vt).exp() - 1.0);
    (1.0 / (is / vt * (vd / vt).exp() + opts.gmin), id)
}

/// A diode fed 1 mA by an ideal current source, no capacitance: the
/// node voltage is the shot current `2qI` through `r_d` — white,
/// density `2qI·r_d²`. The periodogram over most of the band, the
/// `.noise` density of the same deck (its `onoise_spectrum`², √V²/Hz
/// in ngspice's default form) and the closed form all agree.
#[test]
fn diode_shot_noise_is_2qi() {
    let nt = 1e-9;
    let n = 1 << 17;
    let is = 1e-14;
    let deck = format!(
        "shot\n.options devnoise seed=5\ni1 0 1 dc 1m ac 1\nd1 1 0 dd\n\
         .model dd d is={is}\n.tran {nt} {}\n.noise v(1) i1 lin 3 1meg 3meg\n.end\n",
        n as f64 * nt
    );
    let (c, opts, _) = load(&deck);
    let (rd, id) = diode_rd(&c, &opts, is);
    let s_ref = 2.0 * CHARGE * id * rd * rd;
    let (nz, _) = run(&deck, 1, |_| {});
    let on = col(&nz[0], "onoise_spectrum");
    let s_noise = on[1] * on[1];
    let (sols, _) = run(&deck, 0, |_| {});
    let x = at_grid(&sols[0], "v(1)", nt);
    let (freqs, per) = welch(&x, 1024, nt);
    let (p, sp) = band(&freqs, &per, 0.02 / nt, 0.47 / nt, |_| 1.0);
    println!(
        "shot: I = {id:.6e} A, r_d = {rd:.4} Ω; P/(2qI·r_d²) = {:.5} (σ {:.4}); \
         .noise/(2qI·r_d²) = {:.6}",
        p / s_ref,
        sp / s_ref,
        s_noise / s_ref
    );
    assert!((s_noise / s_ref - 1.0).abs() < 1e-3);
    assert!((p / s_ref - 1.0).abs() < 5.0 * sp / s_ref);
}

/// The same diode with `KF = 1e-10, AF = 1`: the flicker generator
/// `KF·I/f` dominates the shot floor by > 10³ below 0.1/NT. Its
/// periodogram slope over [10⁻³, 10⁻¹]/NT is −1 (±0.08, the ruler of
/// the Kasdin unit test), and its level against Kasdin's discrete law
/// `S(f) = r_d²·[2qI + KF·I·π·2NT/(2 sin πf·NT)]` (the generator's
/// `Q = π·KF·I`) at 5σ.
#[test]
fn diode_flicker_slope_and_level() {
    let nt = 1e-6;
    let n = 1 << 18;
    let (is, kf) = (1e-14, 1e-10);
    let deck = format!(
        "flicker\n.options devnoise seed=8\ni1 0 1 dc 1m\nd1 1 0 dd\n\
         .model dd d is={is} kf={kf} af=1\n.tran {nt} {}\n.end\n",
        n as f64 * nt
    );
    let (c, opts, _) = load(&deck);
    let (rd, id) = diode_rd(&c, &opts, is);
    let (sols, _) = run(&deck, 0, |_| {});
    let x = at_grid(&sols[0], "v(1)", nt);
    let (freqs, per) = welch(&x, 1 << 14, nt);
    let law = |f: f64| {
        rd * rd * (2.0 * CHARGE * id + kf * id * PI * 2.0 * nt / (2.0 * (PI * f * nt).sin()))
    };
    let (lvl, slvl) = band(&freqs, &per, 1e-3 / nt, 1e-1 / nt, |f| 1.0 / law(f));
    // Slope: log-log fit of 24 log-spaced band averages.
    let (f_lo, f_hi, nb) = (1e-3 / nt, 1e-1 / nt, 24);
    let mut acc = vec![(0.0, 0usize); nb];
    for (i, &f) in freqs.iter().enumerate() {
        if f < f_lo || f >= f_hi {
            continue;
        }
        let b = ((f / f_lo).ln() / (f_hi / f_lo).ln() * nb as f64) as usize;
        acc[b].0 += per[i].iter().sum::<f64>() / per[i].len() as f64;
        acc[b].1 += 1;
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
    println!("flicker: slope {slope:.4}; level/law = {lvl:.5} (σ {slvl:.4})");
    assert!((slope + 1.0).abs() < 0.08, "slope {slope}");
    assert!((lvl - 1.0).abs() < 5.0 * slvl, "level {lvl}");
}

// ---------------------------------------------------------------------
// Whole amplifiers against `.noise`.

/// The output periodogram of an amplifier in a band well below the
/// grid rate, against the band average of `onoise_spectrum²` from the
/// `.noise` card of the same deck (linear sweep over the same band).
fn against_dot_noise(
    deck: &str,
    out: &str,
    nt: f64,
    f_lo: f64,
    f_hi: f64,
    seg: usize,
) -> (f64, f64) {
    let (nz, _) = run(deck, 1, |_| {});
    let fr = col(&nz[0], "frequency");
    let on = col(&nz[0], "onoise_spectrum");
    let (sols, _) = run(deck, 0, |_| {});
    let x = at_grid(&sols[0], out, nt);
    let x = &x[x.len() / 16..]; // start-up
    let (freqs, per) = welch(x, seg, nt);
    // Interpolate .noise's density (log-log) at each bin.
    let dens = |f: f64| {
        let i = fr.partition_point(|&g| g < f).clamp(1, fr.len() - 1);
        let (f0, f1) = (fr[i - 1], fr[i]);
        let (d0, d1) = (on[i - 1] * on[i - 1], on[i] * on[i]);
        let u = (f / f0).ln() / (f1 / f0).ln();
        (d0.ln() + u * (d1.ln() - d0.ln())).exp()
    };
    band(&freqs, &per, f_lo, f_hi, |f| 1.0 / dens(f))
}

/// A common-emitter stage (RB/RC/RE thermal, Ic/Ib shot, Ib flicker
/// with KF): the output noise in [10, 100] kHz at NT = 100 ns
/// against `.noise`, 5σ.
#[test]
fn bjt_amplifier_matches_dot_noise() {
    let nt = 100e-9;
    let n = (1 << 18) * long();
    let deck = format!(
        "ce\n.options devnoise seed=21\nvcc 9 0 5\nvin 1 0 dc 0 ac 1\nrs 1 2 1k\n\
         cin 2 3 10u\nrb1 9 3 100k\nrb2 3 0 22k\nrc 9 4 4.7k\nre 5 0 1k\nce 5 0 100u\n\
         q1 4 3 5 qn\ncl 4 0 1n\n\
         .model qn npn is=1e-15 bf=150 rb=200 rc=20 re=2 kf=1e-15 af=1\n\
         .tran {nt} {}\n.noise v(4) vin dec 20 1k 1meg\n.end\n",
        n as f64 * nt
    );
    let (r, s) = against_dot_noise(&deck, "v(4)", nt, 1e4, 1e5, 1 << 12);
    println!("BJT CE: tran/.noise in [10k,100k] = {r:.5} (σ {s:.4})");
    assert!((r - 1.0).abs() < 5.0 * s, "{r} ± {s}");
}

/// A MOS1 common-source stage (channel thermal ⅔gm, RD/RS thermal,
/// flicker `KF·gm²/(f^AF·W·L·Cox)` with AF = 1): same check.
#[test]
fn mos_amplifier_matches_dot_noise() {
    let nt = 100e-9;
    let n = (1 << 18) * long();
    let deck = format!(
        "cs\n.options devnoise seed=22\nvdd 9 0 5\nvin 1 0 dc 1.5 ac 1\nrg 1 2 10k\n\
         m1 4 2 5 0 nm l=2u w=50u\nrd 9 4 10k\nrs 5 0 200\ncl 4 0 1n\n\
         .model nm nmos level=1 vto=0.7 kp=100u lambda=0.02 rd=20 rs=20 tox=20n \
         kf=1e-24 af=1\n.tran {nt} {}\n.noise v(4) vin dec 20 1k 1meg\n.end\n",
        n as f64 * nt
    );
    let (r, s) = against_dot_noise(&deck, "v(4)", nt, 1e4, 1e5, 1 << 12);
    println!("MOS CS: tran/.noise in [10k,100k] = {r:.5} (σ {s:.4})");
    assert!((r - 1.0).abs() < 5.0 * s, "{r} ± {s}");
}

// ---------------------------------------------------------------------
// Reproducibility and switches.

const PAIR: &str = "pair
.options devnoise
r1 1 0 1k
c1 1 0 1n
.tran 10n 20u
.end
";

#[test]
fn same_seed_same_bits_other_seed_other_noise() {
    let a = run(PAIR, 0, |_| {}).0.remove(0);
    let b = run(PAIR, 0, |_| {}).0.remove(0);
    let (va, vb) = (col(&a, "v(1)"), col(&b, "v(1)"));
    assert!(va.iter().zip(vb).all(|(x, y)| x.to_bits() == y.to_bits()));
    let c = run(PAIR, 0, |o| o.seed = 2).0.remove(0);
    let same = col(&c, "v(1)")
        .iter()
        .zip(va)
        .filter(|(x, y)| x == y)
        .count();
    assert_eq!(same, 1, "only t = 0 coincides");
    // Without the option the circuit is noiseless (all zero).
    let q = run(PAIR, 0, |o| o.dev_noise = false).0.remove(0);
    assert!(col(&q, "v(1)").iter().all(|&v| v == 0.0));
}

/// A device keeps its noise when other noisy devices (and a TRNOISE
/// source) join elsewhere in the circuit, or the deck lists it
/// later: its streams are keyed by its own name.
#[test]
fn a_device_keeps_its_noise_when_others_join() {
    // A lone resistor: its node voltage is its own generator's
    // current (no integration error to differ between time grids).
    let alone = PAIR.replace("c1 1 0 1n\n", "");
    let other = alone
        .replace(
            "r1 1 0 1k",
            "r9 7 0 3k\nd9 8 0 dd\ni9 0 8 1m\nvn 6 0 trnoise(1m 30n)\nr6 6 0 1\nr1 1 0 1k",
        )
        .replace(".end", ".model dd d kf=1e-12\n.end");
    let a = run(&alone, 0, |_| {}).0.remove(0);
    let b = run(&other, 0, |_| {}).0.remove(0);
    let (ga, gb) = (at_grid(&a, "v(1)", 10e-9), at_grid(&b, "v(1)", 10e-9));
    assert_eq!(ga.len(), gb.len());
    let scale = ga.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(
        ga.iter()
            .zip(&gb)
            .all(|(x, y)| (x - y).abs() <= 1e-9 * scale),
        "noise of r1 changed"
    );
}

/// `devnt` sets the grid; a TSTEP-default deck plants a breakpoint
/// on every TSTEP.
#[test]
fn devnt_sets_the_grid() {
    let deck = PAIR.replace(".options devnoise", ".options devnoise devnt=40n");
    let (sols, notes) = run(&deck, 0, |_| {});
    assert!(notes.iter().any(|s| s.contains("devnt 4e-8")), "{notes:?}");
    let g = at_grid(&sols[0], "v(1)", 40e-9);
    assert_eq!(g.len(), 500);
}

const PROBES: &[&str] = &["devnoise_rc", "devnoise_diode", "devnoise_bjt"];

#[test]
fn probes_match_their_dune_golden() {
    let bless = std::env::var_os("DUNE_BLESS").is_some();
    for stem in PROBES {
        let text = std::fs::read_to_string(corpus(&format!("regress/{stem}.cir"))).unwrap();
        let (mut sols, notes) = run(&text, 0, |_| {});
        let sol = sols.remove(0);
        assert!(
            notes.iter().any(|n| n.contains("device noise:")),
            "{notes:?}"
        );
        let golden = corpus(&format!("regress/golden/{stem}.dune.raw"));
        if bless {
            let raw = Rawfile {
                plots: vec![sol.to_plot(stem, "")],
                warnings: vec![],
            };
            std::fs::write(&golden, raw.to_bytes(Format::Binary)).unwrap();
            continue;
        }
        let raw = Rawfile::read_from(&golden)
            .unwrap_or_else(|e| panic!("{}: {e} (bless with DUNE_BLESS=1)", golden.display()));
        let plot = &raw.plots[0];
        assert_eq!(plot.num_points(), sol.num_points(), "{stem}: point count");
        for (name, _) in &sol.variables {
            let want = plot.real_values(name).unwrap();
            let got = col(&sol, name);
            // Bit-exact streams; the LU around them may move ulps
            // between builds (SIMD): 1e-12 relative, as the TRNOISE
            // probes.
            let scale = want.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-30);
            for (i, (a, b)) in got.iter().zip(want).enumerate() {
                assert!(
                    (a - b).abs() <= 1e-12 * scale,
                    "{stem}: {name}[{i}] = {a:e}, golden {b:e}"
                );
            }
        }
    }
    assert!(!bless, "goldens blessed; rerun without DUNE_BLESS");
}
