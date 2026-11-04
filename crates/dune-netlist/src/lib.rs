//! SPICE3f5 netlist lexer, parser, and intermediate representation.
//!
//! The grammar contract is the SPICE3f5 user's manual
//! (`corpus/spice3f5/lib/helpdir/spice.txt`) and the C parser in
//! `corpus/spice3f5/src/lib/inp`. Malformed input must always produce a
//! structured diagnostic with a span — never a panic, never a hang.
//!
//! Porting notes live in `docs/dune-netlist.md`.

#![forbid(unsafe_code)]

pub mod deck;
pub mod diag;
pub mod span;

// `allow(dead_code)` until the card parsers land (next commits).
#[allow(dead_code)]
mod scan;

pub use deck::{Deck, IncludeLoader};
pub use diag::{Diagnostic, Severity};
pub use span::{FileId, Location, SourceMap, Span};

/// Hard failures outside the deck itself (the root file is unreadable).
/// Everything found *inside* a deck is a [`Diagnostic`], not an error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot read {path}: {message}")]
    Io {
        path: std::path::PathBuf,
        message: String,
    },
}
