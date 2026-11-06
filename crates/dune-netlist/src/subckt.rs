//! Subcircuit expansion (`fte/subckt.c`), done on the IR instead of
//! by string rewriting.
//!
//! Naming follows the ngspice convention (the 3f5 used `:` where
//! ngspice uses `.`; the oracle compares vector names against
//! ngspice, see `docs/dune-netlist.md`):
//!
//! - a device `r1` inside instance `x1` becomes `r.x1.r1` — the type
//!   letter stays first, then the instance path, then the name;
//! - an internal node `n1` becomes `x1.n1`; ground (`0`) and nodes
//!   bound to ports keep the caller's names;
//! - device references (the inductors of a `K`, the controlling
//!   source of `F`/`H`/`W`) are renamed like devices;
//! - a model defined inside a subcircuit becomes `x1.mname` and only
//!   elements of that instance see it; models defined outside stay
//!   visible (the C translates every local name unconditionally).

use std::collections::HashMap;

use crate::diag::Diagnostic;
use crate::ir::{Device, Element, Model};
use crate::span::Span;

/// Everything collected in one lexical scope (the top level or one
/// `.subckt` body).
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub elements: Vec<Element>,
    pub models: Vec<Model>,
    pub subckts: Vec<SubcktDef>,
}

#[derive(Debug)]
pub(crate) struct SubcktDef {
    pub name: String,
    pub ports: Vec<String>,
    pub scope: Scope,
    pub span: Span,
}

/// Nesting bound for subcircuit instantiation.
const MAX_DEPTH: usize = 64;
/// Single work budget for the whole expansion: every element visit,
/// instantiation and emitted model costs one unit, so a subcircuit
/// bomb (wide `X`-only trees, model-heavy bodies) cannot exhaust
/// memory or CPU regardless of its shape.
const MAX_WORK: usize = 1 << 20;

/// Flatten the top scope: expand every `X` call, return the flat
/// element list and all models (global + instantiated local ones).
pub(crate) fn expand(top: &Scope, diags: &mut Vec<Diagnostic>) -> (Vec<Element>, Vec<Model>) {
    let mut ex = Expander {
        diags,
        elements: Vec::new(),
        models: top.models.clone(),
        work: 0,
        overflowed: false,
    };
    let mut defs: Vec<&[SubcktDef]> = vec![&top.subckts];
    let empty_nodes = HashMap::new();
    let empty_models = HashMap::new();
    for el in &top.elements {
        ex.emit(el, &mut defs, "", &empty_nodes, &empty_models, 0);
    }
    (ex.elements, ex.models)
}

struct Expander<'a> {
    diags: &'a mut Vec<Diagnostic>,
    elements: Vec<Element>,
    models: Vec<Model>,
    work: usize,
    overflowed: bool,
}

impl Expander<'_> {
    /// Spend one unit of the expansion budget; `false` means stop
    /// (the diagnostic is emitted once).
    fn charge(&mut self, span: Span) -> bool {
        if self.overflowed {
            return false;
        }
        self.work += 1;
        if self.work > MAX_WORK {
            self.overflowed = true;
            self.diags.push(Diagnostic::error(
                format!("subcircuit expansion exceeds {MAX_WORK} steps; giving up"),
                span,
            ));
            return false;
        }
        true
    }

    /// Emit one element: either a flattened copy or, for `X`, the
    /// instantiated body of its subcircuit.
    fn emit<'s>(
        &mut self,
        el: &'s Element,
        defs: &mut Vec<&'s [SubcktDef]>,
        path: &str,
        node_map: &HashMap<String, String>,
        model_map: &HashMap<String, String>,
        depth: usize,
    ) {
        if !self.charge(el.span) {
            return;
        }
        if let Device::SubcktCall { subname } = &el.device {
            self.instantiate(el, subname, defs, path, node_map, model_map, depth);
            return;
        }
        let mut out = el.clone();
        out.name = rename(&el.name, path);
        for n in &mut out.nodes {
            *n = map_node(n, path, node_map);
        }
        match &mut out.device {
            Device::Mutual { l1, l2, .. } => {
                *l1 = rename(l1, path);
                *l2 = rename(l2, path);
            }
            Device::Cccs { vname, .. }
            | Device::Ccvs { vname, .. }
            | Device::SwitchI { vname, .. } => {
                *vname = rename(vname, path);
            }
            _ => {}
        }
        if let Some(m) = device_model_mut(&mut out.device)
            && let Some(renamed) = model_map.get(m.as_str())
        {
            *m = renamed.clone();
        }
        self.elements.push(out);
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate<'s>(
        &mut self,
        call: &'s Element,
        subname: &str,
        defs: &mut Vec<&'s [SubcktDef]>,
        path: &str,
        node_map: &HashMap<String, String>,
        model_map: &HashMap<String, String>,
        depth: usize,
    ) {
        if depth >= MAX_DEPTH {
            self.diags.push(Diagnostic::error(
                format!("subcircuits nested deeper than {MAX_DEPTH} levels (recursive?)"),
                call.span,
            ));
            return;
        }
        // Innermost definition wins.
        let Some(def) = defs
            .iter()
            .rev()
            .copied()
            .find_map(|scope| scope.iter().find(|d| d.name == subname))
        else {
            self.diags.push(Diagnostic::error(
                format!("no such subcircuit `{subname}`"),
                call.span,
            ));
            return;
        };
        if def.ports.len() != call.nodes.len() {
            self.diags.push(Diagnostic::error(
                format!(
                    "subcircuit `{subname}` has {} ports but the call connects {}",
                    def.ports.len(),
                    call.nodes.len()
                ),
                call.span,
            ));
            return;
        }
        let inst_path = if path.is_empty() {
            call.name.clone()
        } else {
            format!("{path}.{}", call.name)
        };
        // Ports bind to the caller's (already mapped) nodes.
        let mut inner_nodes = HashMap::new();
        for (port, actual) in def.ports.iter().zip(&call.nodes) {
            inner_nodes.insert(port.clone(), map_node(actual, path, node_map));
        }
        // Local models shadow outer renames.
        let mut inner_models = model_map.clone();
        for m in &def.scope.models {
            if !self.charge(m.span) {
                return;
            }
            let new_name = format!("{inst_path}.{}", m.name);
            inner_models.insert(m.name.clone(), new_name.clone());
            let mut renamed = m.clone();
            renamed.name = new_name;
            self.models.push(renamed);
        }
        defs.push(&def.scope.subckts);
        for el in &def.scope.elements {
            self.emit(el, defs, &inst_path, &inner_nodes, &inner_models, depth + 1);
        }
        defs.pop();
    }
}

/// `r1` in instance path `x1` → `r.x1.r1` (ngspice convention).
fn rename(name: &str, path: &str) -> String {
    if path.is_empty() {
        return name.to_string();
    }
    let letter = name.chars().next().unwrap_or('?');
    format!("{letter}.{path}.{name}")
}

fn map_node(node: &str, path: &str, node_map: &HashMap<String, String>) -> String {
    if node == "0" {
        return "0".to_string();
    }
    if let Some(mapped) = node_map.get(node) {
        return mapped.clone();
    }
    if path.is_empty() {
        node.to_string()
    } else {
        format!("{path}.{node}")
    }
}

fn device_model_mut(device: &mut Device) -> Option<&mut String> {
    match device {
        Device::Resistor { model, .. } | Device::Capacitor { model, .. } => model.as_mut(),
        Device::Diode { model, .. }
        | Device::Bjt { model, .. }
        | Device::Jfet { model, .. }
        | Device::Mesfet { model, .. }
        | Device::Mosfet { model, .. }
        | Device::SwitchV { model, .. }
        | Device::SwitchI { model, .. }
        | Device::TlineLossy { model }
        | Device::Urc { model, .. } => Some(model),
        _ => None,
    }
}
