//! VDMOS unit tests: the card defaults, the temperature update and
//! the helper functions against their C definitions, and small
//! decks against closed forms.

use super::model::{VdmosTemp, dev_cap, limitlog};
use super::*;
use crate::consts::REFTEMP;
use crate::{Circuit, Options};

fn prepared(m: &VdmosModel) -> VdmosModel {
    m.prepare("m1", REFTEMP, &mut Vec::new()).unwrap()
}

#[test]
fn setup_defaults_follow_the_channel_type() {
    let n = prepared(&VdmosModel::new(true));
    assert_eq!((n.kp, n.vto), (Some(20.0), Some(3.0)));
    let p = prepared(&VdmosModel::new(false));
    assert_eq!((p.kp, p.vto), (Some(10.0), Some(-3.0)));
    assert_eq!(n.tnom, Some(REFTEMP));
    // Clamps and the fatal PHI check of `VDMOSsetup`.
    let mut m = VdmosModel::new(true);
    m.set("eg", 0.05).unwrap();
    m.set("fc", 0.99).unwrap();
    m.set("is", 0.0).unwrap();
    let mut notes = Vec::new();
    let c = m.prepare("m1", REFTEMP, &mut notes).unwrap();
    assert_eq!((c.eg, c.fc, c.is), (0.1, 0.95, 1e-28));
    assert_eq!(notes.len(), 2);
    m.set("phi", 0.0).unwrap();
    assert!(m.prepare("m1", REFTEMP, &mut notes).is_err());
}

#[test]
fn keyword_table_aliases_and_unknowns() {
    let mut m = VdmosModel::new(true);
    m.set("vth0", 2.5).unwrap();
    m.set("vtotc", -4e-3).unwrap();
    m.set("bex", -1.2).unwrap();
    m.set("m", 0.4).unwrap();
    m.set("vdmosp", 1.0).unwrap();
    assert_eq!(m.vto, Some(2.5));
    assert_eq!((m.tcvth, m.mu, m.mj), (-4e-3, -1.2, 0.4));
    assert!(!m.nmos);
    assert!(m.set("nonsense", 1.0).is_err());
    // Quasi-saturation needs both RQ and VQ.
    m.set("rq", 1.0).unwrap();
    assert!(!m.qs());
    m.set("vq", 2.0).unwrap();
    assert!(m.qs());
}

#[test]
fn temperature_update_collapses_at_tnom() {
    let mut m = VdmosModel::new(true);
    for (k, v) in [
        ("kp", 8.0),
        ("vto", 2.7),
        ("rd", 0.2),
        ("rs", 0.1),
        ("rg", 4.0),
    ] {
        m.set(k, v).unwrap();
    }
    let p = prepared(&m);
    let t = VdmosTemp::new(&p, 2.0, REFTEMP, 1e-3, &mut Vec::new(), "m1");
    assert!((t.t_transconductance - 16.0).abs() < 1e-12);
    assert!((t.t_vth - 2.7).abs() < 1e-15);
    assert!((t.drain_resistance - 0.1).abs() < 1e-15);
    assert!((t.source_conductance - 20.0).abs() < 1e-12);
    assert!((t.gate_conductance - 0.5).abs() < 1e-15);
    assert!((t.t_phi - 0.6).abs() < 1e-12);
    assert!((t.t_jct_pot - 0.8).abs() < 1e-12);
    assert!((t.t_sat_cur - 2e-14).abs() < 1e-26);
    // KP follows (T/TNOM)^MU, VTO is linear in TCVTH.
    m.set("tcvth", -5e-3).unwrap();
    let p = prepared(&m);
    let t = VdmosTemp::new(&p, 1.0, REFTEMP + 50.0, 1e-3, &mut Vec::new(), "m1");
    let ratio: f64 = (REFTEMP + 50.0) / REFTEMP;
    assert!((t.t_transconductance - 8.0 * ratio.powf(-1.5)).abs() < 1e-12);
    assert!((t.t_vth - (2.7 - 0.25)).abs() < 1e-12);
}

#[test]
fn breakdown_knee_matches_ibv() {
    let mut m = VdmosModel::new(true);
    m.set("bv", 50.0).unwrap();
    m.set("ibv", 1e-3).unwrap();
    m.set("nbv", 1.3).unwrap();
    let p = prepared(&m);
    let t = VdmosTemp::new(&p, 1.0, REFTEMP, 1e-3, &mut Vec::new(), "m1");
    let vt = crate::consts::KOVERQ * REFTEMP;
    let x = t.t_brkdwn_v;
    let xcbv = t.t_sat_cur * (f64::exp((50.0 - x) / (1.3 * vt)) - 1.0 + x / vt);
    assert!((xcbv - 1e-3).abs() <= 1e-3 * 1e-3, "{xcbv}");
}

#[test]
fn gate_drain_capacitance_spans_cgdmin_to_cgdmax() {
    let (cgdmin, cgdmax, a, cgs) = (1e-11, 1e-9, 0.7, 5e-10);
    let (hgs, h0) = dev_cap(0.0, cgdmin, cgdmax, a, cgs);
    assert_eq!(hgs, 0.5 * cgs);
    // Continuous at vgd = 0 (tanh and atan both vanish there).
    let (_, hp) = dev_cap(1e-12, cgdmin, cgdmax, a, cgs);
    assert!((hp - h0).abs() < 1e-20);
    // Halves of the limits far from zero.
    let (_, hi) = dev_cap(1e3, cgdmin, cgdmax, a, cgs);
    let (_, lo) = dev_cap(-1e12, cgdmin, cgdmax, a, cgs);
    let s = (cgdmax - cgdmin) / (1.0 + std::f64::consts::PI / 2.0);
    assert!((hi - 0.5 * cgdmax).abs() < 1e-22);
    assert!((lo - 0.5 * cgdmin).abs() < 1e-3 * s);
}

#[test]
fn limitlog_damps_beyond_the_tolerance() {
    let mut check = true;
    assert_eq!(limitlog(25.0, 20.0, 10.0, &mut check), 25.0);
    assert!(!check);
    let v = limitlog(1020.0, 20.0, 10.0, &mut check);
    assert!(check);
    assert!((v - (30.0 + 2.0)).abs() < 1e-12);
    let v = limitlog(-980.0, 20.0, 10.0, &mut check);
    assert!((v - (10.0 - 2.0)).abs() < 1e-12);
    assert_eq!(limitlog(f64::NAN, 0.0, 10.0, &mut check), 0.0);
    assert!(check);
}

fn op(deck: &str) -> crate::Solution {
    let parsed = dune_netlist::parse_str("t", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let opts = Options::default();
    let (mut sols, _) = c.run(&parsed.netlist.analyses[0], &opts).unwrap();
    sols.remove(0)
}

#[test]
fn saturation_current_is_square_law() {
    // KSUBTHRES small enough that the weak-inversion blend is the
    // identity at 2 V of overdrive; no resistances.
    let s = op("t\nvd d 0 dc 10\nvg g 0 dc 5\nm1 d g 0 mm\n\
         .model mm vdmos (kp=10 vto=3 ksubthres=0.01 cjo=0)\n.op\n.end\n");
    let id = -s.column("i(vd)").unwrap()[0];
    // kp/2·vgst², plus the 1e-15 S shunt and the diode leakage.
    assert!((id - 20.0).abs() < 1e-9, "{id}");
}

#[test]
fn pchannel_mirrors_nchannel() {
    let n = op("t\nvd d 0 dc 4\nvg g 0 dc 4\nm1 d g 0 mm\n\
         .model mm vdmos (kp=6 vto=2.5 lambda=0.02 mtriode=1.3 rd=0.1 rs=0.05)\n.op\n.end\n");
    let p = op("t\nvd d 0 dc -4\nvg g 0 dc -4\nm1 d g 0 mm\n\
         .model mm vdmos pchan (kp=6 vto=-2.5 lambda=0.02 mtriode=1.3 rd=0.1 rs=0.05)\n.op\n.end\n");
    let a = n.column("i(vd)").unwrap()[0];
    let b = p.column("i(vd)").unwrap()[0];
    assert!((a + b).abs() < 1e-12 * a.abs(), "{a} {b}");
}

#[test]
fn body_diode_conducts_from_source_to_drain() {
    // Gate off, drain pulled 0.7 V below the source: the body diode
    // carries IS·(exp(vd/vt) − 1) plus gmin·vd.
    let s = op("t\nvd d 0 dc -0.6\nm1 d 0 0 mm\n.model mm vdmos (is=1e-12)\n.op\n.end\n");
    let i = s.column("i(vd)").unwrap()[0];
    let vt = crate::consts::KOVERQ * REFTEMP;
    let want = 1e-12 * (f64::exp(0.6 / vt) - 1.0) + 1e-12 * 0.6;
    // i(vd) is the current into the source's + node: it delivers the
    // diode current.
    assert!((i - want).abs() < 1e-6 * want, "{i} vs {want}");
}

#[test]
fn dangling_temperature_nodes_read_zero() {
    let s = op("t\nvd d 0 dc 2\nvg g 0 dc 4\nm1 d g 0 tj tc mm\n\
         .model mm vdmos (kp=6 vto=3)\n.op\n.end\n");
    assert_eq!(s.column("v(tj)").unwrap()[0], 0.0);
    assert_eq!(s.column("v(tc)").unwrap()[0], 0.0);
}

#[test]
fn self_heating_balances_power_and_thermal_resistance() {
    // Fixed bias, junction to ambient through RTHJC + RTHCA: the
    // junction rises by P·(RTHJC + RTHCA) over the 27 °C ambient.
    let s = op("t\nvd d 0 dc 1\nvg g 0 dc 3.5\nm1 d g 0 tj tc mm thermal\n\
         .model mm vdmos (kp=4 vto=3 rthjc=2 rthca=8 mu=0 cjo=0)\n.op\n.end\n");
    let p = -s.column("i(vd)").unwrap()[0] * 1.0;
    let tj = s.column("v(tj)").unwrap()[0];
    assert!((tj - (27.0 + 10.0 * p)).abs() < 1e-2, "tj {tj}, p {p}");
    assert!(s.column("v(m1#ckttemp)").is_some());
}
