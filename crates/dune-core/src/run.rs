//! [`Circuit::run`]: one analysis card of the netlist IR, dispatched
//! to its engine call. This is the single card→analysis mapping the
//! CLI, the oracle, the wasm build and the language bindings share,
//! so a deck runs the same everywhere.

use dune_netlist::ir::{self, Analysis, Variation};

use crate::ac::AcSweep;
use crate::circuit::Circuit;
use crate::dc::DcSweep;
use crate::error::Error;
use crate::options::Options;
use crate::pz::{PzInput, PzWhich};
use crate::sens::SensAc;
use crate::solution::Solution;
use crate::tf::OutputVar;

impl Circuit {
    /// Run one analysis card (`.op`, `.dc`, `.tran`, `.ac`, `.pz`,
    /// `.noise`, `.disto`, `.tf`, `.sens`) as `spice3 -b` would.
    ///
    /// Returns the plots in rawfile order with their 3f5/ngspice
    /// plotnames — one for most analyses; `.noise` gives the density
    /// plot plus, when the band has more than one frequency, the
    /// integrated totals; `.disto` gives 2 or 3 plots — and the
    /// analysis' notes (non-fatal remarks the caller may show).
    ///
    /// A `.tf`/`.sens` output that is not `V(..)`/`I(..)` is an
    /// [`Error::Build`]; engine failures pass through unchanged.
    pub fn run(
        &self,
        analysis: &Analysis,
        opts: &Options,
    ) -> Result<(Vec<Solution>, Vec<String>), Error> {
        Ok(match analysis {
            Analysis::Op { .. } => {
                let op = self.op(opts)?;
                (vec![op.solution], op.notes)
            }
            Analysis::Dc { sweeps, .. } => {
                let sweeps: Vec<DcSweep> = sweeps
                    .iter()
                    .map(|s| DcSweep {
                        src: s.src.clone(),
                        start: s.start,
                        stop: s.stop,
                        step: s.step,
                    })
                    .collect();
                let dc = self.dc(opts, &sweeps)?;
                (vec![dc.solution], dc.notes)
            }
            Analysis::Tran {
                tstep,
                tstop,
                tstart,
                tmax,
                uic,
                ..
            } => {
                let tr = self.tran(opts, *tstep, *tstop, tstart.unwrap_or(0.0), *tmax, *uic)?;
                (vec![tr.solution], tr.notes)
            }
            Analysis::Ac {
                variation,
                points,
                fstart,
                fstop,
                ..
            } => {
                let ac = self.ac(opts, sweep(*variation), *points as usize, *fstart, *fstop)?;
                (vec![ac.solution], ac.notes)
            }
            Analysis::Pz {
                nodes,
                input,
                which,
                ..
            } => {
                let input = match input {
                    ir::PzInput::Current => PzInput::Current,
                    ir::PzInput::Voltage => PzInput::Voltage,
                };
                let which = match which {
                    ir::PzWhich::Poles => PzWhich::Poles,
                    ir::PzWhich::Zeros => PzWhich::Zeros,
                    ir::PzWhich::Both => PzWhich::Both,
                };
                let pz = self.pz(
                    opts,
                    [
                        nodes[0].as_str(),
                        nodes[1].as_str(),
                        nodes[2].as_str(),
                        nodes[3].as_str(),
                    ],
                    input,
                    which,
                )?;
                (vec![pz.solution], pz.notes)
            }
            Analysis::Noise {
                output,
                output_ref,
                src,
                variation,
                points,
                fstart,
                fstop,
                pts_per_summary,
                ..
            } => {
                let output = OutputVar::Voltage {
                    node: output.clone(),
                    reference: output_ref.clone(),
                };
                let no = self.noise(
                    opts,
                    &output,
                    src,
                    sweep(*variation),
                    *points as usize,
                    *fstart,
                    *fstop,
                    pts_per_summary.map(|p| p as usize),
                )?;
                let mut plots = vec![no.density];
                plots.extend(no.integrated);
                (plots, no.notes)
            }
            Analysis::Disto {
                variation,
                points,
                fstart,
                fstop,
                f2overf1,
                ..
            } => {
                let di = self.disto(
                    opts,
                    sweep(*variation),
                    *points as usize,
                    *fstart,
                    *fstop,
                    *f2overf1,
                )?;
                (di.solutions, di.notes)
            }
            Analysis::Tf { outvar, insrc, .. } => {
                let tf = self.tf(opts, &output_var(outvar)?, insrc)?;
                (vec![tf.solution], tf.notes)
            }
            Analysis::Sens {
                outvar,
                filters,
                ac,
                ..
            } => {
                let ac = ac.map(|(variation, points, fstart, fstop)| SensAc {
                    sweep: sweep(variation),
                    points: points as usize,
                    fstart,
                    fstop,
                });
                let sens = self.sens(opts, &output_var(outvar)?, filters, ac)?;
                (vec![sens.solution], sens.notes)
            }
        })
    }
}

/// The card's sweep keyword in the engine's form.
fn sweep(v: Variation) -> AcSweep {
    match v {
        Variation::Dec => AcSweep::Decade,
        Variation::Oct => AcSweep::Octave,
        Variation::Lin => AcSweep::Linear,
    }
}

/// The `.tf`/`.sens` output variable in the engine's form.
fn output_var(v: &ir::OutVar) -> Result<OutputVar, Error> {
    match v {
        ir::OutVar::Voltage {
            node, reference, ..
        } => Ok(OutputVar::Voltage {
            node: node.clone(),
            reference: reference.clone(),
        }),
        ir::OutVar::Current { vsrc, .. } => Ok(OutputVar::Current { vsrc: vsrc.clone() }),
        ir::OutVar::Raw(r) => Err(Error::Build(format!("`{r}` is not an output variable"))),
    }
}
