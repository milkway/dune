//! Element card parsing, one branch per 3f5 device letter
//! (`corpus/spice3f5/src/lib/inp/inp2*.c`; general forms in
//! `spice.txt` §3). Keyword parameters are collected generically the
//! way `INPdevParse` + `INPgetValue` do: an optional leading number,
//! then keywords each taking all the consecutive numbers that follow
//! (none → flag, one → value, several → vector). Validation against
//! each device's true parameter table happens in `dune-core` when the
//! device exists; here the card only has to be shaped right.

use std::collections::HashSet;

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::ir::{
    AcSpec, Device, DistoSpec, Element, Expr, OnOff, Param, ParamValue, RfPort, SourceKind, TranFn,
    TranFnKind,
};
use crate::param::ParamEnv;
use crate::scan::Scanner;
use crate::span::Span;

pub(crate) struct ElemCtx<'a> {
    pub sc: Scanner<'a>,
    pub card: &'a Card,
    pub diags: &'a mut Vec<Diagnostic>,
    pub env: &'a ParamEnv,
}

impl ElemCtx<'_> {
    pub(crate) fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(msg.into(), span));
    }

    pub(crate) fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::warning(msg.into(), span));
    }

    /// Next token, or an error naming what was missing.
    pub(crate) fn tok_req(&mut self, what: &str) -> Option<(String, Span)> {
        match self.sc.next_tok() {
            Some(t) => {
                let span = self.sc.span(&t);
                Some((t.lc(), span))
            }
            None => {
                let span = self.sc.here();
                self.err(span, format!("missing {what}"));
                None
            }
        }
    }

    pub(crate) fn node_req(&mut self, what: &str) -> Option<String> {
        self.tok_req(what).map(|(t, _)| t)
    }

    /// A number if one starts here: a plain SPICE number or a basic
    /// `{expr}` parameter expression. Consumes nothing otherwise.
    pub(crate) fn value_opt(&mut self) -> Option<f64> {
        if let Some((v, _)) = self.sc.try_value() {
            return Some(v);
        }
        let tok = self.sc.peek_tok()?;
        if !tok.text.starts_with('{') {
            return None;
        }
        let span = self.sc.span(&tok);
        let text = tok.lc();
        self.sc.next_tok();
        match self.env.eval_use(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                self.err(span, e);
                Some(0.0)
            }
        }
    }

    pub(crate) fn value_req(&mut self, what: &str) -> Option<f64> {
        match self.value_opt() {
            Some(v) => Some(v),
            None => {
                let span = self
                    .sc
                    .peek_tok()
                    .map(|t| self.sc.span(&t))
                    .unwrap_or_else(|| self.sc.here());
                self.err(span, format!("missing {what}"));
                None
            }
        }
    }

    /// `INPdevParse`: optional leading number, then keyword
    /// parameters until the card ends.
    pub(crate) fn tail_params(&mut self) -> (Option<f64>, Vec<Param>) {
        let leading = self.value_opt();
        (leading, self.keyword_params())
    }

    pub(crate) fn keyword_params(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        while let Some(tok) = self.sc.next_tok() {
            let span = self.sc.span(&tok);
            let name = tok.lc();
            let mut nums = Vec::new();
            while let Some(v) = self.value_opt() {
                nums.push(v);
            }
            let value = match nums.len() {
                0 => ParamValue::Flag,
                1 => ParamValue::Num(nums[0]),
                _ => ParamValue::NumList(nums),
            };
            params.push(Param { name, value, span });
        }
        params
    }

    /// Pull `area`/`off` out of a junction device's tail, keeping
    /// everything else (`ic=`, `temp=`) as params.
    pub(crate) fn junction_tail(&mut self) -> (Option<f64>, bool, Vec<Param>) {
        let (area, mut params) = self.tail_params();
        let mut off = false;
        params.retain(|p| {
            if p.name == "off" && p.value == ParamValue::Flag {
                off = true;
                false
            } else {
                true
            }
        });
        (area, off, params)
    }
}

/// Parse one element card. `model_names` is the deck-wide set of
/// `.model` names, needed to tell a BJT's optional substrate node
/// from its model name the same way `INP2Q` does; `vdmos_models` the
/// subset whose type is ngspice's VDMOS, whose `M` instances take 3
/// to 5 nodes (`INP2M`).
pub(crate) fn parse_element(
    card: &Card,
    diags: &mut Vec<Diagnostic>,
    model_names: &HashSet<String>,
    vdmos_models: &HashSet<String>,
    env: &ParamEnv,
) -> Option<Element> {
    let mut ctx = ElemCtx {
        sc: Scanner::new(card),
        card,
        diags,
        env,
    };
    let name_tok = ctx.sc.next_tok()?;
    let span = ctx.sc.span(&name_tok);
    let name = name_tok.lc();
    let letter = name.chars().next().unwrap_or('\0');

    let (nodes, device) = match letter {
        'r' => two_terminal_rc(&mut ctx, true)?,
        'c' => two_terminal_rc(&mut ctx, false)?,
        'l' => inductor(&mut ctx)?,
        'k' => mutual(&mut ctx)?,
        'v' => source(&mut ctx, SourceKind::Voltage)?,
        'i' => source(&mut ctx, SourceKind::Current)?,
        'e' => vc_source(&mut ctx, true)?,
        'g' => vc_source(&mut ctx, false)?,
        'f' => cc_source(&mut ctx, true)?,
        'h' => cc_source(&mut ctx, false)?,
        'd' => diode(&mut ctx)?,
        'q' => bjt(&mut ctx, model_names)?,
        'j' => fet3(&mut ctx, true)?,
        'z' => fet3(&mut ctx, false)?,
        'm' => mosfet(&mut ctx, model_names, vdmos_models)?,
        's' => switch_v(&mut ctx)?,
        'w' => switch_i(&mut ctx)?,
        't' => tline_ideal(&mut ctx)?,
        'o' => tline_lossy(&mut ctx)?,
        'u' => urc(&mut ctx)?,
        'b' => behavioral(&mut ctx)?,
        'x' => subckt_call(&mut ctx)?,
        _ => {
            ctx.err(span, format!("unknown device type `{name}`"));
            return None;
        }
    };
    let span = span.to(card.span_of(0, card.text.len()));
    Some(Element {
        name,
        nodes,
        device,
        span,
    })
}

type Parsed = Option<(Vec<String>, Device)>;

/// `R`/`C`: `name N1 N2 [VALUE] [MNAME] [keyword params]`. A number
/// after the nodes is the value; a non-number is a model name
/// (`INP2R`: "either not a number -> model").
fn two_terminal_rc(ctx: &mut ElemCtx, is_r: bool) -> Parsed {
    let n1 = ctx.node_req("first node")?;
    let n2 = ctx.node_req("second node")?;
    let value = ctx.value_opt();
    // A following bare name is the model, unless it is a keyword
    // parameter (keywords are followed by a number or end the card
    // only in the model position... the C simply takes the next
    // token as the model when one is present and it is not of the
    // form kw=val; since `=` vanished in lexing, mirror the C: a
    // token followed by a value is a parameter, otherwise it is the
    // model name — except the known flags.
    let mut model = None;
    if let Some(tok) = ctx.sc.peek_tok() {
        let lc = tok.lc();
        if !is_keyword_param(&lc) {
            ctx.sc.next_tok();
            model = Some(lc);
        }
    }
    let (lead, params) = ctx.tail_params();
    let value = value.or(lead);
    let device = if is_r {
        Device::Resistor {
            value,
            model,
            params,
        }
    } else {
        Device::Capacitor {
            value,
            model,
            params,
        }
    };
    Some((vec![n1, n2], device))
}

/// Keywords that may legally follow the optional model name of a
/// semiconductor R/C (`w l temp ic resistance capacitance`, and
/// ngspice-47's `dtemp`): a bare token matching one of these is a
/// parameter, not a model name.
fn is_keyword_param(tok: &str) -> bool {
    matches!(
        tok,
        "w" | "l" | "temp" | "dtemp" | "ic" | "resistance" | "capacitance" | "area" | "off"
    )
}

fn inductor(ctx: &mut ElemCtx) -> Parsed {
    let n1 = ctx.node_req("first node")?;
    let n2 = ctx.node_req("second node")?;
    let value = ctx.value_opt();
    let (lead, params) = ctx.tail_params();
    Some((
        vec![n1, n2],
        Device::Inductor {
            value: value.or(lead),
            params,
        },
    ))
}

/// `K`: `KXXXXXXX LYYYYYYY LZZZZZZZ VALUE`.
fn mutual(ctx: &mut ElemCtx) -> Parsed {
    let (l1, s1) = ctx.tok_req("first coupled inductor")?;
    let (l2, s2) = ctx.tok_req("second coupled inductor")?;
    for (l, s) in [(&l1, s1), (&l2, s2)] {
        if !l.starts_with('l') {
            ctx.err(s, format!("`{l}` is not an inductor name"));
        }
    }
    let k = ctx.value_req("coupling coefficient")?;
    Some((Vec::new(), Device::Mutual { l1, l2, k }))
}

/// `V`/`I`: `name N+ N- <<DC> VALUE> <AC <MAG <PHASE>>> <tranfn(…)>
/// <DISTOF1 <MAG <PHASE>>> <DISTOF2 <MAG <PHASE>>>`.
fn source(ctx: &mut ElemCtx, kind: SourceKind) -> Parsed {
    let n1 = ctx.node_req("positive node")?;
    let n2 = ctx.node_req("negative node")?;
    let mut dc = ctx.value_opt(); // bare leading number is the DC value
    let mut ac = None;
    let mut tran: Option<TranFn> = None;
    let mut distof1 = None;
    let mut distof2 = None;
    // ngspice RF port state, set in card order as `VSRCparam` does
    // (`vsrcpar.c`): `portnum` resets a non-positive Z0 to 50, so
    // `z0=-1 portnum 1` is a 50 Ω port but `portnum 1 z0=-1` is
    // not a port at all.
    let mut port_num: Option<i64> = None;
    let mut port_z0 = 0.0_f64;
    let mut port_z0_given = false;

    while let Some(tok) = ctx.sc.next_tok() {
        let span = ctx.sc.span(&tok);
        match tok.lc().as_str() {
            "portnum" if kind == SourceKind::Voltage => {
                // IF_INTEGER: `(int) INPevaluate(…)`, truncation.
                let v = ctx.value_req("port number")?;
                port_num = Some(v.trunc().clamp(i32::MIN as f64, i32::MAX as f64) as i64);
                if port_z0 <= 0.0 {
                    port_z0 = 50.0;
                }
            }
            "z0" if kind == SourceKind::Voltage => {
                port_z0 = ctx.value_req("port impedance")?;
                port_z0_given = true;
            }
            p @ ("pwr" | "freq" | "phase") if kind == SourceKind::Voltage => {
                // These turn the port into ngspice's `PORT` sinusoid
                // (time domain and DC operating point), not ported.
                ctx.value_opt();
                ctx.err(
                    span,
                    format!("RF port power-source parameter `{p}` is not supported"),
                );
            }
            "dc" => dc = Some(ctx.value_req("DC value")?),
            "ac" => {
                let mag = ctx.value_opt();
                let phase = if mag.is_some() { ctx.value_opt() } else { None };
                ac = Some(AcSpec {
                    mag: mag.unwrap_or(1.0),
                    phase: phase.unwrap_or(0.0),
                });
            }
            "distof1" | "distof2" => {
                let mag = ctx.value_opt();
                let phase = if mag.is_some() { ctx.value_opt() } else { None };
                let spec = DistoSpec {
                    mag: mag.unwrap_or(1.0),
                    phase: phase.unwrap_or(0.0),
                };
                if tok.eq_ic("distof1") {
                    distof1 = Some(spec);
                } else {
                    distof2 = Some(spec);
                }
            }
            fname @ ("pulse" | "sin" | "exp" | "pwl" | "sffm" | "trnoise" | "trrandom") => {
                let kind = match fname {
                    "pulse" => TranFnKind::Pulse,
                    "sin" => TranFnKind::Sin,
                    "exp" => TranFnKind::Exp,
                    "pwl" => TranFnKind::Pwl,
                    "sffm" => TranFnKind::Sffm,
                    "trnoise" => TranFnKind::TrNoise,
                    _ => TranFnKind::TrRandom,
                };
                let mut args = Vec::new();
                while let Some(v) = ctx.value_opt() {
                    args.push(v);
                }
                // ngspice's `VSRCparam`/`ISRCparam` read the first two
                // coefficients unconditionally (`NA NT`, `TYPE TS`)
                // and at most 7 / 5.
                let arity = match kind {
                    TranFnKind::TrNoise => {
                        Some((2, 7, "TRNOISE(NA NT <NALPHA NAMP <RTSAM RTSCAPT RTSEMT>>)"))
                    }
                    TranFnKind::TrRandom => {
                        Some((2, 5, "TRRANDOM(TYPE TS <TD <PARAM1 <PARAM2>>>)"))
                    }
                    _ => None,
                };
                if let Some((lo, hi, form)) = arity
                    && !(lo..=hi).contains(&args.len())
                {
                    ctx.err(
                        span,
                        format!("{form} takes {lo} to {hi} arguments, got {}", args.len()),
                    );
                }
                if tran.is_some() {
                    ctx.warn(span, "more than one transient function; the last one wins");
                }
                tran = Some(TranFn { kind, args });
            }
            other => {
                ctx.err(span, format!("unknown parameter ({other})"));
            }
        }
    }

    // `VSRCtemp`: an absent Z0 defaults to 50 Ω; the source is a
    // port when both the index and the impedance are positive.
    let port = port_num.and_then(|num| {
        let z0 = if port_z0_given { port_z0 } else { 50.0 };
        u32::try_from(num)
            .ok()
            .filter(|&n| n > 0 && z0 > 0.0)
            .map(|num| RfPort { num, z0 })
    });

    Some((
        vec![n1, n2],
        Device::Source {
            kind,
            dc,
            ac,
            tran,
            distof1,
            distof2,
            port,
        },
    ))
}

/// `E`/`G`: `name N+ N- NC+ NC- VALUE` (the 3f5 also accepts the
/// value through the `gain` keyword).
fn vc_source(ctx: &mut ElemCtx, is_e: bool) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
        ctx.node_req("positive controlling node")?,
        ctx.node_req("negative controlling node")?,
    ];
    let (lead, params) = ctx.tail_params();
    let gain = lead.or_else(|| find_num(&params, "gain"));
    if gain.is_none() {
        let span = ctx.sc.here();
        ctx.err(span, "missing gain");
    }
    let device = if is_e {
        Device::Vcvs { gain }
    } else {
        Device::Vccs { gain }
    };
    Some((nodes, device))
}

/// `F`/`H`: `name N+ N- VNAM VALUE`.
fn cc_source(ctx: &mut ElemCtx, is_f: bool) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
    ];
    let (vname, vspan) = ctx.tok_req("controlling voltage source")?;
    if !vname.starts_with('v') {
        ctx.err(vspan, format!("`{vname}` is not a voltage source name"));
    }
    let (lead, params) = ctx.tail_params();
    let gain = lead.or_else(|| find_num(&params, "gain"));
    if gain.is_none() {
        let span = ctx.sc.here();
        ctx.err(span, "missing gain");
    }
    let device = if is_f {
        Device::Cccs { vname, gain }
    } else {
        Device::Ccvs { vname, gain }
    };
    Some((nodes, device))
}

fn find_num(params: &[Param], name: &str) -> Option<f64> {
    params
        .iter()
        .find_map(|p| match (&p.value, p.name == name) {
            (ParamValue::Num(v), true) => Some(*v),
            _ => None,
        })
}

fn diode(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
    let (area, off, params) = ctx.junction_tail();
    Some((
        nodes,
        Device::Diode {
            model,
            area,
            off,
            params,
        },
    ))
}

/// `Q`: `name NC NB NE <NS> MNAME …`. Whether the 4th name is the
/// substrate node or the model is decided by looking the token up in
/// the model table, exactly like `INP2Q`.
fn bjt(ctx: &mut ElemCtx, model_names: &HashSet<String>) -> Parsed {
    let mut nodes = vec![
        ctx.node_req("collector node")?,
        ctx.node_req("base node")?,
        ctx.node_req("emitter node")?,
    ];
    let (tok4, _) = ctx.tok_req("model name")?;
    let model = if model_names.contains(&tok4) {
        tok4
    } else {
        nodes.push(tok4);
        ctx.tok_req("model name")?.0
    };
    let (area, off, params) = ctx.junction_tail();
    Some((
        nodes,
        Device::Bjt {
            model,
            area,
            off,
            params,
        },
    ))
}

/// `J`/`Z`: `name ND NG NS MNAME <AREA> <OFF> <IC=…>`.
fn fet3(ctx: &mut ElemCtx, is_j: bool) -> Parsed {
    let nodes = vec![
        ctx.node_req("drain node")?,
        ctx.node_req("gate node")?,
        ctx.node_req("source node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
    let (area, off, params) = ctx.junction_tail();
    let device = if is_j {
        Device::Jfet {
            model,
            area,
            off,
            params,
        }
    } else {
        Device::Mesfet {
            model,
            area,
            off,
            params,
        }
    };
    Some((nodes, device))
}

/// `M`: `name ND NG NS NB MNAME …`, or for an ngspice VDMOS model
/// `name ND NG NS <TJ <TC>> MNAME …` (`INP2M`: after the third node
/// every token is looked up in the model table, and the first model
/// name ends the node list; VDMOS takes 3 to 5 nodes, the other
/// MOSFETs exactly 4). A token is taken as a 4th/5th node of a VDMOS
/// instance only when the model name follows it, so a 4-terminal
/// card keeps its 3f5 reading whatever its model table holds.
fn mosfet(
    ctx: &mut ElemCtx,
    model_names: &HashSet<String>,
    vdmos_models: &HashSet<String>,
) -> Parsed {
    let mut nodes = vec![
        ctx.node_req("drain node")?,
        ctx.node_req("gate node")?,
        ctx.node_req("source node")?,
    ];
    let model_span;
    let model = match ctx.sc.peek_tok() {
        Some(t) if vdmos_models.contains(&t.lc()) => {
            let (m, sp) = ctx.tok_req("model name")?;
            model_span = sp;
            m
        }
        _ => {
            nodes.push(ctx.node_req("bulk node")?);
            let mut probe = ctx.sc.clone();
            let fifth = probe.next_tok().map(|t| t.lc());
            let sixth = probe.next_tok().map(|t| t.lc());
            if let (Some(f), Some(x)) = (&fifth, &sixth)
                && !model_names.contains(f)
                && vdmos_models.contains(x)
            {
                nodes.push(ctx.node_req("case temperature node")?);
            }
            let (m, sp) = ctx.tok_req("model name")?;
            model_span = sp;
            m
        }
    };
    let (lead, params) = ctx.tail_params();
    if let Some(l) = lead {
        // `M1 … MNAME 2u` is not 3f5 syntax; keep it as `l` would be
        // wrong, so diagnose it.
        let span = ctx.sc.here();
        ctx.err(
            span,
            format!("unexpected bare number {l} after MOSFET model"),
        );
    }
    // ngspice-47's front end (`inp_vdmos_model`): an instance with the
    // `thermal` flag needs exactly five nodes — drain, gate, source,
    // junction and case temperature.
    if vdmos_models.contains(&model)
        && nodes.len() != 5
        && params
            .iter()
            .any(|p| p.name == "thermal" && p.value == ParamValue::Flag)
    {
        ctx.err(
            model_span,
            "VDMOS instance with the `thermal` flag needs exactly 5 nodes: \
             drain, gate, source, tjunction, tcase",
        );
        return None;
    }
    // `off` is a flag among the keyword params for MOSFETs; leave it
    // in `params` (the core treats it with the other instance params).
    Some((nodes, Device::Mosfet { model, params }))
}

fn switch_state(ctx: &mut ElemCtx) -> Option<OnOff> {
    let tok = ctx.sc.peek_tok()?;
    let state = if tok.eq_ic("on") {
        Some(OnOff::On)
    } else if tok.eq_ic("off") {
        Some(OnOff::Off)
    } else {
        None
    };
    if state.is_some() {
        ctx.sc.next_tok();
    }
    state
}

/// `S`: `name N+ N- NC+ NC- MODEL <ON|OFF>`.
fn switch_v(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
        ctx.node_req("positive controlling node")?,
        ctx.node_req("negative controlling node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
    let state = switch_state(ctx);
    trailing_junk(ctx);
    Some((nodes, Device::SwitchV { model, state }))
}

/// `W`: `name N+ N- VNAM MODEL <ON|OFF>`.
fn switch_i(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
    ];
    let (vname, vspan) = ctx.tok_req("controlling voltage source")?;
    if !vname.starts_with('v') {
        ctx.err(vspan, format!("`{vname}` is not a voltage source name"));
    }
    let (model, _) = ctx.tok_req("model name")?;
    let state = switch_state(ctx);
    trailing_junk(ctx);
    Some((
        nodes,
        Device::SwitchI {
            vname,
            model,
            state,
        },
    ))
}

fn trailing_junk(ctx: &mut ElemCtx) {
    while let Some(tok) = ctx.sc.next_tok() {
        let span = ctx.sc.span(&tok);
        let t = tok.lc();
        ctx.err(span, format!("unknown parameter ({t})"));
    }
}

/// `T`: `name N1 N2 N3 N4 Z0=V <TD=V> <F=FREQ <NL=NRMLEN>> <IC=…>`.
fn tline_ideal(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("port 1 positive node")?,
        ctx.node_req("port 1 negative node")?,
        ctx.node_req("port 2 positive node")?,
        ctx.node_req("port 2 negative node")?,
    ];
    let params = ctx.keyword_params();
    Some((nodes, Device::TlineIdeal { params }))
}

/// `O`: `name N1 N2 N3 N4 MNAME`.
fn tline_lossy(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("port 1 positive node")?,
        ctx.node_req("port 1 negative node")?,
        ctx.node_req("port 2 positive node")?,
        ctx.node_req("port 2 negative node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
    trailing_junk(ctx);
    Some((nodes, Device::TlineLossy { model }))
}

/// `U`: `name N1 N2 N3 MNAME L=LEN <N=LUMPS>`.
fn urc(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("node 1")?,
        ctx.node_req("node 2")?,
        ctx.node_req("capacitance node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
    let params = ctx.keyword_params();
    Some((nodes, Device::Urc { model, params }))
}

/// `B`: `name N+ N- <V=EXPR> <I=EXPR>` (`INP2B` + `INPdevParse`):
/// keyword `v` or `i`, then a parse tree read with `INPgetTree`
/// (`expr.rs`) for as long as it goes; the card scan resumes after
/// it. The last `v=`/`i=` wins (`ASRCparam` overwrites type and
/// tree); any other keyword is an unknown parameter, which ends the
/// card as in the C.
fn behavioral(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
    ];
    // `INPdevParse`'s optional leading number: `INP2B` never reads it.
    if let Some((_, tok)) = ctx.sc.try_value() {
        let span = ctx.sc.span(&tok);
        ctx.warn(span, "leading value on a B source is ignored");
    }
    let mut source: Option<(SourceKind, Expr)> = None;
    let mut failed = false;
    while let Some(tok) = ctx.sc.next_tok() {
        let span = ctx.sc.span(&tok);
        let kind = match tok.lc().as_str() {
            "v" => SourceKind::Voltage,
            "i" => SourceKind::Current,
            other => {
                ctx.err(span, format!("unknown parameter ({other})"));
                failed = true;
                break;
            }
        };
        // `INPgetTok`'s trailing gobble (blanks, `=`, `,`).
        let text = &ctx.card.text;
        let mut start = tok.end;
        while start < text.len() && matches!(text.as_bytes()[start], b' ' | b'\t' | b'=' | b',') {
            start += 1;
        }
        let (res, used) = crate::expr::parse_prefix(&text[start..]);
        match res {
            Ok(expr) => source = Some((kind, expr)),
            Err(e) => {
                let span = ctx
                    .card
                    .span_of(start + e.start, start + e.end.max(e.start + 1));
                ctx.err(span, e.message);
                failed = true;
                break;
            }
        }
        ctx.sc.seek(start + used);
    }
    match source {
        Some((kind, expr)) if !failed => Some((nodes, Device::Behavioral { kind, expr })),
        Some(_) => None,
        None => {
            if !failed {
                let span = ctx.sc.here();
                ctx.err(span, "B source needs V=<expr> or I=<expr>");
            }
            None
        }
    }
}

/// `X`: `name N1 <N2 …> SUBNAM` — the last token is the subcircuit.
fn subckt_call(ctx: &mut ElemCtx) -> Parsed {
    let mut names = Vec::new();
    let text = ctx.card.text.as_bytes();
    while let Some(tok) = ctx.sc.next_tok() {
        // `name=value` (or ngspice's `params:` keyword) passes
        // subcircuit parameters, which do not exist yet; without this
        // check the value would be taken for the subcircuit name.
        let rest = &text[tok.end..];
        let eq = rest
            .iter()
            .find(|b| !matches!(b, b' ' | b'\t'))
            .is_some_and(|&b| b == b'=');
        if eq || tok.eq_ic("params:") {
            let span = ctx.sc.span(&tok);
            ctx.err(
                span,
                format!(
                    "subcircuit parameters on X lines (`{}`) are not supported yet",
                    if eq {
                        format!("{}=…", tok.lc())
                    } else {
                        tok.lc()
                    }
                ),
            );
            return None;
        }
        names.push(tok.lc());
    }
    let Some(subname) = names.pop() else {
        let span = ctx.sc.here();
        ctx.err(span, "subcircuit call needs nodes and a subcircuit name");
        return None;
    };
    Some((names, Device::SubcktCall { subname }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::read_with_loader;
    use crate::diag::Severity;
    use std::path::Path;

    fn parse(line: &str) -> (Option<Element>, Vec<Diagnostic>) {
        parse_with_models(line, &[])
    }

    fn parse_with_models(line: &str, models: &[&str]) -> (Option<Element>, Vec<Diagnostic>) {
        struct NoInc;
        impl crate::deck::IncludeLoader for NoInc {
            fn load(&mut self, _: &Path, _: u64) -> Result<String, String> {
                Err("none".into())
            }
        }
        let mut deck = read_with_loader(Path::new("t.cir"), format!("t\n{line}\n"), &mut NoInc);
        let card = deck.cards.remove(0);
        let mut diags = Vec::new();
        let names: HashSet<String> = models.iter().map(|s| s.to_string()).collect();
        let env = ParamEnv::default();
        let vd: HashSet<String> = models
            .iter()
            .filter(|s| s.starts_with("vd"))
            .map(|s| s.to_string())
            .collect();
        let e = parse_element(&card, &mut diags, &names, &vd, &env);
        (e, diags)
    }

    fn ok(line: &str) -> Element {
        let (e, diags) = parse(line);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{line}: {diags:?}"
        );
        e.expect(line)
    }

    #[test]
    fn resistor_forms() {
        let e = ok("R1 1 2 100");
        assert_eq!(e.nodes, ["1", "2"]);
        assert!(matches!(
            e.device,
            Device::Resistor { value: Some(v), model: None, .. } if v == 100.0
        ));
        // Semiconductor form: value, model, geometry.
        let e = ok("rload 5 0 10k rmod w=2u l=10u");
        match &e.device {
            Device::Resistor {
                value,
                model,
                params,
            } => {
                assert_eq!(*value, Some(1e4));
                assert_eq!(model.as_deref(), Some("rmod"));
                assert_eq!(params.len(), 2);
                assert_eq!(params[0].name, "w");
            }
            d => panic!("{d:?}"),
        }
        // Model without value.
        let e = ok("r2 a b rmod l=5u");
        assert!(
            matches!(&e.device, Device::Resistor { value: None, model: Some(m), .. } if m == "rmod")
        );
    }

    #[test]
    fn capacitor_with_ic() {
        let e = ok("COSC 17 23 10U IC=3V");
        match &e.device {
            Device::Capacitor { value, params, .. } => {
                assert!((value.unwrap() - 1e-5).abs() < 1e-18);
                assert_eq!(params[0].name, "ic");
                assert_eq!(params[0].value, ParamValue::Num(3.0));
            }
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn coupled_inductors() {
        let e = ok("K43 LAA LBB 0.999");
        assert!(e.nodes.is_empty());
        assert!(matches!(
            &e.device,
            Device::Mutual { l1, l2, k } if l1 == "laa" && l2 == "lbb" && *k == 0.999
        ));
        let (_, diags) = parse("k1 r1 l2 0.5");
        assert!(diags.iter().any(|d| d.message.contains("not an inductor")));
    }

    #[test]
    fn transient_noise_functions() {
        // ngspice-47 manual §4.1.7/§4.1.8 forms.
        let e = ok("VNoi 1 0 DC 0 TRNOISE(20n 0.5n 0 0)");
        match &e.device {
            Device::Source { dc, tran, .. } => {
                assert_eq!(*dc, Some(0.0));
                let t = tran.as_ref().unwrap();
                assert_eq!(t.kind, TranFnKind::TrNoise);
                assert_eq!(t.args, vec![20e-9, 0.5e-9, 0.0, 0.0]);
            }
            d => panic!("{d:?}"),
        }
        let e = ok("inoi 1 0 trnoise 0 0 0 0 15m 22u 50u");
        match &e.device {
            Device::Source { tran, .. } => {
                let t = tran.as_ref().unwrap();
                assert_eq!(t.kind, TranFnKind::TrNoise);
                assert_eq!(t.args.len(), 7);
            }
            d => panic!("{d:?}"),
        }
        let e = ok("VR2 r2 0 dc 0 trrandom (2 10m 0 1)");
        match &e.device {
            Device::Source { tran, .. } => {
                let t = tran.as_ref().unwrap();
                assert_eq!(t.kind, TranFnKind::TrRandom);
                assert_eq!(t.args, vec![2.0, 10e-3, 0.0, 1.0]);
            }
            d => panic!("{d:?}"),
        }
        let (_, diags) = parse("v1 1 0 trnoise(1m)");
        assert!(diags.iter().any(|d| d.message.contains("takes 2 to 7")));
        let (_, diags) = parse("v1 1 0 trrandom(1 1m 0 1 0 9)");
        assert!(diags.iter().any(|d| d.message.contains("takes 2 to 5")));
    }

    #[test]
    fn source_card_full() {
        let e = ok("VIN 13 2 0.001 AC 1 SIN(0 1 1MEG)");
        match &e.device {
            Device::Source {
                kind, dc, ac, tran, ..
            } => {
                assert_eq!(*kind, SourceKind::Voltage);
                assert_eq!(*dc, Some(0.001));
                assert_eq!(
                    ac.unwrap(),
                    AcSpec {
                        mag: 1.0,
                        phase: 0.0
                    }
                );
                let t = tran.as_ref().unwrap();
                assert_eq!(t.kind, TranFnKind::Sin);
                assert_eq!(t.args, vec![0.0, 1.0, 1e6]);
            }
            d => panic!("{d:?}"),
        }
        // DC keyword form, AC with mag+phase, distof.
        let e = ok("isrc 23 21 ac 0.333 45.0 sffm(0 1 10k 5 1k) distof1 1m");
        match &e.device {
            Device::Source {
                kind,
                ac,
                tran,
                distof1,
                ..
            } => {
                assert_eq!(*kind, SourceKind::Current);
                assert_eq!(
                    ac.unwrap(),
                    AcSpec {
                        mag: 0.333,
                        phase: 45.0
                    }
                );
                assert_eq!(tran.as_ref().unwrap().kind, TranFnKind::Sffm);
                assert_eq!(
                    distof1.unwrap(),
                    DistoSpec {
                        mag: 1e-3,
                        phase: 0.0
                    }
                );
            }
            d => panic!("{d:?}"),
        }
        // Bare AC keyword defaults to magnitude 1.
        let e = ok("v2 1 0 dc 5 ac");
        assert!(matches!(
            &e.device,
            Device::Source { dc: Some(d), ac: Some(a), .. } if *d == 5.0 && a.mag == 1.0
        ));
        let (_, diags) = parse("v1 1 0 bogus 3");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("unknown parameter (bogus)"))
        );
    }

    #[test]
    fn rf_port_parameters() {
        let port = |card: &str| match ok(card).device {
            Device::Source { port, .. } => port,
            d => panic!("{d:?}"),
        };
        assert_eq!(
            port("v1 1 0 dc 0 ac 1 portnum 1"),
            Some(RfPort { num: 1, z0: 50.0 })
        );
        assert_eq!(
            port("v2 2 0 portnum=2 z0=75"),
            Some(RfPort { num: 2, z0: 75.0 })
        );
        // `IF_INTEGER` truncates.
        assert_eq!(
            port("v2 2 0 portnum 2.7"),
            Some(RfPort { num: 2, z0: 50.0 })
        );
        // Not ports: index 0, Z0 given non-positive after the index,
        // Z0 without an index.
        assert_eq!(port("v3 3 0 portnum 0"), None);
        assert_eq!(port("v3 3 0 portnum 1 z0 -5"), None);
        assert_eq!(port("v3 3 0 z0 75"), None);
        // `VSRCparam` order quirk: the index resets a bad Z0 to 50.
        assert_eq!(
            port("v3 3 0 z0 -5 portnum 1"),
            Some(RfPort { num: 1, z0: 50.0 })
        );
        // Current sources have no ports; the power-source keywords
        // are refused.
        let (_, diags) = parse("i1 1 0 portnum 1");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("unknown parameter (portnum)"))
        );
        let (_, diags) = parse("v1 1 0 portnum 1 pwr 0.01");
        assert!(
            diags
                .iter()
                .any(|d| d.message.contains("`pwr` is not supported"))
        );
    }

    #[test]
    fn pwl_collects_pairs() {
        let e = ok("VCLOCK 7 5 PWL(0 -7 10NS -7 11NS -3 17NS -3 18NS -7)");
        match &e.device {
            Device::Source { tran: Some(t), .. } => {
                assert_eq!(t.kind, TranFnKind::Pwl);
                assert_eq!(t.args.len(), 10);
                assert_eq!(t.args[2], 1e-8);
            }
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn vdmos_instances_take_three_to_five_nodes() {
        // `vdm` is a VDMOS model (the helper marks names starting
        // with `vd`), `nm` an ordinary MOS model.
        let models = ["vdm", "nm"];
        let (e, d) = parse_with_models("m1 d g s vdm m=2", &models);
        let e = e.unwrap();
        assert!(d.is_empty(), "{d:?}");
        assert_eq!(e.nodes, ["d", "g", "s"]);
        assert!(matches!(&e.device, Device::Mosfet { model, .. } if model == "vdm"));
        let (e, _) = parse_with_models("m1 d g s tj tc vdm thermal", &models);
        let e = e.unwrap();
        assert_eq!(e.nodes, ["d", "g", "s", "tj", "tc"]);
        let (e, _) = parse_with_models("m1 d g s tj vdm", &models);
        assert_eq!(e.unwrap().nodes.len(), 4);
        // A 4-terminal card reads as before.
        let (e, _) = parse_with_models("m1 d g s b nm l=1u", &models);
        let e = e.unwrap();
        assert_eq!(e.nodes, ["d", "g", "s", "b"]);
        assert!(matches!(&e.device, Device::Mosfet { model, .. } if model == "nm"));
        // `thermal` needs the two temperature nodes.
        let (e, d) = parse_with_models("m1 d g s vdm thermal", &models);
        assert!(e.is_none());
        assert!(d.iter().any(|d| d.message.contains("exactly 5 nodes")));
    }

    #[test]
    fn dependent_sources() {
        let e = ok("E1 2 3 14 1 2.0");
        assert_eq!(e.nodes.len(), 4);
        assert!(matches!(e.device, Device::Vcvs { gain: Some(g) } if g == 2.0));
        let e = ok("GBUF 1 2 10 3 1M");
        assert!(matches!(e.device, Device::Vccs { gain: Some(g) } if g == 1e-3));
        let e = ok("F1 13 5 VSENS 5");
        assert!(matches!(
            &e.device,
            Device::Cccs { vname, gain: Some(g) } if vname == "vsens" && *g == 5.0
        ));
        let e = ok("HX 5 17 VZ 0.5K");
        assert!(matches!(
            &e.device,
            Device::Ccvs { vname, gain: Some(g) } if vname == "vz" && *g == 500.0
        ));
        let (_, diags) = parse("e1 1 2 3 4");
        assert!(diags.iter().any(|d| d.message.contains("missing gain")));
    }

    #[test]
    fn diode_and_fets() {
        let e = ok("DBRIDGE 2 10 DIODE1 1.5 OFF IC=0.2");
        match &e.device {
            Device::Diode {
                model,
                area,
                off,
                params,
            } => {
                assert_eq!(model, "diode1");
                assert_eq!(*area, Some(1.5));
                assert!(*off);
                assert_eq!(params[0].name, "ic");
            }
            d => panic!("{d:?}"),
        }
        let e = ok("J1 7 2 3 JM1 OFF");
        assert!(matches!(&e.device, Device::Jfet { model, off: true, .. } if model == "jm1"));
        let e = ok("Z1 7 2 3 ZM1");
        assert!(matches!(&e.device, Device::Mesfet { .. }));
    }

    #[test]
    fn bjt_substrate_is_model_table_driven() {
        // With `mod1` defined, the 4th token is the model: 3 nodes.
        let (e, _) = parse_with_models("q1 3 1 4 mod1", &["mod1"]);
        let e = e.unwrap();
        assert_eq!(e.nodes, ["3", "1", "4"]);
        assert!(matches!(&e.device, Device::Bjt { model, .. } if model == "mod1"));
        // Without it, the 4th token is the substrate node.
        let (e, _) = parse_with_models("q2 3 1 4 2 mod1 1.5", &["mod1"]);
        let e = e.unwrap();
        assert_eq!(e.nodes, ["3", "1", "4", "2"]);
        assert!(matches!(
            &e.device,
            Device::Bjt { model, area: Some(a), .. } if model == "mod1" && *a == 1.5
        ));
    }

    #[test]
    fn mosfet_with_geometry() {
        let e = ok("M31 2 17 6 10 MOD1 L=5U W=2U AD=100P AS=100P IC=2,0.5,1");
        match &e.device {
            Device::Mosfet { model, params } => {
                assert_eq!(model, "mod1");
                assert_eq!(params.len(), 5);
                assert_eq!(
                    params.last().unwrap().value,
                    ParamValue::NumList(vec![2.0, 0.5, 1.0])
                );
            }
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn switches() {
        let e = ok("s1 1 2 3 4 switch1 ON");
        assert!(matches!(
            &e.device,
            Device::SwitchV { model, state: Some(OnOff::On) } if model == "switch1"
        ));
        let e = ok("w1 1 2 vclock switchmod1");
        assert!(matches!(
            &e.device,
            Device::SwitchI { vname, state: None, .. } if vname == "vclock"
        ));
    }

    #[test]
    fn transmission_lines_and_urc() {
        let e = ok("T1 1 0 2 0 Z0=50 TD=10NS");
        match &e.device {
            Device::TlineIdeal { params } => {
                assert_eq!(params[0].name, "z0");
                assert_eq!(params[0].value, ParamValue::Num(50.0));
            }
            d => panic!("{d:?}"),
        }
        let e = ok("O23 1 0 2 0 LOSSYMOD");
        assert!(matches!(&e.device, Device::TlineLossy { model } if model == "lossymod"));
        let e = ok("U1 1 2 0 URCMOD L=50U");
        assert!(matches!(&e.device, Device::Urc { model, .. } if model == "urcmod"));
    }

    #[test]
    fn behavioral_parses_the_expression() {
        use crate::ir::{ExprFunc, ExprOp};
        let e = ok("B1 0 1 I=cos(v(1))+sin(v(2))");
        match &e.device {
            Device::Behavioral {
                kind: SourceKind::Current,
                expr: Expr::Bin(ExprOp::Plus, l, r),
            } => {
                assert_eq!(
                    **l,
                    Expr::Call(ExprFunc::Cos, Box::new(Expr::V("1".into())))
                );
                assert_eq!(
                    **r,
                    Expr::Call(ExprFunc::Sin, Box::new(Expr::V("2".into())))
                );
            }
            d => panic!("{d:?}"),
        }
        let e = ok("B2 2 0 V = v(1) * 2");
        assert!(matches!(
            &e.device,
            Device::Behavioral {
                kind: SourceKind::Voltage,
                expr: Expr::Bin(ExprOp::Times, _, _)
            }
        ));
        // The last keyword wins; the expression stops before it.
        let e = ok("B3 3 4 V=v(1) I=17");
        assert!(matches!(
            &e.device,
            Device::Behavioral { kind: SourceKind::Current, expr: Expr::Num(n) } if *n == 17.0
        ));
    }

    #[test]
    fn behavioral_errors_carry_spans() {
        for (line, needle) in [
            ("B1 1 0 V=foo(1)", "no such function"),
            ("B1 1 0 V=v(1)+", "syntax error"),
            ("B1 1 0 W=1", "unknown parameter (w)"),
            ("B1 1 0", "needs V=<expr>"),
            ("B1 1 0 V=2*time", "time"),
            ("B1 1 0 V=v(1) junk", "unknown parameter (junk)"),
        ] {
            let (el, diags) = parse(line);
            assert!(el.is_none(), "{line}");
            assert!(
                diags
                    .iter()
                    .any(|d| d.severity == Severity::Error && d.message.contains(needle)),
                "{line}: {diags:?}"
            );
        }
    }

    #[test]
    fn subckt_call_takes_last_token_as_name() {
        let e = ok("X1 2 4 17 3 1 MULTI");
        assert_eq!(e.nodes, ["2", "4", "17", "3", "1"]);
        assert!(matches!(&e.device, Device::SubcktCall { subname } if subname == "multi"));
    }

    #[test]
    fn subckt_call_with_parameters_is_a_precise_error() {
        for (line, want) in [
            ("X1 a b inv w=0.15 l = 1u", "`w=…`"),
            ("x1 a b inv params: w=1", "`params:`"),
        ] {
            let (el, diags) = parse(line);
            assert!(el.is_none(), "{line}");
            assert_eq!(diags.len(), 1, "{line}: {diags:?}");
            assert!(diags[0].message.contains("not supported yet"), "{line}");
            assert!(
                diags[0].message.contains(want),
                "{line}: {}",
                diags[0].message
            );
        }
    }

    #[test]
    fn missing_fields_diagnose_not_panic() {
        for bad in [
            "r1 1",
            "c1",
            "v1 1",
            "e1 1 2 3",
            "d1 1 0",
            "k1 l1",
            "m1 1 2 3 4",
        ] {
            let (_, diags) = parse(bad);
            assert!(
                diags.iter().any(|d| d.severity == Severity::Error),
                "{bad} produced no error"
            );
        }
    }
}
