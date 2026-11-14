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
pub mod ir;
pub mod span;

mod dot;
mod elem;
mod expr;
mod meas;
mod param;
mod parse;
mod pexpr;
mod scan;
mod subckt;

pub use deck::{Deck, IncludeLoader};
pub use diag::{Diagnostic, Severity};
pub use expr::{ExprError, MAX_DEPTH as EXPR_MAX_DEPTH, MAX_NODES as EXPR_MAX_NODES, parse_expr};
pub use meas::{MeasSyntaxError, MeasWarning, parse_measure_body};
pub use parse::{ParseOutcome, parse_file, parse_str, parse_with_loader};
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
