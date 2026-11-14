//! Subcircuit parameters, per-instance evaluation, model binning and
//! the `X`-line multiplier (G3, G4, G6 of `docs/pdk-parser.md`). Each
//! test cites the ngspice-47 probe it reproduces (`sN case`: case
//! `case` of `.claude/scratch/g3-probes/sN.txt`; `Pnn`: appendix A of
//! `docs/pdk-parser.md`).

use dune_netlist::ir::*;
use dune_netlist::{ParseOutcome, Severity, parse_str};

fn parse(deck: &str) -> ParseOutcome {
    parse_str("t.cir", &format!("t\n{deck}\n.end\n"))
}

fn errors(out: &ParseOutcome) -> Vec<String> {
    out.diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

fn clean(deck: &str) -> Netlist {
    let out = parse(deck);
    let e = errors(&out);
    assert!(e.is_empty(), "{deck}\n{e:?}");
    out.netlist
}

fn element<'a>(nl: &'a Netlist, name: &str) -> &'a Element {
    nl.elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no element `{name}` in {:?}", names(nl)))
}

fn names(nl: &Netlist) -> Vec<&str> {
    nl.elements.iter().map(|e| e.name.as_str()).collect()
}

/// The value of resistor `name`, rounded to 12 significant digits (the
/// card sees `%.15e`-rounded values, 7 → 7.000000000000001).
fn r(nl: &Netlist, name: &str) -> f64 {
    match &element(nl, name).device {
        Device::Resistor { value: Some(v), .. } => round(*v),
        d => panic!("{d:?}"),
    }
}

fn round(v: f64) -> f64 {
    let s = format!("{v:.11e}");
    s.parse().unwrap()
}

fn param(params: &[Param], name: &str) -> Option<f64> {
    params
        .iter()
        .find_map(|p| match (&p.value, p.name == name) {
            (ParamValue::Num(v), true) => Some(*v),
            _ => None,
        })
}

/// `x1 1 s …` driving one resistor `r1 a 0 {expr}`: its value.
fn one(deck: &str) -> f64 {
    r(&clean(deck), "r.x1.r1")
}

#[test]
fn precedence_x_line_over_defaults_over_globals_p04() {
    let nl = clean(
        ".param w=7\n\
         .subckt sbody a\n.param w=1 l=2\nr1 a 0 {w*1000+l}\n.ends\n\
         .subckt sline a w=1 l=2\nr1 a 0 {w*1000+l}\n.ends\n\
         .subckt skw a params: w=1 l=2\nr1 a 0 {w*1000+l}\n.ends\n\
         .subckt sglob a\nr1 a 0 {w*1000}\n.ends\n\
         x1 n1 sbody w=5\nx2 n2 sline w=5\nx3 n3 skw w=5\nx4 n4 sglob\nx5 n5 sbody",
    );
    assert_eq!(r(&nl, "r.x1.r1"), 5002.0);
    assert_eq!(r(&nl, "r.x2.r1"), 5002.0);
    assert_eq!(r(&nl, "r.x3.r1"), 5002.0);
    assert_eq!(r(&nl, "r.x4.r1"), 7000.0);
    assert_eq!(r(&nl, "r.x5.r1"), 1002.0);
}

#[test]
fn defaults_see_each_other_and_the_actuals_s1() {
    // s1 def_glob, def_def, def_def_rev, body_xref, body_fwd.
    assert_eq!(
        one(".param g=3\n.subckt s a w={g*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        6000.0
    );
    assert_eq!(
        one(".subckt s a w=1 l={w*2}\nr1 a 0 {l*1000}\n.ends\nx1 1 s w=5"),
        10000.0
    );
    assert_eq!(
        one(".subckt s a l={w*2} w=1\nr1 a 0 {l*1000}\n.ends\nx1 1 s w=5"),
        10000.0
    );
    assert_eq!(
        one(".subckt s a w=1\n.param lw={w*3}\nr1 a 0 {lw*1000}\n.ends\nx1 1 s w=5"),
        15000.0
    );
    assert_eq!(
        one(
            ".subckt s a w=1\n.param a1={a2+1}\n.param a2={w*2}\nr1 a 0 {a1*1000}\n.ends\n\
             x1 1 s w=5"
        ),
        11000.0
    );
    // P15: chained locals mixing a global and the instance.
    assert_eq!(
        one(
            ".param g=10\n.subckt s a w=1\n.param lw={w*g}\n.param lw2={lw+1}\n\
             r1 a 0 {lw2*100}\n.ends\nx1 1 s w=2"
        ),
        2100.0
    );
}

#[test]
fn last_definition_wins_inside_a_body_s1() {
    // s1 body_redef, line_then_body, line_then_body_x; s2 body_twice_x,
    // body_param_after_elem_ref_glob, x_dup.
    assert_eq!(
        one(".subckt s a\n.param p=1\nr1 a 0 {p*1000}\n.param p=4\n.ends\nx1 1 s"),
        4000.0
    );
    assert_eq!(
        one(".subckt s a w=1\n.param w=2\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        2000.0
    );
    assert_eq!(
        one(".subckt s a w=1\n.param w=2\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=7"),
        7000.0
    );
    assert_eq!(
        one(".subckt s a\n.param w=1\n.param w=2\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=5"),
        5000.0
    );
    assert_eq!(
        one(".param g=2\n.subckt s a\nr1 a 0 {g*1000}\n.param g=5\n.ends\nx1 1 s"),
        5000.0
    );
    assert_eq!(
        one(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=2 w=3"),
        3000.0
    );
}

#[test]
fn self_reference_reads_the_enclosing_scope_s1_s3() {
    // s1 body_self, body_self_x; s2 def_self_glob, def_self_noglob;
    // s3 line_body_self(_glob), x_self_nested, x_self_noparent.
    assert_eq!(
        one(".param w=3\n.subckt s a\n.param w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        6000.0
    );
    assert_eq!(
        one(".subckt s a w=1\n.param w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=3"),
        3000.0
    );
    assert_eq!(
        one(".param w=3\n.subckt s a w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        6000.0
    );
    assert_eq!(
        one(".param w=3\n.subckt s a w=1\n.param w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        6000.0
    );
    let nl = clean(
        ".subckt leaf a w=1\nr1 a 0 {w*1000}\n.ends\n.subckt mid a w=2\n\
         x1 a leaf w={w*3}\n.ends\nx1 1 mid w=5",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 15000.0);
    for deck in [
        ".subckt s a w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s",
        ".subckt s a w=1\n.param w={w*2}\nr1 a 0 {w*1000}\n.ends\nx1 1 s",
        ".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={w*3}",
    ] {
        let e = errors(&parse(deck));
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].contains("unknown parameter `w`"), "{deck}: {e:?}");
    }
    // A cycle through two locals is a cycle, not a self-reference.
    let e = errors(&parse(
        ".param w=3\n.subckt s a\n.param v={w*2}\n.param w={v}\nr1 a 0 {w}\n.ends\nx1 1 s",
    ));
    assert!(e.iter().any(|m| m.contains("cycle")), "{e:?}");
}

#[test]
fn actuals_are_evaluated_in_the_callee_scope_s3() {
    // s3 xcl_both, xcl_both_mid, xcl_actual_actual, xcl_actual_body(_glob),
    // xcl_caller_only; s1 xexpr_caller, x_ref_own_param.
    assert_eq!(
        one(".param l=5\n.subckt s a w=1 l=2\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={l}"),
        2000.0
    );
    assert_eq!(
        one(".subckt s a w=1 l=2\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={l} l=3"),
        3000.0
    );
    assert_eq!(
        one(".param k=9\n.subckt s a w=1\n.param k=4\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={k}"),
        4000.0
    );
    let nl = clean(
        ".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\n.subckt mid a q=6\nx1 a s w={q}\n.ends\n\
         x1 1 mid",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 6000.0);
    let nl = clean(
        ".param g=3\n.subckt leaf a w=1\nr1 a 0 {w*1000}\n.ends\n.subckt mid a w=1\n\
         x1 a leaf w={w*2}\n.ends\nx1 1 mid w=5",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 10000.0);
    assert_eq!(
        one(".param w=7\n.subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={w+1}"),
        8000.0
    );
    // P17, s3 x_quoted_tern / x_bare_name, s1 x_quote_spaces / x_bare_expr.
    assert_eq!(
        one(".param d_sab=0\n.subckt s a lr=1\nr1 a 0 {lr*1k}\n.ends\n\
             x1 1 s lr='(d_sab==0) ? 2 : d_sab'"),
        2000.0
    );
    assert_eq!(
        one(".param g=4\n.subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=g"),
        4000.0
    );
    assert_eq!(
        one(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w={ 2 + 3 }"),
        5000.0
    );
    assert_eq!(
        one(".param g=2\n.subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=g*3"),
        6000.0
    );
}

#[test]
fn scoping_is_dynamic_through_the_callers_s1_s2() {
    // s1 dyn_scope, dyn_scope_body, lex_nested; s2 dyn_chain, dyn_default.
    let nl = clean(
        ".param w=7\n.subckt leaf a\nr1 a 0 {w*1000}\n.ends\n.subckt outer a w=3\n\
         x1 a leaf\n.ends\nx1 1 outer",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 3000.0);
    let nl = clean(
        ".param w=7\n.subckt leaf a\nr1 a 0 {w*1000}\n.ends\n.subckt mid a\nx1 a leaf\n.ends\n\
         .subckt outer a w=3\nx1 a mid\n.ends\nx1 1 outer",
    );
    assert_eq!(r(&nl, "r.x1.x1.x1.r1"), 3000.0);
    let nl = clean(
        ".subckt leaf a l={w*2}\nr1 a 0 {l*1000}\n.ends\n.subckt outer a w=3\nx1 a leaf\n\
         .ends\nx1 1 outer",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 6000.0);
    let nl = clean(
        ".param w=7\n.subckt outer a w=3\n.subckt inner b\nr1 b 0 {w*1000}\n.ends\n\
         x1 a inner\n.ends\nx1 1 outer",
    );
    assert_eq!(r(&nl, "r.x1.x1.r1"), 3000.0);
    // s2 glob_after_sub_shadow: a default shadows a global defined later.
    assert_eq!(
        one(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s\n.param w=9"),
        1000.0
    );
}

#[test]
fn names_are_case_insensitive_s1() {
    assert_eq!(
        one(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s W=5"),
        5000.0
    );
    assert_eq!(
        one(".subckt s a W=1\nr1 a 0 {w*1000}\n.ends\nx1 1 S w=5"),
        5000.0
    );
    // `params:` on both lines and blanks around `=` (s1 params_kw_x,
    // port_eq_space, x_eq_space).
    assert_eq!(
        one(".subckt s a params: w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s params: w=5"),
        5000.0
    );
    assert_eq!(
        one(".subckt s a w = 2\nr1 a 0 {w*1000}\n.ends\nx1 1 s"),
        2000.0
    );
    assert_eq!(
        one(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w = 4"),
        4000.0
    );
}

#[test]
fn unknown_actuals_s1_s2() {
    // Ignored (with a warning here) when the subcircuit has formals
    // (s1 undeclared_x, s2 count_more) and not visible in the body
    // (s4 m_undecl_visible); an error when it has none (s1
    // undeclared_x_noparams, x_glob_override).
    let out = parse(".subckt s a w=1\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=2 foo=3");
    assert!(errors(&out).is_empty(), "{:?}", out.diags);
    assert_eq!(r(&out.netlist, "r.x1.r1"), 2000.0);
    assert!(out.diags.iter().any(|d| {
        d.severity == Severity::Warning
            && d.message
                .contains("`foo` is not a parameter of subcircuit `s`")
    }));
    let e = errors(&parse(
        ".subckt s a w=1\nr1 a 0 {foo*1000}\n.ends\nx1 1 s w=2 foo=3",
    ));
    assert!(e[0].contains("unknown parameter `foo`"), "{e:?}");
    let out = parse(".param w=7\n.subckt s a\nr1 a 0 {w*1000}\n.ends\nx1 1 s w=3");
    let e = errors(&out);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("has no parameters, but the call passes `w`"));
    assert_eq!(out.sources.snippet(out.diags[0].span.unwrap()), "w");
    // A body `.param` is a formal (s1 undeclared_x_bodyparam).
    assert_eq!(
        one(".subckt s a\n.param foo=1\nr1 a 0 {foo*1000}\n.ends\nx1 1 s foo=3"),
        3000.0
    );
}

#[test]
fn bodies_are_lazy_s1_s2_p05f() {
    // Never instantiated: nothing in the body is evaluated or even
    // parsed (s2 sub_unused_undef_elem, sub_unused_syntax,
    // body_cycle_unused_sub; P05f).
    for deck in [
        ".subckt s a\nr1 a 0 {nosuch}\n.ends\nr2 1 0 1k",
        ".subckt s a\nr1 a 0 {1+}\n.ends\nr2 1 0 1k",
        ".subckt s a\n.param p={q}\n.param q={p}\nr1 a 0 1k\n.ends\nr2 1 0 1k",
        ".subckt s a w={1/0}\nr1 a 0 {w}\n.ends\nr2 1 0 1k",
        ".subckt s a\n.param w=\nq1 a\n.ends\nr2 1 0 1k",
    ] {
        let out = parse(deck);
        assert!(errors(&out).is_empty(), "{deck}: {:?}", out.diags);
    }
    // Instantiated: every `.param` of the instance counts, used or
    // not (s1 body_undef_unused, s2 body_cycle), element errors too
    // (s1 elem_undef_inst), reported once however many instances.
    for (deck, want) in [
        (
            ".subckt s a\n.param q={nosuch}\nr1 a 0 1k\n.ends\nx1 1 s\nx2 2 s",
            "unknown parameter `nosuch`",
        ),
        (
            ".subckt s a\n.param p={q}\n.param q={p}\nr1 a 0 1k\n.ends\nx1 1 s",
            "cycle",
        ),
        (
            ".subckt s a\nr1 a 0 {nosuch}\n.ends\nx1 1 s\nx2 2 s\nx3 3 s",
            "unknown parameter `nosuch`",
        ),
    ] {
        let e = errors(&parse(deck));
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].contains(want), "{deck}: {e:?}");
    }
    // An actual that is not finite fails only where used (s2
    // x_value_nonfinite(_used)).
    assert!(errors(&parse(".subckt s a w=1\nr1 a 0 1k\n.ends\nx1 1 s w={1/0}")).is_empty());
    let e = errors(&parse(
        ".subckt s a w=1\nr1 a 0 {1k+w*0}\n.ends\nx1 1 s w={1/0}",
    ));
    assert!(e[0].contains("not a finite number"), "{e:?}");
}

#[test]
fn models_are_lazy_and_per_instance_p08_s1() {
    // P08 / s1 model_local_inst: one copy per instance.
    let nl = clean(
        ".subckt rs a w=1\n.model rmod r rsh={w*100}\nr1 a 0 rmod l=1 w=1\n.ends\n\
         x1 n1 rs w=1\nx2 n2 rs w=3",
    );
    let rsh = |name: &str| {
        let m = nl.models.iter().find(|m| m.name == name).unwrap();
        param(&m.params, "rsh").unwrap()
    };
    assert_eq!(round(rsh("x1.rmod")), 100.0);
    assert_eq!(round(rsh("x2.rmod")), 300.0);
    match &element(&nl, "r.x2.r1").device {
        Device::Resistor { model, .. } => assert_eq!(model.as_deref(), Some("x2.rmod")),
        d => panic!("{d:?}"),
    }
    // Unused models are never evaluated, local (s1
    // model_undef_unused_local) or global (P05c, P05f); used ones are
    // (s1 model_undef_used_local, model_undef_used_top).
    for deck in [
        ".subckt s a\n.model rm r rsh={nosuch}\nr1 a 0 1k\n.ends\nx1 1 s",
        ".model rm r rsh={sqrt(-4)}\nr1 1 0 1k",
        ".model rmu r rsh={nosuch2}\nr1 1 0 1k",
    ] {
        let out = parse(deck);
        assert!(errors(&out).is_empty(), "{deck}: {:?}", out.diags);
        assert!(out.netlist.models.is_empty(), "{deck}");
    }
    for deck in [
        ".subckt s a\n.model rm r rsh={nosuch}\nr1 a 0 rm l=1 w=1\n.ends\nx1 1 s\nx2 2 s",
        ".model rm r rsh={nosuch}\nr1 1 0 rm l=1 w=1",
    ] {
        let e = errors(&parse(deck));
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].contains("unknown parameter `nosuch`"), "{deck}: {e:?}");
    }
    // A valid unused global model stays in the netlist.
    let nl = clean(".model rm r rsh=10\nr1 1 0 1k");
    assert_eq!(nl.models.len(), 1);
}

#[test]
fn given_ness_is_kept_p23() {
    // `ad={ad}` with `ad=0` still writes `ad` on the card: the device
    // sees it as given (sky130 passes `ad=0 as=0 pd=0 ps=0`).
    let nl = clean(
        ".model nm nmos level=1\n.subckt s d ad=0\nm1 d d 0 0 nm l=1u w=1u ad={ad}\n.ends\n\
         x1 1 s",
    );
    match &element(&nl, "m.x1.m1").device {
        Device::Mosfet { params, .. } => assert_eq!(param(params, "ad"), Some(0.0)),
        d => panic!("{d:?}"),
    }
    assert_eq!(
        one(".subckt s a\n.param ad=0 w=1\nr1 a 0 {1000+ad+w}\n.ends\nx1 1 s ad=5"),
        1006.0
    );
}

#[test]
fn recursion_and_depth_are_refused() {
    // ngspice H6 stops after ~22 levels; here the first repeat is an
    // error, direct or through another subcircuit.
    for deck in [
        ".subckt sa a\nxa a sa\n.ends\nx1 1 sa",
        ".subckt sa a\nxb a sb\n.ends\n.subckt sb a\nxa a sa\n.ends\nx1 1 sa",
    ] {
        let e = errors(&parse(deck));
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].contains("recursive"), "{deck}: {e:?}");
    }
}

const B1: &str =
    ".model mm.1 nmos level=49 version=3.3 lmin=0.1u lmax=1u wmin=1u wmax=10u vth0=0.3";
const B2: &str = ".model mm.2 nmos level=49 version=3.3 lmin=1u lmax=10u wmin=1u wmax=10u vth0=0.6";

fn picked(deck: &str, el: &str) -> String {
    let nl = clean(deck);
    match &element(&nl, el).device {
        Device::Mosfet { model, .. } => model.clone(),
        d => panic!("{d:?}"),
    }
}

#[test]
fn binning_rule_p09_s5() {
    let m = |b: &str, lw: &str| picked(&format!("{b}\nm1 1 2 0 0 mm {lw}"), "m1");
    // s5 b_mid1, b_edge_last_first, b_edge_rev_order: inclusive, the
    // most recently defined bin first.
    assert_eq!(m(&format!("{B1}\n{B2}"), "l=0.5u w=2u"), "mm.1");
    assert_eq!(m(&format!("{B1}\n{B2}"), "l=1u w=2u"), "mm.2");
    assert_eq!(m(&format!("{B2}\n{B1}"), "l=1u w=2u"), "mm.1");
    // s5 b_tol_in, b_tol_low: 1e-9 absolute tolerance.
    assert_eq!(m(B1, "l=1.0009u w=2u"), "mm.1");
    assert_eq!(m(B1, "l=0.0991u w=2u"), "mm.1");
    // s5 b_exact_wins, b_missing_limit, b_zero_padded, b_limit_expr,
    // b_lw_expr.
    assert_eq!(
        m(
            &format!("{B1}\n.model mm nmos level=49 version=3.3 vth0=0.9"),
            "l=0.5u w=2u"
        ),
        "mm"
    );
    assert_eq!(
        m(
            &format!(".model mm.1 nmos level=49 lmin=0.1u lmax=1u wmin=1u vth0=0.3\n{B2}"),
            "l=1u w=2u"
        ),
        "mm.2"
    );
    assert_eq!(
        m(
            ".model mm.01 nmos level=49 lmin=0.1u lmax=1u wmin=1u wmax=10u",
            "l=0.5u w=2u"
        ),
        "mm.01"
    );
    assert_eq!(
        m(
            ".param lx=1u\n.model mm.1 nmos level=49 lmin=0.1u lmax={lx} wmin=1u wmax=10u",
            "l=1u w=2u"
        ),
        "mm.1"
    );
    assert_eq!(
        m(&format!(".param ll=0.5u\n{B1}"), "l={ll} w={2*1u}"),
        "mm.1"
    );
    // BSIM4 levels bin too (the model itself is the elaborator's).
    assert_eq!(
        m(
            ".model mm.3 nmos level=54 lmin=0.1u lmax=1u wmin=1u wmax=10u",
            "l=0.5u w=2u"
        ),
        "mm.3"
    );
}

#[test]
fn binning_failures_p09c_s5() {
    for (deck, want) in [
        (format!("{B1}\nm1 1 2 0 0 mm l=1.0011u w=2u"), "could not find a valid model"),
        (format!("{B1}\nm1 1 2 0 0 mm l=50u w=2u"), "could not find a valid model"),
        (format!("{B1}\nm1 1 2 0 0 mm w=2u"), "gives no `l` and `w`"),
        (
            ".model mm.1 nmos level=49 lmin=1u lmax=0.1u wmin=1u wmax=10u\nm1 1 2 0 0 mm l=0.5u w=2u"
                .to_string(),
            "could not find a valid model",
        ),
    ] {
        let out = parse(&deck);
        let e = errors(&out);
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].contains(want), "{deck}: {e:?}");
        assert_eq!(out.sources.locate(out.diags[0].span.unwrap()).line, 3);
    }
    // Not a binnable type: no bin fits (s5 b_mos1, ngspice "could not
    // find a valid modelname"). Not digits (b_alpha_suffix): not a
    // family, the name stays and the elaborator reports it unknown.
    let e = errors(&parse(
        ".model mm.1 nmos level=1 lmin=0.1u lmax=1u wmin=1u wmax=10u\nm1 1 2 0 0 mm l=0.5u w=2u",
    ));
    assert!(e[0].contains("could not find a valid model"), "{e:?}");
    let out = parse(
        ".model mm.a nmos level=49 lmin=0.1u lmax=1u wmin=1u wmax=10u\nm1 1 2 0 0 mm l=0.5u w=2u",
    );
    assert!(errors(&out).is_empty(), "{:?}", out.diags);
}

#[test]
fn binning_scale_and_wnflag_s5() {
    // s5 b_scale, b_scale_tol: L and W times `scale`, limits not scaled.
    let deck = |b: &str, lw: &str| format!(".option scale=1u\n{b}\nm1 1 2 0 0 mm {lw}");
    assert_eq!(
        picked(&deck(&format!("{B1}\n{B2}"), "l=5 w=2"), "m1"),
        "mm.2"
    );
    assert_eq!(picked(&deck(B1, "l=1.0009 w=2"), "m1"), "mm.1");
    // W/nf only with wnflag (`INPgetModBin`; not observable with
    // BSIM3, which has no `nf`: unit-level only).
    let b2 = ".model mm.2 nmos level=54 lmin=0.1u lmax=1u wmin=10.5u wmax=20u";
    let b1 = ".model mm.1 nmos level=54 lmin=0.1u lmax=1u wmin=1u wmax=10u";
    let lw = "l=0.5u w=12u nf=2";
    assert_eq!(
        picked(&format!("{b1}\n{b2}\nm1 1 2 0 0 mm {lw}"), "m1"),
        "mm.2"
    );
    assert_eq!(
        picked(
            &format!(".option wnflag=1\n{b1}\n{b2}\nm1 1 2 0 0 mm {lw}"),
            "m1"
        ),
        "mm.1"
    );
    assert_eq!(
        picked(
            &format!(".option wnflag=1\n{b1}\n{b2}\nm1 1 2 0 0 mm {lw} wnflag=0"),
            "m1"
        ),
        "mm.2"
    );
}

#[test]
fn local_bin_families_s5() {
    // s5 b_local: bins inside the subcircuit, specialised per instance.
    let nl = clean(&format!(
        ".subckt s d g w=2u\n\
         .model mm.1 nmos level=49 lmin=0.1u lmax=1u wmin=1u wmax=10u vth0={{0.3+w*1e4}}\n\
         {B2}\nm1 d g 0 0 mm l=0.5u w={{w}}\n.ends\nx1 1 2 s w=3u\nx2 1 2 s"
    ));
    let vth0 = |name: &str| {
        let m = nl.models.iter().find(|m| m.name == name).unwrap();
        param(&m.params, "vth0").unwrap()
    };
    assert_eq!(round(vth0("x1.mm.1")), 0.33);
    assert_eq!(round(vth0("x2.mm.1")), 0.32);
    // Only the picked bin is evaluated and emitted.
    assert_eq!(nl.models.len(), 2, "{:?}", nl.models);
    // s5 b_local_shadow, b_local_exact_vs_glob_family,
    // b_local_family_vs_glob_exact: the instance's names win.
    let x = |inner: &str, outer: &str| {
        picked(
            &format!("{outer}\n.subckt s d g\n{inner}\nm1 d g 0 0 mm l=0.5u w=2u\n.ends\nx1 1 2 s"),
            "m.x1.m1",
        )
    };
    assert_eq!(x("", B1), "mm.1");
    assert_eq!(x(B1, B1), "x1.mm.1");
    assert_eq!(x(".model mm nmos level=49", B1), "x1.mm");
    assert_eq!(x(B1, ".model mm nmos level=49"), "x1.mm.1");
}

fn rval(nl: &Netlist, name: &str) -> f64 {
    r(nl, name)
}

#[test]
fn x_line_multiplier_s4_p13() {
    // P13 / s4 m_r: `m` multiplies; declared on the line or in the
    // body it is an ordinary parameter (m_declared, m_declared_body).
    assert_eq!(one(".subckt s a\nr1 a 0 1k\n.ends\nx1 1 s m=2"), 500.0);
    assert_eq!(one(".subckt s a m=1\nr1 a 0 1k\n.ends\nx1 1 s m=4"), 1000.0);
    assert_eq!(
        one(".subckt s a\n.param m=1\nr1 a 0 1k\n.ends\nx1 1 s m=4"),
        1000.0
    );
    // The device's own `m` is replaced, not multiplied (m_r_own,
    // m_r_own_otherinst — a call with `m=` anywhere marks the
    // subcircuit, so an instance without it gets m=1).
    let nl = clean(".subckt s a\nr1 a 0 1k m=2\n.ends\nx1 1 s m=3\nx2 2 s");
    assert_eq!(rval(&nl, "r.x1.r1"), 333.333333333);
    assert_eq!(rval(&nl, "r.x2.r1"), 1000.0);
    // `m` is visible in the body (m_expr_body), fractional (m_frac).
    assert_eq!(
        one(".subckt s a\nr1 a 0 {m*1000}\n.ends\nx1 1 s m=2"),
        1000.0
    );
    assert_eq!(one(".subckt s a\nr1 a 0 1k\n.ends\nx1 1 s m=0.5"), 2000.0);
    // Nested calls pass the caller's `m` down, replacing their own
    // (m_nested_both); a nested subcircuit defined first works here
    // (ngspice: "Mismatch: 0 formal but 1 actual", m_nested_def_before).
    let nl = clean(".subckt in a\nr1 a 0 1k\n.ends\n.subckt s a\nx1 a in m=3\n.ends\nx1 1 s m=2");
    assert_eq!(rval(&nl, "r.x1.x1.r1"), 500.0);
    let nl = clean(".subckt in a\nr1 a 0 1k\n.ends\n.subckt s a\nx1 a in\n.ends\nx1 1 s m=2");
    assert_eq!(rval(&nl, "r.x1.x1.r1"), 500.0);
    // m ≤ 0 is refused (ngspice simulates it: m=0 opens, m<0 negates).
    let e = errors(&parse(".subckt s a\nr1 a 0 1k\n.ends\nx1 1 s m=0"));
    assert!(e[0].contains("must be a positive number"), "{e:?}");
}

#[test]
fn x_line_multiplier_per_device_kind_s4() {
    let nl = clean(
        ".model dm d is=1e-14\n.model qm npn is=1e-15\n.model jm njf\n\
         .model b3 nmos level=49\n.model vb npn level=4\n\
         .subckt s a b\n\
         c1 a 0 1u\nl1 a b 1m\nd1 a 0 dm\nd2 a 0 dm 3\nq1 a b 0 qm\nq2 a b 0 vb\nj1 a b 0 jm\n\
         m1 a b 0 0 b3 l=1u w=1u\ng1 a 0 b 0 1m\nf1 a 0 vs 2\nvs b 0 0\ne1 a 0 b 0 2\n\
         i1 a 0 1m ac 2\nb1 a 0 i=v(b)\nb2 a 0 v=v(b)\n\
         .ends\nx1 1 2 s m=2",
    );
    let dev = |n: &str| &element(&nl, n).device;
    assert!(matches!(dev("c.x1.c1"), Device::Capacitor { value: Some(v), .. } if *v == 2e-6));
    assert!(matches!(dev("l.x1.l1"), Device::Inductor { value: Some(v), .. } if *v == 0.5e-3));
    assert!(matches!(dev("d.x1.d1"), Device::Diode { area: Some(a), .. } if *a == 2.0));
    assert!(matches!(dev("d.x1.d2"), Device::Diode { area: Some(a), .. } if *a == 6.0));
    assert!(matches!(dev("q.x1.q1"), Device::Bjt { area: Some(a), .. } if *a == 2.0));
    assert!(
        matches!(dev("q.x1.q2"), Device::Bjt { area: None, params, .. } if param(params, "m") == Some(2.0))
    );
    assert!(matches!(dev("j.x1.j1"), Device::Jfet { area: Some(a), .. } if *a == 2.0));
    assert!(
        matches!(dev("m.x1.m1"), Device::Mosfet { params, .. } if param(params, "m") == Some(2.0))
    );
    assert!(matches!(dev("g.x1.g1"), Device::Vccs { gain: Some(g) } if *g == 2e-3));
    assert!(matches!(dev("f.x1.f1"), Device::Cccs { gain: Some(g), .. } if *g == 4.0));
    assert!(matches!(dev("e.x1.e1"), Device::Vcvs { gain: Some(g) } if *g == 2.0));
    assert!(matches!(dev("v.x1.vs"), Device::Source { dc: Some(v), .. } if *v == 0.0));
    assert!(matches!(
        dev("i.x1.i1"),
        Device::Source { dc: Some(v), ac: Some(a), .. } if *v == 2e-3 && a.mag == 4.0
    ));
    assert!(matches!(
        dev("b.x1.b1"),
        Device::Behavioral { expr: Expr::Bin(ExprOp::Times, k, _), .. } if **k == Expr::Num(2.0)
    ));
    assert!(matches!(
        dev("b.x1.b2"),
        Device::Behavioral {
            expr: Expr::V(_) | Expr::Bin(ExprOp::Minus, ..),
            ..
        }
    ));
    // What cannot express it is a spanned error at the device.
    for (dev, what) in [
        (
            ".model nm nmos level=1\nm1 a a 0 0 nm l=1u w=1u",
            "MOS level 1",
        ),
        (".model rm r rsh=1\nr1 a 0 rm l=1 w=1", "model geometry"),
        ("i1 a 0 pulse(0 1 0 1n 1n 1n 2n)", "transient"),
    ] {
        let out = parse(&format!(".subckt s a\n{dev}\n.ends\nx1 1 s m=2"));
        let e = errors(&out);
        assert_eq!(e.len(), 1, "{dev}: {e:?}");
        assert!(
            e[0].contains("not supported yet") && e[0].contains(what),
            "{dev}: {e:?}"
        );
    }
    // ... unless m is 1 (another call passes `m`).
    let out =
        parse(".model nm nmos level=1\n.subckt s a\nm1 a a 0 0 nm l=1u w=1u\n.ends\nx1 1 s m=1");
    assert!(errors(&out).is_empty(), "{:?}", out.diags);
}
