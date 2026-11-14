//! User functions (`.func`, and `.param name(args) = body`). Each test
//! cites the ngspice-47 probe it reproduces: `fN case` is case `case`
//! of `.claude/scratch/g9-probes/fN.txt` (rules in
//! `docs/dune-netlist.md`, "Funções do usuário").

use std::time::{Duration, Instant};

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

/// The resistance of the (single) resistor named `name`, to 12 digits.
fn r(deck: &str, name: &str) -> f64 {
    let out = parse(deck);
    let e = errors(&out);
    assert!(e.is_empty(), "{deck}\n{e:?}");
    let el = out
        .netlist
        .elements
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no `{name}`"));
    match &el.device {
        Device::Resistor { value: Some(v), .. } => format!("{v:.11e}").parse().unwrap(),
        d => panic!("{d:?}"),
    }
}

fn err(deck: &str) -> String {
    let e = errors(&parse(deck));
    assert!(!e.is_empty(), "{deck}: expected an error");
    e.join(" | ")
}

#[test]
fn definition_forms_f1() {
    // f_brace, f_eq, f_eq_bare, f_bare, f_quote, f_space_name,
    // f_body_two_tokens, f_continuation, f_indented, f_params_kw(_quote)
    for def in [
        ".func f(x) {x*2}",
        ".func f(x) = {x*2}",
        ".func f(x) = x*2",
        ".func f(x) x*2",
        ".func f(x) 'x*2'",
        ".func f (x) {x*2}",
        ".func f(x) x * 2",
        ".func f(x)\n+ {x*2}",
        "  .FUNC F(X) {X*2}",
        ".param f(x) = {x*2}",
        ".param f(x) = 'x*2'",
    ] {
        assert_eq!(
            r(&format!("{def}\nr1 1 0 {{f(3)*1000}}"), "r1"),
            6000.0,
            "{def}"
        );
    }
    assert_eq!(
        r(".func g( a , b ) {a*10+b}\nr1 1 0 {g(3,4)*1000}", "r1"),
        34000.0
    );
    assert_eq!(r(".func k() {7}\nr1 1 0 {k()*1000}", "r1"), 7000.0);
    assert_eq!(r(".func f(Ab) {ab*2}\nr1 1 0 {F(3)*1000}", "r1"), 6000.0);
}

#[test]
fn malformed_definitions_are_errors_f1() {
    // f_noargs_def, f_comma_empty, f_unbalanced: fatal in ngspice too.
    assert!(err(".func f {2}\nr1 1 0 1k").contains("parentheses"));
    assert!(err(".func f(x,) {x*2}\nr1 1 0 1k").contains("a name was expected"));
    assert!(err(".func f(x {x*2}\nr1 1 0 1k").contains("not closed"));
    // f_arg_same_names: ngspice silently takes the first.
    assert!(err(".func f(x,x) {x*2}\nr1 1 0 1k").contains("twice"));
    // f_body_empty: accepted while unused, an error when called.
    assert_eq!(r(".func f(x)\nr1 1 0 1k", "r1"), 1000.0);
    assert!(err(".func f(x)\nr1 1 0 {f(1)}").contains("no body"));
}

#[test]
fn calls_everywhere_f1() {
    let f = ".func f(x) {x*2}\n";
    // f_in_param, f_in_param_bare, f_in_quote, f_kw_bare, f_in_model,
    // f_in_subckt_line, f_in_x_line, f_nested_call, f_in_func(_fwd)
    assert_eq!(
        r(&format!("{f}.param p={{f(3)}}\nr1 1 0 {{p*1000}}"), "r1"),
        6000.0
    );
    assert_eq!(
        r(&format!("{f}.param p=f(3)\nr1 1 0 {{p*1000}}"), "r1"),
        6000.0
    );
    assert_eq!(r(&format!("{f}r1 1 0 'f(3)*1000'"), "r1"), 6000.0);
    assert_eq!(r(&format!("{f}r1 1 0 {{f(f(3))*1000}}"), "r1"), 12000.0);
    assert_eq!(
        r(
            &format!("{f}.subckt s a w={{f(2)}}\nr1 a 0 {{w*1000}}\n.ends\nx1 1 s"),
            "r.x1.r1"
        ),
        4000.0
    );
    assert_eq!(
        r(
            &format!("{f}.subckt s a w=1\nr1 a 0 {{w*1000}}\n.ends\nx1 1 s w={{f(3)}}"),
            "r.x1.r1"
        ),
        6000.0
    );
    assert_eq!(
        r(
            ".func g(x) {f(x)+1}\n.func f(x) {x*2}\nr1 1 0 {g(3)*1000}",
            "r1"
        ),
        7000.0
    );
    // f_after_use
    assert_eq!(r("r1 1 0 {f(3)*1000}\n.func f(x) {x*2}", "r1"), 6000.0);
    // f_arg_expr, f_arg_neg, f_body_neg, f_body_pow, f_pow_arg, f_arg_param_neg
    assert_eq!(r(".func f(x) {x*x}\nr1 1 0 {f(1+2)*1000}", "r1"), 9000.0);
    assert_eq!(r(".func f(x) {x*x}\nr1 1 0 {f(-3)*1000}", "r1"), 9000.0);
    assert_eq!(r(".func f(x) {-x}\nr1 1 0 {-f(3)*1000}", "r1"), 3000.0);
    assert_eq!(r(".func f(x) {x^2}\nr1 1 0 {f(-3)*1000}", "r1"), 9000.0);
    assert_eq!(r(".func f(x) {2^x}\nr1 1 0 {f(-1)*1000}", "r1"), 500.0);
    assert_eq!(
        r(".param a=3\n.func f(x) {2*x}\nr1 1 0 {-f(-a)*1000}", "r1"),
        6000.0
    );
}

#[test]
fn arity_and_unknown_names_f1() {
    // f_arity_more, f_arity_less, f_redef_arity_old, f_unknown_fn,
    // f_zero_noparen_use
    assert!(err(".func f(x) {x*2}\nr1 1 0 {f(3,4)}").contains("takes 1 argument"));
    assert!(err(".func g(a,b) {a+b}\nr1 1 0 {g(3)}").contains("takes 2 argument"));
    assert!(
        err(".func f(x) {x*2}\n.func f(x,y) {x*y}\nr1 1 0 {f(3)}").contains("takes 2 argument")
    );
    assert!(err("r1 1 0 {nosuch(3)}").contains("unknown function `nosuch`"));
    assert!(err(".func k() {7}\nr1 1 0 {k*1000}").contains("unknown parameter `k`"));
}

#[test]
fn names_and_shadowing_f1() {
    // f_glob_param, f_glob_param_after, f_param_uses_func_fwd
    assert_eq!(
        r(".param k=5\n.func f(x) {x*k}\nr1 1 0 {f(3)*1000}", "r1"),
        15000.0
    );
    assert_eq!(
        r(".func f(x) {x*k}\n.param k=5\nr1 1 0 {f(3)*1000}", "r1"),
        15000.0
    );
    assert_eq!(
        r(
            ".param p={f(3)}\n.func f(x) {x*k}\n.param k=5\nr1 1 0 {p*1000}",
            "r1"
        ),
        15000.0
    );
    // f_shadow_param, f_shadow_builtin, f_shadow_builtin_arity,
    // f_redef, f_name_is_param, f_arg_prefix, f_arg_in_fname
    assert_eq!(
        r(".param x=100\n.func f(x) {x*2}\nr1 1 0 {f(3)*1000}", "r1"),
        6000.0
    );
    assert_eq!(
        r(".func sqrt(x) {x*2}\nr1 1 0 {sqrt(9)*1000}", "r1"),
        18000.0
    );
    assert_eq!(
        r(".func min(a,b,c) {a+b+c}\nr1 1 0 {min(1,2,3)*1000}", "r1"),
        6000.0
    );
    assert_eq!(
        r(
            ".func f(x) {x*2}\n.func f(x) {x*3}\nr1 1 0 {f(3)*1000}",
            "r1"
        ),
        9000.0
    );
    assert_eq!(
        r(".param f=2\n.func f(x) {x*3}\nr1 1 0 {f(2)*1000+f}", "r1"),
        6002.0
    );
    assert_eq!(
        r(".func f(x) {x2*x}\n.param x2=10\nr1 1 0 {f(3)*100}", "r1"),
        3000.0
    );
    assert_eq!(r(".func f(a) {abs(a)}\nr1 1 0 {f(-3)*1000}", "r1"), 3000.0);
    // Unused bodies are not evaluated: f_undef_in_body_unused,
    // f_syntax_unused; used, the error points into the body.
    assert_eq!(r(".func f(x) {x*nosuch}\nr1 1 0 1k", "r1"), 1000.0);
    assert_eq!(r(".func f(x) {x**+}\nr1 1 0 1k", "r1"), 1000.0);
    let e = err(".func f(x) {x*nosuch}\nr1 1 0 {f(1)}");
    assert!(
        e.contains("unknown parameter `nosuch` (in function `f`)"),
        "{e}"
    );
}

#[test]
fn subcircuit_scope_f1_f2() {
    // f_local, f_local_leak, f_local_shadow, f_local_shadow_top
    let local = ".subckt s a\n.func f(x) {x*2}\nr1 a 0 {f(3)*1000}\n.ends\nx1 1 s";
    assert_eq!(r(local, "r.x1.r1"), 6000.0);
    assert!(
        err(".subckt s a\n.func f(x) {x*2}\nr1 a 0 1k\n.ends\nx1 1 s\nr2 1 0 {f(3)}")
            .contains("unknown function `f`")
    );
    let shadow = ".func f(x) {x*2}\n.subckt s a\n.func f(x) {x*5}\nr1 a 0 {f(3)*1000}\n.ends\n\
                  x1 1 s\nr2 1 0 {f(3)*1000}";
    assert_eq!(r(shadow, "r.x1.r1"), 15000.0);
    assert_eq!(r(shadow, "r2"), 6000.0);
    // f_local_sees_inst_param, f_glob_param_shadow_inst: a body's other
    // names are read at the call site.
    assert_eq!(
        r(
            ".func f(x) {x*w}\n.subckt s a w=1\nr1 a 0 {f(3)*1000}\n.ends\nx1 1 s w=4",
            "r.x1.r1"
        ),
        12000.0
    );
    // f_local_lexical (not seen by a subcircuit it instantiates),
    // f_local_nested_def (seen by one defined inside it)
    assert!(
        err(
            ".subckt outer a\n.func f(x) {x*2}\nx1 a inner\n.ends\n.subckt inner b\n\
             r1 b 0 {f(3)}\n.ends\nx1 1 outer"
        )
        .contains("unknown function `f`")
    );
    assert_eq!(
        r(
            ".subckt outer a\n.func f(x) {x*2}\n.subckt inner b\nr1 b 0 {f(3)*1000}\n.ends\n\
             x1 a inner\n.ends\nx1 1 outer",
            "r.x1.x1.r1"
        ),
        6000.0
    );
    // f_local_param_body, f_local_redef_after, f_local_param_self
    assert_eq!(
        r(
            ".subckt s a w=2\n.func f(x) {x*w}\n.param q={f(5)}\nr1 a 0 {q*100}\n.ends\nx1 1 s w=3",
            "r.x1.r1"
        ),
        1500.0
    );
    assert_eq!(
        r(
            ".subckt s a\nr1 a 0 {f(3)*1000}\n.func f(x) {x*2}\n.ends\nx1 1 s",
            "r.x1.r1"
        ),
        6000.0
    );
    assert_eq!(
        r(
            ".func f(x) {x*w}\n.subckt s a w=2\n.param w={f(3)}\nr1 a 0 {w*1000}\n.ends\n\
             .param w=5\nx1 1 s",
            "r.x1.r1"
        ),
        15000.0
    );
    // f_top_calls_local, f_top_calls_top_shadowed
    assert_eq!(
        r(
            ".func f(x) {g(x)*2}\n.subckt s a\n.func g(x) {x+1}\nr1 a 0 {f(3)*1000}\n.ends\nx1 1 s",
            "r.x1.r1"
        ),
        8000.0
    );
    assert_eq!(
        r(
            ".func g(x) {x+10}\n.func f(x) {g(x)*2}\n.subckt s a\n.func g(x) {x+1}\n\
             r1 a 0 {f(3)*1000}\n.ends\nx1 1 s",
            "r.x1.r1"
        ),
        26000.0
    );
    // f_local_unused_sub: an unused subcircuit's recursion is not seen.
    assert_eq!(
        r(".subckt s a\n.func f(x) {f(x)}\n.ends\nr1 1 0 1k", "r1"),
        1000.0
    );
}

#[test]
fn local_models_calling_functions_do_not_share_f2() {
    // f_model_local_func: rsh = f(100) = 100·w differs per instance.
    let out = parse(
        ".subckt s a w=1\n.func f(x) {x*w}\n.model rm r rsh={f(100)}\nr1 a 0 rm l=1 w=1\n.ends\n\
         x1 1 s w=2\nx2 1 s w=3",
    );
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    let rsh: Vec<f64> = out
        .netlist
        .models
        .iter()
        .filter_map(|m| {
            m.params
                .iter()
                .find(|p| p.name == "rsh")
                .and_then(|p| match p.value {
                    ParamValue::Num(v) => Some(v),
                    _ => None,
                })
        })
        .collect();
    assert_eq!(rsh.len(), 2, "{:?}", out.netlist.models);
    assert!(
        (rsh[0] - 200.0).abs() < 1e-9 && (rsh[1] - 300.0).abs() < 1e-9,
        "{rsh:?}"
    );
}

#[test]
fn recursion_is_a_spanned_error_f1() {
    // f_recursive, f_mutual, f_recursive_unused: ngspice-47 crashes.
    for deck in [
        ".func f(x) {f(x)+1}\nr1 1 0 {f(3)}",
        ".func f(x) {g(x)+1}\n.func g(x) {f(x)+1}\nr1 1 0 {f(3)}",
        ".func f(x) {f(x)+1}\nr1 1 0 1k",
        ".subckt s a\n.func f(x) {f(x)+1}\nr1 a 0 {f(1)}\n.ends\nx1 1 s",
    ] {
        let out = parse(deck);
        let e: Vec<_> = out
            .diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(e.len(), 1, "{deck}: {e:?}");
        assert!(e[0].message.contains("recursive function"), "{deck}");
        assert!(e[0].span.is_some());
    }
}

#[test]
fn b_sources_say_not_supported() {
    // f_bsrc_bare: ngspice expands `.func` in B lines too.
    let e = err(".func f(x) {x*2}\nb1 1 0 v=f(3)\nr1 1 0 1");
    assert!(e.contains("not supported yet"), "{e}");
}

#[test]
fn hostile_functions_are_bounded() {
    // Doubling chain: f0 = x, f(k) = f(k-1)(x) + f(k-1)(x): 2^60 calls.
    let mut deck = String::from(".func f0(x) {x}\n");
    for k in 1..=60 {
        deck.push_str(&format!(".func f{k}(x) {{f{}(x)+f{}(x)}}\n", k - 1, k - 1));
    }
    deck.push_str("r1 1 0 {f60(1)}\n");
    let t = Instant::now();
    let e = err(&deck);
    assert!(e.contains("exceeds"), "{e}");
    assert!(t.elapsed() < Duration::from_secs(20));

    // A long non-recursive chain stops at the depth bound.
    let mut deck = String::from(".func g0(x) {x+1}\n");
    for k in 1..=400 {
        deck.push_str(&format!(".func g{k}(x) {{g{}(x)}}\n", k - 1));
    }
    deck.push_str("r1 1 0 {g400(1)}\n");
    assert!(err(&deck).contains("nested deeper"));

    // Many parameters calling one function: the budget is deck-wide.
    let mut deck = String::from(".func h(x) {x+1}\n");
    for k in 0..20000 {
        deck.push_str(&format!(".param p{k}={{h({k})}}\n"));
    }
    deck.push_str("r1 1 0 {p19999}\n");
    assert_eq!(r(&deck, "r1"), 20000.0);

    // A cycle through the call-site fallback is caught by the depth
    // bound, not by a crash.
    let e = err(".func f(x) {g(x)}\n.subckt s a\n.func g(x) {f(x)}\nr1 a 0 {f(1)}\n.ends\nx1 1 s");
    assert!(e.contains("nested deeper"), "{e}");
}
