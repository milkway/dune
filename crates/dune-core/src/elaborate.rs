//! IR → engine elaboration: `Circuit::from_netlist` (D-0008). The
//! netlist crate guarantees shape (nodes present, numbers parsed);
//! this pass resolves models, applies defaults and rejects what the
//! engine cannot simulate yet.

use std::collections::HashMap;

use dune_netlist::ir::{self, Netlist, ParamValue};

use crate::circuit::{
    Bjt, Bsim1, Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Mos1, Mos2, Mos6, Mutual,
    Resistor, Vccs, Vcvs, Vsource,
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

        // Diode, BJT and MOS models.
        let mut dmodels: HashMap<&str, DiodeModel> = HashMap::new();
        let mut qmodels: HashMap<&str, crate::bjt::BjtModel> = HashMap::new();
        enum MosModel {
            M1(crate::mos1::Mos1Model),
            M2(crate::mos2::Mos2Model),
            M6(crate::mos6::Mos6Model),
            B1(Box<crate::bsim1::Bsim1Model>),
        }
        let mut mmodels: HashMap<&str, MosModel> = HashMap::new();
        // Unsupported-level MOS models only become an error when an
        // element uses them: decks routinely carry alternative model
        // cards for levels they never instantiate (mos6inv.cir).
        let mut mos_unsupported: HashMap<&str, String> = HashMap::new();
        for m in &nl.models {
            match m.mtype.as_str() {
                "d" => match diode_model(m, &mut notes) {
                    Ok(dm) => {
                        dmodels.insert(m.name.as_str(), dm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                },
                "npn" | "pnp" => match bjt_model(m, &mut notes) {
                    Ok(bm) => {
                        qmodels.insert(m.name.as_str(), bm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                },
                "nmos" | "pmos" => {
                    // `LEVEL` picks the MOS model; 1 is the default
                    // (`MOSdSetup` dispatch in `inpdomod.c`).
                    let level = num_param(&m.params, "level").unwrap_or(1.0);
                    if level == 1.0 {
                        match mos1_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::M1(mm));
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    } else if level == 2.0 {
                        match mos2_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::M2(mm));
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    } else if level == 4.0 {
                        match bsim1_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::B1(Box::new(mm)));
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    } else if level == 6.0 {
                        match mos6_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::M6(mm));
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    } else {
                        mos_unsupported.insert(
                            m.name.as_str(),
                            format!(
                                "model `{}`: MOS level {level} is not implemented yet (Phase 3)",
                                m.name
                            ),
                        );
                    }
                }
                _ => {}
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
                ir::Device::Bjt {
                    model,
                    area,
                    off,
                    params,
                } => {
                    let Some(bm) = qmodels.get(model.as_str()) else {
                        problems.push(format!(
                            "`{name}`: unable to find definition of model `{model}`"
                        ));
                        continue;
                    };
                    let mut temp = None;
                    let mut ic = (0.0, 0.0);
                    for p in params {
                        match (p.name.as_str(), &p.value) {
                            ("temp", ParamValue::Num(v)) => temp = Some(v + CTOK),
                            ("ic", ParamValue::Num(v)) => ic = (*v, 0.0),
                            ("ic", ParamValue::NumList(v)) => {
                                ic = (
                                    v.first().copied().unwrap_or(0.0),
                                    v.get(1).copied().unwrap_or(0.0),
                                );
                            }
                            ("area", ParamValue::Num(_)) => {}
                            _ => problems.push(format!("`{name}`: unknown parameter ({})", p.name)),
                        }
                    }
                    let mut nodes4 = [
                        el.nodes[0].clone(),
                        el.nodes[1].clone(),
                        el.nodes[2].clone(),
                        "0".to_string(),
                    ];
                    if let Some(s) = el.nodes.get(3) {
                        nodes4[3] = s.clone();
                    }
                    add(
                        &mut ckt,
                        Bjt {
                            name,
                            nodes: nodes4,
                            model: bm.clone(),
                            area: area.unwrap_or(1.0),
                            off: *off,
                            ic,
                            temp,
                        },
                        &mut problems,
                    );
                }
                ir::Device::Mosfet { model, params } => {
                    let Some(mm) = mmodels.get(model.as_str()) else {
                        if let Some(reason) = mos_unsupported.get(model.as_str()) {
                            problems.push(format!("`{name}`: {reason}"));
                        } else {
                            problems.push(format!(
                                "`{name}`: unable to find definition of model `{model}`"
                            ));
                        }
                        continue;
                    };
                    let mut inst = MosInst::new();
                    for p in params {
                        match (p.name.as_str(), &p.value) {
                            ("l", ParamValue::Num(v)) => inst.l = Some(*v),
                            ("w", ParamValue::Num(v)) => inst.w = Some(*v),
                            ("ad", ParamValue::Num(v)) => inst.ad = Some(*v),
                            ("as", ParamValue::Num(v)) => inst.as_ = Some(*v),
                            ("pd", ParamValue::Num(v)) => inst.pd = *v,
                            ("ps", ParamValue::Num(v)) => inst.ps = *v,
                            ("nrd", ParamValue::Num(v)) => inst.nrd = *v,
                            ("nrs", ParamValue::Num(v)) => inst.nrs = *v,
                            ("off", ParamValue::Flag) => inst.off = true,
                            ("temp", ParamValue::Num(v)) => inst.temp = Some(v + CTOK),
                            ("ic", ParamValue::Num(v)) => inst.ic = (*v, 0.0, 0.0),
                            ("ic", ParamValue::NumList(v)) => {
                                inst.ic = (
                                    v.first().copied().unwrap_or(0.0),
                                    v.get(1).copied().unwrap_or(0.0),
                                    v.get(2).copied().unwrap_or(0.0),
                                );
                            }
                            _ => problems.push(format!("`{name}`: unknown parameter ({})", p.name)),
                        }
                    }
                    let nodes = four(&el.nodes);
                    match mm {
                        MosModel::M1(m1) => add(
                            &mut ckt,
                            Mos1 {
                                name,
                                nodes,
                                model: m1.clone(),
                                l: inst.l,
                                w: inst.w,
                                ad: inst.ad,
                                as_: inst.as_,
                                pd: inst.pd,
                                ps: inst.ps,
                                nrd: inst.nrd,
                                nrs: inst.nrs,
                                off: inst.off,
                                ic: inst.ic,
                                temp: inst.temp,
                            },
                            &mut problems,
                        ),
                        MosModel::M2(m2) => add(
                            &mut ckt,
                            Mos2 {
                                name,
                                nodes,
                                model: m2.clone(),
                                l: inst.l,
                                w: inst.w,
                                ad: inst.ad,
                                as_: inst.as_,
                                pd: inst.pd,
                                ps: inst.ps,
                                nrd: inst.nrd,
                                nrs: inst.nrs,
                                off: inst.off,
                                ic: inst.ic,
                                temp: inst.temp,
                            },
                            &mut problems,
                        ),
                        MosModel::M6(m6) => add(
                            &mut ckt,
                            Mos6 {
                                name,
                                nodes,
                                model: m6.clone(),
                                l: inst.l,
                                w: inst.w,
                                ad: inst.ad,
                                as_: inst.as_,
                                pd: inst.pd,
                                ps: inst.ps,
                                nrd: inst.nrd,
                                nrs: inst.nrs,
                                off: inst.off,
                                ic: inst.ic,
                                temp: inst.temp,
                            },
                            &mut problems,
                        ),
                        MosModel::B1(b1) => add(
                            &mut ckt,
                            Bsim1 {
                                name,
                                nodes,
                                model: (**b1).clone(),
                                l: inst.l,
                                w: inst.w,
                                ad: inst.ad,
                                as_: inst.as_,
                                pd: inst.pd,
                                ps: inst.ps,
                                nrd: inst.nrd,
                                nrs: inst.nrs,
                                off: inst.off,
                                ic: inst.ic,
                            },
                            &mut problems,
                        ),
                    }
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

/// `.model x npn/pnp(...)` → [`crate::bjt::BjtModel`], 3f5 names
/// with the documented aliases (`bjt.c`).
fn bjt_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::bjt::BjtModel, String> {
    let mut bm = crate::bjt::BjtModel::new(m.mtype == "npn");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            // The npn/pnp type may also appear as a flag parameter.
            ParamValue::Flag if p.name == "npn" || p.name == "pnp" => {
                bm.npn = p.name == "npn";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "is" => bm.is = v,
            "bf" => bm.bf = v,
            "nf" => bm.nf = v,
            "vaf" | "va" => bm.vaf = v,
            "ikf" | "ik" => bm.ikf = v,
            "ise" => bm.ise = v,
            "ne" => bm.ne = v,
            "br" => bm.br = v,
            "nr" => bm.nr = v,
            "var" | "vb" => bm.var = v,
            "ikr" => bm.ikr = v,
            "isc" => bm.isc = v,
            "nc" => bm.nc = v,
            "rb" => bm.rb = v,
            "irb" => bm.irb = v,
            "rbm" => bm.rbm = Some(v),
            "re" => bm.re = v,
            "rc" => bm.rc = v,
            "cje" => bm.cje = v,
            "vje" | "pe" => bm.vje = v,
            "mje" | "me" => bm.mje = v,
            "tf" => bm.tf = v,
            "xtf" => bm.xtf = v,
            "vtf" => bm.vtf = v,
            "itf" => bm.itf = v,
            "ptf" => bm.ptf = v,
            "cjc" => bm.cjc = v,
            "vjc" | "pc" => bm.vjc = v,
            "mjc" | "mc" => bm.mjc = v,
            "xcjc" => bm.xcjc = v,
            "tr" => bm.tr = v,
            "cjs" | "ccs" => bm.cjs = v,
            "vjs" | "ps" => bm.vjs = v,
            "mjs" | "ms" => bm.mjs = v,
            "xtb" => bm.xtb = v,
            "eg" => bm.eg = v,
            "xti" => bm.xti = v,
            "fc" => bm.fc = Some(v),
            "kf" => bm.kf = v,
            "af" => bm.af = v,
            "tnom" => bm.tnom = Some(v + CTOK),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(bm)
}

/// Map `.model x nmos/pmos(level=1 …)` parameters onto
/// [`crate::mos1::Mos1Model`], 3f5 names with the documented aliases
/// (`mos1.c`).
fn mos1_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::mos1::Mos1Model, String> {
    let mut mm = crate::mos1::Mos1Model::new(m.mtype == "nmos");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            // The nmos/pmos type may also appear as a flag parameter.
            ParamValue::Flag if p.name == "nmos" || p.name == "pmos" => {
                mm.nmos = p.name == "nmos";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {} // dispatched by the caller
            "vto" | "vt0" => mm.vto = Some(v),
            "kp" => mm.kp = Some(v),
            "gamma" => mm.gamma = Some(v),
            "phi" => mm.phi = Some(v),
            "lambda" => mm.lambda = v,
            "rd" => mm.rd = Some(v),
            "rs" => mm.rs = Some(v),
            "cbd" => mm.cbd = Some(v),
            "cbs" => mm.cbs = Some(v),
            "is" => mm.is = v,
            "pb" => mm.pb = v,
            "cgso" => mm.cgso = v,
            "cgdo" => mm.cgdo = v,
            "cgbo" => mm.cgbo = v,
            "rsh" => mm.rsh = Some(v),
            "cj" => mm.cj = Some(v),
            "mj" => mm.mj = v,
            "cjsw" => mm.cjsw = Some(v),
            "mjsw" => mm.mjsw = v,
            "js" => mm.js = v,
            "tox" => mm.tox = Some(v),
            "ld" => mm.ld = v,
            "u0" | "uo" => mm.u0 = Some(v),
            "fc" => mm.fc = v,
            "nsub" => mm.nsub = Some(v),
            "tpg" => mm.tpg = Some(v),
            "nss" => mm.nss = Some(v),
            "kf" => mm.kf = v,
            "af" => mm.af = v,
            "tnom" => mm.tnom = Some(v + CTOK),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(mm)
}

/// MOSFET instance parameters shared by every level
/// (`l`/`w`/`ad`/`as` keep given-ness for the `.options` defaults;
/// `nrd`/`nrs` default to 1).
#[derive(Default)]
struct MosInst {
    l: Option<f64>,
    w: Option<f64>,
    ad: Option<f64>,
    as_: Option<f64>,
    pd: f64,
    ps: f64,
    nrd: f64,
    nrs: f64,
    off: bool,
    ic: (f64, f64, f64),
    temp: Option<f64>,
}

impl MosInst {
    fn new() -> Self {
        MosInst {
            nrd: 1.0,
            nrs: 1.0,
            ..Default::default()
        }
    }
}

/// Map `.model x nmos/pmos(level=2 …)` parameters onto
/// [`crate::mos2::Mos2Model`] (`mos2.c`).
fn mos2_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::mos2::Mos2Model, String> {
    let mut mm = crate::mos2::Mos2Model::new(m.mtype == "nmos");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "nmos" || p.name == "pmos" => {
                mm.nmos = p.name == "nmos";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {}
            "vto" | "vt0" => mm.vto = Some(v),
            "kp" => mm.kp = Some(v),
            "gamma" => mm.gamma = Some(v),
            "phi" => mm.phi = Some(v),
            "lambda" => mm.lambda = v,
            "rd" => mm.rd = Some(v),
            "rs" => mm.rs = Some(v),
            "cbd" => mm.cbd = Some(v),
            "cbs" => mm.cbs = Some(v),
            "is" => mm.is = v,
            "pb" => mm.pb = v,
            "cgso" => mm.cgso = v,
            "cgdo" => mm.cgdo = v,
            "cgbo" => mm.cgbo = v,
            "rsh" => mm.rsh = Some(v),
            "cj" => mm.cj = Some(v),
            "mj" => mm.mj = v,
            "cjsw" => mm.cjsw = Some(v),
            "mjsw" => mm.mjsw = v,
            "js" => mm.js = v,
            "tox" => mm.tox = Some(v),
            "ld" => mm.ld = v,
            "u0" | "uo" => mm.u0 = Some(v),
            "fc" => mm.fc = v,
            "nsub" => mm.nsub = Some(v),
            "tpg" => mm.tpg = Some(v),
            "nss" => mm.nss = Some(v),
            "delta" => mm.delta = v,
            "uexp" => mm.uexp = v,
            "ucrit" => mm.ucrit = v,
            "vmax" => mm.vmax = v,
            "xj" => mm.xj = v,
            "neff" => mm.neff = v,
            "nfs" => mm.nfs = v,
            "kf" => mm.kf = v,
            "af" => mm.af = v,
            "tnom" => mm.tnom = Some(v + CTOK),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(mm)
}

/// Map `.model x nmos/pmos(level=6 …)` parameters onto
/// [`crate::mos6::Mos6Model`] (`mos6.c`).
fn mos6_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::mos6::Mos6Model, String> {
    let mut mm = crate::mos6::Mos6Model::new(m.mtype == "nmos");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "nmos" || p.name == "pmos" => {
                mm.nmos = p.name == "nmos";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {}
            "vto" | "vt0" => mm.vto = Some(v),
            "kv" => mm.kv = v,
            "nv" => mm.nv = v,
            "kc" => mm.kc = Some(v),
            "nc" => mm.nc = v,
            "nvth" => mm.nvth = v,
            "ps" => mm.ps = v,
            "gamma" => mm.gamma = Some(v),
            "gamma1" => mm.gamma1 = v,
            "sigma" => mm.sigma = v,
            "phi" => mm.phi = Some(v),
            "lambda" => mm.lambda = Some(v),
            "lambda0" => mm.lambda0 = Some(v),
            "lambda1" => mm.lambda1 = v,
            "rd" => mm.rd = Some(v),
            "rs" => mm.rs = Some(v),
            "cbd" => mm.cbd = Some(v),
            "cbs" => mm.cbs = Some(v),
            "is" => mm.is = v,
            "pb" => mm.pb = v,
            "cgso" => mm.cgso = v,
            "cgdo" => mm.cgdo = v,
            "cgbo" => mm.cgbo = v,
            "rsh" => mm.rsh = Some(v),
            "cj" => mm.cj = Some(v),
            "mj" => mm.mj = v,
            "cjsw" => mm.cjsw = Some(v),
            "mjsw" => mm.mjsw = v,
            "js" => mm.js = v,
            "tox" => mm.tox = Some(v),
            "ld" => mm.ld = v,
            "u0" | "uo" => mm.u0 = Some(v),
            "fc" => mm.fc = v,
            "nsub" => mm.nsub = Some(v),
            "tpg" => mm.tpg = Some(v),
            "nss" => mm.nss = Some(v),
            "kf" => mm.kf = v,
            "af" => mm.af = v,
            "tnom" => mm.tnom = Some(v + CTOK),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(mm)
}

/// Map `.model x nmos/pmos(level=4 …)` parameters onto
/// [`crate::bsim1::Bsim1Model`] (`b1.c` keyword table).
#[allow(clippy::too_many_lines)]
fn bsim1_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::bsim1::Bsim1Model, String> {
    let mut b = crate::bsim1::Bsim1Model::new(m.mtype == "nmos");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "nmos" || p.name == "pmos" => {
                b.nmos = p.name == "nmos";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {}
            "vfb" => b.vfb0 = v,
            "lvfb" => b.vfbl = v,
            "wvfb" => b.vfbw = v,
            "phi" => b.phi0 = v,
            "lphi" => b.phil = v,
            "wphi" => b.phiw = v,
            "k1" => b.k10 = v,
            "lk1" => b.k1l = v,
            "wk1" => b.k1w = v,
            "k2" => b.k20 = v,
            "lk2" => b.k2l = v,
            "wk2" => b.k2w = v,
            "eta" => b.eta0 = v,
            "leta" => b.etal = v,
            "weta" => b.etaw = v,
            "x2e" => b.etab0 = v,
            "lx2e" => b.etabl = v,
            "wx2e" => b.etabw = v,
            "x3e" => b.etad0 = v,
            "lx3e" => b.etadl = v,
            "wx3e" => b.etadw = v,
            "dl" => b.deltal = v,
            "dw" => b.deltaw = v,
            "muz" => b.mobzero = v,
            "x2mz" => b.mobzerob0 = v,
            "lx2mz" => b.mobzerobl = v,
            "wx2mz" => b.mobzerobw = v,
            "mus" => b.mobvdd0 = v,
            "lmus" => b.mobvddl = v,
            "wmus" => b.mobvddw = v,
            "x2ms" => b.mobvddb0 = v,
            "lx2ms" => b.mobvddbl = v,
            "wx2ms" => b.mobvddbw = v,
            "x3ms" => b.mobvddd0 = v,
            "lx3ms" => b.mobvdddl = v,
            "wx3ms" => b.mobvdddw = v,
            "u0" => b.ugs0 = v,
            "lu0" => b.ugsl = v,
            "wu0" => b.ugsw = v,
            "x2u0" => b.ugsb0 = v,
            "lx2u0" => b.ugsbl = v,
            "wx2u0" => b.ugsbw = v,
            "u1" => b.uds0 = v,
            "lu1" => b.udsl = v,
            "wu1" => b.udsw = v,
            "x2u1" => b.udsb0 = v,
            "lx2u1" => b.udsbl = v,
            "wx2u1" => b.udsbw = v,
            "x3u1" => b.udsd0 = v,
            "lx3u1" => b.udsdl = v,
            "wx3u1" => b.udsdw = v,
            "n0" => b.n00 = v,
            "ln0" => b.n0l = v,
            "wn0" => b.n0w = v,
            "nb" => b.nb0 = v,
            "lnb" => b.nbl = v,
            "wnb" => b.nbw = v,
            "nd" => b.nd0 = v,
            "lnd" => b.ndl = v,
            "wnd" => b.ndw = v,
            "tox" => b.tox = v,
            "temp" => b.temp = v,
            "vdd" => b.vdd = v,
            "cgso" => b.cgso = v,
            "cgdo" => b.cgdo = v,
            "cgbo" => b.cgbo = v,
            "xpart" => b.xpart = v,
            "rsh" => b.rsh = v,
            "js" => b.js = v,
            "pb" => b.pb = v,
            "mj" => b.mj = v,
            "pbsw" => b.pbsw = v,
            "mjsw" => b.mjsw = v,
            "cj" => b.cj = v,
            "cjsw" => b.cjsw = v,
            "wdf" => b.wdf = v,
            "dell" => b.dell = v,
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(b)
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
fn diode_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<DiodeModel, String> {
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
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(dm)
}
