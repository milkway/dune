//! `.tf` and `.sens` on circuits with closed-form answers.

use dune_core::{Circuit, Isource, Options, OutputVar, Resistor, Vsource};

fn vsrc(name: &str, p: &str, n: &str, dc: f64) -> Vsource {
    Vsource {
        name: name.into(),
        nodes: [p.into(), n.into()],
        dc: Some(dc),
        wave: None,
        ac: None,
        distof1: None,
        distof2: None,
        port: None,
    }
}

fn res(name: &str, p: &str, n: &str, value: f64) -> Resistor {
    Resistor {
        name: name.into(),
        nodes: [p.into(), n.into()],
        value,
    }
}

fn close(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs())
}

/// vin — 1k — out — 3k — gnd: gain R2/(R1+R2), Rin R1+R2, Rout R1‖R2.
fn divider() -> Circuit {
    let mut ckt = Circuit::new("divider");
    ckt.add(vsrc("vin", "1", "0", 1.0)).unwrap();
    ckt.add(res("r1", "1", "2", 1e3)).unwrap();
    ckt.add(res("r2", "2", "0", 3e3)).unwrap();
    ckt
}

#[test]
fn tf_of_a_divider() {
    let out = OutputVar::Voltage {
        node: "2".into(),
        reference: None,
    };
    let tf = divider().tf(&Options::default(), &out, "vin").unwrap();
    assert!(close(tf.transfer_function, 0.75, 1e-12));
    assert!(close(tf.input_impedance, 4e3, 1e-12));
    assert!(close(tf.output_impedance, 750.0, 1e-12));
    assert_eq!(tf.solution.plotname, "Transfer Function");
    assert_eq!(
        tf.solution.column("v(output_impedance_at_V(2))"),
        Some(&[tf.output_impedance][..])
    );
    assert!(tf.solution.column("v(vin#Input_impedance)").is_some());
}

#[test]
fn tf_current_in_current_out() {
    // iin into node 1 — r1 1k — node 2 —(r3 2k to gnd)— r2 3k — vx (0 V) to gnd.
    let mut ckt = Circuit::new("current");
    ckt.add(Isource {
        name: "iin".into(),
        nodes: ["0".into(), "1".into()],
        dc: Some(1e-3),
        wave: None,
        ac: None,
        distof1: None,
        distof2: None,
    })
    .unwrap();
    ckt.add(res("r1", "1", "2", 1e3)).unwrap();
    ckt.add(res("r2", "2", "3", 3e3)).unwrap();
    ckt.add(vsrc("vx", "3", "0", 0.0)).unwrap();
    ckt.add(res("r3", "2", "0", 2e3)).unwrap();
    let out = OutputVar::Current { vsrc: "vx".into() };
    let tf = ckt.tf(&Options::default(), &out, "iin").unwrap();
    // Current divider 2k/(2k+3k); Rin = 1k + 2k‖3k.
    assert!(close(tf.transfer_function, 0.4, 1e-12));
    assert!(close(tf.input_impedance, 2.2e3, 1e-12));
    // The C clamps 1/max(1e-20, i): a negative branch response
    // reads as 1e20.
    assert_eq!(tf.output_impedance, 1e20);
}

#[test]
fn tf_rejects_a_non_source_input() {
    let out = OutputVar::Voltage {
        node: "2".into(),
        reference: None,
    };
    let err = divider().tf(&Options::default(), &out, "r1").unwrap_err();
    assert!(err.to_string().contains("not of proper type"), "{err}");
    let err = divider()
        .tf(&Options::default(), &out, "vnone")
        .unwrap_err();
    assert!(err.to_string().contains("not in circuit"), "{err}");
}

#[test]
fn dc_sens_of_a_divider() {
    let out = OutputVar::Voltage {
        node: "2".into(),
        reference: None,
    };
    let s = divider()
        .sens(&Options::default(), &out, &[], None)
        .unwrap();
    let v = |name: &str| s.solution.column(name).unwrap()[0];
    // V2 = V·R2/(R1+R2): ∂/∂R1 = −V·R2/(R1+R2)², ∂/∂R2 = V·R1/(R1+R2)²,
    // ∂/∂V = R2/(R1+R2); the forward difference of δ = 1e-6·p sits
    // within ~1e-6 relative of the derivative.
    assert!(close(v("v(r1)"), -3e3 / 16e6, 1e-5));
    assert!(close(v("v(r2)"), 1e3 / 16e6, 1e-5));
    assert!(close(v("v(vin)"), 0.75, 1e-9));
    // m divides R, scale multiplies it.
    assert!(close(v("v(r1_m)"), 3e3 * 1e3 / 16e6, 1e-5));
    assert!(close(v("v(r1_scale)"), -3e3 * 1e3 / 16e6, 1e-5));
    // Inert entries exist and are zero.
    assert_eq!(v("v(r1:rsh)"), 0.0);
    assert_eq!(v("v(vin_z0)"), 0.0);
    // Filters select by glob.
    let f = divider()
        .sens(&Options::default(), &out, &["r?_m".into()], None)
        .unwrap();
    let names: Vec<&str> = f
        .solution
        .variables
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(names, ["v(r2_m)", "v(r1_m)"]);
}
