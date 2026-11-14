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
//! (`b_transformation_wanted`), or a `q=` on a capacitor. ngspice then
//! hands the B lines to numparam with every parameter name in braces
//! (`inp_modify_exp`) and parses them with its B-source grammar
//! (`inpptree-parser.y`: `^` left-associative, unary minus looser
//! than `^`). Dune evaluates the parameters here and builds the
//! 3f5-grammar tree of `ir::Expr` directly, keeping ngspice's
//! structure; the ngspice functions without a 3f5 counterpart are
//! rewritten exactly where the values agree (`min`, `max`, `pwr`;
//! `docs/dune-netlist.md`), and the rest is refused with a spanned
//! diagnostic.

use crate::deck::Card;
use crate::ir::{Device, Element, Expr, ExprFunc, ExprOp, ParamValue, SourceKind};
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

/// Whether the card is a behavioural R/C/L that ngspice rewrites.
pub(crate) fn wanted(card: &Card, letter: char) -> bool {
    if !matches!(letter, 'r' | 'c' | 'l') {
        return false;
    }
    let Some(at) = after_nodes(&card.text) else {
        return false;
    };
    let rest = &card.text[at..];
    transformation_wanted(rest) || (letter == 'c' && charge_form(rest))
}

/// A failure while translating the expression: message and byte
/// range inside the expression text.
pub(crate) struct BError {
    pub msg: String,
    pub start: usize,
    pub end: usize,
}

fn berr(msg: impl Into<String>, start: usize, end: usize) -> BError {
    BError {
        msg: msg.into(),
        start,
        end: end.max(start + 1),
    }
}

/// Deepest nesting the translator accepts (as `pexpr::MAX_NEST`).
const MAX_NEST: usize = 100;

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
    /// Blanks, and the braces `inp_modify_exp` blanks out.
    fn skip_ws(&mut self) {
        while self.pos < self.s.len() && matches!(self.s[self.pos], b' ' | b'\t' | b'{' | b'}') {
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

/// A subtree with its depth and node count, carried while building so
/// the limits are enforced at every node: no tree past them is ever
/// built, walked or dropped (the left-associative loops would
/// otherwise grow one level per term).
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

struct Parser<'a, 'e> {
    lx: Lexer<'a>,
    tok: Tok,
    start: usize,
    end: usize,
    env: &'e ParamEnv<'e>,
    depth: usize,
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

impl<'a, 'e> Parser<'a, 'e> {
    fn bump(&mut self) -> Result<(), BError> {
        let (t, s, e) = self.lx.next()?;
        self.tok = t;
        self.start = s;
        self.end = e;
        Ok(())
    }

    fn enter(&mut self) -> Result<(), BError> {
        self.depth += 1;
        if self.depth > MAX_NEST {
            return Err(berr(
                format!(
                    "expression nested too deeply (more than {MAX_NEST} parentheses, calls or signs)"
                ),
                self.start,
                self.end,
            ));
        }
        Ok(())
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
                self.depth -= 1;
                tcall(ExprFunc::Neg, x, s, e)
            }
            Tok::Op("+") => {
                self.enter()?;
                self.bump()?;
                let x = self.unary()?;
                self.depth -= 1;
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
        self.depth -= 1;
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
                self.depth -= 1;
                Ok(x)
            }
            Tok::Ident(name) => {
                self.bump()?;
                if self.tok == Tok::LParen {
                    self.enter()?;
                    let r = self.call(&name, s, e);
                    self.depth -= 1;
                    r
                } else {
                    self.name(&name, s, e).map(T::leaf)
                }
            }
            t => {
                self.tok = t;
                Err(berr("syntax error", s, e))
            }
        }
    }

    /// A bare name: the constants `pi` and `e`, else a parameter,
    /// evaluated now as numparam does after `inp_modify_exp`.
    fn name(&mut self, name: &str, s: usize, e: usize) -> Result<Expr, BError> {
        match name {
            "pi" => Ok(Expr::Num(std::f64::consts::PI)),
            "e" => Ok(Expr::Num(std::f64::consts::E)),
            "time" | "temper" | "hertz" => Err(berr(
                format!("`{name}` in a behavioural R/C/L is not supported yet"),
                s,
                e,
            )),
            _ => match self.env.get(name) {
                Some(v) => Ok(Expr::Num(v)),
                None => Err(berr(format!("undefined parameter `{name}`"), s, e)),
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

    fn call(&mut self, name: &str, s: usize, e: usize) -> Result<T, BError> {
        // `self.tok` is the `(`.
        if name == "v" || name == "i" {
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
            return Ok(r);
        }
        self.bump()?;
        let mut args: Vec<T> = Vec::new();
        if self.tok != Tok::RParen {
            loop {
                if args.len() >= 2 {
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
/// in `env`. The result stays [`LIM_DEPTH`]/[`LIM_NODES`] below the
/// B-source limits, so [`elements`] can wrap it.
pub(crate) fn translate(text: &str, env: &ParamEnv) -> Result<Expr, BError> {
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
    let mut p = Parser {
        lx: Lexer {
            s: text.as_bytes(),
            pos: 0,
        },
        tok: Tok::End,
        start: 0,
        end: 0,
        env,
        depth: 0,
    };
    p.bump()?;
    let t = p.add()?;
    if p.tok != Tok::End {
        return Err(berr("unexpected text after the expression", p.start, p.end));
    }
    Ok(t.e)
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
    pub span: Span,
}

/// The ngspice-47 rewriting (`inp_compat`), as elements.
pub(crate) fn elements(b: Behav) -> Vec<Element> {
    let (name, n1, n2, span) = (b.name, b.n1, b.n2, b.span);
    match b.letter {
        'r' => {
            // `b%s %s %s i = v(%s, %s)/(%s) … m=… reciprocm=0`: the
            // B source's `m` multiplies the current.
            let mut expr = bin(
                ExprOp::Divide,
                bin(ExprOp::Minus, Expr::V(n1.into()), Expr::V(n2.into())),
                b.expr,
            );
            if let Some(m) = b.m
                && m != 1.0
            {
                expr = bin(ExprOp::Times, Expr::Num(m), expr);
            }
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
                        expr: b.expr,
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
                        expr: bin(
                            ExprOp::Times,
                            bin(ExprOp::Times, Expr::Num(1e6), Expr::I(meas)),
                            b.expr,
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
                        expr: bin(
                            ExprOp::Divide,
                            bin(ExprOp::Times, Expr::V(int2), b.expr),
                            Expr::Num(b.m.unwrap_or(1.0)),
                        ),
                    },
                    span,
                ),
            ]
        }
    }
}

/// The value of a keyword parameter that must be one number.
pub(crate) fn num(v: &ParamValue) -> Option<f64> {
    match v {
        ParamValue::Num(x) => Some(*x),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(s: &str) -> Result<Expr, String> {
        translate(s, &ParamEnv::default()).map_err(|e| e.msg)
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
            ("x+v(1)", "undefined parameter"),
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
    /// overflow a small stack (the thread has the 2 MiB of a default spawned thread).
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
                let e = translate(&s, &ParamEnv::default()).unwrap_err();
                assert!(e.msg.contains(what), "{}: {}", &s[..20], e.msg);
                assert!(e.start < s.len().max(1));
            }
        };
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(run)
            .unwrap()
            .join()
            .unwrap();
    }
}
