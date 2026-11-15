//! The parameter expression language of `.param`, `{…}` and `'…'`
//! (ngspice's "numparam" extension; the 3f5 has none).
//!
//! This is a second language, distinct from the B-source expressions
//! of `expr.rs` (a port of the 3f5's `inpptree.c`). Its contract is
//! the ngspice manual plus the behaviour of ngspice-47 observed on
//! probe decks — every rule below cites its probe in
//! `docs/dune-netlist.md` ("Linguagem de expressões de parâmetros").
//! ngspice's own implementation (LGPL) was not consulted.
//!
//! Grammar, loosest first:
//!
//! ```text
//! ternary := or [ '?' or ':' ternary ]          right-assoc
//! or      := and { '||' and }
//! and     := cmp { '&&' cmp }
//! cmp     := add { ('=='|'='|'!='|'<>'|'<'|'>'|'<='|'>=') add }   one level, left-assoc
//! add     := mul { ('+'|'-') mul }
//! mul     := unary { ('*'|'/'|'%') unary }
//! unary   := ('-'|'+') power | power             only where a sign may start an operand
//! power   := not { ('^'|'**') not }              left-assoc, |base|^exp
//! not     := '!' not | primary
//! primary := number | '-' number | name | name '(' args ')' | '(' ternary ')'
//!          | '{' ternary '}' | "'" ternary "'"
//! ```
//!
//! The sign rule is ngspice's: at the start of the expression, after
//! `(` and after `,` a sign is a unary operator looser than `^`
//! (`-2^2 = -4`); after any other operator (`?` and `:` included) a
//! `-` must be immediately followed by a number and makes a negative
//! literal (`3*-2^2 = 12`, `1?-2^2:3 = 4`). ngspice rejects or
//! misreads the other forms (`2*-(3)`, `1<-a`, `2*+3`), so they are
//! errors here.
//!
//! An expression compiles to a small stack program ([`Expr`]); the
//! evaluator is a loop, never recursion, and the parser's recursion
//! is bounded by [`MAX_NEST`], so hostile input cannot overflow the
//! stack. Inputs longer than [`MAX_LEN`] are refused.

use std::cell::Cell;

use crate::func::{FuncScope, UserFn};
use crate::span::Span;

/// Longest accepted expression text (the open PDKs reach 613 bytes).
pub(crate) const MAX_LEN: usize = 64 * 1024;
/// Deepest accepted nesting of parentheses, calls, signs and `!`.
pub(crate) const MAX_NEST: usize = 100;
/// Deepest accepted chain of user-function (`.func`) calls in one
/// evaluation. Recursion is refused when the functions are defined
/// (`func.rs`); this bounds the evaluator's frame stack regardless.
pub(crate) const MAX_CALL_DEPTH: usize = 256;
/// Work (operations plus names resolved inside user-function bodies)
/// one deck may spend, all evaluations together: a chain of functions
/// that each call the next twice doubles the work per level, so the
/// budget, not the depth, is what stops it.
pub(crate) const MAX_FN_WORK: u64 = 1 << 24;

/// A parse or evaluation failure, with a byte range inside the
/// expression text it came from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PError {
    pub msg: String,
    pub start: usize,
    pub end: usize,
    /// Where the problem is when it is not inside the expression text
    /// itself: in the body of a user function it called.
    pub at: Option<Span>,
}

impl PError {
    pub(crate) fn new(msg: impl Into<String>, start: usize, end: usize) -> Self {
        PError {
            msg: msg.into(),
            start,
            end: end.max(start),
            at: None,
        }
    }

    /// An error whose diagnostic was already reported (a failed
    /// definition it depends on): callers stay silent.
    pub(crate) fn silenced(start: usize, end: usize) -> Self {
        PError::new(String::new(), start, end)
    }

    /// The same error seen from a call into function `f`: placed in
    /// `f`'s body (bytes `s..e` of it unless it already has a place).
    fn in_function(self, f: &UserFn, s: usize, e: usize) -> Self {
        if self.msg.is_empty() {
            return self;
        }
        PError {
            msg: format!("{} (in function `{}`)", self.msg, f.name),
            start: 0,
            end: 0,
            at: Some(self.at.unwrap_or_else(|| f.span_of(s, e))),
        }
    }
}

/// Built-in functions of ngspice-47's parameter language: exactly the
/// ones probed to exist (`docs/dune-netlist.md`). `round`, `trunc`,
/// `sign`, `atan2`, `hypot`, `fabs`, `u`, `uramp`, `if`, `db`,
/// `pwrs`, `random`, `mod`, `v`, `ddt` do not exist there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Func {
    Sqrt,
    Abs,
    Min,
    Max,
    Pow,
    Pwr,
    Exp,
    Log,
    Ln,
    Log10,
    Floor,
    Ceil,
    Int,
    Nint,
    Sgn,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Sqr,
    TernaryFcn,
    Agauss,
    Gauss,
    Aunif,
    Unif,
    Limit,
}

impl Func {
    fn lookup(name: &str) -> Option<Func> {
        use Func::*;
        Some(match name {
            "sqrt" => Sqrt,
            "abs" => Abs,
            "min" => Min,
            "max" => Max,
            "pow" => Pow,
            "pwr" => Pwr,
            "exp" => Exp,
            "log" => Log,
            "ln" => Ln,
            "log10" => Log10,
            "floor" => Floor,
            "ceil" => Ceil,
            "int" => Int,
            "nint" => Nint,
            "sgn" => Sgn,
            "sin" => Sin,
            "cos" => Cos,
            "tan" => Tan,
            "asin" => Asin,
            "acos" => Acos,
            "atan" => Atan,
            "sinh" => Sinh,
            "cosh" => Cosh,
            "tanh" => Tanh,
            "asinh" => Asinh,
            "acosh" => Acosh,
            "atanh" => Atanh,
            "sqr" => Sqr,
            "ternary_fcn" => TernaryFcn,
            "agauss" => Agauss,
            "gauss" => Gauss,
            "aunif" => Aunif,
            "unif" => Unif,
            "limit" => Limit,
            _ => return None,
        })
    }

    /// Accepted argument counts. ngspice-47 garbles wrong counts
    /// instead of rejecting them (`sqrt(1,2)` is `sqrt(2)`,
    /// `min(3,1,2)` is 2, `pow(2)` is 1); here they are errors.
    fn arity(self) -> (usize, usize) {
        use Func::*;
        match self {
            Min | Max | Pow | Pwr | Aunif | Unif | Limit => (2, 2),
            TernaryFcn => (3, 3),
            Agauss | Gauss => (2, 3),
            _ => (1, 1),
        }
    }

    pub(crate) fn is_random(self) -> bool {
        matches!(
            self,
            Func::Agauss | Func::Gauss | Func::Aunif | Func::Unif | Func::Limit
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Num(f64),
    /// Index into [`Expr::vars`].
    Var(u32),
    Neg,
    Not,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    /// Callee, argument count, index into [`Expr::calls`].
    Call(Callee, u16, u32),
    /// Pop; jump to the target when the value is exactly zero.
    JmpIfZero(u32),
    Jmp(u32),
}

/// What a call names. A built-in may still be shadowed by a user
/// function (`.func sqrt(x) …` wins in ngspice-47, probe `f1
/// f_shadow_builtin`), so the choice is made when evaluating.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Callee {
    Builtin(Func),
    /// Not a built-in: a user function, or an unknown one.
    Named,
}

/// A call in an expression: the function name (lowercased) and its
/// byte range.
#[derive(Clone, Debug)]
pub(crate) struct CallSite {
    pub name: Box<str>,
    pub start: u32,
    pub end: u32,
}

/// A name used by an expression (lowercased), with its byte range.
#[derive(Clone, Debug)]
pub(crate) struct VarRef {
    pub name: Box<str>,
    pub start: u32,
    pub end: u32,
}

/// A compiled parameter expression.
#[derive(Clone, Debug)]
pub(crate) struct Expr {
    code: Vec<Op>,
    vars: Vec<VarRef>,
    /// Each call's function name and byte range.
    calls: Vec<CallSite>,
}

/// How the random functions (`agauss gauss aunif unif limit`) are
/// evaluated (D-0065, D-0075). ngspice-47 draws on every run, seeded
/// by `.options seed=N` or by the clock; Dune evaluates at the nominal
/// value unless the deck (or the caller, `Netlist::with_seed`) gives
/// a seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RandomPolicy {
    /// Every random function returns its nominal (first) argument.
    Nominal,
    /// Draws keyed by the seed and the place of the draw.
    Seeded(u64),
}

/// State shared by the evaluations of one deck.
///
/// **Draw keys.** A seeded draw is not taken from a running stream
/// (whose position would depend on the order of evaluation) but is a
/// pure function of `(seed, root, n)`: `root` names the evaluation
/// that asked for it — a value on a card, a `.param` definition, an
/// `X`-line actual, a `.model` parameter: the instance path and the
/// byte position of the value ([`EvalCtx::with_root`]) — and `n`
/// counts the draws made inside that evaluation, in program order
/// (a random `.param` read twice draws twice). Evaluating the same
/// value again — the global scope resolved a second time at the
/// circuit temperature, a re-expansion at another temperature
/// (`Netlist::at_temperature`) — repeats its draws exactly; distinct
/// values, or one card in two subcircuit instances, draw
/// independently. Nothing depends on hash-map order, threads or the
/// platform: the mixing is integer arithmetic and the normal deviate
/// uses `libm` (pure Rust, the same bits on every target, wasm
/// included).
#[derive(Debug)]
pub(crate) struct EvalCtx {
    pub policy: RandomPolicy,
    /// Key of the evaluation being computed (see above).
    draw_root: Cell<u64>,
    /// Draws made so far inside it.
    draw_n: Cell<u64>,
    /// Nesting of [`EvalCtx::with_root`] calls: only the outermost
    /// sets the root.
    root_depth: Cell<u32>,
    /// The card being read, when draws are on: a hash of its text and
    /// where it starts (`ParamEnv::with_card`).
    pub card: Cell<Option<(u64, u32, u32)>>,
    /// Random `.param`s being re-evaluated inside each other right now
    /// (`param.rs`, bounded by [`MAX_RANDOM_DEPTH`]).
    pub random_depth: Cell<u32>,
    /// Set once any random function has been evaluated.
    pub random_used: Cell<bool>,
    /// Work spent in user-function bodies so far ([`MAX_FN_WORK`]).
    pub fn_work: Cell<u64>,
    /// How many user-function bodies are resolving names right now.
    pub in_body: Cell<u32>,
    /// Set when an evaluation read `temper` (directly or through a
    /// parameter that depends on it).
    pub temper_touched: Cell<bool>,
}

/// Most random `.param`s re-evaluated inside one another (each
/// reference to one evaluates its expression again, D-0075).
pub(crate) const MAX_RANDOM_DEPTH: u32 = 64;

/// SplitMix64's finalizer: a bijective 64-bit mix.
pub(crate) fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fold `x` into the key `h`.
pub(crate) fn mix_in(h: u64, x: u64) -> u64 {
    mix64(h ^ mix64(x.wrapping_add(0x9E37_79B9_7F4A_7C15)))
}

/// The uniforms of one draw: SplitMix64 from the draw's key.
struct Stream(u64);

impl Stream {
    /// Uniform on `[0, 1)`, 53 bits.
    fn uniform(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        (mix64(self.0) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Standard normal, Marsaglia's polar method (`libm` for the
    /// logarithm and the root: the same bits on every platform). The
    /// rejection loop is capped; 64 rejections in a row (probability
    /// 0.215⁶⁴) give 0.
    fn normal(&mut self) -> f64 {
        for _ in 0..64 {
            let u = 2.0 * self.uniform() - 1.0;
            let v = 2.0 * self.uniform() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                return u * libm::sqrt(-2.0 * libm::log(s) / s);
            }
        }
        0.0
    }
}

impl EvalCtx {
    pub fn new(policy: RandomPolicy) -> Self {
        EvalCtx {
            policy,
            draw_root: Cell::new(0),
            draw_n: Cell::new(0),
            root_depth: Cell::new(0),
            card: Cell::new(None),
            random_depth: Cell::new(0),
            random_used: Cell::new(false),
            fn_work: Cell::new(0),
            in_body: Cell::new(0),
            temper_touched: Cell::new(false),
        }
    }

    /// Spend `units` of the user-function budget; `false` once spent.
    pub fn charge(&self, units: u64) -> bool {
        let w = self.fn_work.get().saturating_add(units);
        self.fn_work.set(w);
        w <= MAX_FN_WORK
    }

    /// Run `f` as the evaluation keyed `root` (see the type's notes),
    /// unless one is already running (then its draws continue that
    /// one's count).
    pub fn with_root<R>(&self, root: u64, f: impl FnOnce() -> R) -> R {
        let depth = self.root_depth.get();
        if depth == 0 {
            self.draw_root.set(root);
            self.draw_n.set(0);
        }
        self.root_depth.set(depth + 1);
        let r = f();
        self.root_depth.set(depth);
        r
    }

    /// The stream of the next draw of the running evaluation.
    fn next_stream(&self, seed: u64) -> Stream {
        let n = self.draw_n.get();
        self.draw_n.set(n.wrapping_add(1));
        Stream(mix_in(mix_in(mix64(seed), self.draw_root.get()), n))
    }
}

impl Expr {
    /// Names the expression reads, in order of appearance.
    pub fn vars(&self) -> &[VarRef] {
        &self.vars
    }

    /// Size of the compiled program (a work unit for budgets).
    pub fn size(&self) -> usize {
        self.code.len()
    }

    /// The calls the expression makes, in order of appearance.
    pub fn calls(&self) -> &[CallSite] {
        &self.calls
    }

    /// Byte range of the first random-function call, if any.
    pub fn first_random_call(&self) -> Option<(usize, usize)> {
        self.code.iter().find_map(|op| match op {
            Op::Call(Callee::Builtin(f), _, i) if f.is_random() => {
                let c = &self.calls[*i as usize];
                Some((c.start as usize, c.end as usize))
            }
            _ => None,
        })
    }

    /// Evaluate without user functions (see [`Expr::eval_in`]).
    #[cfg(test)]
    pub fn eval(
        &self,
        lookup: &mut dyn FnMut(&VarRef) -> Result<f64, PError>,
        ctx: &EvalCtx,
    ) -> Result<f64, PError> {
        self.eval_in(lookup, ctx, None)
    }

    /// Evaluate. `lookup` resolves a (lowercased) name; every name is
    /// resolved before anything is computed, so an undefined name is
    /// an error even in an untaken `?:` branch (ngspice-47 behaves the
    /// same way); `lookup` builds the error for a missing name.
    ///
    /// `funcs` is the user-function scope (`.func`). A call to one
    /// pushes a frame whose formal arguments take the call's values
    /// and whose other names go through `lookup` — the scope of the
    /// call site, as ngspice's textual expansion has it (probe `f1
    /// f_local_sees_inst_param`); calls inside a body resolve in the
    /// scope that defined it, then in `funcs`. Frames live on the heap: the evaluator
    /// never recurses, and depth and work are bounded
    /// ([`MAX_CALL_DEPTH`], [`MAX_FN_WORK`]).
    pub fn eval_in<'s>(
        &'s self,
        lookup: &mut dyn FnMut(&VarRef) -> Result<f64, PError>,
        ctx: &EvalCtx,
        funcs: Option<&'s FuncScope<'s>>,
    ) -> Result<f64, PError> {
        struct Frame<'s> {
            expr: &'s Expr,
            pc: usize,
            vals: Vec<f64>,
            /// Value stack height when the frame was entered.
            base: usize,
            scope: Option<&'s FuncScope<'s>>,
            func: Option<&'s UserFn<'s>>,
        }
        /// An error at bytes `s..e` of the frame's expression: inside a
        /// function body it points at the body and names the function.
        fn at(fr: &Frame, msg: String, s: usize, e: usize) -> PError {
            let pe = PError::new(msg, s, e);
            match fr.func {
                Some(f) => pe.in_function(f, s, e),
                None => pe,
            }
        }

        let mut vals = Vec::with_capacity(self.vars.len());
        for v in &self.vars {
            vals.push(lookup(v)?);
        }
        let mut frames: Vec<Frame<'s>> = vec![Frame {
            expr: self,
            pc: 0,
            vals,
            base: 0,
            scope: funcs,
            func: None,
        }];
        let mut st: Vec<f64> = Vec::with_capacity(16);
        // The compiler only emits well-formed programs, but a stack
        // underflow must still not panic.
        macro_rules! pop {
            () => {
                st.pop().unwrap_or(f64::NAN)
            };
        }
        loop {
            let depth = frames.len();
            let Some(fr) = frames.last_mut() else {
                return Ok(f64::NAN);
            };
            if fr.pc >= fr.expr.code.len() {
                // Frame done: its value replaces its stack segment.
                let v = if st.len() > fr.base { pop!() } else { f64::NAN };
                st.truncate(fr.base);
                frames.pop();
                if frames.is_empty() {
                    return Ok(v);
                }
                st.push(v);
                continue;
            }
            let op = fr.expr.code[fr.pc];
            fr.pc += 1;
            match op {
                Op::Num(x) => st.push(x),
                Op::Var(i) => st.push(fr.vals.get(i as usize).copied().unwrap_or(f64::NAN)),
                Op::Neg => {
                    let a = pop!();
                    st.push(-a);
                }
                Op::Not => {
                    let a = pop!();
                    st.push(truth(a == 0.0));
                }
                Op::JmpIfZero(t) => {
                    if pop!() == 0.0 {
                        fr.pc = t as usize;
                    }
                }
                Op::Jmp(t) => fr.pc = t as usize,
                Op::Call(callee, argc, ci) => {
                    let n = argc as usize;
                    let site = &fr.expr.calls[ci as usize];
                    let (s, e) = (site.start as usize, site.end as usize);
                    // A call inside a body resolves where the body was
                    // defined, then — the expanded text lands there — at
                    // the call site (probe `f2 f_top_calls_local`).
                    let user = fr
                        .scope
                        .and_then(|sc| sc.find(&site.name))
                        .or_else(|| funcs.and_then(|sc| sc.find(&site.name)));
                    let Some((f, fscope)) = user else {
                        let Callee::Builtin(b) = callee else {
                            let msg = format!("unknown function `{}`", site.name);
                            return Err(at(fr, msg, s, e));
                        };
                        // ngspice-47 garbles wrong counts instead of
                        // rejecting them (`sqrt(1,2)` is `sqrt(2)`); here
                        // they are errors.
                        let (lo, hi) = b.arity();
                        if n < lo || n > hi {
                            let want = if lo == hi {
                                format!("{lo}")
                            } else {
                                format!("{lo} or {hi}")
                            };
                            let msg = format!("`{}` takes {want} argument(s), got {n}", site.name);
                            return Err(at(fr, msg, s, e));
                        }
                        let from = st.len().saturating_sub(n);
                        let v = call(b, &st[from..], ctx);
                        st.truncate(from);
                        st.push(v);
                        continue;
                    };
                    if f.recursive.get() {
                        // Reported where the function is defined.
                        return Err(PError::silenced(s, e));
                    }
                    if n != f.formals.len() {
                        let msg = format!(
                            "function `{}` takes {} argument(s), got {n}",
                            f.name,
                            f.formals.len()
                        );
                        return Err(at(fr, msg, s, e));
                    }
                    let body = match &f.body {
                        Ok(b) => b,
                        Err(pe) => return Err(pe.clone().in_function(f, pe.start, pe.end)),
                    };
                    if depth >= MAX_CALL_DEPTH {
                        let msg =
                            format!("user functions nested deeper than {MAX_CALL_DEPTH} calls");
                        return Err(at(fr, msg, s, e));
                    }
                    if !ctx.charge(1 + (body.code.len() + body.vars.len()) as u64) {
                        let msg = format!(
                            "user-function evaluation exceeds {MAX_FN_WORK} steps; giving up"
                        );
                        return Err(at(fr, msg, s, e));
                    }
                    let base = st.len().saturating_sub(n);
                    let mut vals = Vec::with_capacity(body.vars.len());
                    for v in &body.vars {
                        match f.formals.iter().position(|a| **a == *v.name) {
                            Some(k) => vals.push(st.get(base + k).copied().unwrap_or(f64::NAN)),
                            None => {
                                ctx.in_body.set(ctx.in_body.get() + 1);
                                let r = lookup(v);
                                ctx.in_body.set(ctx.in_body.get().saturating_sub(1));
                                match r {
                                    Ok(x) => vals.push(x),
                                    Err(pe) => {
                                        let (vs, ve) = (v.start as usize, v.end as usize);
                                        return Err(pe.in_function(f, vs, ve));
                                    }
                                }
                            }
                        }
                    }
                    st.truncate(base);
                    frames.push(Frame {
                        expr: body,
                        pc: 0,
                        vals,
                        base,
                        scope: Some(fscope),
                        func: Some(f),
                    });
                }
                bin => {
                    let b = pop!();
                    let a = pop!();
                    st.push(binary(bin, a, b));
                }
            }
        }
    }
}

fn truth(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// `a^b` and `a**b`: the magnitude of the base is raised (probe:
/// `(-2)^3 = 8`, `(-2)^2.5 = 5.657`, `(-8)^(1/3) = 2`), like `pwr`.
fn pwr(a: f64, b: f64) -> f64 {
    a.abs().powf(b)
}

fn binary(op: Op, a: f64, b: f64) -> f64 {
    match op {
        Op::Add => a + b,
        Op::Sub => a - b,
        Op::Mul => a * b,
        Op::Div => a / b,
        Op::Mod => a % b, // C fmod: `7.5%2 = 1.5`, `-7%3 = -1`, `7%0 = nan`
        Op::Pow => pwr(a, b),
        Op::Eq => truth(a == b),
        Op::Ne => truth(a != b),
        Op::Lt => truth(a < b),
        Op::Gt => truth(a > b),
        Op::Le => truth(a <= b),
        Op::Ge => truth(a >= b),
        Op::And => truth(a != 0.0 && b != 0.0),
        Op::Or => truth(a != 0.0 || b != 0.0),
        _ => f64::NAN,
    }
}

fn call(f: Func, a: &[f64], ctx: &EvalCtx) -> f64 {
    let x = a.first().copied().unwrap_or(f64::NAN);
    let y = a.get(1).copied().unwrap_or(f64::NAN);
    use Func::*;
    match f {
        Sqrt => x.sqrt(),
        Abs => x.abs(),
        Min => x.min(y),
        Max => x.max(y),
        Pow => x.powf(y), // C pow: `pow(-2,3) = -8`, `pow(-8,1/3) = nan`
        Pwr => pwr(x, y), // `pwr(-2,3) = 8`
        Exp => x.exp(),
        Log | Ln => x.ln(), // `log` is natural (`log(100) = 4.605`)
        Log10 => x.log10(),
        Floor => x.floor(),
        Ceil => x.ceil(),
        Int => x.trunc(),            // `int(-2.7) = -2`
        Nint => x.round_ties_even(), // `nint(2.5) = 2`, `nint(3.5) = 4`, `nint(-2.5) = -2`
        Sgn => {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                x // 0 (and NaN stays NaN)
            }
        }
        Sin => x.sin(),
        Cos => x.cos(),
        Tan => x.tan(),
        Asin => x.asin(),
        Acos => x.acos(),
        Atan => x.atan(),
        Sinh => x.sinh(),
        Cosh => x.cosh(),
        Tanh => x.tanh(),
        Asinh => x.asinh(),
        Acosh => x.acosh(),
        Atanh => x.atanh(),
        Sqr => x * x,
        TernaryFcn => {
            if x != 0.0 {
                y
            } else {
                a.get(2).copied().unwrap_or(f64::NAN)
            }
        }
        Agauss | Gauss | Aunif | Unif | Limit => {
            ctx.random_used.set(true);
            match ctx.policy {
                RandomPolicy::Nominal => x,
                RandomPolicy::Seeded(seed) => draw(f, a, ctx, seed),
            }
        }
    }
}

/// One draw, with the meanings the ngspice manual gives:
/// `agauss(nom, avar, sigma)` = nom + avar/sigma·N(0,1), `gauss(nom,
/// rvar, sigma)` = nom + nom·rvar/sigma·N(0,1) (relative), `aunif(nom,
/// avar)` uniform on nom ± avar, `unif(nom, rvar)` uniform on
/// nom·(1 ± rvar), `limit(nom, avar)` either `nom-avar` or `nom+avar`
/// with probability ½ each. The two-argument `agauss`/`gauss` gives
/// the nominal value, as ngspice-47 does (probes `temper-seed-probes
/// t3 c_two_args`, `c_gauss_two_args`), and draws nothing.
fn draw(f: Func, a: &[f64], ctx: &EvalCtx, seed: u64) -> f64 {
    let nom = a.first().copied().unwrap_or(f64::NAN);
    let var = a.get(1).copied().unwrap_or(f64::NAN);
    if matches!(f, Func::Agauss | Func::Gauss) && a.len() < 3 {
        return nom;
    }
    let sigma = a.get(2).copied().unwrap_or(1.0);
    let mut st = ctx.next_stream(seed);
    match f {
        Func::Agauss => nom + var / sigma * st.normal(),
        Func::Gauss => nom + nom * var / sigma * st.normal(),
        Func::Aunif => nom + var * (2.0 * st.uniform() - 1.0),
        Func::Unif => nom + nom * var * (2.0 * st.uniform() - 1.0),
        Func::Limit => {
            if st.uniform() < 0.5 {
                nom - var
            } else {
                nom + var
            }
        }
        _ => nom,
    }
}

// ---------------------------------------------------------------- lexer

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret, // `^` or `**`
    LParen,
    RParen,
    LBrace,
    RBrace,
    /// `'`: opens a group in operand position, closes it after one.
    Quote,
    Comma,
    Question,
    Colon,
    Bang,
    Eq, // `==` or `=`
    Ne, // `!=` or `<>`
    Lt,
    Gt,
    Le,
    Ge,
    AndAnd,
    OrOr,
    End,
}

#[derive(Clone, Copy, Debug)]
struct Lexed {
    tok: Tok,
    start: usize,
    end: usize,
}

/// Read an unsigned number at `b[i]` the way ngspice-47 reads one
/// inside an expression: decimal mantissa, optional exponent (only
/// when digits follow the `e`), then an optional run of letters
/// whose start picks the scale (`meg` 1e6, else `t g k m u n p f a`;
/// `mil` is *milli* here, `x` and other letters scale by 1), and the
/// value is the correctly rounded decimal (probes: `1.1k` is exactly
/// 1100, `0.3m` exactly 3e-4). A digit, `.` or `_` right after the
/// letters is an error (`2d3`; ngspice has no Fortran exponent here),
/// and so is a hexadecimal literal (ngspice's `0x10` is 16 — a known
/// difference).
fn lex_number(b: &[u8], i0: usize) -> Result<(f64, usize), PError> {
    let mut i = i0;
    let mut digits = String::new();
    let mut frac_digits: i64 = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        digits.push(b[i] as char);
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            digits.push(b[i] as char);
            frac_digits += 1;
            i += 1;
        }
    }
    if digits.is_empty() {
        return Err(PError::new("malformed number", i0, i));
    }
    let mut exp: i64 = 0;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        let mut neg = false;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            neg = b[j] == b'-';
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            let mut e: i64 = 0;
            while j < b.len() && b[j].is_ascii_digit() {
                e = (e * 10 + i64::from(b[j] - b'0')).min(100_000);
                j += 1;
            }
            exp = if neg { -e } else { e };
            i = j;
        }
    }
    let letters_start = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    let letters = &b[letters_start..i];
    if i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.' || b[i] == b'_') {
        let msg = if letters.eq_ignore_ascii_case(b"x") && digits == "0" {
            "hexadecimal numbers are not supported"
        } else {
            "malformed number"
        };
        return Err(PError::new(msg, i0, i + 1));
    }
    let scale: i64 = if letters.len() >= 3 && letters[..3].eq_ignore_ascii_case(b"meg") {
        6
    } else {
        match letters.first().map(u8::to_ascii_lowercase) {
            Some(b't') => 12,
            Some(b'g') => 9,
            Some(b'k') => 3,
            Some(b'm') => -3,
            Some(b'u') => -6,
            Some(b'n') => -9,
            Some(b'p') => -12,
            Some(b'f') => -15,
            Some(b'a') => -18,
            _ => 0,
        }
    };
    let e = exp - frac_digits + scale;
    // The mantissa digits are an integer; `digits e E` is parsed with
    // correct rounding. Huge exponents saturate to 0 / inf.
    let text = format!("{digits}e{e}");
    let v = text.parse::<f64>().unwrap_or(f64::NAN);
    Ok((v, i))
}

struct Lexer<'a> {
    src: &'a str,
    b: &'a [u8],
    pos: usize,
    peeked: Option<Lexed>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            src,
            b: src.as_bytes(),
            pos: 0,
            peeked: None,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.b.len() && matches!(self.b[self.pos], b' ' | b'\t' | b'\r' | b'\n') {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Result<Lexed, PError> {
        if let Some(l) = self.peeked {
            return Ok(l);
        }
        let l = self.lex()?;
        self.peeked = Some(l);
        Ok(l)
    }

    fn next(&mut self) -> Result<Lexed, PError> {
        let l = self.peek()?;
        self.peeked = None;
        Ok(l)
    }

    /// True when the next significant byte is a digit, or a `.`
    /// followed by a digit (a negative literal may follow a `-`).
    fn digit_follows(&self, at: usize) -> bool {
        match self.b.get(at) {
            Some(c) if c.is_ascii_digit() => true,
            Some(b'.') => self.b.get(at + 1).is_some_and(u8::is_ascii_digit),
            _ => false,
        }
    }

    fn lex(&mut self) -> Result<Lexed, PError> {
        self.skip_ws();
        let s = self.pos;
        let b = self.b;
        if s >= b.len() {
            return Ok(Lexed {
                tok: Tok::End,
                start: s,
                end: s,
            });
        }
        let c = b[s];
        let two = |x: u8| b.get(s + 1) == Some(&x);
        let (tok, len) = match c {
            b'0'..=b'9' | b'.' => {
                let (v, e) = lex_number(b, s)?;
                self.pos = e;
                return Ok(Lexed {
                    tok: Tok::Num(v),
                    start: s,
                    end: e,
                });
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let mut e = s + 1;
                while e < b.len() && (b[e].is_ascii_alphanumeric() || b[e] == b'_') {
                    e += 1;
                }
                self.pos = e;
                return Ok(Lexed {
                    tok: Tok::Ident,
                    start: s,
                    end: e,
                });
            }
            b'+' => (Tok::Plus, 1),
            b'-' => (Tok::Minus, 1),
            b'*' if two(b'*') => (Tok::Caret, 2),
            b'*' => (Tok::Star, 1),
            b'/' => (Tok::Slash, 1),
            b'%' => (Tok::Percent, 1),
            b'^' => (Tok::Caret, 1),
            b'(' => (Tok::LParen, 1),
            b')' => (Tok::RParen, 1),
            b'{' => (Tok::LBrace, 1),
            b'}' => (Tok::RBrace, 1),
            b'\'' => (Tok::Quote, 1),
            b',' => (Tok::Comma, 1),
            b'?' => (Tok::Question, 1),
            b':' => (Tok::Colon, 1),
            b'!' if two(b'=') => (Tok::Ne, 2),
            b'!' => (Tok::Bang, 1),
            b'=' if two(b'=') => (Tok::Eq, 2),
            b'=' => (Tok::Eq, 1), // a single `=` compares too (probe: `1=1` is 1)
            b'<' if two(b'=') => (Tok::Le, 2),
            b'<' if two(b'>') => (Tok::Ne, 2),
            b'<' => (Tok::Lt, 1),
            b'>' if two(b'=') => (Tok::Ge, 2),
            b'>' => (Tok::Gt, 1),
            b'&' if two(b'&') => (Tok::AndAnd, 2),
            b'|' if two(b'|') => (Tok::OrOr, 2),
            _ => {
                let ch = self.src[s..].chars().next().unwrap_or('?');
                let e = s + ch.len_utf8();
                return Err(PError::new(
                    format!("unexpected `{ch}` in expression"),
                    s,
                    e,
                ));
            }
        };
        self.pos = s + len;
        Ok(Lexed {
            tok,
            start: s,
            end: s + len,
        })
    }
}

// --------------------------------------------------------------- parser

/// Where an operand starts, which decides what a sign means there.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    /// Expression start, after `(` or `,`: a sign is a unary operator.
    Start,
    /// After an operator, `?`, `:` or a unary sign: only `-number`.
    AfterOp,
    /// After `!`: no sign at all (ngspice rejects `!-1`).
    AfterNot,
}

struct Parser<'a> {
    lx: Lexer<'a>,
    code: Vec<Op>,
    vars: Vec<VarRef>,
    calls: Vec<CallSite>,
    depth: usize,
}

/// Compile `src` (the text between the delimiters).
pub(crate) fn parse(src: &str) -> Result<Expr, PError> {
    if src.len() > MAX_LEN {
        return Err(PError::new(
            format!("parameter expression longer than {MAX_LEN} bytes"),
            0,
            0,
        ));
    }
    let mut p = Parser {
        lx: Lexer::new(src),
        code: Vec::new(),
        vars: Vec::new(),
        calls: Vec::new(),
        depth: 0,
    };
    let first = p.lx.peek()?;
    if first.tok == Tok::End {
        return Err(PError::new("empty parameter expression", 0, src.len()));
    }
    p.ternary(Ctx::Start)?;
    let t = p.lx.next()?;
    if t.tok != Tok::End {
        return Err(PError::new(
            format!("unexpected `{}` in expression", &src[t.start..t.end]),
            t.start,
            t.end,
        ));
    }
    Ok(Expr {
        code: p.code,
        vars: p.vars,
        calls: p.calls,
    })
}

impl Parser<'_> {
    fn enter(&mut self, at: usize) -> Result<(), PError> {
        self.depth += 1;
        if self.depth > MAX_NEST {
            return Err(PError::new(
                format!("parameter expression nested deeper than {MAX_NEST} levels"),
                at,
                at + 1,
            ));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn text(&self, l: &Lexed) -> &str {
        &self.lx.src[l.start..l.end]
    }

    fn unexpected(&self, l: &Lexed) -> PError {
        if l.tok == Tok::End {
            PError::new(
                "expression ends where a value was expected",
                l.start.saturating_sub(1),
                l.start,
            )
        } else {
            PError::new(
                format!("unexpected `{}` in expression", self.text(l)),
                l.start,
                l.end,
            )
        }
    }

    fn patch(&mut self, at: usize) {
        let target = self.code.len() as u32;
        match &mut self.code[at] {
            Op::JmpIfZero(t) | Op::Jmp(t) => *t = target,
            _ => {}
        }
    }

    fn ternary(&mut self, ctx: Ctx) -> Result<(), PError> {
        let here = self.lx.peek()?.start;
        self.enter(here)?;
        self.or(ctx)?;
        let q = self.lx.peek()?;
        if q.tok == Tok::Question {
            self.lx.next()?;
            let jz = self.code.len();
            self.code.push(Op::JmpIfZero(0));
            self.or(Ctx::AfterOp)?;
            let c = self.lx.next()?;
            match c.tok {
                Tok::Colon => {}
                Tok::Question => {
                    return Err(PError::new(
                        "a `?:` inside the middle operand of another `?:` must be \
                         parenthesized (ngspice-47 misreads it)",
                        c.start,
                        c.end,
                    ));
                }
                _ => {
                    return Err(PError::new("`?` without its `:`", q.start, q.end));
                }
            }
            let j = self.code.len();
            self.code.push(Op::Jmp(0));
            self.patch(jz);
            self.ternary(Ctx::AfterOp)?;
            self.patch(j);
        }
        self.leave();
        Ok(())
    }

    fn binary_level(
        &mut self,
        ctx: Ctx,
        next: fn(&mut Self, Ctx) -> Result<(), PError>,
        op_of: fn(Tok) -> Option<Op>,
    ) -> Result<(), PError> {
        next(self, ctx)?;
        loop {
            let t = self.lx.peek()?;
            let Some(op) = op_of(t.tok) else { break };
            self.lx.next()?;
            next(self, Ctx::AfterOp)?;
            self.code.push(op);
        }
        Ok(())
    }

    fn or(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.binary_level(ctx, Self::and, |t| (t == Tok::OrOr).then_some(Op::Or))
    }

    fn and(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.binary_level(ctx, Self::cmp, |t| (t == Tok::AndAnd).then_some(Op::And))
    }

    fn cmp(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.binary_level(ctx, Self::add, |t| match t {
            Tok::Eq => Some(Op::Eq),
            Tok::Ne => Some(Op::Ne),
            Tok::Lt => Some(Op::Lt),
            Tok::Gt => Some(Op::Gt),
            Tok::Le => Some(Op::Le),
            Tok::Ge => Some(Op::Ge),
            _ => None,
        })
    }

    fn add(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.binary_level(ctx, Self::mul, |t| match t {
            Tok::Plus => Some(Op::Add),
            Tok::Minus => Some(Op::Sub),
            _ => None,
        })
    }

    fn mul(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.binary_level(ctx, Self::unary, |t| match t {
            Tok::Star => Some(Op::Mul),
            Tok::Slash => Some(Op::Div),
            Tok::Percent => Some(Op::Mod),
            _ => None,
        })
    }

    fn unary(&mut self, ctx: Ctx) -> Result<(), PError> {
        let t = self.lx.peek()?;
        if ctx == Ctx::Start && matches!(t.tok, Tok::Minus | Tok::Plus) {
            self.lx.next()?;
            self.enter(t.start)?;
            self.power(Ctx::AfterOp)?;
            self.leave();
            if t.tok == Tok::Minus {
                self.code.push(Op::Neg);
            }
            return Ok(());
        }
        self.power(ctx)
    }

    fn power(&mut self, ctx: Ctx) -> Result<(), PError> {
        self.not(ctx)?;
        while self.lx.peek()?.tok == Tok::Caret {
            self.lx.next()?;
            self.not(Ctx::AfterOp)?;
            self.code.push(Op::Pow);
        }
        Ok(())
    }

    fn not(&mut self, ctx: Ctx) -> Result<(), PError> {
        let t = self.lx.peek()?;
        if t.tok == Tok::Bang {
            self.lx.next()?;
            self.enter(t.start)?;
            self.not(Ctx::AfterNot)?;
            self.leave();
            self.code.push(Op::Not);
            return Ok(());
        }
        self.primary(ctx)
    }

    fn primary(&mut self, ctx: Ctx) -> Result<(), PError> {
        let t = self.lx.next()?;
        match t.tok {
            Tok::Num(v) => self.code.push(Op::Num(v)),
            Tok::Minus | Tok::Plus => {
                // Only reachable outside `Ctx::Start` (unary() took
                // those): `-` glued to a number is a negative literal.
                if ctx == Ctx::AfterOp && t.tok == Tok::Minus && self.lx.digit_follows(t.end) {
                    let n = self.lx.next()?;
                    let Tok::Num(v) = n.tok else {
                        return Err(self.unexpected(&n));
                    };
                    self.code.push(Op::Num(-v));
                } else {
                    let msg = if ctx == Ctx::AfterNot {
                        "a sign cannot follow `!` (ngspice-47 rejects it): write `!(-x)`"
                    } else if t.tok == Tok::Plus {
                        "a `+` sign cannot follow an operator (ngspice-47 rejects it)"
                    } else {
                        "after an operator a `-` sign must be followed by a number \
                         (ngspice-47 rejects or misreads the rest): write `2*(-x)`"
                    };
                    return Err(PError::new(msg, t.start, t.end));
                }
            }
            Tok::LParen | Tok::LBrace | Tok::Quote => {
                // `{…}` and `'…'` nested in an expression group like
                // parentheses (probes c6 b_nested_brace, b_brace_quote,
                // b_brace_in_quote: ngspice turns quotes into braces).
                let (close, open_s, close_s) = match t.tok {
                    Tok::LParen => (Tok::RParen, "(", ")"),
                    Tok::LBrace => (Tok::RBrace, "{", "}"),
                    _ => (Tok::Quote, "'", "'"),
                };
                self.ternary(Ctx::Start)?;
                let c = self.lx.next()?;
                if c.tok != close {
                    return Err(if c.tok == Tok::End {
                        PError::new(
                            format!("`{open_s}` without its `{close_s}`"),
                            t.start,
                            t.end,
                        )
                    } else {
                        self.unexpected(&c)
                    });
                }
            }
            Tok::Ident => {
                let name = self.text(&t).to_ascii_lowercase();
                if self.lx.peek()?.tok == Tok::LParen {
                    let lp = self.lx.next()?;
                    // Unknown names and argument counts are checked when
                    // evaluating: a user function may define or shadow
                    // the name (`.func`).
                    let callee = match Func::lookup(&name) {
                        Some(f) => Callee::Builtin(f),
                        None if name == "v" || name == "i" => {
                            // Only an R/C/L value (rewritten into B
                            // sources, `behav.rs`) or a B source
                            // reads the circuit.
                            return Err(PError::new(
                                format!(
                                    "`{name}(…)` is not allowed here: only a behavioural R/C/L \
                                     value or a B source reads the circuit"
                                ),
                                t.start,
                                t.end,
                            ));
                        }
                        None => Callee::Named,
                    };
                    self.enter(lp.start)?;
                    let mut argc = 0usize;
                    if self.lx.peek()?.tok != Tok::RParen {
                        loop {
                            self.ternary(Ctx::Start)?;
                            argc += 1;
                            if argc > u16::MAX as usize {
                                return Err(PError::new("too many arguments", t.start, t.end));
                            }
                            let s = self.lx.next()?;
                            match s.tok {
                                Tok::Comma => continue,
                                Tok::RParen => break,
                                Tok::End => {
                                    return Err(PError::new(
                                        "`(` without its `)`",
                                        lp.start,
                                        lp.end,
                                    ));
                                }
                                _ => return Err(self.unexpected(&s)),
                            }
                        }
                    } else {
                        self.lx.next()?;
                    }
                    self.leave();
                    let ci = self.calls.len() as u32;
                    self.calls.push(CallSite {
                        name: name.into_boxed_str(),
                        start: t.start as u32,
                        end: t.end as u32,
                    });
                    self.code.push(Op::Call(callee, argc as u16, ci));
                } else {
                    if Func::lookup(&name).is_some() {
                        return Err(PError::new(
                            format!("`{name}` is a function and needs arguments"),
                            t.start,
                            t.end,
                        ));
                    }
                    let i = self.vars.len() as u32;
                    self.vars.push(VarRef {
                        name: name.into_boxed_str(),
                        start: t.start as u32,
                        end: t.end as u32,
                    });
                    self.code.push(Op::Var(i));
                }
            }
            _ => return Err(self.unexpected(&t)),
        }
        Ok(())
    }
}

/// The value a device, model or control card sees for a parameter
/// expression: ngspice-47 substitutes the result as text formatted
/// with `%.15e` and the card parser reads it back with `INPevaluate`
/// (probe: `{7}` reaches the device as 7.000000000000001, `{1/3}`
/// as 0.333…315; 25 values matched exactly, `docs/dune-netlist.md`).
/// Only finite values get here.
pub(crate) fn as_substituted(v: f64) -> f64 {
    crate::scan::evaluate(&format!("{v:.15e}")).unwrap_or(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ev_with(src: &str, vars: &[(&str, f64)]) -> Result<f64, PError> {
        let map: HashMap<String, f64> = vars.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let e = parse(src)?;
        let ctx = EvalCtx::new(RandomPolicy::Nominal);
        e.eval(
            &mut |v: &VarRef| {
                map.get(&*v.name)
                    .copied()
                    .ok_or_else(|| PError::new("unknown", v.start as usize, v.end as usize))
            },
            &ctx,
        )
    }

    fn ev(src: &str) -> f64 {
        ev_with(src, &[("a", 2.0), ("z", f64::INFINITY)]).unwrap_or_else(|e| panic!("{src}: {e:?}"))
    }

    fn err(src: &str) -> String {
        match ev_with(src, &[("a", 2.0)]) {
            Ok(v) => panic!("{src}: expected an error, got {v}"),
            Err(e) => e.msg,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-15 * b.abs().max(1.0)
    }

    // Each case cites the probe (`.claude/scratch/g2-probes/cN`).

    #[test]
    fn arithmetic_precedence_c1() {
        assert_eq!(ev("2+3*4"), 14.0);
        assert_eq!(ev("2*3^2"), 18.0);
        assert_eq!(ev("1-2-3"), -4.0);
        assert_eq!(ev("8/2/2"), 2.0);
        assert_eq!(ev("8/2*4"), 16.0);
        assert_eq!(ev(" 2 + 3 * 4 "), 14.0);
        assert_eq!(ev("1\t+\t2"), 3.0); // c8 o_tab
    }

    #[test]
    fn power_is_left_associative_on_magnitudes_c1_c3() {
        assert_eq!(ev("2^3^2"), 64.0);
        assert_eq!(ev("2**3**2"), 64.0);
        assert_eq!(ev("(-2)^2"), 4.0);
        assert_eq!(ev("(-2)^3"), 8.0);
        assert_eq!(ev("(-2)**3"), 8.0);
        assert!(close(ev("(-8)^(1/3)"), 2.0));
        assert!(close(ev("(-2)^2.5"), 5.656854249492381));
        assert_eq!(ev("0^0"), 1.0);
        assert_eq!(ev("2^-2"), 0.25);
        assert_eq!(ev("2**-1"), 0.5);
        assert_eq!(ev("2^2^-1"), 0.25); // regress/param_pow.cir
    }

    #[test]
    fn signs_follow_ngspice_c1_c2_c8() {
        assert_eq!(ev("-2^2"), -4.0);
        assert_eq!(ev("-2**2"), -4.0);
        assert_eq!(ev("-a^2"), -4.0);
        assert_eq!(ev("--3"), 3.0);
        assert_eq!(ev("+3"), 3.0);
        assert_eq!(ev("-2+3"), 1.0);
        assert_eq!(ev("-1<0"), 1.0);
        assert_eq!(ev("-1==-1"), 1.0);
        assert_eq!(ev("-2*2^2"), -8.0);
        assert_eq!(ev("3*-2^2"), 12.0);
        assert_eq!(ev("0+-2^2"), 4.0);
        assert_eq!(ev("0-2^2"), -4.0);
        assert_eq!(ev("1--2^2"), -3.0);
        assert_eq!(ev("3--2"), 5.0);
        assert_eq!(ev("2*-3"), -6.0);
        assert_eq!(ev("2*-.5"), -1.0);
        assert_eq!(ev("2*-1k"), -2000.0);
        assert_eq!(ev("2*-1e1"), -20.0);
        assert_eq!(ev("1>-2"), 1.0);
        assert_eq!(ev("(1<-2^2)"), 1.0);
        assert_eq!(ev("1?-2^2:3"), 4.0);
        assert_eq!(ev("0?3:-2^2"), 4.0);
        assert_eq!(ev("max(1,-2^2)"), 1.0);
        assert_eq!(ev("max(-2^2,-5)"), -4.0);
        assert_eq!(ev("2*(-2^2)"), -8.0);
        assert_eq!(ev("1+(-2^2)"), -3.0);
        assert_eq!(ev("-(-2)"), 2.0);
        assert_eq!(ev("-(2)^2"), -4.0);
        assert_eq!(ev("-(-2^2)"), 4.0);
        assert_eq!(ev("-abs(-3)^2"), -9.0);
        assert_eq!(ev("-!0"), -1.0);
        // Rejected by ngspice-47 (or misread): errors here.
        for bad in [
            "++3",
            "2*+3",
            "1++2",
            "2^+2",
            "2^-(1+1)",
            "2*-(3)",
            "2+-(3)",
            "2*-abs(3)",
            "3*-a",
            "2^-a",
            "1<-a",
            "--a",
            "- -a",
            "0||-a",
            "0?3:-a",
            "1?-a:3",
            "2*--3",
            "3*-a^2",
            "!-1",
        ] {
            assert!(err(bad).contains("sign"), "{bad}: {}", err(bad));
        }
    }

    #[test]
    fn not_binds_tightest_c1_c2_c8() {
        assert_eq!(ev("!0+1"), 2.0);
        assert_eq!(ev("!1+1"), 1.0);
        assert_eq!(ev("!5"), 0.0);
        assert_eq!(ev("!0.5"), 0.0);
        assert_eq!(ev("!2^0"), 1.0); // (!2)^0
        assert_eq!(ev("!0^2"), 1.0);
        assert_eq!(ev("1+!0"), 2.0);
        assert_eq!(ev("!!2"), 1.0);
        assert_eq!(ev("2*!0"), 2.0);
        assert_eq!(ev("!(-1)"), 0.0);
        assert_eq!(ev("!(1<0)"), 1.0);
    }

    #[test]
    fn comparisons_are_one_left_associative_level_c1_c2_c8() {
        assert_eq!(ev("1+1==2"), 1.0);
        assert_eq!(ev("1<2+1"), 1.0);
        assert_eq!(ev("2==2==1"), 1.0);
        assert_eq!(ev("3>2>1"), 0.0);
        assert_eq!(ev("1<2==1"), 1.0);
        assert_eq!(ev("2==1<3"), 1.0); // (2==1)<3, not 2==(1<3)
        assert_eq!(ev("1==2<1"), 1.0);
        assert_eq!(ev("1!=2"), 1.0);
        assert_eq!(ev("1<>2"), 1.0);
        assert_eq!(ev("1=1"), 1.0);
        assert_eq!(ev("1=2"), 0.0);
        assert_eq!(ev("(2>=2)+(1<=0)+(1>0)*10+(1<0)*100"), 11.0);
        assert_eq!(ev("(1==1) + (2!=2) + (1<=2) + (3>=4)"), 2.0); // P05 e10
    }

    #[test]
    fn logic_c1_c2() {
        assert_eq!(ev("1||0&&0"), 1.0);
        assert_eq!(ev("0&&0||1"), 1.0);
        assert_eq!(ev("0&&1||1&&1"), 1.0);
        assert_eq!(ev("1||1&&0"), 1.0); // && binds tighter
        assert_eq!(ev("2&&3"), 1.0);
        assert_eq!(ev("0||0.5"), 1.0);
        assert_eq!(ev("1<2&&2<3"), 1.0);
        assert_eq!(ev("1&&0+1"), 1.0);
        assert_eq!(ev("1&&2==2"), 1.0);
        assert_eq!(ev("!0&&0"), 0.0);
        assert_eq!(ev("-1&&0"), 0.0);
        assert!(err("1&0").contains("unexpected `&`"));
        assert!(err("1|0").contains("unexpected `|`"));
        assert!(err("~0").contains("unexpected `~`"));
    }

    #[test]
    fn ternary_c1_c2() {
        assert_eq!(ev("1?2:3"), 2.0);
        assert_eq!(ev("0?2:0?4:5"), 5.0);
        assert_eq!(ev("1?2:0?4:5"), 2.0);
        assert_eq!(ev("1+1?2:3"), 2.0);
        assert_eq!(ev("0+0?2:3"), 3.0);
        assert_eq!(ev("1?2+1:3"), 3.0);
        assert_eq!(ev("0||1?5:6"), 5.0);
        assert_eq!(ev("0.5?1:2"), 1.0);
        assert_eq!(ev("-1?1:2"), 1.0);
        assert_eq!(ev(" (1 > 0) ? 10 : 20 "), 10.0);
        assert_eq!(ev("1?(0?7:8):9"), 8.0);
        assert_eq!(ev("(0?1:0)?7:9"), 9.0);
        assert_eq!(ev("max(1?2:3, 0)"), 2.0);
        assert_eq!(ev("2*(1?3:4)"), 6.0);
        assert_eq!(ev("2*1?3:4"), 3.0);
        assert_eq!(ev("1?2:z"), 2.0); // untaken non-finite branch
        assert!(err("1?0?7:8:9").contains("parenthesized"));
        assert!(err("1?2").contains("without its `:`"));
        assert!(err("1:2").contains("unexpected `:`"));
    }

    #[test]
    fn modulo_c1_c2_c8() {
        assert_eq!(ev("7%3"), 1.0);
        assert_eq!(ev("7.5%2"), 1.5);
        assert_eq!(ev("-7%3"), -1.0);
        assert_eq!(ev("7%-3"), 1.0);
        assert!(ev("7%0").is_nan());
        assert_eq!(ev("2*5%3"), 1.0);
        assert_eq!(ev("5%3*2"), 4.0);
        assert!(close(ev("12/5%2"), 0.4));
        assert_eq!(ev("2^3%5"), 3.0);
    }

    #[test]
    fn non_finite_values_pass_through_c1() {
        assert_eq!(ev("1/0"), f64::INFINITY);
        assert!(ev("0/0").is_nan());
        assert_eq!(ev("1/z"), 0.0);
        assert!(ev("(z-z)*0+1").is_nan());
        assert_eq!(ev("pow(0,-1)"), f64::INFINITY);
        assert_eq!(ev("exp(1000)"), f64::INFINITY);
        assert_eq!(ev("log(0)"), f64::NEG_INFINITY);
        assert!(ev("log(-1)").is_nan());
        assert!(ev("sqrt(-4)").is_nan());
        assert_eq!(ev("1e400"), f64::INFINITY);
        assert_eq!(ev("1/1e400"), 0.0);
        assert_eq!(ev("1e-400*1e300"), 0.0);
        let nan = [("z", f64::NAN)];
        assert_eq!(ev_with("z==z", &nan), Ok(0.0));
        assert_eq!(ev_with("z?1:2", &nan), Ok(1.0));
    }

    #[test]
    fn functions_c3() {
        assert!(close(ev("sqrt(2)"), 2f64.sqrt()));
        assert_eq!(ev("abs(-2.5)"), 2.5);
        assert_eq!(ev("min(3,1)") + ev("max(3,4)"), 5.0);
        assert!(close(ev("pow(2,0.5)"), 2f64.sqrt()));
        assert_eq!(ev("pow(-2,3)"), -8.0);
        assert_eq!(ev("pow(-2,2)"), 4.0);
        assert!(ev("pow(-8,1/3)").is_nan());
        assert!(ev("pow(-2,2.5)").is_nan());
        assert_eq!(ev("pwr(-2,3)"), 8.0);
        assert_eq!(ev("pwr(-2,2)"), 4.0);
        assert!(close(ev("pwr(-8,1/3)"), 2.0));
        assert_eq!(ev("pwr(0,2)"), 0.0);
        assert_eq!(ev("pwr(0,-1)"), f64::INFINITY);
        assert!(close(ev("exp(1)"), std::f64::consts::E));
        assert!(close(ev("log(100) + ln(1)"), 100f64.ln()));
        assert!(close(ev("log(2)"), std::f64::consts::LN_2));
        assert!(close(ev("log10(1000)"), 3.0));
        assert_eq!(ev("floor(-2.5)"), -3.0);
        assert_eq!(ev("ceil(-2.5)"), -2.0);
        assert_eq!(ev("floor(2.7) + ceil(2.1)"), 5.0);
        assert_eq!(ev("int(-2.7)"), -2.0);
        assert_eq!(ev("abs(-3) + int(2.9) + nint(2.5)"), 7.0);
        assert_eq!(ev("nint(3.5)"), 4.0);
        assert_eq!(ev("nint(-2.5)"), -2.0);
        assert_eq!(ev("nint(-2.6)"), -3.0);
        assert_eq!(ev("nint(2.4)"), 2.0);
        assert_eq!(ev("sgn(0)"), 0.0);
        assert_eq!(ev("sgn(5)"), 1.0);
        assert_eq!(ev("sgn(-3)"), -1.0);
        for (f, x, want) in [
            ("sin", 1.0, 1f64.sin()),
            ("cos", 1.0, 1f64.cos()),
            ("tan", 1.0, 1f64.tan()),
            ("asin", 0.5, 0.5f64.asin()),
            ("acos", 0.5, 0.5f64.acos()),
            ("atan", 2.0, 2f64.atan()),
            ("sinh", 1.0, 1f64.sinh()),
            ("cosh", 1.0, 1f64.cosh()),
            ("tanh", 1.0, 1f64.tanh()),
            ("asinh", 1.0, 1f64.asinh()),
            ("acosh", 2.0, 2f64.acosh()),
            ("atanh", 0.5, 0.5f64.atanh()),
        ] {
            assert_eq!(ev(&format!("{f}({x})")), want, "{f}");
        }
        assert!(ev("acosh(0.5)").is_nan());
        assert!(ev("asin(2)").is_nan());
        assert_eq!(ev("sqr(3)"), 9.0);
        assert_eq!(ev("ternary_fcn(1,2,3)"), 2.0);
        assert_eq!(ev("ternary_fcn(0,2,3)"), 3.0);
        assert_eq!(ev("SQRT(4)+Max(1,2)"), 4.0);
        assert_eq!(ev("sqrt (4)"), 2.0);
        assert_eq!(ev("max (1, 2)"), 2.0);
        assert_eq!(ev("max(min(1,2),sqrt(abs(-16)))"), 4.0);
        assert_eq!(ev("floor(2.5k)"), 2500.0);
        for missing in [
            "round", "trunc", "sign", "atan2", "hypot", "u", "uramp", "fabs", "db", "if", "pwrs",
            "random", "rand", "mod", "ddt", "deriv", "nosuch",
        ] {
            assert!(
                err(&format!("{missing}(1)")).contains("unknown function"),
                "{missing}"
            );
        }
        assert!(err("v(1)").contains("behavioural R/C/L"));
        assert!(err("i(vx)").contains("behavioural R/C/L"));
        // Wrong argument counts: ngspice garbles them, Dune refuses.
        for bad in [
            "sqrt(1,2)",
            "min(3,1,2)",
            "min(3)",
            "pow(2)",
            "sqrt()",
            "agauss(1,0,1,1)",
            "aunif(1,0,1)",
            "limit(5,0,3)",
        ] {
            assert!(err(bad).contains("argument"), "{bad}");
        }
        assert!(err("sqrt").contains("needs arguments"));
    }

    #[test]
    fn random_functions_nominal_and_seeded_c3() {
        for (src, want) in [
            ("agauss(1,0.1,3)", 1.0),
            ("agauss(1,0)", 1.0),
            ("gauss(2,0.1,1)", 2.0),
            ("aunif(1,0.5)", 1.0),
            ("unif(3,0.5)", 3.0),
            ("limit(5,1)", 5.0),
            ("0*agauss(0,1,1)", 0.0),
        ] {
            let e = parse(src).unwrap();
            let ctx = EvalCtx::new(RandomPolicy::Nominal);
            let v = e.eval(&mut |_| Ok(0.0), &ctx).unwrap();
            assert_eq!(v, want, "{src}");
            assert!(ctx.random_used.get());
            assert!(e.first_random_call().is_some());
        }
        // Seeded: deterministic, and within the documented ranges.
        let draw = |seed, src: &str| {
            let ctx = EvalCtx::new(RandomPolicy::Seeded(seed));
            parse(src).unwrap().eval(&mut |_| Ok(0.0), &ctx).unwrap()
        };
        assert_eq!(draw(7, "agauss(1,0.1,3)"), draw(7, "agauss(1,0.1,3)"));
        assert_ne!(draw(7, "agauss(1,0.1,3)"), draw(8, "agauss(1,0.1,3)"));
        for seed in 0..200 {
            let u = draw(seed, "aunif(1,0.5)");
            assert!((0.5..=1.5).contains(&u));
            let l = draw(seed, "limit(5,1)");
            assert!(l == 4.0 || l == 6.0);
            assert!(draw(seed, "agauss(1,0.1,3)").is_finite());
        }
        assert!(parse("sqrt(2)").unwrap().first_random_call().is_none());
    }

    #[test]
    fn numbers_c4_c5() {
        assert_eq!(ev("2k"), 2000.0);
        assert_eq!(ev("2K"), 2000.0);
        assert_eq!(ev("2meg"), 2e6);
        assert_eq!(ev("2MEG"), 2e6);
        assert_eq!(ev("2Meg"), 2e6);
        assert_eq!(ev("2mil"), 2e-3); // milli here (S9)
        assert_eq!(ev("1milk"), 1e-3);
        assert_eq!(ev("2m"), 2e-3);
        assert_eq!(ev("2u"), 2e-6);
        assert_eq!(ev("2n"), 2e-9);
        assert_eq!(ev("2p"), 2e-12);
        assert_eq!(ev("2f"), 2e-15);
        assert_eq!(ev("2g"), 2e9);
        assert_eq!(ev("2t"), 2e12);
        assert_eq!(ev("2a"), 2e-18);
        assert_eq!(ev("2x"), 2.0);
        assert_eq!(ev("2kohm"), 2000.0);
        assert_eq!(ev("2ohm"), 2.0);
        assert_eq!(ev("2v"), 2.0);
        assert_eq!(ev("2hz"), 2.0);
        assert_eq!(ev("2e3"), 2000.0);
        assert_eq!(ev("2E3"), 2000.0);
        assert_eq!(ev("2e+3"), 2000.0);
        assert_eq!(ev("2e-3"), 2e-3);
        assert_eq!(ev("2e"), 2.0);
        assert_eq!(ev("2e3k"), 2e6);
        assert_eq!(ev(".5"), 0.5);
        assert_eq!(ev("5."), 5.0);
        assert_eq!(ev("5.k"), 5000.0);
        assert_eq!(ev("2us"), 2e-6);
        assert_eq!(ev("2ms"), 2e-3);
        assert_eq!(ev("2mm"), 2e-3);
        assert_eq!(ev("2megk"), 2e6);
        assert_eq!(ev("2kx"), 2000.0);
        assert_eq!(ev("007"), 7.0);
        assert_eq!(ev("2e3+1"), 2001.0);
        assert_eq!(ev("2e-3-1"), 2e-3 - 1.0);
        assert_eq!(ev("2u*a*1e6"), 2e-6 * 2.0 * 1e6);
        // Correctly rounded scaled decimals (c5 s_*).
        assert_eq!(ev("1.1k"), 1100.0);
        assert_eq!(ev("0.3m"), 0.3e-3);
        assert_eq!(ev("0.7u"), 0.7e-6);
        assert_eq!(ev("1.3meg"), 1.3e6);
        assert_eq!(ev("4.7p"), 4.7e-12);
        assert_eq!(ev("1.23456789012345678901234567890"), 1.2345678901234567);
        assert!(err("2d3").contains("malformed number"));
        assert!(err("0x10").contains("hexadecimal"));
        assert!(err(".").contains("malformed number"));
        assert!(err("2 e3").contains("unexpected `e3`"));
        assert!(err("1.2.3").contains("malformed"));
    }

    #[test]
    fn names_c4() {
        assert_eq!(ev_with("_a", &[("_a", 3.0)]), Ok(3.0));
        assert_eq!(ev_with("A1_2", &[("a1_2", 3.0)]), Ok(3.0));
        assert!(err("a.b").contains("unexpected `.`") || err("a.b").contains("malformed"));
        assert!(err("a$b").contains("unexpected `$`"));
        // No built-in constants (pi, e, true, … are undefined names).
        let e = parse("pi+e+true+temper").unwrap();
        let names: Vec<&str> = e.vars().iter().map(|v| &*v.name).collect();
        assert_eq!(names, ["pi", "e", "true", "temper"]);
    }

    #[test]
    fn malformed_c8() {
        for bad in [
            "(1+2", "1+2)", "a b", "1 2", "1+", "*1", "1,2", "2***2", "2^*2", "",
        ] {
            assert!(ev_with(bad, &[("a", 1.0), ("b", 1.0)]).is_err(), "{bad}");
        }
        assert_eq!(ev("((((((((((((((((((((1))))))))))))))))))))"), 1.0);
    }

    #[test]
    fn hostile_input_is_bounded() {
        let deep = "(".repeat(5000) + "1" + &")".repeat(5000);
        assert!(err(&deep).contains("nested deeper"));
        let signs = "-".repeat(100_000) + "1";
        assert!(ev_with(&signs[..60_000], &[]).is_err());
        let nots = "!".repeat(60_000) + "1";
        assert!(err(&nots).contains("nested deeper"));
        let tern = "1?2:".repeat(10_000) + "3";
        assert!(err(&tern).contains("nested deeper"));
        let calls = "abs(".repeat(5000) + "1" + &")".repeat(5000);
        assert!(err(&calls).contains("nested deeper"));
        let long = "1+".repeat(40_000) + "1";
        assert!(err(&long).contains("longer than"));
        // A long flat sum within the limit evaluates without recursion.
        let sum = "1+".repeat(30_000) + "1";
        assert_eq!(ev(&sum), 30_001.0);
        let pows = "1^".repeat(30_000) + "1";
        assert_eq!(ev(&pows), 1.0);
    }

    #[test]
    #[allow(clippy::excessive_precision)] // literals copy ngspice's printed digits
    fn substitution_rounds_like_ngspice_c1_c2_c7() {
        // Observed device values (`print @dev[param]`, `v(1)`).
        for (v, seen) in [
            (7.0, 7.000_000_000_000_001),
            (3.0, 3.000_000_000_000_000_4),
            (-3.0, -3.000_000_000_000_000_4),
            (2.0, 2.0),
            (14.0, 14.0),
            (1.0 / 3.0, 0.333_333_333_333_333_31), // printed 3.33333333333333315e-01
            (2.0 / 3.0, 0.666_666_666_666_666_63), // printed 6.66666666666666630e-01
            (0.1 + 0.2, 0.3),
            (1.0 + 1e-15, 1.000_000_000_000_001_1),
            // c9: `print @v1[dc]` with numdgt=17.
            (1e-9, 9.999_999_999_999_998_55e-10),
            (5e-9, 4.999_999_999_999_999_28e-9),
            (1e4, 1e4),
            (21.0, 21.0),
            (-1.7e-13, -1.700_000_000_000_000_1e-13),
            (6.02e23, 6.019_999_999_999_999_96e23),
            (1.6e-300, 1.599_999_997_570_694_23e-300),
            (1e-310, 0.0),
            // Printed 3.14159265358979312 and 2.71828182845904509: unchanged.
            (std::f64::consts::PI, std::f64::consts::PI),
            (std::f64::consts::E, std::f64::consts::E),
        ] {
            assert_eq!(as_substituted(v), seen, "{v}");
        }
    }
}
