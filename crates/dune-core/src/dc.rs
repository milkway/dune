//! `.dc` — DC transfer curves (`ckt/dctrcurv.c`): one or two V/I
//! sources swept, first source innermost, operating point at every
//! step.

use dune_raw::VarType;

use crate::circuit::{Circuit, VarKind};
use crate::engine::{Engine, SweepRef};
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// One sweep level: `.dc SRC START STOP STEP`.
#[derive(Clone, Debug)]
pub struct DcSweep {
    pub src: String,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

/// Result of a DC transfer-curve analysis.
#[derive(Clone, Debug)]
pub struct Dc {
    pub solution: Solution,
    pub notes: Vec<String>,
}

impl Circuit {
    /// Run a DC transfer-curve analysis over one or two sources.
    pub fn dc(&self, opts: &Options, sweeps: &[DcSweep]) -> Result<Dc, Error> {
        if sweeps.is_empty() || sweeps.len() > 2 {
            return Err(Error::Build(".dc takes one or two sweep sources".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);

        let mut refs = Vec::new();
        let mut scale_is_vsrc = true;
        for (level, s) in sweeps.iter().enumerate() {
            let name = s.src.to_ascii_lowercase();
            let Some(&(dev_index, is_vsrc)) = engine.sources.get(&name) else {
                // The C's "DCtrCurv: source %s not in circuit".
                return Err(Error::Build(format!(
                    "dc sweep source `{name}` not in circuit"
                )));
            };
            // The 3f5 would loop forever on a zero step; ngspice
            // rejects it, and so do we (corpus/KNOWN_DIFFS.md).
            if s.step == 0.0 || !s.step.is_finite() || !s.start.is_finite() {
                return Err(Error::Build(format!(
                    "dc sweep of `{name}` has a zero or non-finite increment"
                )));
            }
            if level == 0 {
                scale_is_vsrc = is_vsrc;
            }
            refs.push(SweepRef {
                dev_index,
                start: s.start,
                stop: s.stop,
                step: s.step,
            });
        }

        let rows = engine.dc(opts, &refs)?;

        // ngspice calls the sweep vector `v-sweep` / `i-sweep` and
        // saves it like any node/branch vector.
        let (scale_name, scale_type) = if scale_is_vsrc {
            ("v(v-sweep)".to_string(), VarType::Voltage)
        } else {
            ("i(i-sweep)".to_string(), VarType::Current)
        };
        let mut variables = vec![(scale_name, scale_type)];
        let mut columns = vec![rows.iter().map(|(scale, _)| *scale).collect::<Vec<f64>>()];
        for i in 1..engine.vars.len() {
            let name = &engine.vars.names[i];
            match engine.vars.kinds[i] {
                VarKind::Node if crate::circuit::saved_node(name) => {
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
        Ok(Dc {
            solution: Solution {
                plotname: "DC transfer characteristic".to_string(),
                variables,
                data: dune_raw::Data::Real(columns),
                scale: Some(0),
            },
            notes,
        })
    }
}
