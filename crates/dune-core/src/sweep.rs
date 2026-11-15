//! Parametric sweeps and Monte Carlo over a parsed deck (issue #10,
//! D-0028 track 1).
//!
//! SPICE3 has no `.step`: varying a value means re-running the deck
//! by hand. [`sweep`] does exactly that, for a set of *concrete*
//! assignments the caller supplies — one row of values per run. The
//! engine holds no RNG and builds no grids: a grid is the caller's
//! Cartesian product, a Monte Carlo is the caller's random draws, so
//! distributions stay idiomatic to each host language and every run
//! is reproducible from the caller's seed.
//!
//! Each run is a fresh, independent simulation: the assignments are
//! written into a copy of the netlist IR, the circuit is elaborated
//! again with [`Circuit::from_netlist`] and every analysis card runs
//! through [`Circuit::run`] — the same path a single `dune -b` run
//! takes, so a sweep reproduces N individual runs bit for bit. No
//! state (operating point, factorization) carries between runs; that
//! is issue #12's job (re-solve), which can reuse [`SweepParam`] to
//! address what it alters.
//!
//! Parameter names follow the ngspice `@device[param]` syntax, plus
//! two shorthands (see [`SweepParam::parse`]).
//!
//! A deck's own random functions (`agauss`, `gauss`, `aunif`, `unif`,
//! `limit`, D-0075) are swept through [`SweepParam::Seed`]: each run
//! is the deck drawn with that run's seed ([`Netlist::with_seed`]),
//! exactly what `.options seed=N` written in it gives. The engine
//! still holds no RNG: the seeds are the caller's list.

use dune_netlist::Severity;
use dune_netlist::ir::{AcSpec, Device, Netlist, Param, ParamValue, TranFnKind};

use crate::circuit::Circuit;
use crate::consts::CTOK;
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// One swept quantity.
///
/// Built with [`SweepParam::parse`] from the forms below (case
/// insensitive, like the deck); names are the elaborated ones, so an
/// element inside a subcircuit is addressed by its expanded name
/// (`r.x1.r1`, as in the rawfile).
///
/// | text | quantity |
/// |---|---|
/// | `temp` | circuit temperature `TEMP`, in °C (as `.options temp=`) |
/// | `r1`, `c1`, `l1` | the element value (Ω, F, H) |
/// | `vin`, `iin` | the source's DC level |
/// | `@r1[resistance]`, `@r1[r]` | resistor value |
/// | `@c1[capacitance]`, `@c1[c]` | capacitor value |
/// | `@l1[inductance]`, `@l1[l]` | inductor value |
/// | `@vin[dc]`, `@vin[acmag]`, `@vin[acphase]` | V/I source DC level, AC magnitude, AC phase (degrees) |
/// | `@m1[w]`, `@m1[l]` | MOSFET drawn width and length (m) |
/// | `seed` | the seed of the deck's random functions and transient noise (as `.options seed=`) |
///
/// Model parameters are not sweepable yet (v2 of issue #10).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SweepParam {
    /// Circuit temperature (`TEMP`), in °C.
    Temp,
    /// The seed of the run (D-0075): the deck is drawn again with it
    /// ([`Netlist::with_seed`]) and the engine's transient-noise
    /// sources follow it (`Options::seed`) — what `.options seed=N`
    /// written in the deck does. Values are whole numbers from 1 to
    /// [`SEED_MAX`].
    Seed,
    /// A parameter of one element: `param` is the canonical ngspice
    /// name (`resistance`, `capacitance`, `inductance`, `dc`,
    /// `acmag`, `acphase`, `w`, `l`); `None` is the element's primary
    /// value (R/C/L value, V/I DC level).
    Instance { name: String, param: Option<String> },
}

impl SweepParam {
    /// Parse `temp`, `seed`, `name` or `@name[param]` (see the type's
    /// table; an element named `seed` is addressed as `@seed[...]`).
    /// Whether the element exists and takes the parameter is checked
    /// against a netlist by [`sweep`].
    pub fn parse(text: &str) -> Result<Self, Error> {
        let t = text.trim().to_ascii_lowercase();
        let bad = || {
            Error::Build(format!(
                "sweep parameter `{text}`: expected `temp`, `seed`, an element name or \
                 `@name[param]`"
            ))
        };
        if t == "temp" {
            return Ok(SweepParam::Temp);
        }
        if t == "seed" {
            return Ok(SweepParam::Seed);
        }
        if let Some(rest) = t.strip_prefix('@') {
            let (name, param) = rest
                .strip_suffix(']')
                .and_then(|r| r.split_once('['))
                .ok_or_else(bad)?;
            let (name, param) = (name.trim(), param.trim());
            if name.is_empty() || param.is_empty() || !is_word(name) || !is_word(param) {
                return Err(bad());
            }
            let param = match param {
                "r" => "resistance",
                "c" => "capacitance",
                other => other,
            };
            return Ok(SweepParam::Instance {
                name: name.to_string(),
                param: Some(param.to_string()),
            });
        }
        if t.is_empty() || !is_word(&t) {
            return Err(bad());
        }
        Ok(SweepParam::Instance {
            name: t,
            param: None,
        })
    }

    /// The canonical text form (`temp`, `seed`, `r1`, `@m1[w]`), which
    /// [`parse`](Self::parse) maps back to `self`.
    pub fn key(&self) -> String {
        match self {
            SweepParam::Temp => "temp".into(),
            SweepParam::Seed => "seed".into(),
            SweepParam::Instance { name, param: None } => name.clone(),
            SweepParam::Instance {
                name,
                param: Some(p),
            } => format!("@{name}[{p}]"),
        }
    }
}

impl std::fmt::Display for SweepParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key())
    }
}

impl std::str::FromStr for SweepParam {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        SweepParam::parse(s)
    }
}

/// No brackets, `@`, `=` or blanks inside a name or a parameter.
fn is_word(s: &str) -> bool {
    !s.chars()
        .any(|c| c.is_whitespace() || matches!(c, '[' | ']' | '@' | '=' | '(' | ')' | ','))
}

/// What one run produced: the plots of every analysis card, in deck
/// order (as [`Circuit::run`] gives them), and the notes of the
/// elaboration and of each analysis.
#[derive(Debug)]
pub struct SweepOutput {
    pub solutions: Vec<Solution>,
    pub notes: Vec<String>,
}

/// One run of a sweep: the values it assigned (in the order of the
/// sweep's parameters) and its outcome. A failing run (no
/// convergence, say — routine in a Monte Carlo tail) does not stop
/// the others.
#[derive(Debug)]
pub struct SweepRun {
    pub values: Vec<f64>,
    pub outcome: Result<SweepOutput, Error>,
}

/// The largest seed a sweep takes: ngspice-47 reads a seed as an
/// unsigned 32-bit integer (`.options seed=4294967303` draws as 7),
/// so a larger one would only alias a smaller one.
pub const SEED_MAX: u64 = 0xFFFF_FFFF;

/// How a resolved parameter writes into the IR.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Temp,
    /// The run's seed: the netlist is drawn again with it.
    Seed,
    /// R/C/L `value`.
    Value(usize),
    SrcDc(usize),
    SrcAcMag(usize),
    SrcAcPhase(usize),
    /// MOSFET keyword parameter (`w` or `l`).
    MosGeom(usize, &'static str),
}

/// Resolve every parameter against the netlist, refusing unknown
/// elements, parameters the element does not take and duplicates.
fn resolve(nl: &Netlist, params: &[SweepParam]) -> Result<Vec<Slot>, Error> {
    let mut slots = Vec::with_capacity(params.len());
    for p in params {
        let (name, param) = match p {
            SweepParam::Instance { name, param } => (name, param),
            SweepParam::Temp | SweepParam::Seed => {
                let slot = if *p == SweepParam::Temp {
                    Slot::Temp
                } else {
                    Slot::Seed
                };
                if slots.contains(&slot) {
                    return Err(Error::Build(format!("sweep parameter `{p}` given twice")));
                }
                slots.push(slot);
                continue;
            }
        };
        let Some(idx) = nl.elements.iter().position(|e| &e.name == name) else {
            return Err(Error::Build(format!(
                "sweep parameter `{p}`: no element `{name}` in the circuit"
            )));
        };
        let el = &nl.elements[idx];
        let param = param.as_deref();
        let slot = match (&el.device, param) {
            (Device::Resistor { .. }, None | Some("resistance"))
            | (Device::Capacitor { .. }, None | Some("capacitance"))
            | (Device::Inductor { .. }, None | Some("inductance")) => Slot::Value(idx),
            // `@l1[l]`: the inductor's own letter, as ngspice allows.
            (Device::Inductor { .. }, Some("l")) => Slot::Value(idx),
            (Device::Source { .. }, None | Some("dc")) => Slot::SrcDc(idx),
            (Device::Source { .. }, Some("acmag")) => Slot::SrcAcMag(idx),
            (Device::Source { .. }, Some("acphase")) => Slot::SrcAcPhase(idx),
            (Device::Mosfet { .. }, Some("w")) => Slot::MosGeom(idx, "w"),
            (Device::Mosfet { .. }, Some("l")) => Slot::MosGeom(idx, "l"),
            (_, None) => {
                return Err(Error::Build(format!(
                    "sweep parameter `{p}`: `{name}` has no primary value to sweep \
                     (R, C, L values and V/I DC levels do); name a parameter as `@{name}[...]`"
                )));
            }
            (_, Some(other)) => {
                return Err(Error::Build(format!(
                    "sweep parameter `{p}`: `{name}` has no sweepable parameter `{other}` \
                     (sweepable: R/C/L value, V/I dc/acmag/acphase, MOSFET w/l, temp, seed)"
                )));
            }
        };
        if let Some(j) = slots.iter().position(|s| *s == slot) {
            return Err(Error::Build(format!(
                "sweep parameter `{p}` given twice (also as `{}`)",
                params[j]
            )));
        }
        slots.push(slot);
    }
    Ok(slots)
}

/// Write one run's values into a copy of the netlist and options.
fn assign(nl: &mut Netlist, opts: &mut Options, slots: &[Slot], values: &[f64]) {
    for (slot, &v) in slots.iter().zip(values) {
        match *slot {
            // °C, like the `.options temp=` card (`Options::apply`).
            Slot::Temp => opts.temp = v + CTOK,
            // Drawn into the netlist before (`run_one`).
            Slot::Seed => {}
            Slot::Value(i) => match &mut nl.elements[i].device {
                Device::Resistor { value, .. }
                | Device::Capacitor { value, .. }
                | Device::Inductor { value, .. } => *value = Some(v),
                _ => unreachable!("resolved as an R/C/L value"),
            },
            Slot::SrcDc(i) => {
                if let Device::Source { dc, .. } = &mut nl.elements[i].device {
                    *dc = Some(v);
                }
            }
            Slot::SrcAcMag(i) | Slot::SrcAcPhase(i) => {
                if let Device::Source { ac, .. } = &mut nl.elements[i].device {
                    // A source without an AC spec gains one with the
                    // other half zero (a zero-magnitude source excites
                    // nothing, as no spec at all).
                    let spec = ac.get_or_insert(AcSpec {
                        mag: 0.0,
                        phase: 0.0,
                    });
                    if matches!(slot, Slot::SrcAcMag(_)) {
                        spec.mag = v;
                    } else {
                        spec.phase = v;
                    }
                }
            }
            Slot::MosGeom(i, key) => {
                let span = nl.elements[i].span;
                if let Device::Mosfet { params, .. } = &mut nl.elements[i].device {
                    // The elaborator takes the last `w=`/`l=`: rewrite
                    // every occurrence so no stale one remains.
                    let mut found = false;
                    for p in params.iter_mut().filter(|p| p.name == key) {
                        p.value = ParamValue::Num(v);
                        found = true;
                    }
                    if !found {
                        params.push(Param {
                            name: key.to_string(),
                            value: ParamValue::Num(v),
                            span,
                        });
                    }
                }
            }
        }
    }
}

/// One run of a netlist that reads `temper` (D-0075): elaborate it
/// re-expanded at the run's temperature, then set the swept values
/// with [`Circuit::alter`] — recorded, so a `.dc temp` card of the
/// deck, which re-expands it again at every point, keeps them.
fn run_temper(
    nl: &Netlist,
    opts: &Options,
    slots: &[Slot],
    values: &[f64],
) -> Result<SweepOutput, Error> {
    let mut opts = opts.clone();
    for (slot, &v) in slots.iter().zip(values) {
        if *slot == Slot::Temp {
            opts.temp = v + CTOK;
        }
    }
    let (circuit, mut notes) = Circuit::from_netlist(nl)?;
    let mut circuit = match circuit.at_temp(&opts)? {
        Some(c) => c,
        None => circuit,
    };
    for (slot, &v) in slots.iter().zip(values) {
        let (i, param) = match *slot {
            Slot::Temp | Slot::Seed => continue,
            Slot::Value(i) => (i, ""),
            Slot::SrcDc(i) => (i, "dc"),
            Slot::SrcAcMag(i) => (i, "acmag"),
            Slot::SrcAcPhase(i) => (i, "acphase"),
            Slot::MosGeom(i, key) => (i, key),
        };
        circuit.alter(&nl.elements[i].name, param, v)?;
    }
    let mut solutions = Vec::new();
    for analysis in &nl.analyses {
        let (sols, n) = circuit.run(analysis, &opts)?;
        notes.extend(n);
        solutions.extend(sols);
    }
    Ok(SweepOutput { solutions, notes })
}

/// One run of a netlist without `temper`: assign the values into a
/// copy of the IR, elaborate, run every analysis card.
fn run_plain(
    nl: &Netlist,
    opts: &Options,
    slots: &[Slot],
    values: &[f64],
) -> Result<SweepOutput, Error> {
    let mut nl = nl.clone();
    let mut opts = opts.clone();
    assign(&mut nl, &mut opts, slots, values);
    let (circuit, mut notes) = Circuit::from_netlist(&nl)?;
    let mut solutions = Vec::new();
    for analysis in &nl.analyses {
        let (sols, n) = circuit.run(analysis, &opts)?;
        notes.extend(n);
        solutions.extend(sols);
    }
    Ok(SweepOutput { solutions, notes })
}

/// The netlist drawn with `seed` ([`Netlist::with_seed`]); an error of
/// the new evaluation fails the run. Its warnings are those the
/// original parse already gave (the draws change values, not cards),
/// so they are not repeated per run.
fn reseed(nl: &Netlist, seed: u64) -> Result<Netlist, Error> {
    let (seeded, diags) = nl.with_seed(seed);
    let errors: Vec<&str> = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.as_str())
        .collect();
    if !errors.is_empty() {
        return Err(Error::Build(format!(
            "drawing the random functions with seed {seed}: {}",
            errors.join("; ")
        )));
    }
    Ok(seeded)
}

/// One run: draw the netlist with the run's seed (when `seed` is
/// swept), then assign, elaborate and run every analysis card.
fn run_one(nl: &Netlist, opts: &Options, plan: &Plan<'_>, values: &[f64]) -> SweepRun {
    let outcome = (|| {
        let seed = plan
            .slots
            .iter()
            .zip(values)
            .find(|(s, _)| **s == Slot::Seed)
            .map(|(_, &v)| v as u64);
        let mut opts = opts.clone();
        let seeded;
        let reslots;
        let (nl, slots) = match seed {
            None => (nl, plan.slots.as_slice()),
            Some(s) => {
                // `.options seed=s` in the deck: the draws and the
                // engine's transient noise.
                opts.seed = s;
                seeded = reseed(nl, s)?;
                // Resolved again on the drawn netlist (its elements are
                // the original's unless a `.if` reads a draw).
                reslots = resolve(&seeded, plan.params)?;
                (&seeded, reslots.as_slice())
            }
        };
        let mut out = if nl.uses_temper() {
            run_temper(nl, &opts, slots, values)
        } else {
            run_plain(nl, &opts, slots, values)
        }?;
        if let Some(note) = &plan.seed_note {
            out.notes.push(note.clone());
        }
        Ok(out)
    })();
    SweepRun {
        values: values.to_vec(),
        outcome,
    }
}

/// A checked sweep: the parameters, where each one writes, and the
/// note every run carries when `seed` is swept over a deck without
/// random functions.
struct Plan<'a> {
    params: &'a [SweepParam],
    slots: Vec<Slot>,
    seed_note: Option<String>,
}

/// Whether a run with `opts` draws transient noise from the seed: a
/// `TRNOISE`/`TRRANDOM` source (unless `notrnoise`) or the transient
/// device noise (`devnoise`).
fn draws_noise(nl: &Netlist, opts: &Options) -> bool {
    let sources = !opts.notrnoise
        && nl.elements.iter().any(|e| {
            matches!(
                &e.device,
                Device::Source { tran: Some(f), .. }
                    if matches!(f.kind, TranFnKind::TrNoise | TranFnKind::TrRandom)
            )
        });
    sources || opts.dev_noise
}

/// Check the shape and values of the runs and resolve the
/// parameters.
fn prepare<'a>(
    nl: &Netlist,
    opts: &Options,
    params: &'a [SweepParam],
    runs: &[Vec<f64>],
) -> Result<Plan<'a>, Error> {
    let slots = resolve(nl, params)?;
    for (r, row) in runs.iter().enumerate() {
        if row.len() != params.len() {
            return Err(Error::Build(format!(
                "sweep run {r}: {} values for {} parameters",
                row.len(),
                params.len()
            )));
        }
        if let Some((p, v)) = params.iter().zip(row).find(|(_, v)| !v.is_finite()) {
            return Err(Error::Build(format!(
                "sweep run {r}: `{p}` = {v} is not finite"
            )));
        }
        if let Some((_, v)) = slots
            .iter()
            .zip(row)
            .find(|(s, v)| **s == Slot::Seed && !is_seed(**v))
        {
            return Err(Error::Build(format!(
                "sweep run {r}: `seed` = {v} is not a seed (a whole number from 1 to {SEED_MAX})"
            )));
        }
    }
    let seed_note = (slots.contains(&Slot::Seed) && !nl.uses_random()).then(|| {
        if draws_noise(nl, opts) {
            "`seed` is swept but the deck has no random function (agauss, gauss, aunif, unif, \
             limit): the seed only changes its transient noise"
                .to_string()
        } else {
            "`seed` is swept but the deck has no random function (agauss, gauss, aunif, unif, \
             limit) and no transient noise: every run is the same circuit"
                .to_string()
        }
    });
    Ok(Plan {
        params,
        slots,
        seed_note,
    })
}

/// A whole number in `1..=SEED_MAX`.
fn is_seed(v: f64) -> bool {
    v.fract() == 0.0 && (1.0..=SEED_MAX as f64).contains(&v)
}

/// Run the deck once per row of `runs`, each row assigning
/// `runs[r][k]` to `params[k]`, sequentially and in order.
///
/// `opts` are the run's options (the deck's `.options` already
/// applied, plus any caller overrides); a swept `temp` replaces
/// `opts.temp` per run. Every run is independent and identical to
/// simulating the deck with those values written into it — bit for
/// bit, the same path as [`Circuit::from_netlist`] + [`Circuit::run`].
///
/// A netlist that reads `temper` ([`Netlist::uses_temper`], D-0075)
/// is re-expanded at each run's temperature and its swept values are
/// set with [`Circuit::alter`] instead of being written into the IR.
///
/// A swept `seed` ([`SweepParam::Seed`]) draws the netlist again per
/// run with [`Netlist::with_seed`] and sets `opts.seed`, winning over
/// the deck's `.options seed` and the caller's; the other parameters
/// then apply to the drawn netlist as usual (a swept `temp`
/// re-expands it with the same draws). Over a deck with no random
/// function ([`Netlist::uses_random`]) the seed only changes the
/// transient noise (`TRNOISE`/`TRRANDOM` sources, `devnoise`); every
/// run then carries a note saying so — or saying that the runs are
/// all the same circuit, when the deck has no transient noise either.
///
/// The outer error is for the sweep itself (an unknown element or
/// parameter, a duplicate, a row of the wrong length, a non-finite
/// value, a seed that is not a whole number in `1..=`[`SEED_MAX`]),
/// checked before any run; each run's own failure is in its
/// [`SweepRun::outcome`].
pub fn sweep(
    netlist: &Netlist,
    opts: &Options,
    params: &[SweepParam],
    runs: &[Vec<f64>],
) -> Result<Vec<SweepRun>, Error> {
    let plan = prepare(netlist, opts, params, runs)?;
    Ok(runs
        .iter()
        .map(|row| run_one(netlist, opts, &plan, row))
        .collect())
}

/// [`sweep`] with the runs spread over `threads` worker threads
/// (`0`: one per core, rayon's default). Results come back in run
/// order and are bit-identical to [`sweep`]'s: each run is its own
/// circuit, with no reduction across runs.
///
/// Native only (feature `parallel`); `wasm32` builds use [`sweep`].
#[cfg(feature = "parallel")]
pub fn sweep_parallel(
    netlist: &Netlist,
    opts: &Options,
    params: &[SweepParam],
    runs: &[Vec<f64>],
    threads: usize,
) -> Result<Vec<SweepRun>, Error> {
    use rayon::prelude::*;
    let plan = prepare(netlist, opts, params, runs)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .map_err(|e| Error::Build(format!("sweep thread pool: {e}")))?;
    Ok(pool.install(|| {
        runs.par_iter()
            .map(|row| run_one(netlist, opts, &plan, row))
            .collect()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_forms() {
        assert_eq!(SweepParam::parse("TEMP").unwrap(), SweepParam::Temp);
        assert_eq!(SweepParam::parse(" Seed ").unwrap(), SweepParam::Seed);
        assert_eq!(SweepParam::Seed.key(), "seed");
        assert_eq!(
            SweepParam::parse("@seed[r]").unwrap().key(),
            "@seed[resistance]"
        );
        assert_eq!(
            SweepParam::parse("R1").unwrap(),
            SweepParam::Instance {
                name: "r1".into(),
                param: None
            }
        );
        let w = SweepParam::parse("@M1[W]").unwrap();
        assert_eq!(w.key(), "@m1[w]");
        assert_eq!(SweepParam::parse(&w.key()).unwrap(), w);
        assert_eq!(
            SweepParam::parse("@r.x1.r1[r]").unwrap().key(),
            "@r.x1.r1[resistance]"
        );
        for bad in ["", "@", "@r1", "@r1[]", "@[w]", "r 1", "@m1[w", "a=b"] {
            assert!(SweepParam::parse(bad).is_err(), "{bad:?}");
        }
    }
}
