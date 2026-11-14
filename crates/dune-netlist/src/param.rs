//! Global `.param` resolution and the evaluation of parameter values
//! on cards (an ngspice extension; G2 of `docs/pdk-parser.md`).
//!
//! The expression language itself lives in `pexpr.rs`. This module
//! implements the global scope as observed in ngspice-47
//! (`docs/dune-netlist.md`, "Linguagem de expressões de parâmetros"):
//!
//! - every top-level `.param` definition is collected first; for a
//!   name defined more than once the **last** definition is the only
//!   one that exists (earlier ones are never evaluated, even for uses
//!   that precede the redefinition);
//! - definitions may refer to names defined later (forward
//!   references); evaluation follows the dependency graph, iteratively
//!   (no recursion), each definition once;
//! - a dependency cycle, an undefined name, a syntax error or a
//!   random/`temper` problem in any global `.param` is an error with a
//!   span, used or not (ngspice-47 aborts on all of them);
//! - a `.param` may hold `inf`/NaN and pass it to other `.param`s; a
//!   non-finite value reaching a card is an error (D-0064);
//! - the value a card sees is the result formatted with `%.15e` and
//!   read back by `INPevaluate` ([`crate::pexpr::as_substituted`]).

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use crate::deck::Card;
use crate::diag::Diagnostic;
use crate::pexpr::{self, EvalCtx, Expr, PError, RandomPolicy, VarRef};
use crate::span::Span;

/// Most global `.param` definitions read from one deck (sky130's
/// whole library has 170 406), so a hostile deck cannot make the
/// resolver's tables exhaust memory.
pub(crate) const MAX_PARAMS: usize = 1_000_000;

/// The resolved global parameters of one deck.
#[derive(Debug)]
pub(crate) struct ParamEnv {
    map: HashMap<String, f64>,
    /// Names whose definition failed with an error already reported:
    /// their uses fail silently instead of cascading.
    poisoned: HashSet<String>,
    ctx: EvalCtx,
    /// Span of the first random-function call evaluated.
    random_first: Cell<Option<Span>>,
}

impl Default for ParamEnv {
    fn default() -> Self {
        ParamEnv {
            map: HashMap::new(),
            poisoned: HashSet::new(),
            ctx: EvalCtx::new(RandomPolicy::Nominal),
            random_first: Cell::new(None),
        }
    }
}

/// One top-level `.param name = value` definition.
pub(crate) struct ParamDef<'c> {
    /// Lowercased.
    pub name: String,
    pub name_span: Span,
    pub card: &'c Card,
    /// Byte offset of `value` inside `card.text`.
    pub value_start: usize,
    /// The expression text, without its `{…}`/`'…'` delimiters.
    pub value: &'c str,
}

/// A value that could not be computed: a diagnostic, or `None` when
/// the cause was already reported (a failed `.param` it depends on).
pub(crate) type ValueError = Option<Diagnostic>;

impl ParamEnv {
    /// The warning that random functions were evaluated at their
    /// nominal value, once per deck, at the first such call.
    pub fn random_note(&self) -> Option<Diagnostic> {
        let span = self.random_first.get()?;
        Some(Diagnostic::warning(
            "random functions (agauss, gauss, aunif, unif, limit) are evaluated at their \
             nominal value; ngspice draws a new random value on every run",
            span,
        ))
    }

    fn lookup(&self, v: &VarRef, silenced: &Cell<bool>) -> Result<f64, PError> {
        if let Some(x) = self.map.get(&*v.name) {
            return Ok(*x);
        }
        let (s, e) = (v.start as usize, v.end as usize);
        if self.poisoned.contains(&*v.name) {
            silenced.set(true);
            return Err(PError {
                msg: String::new(),
                start: s,
                end: e,
            });
        }
        let msg = if &*v.name == "temper" {
            "`temper` in parameter expressions is not supported yet (it needs re-expansion \
             when the temperature changes)"
                .to_string()
        } else {
            format!("unknown parameter `{}`", v.name)
        };
        Err(PError {
            msg,
            start: s,
            end: e,
        })
    }

    /// Evaluate a compiled expression against the global parameters.
    /// `span_of` maps byte ranges of the expression text to spans.
    fn eval_expr(
        &self,
        expr: &Expr,
        span_of: &dyn Fn(usize, usize) -> Span,
    ) -> Result<f64, ValueError> {
        let silenced = Cell::new(false);
        let r = expr.eval(&mut |v| self.lookup(v, &silenced), &self.ctx);
        match r {
            Ok(v) => {
                if self.random_first.get().is_none()
                    && let Some((s, e)) = expr.first_random_call()
                {
                    self.random_first.set(Some(span_of(s, e)));
                }
                Ok(v)
            }
            Err(_) if silenced.get() => Err(None),
            Err(e) => Err(Some(Diagnostic::error(e.msg, span_of(e.start, e.end)))),
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
        let v = self.eval_expr(&expr, &|s, e| span_of(off + s, off + e))?;
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

/// Resolve the global `.param` definitions (in deck order) into an
/// environment, reporting every failure. Linear in the total size of
/// the definitions, with memory linear in the number of *distinct*
/// dependencies: each winning definition is compiled once to find
/// the parameters it reads (then dropped), and compiled again when
/// its turn comes in the dependency walk, which uses an explicit
/// stack — no recursion, whatever the chain length.
pub(crate) fn resolve(defs: &[ParamDef], diags: &mut Vec<Diagnostic>) -> ParamEnv {
    let mut env = ParamEnv::default();
    let n_defs = defs.len();

    // Last definition wins: only the final definition of a name exists.
    let mut winner: HashMap<&str, u32> = HashMap::with_capacity(n_defs);
    for (i, d) in defs.iter().enumerate() {
        winner.insert(d.name.as_str(), i as u32);
    }
    let mut order: Vec<u32> = winner.values().copied().collect();
    order.sort_unstable();

    // Dependencies of each winner, deduplicated, in CSR form: the
    // global parameters (by definition index) its expression reads.
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
                            .filter_map(|v| winner.get(&*v.name).copied()),
                    );
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
            let value = match pexpr::parse(d.value) {
                Ok(expr) => env.eval_expr(&expr, &span_of),
                Err(e) => Err(Some(Diagnostic::error(e.msg, span_of(e.start, e.end)))),
            };
            match value {
                Ok(v) => {
                    env.map.insert(d.name.clone(), v);
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
        let (e, _) = run("t\n.param t1={temper}\nv1 1 0 1\n.end\n");
        assert!(e[0].contains("temper"), "{e:?}");
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
