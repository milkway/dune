//! Behavioural R, C and L: ngspice-47's front-end rewriting of an
//! element whose value depends on the circuit (`inp_compat` in
//! `src/frontend/inpcom.c`, Modified BSD; "Copyright 1990 Regents of
//! the University of California"), done here with the same topology
//! and the same generated names, so rawfile vectors and internal
//! nodes match ngspice:
//!
//! ```text
//! Rxxx n1 n2 R = {eq}  →  bRxxx n1 n2 i = v(n1, n2)/(eq)        (m multiplies)
//! Cxxx n1 n2 C = {eq}  →  gCxxx Cxxx_int1 0 n2 n1 {m}e9
//!                          rCxxx Cxxx_int1 0 1e-9
//!                          cCxxx Cxxx_int1 Cxxx_int2 1e-6
//!                          vcurrmeasCxxx Cxxx_int2 0 0
//!                          rCxxxpar n2 n1 1e15
//!                          bCxxx n1 n2 i = 1e6 * i(vcurrmeasCxxx) * (eq)
//! Cxxx n1 n2 Q = {eq}  →  gCxxx n1 n2 Cxxx_int1 0 {m}
//!                          lCxxx Cxxx_int1 0 1
//!                          bCxxx 0 Cxxx_int1 i = (eq)
//! Lxxx n1 n2 L = {eq}  →  fLxxx Lxxx_int2 0 bLxxx -1
//!                          lLxxx Lxxx_int2 0 1
//!                          bLxxx n2 n1 v = v(Lxxx_int2) * (eq) / {m}
//! ```
//!
//! The rewriting applies when the text after the name and the two
//! nodes holds `v(`, `i(`, `time`, `temper` or `hertz`
//! (`b_transformation_wanted`) — also once the parameters that depend
//! on `temper` and the user functions are expanded, as ngspice tests
//! the expanded line ([`wanted`]) — or a `q=` on a capacitor. The
//! expression is the first `{…}` after the nodes as ngspice sees the
//! line ([`equation`]); `temper` in it is the circuit temperature,
//! user functions are expanded in place, and `tc1`/`tc2` become the B
//! source's temperature factor at that temperature ([`tc_factor`]),
//! all evaluated once (`parse.rs` `temper_checks`). ngspice then
//! hands the B lines to numparam with every parameter name in braces
//! (`inp_modify_exp`) and parses them with its B-source grammar
//! (`inpptree-parser.y`: `^` left-associative, unary minus looser
//! than `^`). Dune evaluates the parameters here and builds the
//! 3f5-grammar tree of `ir::Expr` directly, keeping ngspice's
//! structure; the ngspice functions without a 3f5 counterpart are
//! rewritten exactly where the values agree (`min`, `max`, `pwr`;
//! `docs/dune-netlist.md`), and the rest is refused with a spanned
//! diagnostic.

use std::cell::Cell;

use crate::deck::Card;
use crate::func::{FuncScope, UserFn};
use crate::ir::{Device, Element, Expr, ExprFunc, ExprOp, SourceKind};
use crate::param::ParamEnv;
use crate::span::Span;

/// `b_transformation_wanted`: `v(`, `i(`, or the words `time`,
/// `temper`, `hertz`, not preceded by an identifier character.
pub(crate) fn transformation_wanted(rest: &str) -> bool {
    let b = rest.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    for i in 0..b.len() {
        if i > 0 && ident(b[i - 1]) {
            continue;
        }
        let tail = &b[i..];
        let lower = |s: &[u8]| tail.len() >= s.len() && tail[..s.len()].eq_ignore_ascii_case(s);
        if lower(b"v(") || lower(b"i(") {
            return true;
        }
        for w in [&b"temper"[..], b"hertz", b"time"] {
            if lower(w) && !tail.get(w.len()).copied().is_some_and(ident) {
                return true;
            }
        }
    }
    false
}

/// The text of a card after its name and two nodes (`None` with
/// fewer tokens).
pub(crate) fn after_nodes(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = 0;
    for _ in 0..3 {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b',' | b'(' | b')') {
            i += 1;
        }
        if i >= b.len() {
            return None;
        }
        while i < b.len() && !matches!(b[i], b' ' | b'\t' | b',' | b'(' | b')') {
            i += 1;
        }
    }
    Some(i)
}

/// Where the behavioural expression of a card is: the inner text
/// `start..end` and the outer range `lo..hi` (with its delimiters),
/// and the keyword whose value it is (`None` when positional).
pub(crate) struct Equation {
    pub start: usize,
    pub end: usize,
    pub lo: usize,
    pub hi: usize,
    pub keyword: Option<String>,
}

/// The behavioural expression of `text`, searched from byte `at` (the
/// end of the second node) as ngspice-47 finds it: `inp_compat` takes
/// the first `{` of the line — after `inp_change_quotes` made every
/// `'…'` a `{…}` and `inp_fix_param_values` wrapped every bare value
/// after an `=` in braces (one that starts with a digit, `.`, a signed
/// number, `{`, `true` or `false` is left alone; the value runs to the
/// first blank outside parentheses). So in `rb a b rm l=r_l r='…'` the
/// expression is `r_l`, not the `r=` value (probes `behav-probes b3
/// r_kw_bare_first`, `r_kw_bare_expr`, `r_kw_bare_paren`,
/// `r_kw_brace_first`; the gf180 `ppolyf_u` body resistor is written
/// so). `Ok(None)`: no expression; `Err`: an unclosed one, or a
/// vector/complex value (`[…]`, `<…>`) before it.
pub(crate) fn equation(text: &str, at: usize) -> Result<Option<Equation>, BError> {
    let b = text.as_bytes();
    // Checked before anything is scanned (principle 4).
    if b.len().saturating_sub(at) > crate::pexpr::MAX_LEN {
        return Err(berr(
            format!(
                "behavioural R/C/L card too long (more than {} bytes after the nodes)",
                crate::pexpr::MAX_LEN
            ),
            0,
            b.len().saturating_sub(at),
        ));
    }
    let blank = |c: u8| c == b' ' || c == b'\t';
    // The identifier right before a `=` at `q` (blanks skipped).
    let keyword_before = |q: usize| -> Option<String> {
        let mut e = q;
        while e > at && blank(b[e - 1]) {
            e -= 1;
        }
        let mut s = e;
        while s > at && (b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_') {
            s -= 1;
        }
        (s < e).then(|| text[s..e].to_ascii_lowercase())
    };
    // The keyword of a group at `q`: `name =` before it.
    let group_keyword = |q: usize| -> Option<String> {
        let mut e = q;
        while e > at && blank(b[e - 1]) {
            e -= 1;
        }
        (e > at && b[e - 1] == b'=')
            .then(|| keyword_before(e - 1))
            .flatten()
    };
    let mut p = at;
    while p < b.len() {
        match b[p] {
            b'{' => {
                let Some(close) = crate::scan::match_brace(b, p) else {
                    return Err(berr("`{` without its closing `}`", p - at, b.len() - at));
                };
                return Ok(Some(Equation {
                    start: p + 1,
                    end: close - 1,
                    lo: p,
                    hi: close,
                    keyword: group_keyword(p),
                }));
            }
            b'\'' => {
                let Some(k) = b[p + 1..].iter().position(|&c| c == b'\'') else {
                    return Err(berr("`'` without its closing `'`", p - at, b.len() - at));
                };
                return Ok(Some(Equation {
                    start: p + 1,
                    end: p + 1 + k,
                    lo: p,
                    hi: p + 2 + k,
                    keyword: group_keyword(p),
                }));
            }
            b'=' if b.get(p + 1) == Some(&b'=') => p += 2,
            b'=' if p > at && matches!(b[p - 1], b'!' | b'<' | b'>') => p += 1,
            b'=' => {
                // `inp_casefix` deleted the `"`s before this.
                let mut v = p + 1;
                while v < b.len() && (blank(b[v]) || b[v] == b'"') {
                    v += 1;
                }
                let c = b.get(v).copied().unwrap_or(b' ');
                let c1 = b.get(v + 1).copied().unwrap_or(b' ');
                let c2 = b.get(v + 2).copied().unwrap_or(b' ');
                // A bounded prefix: a card with many `=` stays linear.
                let starts = |w: &[u8]| {
                    b[v..]
                        .get(..w.len())
                        .is_some_and(|s| s.eq_ignore_ascii_case(w))
                };
                let left_alone = v >= b.len()
                    || c.is_ascii_digit()
                    || matches!(c, b'{' | b'\'' | b'.')
                    || (matches!(c, b'+' | b'-')
                        && (c1.is_ascii_digit() || (c1 == b'.' && c2.is_ascii_digit())))
                    || starts(b"true")
                    || starts(b"false");
                if left_alone {
                    p += 1;
                    continue;
                }
                if matches!(c, b'[' | b'<') {
                    return Err(berr(
                        "a vector or complex value before the behavioural expression is not \
                         supported",
                        v - at,
                        v + 1 - at,
                    ));
                }
                let mut e = v;
                let mut parens = 0usize;
                while e < b.len() && (!blank(b[e]) || parens > 0) {
                    match b[e] {
                        b'(' => parens += 1,
                        b')' => parens = parens.saturating_sub(1),
                        _ => {}
                    }
                    e += 1;
                }
                return Ok(Some(Equation {
                    start: v,
                    end: e,
                    lo: v,
                    hi: e,
                    keyword: keyword_before(p),
                }));
            }
            _ => p += 1,
        }
    }
    Ok(None)
}

/// Whether a capacitor card is in the charge form (`q=`; ngspice
/// tests the whole line, blanks around `=` already squeezed).
fn charge_form(rest: &str) -> bool {
    let b = rest.as_bytes();
    (0..b.len()).any(|i| {
        b[i].eq_ignore_ascii_case(&b'q')
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_'))
            && b[i + 1..].iter().find(|c| !matches!(c, b' ' | b'\t')) == Some(&b'=')
    })
}

/// Whether the card is a behavioural R/C/L that ngspice rewrites:
/// its text after the nodes triggers `b_transformation_wanted`, or a
/// capacitor is in the charge form — or the text reaches one of the
/// trigger words only through a name ngspice expands first.
///
/// ngspice-47 tests the line after `inp_fix_temper_in_param` (a
/// `.param` that names `temper` becomes a function, and every use of
/// it a call) and `inp_expand_macros_in_deck` (each user-function call
/// replaced by its body), both in `frontend/inpcom.c`. So `r1 1 0
/// {p}` with `.param p={temper}`, `{f()}` with `.func f() {temper}` or
/// `{g(1)}` with `.func g(x) {x*v(1)}` are rewritten as well (probes
/// `behav-probes b1 r_tparam_op`, `r_tparam_sub`, `r_func_op`; the
/// rewritten R has no thermal noise, `r_temper_noise`, and the
/// rewritten C and L add their internal nodes to the rawfile,
/// `c_temper_op`, `l_temper_op`).
pub(crate) fn wanted(card: &Card, letter: char, env: &ParamEnv) -> bool {
    if !matches!(letter, 'r' | 'c' | 'l') {
        return false;
    }
    let Some(at) = after_nodes(&card.text) else {
        return false;
    };
    let rest = &card.text[at..];
    transformation_wanted(rest)
        || (letter == 'c' && charge_form(rest))
        || expands_to_trigger(rest, env)
}

/// The identifiers of `text` (not preceded by an identifier
/// character, not starting with a digit), lowercased, each with
/// whether a `(` follows it.
fn identifiers(text: &str) -> impl Iterator<Item = (String, bool)> + '_ {
    let b = text.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            if (b[i].is_ascii_alphabetic() || b[i] == b'_') && (i == 0 || !ident(b[i - 1])) {
                let s = i;
                while i < b.len() && ident(b[i]) {
                    i += 1;
                }
                let name = text[s..i].to_ascii_lowercase();
                let mut j = i;
                while j < b.len() && matches!(b[j], b' ' | b'\t') {
                    j += 1;
                }
                return Some((name, b.get(j) == Some(&b'(')));
            }
            i += 1;
        }
        None
    })
}

/// Whether `rest` names a parameter that depends on `temper`, or calls
/// a user function whose body (transitively) holds a trigger word or
/// such a parameter: the names ngspice expands before it tests the
/// line (see [`wanted`]). Each body is read once, charged to the
/// deck's user-function budget (`ParamEnv::charge_fn_work`).
fn expands_to_trigger(rest: &str, env: &ParamEnv) -> bool {
    let mut todo: Vec<(&UserFn, Option<&FuncScope>)> = Vec::new();
    for (name, call) in identifiers(rest) {
        if call {
            if let Some((f, sc)) = env.funcs.and_then(|s| s.find(&name)) {
                todo.push((f, Some(sc)));
            }
        } else if env.is_tainted(&name) {
            return true;
        }
    }
    let mut seen: std::collections::HashSet<*const UserFn> = Default::default();
    while let Some((f, sc)) = todo.pop() {
        if !seen.insert(std::ptr::from_ref(f)) {
            continue;
        }
        let body = f.body_text();
        // Charged to the deck's user-function budget; once it is spent
        // the card goes to the rewriting, whose translator charges the
        // same budget and stops with a spanned error.
        if !env.charge_fn_work(body.len() as u64 + 1) {
            return true;
        }
        if transformation_wanted(body) {
            return true;
        }
        for (name, call) in identifiers(body) {
            if call {
                let found = sc
                    .and_then(|s| s.find(&name))
                    .or_else(|| env.funcs.and_then(|s| s.find(&name)));
                if let Some((g, gsc)) = found {
                    todo.push((g, Some(gsc)));
                }
            } else if !f.formals.iter().any(|a| **a == *name) && env.is_tainted(&name) {
                return true;
            }
        }
    }
    false
}

/// A failure while translating the expression: message and byte
/// range inside the expression text, or — inside a user function's
/// body — the span there. An empty message is a failure already
/// reported elsewhere (a failed `.param`, a recursive function).
pub(crate) struct BError {
    pub msg: String,
    pub start: usize,
    pub end: usize,
    pub at: Option<Span>,
}

#[cold]
fn berr(msg: impl Into<String>, start: usize, end: usize) -> BError {
    BError {
        msg: msg.into(),
        start,
        end: end.max(start + 1),
        at: None,
    }
}

/// Deepest nesting the translator accepts, counting parentheses,
/// signs and calls (user-function bodies included). Lower than
/// `pexpr::MAX_NEST` because this parser is recursive descent with
/// five to seven frames per level: measured in a debug build, 9.5 KB
/// of stack per parenthesis and 12 KB per call, so 40 levels stay
/// under 0.5 MiB and the parser runs in a 1 MiB thread (the PDK
/// expressions nest fewer than ten levels).
const MAX_NEST: usize = 40;

/// 0 °C in kelvin (`CONSTCtoK`): ngspice's B source reads `temper`
/// as `CKTtemp - CONSTCtoK` with `CKTtemp = T + CONSTCtoK`.
const CTOK: f64 = 273.15;

/// The value a B source gives `temper` at circuit temperature `t` °C
/// (`PT_TEMPERATURE`, `spicelib/parser/ifeval.c`).
fn b_temper(t: f64) -> f64 {
    (t + CTOK) - CTOK
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    End,
}

struct Lexer<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Lexer<'_> {
    /// Blanks, and the braces `inp_modify_exp` blanks out (in a
    /// user-function body also its `'…'` and the `"` ngspice deletes).
    fn skip_ws(&mut self) {
        while self.pos < self.s.len()
            && matches!(self.s[self.pos], b' ' | b'\t' | b'{' | b'}' | b'\'' | b'"')
        {
            self.pos += 1;
        }
    }

    /// The next token and its byte range.
    fn next(&mut self) -> Result<(Tok, usize, usize), BError> {
        self.skip_ws();
        let start = self.pos;
        let Some(&c) = self.s.get(self.pos) else {
            return Ok((Tok::End, start, start));
        };
        let two = self.s.get(self.pos + 1).copied();
        let tok = match c {
            b'(' => Tok::LParen,
            b')' => Tok::RParen,
            b',' => Tok::Comma,
            b'+' => Tok::Op("+"),
            b'-' => Tok::Op("-"),
            b'/' => Tok::Op("/"),
            b'^' => Tok::Op("^"),
            b'*' if two == Some(b'*') => {
                self.pos += 2;
                return Ok((Tok::Op("^"), start, self.pos));
            }
            b'*' => Tok::Op("*"),
            b'?' | b':' | b'<' | b'>' | b'=' | b'!' | b'&' | b'|' => {
                return Err(berr(
                    "comparisons, logical operators and `?:` are not supported yet in a \
                     behavioural R/C/L (the B-source grammar is the 3f5 one)",
                    start,
                    start + 1,
                ));
            }
            c if c.is_ascii_digit() || c == b'.' => {
                let Some((v, used)) = crate::scan::evaluate_prefix(&self.s[self.pos..]) else {
                    return Err(berr("bad number", start, start + 1));
                };
                self.pos += used;
                // INPevaluate leaves the unit letters; `inp_modify_exp`
                // skips them.
                while self.pos < self.s.len() && self.s[self.pos].is_ascii_alphabetic() {
                    self.pos += 1;
                }
                return Ok((Tok::Num(v), start, self.pos));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while self.pos < self.s.len()
                    && (self.s[self.pos].is_ascii_alphanumeric()
                        || matches!(
                            self.s[self.pos],
                            b'_' | b'!' | b'#' | b'$' | b'%' | b'[' | b']'
                        ))
                {
                    self.pos += 1;
                }
                let name = String::from_utf8_lossy(&self.s[start..self.pos]).to_ascii_lowercase();
                return Ok((Tok::Ident(name), start, self.pos));
            }
            _ => {
                return Err(berr(
                    format!("unexpected `{}`", c as char),
                    start,
                    start + 1,
                ));
            }
        };
        self.pos += 1;
        Ok((tok, start, self.pos))
    }
}

/// Deepest tree the translator builds: the B-source limit
/// (`expr::MAX_DEPTH`) less the levels [`elements`] puts on top.
const LIM_DEPTH: u32 = crate::expr::MAX_DEPTH - 4;
/// Most nodes the translator builds: the B-source limit
/// (`expr::MAX_NODES`) less what [`elements`] adds.
const LIM_NODES: u32 = crate::expr::MAX_NODES - 8;
/// The deck's user-function budget (bytes of body text read plus
/// nodes of argument trees copied are charged to it, shared with the
/// parameter evaluator).
const MAX_FN_WORK: u64 = crate::pexpr::MAX_FN_WORK;

/// A subtree with its depth and node count, carried while building so
/// the limits are enforced at every node: no tree past them is ever
/// built, walked or dropped (the left-associative loops would
/// otherwise grow one level per term).
#[derive(Clone)]
struct T {
    e: Expr,
    d: u32,
    n: u32,
}

impl T {
    fn leaf(e: Expr) -> T {
        T { e, d: 1, n: 1 }
    }
}

/// State shared by the parser of the expression and those of the
/// user-function bodies it expands.
struct Shared {
    depth: Cell<usize>,
}

/// Inside a user function's body: its formal arguments bound to the
/// argument trees, and the scope it was defined in (where the calls
/// of the body resolve first).
struct FnCtx<'a, 'e> {
    formals: &'e [Box<str>],
    args: &'a [T],
    scope: &'e FuncScope<'e>,
}

struct Parser<'a, 'e> {
    lx: Lexer<'a>,
    tok: Tok,
    start: usize,
    end: usize,
    env: &'e ParamEnv<'e>,
    /// Maps byte ranges of the text being parsed to spans.
    span_of: &'a dyn Fn(usize, usize) -> Span,
    st: &'a Shared,
    func: Option<FnCtx<'a, 'e>>,
}

fn bin(op: ExprOp, a: Expr, b: Expr) -> Expr {
    Expr::Bin(op, Box::new(a), Box::new(b))
}

fn call(f: ExprFunc, a: Expr) -> Expr {
    Expr::Call(f, Box::new(a))
}

/// Check the limits for a node of depth `d` and `n` nodes.
fn fits(d: u32, n: u32, s: usize, e: usize) -> Result<(), BError> {
    if d > LIM_DEPTH {
        return Err(berr(
            format!("expression nested too deeply (more than {LIM_DEPTH} levels)"),
            s,
            e,
        ));
    }
    if n > LIM_NODES {
        return Err(berr(
            format!("expression too large (more than {LIM_NODES} nodes)"),
            s,
            e,
        ));
    }
    Ok(())
}

/// A binary node, within the limits.
fn tbin(op: ExprOp, a: T, b: T, s: usize, e: usize) -> Result<T, BError> {
    let d = a.d.max(b.d) + 1;
    let n = a.n.saturating_add(b.n).saturating_add(1);
    fits(d, n, s, e)?;
    Ok(T {
        e: bin(op, a.e, b.e),
        d,
        n,
    })
}

/// A function node, within the limits.
fn tcall(f: ExprFunc, a: T, s: usize, e: usize) -> Result<T, BError> {
    let (d, n) = (a.d + 1, a.n.saturating_add(1));
    fits(d, n, s, e)?;
    Ok(T {
        e: call(f, a.e),
        d,
        n,
    })
}

#[cold]
fn too_deep(s: usize, e: usize) -> BError {
    berr(
        format!(
            "expression nested too deeply (more than {MAX_NEST} parentheses, calls, signs or \
             function bodies)"
        ),
        s,
        e,
    )
}

impl<'a, 'e> Parser<'a, 'e> {
    fn bump(&mut self) -> Result<(), BError> {
        let (t, s, e) = self.lx.next()?;
        self.tok = t;
        self.start = s;
        self.end = e;
        Ok(())
    }

    fn enter(&mut self) -> Result<(), BError> {
        let d = self.st.depth.get() + 1;
        self.st.depth.set(d);
        if d > MAX_NEST {
            return Err(too_deep(self.start, self.end));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.st.depth.set(self.st.depth.get().saturating_sub(1));
    }

    /// Spend `units` of the deck's user-function budget.
    fn charge(&mut self, units: u64, s: usize, e: usize) -> Result<(), BError> {
        if self.env.charge_fn_work(units) {
            Ok(())
        } else {
            Err(berr(
                format!(
                    "user-function evaluation exceeds {MAX_FN_WORK} steps in this deck; giving up"
                ),
                s,
                e,
            ))
        }
    }

    fn expect(&mut self, t: Tok, what: &str) -> Result<(), BError> {
        if self.tok != t {
            return Err(berr(format!("expected {what}"), self.start, self.end));
        }
        self.bump()
    }

    /// `exp: exp '+' exp | exp '-' exp` (left).
    fn add(&mut self) -> Result<T, BError> {
        let mut l = self.mul()?;
        loop {
            let op = match self.tok {
                Tok::Op("+") => ExprOp::Plus,
                Tok::Op("-") => ExprOp::Minus,
                _ => return Ok(l),
            };
            let (s, e) = (self.start, self.end);
            self.bump()?;
            let r = self.mul()?;
            l = tbin(op, l, r, s, e)?;
        }
    }

    /// `'*'`, `'/'` (left).
    fn mul(&mut self) -> Result<T, BError> {
        let mut l = self.unary()?;
        loop {
            let op = match self.tok {
                Tok::Op("*") => ExprOp::Times,
                Tok::Op("/") => ExprOp::Divide,
                _ => return Ok(l),
            };
            let (s, e) = (self.start, self.end);
            self.bump()?;
            let r = self.unary()?;
            l = tbin(op, l, r, s, e)?;
        }
    }

    /// `'-' exp %prec NEG`: looser than `^` (`-2^2 = -4`); a `+` sign
    /// is dropped.
    fn unary(&mut self) -> Result<T, BError> {
        match self.tok {
            Tok::Op("-") => {
                let (s, e) = (self.start, self.end);
                self.enter()?;
                self.bump()?;
                let x = self.unary()?;
                self.leave();
                tcall(ExprFunc::Neg, x, s, e)
            }
            Tok::Op("+") => {
                self.enter()?;
                self.bump()?;
                let x = self.unary()?;
                self.leave();
                Ok(x)
            }
            _ => self.power(),
        }
    }

    /// `exp '^' exp` (`%left '^'`, `**` too): left-associative, its
    /// operands are primaries or signed ones.
    fn power(&mut self) -> Result<T, BError> {
        let mut l = self.primary()?;
        while self.tok == Tok::Op("^") {
            let (s, e) = (self.start, self.end);
            self.bump()?;
            let r = match self.tok {
                Tok::Op("-") | Tok::Op("+") => self.unary_primary()?,
                _ => self.primary()?,
            };
            l = tbin(ExprOp::Power, l, r, s, e)?;
        }
        Ok(l)
    }

    /// A signed primary (the right operand of `^`).
    fn unary_primary(&mut self) -> Result<T, BError> {
        let neg = self.tok == Tok::Op("-");
        let (s, e) = (self.start, self.end);
        self.enter()?;
        self.bump()?;
        let x = match self.tok {
            Tok::Op("-") | Tok::Op("+") => self.unary_primary()?,
            _ => self.primary()?,
        };
        self.leave();
        if neg {
            tcall(ExprFunc::Neg, x, s, e)
        } else {
            Ok(x)
        }
    }

    fn primary(&mut self) -> Result<T, BError> {
        let (s, e) = (self.start, self.end);
        match std::mem::replace(&mut self.tok, Tok::End) {
            Tok::Num(v) => {
                self.bump()?;
                Ok(T::leaf(Expr::Num(v)))
            }
            Tok::LParen => {
                self.enter()?;
                self.bump()?;
                let x = self.add()?;
                self.expect(Tok::RParen, "`)`")?;
                self.leave();
                Ok(x)
            }
            Tok::Ident(name) => {
                self.bump()?;
                if self.tok == Tok::LParen {
                    self.enter()?;
                    let r = self.call(&name, s, e);
                    self.leave();
                    r
                } else {
                    self.name(&name, s, e)
                }
            }
            t => {
                self.tok = t;
                Err(berr("syntax error", s, e))
            }
        }
    }

    /// A bare name: a formal argument of the user function being
    /// expanded, the constants `pi` and `e`, `temper` (the circuit
    /// temperature, as the B source reads it), else a parameter,
    /// evaluated now as numparam does after `inp_modify_exp` — with
    /// the rules of `temper` and of the parameters that depend on it
    /// (`ParamEnv::read_for_behav`).
    fn name(&mut self, name: &str, s: usize, e: usize) -> Result<T, BError> {
        if let Some(f) = &self.func
            && let Some(k) = f.formals.iter().position(|a| **a == *name)
        {
            let a = f.args[k].clone();
            self.charge(u64::from(a.n), s, e)?;
            return Ok(a);
        }
        match name {
            "pi" => Ok(T::leaf(Expr::Num(std::f64::consts::PI))),
            "e" => Ok(T::leaf(Expr::Num(std::f64::consts::E))),
            "time" | "hertz" => Err(berr(
                format!("`{name}` in a behavioural R/C/L is not supported yet"),
                s,
                e,
            )),
            _ => match self.env.read_for_behav(name, (self.span_of)(s, e)) {
                Ok(v) if name == "temper" => Ok(T::leaf(Expr::Num(b_temper(v)))),
                Ok(v) => Ok(T::leaf(Expr::Num(v))),
                Err(None) => Err(BError {
                    msg: String::new(),
                    start: s,
                    end: e,
                    at: None,
                }),
                Err(Some(msg)) => Err(berr(msg, s, e)),
            },
        }
    }

    /// A node or branch name inside `v(…)`/`i(…)`: everything up to
    /// the `,` or `)`, as `inp_modify_exp` copies it.
    fn ref_name(&mut self) -> Result<String, BError> {
        self.lx.skip_ws();
        let st = self.lx.pos;
        while self.lx.pos < self.lx.s.len()
            && !matches!(self.lx.s[self.lx.pos], b',' | b')' | b' ' | b'\t')
        {
            self.lx.pos += 1;
        }
        if self.lx.pos == st {
            return Err(berr("empty name", st, st + 1));
        }
        Ok(String::from_utf8_lossy(&self.lx.s[st..self.lx.pos]).to_ascii_lowercase())
    }

    /// `v(a)`, `v(a, b)`, `i(vx)`; `self.tok` is the `(`.
    fn node_ref(&mut self, name: &str, s: usize, e: usize) -> Result<T, BError> {
        let a = self.ref_name()?;
        self.bump()?;
        let r = if name == "v" {
            if self.tok == Tok::Comma {
                let b = self.ref_name()?;
                self.bump()?;
                tbin(
                    ExprOp::Minus,
                    T::leaf(Expr::V(a)),
                    T::leaf(Expr::V(b)),
                    s,
                    e,
                )?
            } else {
                T::leaf(Expr::V(a))
            }
        } else {
            T::leaf(Expr::I(a))
        };
        self.expect(Tok::RParen, "`)`")?;
        Ok(r)
    }

    /// The comma-separated arguments up to the `)` (at most `max`;
    /// `self.tok` is the `(`).
    fn args(&mut self, max: usize, name: &str, s: usize, e: usize) -> Result<Vec<T>, BError> {
        self.bump()?;
        let mut args: Vec<T> = Vec::new();
        if self.tok != Tok::RParen {
            loop {
                if args.len() >= max {
                    return Err(berr(format!("too many arguments to `{name}`"), s, e));
                }
                args.push(self.add()?);
                if self.tok == Tok::Comma {
                    self.bump()?;
                } else {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        Ok(args)
    }

    /// A call; `self.tok` is the `(`. A user function (`.func`)
    /// shadows a built-in one and is expanded in place, as ngspice
    /// expands its body into the line before the rewriting.
    fn call(&mut self, name: &str, s: usize, e: usize) -> Result<T, BError> {
        if name == "v" || name == "i" {
            return self.node_ref(name, s, e);
        }
        // A call inside a body resolves where the body was defined,
        // then at the call site (as `pexpr` evaluates user functions).
        let user = self
            .func
            .as_ref()
            .and_then(|f| f.scope.find(name))
            .or_else(|| self.env.funcs.and_then(|sc| sc.find(name)));
        match user {
            Some((f, sc)) => {
                let args = self.args(f.formals.len().saturating_add(1), name, s, e)?;
                self.expand(f, sc, args, s, e)
            }
            None => {
                let args = self.args(2, name, s, e)?;
                builtin(name, args, s, e)
            }
        }
    }

    /// The body of user function `f` (defined in `sc`) with its
    /// formals bound to `args`, parsed with the B-source grammar —
    /// the expanded text ngspice would parse. Errors inside the body
    /// point there.
    fn expand(
        &mut self,
        f: &'e UserFn<'e>,
        sc: &'e FuncScope<'e>,
        args: Vec<T>,
        s: usize,
        e: usize,
    ) -> Result<T, BError> {
        if f.recursive.get() {
            // Reported where the function is defined.
            return Err(BError {
                msg: String::new(),
                start: s,
                end: e,
                at: None,
            });
        }
        if args.len() != f.formals.len() {
            return Err(berr(
                format!(
                    "function `{}` takes {} argument(s), got {}",
                    f.name,
                    f.formals.len(),
                    args.len()
                ),
                s,
                e,
            ));
        }
        let body = f.body_text();
        if body.is_empty() {
            return Err(berr(format!("function `{}` has no body", f.name), s, e));
        }
        self.charge(body.len() as u64 + 1, s, e)?;
        let span_of = |a: usize, b: usize| f.span_of(a, b);
        let mut sub = Parser {
            lx: Lexer {
                s: body.as_bytes(),
                pos: 0,
            },
            tok: Tok::End,
            start: 0,
            end: 0,
            env: self.env,
            span_of: &span_of,
            st: self.st,
            func: Some(FnCtx {
                formals: &f.formals,
                args: &args,
                scope: sc,
            }),
        };
        let r = sub.whole();
        r.map_err(|mut err| {
            if err.at.is_none() && !err.msg.is_empty() {
                err.at = Some(f.span_of(err.start, err.end));
            }
            err
        })
    }

    /// The whole text as one expression.
    fn whole(&mut self) -> Result<T, BError> {
        self.bump()?;
        let t = self.add()?;
        if self.tok != Tok::End {
            return Err(berr(
                "unexpected text after the expression",
                self.start,
                self.end,
            ));
        }
        Ok(t)
    }
}

/// A built-in function of the B-source grammar applied to its
/// arguments.
fn builtin(name: &str, mut args: Vec<T>, s: usize, e: usize) -> Result<T, BError> {
    let argc = args.len();
    let want = |n: usize| -> Result<(), BError> {
        if argc == n {
            Ok(())
        } else {
            Err(berr(format!("`{name}` takes {n} argument(s)"), s, e))
        }
    };
    let f = match name {
        "abs" => ExprFunc::Abs,
        "acos" => ExprFunc::Acos,
        "acosh" => ExprFunc::Acosh,
        "asin" => ExprFunc::Asin,
        "asinh" => ExprFunc::Asinh,
        "atan" => ExprFunc::Atan,
        "atanh" => ExprFunc::Atanh,
        "cos" => ExprFunc::Cos,
        "cosh" => ExprFunc::Cosh,
        "exp" => ExprFunc::Exp,
        // ngspice's `ln` and `log` are both the natural log
        // (`PTlog`); `log10` is the 3f5's `log`.
        "ln" | "log" => ExprFunc::Ln,
        "log10" => ExprFunc::Log,
        "sgn" => ExprFunc::Sgn,
        "sin" => ExprFunc::Sin,
        "sinh" => ExprFunc::Sinh,
        "sqrt" => ExprFunc::Sqrt,
        "tan" => ExprFunc::Tan,
        "tanh" => ExprFunc::Tanh,
        "u" => ExprFunc::U,
        "uramp" => ExprFunc::Uramp,
        "pow" | "pwr" | "min" | "max" => {
            want(2)?;
            let b = args.pop().unwrap_or(T::leaf(Expr::Num(0.0)));
            let a = args.pop().unwrap_or(T::leaf(Expr::Num(0.0)));
            return binary_func(name, a, b, s, e);
        }
        "ceil" | "floor" | "nint" | "u2" | "pwl" | "eq0" | "ne0" | "gt0" | "lt0" | "ge0"
        | "le0" | "ddt" => {
            return Err(berr(
                format!("`{name}()` in a behavioural R/C/L is not supported yet"),
                s,
                e,
            ));
        }
        _ => return Err(berr(format!("unknown function `{name}`"), s, e)),
    };
    want(1)?;
    let a = args.pop().unwrap_or(T::leaf(Expr::Num(0.0)));
    tcall(f, a, s, e)
}

/// `pow` (`PTpower`), `pwr` (`PTpwr`), `min`, `max` as 3f5 trees:
/// `pow(a,b) = a^b`; `pwr(a,b) = sgn(a)·|a|^b`;
/// `min(a,b) = a·u(b−a) + b·u(a−b)`, `max(a,b) = a·u(a−b) + b·u(b−a)`
/// (exact values away from ties and `a = 0`, see the docs). The
/// limits are checked before anything is cloned.
fn binary_func(name: &str, a: T, b: T, s: usize, e: usize) -> Result<T, BError> {
    let (d, n) = (a.d.max(b.d), a.n.saturating_add(b.n));
    let (dd, nn) = match name {
        "pow" => (d + 1, n.saturating_add(1)),
        "pwr" => (d + 2, n.saturating_add(a.n).saturating_add(3)),
        _ => (d + 3, n.saturating_mul(2).saturating_add(7)),
    };
    fits(dd, nn, s, e)?;
    let (a, b) = (a.e, b.e);
    let e = match name {
        "pow" => bin(ExprOp::Power, a, b),
        "pwr" => bin(
            ExprOp::Times,
            call(ExprFunc::Sgn, a.clone()),
            bin(ExprOp::Power, call(ExprFunc::Abs, a), b),
        ),
        _ => {
            let (amb, bma) = (
                bin(ExprOp::Minus, a.clone(), b.clone()),
                bin(ExprOp::Minus, b.clone(), a.clone()),
            );
            let (ua, ub) = if name == "min" {
                (call(ExprFunc::U, bma), call(ExprFunc::U, amb))
            } else {
                (call(ExprFunc::U, amb), call(ExprFunc::U, bma))
            };
            bin(
                ExprOp::Plus,
                bin(ExprOp::Times, a, ua),
                bin(ExprOp::Times, b, ub),
            )
        }
    };
    Ok(T { e, d: dd, n: nn })
}

/// Translate an ngspice behavioural expression (the text inside the
/// braces or quotes) into a B-source tree, evaluating its parameters
/// in `env` and expanding its user functions. `span_of` maps byte
/// ranges of `text` to spans. The result stays
/// [`LIM_DEPTH`]/[`LIM_NODES`] below the B-source limits, so
/// [`elements`] can wrap it.
pub(crate) fn translate(
    text: &str,
    env: &ParamEnv,
    span_of: &dyn Fn(usize, usize) -> Span,
) -> Result<Expr, BError> {
    if text.len() > crate::pexpr::MAX_LEN {
        return Err(berr(
            format!(
                "expression too long (more than {} bytes)",
                crate::pexpr::MAX_LEN
            ),
            0,
            1,
        ));
    }
    let st = Shared {
        depth: Cell::new(0),
    };
    let mut p = Parser {
        lx: Lexer {
            s: text.as_bytes(),
            pos: 0,
        },
        tok: Tok::End,
        start: 0,
        end: 0,
        env,
        span_of,
        st: &st,
        func: None,
    };
    p.whole().map(|t| t.e)
}

/// The temperature factor ngspice-47's B source applies to its value
/// when `inp_compat` passes the R/C/L's `tc1`/`tc2` on (`ASRCload`,
/// `spicelib/devices/asrc/asrcload.c`, Modified BSD): `f = 1 + tc1·Δ +
/// tc2·Δ²` with `Δ = (T + CONSTCtoK) − 300.15` — the nominal
/// temperature is fixed at 27 °C there, not `tnom` — inverted when
/// `reciproctc=1` (R and C), then multiplied by the B source's own
/// `m` (`reciprocm=0`). Exact at the fixed circuit temperature `t`.
pub(crate) fn tc_factor(tc1: f64, tc2: f64, t: f64, reciprocal: bool, m: f64) -> f64 {
    let d = (t + CTOK) - 300.15;
    let mut f = 1.0 + tc1 * d + tc2 * d * d;
    if reciprocal {
        f = 1.0 / f;
    }
    f * m
}

/// One generated element.
fn el(name: String, nodes: &[&str], device: Device, span: Span) -> Element {
    Element {
        name,
        nodes: nodes.iter().map(|s| s.to_string()).collect(),
        device,
        span,
    }
}

/// What the rewriting needs from the card.
pub(crate) struct Behav<'a> {
    pub letter: char,
    pub name: &'a str,
    pub n1: &'a str,
    pub n2: &'a str,
    pub expr: Expr,
    /// The card's `m=` (`eval_m`/`eval_mvalue`), `None` without one.
    pub m: Option<f64>,
    /// A charge-form capacitor (`q=`).
    pub charge: bool,
    /// The card's `tc1`, `tc2` (0 without them).
    pub tc: (f64, f64),
    /// The circuit temperature (°C) the `tc` factor is taken at.
    pub temp: f64,
    pub span: Span,
}

/// `k·x`, or `x` when `k` is 1.
fn scaled(k: f64, x: Expr) -> Expr {
    if k == 1.0 {
        x
    } else {
        bin(ExprOp::Times, Expr::Num(k), x)
    }
}

/// The ngspice-47 rewriting (`inp_compat`), as elements.
pub(crate) fn elements(b: Behav) -> Vec<Element> {
    let (name, n1, n2, span) = (b.name, b.n1, b.n2, b.span);
    let (tc1, tc2) = b.tc;
    // `reciproctc=1` on the B line of a capacitor, 0 on an inductor's.
    let kc = tc_factor(tc1, tc2, b.temp, true, 1.0);
    match b.letter {
        'r' => {
            // `b%s %s %s i = v(%s, %s)/(%s) tc1=… tc2=… m=…
            // reciproctc=1 reciprocm=0`: the B source divides the
            // current by the temperature factor and multiplies it by
            // its `m` (`tc_factor`).
            let k = tc_factor(tc1, tc2, b.temp, true, b.m.unwrap_or(1.0));
            let expr = scaled(
                k,
                bin(
                    ExprOp::Divide,
                    bin(ExprOp::Minus, Expr::V(n1.into()), Expr::V(n2.into())),
                    b.expr,
                ),
            );
            vec![el(
                format!("b{name}"),
                &[n1, n2],
                Device::Behavioral {
                    kind: SourceKind::Current,
                    expr,
                },
                span,
            )]
        }
        'c' if b.charge => {
            let int1 = format!("{name}_int1");
            vec![
                el(
                    format!("g{name}"),
                    &[n1, n2, &int1, "0"],
                    Device::Vccs {
                        gain: Some(b.m.unwrap_or(1.0)),
                    },
                    span,
                ),
                el(
                    format!("l{name}"),
                    &[&int1, "0"],
                    Device::Inductor {
                        value: Some(1.0),
                        params: Vec::new(),
                    },
                    span,
                ),
                el(
                    format!("b{name}"),
                    &["0", &int1],
                    Device::Behavioral {
                        kind: SourceKind::Current,
                        expr: scaled(kc, b.expr),
                    },
                    span,
                ),
            ]
        }
        'c' => {
            let (int1, int2) = (format!("{name}_int1"), format!("{name}_int2"));
            let meas = format!("vcurrmeas{name}");
            // `"g%s %s_int1 0 %s %s %se9"` with `%s` the `eval_mvalue`
            // string: "1" without `m=`, giving 1e9; with `m=` it is
            // `"%15.8e"`, and `"2.00000000e+00e9"` reads as 2 — the
            // capacitor shrinks by 1e-9 (probe `c_ac_m`, docs).
            let gain = b.m.unwrap_or(1e9);
            vec![
                el(
                    format!("g{name}"),
                    &[&int1, "0", n2, n1],
                    Device::Vccs { gain: Some(gain) },
                    span,
                ),
                el(
                    format!("r{name}"),
                    &[&int1, "0"],
                    Device::Resistor {
                        value: Some(1e-9),
                        model: None,
                        params: Vec::new(),
                    },
                    span,
                ),
                el(
                    format!("c{name}"),
                    &[&int1, &int2],
                    Device::Capacitor {
                        value: Some(1e-6),
                        model: None,
                        params: Vec::new(),
                    },
                    span,
                ),
                el(
                    meas.clone(),
                    &[&int2, "0"],
                    Device::Source {
                        kind: SourceKind::Voltage,
                        dc: Some(0.0),
                        ac: None,
                        tran: None,
                        distof1: None,
                        distof2: None,
                        port: None,
                    },
                    span,
                ),
                el(
                    format!("r{name}par"),
                    &[n2, n1],
                    Device::Resistor {
                        value: Some(1e15),
                        model: None,
                        params: Vec::new(),
                    },
                    span,
                ),
                el(
                    format!("b{name}"),
                    &[n1, n2],
                    Device::Behavioral {
                        kind: SourceKind::Current,
                        expr: scaled(
                            kc,
                            bin(
                                ExprOp::Times,
                                bin(ExprOp::Times, Expr::Num(1e6), Expr::I(meas)),
                                b.expr,
                            ),
                        ),
                    },
                    span,
                ),
            ]
        }
        _ => {
            let int2 = format!("{name}_int2");
            let bname = format!("b{name}");
            vec![
                el(
                    format!("f{name}"),
                    &[&int2, "0"],
                    Device::Cccs {
                        vname: bname.clone(),
                        gain: Some(-1.0),
                    },
                    span,
                ),
                el(
                    format!("l{name}"),
                    &[&int2, "0"],
                    Device::Inductor {
                        value: Some(1.0),
                        params: Vec::new(),
                    },
                    span,
                ),
                el(
                    bname,
                    &[n2, n1],
                    Device::Behavioral {
                        kind: SourceKind::Voltage,
                        expr: scaled(
                            tc_factor(tc1, tc2, b.temp, false, 1.0),
                            bin(
                                ExprOp::Divide,
                                bin(ExprOp::Times, Expr::V(int2), b.expr),
                                Expr::Num(b.m.unwrap_or(1.0)),
                            ),
                        ),
                    },
                    span,
                ),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nospan(s: usize, e: usize) -> Span {
        Span::new(crate::span::FileId(0), s, e)
    }

    fn tr(s: &str) -> Result<Expr, String> {
        translate(s, &ParamEnv::default(), &nospan).map_err(|e| e.msg)
    }

    fn num(e: &Expr) -> f64 {
        match e {
            Expr::Num(v) => *v,
            Expr::Bin(op, a, b) => {
                let (a, b) = (num(a), num(b));
                match op {
                    ExprOp::Plus => a + b,
                    ExprOp::Minus => a - b,
                    ExprOp::Times => a * b,
                    ExprOp::Divide => a / b,
                    ExprOp::Power => a.powf(b),
                }
            }
            Expr::Call(ExprFunc::Neg, a) => -num(a),
            Expr::Call(ExprFunc::U, a) => {
                let x = num(a);
                if x > 0.0 {
                    1.0
                } else if x < 0.0 {
                    0.0
                } else {
                    0.5
                }
            }
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn wanted_follows_b_transformation_wanted() {
        assert!(transformation_wanted(" r='1k*v(1)'"));
        assert!(transformation_wanted(" {1+i(vx)}"));
        assert!(transformation_wanted(" {1+time}"));
        assert!(!transformation_wanted(" rtime 1k"));
        assert!(!transformation_wanted(" {1k*dv(1)}"));
        assert!(charge_form(" q = '1p*v(1)'"));
        assert!(!charge_form(" c='1p*v(1)' aq"));
    }

    /// ngspice's B grammar (`inpptree-parser.y`): `^` and `**` left
    /// associative, unary minus looser than `^` (probes `r_powassoc`,
    /// `r_unaryneg` in dev-probes s3).
    #[test]
    fn precedence_is_ngspices() {
        assert_eq!(num(&tr("2^3^2").unwrap()), 64.0);
        assert_eq!(num(&tr("2**3**2").unwrap()), 64.0);
        assert_eq!(num(&tr("-2^2").unwrap()), -4.0);
        assert_eq!(num(&tr("2^-1").unwrap()), 0.5);
        assert_eq!(num(&tr("1+2*3-4/2").unwrap()), 5.0);
        assert_eq!(num(&tr("1k*2meg").unwrap()), 2e9);
        assert_eq!(num(&tr("{2}*3").unwrap()), 6.0);
    }

    #[test]
    fn min_max_are_exact_away_from_ties() {
        for (a, b) in [(1e-20_f64, 1.0_f64), (3.0, -2.0), (5.0, 5.0)] {
            let e = tr(&format!("min({a:e},{b:e})")).unwrap();
            assert_eq!(num(&e), a.min(b), "min {a} {b}");
            let e = tr(&format!("max({a:e},{b:e})")).unwrap();
            assert_eq!(num(&e), a.max(b), "max {a} {b}");
        }
    }

    #[test]
    fn refusals_are_reported() {
        for (s, what) in [
            ("v(1)>1 ? 1 : 2", "comparisons"),
            ("1+temper", "temper"),
            ("1+time", "time"),
            ("floor(v(1))", "floor"),
            ("foo(v(1))", "unknown function"),
            ("x+v(1)", "unknown parameter"),
            ("min(v(1))", "2 argument"),
            ("(v(1)", "`)`"),
        ] {
            let e = tr(s).unwrap_err();
            assert!(e.contains(what), "{s}: {e}");
        }
    }

    #[test]
    fn nodes_and_branches() {
        assert_eq!(
            tr("v(a, b)").unwrap(),
            bin(ExprOp::Minus, Expr::V("a".into()), Expr::V("b".into()))
        );
        assert_eq!(tr("i(VX)").unwrap(), Expr::I("vx".into()));
        assert_eq!(
            tr("log(v(1))").unwrap(),
            call(ExprFunc::Ln, Expr::V("1".into()))
        );
        assert_eq!(
            tr("log10(v(1))").unwrap(),
            call(ExprFunc::Log, Expr::V("1".into()))
        );
    }

    /// Hostile input (principle 4): every bound is enforced while the
    /// tree is built, so these end fast with an error and never
    /// overflow a small stack (the thread has 1 MiB, half of a
    /// default spawned thread).
    #[test]
    fn hostile_expressions_are_bounded() {
        let run = || {
            let cases: Vec<(String, &str)> = vec![
                // 1e6-term chain: refused by length (MAX_LEN).
                ("1+".repeat(1_000_000) + "1", "too long"),
                // Below MAX_LEN, a left-deep chain: refused by depth.
                ("v(a)+".repeat(10_000) + "1", "nested too deeply"),
                ("v(a)^".repeat(10_000) + "2", "nested too deeply"),
                ("v(a)*2/".repeat(5_000) + "1", "nested too deeply"),
                (
                    format!("{}v(a){}", "(".repeat(100_000), ")".repeat(100_000)),
                    "too long",
                ),
                (
                    format!("{}v(a){}", "(".repeat(20_000), ")".repeat(20_000)),
                    "nested too deeply",
                ),
                (
                    format!("{}v(a){}", "abs(".repeat(10_000), ")".repeat(10_000)),
                    "nested too deeply",
                ),
                ("-".repeat(30_000) + "v(a)", "nested too deeply"),
                // Duplication by min/max: refused by node count.
                (
                    {
                        let mut s = "v(a)+v(b)+v(c)".to_string();
                        for _ in 0..12 {
                            s = format!("min({s},{s})");
                        }
                        s
                    },
                    "",
                ),
            ];
            for (s, what) in cases {
                let e = translate(&s, &ParamEnv::default(), &nospan).unwrap_err();
                assert!(e.msg.contains(what), "{}: {}", &s[..20], e.msg);
                assert!(e.start < s.len().max(1));
            }
        };
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(run)
            .unwrap()
            .join()
            .unwrap();
    }

    /// The deepest nesting accepted, of each kind, parses in a 1 MiB
    /// thread (debug build included); one level more is refused.
    #[test]
    fn deepest_nesting_fits_a_1mib_stack() {
        let run = || {
            let p = ParamEnv::default();
            let n = MAX_NEST - 1;
            for (open, close) in [("(", ")"), ("abs(", ")"), ("-(", ")"), ("2^(", ")")] {
                let levels = if open == "-(" { n / 2 } else { n };
                let s = format!("{}v(a){}", open.repeat(levels), close.repeat(levels));
                assert!(translate(&s, &p, &nospan).is_ok(), "{open} x {levels}");
                let s = format!(
                    "{}v(a){}",
                    open.repeat(MAX_NEST + 1),
                    close.repeat(MAX_NEST + 1)
                );
                let e = translate(&s, &p, &nospan).unwrap_err();
                assert!(e.msg.contains("nested too deeply"), "{}", e.msg);
            }
        };
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(run)
            .unwrap()
            .join()
            .unwrap();
    }
}
