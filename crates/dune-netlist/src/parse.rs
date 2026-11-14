//! Parse driver: deck → [`Netlist`] + diagnostics.
//!
//! Mirrors the 3f5's two passes (`inppas1.c` models first, then
//! `inppas2.c`), plus a structural pass of our own: `.control` blocks
//! are swallowed verbatim, `.subckt`/`.ends` build scopes that
//! `subckt::expand` flattens afterwards, and `.param` constants are
//! collected up front so any card can use `{…}` references.

use std::collections::HashSet;
use std::path::Path;

use crate::deck::{self, Card, Deck, IncludeLoader};
use crate::diag::Diagnostic;
use crate::elem::{ElemCtx, parse_element};
use crate::ir::{Model, Netlist};
use crate::param::{ParamDef, ParamEnv};
use crate::scan::Scanner;
use crate::span::SourceMap;
use crate::subckt::{Scope, SubcktDef, expand};

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
            // `.params` is ngspice's synonym (the IHP libraries use it).
            // Definitions inside `.subckt` belong to the subcircuit's
            // scope (G3, not supported yet: pass B reports them).
            ".param" | ".params" if subckt_depth == 0 && !params_full => {
                params_full = collect_params(card, &mut param_defs, &mut all_diags);
            }
            _ => {}
        }
    }
    let env = crate::param::resolve(&param_defs, &mut all_diags);
    drop(param_defs);

    // Pass B: the real parse.
    let mut top = Scope::default();
    let mut stack: Vec<SubcktDef> = Vec::new();
    let mut in_control = false;
    let mut saw_end = false;
    let mut after_end = 0usize;

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
        match kw.as_str() {
            ".control" => in_control = true,
            ".endc" => {
                all_diags.push(Diagnostic::error(".endc without .control", span));
            }
            ".end" => saw_end = true,
            ".subckt" => {
                let mut sc = Scanner::new(card);
                sc.next_tok();
                let Some(name) = sc.next_tok() else {
                    all_diags.push(Diagnostic::error(".subckt needs a name", span));
                    continue;
                };
                let name = name.lc();
                let mut ports = Vec::new();
                while let Some(t) = sc.next_tok() {
                    ports.push(t.lc());
                }
                stack.push(SubcktDef {
                    name,
                    ports,
                    scope: Scope::default(),
                    span,
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
                    Some(def) => {
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
                        let parent = match stack.last_mut() {
                            Some(outer) => &mut outer.scope,
                            None => &mut top,
                        };
                        parent.subckts.push(def);
                    }
                }
            }
            ".model" => {
                let scope = match stack.last_mut() {
                    Some(def) => &mut def.scope,
                    None => &mut top,
                };
                if let Some(model) = parse_model(card, &mut all_diags, &env) {
                    scope.models.push(model);
                }
            }
            ".param" | ".params" => {
                // Top-level ones were collected in pass A.
                if !stack.is_empty() {
                    all_diags.push(Diagnostic::error(
                        format!(
                            "{kw} inside .subckt is not supported yet (subcircuit \
                             parameter scope)"
                        ),
                        span,
                    ));
                }
            }
            k if k.starts_with('.') => {
                if !stack.is_empty() {
                    all_diags.push(Diagnostic::error(
                        format!("{k} is not allowed inside .subckt"),
                        span,
                    ));
                } else if !crate::dot::parse_dot(k, card, &mut all_diags, &env, &mut nl) {
                    all_diags.push(Diagnostic::error(
                        format!("unknown control card `{k}`"),
                        span,
                    ));
                }
            }
            "" => {}
            _ => {
                let scope = match stack.last_mut() {
                    Some(def) => &mut def.scope,
                    None => &mut top,
                };
                if let Some(el) =
                    parse_element(card, &mut all_diags, &model_names, &vdmos_models, &env)
                {
                    scope.elements.push(el);
                }
            }
        }
    }

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

    all_diags.extend(env.random_note());

    let (elements, models) = expand(&top, &mut all_diags);
    nl.elements = elements;
    nl.models = models;

    ParseOutcome {
        netlist: nl,
        diags: all_diags,
        sources,
    }
}

/// `.param name=value [,] name=value …`: collect the definitions of
/// one card (evaluated later, all at once — forward references are
/// legal). A value is `{expr}`, `'expr'` or a bare expression that
/// runs to the first blank or `,` outside parentheses (ngspice's
/// `inp_split_multi_param_lines` cuts there). Text after a value that
/// does not start the next `name=` is an error: ngspice silently
/// drops it on a multi-assignment line (`.param b = a * 3 c=1` gives
/// `b = a`) and rejects it on a single one.
fn collect_params<'c>(
    card: &'c Card,
    defs: &mut Vec<ParamDef<'c>>,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let text = card.text.as_str();
    let b = text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    // Skip the keyword (directives may be indented).
    let mut p = 0usize;
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    while p < b.len() && !blank(b[p]) {
        p += 1;
    }
    loop {
        while p < b.len() && (blank(b[p]) || b[p] == b',') {
            p += 1;
        }
        if p >= b.len() {
            return false;
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
            let msg = if defs.last().is_some_and(|d| std::ptr::eq(d.card, card)) {
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
        });
    }
}

/// `.model MNAME TYPE(PNAME1=PVAL1 …)`.
fn parse_model(card: &Card, diags: &mut Vec<Diagnostic>, env: &ParamEnv) -> Option<Model> {
    let span = card.span();
    let mut ctx = ElemCtx {
        sc: Scanner::new(card),
        card,
        diags,
        env,
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
