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
mod bsim1;
mod bsim2;
mod circuit;
mod dc;
mod dderivs;
mod devices;
mod diode;
mod disto;
mod elaborate;
mod engine;
mod error;
mod integ;
mod jfet;
mod ltra;
mod matrix;
mod mes;
mod mos1;
mod mos2;
mod mos3;
mod mos6;
mod mosdisto;
mod noise;
mod op;
mod options;
mod pz;
mod select;
mod sens;
mod solution;
mod srcfn;
mod state;
mod switch;
mod tf;
mod tra;
mod tran;

pub use ac::{Ac, AcSweep};
pub use bjt::BjtModel;
pub use bsim1::Bsim1Model;
pub use bsim2::Bsim2Model;
pub use circuit::{
    Bjt, Bsim1, Bsim2, Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Jfet, Ltra, Mes,
    Mos1, Mos2, Mos3, Mos6, Mutual, Resistor, Spec, SwitchI, SwitchV, Tra, Vccs, Vcvs, Vsource,
};
pub use dc::{Dc, DcSweep};
pub use diode::DiodeModel;
pub use disto::Disto;
pub use error::Error;
pub use integ::Method;
pub use jfet::JfetModel;
pub use ltra::{LtraInterp, LtraModel};
pub use mes::MesModel;
pub use mos1::Mos1Model;
pub use mos2::Mos2Model;
pub use mos3::Mos3Model;
pub use mos6::Mos6Model;
pub use noise::Noise;
pub use op::Op;
pub use options::Options;
pub use pz::{Pz, PzInput, PzWhich};
pub use select::{ColData, OutColumn, OutExpr, Part, Selection};
pub use sens::{Sens, SensAc};
pub use solution::Solution;
pub use srcfn::Wave;
pub use switch::{CswModel, SwModel};
pub use tf::{OutputVar, Tf};
pub use tran::Tran;
