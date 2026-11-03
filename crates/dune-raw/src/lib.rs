//! SPICE3 rawfile reader and writer, byte-compatible with what ngspice
//! reads and writes (`Title`, `Date`, `Plotname`, `Flags`, `No. Variables`,
//! `No. Points`, `Variables:`, `Values:`/`Binary:`).
//!
//! Reference: `corpus/spice3f5/src/lib/fte/rawfile.c`.

#![forbid(unsafe_code)]
