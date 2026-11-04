//! SPICE3 rawfile reader and writer.
//!
//! The format contract is `corpus/spice3f5/src/lib/fte/rawfile.c`
//! (`raw_write`/`raw_read`, Wayne A. Christopher, 1986): a header of
//! `Keyword: value` lines (`Title`, `Date`, `Plotname`, `Flags`,
//! `No. Variables`, `No. Points`, optional `Dimensions`, `Command`,
//! `Option`), a `Variables:` table, and a `Values:` (ASCII) or `Binary:`
//! (native-endian `f64`) data section. One file may carry several plots
//! appended back to back; a plot is complex as a whole if any of its
//! vectors is complex.
//!
//! The writer reproduces the 3f5 output byte for byte (15-digit `%e`
//! floats, point index starting each ASCII row, one value per line, a
//! blank line after each point). The reader is deliberately flexible,
//! like the C one: it only requires one `Variables:` and one data section
//! per plot, in that order, and treats whitespace loosely. Known
//! divergences between 3f5 and ngspice output are listed in
//! `docs/dune-raw.md`.
//!
//! Malformed input produces a structured [`Error`] with a line number —
//! never a panic.

#![forbid(unsafe_code)]

mod error;
mod plot;
mod read;
mod types;
mod write;

pub use error::Error;
pub use plot::{Data, Plot, Variable};
pub use types::VarType;
pub use write::{Format, fmt_e};

/// A parsed rawfile: one or more plots, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Rawfile {
    pub plots: Vec<Plot>,
    /// Non-fatal reader diagnostics (unknown flags, bad attributes…),
    /// mirroring the warnings the 3f5 reader prints to stderr.
    pub warnings: Vec<String>,
}

impl Rawfile {
    /// Parse a rawfile from bytes (ASCII or binary data sections).
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        read::parse(bytes)
    }

    /// Read and parse a rawfile from disk.
    pub fn read_from(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        Self::parse(&std::fs::read(path)?)
    }

    /// Write every plot, back to back, as the 3f5 `raw_write` would.
    pub fn write(&self, w: &mut impl std::io::Write, format: Format) -> std::io::Result<()> {
        for plot in &self.plots {
            plot.write(w, format)?;
        }
        Ok(())
    }

    /// Serialize to an in-memory buffer.
    pub fn to_bytes(&self, format: Format) -> Vec<u8> {
        let mut buf = Vec::new();
        self.write(&mut buf, format)
            .expect("writing to a Vec cannot fail");
        buf
    }
}
