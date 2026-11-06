//! IR → engine elaboration: `Circuit::from_netlist` (D-0008). The
//! netlist crate guarantees shape (nodes present, numbers parsed);
//! this pass resolves models, applies defaults and rejects what the
//! engine cannot simulate yet.

use std::collections::HashMap;

use dune_netlist::ir::{self, Netlist, ParamValue};

use crate::circuit::{
    Bjt, Bsim1, Bsim2, Bsim3, Capacitor, Cccs, Ccvs, Circuit, Diode, Inductor, Isource, Jfet, Ltra,
    Mes, Mos1, Mos2, Mos3, Mos6, Mutual, Resistor, SwitchI, SwitchV, Tra, Vccs, Vcvs, Vsource,
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
            M3(crate::mos3::Mos3Model),
            M6(crate::mos6::Mos6Model),
            B1(Box<crate::bsim1::Bsim1Model>),
            B2(Box<crate::bsim2::Bsim2Model>),
            B3(Box<crate::bsim3::Bsim3Model>),
        }
        let mut mmodels: HashMap<&str, MosModel> = HashMap::new();
        // Unsupported-level MOS models only become an error when an
        // element uses them: decks routinely carry alternative model
        // cards for levels they never instantiate (mos6inv.cir).
        let mut unsupported_models: HashMap<&str, String> = HashMap::new();
        let mut omodels: HashMap<&str, crate::ltra::LtraModel> = HashMap::new();
        let mut jmodels: HashMap<&str, crate::jfet::JfetModel> = HashMap::new();
        let mut zmodels: HashMap<&str, crate::mes::MesModel> = HashMap::new();
        let mut smodels: HashMap<&str, crate::switch::SwModel> = HashMap::new();
        let mut wmodels: HashMap<&str, crate::switch::CswModel> = HashMap::new();
        for m in &nl.models {
            match m.mtype.as_str() {
                "sw" => match sw_model(m, &mut notes) {
                    Ok(sm) => {
                        smodels.insert(m.name.as_str(), sm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                },
                "csw" => match csw_model(m, &mut notes) {
                    Ok(wm) => {
                        wmodels.insert(m.name.as_str(), wm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                },
                // `LEVEL` on J/Z cards is ngspice's selector (JFET
                // 2 = Parker-Skellern; MES 2 = MESA, 5/6 = HFET);
                // only level 1, the 3f5 model, is implemented, and
                // like the MOS levels an unsupported card only errs
                // when an element instantiates it.
                "njf" | "pjf" => {
                    let level = num_param(&m.params, "level").unwrap_or(1.0);
                    if level != 1.0 {
                        unsupported_models.insert(
                            m.name.as_str(),
                            format!("model `{}`: JFET level {level} is not implemented", m.name),
                        );
                    } else {
                        match jfet_model(m, &mut notes) {
                            Ok(jm) => {
                                jmodels.insert(m.name.as_str(), jm);
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    }
                }
                "nmf" | "pmf" => {
                    let level = num_param(&m.params, "level").unwrap_or(1.0);
                    if level != 1.0 {
                        unsupported_models.insert(
                            m.name.as_str(),
                            format!(
                                "model `{}`: MESFET level {level} is not implemented",
                                m.name
                            ),
                        );
                    } else {
                        match mes_model(m, &mut notes) {
                            Ok(zm) => {
                                zmodels.insert(m.name.as_str(), zm);
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    }
                }
                "ltra" => match ltra_model(m, &mut notes) {
                    Ok(lm) => {
                        omodels.insert(m.name.as_str(), lm);
                    }
                    Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                },
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
                    } else if level == 3.0 {
                        match mos3_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::M3(mm));
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
                    } else if level == 5.0 {
                        match bsim2_model(m, &mut notes) {
                            Ok(mm) => {
                                mmodels.insert(m.name.as_str(), MosModel::B2(Box::new(mm)));
                            }
                            Err(e) => problems.push(format!("model `{}`: {e}", m.name)),
                        }
                    } else if level == 8.0 || level == 49.0 {
                        // ngspice's `inpdomod.c`: `VERSION` picks the
                        // BSIM3 code base; 3.3 (the default) is the
                        // one implemented.
                        match bsim3_model(m, &mut notes) {
                            Ok(Ok(mm)) => {
                                mmodels.insert(m.name.as_str(), MosModel::B3(Box::new(mm)));
                            }
                            Ok(Err(reason)) => {
                                unsupported_models.insert(
                                    m.name.as_str(),
                                    format!("model `{}`: {reason}", m.name),
                                );
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
                        unsupported_models.insert(
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
                    kind,
                    dc,
                    tran,
                    ac,
                    distof1,
                    distof2,
                    port,
                } => {
                    let ac = ac.as_ref().map(|a| (a.mag, a.phase));
                    let distof1 = distof1.as_ref().map(|d| (d.mag, d.phase));
                    let distof2 = distof2.as_ref().map(|d| (d.mag, d.phase));
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
                                distof1,
                                distof2,
                                port: port.map(|p| crate::circuit::RfPort {
                                    num: p.num,
                                    z0: p.z0,
                                }),
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
                                distof1,
                                distof2,
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
                        if let Some(reason) = unsupported_models.get(model.as_str()) {
                            problems.push(format!("`{name}`: {reason}"));
                        } else {
                            problems.push(format!(
                                "`{name}`: unable to find definition of model `{model}`"
                            ));
                        }
                        continue;
                    };
                    let mut inst = MosInst::new();
                    let is_b3 = matches!(mm, MosModel::B3(_));
                    for p in params {
                        if is_b3 {
                            // The BSIM3 instance table (`b3.c`) adds
                            // these and has no `TEMP`.
                            match (p.name.as_str(), &p.value) {
                                ("m", ParamValue::Num(v)) => {
                                    inst.m = *v;
                                    continue;
                                }
                                ("delvto", ParamValue::Num(v)) => {
                                    inst.delvto = *v;
                                    continue;
                                }
                                ("mulu0", ParamValue::Num(v)) => {
                                    inst.mulu0 = *v;
                                    continue;
                                }
                                ("icvds", ParamValue::Num(v)) => {
                                    inst.ic.0 = *v;
                                    continue;
                                }
                                ("icvgs", ParamValue::Num(v)) => {
                                    inst.ic.1 = *v;
                                    continue;
                                }
                                ("icvbs", ParamValue::Num(v)) => {
                                    inst.ic.2 = *v;
                                    continue;
                                }
                                ("nqsmod" | "acnqsmod", ParamValue::Num(v)) => {
                                    if *v != 0.0 {
                                        problems.push(format!(
                                            "`{name}`: BSIM3 {} = {v} (the non-quasi-static model) is not implemented",
                                            p.name
                                        ));
                                    }
                                    continue;
                                }
                                // `GEO` only matters with ACM > 0.
                                ("geo", ParamValue::Num(_)) => continue,
                                ("temp", _) => {
                                    problems.push(format!("`{name}`: unknown parameter (temp)"));
                                    continue;
                                }
                                _ => {}
                            }
                        }
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
                        MosModel::M3(m3) => add(
                            &mut ckt,
                            Mos3 {
                                name,
                                nodes,
                                model: m3.clone(),
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
                        MosModel::B3(b3) => add(
                            &mut ckt,
                            Bsim3 {
                                name,
                                nodes,
                                model: (**b3).clone(),
                                l: inst.l,
                                w: inst.w,
                                ad: inst.ad.unwrap_or(0.0),
                                as_: inst.as_.unwrap_or(0.0),
                                pd: inst.pd,
                                ps: inst.ps,
                                nrd: inst.nrd,
                                nrs: inst.nrs,
                                m: inst.m,
                                delvto: inst.delvto,
                                mulu0: inst.mulu0,
                                off: inst.off,
                                ic: inst.ic,
                            },
                            &mut problems,
                        ),
                        MosModel::B2(b2) => add(
                            &mut ckt,
                            Bsim2 {
                                name,
                                nodes,
                                model: (**b2).clone(),
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
                ir::Device::Jfet {
                    model,
                    area,
                    off,
                    params,
                } => {
                    let Some(jm) = jmodels.get(model.as_str()) else {
                        if let Some(reason) = unsupported_models.get(model.as_str()) {
                            problems.push(format!("`{name}`: {reason}"));
                        } else {
                            problems.push(format!(
                                "`{name}`: unable to find definition of model `{model}`"
                            ));
                        }
                        continue;
                    };
                    let mut area = *area;
                    let mut temp = None;
                    let mut ic = (0.0, 0.0);
                    for p in params {
                        match (p.name.as_str(), &p.value) {
                            ("temp", ParamValue::Num(v)) => temp = Some(v + CTOK),
                            ("area", ParamValue::Num(v)) => area = Some(*v),
                            ("ic", ParamValue::Num(v)) => ic.0 = *v,
                            ("ic", ParamValue::NumList(v)) if (1..=2).contains(&v.len()) => {
                                ic = (v[0], v.get(1).copied().unwrap_or(ic.1));
                            }
                            _ => problems.push(format!("`{name}`: unknown parameter ({})", p.name)),
                        }
                    }
                    add(
                        &mut ckt,
                        Jfet {
                            name,
                            nodes: three(&el.nodes),
                            model: jm.clone(),
                            area: area.unwrap_or(1.0),
                            off: *off,
                            ic,
                            temp,
                        },
                        &mut problems,
                    );
                }
                ir::Device::Mesfet {
                    model,
                    area,
                    off,
                    params,
                } => {
                    let Some(zm) = zmodels.get(model.as_str()) else {
                        if let Some(reason) = unsupported_models.get(model.as_str()) {
                            problems.push(format!("`{name}`: {reason}"));
                        } else {
                            problems.push(format!(
                                "`{name}`: unable to find definition of model `{model}`"
                            ));
                        }
                        continue;
                    };
                    let mut area = *area;
                    let mut ic = (0.0, 0.0);
                    for p in params {
                        match (p.name.as_str(), &p.value) {
                            ("area", ParamValue::Num(v)) => area = Some(*v),
                            ("icvds", ParamValue::Num(v)) => ic.0 = *v,
                            ("icvgs", ParamValue::Num(v)) => ic.1 = *v,
                            ("ic", ParamValue::Num(v)) => ic.0 = *v,
                            ("ic", ParamValue::NumList(v)) if (1..=2).contains(&v.len()) => {
                                ic = (v[0], v.get(1).copied().unwrap_or(ic.1));
                            }
                            _ => problems.push(format!("`{name}`: unknown parameter ({})", p.name)),
                        }
                    }
                    add(
                        &mut ckt,
                        Mes {
                            name,
                            nodes: three(&el.nodes),
                            model: zm.clone(),
                            area: area.unwrap_or(1.0),
                            off: *off,
                            ic,
                        },
                        &mut problems,
                    );
                }
                ir::Device::SwitchV { model, state } => {
                    let Some(sm) = smodels.get(model.as_str()) else {
                        problems.push(format!(
                            "`{name}`: unable to find definition of model `{model}`"
                        ));
                        continue;
                    };
                    add(
                        &mut ckt,
                        SwitchV {
                            name,
                            nodes: four(&el.nodes),
                            model: sm.clone(),
                            on: *state == Some(ir::OnOff::On),
                        },
                        &mut problems,
                    );
                }
                ir::Device::SwitchI {
                    vname,
                    model,
                    state,
                } => {
                    let Some(wm) = wmodels.get(model.as_str()) else {
                        problems.push(format!(
                            "`{name}`: unable to find definition of model `{model}`"
                        ));
                        continue;
                    };
                    add(
                        &mut ckt,
                        SwitchI {
                            name,
                            nodes: two(&el.nodes),
                            ctrl: vname.clone(),
                            model: wm.clone(),
                            on: *state == Some(ir::OnOff::On),
                        },
                        &mut problems,
                    );
                }
                ir::Device::TlineLossy { model } => {
                    let Some(lm) = omodels.get(model.as_str()) else {
                        problems.push(format!(
                            "`{name}`: unable to find definition of model `{model}`"
                        ));
                        continue;
                    };
                    add(
                        &mut ckt,
                        Ltra {
                            name,
                            nodes: four(&el.nodes),
                            model: lm.clone(),
                        },
                        &mut problems,
                    );
                }
                ir::Device::TlineIdeal { params } => match tra_spec(&name, &el.nodes, params) {
                    Ok(t) => add(&mut ckt, t, &mut problems),
                    Err(e) => problems.push(e),
                },
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

/// `T` card parameters (`TRApTable` in `dev/tra/tra.c`): `Z0`/`ZO`,
/// `TD`, `F`, `NL`, `V1`, `I1`, `V2`, `I2` and the `IC=` vector,
/// whose short forms fill from the front (`TRAparam`'s fall-through
/// switch). `REL`/`ABS` are output-only there, so not accepted.
fn tra_spec(name: &str, nodes: &[String], params: &[ir::Param]) -> Result<Tra, String> {
    let mut z0 = None;
    let mut t = Tra {
        name: name.to_string(),
        nodes: four(nodes),
        z0: 0.0,
        td: None,
        f: None,
        nl: None,
        ic: [0.0; 4],
    };
    for p in params {
        match (p.name.as_str(), &p.value) {
            ("z0" | "zo", ParamValue::Num(v)) => z0 = Some(*v),
            ("td", ParamValue::Num(v)) => t.td = Some(*v),
            ("f", ParamValue::Num(v)) => t.f = Some(*v),
            ("nl", ParamValue::Num(v)) => t.nl = Some(*v),
            ("v1", ParamValue::Num(v)) => t.ic[0] = *v,
            ("i1", ParamValue::Num(v)) => t.ic[1] = *v,
            ("v2", ParamValue::Num(v)) => t.ic[2] = *v,
            ("i2", ParamValue::Num(v)) => t.ic[3] = *v,
            ("ic", ParamValue::Num(v)) => t.ic[0] = *v,
            ("ic", ParamValue::NumList(v)) if (1..=4).contains(&v.len()) => {
                t.ic[..v.len()].copy_from_slice(v);
            }
            _ => return Err(format!("`{name}`: unknown parameter ({})", p.name)),
        }
    }
    // `TRAsetup`: "transmission line z0 must be given".
    t.z0 = z0.ok_or_else(|| format!("`{name}`: transmission line z0 must be given"))?;
    Ok(t)
}

/// `.model x ltra(...)` → [`crate::ltra::LtraModel`]: the ngspice
/// input table (`ltra.c`), which is the 3f5's plus `REL`/`ABS` as
/// inputs. Flags take no value (a value given anyway is ignored, as
/// `LTRAmParam` ignores it); `fullcontrol`, `halfcontrol`, `print`,
/// `noprint`, `choprel`, … are `#ifdef notdef` in the table and so
/// unrecognized.
fn ltra_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::ltra::LtraModel, String> {
    use crate::ltra::LtraInterp;
    let mut lm = crate::ltra::LtraModel::default();
    for p in &m.params {
        let num = match p.value {
            ParamValue::Num(v) => Some(v),
            _ => None,
        };
        let need = || num.ok_or_else(|| format!("parameter {} needs a numeric value", p.name));
        match p.name.as_str() {
            "ltra" => {}
            "r" => lm.r = Some(need()?),
            "l" => lm.l = Some(need()?),
            "g" => lm.g = Some(need()?),
            "c" => lm.c = Some(need()?),
            "len" => lm.len = Some(need()?),
            "rel" => lm.rel = Some(need()?),
            "abs" => lm.abs = Some(need()?),
            "compactrel" => lm.compactrel = need()?,
            "compactabs" => lm.compactabs = need()?,
            // `nocontrol` is the default (and only reachable) mode.
            "nocontrol" => {}
            "steplimit" => lm.steplimit = true,
            "nosteplimit" => lm.steplimit = false,
            "lininterp" => lm.interp = Some(LtraInterp::Linear),
            "quadinterp" => lm.interp = Some(LtraInterp::Quadratic),
            "mixedinterp" => lm.interp = Some(LtraInterp::Mixed),
            "truncnr" => lm.truncnr = true,
            "truncdontcut" => lm.truncdontcut = true,
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(lm)
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
    /// BSIM3 only: `M`, `DELVTO`, `MULU0`.
    m: f64,
    delvto: f64,
    mulu0: f64,
}

impl MosInst {
    fn new() -> Self {
        MosInst {
            nrd: 1.0,
            nrs: 1.0,
            m: 1.0,
            mulu0: 1.0,
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

/// Map `.model x nmos/pmos(level=3 …)` parameters onto
/// [`crate::mos3::Mos3Model`] (`mos3.c`). `XD`/`ALPHA` are output-
/// only in practice (`mos3mpar.c` has no case for them) and
/// `input_delta` is the C's internal alias; all three fall through
/// to the unrecognized-parameter note.
fn mos3_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::mos3::Mos3Model, String> {
    let mut mm = crate::mos3::Mos3Model::new(m.mtype == "nmos");
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
            "vmax" => mm.vmax = v,
            "xj" => mm.xj = v,
            "nfs" => mm.nfs = v,
            "eta" => mm.eta = v,
            "delta" => mm.delta = v,
            "theta" => mm.theta = v,
            "kappa" => mm.kappa = v,
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

/// Map `.model x nmos/pmos(level=8|49 …)` onto
/// [`crate::bsim3::Bsim3Model`] (`b3.c` keyword table via
/// [`crate::bsim3::Bsim3Model::set`]). The outer `Err` is a broken
/// card; `Ok(Err(reason))` a card for a BSIM3 variant that is not
/// implemented (another `VERSION`, the NQS model, `ACM > 0`, a
/// `CAPMOD` outside 0–3), which only errs when instantiated.
fn bsim3_model(
    m: &ir::Model,
    notes: &mut Vec<String>,
) -> Result<Result<crate::bsim3::Bsim3Model, String>, String> {
    let mut b = crate::bsim3::Bsim3Model::new(m.mtype == "nmos");
    for p in &m.params {
        if p.name == "version" {
            // A string in ngspice (`IF_STRING`); the lexer may read
            // it as a number or a number list (`3.3.0`).
            let v = match &p.value {
                ParamValue::Str(s) => s.clone(),
                ParamValue::Num(v) => format!("{v}"),
                ParamValue::NumList(v) => v
                    .iter()
                    .map(|x| format!("{x}"))
                    .collect::<Vec<_>>()
                    .join("."),
                ParamValue::Flag => return Err("parameter version needs a value".into()),
            };
            if !v.starts_with("3.3") {
                return Ok(Err(format!(
                    "BSIM3 version {v} is not implemented (only 3.3.x, ngspice's default)"
                )));
            }
            b.set_version(&v);
            continue;
        }
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "nmos" || p.name == "pmos" => continue,
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        if p.name == "level" {
            continue;
        }
        if let Err(e) = b.set(&p.name, v) {
            // `INPmakeMod`: unknown parameters are a warning.
            notes.push(format!("model `{}`: {e} (ignored)", m.name));
        }
    }
    let int = |k: &str| b.get_int(k);
    if int("nqsmod") == Some(1) || int("acnqsmod") == Some(1) {
        return Ok(Err(
            "the BSIM3 non-quasi-static model (NQSMOD/ACNQSMOD = 1) is not implemented".into(),
        ));
    }
    if int("acm").is_some_and(|a| a != 0) {
        return Ok(Err(
            "BSIM3 ACM > 0 junction geometry is not implemented".into()
        ));
    }
    if int("capmod").is_some_and(|c| !(0..=3).contains(&c)) {
        return Ok(Err("BSIM3 CAPMOD must be 0, 1, 2 or 3".into()));
    }
    Ok(Ok(b))
}

/// Map `.model x nmos/pmos(level=5 …)` parameters onto
/// [`crate::bsim2::Bsim2Model`] (`b2.c` keyword table; defaults
/// from `b2set.c` via [`crate::bsim2::Bsim2Model::new`]).
#[allow(clippy::too_many_lines)]
fn bsim2_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::bsim2::Bsim2Model, String> {
    let mut b = crate::bsim2::Bsim2Model::new(m.mtype == "nmos");
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
            "vfb" => b.vfb = v,
            "lvfb" => b.lvfb = v,
            "wvfb" => b.wvfb = v,
            "phi" => b.phi = v,
            "lphi" => b.lphi = v,
            "wphi" => b.wphi = v,
            "k1" => b.k1 = v,
            "lk1" => b.lk1 = v,
            "wk1" => b.wk1 = v,
            "k2" => b.k2 = v,
            "lk2" => b.lk2 = v,
            "wk2" => b.wk2 = v,
            "eta0" => b.eta0 = v,
            "leta0" => b.leta0 = v,
            "weta0" => b.weta0 = v,
            "etab" => b.etab = v,
            "letab" => b.letab = v,
            "wetab" => b.wetab = v,
            "dl" => b.dl = v,
            "dw" => b.dw = v,
            "mu0" => b.mu0 = v,
            "mu0b" => b.mu0b = v,
            "lmu0b" => b.lmu0b = v,
            "wmu0b" => b.wmu0b = v,
            "mus0" => b.mus0 = v,
            "lmus0" => b.lmus0 = v,
            "wmus0" => b.wmus0 = v,
            "musb" => b.musb = v,
            "lmusb" => b.lmusb = v,
            "wmusb" => b.wmusb = v,
            "mu20" => b.mu20 = v,
            "lmu20" => b.lmu20 = v,
            "wmu20" => b.wmu20 = v,
            "mu2b" => b.mu2b = v,
            "lmu2b" => b.lmu2b = v,
            "wmu2b" => b.wmu2b = v,
            "mu2g" => b.mu2g = v,
            "lmu2g" => b.lmu2g = v,
            "wmu2g" => b.wmu2g = v,
            "mu30" => b.mu30 = v,
            "lmu30" => b.lmu30 = v,
            "wmu30" => b.wmu30 = v,
            "mu3b" => b.mu3b = v,
            "lmu3b" => b.lmu3b = v,
            "wmu3b" => b.wmu3b = v,
            "mu3g" => b.mu3g = v,
            "lmu3g" => b.lmu3g = v,
            "wmu3g" => b.wmu3g = v,
            "mu40" => b.mu40 = v,
            "lmu40" => b.lmu40 = v,
            "wmu40" => b.wmu40 = v,
            "mu4b" => b.mu4b = v,
            "lmu4b" => b.lmu4b = v,
            "wmu4b" => b.wmu4b = v,
            "mu4g" => b.mu4g = v,
            "lmu4g" => b.lmu4g = v,
            "wmu4g" => b.wmu4g = v,
            "ua0" => b.ua0 = v,
            "lua0" => b.lua0 = v,
            "wua0" => b.wua0 = v,
            "uab" => b.uab = v,
            "luab" => b.luab = v,
            "wuab" => b.wuab = v,
            "ub0" => b.ub0 = v,
            "lub0" => b.lub0 = v,
            "wub0" => b.wub0 = v,
            "ubb" => b.ubb = v,
            "lubb" => b.lubb = v,
            "wubb" => b.wubb = v,
            "u10" => b.u10 = v,
            "lu10" => b.lu10 = v,
            "wu10" => b.wu10 = v,
            "u1b" => b.u1b = v,
            "lu1b" => b.lu1b = v,
            "wu1b" => b.wu1b = v,
            "u1d" => b.u1d = v,
            "lu1d" => b.lu1d = v,
            "wu1d" => b.wu1d = v,
            "n0" => b.n0 = v,
            "ln0" => b.ln0 = v,
            "wn0" => b.wn0 = v,
            "nb" => b.nb = v,
            "lnb" => b.lnb = v,
            "wnb" => b.wnb = v,
            "nd" => b.nd = v,
            "lnd" => b.lnd = v,
            "wnd" => b.wnd = v,
            "vof0" => b.vof0 = v,
            "lvof0" => b.lvof0 = v,
            "wvof0" => b.wvof0 = v,
            "vofb" => b.vofb = v,
            "lvofb" => b.lvofb = v,
            "wvofb" => b.wvofb = v,
            "vofd" => b.vofd = v,
            "lvofd" => b.lvofd = v,
            "wvofd" => b.wvofd = v,
            "ai0" => b.ai0 = v,
            "lai0" => b.lai0 = v,
            "wai0" => b.wai0 = v,
            "aib" => b.aib = v,
            "laib" => b.laib = v,
            "waib" => b.waib = v,
            "bi0" => b.bi0 = v,
            "lbi0" => b.lbi0 = v,
            "wbi0" => b.wbi0 = v,
            "bib" => b.bib = v,
            "lbib" => b.lbib = v,
            "wbib" => b.wbib = v,
            "vghigh" => b.vghigh = v,
            "lvghigh" => b.lvghigh = v,
            "wvghigh" => b.wvghigh = v,
            "vglow" => b.vglow = v,
            "lvglow" => b.lvglow = v,
            "wvglow" => b.wvglow = v,
            "tox" => b.tox = v,
            "temp" => b.temp = v,
            "vdd" => b.vdd = v,
            "vgg" => b.vgg = v,
            "vbb" => b.vbb = v,
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

fn three(nodes: &[String]) -> [String; 3] {
    [nodes[0].clone(), nodes[1].clone(), nodes[2].clone()]
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

/// `.model x njf/pjf(...)` → [`crate::jfet::JfetModel`] (`jfet.c`
/// model table: `VT0` with `VTO` as alias, the Sydney `B`). `LEVEL`
/// is ngspice's selector (2 = Parker–Skellern); only level 1, the
/// 3f5 model, is accepted.
fn jfet_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::jfet::JfetModel, String> {
    let mut jm = crate::jfet::JfetModel::new(m.mtype == "njf");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "njf" || p.name == "pjf" => {
                jm.njf = p.name == "njf";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {}
            "vt0" | "vto" => jm.vto = v,
            "beta" => jm.beta = v,
            "lambda" => jm.lambda = v,
            "rd" => jm.rd = v,
            "rs" => jm.rs = v,
            "cgs" => jm.cgs = v,
            "cgd" => jm.cgd = v,
            "pb" => jm.pb = v,
            "is" => jm.is = v,
            "fc" => jm.fc = v,
            "b" => jm.b = v,
            "tnom" => jm.tnom = Some(v + CTOK),
            "kf" => jm.kf = v,
            "af" => jm.af = v,
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(jm)
}

/// `RON`/`ROFF` of a switch model: a zero resistance would make
/// the C's `1.0/value` conductance infinite and poison the matrix.
fn switch_resistance(name: &str, v: f64) -> Result<f64, String> {
    if v == 0.0 || !v.is_finite() {
        return Err(format!("{name} must be a nonzero finite resistance"));
    }
    Ok(v)
}

/// `.model x sw(...)` → [`crate::switch::SwModel`] (`sw.c` model
/// table; `GON`/`GOFF` are output-only there). `VH` keeps its sign as
/// in ngspice (`swmparam.c`); the 3f5 stored `|VH|`.
fn sw_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::switch::SwModel, String> {
    let mut sm = crate::switch::SwModel::default();
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "sw" => continue,
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "vt" => sm.vt = v,
            "vh" => sm.vh = v,
            "ron" => sm.ron = Some(switch_resistance("ron", v)?),
            "roff" => sm.roff = Some(switch_resistance("roff", v)?),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(sm)
}

/// `.model x csw(...)` → [`crate::switch::CswModel`] (`csw.c` model
/// table). `IH` keeps its sign as in ngspice (`cswmpar.c`); the 3f5
/// stored `|IH|`.
fn csw_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::switch::CswModel, String> {
    let mut wm = crate::switch::CswModel::default();
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "csw" => continue,
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "it" => wm.it = v,
            "ih" => wm.ih = v,
            "ron" => wm.ron = Some(switch_resistance("ron", v)?),
            "roff" => wm.roff = Some(switch_resistance("roff", v)?),
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(wm)
}

/// `.model x nmf/pmf(...)` → [`crate::mes::MesModel`] (`mes.c` model
/// table). `LEVEL` is ngspice's selector (1 = Statz, the 3f5 model;
/// 2 MESA, 5/6 HFET); only level 1 is accepted.
fn mes_model(m: &ir::Model, notes: &mut Vec<String>) -> Result<crate::mes::MesModel, String> {
    let mut zm = crate::mes::MesModel::new(m.mtype == "nmf");
    for p in &m.params {
        let v = match p.value {
            ParamValue::Num(v) => v,
            ParamValue::Flag if p.name == "nmf" || p.name == "pmf" => {
                zm.nmf = p.name == "nmf";
                continue;
            }
            _ => return Err(format!("parameter {} needs a numeric value", p.name)),
        };
        match p.name.as_str() {
            "level" => {}
            "vt0" | "vto" => zm.vto = v,
            "alpha" => zm.alpha = v,
            "beta" => zm.beta = v,
            "lambda" => zm.lambda = v,
            "b" => zm.b = v,
            "rd" => zm.rd = v,
            "rs" => zm.rs = v,
            "cgs" => zm.cgs = v,
            "cgd" => zm.cgd = v,
            "pb" => zm.pb = v,
            "is" => zm.is = v,
            "fc" => zm.fc = v,
            "kf" => zm.kf = v,
            "af" => zm.af = v,
            other => notes.push(format!(
                "model `{}`: unrecognized parameter ({other}) - ignored",
                m.name
            )),
        }
    }
    Ok(zm)
}
