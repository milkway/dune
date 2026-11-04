//! IR → engine elaboration: `Circuit::from_netlist` (D-0008). The
//! netlist crate guarantees shape (nodes present, numbers parsed);
//! this pass resolves models, applies defaults and rejects what the
//! engine cannot simulate yet.

use std::collections::HashMap;

use dune_netlist::ir::{self, Netlist, ParamValue};

use crate::circuit::{
    Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Mutual, Resistor, Vccs, Vcvs, Vsource,
};
use crate::consts::CTOK;
use crate::diode::DiodeModel;
use crate::error::Error;

impl Circuit {
    /// Build a circuit from a parsed (and already subcircuit-
    /// expanded) netlist. `notes` collects remarks worth printing;
    /// anything the engine cannot represent faithfully is an error.
    pub fn from_netlist(nl: &Netlist) -> Result<(Circuit, Vec<String>), Error> {
        let mut problems: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();

        // Diode models (`.model … d(…)`).
        let mut dmodels: HashMap<&str, DiodeModel> = HashMap::new();
        for m in &nl.models {
            if m.mtype == "d" {
                match diode_model(m) {
                    Ok(dm) => {
                        dmodels.insert(m.name.as_str(), dm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                }
            }
        }

        let mut ckt = Circuit::new(nl.title.clone());

        for el in &nl.elements {
            let name = el.name.clone();
            match &el.device {
                ir::Device::Resistor {
                    value,
                    model,
                    params,
                } => {
                    if model.is_some() {
                        problems.push(format!(
                            "`{name}`: semiconductor resistor models are not implemented yet"
                        ));
                        continue;
                    }
                    if !params.is_empty() {
                        notes.push(format!("`{name}`: resistor parameters ignored (Phase 1)"));
                    }
                    match value {
                        Some(v) => add(
                            &mut ckt,
                            Resistor {
                                name,
                                nodes: two(&el.nodes),
                                value: *v,
                            },
                            &mut problems,
                        ),
                        None => problems.push(format!("`{name}`: resistor has no value")),
                    }
                }
                ir::Device::Capacitor {
                    value,
                    model,
                    params,
                } => {
                    if model.is_some() {
                        problems.push(format!(
                            "`{name}`: semiconductor capacitor models are not implemented yet"
                        ));
                        continue;
                    }
                    let ic = num_param(params, "ic");
                    match value {
                        Some(v) => add(
                            &mut ckt,
                            Capacitor {
                                name,
                                nodes: two(&el.nodes),
                                value: *v,
                                ic,
                            },
                            &mut problems,
                        ),
                        None => problems.push(format!("`{name}`: capacitor has no value")),
                    }
                }
                ir::Device::Inductor { value, params } => {
                    let ic = num_param(params, "ic");
                    match value {
                        Some(v) => add(
                            &mut ckt,
                            Inductor {
                                name,
                                nodes: two(&el.nodes),
                                value: *v,
                                ic,
                            },
                            &mut problems,
                        ),
                        None => problems.push(format!("`{name}`: inductor has no value")),
                    }
                }
                ir::Device::Mutual { l1, l2, k } => add(
                    &mut ckt,
                    Mutual {
                        name,
                        l1: l1.clone(),
                        l2: l2.clone(),
                        k: *k,
                    },
                    &mut problems,
                ),
                ir::Device::Source {
                    kind, dc, tran, ac, ..
                } => {
                    let ac = ac.as_ref().map(|a| (a.mag, a.phase));
                    let wave = tran.as_ref().map(|t| {
                        let args = t.args.clone();
                        match t.kind {
                            ir::TranFnKind::Pulse => crate::srcfn::Wave::Pulse(args),
                            ir::TranFnKind::Sin => crate::srcfn::Wave::Sin(args),
                            ir::TranFnKind::Exp => crate::srcfn::Wave::Exp(args),
                            ir::TranFnKind::Pwl => crate::srcfn::Wave::Pwl(args),
                            ir::TranFnKind::Sffm => crate::srcfn::Wave::Sffm(args),
                        }
                    });
                    match kind {
                        ir::SourceKind::Voltage => add(
                            &mut ckt,
                            Vsource {
                                name,
                                nodes: two(&el.nodes),
                                dc: *dc,
                                wave,
                                ac,
                            },
                            &mut problems,
                        ),
                        ir::SourceKind::Current => add(
                            &mut ckt,
                            Isource {
                                name,
                                nodes: two(&el.nodes),
                                dc: *dc,
                                wave,
                                ac,
                            },
                            &mut problems,
                        ),
                    }
                }
                ir::Device::Vcvs { gain } => add(
                    &mut ckt,
                    Vcvs {
                        name,
                        nodes: four(&el.nodes),
                        gain: gain.unwrap_or(0.0),
                    },
                    &mut problems,
                ),
                ir::Device::Vccs { gain } => add(
                    &mut ckt,
                    Vccs {
                        name,
                        nodes: four(&el.nodes),
                        gain: gain.unwrap_or(0.0),
                    },
                    &mut problems,
                ),
                ir::Device::Cccs { vname, gain } => add(
                    &mut ckt,
                    Cccs {
                        name,
                        nodes: two(&el.nodes),
                        ctrl: vname.clone(),
                        gain: gain.unwrap_or(0.0),
                    },
                    &mut problems,
                ),
                ir::Device::Ccvs { vname, gain } => add(
                    &mut ckt,
                    Ccvs {
                        name,
                        nodes: two(&el.nodes),
                        ctrl: vname.clone(),
                        gain: gain.unwrap_or(0.0),
                    },
                    &mut problems,
                ),
                ir::Device::Diode {
                    model,
                    area,
                    off,
                    params,
                } => {
                    let Some(dm) = dmodels.get(model.as_str()) else {
                        problems.push(format!(
                            "`{name}`: unable to find definition of model `{model}`"
                        ));
                        continue;
                    };
                    let mut temp = None;
                    let mut ic = None;
                    for p in params {
                        match (p.name.as_str(), &p.value) {
                            ("temp", ParamValue::Num(v)) => temp = Some(v + CTOK),
                            ("ic", ParamValue::Num(v)) => ic = Some(*v),
                            _ => problems.push(format!("`{name}`: unknown parameter ({})", p.name)),
                        }
                    }
                    add(
                        &mut ckt,
                        Diode {
                            name,
                            nodes: two(&el.nodes),
                            model: dm.clone(),
                            area: area.unwrap_or(1.0),
                            off: *off,
                            temp,
                            ic,
                        },
                        &mut problems,
                    );
                }
                other => {
                    problems.push(format!(
                        "`{name}`: {} is not implemented yet (Phase {})",
                        device_kind_name(other),
                        device_phase(other),
                    ));
                }
            }
        }

        for ni in &nl.node_inits {
            match ni.kind {
                ir::NodeInitKind::Nodeset => ckt.nodeset(ni.node.clone(), ni.value),
                ir::NodeInitKind::Ic => ckt.ic(ni.node.clone(), ni.value),
            }
        }

        if !problems.is_empty() {
            return Err(Error::Elab(problems.join("\n")));
        }
        Ok((ckt, notes))
    }
}

fn add(ckt: &mut Circuit, spec: impl Into<crate::circuit::Spec>, problems: &mut Vec<String>) {
    if let Err(e) = ckt.add(spec) {
        problems.push(e.to_string());
    }
}

fn num_param(params: &[ir::Param], name: &str) -> Option<f64> {
    params
        .iter()
        .find_map(|p| match (&p.value, p.name == name) {
            (ParamValue::Num(v), true) => Some(*v),
            _ => None,
        })
}

fn two(nodes: &[String]) -> [String; 2] {
    [nodes[0].clone(), nodes[1].clone()]
}

fn four(nodes: &[String]) -> [String; 4] {
    [
        nodes[0].clone(),
        nodes[1].clone(),
        nodes[2].clone(),
        nodes[3].clone(),
    ]
}

fn device_kind_name(d: &ir::Device) -> &'static str {
    match d {
        ir::Device::Bjt { .. } => "the BJT",
        ir::Device::Mosfet { .. } => "the MOSFET",
        ir::Device::Jfet { .. } => "the JFET",
        ir::Device::Mesfet { .. } => "the MESFET",
        ir::Device::SwitchV { .. } | ir::Device::SwitchI { .. } => "the switch",
        ir::Device::TlineIdeal { .. } | ir::Device::TlineLossy { .. } => "the transmission line",
        ir::Device::Urc { .. } => "the URC line",
        ir::Device::Behavioral { .. } => "the B source",
        ir::Device::SubcktCall { .. } => "an unexpanded subcircuit call",
        _ => "this device",
    }
}

fn device_phase(d: &ir::Device) -> &'static str {
    match d {
        ir::Device::SubcktCall { .. } => "1 — the definition is missing",
        _ => "3",
    }
}

/// Map `.model x d(...)` parameters onto [`DiodeModel`], 3f5 names
/// (`dev/dio/diompar.c`; `cj0` is accepted for `cjo` like the C).
fn diode_model(m: &ir::Model) -> Result<DiodeModel, String> {
    let mut dm = DiodeModel::default();
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "is" => dm.is = v,
            "rs" => dm.rs = v,
            "n" => dm.n = v,
            "tt" => dm.tt = v,
            "cjo" | "cj0" => dm.cjo = v,
            "vj" => dm.vj = v,
            "m" => dm.m = v,
            "eg" => dm.eg = v,
            "xti" => dm.xti = v,
            "kf" => dm.kf = v,
            "af" => dm.af = v,
            "fc" => dm.fc = v,
            "bv" => dm.bv = Some(v),
            "ibv" => dm.ibv = v,
            "tnom" => dm.tnom = Some(v + CTOK),
            other => return Err(format!("unknown parameter ({other})")),
        }
    }
    Ok(dm)
}
