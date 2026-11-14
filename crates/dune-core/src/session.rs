//! Interactive re-solve (issue #12, D-0028 track 3): a [`Session`]
//! keeps a parsed and elaborated circuit alive, [`Circuit::alter`]
//! changes instance values in place (the spice3 `alter` command's
//! job, with ngspice's parameter names), and [`Session::run`] runs
//! the deck's analyses again without re-parsing or re-elaborating.
//!
//! **Contract.** With warm starts off (the default) a re-run is the
//! fresh run of the altered deck, bit for bit: an alteration writes
//! exactly the spec field elaboration would have written from the
//! altered card, and every analysis builds its engine from the specs
//! as a fresh run does. What a re-run skips is the parse
//! (lexing, `.subckt` expansion, `.include`) and the elaboration
//! (model cards, instance parameters) — and, in the bindings, the pic
//! extraction. The dense LU backend (D-0005, D-0013) has no symbolic
//! phase to keep: its `analyze` only sizes the matrices, so there is
//! no factorization worth carrying between runs; a sparse backend
//! would hang its symbolic reuse on the same session (see
//! `docs/dune-sessao.md`).
//!
//! **Warm start** ([`Session::set_warm_start`]) is the opt-in that
//! trades the bit-for-bit contract for speed: a standalone operating
//! point (`.op`, and the bias point of `.ac`, `.noise`, `.tf`,
//! `.sens`, `.pz`, `.disto`, `.sp`) first tries a Newton solve
//! continued from the previous operating point, exactly as a `.dc`
//! sweep continues between points. A converged warm solve is a
//! solution to the same tolerances (`RELTOL`/`VNTOL`/`ABSTOL`) as the
//! cold one, not the same bits; when it does not converge the
//! analysis is re-run cold from scratch, so a miss costs time, never
//! the result. Circuits with `.nodeset`s and transient operating
//! points are never warm-started; a circuit with several stable
//! operating points (a latch) may settle in a different one, as a
//! sweep would.

use std::sync::{Arc, Mutex, MutexGuard};

use dune_netlist::ir::{Analysis, Netlist};

use crate::circuit::{Circuit, Spec};
use crate::consts::CTOK;
use crate::error::Error;
use crate::options::Options;
use crate::run::CardOutput;
use crate::solution::Solution;
use crate::srcfn::Wave;

/// A value for [`Circuit::alter`]: a number, or the argument list of a
/// source function (`pulse`, `sin`, `exp`, `pwl`, `sffm`).
#[derive(Clone, Debug, PartialEq)]
pub enum AlterValue {
    Num(f64),
    List(Vec<f64>),
}

impl From<f64> for AlterValue {
    fn from(x: f64) -> Self {
        AlterValue::Num(x)
    }
}

impl From<Vec<f64>> for AlterValue {
    fn from(v: Vec<f64>) -> Self {
        AlterValue::List(v)
    }
}

impl From<&[f64]> for AlterValue {
    fn from(v: &[f64]) -> Self {
        AlterValue::List(v.to_vec())
    }
}

/// Longest source-function argument list [`Circuit::alter`] accepts
/// (a PWL of half a million points).
const MAX_WAVE_ARGS: usize = 1 << 20;

/// The parameters [`Circuit::alter`] knows, per element letter, in
/// the order error messages list them (ngspice `IFparm` names; `""`
/// is the element's principal value).
fn params_of(spec: &Spec) -> &'static [&'static str] {
    match spec {
        Spec::Resistor(_) => &["resistance"],
        Spec::Capacitor(_) => &["capacitance", "ic"],
        Spec::Inductor(_) => &["inductance", "ic"],
        Spec::Vsource(_) | Spec::Isource(_) => &[
            "dc", "acmag", "acphase", "pulse", "sin", "exp", "pwl", "sffm", "trnoise", "trrandom",
        ],
        Spec::Vcvs(_) | Spec::Vccs(_) | Spec::Cccs(_) | Spec::Ccvs(_) => &["gain"],
        Spec::Mos1(_) | Spec::Mos2(_) | Spec::Mos3(_) | Spec::Mos6(_) => &["w", "l", "temp"],
        Spec::Bsim1(_) | Spec::Bsim2(_) => &["w", "l"],
        #[cfg(feature = "bsim")]
        Spec::Bsim3(_) => &["w", "l"],
        #[cfg(feature = "bsim")]
        Spec::Bsim4(_) => &["w", "l"],
        Spec::Diode(_) | Spec::Bjt(_) | Spec::Jfet(_) => &["area", "temp"],
        Spec::Mes(_) => &["area"],
        _ => &[],
    }
}

/// The principal parameter an empty `param` means, and the aliases
/// spice3 accepts for it.
fn canonical<'a>(spec: &Spec, param: &'a str) -> &'a str {
    match (spec, param) {
        (Spec::Resistor(_), "" | "r") => "resistance",
        (Spec::Capacitor(_), "" | "c") => "capacitance",
        (Spec::Inductor(_), "" | "l") => "inductance",
        (Spec::Vsource(_) | Spec::Isource(_), "") => "dc",
        (Spec::Vcvs(_) | Spec::Vccs(_) | Spec::Cccs(_) | Spec::Ccvs(_), "") => "gain",
        (Spec::Diode(_) | Spec::Bjt(_) | Spec::Jfet(_) | Spec::Mes(_), "") => "area",
        _ => param,
    }
}

fn num(name: &str, param: &str, v: &AlterValue) -> Result<f64, Error> {
    match v {
        AlterValue::Num(x) if x.is_finite() => Ok(*x),
        AlterValue::Num(x) => Err(Error::Build(format!(
            "`{name}`: {param} must be finite, got {x}"
        ))),
        AlterValue::List(_) => Err(Error::Build(format!(
            "`{name}`: {param} takes a number, not a list"
        ))),
    }
}

fn positive(name: &str, param: &str, v: &AlterValue) -> Result<f64, Error> {
    let x = num(name, param, v)?;
    if x > 0.0 {
        Ok(x)
    } else {
        Err(Error::Build(format!(
            "`{name}`: {param} must be positive, got {x}"
        )))
    }
}

fn list(name: &str, param: &str, v: &AlterValue) -> Result<Vec<f64>, Error> {
    match v {
        AlterValue::List(xs) if xs.len() > MAX_WAVE_ARGS => Err(Error::Build(format!(
            "`{name}`: {param} has {} arguments; the limit is {MAX_WAVE_ARGS}",
            xs.len()
        ))),
        AlterValue::List(xs) if xs.iter().all(|x| x.is_finite()) => Ok(xs.clone()),
        AlterValue::List(_) => Err(Error::Build(format!(
            "`{name}`: {param} arguments must be finite"
        ))),
        AlterValue::Num(_) => Err(Error::Build(format!(
            "`{name}`: {param} takes the function's argument list"
        ))),
    }
}

/// `(kind, args)` of a source's transient function under its
/// ngspice parameter name.
fn wave_param(w: &Wave) -> (&'static str, &[f64]) {
    match w {
        Wave::Pulse(a) => ("pulse", a),
        Wave::Sin(a) => ("sin", a),
        Wave::Exp(a) => ("exp", a),
        Wave::Pwl(a) => ("pwl", a),
        Wave::Sffm(a) => ("sffm", a),
        Wave::TrNoise(a) => ("trnoise", a),
        Wave::TrRandom(a) => ("trrandom", a),
    }
}

/// The source fields an alteration touches (V and I alike).
struct SrcFields<'a> {
    dc: &'a mut Option<f64>,
    wave: &'a mut Option<Wave>,
    ac: &'a mut Option<(f64, f64)>,
}

impl Circuit {
    /// Change one parameter of element `refdes` in place — the
    /// spice3/ngspice `alter` command. The next analysis sees the
    /// circuit exactly as if the deck had been written with the new
    /// value (same spec field, same unit as the card).
    ///
    /// `refdes` is the element name (case-insensitive; subcircuit
    /// instances by their expanded name, e.g. `r.x1.r1`). `param` is
    /// the ngspice parameter name, `""` for the element's principal
    /// value:
    ///
    /// | element | `param` (`""` →) | value |
    /// |---|---|---|
    /// | `R` | `resistance` (`r`) | ohms, nonzero |
    /// | `C` / `L` | `capacitance` (`c`) / `inductance` (`l`), `ic` | F / H; IC in V / A |
    /// | `V` / `I` | `dc`, `acmag`, `acphase` (degrees) | V or A |
    /// | `V` / `I` | `pulse`, `sin`, `exp`, `pwl`, `sffm`, `trnoise`, `trrandom` | [`AlterValue::List`]: the card's arguments; replaces the transient function |
    /// | `E` `F` `G` `H` | `gain` | as on the card |
    /// | MOS (all levels, BSIM) | `w`, `l` | meters, positive |
    /// | MOS 1/2/3/6, `D`, `Q`, `J` | `temp` | °C, like `TEMP=` on the card |
    /// | `D`, `Q`, `J`, `Z` | `area` | positive |
    ///
    /// Altering `acmag`/`acphase` of a source without an `AC`
    /// specification gives it one (the other half 0), as the card
    /// `AC 0 <phase>` would. Inductor values also move the coupling of
    /// their `K` elements (`MUTfactor` is recomputed at each run).
    /// Topology never changes, so an altered circuit keeps its
    /// unknowns and output vectors. The circuit-wide temperature is
    /// an option: [`Session::set_temp`].
    ///
    /// Errors ([`Error::Build`]): unknown element, a parameter the
    /// element does not have (the message lists the valid ones), a
    /// non-finite value, a zero resistance, a nonpositive `W`/`L`/
    /// `area`, a number where a list is due or vice versa.
    pub fn alter(
        &mut self,
        refdes: &str,
        param: &str,
        value: impl Into<AlterValue>,
    ) -> Result<(), Error> {
        let value = value.into();
        let name = refdes.to_ascii_lowercase();
        let param_lc = param.to_ascii_lowercase();
        let spec = self
            .specs
            .iter_mut()
            .find(|s| s.name() == name)
            .ok_or_else(|| Error::Build(format!("no element `{name}` in the circuit")))?;
        let p = canonical(spec, &param_lc).to_string();
        let p = p.as_str();
        let v = &value;
        let n = name.as_str();
        let bad = |spec: &Spec| {
            let valid = params_of(spec);
            if valid.is_empty() {
                Error::Build(format!("`{n}`: this element has no alterable parameters"))
            } else {
                Error::Build(format!(
                    "`{n}` has no alterable parameter `{p}`; valid: {}",
                    valid.join(", ")
                ))
            }
        };
        match spec {
            Spec::Resistor(r) if p == "resistance" => {
                let x = num(n, p, v)?;
                if x == 0.0 {
                    // spice.txt §3.1.1, as `Circuit::add`.
                    return Err(Error::Build(format!(
                        "resistor `{n}` cannot have zero resistance"
                    )));
                }
                r.value = x;
            }
            Spec::Capacitor(c) if p == "capacitance" => c.value = num(n, p, v)?,
            Spec::Capacitor(c) if p == "ic" => c.ic = Some(num(n, p, v)?),
            Spec::Inductor(l) if p == "inductance" => l.value = num(n, p, v)?,
            Spec::Inductor(l) if p == "ic" => l.ic = Some(num(n, p, v)?),
            Spec::Vsource(_) | Spec::Isource(_) if params_of(spec).contains(&p) => {
                let f = match spec {
                    Spec::Vsource(s) => SrcFields {
                        dc: &mut s.dc,
                        wave: &mut s.wave,
                        ac: &mut s.ac,
                    },
                    Spec::Isource(s) => SrcFields {
                        dc: &mut s.dc,
                        wave: &mut s.wave,
                        ac: &mut s.ac,
                    },
                    _ => unreachable!("matched V/I above"),
                };
                match p {
                    "dc" => *f.dc = Some(num(n, p, v)?),
                    "acmag" => {
                        let x = num(n, p, v)?;
                        *f.ac = Some((x, f.ac.map_or(0.0, |a| a.1)));
                    }
                    "acphase" => {
                        let x = num(n, p, v)?;
                        *f.ac = Some((f.ac.map_or(0.0, |a| a.0), x));
                    }
                    _ => {
                        let args = list(n, p, v)?;
                        *f.wave = Some(match p {
                            "pulse" => Wave::Pulse(args),
                            "sin" => Wave::Sin(args),
                            "exp" => Wave::Exp(args),
                            "pwl" => Wave::Pwl(args),
                            "sffm" => Wave::Sffm(args),
                            "trnoise" => Wave::TrNoise(args),
                            _ => Wave::TrRandom(args),
                        });
                    }
                }
            }
            Spec::Vcvs(s) if p == "gain" => s.gain = num(n, p, v)?,
            Spec::Vccs(s) if p == "gain" => s.gain = num(n, p, v)?,
            Spec::Cccs(s) if p == "gain" => s.gain = num(n, p, v)?,
            Spec::Ccvs(s) if p == "gain" => s.gain = num(n, p, v)?,
            #[cfg(feature = "bsim")]
            Spec::Bsim4(m) if p == "w" || p == "l" => {
                let x = positive(n, p, v)?;
                m.set_param(p, x);
            }
            #[cfg(feature = "bsim")]
            Spec::Bsim3(m) if p == "w" || p == "l" => {
                let x = Some(positive(n, p, v)?);
                if p == "w" {
                    m.w = x;
                } else {
                    m.l = x;
                }
            }
            Spec::Mos1(_)
            | Spec::Mos2(_)
            | Spec::Mos3(_)
            | Spec::Mos6(_)
            | Spec::Bsim1(_)
            | Spec::Bsim2(_)
                if params_of(spec).contains(&p) =>
            {
                let (w, l, temp) = match spec {
                    Spec::Mos1(m) => (&mut m.w, &mut m.l, Some(&mut m.temp)),
                    Spec::Mos2(m) => (&mut m.w, &mut m.l, Some(&mut m.temp)),
                    Spec::Mos3(m) => (&mut m.w, &mut m.l, Some(&mut m.temp)),
                    Spec::Mos6(m) => (&mut m.w, &mut m.l, Some(&mut m.temp)),
                    Spec::Bsim1(m) => (&mut m.w, &mut m.l, None),
                    Spec::Bsim2(m) => (&mut m.w, &mut m.l, None),
                    _ => unreachable!("matched MOS above"),
                };
                match (p, temp) {
                    ("w", _) => *w = Some(positive(n, p, v)?),
                    ("l", _) => *l = Some(positive(n, p, v)?),
                    // `TEMP=` is °C on the card (`+ CTOK` in
                    // elaboration, the same operation).
                    (_, Some(t)) => *t = Some(num(n, p, v)? + CTOK),
                    (_, None) => unreachable!("BSIM lists no temp"),
                }
            }
            Spec::Diode(_) | Spec::Bjt(_) | Spec::Jfet(_) | Spec::Mes(_)
                if params_of(spec).contains(&p) =>
            {
                let (area, temp) = match spec {
                    Spec::Diode(d) => (&mut d.area, Some(&mut d.temp)),
                    Spec::Bjt(q) => (&mut q.area, Some(&mut q.temp)),
                    Spec::Jfet(j) => (&mut j.area, Some(&mut j.temp)),
                    Spec::Mes(z) => (&mut z.area, None),
                    _ => unreachable!("matched D/Q/J/Z above"),
                };
                match (p, temp) {
                    ("area", _) => *area = positive(n, p, v)?,
                    (_, Some(t)) => *t = Some(num(n, p, v)? + CTOK),
                    (_, None) => unreachable!("MES lists no temp"),
                }
            }
            _ => return Err(bad(spec)),
        }
        Ok(())
    }

    /// The current value of an alterable parameter (same names and
    /// units as [`Circuit::alter`]), e.g. to seed a slider. A value
    /// the element never got — a MOS `W` left to `.options defw`, a
    /// source without `AC` — reads as `None`; a source without a
    /// transient function reads `None` for `pulse` and friends, and
    /// only the function the source has reads `Some`.
    pub fn param(&self, refdes: &str, param: &str) -> Result<Option<AlterValue>, Error> {
        let name = refdes.to_ascii_lowercase();
        let param_lc = param.to_ascii_lowercase();
        let spec = self
            .specs
            .iter()
            .find(|s| s.name() == name)
            .ok_or_else(|| Error::Build(format!("no element `{name}` in the circuit")))?;
        let p = canonical(spec, &param_lc);
        let n = |x: f64| Some(AlterValue::Num(x));
        let opt = |x: Option<f64>| x.map(AlterValue::Num);
        let src = |dc: Option<f64>, ac: Option<(f64, f64)>, wave: &Option<Wave>| match p {
            "dc" => Some(opt(dc)),
            "acmag" => Some(opt(ac.map(|a| a.0))),
            "acphase" => Some(opt(ac.map(|a| a.1))),
            "pulse" | "sin" | "exp" | "pwl" | "sffm" => Some(
                wave.as_ref()
                    .map(wave_param)
                    .filter(|(k, _)| *k == p)
                    .map(|(_, a)| AlterValue::List(a.to_vec())),
            ),
            _ => None,
        };
        let got: Option<Option<AlterValue>> = match spec {
            Spec::Resistor(r) if p == "resistance" => Some(n(r.value)),
            Spec::Capacitor(x) if p == "capacitance" => Some(n(x.value)),
            Spec::Capacitor(x) if p == "ic" => Some(opt(x.ic)),
            Spec::Inductor(x) if p == "inductance" => Some(n(x.value)),
            Spec::Inductor(x) if p == "ic" => Some(opt(x.ic)),
            Spec::Vsource(s) => src(s.dc, s.ac, &s.wave),
            Spec::Isource(s) => src(s.dc, s.ac, &s.wave),
            Spec::Vcvs(s) if p == "gain" => Some(n(s.gain)),
            Spec::Vccs(s) if p == "gain" => Some(n(s.gain)),
            Spec::Cccs(s) if p == "gain" => Some(n(s.gain)),
            Spec::Ccvs(s) if p == "gain" => Some(n(s.gain)),
            Spec::Mos1(m) => mos(p, m.w, m.l, Some(m.temp)),
            Spec::Mos2(m) => mos(p, m.w, m.l, Some(m.temp)),
            Spec::Mos3(m) => mos(p, m.w, m.l, Some(m.temp)),
            Spec::Mos6(m) => mos(p, m.w, m.l, Some(m.temp)),
            Spec::Bsim1(m) => mos(p, m.w, m.l, None),
            Spec::Bsim2(m) => mos(p, m.w, m.l, None),
            #[cfg(feature = "bsim")]
            Spec::Bsim3(m) => mos(p, m.w, m.l, None),
            #[cfg(feature = "bsim")]
            Spec::Bsim4(m) => mos(p, m.param("w"), m.param("l"), None),
            Spec::Diode(d) => area_temp(p, d.area, Some(d.temp)),
            Spec::Bjt(q) => area_temp(p, q.area, Some(q.temp)),
            Spec::Jfet(j) => area_temp(p, j.area, Some(j.temp)),
            Spec::Mes(z) => area_temp(p, z.area, None),
            _ => None,
        };
        fn mos(
            p: &str,
            w: Option<f64>,
            l: Option<f64>,
            temp: Option<Option<f64>>,
        ) -> Option<Option<AlterValue>> {
            match (p, temp) {
                ("w", _) => Some(w.map(AlterValue::Num)),
                ("l", _) => Some(l.map(AlterValue::Num)),
                ("temp", Some(t)) => Some(t.map(|k| AlterValue::Num(k - CTOK))),
                _ => None,
            }
        }
        fn area_temp(p: &str, area: f64, temp: Option<Option<f64>>) -> Option<Option<AlterValue>> {
            match (p, temp) {
                ("area", _) => Some(Some(AlterValue::Num(area))),
                ("temp", Some(t)) => Some(t.map(|k| AlterValue::Num(k - CTOK))),
                _ => None,
            }
        }
        got.ok_or_else(|| {
            let valid = params_of(spec);
            Error::Build(format!(
                "`{name}` has no alterable parameter `{p}`; valid: {}",
                valid.join(", ")
            ))
        })
    }

    /// The parameter names [`Circuit::alter`] accepts for `refdes`
    /// (empty for an element with none).
    pub fn alterable(&self, refdes: &str) -> Result<&'static [&'static str], Error> {
        let name = refdes.to_ascii_lowercase();
        self.specs
            .iter()
            .find(|s| s.name() == name)
            .map(params_of)
            .ok_or_else(|| Error::Build(format!("no element `{name}` in the circuit")))
    }
}

/// Warm-start counters of a [`Session`] ([`Session::warm_stats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WarmStats {
    /// Operating points solved from the previous one.
    pub hits: u64,
    /// Warm attempts that did not converge (each re-ran its analysis
    /// cold).
    pub misses: u64,
}

#[derive(Default)]
struct Warm {
    enabled: bool,
    /// Set while a missed analysis re-runs cold.
    suspended: bool,
    /// The last operating point (`CKTrhsOld`, ground included) and
    /// its state bank 0; empty before the first one.
    sol: Vec<f64>,
    states0: Vec<f64>,
    /// The last analysis missed (read and cleared by the session).
    missed: bool,
    stats: WarmStats,
}

/// A circuit's handle on its session's warm-start state. A plain
/// [`Circuit`] has none (the engine's `op` then is `CKTop`, nothing
/// else); a [`Session`] installs one and every engine built from the
/// circuit shares it. Cloning a circuit does not share it.
#[derive(Default)]
pub(crate) struct WarmSlot(Option<Arc<Mutex<Warm>>>);

impl Clone for WarmSlot {
    fn clone(&self) -> Self {
        WarmSlot(None)
    }
}

impl WarmSlot {
    fn new(enabled: bool) -> Self {
        WarmSlot(Some(Arc::new(Mutex::new(Warm {
            enabled,
            ..Warm::default()
        }))))
    }

    fn lock(&self) -> Option<MutexGuard<'_, Warm>> {
        self.0
            .as_ref()
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// The same slot, for an engine built from the circuit.
    pub(crate) fn share(&self) -> WarmSlot {
        WarmSlot(self.0.clone())
    }

    /// The previous operating point, when warm starts are on and it
    /// fits this engine (`n_sol` unknowns incl. ground, `n_states`
    /// state slots).
    pub(crate) fn guess(&self, n_sol: usize, n_states: usize) -> Option<(Vec<f64>, Vec<f64>)> {
        let w = self.lock()?;
        (w.enabled && !w.suspended && w.sol.len() == n_sol && w.states0.len() == n_states)
            .then(|| (w.sol.clone(), w.states0.clone()))
    }

    pub(crate) fn store(&self, sol: &[f64], states0: &[f64]) {
        if let Some(mut w) = self.lock() {
            w.sol.clear();
            w.sol.extend_from_slice(sol);
            w.states0.clear();
            w.states0.extend_from_slice(states0);
        }
    }

    pub(crate) fn hit(&self, sol: &[f64], states0: &[f64]) {
        self.store(sol, states0);
        if let Some(mut w) = self.lock() {
            w.stats.hits += 1;
        }
    }

    pub(crate) fn miss(&self) {
        if let Some(mut w) = self.lock() {
            w.missed = true;
            w.stats.misses += 1;
        }
    }

    fn take_missed(&self) -> bool {
        self.lock()
            .is_some_and(|mut w| std::mem::take(&mut w.missed))
    }

    fn set_suspended(&self, on: bool) {
        if let Some(mut w) = self.lock() {
            w.suspended = on;
        }
    }

    fn set_enabled(&self, on: bool) {
        if let Some(mut w) = self.lock() {
            w.enabled = on;
            if !on {
                w.sol.clear();
                w.states0.clear();
            }
        }
    }

    fn enabled(&self) -> bool {
        self.lock().is_some_and(|w| w.enabled)
    }

    fn stats(&self) -> WarmStats {
        self.lock().map(|w| w.stats).unwrap_or_default()
    }
}

/// A live circuit for interactive re-solves: the elaborated circuit,
/// its options and its analysis cards, kept between runs (issue #12).
///
/// ```
/// use dune_core::Session;
/// let deck = "divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n";
/// let parsed = dune_netlist::parse_str("deck.cir", deck);
/// let (mut s, _notes) = Session::from_netlist(&parsed.netlist).unwrap();
/// let (plots, _) = s.run().unwrap();
/// s.alter("r2", "", 3e3).unwrap();
/// let (again, _) = s.run().unwrap();
/// let v2 = |p: &dune_core::Solution| p.column("v(2)").unwrap()[0];
/// assert_eq!(v2(&plots[0]), 5.0);
/// assert_eq!(v2(&again[0]), 7.5);
/// ```
///
/// See the module notes for the bit-for-bit contract and the
/// warm-start opt-in.
pub struct Session {
    circuit: Circuit,
    options: Options,
    analyses: Vec<Analysis>,
}

impl Clone for Session {
    /// An independent session with the same circuit, options, cards
    /// and warm-start setting, and no warm-start history.
    fn clone(&self) -> Self {
        Session::new(
            self.circuit.clone(),
            self.options.clone(),
            self.analyses.clone(),
        )
        .with_warm_start(self.warm_start())
    }
}

impl Session {
    /// A session over an already built circuit; `analyses` are what
    /// [`Session::run`] runs, in order.
    pub fn new(mut circuit: Circuit, options: Options, analyses: Vec<Analysis>) -> Self {
        circuit.warm = WarmSlot::new(false);
        Session {
            circuit,
            options,
            analyses,
        }
    }

    /// Elaborate a parsed deck into a session: the circuit
    /// ([`Circuit::from_netlist`]), the deck's `.options` (warnings
    /// about them are dropped — apply them yourself with
    /// [`Options::apply`] and use [`Session::new`] to keep them) and
    /// its analysis cards. Returns the elaboration notes.
    pub fn from_netlist(nl: &Netlist) -> Result<(Self, Vec<String>), Error> {
        let (circuit, notes) = Circuit::from_netlist(nl)?;
        let mut options = Options::default();
        options.apply(&nl.options, &mut Vec::new());
        Ok((Session::new(circuit, options, nl.analyses.clone()), notes))
    }

    fn with_warm_start(mut self, on: bool) -> Self {
        self.set_warm_start(on);
        self
    }

    /// [`Circuit::alter`] on the session's circuit.
    pub fn alter(
        &mut self,
        refdes: &str,
        param: &str,
        value: impl Into<AlterValue>,
    ) -> Result<(), Error> {
        self.circuit.alter(refdes, param, value)
    }

    /// [`Circuit::param`] on the session's circuit.
    pub fn param(&self, refdes: &str, param: &str) -> Result<Option<AlterValue>, Error> {
        self.circuit.param(refdes, param)
    }

    /// The circuit temperature, °C (`.options temp=`, the card's
    /// unit). Instance `TEMP=` overrides still win, as in the 3f5.
    pub fn set_temp(&mut self, celsius: f64) -> Result<(), Error> {
        if !celsius.is_finite() {
            return Err(Error::Build(format!(
                "temperature must be finite, got {celsius}"
            )));
        }
        // `Options::apply` does the same `v + CTOK`.
        self.options.temp = celsius + CTOK;
        Ok(())
    }

    /// The circuit temperature, °C.
    pub fn temp(&self) -> f64 {
        self.options.temp - CTOK
    }

    /// Turn the operating-point warm start on or off (off by
    /// default; turning it off forgets the stored point). See the
    /// module notes: on, results agree with a fresh run to the
    /// Newton tolerances instead of bit for bit.
    pub fn set_warm_start(&mut self, on: bool) {
        self.circuit.warm.set_enabled(on);
    }

    pub fn warm_start(&self) -> bool {
        self.circuit.warm.enabled()
    }

    /// Warm-start hits and misses so far.
    pub fn warm_stats(&self) -> WarmStats {
        self.circuit.warm.stats()
    }

    pub fn circuit(&self) -> &Circuit {
        &self.circuit
    }

    pub fn options(&self) -> &Options {
        &self.options
    }

    /// The options, for changes beyond [`Session::set_temp`]. They
    /// apply from the next run on.
    pub fn options_mut(&mut self) -> &mut Options {
        &mut self.options
    }

    /// The analysis cards [`Session::run`] runs.
    pub fn analyses(&self) -> &[Analysis] {
        &self.analyses
    }

    /// Run every analysis card in order, as [`Circuit::run`] does for
    /// each: the plots in rawfile order and the analyses' notes (the
    /// elaboration notes came with [`Session::from_netlist`]). The
    /// first error stops the run.
    pub fn run(&mut self) -> Result<(Vec<Solution>, Vec<String>), Error> {
        let mut plots = Vec::new();
        let mut notes = Vec::new();
        for a in &self.analyses {
            let (p, n) = run_one(&self.circuit, &self.options, a)?.into_plots();
            plots.extend(p);
            notes.extend(n);
        }
        Ok((plots, notes))
    }

    /// Run one analysis card (any card, not only the deck's) on the
    /// session's circuit.
    pub fn run_analysis(
        &mut self,
        analysis: &Analysis,
    ) -> Result<(Vec<Solution>, Vec<String>), Error> {
        run_one(&self.circuit, &self.options, analysis).map(CardOutput::into_plots)
    }

    /// [`Session::run_analysis`] keeping a `.pss` result whole, as
    /// [`Circuit::run_card`] does (same warm start, same cold re-run
    /// after a miss).
    pub fn run_card(&mut self, analysis: &Analysis) -> Result<CardOutput, Error> {
        run_one(&self.circuit, &self.options, analysis)
    }
}

/// [`Circuit::run_card`], re-run cold when its warm start missed.
fn run_one(circuit: &Circuit, options: &Options, analysis: &Analysis) -> Result<CardOutput, Error> {
    let first = circuit.run_card(analysis, options);
    if first.is_ok() || !circuit.warm.take_missed() {
        return first;
    }
    circuit.warm.set_suspended(true);
    let cold = circuit.run_card(analysis, options);
    circuit.warm.set_suspended(false);
    cold
}
