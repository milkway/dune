//! Circuit simulation engine derived from SPICE3f5: MNA, devices,
//! Newton-Raphson, analyses. The netlist IR (`dune-netlist`) is the
//! bridge from decks; results mirror the rawfile (D-0008).
//!
//! Porting notes live in `docs/dune-core.md`; the C sources cited in
//! each module are under `corpus/spice3f5/src/lib`.

#![forbid(unsafe_code)]

pub mod consts;
pub mod linalg;

mod circuit;
mod devices;
mod diode;
mod elaborate;
mod engine;
mod error;
mod matrix;
mod op;
mod options;
mod solution;

pub use circuit::{
    Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Mutual, Resistor, Spec, Vccs, Vcvs,
    Vsource,
};
pub use diode::DiodeModel;
pub use error::Error;
pub use op::Op;
pub use options::Options;
pub use solution::Solution;
