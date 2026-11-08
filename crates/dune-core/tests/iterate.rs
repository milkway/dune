//! Which Newton iterate the analyses report (docs/dune-core.md §Qual
//! iterado cada análise lê): the C's `NIiter` returns converged
//! without its final `rhs`/`rhsOld` swap and `CKTdump` reads
//! `CKTrhsOld`, and `DCtrCurv` solves the points after the first in
//! `MODEINITPRED` from rotated state banks. The two probes below sit
//! where getting either wrong moves the answer far above rounding
//! (1.7e-4 V and 5e-4 V); done as the C does, they match their
//! ngspice-47 goldens to rounding. The oracle's 2×SPICE ruler cannot
//! pin this (the iterates differ by less than one convergence
//! tolerance), so the bound here is 1e-12 of the column's scale.

use std::path::{Path, PathBuf};

use dune_core::{Circuit, Options};
use dune_raw::Rawfile;

fn corpus(rel: &str) -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../corpus")).join(rel)
}

fn lockstep(stem: &str) {
    let text = std::fs::read_to_string(corpus(&format!("regress/{stem}.cir"))).unwrap();
    let parsed = dune_netlist::parse_str(stem, &text);
    assert!(parsed.is_clean(), "{:?}", parsed.diags);
    let (c, _) = Circuit::from_netlist(&parsed.netlist).unwrap();
    let mut opts = Options::default();
    let mut d = Vec::new();
    opts.apply(&parsed.netlist.options, &mut d);
    let (mut sols, _) = c.run(&parsed.netlist.analyses[0], &opts).unwrap();
    let sol = sols.remove(0);

    let raw = Rawfile::read_from(corpus(&format!("golden/{stem}.raw"))).unwrap();
    let plot = &raw.plots[0];
    assert_eq!(plot.num_points(), sol.num_points(), "{stem}: point count");
    for (name, _) in &sol.variables {
        let want = plot
            .real_values(name)
            .unwrap_or_else(|| panic!("{stem}: golden has no {name}"));
        let got = sol.column(name).unwrap();
        let scale = want.iter().fold(0.0f64, |m, v| m.max(v.abs())).max(1e-30);
        for (i, (a, b)) in got.iter().zip(want).enumerate() {
            assert!(
                (a - b).abs() <= 1e-12 * scale,
                "{stem}: {name}[{i}] = {a:e}, ngspice-47 {b:e}"
            );
        }
    }
}

/// `.op` with a triple root: Newton converges linearly, so the
/// penultimate iterate (the C's) and the last solve differ by 1.7e-4 V.
#[test]
fn op_reports_the_iterate_the_convergence_test_passed() {
    lockstep("iter_op");
}

/// `.dc` of a MOS1 inverter plus a switch: starting the later points
/// in INITFLOAT instead of the C's INITPRED moves v(3) by 5e-4 V.
#[test]
fn dc_continues_in_initpred_from_rotated_states() {
    lockstep("iter_dcpred");
}
