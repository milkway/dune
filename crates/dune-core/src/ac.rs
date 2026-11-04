//! `.ac` — small-signal frequency sweep (`ckt/acan.c` via the
//! engine): linearize at the operating point, solve the complex MNA
//! system per frequency.

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::circuit::{Circuit, VarKind};
use crate::engine::{AcStepType, Engine};
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// `.ac` sweep spacing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcSweep {
    /// `DEC n`: n points per decade.
    Decade,
    /// `OCT n`: n points per octave.
    Octave,
    /// `LIN n`: n points from fstart to fstop.
    Linear,
}

/// Result of an AC analysis.
#[derive(Clone, Debug)]
pub struct Ac {
    pub solution: Solution,
    pub notes: Vec<String>,
}

impl Circuit {
    /// Run `.ac DEC|OCT|LIN N FSTART FSTOP`.
    pub fn ac(
        &self,
        opts: &Options,
        sweep: AcSweep,
        points: usize,
        fstart: f64,
        fstop: f64,
    ) -> Result<Ac, Error> {
        let ok = fstart > 0.0 && fstart.is_finite() && fstop.is_finite() && fstop >= fstart;
        if !ok {
            return Err(Error::Build(".ac frequencies are inconsistent".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        let step_type = match sweep {
            AcSweep::Decade => AcStepType::Decade,
            AcSweep::Octave => AcStepType::Octave,
            AcSweep::Linear => AcStepType::Linear,
        };
        let rows = engine.ac(opts, step_type, points, fstart, fstop)?;

        let mut variables = vec![("frequency".to_string(), VarType::Frequency)];
        let mut columns: Vec<Vec<Complex64>> =
            vec![rows.iter().map(|(f, _)| Complex64::new(*f, 0.0)).collect()];
        for i in 1..engine.vars.len() {
            let name = &engine.vars.names[i];
            match engine.vars.kinds[i] {
                VarKind::Node if !name.contains('#') => {
                    variables.push((format!("v({name})"), VarType::Voltage));
                    columns.push(rows.iter().map(|(_, sol)| sol[i]).collect());
                }
                VarKind::Branch => {
                    variables.push((format!("i({name})"), VarType::Current));
                    columns.push(rows.iter().map(|(_, sol)| sol[i]).collect());
                }
                _ => {}
            }
        }
        Ok(Ac {
            solution: Solution {
                plotname: "AC Analysis".to_string(),
                variables,
                data: Data::Complex(columns),
                scale: Some(0),
            },
            notes,
        })
    }
}
