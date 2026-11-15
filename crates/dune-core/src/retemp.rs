//! Re-expansion of a temperature-dependent netlist (D-0075).
//!
//! ngspice-47 re-evaluates every `{…temper…}` of a `.model` card or a
//! device line, and every behavioural R/C/L with its `tc1`/`tc2`, at
//! each temperature it simulates (`inp_evaluate_temper`, called from
//! `cktdojob.c` and at every point of a `.dc temp` sweep,
//! `dctrcurv.c`). The netlist front end evaluates those values when
//! it reads the deck, so a circuit elaborated from a netlist that
//! [uses `temper`](dune_netlist::ir::Netlist::uses_temper) keeps the
//! netlist and the temperature it was evaluated at. Whenever it is
//! built at another temperature ([`Circuit::build`] with a different
//! `opts.temp`: a `.dc temp` point, a [`Session::set_temp`]
//! (crate::Session::set_temp), a swept `temp`, a caller's
//! `Options::temp` override), the deck is re-expanded at that
//! temperature and elaborated again, the caller's alterations
//! ([`Circuit::alter`]) are replayed on the new circuit, and the
//! topology is checked to be the same — so unknowns and output
//! vectors never change between temperatures.
//!
//! A circuit without such a netlist never takes this path: its build
//! is the 3f5 one, bit for bit.

use std::sync::Arc;

use dune_netlist::Severity;
use dune_netlist::ir::Netlist;

use crate::circuit::Circuit;
use crate::consts::CTOK;
use crate::error::Error;
use crate::options::Options;
use crate::session::AlterValue;

/// The netlist a circuit was elaborated from, when it depends on
/// `temper`, and the temperature (kelvin) its values were evaluated
/// at.
pub(crate) struct TemperDeck {
    netlist: Arc<Netlist>,
    kelvin: f64,
    /// How many specs came from the netlist; specs added afterwards
    /// with [`Circuit::add`] are carried over as they are.
    specs: usize,
}

/// Most re-expansions one `.dc temp` sweep may ask for (each one
/// parses the whole deck again, within the parser's own budgets;
/// sky130's library takes ~50 ms).
pub(crate) const MAX_REEXPANSIONS: usize = 10_000;

/// Most alterations remembered for the replay (one per element and
/// parameter: a later alteration of the same pair replaces the
/// earlier one).
const MAX_ALTERS: usize = 1 << 20;

impl Circuit {
    /// Attach the re-expansion handle after elaborating `nl`, when
    /// the netlist depends on `temper`.
    pub(crate) fn attach_temper(&mut self, nl: &Netlist) {
        if !nl.uses_temper() {
            return;
        }
        // The temperature the deck was evaluated at, as the caller's
        // options will see it (`Options::apply` on the deck's
        // `.options`/`.temp`, the same `°C + CTOK`).
        let mut o = Options::default();
        o.apply(&nl.options, &mut Vec::new());
        self.temper = Some(Arc::new(TemperDeck {
            netlist: Arc::new(nl.clone()),
            kelvin: o.temp,
            specs: self.specs.len(),
        }));
    }

    /// Remember an alteration for the replay on a re-expanded circuit
    /// (only a circuit that can be re-expanded keeps them).
    pub(crate) fn record_alter(&mut self, name: &str, param: &str, value: &AlterValue) {
        if self.temper.is_none() {
            return;
        }
        if let Some(a) = self
            .alters
            .iter_mut()
            .find(|(n, p, _)| n == name && p == param)
        {
            a.2 = value.clone();
        } else if self.alters.len() < MAX_ALTERS {
            self.alters
                .push((name.to_string(), param.to_string(), value.clone()));
        }
    }

    /// The circuit to build for `opts`: `None` when it is `self` (no
    /// `temper`, or already evaluated at `opts.temp`).
    pub(crate) fn at_temp(&self, opts: &Options) -> Result<Option<Circuit>, Error> {
        match &self.temper {
            Some(deck) if opts.temp != deck.kelvin => self.reexpanded(deck, opts.temp).map(Some),
            _ => Ok(None),
        }
    }

    /// Re-expand the netlist at `kelvin` and elaborate it again,
    /// carrying over what the caller changed since.
    fn reexpanded(&self, deck: &TemperDeck, kelvin: f64) -> Result<Circuit, Error> {
        let celsius = kelvin - CTOK;
        let (nl, diags) = deck.netlist.at_temperature(celsius);
        let errors: Vec<&str> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.message.as_str())
            .collect();
        if !errors.is_empty() {
            return Err(Error::Build(format!(
                "re-expanding the netlist at {celsius} °C (it reads `temper`): {}",
                errors.join("; ")
            )));
        }
        let (mut c, _notes) = Circuit::elaborate(&nl)?;
        let same = c.specs.len() == deck.specs
            && deck.specs <= self.specs.len()
            && c.specs
                .iter()
                .zip(&self.specs)
                .all(|(a, b)| a.name() == b.name());
        if !same {
            let first = c
                .specs
                .iter()
                .zip(&self.specs)
                .find(|(a, b)| a.name() != b.name())
                .map(|(a, b)| format!(": `{}` where `{}` was", a.name(), b.name()))
                .unwrap_or_default();
            return Err(Error::Build(format!(
                "at {celsius} °C the netlist expands to a different set of elements than at {} \
                 °C{first}; the topology cannot depend on the temperature",
                deck.kelvin - CTOK
            )));
        }
        c.specs.extend(self.specs[deck.specs..].iter().cloned());
        c.nodesets = self.nodesets.clone();
        c.ics = self.ics.clone();
        c.warm = self.warm.share();
        c.temper = Some(Arc::new(TemperDeck {
            netlist: Arc::clone(&deck.netlist),
            kelvin,
            specs: deck.specs,
        }));
        for (name, param, value) in &self.alters {
            c.alter(name, param, value.clone())?;
        }
        Ok(c)
    }
}
