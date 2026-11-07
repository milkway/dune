//! Re-solve speed (issue #12): a fresh run of the altered deck
//! (parse + elaborate + every card) against `Session::alter` +
//! `Session::run`, cold and warm, on a few corpus decks. The deck's
//! element `<el>` parameter `<param>` alternates between two values
//! as a slider would.
//!
//! cargo run --release -p dune-core --example session_speed

use std::time::{Duration, Instant};

use dune_core::{Circuit, Options, Session};

struct Case {
    label: &'static str,
    path: &'static str,
    /// Keep only these analysis cards (`None`: the deck's own).
    cards: Option<&'static str>,
    el: &'static str,
    param: &'static str,
    /// (card text, value) pairs the slider moves between.
    from: (&'static str, f64),
    to: (&'static str, f64),
}

fn deck_text(c: &Case, text: &str) -> String {
    let base = std::fs::read_to_string(c.path).unwrap();
    let base = base.replace(c.from.0, text);
    match c.cards {
        None => base,
        Some(cards) => {
            let body: String = base
                .lines()
                .filter(|l| {
                    let l = l.trim_start().to_ascii_lowercase();
                    ![".op", ".ac", ".dc", ".tran", ".end", ".print", ".plot"]
                        .iter()
                        .any(|k| l.split_whitespace().next() == Some(*k))
                })
                .map(|l| format!("{l}\n"))
                .collect();
            format!("{body}{cards}.end\n")
        }
    }
}

fn fresh(deck: &str) {
    let parsed = dune_netlist::parse_str("deck.cir", deck);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut o = Options::default();
    o.apply(&parsed.netlist.options, &mut Vec::new());
    for a in &parsed.netlist.analyses {
        std::hint::black_box(c.run(a, &o).unwrap());
    }
}

/// Every value of a plot with its vector name (complex: re and im).
fn flat(p: &dune_core::Solution) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (k, (name, _)) in p.variables.iter().enumerate() {
        match &p.data {
            dune_core::Data::Real(c) => out.extend(c[k].iter().map(|x| (name.clone(), *x))),
            dune_core::Data::Complex(c) => out.extend(
                c[k].iter()
                    .flat_map(|z| [(name.clone(), z.re), (name.clone(), z.im)]),
            ),
        }
    }
    out
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn time(n: usize, mut f: impl FnMut(usize)) -> Duration {
    for i in 0..3 {
        f(i);
    }
    median(
        (0..n)
            .map(|i| {
                let t = Instant::now();
                f(i);
                t.elapsed()
            })
            .collect(),
    )
}

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus/");
    let p = |f: &str| -> &'static str { Box::leak(format!("{root}{f}").into_boxed_str()) };
    let cases = [
        Case {
            label: "bsim3_inv .op",
            path: p("regress/bsim3_inv.cir"),
            cards: Some(".op\n"),
            el: "mn3",
            param: "w",
            from: ("mn3 7 6 8 0 n1 l=1u w=20u", 20e-6),
            to: ("mn3 7 6 8 0 n1 l=1u w=22u", 22e-6),
        },
        Case {
            label: "bsim3_inv .op+.ac",
            path: p("regress/bsim3_inv.cir"),
            cards: Some(".op\n.ac dec 10 10k 10g\n"),
            el: "mn3",
            param: "w",
            from: ("mn3 7 6 8 0 n1 l=1u w=20u", 20e-6),
            to: ("mn3 7 6 8 0 n1 l=1u w=22u", 22e-6),
        },
        Case {
            label: "bsim3_inv deck (.ac .op .tran)",
            path: p("regress/bsim3_inv.cir"),
            cards: None,
            el: "mn3",
            param: "w",
            from: ("mn3 7 6 8 0 n1 l=1u w=20u", 20e-6),
            to: ("mn3 7 6 8 0 n1 l=1u w=22u", 22e-6),
        },
        Case {
            label: "mosamp2 .op",
            path: p("spice3f5/examples/mosamp2.cir"),
            cards: Some(".op\n"),
            el: "m7",
            param: "w",
            from: ("m7   5 20  8 32 m w=482.6u", 482.6e-6),
            to: ("m7   5 20  8 32 m w=500u", 500e-6),
        },
        Case {
            label: "mosamp2 deck (.tran)",
            path: p("spice3f5/examples/mosamp2.cir"),
            cards: None,
            el: "m7",
            param: "w",
            from: ("m7   5 20  8 32 m w=482.6u", 482.6e-6),
            to: ("m7   5 20  8 32 m w=500u", 500e-6),
        },
        Case {
            label: "mos6inv .op",
            path: p("spice3f5/examples/mos6inv.cir"),
            cards: Some(".op\n"),
            el: "vin",
            param: "dc",
            from: ("VIN  1    0  DC 0 PWL", 0.0),
            to: ("VIN  1    0  DC 0.5 PWL", 0.5),
        },
    ];
    println!(
        "| case | fresh | rerun cold | speedup | rerun warm | speedup | warm hits/misses | warm vs cold, max Δ/tol |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    for c in &cases {
        let decks = [deck_text(c, c.from.0), deck_text(c, c.to.0)];
        assert!(
            decks[0] != decks[1],
            "{}: replacement did not apply",
            c.label
        );
        let values = [c.from.1, c.to.1];
        let n = if c.cards.is_none() { 30 } else { 300 };
        let t_fresh = time(n, |i| fresh(&decks[i % 2]));
        let parsed = dune_netlist::parse_str("deck.cir", &decks[0]);
        let (mut s, _) = Session::from_netlist(&parsed.netlist).unwrap();
        let t_cold = time(n, |i| {
            s.alter(c.el, c.param, values[(i + 1) % 2]).unwrap();
            std::hint::black_box(s.run().unwrap());
        });
        let (mut w, _) = Session::from_netlist(&parsed.netlist).unwrap();
        w.set_warm_start(true);
        let t_warm = time(n, |i| {
            w.alter(c.el, c.param, values[(i + 1) % 2]).unwrap();
            std::hint::black_box(w.run().unwrap());
        });
        let st = w.warm_stats();
        // How far the warm result sits from the cold one, in units of
        // the Newton tolerance `reltol·max(|a|,|b|) + vntol|abstol`.
        let (cw, cc) = (w.run().unwrap().0, s.run().unwrap().0);
        let o = s.options();
        let mut worst: f64 = 0.0;
        let mut identical = true;
        for (a, b) in cw.iter().zip(&cc) {
            let ra: Vec<(String, f64)> = flat(a);
            let rb: Vec<(String, f64)> = flat(b);
            for ((name, x), (_, y)) in ra.iter().zip(&rb) {
                identical &= x.to_bits() == y.to_bits();
                let floor = if name.starts_with("i(") {
                    o.abstol
                } else {
                    o.vntol
                };
                worst = worst.max((x - y).abs() / (o.reltol * x.abs().max(y.abs()) + floor));
            }
        }
        let x = |t: Duration| t_fresh.as_secs_f64() / t.as_secs_f64();
        println!(
            "| {} | {:.1?} | {:.1?} | {:.2}× | {:.1?} | {:.2}× | {}/{} | {} |",
            c.label,
            t_fresh,
            t_cold,
            x(t_cold),
            t_warm,
            x(t_warm),
            st.hits,
            st.misses,
            if identical {
                "identical".to_string()
            } else {
                format!("{worst:.1e}")
            }
        );
    }
}
