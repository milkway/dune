//! Whole-analysis benchmark (P-3): a `.op` Newton loop and a full
//! `.tran` per deck, through the public API (`Circuit::from_netlist`
//! → `op`/`tran`). The engine uses the backend the build selected
//! (`default_solver`), so run it once per feature set:
//!
//! ```sh
//! cargo bench -p dune-core --bench sim                    # faer
//! cargo bench -p dune-core --bench sim --features solver-xingo  # xingo
//! ```
//!
//! Benchmark ids carry the backend label, so both runs coexist in
//! `target/criterion`. Decks: corpus circuits (small: 3–44 unknowns)
//! plus synthetic RC meshes (`k`×`k` 1 kΩ grid, 1 pF per node to
//! ground, pulse-driven corner) for the crossover.

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dune_core::{Circuit, Options};
use dune_netlist::ir::Analysis;

const BACKEND: &str = if cfg!(feature = "solver-xingo") {
    "xingo"
} else {
    "faer"
};

struct Deck {
    name: String,
    circuit: Circuit,
    options: Options,
    tran: Option<(f64, f64, f64, Option<f64>, bool)>,
}

fn build(name: &str, parsed: dune_netlist::ParseOutcome) -> Deck {
    assert!(parsed.is_clean(), "{name}: parse errors");
    let (circuit, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let tran = parsed.netlist.analyses.iter().find_map(|a| match a {
        Analysis::Tran {
            tstep,
            tstop,
            tstart,
            tmax,
            uic,
            ..
        } => Some((*tstep, *tstop, tstart.unwrap_or(0.0), *tmax, *uic)),
        _ => None,
    });
    Deck {
        name: name.into(),
        circuit,
        options,
        tran,
    }
}

fn corpus(rel: &str) -> Deck {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "..", "corpus", rel]
        .iter()
        .collect();
    let name = path.file_stem().unwrap().to_string_lossy().into_owned();
    build(&name, dune_netlist::parse_file(&path).unwrap())
}

fn rc_mesh(k: usize) -> Deck {
    let node = |i: usize, j: usize| format!("n{i}_{j}");
    let mut s = format!(
        "rc mesh {k}x{k}\nvin {} 0 pulse(0 1 1n 1n 1n 50n)\n",
        node(0, 0)
    );
    for i in 0..k {
        for j in 0..k {
            if j + 1 < k {
                s += &format!("rh{i}_{j} {} {} 1k\n", node(i, j), node(i, j + 1));
            }
            if i + 1 < k {
                s += &format!("rv{i}_{j} {} {} 1k\n", node(i, j), node(i + 1, j));
            }
            s += &format!("c{i}_{j} {} 0 1p\n", node(i, j));
        }
    }
    s += ".tran 1n 100n\n.end\n";
    build(
        &format!("rcmesh{k}x{k}"),
        dune_netlist::parse_str("mesh.cir", &s),
    )
}

fn sim(c: &mut Criterion) {
    let op_decks = [
        "spice3f5/examples/mosmem.cir",
        "spice3f5/examples/schmitt.cir",
        "spice3f5/examples/rca3040.cir",
        "spice3f5/examples/mos6inv.cir",
    ];
    let mut g = c.benchmark_group("op");
    for rel in op_decks {
        let d = corpus(rel);
        g.bench_function(BenchmarkId::new(BACKEND, &d.name), |b| {
            b.iter(|| black_box(d.circuit.op(&d.options).unwrap()))
        });
    }
    g.finish();

    let mut decks: Vec<Deck> = [
        "spice3f5/examples/rc.cir",
        "basic/trrlc.cir",
        "spice3f5/examples/rtlinv.cir",
        "spice3f5/examples/schmitt.cir",
    ]
    .iter()
    .map(|r| corpus(r))
    .collect();
    decks.extend([4, 10, 16, 24].map(rc_mesh));
    let mut g = c.benchmark_group("tran");
    g.sample_size(10);
    for d in &decks {
        let (tstep, tstop, tstart, tmax, uic) = d.tran.expect("deck has .tran");
        g.bench_function(BenchmarkId::new(BACKEND, &d.name), |b| {
            b.iter(|| {
                black_box(
                    d.circuit
                        .tran(&d.options, tstep, tstop, tstart, tmax, uic)
                        .unwrap(),
                )
            })
        });
    }
    g.finish();
}

criterion_group!(benches, sim);
criterion_main!(benches);
