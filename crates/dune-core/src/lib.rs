//! Circuit simulation engine derived from Berkeley SPICE3f5.
//!
//! Reference: Quarles, *Analysis of Performance and Convergence Issues for
//! Circuit Simulation*, UCB/ERL M89/42 (1989), and the C sources in
//! `corpus/spice3f5/src/lib/ckt` and `src/lib/dev`.
//!
//! Linear solvers live behind `linalg::backend` (see `DECISIONS.md`,
//! D-0005); no other module may depend on a concrete solver crate.

#![forbid(unsafe_code)]
