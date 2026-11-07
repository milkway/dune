//! The `corpus/pic` figures: extraction text against the hand-written
//! goldens in `corpus/pic/golden/`, the extracted netlist through the
//! normal `dune-netlist` parser, and a simulation through
//! `Circuit::run` checked against closed forms (or sane ranges, for
//! the nonlinear ones).

use std::path::PathBuf;

use dune_core::{Circuit, Options, Solution};
use dune_pic::{ExtractOptions, extract};

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/pic")
        .join(rel)
}

/// Whitespace-normalized lines (runs of blanks collapse, blank lines
/// drop).
fn normalize(s: &str) -> Vec<String> {
    s.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Extract `name.pic`, compare with `golden/name.cir`, parse, run
/// every analysis.
fn check(name: &str) -> Vec<Solution> {
    let src = std::fs::read_to_string(corpus(&format!("{name}.pic"))).unwrap();
    let ex = extract(&src, &ExtractOptions::default()).unwrap();
    assert!(
        ex.diagnostics.is_empty(),
        "{name}: unexpected diagnostics: {:#?}",
        ex.diagnostics
    );
    let golden = std::fs::read_to_string(corpus(&format!("golden/{name}.cir"))).unwrap();
    assert_eq!(
        normalize(&ex.netlist),
        normalize(&golden),
        "{name}: extraction differs from golden\n{}",
        ex.netlist
    );

    // As `dune pic --run` does: the deck sits next to the figure, so a
    // relative `.include` resolves there.
    let parsed = dune_netlist::parse_with_loader(
        &corpus(&format!("{name}.cir")),
        ex.netlist.clone(),
        &mut dune_netlist::deck::FsLoader,
    );
    assert!(parsed.is_clean(), "{name}: {:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let mut sols = Vec::new();
    for a in &parsed.netlist.analyses {
        sols.extend(ckt.run(a, &options).unwrap().0);
    }
    sols
}

fn col<'a>(s: &'a Solution, name: &str) -> &'a [f64] {
    s.column(name).unwrap_or_else(|| {
        panic!(
            "no `{name}` in {}: {:?}",
            s.plotname,
            s.variables.iter().map(|v| &v.0).collect::<Vec<_>>()
        )
    })
}

/// Linear interpolation of column `y` at abscissa `t`.
fn at(t_col: &[f64], y: &[f64], t: f64) -> f64 {
    let k = t_col.partition_point(|&x| x < t).clamp(1, t_col.len() - 1);
    let (t0, t1) = (t_col[k - 1], t_col[k]);
    y[k - 1] + (y[k] - y[k - 1]) * (t - t0) / (t1 - t0)
}

#[test]
fn rc_step_response_is_exponential() {
    let sols = check("rc");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "Transient Analysis");
    let t = col(s, "time");
    let v = col(s, "v(vout)");
    // 5 (1 - exp(-t/tau)), tau = 1 ms; the 1 us rise shifts it by 0.5 us.
    for &tt in &[0.5e-3, 1e-3, 2e-3, 4e-3] {
        let want = 5.0 * (1.0 - (-(tt - 0.5e-6) / 1e-3f64).exp());
        let got = at(t, v, tt);
        assert!(
            (got - want).abs() < 5e-3,
            "v(vout)({tt}) = {got}, want {want}"
        );
    }
    // The input node follows the source.
    assert!((at(t, col(s, "v(vin)"), 2e-3) - 5.0).abs() < 1e-9);
}

#[test]
fn rlc_ac_matches_transfer_function() {
    let sols = check("rlc");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "AC Analysis");
    let f = s.column_complex("frequency").unwrap();
    let v = s.column_complex("v(vout)").unwrap();
    assert_eq!(f.len(), 3 * 20 + 1);
    let (r, l, c) = (20.0, 10e-3, 1e-6);
    let mut peak: (f64, f64) = (0.0, 0.0);
    for (fk, vk) in f.iter().zip(v) {
        let w = 2.0 * std::f64::consts::PI * fk.re;
        // H = 1 / (1 - w^2 LC + j w RC)
        let h = num_complex::Complex64::new(1.0 - w * w * l * c, w * r * c).inv();
        assert!(
            (vk - h).norm() <= 1e-6 * h.norm().max(1e-3),
            "f={} {vk} vs {h}",
            fk.re
        );
        if vk.norm() > peak.1 {
            peak = (fk.re, vk.norm());
        }
    }
    // Q = 5 at f0 = 1.59 kHz (the grid point nearest f0 sits ~1% off).
    assert!((peak.0 - 1591.5).abs() < 60.0, "peak at {} Hz", peak.0);
    assert!(peak.1 > 4.5 && peak.1 < 5.1, "peak |H| = {}", peak.1);
}

#[test]
fn rect_charges_the_reservoir() {
    let sols = check("rect");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    let t = col(s, "time");
    let v = col(s, "v(vout)");
    let peak = v.iter().cloned().fold(f64::MIN, f64::max);
    // One diode drop (plus RS) under the 10 V crest.
    assert!(peak > 9.0 && peak < 9.6, "peak {peak}");
    // Ripple: the load discharges ~ I/C * T = 9.3 mA / 100 uF * 16.7 ms.
    let min_late = t
        .iter()
        .zip(v)
        .filter(|(tt, _)| **tt > 20e-3)
        .map(|(_, vv)| *vv)
        .fold(f64::MAX, f64::min);
    assert!(
        min_late > 7.2 && min_late < peak - 0.5,
        "ripple trough {min_late}"
    );
    // The diode never conducts backwards: v(vout) <= source + small.
    assert!(v.iter().all(|&x| x > -1e-3), "negative output");
}

#[test]
fn bjtamp_biases_and_amplifies() {
    let sols = check("bjtamp");
    assert_eq!(sols.len(), 2);
    let op = &sols[0];
    assert_eq!(op.plotname, "Operating Point");
    let vb = col(op, "v(vb)")[0];
    let ve = col(op, "v(n1)")[0];
    let vc = col(op, "v(vout)")[0];
    // Hand analysis: Vth = 2.105 V, Rth = 8.25k, IB ~ 13 uA, IC ~ 1.3 mA.
    assert!((vb - ve - 0.68).abs() < 0.08, "vbe = {}", vb - ve);
    assert!((ve - 1.33).abs() < 0.1, "ve = {ve}");
    assert!((vc - 5.8).abs() < 0.5, "vc = {vc}");
    assert!((col(op, "v(vcc)")[0] - 12.0).abs() < 1e-9);

    let tr = &sols[1];
    assert_eq!(tr.plotname, "Transient Analysis");
    let t = col(tr, "time");
    let vout = col(tr, "v(vout)");
    // Last full cycle (2..3 ms): peak-to-peak ~ 2 * 10 mV * R3/(R4 + re).
    let late: Vec<f64> = t
        .iter()
        .zip(vout)
        .filter(|(tt, _)| **tt >= 2e-3)
        .map(|(_, v)| *v)
        .collect();
    let pp = late.iter().cloned().fold(f64::MIN, f64::max)
        - late.iter().cloned().fold(f64::MAX, f64::min);
    let gain = pp / 20e-3;
    assert!(gain > 4.0 && gain < 4.7, "|gain| = {gain}");
    // Inverting: at the input crest (2.25 ms) the output is below bias.
    assert!(at(t, vout, 2.25e-3) < vc, "not inverting");
}

#[test]
fn opamp_inverting_gain_through_include() {
    // The X card's body comes from `.include opamp_ideal.sub`, resolved
    // next to the figure.
    let sols = check("opamp");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "DC transfer characteristic");
    let vin = col(s, "v(vin)");
    let vout = col(s, "v(vout)");
    assert_eq!(vin.len(), 9);
    // Finite-gain closed form: -(R2/R1) / (1 + (1 + R2/R1)/A), A = 200k
    // (the macromodel's 2 Meg input and 75 ohm output resistances move
    // it by < 1e-6 relative).
    let (r1, r2, a) = (1e3, 10e3, 200e3);
    let gain = -(r2 / r1) / (1.0 + (1.0 + r2 / r1) / a);
    for (&vi, &vo) in vin.iter().zip(vout) {
        assert!(
            (vo - gain * vi).abs() <= 1e-5 * (gain * vi).abs().max(1e-6),
            "vin {vi}: vout {vo}, want {}",
            gain * vi
        );
    }
    // The ideal -R2/R1 within the loop-gain error.
    let k = vin.iter().position(|&v| (v - 0.5).abs() < 1e-12).unwrap();
    assert!((vout[k] + 5.0).abs() < 1e-3, "vout(0.5) = {}", vout[k]);
    // The virtual ground holds.
    assert!(col(s, "v(vm)").iter().all(|v| v.abs() < 1e-4));
}

#[test]
fn tline_reflection_from_a_mismatched_load() {
    let sols = check("tline");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "Transient Analysis");
    let t = col(s, "time");
    let vin = col(s, "v(vin)");
    let vend = col(s, "v(vend)");
    // Matched source: launched wave 0.5 V; Gamma_L = (150-50)/(150+50)
    // = 0.5, so the load sees 0.75 V from TD on and the reflection
    // lifts the input to 0.75 V from 2 TD on (the 10 ps edge is
    // centered at 5 ps).
    let cases = [
        (0.5e-9, 0.5, 0.0),
        (0.9e-9, 0.5, 0.0),
        (1.5e-9, 0.5, 0.75),
        (1.9e-9, 0.5, 0.75),
        (2.5e-9, 0.75, 0.75),
        (4.5e-9, 0.75, 0.75),
    ];
    for (tt, want_in, want_end) in cases {
        let (gi, ge) = (at(t, vin, tt), at(t, vend, tt));
        assert!((gi - want_in).abs() < 1e-3, "v(vin)({tt}) = {gi}");
        assert!((ge - want_end).abs() < 1e-3, "v(vend)({tt}) = {ge}");
    }
}

#[test]
fn bandpass_ac_matches_transfer_function() {
    let sols = check("bandpass");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "AC Analysis");
    let f = s.column_complex("frequency").unwrap();
    let v = s.column_complex("v(vout)").unwrap();
    assert_eq!(f.len(), 3 * 20 + 1);
    let (r, l, c) = (50.0, 1e-3, 100e-9);
    let mut peak: (f64, f64) = (0.0, 0.0);
    for (fk, vk) in f.iter().zip(v) {
        let w = 2.0 * std::f64::consts::PI * fk.re;
        // H = R / (R + j(w L - 1/(w C)))
        let h = num_complex::Complex64::new(r, 0.0)
            / num_complex::Complex64::new(r, w * l - 1.0 / (w * c));
        assert!(
            (vk - h).norm() <= 1e-6 * h.norm().max(1e-3),
            "f={} {vk} vs {h}",
            fk.re
        );
        if vk.norm() > peak.1 {
            peak = (fk.re, vk.norm());
        }
    }
    // Unity peak at f0 = 15.9 kHz (grid point within ~6%); Q = 2 keeps
    // the grid's |H| close to 1.
    assert!((peak.0 - 15915.0).abs() < 1000.0, "peak at {} Hz", peak.0);
    assert!(
        peak.1 > 0.99 && peak.1 <= 1.0 + 1e-9,
        "peak |H| = {}",
        peak.1
    );
}

#[test]
fn cmos_inverter_switches_rail_to_rail() {
    let sols = check("cmosinv");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "Transient Analysis");
    let t = col(s, "time");
    let vout = col(s, "v(vout)");
    let ivdd = col(s, "i(v2)");
    // Input low (op point, and 27..55 ns after the falling edge):
    // output at VDD; input high (6..26 ns): output at 0. No static
    // current in either state (MOS1 has no subthreshold conduction).
    for &tt in &[2e-9, 4.5e-9, 40e-9, 54e-9] {
        let v = at(t, vout, tt);
        assert!((v - 5.0).abs() < 1e-3, "v(vout)({tt}) = {v}");
        assert!(at(t, ivdd, tt).abs() < 1e-9, "i(v2)({tt})");
    }
    for &tt in &[15e-9, 25e-9] {
        let v = at(t, vout, tt);
        assert!(v.abs() < 1e-3, "v(vout)({tt}) = {v}");
        assert!(at(t, ivdd, tt).abs() < 1e-9, "i(v2)({tt})");
    }
    // Propagation delays (input crosses 2.5 V at 5.5 ns and 26.5 ns):
    // the ~0.5 ns RC of 1 pF on the on-resistance, a few hundred ps.
    let cross = |from: f64, rising: bool| {
        let k = t
            .iter()
            .zip(t.iter().skip(1))
            .zip(vout.windows(2))
            .position(|((&t0, _), w)| {
                t0 >= from
                    && if rising {
                        w[0] < 2.5 && w[1] >= 2.5
                    } else {
                        w[0] > 2.5 && w[1] <= 2.5
                    }
            })
            .unwrap_or_else(|| panic!("no crossing after {from}"));
        t[k] + (2.5 - vout[k]) * (t[k + 1] - t[k]) / (vout[k + 1] - vout[k])
    };
    let tphl = cross(5e-9, false) - 5.5e-9;
    let tplh = cross(26e-9, true) - 26.5e-9;
    assert!(tphl > 0.05e-9 && tphl < 2e-9, "tpHL = {tphl}");
    assert!(tplh > 0.05e-9 && tplh < 2e-9, "tpLH = {tplh}");
    // The pull-up has half the transconductance per square (KP 50u vs
    // 110u) on twice the width: comparable, not equal, delays.
    assert!(tplh / tphl > 0.5 && tplh / tphl < 2.0, "{tplh} / {tphl}");
}

#[test]
fn ltraline_lossless_delay() {
    // An LC LTRA (lossless) between matched terminations: the step
    // arrives after LEN*sqrt(L*C) = 1 ns at half amplitude.
    let src = ".PS
spice = 1
In: (0.8,1); End: (3,1)
V1: vsource((0,1), (0,0), PULSE(0 1 0 10p 10p 20n 40n))
R1: resistor(V1.P, In, 50)
O1: ltraline(In, End, LMOD)
R2: resistor(End, (3,0), 50)
line from O1.N1 to (O1.N1.x,0); line from O1.N2 to (O1.N2.x,0)
line from V1.N to R2.N
dot at (O1.N1.x,0); dot at (O1.N2.x,0)
ground((0.4,0))
.model LMOD LTRA(R=0 G=0 L=250n C=100p LEN=0.2)
.tran 10p 3n
.PE
";
    let ex = extract(src, &ExtractOptions::default()).unwrap();
    assert!(ex.diagnostics.is_empty(), "{:#?}", ex.diagnostics);
    assert!(
        ex.netlist.contains("\nO1 In 0 End 0 LMOD\n"),
        "{}",
        ex.netlist
    );
    let parsed = dune_netlist::parse_str("ltra.cir", &ex.netlist);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let s = &ckt
        .run(&parsed.netlist.analyses[0], &Options::default())
        .unwrap()
        .0[0];
    let t = col(s, "time");
    let vend = col(s, "v(end)");
    // Z0 = sqrt(L/C) = 50, TD = 0.2 * sqrt(250n * 100p) = 1 ns.
    assert!(at(t, vend, 0.8e-9).abs() < 1e-3);
    assert!((at(t, vend, 1.5e-9) - 0.5).abs() < 5e-3);
    assert!((at(t, vend, 2.8e-9) - 0.5).abs() < 5e-3);
    assert!((at(t, col(s, "v(in)"), 0.5e-9) - 0.5).abs() < 5e-3);
}

#[test]
fn vcvs_vccs_cascade_gain() {
    let sols = check("vcamp");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "DC transfer characteristic");
    let vin = col(s, "v(vin)");
    assert_eq!(vin.len(), 5);
    // Divider 1/2, VCVS x10, VCCS 1 mS into 2k (current pulled out of
    // the P node): v(vm) = 5 vin, v(vout) = -10 vin. Linear, so exact
    // to roundoff.
    for (k, &vi) in vin.iter().enumerate() {
        for (name, gain) in [("v(vx)", 0.5), ("v(vm)", 5.0), ("v(vout)", -10.0)] {
            let got = col(s, name)[k];
            assert!(
                (got - gain * vi).abs() <= 1e-9,
                "{name} at vin={vi}: {got}, want {}",
                gain * vi
            );
        }
    }
}

#[test]
fn cccs_ccvs_read_the_ammeter_current() {
    let sols = check("ccprobe");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "Operating Point");
    // i(v0) = 2 V / 1k; F: 3 i(v0) out of Va through 1k; H: 500 i(v0).
    let i0 = col(s, "i(v0)")[0];
    assert!((i0 - 2e-3).abs() < 1e-12, "i(v0) = {i0}");
    let va = col(s, "v(va)")[0];
    let vb = col(s, "v(vb)")[0];
    assert!((va + 6.0).abs() < 1e-9, "v(va) = {va}");
    assert!((vb - 1.0).abs() < 1e-9, "v(vb) = {vb}");
}

#[test]
fn coupled_tanks_match_nodal_closed_form() {
    use num_complex::Complex64 as C64;
    let sols = check("tuned");
    assert_eq!(sols.len(), 1);
    let s = &sols[0];
    assert_eq!(s.plotname, "AC Analysis");
    let f = s.column_complex("frequency").unwrap();
    let v = s.column_complex("v(vout)").unwrap();
    assert_eq!(f.len(), 401);
    let (rs, rl, l, c, k) = (1e3, 1e3, 10e-6, 10e-9, 0.1);
    // Inductor currents from the inverse inductance matrix:
    // I = Gamma V / (j w), Gamma = [L M; M L]^-1.
    let m = k * l;
    let det = l * l - m * m;
    let (g11, g12) = (l / det, -m / det);
    let mut mags = Vec::with_capacity(f.len());
    for (fk, vk) in f.iter().zip(v) {
        let w = 2.0 * std::f64::consts::PI * fk.re;
        let jw = C64::new(0.0, w);
        // Node 1: (V1 - 1)/Rs + jwC V1 + (g11 V1 + g12 V2)/jw = 0
        // Node 2: V2/RL + jwC V2 + (g12 V1 + g11 V2)/jw = 0
        let a11 = 1.0 / rs + jw * c + g11 / jw;
        let a12 = g12 / jw;
        let a22 = 1.0 / rl + jw * c + g11 / jw;
        let v2 = -(a12 / rs) / (a11 * a22 - a12 * a12);
        assert!(
            (vk - v2).norm() <= 1e-6 * v2.norm().max(1e-3),
            "f={} {vk} vs {v2}",
            fk.re
        );
        mags.push(vk.norm());
    }
    // Overcoupled (kQ = 3.2): two peaks near f0/sqrt(1 +- k), each at
    // the matched-transfer limit 1/2 (Rs = RL, lossless coupling).
    let peaks: Vec<(f64, f64)> = (1..mags.len() - 1)
        .filter(|&i| mags[i] > mags[i - 1] && mags[i] > mags[i + 1])
        .map(|i| (f[i].re, mags[i]))
        .collect();
    assert_eq!(peaks.len(), 2, "{peaks:?}");
    let f0 = 1.0 / (2.0 * std::f64::consts::PI * (l * c).sqrt());
    let (lo, hi) = (f0 / (1.0 + k).sqrt(), f0 / (1.0 - k).sqrt());
    assert!((peaks[0].0 - lo).abs() / lo < 0.01, "{peaks:?} vs {lo}");
    assert!((peaks[1].0 - hi).abs() / hi < 0.01, "{peaks:?} vs {hi}");
    for p in &peaks {
        assert!((p.1 - 0.5).abs() < 2e-3, "peak |H| = {}", p.1);
    }
}

#[test]
fn bulk_bias_raises_the_threshold() {
    let sols = check("mos4bias");
    assert_eq!(sols.len(), 2);
    // MOS1 saturation, LAMBDA = 0: Id = KP/2 W/L (VGS - VT)^2 with
    // VT = VTO + GAMMA (sqrt(PHI - VBS) - sqrt(PHI)).
    let id = |vbs: f64| {
        let vt = 0.7 + 0.5 * ((0.6 - vbs).sqrt() - 0.6f64.sqrt());
        0.5 * 100e-6 * 10.0 * (2.0 - vt).powi(2)
    };
    let op = &sols[0];
    assert_eq!(op.plotname, "Operating Point");
    let got = -col(op, "i(v1)")[0];
    assert!((got - id(-2.0)).abs() <= 1e-6 * id(-2.0), "{got}");
    assert!((got - 0.388e-3).abs() < 1e-6, "{got}");
    // The drain sits at 5 - Id * 1k, deep in saturation.
    assert!((col(op, "v(vd)")[0] - (5.0 - 1e3 * got)).abs() < 1e-9);

    let dc = &sols[1];
    assert_eq!(dc.plotname, "DC transfer characteristic");
    let vb = col(dc, "v(vb)");
    let i = col(dc, "i(v1)");
    assert_eq!(vb.len(), 4);
    for (&vbs, &ik) in vb.iter().zip(i) {
        assert!(
            (-ik - id(vbs)).abs() <= 1e-6 * id(vbs),
            "vbs {vbs}: {}",
            -ik
        );
    }
    // Body effect: no bias 0.845 mA, VBS = -3 V less than half.
    assert!((-i[0] - 0.845e-3).abs() < 1e-6);
    assert!(-i[3] < 0.4 * -i[0]);
}

/// Run every analysis of a deck file (as `dune -b` does).
fn run_deck(path: &std::path::Path) -> Vec<Solution> {
    let text = std::fs::read_to_string(path).unwrap();
    let parsed = dune_netlist::parse_with_loader(path, text, &mut dune_netlist::deck::FsLoader);
    assert!(parsed.is_clean(), "{}: {:?}", path.display(), parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let mut sols = Vec::new();
    for a in &parsed.netlist.analyses {
        sols.extend(ckt.run(a, &options).unwrap().0);
    }
    sols
}

/// The figure's solutions against the Berkeley deck's, column by
/// column through `map` (deck column, figure column). The figure keeps
/// the deck's card order, so the circuits are the same matrices and
/// the results agree to roundoff.
fn same_as_berkeley(name: &str, map: &[(&str, &str)]) {
    let fig = check(name);
    let deck = run_deck(
        &corpus("")
            .join("../spice3f5/examples")
            .join(format!("{name}.cir")),
    );
    assert_eq!(fig.len(), deck.len());
    for (f, d) in fig.iter().zip(&deck) {
        assert_eq!(f.plotname, d.plotname);
        assert_eq!(f.variables.len(), d.variables.len(), "{}", f.plotname);
        for &(dc, fc) in map {
            let (a, b) = (col(d, dc), col(f, fc));
            assert_eq!(a.len(), b.len(), "{}: {dc}", f.plotname);
            for (k, (x, y)) in a.iter().zip(b).enumerate() {
                assert!(
                    (x - y).abs() <= 1e-9 * x.abs().max(1e-3),
                    "{} {dc} vs {fc} [{k}]: {x} vs {y}",
                    f.plotname
                );
            }
        }
    }
}

#[test]
fn diffpair_figure_simulates_like_the_berkeley_deck() {
    // Berkeley node -> figure place (see the figure's header).
    same_as_berkeley(
        "diffpair",
        &[
            ("time", "time"),
            ("v(1)", "v(in)"),
            ("v(2)", "v(b1)"),
            ("v(3)", "v(b2)"),
            ("v(4)", "v(vc1)"),
            ("v(5)", "v(vc2)"),
            ("v(6)", "v(tail)"),
            ("v(7)", "v(bias)"),
            ("v(8)", "v(vp)"),
            ("v(9)", "v(vn)"),
            ("i(vin)", "i(vin)"),
            ("i(vcc)", "i(vcc)"),
            ("i(vee)", "i(vee)"),
        ],
    );
}

#[test]
fn rtlinv_figure_simulates_like_the_berkeley_deck() {
    same_as_berkeley(
        "rtlinv",
        &[
            ("v(1)", "v(in)"),
            ("v(2)", "v(b1)"),
            ("v(3)", "v(out1)"),
            ("v(4)", "v(b2)"),
            ("v(5)", "v(out2)"),
            ("v(6)", "v(vp)"),
            ("i(vcc)", "i(vcc)"),
            ("i(vin)", "i(vin)"),
        ],
    );
}

// --- 4th wave: figures of the basic/ decks, rca3040 and device probes --------

/// Every column of every plot of the deck at `deck` (relative to
/// `corpus/`) against the figure's, the deck's node names translated
/// through `nodes` (deck node, figure place); currents and anything
/// unmapped keep their name. The figures keep the deck's card order
/// and pin order, so the matrices are the same and the results agree
/// to roundoff (in practice: bit for bit). Returns the largest
/// absolute difference seen.
fn same_as_deck(name: &str, deck: &str, nodes: &[(&str, &str)]) -> f64 {
    let fig = check(name);
    let deck = run_deck(&corpus("").join("..").join(deck));
    assert_eq!(fig.len(), deck.len(), "{name}: plot count");
    let rename = |v: &str| -> String {
        if let Some(n) = v.strip_prefix("v(").and_then(|r| r.strip_suffix(')'))
            && let Some((_, p)) = nodes.iter().find(|(d, _)| d.eq_ignore_ascii_case(n))
        {
            return format!("v({p})");
        }
        v.to_string()
    };
    let mut worst = 0.0f64;
    for (f, d) in fig.iter().zip(&deck) {
        assert_eq!(f.plotname, d.plotname, "{name}");
        assert_eq!(
            f.variables.len(),
            d.variables.len(),
            "{name}: {}",
            f.plotname
        );
        for (dv, _) in &d.variables {
            let fv = rename(dv);
            let pairs: Vec<(f64, f64)> = match (d.column(dv), f.column(&fv)) {
                (Some(a), Some(b)) => {
                    assert_eq!(a.len(), b.len(), "{name} {}: {dv}", f.plotname);
                    a.iter().copied().zip(b.iter().copied()).collect()
                }
                _ => {
                    let a = d.column_complex(dv).unwrap();
                    let b = f.column_complex(&fv).unwrap_or_else(|| {
                        panic!("{name} {}: no `{fv}` for deck `{dv}`", f.plotname)
                    });
                    assert_eq!(a.len(), b.len(), "{name} {}: {dv}", f.plotname);
                    a.iter()
                        .zip(b)
                        .flat_map(|(x, y)| [(x.re, y.re), (x.im, y.im)])
                        .collect()
                }
            };
            for (k, (x, y)) in pairs.into_iter().enumerate() {
                assert!(
                    (x - y).abs() <= 1e-9 * x.abs().max(1e-3),
                    "{name} {} {dv} vs {fv} [{k}]: {x} vs {y}",
                    f.plotname
                );
                worst = worst.max((x - y).abs());
            }
        }
    }
    worst
}

#[test]
fn divider_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "divider",
        "basic/divider.cir",
        &[("1", "vs"), ("2", "vdiv"), ("3", "vi")],
    );
    assert_eq!(worst, 0.0);
    let op = &check("divider")[0];
    assert!((col(op, "v(vdiv)")[0] - 10.0 / 3.0).abs() < 1e-9);
    assert!((col(op, "v(vi)")[0] - 4.7).abs() < 1e-9);
}

#[test]
fn depsrc_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "depsrc",
        "basic/depsrc.cir",
        &[
            ("1", "vc"),
            ("2", "ve"),
            ("3", "vg"),
            ("4", "vf"),
            ("5", "vh"),
        ],
    );
    assert_eq!(worst, 0.0);
    // i(v1) = -2 mA; E: 2.5 * 2; G: 1m * 2 into 2k; F: 10 i(v1) into
    // 100; H: -500 i(v1).
    let op = &check("depsrc")[0];
    for (node, want) in [
        ("v(ve)", 5.0),
        ("v(vg)", 4.0),
        ("v(vf)", -2.0),
        ("v(vh)", 1.0),
    ] {
        let got = col(op, node)[0];
        assert!((got - want).abs() < 1e-9, "{node} = {got}");
    }
}

#[test]
fn dc2src_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "dc2src",
        "basic/dc2src.cir",
        &[("1", "v1n"), ("2", "v2n"), ("3", "vx"), ("4", "vout")],
    );
    assert_eq!(worst, 0.0);
    // Millman: v(vx) = (2 v1 + v2) / 5, v(vout) = -1.5 v(vx).
    let s = &check("dc2src")[0];
    let (v1, v2) = (col(s, "v(v1n)"), col(s, "v(v2n)"));
    assert_eq!(v1.len(), 5 * 5);
    for k in 0..v1.len() {
        let vx = (2.0 * v1[k] + v2[k]) / 5.0;
        assert!((col(s, "v(vx)")[k] - vx).abs() < 1e-9);
        assert!((col(s, "v(vout)")[k] + 1.5 * vx).abs() < 1e-9);
    }
}

#[test]
fn acrlc_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "acrlc",
        "basic/acrlc.cir",
        &[
            ("1", "in"),
            ("2", "pri"),
            ("3", "sec"),
            ("4", "vc"),
            ("5", "vout"),
        ],
    );
    assert_eq!(worst, 0.0);
}

#[test]
fn lck_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "lck",
        "basic/lck.cir",
        &[("1", "vs"), ("2", "vl"), ("3", "sec")],
    );
    assert_eq!(worst, 0.0);
    // At dc the coils short and the capacitors open.
    let op = &check("lck")[0];
    assert!((col(op, "v(vl)")[0] - 3.0).abs() < 1e-9);
    assert!(col(op, "v(sec)")[0].abs() < 1e-9);
}

#[test]
fn truic_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck("truic", "basic/truic.cir", &[("1", "vc"), ("2", "vl")]);
    assert_eq!(worst, 0.0);
    // uic starts from the stored state (C1 at 2 V, L1 at 1 mA); as in
    // ngspice, no t = 0 point is emitted, so the first one sits just
    // off it.
    let s = &check("truic")[0];
    assert!(col(s, "time")[0] > 0.0);
    assert!((col(s, "v(vc)")[0] - 2.0).abs() < 1e-3);
    assert!((col(s, "i(l1)")[0] - 1e-3).abs() < 1e-4);
}

#[test]
fn trrlc_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "trrlc",
        "basic/trrlc.cir",
        &[("1", "in"), ("2", "vr"), ("3", "vout")],
    );
    assert_eq!(worst, 0.0);
}

#[test]
fn trpwl_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck(
        "trpwl",
        "basic/trpwl.cir",
        &[("1", "in1"), ("2", "out1"), ("3", "in2"), ("4", "out2")],
    );
    assert_eq!(worst, 0.0);
}

#[test]
fn acdiode_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck("acdiode", "basic/acdiode.cir", &[("1", "va")]);
    assert_eq!(worst, 0.0);
}

#[test]
fn dfwd_figure_simulates_like_the_basic_deck() {
    let worst = same_as_deck("dfwd", "basic/dfwd.cir", &[("1", "va"), ("2", "vk")]);
    assert_eq!(worst, 0.0);
}

#[test]
fn jfet_amp_figure_simulates_like_the_regress_deck() {
    let worst = same_as_deck(
        "jfet_amp",
        "regress/jfet_amp.cir",
        &[
            ("1", "vp"),
            ("2", "vg"),
            ("3", "vd"),
            ("4", "vs"),
            ("5", "in"),
        ],
    );
    assert_eq!(worst, 0.0);
}

#[test]
fn mos3_inv_figure_simulates_like_the_regress_deck() {
    let worst = same_as_deck(
        "mos3_inv",
        "regress/mos3_inv.cir",
        &[
            ("1", "vp"),
            ("2", "in"),
            ("3", "out1"),
            ("4", "out2"),
            ("5", "ain"),
            ("6", "vg"),
            ("7", "vd"),
            ("8", "vs"),
        ],
    );
    assert_eq!(worst, 0.0);
}

#[test]
fn rca3040_figure_simulates_like_the_berkeley_deck() {
    let worst = same_as_deck(
        "rca3040",
        "spice3f5/examples/rca3040.cir",
        &[
            ("1", "in"),
            ("2", "vp"),
            ("3", "vn"),
            ("5", "ba"),
            ("6", "bb"),
            ("7", "tail"),
            ("8", "bias"),
            ("9", "vt"),
            ("10", "ca"),
            ("11", "cb"),
            ("12", "vcas"),
            ("13", "vd"),
            ("14", "oa"),
            ("15", "ob"),
            ("16", "out"),
            ("17", "outb"),
            ("30", "b1"),
            ("31", "b2"),
        ],
    );
    assert_eq!(worst, 0.0);
}
