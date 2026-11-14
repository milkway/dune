//! Unit tests of the KSPICE lines: setup guards, the Padé fits, the
//! assignment semantics of the stamps, and the per-instance state the
//! C keeps in statics.

use super::*;
use crate::engine::{Engine, TranParams};
use crate::{Circuit, Options};

fn model(r: f64, l: f64, g: f64, c: f64, len: f64) -> TxlModel {
    TxlModel {
        r: Some(r),
        l: Some(l),
        g: Some(g),
        c: Some(c),
        length: Some(len),
    }
}

#[test]
fn ps_truncates_and_saturates() {
    assert_eq!(ps(2.53e-9), 2530);
    assert_eq!(ps(2.5299999999999997e-9), 2529);
    assert_eq!(ps(-1.5e-12), -1);
    assert_eq!(ps(1.0), i32::MAX);
    assert_eq!(ps(f64::NAN), 0);
}

#[test]
fn set_buffer_keeps_the_last_assignment_of_an_aliased_position() {
    use crate::matrix::Slot;
    let mut b = SetBuf::default();
    b.set(Slot(3), 1.0);
    b.set(Slot(4), 2.0);
    b.set(Slot(3), -5.0);
    b.set_rhs(2, 1.0);
    b.set_rhs(2, 7.0);
    assert_eq!(b.m, vec![(Slot(3), -5.0), (Slot(4), 2.0)]);
    assert_eq!(b.r, vec![(2, 7.0)]);
}

#[test]
fn lossless_txl_is_sqrt_c_over_l() {
    let p = TxlParams::new("y1", &model(0.0, 8.972e-9, 0.0, 0.468e-12, 16.0), Some(0.5)).unwrap();
    assert!(p.line.lsl);
    let tau = (8.972e-9_f64 * 0.468e-12).sqrt() * 0.5;
    assert!((p.line.taul - tau * 1e12).abs() < 1e-9);
    assert!((p.line.sqt_cdl - (0.468e-12_f64 / 8.972e-9).sqrt()).abs() < 1e-15);
    assert_eq!(p.line.h2_aten, 1.0);
    assert_eq!(p.g_dc, 0.0);
}

/// The lossy fit: the residues of `h1` sum to `h1C`, the delay is
/// `sqrt(LC)·len`, and the attenuation `e^{-a0}` is below one.
#[test]
fn lossy_txl_fit_is_consistent() {
    let m = model(12.45, 8.972e-9, 0.0, 0.468e-12, 16.0);
    let p = TxlParams::new("y1", &m, None).unwrap();
    let t = &p.line;
    assert!(!t.lsl);
    let s: f64 = t.h1.iter().map(|x| x.c).sum();
    assert!((s - t.h1c).abs() <= 1e-12 * s.abs());
    let tau = (8.972e-9_f64 * 0.468e-12).sqrt() * 16.0 * 1e12;
    assert!((t.taul - tau).abs() < 1e-6 * tau);
    assert!(t.h2_aten > 0.0 && t.h2_aten < 1.0);
    assert!((t.h3_aten - t.h2_aten * t.sqt_cdl).abs() < 1e-18);
    assert_eq!(p.g_dc, 12.45 * 16.0);
    // Every pole is stable.
    assert!(t.h1.iter().all(|x| x.x < 0.0));
    assert!(t.h2[0].x < 0.0);
}

#[test]
fn txl_setup_refuses_what_the_c_exits_on() {
    let mut m = model(1.0, 1e-9, 0.0, 1e-12, 1.0);
    m.r = None;
    assert!(
        TxlParams::new("y1", &m, None)
            .unwrap_err()
            .contains("series resistance not given")
    );
    let m = model(1.0, 1e-9, 0.0, 1e-12, 0.0);
    assert!(
        TxlParams::new("y1", &m, None)
            .unwrap_err()
            .contains("zero length")
    );
    let m = model(f64::NAN, 1e-9, 0.0, 1e-12, 1.0);
    assert!(TxlParams::new("y1", &m, None).is_err());
    // C = 0: the lossless delay is zero; the C would walk off its
    // history.
    let m = model(0.0, 1e-9, 0.0, 0.0, 1.0);
    assert!(
        TxlParams::new("y1", &m, None)
            .unwrap_err()
            .contains("not positive")
    );
    // A distortionless lossy line divides by G/C − R/L = 0 in `mac`.
    let m = model(1e6, 1e-9, 1e3, 1e-12, 1.0);
    assert!(TxlParams::new("y1", &m, None).is_err());
}

fn cpl_model(n: usize) -> CplModel {
    let tri = n * (n + 1) / 2;
    let diag = |d: f64, o: f64| {
        let mut v = Vec::with_capacity(tri);
        for i in 0..n {
            for j in i..n {
                v.push(if i == j { d } else { o });
            }
        }
        v
    };
    CplModel {
        r: Some(diag(1.0, 0.0)),
        l: Some(diag(3.83e-7, 5.79e-8)),
        g: Some(diag(0.0, 0.0)),
        c: Some(diag(1.15e-10, -1.04e-11)),
        length: Some(0.1),
    }
}

#[test]
fn cpl_setup_bounds_the_conductor_count_and_the_vectors() {
    for n in [0, MAX_CPL_LINES + 1] {
        assert!(CplParams::new("p1", &cpl_model(2), n, None).is_err());
    }
    let mut m = cpl_model(3);
    m.c.as_mut().unwrap().pop();
    let e = CplParams::new("p1", &m, 3, None).unwrap_err();
    assert!(e.contains("6 of the upper triangle"), "{e}");
    // Extra values are ignored, as by the C.
    let mut m = cpl_model(2);
    for v in [&mut m.r, &mut m.l, &mut m.g, &mut m.c] {
        v.as_mut().unwrap().push(9.0);
    }
    assert!(CplParams::new("p1", &m, 2, None).is_ok());
    let mut m = cpl_model(2);
    m.length = None;
    assert!(
        CplParams::new("p1", &m, 2, None)
            .unwrap_err()
            .contains("length")
    );
}

/// The modal delays of a symmetric pair are those of the even and odd
/// modes, `sqrt((L11 ± L12)(C11 ± C12))·len`; `R` is clamped to 1e-4
/// in the DC resistance.
#[test]
fn cpl_modes_of_a_symmetric_pair() {
    let mut m = cpl_model(2);
    m.r = Some(vec![0.0, 0.0, 2.0]);
    let p = CplParams::new("p1", &m, 2, None).unwrap();
    let (l11, l12, c11, c12) = (3.83e-7_f64, 5.79e-8, 1.15e-10, -1.04e-11);
    let even = ((l11 + l12) * (c11 + c12)).sqrt() * 0.1 * 1e12;
    let odd = ((l11 - l12) * (c11 - c12)).sqrt() * 0.1 * 1e12;
    let mut got = p.line.taul.clone();
    got.sort_by(f64::total_cmp);
    let mut want = vec![even, odd];
    want.sort_by(f64::total_cmp);
    for (g, w) in got.iter().zip(&want) {
        assert!((g - w).abs() < 1e-3 * w, "{got:?} vs {want:?}");
    }
    assert_eq!(p.g_dc, vec![1e-4 * 0.1, 2.0 * 0.1]);
    assert!(p.line.h1t.iter().all(Option::is_some));
}

fn engine(deck: &str) -> (Engine, Options) {
    let parsed = dune_netlist::parse_str("t.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    opts.apply(&parsed.netlist.options, &mut Vec::new());
    let built = ckt.build(&opts).unwrap();
    (Engine::new(built), opts)
}

fn tran(tstep: f64, tstop: f64, tmax: Option<f64>) -> TranParams {
    TranParams {
        tstep,
        tstop,
        tstart: 0.0,
        tmax,
        uic: false,
    }
}

/// A short lossy line under a coarse step: the load lowers the step
/// cap to `0.9·τ` (`CKTmaxStep`), and the history only keeps the
/// samples one delay back.
#[test]
fn txl_caps_the_step_and_prunes_its_history() {
    let deck = "t\nv1 in 0 pulse(0 1 0.2n 0.1n 0.1n 2n 10n)\nr1 in a 10\n\
                y1 a 0 b 0 ymod\nr2 b 0 1k\n\
                .model ymod txl R=12.45 L=8.972e-9 G=0 C=0.468e-12 length=1\n\
                .options compat=spice3\n.end\n";
    let (mut e, opts) = engine(deck);
    let rows = e.tran(&opts, &tran(0.1e-9, 10e-9, None)).unwrap();
    let d = &e.devices_for_test()[2];
    let tau = (8.972e-9_f64 * 0.468e-12).sqrt();
    let cap = d.max_step().expect("the step outran the delay");
    assert!(
        (cap - 0.9 * tau).abs() < 1e-6 * tau,
        "{cap} vs {}",
        0.9 * tau
    );
    let hist = d.history_len().unwrap();
    assert!(
        hist < 10 && rows.len() > 100,
        "{hist} samples of {}",
        rows.len()
    );
    // The steps after the cap respect it.
    let late: Vec<f64> = rows.windows(2).skip(20).map(|w| w[1].0 - w[0].0).collect();
    assert!(late.iter().all(|h| *h <= cap * (1.0 + 1e-9)));
}

/// An uncoupled CPL (zero mutual L and C) leaves coupling fits empty:
/// its DC works, its transient is refused at the first non-DC load
/// ("Forbidden combination of model parameters").
#[test]
fn uncoupled_cpl_is_refused_in_transient_only() {
    let deck = "t\nv1 in 0 pulse(0 1 0.2n 0.1n 0.1n 2n 10n)\nr1 in a1 50\nr2 0 a2 50\n\
                p1 a1 a2 0 b1 b2 0 pmod\nr3 b1 0 50\nr4 b2 0 50\n\
                .model pmod cpl length=0.1 R=1 0 1 L=3.83e-7 0 3.83e-7 G=0 0 0 \
                C=1.15e-10 0 1.15e-10\n.end\n";
    let (mut e, opts) = engine(deck);
    e.op(&opts).unwrap();
    let err = e.tran(&opts, &tran(0.02e-9, 1e-9, None)).unwrap_err();
    assert!(err.to_string().contains("forbidden combination"), "{err}");
}

/// The dc setup restarts at every DC load: two transients on one
/// engine give the same waveform.
#[test]
fn txl_state_restarts_with_each_analysis() {
    let deck = "t\nv1 in 0 pulse(0 1 0.2n 0.1n 0.1n 2n 10n)\nr1 in a 50\n\
                y1 a 0 b 0 ymod\nr2 b 0 50\n\
                .model ymod txl R=12.45 L=8.972e-9 G=0 C=0.468e-12 length=16\n\
                .options compat=spice3\n.end\n";
    let (mut e, opts) = engine(deck);
    let p = tran(0.02e-9, 5e-9, Some(0.01e-9));
    let a = e.tran(&opts, &p).unwrap();
    let b = e.tran(&opts, &p).unwrap();
    assert_eq!(a.len(), b.len());
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.0.to_bits(), y.0.to_bits());
        assert_eq!(x.1, y.1);
    }
}

/// Under TR-BDF2 (Dune's profile) the lazy accept of the load appends
/// the intermediate stage point; a step the LTE control then rejects
/// rolls it back (this deck rejects four). The waveform stays close to
/// the trapezoidal run.
#[test]
fn trbdf2_stage_samples_roll_back_with_a_rejected_step() {
    let body = "v1 in 0 pulse(0 3 0.2003n 0.05n 0.05n 2n 10n)\nr1 in a 20\n\
                y1 a 0 b 0 ymod\nd1 b c dmod\nc1 c 0 2p\nr2 c 0 500\n\
                .model dmod d is=1e-14 cjo=1p tt=0.2n\n\
                .model ymod txl R=12.45 L=8.972e-9 G=0 C=0.468e-12 length=16\n";
    let run = |opts: &str| {
        let (mut e, o) = engine(&format!("t\n{body}{opts}.end\n"));
        let rows = e.tran(&o, &tran(0.02e-9, 10e-9, None)).unwrap();
        let c = e.vars.names.iter().position(|n| n == "c").unwrap();
        rows.into_iter().map(|(t, x)| (t, x[c])).collect::<Vec<_>>()
    };
    let dune = run("");
    let spice3 = run(".options compat=spice3\n");
    let at = |w: &[(f64, f64)], t: f64| {
        let k = w.partition_point(|p| p.0 < t).clamp(1, w.len() - 1);
        let (a, b) = (w[k - 1], w[k]);
        a.1 + (b.1 - a.1) * (t - a.0) / (b.0 - a.0)
    };
    let worst = spice3
        .iter()
        .map(|&(t, v)| (at(&dune, t) - v).abs())
        .fold(0.0, f64::max);
    assert!(worst < 0.05, "{worst}");
}
