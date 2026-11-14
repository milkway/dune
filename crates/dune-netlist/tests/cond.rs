//! Conditional blocks (`.if`/`.elseif`/`.else`/`.endif`). Each test
//! cites the ngspice-47 probe it reproduces: `iN case` is case `case`
//! of `.claude/scratch/g9-probes/iN.txt` (rules in
//! `docs/dune-netlist.md`, "Condicionais").

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

/// Resistor values by element name.
fn rs(deck: &str) -> Vec<(String, f64)> {
    clean(deck)
        .elements
        .iter()
        .filter_map(|e| match &e.device {
            Device::Resistor { value: Some(v), .. } => {
                Some((e.name.clone(), format!("{v:.11e}").parse().unwrap()))
            }
            _ => None,
        })
        .collect()
}

fn one(deck: &str) -> f64 {
    let r = rs(deck);
    assert_eq!(r.len(), 1, "{deck}: {r:?}");
    r[0].1
}

fn err(deck: &str) -> String {
    let e = errors(&parse(deck));
    assert!(!e.is_empty(), "{deck}: expected an error");
    e.join(" | ")
}

const AB: &str = "r1 1 0 1k\n.else\nr1 1 0 2k\n.endif";

#[test]
fn condition_forms_i1() {
    // i_top_true, i_top_false, i_glued, i_spaces, i_single_eq,
    // i_value(_zero), i_half, i_neg, i_logic(_or), i_func_cond,
    // i_udf_cond, i_case, i_continuation, i_cond_ternary,
    // i_cond_nested_parens, i_cond_float_eq
    let cases: &[(&str, f64)] = &[
        (".param a=1\n.if (a==1)", 1000.0),
        (".param a=0\n.if (a==1)", 2000.0),
        (".param a=1\n.if(a==1)", 1000.0),
        (".param a=1\n.if ( a == 1 )", 1000.0),
        (".param a=1\n.if (a=1)", 1000.0),
        (".param a=3\n.if (a)", 1000.0),
        (".param a=0\n.if (a)", 2000.0),
        (".if (0.5)", 1000.0),
        (".if (-1)", 1000.0),
        (".param a=1 b=2\n.if (a==1 && b<2)", 2000.0),
        (".param a=1 b=2\n.if (a==2 || b==2)", 1000.0),
        (
            ".param ng=3\n.if (floor(floor(ng/2+0.501)*2+0.001) != ng)",
            1000.0,
        ),
        (
            ".func odd(n) {floor(n/2)*2 != n}\n.param ng=3\n.if (odd(ng))",
            1000.0,
        ),
        (".param a=1\n.IF (a==1)", 1000.0),
        (".param a=1\n.if (a==\n+ 1)", 1000.0),
        (".param a=1\n.if (a ? 0 : 1)", 2000.0),
        (".param a=1\n.if ((a==1) && (a<2))", 1000.0),
        (".param a=0.1\n.if (a*3==0.3)", 2000.0),
    ];
    for (head, want) in cases {
        let deck = format!("{head}\n{AB}");
        assert_eq!(one(&deck), *want, "{deck}");
    }
    // i_fwd_param: a parameter defined after the block.
    assert_eq!(one(&format!(".if (a==1)\n{AB}\n.param a=1")), 1000.0);
}

#[test]
fn chains_and_nesting_i1() {
    let chain = "r1 1 0 1k\n.elseif (a==2)\nr1 1 0 2k\n.else\nr1 1 0 3k\n.endif";
    // i_elseif, i_elseif_else, i_elseif_first, i_elseif_two
    assert_eq!(one(&format!(".param a=2\n.if (a==1)\n{chain}")), 2000.0);
    assert_eq!(one(&format!(".param a=5\n.if (a==1)\n{chain}")), 3000.0);
    assert_eq!(
        one(".param a=1\n.if (a==1)\nr1 1 0 1k\n.elseif (a>0)\nr1 1 0 2k\n.endif"),
        1000.0
    );
    assert_eq!(
        one(
            ".param a=3\n.if (a==1)\nr1 1 0 1k\n.elseif (a==2)\nr1 1 0 2k\n.elseif (a==3)\n\
             r1 1 0 4k\n.endif"
        ),
        4000.0
    );
    // i5 i_else_if_*: `.else if (c)` is a plain `.else` in ngspice.
    assert!(
        err(".param a=3\n.if (a==1)\nr1 1 0 1k\n.else if (a==2)\nr1 1 0 2k\n.endif")
            .contains("plain")
    );
    // i_no_else, i_nested, i_nested_false_outer, i_indented
    assert_eq!(
        one(".param a=0\nr2 1 0 3k\n.if (a==1)\nr1 1 0 1k\n.endif"),
        3000.0
    );
    let nested = ".if (a==1)\n.if (b==1)\nr1 1 0 1k\n.else\nr1 1 0 2k\n.endif\n.else\n\
                  r1 1 0 3k\n.endif";
    assert_eq!(one(&format!(".param a=1 b=0\n{nested}")), 2000.0);
    assert_eq!(one(&format!(".param a=0 b=1\n{nested}")), 3000.0);
    assert_eq!(
        one(".param a=1\n  .if (a==1)\n    r1 1 0 1k\n  .else\n    r1 1 0 2k\n  .endif"),
        1000.0
    );
    // i2 i_deep: 21 levels.
    let deep = format!(
        "{}r1 1 0 1k\n{}",
        ".if (1)\n".repeat(21),
        ".endif\n".repeat(21)
    );
    assert_eq!(one(&deep), 1000.0);
}

#[test]
fn what_is_conditional_i1_i2_i3() {
    // Conditional: models (i3 i_model_top_a/b), X lines (i2
    // i_x_top_false), .temp (i4 i_d_temp_*), analysis cards.
    let m = ".model rm r rsh=1\n.else\n.model rm r rsh=2\n.endif\nr1 1 0 rm l=1k w=1";
    let nl = clean(&format!(".param a=1\n.if (a==0)\n{m}"));
    assert_eq!(nl.models.len(), 1);
    assert!(matches!(nl.models[0].params[0].value, ParamValue::Num(v) if v == 2.0));
    let nl = clean(
        ".subckt leaf a\nr1 a 0 1k\n.ends\n.param a=0\nr2 1 0 2k\n.if (a==1)\nx1 1 leaf\n.endif",
    );
    assert_eq!(nl.elements.len(), 1);
    let nl = clean(".param a=0\n.if (a==1)\n.temp 50\n.else\n.temp 100\n.endif\nr1 1 0 1k");
    let temp = nl.options.iter().rev().find(|p| p.name == "temp").unwrap();
    assert!(matches!(temp.value, ParamValue::Num(v) if v == 100.0));
    let nl = clean(".param a=0\nr1 1 0 1k\n.if (a==1)\n.tran 1u 2u\n.endif\n.op");
    assert_eq!(nl.analyses.len(), 1);
    // Unconditional: .param (i_param_in_branch(_false)), .func
    // (i_func_in), .options (i4 i_d_options_*), .subckt definitions
    // (i2 i_subckt_def_false: the first one wins).
    let p = ".if (a==1)\n.param b=4\n.else\n.param b=5\n.endif\nr1 1 0 {b*1000}";
    assert_eq!(one(&format!(".param a=1\n{p}")), 5000.0);
    assert_eq!(
        one(
            ".param a=1\n.if (a==1)\n.func f(x) {x*2}\n.else\n.func f(x) {x*3}\n.endif\nr1 1 0 {f(1)*1000}"
        ),
        3000.0
    );
    let nl = clean(
        ".param a=1\n.if (a==1)\n.options temp=50\n.else\n.options temp=100\n.endif\nr1 1 0 1k",
    );
    let temp = nl.options.iter().rev().find(|p| p.name == "temp").unwrap();
    assert!(matches!(temp.value, ParamValue::Num(v) if v == 100.0));
    assert_eq!(
        one(
            ".param a=0\n.if (a==1)\n.subckt leaf b\nr1 b 0 1k\n.ends\n.else\n.subckt leaf b\n\
             r1 b 0 2k\n.ends\n.endif\nx1 1 leaf"
        ),
        1000.0
    );
}

#[test]
fn per_instance_in_subcircuits_i1_i2() {
    let s = ".subckt s a sel=0\n.if (sel==1)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n.ends\n";
    // i_sub_param, i_sub_param2: each instance picks its branch.
    let r = rs(&format!("{s}x1 1 s sel=1\nx2 1 s"));
    assert_eq!(
        r,
        vec![("r.x1.r1".into(), 1000.0), ("r.x2.r1".into(), 2000.0)]
    );
    // i_sub_glob, i_sub_body_param, i_sub_cond_fwd_body, i_sub_dyn, i_cond_m
    assert_eq!(
        one(
            ".param g=1\n.subckt s a\n.if (g==1)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n.ends\nx1 1 s"
        ),
        1000.0
    );
    assert_eq!(
        one(
            ".subckt s a w=1\n.param q={w*2}\n.if (q>3)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n\
             .ends\nx1 1 s w=2"
        ),
        1000.0
    );
    assert_eq!(
        one(
            ".subckt s a\n.if (q==4)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n.param q=4\n.ends\nx1 1 s"
        ),
        1000.0
    );
    assert_eq!(
        one(
            ".subckt leaf a\n.if (sel==1)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n.ends\n\
             .subckt s a sel=0\nx1 a leaf\n.ends\nx1 1 s sel=1"
        ),
        1000.0
    );
    // i_sub_model: per-instance conditional models with one name.
    let nl = clean(
        ".subckt s a sel=0\n.if (sel==1)\n.model rm r rsh=1\n.else\n.model rm r rsh=2\n.endif\n\
         r1 a 0 rm l=1k w=1\n.ends\nx1 1 s sel=1\nx2 1 s",
    );
    let rsh: Vec<f64> = nl
        .models
        .iter()
        .map(|m| match m.params[0].value {
            ParamValue::Num(v) => v,
            _ => f64::NAN,
        })
        .collect();
    assert_eq!(rsh, vec![1.0, 2.0]);
    // i_sub_x, i_sub_nested_subckt(_false)
    assert_eq!(
        one(
            ".subckt leaf a\nr1 a 0 1k\n.ends\n.subckt s a sel=0\n.if (sel==1)\nx1 a leaf\n.else\n\
             r1 a 0 2k\n.endif\n.ends\nx1 1 s sel=1"
        ),
        1000.0
    );
    assert_eq!(
        one(
            ".subckt s a sel=0\n.if (sel==1)\n.subckt leaf b\nr1 b 0 1k\n.ends\n.else\n\
             .subckt leaf b\nr1 b 0 2k\n.ends\n.endif\nx1 a leaf\n.ends\nx1 1 s sel=0"
        ),
        1000.0
    );
    // i_sub_ihp: the IHP nesting, ng=2 takes the even path.
    let ihp = ".subckt s a w=1 as=0 ng=1 rfmode=0\n.if (rfmode == 0)\n    .if (as <= 1e-50)\n\
               \x20       .if (floor(floor(ng/2+0.501)*2+0.001) != ng)\n            r1 a 0 1k\n\
               \x20       .else\n            r1 a 0 2k\n        .endif\n    .else\n\
               \x20       r1 a 0 3k\n    .endif\n.else\n    r1 a 0 4k\n.endif\n.ends\n";
    assert_eq!(one(&format!("{ihp}x1 1 s ng=2")), 2000.0);
    assert_eq!(one(&format!("{ihp}x1 1 s ng=1")), 1000.0);
    assert_eq!(one(&format!("{ihp}x1 1 s as=1")), 3000.0);
    assert_eq!(one(&format!("{ihp}x1 1 s rfmode=1")), 4000.0);
    // i_sub_undef_unused: an unused subcircuit's conditions are not read.
    assert_eq!(
        one(".subckt s a\n.if (nosuch==1)\nr1 a 0 1k\n.endif\n.ends\nr1 1 0 1k"),
        1000.0
    );
}

#[test]
fn every_condition_is_evaluated_i1_i2() {
    // i_undef, i_undef_untaken_elseif, i_nested_untaken_undef,
    // i_taken_then_elseif_bad, i_sub_undef_used, i_sub_param_cond_top,
    // i_temper
    for deck in [
        format!(".if (nosuch==1)\n{AB}"),
        ".param a=1\n.if (a==1)\nr1 1 0 1k\n.elseif (nosuch==1)\nr1 1 0 2k\n.endif".into(),
        ".if (0)\n.if (nosuch==1)\nr1 1 0 1k\n.endif\n.endif\nr2 1 0 2k".into(),
        ".subckt s a\n.if (nosuch==1)\nr1 a 0 1k\n.else\nr1 a 0 2k\n.endif\n.ends\nx1 1 s".into(),
        ".subckt s a w=1\nr1 a 0 1k\n.ends\n.if (w==1)\nr2 1 0 1k\n.endif\nx1 1 s".into(),
    ] {
        assert!(err(&deck).contains("unknown parameter"), "{deck}");
    }
    assert!(!err(".if (1)\nr1 1 0 1k\n.elseif (1+)\nr1 1 0 2k\n.endif").is_empty());
}

#[test]
fn malformed_and_unbalanced_are_spanned_errors_i1() {
    let cases: &[(&str, &str)] = &[
        // fatal in ngspice too: i_noparen, i_brace, i_quote, i_no_cond,
        // i_empty_cond, i_bad_cond, i_elseif_noparen,
        // i_cond_outer_paren_split
        (".param a=1\n.if a==1\nr1 1 0 1k\n.endif", "parentheses"),
        (".param a=1\n.if {a==1}\nr1 1 0 1k\n.endif", "parentheses"),
        (".param a=1\n.if 'a==1'\nr1 1 0 1k\n.endif", "parentheses"),
        (".if\nr1 1 0 1k\n.endif", "parentheses"),
        (".if ()\nr1 1 0 1k\n.endif", "unexpected"),
        (
            ".param a=2\n.if (a==1)\nr1 1 0 1k\n.elseif a==2\nr1 1 0 2k\n.endif",
            "parentheses",
        ),
        (
            ".param a=1\n.if (a==1) && (a<2)\nr1 1 0 1k\n.endif",
            "after the condition",
        ),
        // i_endif_alone, i_else_alone, i_else_twice, i_elseif_after_else,
        // i_sub_cross
        ("r1 1 0 1k\n.endif", ".endif without .if"),
        ("r1 1 0 1k\n.else", ".else without .if"),
        (
            ".param a=0\n.if (a==1)\nr1 1 0 1k\n.else\nr1 1 0 2k\n.else\nr1 1 0 3k\n.endif",
            "second .else",
        ),
        (
            ".param a=0\n.if (a==1)\nr1 1 0 1k\n.else\nr1 1 0 2k\n.elseif (a==0)\nr1 1 0 3k\n.endif",
            "after .else",
        ),
        (
            ".subckt s a\n.if (1)\nr1 a 0 1k\n.ends\n.endif\nx1 1 s",
            "without",
        ),
        // accepted by ngspice, errors here: i_unclosed, i_sub_unclosed,
        // i_trailing_endif, i_trailing_else, i_cond_trailing, i_cond_inf
        (".param a=1\n.if (a==1)\nr1 1 0 1k", "without .endif"),
        (
            ".subckt s a\n.if (1)\nr1 a 0 1k\n.ends\nx1 1 s",
            "without .endif",
        ),
        (
            ".param a=1\n.if (a==1)\nr1 1 0 1k\n.endif junk",
            "after .endif",
        ),
        (
            ".param a=0\n.if (a==1)\nr1 1 0 1k\n.else junk\nr1 1 0 2k\n.endif",
            "after .else",
        ),
        (
            ".param a=1\n.if (a==1) junk\nr1 1 0 1k\n.endif",
            "after the condition",
        ),
        (".if (1/0)\nr1 1 0 1k\n.endif", "not a finite number"),
    ];
    for (deck, want) in cases {
        let out = parse(deck);
        let e: Vec<_> = out
            .diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            e.iter()
                .any(|d| d.message.contains(want) && d.span.is_some()),
            "{deck}: {e:?}"
        );
    }
}

#[test]
fn hostile_nesting_is_bounded() {
    // 10 000 nested blocks: one error, linear time, no recursion.
    let n = 10_000;
    let deck = format!(
        "{}r1 1 0 1k\n{}",
        ".if (1)\n".repeat(n),
        ".endif\n".repeat(n)
    );
    let e = err(&deck);
    assert!(e.contains("nested deeper"), "{e}");
    // A subcircuit with many branches instantiated many times is
    // charged to the expansion budget.
    let mut body = String::from(".subckt s a k=1\n");
    for i in 0..2000 {
        body.push_str(&format!(".if (k=={i})\nr{i} a 0 1k\n.endif\n"));
    }
    body.push_str(".ends\n");
    for i in 0..2000 {
        body.push_str(&format!("x{i} 1 s k={i}\n"));
    }
    let out = parse(&body);
    assert!(
        errors(&out).iter().any(|e| e.contains("exceeds")),
        "{:?}",
        errors(&out)
    );
}
