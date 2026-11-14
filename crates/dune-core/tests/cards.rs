//! Semiconductor R/C cards (`restemp.c`, `captemp.c`), `.dc temp`
//! / `.dc RNAME` (ngspice-47 `dctrcurv.c`) and `.options keepopinfo`.

use dune_core::{Circuit, Options, Solution};

fn run(deck: &str) -> (Vec<Solution>, Vec<String>) {
    let parsed = dune_netlist::parse_str("cards.cir", deck);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (ckt, mut notes) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut diags = Vec::new();
    opts.apply(&parsed.netlist.options, &mut diags);
    let mut plots = Vec::new();
    for a in &parsed.netlist.analyses {
        let (s, n) = ckt.run(a, &opts).unwrap();
        plots.extend(s);
        notes.extend(n);
    }
    (plots, notes)
}

fn real(s: &Solution, name: &str) -> Vec<f64> {
    let i = s
        .variables
        .iter()
        .position(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no {name}"));
    match &s.data {
        dune_raw::Data::Real(c) => c[i].clone(),
        dune_raw::Data::Complex(c) => c[i].iter().map(|z| z.re).collect(),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs())
}

#[test]
fn resistor_geometry_narrow_is_the_3f5_formula() {
    // R = RSH*(L-NARROW)/(W-NARROW) = 100*(10u-1u)/(2u-1u) = 900.
    let (p, _) = run("narrow\nv1 1 0 1\nr1 1 0 rm l=10u w=2u\n\
                      .model rm r rsh=100 narrow=1u\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1.0 / 900.0));
}

#[test]
fn resistor_without_sheet_resistance_falls_back_to_1k() {
    let (p, notes) = run("fallback\nv1 1 0 1\nr1 1 0 rm l=10u\n\
                          .model rm r tc1=0\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1e-3));
    assert!(notes.iter().any(|n| n.contains("set to 1000")), "{notes:?}");
}

#[test]
fn resistor_default_width_is_ten_microns() {
    // DEFW = 10e-6 in the code (spice.txt says 1e-6): 50*20u/10u.
    let (p, _) = run("defw\nv1 1 0 1\nr1 1 0 rm l=20u\n.model rm r rsh=50\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1.0 / 100.0));
}

#[test]
fn instance_temp_uses_model_tc_and_tnom() {
    // factor = 1 + 1e-3*(127-27) + 1e-5*100^2 = 1.2.
    let (p, _) = run("temp\nv1 1 0 1\nr1 1 0 1k rm temp=127\n\
                      .model rm r tc1=1e-3 tc2=1e-5\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1.0 / 1200.0));
}

#[test]
fn instance_dtemp_offsets_the_circuit_temperature() {
    // ngspice-47 restemp.c: T = CKTtemp + DTEMP = 27 + 100 = 127,
    // factor 1.2 as above; with TEMP= too, TEMP wins (the C's note).
    let (p, notes) = run("dtemp\nv1 1 0 1\nr1 1 0 1k rm dtemp=100\n\
                          v2 2 0 1\nr2 2 0 1k rm temp=127 dtemp=-50\n\
                          .model rm r tc1=1e-3 tc2=1e-5\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1.0 / 1200.0));
    assert!(close(real(&p[0], "i(v2)")[0], -1.0 / 1200.0));
    assert!(
        notes.iter().any(|n| n.contains("dtemp ignored")),
        "{notes:?}"
    );
    // A plain resistor takes `dtemp` as a parameter, not a model name.
    let (p, _) = run("dtemp plain\nv1 1 0 1\nr1 1 0 1k dtemp=10\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -1e-3));
}

#[test]
fn bsim1_instance_temp_is_an_unknown_parameter() {
    // b1.c has `temp` only in the model table (3f5 and ngspice-47,
    // whose parser stops on "unknown parameter (temp)"); the card
    // used to drop it silently.
    for kw in ["temp=50", "dtemp=5"] {
        let deck = format!(
            "b1\nv1 1 0 2\nm1 1 1 0 0 nb l=2u w=10u {kw}\n.model nb nmos level=4\n.op\n.end\n"
        );
        let parsed = dune_netlist::parse_str("b1.cir", &deck);
        let err = Circuit::from_netlist(&parsed.netlist)
            .err()
            .expect("must fail");
        assert!(err.to_string().contains("unknown parameter"), "{err}");
    }
}

#[test]
fn resistor_temperature_sensitivity_matches_a_finite_difference() {
    // cktsens.c: sens_setp + sens_temp (REStemp) — `_temp` moves the
    // instance temperature (here CKTtemp + DTEMP = 70 C); `_dtemp`
    // stays inert, as in ngspice's full walk.
    let deck = |extra: &str| {
        format!(
            "s\n.options temp=60\nv1 1 0 dc 5\nr1 1 2 1k\nr2 2 0 rtc 2k {extra}\n\
             .model rtc r tc1=2e-3 tc2=1e-5\n.sens v(2) r2_temp r2_dtemp\n.end\n"
        )
    };
    let (p, _) = run(&deck("dtemp=10"));
    let s_temp = real(&p[0], "v(r2_temp)")[0];
    assert_eq!(real(&p[0], "v(r2_dtemp)")[0], 0.0);
    let v2 = |extra: &str| {
        let d = format!(
            "s\n.options temp=60\nv1 1 0 dc 5\nr1 1 2 1k\nr2 2 0 rtc 2k {extra}\n\
             .model rtc r tc1=2e-3 tc2=1e-5\n.op\n.end\n"
        );
        real(&run(&d).0[0], "v(2)")[0]
    };
    let fd = (v2("temp=70.01") - v2("temp=69.99")) / 0.02;
    assert!((s_temp - fd).abs() <= 1e-6 * fd.abs(), "{s_temp} vs {fd}");
    // ngspice with only `r2_dtemp` walked gives 2.777359e-3 here.
    assert!((s_temp - 2.777359e-3).abs() <= 1e-6, "{s_temp}");
}

#[test]
fn capacitor_geometry_narrow_is_the_3f5_formula() {
    // C = CJ*(W-N)*(L-N) + 2*CJSW*((L-N)+(W-N)), N=1u, L=11u, W=6u:
    // 1e-3*5u*10u + 2*1e-9*15u = 5e-14 + 3e-14 = 8e-14.
    let c = 8e-14;
    let (p, _) = run("cap\nv1 1 0 dc 0 ac 1\nc1 1 0 cm l=11u w=6u\n\
                      .model cm c cj=1e-3 cjsw=1e-9 narrow=1u\n.ac lin 1 1meg 1meg\n.end\n");
    let i = match &p[0].data {
        dune_raw::Data::Complex(cols) => {
            let k = p[0]
                .variables
                .iter()
                .position(|(n, _)| n == "i(v1)")
                .unwrap();
            cols[k][0]
        }
        _ => unreachable!(),
    };
    let w = 2.0 * std::f64::consts::PI * 1e6;
    assert!(close(-i.im, w * c), "{i}");
}

#[test]
fn dc_sweeps_temperature_and_resistance() {
    let (p, _) = run("dct\nv1 1 0 1\nr1 1 0 rm l=10u w=10u\n\
                      .model rm r rsh=1k tc1=1e-2\n.dc temp 27 127 50\n.end\n");
    assert_eq!(p[0].variables[0].0, "temp-sweep");
    let t = real(&p[0], "temp-sweep");
    assert_eq!(t, [27.0, 77.0, 127.0]);
    let i = real(&p[0], "i(v1)");
    for (k, tc) in t.iter().enumerate() {
        assert!(close(i[k], -1.0 / (1e3 * (1.0 + 1e-2 * (tc - 27.0)))));
    }
    let (p, _) = run("dcr\nv1 1 0 1\nr1 1 0 1k\n.dc r1 1k 3k 1k\n.end\n");
    assert_eq!(p[0].variables[0].0, "res-sweep");
    let i = real(&p[0], "i(v1)");
    assert!(close(i[2], -1.0 / 3e3), "{i:?}");
}

#[test]
fn keepopinfo_dumps_the_operating_point_first() {
    let (p, _) = run(
        "kop\n.options keepopinfo\nv1 1 0 dc 1 ac 1\nr1 1 2 1k\nr2 2 0 1k\n\
                      .ac dec 1 1k 10k\n.end\n",
    );
    let names: Vec<&str> = p.iter().map(|s| s.plotname.as_str()).collect();
    assert_eq!(names, ["AC Operating Point", "AC Analysis"]);
    assert!(close(real(&p[0], "v(2)")[0], 0.5));
}

#[test]
fn own_multiplier_scales_resistors_by_value_and_geometry() {
    // ngspice-47 `G = m/R` (fix-probes r_m, r_m_geom): 1k/2 ∥ 1k/4,
    // then RSH·L/W = 1k by geometry over m=4; no "ignored" note.
    let (p, notes) = run("m\nv1 1 0 1\nr1 1 0 1k m=2\nr2 1 0 rm l=10u w=1u m=4\n\
                          .model rm r rsh=100\n.op\n.end\n");
    assert!(close(real(&p[0], "i(v1)")[0], -(2e-3 + 4e-3)));
    assert!(notes.iter().all(|n| !n.contains("ignored")), "{notes:?}");
}

#[test]
fn own_multiplier_keeps_the_diode_breakdown_knee() {
    // m parallel diodes in breakdown carry m·IBV at -BV: 2 mA through
    // m=2 sits where 1 mA sits in one diode (fix-probes d_m_bv: an
    // area of 2 would sit vt·ln 2 lower).
    let deck = |card: &str, i: &str| {
        format!("bv\ni1 1 0 {i}\n{card}\n.model dm d is=1e-14 bv=5 ibv=1e-6\n.op\n.end\n")
    };
    let v = |card: &str, i: &str| real(&run(&deck(card, i)).0[0], "v(1)")[0];
    let one = v("d1 1 0 dm", "1m");
    let m2 = v("d1 1 0 dm m=2", "2m");
    let area2 = v("d1 1 0 dm area=2", "2m");
    // Equal within the Newton tolerance (RELTOL·|v| ≈ 5 mV here; the
    // two land 0.3 mV apart), 18 mV away from the area reading.
    assert!((one - m2).abs() < 1e-3, "{one} {m2}");
    assert!((m2 - area2 - 0.0179).abs() < 1e-3, "{area2} {m2}");
}

#[test]
fn temp_card_sets_the_circuit_temperature() {
    // `.temp` wins over `.options temp` wherever it stands
    // (fix-probes t_opt_card, t_card_opt): factor 1 + 0.01·100.
    for order in [".options temp=77\n.temp 127", ".temp 127\n.options temp=77"] {
        let (p, _) = run(&format!(
            "t\nv1 1 0 1\nr1 1 0 rm 1k\n.model rm r tc1=0.01\n{order}\n.op\n.end\n"
        ));
        assert!(close(real(&p[0], "i(v1)")[0], -1.0 / 2000.0), "{order}");
    }
}
