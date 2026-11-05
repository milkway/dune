//! Circuit simulation engine derived from SPICE3f5: MNA, devices,
//! Newton-Raphson, analyses. The netlist IR (`dune-netlist`) is the
//! bridge from decks; results mirror the rawfile (D-0008).
//!
//! Porting notes live in `docs/dune-core.md`; the C sources cited in
//! each module are under `corpus/spice3f5/src/lib`.

#![forbid(unsafe_code)]

pub mod consts;
pub mod linalg;

mod ac;
mod bjt;
mod circuit;
mod dc;
mod devices;
mod diode;
mod elaborate;
mod engine;
mod error;
mod integ;
mod matrix;
mod mos1;
mod mos2;
mod mos6;
mod noise;
mod op;
mod options;
mod pz;
mod select;
mod solution;
mod srcfn;
mod state;
mod tran;

pub use ac::{Ac, AcSweep};
pub use bjt::BjtModel;
pub use circuit::{
    Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Mos1, Mos2, Mos6, Mutual, Resistor,
    Spec, Vccs, Vcvs, Vsource,
};
pub use dc::{Dc, DcSweep};
pub use diode::DiodeModel;
pub use error::Error;
pub use integ::Method;
pub use mos1::Mos1Model;
pub use mos2::Mos2Model;
pub use mos6::Mos6Model;
pub use noise::Noise;
pub use op::Op;
pub use options::Options;
pub use pz::{Pz, PzInput, PzWhich};
pub use select::{ColData, OutColumn, OutExpr, Part, Selection};
pub use solution::Solution;
pub use srcfn::Wave;
pub use tran::Tran;
