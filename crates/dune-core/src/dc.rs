//! `.dc` — DC transfer curves (`ckt/dctrcurv.c`): one or two
//! variables swept, first innermost, operating point at every step.
//! The 3f5 sweeps V/I sources only; ngspice-47 adds resistors and
//! the circuit temperature (`.dc temp …`), which Dune follows
//! (corpus/KNOWN_DIFFS.md).

use dune_raw::VarType;

use crate::circuit::{Circuit, Spec, VarKind};
use crate::consts::CTOK;
use crate::engine::{Engine, SweepRef, SweepTarget};
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
    /// Run a DC transfer-curve analysis over one or two sweep
    /// variables: V/I sources, resistors or `temp` (°C).
    pub fn dc(&self, opts: &Options, sweeps: &[DcSweep]) -> Result<Dc, Error> {
        if sweeps.is_empty() || sweeps.len() > 2 {
            return Err(Error::Build(".dc takes one or two sweep sources".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);

        // ngspice-47 `dctrcurv.c` looks the name up as a resistor,
        // then a V and an I source, then the word `temp`; the 3f5
        // only knows the sources (corpus/KNOWN_DIFFS.md).
        enum Scale {
            V,
            I,
            Res,
            Temp,
        }
        let mut refs = Vec::new();
        let mut scale = Scale::V;
        for (level, s) in sweeps.iter().enumerate() {
            let name = s.src.to_ascii_lowercase();
            let resistor = self
                .specs
                .iter()
                .position(|sp| matches!(sp, Spec::Resistor(r) if r.name == name));
            let (target, kind) = if let Some(d) = resistor {
                (SweepTarget::Device(d), Scale::Res)
            } else if let Some(&(d, is_vsrc)) = engine.sources.get(&name) {
                (
                    SweepTarget::Device(d),
                    if is_vsrc { Scale::V } else { Scale::I },
                )
            } else if name == "temp" {
                (SweepTarget::Temp, Scale::Temp)
            } else {
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
                scale = kind;
            }
            refs.push(SweepRef {
                target,
                start: s.start,
                stop: s.stop,
                step: s.step,
            });
        }

        // A temperature level rebuilds the devices at each point
        // (`CKTtemp`): instance `TEMP=` values stay, everything that
        // follows the circuit temperature moves.
        let mut retemp = |celsius: f64| -> Result<Vec<Box<dyn crate::devices::Device>>, Error> {
            let mut o = opts.clone();
            o.temp = celsius + CTOK;
            Ok(self.build(&o)?.devices)
        };
        let rows = engine.dc(opts, &refs, &mut retemp)?;

        // ngspice calls the sweep vector `v-sweep` / `i-sweep` (saved
        // like any node/branch vector) and `res-sweep` /
        // `temp-sweep`, typed by their own names.
        let (scale_name, scale_type) = match scale {
            Scale::V => ("v(v-sweep)".to_string(), VarType::Voltage),
            Scale::I => ("i(i-sweep)".to_string(), VarType::Current),
            Scale::Res => (
                "res-sweep".to_string(),
                VarType::Other("res-sweep".to_string()),
            ),
            Scale::Temp => (
                "temp-sweep".to_string(),
                VarType::Other("temp-sweep".to_string()),
            ),
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
