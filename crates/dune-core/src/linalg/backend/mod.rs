//! Solver backends. Only this subtree may depend on external linear
//! algebra crates (CLAUDE.md, Princípios 3).

#[cfg(feature = "faer")]
pub mod faer_dense;
