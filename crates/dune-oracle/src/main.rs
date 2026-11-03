//! Numerical oracle harness (see `DECISIONS.md`, D-0006): runs ngspice in
//! batch mode, compares SPICE3 rawfiles against Dune's output with the
//! SPICE tolerances (`reltol=1e-3`, `abstol=1e-12`, `vntol=1e-6`).

#![forbid(unsafe_code)]

fn main() -> anyhow::Result<()> {
    anyhow::bail!("dune-oracle: not implemented yet (Phase 0)");
}
