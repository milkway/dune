//! SPICE3f5 netlist lexer, parser, and intermediate representation.
//!
//! The grammar contract is the SPICE3f5 user's manual
//! (`corpus/spice3f5/lib/helpdir/spice.txt`) and the C parser in
//! `corpus/spice3f5/src/lib/inp`. Malformed input must always produce a
//! structured diagnostic with a span — never a panic, never a hang.

#![forbid(unsafe_code)]
