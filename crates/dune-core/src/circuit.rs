//! Public circuit construction (D-0008): one typed spec per device,
//! added to a [`Circuit`]; name/node resolution happens at build
//! time so declaration order never matters (the deck's order does
//! not either).

use std::collections::HashMap;

use crate::diode::{DiodeModel, DiodeTemp};
use crate::error::Error;
use crate::matrix::Var;
use crate::options::Options;

/// `RXXXXXXX N1 N2 VALUE` (ohms; nonzero).
#[derive(Clone, Debug)]
pub struct Resistor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
}

/// `CXXXXXXX N+ N- VALUE <IC=INCOND>` (farads).
#[derive(Clone, Debug)]
pub struct Capacitor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
    /// Initial voltage, used under `.tran … UIC`.
    pub ic: Option<f64>,
}

/// `LYYYYYYY N+ N- VALUE <IC=INCOND>` (henries).
#[derive(Clone, Debug)]
pub struct Inductor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
    /// Initial current, used under `.tran … UIC`.
    pub ic: Option<f64>,
}

/// `KXXXXXXX L1 L2 K`. No effect at the operating point.
#[derive(Clone, Debug)]
pub struct Mutual {
    pub name: String,
    pub l1: String,
    pub l2: String,
    pub k: f64,
}

/// `VXXXXXXX N+ N- <DC VALUE> <tranfn(…)>`.
#[derive(Clone, Debug)]
pub struct Vsource {
    pub name: String,
    pub nodes: [String; 2],
    /// `None` when no DC value was given (transient functions then
    /// also provide the DC-mode value, like `VSRCload`).
    pub dc: Option<f64>,
    pub wave: Option<crate::srcfn::Wave>,
}

/// `IYYYYYYY N+ N- <DC VALUE> <tranfn(…)>` (flows through the source
/// from `+` to `-`).
#[derive(Clone, Debug)]
pub struct Isource {
    pub name: String,
    pub nodes: [String; 2],
    pub dc: Option<f64>,
    pub wave: Option<crate::srcfn::Wave>,
}

/// `EXXXXXXX N+ N- NC+ NC- VALUE` (VCVS).
#[derive(Clone, Debug)]
pub struct Vcvs {
    pub name: String,
    pub nodes: [String; 4],
    pub gain: f64,
}

/// `GXXXXXXX N+ N- NC+ NC- VALUE` (VCCS, mhos).
#[derive(Clone, Debug)]
pub struct Vccs {
    pub name: String,
    pub nodes: [String; 4],
    pub gain: f64,
}

/// `FXXXXXXX N+ N- VNAM VALUE` (CCCS).
#[derive(Clone, Debug)]
pub struct Cccs {
    pub name: String,
    pub nodes: [String; 2],
    pub ctrl: String,
    pub gain: f64,
}

/// `HXXXXXXX N+ N- VNAM VALUE` (CCVS, ohms).
#[derive(Clone, Debug)]
pub struct Ccvs {
    pub name: String,
    pub nodes: [String; 2],
    pub ctrl: String,
    pub gain: f64,
}

/// `DXXXXXXX N+ N- MNAME <AREA> <OFF>` with its model inline.
#[derive(Clone, Debug)]
pub struct Diode {
    pub name: String,
    pub nodes: [String; 2],
    pub model: DiodeModel,
    pub area: f64,
    pub off: bool,
    /// Instance temperature override, kelvin (`TEMP=` is °C on the
    /// card).
    pub temp: Option<f64>,
    /// `IC=VD`: initial junction voltage under `.tran … UIC`.
    pub ic: Option<f64>,
}

#[derive(Clone, Debug)]
pub enum Spec {
    Resistor(Resistor),
    Capacitor(Capacitor),
    Inductor(Inductor),
    Mutual(Mutual),
    Vsource(Vsource),
    Isource(Isource),
    Vcvs(Vcvs),
    Vccs(Vccs),
    Cccs(Cccs),
    Ccvs(Ccvs),
    Diode(Diode),
}

macro_rules! impl_into_spec {
    ($($t:ident),*) => {$(
        impl From<$t> for Spec {
            fn from(s: $t) -> Spec { Spec::$t(s) }
        }
    )*};
}
impl_into_spec!(
    Resistor, Capacitor, Inductor, Mutual, Vsource, Isource, Vcvs, Vccs, Cccs, Ccvs, Diode
);

impl Spec {
    fn name(&self) -> &str {
        match self {
            Spec::Resistor(s) => &s.name,
            Spec::Capacitor(s) => &s.name,
            Spec::Inductor(s) => &s.name,
            Spec::Mutual(s) => &s.name,
            Spec::Vsource(s) => &s.name,
            Spec::Isource(s) => &s.name,
            Spec::Vcvs(s) => &s.name,
            Spec::Vccs(s) => &s.name,
            Spec::Cccs(s) => &s.name,
            Spec::Ccvs(s) => &s.name,
            Spec::Diode(s) => &s.name,
        }
    }
}

/// A circuit under construction. Analyses live in `analysis/`
/// (Phase 1: [`Circuit::op`]).
#[derive(Default)]
pub struct Circuit {
    pub title: String,
    pub(crate) specs: Vec<Spec>,
    /// `.nodeset` entries (node, volts).
    pub(crate) nodesets: Vec<(String, f64)>,
    /// `.ic` entries (node, volts), applied at the transient
    /// operating point.
    pub(crate) ics: Vec<(String, f64)>,
}

impl Circuit {
    pub fn new(title: impl Into<String>) -> Self {
        Circuit {
            title: title.into(),
            ..Circuit::default()
        }
    }

    /// Add one device. Names and nodes are case-insensitive (the 3f5
    /// lowercases the whole deck).
    pub fn add(&mut self, spec: impl Into<Spec>) -> Result<(), Error> {
        let mut spec = spec.into();
        normalize(&mut spec);
        let name = spec.name();
        if name.is_empty() {
            return Err(Error::Build("device name cannot be empty".into()));
        }
        if self.specs.iter().any(|s| s.name() == name) {
            return Err(Error::Build(format!("device `{name}` already defined")));
        }
        if let Spec::Resistor(r) = &spec {
            if r.value == 0.0 || !r.value.is_finite() {
                // spice.txt §3.1.1: "may be positive or negative but
                // not zero".
                return Err(Error::Build(format!(
                    "resistor `{}` cannot have zero resistance",
                    r.name
                )));
            }
        }
        self.specs.push(spec);
        Ok(())
    }

    /// `.nodeset v(node) = value`.
    pub fn nodeset(&mut self, node: impl Into<String>, value: f64) {
        self.nodesets
            .push((node.into().to_ascii_lowercase(), value));
    }

    /// `.ic v(node) = value` (enforced at the transient operating
    /// point; `.tran … UIC` ignores it in favor of device ICs).
    pub fn ic(&mut self, node: impl Into<String>, value: f64) {
        self.ics.push((node.into().to_ascii_lowercase(), value));
    }
}

fn normalize(spec: &mut Spec) {
    #[allow(clippy::ptr_arg)] // uniform `&mut String` for iter_mut().for_each
    fn lc(s: &mut String) {
        s.make_ascii_lowercase();
    }
    match spec {
        Spec::Resistor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Capacitor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Inductor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mutual(s) => {
            lc(&mut s.name);
            lc(&mut s.l1);
            lc(&mut s.l2);
        }
        Spec::Vsource(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Isource(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Vcvs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Vccs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Cccs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
            lc(&mut s.ctrl);
        }
        Spec::Ccvs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
            lc(&mut s.ctrl);
        }
        Spec::Diode(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
    }
}

/// The unknowns of the MNA system: index 0 is ground, then node
/// voltages and branch currents in order of creation (the C's
/// `CKTnode` list).
pub(crate) struct Vars {
    pub names: Vec<String>,
    pub kinds: Vec<VarKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VarKind {
    Ground,
    Node,
    Branch,
}

impl Vars {
    fn new() -> Self {
        Vars {
            names: vec!["0".into()],
            kinds: vec![VarKind::Ground],
        }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    fn push(&mut self, name: String, kind: VarKind) -> Var {
        self.names.push(name);
        self.kinds.push(kind);
        self.names.len() - 1
    }
}

/// Fully resolved circuit: instances bound to variable indices.
pub(crate) struct Built {
    pub vars: Vars,
    pub devices: Vec<Box<dyn crate::devices::Device>>,
    pub nodesets: Vec<(Var, f64)>,
    pub ics: Vec<(Var, f64)>,
    /// Independent V/I source element name → index in `devices`
    /// (what a `.dc` sweep drives) and whether it is a V source.
    pub sources: HashMap<String, (usize, bool)>,
    /// Total device state slots (`CKTnumStates`).
    pub num_states: usize,
    /// Diode temperature-adjust notes etc., surfaced by the caller.
    pub notes: Vec<String>,
}

impl Circuit {
    /// Resolve names and build device instances (the 3f5's setup
    /// pass).
    pub(crate) fn build(&self, opts: &Options) -> Result<Built, Error> {
        use crate::devices::*;

        let mut vars = Vars::new();
        let mut node_ids: HashMap<String, Var> = HashMap::new();
        node_ids.insert("0".into(), 0);
        let node = |vars: &mut Vars, node_ids: &mut HashMap<String, Var>, name: &str| -> Var {
            *node_ids
                .entry(name.to_string())
                .or_insert_with(|| vars.push(name.to_string(), VarKind::Node))
        };

        // Pass 1: node variables in order of first appearance,
        // branch variables per device, the name→branch map (F/H
        // control references), state-slot bases (`CKTnumStates`) and
        // the inductor info the mutuals need.
        let mut branches: HashMap<String, Var> = HashMap::new();
        let mut st = crate::state::StateAlloc::new();
        let mut state_base: Vec<usize> = Vec::with_capacity(self.specs.len());
        let mut inductors: HashMap<String, (usize, f64)> = HashMap::new(); // name → (flux, L)
        for spec in &self.specs {
            state_base.push(match spec {
                Spec::Capacitor(_) => st.alloc(2),
                Spec::Inductor(s) => {
                    let flux = st.alloc(2);
                    inductors.insert(s.name.clone(), (flux, s.value));
                    flux
                }
                Spec::Diode(_) => st.alloc(5),
                _ => 0,
            });
            let node_names: &[String] = match spec {
                Spec::Resistor(s) => &s.nodes,
                Spec::Capacitor(s) => &s.nodes,
                Spec::Inductor(s) => &s.nodes,
                Spec::Mutual(_) => &[],
                Spec::Vsource(s) => &s.nodes,
                Spec::Isource(s) => &s.nodes,
                Spec::Vcvs(s) => &s.nodes,
                Spec::Vccs(s) => &s.nodes,
                Spec::Cccs(s) => &s.nodes,
                Spec::Ccvs(s) => &s.nodes,
                Spec::Diode(s) => &s.nodes,
            };
            for n in node_names {
                node(&mut vars, &mut node_ids, n);
            }
            match spec {
                Spec::Vsource(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                Spec::Inductor(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                Spec::Vcvs(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                Spec::Ccvs(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                // `DIOsetup`: internal node past RS, named
                // `<name>#internal` like `CKTmkVolt` does.
                Spec::Diode(s) if s.model.rs != 0.0 => {
                    node(&mut vars, &mut node_ids, &format!("{}#internal", s.name));
                }
                _ => {}
            }
        }
        let num_states = st.total();

        // Pass 2: instances.
        let mut devices: Vec<Box<dyn Device>> = Vec::new();
        let mut sources: HashMap<String, (usize, bool)> = HashMap::new();
        let mut notes = Vec::new();
        let get = |node_ids: &HashMap<String, Var>, n: &str| node_ids[n];
        for (spec, &base) in self.specs.iter().zip(&state_base) {
            match spec {
                Spec::Resistor(s) => {
                    devices.push(Box::new(ResistorI::new(
                        1.0 / s.value,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                    )));
                }
                Spec::Capacitor(s) => {
                    devices.push(Box::new(CapacitorI::new(
                        s.value,
                        s.ic.unwrap_or(0.0),
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        base,
                    )));
                }
                Spec::Inductor(s) => {
                    devices.push(Box::new(InductorI::new(
                        s.value,
                        s.ic.unwrap_or(0.0),
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        branches[&s.name],
                        base,
                    )));
                }
                Spec::Mutual(s) => {
                    for l in [&s.l1, &s.l2] {
                        if !inductors.contains_key(l) {
                            return Err(Error::Build(format!(
                                "`{}`: coupled inductor `{l}` does not exist",
                                s.name
                            )));
                        }
                    }
                    let (flux1, l1) = inductors[&s.l1];
                    let (flux2, l2) = inductors[&s.l2];
                    // `MUTfactor = k·sqrt(L1·L2)` (`mutsetup.c`).
                    let factor = s.k * (l1 * l2).abs().sqrt();
                    devices.push(Box::new(MutualI::new(
                        factor,
                        branches[&s.l1],
                        branches[&s.l2],
                        flux1,
                        flux2,
                    )));
                }
                Spec::Vsource(s) => {
                    sources.insert(s.name.clone(), (devices.len(), true));
                    devices.push(Box::new(VsourceI::new(
                        s.dc,
                        s.wave.clone(),
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        branches[&s.name],
                    )));
                }
                Spec::Isource(s) => {
                    sources.insert(s.name.clone(), (devices.len(), false));
                    devices.push(Box::new(IsourceI {
                        dc: s.dc,
                        wave: s.wave.clone(),
                        pos: get(&node_ids, &s.nodes[0]),
                        neg: get(&node_ids, &s.nodes[1]),
                    }));
                }
                Spec::Vcvs(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    devices.push(Box::new(VcvsI::new(s.gain, n, branches[&s.name])));
                }
                Spec::Vccs(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    devices.push(Box::new(VccsI::new(s.gain, n)));
                }
                Spec::Cccs(s) => {
                    let ctrl = ctrl_branch(&branches, &s.name, &s.ctrl)?;
                    devices.push(Box::new(CccsI::new(
                        s.gain,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        ctrl,
                    )));
                }
                Spec::Ccvs(s) => {
                    let ctrl = ctrl_branch(&branches, &s.name, &s.ctrl)?;
                    devices.push(Box::new(CcvsI::new(
                        s.gain,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        branches[&s.name],
                        ctrl,
                    )));
                }
                Spec::Diode(s) => {
                    let pos = get(&node_ids, &s.nodes[0]);
                    let neg = get(&node_ids, &s.nodes[1]);
                    let pos_prime = if s.model.rs != 0.0 {
                        get(&node_ids, &format!("{}#internal", s.name))
                    } else {
                        pos
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let t = DiodeTemp::new(&s.name, &s.model, s.area, temp, opts.tnom, opts.reltol);
                    notes.extend(t.notes.iter().cloned());
                    devices.push(Box::new(crate::diode::DiodeI::new(
                        pos,
                        neg,
                        pos_prime,
                        t,
                        s.area,
                        s.off,
                        s.ic.unwrap_or(0.0),
                        base,
                    )));
                }
            }
        }

        // Nodesets and ics must name existing nodes.
        let mut nodesets = Vec::new();
        for (name, v) in &self.nodesets {
            match node_ids.get(name) {
                Some(&var) if var != 0 => nodesets.push((var, *v)),
                Some(_) => notes.push(".nodeset on ground ignored".to_string()),
                None => notes.push(format!(".nodeset names unknown node `{name}`")),
            }
        }
        let mut ics = Vec::new();
        for (name, v) in &self.ics {
            match node_ids.get(name) {
                Some(&var) if var != 0 => ics.push((var, *v)),
                Some(_) => notes.push(".ic on ground ignored".to_string()),
                None => notes.push(format!(".ic names unknown node `{name}`")),
            }
        }

        Ok(Built {
            vars,
            devices,
            nodesets,
            ics,
            sources,
            num_states,
            notes,
        })
    }
}

fn ctrl_branch(branches: &HashMap<String, Var>, name: &str, ctrl: &str) -> Result<Var, Error> {
    if !ctrl.starts_with('v') {
        return Err(Error::Build(format!(
            "`{name}`: controlling source `{ctrl}` must be a voltage source"
        )));
    }
    branches
        .get(ctrl)
        .copied()
        .ok_or_else(|| Error::Build(format!("`{name}`: controlling source `{ctrl}` not found")))
}
