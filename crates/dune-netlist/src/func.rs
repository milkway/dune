//! User functions: `.func name(args) body` and its synonym
//! `.param name(args) = body` (ngspice extensions; the 3f5 has none).
//!
//! ngspice-47 implements them as a textual macro expansion before the
//! parameter evaluator sees the line (`frontend/inpcom.c`, Modified
//! BSD: `inp_get_func_from_line`, `inp_expand_macros_in_deck`; the
//! `.param` form is rewritten by `inp_fix_macro_param_func_paren_io`).
//! Dune evaluates calls instead (`pexpr::Expr::eval_in`), with the
//! observable rules of that expansion (probes `f1` in
//! `.claude/scratch/g9-probes/`, cited in `docs/dune-netlist.md`):
//!
//! - functions are collected per `.subckt` nesting level before any
//!   line is expanded: a call may precede the definition, and a body
//!   may call a function defined after it (`f_after_use`,
//!   `f_in_func_fwd`);
//! - the scope is lexical: a function defined inside a `.subckt` is
//!   seen by that body and by subcircuits defined inside it, not by a
//!   subcircuit it merely instantiates, nor outside (`f_local_leak`,
//!   `f_local_lexical`, `f_local_nested_def`); an inner definition
//!   shadows an outer one, and a user function shadows a built-in
//!   (`f_local_shadow`, `f_shadow_builtin`);
//! - a redefinition replaces the earlier one (`f_redef`);
//! - formal arguments shadow parameters (`f_shadow_param`); every other
//!   name in a body is read where the call is, since the text lands
//!   there (`f_local_sees_inst_param`, `f_glob_param_after`);
//! - an unused body is never evaluated: an undefined name or a syntax
//!   error in it only matters when it is called (`f_syntax_unused`,
//!   `f_undef_in_body_unused`).
//!
//! Recursion, which crashes ngspice-47 (segmentation fault, even
//! unused — `f_recursive*`), is an error with a span here, found when
//! the definitions of a scope are complete.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::pexpr::{self, EvalCtx, Expr, PError};
use crate::span::Span;

/// Most formal arguments of one function (ngspice's `N_PARAMS`).
const MAX_FORMALS: usize = 1000;
/// Longest dependency chain printed in a recursion diagnostic.
const CYCLE_NAMES_SHOWN: usize = 8;

/// One user function, compiled.
#[derive(Debug)]
pub(crate) struct UserFn<'c> {
    /// Lowercased.
    pub name: String,
    /// Lowercased formal argument names.
    pub formals: Vec<Box<str>>,
    /// The compiled body, or why it does not compile (offsets in the
    /// body text), reported only when the function is called.
    pub body: Result<Expr, PError>,
    pub name_span: Span,
    card: &'c Card,
    /// Byte range of the body inside `card.text`.
    body_start: usize,
    body_end: usize,
    /// Set when the function is part of a recursion (already reported).
    pub recursive: Cell<bool>,
}

impl UserFn<'_> {
    /// Span of bytes `s..e` of the body text.
    pub fn span_of(&self, s: usize, e: usize) -> Span {
        self.card.span_of(self.body_start + s, self.body_start + e)
    }

    /// The body as written (empty when the definition has none).
    pub fn body_text(&self) -> &str {
        &self.card.text[self.body_start..self.body_end]
    }
}

/// The functions defined at one nesting level (the top level, or one
/// `.subckt` body).
#[derive(Debug, Default)]
pub(crate) struct FuncTable<'c> {
    map: HashMap<String, UserFn<'c>>,
}

impl<'c> FuncTable<'c> {
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Add a definition; a later one with the same name replaces it
    /// (probe `f_redef`, `f_redef_arity_old`).
    pub fn insert(&mut self, f: UserFn<'c>) {
        self.map.insert(f.name.clone(), f);
    }

    /// Refuse recursion: a function that reaches itself through calls
    /// resolved in this table (a call inside a body resolves in the
    /// defining scope first, so a cycle cannot leave the table). Each
    /// cycle is reported once, at the call that closes it, and its
    /// members are marked. Iterative, linear in the calls.
    pub fn check_recursion(&self, diags: &mut Vec<Diagnostic>) {
        let mut names: Vec<&str> = self.map.keys().map(String::as_str).collect();
        names.sort_unstable(); // deterministic diagnostics
        let index: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        let fns: Vec<&UserFn> = names.iter().map(|n| &self.map[*n]).collect();
        // Callees in this table, by index, with the call's position.
        let edges: Vec<Vec<(usize, usize)>> = fns
            .iter()
            .map(|f| match &f.body {
                Ok(b) => b
                    .calls()
                    .iter()
                    .enumerate()
                    .filter_map(|(k, c)| index.get(&*c.name).map(|&j| (j, k)))
                    .collect(),
                Err(_) => Vec::new(),
            })
            .collect();
        const UNSEEN: u8 = 0;
        const OPEN: u8 = 1;
        const DONE: u8 = 2;
        let mut state = vec![UNSEEN; fns.len()];
        let mut stack_pos = vec![usize::MAX; fns.len()];
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for root in 0..fns.len() {
            if state[root] != UNSEEN {
                continue;
            }
            state[root] = OPEN;
            stack_pos[root] = 0;
            stack.push((root, 0));
            while let Some(&(n, k)) = stack.last() {
                if k < edges[n].len() {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                    let (d, call) = edges[n][k];
                    match state[d] {
                        UNSEEN => {
                            state[d] = OPEN;
                            stack_pos[d] = stack.len();
                            stack.push((d, 0));
                        }
                        OPEN => {
                            let from = stack_pos[d];
                            let members: Vec<usize> = stack[from..].iter().map(|t| t.0).collect();
                            if !members.iter().any(|&m| fns[m].recursive.get()) {
                                let mut chain: Vec<&str> = members
                                    .iter()
                                    .take(CYCLE_NAMES_SHOWN)
                                    .map(|&m| fns[m].name.as_str())
                                    .collect();
                                if members.len() > CYCLE_NAMES_SHOWN {
                                    chain.push("…");
                                }
                                chain.push(fns[d].name.as_str());
                                let span = match &fns[n].body {
                                    Ok(b) => {
                                        let c = &b.calls()[call];
                                        fns[n].span_of(c.start as usize, c.end as usize)
                                    }
                                    Err(_) => fns[n].name_span,
                                };
                                diags.push(Diagnostic::error(
                                    format!(
                                        "recursive function (ngspice-47 crashes on it): {}",
                                        chain.join(" → ")
                                    ),
                                    span,
                                ));
                            }
                            for m in members {
                                fns[m].recursive.set(true);
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                stack.pop();
                state[n] = DONE;
            }
        }
    }
}

/// The functions visible at one place: a table and the scopes that
/// enclose it lexically.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FuncScope<'a> {
    pub table: &'a FuncTable<'a>,
    pub parent: Option<&'a FuncScope<'a>>,
}

impl<'a> FuncScope<'a> {
    /// The function `name` resolves to here, and the scope it was
    /// defined in (where the calls in its body resolve).
    pub fn find<'b>(&'b self, name: &str) -> Option<(&'b UserFn<'b>, &'b FuncScope<'b>)> {
        let mut sc: &'b FuncScope<'b> = self;
        loop {
            if let Some(f) = sc.table.map.get(name) {
                return Some((f, sc));
            }
            sc = sc.parent?;
        }
    }

    /// Whether no function is visible here.
    fn is_empty(&self) -> bool {
        let mut sc = self;
        loop {
            if !sc.table.is_empty() {
                return false;
            }
            match sc.parent {
                Some(p) => sc = p,
                None => return true,
            }
        }
    }

    /// The names `expr` reads through the user functions it calls,
    /// transitively — the other names of their bodies, which resolve
    /// where `expr` is evaluated — appended to `out`. Each function is
    /// visited once; the walk is charged to the user-function budget
    /// of `ctx` and stops (incomplete) when it is spent.
    pub fn free_names(&self, expr: &Expr, ctx: &EvalCtx, out: &mut Vec<Box<str>>) {
        if self.is_empty() || expr.calls().is_empty() {
            return;
        }
        let mut seen: HashSet<*const UserFn> = HashSet::new();
        let mut todo: Vec<(&UserFn, &FuncScope)> = expr
            .calls()
            .iter()
            .filter_map(|c| self.find(&c.name))
            .collect();
        while let Some((f, sc)) = todo.pop() {
            if !seen.insert(std::ptr::from_ref(f)) {
                continue;
            }
            let Ok(body) = &f.body else { continue };
            if !ctx.charge(1 + (body.vars().len() + body.calls().len()) as u64) {
                return;
            }
            out.extend(
                body.vars()
                    .iter()
                    .filter(|v| !f.formals.iter().any(|a| **a == *v.name))
                    .map(|v| v.name.clone()),
            );
            todo.extend(
                body.calls()
                    .iter()
                    .filter_map(|c| sc.find(&c.name).or_else(|| self.find(&c.name))),
            );
        }
    }
}

/// Whether a `.param` card defines a function instead: its first
/// token after the keyword (up to a blank or `=`) holds a `(`
/// (`inp_fix_macro_param_func_paren_io`; probe `f_params_kw`).
pub(crate) fn is_param_function(card: &Card) -> bool {
    let b = card.text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    let mut p = 0;
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    while p < b.len() && !blank(b[p]) {
        p += 1;
    }
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    while p < b.len() && !blank(b[p]) && b[p] != b'=' {
        if b[p] == b'(' {
            return true;
        }
        p += 1;
    }
    false
}

/// Read `.func name(a, b, …) [=] body` (or `.param name(a, …) = body`)
/// from a card. The forms ngspice-47 rejects — no parentheses, an
/// empty argument (`f(x,)`), an unclosed list — are errors, as there
/// (`f_noargs_def`, `f_comma_empty`, `f_unbalanced`); so are a
/// duplicated formal (ngspice silently uses the first,
/// `f_arg_same_names`) and a missing body (ngspice accepts the line,
/// `f_body_empty`). The body is everything after the `)` and an
/// optional `=` (`f_eq`, `f_bare`, `f_body_two_tokens`); `{…}` and
/// `'…'` in it group like parentheses.
pub(crate) fn parse_def<'c>(card: &'c Card, diags: &mut Vec<Diagnostic>) -> Option<UserFn<'c>> {
    let text = card.text.as_str();
    let b = text.as_bytes();
    let blank = |c: u8| c == b' ' || c == b'\t';
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut p = 0;
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    while p < b.len() && !blank(b[p]) {
        p += 1;
    }
    let err = |msg: String, s: usize, e: usize, diags: &mut Vec<Diagnostic>| {
        diags.push(Diagnostic::error(
            msg,
            card.span_of(s, e.max(s + 1).min(b.len().max(s + 1))),
        ));
        None
    };
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    let ns = p;
    if p < b.len() && (b[p].is_ascii_alphabetic() || b[p] == b'_') {
        while p < b.len() && ident(b[p]) {
            p += 1;
        }
    }
    let ne = p;
    if ns == ne {
        return err(
            "`.func` needs a function name: `.func name(args) {body}`".into(),
            ns,
            ns + 1,
            diags,
        );
    }
    let name = text[ns..ne].to_ascii_lowercase();
    let name_span = card.span_of(ns, ne);
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    if b.get(p) != Some(&b'(') {
        return err(
            format!("function `{name}` needs its argument list in parentheses: `{name}(args)`"),
            ns,
            ne,
            diags,
        );
    }
    let lp = p;
    p += 1;
    let mut formals: Vec<Box<str>> = Vec::new();
    loop {
        while p < b.len() && blank(b[p]) {
            p += 1;
        }
        if b.get(p) == Some(&b')') && formals.is_empty() {
            break;
        }
        let s = p;
        if p < b.len() && (b[p].is_ascii_alphabetic() || b[p] == b'_') {
            while p < b.len() && ident(b[p]) {
                p += 1;
            }
        }
        if s == p {
            return err(
                format!("malformed argument list of function `{name}`: a name was expected"),
                s,
                s + 1,
                diags,
            );
        }
        let a = text[s..p].to_ascii_lowercase();
        if formals.iter().any(|f| **f == *a) {
            return err(
                format!("function `{name}` names its argument `{a}` twice"),
                s,
                p,
                diags,
            );
        }
        if formals.len() >= MAX_FORMALS {
            return err(
                format!("function `{name}` has more than {MAX_FORMALS} arguments"),
                s,
                p,
                diags,
            );
        }
        formals.push(a.into_boxed_str());
        while p < b.len() && blank(b[p]) {
            p += 1;
        }
        match b.get(p) {
            Some(b',') => p += 1,
            Some(b')') => break,
            _ => {
                return err(
                    format!("the argument list of function `{name}` is not closed by `)`"),
                    lp,
                    lp + 1,
                    diags,
                );
            }
        }
    }
    p += 1; // `)`
    while p < b.len() && blank(b[p]) {
        p += 1;
    }
    if b.get(p) == Some(&b'=') {
        p += 1;
        while p < b.len() && blank(b[p]) {
            p += 1;
        }
    }
    let mut end = b.len();
    while end > p && blank(b[end - 1]) {
        end -= 1;
    }
    let body = if p == end {
        Err(PError::new(format!("function `{name}` has no body"), 0, 0))
    } else {
        pexpr::parse(&text[p..end])
    };
    Some(UserFn {
        name,
        formals,
        body,
        name_span,
        card,
        body_start: p,
        body_end: end,
        recursive: Cell::new(false),
    })
}
