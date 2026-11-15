//! Seeded random functions (D-0075). Each structural test cites the
//! ngspice-47 probe whose pattern of shared and independent draws it
//! reproduces: `tN case` is `.claude/scratch/temper-seed-probes/tN/case.cir`
//! (in the worktree of the leva; rules in `docs/dune-netlist.md`,
//! "Funções aleatórias"). The numbers cannot match ngspice's (another
//! generator); the statistics are checked against the definitions of
//! the ngspice manual.

use dune_netlist::ir::*;
use dune_netlist::{ParseOutcome, Severity, parse_str};

fn parse(deck: &str) -> ParseOutcome {
    parse_str("t.cir", &format!("t\n{deck}\n.end\n"))
}

fn clean(deck: &str) -> Netlist {
    let out = parse(deck);
    let e: Vec<_> = out
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect();
    assert!(e.is_empty(), "{deck}\n{e:?}");
    out.netlist
}

fn r(nl: &Netlist, name: &str) -> f64 {
    match nl
        .elements
        .iter()
        .find(|e| e.name == name)
        .map(|e| &e.device)
    {
        Some(Device::Resistor { value: Some(v), .. }) => *v,
        d => panic!("{name}: {d:?}"),
    }
}

fn rsh(nl: &Netlist, model: &str) -> f64 {
    let m = nl.models.iter().find(|m| m.name == model).unwrap();
    m.params
        .iter()
        .find_map(|p| match (p.name.as_str(), &p.value) {
            ("rsh", ParamValue::Num(v)) => Some(*v),
            _ => None,
        })
        .unwrap()
}

/// `n` resistors `r1…rn` with the value `value`, fed by current
/// sources.
fn bank(n: usize, value: &str) -> String {
    (1..=n)
        .map(|k| format!("r{k} {k} 0 {value}\ni{k} 0 {k} 1\n"))
        .collect()
}

fn stats(xs: &[f64]) -> (f64, f64) {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n;
    (mean, var.sqrt())
}

const N: usize = 4000;

fn sample(seed: u64, call: &str) -> Vec<f64> {
    let nl = clean(&format!(
        ".options seed={seed}\n{}",
        bank(N, &format!("{{{call}}}"))
    ));
    (1..=N).map(|k| r(&nl, &format!("r{k}"))).collect()
}

#[test]
fn distributions_follow_the_manual() {
    // N = 4000 independent draws (t2 b_direct: each card draws). The
    // tolerances are 4.5 standard errors: SE(mean) = σ/√N, SE(σ) ≈
    // σ/√(2N) for a normal (√((κ−1)/(4N))·σ for a uniform, κ = 1.8).
    let n = N as f64;
    let check = |what: &str, xs: &[f64], mu: f64, sd: f64, se_sd: f64| {
        let (m, s) = stats(xs);
        assert!((m - mu).abs() < 4.5 * sd / n.sqrt(), "{what}: mean {m}");
        assert!((s - sd).abs() < 4.5 * se_sd, "{what}: sd {s}");
    };
    // agauss(nom, avar, sigma) = nom + avar/sigma·N(0,1) (absolute).
    let a = sample(11, "agauss(1k,100,3)");
    check(
        "agauss/3",
        &a,
        1000.0,
        100.0 / 3.0,
        100.0 / 3.0 / (2.0 * n).sqrt(),
    );
    let a1 = sample(12, "agauss(1k,100,1)");
    check("agauss/1", &a1, 1000.0, 100.0, 100.0 / (2.0 * n).sqrt());
    // The normal's mass inside ±1σ (0.6827, SE 0.0074).
    let inside = a1.iter().filter(|x| (*x - 1000.0).abs() < 100.0).count() as f64 / n;
    assert!((inside - 0.6827).abs() < 0.033, "{inside}");
    // gauss(nom, rvar, sigma) = nom·(1 + rvar/sigma·N(0,1)) (relative).
    let g = sample(13, "gauss(2k,0.1,2)");
    check("gauss", &g, 2000.0, 100.0, 100.0 / (2.0 * n).sqrt());
    // aunif(nom, avar): uniform on nom ± avar; unif(nom, rvar): on
    // nom·(1 ± rvar). σ = half-width/√3, bounded.
    let se_u = |sd: f64| sd * (0.8f64 / (4.0 * n)).sqrt();
    let u = sample(14, "aunif(1k,100)");
    check(
        "aunif",
        &u,
        1000.0,
        100.0 / 3f64.sqrt(),
        se_u(100.0 / 3f64.sqrt()),
    );
    assert!(u.iter().all(|x| (900.0..=1100.0).contains(x)));
    let u = sample(15, "unif(1k,0.2)");
    check(
        "unif",
        &u,
        1000.0,
        200.0 / 3f64.sqrt(),
        se_u(200.0 / 3f64.sqrt()),
    );
    assert!(u.iter().all(|x| (800.0..=1200.0).contains(x)));
    // limit(nom, avar): nom − avar or nom + avar, ½ each (SE 0.0079).
    let l = sample(16, "limit(1k,100)");
    assert!(l.iter().all(|x| *x == 900.0 || *x == 1100.0));
    let up = l.iter().filter(|x| **x == 1100.0).count() as f64 / n;
    assert!((up - 0.5).abs() < 0.036, "{up}");
    // The two-argument agauss/gauss stays at nominal, as in ngspice
    // (t3 c_two_args, c_gauss_two_args).
    let nl =
        clean(".options seed=3\n.param p=agauss(1k,100) q=gauss(1k,0.1)\nr1 1 0 {p}\nr2 2 0 {q}");
    assert_eq!((r(&nl, "r1"), r(&nl, "r2")), (1000.0, 1000.0));
}

#[test]
fn every_reference_to_a_random_param_draws_again() {
    let s = ".options seed=7\n.param p=agauss(1k,100,1)\n";
    // t2 b_glob_two_devices: two devices, two draws.
    let nl = clean(&format!("{s}{}", bank(2, "{p}")));
    assert_ne!(r(&nl, "r1"), r(&nl, "r2"));
    // t2 b_twice_in_expr: twice in one expression, two draws.
    let nl = clean(&format!("{s}{}", bank(1, "{1k+p-p}")));
    assert_ne!(r(&nl, "r1"), 1000.0);
    // t3 c_expr_param: a random call anywhere in the .param.
    let nl = clean(&format!(
        ".options seed=7\n.param p={{1+agauss(1k,100,1)}}\n{}",
        bank(2, "{p}")
    ));
    assert_ne!(r(&nl, "r1"), r(&nl, "r2"));
    // t2 b_func: a .func whose body draws, called twice.
    let nl = clean(&format!(
        ".options seed=7\n.func f() {{agauss(1k,100,1)}}\n{}",
        bank(2, "{f()}")
    ));
    assert_ne!(r(&nl, "r1"), r(&nl, "r2"));
    // t2 b_param_in_param_twice: `q={p-p}` holds two draws.
    let nl = clean(&format!("{s}.param q={{p-p}}\n{}", bank(1, "{1k+q}")));
    assert_ne!(r(&nl, "r1"), 1000.0);
}

#[test]
fn a_param_that_only_reads_one_keeps_one_value() {
    let s = ".options seed=7\n.param p=agauss(1k,100,1)\n";
    // t2 b_through_param, b_through_param2: q is evaluated once.
    let nl = clean(&format!(
        "{s}.param q={{p*2}}\nr1 1 0 {{q}}\nr2 2 0 {{q}}\nr3 3 0 {{p}}"
    ));
    assert_eq!(r(&nl, "r1"), r(&nl, "r2"));
    assert_ne!(r(&nl, "r1"), 2.0 * r(&nl, "r3"));
    // t3 c_param_of_func: a .param calling a drawing .func is a value.
    let nl = clean(
        ".options seed=7\n.func f() {agauss(1k,100,1)}\n.param q={f()}\nr1 1 0 {q}\nr2 2 0 {q}",
    );
    assert_eq!(r(&nl, "r1"), r(&nl, "r2"));
    // t2 b_model_shared, b_model_param: a global model is evaluated
    // once, whatever reads it.
    let nl = clean(&format!(
        "{s}.model rm r rsh={{p}}\nr1 1 0 rm l=1 w=1\nr2 2 0 rm l=1 w=1\nr3 3 0 {{p}}"
    ));
    assert_eq!(nl.models.len(), 1);
    assert_ne!(rsh(&nl, "rm"), r(&nl, "r3"));
}

const X: &str = "x1 1 s\nx2 2 s\n";

#[test]
fn subcircuit_instances_draw_as_ngspice_does() {
    let s = ".options seed=7\n";
    // t2 b_sub_local, b_sub_default, b_sub_global: a random .param of
    // the body, a random default, or a global random .param read in
    // the body — every reference, in every instance, draws.
    for sub in [
        ".subckt s a\n.param lp=agauss(1k,100,1)\nr1 a 0 {lp}\nr2 a 0 {lp}\n.ends\n",
        ".subckt s a lp=agauss(1k,100,1)\nr1 a 0 {lp}\nr2 a 0 {lp}\n.ends\n",
        ".param lp=agauss(1k,100,1)\n.subckt s a\nr1 a 0 {lp}\nr2 a 0 {lp}\n.ends\n",
    ] {
        let nl = clean(&format!("{s}{sub}{X}"));
        let v: Vec<f64> = ["r.x1.r1", "r.x1.r2", "r.x2.r1", "r.x2.r2"]
            .iter()
            .map(|n| r(&nl, n))
            .collect();
        for i in 0..4 {
            for j in i + 1..4 {
                assert_ne!(v[i], v[j], "{sub}: {v:?}");
            }
        }
    }
    // t2 b_sub_actual, b_sub_actual_param; t3 c_local_of_global,
    // c_default_of_global, c_local_value_param: a value computed once
    // per instance (an X-line actual, a local or default .param that
    // reads a random one) is shared inside the instance, not across.
    for (sub, call) in [
        (
            ".subckt s a lp=1\nr1 a 0 {lp}\nr2 a 0 {lp}\n.ends\n",
            "x1 1 s lp={agauss(1k,100,1)}\nx2 2 s lp={agauss(1k,100,1)}\n",
        ),
        (
            ".param p=agauss(1k,100,1)\n.subckt s a lp=1\nr1 a 0 {lp}\nr2 a 0 {lp}\n.ends\n",
            "x1 1 s lp={p}\nx2 2 s lp={p}\n",
        ),
        (
            ".param p=agauss(1k,100,1)\n.subckt s a\n.param lq={p}\nr1 a 0 {lq}\nr2 a 0 {lq}\n.ends\n",
            X,
        ),
        (
            ".param p=agauss(1k,100,1)\n.subckt s a lq={p}\nr1 a 0 {lq}\nr2 a 0 {lq}\n.ends\n",
            X,
        ),
        (
            ".subckt s a\n.param lp=agauss(1k,100,1)\n.param lq={lp}\nr1 a 0 {lq}\nr2 a 0 {lq}\n.ends\n",
            X,
        ),
    ] {
        let nl = clean(&format!("{s}{sub}{call}"));
        assert_eq!(r(&nl, "r.x1.r1"), r(&nl, "r.x1.r2"), "{sub}");
        assert_eq!(r(&nl, "r.x2.r1"), r(&nl, "r.x2.r2"), "{sub}");
        assert_ne!(r(&nl, "r.x1.r1"), r(&nl, "r.x2.r1"), "{sub}");
    }
    // t2 b_sub_model, b_sub_model_glob: a local model is one per
    // instance — never shared between instances when it draws, even
    // though their inputs agree (the D-0067 sharing is off for it).
    for sub in [
        ".subckt s a\n.model rm r rsh={agauss(1k,100,1)}\nr1 a 0 rm l=1 w=1\nr2 a 0 rm l=1 w=1\n.ends\n",
        ".param p=agauss(1k,100,1)\n.subckt s a\n.model rm r rsh={p}\nr1 a 0 rm l=1 w=1\nr2 a 0 rm l=1 w=1\n.ends\n",
    ] {
        let nl = clean(&format!("{s}{sub}{X}"));
        assert_eq!(nl.models.len(), 2, "{sub}");
        assert_ne!(rsh(&nl, "x1.rm"), rsh(&nl, "x2.rm"), "{sub}");
    }
    // At nominal the instances still share one evaluated model.
    let nl = clean(&format!(
        ".subckt s a\n.model rm r rsh={{agauss(1k,100,1)}}\nr1 a 0 rm l=1 w=1\n.ends\n{X}"
    ));
    assert_eq!(nl.models.len(), 1);
}

#[test]
fn draws_are_reproducible_and_keyed_by_the_deck() {
    let deck = |seed: &str| {
        format!(
            ".options seed={seed}\n.param p=agauss(1k,100,1)\n.subckt s a\nr1 a 0 {{p}}\n.ends\n\
             x1 1 s\nx2 2 s\nr3 3 0 {{p}}\nr4 4 0 {{aunif(1k,100)}}"
        )
    };
    let values = |nl: &Netlist| {
        ["r.x1.r1", "r.x2.r1", "r3", "r4"]
            .iter()
            .map(|n| r(nl, n))
            .collect::<Vec<_>>()
    };
    let a = values(&clean(&deck("5")));
    assert_eq!(a, values(&clean(&deck("5"))));
    assert_ne!(a, values(&clean(&deck("6"))));
    // A title, a comment or an unrelated card does not move the draws
    // (keys are the card's text and the instance path).
    let shifted = format!("* a comment\nv9 9 0 1\n{}", deck("5"));
    assert_eq!(a, values(&clean(&shifted)));
    let out = parse_str(
        "t.cir",
        &format!("another, longer title\n{}\n.end\n", deck("5")),
    );
    assert_eq!(a, values(&out.netlist));
    // ngspice reads the seed as an unsigned int: 7.6 and 2^32+7 are 7
    // (t2 a_seedfrac, a_seedbig); `rndseed` is the same number
    // (a_rndseed).
    let seven = values(&clean(&deck("7")));
    assert_eq!(seven, values(&clean(&deck("7.6"))));
    assert_eq!(seven, values(&clean(&deck("4294967303"))));
    assert_eq!(
        seven,
        values(&clean(
            &deck("7").replace(".options seed=7", ".options rndseed=7")
        ))
    );
    // with_seed(s) is the deck with `.options seed=s`, and writes it
    // into the options for the engine's noise sources.
    let nominal = clean(&deck("5").replace(".options seed=5\n", ""));
    assert_eq!(nominal.random_mode(), RandomMode::Nominal);
    let (seeded, d) = nominal.with_seed(7);
    assert!(d.iter().all(|d| d.severity != Severity::Error));
    assert_eq!(values(&seeded), seven);
    assert_eq!(seeded.random_mode(), RandomMode::Seeded(7));
    let seeds: Vec<_> = seeded.options.iter().filter(|p| p.name == "seed").collect();
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0].value, ParamValue::Num(7.0));
}

#[test]
fn the_seed_option_as_ngspice_reads_it() {
    let p = ".param p=agauss(1k,100,1)\nr1 1 0 {p}";
    let warned = |out: &ParseOutcome, what: &str| {
        out.diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains(what))
    };
    // Default: nominal, with today's warning (a deliberate difference:
    // ngspice draws differently on every run).
    let out = parse(p);
    assert_eq!(r(&out.netlist, "r1"), 1000.0);
    assert!(warned(&out, "nominal value"));
    assert_eq!(out.netlist.random_mode(), RandomMode::Nominal);
    // Zero, negative, a word: "Cannot convert … skipped" in ngspice
    // (t2 a_seed0, a_seedneg, a_seedword) — a warning, nominal here.
    for bad in ["0", "-5", "abc"] {
        let out = parse(&format!(".options seed={bad}\n{p}"));
        assert_eq!(r(&out.netlist, "r1"), 1000.0, "{bad}");
        assert!(warned(&out, "positive integer"), "{bad}");
    }
    // Two seeds: the first counts, with a warning (t2 a_two_options,
    // a_two_options_rev), and the later one leaves the options.
    let out = parse(&format!(".options seed=7\n.options seed=8\n{p}"));
    assert!(warned(&out, "first one counts"));
    assert_eq!(out.netlist.random_mode(), RandomMode::Seeded(7));
    let seeds: Vec<_> = out
        .netlist
        .options
        .iter()
        .filter(|p| p.name == "seed")
        .collect();
    assert_eq!((seeds.len(), &seeds[0].value), (1, &ParamValue::Num(7.0)));
    // A seed after the cards counts too (t2 a_option_after).
    let after = parse(&format!("{p}\n.options seed=7"));
    assert_eq!(after.netlist.random_mode(), RandomMode::Seeded(7));
    // seed=random: the caller must supply the seed.
    let out = parse(&format!(".options seed=random\n{p}"));
    assert_eq!(out.netlist.random_mode(), RandomMode::SeedRequested);
    assert_eq!(r(&out.netlist, "r1"), 1000.0);
    assert!(warned(&out, "seed=random"));
    let (nl, _) = out.netlist.with_seed(7);
    assert_eq!(
        r(&nl, "r1"),
        r(&clean(&format!(".options seed=7\n{p}")), "r1")
    );
    assert!(
        nl.options
            .iter()
            .all(|o| o.value != ParamValue::Str("random".into()))
    );
    // A seed without random functions changes nothing.
    let nl = clean(".options seed=7\nr1 1 0 1k");
    assert_eq!(r(&nl, "r1"), 1000.0);
}

#[test]
fn re_expansion_at_another_temperature_repeats_the_draws() {
    // A seeded deck that also reads temper: the re-expansion at any
    // temperature gives the same draws, so a temperature sweep does
    // not jump between points.
    let deck = ".options seed=9\n.param p=agauss(1k,100,1)\n\
                .subckt s a\n.param lp=aunif(1,0.1)\nr1 a 0 {p*lp}\n.ends\n\
                x1 1 s\nx2 2 s\nr3 3 0 {p+temper}\n.model rm r rsh={p}\nr4 4 0 rm l=1 w=1";
    let nl = clean(deck);
    assert!(nl.uses_temper());
    let draws = |nl: &Netlist| (r(nl, "r.x1.r1"), r(nl, "r.x2.r1"), rsh(nl, "rm"));
    let base = draws(&nl);
    for t in [-40.0, 27.0, 85.0, 125.0] {
        let (at, d) = nl.at_temperature(t);
        assert!(d.iter().all(|d| d.severity != Severity::Error));
        assert_eq!(draws(&at), base, "{t}");
        assert_eq!(at.random_mode(), RandomMode::Seeded(9));
    }
    // The same through with_seed + at_temperature.
    let (again, _) = nl.with_seed(9);
    assert_eq!(draws(&again.at_temperature(60.0).0), base);
    // A deck at another .temp draws the same too (the global scope is
    // resolved a second time there).
    let hot = clean(&format!(".temp 85\n{deck}"));
    assert_eq!(draws(&hot), base);
}

#[test]
fn hostile_random_parameters_are_bounded() {
    // Each reference re-evaluates: a chain that doubles at every level
    // would cost 2^40 evaluations; the budget stops it with a span.
    let mut deck = String::from(".options seed=1\n.param p0=agauss(1,0.1,1)\n");
    for k in 1..=40 {
        deck.push_str(&format!(
            ".param p{k}={{p{}+p{}+agauss(0,1,1)}}\n",
            k - 1,
            k - 1
        ));
    }
    deck.push_str("r1 1 0 {p40}");
    let out = parse(&deck);
    assert!(
        out.diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.span.is_some()),
        "{:?}",
        out.diags
    );
    // A chain deeper than the re-evaluation depth limit is an error
    // too, not a stack overflow.
    let mut deck = String::from(".options seed=1\n.param p0=agauss(1,0.1,1)\n");
    for k in 1..=200 {
        deck.push_str(&format!(".param p{k}={{p{}+agauss(0,1,1)}}\n", k - 1));
    }
    deck.push_str("r1 1 0 {p200}");
    let out = parse(&deck);
    assert!(out.diags.iter().any(|d| d.severity == Severity::Error));
}
