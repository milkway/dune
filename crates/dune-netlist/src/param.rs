//! `.param` resolution — the global scope and the local scope of each
//! subcircuit instance — and the evaluation of parameter values on
//! cards (an ngspice extension; G2 and G3 of `docs/pdk-parser.md`).
//!
//! The expression language itself lives in `pexpr.rs`. This module
//! implements the scopes as observed in ngspice-47
//! (`docs/dune-netlist.md`, "Linguagem de expressões de parâmetros"
//! and "Parâmetros de subcircuito"):
//!
//! - every `.param` definition of a scope is collected first; for a
//!   name defined more than once the **last** definition is the only
//!   one that exists (earlier ones are never evaluated, even for uses
//!   that precede the redefinition);
//! - definitions may refer to names defined later (forward
//!   references); evaluation follows the dependency graph, iteratively
//!   (no recursion), each definition once;
//! - a dependency cycle, an undefined name, a syntax error or a
//!   random/`temper` problem in any `.param` of a scope that exists is
//!   an error with a span, used or not (ngspice-47 aborts on all of
//!   them); a subcircuit's scope exists only per instance;
//! - in a subcircuit instance's scope a definition that names itself
//!   (`.param w={w*2}`, `w={w+1}` on the `X` line) reads the name in
//!   the enclosing scope — the caller's instance, then the global
//!   one (probes `s1 body_self`, `s3 x_self_nested`); unknown names
//!   are looked up the same way (dynamic scoping, `s1 dyn_scope`);
//! - a `.param` may hold `inf`/NaN and pass it to other `.param`s; a
//!   non-finite value reaching a card is an error (D-0064);
//! - the value a card sees is the result formatted with `%.15e` and
//!   read back by `INPevaluate` ([`crate::pexpr::as_substituted`]).

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::func::FuncScope;
use crate::pexpr::{self, EvalCtx, Expr, PError, RandomPolicy, VarRef};
use crate::span::Span;

/// Most global `.param` definitions read from one deck (sky130's
/// whole library has 170 406), so a hostile deck cannot make the
/// resolver's tables exhaust memory.
pub(crate) const MAX_PARAMS: usize = 1_000_000;

/// The resolved parameters of one scope: the global one, or one
/// subcircuit instance's, which reads through to its caller's.
#[derive(Debug)]
pub(crate) struct ParamEnv<'p> {
    map: HashMap<String, f64>,
    /// Names whose definition failed with an error already reported:
    /// their uses fail silently instead of cascading.
    poisoned: HashSet<String>,
    /// The enclosing (calling) scope; `None` for the global scope.
    parent: Option<&'p ParamEnv<'p>>,
    /// The user functions (`.func`) visible here — lexical, unlike
    /// `parent`, which follows the calls.
    pub(crate) funcs: Option<&'p FuncScope<'p>>,
    /// Evaluation state; only the global scope's is used.
    ctx: EvalCtx,
    /// Span of the first random-function call evaluated (global
    /// scope only; instance scopes record into it).
    random_first: Cell<Option<Span>>,
    /// Names of this scope whose value depends on `temper`.
    tainted: HashSet<String>,
    /// Seeded mode only: the `.param`s of this scope whose own
    /// expression calls a random function. ngspice-47 makes each of
    /// them a function, so every reference evaluates it again and
    /// draws anew (probes `temper-seed-probes t2 b_glob_two_devices`,
    /// `b_twice_in_expr`, `b_sub_local`, `b_sub_default`, `t3
    /// c_expr_param`); a parameter that only reads one (`q={p}`) keeps
    /// one value (`b_through_param`, `c_local_of_global`). The span
    /// is the definition's value.
    random_defs: HashMap<String, (Expr, Span)>,
    /// The key of this scope in the draw keys: 0 for the global
    /// scope, a hash of the instance path for a subcircuit instance
    /// (`pexpr::EvalCtx` notes).
    path: u64,
    /// Global scope only: the circuit temperature (°C) `temper` reads,
    /// where the card being evaluated may read it, and the first
    /// place it was read.
    temperature: Cell<f64>,
    temper_use: Cell<TemperUse>,
    temper_first: Cell<Option<Span>>,
}

/// Where `temper` (and a parameter that depends on it) may be read —
/// ngspice-47 re-evaluates `{…temper…}` itself only on `.model` cards
/// and on device lines other than `V I E G F H B` (`inp_temper_compat`,
/// `frontend/inpcom.c`, Modified BSD), and turns a `.param` naming it
/// into a function so that those lines see it (`inp_fix_temper_in_param`);
/// everywhere else the parameter preprocessor answers "Undefined
/// parameter [temper]" (probes `g9-probes t1`, `t2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemperUse {
    /// A `.model` value or a device card that ngspice re-evaluates.
    Device,
    /// A `.param` definition or a `.subckt`-line default: it may name
    /// `temper` (and then depends on it), but not through the body of
    /// a user function (`t1 t_func_glob`).
    Param,
    /// Not available here; the text says where "here" is.
    Forbidden(&'static str),
}

/// Default circuit temperature (°C), as the 3f5's `CKTtemp`.
const DEFAULT_TEMP: f64 = 27.0;

impl Default for ParamEnv<'_> {
    fn default() -> Self {
        ParamEnv {
            map: HashMap::new(),
            poisoned: HashSet::new(),
            parent: None,
            funcs: None,
            ctx: EvalCtx::new(RandomPolicy::Nominal),
            random_first: Cell::new(None),
            tainted: HashSet::new(),
            random_defs: HashMap::new(),
            path: 0,
            temperature: Cell::new(DEFAULT_TEMP),
            temper_use: Cell::new(TemperUse::Forbidden("this position")),
            temper_first: Cell::new(None),
        }
    }
}

/// One `.param name = value` definition (global or local), a
/// parameter default on a `.subckt` line or an actual parameter on
/// an `X` line.
#[derive(Clone, Debug)]
pub(crate) struct ParamDef<'c> {
    /// Lowercased.
    pub name: String,
    pub name_span: Span,
    pub card: &'c Card,
    /// Byte offset of `value` inside `card.text`.
    pub value_start: usize,
    /// The expression text, without its `{…}`/`'…'` delimiters.
    pub value: &'c str,
    /// Whether the value may read `temper` (not on an `X` line).
    pub temper_ok: bool,
}

/// A value that could not be computed: a diagnostic, or `None` when
/// the cause was already reported (a failed `.param` it depends on).
pub(crate) type ValueError = Option<Diagnostic>;

impl<'p> ParamEnv<'p> {
    /// The global scope at the root of the chain.
    pub(crate) fn root(&self) -> &ParamEnv<'p> {
        let mut env = self;
        while let Some(p) = env.parent {
            env = p;
        }
        env
    }

    /// The value of `name` in this scope or an enclosing one. A random
    /// `.param` (seeded mode) is drawn, keyed by this scope and the
    /// name.
    pub fn get(&self, name: &str) -> Option<f64> {
        let mut env = self;
        loop {
            if let Some(v) = env.map.get(name) {
                return Some(*v);
            }
            if env.random_defs.contains_key(name) {
                let v = VarRef {
                    name: name.into(),
                    start: 0,
                    end: name.len().min(u32::MAX as usize) as u32,
                };
                let silenced = Cell::new(false);
                let root = pexpr::mix_in(self.path, fnv(name));
                return self
                    .root()
                    .ctx
                    .with_root(root, || self.lookup(&v, &silenced))
                    .ok();
            }
            if env.poisoned.contains(name) {
                return None;
            }
            env = env.parent?;
        }
    }

    /// The warning that random functions were evaluated at their
    /// nominal value, once per deck, at the first such call.
    pub fn random_note(&self) -> Option<Diagnostic> {
        let root = self.root();
        let span = root.random_first.get()?;
        if root.ctx.policy != RandomPolicy::Nominal {
            return None;
        }
        Some(Diagnostic::warning(
            "random functions (agauss, gauss, aunif, unif, limit) are evaluated at their \
             nominal value; ngspice draws a new random value on every run (`.options seed=N` \
             draws them reproducibly)",
            span,
        ))
    }

    /// Run `f` reading `card`: with draws on, its draws are keyed by
    /// the card's text (see `span_key`).
    pub fn with_card<R>(&self, card: &Card, f: impl FnOnce() -> R) -> R {
        if !self.seeded() {
            return f();
        }
        let ctx = &self.root().ctx;
        let at = card.span();
        let before = ctx
            .card
            .replace(Some((fnv(&card.text), at.file.0, at.start)));
        let r = f();
        ctx.card.set(before);
        r
    }

    /// Whether random functions draw (a seed is in effect).
    pub fn seeded(&self) -> bool {
        self.root().ctx.policy != RandomPolicy::Nominal
    }

    /// Whether some random function was evaluated (nominal or not).
    pub fn random_used(&self) -> bool {
        self.root().random_first.get().is_some() || self.root().ctx.random_used.get()
    }

    /// Whether some parameter of this scope depends on `temper`.
    pub fn has_tainted(&self) -> bool {
        !self.tainted.is_empty()
    }

    /// Set the circuit temperature `temper` reads (°C).
    pub fn set_temperature(&self, t: f64) {
        self.root().temperature.set(t);
    }

    /// The circuit temperature `temper` reads (°C).
    pub fn temperature(&self) -> f64 {
        self.root().temperature.get()
    }

    /// Where the first `temper` was read, if any.
    pub fn temper_first(&self) -> Option<Span> {
        self.root().temper_first.get()
    }

    /// Whether `name`, as this scope resolves it, depends on `temper`
    /// (ngspice-47 makes such a `.param` a function and expands it
    /// into the line, `inp_fix_temper_in_param`).
    pub fn is_tainted(&self, name: &str) -> bool {
        let mut env = self;
        loop {
            if env.map.contains_key(name) || env.random_defs.contains_key(name) {
                return env.tainted.contains(name);
            }
            if env.poisoned.contains(name) {
                return false;
            }
            match env.parent {
                Some(p) => env = p,
                None => return false,
            }
        }
    }

    /// Spend `units` of the deck's user-function budget (shared with
    /// the parameter evaluator, `pexpr::MAX_FN_WORK`); `false` once it
    /// is spent.
    pub fn charge_fn_work(&self, units: u64) -> bool {
        self.root().ctx.charge(units)
    }

    /// Record that the card at `at` uses the circuit temperature,
    /// evaluated now (the `tc1`/`tc2` factor of a behavioural R/C/L):
    /// it counts as a read of `temper` for the once-only warning and
    /// the `.dc temp` refusal.
    pub fn note_temperature_use(&self, at: Span) {
        let root = self.root();
        if root.temper_first.get().is_none() {
            root.temper_first.set(Some(at));
        }
    }

    /// The value of a bare name in a behavioural R/C/L expression
    /// (`behav.rs`): `temper` or a parameter, with the same rules of
    /// `temper` as a value on the card (`read_temper`); a read of
    /// `temper` is recorded at `at`. `Err(None)` is a failure already
    /// reported (a failed `.param`).
    pub fn read_for_behav(&self, name: &str, at: Span) -> Result<f64, Option<String>> {
        let root = self.root();
        let v = VarRef {
            name: name.into(),
            start: 0,
            end: name.len().min(u32::MAX as usize) as u32,
        };
        let silenced = Cell::new(false);
        let before = root.ctx.temper_touched.replace(false);
        let key = span_key(self.path, at, root.ctx.card.get());
        let r = root.ctx.with_root(key, || self.lookup(&v, &silenced));
        let touched = root.ctx.temper_touched.get();
        root.ctx.temper_touched.set(before || touched);
        match r {
            Ok(x) => {
                if touched
                    && root.temper_use.get() == TemperUse::Device
                    && root.temper_first.get().is_none()
                {
                    root.temper_first.set(Some(at));
                }
                Ok(x)
            }
            Err(e) if silenced.get() || e.msg.is_empty() => Err(None),
            Err(e) => Err(Some(e.msg)),
        }
    }

    /// Run `f` with `temper` readable as `u` says, then restore.
    pub fn with_temper<R>(&self, u: TemperUse, f: impl FnOnce() -> R) -> R {
        let root = self.root();
        let before = root.temper_use.replace(u);
        let r = f();
        root.temper_use.set(before);
        r
    }

    /// The use of `temper` a device card allows: ngspice-47 leaves
    /// `V I E G F H B` lines to the parameter preprocessor (`t1
    /// t_v_pos`, `t_i_pos`), and its `K` parser rejects the rewritten
    /// value (`t2 t_k`).
    pub fn temper_for_card(card: &Card) -> TemperUse {
        let first = card
            .text
            .trim_start()
            .bytes()
            .next()
            .map(|c| c.to_ascii_lowercase());
        match first {
            Some(b'v' | b'i' | b'e' | b'g' | b'f' | b'h' | b'b' | b'k') => {
                TemperUse::Forbidden("a V, I, E, G, F, H, B or K card")
            }
            _ => TemperUse::Device,
        }
    }

    /// Read `temper` or a parameter that depends on it (`v`, found in
    /// `env`, whose `tainted` set it is in): allowed where the card
    /// being evaluated allows it, and only in the scope that defined
    /// the parameter (`outer`: found in an enclosing scope —
    /// ngspice turns the `.param` into a function of its own
    /// subcircuit level, `t2 t_tparam_in_sub`,
    /// `t_sub_local_nested_use`).
    fn read_temper(&self, v: &VarRef, outer: bool, value: f64) -> Result<f64, PError> {
        let root = self.root();
        let (s, e) = (v.start as usize, v.end as usize);
        let direct = &*v.name == "temper";
        let what = if direct {
            "`temper`".to_string()
        } else {
            format!("`{}` (it depends on `temper`)", v.name)
        };
        if outer {
            return Err(PError::new(
                format!(
                    "{what} is defined in an enclosing scope; ngspice-47 makes a parameter that depends on `temper` a function of its own subcircuit level only"
                ),
                s,
                e,
            ));
        }
        match root.temper_use.get() {
            TemperUse::Device => {}
            TemperUse::Param if direct && root.ctx.in_body.get() > 0 => {
                return Err(PError::new(
                    "`temper` inside a user function called from a .param is not evaluated by ngspice-47 (Undefined parameter); name `temper` in the .param itself",
                    s,
                    e,
                ));
            }
            TemperUse::Param => {}
            TemperUse::Forbidden(where_) => {
                return Err(PError::new(
                    format!(
                        "{what} is not available in {where_}: ngspice-47 evaluates it only in .model cards, device values and .param (Undefined parameter)"
                    ),
                    s,
                    e,
                ));
            }
        }
        root.ctx.temper_touched.set(true);
        Ok(value)
    }

    /// Resolve `v` in this scope, then in the enclosing ones.
    fn lookup(&self, v: &VarRef, silenced: &Cell<bool>) -> Result<f64, PError> {
        if &*v.name == "temper" {
            return self.read_temper(v, false, self.root().temperature.get());
        }
        let mut env = self;
        let mut outer = false;
        loop {
            if let Some(x) = env.map.get(&*v.name) {
                if env.tainted.contains(&*v.name) {
                    return env.read_temper(v, outer, *x);
                }
                return Ok(*x);
            }
            if let Some((expr, at)) = env.random_defs.get(&*v.name) {
                if env.tainted.contains(&*v.name) {
                    env.read_temper(v, outer, 0.0)?;
                }
                return env.eval_random(expr, *at, v, silenced);
            }
            outer = true;
            let (s, e) = (v.start as usize, v.end as usize);
            if env.poisoned.contains(&*v.name) {
                silenced.set(true);
                return Err(PError::silenced(s, e));
            }
            match env.parent {
                Some(p) => env = p,
                None => {
                    return Err(PError::new(format!("unknown parameter `{}`", v.name), s, e));
                }
            }
        }
    }

    /// Evaluate random `.param` `v` of this scope again (seeded mode:
    /// every reference draws anew). Nested re-evaluations are bounded
    /// by [`pexpr::MAX_RANDOM_DEPTH`] and charged to the deck's
    /// function budget; a failure inside the definition points at it.
    fn eval_random(
        &self,
        expr: &Expr,
        at: Span,
        v: &VarRef,
        silenced: &Cell<bool>,
    ) -> Result<f64, PError> {
        let root = self.root();
        let (s, e) = (v.start as usize, v.end as usize);
        let depth = root.ctx.random_depth.get();
        if depth >= pexpr::MAX_RANDOM_DEPTH {
            return Err(PError::new(
                format!(
                    "random parameters re-evaluated inside each other more than {} deep at `{}`",
                    pexpr::MAX_RANDOM_DEPTH,
                    v.name
                ),
                s,
                e,
            ));
        }
        if !root.ctx.charge(1 + expr.size() as u64) {
            return Err(PError::new(
                format!(
                    "the deck's evaluation budget is spent re-drawing random parameter `{}`",
                    v.name
                ),
                s,
                e,
            ));
        }
        root.ctx.random_depth.set(depth + 1);
        let r = expr.eval_in(
            &mut |w| match self.parent {
                // A definition naming itself reads the enclosing scope.
                Some(p) if *w.name == *v.name => p.lookup(w, silenced),
                _ => self.lookup(w, silenced),
            },
            &root.ctx,
            self.funcs,
        );
        root.ctx.random_depth.set(depth);
        r.map_err(|mut err| {
            if err.at.is_none() && !err.msg.is_empty() {
                err.at = Some(at);
            }
            err
        })
    }

    /// Evaluate a compiled expression in this scope. `own`, in an
    /// instance scope, is the name being defined: it reads the
    /// enclosing scope's value. `span_of` maps byte ranges of the
    /// expression text to spans.
    fn eval_expr(
        &self,
        expr: &Expr,
        own: Option<&str>,
        span_of: &dyn Fn(usize, usize) -> Span,
    ) -> Result<f64, ValueError> {
        let silenced = Cell::new(false);
        let root = self.root();
        let random_before = root.ctx.random_used.get();
        let key = span_key(self.path, span_of(0, 0), root.ctx.card.get());
        let r = root.ctx.with_root(key, || {
            expr.eval_in(
                &mut |v| match (own, self.parent) {
                    (Some(n), Some(p)) if *v.name == *n => p.lookup(v, &silenced),
                    _ => self.lookup(v, &silenced),
                },
                &root.ctx,
                self.funcs,
            )
        });
        match r {
            Ok(v) => {
                if root.ctx.temper_touched.get()
                    && root.temper_use.get() == TemperUse::Device
                    && root.temper_first.get().is_none()
                {
                    root.temper_first.set(Some(span_of(0, 0)));
                }
                if root.random_first.get().is_none() && root.ctx.random_used.get() {
                    // The first random call, or (drawn inside a user
                    // function's body) the first call of the expression.
                    let at = expr.first_random_call().or_else(|| {
                        (!random_before)
                            .then(|| expr.calls().first())
                            .flatten()
                            .map(|c| (c.start as usize, c.end as usize))
                    });
                    if let Some((s, e)) = at {
                        root.random_first.set(Some(span_of(s, e)));
                    }
                }
                Ok(v)
            }
            Err(e) if silenced.get() || e.msg.is_empty() => Err(None),
            Err(e) => {
                let span = e.at.unwrap_or_else(|| span_of(e.start, e.end));
                Err(Some(Diagnostic::error(e.msg, span)))
            }
        }
    }

    /// Evaluate a parameter value written on a card: `{expr}`,
    /// `'expr'` or a bare expression (after `name=`), as the card
    /// sees it — finite, and rounded the way ngspice's text
    /// substitution rounds it. `span_of` maps byte ranges of `token`.
    pub fn eval_value(
        &self,
        token: &str,
        span_of: &dyn Fn(usize, usize) -> Span,
    ) -> Result<f64, ValueError> {
        let (inner, off) = match delimited(token) {
            Ok(x) => x,
            Err(msg) => return Err(Some(Diagnostic::error(msg, span_of(0, token.len())))),
        };
        // ngspice deletes `"` from every card but `.param`, `.subckt`
        // and `X` before substituting (`inp_casefix`, Modified BSD;
        // probe c10: `{2*"3+1"}` is 7, sky130 writes
        // `dw = {"-tol_nfom/2-nfom_dw/2"}`). A blank keeps the offsets.
        let unquoted;
        let inner = if inner.contains('"') {
            unquoted = inner.replace('"', " ");
            unquoted.as_str()
        } else {
            inner
        };
        let expr = pexpr::parse(inner).map_err(|e| {
            Some(Diagnostic::error(
                e.msg,
                span_of(off + e.start, off + e.end),
            ))
        })?;
        let v = self.eval_expr(&expr, None, &|s, e| span_of(off + s, off + e))?;
        if v.is_finite() {
            Ok(pexpr::as_substituted(v))
        } else {
            Err(Some(Diagnostic::error(
                format!("parameter expression `{token}` evaluates to {v}, not a finite number"),
                span_of(0, token.len()),
            )))
        }
    }
}

/// The names a card value token reads (as [`ParamEnv::eval_value`]
/// would parse it), or `None` if it does not parse or calls one of
/// `user_fns` (whose body may read more names).
pub(crate) fn value_vars(token: &str, user_fns: &HashSet<String>) -> Option<Vec<Box<str>>> {
    let (inner, _) = delimited(token).ok()?;
    let expr = pexpr::parse(&inner.replace('"', " ")).ok()?;
    if expr.calls().iter().any(|c| user_fns.contains(&*c.name)) {
        return None;
    }
    Some(expr.vars().iter().map(|v| v.name.clone()).collect())
}

/// Strip `{…}` or `'…'` from a value token; a bare token is its own
/// expression. Returns the inner text and its offset in `token`.
fn delimited(token: &str) -> Result<(&str, usize), String> {
    if let Some(rest) = token.strip_prefix('{') {
        match rest.strip_suffix('}') {
            Some(inner) => Ok((inner, 1)),
            None => Err(format!("missing `}}` in parameter expression `{token}`")),
        }
    } else if let Some(rest) = token.strip_prefix('\'') {
        match rest.strip_suffix('\'') {
            Some(inner) => Ok((inner, 1)),
            None => Err(format!(
                "missing closing `'` in parameter expression `{token}`"
            )),
        }
    } else {
        Ok((token, 0))
    }
}

/// Longest dependency chain printed in a cycle diagnostic.
const CYCLE_NAMES_SHOWN: usize = 8;

/// Resolve the global `.param` definitions (in deck order) into the
/// global scope, reporting every failure.
/// `temperature` is the circuit temperature `temper` reads (°C).
/// `policy` says how random functions are evaluated (D-0075).
pub(crate) fn resolve<'p>(
    defs: &[ParamDef],
    funcs: Option<&'p FuncScope<'p>>,
    temperature: f64,
    policy: RandomPolicy,
    diags: &mut Vec<Diagnostic>,
) -> ParamEnv<'p> {
    resolve_scope(defs, None, funcs, Some((temperature, policy)), 0, diags)
}

/// Resolve one subcircuit instance's definitions — the `.subckt`
/// line defaults and body `.param`s in order, then the `X` line's
/// actual parameters (which therefore win) — into a scope that reads
/// through to `parent`, the caller's scope.
/// `path` is the instance's expanded name (`x1.x2`), which keys its
/// draws.
pub(crate) fn resolve_local<'p>(
    defs: &[ParamDef],
    parent: &'p ParamEnv<'p>,
    funcs: Option<&'p FuncScope<'p>>,
    path: &str,
    diags: &mut Vec<Diagnostic>,
) -> ParamEnv<'p> {
    resolve_scope(defs, Some(parent), funcs, None, fnv(path), diags)
}

/// FNV-1a of a name (draw keys).
pub(crate) fn fnv(name: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The draw key of an evaluation at `at` in the scope keyed `path`:
/// the text of the card being read and the offset of `at` in it — so
/// a title, a comment or a card added elsewhere does not move the
/// draws — else (no card known) the file and byte position.
fn span_key(path: u64, at: Span, card: Option<(u64, u32, u32)>) -> u64 {
    match card {
        Some((text, file, start)) if file == at.file.0 && at.start >= start => {
            pexpr::mix_in(pexpr::mix_in(path, text), u64::from(at.start - start))
        }
        _ => {
            let k = pexpr::mix_in(path ^ 0x5bd1_e995, u64::from(at.file.0));
            pexpr::mix_in(k, u64::from(at.start))
        }
    }
}

/// Resolve the definitions of one scope (in order) into an
/// environment, reporting every failure. Linear in the total size of
/// the definitions, with memory linear in the number of *distinct*
/// dependencies: each winning definition is compiled once to find
/// the parameters it reads (then dropped), and compiled again when
/// its turn comes in the dependency walk, which uses an explicit
/// stack — no recursion, whatever the chain length. With a `parent`
/// (an instance scope), a definition's reference to its own name is
/// not a dependency: it reads the parent's value.
fn resolve_scope<'p>(
    defs: &[ParamDef],
    parent: Option<&'p ParamEnv<'p>>,
    funcs: Option<&'p FuncScope<'p>>,
    global: Option<(f64, RandomPolicy)>,
    path: u64,
    diags: &mut Vec<Diagnostic>,
) -> ParamEnv<'p> {
    let mut env = ParamEnv {
        parent,
        funcs,
        path,
        ..ParamEnv::default()
    };
    if let Some((t, policy)) = global {
        env.temperature.set(t);
        env.ctx = EvalCtx::new(policy);
    }
    let seeded = env.root().ctx.policy != RandomPolicy::Nominal;
    let mut via_funcs: Vec<Box<str>> = Vec::new();
    let local = parent.is_some();
    let n_defs = defs.len();

    // Last definition wins: only the final definition of a name exists.
    let mut winner: HashMap<&str, u32> = HashMap::with_capacity(n_defs);
    for (i, d) in defs.iter().enumerate() {
        winner.insert(d.name.as_str(), i as u32);
    }
    let mut order: Vec<u32> = winner.values().copied().collect();
    order.sort_unstable();

    // Dependencies of each winner, deduplicated, in CSR form: the
    // parameters of this scope (by definition index) its expression
    // reads.
    let mut compiled = vec![false; n_defs];
    let mut dep_start: Vec<u32> = vec![0; n_defs + 1];
    let mut dep_list: Vec<u32> = Vec::new();
    {
        let mut row: Vec<u32> = Vec::new();
        let mut next = 0usize; // definitions [next, i) have empty rows
        for &i in &order {
            let i = i as usize;
            for s in &mut dep_start[next + 1..=i] {
                *s = dep_list.len() as u32;
            }
            let d = &defs[i];
            match pexpr::parse(d.value) {
                Ok(e) => {
                    compiled[i] = true;
                    row.clear();
                    row.extend(
                        e.vars()
                            .iter()
                            .filter(|v| !(local && *v.name == *d.name))
                            .filter_map(|v| winner.get(&*v.name).copied()),
                    );
                    // The names read inside the user functions it calls
                    // are dependencies too (probe `f1 f_param_uses_func_fwd`).
                    if let Some(fs) = funcs {
                        via_funcs.clear();
                        let ctx = parent.map_or(&env.ctx, |p| &p.root().ctx);
                        fs.free_names(&e, ctx, &mut via_funcs);
                        row.extend(
                            via_funcs
                                .iter()
                                .filter(|n| !(local && ***n == *d.name))
                                .filter_map(|n| winner.get(&**n).copied()),
                        );
                    }
                    row.sort_unstable();
                    row.dedup();
                    dep_list.extend_from_slice(&row);
                }
                Err(e) => {
                    diags.push(Diagnostic::error(
                        e.msg,
                        d.card
                            .span_of(d.value_start + e.start, d.value_start + e.end),
                    ));
                    env.poisoned.insert(d.name.clone());
                }
            }
            dep_start[i + 1] = dep_list.len() as u32;
            next = i + 1;
        }
        for s in &mut dep_start[next + 1..] {
            *s = dep_list.len() as u32;
        }
    }
    let deps = |n: usize| &dep_list[dep_start[n] as usize..dep_start[n + 1] as usize];

    const UNSEEN: u8 = 0;
    const OPEN: u8 = 1;
    const DONE: u8 = 2;
    let mut state = vec![UNSEEN; n_defs];
    let mut in_cycle = vec![false; n_defs];
    let mut stack_pos = vec![u32::MAX; n_defs];
    let mut stack: Vec<(u32, u32)> = Vec::new();

    for &root in &order {
        let root = root as usize;
        if state[root] != UNSEEN || !compiled[root] {
            continue;
        }
        state[root] = OPEN;
        stack_pos[root] = 0;
        stack.push((root as u32, 0));
        while let Some(&(n, k)) = stack.last() {
            let (n, k) = (n as usize, k as usize);
            if k < deps(n).len() {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                let d = deps(n)[k] as usize;
                match state[d] {
                    UNSEEN if compiled[d] => {
                        state[d] = OPEN;
                        stack_pos[d] = stack.len() as u32;
                        stack.push((d as u32, 0));
                    }
                    OPEN => {
                        // A cycle d … n → d: report it once, at the
                        // reference that closes it.
                        let from = stack_pos[d] as usize;
                        if !stack[from..].iter().any(|t| in_cycle[t.0 as usize]) {
                            diags.push(cycle_error(defs, &stack[from..], n, d));
                        }
                        for t in &stack[from..] {
                            in_cycle[t.0 as usize] = true;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            stack.pop();
            state[n] = DONE;
            let d = &defs[n];
            if in_cycle[n] {
                env.poisoned.insert(d.name.clone());
                continue;
            }
            let span_of = |s: usize, e: usize| d.card.span_of(d.value_start + s, d.value_start + e);
            let own = local.then_some(d.name.as_str());
            // An `X`-line actual may not read `temper` (`t1 t_sub_actual`).
            let rules = if d.temper_ok {
                TemperUse::Param
            } else {
                TemperUse::Forbidden("a subcircuit call's parameters (X line)")
            };
            let tctx = &env.root().ctx;
            tctx.temper_touched.set(false);
            let mut random = None;
            let value = match pexpr::parse(d.value) {
                Ok(expr) => {
                    let v = env.with_card(d.card, || {
                        env.with_temper(rules, || env.eval_expr(&expr, own, &span_of))
                    });
                    // Seeded: a definition that calls a random function
                    // itself becomes a function of its scope (evaluated
                    // once above for its diagnostics).
                    // An `X`-line actual (`temper_ok` false) stays one
                    // value per instance (`t2 b_sub_actual`).
                    if seeded && d.temper_ok && expr.first_random_call().is_some() {
                        random = Some(expr);
                    }
                    v
                }
                Err(e) => Err(Some(Diagnostic::error(e.msg, span_of(e.start, e.end)))),
            };
            let touched = env.root().ctx.temper_touched.replace(false);
            match value {
                Ok(v) => {
                    match random {
                        Some(expr) => {
                            let at = span_of(0, d.value.len());
                            env.random_defs.insert(d.name.clone(), (expr, at));
                        }
                        None => {
                            env.map.insert(d.name.clone(), v);
                        }
                    }
                    if touched {
                        env.tainted.insert(d.name.clone());
                    }
                }
                Err(diag) => {
                    diags.extend(diag);
                    env.poisoned.insert(d.name.clone());
                }
            }
        }
    }
    env
}

/// The diagnostic for a dependency cycle: `members` is the stack from
/// `d` to `n`, and `n` reads `d`. The span is `n`'s reference to `d`.
fn cycle_error(defs: &[ParamDef], members: &[(u32, u32)], n: usize, d: usize) -> Diagnostic {
    let mut names: Vec<&str> = members
        .iter()
        .take(CYCLE_NAMES_SHOWN)
        .map(|t| defs[t.0 as usize].name.as_str())
        .collect();
    if members.len() > CYCLE_NAMES_SHOWN {
        names.push("…");
    }
    names.push(defs[d].name.as_str());
    let dn = &defs[n];
    let span = pexpr::parse(dn.value)
        .ok()
        .and_then(|e| {
            e.vars()
                .iter()
                .find(|v| *v.name == *defs[d].name)
                .map(|v| (v.start as usize, v.end as usize))
        })
        .map(|(s, e)| dn.card.span_of(dn.value_start + s, dn.value_start + e))
        .unwrap_or(dn.name_span);
    Diagnostic::error(
        format!("parameter dependency cycle: {}", names.join(" → ")),
        span,
    )
}

#[cfg(test)]
mod tests {
    use crate::diag::Severity;
    use crate::parse::parse_str;

    fn run(deck: &str) -> (Vec<String>, crate::ir::Netlist) {
        let out = parse_str("t.cir", deck);
        let errs = out
            .diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.clone())
            .collect();
        (errs, out.netlist)
    }

    fn dc(deck: &str) -> f64 {
        let (errs, nl) = run(deck);
        assert!(errs.is_empty(), "{errs:?}");
        match &nl.elements[0].device {
            crate::ir::Device::Source { dc: Some(v), .. } => *v,
            d => panic!("{d:?}"),
        }
    }

    // Probes in `.claude/scratch/g2-probes/c5` (resolution) and P02/P03.

    #[test]
    fn forward_references_and_last_definition_wins() {
        assert_eq!(
            dc("t\n.param a={b+1}\n.param b=2\nv1 1 0 {a}\n.end\n"),
            3.0f64.next_up()
        );
        assert_eq!(dc("t\n.param c=1\nv1 1 0 {c}\n.param c=5\n.end\n"), 5.0);
        assert_eq!(
            dc("t\n.param c=1\n.param d={c}\n.param c=5\nv1 1 0 {d}\n.end\n"),
            5.0
        );
        assert_eq!(
            dc("t\n.param a={b}\n.param b={c}\n.param c=7\nv1 1 0 {a}\n.end\n"),
            7.0f64.next_up()
        );
        assert_eq!(dc("t\n.param a=1 a=2\nv1 1 0 {a}\n.end\n"), 2.0);
        assert_eq!(dc("t\n.param A=1\n.param a=2\nv1 1 0 {A}\n.end\n"), 2.0);
        // An overridden definition is never evaluated.
        assert_eq!(
            dc("t\n.param a={nosuch}\n.param a=2\nv1 1 0 {a}\n.end\n"),
            2.0
        );
    }

    #[test]
    fn global_errors_are_fatal_even_unused() {
        let (e, _) = run("t\n.param c=1\n.param c={c+1}\nv1 1 0 1\n.end\n");
        assert!(e[0].contains("cycle: c → c"), "{e:?}");
        let (e, _) = run("t\n.param a={b}\n.param b={a}\nv1 1 0 1\n.end\n");
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("cycle: a → b → a"), "{e:?}");
        let (e, _) =
            run("t\n.param a={b}\n.param b={c}\n.param c={a}\n.param d={a}\nv1 1 0 {d}\n.end\n");
        assert_eq!(e.len(), 1, "{e:?}");
        let (e, _) = run("t\n.param a={nosuch}\nv1 1 0 1\n.end\n");
        assert!(e[0].contains("unknown parameter `nosuch`"), "{e:?}");
        let (e, _) = run("t\n.param a={foo(1)}\nv1 1 0 1\n.end\n");
        assert!(e[0].contains("unknown function `foo`"), "{e:?}");
        // A `.param` naming `temper` is a value (g9-probes t1 t_glob_unused).
        let (e, _) = run("t\n.param t1={temper}\nv1 1 0 1\n.end\n");
        assert!(e.is_empty(), "{e:?}");
        // A failed definition does not cascade into its users.
        let (e, _) = run("t\n.param a={1+}\n.param b={a*2}\nv1 1 0 {b}\n.end\n");
        assert_eq!(e.len(), 1, "{e:?}");
        // Non-finite values pass between .params (D-0064).
        assert_eq!(dc("t\n.param z={1/0}\nv1 1 0 {1/z}\n.end\n"), 0.0);
        let (e, _) = run("t\n.param a={sqrt(-1)}\n.param b={a}\nv1 1 0 1\n.end\n");
        assert!(e.is_empty(), "{e:?}");
    }

    #[test]
    fn long_chains_and_wide_fanout_are_linear() {
        // 20 000-deep forward chain (ngspice H5 is the backward one).
        let mut deck = String::from("t\n");
        for i in (1..20_000).rev() {
            deck.push_str(&format!(".param p{i}={{p{}+1}}\n", i - 1));
        }
        deck.push_str(".param p0=0\nv1 1 0 {p19999}\n.end\n");
        assert_eq!(dc(&deck), 19_999.0);
        // A long cycle reports once, with an abbreviated path.
        let mut deck = String::from("t\n");
        for i in 0..5000 {
            deck.push_str(&format!(".param q{i}={{q{}}}\n", (i + 1) % 5000));
        }
        deck.push_str("v1 1 0 1\n.end\n");
        let (e, _) = run(&deck);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains('…'));
        // Fan-out: one name used many times by many definitions.
        let mut deck = String::from("t\n.param x=1\n");
        let terms = vec!["x"; 200].join("+");
        for i in 0..2000 {
            deck.push_str(&format!(".param f{i}={{{terms}}}\n"));
        }
        deck.push_str("v1 1 0 {f1999}\n.end\n");
        assert_eq!(dc(&deck), 200.0);
    }

    #[test]
    fn random_functions_note_once() {
        let out = parse_str(
            "t.cir",
            "t\n.param g=agauss(1,0.1,1)\n.param h={gauss(2,0.1,1)}\nv1 1 0 {g+h}\n.end\n",
        );
        let notes: Vec<_> = out
            .diags
            .iter()
            .filter(|d| d.message.contains("nominal"))
            .collect();
        assert_eq!(notes.len(), 1, "{:?}", out.diags);
        assert_eq!(notes[0].severity, Severity::Warning);
        assert!(out.is_clean());
    }
}
