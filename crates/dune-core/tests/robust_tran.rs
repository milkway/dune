//! The opt-in robust transient options of issue #6: `method=trbdf2`
//! (TR-BDF2 composite) and `ltrabkrate=N` (LTRA breakpoint hygiene).
//! The faithful default is the oracle's business (`dune-oracle --ci`);
//! these check the robust modes against physics and closed forms.

use dune_core::{Circuit, Method, Options};

fn corpus(rel: &str) -> String {
    let path = format!("{}/../../corpus/{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Parse a deck, apply its `.options`, return the circuit, options and
/// the first `.tran`.
fn load(name: &str, text: &str) -> (Circuit, Options, dune_netlist::ir::Analysis) {
    let parsed = dune_netlist::parse_str(name, text);
    let (ckt, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut diags = Vec::new();
    opts.apply(&parsed.netlist.options, &mut diags);
    assert!(diags.is_empty(), "{diags:?}");
    let tran = parsed.netlist.analyses[0].clone();
    (ckt, opts, tran)
}

fn run(ckt: &Circuit, opts: &Options, a: &dune_netlist::ir::Analysis) -> dune_core::Solution {
    let (mut sols, _) = ckt.run(a, opts).unwrap();
    sols.remove(0)
}

/// Sign changes of successive differences: a period-2 ring flips on
/// every point.
fn alternations(v: &[f64]) -> usize {
    v.windows(3)
        .filter(|w| (w[2] - w[1]) * (w[1] - w[0]) < 0.0)
        .count()
}

#[test]
fn options_parse_the_robust_values() {
    let (_, opts, _) = load(
        "o.cir",
        "o\nr1 1 0 1\nv1 1 0 1\n.options method=trbdf2 ltrabkrate=10\n.tran 1n 10n\n.end\n",
    );
    assert_eq!(opts.method, Some(Method::TrBdf2));
    assert_eq!(opts.ltra_break_rate, Some(10));
    let d = Options::default();
    assert_eq!(d.method, None);
    assert_eq!(d.ltra_break_rate, None);
}

/// The defaults profile (D-0046): a `.tran` without `method=`
/// integrates with TR-BDF2 under Dune's profile and with the
/// trapezoidal rule under `compat=spice3`; the `.pss` shooting stays
/// on the trapezoidal rule under both; an explicit method wins
/// everywhere, wherever it sits on the card.
#[test]
fn the_profile_picks_the_default_method() {
    use dune_core::Compat;
    let deck = |opt: &str| format!("o\nr1 1 0 1\nv1 1 0 1\n{opt}\n.tran 1n 10n\n.end\n");
    let opts = |opt: &str| load("o.cir", &deck(opt)).1;

    let d = opts("");
    assert_eq!(d.compat, Compat::Dune);
    assert_eq!(d.tran_method(), Method::TrBdf2);
    assert_eq!(d.pss_method(), Method::Trapezoidal);

    let s = opts(".options compat=spice3");
    assert_eq!((s.compat, s.method), (Compat::Spice3, None));
    assert_eq!(s.tran_method(), Method::Trapezoidal);
    assert_eq!(s.pss_method(), Method::Trapezoidal);
    assert_eq!(Options::spice3().tran_method(), Method::Trapezoidal);

    for card in [
        ".options compat=spice3 method=gear",
        ".options method=gear compat=spice3",
    ] {
        let g = opts(card);
        assert_eq!(g.tran_method(), Method::Gear, "{card}");
        assert_eq!(g.pss_method(), Method::Gear, "{card}");
    }
    assert_eq!(opts(".options compat=dune").tran_method(), Method::TrBdf2);
    // An unknown profile is a warning and changes nothing.
    let parsed = dune_netlist::parse_str("o.cir", &deck(".options compat=hspice"));
    let (mut o, mut diags) = (Options::default(), Vec::new());
    o.apply(&parsed.netlist.options, &mut diags);
    assert_eq!((o.compat, diags.len()), (Compat::Dune, 1));
}

/// `corpus/regress/trbdf2_ring.cir`: the dioload diode string of
/// ltra_3. The trapezoidal rule rings period-2 on the string's inner
/// nodes (≈0.7 of all points alternate); TR-BDF2 does not.
#[test]
fn trbdf2_does_not_ring_where_trap_does() {
    let (ckt, opts, tran) = load("ring.cir", &corpus("regress/trbdf2_ring.cir"));
    assert_eq!(opts.method, Some(Method::TrBdf2));
    let tb = run(&ckt, &opts, &tran);
    let trap = run(
        &ckt,
        &Options {
            method: Some(Method::Trapezoidal),
            ..opts.clone()
        },
        &tran,
    );
    for node in ["v(2)", "v(3)"] {
        let (a, n) = (tb.column(node).unwrap(), trap.column(node).unwrap());
        let (alt_tb, alt_trap) = (alternations(a), alternations(n));
        assert!(
            alt_trap * 2 > n.len(),
            "trap {node}: {alt_trap}/{}",
            n.len()
        );
        assert!(alt_tb * 10 < a.len(), "trbdf2 {node}: {alt_tb}/{}", a.len());
    }
}

/// `rc.cir` (3f5 example): a 1 Ω / 1 F RC driven by a 0→1 V ramp over
/// 0.1 s, closed form known. TR-BDF2's error must not exceed the
/// trapezoidal rule's at the same TMAX.
#[test]
fn trbdf2_error_does_not_exceed_trap_on_rc() {
    let (ckt, opts, tran) = load("rc.cir", &corpus("spice3f5/examples/rc.cir"));
    let exact = |t: f64| {
        let tr = 0.1;
        if t <= tr {
            (t - (1.0 - (-t).exp())) / tr
        } else {
            1.0 - (tr.exp() - 1.0) / tr * (-t).exp()
        }
    };
    let err = |method| {
        let sol = run(
            &ckt,
            &Options {
                method: Some(method),
                ..opts.clone()
            },
            &tran,
        );
        let t = sol.column("time").unwrap();
        let v = sol.column("v(2)").unwrap();
        t.iter()
            .zip(v)
            .map(|(&t, &v)| (v - exact(t)).abs())
            .fold(0.0, f64::max)
    };
    let (e_trap, e_tb) = (err(Method::Trapezoidal), err(Method::TrBdf2));
    assert!(e_trap > 1e-5 && e_trap < 1e-3, "{e_trap}");
    assert!(e_tb <= e_trap, "trbdf2 {e_tb} vs trap {e_trap}");
}

/// The breakpoint hygiene leaves a deck without an echo cascade
/// untouched: `regress/ltra_rc.cir` (slope breakpoints at every pulse
/// edge) is bit-identical with `ltrabkrate` on and off. (Measured too:
/// ltra_1, ltra_2, ltra_3 on arm64 and ltra_rg — all bit-identical.)
#[test]
fn ltra_hygiene_is_a_no_op_without_a_cascade() {
    let (ckt, opts, tran) = load("ltra_rc.cir", &corpus("regress/ltra_rc.cir"));
    let off = run(&ckt, &opts, &tran);
    let on = run(
        &ckt,
        &Options {
            ltra_break_rate: Some(10),
            ..opts.clone()
        },
        &tran,
    );
    assert_eq!(off.data, on.data);
}
