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
    AcSpec, Device, DistoSpec, Element, OnOff, Param, ParamValue, SourceKind, TranFn, TranFnKind,
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
        match self.env.eval_brace(&text) {
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
/// from its model name the same way `INP2Q` does.
pub(crate) fn parse_element(
    card: &Card,
    diags: &mut Vec<Diagnostic>,
    model_names: &HashSet<String>,
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
        'm' => mosfet(&mut ctx)?,
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
/// semiconductor R/C (`w l temp ic resistance capacitance`): a bare
/// token matching one of these is a parameter, not a model name.
fn is_keyword_param(tok: &str) -> bool {
    matches!(
        tok,
        "w" | "l" | "temp" | "ic" | "resistance" | "capacitance" | "area" | "off"
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

    while let Some(tok) = ctx.sc.next_tok() {
        let span = ctx.sc.span(&tok);
        match tok.lc().as_str() {
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
            fname @ ("pulse" | "sin" | "exp" | "pwl" | "sffm") => {
                let kind = match fname {
                    "pulse" => TranFnKind::Pulse,
                    "sin" => TranFnKind::Sin,
                    "exp" => TranFnKind::Exp,
                    "pwl" => TranFnKind::Pwl,
                    _ => TranFnKind::Sffm,
                };
                let mut args = Vec::new();
                while let Some(v) = ctx.value_opt() {
                    args.push(v);
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

    Some((
        vec![n1, n2],
        Device::Source {
            kind,
            dc,
            ac,
            tran,
            distof1,
            distof2,
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

fn mosfet(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("drain node")?,
        ctx.node_req("gate node")?,
        ctx.node_req("source node")?,
        ctx.node_req("bulk node")?,
    ];
    let (model, _) = ctx.tok_req("model name")?;
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

/// `B`: `name N+ N- <V=EXPR> <I=EXPR>`. The expression is kept raw
/// (parse trees are built when a corpus circuit needs the device).
fn behavioral(ctx: &mut ElemCtx) -> Parsed {
    let nodes = vec![
        ctx.node_req("positive node")?,
        ctx.node_req("negative node")?,
    ];
    // The rest of the card is `v=<expr>` or `i=<expr>`; expressions
    // may contain every separator, so leave the scanner and read the
    // raw text.
    let rest_start = ctx
        .sc
        .peek_tok()
        .map(|t| t.start)
        .unwrap_or(ctx.card.text.len());
    let rest = ctx.card.text[rest_start..].trim();
    let span = ctx.card.span_of(rest_start, ctx.card.text.len());
    let lower = rest.to_ascii_lowercase();
    let (v_expr, i_expr) = if let Some(e) = lower.strip_prefix('v').and_then(after_eq) {
        (Some(rest[rest.len() - e.len()..].trim().to_string()), None)
    } else if let Some(e) = lower.strip_prefix('i').and_then(after_eq) {
        (None, Some(rest[rest.len() - e.len()..].trim().to_string()))
    } else {
        ctx.err(span, "B source needs V=<expr> or I=<expr>");
        (None, None)
    };
    Some((nodes, Device::Behavioral { v_expr, i_expr }))
}

fn after_eq(s: &str) -> Option<&str> {
    let s = s.trim_start();
    s.strip_prefix('=')
}

/// `X`: `name N1 <N2 …> SUBNAM` — the last token is the subcircuit.
fn subckt_call(ctx: &mut ElemCtx) -> Parsed {
    let mut names = Vec::new();
    while let Some(tok) = ctx.sc.next_tok() {
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
        let e = parse_element(&card, &mut diags, &names, &env);
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
    fn behavioral_keeps_raw_expression() {
        let e = ok("B1 0 1 I=cos(v(1))+sin(v(2))");
        assert!(matches!(
            &e.device,
            Device::Behavioral { v_expr: None, i_expr: Some(x) } if x == "cos(v(1))+sin(v(2))"
        ));
        let e = ok("B2 2 0 V = v(1) * 2");
        assert!(matches!(
            &e.device,
            Device::Behavioral { v_expr: Some(x), .. } if x == "v(1) * 2"
        ));
    }

    #[test]
    fn subckt_call_takes_last_token_as_name() {
        let e = ok("X1 2 4 17 3 1 MULTI");
        assert_eq!(e.nodes, ["2", "4", "17", "3", "1"]);
        assert!(matches!(&e.device, Device::SubcktCall { subname } if subname == "multi"));
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
