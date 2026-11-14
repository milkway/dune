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
mod asrc;
mod b1disto;
mod bjt;
mod bsim1;
mod bsim2;
mod bsim3;
mod circuit;
mod dc;
mod dderivs;
mod devices;
mod devnoise;
mod diode;
mod disto;
mod elaborate;
mod engine;
mod error;
pub mod four;
mod integ;
mod jfet;
mod ltra;
mod ltrafit;
mod matrix;
pub mod measure;
mod mes;
mod mos1;
mod mos2;
mod mos3;
mod mos6;
mod mosdisto;
mod noise;
mod op;
mod options;
#[cfg(feature = "parallel")]
mod parload;
mod pss;
mod pz;
mod run;
mod select;
mod sens;
mod session;
mod solution;
mod sp;
mod srcfn;
mod state;
mod sweep;
mod switch;
mod tf;
mod tra;
mod tran;
mod trnoise;

pub use ac::{Ac, AcSweep};
pub use bjt::BjtModel;
pub use bsim1::Bsim1Model;
pub use bsim2::Bsim2Model;
pub use bsim3::Bsim3Model;
pub use circuit::{
    Asrc, Bjt, Bsim1, Bsim2, Bsim3, Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Jfet,
    Ltra, Mes, Mos1, Mos2, Mos3, Mos6, Mutual, Resistor, RfPort, Spec, SwitchI, SwitchV, Tra, Vccs,
    Vcvs, Vsource,
};
pub use dc::{Dc, DcSweep};
pub use diode::DiodeModel;
pub use disto::Disto;
/// Rawfile column data and variable kinds, as carried by
/// [`Solution::data`] and [`Solution::variables`]; re-exported so the
/// public surface does not need `dune_raw` (the `dune_raw` paths name
/// the same types).
pub use dune_raw::{Data, VarType};
pub use error::Error;
pub use four::{FourError, FourOutcome, FourParams, Fourier, Harmonic, four_all};
pub use integ::Method;
pub use jfet::JfetModel;
pub use ltra::{LtraInterp, LtraModel};
pub use measure::{MeasClause, MeasDetail, MeasureError, MeasureOutcome, Measurement, measure_all};
pub use mes::MesModel;
pub use mos1::Mos1Model;
pub use mos2::Mos2Model;
pub use mos3::Mos3Model;
pub use mos6::Mos6Model;
pub use noise::Noise;
pub use op::Op;
pub use options::{Compat, LTRA_BREAK_RATE, Options, SPICE_TRTOL};
pub use pss::{PSS_FREQ_PLOT, PSS_TIME_PLOT, Pss, PssParams};
pub use pz::{Pz, PzInput, PzWhich};
pub use run::CardOutput;
pub use select::{ColData, OutColumn, OutExpr, Part, Selection};
pub use sens::{Sens, SensAc};
pub use session::{AlterValue, Session, WarmStats};
pub use solution::Solution;
pub use sp::Sp;
pub use srcfn::Wave;
#[cfg(feature = "parallel")]
pub use sweep::sweep_parallel;
pub use sweep::{SweepOutput, SweepParam, SweepRun, sweep};
pub use switch::{CswModel, SwModel};
pub use tf::{OutputVar, Tf};
pub use tran::Tran;
