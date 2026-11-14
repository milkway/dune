//! Unit tests of the VBIC port: the keyword table, the setup
//! defaults, the temperature mapping and the device equations.

use super::fj::{FjBias, vbic_4t_et_cf_fj};
use super::model::{NUM_PARAMS, P_CTH, P_RTH, load_block, temp_map};
use super::*;

#[test]
fn keywords_and_aliases_reach_the_c_slots() {
    let mut m = VbicModel::new(true);
    m.set("v0", 0.5).unwrap();
    assert_eq!(m.get("vo"), Some(0.5));
    m.set("cbe0", 1e-15).unwrap();
    assert_eq!(m.get("cbeo"), Some(1e-15));
    m.set("tref", 50.0).unwrap();
    assert_eq!(m.get("tnom"), Some(50.0));
    m.set("bvbe", 5.0).unwrap();
    assert_eq!(m.get("vbe_max"), Some(5.0));
    m.set("pnp", 1.0).unwrap();
    assert!(!m.npn);
    assert!(m.set("bf", 100.0).is_err());
    assert_eq!(m.get("bf"), None);
}

#[test]
fn defaults_are_vbicsetup_s() {
    let m = VbicModel::new(true);
    assert_eq!(m.get("tnom"), Some(27.0));
    assert_eq!(m.get("rci"), Some(0.1));
    assert_eq!(m.get("is"), Some(1e-16));
    assert_eq!(m.get("ibei"), Some(1e-18));
    assert_eq!(m.get("aje"), Some(-0.5));
    assert_eq!(m.get("nkf"), Some(0.5));
    assert_eq!(m.get("vers"), Some(1.2));
    assert_eq!(m.get("vsubfwd"), Some(0.2));
    assert!(!m.selfheat());
    assert!(!m.excess_phase());
}

#[test]
fn given_resistances_are_floored_at_r_min() {
    let mut m = VbicModel::new(true);
    m.set("rcx", 0.0).unwrap();
    assert_eq!(m.get("rcx"), Some(0.01));
    m.set("rbp", -3.0).unwrap();
    assert_eq!(m.get("rbp"), Some(0.01));
    // Not a resistance: no floor.
    m.set("vo", 0.0).unwrap();
    assert_eq!(m.get("vo"), Some(0.0));
}

#[test]
fn self_heating_switch_follows_vbicsetup() {
    let mut m = VbicModel::new(true);
    m.set("rth", 100.0).unwrap();
    assert!(m.selfheat());
    // CTH below 1e-12 is raised when RTH is given.
    assert_eq!(m.block()[P_CTH], 1e-12);
    m.set("selft", 0.0).unwrap();
    assert!(!m.selfheat());
    m.set("selft", 1.0).unwrap();
    assert!(m.selfheat());
    let mut z = VbicModel::new(true);
    z.set("rth", 0.0).unwrap();
    assert!(!z.selfheat());
    assert_eq!(z.block()[P_RTH], 0.0);
}

#[test]
fn temperature_mapping_is_identity_at_tnom() {
    let mut m = VbicModel::new(true);
    m.set("cje", 1e-12).unwrap();
    m.set("iben", 1e-15).unwrap();
    m.set("rcx", 10.0).unwrap();
    m.set("xrcx", 1.5).unwrap();
    let pnom = m.block();
    let p = temp_map(&pnom, 27.0);
    for k in [1usize, 11, 16, 31, 34, 36] {
        assert!(
            (p[k] - pnom[k]).abs() <= 1e-12 * pnom[k].abs(),
            "p[{k}] = {} vs {}",
            p[k],
            pnom[k]
        );
    }
    // The built-in potential survives the round trip to a few ulps.
    assert!((p[17] - pnom[17]).abs() < 1e-12);
    // Hotter: IS grows, RCX follows rT^XRCX.
    let hot = temp_map(&pnom, 127.0);
    assert!(hot[11] > 10.0 * pnom[11]);
    let r_t: f64 = (127.0 + 273.15) / (27.0 + 273.15);
    assert!((hot[1] - 10.0 * r_t.powf(1.5)).abs() < 1e-12);
}

#[test]
fn load_block_keeps_the_card_ebbe() {
    let mut m = VbicModel::new(true);
    m.set("vbbe", 5.0).unwrap();
    let p = load_block(&m, 300.15);
    // `VBICtemp` drops the mapped EBBE: the load sees the card's 0.
    assert_eq!(p[104], 0.0);
    assert!((p[0] - 27.0).abs() < 1e-12);
}

fn bias(vbei: f64, vbci: f64) -> FjBias {
    FjBias {
        Vbei: vbei,
        Vbex: vbei,
        Vbci: vbci,
        Vbcx: vbci,
        Vbe: vbei,
        Vbc: vbci,
        Vcei: vbei - vbci,
        ..Default::default()
    }
}

#[test]
fn forward_active_currents_and_derivatives() {
    let m = VbicModel::new(true);
    let p = load_block(&m, 300.15);
    let b = bias(0.7, -2.0);
    let o = vbic_4t_et_cf_fj(&p, &b, 1.0);
    // Ideal transport current IS·exp(Vbei/(NF·Vtv)) with the
    // default (no Early, no knee) qb = 1 up to the smoothing of q1.
    let vtv = 1.380662e-23 * 300.15 / 1.602189e-19;
    let ifi = 1e-16 * ((0.7f64 / vtv).exp() - 1.0);
    assert!((o.Itzf - ifi).abs() < 1e-7 * ifi, "{} vs {ifi}", o.Itzf);
    // dItzf/dVbei by central difference.
    let h = 1e-6;
    let up = vbic_4t_et_cf_fj(&p, &bias(0.7 + h, -2.0), 1.0).Itzf;
    let dn = vbic_4t_et_cf_fj(&p, &bias(0.7 - h, -2.0), 1.0).Itzf;
    let num = (up - dn) / (2.0 * h);
    assert!((o.Itzf_Vbei - num).abs() < 1e-5 * num.abs());
    // SCALE multiplies every output.
    let o2 = vbic_4t_et_cf_fj(&p, &b, 2.5);
    assert!((o2.Ibe - 2.5 * o.Ibe).abs() <= 1e-15 * o.Ibe.abs());
    assert_eq!(p.len(), NUM_PARAMS);
}
