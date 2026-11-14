//! Subcircuit expansion (`fte/subckt.c`), done on the IR instead of
//! by string rewriting, with ngspice's subcircuit parameters
//! (G3 of `docs/pdk-parser.md`), model binning (G4) and the `m`
//! multiplier on `X` lines (G6).
//!
//! Naming follows the ngspice convention (the 3f5 used `:` where
//! ngspice uses `.`; the oracle compares vector names against
//! ngspice, see `docs/dune-netlist.md`):
//!
//! - a device `r1` inside instance `x1` becomes `r.x1.r1` — the type
//!   letter stays first, then the instance path, then the name;
//! - an internal node `n1` becomes `x1.n1`; ground (`0`) and nodes
//!   bound to ports keep the caller's names;
//! - device references (the inductors of a `K`, the controlling
//!   source of `F`/`H`/`W`) are renamed like devices;
//! - a model defined inside a subcircuit becomes `x1.mname` and only
//!   elements of that instance (and of the instances it calls) see
//!   it; models defined outside stay visible.
//!
//! A subcircuit body is kept **unevaluated** — its element and
//! `.model` cards as cards, its `.param`s and `.subckt`-line defaults
//! as definitions — and evaluated per instance during the expansion:
//! the instance's scope is resolved first (`param::resolve_local`,
//! reading through to the caller's scope), then each element card is
//! parsed in it, and a local `.model` is evaluated only when an
//! element of the instance uses it (one copy per instance, ngspice
//! P08). A subcircuit that is never instantiated is never evaluated,
//! and an unused model's errors are never reported (ngspice P05f,
//! probes `s1`/`s2` in `.claude/scratch/g3-probes`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::deck::Card;
use crate::diag::{Diagnostic, Severity};
use crate::elem::parse_element;
use crate::ir::{Device, Element, Expr, ExprOp, Model, Param, ParamValue, SourceKind};
use crate::param::{self, ParamDef, ParamEnv};
use crate::scan::Scanner;
use crate::span::Span;

/// Nesting bound for subcircuit instantiation.
const MAX_DEPTH: usize = 64;
/// Single work budget for the whole expansion. Every element visit,
/// instantiation, card parsed or model evaluated per instance and
/// binning candidate costs at least one unit, and text work costs one
/// unit per 64 bytes of emitted names or per 256 bytes of card parsed,
/// so a subcircuit bomb (wide `X`-only trees, deep paths with long
/// names, model-heavy bodies) cannot exhaust memory or CPU regardless
/// of its shape.
const MAX_WORK: usize = 1 << 20;
/// Distinct diagnostics reported by the expansion (the same problem
/// in many instances is reported once).
const MAX_DIAGS: usize = 1000;

/// One `X` card, read structurally: the actual parameters stay
/// unevaluated — they are evaluated in the called instance's scope
/// (probe `s3 xcl_both_mid`: `w={l}` reads the callee's `l`).
#[derive(Debug)]
pub(crate) struct XCall<'c> {
    pub name: String,
    pub nodes: Vec<String>,
    pub subname: String,
    pub actuals: Vec<ParamDef<'c>>,
    pub span: Span,
    pub card: &'c Card,
}

/// A `.model` card kept unevaluated, with the binning keys read off
/// it (`lmin lmax wmin wmax` and `level`, as ngspice's `parse_line`
/// reads them from the card text).
#[derive(Debug)]
pub(crate) struct ModelCard<'c> {
    pub card: &'c Card,
    /// Lowercased model name (`nch.3` for a bin).
    pub name: String,
    pub mtype: String,
    keys: [Option<KeyVal>; 5],
    /// For a local card: the names its values read, deduplicated, or
    /// `None` when some value does not parse (no sharing then).
    free: Option<Vec<Box<str>>>,
}

#[derive(Clone, Copy, Debug)]
enum KeyVal {
    Num(f64),
    /// Byte range of a `{…}`/`'…'`/bare value token in the card.
    Text(usize, usize),
}

const KEY_NAMES: [&str; 5] = ["level", "lmin", "lmax", "wmin", "wmax"];

impl<'c> ModelCard<'c> {
    /// Read a `.model` card's name, type and binning keys; `local`
    /// (a card inside a subcircuit) also collects the names its values
    /// read, so instances agreeing on them can share one evaluation.
    pub fn new(card: &'c Card, local: bool) -> Self {
        let mut sc = Scanner::new(card);
        sc.next_tok(); // `.model`
        let name = sc.next_tok().map(|t| t.lc()).unwrap_or_default();
        let mtype = sc.next_tok().map(|t| t.lc()).unwrap_or_default();
        let mut keys = [None; 5];
        let mut free: Option<Vec<Box<str>>> = local.then(Vec::new);
        while let Some(tok) = sc.next_tok() {
            let k = KEY_NAMES.iter().position(|n| tok.eq_ic(n));
            let mut first = true;
            loop {
                let v = if let Some((v, _)) = sc.try_value() {
                    KeyVal::Num(v)
                } else if sc
                    .peek_tok()
                    .is_some_and(|t| t.text.starts_with('{') || t.text.starts_with('\''))
                {
                    match sc.next_tok() {
                        Some(t) => KeyVal::Text(t.start, t.end),
                        None => break,
                    }
                } else if let Some(t) = sc.assign_value() {
                    KeyVal::Text(t.start, t.end)
                } else {
                    break;
                };
                if let (KeyVal::Text(s, e), Some(names)) = (v, free.as_mut()) {
                    match param::value_vars(&card.text[s..e]) {
                        Some(vs) => names.extend(vs),
                        None => free = None,
                    }
                }
                if first && let Some(k) = k {
                    keys[k] = Some(v);
                }
                first = false;
            }
        }
        if let Some(names) = free.as_mut() {
            names.sort_unstable();
            names.dedup();
        }
        ModelCard {
            card,
            name,
            mtype,
            keys,
            free,
        }
    }

    /// Whether every binning key present is a plain number (selection
    /// then costs no evaluation).
    fn static_keys(&self) -> bool {
        self.keys
            .iter()
            .all(|k| !matches!(k, Some(KeyVal::Text(..))))
    }

    /// A binning key evaluated in `env`; `None` when absent or when
    /// it does not evaluate (ngspice's `parse_line` then skips the
    /// bin, silently).
    fn key(&self, k: usize, env: &ParamEnv) -> Option<f64> {
        match self.keys[k]? {
            KeyVal::Num(v) => Some(v),
            KeyVal::Text(s, e) => {
                let card = self.card;
                env.eval_value(&card.text[s..e], &|a, b| card.span_of(s + a, s + b))
                    .ok()
            }
        }
    }

    /// The bin's `(lmin, lmax, wmin, wmax)` if the card is a model
    /// ngspice bins — `nmos`/`pmos` of a BSIM3, BSIM4 or HiSIM level
    /// (`INPgetModBin`; `inpdomod.c` maps levels 8/49, 14/54, 68, 73)
    /// — with all four limits.
    fn bin_limits(&self, env: &ParamEnv) -> Option<[f64; 4]> {
        if !matches!(self.mtype.as_str(), "nmos" | "pmos") {
            return None;
        }
        let level = self.key(0, env).unwrap_or(1.0).round();
        if ![8.0, 49.0, 14.0, 54.0, 68.0, 73.0].contains(&level) {
            return None;
        }
        Some([
            self.key(1, env)?,
            self.key(2, env)?,
            self.key(3, env)?,
            self.key(4, env)?,
        ])
    }
}

/// `nch.12` → `Some("nch")`: a bin name is the base, a dot and at
/// least one digit, nothing else (`model_name_match`).
fn bin_base(name: &str) -> Option<&str> {
    let (base, idx) = name.rsplit_once('.')?;
    (!idx.is_empty() && idx.bytes().all(|b| b.is_ascii_digit())).then_some(base)
}

/// Exact and binned model names of one scope, by index.
#[derive(Debug, Default)]
pub(crate) struct ModelIndex {
    /// Name → last definition with that name.
    exact: HashMap<String, usize>,
    /// Bin base → its bins, in definition order.
    families: HashMap<String, Vec<usize>>,
}

impl ModelIndex {
    fn build<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let mut ix = ModelIndex::default();
        for (i, n) in names.enumerate() {
            ix.exact.insert(n.to_string(), i);
            if let Some(b) = bin_base(n) {
                ix.families.entry(b.to_string()).or_default().push(i);
            }
        }
        ix
    }
}

#[derive(Debug)]
pub(crate) enum BodyItem<'c> {
    Elem(&'c Card),
    X(XCall<'c>),
}

#[derive(Debug)]
pub(crate) struct SubcktDef<'c> {
    pub name: String,
    pub ports: Vec<String>,
    /// `.subckt` line defaults, then body `.param`s, in card order.
    pub formals: Vec<ParamDef<'c>>,
    pub items: Vec<BodyItem<'c>>,
    pub models: Vec<ModelCard<'c>>,
    pub model_index: ModelIndex,
    pub subckts: Vec<SubcktDef<'c>>,
    /// Problems found while reading the body, reported when (and
    /// only if) the subcircuit is instantiated.
    pub deferred: Vec<Diagnostic>,
    /// Whether an `X` line passing `m=` makes this subcircuit
    /// multiply its devices (set by [`mark_multiplied`]).
    pub multiplied: bool,
    pub span: Span,
    pub card: &'c Card,
}

impl SubcktDef<'_> {
    fn declares_m(&self) -> bool {
        self.formals.iter().any(|f| f.name == "m")
    }

    /// Fill `model_index` once the body is complete.
    pub fn finish(&mut self) {
        self.model_index = ModelIndex::build(self.models.iter().map(|m| m.name.as_str()));
    }
}

#[derive(Debug)]
pub(crate) enum TopItem<'c> {
    Elem(Element),
    X(XCall<'c>),
}

/// A top-level `.model`: evaluated once with the global parameters,
/// its diagnostics held back until an element uses it.
#[derive(Debug)]
pub(crate) struct GlobalModel<'c> {
    pub card: ModelCard<'c>,
    pub model: Option<Model>,
    pub diags: Vec<Diagnostic>,
}

#[derive(Debug, Default)]
pub(crate) struct TopScope<'c> {
    pub items: Vec<TopItem<'c>>,
    pub models: Vec<GlobalModel<'c>>,
    pub subckts: Vec<SubcktDef<'c>>,
}

/// Deck-wide context the expansion needs.
pub(crate) struct ExpandCtx<'a> {
    pub model_names: &'a HashSet<String>,
    pub vdmos_models: &'a HashSet<String>,
    /// `.option scale` (binning multiplies L and W by it).
    pub scale: f64,
    /// `.option wnflag` (binning divides W by `nf` when set).
    pub wnflag: bool,
}

/// `x1 n1 n2 subname [params:] name=value …`: nodes and subcircuit
/// name up to the first `name=` (or `params:`), then the actual
/// parameters. Errors are appended to `diags`.
pub(crate) fn parse_xcall<'c>(card: &'c Card, diags: &mut Vec<Diagnostic>) -> Option<XCall<'c>> {
    let mut sc = Scanner::new(card);
    let name_tok = sc.next_tok()?;
    let name = name_tok.lc();
    let mut names = Vec::new();
    let text = card.text.as_bytes();
    let mut params_at = None;
    while let Some(tok) = sc.next_tok() {
        let rest = &text[tok.end..];
        let eq = rest
            .iter()
            .find(|b| !matches!(b, b' ' | b'\t'))
            .is_some_and(|&b| b == b'=');
        if eq {
            params_at = Some(tok.start);
            break;
        }
        if tok.eq_ic("params:") {
            params_at = Some(tok.end);
            break;
        }
        names.push(tok.lc());
    }
    let Some(subname) = names.pop() else {
        diags.push(Diagnostic::error(
            "subcircuit call needs nodes and a subcircuit name",
            sc.here(),
        ));
        return None;
    };
    let mut actuals = Vec::new();
    if let Some(p) = params_at {
        crate::parse::collect_assignments(card, p, &mut actuals, diags);
    }
    Some(XCall {
        name,
        nodes: names,
        subname,
        actuals,
        span: card.span_of(0, card.text.len()),
        card,
    })
}

/// `.subckt name port… [params:] name=value …`: the ports end at the
/// first `name=` or `params:` (`s1 port_eq_space`: blanks around `=`
/// are allowed). The defaults are appended to `formals`.
pub(crate) fn parse_subckt_line<'c>(
    card: &'c Card,
    formals: &mut Vec<ParamDef<'c>>,
    diags: &mut Vec<Diagnostic>,
) -> Option<(String, Vec<String>)> {
    let mut sc = Scanner::new(card);
    sc.next_tok();
    let Some(name) = sc.next_tok() else {
        diags.push(Diagnostic::error(".subckt needs a name", card.span()));
        return None;
    };
    let name = name.lc();
    let text = card.text.as_bytes();
    let mut ports = Vec::new();
    while let Some(tok) = sc.next_tok() {
        let rest = &text[tok.end..];
        let eq = rest
            .iter()
            .find(|b| !matches!(b, b' ' | b'\t'))
            .is_some_and(|&b| b == b'=');
        if eq {
            crate::parse::collect_assignments(card, tok.start, formals, diags);
            break;
        }
        if tok.eq_ic("params:") {
            crate::parse::collect_assignments(card, tok.end, formals, diags);
            break;
        }
        ports.push(tok.lc());
    }
    Some((name, ports))
}

/// Mark the subcircuits whose devices an `X`-line `m=` multiplies.
/// ngspice-47 (`inp_fix_subckt_multiplier`, `frontend/inpcom.c`,
/// Modified BSD) gives every subcircuit called with `m=` anywhere in
/// the deck — unless it declares `m` itself, on its line or in a body
/// `.param` (P13, `s4 m_declared_body`) — a parameter `m=1` and
/// appends `m={m}` to its device and `X` lines, so the subcircuits
/// those `X` lines call are marked too. ngspice does the marking in
/// one pass in deck order, so a nested subcircuit defined *before*
/// its caller is missed and the call fails ("Mismatch: 0 formal but
/// 1 actual params", `s4 m_nested_def_before`); here the marking is
/// closed over the call graph, in any order.
pub(crate) fn mark_multiplied(top: &mut TopScope) {
    let mut names: HashSet<String> = HashSet::new();
    for item in &top.items {
        if let TopItem::X(x) = item
            && x.actuals.iter().any(|a| a.name == "m")
        {
            names.insert(x.subname.clone());
        }
    }
    fn walk_calls(defs: &[SubcktDef], f: &mut dyn FnMut(&XCall)) {
        for d in defs {
            for i in &d.items {
                if let BodyItem::X(x) = i {
                    f(x);
                }
            }
            walk_calls(&d.subckts, f);
        }
    }
    walk_calls(&top.subckts, &mut |x| {
        if x.actuals.iter().any(|a| a.name == "m") {
            names.insert(x.subname.clone());
        }
    });
    fn mark(defs: &mut [SubcktDef], names: &mut HashSet<String>) -> bool {
        let mut changed = false;
        for d in defs {
            if !d.multiplied && names.contains(&d.name) && !d.declares_m() {
                d.multiplied = true;
                changed = true;
                for i in &d.items {
                    if let BodyItem::X(x) = i {
                        names.insert(x.subname.clone());
                    }
                }
            }
            changed |= mark(&mut d.subckts, names);
        }
        changed
    }
    while mark(&mut top.subckts, &mut names) {}
}

/// One subcircuit instance being expanded.
struct Frame<'a, 'c> {
    parent: Option<&'a Frame<'a, 'c>>,
    def: &'c SubcktDef<'c>,
    env: &'a ParamEnv<'a>,
    path: &'a str,
    node_map: &'a HashMap<String, String>,
    /// The instance's multiplier when the subcircuit is multiplied.
    mult: Option<f64>,
    /// Local models already evaluated for this instance:
    /// card index → (emitted name, type, level).
    emitted: RefCell<HashMap<usize, ModelRef>>,
}

#[derive(Clone, Debug)]
struct ModelRef {
    name: String,
    mtype: String,
    level: f64,
}

impl ModelRef {
    fn of(name: String, m: &Model) -> Self {
        let level = m
            .params
            .iter()
            .find_map(|p| match (&p.value, p.name == "level") {
                (ParamValue::Num(v), true) => Some(*v),
                _ => None,
            })
            .unwrap_or(1.0);
        ModelRef {
            name,
            mtype: m.mtype.clone(),
            level,
        }
    }
}

/// Flatten the top scope: expand every `X` call, return the flat
/// element list and the models (the global ones that are used or
/// valid, then every instantiated local one).
pub(crate) fn expand(
    top: &TopScope,
    env: &ParamEnv,
    ctx: &ExpandCtx,
    diags: &mut Vec<Diagnostic>,
) -> (Vec<Element>, Vec<Model>) {
    let mut ex = Expander {
        diags,
        seen: HashSet::new(),
        reported: 0,
        elements: Vec::new(),
        models: Vec::new(),
        work: 0,
        overflowed: false,
        top,
        global_index: ModelIndex::build(top.models.iter().map(|m| m.card.name.as_str())),
        global_used: vec![false; top.models.len()],
        shared: HashMap::new(),
        ctx,
    };
    let empty = HashMap::new();
    for item in &top.items {
        match item {
            TopItem::Elem(el) => {
                if !ex.charge(1, el.span) {
                    break;
                }
                ex.finish_element(el.clone(), None, env, "", &empty);
            }
            TopItem::X(x) => ex.instantiate(x, None, env, "", &empty, 0),
        }
    }
    let Expander {
        global_used,
        models: locals,
        elements,
        ..
    } = ex;
    let mut models = Vec::with_capacity(top.models.len() + locals.len());
    for (gm, used) in top.models.iter().zip(global_used) {
        let has_error = gm.diags.iter().any(|d| d.severity == Severity::Error);
        if used {
            diags.extend(gm.diags.iter().cloned());
        }
        if let Some(m) = &gm.model
            && (used || !has_error)
        {
            models.push(m.clone());
        }
    }
    models.extend(locals);
    (elements, models)
}

struct Expander<'a, 'c> {
    diags: &'a mut Vec<Diagnostic>,
    /// (file, start, end, message) of every diagnostic reported.
    seen: HashSet<(u32, u32, u32, String)>,
    reported: usize,
    elements: Vec<Element>,
    models: Vec<Model>,
    work: usize,
    overflowed: bool,
    top: &'c TopScope<'c>,
    global_index: ModelIndex,
    global_used: Vec<bool>,
    /// Local models evaluated so far, by card and the values of the
    /// names the card reads.
    shared: HashMap<Vec<u64>, ModelRef>,
    ctx: &'a ExpandCtx<'a>,
}

impl<'c> Expander<'_, 'c> {
    /// Spend `units` of the expansion budget; `false` means stop (the
    /// diagnostic is emitted once).
    fn charge(&mut self, units: usize, span: Span) -> bool {
        if self.overflowed {
            return false;
        }
        self.work += units.max(1);
        if self.work > MAX_WORK {
            self.overflowed = true;
            self.diags.push(Diagnostic::error(
                format!("subcircuit expansion exceeds {MAX_WORK} steps; giving up"),
                span,
            ));
            return false;
        }
        true
    }

    /// Report a diagnostic once (the same problem recurs in every
    /// instance of a subcircuit), up to [`MAX_DIAGS`].
    fn report(&mut self, d: Diagnostic) {
        let key = match d.span {
            Some(s) => (s.file.0, s.start, s.end, d.message.clone()),
            None => (u32::MAX, 0, 0, d.message.clone()),
        };
        if !self.seen.insert(key) {
            return;
        }
        self.reported += 1;
        if self.reported > MAX_DIAGS {
            if self.reported == MAX_DIAGS + 1 {
                let msg = format!(
                    "more than {MAX_DIAGS} problems during subcircuit expansion; the rest are \
                     not reported"
                );
                self.diags.push(match d.span {
                    Some(s) => Diagnostic::error(msg, s),
                    None => Diagnostic::global_error(msg),
                });
            }
            return;
        }
        self.diags.push(d);
    }

    fn report_all(&mut self, ds: Vec<Diagnostic>) {
        for d in ds {
            self.report(d);
        }
    }

    /// Rename, resolve the model of, multiply and emit one element
    /// (already parsed in its instance's scope).
    fn finish_element(
        &mut self,
        mut out: Element,
        frame: Option<&Frame<'_, 'c>>,
        env: &ParamEnv,
        path: &str,
        node_map: &HashMap<String, String>,
    ) {
        if !path.is_empty() {
            out.name = rename(&out.name, path);
            for n in &mut out.nodes {
                *n = map_node(n, path, node_map);
            }
            match &mut out.device {
                Device::Mutual { l1, l2, .. } => {
                    *l1 = rename(l1, path);
                    *l2 = rename(l2, path);
                }
                Device::Cccs { vname, .. }
                | Device::Ccvs { vname, .. }
                | Device::SwitchI { vname, .. } => {
                    *vname = rename(vname, path);
                }
                // The expression's `v(n)` nodes and `i(vname)` devices
                // map like the card's own (ngspice's `subckt.c` does
                // the same; the 3f5 frontend translated only the two
                // nodes).
                Device::Behavioral { expr, .. } => {
                    expr.visit_refs_mut(&mut |n| *n = map_node(n, path, node_map), &mut |d| {
                        *d = rename(d, path)
                    });
                }
                _ => {}
            }
        }
        let bytes = out.name.len() + out.nodes.iter().map(|n| n.len() + 1).sum::<usize>();
        if !self.charge(1 + bytes / 64, out.span) {
            return;
        }
        let mut kind = None;
        if let Some(m) = device_model(&out.device).map(str::to_string)
            && let Some(r) = self.resolve_model(&m, &out, frame, env)
        {
            if let Some(slot) = device_model_mut(&mut out.device) {
                *slot = r.name.clone();
            }
            kind = Some(r);
        }
        if let Some(f) = frame
            && let Some(m) = f.mult
            && let Err(what) = apply_multiplier(&mut out, m, kind.as_ref())
        {
            self.report(Diagnostic::error(
                format!(
                    "the multiplier m={m} of subcircuit instance `{}` is not supported yet for \
                     {what} (`{}`)",
                    f.path, out.name
                ),
                out.span,
            ));
        }
        // After binning, which scales L and W itself.
        if self.ctx.scale != 1.0 {
            apply_scale(&mut out, self.ctx.scale, kind.as_ref());
        }
        self.elements.push(out);
    }

    /// Find the model an element names: an exact local model of the
    /// instance or of an instance calling it (innermost first), a
    /// local bin family, then the global models (exact, then bins).
    /// `None` leaves the name as written (the elaborator reports an
    /// unknown model).
    fn resolve_model(
        &mut self,
        name: &str,
        el: &Element,
        frame: Option<&Frame<'_, 'c>>,
        env: &ParamEnv,
    ) -> Option<ModelRef> {
        let mut fr = frame;
        while let Some(f) = fr {
            if let Some(&i) = f.def.model_index.exact.get(name) {
                return self.emit_local(f, i);
            }
            if let Some(fam) = f.def.model_index.families.get(name) {
                // ngspice renames the reference to the instance's
                // family (`subckt.c` `model_name_match`) and bins
                // among the instance's cards only (`s5 b_local_*`).
                let cands: Vec<(usize, &ModelCard)> =
                    fam.iter().map(|&i| (i, &f.def.models[i])).collect();
                return match self.select_bin(el, &cands, f.env, name) {
                    Some(i) => self.emit_local(f, i),
                    None => Some(ModelRef {
                        name: format!("{}.{name}", f.path),
                        mtype: String::new(),
                        level: 0.0,
                    }),
                };
            }
            fr = f.parent;
        }
        let top = self.top;
        if let Some(&i) = self.global_index.exact.get(name) {
            self.global_used[i] = true;
            return top.models[i]
                .model
                .as_ref()
                .map(|m| ModelRef::of(m.name.clone(), m));
        }
        let fam = self.global_index.families.get(name)?.clone();
        let cands: Vec<(usize, &ModelCard)> =
            fam.iter().map(|&i| (i, &top.models[i].card)).collect();
        // Global bin limits read the global parameters, whatever the
        // instance shadows.
        let i = self.select_bin(el, &cands, env.root(), name)?;
        self.global_used[i] = true;
        top.models[i]
            .model
            .as_ref()
            .map(|m| ModelRef::of(m.name.clone(), m))
    }

    /// ngspice's `INPgetModBin` rule (`spicelib/parser/inpgmod.c`,
    /// Modified BSD; probes P09 and `s5`): only `M` instances with
    /// both `l` and `w` bin; `L·scale` and `W·scale` (`W/nf` when
    /// `wnflag`) must lie in `[lmin, lmax]` and `[wmin, wmax]`, both
    /// ends inclusive with an absolute tolerance of 1e-9; the limits
    /// are not scaled; the most recently defined bin is tried first.
    /// No fit is an error.
    fn select_bin(
        &mut self,
        el: &Element,
        cands: &[(usize, &ModelCard)],
        env: &ParamEnv,
        base: &str,
    ) -> Option<usize> {
        let Device::Mosfet { params, .. } = &el.device else {
            return None;
        };
        let num = |n: &str| {
            params
                .iter()
                .rev()
                .find_map(|p| match (&p.value, p.name == n) {
                    (ParamValue::Num(v), true) => Some(*v),
                    _ => None,
                })
        };
        let (Some(l), Some(w)) = (num("l"), num("w")) else {
            self.report(Diagnostic::error(
                format!(
                    "`{base}` is a binned model family ({} bins) and `{}` gives no `l` and `w` to \
                     pick a bin with",
                    cands.len(),
                    el.name
                ),
                el.span,
            ));
            return None;
        };
        let nf = match (num("nf"), num("wnflag")) {
            (Some(nf), Some(f)) if f != 0.0 => nf,
            (Some(nf), None) if self.ctx.wnflag => nf,
            _ => 1.0,
        };
        let l = l * self.ctx.scale;
        let w = w / nf * self.ctx.scale;
        let in_range = |v: f64, lo: f64, hi: f64| {
            (v - lo).abs() < 1e-9 || (v - hi).abs() < 1e-9 || (lo < v && v < hi)
        };
        // A bin whose keys are plain numbers costs a comparison; one
        // whose keys are expressions costs their evaluation.
        if !self.charge(1 + cands.len() / 64, el.span) {
            return None;
        }
        for &(i, card) in cands.iter().rev() {
            if !card.static_keys() && !self.charge(1, el.span) {
                return None;
            }
            if let Some([lmin, lmax, wmin, wmax]) = card.bin_limits(env)
                && in_range(l, lmin, lmax)
                && in_range(w, wmin, wmax)
            {
                return Some(i);
            }
        }
        self.report(Diagnostic::error(
            format!(
                "could not find a valid model for `{}`: no bin of `{base}` ({} cards) covers L = \
                 {l:e}, W = {w:e}",
                el.name,
                cands.len()
            ),
            el.span,
        ));
        None
    }

    /// Evaluate local model card `i` of instance `f` (once per
    /// instance) and emit it as `path.name`.
    fn emit_local(&mut self, f: &Frame<'_, 'c>, i: usize) -> Option<ModelRef> {
        if let Some(r) = f.emitted.borrow().get(&i) {
            return Some(r.clone());
        }
        let mc = &f.def.models[i];
        // Instances that agree on every name the card reads share one
        // evaluated copy (named after the first of them): a 1000-device
        // array of one geometry evaluates a 300-parameter PDK bin once.
        let key = mc.free.as_ref().and_then(|names| {
            let mut k = Vec::with_capacity(names.len() + 1);
            k.push(std::ptr::from_ref(mc.card) as usize as u64);
            for n in names {
                k.push(f.env.get(n)?.to_bits());
            }
            Some(k)
        });
        if let Some(r) = key.as_ref().and_then(|k| self.shared.get(k)) {
            let r = r.clone();
            if !self.charge(1, mc.card.span()) {
                return None;
            }
            f.emitted.borrow_mut().insert(i, r.clone());
            return Some(r);
        }
        if !self.charge(1 + mc.card.text.len() / 256, mc.card.span()) {
            return None;
        }
        let mut ds = Vec::new();
        let model = crate::parse::parse_model(mc.card, &mut ds, f.env);
        self.report_all(ds);
        let mut model = model?;
        model.name = format!("{}.{}", f.path, model.name);
        let r = ModelRef::of(model.name.clone(), &model);
        self.models.push(model);
        f.emitted.borrow_mut().insert(i, r.clone());
        if let Some(k) = key {
            self.shared.insert(k, r.clone());
        }
        Some(r)
    }

    /// Instantiate the subcircuit an `X` call names.
    fn instantiate(
        &mut self,
        call: &'c XCall<'c>,
        caller: Option<&Frame<'_, 'c>>,
        caller_env: &ParamEnv,
        path: &str,
        node_map: &HashMap<String, String>,
        depth: usize,
    ) {
        if !self.charge(1, call.span) {
            return;
        }
        if depth >= MAX_DEPTH {
            self.report(Diagnostic::error(
                format!("subcircuits nested deeper than {MAX_DEPTH} levels"),
                call.span,
            ));
            return;
        }
        // Innermost definition wins: the calling instances' local
        // definitions, then the top level.
        let mut def = None;
        let mut fr = caller;
        while let Some(f) = fr {
            if let Some(d) = f.def.subckts.iter().find(|d| d.name == call.subname) {
                def = Some(d);
                break;
            }
            fr = f.parent;
        }
        let def = def.or_else(|| self.top.subckts.iter().find(|d| d.name == call.subname));
        let Some(def) = def else {
            self.report(Diagnostic::error(
                format!("no such subcircuit `{}`", call.subname),
                call.span,
            ));
            return;
        };
        // A subcircuit that calls itself, directly or not, would only
        // stop at the depth limit after an exponential number of
        // instances (ngspice H6 stops after ~22 levels).
        let mut fr = caller;
        while let Some(f) = fr {
            if std::ptr::eq(f.def, def) {
                self.report(Diagnostic::error(
                    format!("subcircuit `{}` instantiates itself (recursive)", def.name),
                    call.span,
                ));
                return;
            }
            fr = f.parent;
        }
        if def.ports.len() != call.nodes.len() {
            self.report(Diagnostic::error(
                format!(
                    "subcircuit `{}` has {} ports but the call connects {}",
                    def.name,
                    def.ports.len(),
                    call.nodes.len()
                ),
                call.span,
            ));
            return;
        }
        self.report_all(def.deferred.clone());

        // The instance's scope: defaults and body `.param`s, then the
        // actual parameters, which therefore win (P04, P23); `m=1`
        // first for a multiplied subcircuit.
        let mut defs: Vec<ParamDef> =
            Vec::with_capacity(def.formals.len() + call.actuals.len() + 2);
        if def.multiplied {
            defs.push(ParamDef {
                name: "m".to_string(),
                name_span: def.span,
                card: def.card,
                value_start: 0,
                value: "1",
            });
        }
        defs.extend(def.formals.iter().cloned());
        if defs.is_empty() && !call.actuals.is_empty() {
            // ngspice: "Mismatch: 0 formal but 1 actual params"
            // (`s1 undeclared_x_noparams`).
            self.report(Diagnostic::error(
                format!(
                    "subcircuit `{}` has no parameters, but the call passes `{}`",
                    def.name, call.actuals[0].name
                ),
                call.actuals[0].name_span,
            ));
            return;
        }
        let n_formals = defs.len();
        for a in &call.actuals {
            if defs[..n_formals].iter().any(|f| f.name == a.name) {
                defs.push(a.clone());
            } else {
                // ngspice ignores it silently (`s2 count_more`); the
                // name is not visible in the body (`s4 m_undecl_visible`).
                self.report(Diagnostic::warning(
                    format!(
                        "`{}` is not a parameter of subcircuit `{}`; ignored",
                        a.name, def.name
                    ),
                    a.name_span,
                ));
            }
        }
        if caller.is_some_and(|f| f.def.multiplied) {
            // The caller's lines all got `m={m}` appended, the `X`
            // line too: its `m` is the caller's, whatever it wrote
            // (`s4 m_nested_both`). Read in the callee's scope, the
            // self-reference goes to the caller.
            defs.push(ParamDef {
                name: "m".to_string(),
                name_span: call.span,
                card: call.card,
                value_start: 0,
                value: "m",
            });
        }
        if !self.charge(defs.len(), call.span) {
            return;
        }
        let mut ds = Vec::new();
        let env = param::resolve_local(&defs, caller_env, &mut ds);
        self.report_all(ds);

        let mult = if def.multiplied {
            match env.get("m") {
                Some(m) if m.is_finite() && m > 0.0 => Some(m),
                Some(m) => {
                    self.report(Diagnostic::error(
                        format!(
                            "subcircuit multiplier m={m} must be a positive number (ngspice \
                             would simulate it)"
                        ),
                        call.span,
                    ));
                    return;
                }
                None => return, // the error is already reported
            }
        } else {
            None
        };

        let inst_path = if path.is_empty() {
            call.name.clone()
        } else {
            format!("{path}.{}", call.name)
        };
        // Ports bind to the caller's (already mapped) nodes.
        let mut inner_nodes = HashMap::new();
        for (port, actual) in def.ports.iter().zip(&call.nodes) {
            inner_nodes.insert(port.clone(), map_node(actual, path, node_map));
        }
        let frame = Frame {
            parent: caller,
            def,
            env: &env,
            path: &inst_path,
            node_map: &inner_nodes,
            mult,
            emitted: RefCell::new(HashMap::new()),
        };
        for item in &def.items {
            if self.overflowed {
                return;
            }
            match item {
                BodyItem::Elem(card) => {
                    if !self.charge(1 + card.text.len() / 256, card.span()) {
                        return;
                    }
                    let mut ds = Vec::new();
                    let el = parse_element(
                        card,
                        &mut ds,
                        self.ctx.model_names,
                        self.ctx.vdmos_models,
                        frame.env,
                    );
                    self.report_all(ds);
                    if let Some(el) = el {
                        self.finish_element(
                            el,
                            Some(&frame),
                            frame.env,
                            frame.path,
                            frame.node_map,
                        );
                    }
                }
                BodyItem::X(x) => self.instantiate(
                    x,
                    Some(&frame),
                    frame.env,
                    frame.path,
                    frame.node_map,
                    depth + 1,
                ),
            }
        }
    }
}

/// Apply an instance multiplier to one device of a multiplied
/// subcircuit, the way ngspice's appended `m={m}` acts on it
/// (`s4`): the device's own `m` is replaced, not multiplied. Devices
/// with a native `m` (BSIM3/BSIM4/VDMOS MOSFETs, VBIC) get the
/// parameter; the others get the equivalent existing parameter —
/// `R/m`, `C·m`, `L/m`, `area·m` (D, J, Z, Gummel–Poon Q), gain·m (G,
/// F), value·m (DC/AC current source, current-type B source).
/// `Err` names what cannot express a multiplier other than 1.
/// ngspice skips the lines of `V E H A K N O P S T U W Y` and
/// voltage-type `B`.
fn apply_multiplier(el: &mut Element, m: f64, model: Option<&ModelRef>) -> Result<(), String> {
    fn drop_m(params: &mut Vec<Param>) {
        params.retain(|p| p.name != "m");
    }
    fn set_m(params: &mut Vec<Param>, m: f64, span: Span) {
        drop_m(params);
        params.push(Param {
            name: "m".to_string(),
            value: ParamValue::Num(m),
            span,
        });
    }
    fn scale_kw(params: &mut [Param], kw: &str, f: impl Fn(f64) -> f64) -> bool {
        let mut hit = false;
        for p in params.iter_mut().filter(|p| p.name == kw) {
            if let ParamValue::Num(v) = &mut p.value {
                *v = f(*v);
                hit = true;
            }
        }
        hit
    }
    let span = el.span;
    let one = m == 1.0;
    match &mut el.device {
        Device::Resistor { value, params, .. } => {
            drop_m(params);
            if one {
            } else if let Some(v) = value {
                *v /= m;
                scale_kw(params, "resistance", |r| r / m);
            } else if !scale_kw(params, "resistance", |r| r / m) {
                return Err("a resistor given by its model geometry".into());
            }
        }
        Device::Capacitor { value, params, .. } => {
            drop_m(params);
            if one {
            } else if let Some(v) = value {
                *v *= m;
                scale_kw(params, "capacitance", |c| c * m);
            } else if !scale_kw(params, "capacitance", |c| c * m) {
                return Err("a capacitor given by its model geometry".into());
            }
        }
        Device::Inductor { value, params } => {
            drop_m(params);
            if one {
            } else if let Some(v) = value {
                *v /= m;
            } else {
                return Err("an inductor without a value".into());
            }
        }
        Device::Diode { area, params, .. }
        | Device::Jfet { area, params, .. }
        | Device::Mesfet { area, params, .. } => {
            drop_m(params);
            if !one {
                *area = Some(area.unwrap_or(1.0) * m);
            }
        }
        Device::Bjt { area, params, .. } => {
            let level = model.map(|r| (r.level + 0.5).floor()).unwrap_or(1.0);
            if level == 4.0 || level == 9.0 {
                set_m(params, m, span);
            } else {
                drop_m(params);
                if !one {
                    *area = Some(area.unwrap_or(1.0) * m);
                }
            }
        }
        Device::Mosfet { params, .. } => {
            let native = model.is_some_and(|r| {
                r.mtype.starts_with("vdmos") || [8.0, 49.0, 14.0, 54.0].contains(&r.level.round())
            });
            if native {
                set_m(params, m, span);
            } else {
                drop_m(params);
                if !one {
                    return Err(match model {
                        Some(r) if !r.mtype.is_empty() => format!("MOS level {}", r.level),
                        _ => "a MOSFET whose model is unknown".into(),
                    });
                }
            }
        }
        Device::Vccs { gain } | Device::Cccs { gain, .. } => {
            if let Some(g) = gain {
                *g *= m;
            }
        }
        Device::Source {
            kind: SourceKind::Current,
            dc,
            ac,
            tran,
            distof1,
            distof2,
            ..
        } => {
            if !one {
                if tran.is_some() || distof1.is_some() || distof2.is_some() {
                    return Err("a current source with a transient or distortion spec".into());
                }
                if let Some(v) = dc {
                    *v *= m;
                }
                if let Some(a) = ac {
                    a.mag *= m;
                }
            }
        }
        Device::Behavioral {
            kind: SourceKind::Current,
            expr,
        } if !one => {
            let e = std::mem::replace(expr, Expr::Num(0.0));
            *expr = Expr::Bin(ExprOp::Times, Box::new(Expr::Num(m)), Box::new(e));
        }
        _ => {}
    }
    Ok(())
}

/// Fold `.option scale` into the instance geometry, where ngspice-47
/// applies it (`*par.c`, Modified BSD; probe `s6`): the `l`, `w` of
/// MOSFETs of levels 1, 2, 3, BSIM3 (8/49) and BSIM4 (14/54) — and
/// HiSIM (68/73), which Dune lacks — times `scale`, their `ad`, `as`
/// times `scale²` and `pd`, `ps` times `scale`; the `l`, `w` of
/// resistors and capacitors times `scale`. MOS6, BSIM1/2, VDMOS,
/// diode, BJT and JFET areas, values and every model parameter are
/// not scaled (`sc_mos6`, `sc_bsim1`, `sc_d_area`, `sc_model_ld`).
/// The netlist therefore carries scaled geometry (D-0002 superset:
/// `scale` stays in the options, for information).
fn apply_scale(el: &mut Element, s: f64, model: Option<&ModelRef>) {
    fn times(params: &mut [Param], names: &[&str], f: f64) {
        for p in params
            .iter_mut()
            .filter(|p| names.contains(&p.name.as_str()))
        {
            if let ParamValue::Num(v) = &mut p.value {
                *v *= f;
            }
        }
    }
    match &mut el.device {
        Device::Mosfet { params, .. } => {
            let scaled = model.is_some_and(|r| {
                matches!(r.mtype.as_str(), "nmos" | "pmos")
                    && [1.0, 2.0, 3.0, 8.0, 49.0, 14.0, 54.0, 68.0, 73.0].contains(&r.level.round())
            });
            if scaled {
                times(params, &["l", "w", "pd", "ps"], s);
                times(params, &["ad", "as"], s * s);
            }
        }
        Device::Resistor { params, .. } | Device::Capacitor { params, .. } => {
            times(params, &["l", "w"], s);
        }
        _ => {}
    }
}

/// `r1` in instance path `x1` → `r.x1.r1` (ngspice convention).
fn rename(name: &str, path: &str) -> String {
    if path.is_empty() {
        return name.to_string();
    }
    let letter = name.chars().next().unwrap_or('?');
    format!("{letter}.{path}.{name}")
}

fn map_node(node: &str, path: &str, node_map: &HashMap<String, String>) -> String {
    if node == "0" {
        return "0".to_string();
    }
    if let Some(mapped) = node_map.get(node) {
        return mapped.clone();
    }
    if path.is_empty() {
        node.to_string()
    } else {
        format!("{path}.{node}")
    }
}

fn device_model(device: &Device) -> Option<&str> {
    match device {
        Device::Resistor { model, .. } | Device::Capacitor { model, .. } => model.as_deref(),
        Device::Diode { model, .. }
        | Device::Bjt { model, .. }
        | Device::Jfet { model, .. }
        | Device::Mesfet { model, .. }
        | Device::Mosfet { model, .. }
        | Device::SwitchV { model, .. }
        | Device::SwitchI { model, .. }
        | Device::TlineLossy { model }
        | Device::Urc { model, .. } => Some(model),
        _ => None,
    }
}

fn device_model_mut(device: &mut Device) -> Option<&mut String> {
    match device {
        Device::Resistor { model, .. } | Device::Capacitor { model, .. } => model.as_mut(),
        Device::Diode { model, .. }
        | Device::Bjt { model, .. }
        | Device::Jfet { model, .. }
        | Device::Mesfet { model, .. }
        | Device::Mosfet { model, .. }
        | Device::SwitchV { model, .. }
        | Device::SwitchI { model, .. }
        | Device::TlineLossy { model }
        | Device::Txl { model, .. }
        | Device::Cpl { model, .. }
        | Device::Urc { model, .. } => Some(model),
        _ => None,
    }
}
