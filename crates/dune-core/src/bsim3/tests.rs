//! Finite-difference checks of the BSIM3 evaluation, temperature and
//! binning.

use std::sync::Arc;

use super::eval::{InstTemp, evaluate};
use super::*;

/// The Berkeley 0.35 µm NMOS card of `corpus/regress/bsim3_iv.cir`
/// (ngspice-47 `examples/mos/modelcard.nmos`).
const N1: &[(&str, f64)] = &[
    ("tnom", 27.0),
    ("nch", 2.498e17),
    ("tox", 9e-9),
    ("xj", 1e-7),
    ("lint", 9.36e-8),
    ("wint", 1.47e-7),
    ("vth0", 0.6322),
    ("k1", 0.756),
    ("k2", -3.83e-2),
    ("k3", -2.612),
    ("dvt0", 2.812),
    ("dvt1", 0.462),
    ("dvt2", -9.17e-2),
    ("nlx", 3.52291e-8),
    ("w0", 1.163e-6),
    ("k3b", 2.233),
    ("vsat", 86301.58),
    ("ua", 6.47e-9),
    ("ub", 4.23e-18),
    ("uc", -4.706281e-11),
    ("rdsw", 650.0),
    ("u0", 388.3203),
    ("a0", 0.3496967),
    ("ags", 0.1),
    ("b0", 0.546),
    ("b1", 1.0),
    ("dwg", -6.0e-9),
    ("dwb", -3.56e-9),
    ("prwb", -0.213),
    ("keta", -3.605872e-2),
    ("a1", 2.778747e-2),
    ("a2", 0.9),
    ("voff", -6.735529e-2),
    ("nfactor", 1.139926),
    ("cit", 1.622527e-4),
    ("cdsc", -2.147181e-5),
    ("eta0", 1.0281729e-2),
    ("etab", -5.042203e-3),
    ("dsub", 0.31871233),
    ("pclm", 1.114846),
    ("pdiblc1", 2.45357e-3),
    ("pdiblc2", 6.406289e-3),
    ("drout", 0.31871233),
    ("pscbe1", 5_000_000.0),
    ("pscbe2", 5e-9),
    ("pdiblcb", -0.234),
    ("ww", -1.420242e-9),
    ("wwn", 0.2613948),
    ("ll", 1.300902e-10),
    ("lln", 0.316394),
    ("kt1", -0.3),
    ("kt2", -0.051),
    ("at", 22400.0),
    ("ute", -1.48),
    ("ua1", 3.31e-10),
    ("ub1", 2.61e-19),
    ("uc1", -3.42e-10),
    ("prt", 764.3),
    ("alpha0", 1e-6),
    ("beta0", 20.0),
];

fn card(extra: &[(&str, f64)]) -> Bsim3Model {
    let mut m = Bsim3Model::new(true);
    for &(k, v) in N1.iter().chain(extra) {
        m.set(k, v).unwrap();
    }
    m
}

struct Dev {
    m: Bsim3Model,
    mt: Bsim3MTemp,
    pp: Bsim3Size,
    it: InstTemp,
}

fn dev(extra: &[(&str, f64)], l: f64, w: f64, temp: f64) -> Dev {
    let mut m = card(extra);
    m.setup(300.15);
    let mut notes = Vec::new();
    let mt = Bsim3MTemp::new(&mut m, temp, &mut notes);
    let pp = Bsim3Size::new("m1", &m, &mt, l, w, &mut notes).unwrap();
    let it = InstTemp {
        vth0: pp.vth0,
        vfb: pp.vfb,
        vfbzb: pp.vfbzb,
        u0temp: pp.u0temp,
    };
    Dev { m, mt, pp, it }
}

fn rel_close(a: f64, b: f64, scale: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * f64::max(a.abs().max(b.abs()), scale)
}

/// gm, gds, gmbs and the substrate-current derivatives against
/// central differences of `Ids`/`Isub`, across subthreshold, linear
/// and saturation and a forward/reverse body bias.
#[test]
fn conductances_match_finite_differences() {
    let d = dev(&[], 0.35e-6, 2e-6, 300.15);
    let ev =
        |vds: f64, vgs: f64, vbs: f64| evaluate(&d.m, &d.mt, &d.pp, &d.it, vds, vgs, vbs, false);
    let h = 1e-6;
    for &(vds, vgs, vbs) in &[
        (0.05, 1.5, 0.0),
        (2.0, 1.5, 0.0),
        (1.0, 0.3, -1.0),
        (3.0, 2.5, -2.0),
        (0.5, 0.9, 0.2),
        (1.5, 0.55, -0.5),
    ] {
        let e = ev(vds, vgs, vbs);
        let fd = |f: &dyn Fn(f64, f64, f64) -> f64, k: usize| {
            let (mut p, mut q) = ([vds, vgs, vbs], [vds, vgs, vbs]);
            p[k] += h;
            q[k] -= h;
            (f(p[0], p[1], p[2]) - f(q[0], q[1], q[2])) / (2.0 * h)
        };
        let ids = |a, b, c| ev(a, b, c).ids;
        let isub = |a, b, c| ev(a, b, c).isub;
        let s = e.ids.abs() * 1e-3;
        assert!(
            rel_close(e.gds, fd(&ids, 0), s, 1e-4),
            "gds at {vds},{vgs},{vbs}: {} vs {}",
            e.gds,
            fd(&ids, 0)
        );
        assert!(
            rel_close(e.gm, fd(&ids, 1), s, 1e-4),
            "gm at {vds},{vgs},{vbs}: {} vs {}",
            e.gm,
            fd(&ids, 1)
        );
        assert!(
            rel_close(e.gmbs, fd(&ids, 2), s, 1e-4),
            "gmbs at {vds},{vgs},{vbs}: {} vs {}",
            e.gmbs,
            fd(&ids, 2)
        );
        let s = e.isub.abs() * 1e-3 + 1e-18;
        assert!(
            rel_close(e.gbds, fd(&isub, 0), s, 1e-3),
            "gbds at {vds},{vgs},{vbs}"
        );
        assert!(
            rel_close(e.gbgs, fd(&isub, 1), s, 1e-3),
            "gbgs at {vds},{vgs},{vbs}"
        );
        assert!(
            rel_close(e.gbbs, fd(&isub, 2), s, 1e-3),
            "gbbs at {vds},{vgs},{vbs}"
        );
    }
}

/// The capacitance matrix of every capMod against differences of
/// the terminal charges (`Cxyb = ∂Qx/∂Vy`, `∂/∂Vs = −Σ∂/∂v·s`).
#[test]
fn capacitances_match_charge_derivatives() {
    for capmod in 0..=3 {
        for xpart in [0.0, 0.5, 1.0] {
            let d = dev(
                &[("capmod", capmod as f64), ("xpart", xpart)],
                0.5e-6,
                4e-6,
                300.15,
            );
            let ev = |vds: f64, vgs: f64, vbs: f64| {
                evaluate(&d.m, &d.mt, &d.pp, &d.it, vds, vgs, vbs, true)
            };
            let h = 1e-6;
            for &(vds, vgs, vbs) in &[
                (0.1, 2.0, -0.05),
                (2.0, 1.5, -0.5),
                (1.0, 0.2, -1.0),
                (1.5, -0.5, -0.05),
                (0.7, 1.2, 0.2),
            ] {
                let e = ev(vds, vgs, vbs);
                let scale = d.m.cox * d.pp.weff_cv * d.pp.leff_cv * 1e-3;
                let q = |sel: usize, a: f64, b: f64, c: f64| {
                    let e = ev(a, b, c);
                    [e.qgate, e.qdrn, e.qbulk][sel]
                };
                let dv = |sel: usize, k: usize| {
                    let (mut p, mut m) = ([vds, vgs, vbs], [vds, vgs, vbs]);
                    p[k] += h;
                    m[k] -= h;
                    (q(sel, p[0], p[1], p[2]) - q(sel, m[0], m[1], m[2])) / (2.0 * h)
                };
                // (g, d, s) columns from (vds, vgs, vbs) partials.
                let cols = |sel: usize| {
                    let (dd, dg, db) = (dv(sel, 0), dv(sel, 1), dv(sel, 2));
                    (dg, dd, -(dd + dg + db))
                };
                let tag = format!("capmod {capmod} xpart {xpart} at {vds},{vgs},{vbs}");
                // capMod 3 keeps a C quirk: in the linear-region branch
                // of VdseffCV, `dVdseffCV_dVb = dT0_dVb * (1.0 - T5) +
                // T5 * dT1_dVb` drops the `-T3` of `T4 = 1 - T3` that
                // the Vg derivative has (`b3ld.c`; BSIM4 uses
                // `T4 - T5`), so the source columns (which carry the
                // body derivative) are off by up to ~1e-3·CoxWL.
                let (tol_s, scale_s) = if capmod == 3 {
                    (2e-3, scale * 1e3)
                } else {
                    (2e-3, scale)
                };
                let (gg, gd, gs) = cols(0);
                assert!(
                    rel_close(e.cggb, gg, scale, 2e-3),
                    "cggb {tag}: {} vs {gg}",
                    e.cggb
                );
                assert!(
                    rel_close(e.cgdb, gd, scale, 2e-3),
                    "cgdb {tag}: {} vs {gd}",
                    e.cgdb
                );
                assert!(
                    rel_close(e.cgsb, gs, scale_s, tol_s),
                    "cgsb {tag}: {} vs {gs}",
                    e.cgsb
                );
                let (bg, bd, bs) = cols(2);
                assert!(
                    rel_close(e.cbgb, bg, scale, 2e-3),
                    "cbgb {tag}: {} vs {bg}",
                    e.cbgb
                );
                assert!(
                    rel_close(e.cbdb, bd, scale, 2e-3),
                    "cbdb {tag}: {} vs {bd}",
                    e.cbdb
                );
                assert!(
                    rel_close(e.cbsb, bs, scale_s, tol_s),
                    "cbsb {tag}: {} vs {bs}",
                    e.cbsb
                );
                let (dg, ddd, ds) = cols(1);
                assert!(
                    rel_close(e.cdgb, dg, scale, 2e-3),
                    "cdgb {tag}: {} vs {dg}",
                    e.cdgb
                );
                assert!(
                    rel_close(e.cddb, ddd, scale, 2e-3),
                    "cddb {tag}: {} vs {ddd}",
                    e.cddb
                );
                assert!(
                    rel_close(e.cdsb, ds, scale_s, tol_s),
                    "cdsb {tag}: {} vs {ds}",
                    e.cdsb
                );
            }
        }
    }
}

/// `BSIM3temp` at TNOM and at 125 °C: U0, VSAT, RDS0 and the
/// junction density follow UTE/AT/PRT/XTI.
#[test]
fn temperature_scaling() {
    let cold = dev(&[("js", 1e-4), ("jsw", 1e-10)], 0.35e-6, 2e-6, 300.15);
    let hot = dev(&[("js", 1e-4), ("jsw", 1e-10)], 0.35e-6, 2e-6, 398.15);
    let tratio: f64 = 398.15 / 300.15;
    assert!((cold.mt.t_ratio - 1.0).abs() < 1e-15);
    // u0 given in cm²/V/s is converted to m²/V/s.
    assert!((cold.pp.u0temp - 388.3203e-4).abs() < 1e-12);
    assert!((hot.pp.u0temp - 388.3203e-4 * tratio.powf(-1.48)).abs() < 1e-12);
    assert!((hot.pp.vsattemp - (86301.58 - 22400.0 * (tratio - 1.0))).abs() < 1e-6);
    let wr = (cold.pp.weff * 1e6).powf(1.0);
    assert!((cold.pp.rds0 - 650.0 / wr).abs() < 1e-9);
    assert!((hot.pp.rds0 - (650.0 + 764.3 * (tratio - 1.0)) / wr).abs() < 1e-9);
    assert_eq!(cold.mt.jct_temp_sat_cur_density, 1e-4);
    assert!(hot.mt.jct_temp_sat_cur_density > 1e3 * 1e-4);
    // The threshold drops with temperature (KT1 < 0).
    let ic = evaluate(&cold.m, &cold.mt, &cold.pp, &cold.it, 0.1, 1.0, 0.0, false);
    let ih = evaluate(&hot.m, &hot.mt, &hot.pp, &hot.it, 0.1, 1.0, 0.0, false);
    assert!(ih.von < ic.von - 0.02);
}

/// Binning: `P = P0 + PL/Leff + PW/Weff + PP/(Leff·Weff)` in µm
/// units, and the knot list shares one size set per drawn (L, W).
#[test]
fn binning_and_knot_cache() {
    let extra = [
        ("lvth0", 0.02),
        ("wvth0", -0.01),
        ("pvth0", 0.005),
        ("lu0", 10.0),
    ];
    let d = dev(&extra, 0.4e-6, 1e-6, 300.15);
    let (il, iw) = (1e-6 / d.pp.leff, 1e-6 / d.pp.weff);
    let want = 0.6322 + 0.02 * il - 0.01 * iw + 0.005 * il * iw;
    assert!((d.pp.vth0 - want).abs() < 1e-12, "{} vs {want}", d.pp.vth0);
    // u0 binned before the cm²→m² conversion.
    assert!((d.pp.u0 - (388.3203 + 10.0 * il) / 1e4).abs() < 1e-12);
    // binunit=0: the L/W terms take meters.
    let d0 = dev(&[("lvth0", 2e-8), ("binunit", 0.0)], 0.4e-6, 1e-6, 300.15);
    assert!((d0.pp.vth0 - (0.6322 + 2e-8 / d0.pp.leff)).abs() < 1e-12);

    let given = card(&extra);
    let mut list = Vec::new();
    let mut notes = Vec::new();
    let a = Bsim3Knots::resolve(
        &mut list, "m1", &given, 0.4e-6, 1e-6, 300.15, 300.15, &mut notes,
    )
    .unwrap();
    let b = Bsim3Knots::resolve(
        &mut list, "m2", &given, 0.4e-6, 1e-6, 300.15, 300.15, &mut notes,
    )
    .unwrap();
    let c = Bsim3Knots::resolve(
        &mut list, "m3", &given, 0.8e-6, 1e-6, 300.15, 300.15, &mut notes,
    )
    .unwrap();
    assert_eq!(list.len(), 1);
    assert!(Arc::ptr_eq(&a.2, &b.2));
    assert!(!Arc::ptr_eq(&a.2, &c.2));
    assert!(Arc::ptr_eq(&a.0, &c.0));
    // Effective sizes: Leff = L − 2·(LINT + LL/L^LLN).
    let dl = 9.36e-8 + 1.300902e-10 / 0.4e-6f64.powf(0.316394);
    assert!((a.2.leff - (0.4e-6 - 2.0 * dl)).abs() < 1e-18);
}

/// `b3set.c` defaults and the given-dependent ones.
#[test]
fn setup_defaults() {
    let mut m = Bsim3Model::new(false);
    m.set("tox", 1e-8).unwrap();
    m.setup(300.15);
    assert_eq!(m.cap_mod, 3);
    assert_eq!(m.mob_mod, 1);
    assert_eq!(m.noi_mod, 1);
    assert_eq!(m.vth0, -0.7);
    assert_eq!(m.u0, 0.025);
    assert_eq!(m.dsub, m.drout);
    assert_eq!(m.toxm, 1e-8);
    assert!((m.cox - 3.453133e-11 / 1e-8).abs() < 1e-18);
    assert!((m.cgdo - 0.6 * 0.15e-6 * m.cox).abs() < 1e-24);
    assert_eq!(m.oxide_trap_density_a, 9.9e18);
    assert_eq!(m.get("tnom"), Some(27.0));
    assert!(m.set("nonsense", 1.0).is_err());
    // vtho is an alias of vth0.
    m.set("vtho", -0.5).unwrap();
    assert_eq!(m.get("vth0"), Some(-0.5));
}
