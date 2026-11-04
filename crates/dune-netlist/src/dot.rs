//! Control (dot) card parsing: analyses, `.options`, `.ic`/`.nodeset`
//! and the output cards. Dispatch is by exact keyword like
//! `INP2dot`'s `strcmp` chain (`corpus/spice3f5/src/lib/inp/inp2dot.c`);
//! `.print`/`.plot`/`.save`/`.four` are parsed here into the IR even
//! though the 3f5 handles them in the front end (`fte/dotcards.c`).

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::ir::{
    AcSuffix, Analysis, DcSweep, Netlist, NodeInit, NodeInitKind, OutVar, Output, OutputKind,
    Param, ParamValue, PzInput, PzWhich, Variation,
};
use crate::param::ParamEnv;
use crate::scan::Scanner;
use crate::span::Span;

/// `.options` keywords whose value is a word, not a number.
const STRING_OPTIONS: &[&str] = &["method"];

pub(crate) struct DotCtx<'a> {
    pub card: &'a Card,
    pub diags: &'a mut Vec<Diagnostic>,
    pub env: &'a ParamEnv,
}

impl DotCtx<'_> {
    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(msg.into(), span));
    }

    fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.diags.push(Diagnostic::warning(msg.into(), span));
    }

    fn value_opt(&mut self, sc: &mut Scanner) -> Option<f64> {
        if let Some((v, _)) = sc.try_value() {
            return Some(v);
        }
        let tok = sc.peek_tok()?;
        if !tok.text.starts_with('{') {
            return None;
        }
        let span = sc.span(&tok);
        let text = tok.lc();
        sc.next_tok();
        match self.env.eval_brace(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                self.err(span, e);
                Some(0.0)
            }
        }
    }

    fn value_req(&mut self, sc: &mut Scanner, what: &str) -> Option<f64> {
        match self.value_opt(sc) {
            Some(v) => Some(v),
            None => {
                let span = sc
                    .peek_tok()
                    .map(|t| sc.span(&t))
                    .unwrap_or_else(|| sc.here());
                self.err(span, format!("missing {what}"));
                None
            }
        }
    }
}

/// Parse one non-structural dot card into `nl`. Returns `false` when
/// the keyword is not recognized here.
pub(crate) fn parse_dot(
    keyword: &str,
    card: &Card,
    diags: &mut Vec<Diagnostic>,
    env: &ParamEnv,
    nl: &mut Netlist,
) -> bool {
    let mut ctx = DotCtx { card, diags, env };
    let span = card.span();
    match keyword {
        ".op" => nl.analyses.push(Analysis::Op { span }),
        ".dc" => dot_dc(&mut ctx, nl, span),
        ".ac" => dot_ac(&mut ctx, nl, span),
        ".tran" => dot_tran(&mut ctx, nl, span),
        ".pz" => dot_pz(&mut ctx, nl, span),
        ".noise" => dot_noise(&mut ctx, nl, span),
        ".disto" => dot_disto(&mut ctx, nl, span),
        ".sens" => dot_sens(&mut ctx, nl, span),
        ".tf" => dot_tf(&mut ctx, nl, span),
        ".options" | ".option" | ".opt" => dot_options(&mut ctx, nl),
        ".ic" => dot_nodeinit(&mut ctx, nl, NodeInitKind::Ic),
        ".nodeset" => dot_nodeinit(&mut ctx, nl, NodeInitKind::Nodeset),
        ".print" => dot_print_plot(&mut ctx, nl, span, OutputKind::Print),
        ".plot" => dot_print_plot(&mut ctx, nl, span, OutputKind::Plot),
        ".save" => dot_save(&mut ctx, nl, span),
        ".four" | ".fourier" => dot_four(&mut ctx, nl, span),
        ".width" => ctx.warn(span, ".width is ignored (output formatting, Phase 1)"),
        ".temp" => ctx.warn(span, ".temp card obsolete - use .options temp and tnom"),
        ".lib" => ctx.err(span, ".lib is not supported yet (use .include)"),
        _ => return false,
    }
    true
}

fn variation(tok: &str) -> Option<Variation> {
    match tok {
        "dec" => Some(Variation::Dec),
        "oct" => Some(Variation::Oct),
        "lin" => Some(Variation::Lin),
        _ => None,
    }
}

/// `.dc SRCNAM VSTART VSTOP VINCR [SRC2 START2 STOP2 INCR2]`
fn dot_dc(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok(); // `.dc`
    let mut sweeps = Vec::new();
    while let Some(tok) = sc.next_tok() {
        let src = tok.lc();
        let (Some(start), Some(stop), Some(step)) = (
            ctx.value_req(&mut sc, "sweep start"),
            ctx.value_req(&mut sc, "sweep stop"),
            ctx.value_req(&mut sc, "sweep increment"),
        ) else {
            return;
        };
        sweeps.push(DcSweep {
            src,
            start,
            stop,
            step,
        });
        if sweeps.len() > 2 {
            let s = sc.here();
            ctx.err(s, ".dc takes at most two sweep sources");
            return;
        }
    }
    if sweeps.is_empty() {
        ctx.err(span, ".dc needs a source and sweep values");
        return;
    }
    nl.analyses.push(Analysis::Dc { sweeps, span });
}

/// `.ac DEC|OCT|LIN N FSTART FSTOP`
fn dot_ac(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    let Some(vtok) = sc.next_tok() else {
        ctx.err(span, ".ac needs dec, oct or lin");
        return;
    };
    let vspan = sc.span(&vtok);
    let Some(var) = variation(&vtok.lc()) else {
        ctx.err(
            vspan,
            format!("expected dec, oct or lin, got `{}`", vtok.text),
        );
        return;
    };
    let (Some(points), Some(fstart), Some(fstop)) = (
        ctx.value_req(&mut sc, "number of points"),
        ctx.value_req(&mut sc, "start frequency"),
        ctx.value_req(&mut sc, "stop frequency"),
    ) else {
        return;
    };
    nl.analyses.push(Analysis::Ac {
        variation: var,
        points,
        fstart,
        fstop,
        span,
    });
}

/// `.tran TSTEP TSTOP <TSTART <TMAX>> <UIC>`
fn dot_tran(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    let (Some(tstep), Some(tstop)) = (
        ctx.value_req(&mut sc, "time step"),
        ctx.value_req(&mut sc, "stop time"),
    ) else {
        return;
    };
    let tstart = ctx.value_opt(&mut sc);
    let tmax = if tstart.is_some() {
        ctx.value_opt(&mut sc)
    } else {
        None
    };
    let mut uic = false;
    while let Some(tok) = sc.next_tok() {
        let tspan = sc.span(&tok);
        if tok.eq_ic("uic") {
            uic = true;
        } else {
            let t = tok.lc();
            ctx.err(tspan, format!("unknown parameter ({t})"));
        }
    }
    nl.analyses.push(Analysis::Tran {
        tstep,
        tstop,
        tstart,
        tmax,
        uic,
        span,
    });
}

/// `.pz NODE1 NODE2 NODE3 NODE4 CUR|VOL POL|ZER|PZ`
fn dot_pz(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    let mut nodes = Vec::new();
    for what in ["node 1", "node 2", "node 3", "node 4"] {
        match sc.next_tok() {
            Some(t) => nodes.push(t.lc()),
            None => {
                let s = sc.here();
                ctx.err(s, format!("missing {what}"));
                return;
            }
        }
    }
    let Some(itok) = sc.next_tok() else {
        let s = sc.here();
        ctx.err(s, "missing cur/vol");
        return;
    };
    let input = match itok.lc().as_str() {
        "cur" => PzInput::Current,
        "vol" => PzInput::Voltage,
        other => {
            let s = sc.span(&itok);
            ctx.err(s, format!("expected cur or vol, got `{other}`"));
            return;
        }
    };
    let Some(wtok) = sc.next_tok() else {
        let s = sc.here();
        ctx.err(s, "missing pol/zer/pz");
        return;
    };
    let which = match wtok.lc().as_str() {
        "pol" => PzWhich::Poles,
        "zer" => PzWhich::Zeros,
        "pz" => PzWhich::Both,
        other => {
            let s = sc.span(&wtok);
            ctx.err(s, format!("expected pol, zer or pz, got `{other}`"));
            return;
        }
    };
    let nodes: [String; 4] = nodes.try_into().expect("four nodes collected");
    nl.analyses.push(Analysis::Pz {
        nodes,
        input,
        which,
        span,
    });
}

/// `.noise V(OUTPUT <,REF>) SRC DEC|LIN|OCT PTS FSTART FSTOP <PPS>`
fn dot_noise(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    // `v ( out <, ref> )` arrives as the tokens `v out <ref>` since
    // parentheses and commas are separators — the same trick
    // `INP2dot` uses for `.nodeset`.
    let Some(vtok) = sc.next_tok() else {
        ctx.err(span, ".noise needs an output, v(node)");
        return;
    };
    if !vtok.eq_ic("v") {
        let s = sc.span(&vtok);
        ctx.err(s, ".noise output must be v(node) or v(node,ref)");
        return;
    }
    let Some(out) = sc.next_tok() else {
        let s = sc.here();
        ctx.err(s, "missing output node");
        return;
    };
    let output = out.lc();
    // The next token is either the reference node or the source.
    // Disambiguate like the C: if the token after it is dec/lin/oct,
    // this token was the source.
    let Some(t1) = sc.next_tok() else {
        let s = sc.here();
        ctx.err(s, "missing noise source");
        return;
    };
    let (output_ref, src) = match sc.peek_tok() {
        Some(t2) if variation(&t2.lc()).is_some() => (None, t1.lc()),
        _ => {
            let r = t1.lc();
            match sc.next_tok() {
                Some(srctok) => (Some(r), srctok.lc()),
                None => {
                    let s = sc.here();
                    ctx.err(s, "missing noise source");
                    return;
                }
            }
        }
    };
    let Some(vtok) = sc.next_tok() else {
        let s = sc.here();
        ctx.err(s, "missing dec, oct or lin");
        return;
    };
    let Some(var) = variation(&vtok.lc()) else {
        let s = sc.span(&vtok);
        ctx.err(s, format!("expected dec, oct or lin, got `{}`", vtok.text));
        return;
    };
    let (Some(points), Some(fstart), Some(fstop)) = (
        ctx.value_req(&mut sc, "number of points"),
        ctx.value_req(&mut sc, "start frequency"),
        ctx.value_req(&mut sc, "stop frequency"),
    ) else {
        return;
    };
    let pts_per_summary = ctx.value_opt(&mut sc);
    nl.analyses.push(Analysis::Noise {
        output,
        output_ref,
        src,
        variation: var,
        points,
        fstart,
        fstop,
        pts_per_summary,
        span,
    });
}

/// `.disto DEC|OCT|LIN NP FSTART FSTOP <F2OVERF1>`
fn dot_disto(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    let Some(vtok) = sc.next_tok() else {
        ctx.err(span, ".disto needs dec, oct or lin");
        return;
    };
    let Some(var) = variation(&vtok.lc()) else {
        let s = sc.span(&vtok);
        ctx.err(s, format!("expected dec, oct or lin, got `{}`", vtok.text));
        return;
    };
    let (Some(points), Some(fstart), Some(fstop)) = (
        ctx.value_req(&mut sc, "number of points"),
        ctx.value_req(&mut sc, "start frequency"),
        ctx.value_req(&mut sc, "stop frequency"),
    ) else {
        return;
    };
    let f2overf1 = ctx.value_opt(&mut sc);
    nl.analyses.push(Analysis::Disto {
        variation: var,
        points,
        fstart,
        fstop,
        f2overf1,
        span,
    });
}

/// `.sens OUTVAR` / `.sens OUTVAR AC DEC|OCT|LIN N FSTART FSTOP`
fn dot_sens(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let flds = fields(ctx.card);
    if flds.len() < 2 {
        ctx.err(span, ".sens needs an output variable");
        return;
    }
    let outvar = parse_outvar_field(ctx, flds[1]);
    let mut ac = None;
    if flds.len() > 2 {
        let text = ctx.card.text[flds[2].0..flds[2].1].to_ascii_lowercase();
        if text != "ac" {
            let s = ctx.card.span_of(flds[2].0, flds[2].1);
            ctx.err(s, format!("expected ac, got `{text}`"));
            return;
        }
        if flds.len() < 7 {
            ctx.err(span, ".sens ac needs dec|oct|lin, points and frequencies");
            return;
        }
        let vtext = ctx.card.text[flds[3].0..flds[3].1].to_ascii_lowercase();
        let Some(var) = variation(&vtext) else {
            let s = ctx.card.span_of(flds[3].0, flds[3].1);
            ctx.err(s, format!("expected dec, oct or lin, got `{vtext}`"));
            return;
        };
        let mut nums = [0.0; 3];
        for (i, f) in flds[4..7].iter().enumerate() {
            match crate::scan::evaluate(&ctx.card.text[f.0..f.1]) {
                Some(v) => nums[i] = v,
                None => {
                    let s = ctx.card.span_of(f.0, f.1);
                    ctx.err(s, "expected a number");
                    return;
                }
            }
        }
        ac = Some((var, nums[0], nums[1], nums[2]));
    }
    nl.analyses.push(Analysis::Sens { outvar, ac, span });
}

/// `.tf OUTVAR INSRC`
fn dot_tf(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let flds = fields(ctx.card);
    if flds.len() != 3 {
        ctx.err(span, ".tf needs an output variable and an input source");
        return;
    }
    let outvar = parse_outvar_field(ctx, flds[1]);
    let insrc = ctx.card.text[flds[2].0..flds[2].1].to_ascii_lowercase();
    nl.analyses.push(Analysis::Tf {
        outvar,
        insrc,
        span,
    });
}

/// `.options OPT1 OPT2 … OPT=VAL …` — `method` takes a word, the
/// rest are flags or numbers (`INPdoOpts`).
fn dot_options(ctx: &mut DotCtx, nl: &mut Netlist) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    while let Some(tok) = sc.next_tok() {
        let span = sc.span(&tok);
        let name = tok.lc();
        if STRING_OPTIONS.contains(&name.as_str()) {
            match sc.next_tok() {
                Some(v) => nl.options.push(Param {
                    name,
                    value: ParamValue::Str(v.lc()),
                    span,
                }),
                None => ctx.err(span, format!("option {name} needs a value")),
            }
            continue;
        }
        let mut nums = Vec::new();
        while let Some(v) = ctx.value_opt(&mut sc) {
            nums.push(v);
        }
        let value = match nums.len() {
            0 => ParamValue::Flag,
            1 => ParamValue::Num(nums[0]),
            _ => ParamValue::NumList(nums),
        };
        nl.options.push(Param { name, value, span });
    }
}

/// `.ic`/`.nodeset`: `V(NODE)=VAL …` seen as the tokens `v node val`.
fn dot_nodeinit(ctx: &mut DotCtx, nl: &mut Netlist, kind: NodeInitKind) {
    let mut sc = Scanner::new(ctx.card);
    sc.next_tok();
    let what = match kind {
        NodeInitKind::Ic => ".ic",
        NodeInitKind::Nodeset => ".nodeset",
    };
    while let Some(tok) = sc.next_tok() {
        let span = sc.span(&tok);
        if !tok.eq_ic("v") {
            ctx.err(span, format!("{what} syntax error: expected v(node)=value"));
            return;
        }
        let Some(node) = sc.next_tok() else {
            let s = sc.here();
            ctx.err(s, format!("{what}: missing node"));
            return;
        };
        let node = node.lc();
        let Some(value) = ctx.value_req(&mut sc, "node value") else {
            return;
        };
        nl.node_inits.push(NodeInit {
            kind,
            node,
            value,
            span,
        });
    }
}

/// `.print PRTYPE OV1 …` / `.plot PRTYPE OV1 <(PLO, PHI)> …`
fn dot_print_plot(ctx: &mut DotCtx, nl: &mut Netlist, span: Span, kind: OutputKind) {
    let flds = fields(ctx.card);
    if flds.len() < 2 {
        ctx.err(span, "missing analysis type (dc, ac, tran, noise or disto)");
        return;
    }
    let atype = ctx.card.text[flds[1].0..flds[1].1].to_ascii_lowercase();
    if !matches!(
        atype.as_str(),
        "dc" | "ac" | "tran" | "noise" | "disto" | "pz"
    ) {
        let s = ctx.card.span_of(flds[1].0, flds[1].1);
        ctx.err(s, format!("unknown analysis type `{atype}`"));
        return;
    }
    let mut vars = Vec::new();
    for f in &flds[2..] {
        let text = &ctx.card.text[f.0..f.1];
        if text.starts_with('(') {
            // `.plot` range `(PLO, PHI)` — recognized and ignored
            // until plotting exists.
            continue;
        }
        vars.push(parse_outvar_field(ctx, *f));
    }
    if vars.is_empty() {
        ctx.err(span, "no output variables");
        return;
    }
    nl.outputs.push(Output {
        kind,
        analysis: Some(atype),
        vars,
        span,
    });
}

/// `.save vec vec …`
fn dot_save(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let flds = fields(ctx.card);
    let vars: Vec<OutVar> = flds[1..]
        .iter()
        .map(|f| parse_outvar_field(ctx, *f))
        .collect();
    if vars.is_empty() {
        ctx.err(span, ".save needs at least one vector");
        return;
    }
    nl.outputs.push(Output {
        kind: OutputKind::Save,
        analysis: None,
        vars,
        span,
    });
}

/// `.four FREQ OV1 …`
fn dot_four(ctx: &mut DotCtx, nl: &mut Netlist, span: Span) {
    let flds = fields(ctx.card);
    if flds.len() < 3 {
        ctx.err(span, ".four needs a frequency and output variables");
        return;
    }
    let Some(freq) = crate::scan::evaluate(&ctx.card.text[flds[1].0..flds[1].1]) else {
        let s = ctx.card.span_of(flds[1].0, flds[1].1);
        ctx.err(s, "expected the fundamental frequency");
        return;
    };
    let vars: Vec<OutVar> = flds[2..]
        .iter()
        .map(|f| parse_outvar_field(ctx, *f))
        .collect();
    nl.outputs.push(Output {
        kind: OutputKind::Four { freq },
        analysis: None,
        vars,
        span,
    });
}

/// Whitespace-separated fields of a card (byte ranges); unlike the
/// element scanner this keeps parentheses, which output variables
/// need (`v(1,2)` is one field).
fn fields(card: &Card) -> Vec<(usize, usize)> {
    let bytes = card.text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'\t' {
            i += 1;
        }
        if i > start {
            out.push((start, i));
        }
    }
    out
}

fn parse_outvar_field(ctx: &mut DotCtx, (start, end): (usize, usize)) -> OutVar {
    let text = ctx.card.text[start..end].to_ascii_lowercase();
    let span = ctx.card.span_of(start, end);
    parse_outvar(&text, span, ctx.diags)
}

/// `v(1)`, `v(4,2)`, `vm(out)`, `vdb(5)`, `i(vin)`, plain names.
pub(crate) fn parse_outvar(text: &str, span: Span, diags: &mut Vec<Diagnostic>) -> OutVar {
    let Some(open) = text.find('(') else {
        return OutVar::Raw(text.to_string());
    };
    let Some(inner) = text[open..]
        .strip_prefix('(')
        .and_then(|t| t.strip_suffix(')'))
    else {
        diags.push(Diagnostic::warning(
            format!("`{text}` looks like an output variable but has no closing parenthesis"),
            span,
        ));
        return OutVar::Raw(text.to_string());
    };
    let head = &text[..open];
    let (is_v, suffix_text) = match head.as_bytes().first() {
        Some(b'v') => (true, &head[1..]),
        Some(b'i') => (false, &head[1..]),
        _ => return OutVar::Raw(text.to_string()),
    };
    let suffix = match suffix_text {
        "" => None,
        "r" => Some(AcSuffix::Real),
        "i" => Some(AcSuffix::Imag),
        "m" => Some(AcSuffix::Mag),
        "p" => Some(AcSuffix::Phase),
        "db" => Some(AcSuffix::Db),
        _ => return OutVar::Raw(text.to_string()),
    };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if is_v {
        match parts.as_slice() {
            [node] if !node.is_empty() => OutVar::Voltage {
                node: node.to_string(),
                reference: None,
                suffix,
            },
            [node, reference] if !node.is_empty() && !reference.is_empty() => OutVar::Voltage {
                node: node.to_string(),
                reference: Some(reference.to_string()),
                suffix,
            },
            _ => {
                diags.push(Diagnostic::warning(
                    format!("malformed voltage output `{text}`"),
                    span,
                ));
                OutVar::Raw(text.to_string())
            }
        }
    } else {
        match parts.as_slice() {
            [vsrc] if !vsrc.is_empty() => OutVar::Current {
                vsrc: vsrc.to_string(),
                suffix,
            },
            _ => {
                diags.push(Diagnostic::warning(
                    format!("malformed current output `{text}`"),
                    span,
                ));
                OutVar::Raw(text.to_string())
            }
        }
    }
}
