//! Finite-difference checks of the translated BSIM4 load: the stamped
//! Jacobian against central differences of the terminal currents the
//! same load stamps (I = G·v − rhs), at several bias points and for
//! several selector combinations; plus the version policy and the
//! parameter table.

use std::sync::Arc;

use super::*;
use crate::devices::Device;
use crate::matrix::{LoadEnv, StampSink};
use crate::state::States;

/// The NMOS card of `corpus/regress/bsim4_idvg.cir`, as (keyword,
/// value).
const NCH: &[(&str, f64)] = &[
    ("toxe", 4.1e-9),
    ("toxp", 3.4e-9),
    ("toxm", 4.1e-9),
    ("dtox", 0.7e-9),
    ("wint", 5e-9),
    ("lint", 1e-8),
    ("dlc", 1.5e-8),
    ("vth0", 0.42),
    ("k1", 0.55),
    ("k2", -0.02),
    ("k3", 1.5),
    ("k3b", 0.5),
    ("w0", 2.5e-6),
    ("dvt0", 1.1),
    ("dvt1", 0.55),
    ("dvt2", -0.03),
    ("dsub", 0.3),
    ("minv", 0.05),
    ("dvtp0", 1e-10),
    ("dvtp1", 0.1),
    ("lpe0", 6e-8),
    ("ndep", 2.5e17),
    ("ngate", 1e23),
    ("voff", -0.09),
    ("nfactor", 1.2),
    ("eta0", 0.08),
    ("etab", -0.05),
    ("u0", 0.032),
    ("ua", 1.0e-9),
    ("ub", 1.5e-18),
    ("uc", 5e-11),
    ("vsat", 1.1e5),
    ("a0", 1.2),
    ("ags", 0.15),
    ("keta", -0.03),
    ("pclm", 1.2),
    ("pdiblc1", 0.2),
    ("pdiblc2", 0.01),
    ("drout", 0.6),
    ("pvag", 0.5),
    ("pscbe1", 8e8),
    ("pscbe2", 1e-5),
    ("fprout", 0.2),
    ("pdits", 0.1),
    ("pditsd", 0.2),
    ("rdsw", 150.0),
    ("prwg", 0.1),
    ("prwb", -0.1),
    ("alpha0", 1e-6),
    ("alpha1", 0.1),
    ("beta0", 20.0),
    ("agidl", 1e-9),
    ("bgidl", 2.3e9),
    ("cgidl", 0.5),
    ("egidl", 0.8),
    ("jss", 1e-6),
    ("jsws", 1e-11),
    ("jswgs", 1e-11),
    ("cjs", 1e-3),
    ("cjsws", 2e-10),
    ("cjswgs", 2e-10),
];

/// A set-up, temperature-processed instance on external nodes 1..4
/// (drain, gate, source, bulk) with its internal nodes after them.
fn instance(extra: &[(&str, f64)], inst: &[(&str, f64)]) -> (Bsim4I, usize) {
    let mut given = Bsim4Model::new(true);
    given.set_name("nch");
    given.set_version("4.8.3");
    for &(k, v) in NCH.iter().chain(extra) {
        given.set(k, v).unwrap();
    }
    let mut notes = Vec::new();
    let md = Bsim4Knots::setup_model(&given, 300.15, &mut notes);
    let mut params: Vec<(String, f64)> = vec![
        ("l".into(), 0.18e-6),
        ("w".into(), 2e-6),
        ("ad".into(), 1e-12),
        ("as".into(), 1e-12),
        ("pd".into(), 5e-6),
        ("ps".into(), 5e-6),
    ];
    params.extend(inst.iter().map(|&(k, v)| (k.to_string(), v)));
    let mut h = new_here(&md, &params, false, &mut notes).unwrap();
    let internal = internal_nodes(&md, &h, false);
    let n = 4 + internal.len();
    assign_nodes(&mut h, [1, 2, 3, 4], |s| {
        internal.iter().position(|x| *x == s).map(|i| 5 + i)
    });
    let mut list = Vec::new();
    let (model, pp) =
        Bsim4Knots::resolve(&mut list, "m1", &given, &mut h, 300.15, 300.15, &mut notes).unwrap();
    (Bsim4I::new(h, model, pp, 0, 300.15), n)
}

/// One DC load at node voltages `v` (index 0 = ground) with the state
/// vector converged onto `v` (so no limiting): (dense G, rhs).
fn load_at(
    dev: &mut Bsim4I,
    slots: &[(usize, usize)],
    n: usize,
    v: &[f64],
) -> (Vec<Vec<f64>>, Vec<f64>) {
    let mut states = States::new(BSIM4_NUM_STATES);
    let mut out = (Vec::new(), Vec::new());
    for _ in 0..4 {
        let mut values = vec![0.0; slots.len() + 1];
        let mut rhs = vec![0.0; n + 1];
        let env = LoadEnv {
            sol: v,
            mode: InitMode::Float,
            kind: AnKind::DcOp,
            uic: false,
            time: 0.0,
            step: 0.0,
            final_time: 0.0,
            gmin: 1e-12,
            reltol: 1e-3,
            abstol: 1e-12,
            srcfact: 1.0,
            ag: [0.0; 7],
            order: 1,
            method: crate::integ::Method::Trapezoidal,
            pred_xfact: 0.0,
            delta: 0.0,
        };
        let mut ctx = env.ctx(
            StampSink::Direct {
                values: &mut values,
                rhs: &mut rhs,
            },
            states.view(),
        );
        dev.load(&mut ctx);
        let mut g = vec![vec![0.0; n + 1]; n + 1];
        for (k, &(r, c)) in slots.iter().enumerate() {
            g[r + 1][c + 1] += values[k + 1];
        }
        out = (g, rhs);
    }
    out
}

/// The terminal currents I = G·v − rhs of a load at `v`.
fn currents(dev: &mut Bsim4I, slots: &[(usize, usize)], n: usize, v: &[f64]) -> Vec<f64> {
    let (g, rhs) = load_at(dev, slots, n, v);
    (0..=n)
        .map(|i| (1..=n).map(|j| g[i][j] * v[j]).sum::<f64>() - rhs[i])
        .collect()
}

/// Worst relative mismatch between the stamped Jacobian and central
/// differences of the currents, over the given bias points (drain,
/// gate, source, bulk).
fn jacobian_mismatch(extra: &[(&str, f64)], inst: &[(&str, f64)], biases: &[[f64; 4]]) -> f64 {
    let (mut dev, n) = instance(extra, inst);
    let mut mb = MatrixBuilder::new(n);
    dev.alloc(&mut mb);
    let slots = mb.entries().to_vec();
    let mut worst: f64 = 0.0;
    for b in biases {
        let mut v = vec![0.0; n + 1];
        v[1..5].copy_from_slice(b);
        // Internal nodes: settle them by a few Newton steps on the
        // device alone with the external nodes held.
        for _ in 0..40 {
            if n == 4 {
                break;
            }
            let (g, rhs) = load_at(&mut dev, &slots, n, &v);
            let m = n - 4;
            let mut a = vec![vec![0.0; m + 1]; m];
            for i in 0..m {
                for j in 0..m {
                    a[i][j] = g[5 + i][5 + j];
                }
                let mut r = rhs[5 + i];
                for j in 1..5 {
                    r -= g[5 + i][j] * v[j];
                }
                a[i][m] = r;
            }
            let x = solve(a);
            v[5..(m + 5)].copy_from_slice(&x[..m]);
        }
        let (g, _) = load_at(&mut dev, &slots, n, &v);
        for j in 1..=n {
            let h = 1e-6;
            let mut vp = v.clone();
            vp[j] += h;
            let mut vm = v.clone();
            vm[j] -= h;
            let ip = currents(&mut dev, &slots, n, &vp);
            let im = currents(&mut dev, &slots, n, &vm);
            for i in 1..=n {
                let fd = (ip[i] - im[i]) / (2.0 * h);
                let scale = g[i].iter().map(|x| x.abs()).fold(0.0, f64::max).max(1e-9);
                let err = (fd - g[i][j]).abs() / scale;
                worst = worst.max(err);
            }
        }
    }
    worst
}

/// Gaussian elimination with partial pivoting on an augmented matrix.
fn solve(mut a: Vec<Vec<f64>>) -> Vec<f64> {
    let m = a.len();
    for c in 0..m {
        let p = (c..m)
            .max_by(|&x, &y| a[x][c].abs().partial_cmp(&a[y][c].abs()).unwrap())
            .unwrap();
        a.swap(c, p);
        for r in 0..m {
            if r != c {
                let f = a[r][c] / a[c][c];
                for k in c..=m {
                    a[r][k] -= f * a[c][k];
                }
            }
        }
    }
    (0..m).map(|i| a[i][m] / a[i][i]).collect()
}

const BIASES: &[[f64; 4]] = &[
    [1.2, 1.0, 0.0, 0.0],
    [0.05, 0.8, 0.0, -0.4],
    [1.8, 0.3, 0.0, 0.0],
    [0.6, 1.5, 0.0, 0.2],
    [-0.4, 1.0, 0.0, 0.0],
    [1.0, -0.5, 0.0, -0.3],
];

#[test]
fn jacobian_default_selectors() {
    let e = jacobian_mismatch(&[], &[], BIASES);
    assert!(e < 1e-4, "worst relative Jacobian mismatch {e}");
}

#[test]
fn jacobian_gate_tunnelling_and_gidl() {
    let extra = [
        ("igcmod", 1.0),
        ("toxe", 1.6e-9),
        ("toxp", 1.2e-9),
        ("toxm", 1.6e-9),
        ("dtox", 0.4e-9),
        ("toxref", 1.6e-9),
        ("gidlmod", 1.0),
    ];
    let e = jacobian_mismatch(&extra, &[], BIASES);
    assert!(e < 1e-4, "worst relative Jacobian mismatch {e}");
}

/// IGBMOD 1: `b4ld.c` builds Vaux of Igbacc from Vbseff but takes its
/// bulk derivative as if dVbseff/dVb were 1 (and Igbinv's from
/// Voxdepinv likewise), so the stamped Jacobian is only approximate —
/// ported literally (docs/bsim4.md, "Fidelidade"). Measured worst
/// 9.1e-4 of the row scale.
#[test]
fn jacobian_igb_approximate_like_the_c() {
    let extra = [
        ("igbmod", 1.0),
        ("toxe", 1.6e-9),
        ("toxp", 1.2e-9),
        ("toxm", 1.6e-9),
        ("dtox", 0.4e-9),
        ("toxref", 1.6e-9),
    ];
    let e = jacobian_mismatch(&extra, &[], BIASES);
    assert!(e < 2e-3, "worst relative Jacobian mismatch {e}");
}

#[test]
fn jacobian_mobility_models() {
    for mob in 1..=6 {
        let e = jacobian_mismatch(
            &[("mobmod", mob as f64), ("ud", 0.1), ("ucs", 1.5)],
            &[],
            BIASES,
        );
        assert!(
            e < 1e-4,
            "mobmod {mob}: worst relative Jacobian mismatch {e}"
        );
    }
}

#[test]
fn jacobian_internal_nodes() {
    // RDSMOD 1 (bias-dependent external Rd/Rs), RGATEMOD 2 and 3,
    // RBODYMOD 1: internal nodes settled before differencing.
    let cases: [(&[(&str, f64)], &[(&str, f64)]); 4] = [
        (&[("rdsmod", 1.0), ("rdw", 100.0), ("rsw", 100.0)], &[]),
        (&[("rshg", 10.0)], &[("rgatemod", 2.0), ("nf", 2.0)]),
        (&[("rshg", 10.0)], &[("rgatemod", 3.0), ("nf", 2.0)]),
        (&[], &[("rbodymod", 1.0)]),
    ];
    for (extra, inst) in cases {
        let e = jacobian_mismatch(extra, inst, BIASES);
        assert!(
            e < 1e-4,
            "{extra:?} {inst:?}: worst relative Jacobian mismatch {e}"
        );
    }
}

#[test]
fn version_policy() {
    assert!(older_version("4.5"));
    assert!(older_version("4.7.0"));
    assert!(older_version("4.0"));
    assert!(!older_version("4.8.3"));
    assert!(!older_version("3.3"));
    let mut m = Bsim4Model::new(true);
    m.set_version("4.5");
    let mut notes = Vec::new();
    let s = Bsim4Knots::setup_model(&m, 300.15, &mut notes);
    assert_eq!(s.v48int_version, 483);
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn integer_selectors_round_like_ngspice() {
    let mut m = Bsim4Model::new(true);
    m.set("capmod", 1.6).unwrap();
    assert_eq!(m.cap_mod, 2);
    assert!(m.set("nosuchparameter", 1.0).is_err());
    // b4mpar.c: NDEP above 1e20 is taken in cm^-3.
    m.set("ndep", 3e23).unwrap();
    assert_eq!(m.ndep, 3e17);
    let _ = Arc::new(m);
}
