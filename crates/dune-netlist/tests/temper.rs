//! `temper` in parameter expressions (G7 of `docs/pdk-parser.md`).
//! Each test cites the ngspice-47 probe it reproduces: `tN case` is
//! case `case` of `.claude/scratch/g9-probes/tN.txt` (rules in
//! `docs/dune-netlist.md`, "`temper`").

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

/// The value of resistor `name`, to 12 digits. A resistor whose value
/// depends on `temper` is rewritten by ngspice-47 into the B source
/// `bNAME n1 n2 i = v(n1, n2)/(value)` (`inp_compat`: probes
/// `behav-probes b1 r_temper_save` — `@br1[i]` exists, `@r1[i]` does
/// not — and `r_temper_noise`, where it has no thermal noise), and so
/// by Dune (`behav.rs`): then the value is the B source's divisor,
/// which must be a constant tree.
fn r(deck: &str, name: &str) -> f64 {
    let nl = clean(deck);
    let v = match nl.elements.iter().find(|e| e.name == name) {
        Some(el) => match &el.device {
            Device::Resistor { value: Some(v), .. } => *v,
            d => panic!("{d:?}"),
        },
        None => behavioural_r(&nl, name),
    };
    format!("{v:.11e}").parse().unwrap()
}

/// The constant resistance of the B source ngspice makes of resistor
/// `name` (`r.x1.r1` → `b.x1.br1`, `r1` → `br1`).
fn behavioural_r(nl: &Netlist, name: &str) -> f64 {
    let bname = match name.split_once('.') {
        Some((_, rest)) => {
            let (path, leaf) = rest.rsplit_once('.').unwrap();
            format!("b.{path}.b{leaf}")
        }
        None => format!("b{name}"),
    };
    let names: Vec<&str> = nl.elements.iter().map(|e| e.name.as_str()).collect();
    let el = nl
        .elements
        .iter()
        .find(|e| e.name == bname)
        .unwrap_or_else(|| panic!("neither {name} nor {bname} in {names:?}"));
    let (n1, n2) = (&el.nodes[0], &el.nodes[1]);
    match &el.device {
        Device::Behavioral {
            kind: SourceKind::Current,
            expr: Expr::Bin(ExprOp::Divide, v, value),
        } => {
            assert_eq!(
                **v,
                Expr::Bin(
                    ExprOp::Minus,
                    Box::new(Expr::V(n1.clone())),
                    Box::new(Expr::V(n2.clone()))
                )
            );
            constant(value)
        }
        d => panic!("{d:?}"),
    }
}

/// The value of a tree without node or branch references.
fn constant(e: &Expr) -> f64 {
    match e {
        Expr::Num(v) => *v,
        Expr::Bin(op, a, b) => {
            let (a, b) = (constant(a), constant(b));
            match op {
                ExprOp::Plus => a + b,
                ExprOp::Minus => a - b,
                ExprOp::Times => a * b,
                ExprOp::Divide => a / b,
                ExprOp::Power => a.powf(b),
            }
        }
        Expr::Call(ExprFunc::Neg, a) => -constant(a),
        e => panic!("not a constant: {e:?}"),
    }
}

/// `rsh` of the only model.
fn rsh(deck: &str) -> f64 {
    let nl = clean(deck);
    nl.models[0]
        .params
        .iter()
        .find_map(|p| match (p.name.as_str(), &p.value) {
            ("rsh", ParamValue::Num(v)) => Some(*v),
            _ => None,
        })
        .unwrap()
}

fn err(deck: &str) -> String {
    let e = errors(&parse(deck));
    assert!(!e.is_empty(), "{deck}: expected an error");
    e.join(" | ")
}

#[test]
fn values_where_ngspice_gives_them_t1_t2() {
    // t_model, t_model_expr, t_model_quote, t_model_bare, t_case
    assert_eq!(rsh(".model rm r rsh={temper}\nr1 1 0 rm l=1 w=1"), 27.0);
    assert_eq!(rsh(".model rm r rsh={temper*2+1}\nr1 1 0 rm l=1 w=1"), 55.0);
    assert_eq!(rsh(".model rm r rsh='temper'\nr1 1 0 rm l=1 w=1"), 27.0);
    assert_eq!(rsh(".model rm r rsh=temper\nr1 1 0 rm l=1 w=1"), 27.0);
    assert_eq!(rsh(".model rm r rsh={TEMPER}\nr1 1 0 rm l=1 w=1"), 27.0);
    // t_r_pos, t_r_pos_expr, t_r_pos_quote, t_mixed_value
    assert_eq!(r("r1 1 0 {temper}", "r1"), 27.0);
    assert_eq!(r("r1 1 0 {temper*2}", "r1"), 54.0);
    assert_eq!(r("r1 1 0 'temper*2'", "r1"), 54.0);
    assert_eq!(r(".param k=3\nr1 1 0 {temper*k}", "r1"), 81.0);
    // t_glob, t_chain, t_chain_sep, t_tparam_model
    assert_eq!(r(".param t1={temper}\nr1 1 0 {t1}", "r1"), 27.0);
    assert_eq!(r(".param p={temper} q={p*2}\nr1 1 0 {q}", "r1"), 54.0);
    assert_eq!(
        rsh(".param p={temper}\n.model rm r rsh={p}\nr1 1 0 rm l=1 w=1"),
        27.0
    );
    // t_sub_local, t_sub_local_pos, t_sub_default, t_sub_default_expr,
    // t_sub_model, t_func, t_temper_if_else_branch
    let sub = |body: &str| r(&format!(".subckt s a\n{body}\n.ends\nx1 1 s"), "r.x1.r1");
    assert_eq!(sub(".param t2={temper*2}\nr1 a 0 {t2}"), 54.0);
    assert_eq!(sub("r1 a 0 {temper}"), 27.0);
    assert_eq!(
        r(
            ".subckt s a t={temper}\nr1 a 0 {t}\n.ends\nx1 1 s",
            "r.x1.r1"
        ),
        27.0
    );
    assert_eq!(r(".func f() {temper}\nr1 1 0 {f()}", "r1"), 27.0);
    assert_eq!(
        r(".param a=1\n.if (a==1)\nr1 1 0 {temper}\n.endif", "r1"),
        27.0
    );
}

#[test]
fn circuit_temperature_t1() {
    // t_opt50_*, t_opt50_after, t_temp75_*, t_temp_and_opt, t_temp_if
    assert_eq!(
        rsh(".options temp=50\n.model rm r rsh={temper}\nr1 1 0 rm l=1 w=1"),
        50.0
    );
    assert_eq!(
        rsh(".model rm r rsh={temper}\nr1 1 0 rm l=1 w=1\n.options temp=50"),
        50.0
    );
    assert_eq!(
        r(".options temp=50\n.param p={temper*2}\nr1 1 0 {p}", "r1"),
        100.0
    );
    assert_eq!(
        r(
            ".options temp=50\n.subckt s a\n.param t2={temper}\nr1 a 0 {t2}\n.ends\nx1 1 s",
            "r.x1.r1"
        ),
        50.0
    );
    assert_eq!(r(".temp 75\nr1 1 0 {temper}", "r1"), 75.0);
    assert_eq!(r(".options temp=50\n.temp 75\nr1 1 0 {temper}", "r1"), 75.0);
    assert_eq!(
        r(
            ".param a=1\n.if (a==1)\n.temp 75\n.endif\nr1 1 0 {temper}",
            "r1"
        ),
        75.0
    );
    assert_eq!(
        r(
            ".param a=0\n.if (a==1)\n.temp 75\n.endif\nr1 1 0 {temper}",
            "r1"
        ),
        27.0
    );
    // t_inst_temp: a device's own temp= does not change temper.
    assert_eq!(
        rsh(".model rm r rsh={temper}\nr1 1 0 rm l=1 w=1 temp=80"),
        27.0
    );
    // .options temp from a parameter.
    assert_eq!(
        r(
            ".param t0=40\n.options temp={t0}\n.param p={temper}\nr1 1 0 {p}",
            "r1"
        ),
        40.0
    );
}

#[test]
fn errors_where_ngspice_errs_t1_t2() {
    for (deck, want) in [
        // t_v_pos, t_v_dc, t_i_pos, t_tparam_v, t_sub_default_v, t_k
        ("v1 1 0 {temper}\nr1 1 0 1", "not available"),
        ("v1 1 0 dc {temper}\nr1 1 0 1", "not available"),
        ("i1 0 1 {temper}\nr1 1 0 1", "not available"),
        (
            ".param p={temper}\nv1 1 0 {p}\nr1 1 0 1",
            "depends on `temper`",
        ),
        (
            ".subckt s a t={temper}\nv1 a 0 {t}\nr1 a 0 1\n.ends\nx1 1 s\nr2 1 0 1",
            "depends on",
        ),
        (
            "l1 1 0 1u\nl2 2 0 1u\nk1 l1 l2 {temper/100}\nr1 1 0 1\nr2 2 0 1",
            "not available",
        ),
        // t_tran_card, t_options_temp, t_if_temper, t_tparam_tran
        ("r1 1 0 1k\n.tran {temper*1e-9} 1u", "control card"),
        (".options temp={temper+10}\nr1 1 0 1k", "control card"),
        (".if (temper>20)\nr1 1 0 1k\n.endif", ".if condition"),
        // t_sub_actual, t_tparam_x_actual
        (
            ".subckt s a t=1\nr1 a 0 {t}\n.ends\nx1 1 s t={temper}",
            "X line",
        ),
        (
            ".param p={temper}\n.subckt s a w=1\nr1 a 0 {w}\n.ends\nx1 1 s w={p}",
            "temper",
        ),
        // t_tparam_in_sub, t_sub_local_nested_use: another scope
        (
            ".param p={temper}\n.subckt s a\nr1 a 0 {p}\n.ends\nx1 1 s",
            "enclosing scope",
        ),
        (
            ".subckt s a\n.param t2={temper}\nx1 a leaf\n.ends\n.subckt leaf b\nr1 b 0 {t2}\n.ends\nx1 1 s",
            "enclosing scope",
        ),
        // t_func_glob: temper reached through a function from a .param
        (
            ".func f() {temper}\n.param p={f()}\nr1 1 0 {p}",
            "user function",
        ),
        // t_user_param: ngspice ignores the definition
        (".param temper=99\nr1 1 0 {temper}", "cannot be defined"),
    ] {
        let e = err(deck);
        assert!(e.contains(want), "{deck}: {e}");
    }
}

#[test]
fn temperature_sweeps_are_refused_with_temper() {
    // ngspice re-evaluates temper at each point of a .dc temp sweep
    // (t2 t_dc_sweep_model); Dune evaluates it once.
    let e = err(".model rm r rsh={temper}\nr1 1 0 rm l=1 w=1\ni1 0 1 1\n.dc temp 0 100 50");
    assert!(e.contains(".dc temp"), "{e}");
    // A sweep without temper, or temper without a sweep, is fine; a
    // temper-dependent .param nobody uses does not count.
    clean("r1 1 0 1k\ni1 0 1 1\n.dc temp 0 100 50");
    clean(".param p={temper}\nr1 1 0 1k\ni1 0 1 1\n.dc temp 0 100 50");
    // The once-only evaluation is said as a warning.
    let out = parse("r1 1 0 {temper}");
    assert!(
        out.diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("evaluated once"))
    );
}

// --- `temper`, `tc1`/`tc2` and `.func` in behavioural R/C/L. Probes
// `bN case` are in `.claude/scratch/behav-probes/bN.txt` (rules in
// `docs/dune-netlist.md`, "R, C e L comportamentais").

fn once_warning(out: &ParseOutcome) -> bool {
    out.diags
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains("evaluated once"))
}

/// The tree of element `name`'s B source with `v(…)` read from `v`.
fn eval(e: &Expr, v: &dyn Fn(&str) -> f64) -> f64 {
    match e {
        Expr::Num(x) => *x,
        Expr::V(n) | Expr::I(n) => v(n),
        Expr::Bin(op, a, b) => {
            let (a, b) = (eval(a, v), eval(b, v));
            match op {
                ExprOp::Plus => a + b,
                ExprOp::Minus => a - b,
                ExprOp::Times => a * b,
                ExprOp::Divide => a / b,
                ExprOp::Power => a.powf(b),
            }
        }
        Expr::Call(ExprFunc::Neg, a) => -eval(a, v),
        Expr::Call(ExprFunc::Abs, a) => eval(a, v).abs(),
        Expr::Call(ExprFunc::U, a) => {
            let x = eval(a, v);
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                0.0
            } else {
                0.5
            }
        }
        e => panic!("{e:?}"),
    }
}

fn bexpr<'a>(nl: &'a Netlist, name: &str) -> &'a Expr {
    match &nl.elements.iter().find(|e| e.name == name).unwrap().device {
        Device::Behavioral { expr, .. } => expr,
        d => panic!("{d:?}"),
    }
}

#[test]
fn temper_inside_a_behavioural_expression() {
    // b1 r_temper_v_85, r_temper_v_27: the B source reads temper as
    // the circuit temperature.
    for (deck, t) in [
        (
            ".options temp=85\nr1 1 0 r={1k*(1+temper/100)*(1+0.1*v(1))}",
            85.0,
        ),
        (".temp 85\nr1 1 0 r={1k*(1+temper/100)*(1+0.1*v(1))}", 85.0),
        ("r1 1 0 r={1k*(1+temper/100)*(1+0.1*v(1))}", 27.0),
    ] {
        let out = parse(deck);
        assert!(errors(&out).is_empty(), "{deck}: {:?}", errors(&out));
        assert!(once_warning(&out), "{deck}");
        let i = eval(bexpr(&out.netlist, "br1"), &|n| {
            if n == "1" { 2.0 } else { 0.0 }
        });
        let r = 1e3 * (1.0 + t / 100.0) * 1.2;
        assert!((i - 2.0 / r).abs() < 1e-15, "{deck}: {i}");
    }
    // A parameter that depends on temper, in the subcircuit's own
    // scope (the gf180 `r_temp`; b2 r_sub_rtemp).
    let out = parse(
        ".subckt rr a b w=1 r_tc1=1e-3 r_temp='1+r_tc1*(temper-25)'\n\
         rb a b r='r_temp*1k/w*(1+0.01*abs(v(a,b)))'\n.ends\n.temp 85\nx1 1 0 rr w=2",
    );
    assert!(errors(&out).is_empty(), "{:?}", errors(&out));
    assert!(once_warning(&out));
    let i = eval(bexpr(&out.netlist, "b.x1.brb"), &|n| {
        if n == "1" { 1.0 } else { 0.0 }
    });
    assert!((i - 1.0 / (1.06 * 500.0 * 1.01)).abs() < 1e-15, "{i}");
    // ngspice re-evaluates it at each point of a .dc temp sweep (b1
    // r_dcsweep_v); Dune refuses the combination.
    let e = err("r1 1 0 r={1k*(1+temper/100)*(1+0.1*v(1))}\ni1 0 1 1m\n.dc temp 0 100 50");
    assert!(e.contains(".dc temp"), "{e}");
    // The other rules of temper hold: not from an enclosing scope.
    let e = err(".param p={temper}\n.subckt s a\nr1 a 0 r={p*(1+v(a))}\n.ends\nx1 1 s");
    assert!(e.contains("enclosing scope"), "{e}");
}

#[test]
fn temper_only_values_are_rewritten_as_ngspice_does() {
    // b1 c_temper_op, l_temper_op: ngspice-47 rewrites a C or L whose
    // value depends on temper, so its internal nodes reach the rawfile.
    let nl = clean(".temp 50\nc1 1 0 {temper*1e-12}");
    for n in ["gc1", "rc1", "cc1", "vcurrmeasc1", "rc1par", "bc1"] {
        assert!(nl.elements.iter().any(|e| e.name == n), "{n}");
    }
    // 1e6·i(vcurrmeasc1)·C, with the current read as 1.
    let k = eval(bexpr(&nl, "bc1"), &|_| 1.0);
    assert!((k - 1e6 * 50e-12).abs() < 1e-18, "{k}");
    let nl = clean("l1 1 0 {temper*1e-6}");
    for n in ["fl1", "ll1", "bl1"] {
        assert!(nl.elements.iter().any(|e| e.name == n), "{n}");
    }
    // Through a parameter, a function, a subcircuit-local parameter
    // (b1 r_tparam_op, r_func_op, r_tparam_sub) — and a model R whose
    // `w` reads temper takes temper as its resistance, as the
    // rewriting picks the first `{…}` (b2 r_wkw_temper).
    assert_eq!(r(".param p={temper*2}\nr1 1 0 {p}", "r1"), 54.0);
    assert_eq!(r(".func f() {temper}\nr1 1 0 {f()}", "r1"), 27.0);
    assert_eq!(
        r(".model rm r rsh=10\nr1 1 0 rm l=1 w={temper}", "r1"),
        27.0
    );
    // A value without temper stays a plain element.
    let nl = clean(".param p=3\nr1 1 0 {p*2}");
    assert!(nl.elements.iter().any(|e| e.name == "r1"));
}

#[test]
fn tc_on_a_behavioural_rcl() {
    // b1 r_tc_85*: i = v/(eq) / (1 + tc1·Δ + tc2·Δ²) with Δ = T − 27
    // (300.15 K fixed: tnom does not matter), times m; an instance
    // temp= is dropped (r_tc_85_temp).
    let f = 1.0 + 0.01 * 58.0 + 1e-4 * 58.0 * 58.0;
    for (deck, m) in [
        (
            ".temp 85\nr1 1 0 r={1k*(1+0.1*v(1))} tc1=0.01 tc2=1e-4",
            1.0,
        ),
        (
            ".options temp=85 tnom=50\nr1 1 0 r={1k*(1+0.1*v(1))} tc1=0.01 tc2=1e-4",
            1.0,
        ),
        (
            ".temp 85\nr1 1 0 r={1k*(1+0.1*v(1))} tc1=0.01 tc2=1e-4 m=2",
            2.0,
        ),
        (
            ".param a=0.01\n.temp 85\nr1 1 0 r={1k*(1+0.1*v(1))} tc1={a} tc2=1e-4",
            1.0,
        ),
    ] {
        let out = parse(deck);
        assert!(errors(&out).is_empty(), "{deck}: {:?}", errors(&out));
        assert!(once_warning(&out), "{deck}");
        let i = eval(bexpr(&out.netlist, "br1"), &|n| {
            if n == "1" { 1.0 } else { 0.0 }
        });
        let want = 1.0 / (1e3 * 1.1) / f * m;
        assert!((i - want).abs() < 1e-18, "{deck}: {i} vs {want}");
    }
    // At 27 °C the factor is 1; a zero tc reads no temperature.
    let out = parse("r1 1 0 r={1k*(1+0.1*v(1))} tc1=0 tc2=0");
    assert!(errors(&out).is_empty() && !once_warning(&out));
    // C and the charge form divide by the factor, L multiplies (b1
    // c_tc_85, b2 c_charge_tc, l_m_tc).
    let k = 1.0 + 0.01 * 33.0;
    let nl = clean(".temp 60\nc1 1 0 q={1p*v(1)} tc1=0.01");
    let q = eval(bexpr(&nl, "bc1"), &|n| if n == "1" { 1.0 } else { 0.0 });
    assert!((q - 1e-12 / k).abs() < 1e-27, "{q}");
    let nl = clean(".temp 60\nl1 1 0 l={2u*(1+0*v(1))} tc1=0.01 m=2");
    let v = eval(bexpr(&nl, "bl1"), &|n| {
        if n == "l1_int2" { 1.0 } else { 0.0 }
    });
    assert!((v - 2e-6 / 2.0 * k).abs() < 1e-20, "{v}");
    // ngspice re-evaluates the factor in a .dc temp sweep; Dune
    // refuses, as for temper.
    let e = err(".temp 85\nr1 1 0 r={1k*(1+0.1*v(1))} tc1=0.01\ni1 0 1 1m\n.dc temp 0 100 50");
    assert!(e.contains(".dc temp"), "{e}");
}

#[test]
fn user_functions_in_a_behavioural_expression() {
    // b1 r_func_v, r_func_v2, b2 r_func_body_v, r_func_nested,
    // r_func_tparam: expanded as ngspice expands the macro.
    let at = |deck: &str| {
        let nl = clean(deck);
        eval(bexpr(&nl, "br1"), &|n| if n == "1" { 1.0 } else { 0.0 })
    };
    let want = 1.0 / 1100.0;
    for deck in [
        ".func f(x) {x*2}\nr1 1 0 r={f(500)*(1+0.1*v(1))}",
        ".func g(x,y) {x*y}\nr1 1 0 r={g(500,2)+g(v(1),100)}",
        ".func g(x) {x*(1+0.1*v(1))}\nr1 1 0 {g(1k)}",
        ".func h(y) {y*2}\n.func g(x) {h(x)*(1+0.1*v(1))}\nr1 1 0 r={g(500)}",
        ".param p={temper*10}\n.func g(x) {x+p}\nr1 1 0 r={g(730)*(1+0.1*v(1))}",
    ] {
        assert!((at(deck) - want).abs() < 1e-18, "{deck}: {}", at(deck));
    }
    // Spanned refusals, never a wrong value: arity, a construct of the
    // body the B grammar here lacks (the span points into the .func
    // card), and recursion (reported once, at the definition).
    let e = err(".func f(x) {x*2}\nr1 1 0 r={f(1,2)*v(1)}");
    assert!(e.contains("takes 1 argument"), "{e}");
    let out = parse(".func f(x) {x>1 ? 1 : 2}\nr1 1 0 r={f(v(1))*1k}");
    let d = out
        .diags
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("an error");
    assert!(d.message.contains("comparisons"), "{}", d.message);
    assert_eq!(out.sources.locate(d.span.unwrap()).line, 2);
    let out = parse(".func f(x) {f(x)+1}\nr1 1 0 r={f(v(1))}");
    let e = errors(&out);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("recursive"), "{e:?}");
}

/// Hostile user functions (principle 4): a chain deeper than the
/// nesting limit and an exponential expansion end with an error, fast,
/// in a 1 MiB thread.
#[test]
fn hostile_user_functions_in_behavioural_expressions() {
    let run = || {
        let mut deck = String::from(".func f0(x) {x+v(1)}\n");
        for k in 1..60 {
            deck += &format!(".func f{k}(x) {{f{}(x)+1}}\n", k - 1);
        }
        deck += "r1 1 0 r={f59(1)}";
        let e = err(&deck);
        assert!(e.contains("nested too deeply"), "{e}");
        let mut deck = String::from(".func d0(x) {x+v(1)}\n");
        for k in 1..30 {
            deck += &format!(".func d{k}(x) {{d{0}(x)+d{0}(x)}}\n", k - 1);
        }
        deck += "r1 1 0 r={d29(1)}";
        let t = std::time::Instant::now();
        let e = err(&deck);
        assert!(e.contains("too large") || e.contains("steps"), "{e}");
        assert!(t.elapsed().as_secs() < 10);
    };
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(run)
        .unwrap()
        .join()
        .unwrap();
}

/// The expression is the first `{…}` after the nodes once ngspice has
/// braced every bare value after an `=` (b3: `inp_fix_param_values`
/// before `inp_compat`) — the gf180 `ppolyf_u` body resistor is
/// written `rb 11 21 ppolyf_u_body l=r_l w=r_w r='…'`, so ngspice-47
/// simulates `r_l` ohms (behav-probes `pdk/gf180_ppolyf_u_*`).
#[test]
fn behavioural_value_is_the_first_brace_as_in_ngspice() {
    let value = |deck: &str| {
        let out = parse(deck);
        assert!(errors(&out).is_empty(), "{deck}: {:?}", errors(&out));
        let v = eval(bexpr(&out.netlist, "br1"), &|n| {
            if n == "1" { 1.0 } else { 0.0 }
        });
        let warned = out
            .diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("first `{…}`"));
        (1.0 / v, warned)
    };
    let base = ".param p=2\n.model rm r\n";
    for (card, want, warned) in [
        ("r1 1 0 rm l=p w=3 r='5+0*v(1)'", 2.0, true),
        ("r1 1 0 rm l=2 w=3 r='5+0*v(1)'", 5.0, false),
        ("r1 1 0 rm l={2} w=3 r='5+0*v(1)'", 2.0, true),
        ("r1 1 0 rm r='5+0*v(1)' l=p w=3", 5.0, false),
        ("r1 1 0 rm l=p*3 r='5+0*v(1)'", 6.0, true),
        ("r1 1 0 rm l=max(p,3) r='5+0*v(1)'", 3.0, true),
        ("r1 1 0 r=p*(1+0*v(1))", 2.0, false),
    ] {
        let (r, w) = value(&format!("{base}{card}"));
        assert!((r - want).abs() < 1e-12, "{card}: {r}");
        assert_eq!(w, warned, "{card}");
    }
    // The dropped keywords are not evaluated: a `v()` in one is not an
    // error (it was "only a behavioural R/C/L value…" before).
    value(&format!("{base}r1 1 0 rm l={{2}} w=3 r='5+0*v(1)'"));
    // tc1/tc2/m count wherever they stand, as ngspice's strstr finds
    // them: before the expression too.
    let nl = clean(".temp 85\nr1 1 0 tc1=0.01 r={1k*(1+0*v(1))}");
    let i = eval(bexpr(&nl, "br1"), &|n| if n == "1" { 1.0 } else { 0.0 });
    assert!((i - 1.0 / 1e3 / 1.58).abs() < 1e-18, "{i}");
}
