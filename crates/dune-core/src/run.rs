//! [`Circuit::run_card`]: one analysis card of the netlist IR,
//! dispatched to its engine call. This is the single card→analysis
//! mapping the CLI, the oracle, the wasm build and the language
//! bindings share (directly, or through [`Circuit::run`], its
//! plots-only form), so a deck runs the same everywhere.

use dune_netlist::ir::{self, Analysis, Variation};

use crate::ac::AcSweep;
use crate::circuit::Circuit;
use crate::dc::DcSweep;
use crate::error::Error;
use crate::options::Options;
use crate::pss::{Pss, PssParams};
use crate::pz::{PzInput, PzWhich};
use crate::sens::SensAc;
use crate::solution::Solution;
use crate::tf::OutputVar;

/// What one analysis card produced ([`Circuit::run_card`]).
#[derive(Clone, Debug)]
pub enum CardOutput {
    /// Every card but `.pss`: its plots in rawfile order and its notes
    /// (see [`Circuit::run`]).
    Plots {
        plots: Vec<Solution>,
        notes: Vec<String>,
    },
    /// A `.pss` card, kept whole: besides its two plots, the period,
    /// `t0` and the shooting history ([`Pss`]) that the plots alone do
    /// not carry.
    Pss(Box<Pss>),
}

impl CardOutput {
    /// The card's plots in rawfile order and its notes — what
    /// [`Circuit::run`] returns: a `.pss` gives its time plot, then
    /// its spectrum.
    pub fn into_plots(self) -> (Vec<Solution>, Vec<String>) {
        match self {
            CardOutput::Plots { plots, notes } => (plots, notes),
            CardOutput::Pss(p) => {
                let Pss {
                    time,
                    spectrum,
                    notes,
                    ..
                } = *p;
                (vec![time, spectrum], notes)
            }
        }
    }
}

impl Circuit {
    /// Run one analysis card (`.op`, `.dc`, `.tran`, `.ac`, `.pz`,
    /// `.noise`, `.disto`, `.tf`, `.sens`, ngspice's `.sp`, and `.pss`) as
    /// `spice3 -b` (`ngspice -b` for `.sp`) would.
    ///
    /// Returns the plots in rawfile order with their 3f5/ngspice
    /// plotnames — one for most analyses; `.noise` gives the density
    /// plot plus, when the band has more than one frequency, the
    /// integrated totals; `.disto` gives 2 or 3 plots; `.pss` the
    /// period and its harmonics — and the analysis' notes (non-fatal
    /// remarks the caller may show). This is [`Circuit::run_card`]
    /// with the `.pss` result reduced to its plots
    /// ([`CardOutput::into_plots`]).
    ///
    /// A `.tf`/`.sens` output that is not `V(..)`/`I(..)` is an
    /// [`Error::Build`]; engine failures pass through unchanged.
    pub fn run(
        &self,
        analysis: &Analysis,
        opts: &Options,
    ) -> Result<(Vec<Solution>, Vec<String>), Error> {
        self.run_card(analysis, opts).map(CardOutput::into_plots)
    }

    /// Run one analysis card as [`Circuit::run`] does, keeping a
    /// `.pss` result whole ([`CardOutput::Pss`]: period, `t0` and the
    /// shooting history besides the plots). The card's parameters are
    /// read here and only here — for `.pss`, the [`PssParams`] of the
    /// card (ngspice's integer truncation of the counts, the
    /// defaults of [`PssParams::default`] for what it omits).
    pub fn run_card(&self, analysis: &Analysis, opts: &Options) -> Result<CardOutput, Error> {
        let (mut plots, notes) = match self.run_plots(analysis, opts)? {
            CardOutput::Plots { plots, notes } => (plots, notes),
            pss @ CardOutput::Pss(_) => return Ok(pss),
        };
        // `.options keepopinfo`: the small-signal analyses dump their
        // operating point as a plot of its own first (`CKTdump` under
        // `CKTkeepOpInfo`). Names are the C's, copy-paste bug
        // included: `pzan.c` calls its plot "Distortion Operating
        // Point" (3f5 `acan.c:73`, `pzan.c:46`, `distoan.c:126`).
        // `.noise` and `.sp` dump only in ngspice-47 (`noisean.c:221`,
        // `span.c:476`), which Dune follows (corpus/KNOWN_DIFFS.md).
        if opts.keepopinfo {
            let name = match analysis {
                Analysis::Ac { .. } | Analysis::Sp { .. } => Some("AC Operating Point"),
                Analysis::Pz { .. } | Analysis::Disto { .. } => Some("Distortion Operating Point"),
                Analysis::Noise { .. } => Some("NOISE Operating Point"),
                _ => None,
            };
            if let Some(name) = name {
                let mut op = self.op(opts)?.solution;
                op.plotname = name.to_string();
                plots.insert(0, op);
            }
        }
        Ok(CardOutput::Plots { plots, notes })
    }

    /// The card's engine call; `keepopinfo` is [`Circuit::run_card`]'s.
    fn run_plots(&self, analysis: &Analysis, opts: &Options) -> Result<CardOutput, Error> {
        let (plots, notes) = match analysis {
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
            Analysis::Sp {
                variation,
                points,
                fstart,
                fstop,
                donoise,
                ..
            } => {
                let sp = self.sp(
                    opts,
                    sweep(*variation),
                    *points as usize,
                    *fstart,
                    *fstop,
                    *donoise,
                )?;
                (vec![sp.solution], sp.notes)
            }
            Analysis::Pss {
                freq,
                tstab,
                osc_node,
                points,
                harmonics,
                sc_iter,
                steady_coeff,
                uic,
                ..
            } => {
                let d = PssParams::default();
                // ngspice reads the counts as IF_INTEGER (truncated).
                let count = |v: Option<f64>, default: usize| match v {
                    Some(v) if v.is_finite() && v >= 0.0 => v.trunc().min(1e9) as usize,
                    Some(_) => 0,
                    None => default,
                };
                let p = PssParams {
                    freq: (*freq != 0.0).then_some(*freq),
                    tstab: *tstab,
                    osc_node: Some(osc_node.clone()),
                    points: count(*points, d.points),
                    harmonics: count(*harmonics, d.harmonics),
                    max_iter: count(*sc_iter, d.max_iter),
                    steady_coeff: steady_coeff.unwrap_or(d.steady_coeff),
                    uic: *uic,
                };
                return Ok(CardOutput::Pss(Box::new(self.pss(opts, &p)?)));
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
        };
        Ok(CardOutput::Plots { plots, notes })
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
