//! Parse driver: deck → [`Netlist`] + diagnostics.
//!
//! Mirrors the 3f5's two passes (`inppas1.c` models first, then
//! `inppas2.c`), plus a structural pass of our own: `.control` blocks
//! are swallowed verbatim, `.subckt`/`.ends` build scopes that
//! `subckt::expand` flattens afterwards, and `.param` constants are
//! collected up front so any card can use `{…}` references.

use std::collections::HashSet;
use std::path::Path;

use crate::cond::{Activity, Conds};
use crate::deck::{self, Card, Deck, IncludeLoader};
use crate::diag::Diagnostic;
use crate::elem::{ElemCtx, parse_element};
use crate::func::{FuncScope, FuncTable};
use crate::ir::{Analysis, Model, Netlist, Param, ParamValue};
use crate::param::{ParamDef, ParamEnv, TemperUse};
use crate::scan::Scanner;
use crate::span::SourceMap;
use crate::subckt::{
    BodyItem, ExpandCtx, GlobalModel, ModelCard, SubcktDef, TopItem, TopScope, expand,
};

/// 3f5 model types (`INPdomodel`): anything else is an error.
const MODEL_TYPES: &[&str] = &[
    "r", "c", "sw", "csw", "urc", "ltra", "d", "npn", "pnp", "njf", "pjf", "nmos", "pmos", "nmf",
    "pmf",
    // ngspice-47's power MOSFET (`inpdomod.c`; `vdmosn`/`vdmosp` are
    // the forms its front end rewrites `vdmos` into).
    "vdmos", "vdmosn", "vdmosp", // ngspice-47's KSPICE lines (`inpdomod.c`).
    "txl", "cpl",
];

/// Whether a `.model` type token names ngspice's VDMOS
/// (`inp_vdmos_model`: the type starts with `vdmos`).
fn is_vdmos_type(t: &str) -> bool {
    matches!(t, "vdmos" | "vdmosn" | "vdmosp")
}

#[derive(Debug)]
pub struct ParseOutcome {
    pub netlist: Netlist,
    pub diags: Vec<Diagnostic>,
    pub sources: SourceMap,
}

impl ParseOutcome {
    /// True when no diagnostic is an error.
    pub fn is_clean(&self) -> bool {
        self.diags
            .iter()
            .all(|d| d.severity != crate::diag::Severity::Error)
    }
}

/// Parse a deck from the file system.
pub fn parse_file(path: &Path) -> Result<ParseOutcome, crate::Error> {
    let deck = deck::read_file(path)?;
    Ok(parse_deck(deck))
}

/// Parse a deck from a string; `.include`/`.lib` resolve through `loader`.
pub fn parse_with_loader(
    name: &Path,
    text: String,
    loader: &mut dyn IncludeLoader,
) -> ParseOutcome {
    parse_deck(deck::read_with_loader(name, text, loader))
}

/// Parse a deck from a string; `.include`/`.lib` resolve on the file
/// system, relative to `name`'s directory.
pub fn parse_str(name: &str, text: &str) -> ParseOutcome {
    parse_with_loader(Path::new(name), text.to_string(), &mut deck::FsLoader)
}

fn first_keyword(card: &Card) -> String {
    Scanner::new(card)
        .next_tok()
        .map(|t| t.lc())
        .unwrap_or_default()
}

fn parse_deck(deck: Deck) -> ParseOutcome {
    let Deck {
        title,
        cards,
        sources,
        diags: mut all_diags,
    } = deck;

    let mut nl = Netlist {
        title,
        ..Netlist::default()
    };

    // Pass A: model names (for `Q` substrate disambiguation) and
    // `.param` constants, skipping `.control` blocks.
    let mut model_names: HashSet<String> = HashSet::new();
    // VDMOS model names: their `M` instances take 3 to 5 nodes.
    let mut vdmos_models: HashSet<String> = HashSet::new();
    let mut param_defs: Vec<ParamDef> = Vec::new();
    let mut params_full = false;
    // User functions: the top level's, and the names defined anywhere
    // (a local model card calling one cannot share its evaluation).
    let mut global_funcs = FuncTable::default();
    let mut func_names: HashSet<String> = HashSet::new();
    let mut in_control = false;
    let mut subckt_depth = 0usize;
    for card in &cards {
        let kw = first_keyword(card);
        if in_control {
            in_control = kw != ".endc";
            continue;
        }
        match kw.as_str() {
            ".control" => in_control = true,
            ".subckt" => subckt_depth += 1,
            ".ends" => subckt_depth = subckt_depth.saturating_sub(1),
            ".model" => {
                let mut sc = Scanner::new(card);
                sc.next_tok();
                if let Some(name) = sc.next_tok() {
                    model_names.insert(name.lc());
                    if sc.next_tok().is_some_and(|t| is_vdmos_type(&t.lc())) {
                        vdmos_models.insert(name.lc());
                    }
                }
            }
            // `.param name(args) = body` is a `.func` (ngspice rewrites
            // it, probe `f1 f_params_kw`).
            ".func" => {
                func_name(card, &mut func_names);
                if subckt_depth == 0
                    && let Some(f) = crate::func::parse_def(card, &mut all_diags)
                {
                    global_funcs.insert(f);
                }
            }
            ".param" | ".params" if crate::func::is_param_function(card) => {
                func_name(card, &mut func_names);
                if subckt_depth == 0
                    && let Some(f) = crate::func::parse_def(card, &mut all_diags)
                {
                    global_funcs.insert(f);
                }
            }
            // `.params` is ngspice's synonym (the IHP libraries use it).
            // Definitions inside `.subckt` belong to the subcircuit's
            // scope (pass B collects them).
            ".param" | ".params" if subckt_depth == 0 && !params_full => {
                params_full = collect_params(card, &mut param_defs, &mut all_diags);
            }
            _ => {}
        }
    }
    global_funcs.check_recursion(&mut all_diags);
    let global_scope = FuncScope {
        table: &global_funcs,
        parent: None,
    };
    // The global scope at the default temperature, then — if the
    // deck sets another one and a parameter depends on `temper` —
    // again at the circuit temperature, which may itself read global
    // parameters (`.options temp={t0}`).
    let mut first_diags = Vec::new();
    let env = crate::param::resolve(&param_defs, Some(&global_scope), 27.0, &mut first_diags);
    let temperature = circuit_temperature(&cards, &env);
    let env = if temperature != 27.0 && env.has_tainted() {
        drop(env);
        crate::param::resolve(
            &param_defs,
            Some(&global_scope),
            temperature,
            &mut all_diags,
        )
    } else {
        all_diags.append(&mut first_diags);
        env
    };
    env.set_temperature(temperature);
    drop(param_defs);

    // Pass B: the real parse. Subcircuit bodies are only read
    // structurally here; `subckt::expand` evaluates them per instance.
    let mut top = TopScope::default();
    let mut stack: Vec<SubcktDef> = Vec::new();
    let mut in_control = false;
    let mut saw_end = false;
    let mut after_end = 0usize;
    // The last `.temp` card: set after every `.options` (ngspice-47
    // applies it once the circuit exists, `frontend/inp.c`).
    let mut temp_card: Option<Param> = None;
    // Top-level `.if` blocks, evaluated as they are read (the global
    // parameters are known); a subcircuit body records its own.
    let mut top_conds = Conds::default();
    let mut top_act = Activity::default();

    for card in &cards {
        let kw = first_keyword(card);
        if saw_end {
            after_end += 1;
            continue;
        }
        if in_control {
            if kw == ".endc" {
                in_control = false;
            } else {
                nl.control.push(card.text.clone());
            }
            continue;
        }
        let span = card.span();
        if let Some(d) = crate::cond::directive(&kw) {
            match stack.last_mut() {
                Some(def) => {
                    def.conds.read(d, card, &mut def.deferred);
                }
                None => {
                    if let Some(b) = top_conds.read(d, card, &mut all_diags) {
                        env.with_temper(TemperUse::Forbidden("an .if condition"), || {
                            top_conds.eval_branch(b, &env, &mut top_act, &mut |d| all_diags.push(d))
                        });
                    }
                }
            }
            continue;
        }
        // In a branch not taken at the top level only the cards
        // ngspice-47 treats unconditionally stay (`.subckt` bodies,
        // `.options`; `.param`/`.func` were read in pass A).
        if stack.is_empty()
            && !top_act.on(top_conds.guard())
            && !matches!(
                kw.as_str(),
                ".control" | ".end" | ".subckt" | ".ends" | ".options" | ".option" | ".opt"
            )
        {
            continue;
        }
        match kw.as_str() {
            ".control" => in_control = true,
            ".endc" => {
                all_diags.push(Diagnostic::error(".endc without .control", span));
            }
            ".end" => saw_end = true,
            ".subckt" => {
                let mut formals = Vec::new();
                let mut deferred = Vec::new();
                let Some((name, ports)) =
                    crate::subckt::parse_subckt_line(card, &mut formals, &mut deferred)
                else {
                    all_diags.append(&mut deferred);
                    continue;
                };
                stack.push(SubcktDef {
                    name,
                    ports,
                    formals,
                    items: Vec::new(),
                    models: Vec::new(),
                    model_index: Default::default(),
                    subckts: Vec::new(),
                    funcs: FuncTable::default(),
                    conds: Conds::default(),
                    deferred,
                    multiplied: false,
                    span,
                    card,
                });
            }
            ".ends" => {
                let mut sc = Scanner::new(card);
                sc.next_tok();
                let named = sc.next_tok().map(|t| t.lc());
                match stack.pop() {
                    None => {
                        all_diags.push(Diagnostic::error(".ends without .subckt", span));
                    }
                    Some(mut def) => {
                        let mut ds = Vec::new();
                        def.conds.finish(&format!(".subckt {}", def.name), &mut ds);
                        def.deferred.append(&mut ds);
                        // Upstream decks carry mismatched names here
                        // (ltra_3.cir has `.ends lump` for `.subckt
                        // rlclump`), so this cannot be fatal.
                        if let Some(n) = named
                            && n != def.name
                        {
                            all_diags.push(Diagnostic::warning(
                                format!(".ends {n} closes .subckt {}", def.name),
                                span,
                            ));
                        }
                        def.finish();
                        match stack.last_mut() {
                            Some(outer) => outer.subckts.push(def),
                            None => top.subckts.push(def),
                        }
                    }
                }
            }
            ".model" => match stack.last_mut() {
                Some(def) => {
                    let mut mc = ModelCard::new(card, true, &func_names);
                    mc.guard = def.conds.guard();
                    def.models.push(mc);
                }
                None => {
                    // Evaluated now with the global parameters; its
                    // diagnostics only count if an element uses it
                    // (ngspice P05c/P05f, `s1 model_undef_used_top`).
                    let mut diags = Vec::new();
                    let model =
                        env.with_temper(TemperUse::Device, || parse_model(card, &mut diags, &env));
                    top.models.push(GlobalModel {
                        card: ModelCard::new(card, false, &func_names),
                        model,
                        diags,
                    });
                }
            },
            ".func" | ".param" | ".params"
                if kw == ".func" || crate::func::is_param_function(card) =>
            {
                // Top-level ones were read in pass A; a subcircuit's
                // belong to its body (lexical scope, `f1 f_local*`).
                if let Some(def) = stack.last_mut()
                    && let Some(f) = crate::func::parse_def(card, &mut def.deferred)
                {
                    def.funcs.insert(f);
                }
            }
            ".param" | ".params" => {
                // Top-level ones were collected in pass A; inside a
                // subcircuit they join its formal parameters.
                if let Some(def) = stack.last_mut() {
                    collect_params(card, &mut def.formals, &mut def.deferred);
                }
            }
            k if k.starts_with('.') => {
                if !stack.is_empty() {
                    let msg = if k == ".temp" {
                        // ngspice-47 sets the circuit temperature from
                        // it when the subcircuit is instantiated
                        // (probes `t_subckt`, `t_sub_unused`).
                        ".temp inside .subckt is not supported; ngspice-47 applies it to the \
                         whole circuit when the subcircuit is used: move it to the top level"
                            .to_string()
                    } else {
                        format!("{k} is not allowed inside .subckt")
                    };
                    all_diags.push(Diagnostic::error(msg, span));
                } else if !env.with_temper(TemperUse::Forbidden("a control card"), || {
                    crate::dot::parse_dot(k, card, &mut all_diags, &env, &mut nl, &mut temp_card)
                }) {
                    all_diags.push(Diagnostic::error(
                        format!("unknown control card `{k}`"),
                        span,
                    ));
                }
            }
            "" => {}
            k if k.starts_with('x') => match stack.last_mut() {
                Some(def) => {
                    if let Some(x) = crate::subckt::parse_xcall(card, &mut def.deferred) {
                        def.items.push((def.conds.guard(), BodyItem::X(x)));
                    }
                }
                None => {
                    if let Some(x) = crate::subckt::parse_xcall(card, &mut all_diags) {
                        top.items.push(TopItem::X(x));
                    }
                }
            },
            _ => match stack.last_mut() {
                Some(def) => def.items.push((def.conds.guard(), BodyItem::Elem(card))),
                None => {
                    if let Some(el) = env.with_temper(ParamEnv::temper_for_card(card), || {
                        parse_element(card, &mut all_diags, &model_names, &vdmos_models, &env)
                    }) {
                        top.items.push(TopItem::Elem(el));
                    }
                }
            },
        }
    }

    top_conds.finish("the deck", &mut all_diags);
    for def in &stack {
        all_diags.push(Diagnostic::error(
            format!(".subckt {} never closed by .ends", def.name),
            def.span,
        ));
    }
    if !saw_end {
        all_diags.push(Diagnostic::global_warning("no .end card"));
    }
    if after_end > 0 {
        all_diags.push(Diagnostic::global_warning(format!(
            "{after_end} card(s) after .end ignored"
        )));
    }

    nl.options.extend(temp_card);
    crate::subckt::mark_multiplied(&mut top);
    let option = |name: &str| {
        nl.options
            .iter()
            .rev()
            .find_map(|p| match (&p.value, p.name == name) {
                (ParamValue::Num(v), true) => Some(*v),
                _ => None,
            })
    };
    let scale = match option("scale") {
        None => 1.0,
        Some(s) if s.is_finite() && s > 0.0 => s,
        Some(s) => {
            let span = nl
                .options
                .iter()
                .rev()
                .find(|p| p.name == "scale")
                .map(|p| p.span);
            let msg = format!(".option scale={s} must be a positive number");
            all_diags.push(match span {
                Some(sp) => Diagnostic::error(msg, sp),
                None => Diagnostic::global_error(msg),
            });
            1.0
        }
    };
    let ctx = ExpandCtx {
        model_names: &model_names,
        vdmos_models: &vdmos_models,
        scale,
        wnflag: option("wnflag").is_some_and(|f| f != 0.0),
    };
    let (elements, models) = expand(&top, &env, &ctx, &mut all_diags);
    nl.elements = elements;
    nl.models = models;
    all_diags.extend(env.random_note());
    if let Some(at) = env.temper_first() {
        temper_checks(&nl, at, &env, &mut all_diags);
    }

    ParseOutcome {
        netlist: nl,
        diags: all_diags,
        sources,
    }
}

/// The circuit temperature `temper` reads (°C): the last `.temp` card
/// in effect, else the last `.options temp=`, else 27 — as the engine
/// will run (`dot.rs` `dot_temp`; probes `g9-probes t1 t_opt50_*`,
/// `t_temp75_*`, `t_temp_and_opt`, `t_opt50_after`, `t_temp_if`). A
/// quiet pre-pass over the top level: `.options` count wherever they
/// stand (they ignore `.if`, `i4 i_d_options_*`), a `.temp` only in a
/// branch taken; diagnostics are left to the real pass.
fn circuit_temperature(cards: &[Card], env: &ParamEnv) -> f64 {
    let mut scratch = Netlist::default();
    let mut temp_card: Option<Param> = None;
    let mut quiet = Vec::new();
    let mut conds = Conds::default();
    let mut act = Activity::default();
    let mut depth = 0usize;
    let mut in_control = false;
    for card in cards {
        let kw = first_keyword(card);
        if in_control {
            in_control = kw != ".endc";
            continue;
        }
        match kw.as_str() {
            ".control" => in_control = true,
            ".subckt" => depth += 1,
            ".ends" => depth = depth.saturating_sub(1),
            ".end" if depth == 0 => break,
            _ if depth > 0 => {}
            k if crate::cond::directive(k).is_some() => {
                if let Some(d) = crate::cond::directive(k)
                    && let Some(b) = conds.read(d, card, &mut quiet)
                {
                    conds.eval_branch(b, env, &mut act, &mut |_| {});
                }
            }
            ".options" | ".option" | ".opt" => {
                crate::dot::parse_dot(&kw, card, &mut quiet, env, &mut scratch, &mut temp_card);
            }
            ".temp" if act.on(conds.guard()) => {
                crate::dot::parse_dot(&kw, card, &mut quiet, env, &mut scratch, &mut temp_card);
            }
            _ => {}
        }
        quiet.clear();
    }
    let num = |p: &Param| match p.value {
        ParamValue::Num(v) if v.is_finite() => Some(v),
        _ => None,
    };
    temp_card
        .as_ref()
        .and_then(num)
        .or_else(|| {
            scratch
                .options
                .iter()
                .rev()
                .find(|p| p.name == "temp")
                .and_then(num)
        })
        .unwrap_or(27.0)
}

/// After a deck read `temper` (first at `at`): it was evaluated once,
/// at the circuit temperature. A `.dc temp` sweep would need the deck
/// re-expanded at every point — ngspice-47 re-evaluates such
/// expressions there (`g9-probes t2 t_dc_sweep_model`); the parser
/// here runs once, before the engine, so the sweep is refused. Any
/// later change of the temperature (an engine-option override from a
/// binding) does not reach `temper` either: a warning says so.
fn temper_checks(nl: &Netlist, at: crate::span::Span, env: &ParamEnv, diags: &mut Vec<Diagnostic>) {
    for a in &nl.analyses {
        if let Analysis::Dc { sweeps, span } = a
            && sweeps.iter().any(|s| s.src.eq_ignore_ascii_case("temp"))
        {
            diags.push(Diagnostic::error(
                "a .dc temp sweep cannot be combined with `temper`: Dune evaluates `temper` \
                 once, when the netlist is read, and re-expanding the deck at every \
                 temperature point is not supported yet",
                *span,
            ));
        }
    }
    let t = env.temperature();
    diags.push(Diagnostic::warning(
        format!(
            "`temper` is evaluated once, at the circuit temperature {t} °C; changing the \
             temperature after the netlist is read does not re-evaluate it"
        ),
        at,
    ));
}

/// Record the name a `.func` (or function-form `.param`) card defines.
fn func_name(card: &Card, names: &mut HashSet<String>) {
    let mut sc = Scanner::new(card);
    sc.next_tok();
    if let Some(t) = sc.next_tok() {
        let n = t.lc();
        let n = n.split('(').next().unwrap_or_default();
        if !n.is_empty() {
            names.insert(n.to_string());
        }
    }
}

/// `.param name=value [,] name=value …`: collect the definitions of
/// one card (evaluated later, all at once — forward references are
/// legal). See [`collect_assignments`].
fn collect_params<'c>(
    card: &'c Card,
    defs: &mut Vec<ParamDef<'c>>,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let b = card.text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    // Skip the keyword (directives may be indented).
    let mut p = 0usize;
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    while p < b.len() && !blank(b[p]) {
        p += 1;
    }
    collect_assignments(card, p, defs, diags)
}

/// `name=value [,] name=value …` from byte `p` of a card (`.param`,
/// the tail of a `.subckt` or `X` line). A value is `{expr}`,
/// `'expr'` or a bare expression that runs to the first blank or `,`
/// outside parentheses (ngspice's `inp_split_multi_param_lines` cuts
/// there). Text after a value that does not start the next `name=` is
/// an error: ngspice silently drops it on a multi-assignment line
/// (`.param b = a * 3 c=1` gives `b = a`) and rejects it on a single
/// one. Returns `true` when the [`crate::param::MAX_PARAMS`] budget
/// is spent.
pub(crate) fn collect_assignments<'c>(
    card: &'c Card,
    mut p: usize,
    defs: &mut Vec<ParamDef<'c>>,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let text = card.text.as_str();
    let b = text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let first = defs.len();
    loop {
        while p < b.len() && (blank(b[p]) || b[p] == b',') {
            p += 1;
        }
        if p >= b.len() {
            return false;
        }
        // `params:` may separate the ports from the parameters.
        if b.len() - p >= 7 && b[p..p + 7].eq_ignore_ascii_case(b"params:") {
            p += 7;
            continue;
        }
        // Name.
        let ns = p;
        if b[p].is_ascii_alphabetic() || b[p] == b'_' {
            while p < b.len() && ident(b[p]) {
                p += 1;
            }
        }
        let ne = p;
        let mut q = p;
        while q < b.len() && blank(b[q]) {
            q += 1;
        }
        let assigned = q < b.len() && b[q] == b'=' && b.get(q + 1) != Some(&b'=');
        if ns == ne || !assigned {
            // Not `name =`: either junk where a name belongs or text
            // left over after the previous value.
            let mut e = ns.max(ne);
            while e < b.len() && !blank(b[e]) {
                e += 1;
            }
            let e = e.max(ns + 1).min(b.len());
            let word = &text[ns..e];
            let msg = if defs.len() > first {
                format!(
                    "unexpected `{word}` after a parameter value (an expression with \
                     blanks must be written as {{…}} or '…')"
                )
            } else if ns == ne {
                format!("`{word}` cannot be a parameter name")
            } else {
                format!("parameter `{word}` has no value")
            };
            diags.push(Diagnostic::error(msg, card.span_of(ns, e)));
            return false;
        }
        let name = text[ns..ne].to_ascii_lowercase();
        let name_span = card.span_of(ns, ne);
        if name == "temper" {
            // ngspice-47 ignores such a definition and keeps reading
            // the circuit temperature (`g9-probes t1 t_user_param`).
            diags.push(Diagnostic::error(
                "`temper` is the circuit temperature and cannot be defined as a parameter \
                 (ngspice-47 ignores this definition)",
                name_span,
            ));
            return false;
        }
        p = q + 1;
        while p < b.len() && blank(b[p]) {
            p += 1;
        }
        let vs = p;
        let (value_start, value_end) = match b.get(p) {
            None => {
                diags.push(Diagnostic::error(
                    format!("parameter `{name}` has no value"),
                    name_span,
                ));
                return false;
            }
            Some(b'{') => match crate::scan::match_brace(b, p) {
                Some(end) => {
                    p = end;
                    (vs + 1, p - 1)
                }
                None => {
                    diags.push(Diagnostic::error(
                        format!("missing `}}` in the value of parameter `{name}`"),
                        card.span_of(vs, vs + 1),
                    ));
                    return false;
                }
            },
            Some(b'\'') => match b[p + 1..].iter().position(|&c| c == b'\'') {
                Some(k) => {
                    p = p + 1 + k + 1;
                    (vs + 1, p - 1)
                }
                None => {
                    diags.push(Diagnostic::error(
                        format!("missing closing `'` in the value of parameter `{name}`"),
                        card.span_of(vs, vs + 1),
                    ));
                    return false;
                }
            },
            Some(b'"') => {
                diags.push(Diagnostic::error(
                    format!("string-valued parameter `{name}` is not supported"),
                    card.span_of(vs, vs + 1),
                ));
                return false;
            }
            Some(_) => {
                let mut depth = 0usize;
                while p < b.len() {
                    match b[p] {
                        b' ' | b'\t' | b',' if depth == 0 => break,
                        b'(' => depth += 1,
                        b')' if depth > 0 => depth -= 1,
                        b'{' => {
                            p = crate::scan::skip_brace(b, p);
                            continue;
                        }
                        _ => {}
                    }
                    p += 1;
                }
                (vs, p)
            }
        };
        if defs.len() >= crate::param::MAX_PARAMS {
            diags.push(Diagnostic::error(
                format!(
                    "more than {} global .param definitions; the rest are ignored",
                    crate::param::MAX_PARAMS
                ),
                name_span,
            ));
            return true;
        }
        defs.push(ParamDef {
            name,
            name_span,
            card,
            value_start,
            value: &text[value_start..value_end],
            temper_ok: true,
        });
    }
}

/// `.model MNAME TYPE(PNAME1=PVAL1 …)`.
pub(crate) fn parse_model(
    card: &Card,
    diags: &mut Vec<Diagnostic>,
    env: &ParamEnv,
) -> Option<Model> {
    let span = card.span();
    let mut ctx = ElemCtx {
        sc: Scanner::new(card),
        card,
        diags,
        env,
        replaced_m: false,
    };
    ctx.sc.next_tok(); // `.model`
    let (name, _) = ctx.tok_req("model name")?;
    let (mtype, tspan) = ctx.tok_req("model type")?;
    if !MODEL_TYPES.contains(&mtype.as_str()) {
        ctx.diags.push(Diagnostic::error(
            format!("unknown model type `{mtype}`"),
            tspan,
        ));
        return None;
    }
    // Flags are legitimate model parameters (the LTRA model has
    // `nocontrol`, `steplimit`, `quadinterp`, …), so nothing to
    // check here; dune-core validates against each model's table.
    let params = ctx.keyword_params();
    Some(Model {
        name,
        mtype,
        params,
        span,
    })
}
