//! `.tf` — DC small-signal transfer function (`ckt/tfanal.c`).
//!
//! After the operating point, the Jacobian of the last Newton
//! iteration is still factored. One solve with a unit excitation on
//! the input source — `+1` on the branch row of a voltage source, a
//! unit current from `N+` to `N-` through a current source — yields
//! the transfer function at the output and the input resistance
//! (`-1/i(branch)`, or `v(N-) - v(N+)`); a second solve with a unit
//! current injected into the output node pair (or `+1` on the output
//! source's branch row) yields the output resistance. The three
//! values form one real point of the `Transfer Function` plot.

use dune_raw::{Data, VarType};

use crate::circuit::{Circuit, Spec, VarKind};
use crate::engine::Engine;
use crate::error::Error;
use crate::matrix::{AnKind, Var};
use crate::options::Options;
use crate::solution::Solution;

/// The output variable of `.tf` and `.sens`: `V(N1<,N2>)` or
/// `I(VXXXXXXX)` (the current through a branch-carrying element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputVar {
    /// `V(NODE)` or `V(NODE,REFERENCE)`.
    Voltage {
        node: String,
        reference: Option<String>,
    },
    /// `I(VSRC)`: the branch current of a voltage source (or any
    /// element with a branch row: E, H, L).
    Current { vsrc: String },
}

/// Result of a `.tf` analysis. The plot holds the same three values
/// under the ngspice vector names (`Transfer_function`,
/// `<src>#Input_impedance`, `output_impedance_at_V(…)` or
/// `<src>#Output_impedance`).
#[derive(Clone, Debug)]
pub struct Tf {
    pub solution: Solution,
    /// Output over input (V/V, A/V, V/A or A/A).
    pub transfer_function: f64,
    /// Small-signal resistance seen by the input source (ohms).
    pub input_impedance: f64,
    /// Small-signal resistance at the output (ohms).
    pub output_impedance: f64,
    pub notes: Vec<String>,
}

/// A resolved output: node pair or branch row.
#[derive(Clone, Copy, Debug)]
pub(crate) enum OutRef {
    Volt(Var, Var),
    Branch(Var),
}

impl OutRef {
    pub fn read(&self, x: &[f64]) -> f64 {
        match *self {
            OutRef::Volt(p, n) => x[p] - x[n],
            OutRef::Branch(b) => x[b],
        }
    }
}

/// Resolve an [`OutputVar`] against the engine's unknowns; `what`
/// names the analysis in errors.
pub(crate) fn resolve_output(
    engine: &Engine,
    output: &OutputVar,
    what: &str,
) -> Result<OutRef, Error> {
    let node = |name: &str| -> Result<Var, Error> {
        let name = name.to_ascii_lowercase();
        if name == "0" {
            return Ok(0);
        }
        engine
            .vars
            .names
            .iter()
            .zip(&engine.vars.kinds)
            .position(|(n, k)| *n == name && !matches!(k, VarKind::Branch))
            .ok_or_else(|| Error::Build(format!("{what} names unknown node `{name}`")))
    };
    match output {
        OutputVar::Voltage { node: p, reference } => Ok(OutRef::Volt(
            node(p)?,
            match reference {
                Some(r) => node(r)?,
                None => 0,
            },
        )),
        OutputVar::Current { vsrc } => Ok(OutRef::Branch(branch_of(engine, vsrc).ok_or_else(
            || Error::Build(format!("{what} output `{vsrc}` has no branch current")),
        )?)),
    }
}

/// `CKTfndBranch`: the branch row of a voltage-defined element.
fn branch_of(engine: &Engine, name: &str) -> Option<Var> {
    let name = name.to_ascii_lowercase();
    engine
        .vars
        .names
        .iter()
        .zip(&engine.vars.kinds)
        .position(|(n, k)| *n == name && matches!(k, VarKind::Branch))
}

/// The input source of `.tf`.
enum Input {
    V(Var),
    I(Var, Var),
}

impl Circuit {
    /// Run `.tf OUTVAR INSRC`.
    pub fn tf(&self, opts: &Options, output: &OutputVar, insrc: &str) -> Result<Tf, Error> {
        let insrc = insrc.to_ascii_lowercase();
        // `TFanal`: the input must be an independent V or I source.
        let mut input_nodes = None;
        for spec in &self.specs {
            match spec {
                Spec::Vsource(v) if v.name == insrc => input_nodes = Some(None),
                Spec::Isource(i) if i.name == insrc => {
                    input_nodes = Some(Some(i.nodes.clone()));
                }
                _ => {}
            }
        }
        let Some(input_nodes) = input_nodes else {
            let known = self.specs.iter().any(|s| s.name() == insrc);
            return Err(Error::Build(if known {
                format!("transfer function source `{insrc}` not of proper type")
            } else {
                format!("transfer function source `{insrc}` not in circuit")
            }));
        };

        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);
        engine.set_kind(AnKind::DcOp, false);
        engine.op(opts)?;

        let out = resolve_output(&engine, output, ".tf")?;
        let var_of = |name: &str| -> Var {
            if name == "0" {
                return 0;
            }
            engine
                .vars
                .names
                .iter()
                .zip(&engine.vars.kinds)
                .position(|(n, k)| n == name && !matches!(k, VarKind::Branch))
                .unwrap_or(0)
        };
        let input = match &input_nodes {
            Some([n1, n2]) => Input::I(var_of(n1), var_of(n2)),
            None => Input::V(branch_of(&engine, &insrc).ok_or_else(|| {
                Error::Build(format!("transfer function source `{insrc}` has no branch"))
            })?),
        };

        let n = engine.vars.len();
        let mut rhs = vec![0.0; n];
        match input {
            Input::I(n1, n2) => {
                rhs[n1] -= 1.0;
                rhs[n2] += 1.0;
            }
            Input::V(br) => rhs[br] += 1.0,
        }
        engine.solve_factored(&mut rhs)?;
        let transfer_function = out.read(&rhs);
        let input_impedance = match input {
            Input::I(n1, n2) => rhs[n2] - rhs[n1],
            Input::V(br) => {
                if rhs[br].abs() < 1e-20 {
                    1e20
                } else {
                    -1.0 / rhs[br]
                }
            }
        };

        let same_source =
            matches!(output, OutputVar::Current { vsrc } if vsrc.eq_ignore_ascii_case(&insrc));
        let output_impedance = if same_source {
            // "no need to compute output resistance when it is the
            // same as the input"
            input_impedance
        } else {
            rhs.fill(0.0);
            match out {
                OutRef::Volt(p, q) => {
                    rhs[p] -= 1.0;
                    rhs[q] += 1.0;
                }
                OutRef::Branch(b) => rhs[b] += 1.0,
            }
            engine.solve_factored(&mut rhs)?;
            match out {
                OutRef::Volt(p, q) => rhs[q] - rhs[p],
                OutRef::Branch(b) => 1.0 / f64::max(1e-20, rhs[b]),
            }
        };

        let out_name = match output {
            OutputVar::Voltage { node, reference } => match reference {
                Some(r) => format!("v(output_impedance_at_V({node},{r}))"),
                None => format!("v(output_impedance_at_V({node}))"),
            },
            OutputVar::Current { vsrc } => format!("v({vsrc}#Output_impedance)"),
        };
        let variables = vec![
            ("v(Transfer_function)".to_string(), VarType::Voltage),
            (format!("v({insrc}#Input_impedance)"), VarType::Voltage),
            (out_name, VarType::Voltage),
        ];
        Ok(Tf {
            solution: Solution {
                plotname: "Transfer Function".to_string(),
                variables,
                data: Data::Real(vec![
                    vec![transfer_function],
                    vec![input_impedance],
                    vec![output_impedance],
                ]),
                scale: None,
            },
            transfer_function,
            input_impedance,
            output_impedance,
            notes,
        })
    }
}
