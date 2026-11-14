//! Conditional blocks: `.if (cond)`, `.elseif (cond)`, `.else`,
//! `.endif` — an ngspice extension used by
//! the IHP SG13G2 libraries inside `.subckt` bodies (G9 of
//! `docs/pdk-parser.md`).
//!
//! ngspice-47 evaluates them in its parameter preprocessor (LGPL,
//! not consulted); the contract here is its observed behaviour (probes
//! `i1`–`i3` in `.claude/scratch/g9-probes/`, cited in
//! `docs/dune-netlist.md`, "Condicionais"):
//!
//! - the condition is a parameter expression in parentheses, true when
//!   nonzero; at the top level it reads the global parameters, inside a
//!   `.subckt` the instance's scope (so each instance picks its own
//!   branch);
//! - every condition of a scope that exists is evaluated, also in
//!   branches not taken and after the taken one (an undefined name
//!   there is an error, `i_undef_untaken_elseif`,
//!   `i_nested_untaken_undef`);
//! - elements, `X` lines, `.model` cards and the other control cards
//!   (`.temp`, analyses) are conditional; `.param`, `.func`,
//!   `.include`/`.lib`, `.options` and `.subckt` definitions are not
//!   (`i_param_in_branch`, `i_func_in`, `i_include_in`,
//!   `i_options_else`, `i_subckt_def_false`); a `.subckt` body inside a
//!   branch is not affected by it.
//!
//! The block structure is read once; [`Conds::evaluate`] picks the
//! active branches per scope (per instance in a subcircuit). Nothing
//! recurses: the open blocks are a stack, the branches a flat list in
//! card order whose parents precede them.

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::param::ParamEnv;
use crate::span::Span;

/// Deepest accepted nesting of `.if` blocks in one scope.
pub(crate) const MAX_IF_DEPTH: usize = 256;

/// The four directives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Directive {
    If,
    ElseIf,
    Else,
    EndIf,
}

/// The directive a card keyword names, if any.
pub(crate) fn directive(kw: &str) -> Option<Directive> {
    // `.if(a==1)` is one token (`i_glued`).
    let kw = kw.split('(').next().unwrap_or_default();
    Some(match kw {
        ".if" => Directive::If,
        ".elseif" => Directive::ElseIf,
        ".else" => Directive::Else,
        ".endif" => Directive::EndIf,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug)]
enum Cond<'c> {
    /// `.else`.
    Always,
    /// The condition could not be read (already reported): false.
    Bad,
    /// `card.text[start..end]` is the parenthesized condition.
    Expr {
        card: &'c Card,
        start: usize,
        end: usize,
    },
}

/// One branch: the body of an `.if`, `.elseif` or `.else`.
#[derive(Debug)]
struct Branch<'c> {
    /// The branch enclosing the whole chain, if any.
    parent: Option<u32>,
    cond: Cond<'c>,
    /// The chain's first branch (its `.if`).
    chain: u32,
}

#[derive(Debug)]
struct Open {
    chain: u32,
    current: u32,
    seen_else: bool,
    span: Span,
}

/// The conditional structure of one scope (the top level, or one
/// `.subckt` body).
#[derive(Debug, Default)]
pub(crate) struct Conds<'c> {
    branches: Vec<Branch<'c>>,
    open: Vec<Open>,
}

/// Which branches of a [`Conds`] are active in one scope.
#[derive(Debug, Default)]
pub(crate) struct Activity {
    active: Vec<bool>,
    /// By chain (its first branch): whether a branch was taken.
    taken: Vec<bool>,
}

impl Activity {
    /// Whether a card under `guard` (the innermost branch holding it)
    /// is in effect.
    pub fn on(&self, guard: Option<u32>) -> bool {
        guard.is_none_or(|g| self.active.get(g as usize).copied().unwrap_or(false))
    }
}

impl<'c> Conds<'c> {
    /// The innermost open branch: the guard of a card read now.
    pub fn guard(&self) -> Option<u32> {
        self.open.last().map(|o| o.current)
    }

    /// Number of branches (each costs one evaluation per scope).
    pub fn len(&self) -> usize {
        self.branches.len()
    }

    /// Read one directive card into the structure; returns the branch
    /// it opens, if any. Malformed or unbalanced directives are errors
    /// (`i_endif_alone`, `i_else_alone`, `i_else_twice`,
    /// `i_elseif_after_else`), and the structure stays usable.
    pub fn read(
        &mut self,
        d: Directive,
        card: &'c Card,
        diags: &mut Vec<Diagnostic>,
    ) -> Option<u32> {
        let text = card.text.as_str();
        let b = text.as_bytes();
        let blank = |c: u8| c == b' ' || c == b'\t';
        let mut p = 0;
        while p < b.len() && blank(b[p]) {
            p += 1;
        }
        let kw_start = p;
        while p < b.len() && !blank(b[p]) && b[p] != b'(' {
            p += 1;
        }
        let kw_span = card.span_of(kw_start, p);
        let name = match d {
            Directive::If => ".if",
            Directive::ElseIf => ".elseif",
            Directive::Else => ".else",
            Directive::EndIf => ".endif",
        };
        let rest_blank = |from: usize| b[from.min(b.len())..].iter().all(|&c| blank(c));
        match d {
            Directive::If | Directive::ElseIf => {
                let cond = read_cond(card, p, name, diags);
                let idx = self.branches.len() as u32;
                if d == Directive::If {
                    if self.open.len() == MAX_IF_DEPTH {
                        diags.push(Diagnostic::error(
                            format!(".if blocks nested deeper than {MAX_IF_DEPTH} levels"),
                            kw_span,
                        ));
                    }
                    self.branches.push(Branch {
                        parent: self.guard(),
                        cond,
                        chain: idx,
                    });
                    self.open.push(Open {
                        chain: idx,
                        current: idx,
                        seen_else: false,
                        span: kw_span,
                    });
                    return Some(idx);
                }
                let Some(top) = self.open.last_mut() else {
                    diags.push(Diagnostic::error(".elseif without .if", kw_span));
                    return None;
                };
                if top.seen_else {
                    diags.push(Diagnostic::error(".elseif after .else", kw_span));
                    return None;
                }
                let chain = top.chain;
                top.current = idx;
                let parent = self.branches[chain as usize].parent;
                self.branches.push(Branch {
                    parent,
                    cond,
                    chain,
                });
                Some(idx)
            }
            Directive::Else => {
                if !rest_blank(p) {
                    // ngspice-47 ignores the text: `.else if (c)` is a
                    // plain `.else`, its condition never read
                    // (`i5 i_else_if_false_cond`, `i_else_if_then_else`).
                    let mut q = p;
                    while q < b.len() && blank(b[q]) {
                        q += 1;
                    }
                    let msg = if b.len() >= q + 2 && b[q..q + 2].eq_ignore_ascii_case(b"if") {
                        "`.else if` is a plain `.else` in ngspice-47 (the condition is ignored); \
                         write `.elseif (…)`"
                    } else {
                        "unexpected text after .else (ngspice-47 ignores it)"
                    };
                    diags.push(Diagnostic::error(msg, card.span_of(p, b.len())));
                }
                let idx = self.branches.len() as u32;
                let Some(top) = self.open.last_mut() else {
                    diags.push(Diagnostic::error(".else without .if", kw_span));
                    return None;
                };
                if top.seen_else {
                    diags.push(Diagnostic::error(
                        "a second .else in one .if block",
                        kw_span,
                    ));
                    return None;
                }
                top.seen_else = true;
                let chain = top.chain;
                top.current = idx;
                let parent = self.branches[chain as usize].parent;
                self.branches.push(Branch {
                    parent,
                    cond: Cond::Always,
                    chain,
                });
                Some(idx)
            }
            Directive::EndIf => {
                if !rest_blank(p) {
                    diags.push(Diagnostic::error(
                        "unexpected text after .endif (ngspice-47 ignores it)",
                        card.span_of(p, b.len()),
                    ));
                }
                if self.open.pop().is_none() {
                    diags.push(Diagnostic::error(".endif without .if", kw_span));
                }
                None
            }
        }
    }

    /// End of the scope: every `.if` still open is an error (ngspice-47
    /// silently closes it at the end of the deck or `.subckt`,
    /// `i_unclosed`, `i_sub_unclosed`).
    pub fn finish(&mut self, scope: &str, diags: &mut Vec<Diagnostic>) {
        for o in self.open.drain(..).rev() {
            diags.push(Diagnostic::error(
                format!(".if without .endif before the end of {scope}"),
                o.span,
            ));
        }
    }

    /// Evaluate branch `b` in `env` (its parent's state already in
    /// `act`). The condition is evaluated even when the branch cannot
    /// be taken, as ngspice does: its errors count.
    pub fn eval_branch(
        &self,
        b: u32,
        env: &ParamEnv,
        act: &mut Activity,
        report: &mut dyn FnMut(Diagnostic),
    ) {
        let Some(br) = self.branches.get(b as usize) else {
            return;
        };
        if act.active.len() < self.branches.len() {
            act.active.resize(self.branches.len(), false);
            act.taken.resize(self.branches.len(), false);
        }
        let truth = match br.cond {
            Cond::Always => true,
            Cond::Bad => false,
            Cond::Expr { card, start, end } => {
                match env.eval_value(&card.text[start..end], &|s, e| {
                    card.span_of(start + s, start + e)
                }) {
                    Ok(v) => v != 0.0,
                    Err(d) => {
                        if let Some(d) = d {
                            report(d);
                        }
                        false
                    }
                }
            }
        };
        let parent_on = br.parent.is_none_or(|p| act.active[p as usize]);
        let on = parent_on && !act.taken[br.chain as usize] && truth;
        act.active[b as usize] = on;
        if on {
            act.taken[br.chain as usize] = true;
        }
    }

    /// Evaluate every branch in `env`, in card order.
    pub fn evaluate(&self, env: &ParamEnv, report: &mut dyn FnMut(Diagnostic)) -> Activity {
        let mut act = Activity::default();
        for b in 0..self.branches.len() {
            self.eval_branch(b as u32, env, &mut act, report);
        }
        act
    }
}

/// Read the parenthesized condition starting at byte `p` (after the
/// keyword): ngspice-47 requires the parentheses (`.if a==1`,
/// `.if {a==1}` and a bare `.if` are fatal "Bad syntax", `i_noparen`,
/// `i_brace`, `i_no_cond`); text after the closing parenthesis is an
/// error here (ngspice ignores it, `i_cond_trailing`).
fn read_cond<'c>(
    card: &'c Card,
    mut p: usize,
    name: &str,
    diags: &mut Vec<Diagnostic>,
) -> Cond<'c> {
    let b = card.text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    if b.get(p) != Some(&b'(') {
        let end = b.len().max(p + 1);
        diags.push(Diagnostic::error(
            format!("{name} needs its condition in parentheses: `{name} (expression)`"),
            card.span_of(p.min(b.len()), end.min(b.len()).max(p.min(b.len()))),
        ));
        return Cond::Bad;
    }
    let start = p;
    let mut depth = 0usize;
    let mut end = None;
    for (i, &c) in b.iter().enumerate().skip(p) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        diags.push(Diagnostic::error(
            format!("the condition of {name} is not closed by `)`"),
            card.span_of(start, start + 1),
        ));
        return Cond::Bad;
    };
    if b[end..].iter().any(|&c| !blank(c)) {
        diags.push(Diagnostic::error(
            format!(
                "unexpected text after the condition of {name} (ngspice-47 ignores it; write \
                 the whole condition inside one pair of parentheses)"
            ),
            card.span_of(end, b.len()),
        ));
        return Cond::Bad;
    }
    Cond::Expr { card, start, end }
}
