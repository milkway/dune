//! `.tran` — transient analysis (`ckt/dctran.c` via the engine).

use dune_raw::VarType;

use crate::circuit::{Circuit, VarKind};
use crate::engine::{Engine, TranParams};
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// Result of a transient analysis: one row per accepted timepoint.
#[derive(Clone, Debug)]
pub struct Tran {
    pub solution: Solution,
    pub notes: Vec<String>,
}

impl Circuit {
    /// Run `.tran TSTEP TSTOP <TSTART <TMAX>> <UIC>`.
    pub fn tran(
        &self,
        opts: &Options,
        tstep: f64,
        tstop: f64,
        tstart: f64,
        tmax: Option<f64>,
        uic: bool,
    ) -> Result<Tran, Error> {
        // NaN fails these too (a NaN comparison is false).
        let step_ok = tstep > 0.0 && tstep.is_finite();
        if !step_ok {
            return Err(Error::Build(".tran step must be positive".into()));
        }
        let times_ok = tstop > 0.0 && tstop.is_finite() && tstop > tstart && tstart >= 0.0;
        if !times_ok {
            return Err(Error::Build(".tran times are inconsistent".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        let rows = engine.tran(
            opts,
            &TranParams {
                tstep,
                tstop,
                tstart,
                tmax,
                uic,
            },
        )?;

        let mut variables = vec![("time".to_string(), VarType::Time)];
        let mut columns = vec![rows.iter().map(|(t, _)| *t).collect::<Vec<f64>>()];
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
        Ok(Tran {
            solution: Solution {
                plotname: "Transient Analysis".to_string(),
                variables,
                columns,
            },
            notes,
        })
    }
}
