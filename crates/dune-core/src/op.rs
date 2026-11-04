//! `.op` — the DC operating point (`ckt/dcop.c`: `CKTop` with
//! `MODEDCOP|MODEINITJCT`, then one output point).

use dune_raw::VarType;

use crate::circuit::{Circuit, VarKind};
use crate::engine::Engine;
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// Result of an operating-point analysis.
#[derive(Clone, Debug)]
pub struct Op {
    pub solution: Solution,
    /// Setup/convergence remarks the 3f5 would print as warnings
    /// (diode breakdown matching, nodeset issues, …).
    pub notes: Vec<String>,
}

impl Circuit {
    /// Compute the operating point.
    pub fn op(&self, opts: &Options) -> Result<Op, Error> {
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        engine.op(opts)?;

        // Mirror what ngspice saves for `.op`: every non-ground,
        // non-internal node as `v(name)` and every branch current as
        // `i(name)` (internal `#` nodes are solved but not output).
        let mut variables = Vec::new();
        let mut columns = Vec::new();
        for i in 1..engine.vars.len() {
            let name = &engine.vars.names[i];
            match engine.vars.kinds[i] {
                VarKind::Node if !name.contains('#') => {
                    variables.push((format!("v({name})"), VarType::Voltage));
                    columns.push(vec![engine.sol[i]]);
                }
                VarKind::Branch => {
                    variables.push((format!("i({name})"), VarType::Current));
                    columns.push(vec![engine.sol[i]]);
                }
                _ => {}
            }
        }
        Ok(Op {
            solution: Solution {
                plotname: "Operating Point".to_string(),
                variables,
                columns,
                scale: None,
            },
            notes,
        })
    }
}
