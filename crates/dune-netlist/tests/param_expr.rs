//! The parameter expression language on real cards (G2 of
//! `docs/pdk-parser.md`). Each case cites its ngspice-47 probe in
//! `docs/dune-netlist.md` ("Linguagem de expressões de parâmetros").

// Expected values copy the digits ngspice-47 printed.
#![allow(clippy::excessive_precision)]

use dune_netlist::ir::*;
use dune_netlist::{ParseOutcome, Severity, parse_str};

fn parse(deck: &str) -> ParseOutcome {
    parse_str("test.cir", deck)
}

fn clean(deck: &str) -> Netlist {
    let out = parse(deck);
    let errs = rendered_errors(&out);
    assert!(errs.is_empty(), "unexpected errors: {errs:#?}");
    out.netlist
}

fn rendered_errors(out: &ParseOutcome) -> Vec<String> {
    out.diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&out.sources))
        .collect()
}

fn errors(deck: &str) -> Vec<String> {
    rendered_errors(&parse(deck))
}

fn el<'a>(nl: &'a Netlist, name: &str) -> &'a Element {
    nl.elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no element {name}"))
}

fn dc(nl: &Netlist, name: &str) -> f64 {
    match &el(nl, name).device {
        Device::Source { dc: Some(v), .. } => *v,
        d => panic!("{d:?}"),
    }
}

fn param(params: &[Param], name: &str) -> f64 {
    match params.iter().find(|p| p.name == name).map(|p| &p.value) {
        Some(ParamValue::Num(v)) => *v,
        other => panic!("{name}: {other:?}"),
    }
}

/// `{7}`-style results reach cards rounded like ngspice's `%.15e`
/// substitution (probe c7): 3 → 3.0000000000000004, 6 → 6.000000000000001.
const THREE: f64 = 3.000_000_000_000_000_4;
const SIX: f64 = 6.000_000_000_000_001;

#[test]
fn param_card_forms_c5() {
    let nl = clean(
        "t\n\
         .param a=1 b='a*2' c={b+1}\n\
         .param d=1, e=2\n\
         .PARAM F = 3 g = 4\n\
         .param h=1\n\
         + i=2\n\
         + j={h+i}\n\
         .param k=max(2, 3) l= max( 2 , 3 ) m=agauss(2,0,1)\n\
         .param n=-2 o=-a q=a?2:3 r=3 ; comment\n\
         v1 1 0 {c}\n\
         v2 2 0 {d*10+e}\n\
         v3 3 0 {f*10+g}\n\
         v4 4 0 {j}\n\
         v5 5 0 {k*100+l*10+m}\n\
         v6 6 0 {n*1000+o*100+q*10+r}\n\
         .end\n",
    );
    assert_eq!(dc(&nl, "v1"), THREE); // l_multi
    assert_eq!(dc(&nl, "v2"), 12.0); // l_comma
    assert_eq!(dc(&nl, "v3"), 34.0); // l_spaces
    assert_eq!(dc(&nl, "v4"), THREE); // l_cont
    assert_eq!(dc(&nl, "v5"), 332.0); // l_bare_func_sp2, l_bare_agauss
    assert_eq!(dc(&nl, "v6"), -2000.0 - 100.0 + 20.0 + 3.0);
}

#[test]
fn param_card_errors_c5() {
    // ngspice: fatal (single assignment) or silently `b = a` (more).
    let e = errors("t\n.param a=2\n.param b = a * 3\nv1 1 0 1\n.end\n");
    assert!(
        e[0].contains("unexpected `*` after a parameter value"),
        "{e:?}"
    );
    assert!(e[0].starts_with("test.cir:3:14:"), "{e:?}");
    let e = errors("t\n.param a=1 2\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("unexpected `2`"), "{e:?}");
    let e = errors("t\n.param a=3 junk\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("unexpected `junk`"), "{e:?}");
    let e = errors("t\n.param a=\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("has no value"), "{e:?}");
    let e = errors("t\n.param a\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("has no value"), "{e:?}");
    let e = errors("t\n.param a='1+2\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("missing closing `'`"), "{e:?}");
    let e = errors("t\n.param a={1+2\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("missing `}`"), "{e:?}");
    let e = errors("t\n.param s=\"abc\"\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("string-valued"), "{e:?}");
    let e = errors("t\n.param 1a=3\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("cannot be a parameter name"), "{e:?}");
    let e = errors("t\n.param a=3$x\nv1 1 0 1\n.end\n");
    assert!(e[0].contains("unexpected `$`"), "{e:?}");
    // An error inside the expression points at it.
    let e = errors("t\n.param a={2 + nosuch}\nv1 1 0 1\n.end\n");
    assert!(e[0].starts_with("test.cir:2:15:"), "{e:?}");
    assert!(e[0].contains("unknown parameter `nosuch`"), "{e:?}");
    // `.param` in a subcircuit is G3.
    let e = errors("t\n.subckt s a\n.param w=1\nr1 a 0 {w}\n.ends\nx1 1 s\n.end\n");
    assert!(
        e.iter()
            .any(|m| m.contains(".param inside .subckt is not supported yet")),
        "{e:?}"
    );
}

#[test]
fn delimiters_and_blanks_c6() {
    let nl = clean(
        "t\n\
         .param a=2\n\
         v1 1 0 ' a * 3 '\n\
         v2 2 0 { a * 3 }\n\
         v3 3 0 dc { max(a, 3) }\n\
         v4 4 0 {{a}+1}\n\
         v5 5 0 {'a'+1}\n\
         v6 6 0 '{a}+1'\n\
         .end\n",
    );
    assert_eq!(dc(&nl, "v1"), SIX);
    assert_eq!(dc(&nl, "v2"), SIX);
    assert_eq!(dc(&nl, "v3"), THREE);
    for v in ["v4", "v5", "v6"] {
        assert_eq!(dc(&nl, v), THREE, "{v}"); // b_nested_brace, b_brace_quote, b_brace_in_quote
    }
    // Double quotes vanish from cards (probe c10), not from `.param`.
    let nl = clean("t\n.param t=4\nv1 1 0 {2*\"3+1\"}\nv2 2 0 {\"-t/2-1\"}\n.end\n");
    assert_eq!(dc(&nl, "v1"), 7f64.next_up());
    assert_eq!(dc(&nl, "v2"), -THREE);
    let e = errors("t\n.param a={\"2+1\"}\nv1 1 0 {a}\n.end\n");
    assert!(e[0].contains("unexpected `\"`"), "{e:?}");
    for bad in ["v1 1 0 'a+1", "v1 1 0 {a+1", "v1 1 0 {}", "v1 1 0 ''"] {
        let e = errors(&format!("t\n.param a=2\n{bad}\n.end\n"));
        assert!(!e.is_empty(), "{bad}");
    }
}

#[test]
fn bare_names_after_equals_c6_c7() {
    let nl = clean(
        "t\n\
         .param rv=7 wv=2 lv=3\n\
         .model rm r rsh=rv\n\
         .model rm2 r(rsh=rv*2 tc1=0)\n\
         .model rm3 r rsh=max(rv, 2) tc1 = lv\n\
         .model rm4 r rsh = ' rv * 1 ' tc2={ lv }\n\
         r1 1 0 rm l=1 w=wv\n\
         r2 1 0 rm l='rv*3'\n\
         r3 1 0 rm l = { rv * 3 }\n\
         v1 1 0 dc=rv\n\
         .end\n",
    );
    let model = |n: &str| nl.models.iter().find(|m| m.name == n).unwrap();
    let seven = 7f64.next_up();
    assert_eq!(param(&model("rm").params, "rsh"), seven); // q_model_bare
    assert_eq!(param(&model("rm2").params, "rsh"), 14.0); // b_model_bare_expr
    assert_eq!(param(&model("rm3").params, "rsh"), seven);
    assert_eq!(param(&model("rm3").params, "tc1"), THREE);
    assert_eq!(param(&model("rm4").params, "rsh"), seven);
    assert_eq!(param(&model("rm4").params, "tc2"), THREE);
    match &el(&nl, "r1").device {
        Device::Resistor { params, .. } => assert_eq!(param(params, "w"), 2.0), // b_rw
        d => panic!("{d:?}"),
    }
    for r in ["r2", "r3"] {
        match &el(&nl, r).device {
            Device::Resistor { params, .. } => {
                assert_eq!(param(params, "l"), 21.0, "{r}")
            }
            d => panic!("{d:?}"),
        }
    }
    assert_eq!(dc(&nl, "v1"), seven);
    // A bare name is only a parameter after `=`; an unknown one is an
    // error (ngspice: "Undefined parameter").
    let e = errors("t\nv1 1 0 dc=nosuch\n.end\n");
    assert!(e[0].contains("unknown parameter `nosuch`"), "{e:?}");
    // A keyword following a value is not swallowed as a value even
    // when a parameter of that name exists.
    let nl = clean("t\n.param l=5\n.model rm r rsh=1\nr1 1 0 rm w=2 l=3\n.end\n");
    match &el(&nl, "r1").device {
        Device::Resistor { params, .. } => {
            assert_eq!(param(params, "w"), 2.0);
            assert_eq!(param(params, "l"), 3.0);
        }
        d => panic!("{d:?}"),
    }
}

#[test]
fn control_cards_c6() {
    let nl = clean(
        "t\n\
         .param ts=1n tf=5n hi=2 t=50 a=2\n\
         v1 1 0 1\n\
         .tran {ts} {tf}\n\
         .dc v1 0 {hi} {hi/2}\n\
         .ic v(1)={a} v(2)=a\n\
         .options temp={t} method=gear\n\
         .end\n",
    );
    // `{1n}` substitutes as 9.99999999999999855e-10 (probe c9 q_1n).
    assert!(
        nl.analyses.iter().any(|a| matches!(
            a,
            Analysis::Tran { tstep, tstop, .. }
                if *tstep == 9.999_999_999_999_998_55e-10 && *tstop == 4.999_999_999_999_999_28e-9
        )),
        "{:?}",
        nl.analyses
    );
    assert_eq!(nl.node_inits.len(), 2);
    assert!(nl.node_inits.iter().all(|n| n.value == 2.0));
    let opt = |n: &str| {
        nl.options
            .iter()
            .find(|p| p.name == n)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(opt("temp"), ParamValue::Num(50.0));
    assert_eq!(opt("method"), ParamValue::Str("gear".into()));
}

#[test]
fn values_reaching_cards_c1_c7() {
    let nl = clean(
        "t\n\
         .param z={1/0}\n\
         v1 1 0 {1/3}\n\
         v2 2 0 {1/z}\n\
         v3 3 0 {2^3^2}\n\
         v4 4 0 {1mil*1e3}\n\
         .end\n",
    );
    assert_eq!(dc(&nl, "v1"), 0.333_333_333_333_333_31);
    assert_eq!(dc(&nl, "v2"), 0.0);
    assert_eq!(dc(&nl, "v3"), 64.0);
    assert_eq!(dc(&nl, "v4"), 1.0); // S9: `mil` is milli inside braces
    for (expr, what) in [
        ("{1/0}", "inf"),
        ("{0/0}", "NaN"),
        ("{sqrt(-4)}", "NaN"),
        ("{temper}", "temper"),
        ("{nosuch}", "unknown parameter"),
        ("{agauss(1,0.1)}", ""),
    ] {
        let e = errors(&format!("t\nv1 1 0 {expr}\n.end\n"));
        if what.is_empty() {
            assert!(e.is_empty(), "{expr}: {e:?}");
        } else {
            assert!(e.iter().any(|m| m.contains(what)), "{expr}: {e:?}");
        }
    }
}

#[test]
fn random_functions_warn_once_at_nominal_c3() {
    let out =
        parse("t\n.param g=agauss(1,0.1,1)\nv1 1 0 {g+aunif(1,0.5)}\nv2 2 0 {limit(5,1)}\n.end\n");
    assert!(out.is_clean());
    let w: Vec<_> = out
        .diags
        .iter()
        .filter(|d| d.message.contains("nominal"))
        .collect();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].render(&out.sources).split(':').nth(1), Some("2"));
    assert_eq!(dc(&out.netlist, "v1"), 2.0);
    assert_eq!(dc(&out.netlist, "v2"), 5.0);
}

#[test]
fn large_parameter_sets_stay_linear() {
    // 170 000 definitions, the size of sky130's full library, with
    // forward references: parse + resolve must stay well under a
    // second in release builds (measured in docs/dune-netlist.md).
    let n = 170_000;
    let mut deck = String::with_capacity(n * 40);
    deck.push_str("t\n");
    for i in 0..n {
        let next = (i + 1) % n;
        if next == 0 {
            deck.push_str(&format!(".param p{i}='1.5u*2'\n"));
        } else {
            deck.push_str(&format!(".param p{i}={{ p{next} * 1 + 0 }}\n"));
        }
    }
    deck.push_str("v1 1 0 {p0*1e6}\n.end\n");
    let nl = clean(&deck);
    assert_eq!(dc(&nl, "v1"), THREE);
}
