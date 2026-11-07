//! Wall time of the parallel device load (issue #9, lever 1) on a
//! synthetic chain of N BSIM3 inverters (the `corpus/regress/
//! bsim3_inv.cir` model cards), per thread count, plus a check that
//! every run is bit-identical to the sequential one.
//!
//! ```sh
//! cargo run --release -p dune-core --features parallel --example parload_bench -- 10 50 200
//! ```
//!
//! Arguments: chain lengths (default `10 50 100 200 400`). Optional
//! `--threads 1,2,4,8`, `--reps 3` (best of) and `--width k` (`k`
//! inverters in parallel per stage: `2·k·n` MOS on only `n + 2`
//! nodes, so the device load dominates the dense LU).

use std::path::Path;
use std::time::Instant;

use dune_core::{Circuit, Data, Options, Solution};

fn chain(n: usize, width: usize) -> dune_netlist::ir::Netlist {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/regress/bsim3_inv.cir");
    let text = std::fs::read_to_string(path).expect("bsim3_inv.cir");
    let models = &text[text.find(".model n1").unwrap()..text.find(".end").unwrap()];
    let mut d = String::from("bsim3 inverter chain\nvdd vdd 0 dc 3.3\n");
    d.push_str("vin in0 0 dc 0 pulse(0 3.3 1n 0.5n 0.5n 4n 10n)\n");
    for k in 0..n {
        let (a, b) = (format!("in{k}"), format!("in{}", k + 1));
        for j in 0..width {
            d.push_str(&format!(
                "mn{k}_{j} {b} {a} 0 0 n1 l=0.35u w=2u ad=2p as=2p pd=6u ps=6u\n\
                 mp{k}_{j} {b} {a} vdd vdd p1 l=0.35u w=5u ad=5p as=5p pd=12u ps=12u\n"
            ));
        }
        d.push_str(&format!("c{k} {b} 0 {}f\n", 30 * width));
    }
    d.push_str(".tran 0.1n 10n\n");
    d.push_str(models);
    d.push_str(".end\n");
    dune_netlist::parse_str("chain.cir", &d).netlist
}

fn bits(s: &[Solution]) -> Vec<u64> {
    s.iter()
        .flat_map(|p| match &p.data {
            Data::Real(cols) => cols
                .iter()
                .flatten()
                .map(|v| v.to_bits())
                .collect::<Vec<_>>(),
            Data::Complex(cols) => cols
                .iter()
                .flatten()
                .flat_map(|z| [z.re.to_bits(), z.im.to_bits()])
                .collect(),
        })
        .collect()
}

fn main() {
    let mut sizes = Vec::new();
    let mut threads = vec![1, 2, 4, 6, 8];
    let mut reps = 3;
    let mut width = 1;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--threads" => {
                threads = args
                    .next()
                    .expect("--threads list")
                    .split(',')
                    .map(|t| t.parse().expect("thread count"))
                    .collect()
            }
            "--width" => width = args.next().expect("--width k").parse().expect("width"),
            "--reps" => reps = args.next().expect("--reps n").parse().expect("reps"),
            n => sizes.push(n.parse::<usize>().expect("chain length")),
        }
    }
    if sizes.is_empty() {
        sizes = vec![10, 50, 100, 200, 400];
    }
    print!("| inverters | MOS |");
    for t in &threads {
        print!(" {t} thr |");
    }
    println!();
    print!("|---|---|");
    for _ in &threads {
        print!("---|");
    }
    println!();
    for n in sizes {
        let nl = chain(n, width);
        let (ckt, _) = Circuit::from_netlist(&nl).expect("elaborates");
        let mut base: Option<(f64, Vec<u64>)> = None;
        print!("| {n} | {} |", 2 * n * width);
        for &t in &threads {
            let opts = Options {
                load_threads: t,
                ..Options::default()
            };
            let mut best = f64::INFINITY;
            let mut out = Vec::new();
            for _ in 0..reps {
                let t0 = Instant::now();
                let (sols, _) = ckt.run(&nl.analyses[0], &opts).expect("runs");
                best = best.min(t0.elapsed().as_secs_f64());
                out = bits(&sols);
            }
            match &base {
                None => {
                    print!(" {:.1} ms |", best * 1e3);
                    base = Some((best, out));
                }
                Some((b, ref_bits)) => {
                    assert_eq!(
                        &out, ref_bits,
                        "{n} inverters, {t} threads: not bit-identical"
                    );
                    print!(" {:.1} ms ({:.2}×) |", best * 1e3, b / best);
                }
            }
        }
        println!();
    }
}
