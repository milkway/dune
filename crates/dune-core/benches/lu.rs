//! LU benchmark (P-3): `factor` + `solve_in_place` of fixed MNA
//! matrices with the pattern analyzed once, i.e. one Newton
//! iteration's linear-algebra cost, for every backend this build has
//! (faer dense always; xingo-sparse with `--features solver-xingo`).
//!
//! Fixtures (`benches/fixtures/*.mna`) are the last real matrix the
//! first engine of each corpus deck factored (captured from a
//! `dune -b` run): `n nnz gmin` then `row col value re im` per
//! pattern entry (`re`/`im` is the first AC matrix, zero when the
//! deck has no small-signal analysis). They are small (13–85
//! unknowns); the synthetic resistive meshes (`mesh/k×k`, a 5-point
//! stencil with a grounded leak at every node) extend the sweep to
//! find the dense/sparse crossover.

use std::hint::black_box;
use std::path::PathBuf;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use dune_core::linalg::{LinearSolver, Pattern};

struct Fixture {
    name: String,
    pattern: Pattern,
    values: Vec<f64>,
    gmin: f64,
    re: Vec<f64>,
    im: Vec<f64>,
}

impl Fixture {
    fn rhs(&self) -> Vec<f64> {
        (0..self.pattern.n)
            .map(|i| 1.0 + (i % 7) as f64 * 1e-3)
            .collect()
    }

    fn has_ac(&self) -> bool {
        self.re.iter().chain(&self.im).any(|v| *v != 0.0)
    }
}

fn load(name: &str) -> Fixture {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "benches", "fixtures"]
        .iter()
        .collect::<PathBuf>()
        .join(format!("{name}.mna"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
    let n: usize = head[0].parse().unwrap();
    let gmin: f64 = head[2].parse().unwrap();
    let (mut entries, mut values, mut re, mut im) = (vec![], vec![], vec![], vec![]);
    for l in lines {
        let f: Vec<&str> = l.split_whitespace().collect();
        entries.push((f[0].parse().unwrap(), f[1].parse().unwrap()));
        values.push(f[2].parse().unwrap());
        re.push(f[3].parse().unwrap());
        im.push(f[4].parse().unwrap());
    }
    Fixture {
        name: name.into(),
        pattern: Pattern { n, entries },
        values,
        gmin,
        re,
        im,
    }
}

/// `k`×`k` resistive mesh: 1 mS links, 1 µS leak to ground per node.
fn mesh(k: usize) -> Fixture {
    let n = k * k;
    let id = |i: usize, j: usize| i * k + j;
    let (mut entries, mut values) = (vec![], vec![]);
    for i in 0..k {
        for j in 0..k {
            let a = id(i, j);
            let mut diag = 1e-6;
            for (di, dj) in [(0i64, 1i64), (1, 0), (0, -1), (-1, 0)] {
                let (ii, jj) = (i as i64 + di, j as i64 + dj);
                if ii >= 0 && jj >= 0 && (ii as usize) < k && (jj as usize) < k {
                    entries.push((a, id(ii as usize, jj as usize)));
                    values.push(-1e-3);
                    diag += 1e-3;
                }
            }
            entries.push((a, a));
            values.push(diag);
        }
    }
    let m = entries.len();
    Fixture {
        name: format!("mesh{k}x{k}"),
        pattern: Pattern { n, entries },
        values,
        gmin: 0.0,
        re: vec![0.0; m],
        im: vec![0.0; m],
    }
}

type MakeSolver = fn() -> Box<dyn LinearSolver>;

fn backends() -> Vec<(&'static str, MakeSolver)> {
    let faer: (&'static str, MakeSolver) = ("faer", || {
        Box::new(dune_core::linalg::backend::faer_dense::FaerDense::default())
    });
    #[cfg_attr(not(feature = "solver-xingo"), allow(unused_mut))]
    let mut v = vec![faer];
    #[cfg(feature = "solver-xingo")]
    v.push(("xingo", || {
        Box::new(dune_core::linalg::backend::xingo_sparse::XingoSparse::default())
    }));
    v
}

fn bench_real(c: &mut Criterion, fixtures: &[Fixture], group: &str) {
    let mut g = c.benchmark_group(group);
    for fx in fixtures {
        if fx.pattern.n >= 900 {
            g.sample_size(10);
        }
        let rhs = fx.rhs();
        for (label, make) in backends() {
            let mut s = make();
            s.analyze(&fx.pattern);
            let mut x = rhs.clone();
            g.bench_with_input(
                BenchmarkId::new(label, format!("{} n={}", fx.name, fx.pattern.n)),
                fx,
                |b, fx| {
                    b.iter(|| {
                        s.factor(black_box(&fx.values), fx.gmin).unwrap();
                        x.copy_from_slice(&rhs);
                        s.solve_in_place(black_box(&mut x)).unwrap();
                    })
                },
            );
        }
    }
    g.finish();
}

fn bench_complex(c: &mut Criterion, fixtures: &[Fixture]) {
    let mut g = c.benchmark_group("lu_complex");
    for fx in fixtures.iter().filter(|f| f.has_ac()) {
        let rhs = fx.rhs();
        for (label, make) in backends() {
            let mut s = make();
            s.analyze(&fx.pattern);
            let (mut xr, mut xi) = (rhs.clone(), vec![0.0; rhs.len()]);
            g.bench_function(
                BenchmarkId::new(label, format!("{} n={}", fx.name, fx.pattern.n)),
                |b| {
                    b.iter(|| {
                        s.factor_complex(black_box(&fx.re), black_box(&fx.im))
                            .unwrap();
                        xr.copy_from_slice(&rhs);
                        xi.fill(0.0);
                        s.solve_complex_in_place(&mut xr, &mut xi).unwrap();
                    })
                },
            );
        }
    }
    g.finish();
}

fn lu(c: &mut Criterion) {
    let corpus: Vec<Fixture> = ["mosmem", "schmitt", "rca3040", "mos6inv", "ltra_3"]
        .iter()
        .map(|n| load(n))
        .collect();
    bench_real(c, &corpus, "lu_corpus");
    bench_complex(c, &corpus);
    let meshes: Vec<Fixture> = [4, 8, 12, 16, 24, 32, 48]
        .iter()
        .map(|&k| mesh(k))
        .collect();
    bench_real(c, &meshes, "lu_mesh");
}

criterion_group!(benches, lu);
criterion_main!(benches);
