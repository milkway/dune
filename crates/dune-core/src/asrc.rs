//! The arbitrary source `B` (`dev/asrc/`) and the parse-tree runtime
//! behind it: symbolic differentiation with the constructor folding
//! of `inpptree.c` (`PTdifferentiate`, `mkcon`/`mkb`/`mkf`), the
//! evaluator of `ifeval.c` and the functions of `ptfuncs.c`.
//!
//! The tree comes parsed from `dune-netlist` (`parse_expr`); here it
//! is laid out in an arena (subtrees are shared between the function
//! and its derivatives, as the C shares pointers) and evaluated once
//! per Newton iteration together with one derivative tree per
//! controlling variable. The load is `ASRCload`: a VCVS/CCVS (`V=`) or
//! VCCS/CCCS (`I=`) generalized to the linearization
//! `f(x) ≈ f(x₀) + Σ ∂f/∂xᵢ·(xᵢ − xᵢ₀)` — no junction limiting (the
//! 3f5 has none for this device).
//!
//! Where ngspice-47 fixed the 3f5, Dune follows ngspice (each one is
//! in `corpus/KNOWN_DIFFS.md`): a `V=` source always stamps its branch
//! equation (the 3f5 stamps it only from the first controlling
//! variable on, so `V=5` leaves the branch row empty — singular), the
//! convergence test reads the new solution (the 3f5's reads the old
//! one and can never fail), the derivative of `f^g` with a
//! non-constant exponent uses `ln f` (the 3f5 writes `ln f'`), and
//! the value is scaled by `srcFact` under the transient operating
//! point (D-0012), and `ln`/`log` of zero return ngspice's finite
//! −1e99 instead of −∞. The function semantics stay the 3f5's (`log` is
//! base 10; `ln`, `log`, `sqrt` take `|x|`; `x^y` with `x < 0` and a
//! non-integer `y` is `|x|^y`).

use dune_netlist::ir::{Expr, ExprFunc, ExprOp, SourceKind};

use crate::matrix::{AcCtx, AnKind, ConvCtx, InitMode, LoadCtx, MatrixBuilder, PzCtx, Slot, Var};

/// A node of the evaluation arena.
#[derive(Clone, Copy, Debug)]
enum Node {
    Const(f64),
    /// Index into the controlling-variable vector.
    Var(usize),
    Bin(ExprOp, u32, u32),
    Func(ExprFunc, u32),
}

/// What a variable of the tree reads: a node voltage or a branch
/// current (`IF_NODE` / `IF_INSTANCE`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum VarRef {
    Node(String),
    Branch(String),
}

/// An expression with its derivative trees (`INPparseTree`).
#[derive(Clone, Debug)]
pub(crate) struct Tree {
    nodes: Vec<Node>,
    root: u32,
    derivs: Vec<u32>,
    pub vars: Vec<VarRef>,
    /// The `u()` and `sgn()` nodes of the expression: where its
    /// value can jump (Dune's event location, D-0051).
    jumps: Vec<u32>,
}

/// `HUGE` (`HUGE_VAL`): the out-of-range return of `ptfuncs.c`.
const HUGE: f64 = f64::INFINITY;

impl Tree {
    /// `INPgetTree` after the parse: collect the variables in order of
    /// first appearance (one per distinct node or device), then
    /// differentiate once per variable.
    pub fn new(expr: &Expr) -> Tree {
        let mut t = Tree {
            nodes: Vec::new(),
            root: 0,
            derivs: Vec::new(),
            vars: Vec::new(),
            jumps: Vec::new(),
        };
        t.root = t.build(expr);
        // Every node so far belongs to the expression itself (the
        // derivative trees come next).
        t.jumps = (0..t.nodes.len() as u32)
            .filter(|&i| {
                matches!(
                    t.nodes[i as usize],
                    Node::Func(ExprFunc::U | ExprFunc::Sgn, _)
                )
            })
            .collect();
        for v in 0..t.vars.len() {
            let d = t.differentiate(t.root, v);
            t.derivs.push(d);
        }
        t
    }

    fn push(&mut self, n: Node) -> u32 {
        self.nodes.push(n);
        (self.nodes.len() - 1) as u32
    }

    fn var_index(&mut self, r: VarRef) -> usize {
        match self.vars.iter().position(|v| *v == r) {
            Some(i) => i,
            None => {
                self.vars.push(r);
                self.vars.len() - 1
            }
        }
    }

    /// `mkbnode`/`mkfnode`/`mknnode`: the parse tree as built, with
    /// no folding.
    fn build(&mut self, e: &Expr) -> u32 {
        match e {
            Expr::Num(v) => self.push(Node::Const(*v)),
            Expr::V(n) => {
                let i = self.var_index(VarRef::Node(n.clone()));
                self.push(Node::Var(i))
            }
            Expr::I(d) => {
                let i = self.var_index(VarRef::Branch(d.clone()));
                self.push(Node::Var(i))
            }
            Expr::Bin(op, l, r) => {
                let l = self.build(l);
                let r = self.build(r);
                self.push(Node::Bin(*op, l, r))
            }
            Expr::Call(f, a) => {
                let a = self.build(a);
                self.push(Node::Func(*f, a))
            }
        }
    }

    fn konst(&self, i: u32) -> Option<f64> {
        match self.nodes[i as usize] {
            Node::Const(c) => Some(c),
            _ => None,
        }
    }

    /// `mkcon`.
    fn mkcon(&mut self, v: f64) -> u32 {
        self.push(Node::Const(v))
    }

    /// `mkb`: a binary node with the C's folding — both constants
    /// fold with the raw operator (no fudge, plain `pow`), then the
    /// 0/1 identities.
    fn mkb(&mut self, op: ExprOp, l: u32, r: u32) -> u32 {
        let (lc, rc) = (self.konst(l), self.konst(r));
        if let (Some(a), Some(b)) = (lc, rc) {
            let v = match op {
                ExprOp::Times => a * b,
                ExprOp::Divide => a / b,
                ExprOp::Plus => a + b,
                ExprOp::Minus => a - b,
                ExprOp::Power => a.powf(b),
            };
            return self.mkcon(v);
        }
        match op {
            ExprOp::Times => {
                // `0*x → 0`, `x*0 → 0`, `1*x → x`, `x*1 → x`.
                if lc == Some(0.0) || rc == Some(1.0) {
                    return l;
                } else if rc == Some(0.0) || lc == Some(1.0) {
                    return r;
                }
            }
            ExprOp::Divide => {
                if lc == Some(0.0) || rc == Some(1.0) {
                    return l;
                }
            }
            ExprOp::Plus => {
                if lc == Some(0.0) {
                    return r;
                } else if rc == Some(0.0) {
                    return l;
                }
            }
            ExprOp::Minus => {
                if rc == Some(0.0) {
                    return l;
                } else if lc == Some(0.0) {
                    return self.mkf(ExprFunc::Neg, r);
                }
            }
            ExprOp::Power => {
                if rc == Some(0.0) {
                    return self.mkcon(1.0);
                } else if rc == Some(1.0) {
                    return l;
                }
            }
        }
        self.push(Node::Bin(op, l, r))
    }

    /// `mkf`: a function node, folded through the `ptfuncs.c`
    /// function itself when the argument is constant.
    fn mkf(&mut self, f: ExprFunc, a: u32) -> u32 {
        if let Some(c) = self.konst(a) {
            return self.mkcon(func(f, c));
        }
        self.push(Node::Func(f, a))
    }

    /// `PTdifferentiate`: ∂p/∂(variable `v`). Recursion depth is the
    /// parse tree's, which the parser bounds.
    fn differentiate(&mut self, p: u32, v: usize) -> u32 {
        match self.nodes[p as usize] {
            Node::Const(_) => self.mkcon(0.0),
            Node::Var(i) => self.mkcon(if i == v { 1.0 } else { 0.0 }),
            Node::Bin(op @ (ExprOp::Plus | ExprOp::Minus), l, r) => {
                let a1 = self.differentiate(l, v);
                let a2 = self.differentiate(r, v);
                self.mkb(op, a1, a2)
            }
            Node::Bin(ExprOp::Times, l, r) => {
                // d(a*b) = d(a)*b + a*d(b)
                let a1 = self.differentiate(l, v);
                let a2 = self.differentiate(r, v);
                let t1 = self.mkb(ExprOp::Times, a1, r);
                let t2 = self.mkb(ExprOp::Times, l, a2);
                self.mkb(ExprOp::Plus, t1, t2)
            }
            Node::Bin(ExprOp::Divide, l, r) => {
                // d(a/b) = (d(a)*b - a*d(b)) / b^2
                let a1 = self.differentiate(l, v);
                let a2 = self.differentiate(r, v);
                let t1 = self.mkb(ExprOp::Times, a1, r);
                let t2 = self.mkb(ExprOp::Times, l, a2);
                let num = self.mkb(ExprOp::Minus, t1, t2);
                let two = self.mkcon(2.0);
                let den = self.mkb(ExprOp::Power, r, two);
                self.mkb(ExprOp::Divide, num, den)
            }
            Node::Bin(ExprOp::Power, l, r) => {
                if let Some(c) = self.konst(r) {
                    // c * l^(c-1) * d(l)
                    let a1 = self.differentiate(l, v);
                    let cc = self.mkcon(c);
                    let cm1 = self.mkcon(c - 1.0);
                    let pw = self.mkb(ExprOp::Power, l, cm1);
                    let t = self.mkb(ExprOp::Times, cc, pw);
                    self.mkb(ExprOp::Times, t, a1)
                } else {
                    // f^g = exp(g*ln f): exp(g*ln f) * (g*d(f)/f + d(g)*ln f).
                    // The 3f5 writes `ln(d(f))` in the last factor;
                    // ngspice has `log(abs(f))` (KNOWN_DIFFS).
                    let a1 = self.differentiate(l, v);
                    let a2 = self.differentiate(r, v);
                    let lnf = self.mkf(ExprFunc::Ln, l);
                    let glnf = self.mkb(ExprOp::Times, r, lnf);
                    let ex = self.mkf(ExprFunc::Exp, glnf);
                    let q = self.mkb(ExprOp::Divide, a1, l);
                    let t1 = self.mkb(ExprOp::Times, r, q);
                    let lnf2 = self.mkf(ExprFunc::Ln, l);
                    let t2 = self.mkb(ExprOp::Times, a2, lnf2);
                    let s = self.mkb(ExprOp::Plus, t1, t2);
                    self.mkb(ExprOp::Times, ex, s)
                }
            }
            Node::Func(f, a) => {
                let u = a;
                let arg1 = match f {
                    ExprFunc::Abs => self.mkf(ExprFunc::Sgn, u),
                    ExprFunc::Sgn | ExprFunc::U => self.mkcon(0.0),
                    ExprFunc::Acos => {
                        // -1 / sqrt(1 - u^2)
                        let m1 = self.mkcon(-1.0);
                        let s = self.one_minus_u2(u);
                        let sq = self.mkf(ExprFunc::Sqrt, s);
                        self.mkb(ExprOp::Divide, m1, sq)
                    }
                    ExprFunc::Acosh => {
                        // 1 / sqrt(u^2 - 1)
                        let one = self.mkcon(1.0);
                        let u2 = self.u_squared(u);
                        let one2 = self.mkcon(1.0);
                        let d = self.mkb(ExprOp::Minus, u2, one2);
                        let sq = self.mkf(ExprFunc::Sqrt, d);
                        self.mkb(ExprOp::Divide, one, sq)
                    }
                    ExprFunc::Asin => {
                        // 1 / sqrt(1 - u^2)
                        let one = self.mkcon(1.0);
                        let s = self.one_minus_u2(u);
                        let sq = self.mkf(ExprFunc::Sqrt, s);
                        self.mkb(ExprOp::Divide, one, sq)
                    }
                    ExprFunc::Asinh => {
                        // 1 / sqrt(u^2 + 1)
                        let one = self.mkcon(1.0);
                        let u2 = self.u_squared(u);
                        let one2 = self.mkcon(1.0);
                        let d = self.mkb(ExprOp::Plus, u2, one2);
                        let sq = self.mkf(ExprFunc::Sqrt, d);
                        self.mkb(ExprOp::Divide, one, sq)
                    }
                    ExprFunc::Atan => {
                        // 1 / (u^2 + 1)
                        let one = self.mkcon(1.0);
                        let u2 = self.u_squared(u);
                        let one2 = self.mkcon(1.0);
                        let d = self.mkb(ExprOp::Plus, u2, one2);
                        self.mkb(ExprOp::Divide, one, d)
                    }
                    ExprFunc::Atanh => {
                        // 1 / (1 - u^2)
                        let one = self.mkcon(1.0);
                        let d = self.one_minus_u2(u);
                        self.mkb(ExprOp::Divide, one, d)
                    }
                    ExprFunc::Cos => {
                        let s = self.mkf(ExprFunc::Sin, u);
                        self.mkf(ExprFunc::Neg, s)
                    }
                    ExprFunc::Cosh => self.mkf(ExprFunc::Sinh, u),
                    ExprFunc::Exp => self.mkf(ExprFunc::Exp, u),
                    ExprFunc::Ln => {
                        let one = self.mkcon(1.0);
                        self.mkb(ExprOp::Divide, one, u)
                    }
                    ExprFunc::Log => {
                        let c = self.mkcon(std::f64::consts::LOG10_E);
                        self.mkb(ExprOp::Divide, c, u)
                    }
                    ExprFunc::Sin => self.mkf(ExprFunc::Cos, u),
                    ExprFunc::Sinh => self.mkf(ExprFunc::Cosh, u),
                    ExprFunc::Sqrt => {
                        // 1 / (2 * sqrt(u))
                        let one = self.mkcon(1.0);
                        let two = self.mkcon(2.0);
                        let sq = self.mkf(ExprFunc::Sqrt, u);
                        let d = self.mkb(ExprOp::Times, two, sq);
                        self.mkb(ExprOp::Divide, one, d)
                    }
                    ExprFunc::Tan => {
                        // 1 / cos(u)^2
                        let one = self.mkcon(1.0);
                        let c = self.mkf(ExprFunc::Cos, u);
                        let two = self.mkcon(2.0);
                        let c2 = self.mkb(ExprOp::Power, c, two);
                        self.mkb(ExprOp::Divide, one, c2)
                    }
                    ExprFunc::Tanh => {
                        // 1 / cosh(u)^2
                        let one = self.mkcon(1.0);
                        let c = self.mkf(ExprFunc::Cosh, u);
                        let two = self.mkcon(2.0);
                        let c2 = self.mkb(ExprOp::Power, c, two);
                        self.mkb(ExprOp::Divide, one, c2)
                    }
                    ExprFunc::Uramp => self.mkf(ExprFunc::U, u),
                    ExprFunc::Neg => self.mkcon(-1.0),
                };
                let arg2 = self.differentiate(u, v);
                self.mkb(ExprOp::Times, arg1, arg2)
            }
        }
    }

    /// `mkb(PT_POWER, u, mkcon(2))`.
    fn u_squared(&mut self, u: u32) -> u32 {
        let two = self.mkcon(2.0);
        self.mkb(ExprOp::Power, u, two)
    }

    /// `mkb(PT_MINUS, mkcon(1), u^2)`.
    fn one_minus_u2(&mut self, u: u32) -> u32 {
        let one = self.mkcon(1.0);
        let u2 = self.u_squared(u);
        self.mkb(ExprOp::Minus, one, u2)
    }

    /// `IFeval`: the function and every derivative at `vals`. `Err`
    /// is the C's `E_PARMVAL` (a function returned `HUGE`).
    pub fn eval(&self, gmin: f64, vals: &[f64], derivs: &mut [f64]) -> Result<f64, ()> {
        let f = self.pteval(self.root, gmin, vals)?;
        for (d, &root) in derivs.iter_mut().zip(&self.derivs) {
            *d = self.pteval(root, gmin, vals)?;
        }
        Ok(f)
    }

    /// How many `u()`/`sgn()` nodes the expression has.
    pub fn num_jumps(&self) -> usize {
        self.jumps.len()
    }

    /// The argument of jump `k` at `vals`.
    pub fn jump_arg(&self, k: usize, gmin: f64, vals: &[f64]) -> Result<f64, ()> {
        let Node::Func(_, a) = self.nodes[self.jumps[k] as usize] else {
            unreachable!("jumps are function nodes");
        };
        self.pteval(a, gmin, vals)
    }

    /// The expression at `vals` with the argument of jump `k` taken
    /// as `side` (±1): its value just before and just after that
    /// argument crosses zero.
    pub fn eval_at_jump(&self, k: usize, side: f64, gmin: f64, vals: &[f64]) -> Result<f64, ()> {
        self.pteval_forced(self.root, gmin, vals, Some((self.jumps[k], side)))
    }

    /// `PTeval` (`PTfudge_factor = gmin` for the divide).
    fn pteval(&self, p: u32, gmin: f64, vals: &[f64]) -> Result<f64, ()> {
        self.pteval_forced(p, gmin, vals, None)
    }

    /// [`Tree::pteval`], one function node optionally reading a given
    /// argument instead of its own.
    fn pteval_forced(
        &self,
        p: u32,
        gmin: f64,
        vals: &[f64],
        forced: Option<(u32, f64)>,
    ) -> Result<f64, ()> {
        let r = match self.nodes[p as usize] {
            Node::Const(c) => return Ok(c),
            Node::Var(i) => return Ok(vals.get(i).copied().unwrap_or(0.0)),
            Node::Func(f, a) => match forced {
                Some((node, side)) if node == p => func(f, side),
                _ => func(f, self.pteval_forced(a, gmin, vals, forced)?),
            },
            Node::Bin(op, l, r) => {
                let a = self.pteval_forced(l, gmin, vals, forced)?;
                let b = self.pteval_forced(r, gmin, vals, forced)?;
                match op {
                    ExprOp::Plus => a + b,
                    ExprOp::Minus => a - b,
                    ExprOp::Times => a * b,
                    ExprOp::Divide => pt_divide(a, b, gmin),
                    ExprOp::Power => pt_power(a, b),
                }
            }
        };
        if r == HUGE { Err(()) } else { Ok(r) }
    }
}

/// `(int) x` for the C idioms below (saturating where the C is
/// undefined).
fn c_int(x: f64) -> f64 {
    if x.is_nan() {
        0.0
    } else {
        x.trunc().clamp(i32::MIN.into(), i32::MAX.into())
    }
}

/// `MODULUS(NUM, LIMIT)` of `ptfuncs.c`.
fn modulus(num: f64, limit: f64) -> f64 {
    num - c_int(num / limit) * limit
}

/// `PTdivide`: the divisor is pushed away from zero by gmin.
fn pt_divide(a: f64, mut b: f64, fudge: f64) -> f64 {
    if b >= 0.0 {
        b += fudge;
    } else {
        b -= fudge;
    }
    if b == 0.0 {
        return HUGE;
    }
    a / b
}

/// `PTpower`: a negative base takes an integer exponent when the
/// exponent is (relatively) within 1e-6 of one, else its magnitude.
fn pt_power(mut a: f64, mut b: f64) -> f64 {
    if a < 0.0 {
        if (b - c_int(b)).abs() / (b + 0.001) < 0.000001 {
            b = c_int(b);
        } else {
            a = -a;
        }
    }
    a.powf(b)
}

/// The one-argument functions of `ptfuncs.c` (3f5 semantics; the
/// inverse hyperbolics are libm's, as with `HAS_ATRIGH`), with
/// ngspice-47's finite −1e99 for `ln`/`log` of 0, where the 3f5
/// would put −∞ into the matrix.
fn func(f: ExprFunc, x: f64) -> f64 {
    use std::f64::consts::PI;
    match f {
        ExprFunc::Abs => {
            if x >= 0.0 {
                x
            } else {
                -x
            }
        }
        ExprFunc::Sgn => {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        ExprFunc::Acos => x.acos(),
        ExprFunc::Acosh => x.acosh(),
        ExprFunc::Asin => x.asin(),
        ExprFunc::Asinh => x.asinh(),
        ExprFunc::Atan => x.atan(),
        ExprFunc::Atanh => x.atanh(),
        ExprFunc::Cos => modulus(x, 2.0 * PI).cos(),
        ExprFunc::Cosh => x.cosh(),
        // Overflow is `HUGE`, an evaluation error, as in the 3f5
        // (ngspice-47 caps at 1e99 past 227.96; no probe needs it).
        ExprFunc::Exp => x.exp(),
        // `|x|` as in the 3f5; at exactly 0 ngspice-47's -1e99 (the
        // 3f5's -inf poisons the first iterate of every `ln(v(n))`,
        // which starts from v = 0).
        ExprFunc::Ln => match if x < 0.0 { -x } else { x } {
            0.0 => -1e99,
            a => a.ln(),
        },
        ExprFunc::Log => match if x < 0.0 { -x } else { x } {
            0.0 => -1e99,
            a => a.log10(),
        },
        ExprFunc::Sin => modulus(x, 2.0 * PI).sin(),
        ExprFunc::Sinh => x.sinh(),
        ExprFunc::Sqrt => (if x < 0.0 { -x } else { x }).sqrt(),
        ExprFunc::Tan => modulus(x, PI).tan(),
        ExprFunc::Tanh => x.tanh(),
        ExprFunc::U => {
            if x < 0.0 {
                0.0
            } else if x > 0.0 {
                1.0
            } else {
                0.5
            }
        }
        ExprFunc::Uramp => {
            if x < 0.0 {
                0.0
            } else {
                x
            }
        }
        ExprFunc::Neg => -x,
    }
}

/// `ASRCload` / `ASRCacLoad` / `ASRCpzLoad` / `ASRCconvTest`.
pub(crate) struct AsrcI {
    kind: SourceKind,
    pos: Var,
    neg: Var,
    /// Branch equation of a `V=` source (`ASRCbranch`); 0 for `I=`.
    br: Var,
    tree: Tree,
    /// Solution index of each tree variable (`ASRCvars`).
    cols: Vec<Var>,
    /// `ASRCposPtr`: for `V=` the four branch entries, then
    /// `(br, col)` per variable; for `I=` `(pos, col)`, `(neg, col)`
    /// per variable.
    slots: Vec<Slot>,
    vals: Vec<f64>,
    derivs: Vec<f64>,
    /// `ASRCprev_value`, for the convergence test.
    prev_value: f64,
    /// `ASRCacValues`: the derivatives stored by the
    /// `MODEINITSMSIG` load.
    ac_derivs: Vec<f64>,
    /// State base (Dune's; the C's ASRC has no states): the
    /// controlling values, then the argument of each `u()`/`sgn()`,
    /// for the event location.
    st: usize,
    /// `GMIN` of the last load (the divide's fudge), for `event`.
    gmin: f64,
}

/// The state slots a B source with this expression takes.
pub(crate) fn num_states(expr: &Expr) -> usize {
    let tree = Tree::new(expr);
    match tree.num_jumps() {
        0 => 0,
        n => tree.vars.len() + n,
    }
}

/// What counts as a jump of a B source's value across a `u()`/`sgn()`
/// zero ([`AsrcI::event`]): more than the SPICE default tolerances
/// (`RELTOL`, and `ABSTOL` — the smaller of the two floors, since
/// the value may be a current).
const JUMP_RELTOL: f64 = 1e-3;
const JUMP_ABSTOL: f64 = 1e-12;

impl AsrcI {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kind: SourceKind,
        pos: Var,
        neg: Var,
        br: Var,
        tree: Tree,
        cols: Vec<Var>,
        st: usize,
    ) -> Self {
        let n = cols.len();
        AsrcI {
            kind,
            pos,
            neg,
            br,
            tree,
            cols,
            slots: Vec::new(),
            vals: vec![0.0; n],
            derivs: vec![0.0; n],
            prev_value: 0.0,
            ac_derivs: vec![0.0; n],
            st,
            gmin: 0.0,
        }
    }

    /// The small-signal stamps shared by the AC and pole-zero loads.
    fn stamp_linear(&self, mut add: impl FnMut(Slot, f64)) {
        let mut j = 0;
        if self.kind == SourceKind::Voltage {
            add(self.slots[0], 1.0);
            add(self.slots[1], -1.0);
            add(self.slots[2], -1.0);
            add(self.slots[3], 1.0);
            j = 4;
            for d in &self.ac_derivs {
                add(self.slots[j], -d);
                j += 1;
            }
        } else {
            for d in &self.ac_derivs {
                add(self.slots[j], *d);
                add(self.slots[j + 1], -d);
                j += 2;
            }
        }
    }
}

impl crate::devices::Device for AsrcI {
    fn par_load(&self) -> bool {
        true
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        self.slots.clear();
        if self.kind == SourceKind::Voltage {
            self.slots.push(mb.slot(self.pos, self.br));
            self.slots.push(mb.slot(self.neg, self.br));
            self.slots.push(mb.slot(self.br, self.neg));
            self.slots.push(mb.slot(self.br, self.pos));
            for &c in &self.cols {
                self.slots.push(mb.slot(self.br, c));
            }
        } else {
            for &c in &self.cols {
                self.slots.push(mb.slot(self.pos, c));
                self.slots.push(mb.slot(self.neg, c));
            }
        }
    }

    fn load(&mut self, ctx: &mut LoadCtx) {
        for (v, &c) in self.vals.iter_mut().zip(&self.cols) {
            *v = ctx.sol[c];
        }
        let Ok(mut rhs) = self.tree.eval(ctx.gmin, &self.vals, &mut self.derivs) else {
            ctx.fault = Some("B source expression out of range");
            return;
        };
        if !rhs.is_finite() || self.derivs.iter().any(|d| !d.is_finite()) {
            ctx.fault = Some("B source expression is not finite");
            return;
        }
        self.prev_value = rhs;
        self.gmin = ctx.gmin;
        if self.tree.num_jumps() > 0 {
            let n = self.vals.len();
            for (i, v) in self.vals.iter().enumerate() {
                ctx.states.set(0, self.st + i, *v);
            }
            for k in 0..self.tree.num_jumps() {
                let arg = self.tree.jump_arg(k, ctx.gmin, &self.vals).unwrap_or(0.0);
                ctx.states.set(0, self.st + n + k, arg);
            }
        }
        if ctx.mode == InitMode::Smsig {
            self.ac_derivs.clone_from(&self.derivs);
        }
        // ngspice-47 `ASRCload`: `factor *= CKTsrcFact` under
        // MODETRANOP (tc/m factors are ngspice extensions, 1 here).
        let factor = if ctx.kind == AnKind::TranOp {
            ctx.srcfact
        } else {
            1.0
        };
        if self.kind == SourceKind::Voltage {
            ctx.add(self.slots[0], 1.0);
            ctx.add(self.slots[1], -1.0);
            ctx.add(self.slots[2], -1.0);
            ctx.add(self.slots[3], 1.0);
            for i in 0..self.cols.len() {
                rhs -= self.vals[i] * self.derivs[i];
                ctx.add(self.slots[4 + i], -(self.derivs[i] * factor));
            }
            ctx.add_rhs(self.br, factor * rhs);
        } else {
            for i in 0..self.cols.len() {
                rhs -= self.vals[i] * self.derivs[i];
                ctx.add(self.slots[2 * i], self.derivs[i] * factor);
                ctx.add(self.slots[2 * i + 1], -(self.derivs[i] * factor));
            }
            ctx.add_rhs(self.pos, -(factor * rhs));
            ctx.add_rhs(self.neg, factor * rhs);
        }
    }

    /// Dune's event location (D-0051; the 3f5 and ngspice let a
    /// `u()`/`sgn()` flip at whatever timepoint first sees its
    /// argument past zero): the earliest fraction of the interval at
    /// which the argument of a `u()` or `sgn()` crossed zero — taken
    /// as linear in time — *and* the expression jumped there. The
    /// jump is the expression at the interpolated controlling values
    /// with that argument read as −1 and as +1; a `sgn()` multiplied
    /// by something that vanishes with its argument does not jump and
    /// is not an event.
    ///
    /// A `u()` has no memory, so an accepted point is always
    /// consistent with the sign of its argument there; what is not
    /// seen is an argument that crosses zero twice inside one step.
    fn event(
        &self,
        states: &crate::state::States,
        from: usize,
        to: usize,
        back: f64,
    ) -> Option<f64> {
        let n = self.vals.len();
        let mut first: Option<f64> = None;
        for k in 0..self.tree.num_jumps() {
            let a1 = states.get(from, self.st + n + k);
            let a0 = states.get(to, self.st + n + k);
            let side = |a: f64| a.partial_cmp(&0.0);
            if side(a1) == side(a0) {
                continue;
            }
            let mut theta = a1 / (a1 - a0);
            if !(0.0..=1.0).contains(&theta) {
                continue;
            }
            // A source that feeds its own argument (a comparator with
            // positive feedback) ends the step with the argument
            // pushed further by its own jump, and the straight line
            // puts the crossing far too early. The argument's rate
            // before the step says where it was heading; when the two
            // disagree by more than a factor of two that one is
            // taken, no later than mid-interval (so retries at least
            // halve the step).
            let rate = (a1 - states.get(from + 1, self.st + n + k)) / back;
            let ahead = -a1 / rate;
            if ahead.is_finite() && ahead > 2.0 * theta {
                theta = ahead.min(0.5);
            }
            if first.is_some_and(|f| f <= theta) {
                continue;
            }
            let vals: Vec<f64> = (0..n)
                .map(|i| {
                    let (v1, v0) = (states.get(from, self.st + i), states.get(to, self.st + i));
                    v1 + theta * (v0 - v1)
                })
                .collect();
            let at = |s: f64| self.tree.eval_at_jump(k, s, self.gmin, &vals);
            let (Ok(before), Ok(after)) = (at(-1.0), at(1.0)) else {
                continue;
            };
            let scale = before.abs().max(after.abs());
            if (after - before).abs() > JUMP_RELTOL * scale + JUMP_ABSTOL {
                first = Some(theta);
            }
        }
        first
    }

    /// `ASRCconvTest` in its ngspice-47 form: the value at the new
    /// solution against the one the last load linearized.
    fn conv_test(&self, ctx: &ConvCtx) -> bool {
        let vals: Vec<f64> = self.cols.iter().map(|&c| ctx.rhs[c]).collect();
        let mut derivs = vec![0.0; vals.len()];
        let Ok(rhs) = self.tree.eval(ctx.gmin, &vals, &mut derivs) else {
            return false;
        };
        let prev = self.prev_value;
        let floor = if self.kind == SourceKind::Voltage {
            ctx.vntol
        } else {
            ctx.abstol
        };
        let tol = ctx.reltol * rhs.abs().max(prev.abs()) + floor;
        (prev - rhs).abs() <= tol
    }

    fn load_ac(&mut self, ctx: &mut AcCtx) {
        self.stamp_linear(|s, v| ctx.add_g(s, v));
    }

    fn load_pz(&mut self, ctx: &mut PzCtx) -> Result<(), &'static str> {
        self.stamp_linear(|s, v| ctx.add_g(s, v));
        Ok(())
    }

    /// No `DEVdisto` in the 3f5 (nor in ngspice): the source enters
    /// `.disto` through its linear AC stamps only.
    fn disto_setup(&mut self, _ctx: &crate::disto::DistoSetupCtx) -> Result<(), &'static str> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_netlist::parse_expr;

    fn tree(s: &str) -> Tree {
        Tree::new(&parse_expr(s).unwrap())
    }

    /// Value and derivatives at `vals`, with gmin 0.
    fn at(s: &str, vals: &[f64]) -> (f64, Vec<f64>) {
        let t = tree(s);
        assert_eq!(t.vars.len(), vals.len(), "{s}");
        let mut d = vec![0.0; vals.len()];
        let f = t.eval(0.0, vals, &mut d).unwrap();
        (f, d)
    }

    /// Central-difference check of every derivative tree.
    fn check_fd(s: &str, vals: &[f64]) {
        let (_, d) = at(s, vals);
        for i in 0..vals.len() {
            let h = 1e-6 * vals[i].abs().max(1.0);
            let mut up = vals.to_vec();
            let mut dn = vals.to_vec();
            up[i] += h;
            dn[i] -= h;
            let fd = (at(s, &up).0 - at(s, &dn).0) / (2.0 * h);
            assert!(
                (fd - d[i]).abs() <= 1e-6 * fd.abs().max(1.0),
                "{s} d/dx{i}: symbolic {} vs fd {fd}",
                d[i]
            );
        }
    }

    #[test]
    fn derivatives_match_finite_differences() {
        for s in [
            "v(1)*v(2)+v(1)/v(2)-v(2)^3",
            "exp(v(1))*sin(v(2))+cos(v(1)*v(2))",
            "sqrt(v(1))+ln(v(1))+log(v(2))",
            "atan(v(1))+asin(v(1)/2)+acos(v(1)/2)+sinh(v(2))+cosh(v(2))+tanh(v(2))",
            "asinh(v(1))+acosh(v(2)+2)+atanh(v(1)/2)+tan(v(1))",
            "v(1)^v(2)",
            "2^v(1)+v(1)^(1+1)",
            "abs(v(1)-3)+uramp(v(2))+u(v(1))+sgn(v(2))",
            "-v(1)*-v(2)",
            "v(1,2)^2+i(vx)*v(1)",
        ] {
            let n = tree(s).vars.len();
            let vals: Vec<f64> = [0.7, 1.3, 0.4][..n].to_vec();
            check_fd(s, &vals);
        }
    }

    #[test]
    fn three_f5_function_semantics() {
        assert_eq!(at("log(v(1))", &[100.0]).0, 2.0);
        assert_eq!(at("ln(v(1))", &[-1.0]).0, 0.0);
        assert_eq!(at("sqrt(v(1))", &[-4.0]).0, 2.0);
        assert_eq!(at("u(v(1))", &[0.0]).0, 0.5);
        // A negative base with an integer exponent stays signed; with
        // a non-integer one, its magnitude is used.
        assert_eq!(at("v(1)^3", &[-2.0]).0, -8.0);
        assert_eq!(at("v(1)^0.5", &[-4.0]).0, 2.0);
        // The divide's gmin fudge.
        let t = tree("1/v(1)");
        let mut d = [0.0];
        assert_eq!(t.eval(1e-12, &[0.0], &mut d).unwrap(), 1e12);
        // `HUGE` is an evaluation error.
        let t = tree("v(1)^(-1)");
        assert!(t.eval(0.0, &[0.0], &mut d).is_err());
        let t = tree("exp(v(1))");
        assert!(t.eval(0.0, &[1000.0], &mut d).is_err());
        // ngspice's finite stand-in for ln(0).
        let t = tree("ln(v(1))");
        assert_eq!(t.eval(1e-12, &[0.0], &mut d).unwrap(), -1e99);
    }

    #[test]
    fn folding_follows_mkb() {
        // d/dx (3*x) folds to the constant 3; d/dx(x^1) to 1.
        let t = tree("3*v(1)");
        assert!(matches!(t.nodes[t.derivs[0] as usize], Node::Const(c) if c == 3.0));
        let t = tree("v(1)^1");
        assert!(matches!(t.nodes[t.derivs[0] as usize], Node::Const(c) if c == 1.0));
    }

    #[test]
    fn variables_dedupe_in_order() {
        let t = tree("v(b)*i(v1)+v(a)+v(b)+i(v1)");
        assert_eq!(
            t.vars,
            vec![
                VarRef::Node("b".into()),
                VarRef::Branch("v1".into()),
                VarRef::Node("a".into())
            ]
        );
    }
}
