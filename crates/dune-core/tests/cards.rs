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
