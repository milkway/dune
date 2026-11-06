//! Solver backends. Only this subtree may depend on external linear
//! algebra crates (CLAUDE.md, Princípios 3).

pub(crate) mod dense_det;
#[cfg(feature = "solver-faer")]
pub mod faer_dense;
#[cfg(feature = "solver-xingo")]
pub mod xingo_sparse;
