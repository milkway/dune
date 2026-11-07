//! `.measure` on simulated circuits against closed-form values.
//!
//! Tolerances: the measurement layer itself is exact on piecewise-
//! linear data (unit tests in `measure.rs`); here the error is the
//! simulation's — transient truncation error (`trtol·reltol`, LTE
//! control) and the sampling of a smooth waveform by linear
//! interpolation — so the bounds are relative 1e-4…1e-3, each
//! justified where used.

use dune_core::{Circuit, MeasDetail, Measurement, Options, Solution, measure_all};

fn run(deck: &str) -> (dune_netlist::ir::Netlist, Vec<Solution>) {
    let parsed = dune_netlist::parse_str("measure.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let opts = Options::default();
    let mut sols = Vec::new();
    for a in &parsed.netlist.analyses {
        sols.extend(ckt.run(a, &opts).unwrap().0);
    }
    (parsed.netlist, sols)
}

/// Every card of the deck, by name.
fn measures(deck: &str) -> Vec<(String, Measurement)> {
    let (nl, sols) = run(deck);
    measure_all(&nl.measures, &sols)
        .into_iter()
        .map(|o| {
            let name = nl.measures[o.card].name.clone();
            let m = o.result.unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, m)
        })
        .collect()
}

fn get(ms: &[(String, Measurement)], name: &str) -> Measurement {
    ms.iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no measure {name}"))
        .1
}

fn rel(got: f64, want: f64, tol: f64, what: &str) {
    let e = ((got - want) / want).abs();
    assert!(
        e <= tol,
        "{what}: got {got:e}, want {want:e} (rel {e:.2e} > {tol:e})"
    );
    eprintln!("{what}: got {got:.9e}, want {want:.9e}, rel {e:.2e}");
}

/// RC step (τ = 1 ns) driven by a 1 ps ramp starting at t0 = 1 ns.
/// After the ramp, v = 1 − k·e^{−(t−t0)/τ} with
/// k = (τ/tr)(e^{tr/τ} − 1), so the level-L crossing is at
/// t0 + τ·ln(k/(1−L)) and the 10–90 % rise time is τ·ln 9 exactly.
/// (PW stays short: the 3f5 `VSRCaccept` compares times with
/// `1e-7·PW`, so a PW of seconds would merge the t0 breakpoint away.)
const RC: &str = "rc step
v1 in 0 pulse(0 1 1n 1p 1p 20n 40n)
r1 in out 1k
c1 out 0 1p
.tran 10p 10n
.meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1
.meas tran thalf when v(out)=0.5
.meas tran vat find v(out) at=3n
.meas tran slope deriv v(out) at=2n
.meas tran vint integ v(out)
.meas tran vavg avg v(out) from=2n to=10n
.meas tran vrms rms v(out) from=2n to=10n
.meas tran vmax max v(out)
.end
";

#[test]
fn rc_step_closed_forms() {
    let tau: f64 = 1e-9;
    let tr: f64 = 1e-12;
    let t0 = 1e-9;
    let k = (tau / tr) * ((tr / tau).exp_m1());
    let v = |t: f64| 1.0 - k * (-(t - t0) / tau).exp();
    let ms = measures(RC);

    // 1e-4: the LTE-controlled steps place v(out) within
    // trtol·reltol of the exact curve; the 10 % / 90 % levels are
    // read by linear interpolation between ~10 ps steps on a
    // 1 ns exponential (curvature error ~1e-5 relative).
    let m = get(&ms, "trise");
    rel(m.value, tau * 9f64.ln(), 1e-4, "RC rise time 10-90%");
    let MeasDetail::Delay { trig, targ } = m.detail else {
        panic!("{m:?}")
    };
    rel(trig, t0 + tau * (k / 0.9).ln(), 1e-4, "RC trig (10%)");
    rel(targ, t0 + tau * (k / 0.1).ln(), 1e-4, "RC targ (90%)");
    rel(
        get(&ms, "thalf").value,
        t0 + tau * (2.0 * k).ln(),
        1e-4,
        "RC 50% crossing",
    );
    rel(get(&ms, "vat").value, v(3e-9), 1e-4, "RC v(3ns)");
    // dv/dt = (k/τ)e^{−(t−t0)/τ}; the three-point derivative on the
    // simulated grid: 1e-3.
    rel(
        get(&ms, "slope").value,
        k / tau * (-(2e-9 - t0) / tau).exp(),
        1e-3,
        "RC dv/dt at 2ns",
    );
    // ∫₀^T v dt: zero before t0, the ramp piece
    // ∫₀^tr [s/tr − (τ/tr)(1 − e^{−s/τ})] ds, then the exponential.
    let t_end = 10e-9;
    let ex = |t: f64| (-(t - t0) / tau).exp();
    let ramp = tr / 2.0 - (tau / tr) * (tr + tau * (-tr / tau).exp_m1());
    let integ = ramp + (t_end - t0 - tr) - k * tau * (ex(t0 + tr) - ex(t_end));
    rel(get(&ms, "vint").value, integ, 1e-4, "RC integral");
    // Average and RMS on [2n, 10n]. AVG does not interpolate the
    // window edges (ngspice's `measure_minMaxAvg`): its first sample
    // lies up to one 10 ps step past FROM, a ≤ 10p/8n · |v(2n) −
    // mean|/mean ≈ 3e-4 shift, hence 1e-3; RMS interpolates them.
    let (a, b) = (2e-9, 10e-9);
    let mean = 1.0 - k * tau * (ex(a) - ex(b)) / (b - a);
    rel(get(&ms, "vavg").value, mean, 1e-3, "RC average on [2n,10n]");
    let msq = 1.0 - 2.0 * k * tau * (ex(a) - ex(b)) / (b - a)
        + k * k * tau / 2.0 * (ex(a).powi(2) - ex(b).powi(2)) / (b - a);
    rel(
        get(&ms, "vrms").value,
        msq.sqrt(),
        1e-4,
        "RC rms on [2n,10n]",
    );
    let m = get(&ms, "vmax");
    rel(m.value, v(10e-9), 1e-4, "RC max");
    assert_eq!(m.detail, MeasDetail::At { at: 10e-9 });
}

/// Series RLC step (the `corpus/basic/trrlc.cir` R, L, C: ζ ≈ 0.158).
/// v(c) peaks at 1 + e^{−πζ/√(1−ζ²)} at π/ω_d and rings with period
/// 2π/ω_d around 1 V.
const RLC: &str = "rlc step
vin 1 0 pulse(0 1 0 1n 1n 1 2)
r1 1 2 200
l1 2 3 1m
c1 3 0 2.5n
.tran 10n 60u 0 10n
.meas tran vpeak max v(3)
.meas tran overshoot pp v(3) from=0 to=8u
.meas tran period trig v(3) val=1 rise=1 targ v(3) val=1 rise=2
.meas tran halfper trig v(3) val=1 rise=1 targ v(3) val=1 fall=2
.end
";

#[test]
fn rlc_overshoot_and_period() {
    let (r, l, c) = (200.0f64, 1e-3f64, 2.5e-9f64);
    let w0 = 1.0 / (l * c).sqrt();
    let zeta = r / 2.0 * (c / l).sqrt();
    let wd = w0 * (1.0 - zeta * zeta).sqrt();
    let peak = 1.0 + (-std::f64::consts::PI * zeta / (1.0 - zeta * zeta).sqrt()).exp();
    let ms = measures(RLC);
    // The pulse's 1 ns ramp delays the response by 0.5 ns (a 5e-5
    // fraction of the period) and the max is read off ≤10 ns
    // samples of a 10 µs sinusoid (≤ (ω_d·10ns)²/8 ≈ 5e-6): 1e-4
    // for the peak, 1e-4 for the period.
    let m = get(&ms, "vpeak");
    rel(m.value, peak, 1e-4, "RLC peak (1+overshoot)");
    let MeasDetail::At { at } = m.detail else {
        panic!()
    };
    rel(
        at,
        std::f64::consts::PI / wd + 0.5e-9,
        2e-3,
        "RLC peak time",
    );
    rel(get(&ms, "overshoot").value, peak, 1e-4, "RLC pp on [0,8u]");
    let td = 2.0 * std::f64::consts::PI / wd;
    rel(get(&ms, "period").value, td, 1e-4, "RLC ringing period");
    rel(get(&ms, "halfper").value, 1.5 * td, 1e-4, "RLC 1.5 periods");
}

/// The damped-sine source of `corpus/basic/trrlc.cir` crosses zero
/// at exactly td + k/(2f): its period is 10 µs.
#[test]
fn trrlc_source_period() {
    let deck = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../corpus/basic/trrlc.cir"
    ))
    .unwrap()
    .replace(
        ".end",
        ".meas tran per trig v(1) val=0 rise=1 td=2.5u targ v(1) val=0 rise=2 td=2.5u\n\
         .meas tran tfall when v(1)=0 fall=1 td=2.5u\n.end",
    );
    let ms = measures(&deck);
    // Near a zero of e^{−θt}·sin(ωt) the chord through two samples
    // h apart misses the zero by ≈ (y''/2y')·h²/4 = θ·h²/4
    // ≈ 1.3e-11 s for θ = 2e4, h = 0.05 µs: ~2e-6 of 7 µs. The
    // period subtracts two such errors of the same sign.
    rel(get(&ms, "per").value, 10e-6, 1e-6, "trrlc source period");
    rel(
        get(&ms, "tfall").value,
        7e-6,
        5e-6,
        "trrlc first falling zero",
    );
}

/// RC low-pass on a dense linear AC sweep: fc = 1/(2πRC) = 1 kHz.
#[test]
fn ac_corner() {
    let deck = "rc bode
vin 1 0 dc 0 ac 1
r1 1 2 1k
c1 2 0 159.15494309189535n
.ac lin 2001 900 1100
.meas ac f3db when vdb(2)=-3
.meas ac ph find vp(2) at=1k
.meas ac g find vm(2) at=1k
.meas ac re find v(2) at=1k
.end
";
    let ms = measures(deck);
    let fc = 1000.0;
    // Linear interpolation over 0.1 Hz steps of a smooth curve:
    // relative error ≪ 1e-6.
    rel(
        get(&ms, "f3db").value,
        fc * (10f64.powf(0.3) - 1.0).sqrt(),
        1e-6,
        "AC -3 dB frequency",
    );
    rel(
        get(&ms, "ph").value,
        -std::f64::consts::FRAC_PI_4,
        1e-6,
        "AC phase at fc (rad)",
    );
    rel(get(&ms, "g").value, 0.5f64.sqrt(), 1e-6, "AC |H| at fc");
    // No part: the real part, like ngspice.
    rel(get(&ms, "re").value, 0.5, 1e-6, "AC Re(H) at fc");
}

/// A resistive divider swept in DC is linear: exact to rounding.
#[test]
fn dc_divider() {
    let deck = "divider
v1 in 0 dc 0
r1 in out 1k
r2 out 0 3k
.dc v1 2 -2 -0.1
.meas dc vx find v(out) at=1.05
.meas dc vw when v(out)=0.3
.meas dc vmax max v(out) from=-1 to=1
.meas dc slope deriv v(out) at=0.33
.meas dc sw find v(in) when v(out)=v(in) cross=1
.end
";
    let ms = measures(deck);
    rel(
        get(&ms, "vx").value,
        0.75 * 1.05,
        1e-12,
        "DC find (descending sweep)",
    );
    rel(get(&ms, "vw").value, 0.4, 1e-12, "DC when");
    rel(get(&ms, "vmax").value, 0.75, 1e-12, "DC max in window");
    rel(get(&ms, "slope").value, 0.75, 1e-12, "DC derivative");
    // v(out) = v(in) only at 0.
    assert!(get(&ms, "sw").value.abs() < 1e-12);
}

/// Measurements never change the simulation: the same deck with and
/// without `.meas` cards gives bit-identical plots.
#[test]
fn measures_do_not_touch_the_simulation() {
    let bare: String = RC
        .lines()
        .filter(|l| !l.starts_with(".meas"))
        .map(|l| format!("{l}\n"))
        .collect();
    let (_, a) = run(RC);
    let (_, b) = run(&bare);
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.variables, y.variables);
        assert_eq!(x.data, y.data);
    }
}
