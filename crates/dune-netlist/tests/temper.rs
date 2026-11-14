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

/// The value of resistor `name`, to 12 digits.
fn r(deck: &str, name: &str) -> f64 {
    let nl = clean(deck);
    let el = nl.elements.iter().find(|e| e.name == name).unwrap();
    match &el.device {
        Device::Resistor { value: Some(v), .. } => format!("{v:.11e}").parse().unwrap(),
        d => panic!("{d:?}"),
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
