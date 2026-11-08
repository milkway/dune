//! B-source expressions: a port of the 3f5 parse-tree front end
//! (`corpus/spice3f5/src/lib/inp/inpptree.c`: `PTlexer`, the
//! operator-precedence `PTparse`, `mkfnode`/`mksnode`/`mkbnode` and
//! `PTcheck`). The result is the undifferentiated tree; derivatives,
//! folding and evaluation live in `dune-core` (`asrc.rs`).
//!
//! Grammar (operator precedence, `prectable` in the C): `+ -` bind
//! loosest and left-associate, then `* /`, then `^` (right-
//! associative), then unary `-`; parentheses group; `name(expr)`
//! calls one of the functions of `ptfuncs.c`; `v(n)`, `v(n1,n2)` and
//! `i(vname)` read node voltages and branch currents; `e` and `pi`
//! are constants. There is no unary `+`. The expression ends at the
//! end of the card or where a value would follow a value (`2 3`,
//! `v(1) i=…`), and the caller goes on reading card parameters from
//! there, as `INPdevParse` does.
//!
//! Deliberate differences from the C (all on inputs the C rejects or
//! mangles; see `docs/dune-netlist.md`):
//! - a number's scale factor is consumed (`2k`): the C's lexer stops
//!   on the `k`, which then fails as an unknown card parameter;
//! - inside `v(…)`/`i(…)` a name runs up to blank, `,`, `(` or `)`,
//!   as in ngspice (`v(1a)`, `v(n-1)`, `v(e)` name nodes; the C read
//!   `v(e)` as `v(2)` and `v(1.0)` as `v(1)`);
//! - the tree is bounded ([`MAX_DEPTH`], [`MAX_NODES`]) so hostile
//!   input cannot exhaust the stack of the recursive passes.

use crate::ir::{Expr, ExprFunc, ExprOp};
use crate::scan::evaluate_prefix;

/// Deepest tree accepted (operands nested inside operators).
pub const MAX_DEPTH: u32 = 512;
/// Largest tree accepted, in nodes.
pub const MAX_NODES: u32 = 10_000;
/// `PT_STACKSIZE`: the parser stack of the C.
const PT_STACKSIZE: usize = 200;

/// A malformed expression: message and byte range inside the text
/// given to the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprError {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

/// Parse a whole B-source expression (`v(1)*2`, `exp(i(vin)/1m)`).
/// Names are lowercased, as the 3f5 lowercases the whole deck; text
/// left after a complete expression is an error.
pub fn parse_expr(text: &str) -> Result<Expr, ExprError> {
    let (res, used) = parse_prefix(text);
    let expr = res?;
    let rest = &text[used..];
    if let Some(off) = rest.find(|c: char| !matches!(c, ' ' | '\t')) {
        return Err(ExprError {
            message: format!("unexpected `{}` after the expression", rest[off..].trim()),
            start: used + off,
            end: text.len(),
        });
    }
    Ok(expr)
}

/// Parse an expression at the start of `text` as far as it goes
/// (`INPgetTree`), returning it and the byte length consumed — the
/// card scan resumes there.
pub(crate) fn parse_prefix(text: &str) -> (Result<Expr, ExprError>, usize) {
    let lower = text.to_ascii_lowercase();
    let mut lx = Lexer {
        s: lower.as_bytes(),
        text: &lower,
        pos: 0,
        last: END,
        last_num: false,
        prev_vi: false,
        name_mode: false,
    };
    let res = parse(&mut lx).and_then(check);
    (res, lx.pos)
}

// Token numbers (`TOK_*`), also the `prectable` indices.
const END: u8 = 0;
const PLUS: u8 = 1;
const MINUS: u8 = 2;
const TIMES: u8 = 3;
const DIVIDE: u8 = 4;
const POWER: u8 = 5;
const UMINUS: u8 = 6;
const LPAREN: u8 = 7;
const RPAREN: u8 = 8;
const VALUE: u8 = 9;
const COMMA: u8 = 10;

const G: u8 = 1;
const L: u8 = 2;
const E: u8 = 3;
const R: u8 = 4;

/// `prectable[11][11]` of `inpptree.c`, rows = top terminal of the
/// stack, columns = incoming token.
#[rustfmt::skip]
const PREC: [[u8; 11]; 11] = [
    /*        $  +  -  *  /  ^  u- (  )  v  , */
    /* $ */ [ R, L, L, L, L, L, L, L, R, L, R ],
    /* + */ [ G, G, G, L, L, L, L, L, G, L, G ],
    /* - */ [ G, G, G, L, L, L, L, L, G, L, G ],
    /* * */ [ G, G, G, G, G, L, L, L, G, L, G ],
    /* / */ [ G, G, G, G, G, L, L, L, G, L, G ],
    /* ^ */ [ G, G, G, G, G, L, L, L, G, L, G ],
    /* u-*/ [ G, G, G, G, G, G, G, L, G, L, R ],
    /* ( */ [ R, L, L, L, L, L, L, L, E, L, L ],
    /* ) */ [ G, G, G, G, G, G, G, R, G, R, G ],
    /* v */ [ G, G, G, G, G, G, G, G, G, R, G ],
    /* , */ [ G, L, L, L, L, L, L, L, G, L, G ],
];

/// An intermediate parse node (`INPparseNode` before `PTcheck`):
/// placeholders and commas are legal only as `v()`/`i()` arguments.
#[derive(Debug)]
enum P {
    Num(f64),
    V(String),
    I(String),
    Bin(ExprOp, Box<Pn>, Box<Pn>),
    Comma(Box<Pn>, Box<Pn>),
    Func(ExprFunc, Box<Pn>),
    /// `PT_PLACEHOLDER`: a bare name, waiting to be a `v`/`i` argument.
    Placeholder(String),
}

#[derive(Debug)]
struct Pn {
    p: P,
    depth: u32,
    nodes: u32,
    start: usize,
    end: usize,
}

impl Pn {
    fn leaf(p: P, start: usize, end: usize) -> Pn {
        Pn {
            p,
            depth: 1,
            nodes: 1,
            start,
            end,
        }
    }
}

#[derive(Debug)]
enum Val {
    Num(f64),
    /// `TYP_STRING`: a bare word.
    Str(String),
    /// A `v(…)`/`i(…)` argument read as a name (Dune/ngspice).
    Name(String),
    /// `TYP_PNODE`: an already-reduced subtree.
    Node(Pn),
}

#[derive(Debug)]
struct El {
    tok: u8,
    val: Option<Val>,
    start: usize,
    end: usize,
}

fn err(message: impl Into<String>, start: usize, end: usize) -> ExprError {
    ExprError {
        message: message.into(),
        start,
        end,
    }
}

struct Lexer<'a> {
    s: &'a [u8],
    text: &'a str,
    pos: usize,
    /// `lasttoken`/`lasttype` of the C (static there; reset per tree
    /// here — a successful parse always leaves `TOK_END` behind).
    last: u8,
    last_num: bool,
    /// The last token was the word `v` or `i`.
    prev_vi: bool,
    /// Inside the parentheses of `v(`/`i(`.
    name_mode: bool,
}

impl Lexer<'_> {
    fn at_end(&self) -> bool {
        self.pos >= self.s.len() || self.s[self.pos] == 0
    }

    /// `PTlexer`.
    fn next(&mut self) -> El {
        while self.pos < self.s.len() && matches!(self.s[self.pos], b' ' | b'\t' | b'=') {
            self.pos += 1;
        }
        let start = self.pos;
        let after_value = self.last == VALUE || self.last == RPAREN;
        let mut val = None;
        let tok = if self.at_end() {
            END
        } else {
            let c = self.s[self.pos];
            if self.name_mode && !after_value && !matches!(c, b',' | b'(' | b')') {
                let mut e = self.pos;
                while e < self.s.len()
                    && !matches!(self.s[e], b' ' | b'\t' | b',' | b'(' | b')' | 0)
                {
                    e += 1;
                }
                val = Some(Val::Name(self.text[self.pos..e].to_string()));
                self.pos = e;
                VALUE
            } else {
                match c {
                    b',' => {
                        self.pos += 1;
                        COMMA
                    }
                    b'-' => {
                        self.pos += 1;
                        if after_value { MINUS } else { UMINUS }
                    }
                    b'+' => {
                        self.pos += 1;
                        PLUS
                    }
                    b'*' => {
                        self.pos += 1;
                        TIMES
                    }
                    b'/' => {
                        self.pos += 1;
                        DIVIDE
                    }
                    b'^' => {
                        self.pos += 1;
                        POWER
                    }
                    b'(' => {
                        if (self.last == VALUE && self.last_num) || self.last == RPAREN {
                            END
                        } else {
                            self.pos += 1;
                            if self.prev_vi {
                                self.name_mode = true;
                            }
                            LPAREN
                        }
                    }
                    b')' => {
                        self.pos += 1;
                        self.name_mode = false;
                        RPAREN
                    }
                    _ if after_value => END,
                    _ => {
                        if let Some((v, n)) = evaluate_prefix(&self.s[self.pos..]) {
                            self.pos += n;
                            val = Some(Val::Num(v));
                        } else {
                            let mut e = self.pos;
                            while e < self.s.len()
                                && !matches!(
                                    self.s[e],
                                    b' ' | b'\t'
                                        | b'('
                                        | b')'
                                        | b'^'
                                        | b'+'
                                        | b'-'
                                        | b'*'
                                        | b'/'
                                        | b','
                                        | 0
                                )
                            {
                                e += 1;
                            }
                            val = Some(Val::Str(self.text[self.pos..e].to_string()));
                            self.pos = e;
                        }
                        VALUE
                    }
                }
            }
        };
        self.last = tok;
        self.last_num = matches!(val, Some(Val::Num(_)));
        self.prev_vi = matches!(&val, Some(Val::Str(s)) if s == "v" || s == "i");
        El {
            tok,
            val,
            start,
            end: self.pos.max(start),
        }
    }
}

/// `PTparse`: operator-precedence parse of one expression.
fn parse(lx: &mut Lexer) -> Result<Pn, ExprError> {
    let mut stack: Vec<El> = vec![El {
        tok: END,
        val: None,
        start: 0,
        end: 0,
    }];
    let mut next = lx.next();
    loop {
        let sp = stack.len() - 1;
        if !(sp > 1 || next.tok != END) {
            break;
        }
        // The top-most terminal (stack[0] is `$`, so this stops).
        let mut i = sp;
        while stack[i].tok == VALUE {
            i -= 1;
        }
        match PREC[usize::from(stack[i].tok)][usize::from(next.tok)] {
            L | E => {
                if sp == PT_STACKSIZE - 1 {
                    return Err(err(
                        "expression too deeply nested (parser stack overflow)",
                        next.start,
                        next.end,
                    ));
                }
                stack.push(next);
                next = lx.next();
            }
            G => reduce(&mut stack)?,
            _ => {
                let (s, e) = if next.tok == END {
                    (stack[sp].start, stack[sp].end.max(next.end))
                } else {
                    (next.start, next.end)
                };
                return Err(err("syntax error in expression", s, e));
            }
        }
    }
    match stack.get_mut(1) {
        Some(el) => {
            let (s, e) = (el.start, el.end);
            makepnode(el)?.ok_or_else(|| err("syntax error in expression", s, e))
        }
        None => Err(err("empty expression", next.start, next.end)),
    }
}

/// The `G` branch of `PTparse`: find the handle and replace it with
/// one reduced value.
fn reduce(stack: &mut Vec<El>) -> Result<(), ExprError> {
    let st = stack.len() - 1;
    let mut sp = st;
    let syntax = |stack: &[El]| {
        let s = stack.get(1).map_or(0, |e| e.start);
        let e = stack.last().map_or(0, |e| e.end);
        err("syntax error in expression", s, e)
    };
    if stack[sp].tok == VALUE {
        sp -= 1;
    }
    while sp > 0 {
        let i = if stack[sp - 1].tok == VALUE { 2 } else { 1 };
        if sp < i {
            return Err(syntax(stack));
        }
        if PREC[usize::from(stack[sp - i].tok)][usize::from(stack[sp].tok)] == L {
            break;
        }
        sp -= i;
    }
    if sp >= 1 && stack[sp - 1].tok == VALUE {
        sp -= 1;
    }
    if sp == 0 {
        return Err(syntax(stack));
    }
    let (hs, he) = (stack[sp].start, stack[st].end);
    let pn = if st == sp {
        take_node(stack, st)?
    } else if stack[sp].tok == UMINUS && st == sp + 1 {
        let arg = take_node(stack, st)?;
        mkfnode("-", arg, hs, he)?
    } else if stack[sp].tok == LPAREN && stack[st].tok == RPAREN {
        take_node(stack, sp + 1)?
    } else if stack[sp + 1].tok == LPAREN && stack[st].tok == RPAREN {
        let arg = take_node(stack, sp + 2)?;
        let fname = match &stack[sp].val {
            Some(Val::Str(s) | Val::Name(s)) if stack[sp].tok == VALUE => s.clone(),
            _ => return Err(syntax(stack)),
        };
        mkfnode(&fname, arg, hs, he)?
    } else {
        let op = stack[sp + 1].tok;
        let l = take_node(stack, sp)?;
        let r = take_node(stack, st)?;
        mkbnode(op, l, r, hs, he)?
    };
    stack.truncate(sp);
    stack.push(El {
        tok: VALUE,
        val: Some(Val::Node(pn)),
        start: hs,
        end: he,
    });
    Ok(())
}

/// `makepnode` on `stack[i]`, which must be a value; anything else is
/// the C's `goto err`.
fn take_node(stack: &mut [El], i: usize) -> Result<Pn, ExprError> {
    let el = &mut stack[i];
    let (s, e) = (el.start, el.end);
    makepnode(el)?.ok_or_else(|| err("syntax error in expression", s, e))
}

/// `makepnode`: a value element becomes a parse node (`None` for a
/// non-value, the C's NULL).
fn makepnode(el: &mut El) -> Result<Option<Pn>, ExprError> {
    if el.tok != VALUE {
        return Ok(None);
    }
    let (s, e) = (el.start, el.end);
    Ok(match el.val.take() {
        Some(Val::Num(v)) => Some(Pn::leaf(P::Num(v), s, e)),
        Some(Val::Str(w)) => Some(mksnode(w, s, e)?),
        Some(Val::Name(w)) => Some(Pn::leaf(P::Placeholder(w), s, e)),
        Some(Val::Node(pn)) => Some(pn),
        None => None,
    })
}

/// `mksnode`: a bare word is a constant (`e`, `pi`), the special
/// signal `time`, or a placeholder for a `v()`/`i()` argument.
fn mksnode(w: String, s: usize, e: usize) -> Result<Pn, ExprError> {
    match w.as_str() {
        // `specSigList` of the 3f5 (`bin/config.c`): the parser
        // accepts it, but `ASRCsetup`/`ASRCload` have no case for an
        // `IF_STRING` variable and the load returns E_BADPARM.
        "time" => Err(err(
            "`time` is not available in a 3f5 B source (its load rejects it); \
             ngspice's time-dependent B sources are not ported",
            s,
            e,
        )),
        "e" => Ok(Pn::leaf(P::Num(std::f64::consts::E), s, e)),
        "pi" => Ok(Pn::leaf(P::Num(std::f64::consts::PI), s, e)),
        _ => Ok(Pn::leaf(P::Placeholder(w), s, e)),
    }
}

/// `mkbnode`.
fn mkbnode(op: u8, l: Pn, r: Pn, s: usize, e: usize) -> Result<Pn, ExprError> {
    let depth = 1 + l.depth.max(r.depth);
    let nodes = 1 + l.nodes + r.nodes;
    let (l, r) = (Box::new(l), Box::new(r));
    let node = match op {
        PLUS => P::Bin(ExprOp::Plus, l, r),
        MINUS => P::Bin(ExprOp::Minus, l, r),
        TIMES => P::Bin(ExprOp::Times, l, r),
        DIVIDE => P::Bin(ExprOp::Divide, l, r),
        POWER => P::Bin(ExprOp::Power, l, r),
        COMMA => P::Comma(l, r),
        _ => return Err(err("syntax error in expression", s, e)),
    };
    finish(node, depth, nodes, s, e)
}

fn finish(p: P, depth: u32, nodes: u32, s: usize, e: usize) -> Result<Pn, ExprError> {
    if depth > MAX_DEPTH {
        return Err(err(
            format!("expression nested deeper than {MAX_DEPTH} levels"),
            s,
            e,
        ));
    }
    if nodes > MAX_NODES {
        return Err(err(
            format!("expression larger than {MAX_NODES} terms"),
            s,
            e,
        ));
    }
    Ok(Pn {
        p,
        depth,
        nodes,
        start: s,
        end: e,
    })
}

/// `mkfnode`: `v(…)`, `i(…)` or a function call.
fn mkfnode(fname: &str, arg: Pn, s: usize, e: usize) -> Result<Pn, ExprError> {
    match fname {
        "v" => match arg.p {
            P::Placeholder(n) => Ok(Pn::leaf(P::V(n), s, e)),
            P::Num(c) => Ok(Pn::leaf(P::V(c_int(c).to_string()), s, e)),
            // `v(a,b)` → `v(a) - v(b)` (recursively, so `v(a,b,c)`
            // is `v(a) - v(b) - v(c)` as in the C).
            P::Comma(a, b) => {
                let (sa, ea, sb, eb) = (a.start, a.end, b.start, b.end);
                let l = mkfnode("v", *a, sa, ea)?;
                let r = mkfnode("v", *b, sb, eb)?;
                mkbnode(MINUS, l, r, s, e)
            }
            _ => Err(err("badly formed node voltage", s, e)),
        },
        "i" => match arg.p {
            P::Placeholder(n) => Ok(Pn::leaf(P::I(n), s, e)),
            P::Num(c) => Ok(Pn::leaf(P::I(c_int(c).to_string()), s, e)),
            _ => Err(err("badly formed branch current", s, e)),
        },
        "-" => {
            let (d, n) = (1 + arg.depth, 1 + arg.nodes);
            finish(P::Func(ExprFunc::Neg, Box::new(arg)), d, n, s, e)
        }
        _ => match ExprFunc::TABLE.iter().find(|(n, _)| *n == fname) {
            Some(&(_, f)) => {
                let (d, n) = (1 + arg.depth, 1 + arg.nodes);
                finish(P::Func(f, Box::new(arg)), d, n, s, e)
            }
            None => Err(err(format!("no such function `{fname}`"), s, e)),
        },
    }
}

/// The C's `(int) constant` for `sprintf("%d")` (saturating where the
/// C would be undefined).
fn c_int(c: f64) -> i64 {
    if c.is_nan() {
        0
    } else {
        c.trunc().clamp(-2.147_483_648e9, 2.147_483_647e9) as i64
    }
}

/// `PTcheck` + conversion: placeholders and commas left anywhere but
/// inside `v()`/`i()` make the tree invalid.
fn check(pn: Pn) -> Result<Expr, ExprError> {
    let (s, e) = (pn.start, pn.end);
    Ok(match pn.p {
        P::Num(v) => Expr::Num(v),
        P::V(n) => Expr::V(n),
        P::I(n) => Expr::I(n),
        P::Bin(op, l, r) => Expr::Bin(op, Box::new(check(*l)?), Box::new(check(*r)?)),
        P::Func(f, a) => Expr::Call(f, Box::new(check(*a)?)),
        P::Comma(..) => return Err(err("misplaced `,` in expression", s, e)),
        P::Placeholder(w) => {
            return Err(err(
                format!(
                    "unknown name `{w}` in expression (expected a number, e, pi, v(…), i(…) or a function call)"
                ),
                s,
                e,
            ));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Expr::*;

    fn p(s: &str) -> Expr {
        parse_expr(s).unwrap_or_else(|e| panic!("{s}: {e:?}"))
    }

    fn bin(op: ExprOp, l: Expr, r: Expr) -> Expr {
        Bin(op, Box::new(l), Box::new(r))
    }

    fn call(f: ExprFunc, a: Expr) -> Expr {
        Call(f, Box::new(a))
    }

    #[test]
    fn precedence_and_associativity() {
        use ExprOp::*;
        // + and - left-associative, below * and /.
        assert_eq!(
            p("1-2+3*4"),
            bin(
                Plus,
                bin(Minus, Num(1.0), Num(2.0)),
                bin(Times, Num(3.0), Num(4.0))
            )
        );
        // ^ binds tighter than * and is right-associative.
        assert_eq!(
            p("2*3^4^5"),
            bin(
                Times,
                Num(2.0),
                bin(Power, Num(3.0), bin(Power, Num(4.0), Num(5.0)))
            )
        );
        // Unary minus binds tighter than ^: -2^2 is (-2)^2.
        assert_eq!(
            p("-2^2"),
            bin(Power, call(ExprFunc::Neg, Num(2.0)), Num(2.0))
        );
        assert_eq!(
            p("2^-x(1)".replace("x", "v").as_str()),
            bin(Power, Num(2.0), call(ExprFunc::Neg, V("1".into())))
        );
    }

    #[test]
    fn functions_and_references() {
        use ExprOp::*;
        assert_eq!(
            p("cos(v(1))+sin(v(2))"),
            bin(
                Plus,
                call(ExprFunc::Cos, V("1".into())),
                call(ExprFunc::Sin, V("2".into()))
            )
        );
        assert_eq!(p("v(a,b)"), bin(Minus, V("a".into()), V("b".into())));
        assert_eq!(
            p("v(a,b,c)"),
            bin(
                Minus,
                bin(Minus, V("a".into()), V("b".into())),
                V("c".into())
            )
        );
        assert_eq!(p("I(VDD)"), I("vdd".into()));
        assert_eq!(
            p("exp(pi^i(vdd))"),
            call(
                ExprFunc::Exp,
                bin(Power, Num(std::f64::consts::PI), I("vdd".into()))
            )
        );
        assert_eq!(p("2k*v(1)"), bin(Times, Num(2000.0), V("1".into())));
        // Names inside v()/i() run to blank , ( ).
        assert_eq!(p("v(1a)"), V("1a".into()));
        assert_eq!(p("v(e)"), V("e".into()));
        assert_eq!(p("v( x1.n-2 )"), V("x1.n-2".into()));
        assert_eq!(p("e"), Num(std::f64::consts::E));
    }

    #[test]
    fn prefix_stops_where_a_value_follows_a_value() {
        let (r, used) = parse_prefix("v(1)*2 i=3");
        assert!(r.is_ok());
        assert_eq!(&"v(1)*2 i=3"[used..], "i=3");
        let (r, used) = parse_prefix("2(3)");
        assert_eq!(r, Ok(Num(2.0)));
        assert_eq!(used, 1);
    }

    #[test]
    fn errors_have_spans_and_never_panic() {
        for bad in [
            "",
            "+1",
            "1+",
            "(",
            ")",
            "()",
            "sin()",
            "foo(1)",
            "v()",
            "v(1)+",
            "v(sin(1))",
            "i(1,2)",
            "x",
            "1,2",
            "time",
            "v(",
            "((((",
            "1 + * 2",
            "-",
            "sin 1",
            "v(a,)",
        ] {
            let e = parse_expr(bad).expect_err(bad);
            assert!(
                e.start <= e.end && e.end <= bad.len().max(e.end),
                "{bad}: {e:?}"
            );
        }
        // Hostile depth / size.
        let deep = "(".repeat(5000) + "1" + &")".repeat(5000);
        assert!(parse_expr(&deep).is_err());
        let long = vec!["1"; 20_000].join("+");
        assert!(parse_expr(&long).is_err());
        let neg = "-".repeat(5000) + "1";
        assert!(parse_expr(&neg).is_err());
        let pow = vec!["2"; 3000].join("^");
        assert!(parse_expr(&pow).is_err());
    }
}
