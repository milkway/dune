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
use crate::param::ParamEnv;
use crate::scan::Scanner;
use crate::span::SourceMap;
use crate::subckt::{Scope, SubcktDef, expand};

/// 3f5 model types (`INPdomodel`): anything else is an error.
const MODEL_TYPES: &[&str] = &[
    "r", "c", "sw", "csw", "urc", "ltra", "d", "npn", "pnp", "njf", "pjf", "nmos", "pmos", "nmf",
    "pmf",
    // ngspice-47's power MOSFET (`inpdomod.c`; `vdmosn`/`vdmosp` are
    // the forms its front end rewrites `vdmos` into).
    "vdmos", "vdmosn", "vdmosp",
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

/// Parse a deck from a string; `.include` resolves through `loader`.
pub fn parse_with_loader(
    name: &Path,
    text: String,
    loader: &mut dyn IncludeLoader,
) -> ParseOutcome {
    parse_deck(deck::read_with_loader(name, text, loader))
}

/// Parse a deck from a string; `.include` resolves on the file
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
    let mut env = ParamEnv::default();
    let mut in_control = false;
    for card in &cards {
        let kw = first_keyword(card);
        if in_control {
            in_control = kw != ".endc";
            continue;
        }
        match kw.as_str() {
            ".control" => in_control = true,
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
            ".param" => collect_params(card, &mut env, &mut all_diags),
            _ => {}
        }
    }

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
            ".param" => {} // collected in pass A
            ".include" => {
                // A column-1 `.include` was spliced by the reader;
                // reaching here means it was indented.
                all_diags.push(Diagnostic::error(
                    ".include is only recognized at column 1",
                    span,
                ));
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

    let (elements, models) = expand(&top, &mut all_diags);
    nl.elements = elements;
    nl.models = models;

    ParseOutcome {
        netlist: nl,
        diags: all_diags,
        sources,
    }
}

/// `.param name=value …` — each value is a number or a basic
/// expression (one token, or a `{…}` token).
fn collect_params(card: &Card, env: &mut ParamEnv, diags: &mut Vec<Diagnostic>) {
    let mut sc = Scanner::new(card);
    sc.next_tok(); // `.param`
    while let Some(name_tok) = sc.next_tok() {
        let name = name_tok.lc();
        let nspan = sc.span(&name_tok);
        if crate::scan::evaluate(&name).is_some() {
            diags.push(Diagnostic::error(
                format!("`{name}` cannot be a parameter name"),
                nspan,
            ));
            return;
        }
        let value = if let Some((v, _)) = sc.try_value() {
            Some(v)
        } else if let Some(vtok) = sc.next_tok() {
            let text = vtok.lc();
            let vspan = sc.span(&vtok);
            let result = if text.starts_with('{') {
                env.eval_brace(&text)
            } else {
                env.eval_expr(&text)
            };
            match result {
                Ok(v) => Some(v),
                Err(e) => {
                    diags.push(Diagnostic::error(e, vspan));
                    None
                }
            }
        } else {
            diags.push(Diagnostic::error(
                format!("parameter `{name}` has no value"),
                nspan,
            ));
            None
        };
        if let Some(v) = value {
            env.define(&name, v);
        }
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
