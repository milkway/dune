//! Engine overrides of a run (issue #16): `SimOptions.options`, the
//! caller's `.options` applied after the deck's cards, and
//! `SimOptions.tmaxDivisor`, the transient refinement knob.
//!
//! An override goes through the deck's own path: each entry becomes
//! the `Param` the netlist parser builds for `.options KEY=VALUE`, and
//! [`Options::apply`] takes it from there — same keys, same defaults,
//! same warnings. A number is taken exactly as given (no decimal
//! round trip); a string is read as the deck's text would be (`"1u"`,
//! `"gear"`). Invalid entries are warnings in the run's
//! `diagnostics`, as an invalid `.options` card is, never a throw.

use dune_core::Options;
use dune_netlist::ir::{Param, ParamValue};
use dune_netlist::{Diagnostic, Severity};
use serde_json::{Map, Value};

use crate::schema::{DiagnosticJson, SimError};

/// Flags whose `.options` card only ever sets them (`IF_FLAG`: the
/// 3f5 reads no value): `false` clears them instead.
const SET_ONLY_FLAGS: [&str; 2] = ["noopiter", "nopage"];

/// The run's overrides: what `SimOptions` adds to the deck.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// `SimOptions.options`, applied after the deck's `.options`.
    pub options: Map<String, Value>,
    /// `SimOptions.tmaxDivisor` ([`Options::tmax_divisor`]).
    pub tmax_divisor: Option<f64>,
}

impl Overrides {
    /// Refuse a divisor outside `[1, ∞)` (kind `options`).
    pub fn check(&self) -> Result<(), SimError> {
        check_divisor(self.tmax_divisor)
    }

    /// The deck's options with the overrides on top, and the warnings
    /// the overrides raised.
    pub fn apply(&self, deck: &Options) -> (Options, Vec<DiagnosticJson>) {
        let mut o = deck.clone();
        let diags = apply_options(&mut o, &self.options);
        o.tmax_divisor = self.tmax_divisor;
        (o, diags)
    }
}

/// `tmaxDivisor` must be a finite number ≥ 1 (or absent).
pub fn check_divisor(d: Option<f64>) -> Result<(), SimError> {
    match d {
        Some(d) if !(d >= 1.0 && d.is_finite()) => Err(SimError::new(
            "options",
            format!("bad options: tmaxDivisor must be a finite number >= 1, got {d}"),
        )),
        _ => Ok(()),
    }
}

fn warning(message: String) -> DiagnosticJson {
    DiagnosticJson {
        severity: "warning".into(),
        kind: None,
        message: format!("opts.options: {message}"),
        file: None,
        line: None,
        column: None,
        end_column: None,
    }
}

/// Apply `map` to `o` entry by entry (keys in sorted order, so the
/// result does not depend on the object's key order).
fn apply_options(o: &mut Options, map: &Map<String, Value>) -> Vec<DiagnosticJson> {
    let mut out = Vec::new();
    for (key, value) in map {
        let name = key.to_ascii_lowercase();
        let ok_name = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !ok_name {
            out.push(warning(format!("bad option name {key:?}; ignored")));
            continue;
        }
        let param = match value {
            Value::Bool(false) if SET_ONLY_FLAGS.contains(&name.as_str()) => {
                match name.as_str() {
                    "noopiter" => o.noopiter = false,
                    _ => o.nopage = false,
                }
                continue;
            }
            Value::Bool(b) => card_param(&name, None).map(|mut p| {
                // `true` is the bare flag; `false` is `KEY=0`
                // (`ltrafit=0`).
                if !b {
                    p.value = ParamValue::Num(0.0);
                }
                p
            }),
            Value::Number(n) => match n.as_f64() {
                Some(x) if x.is_finite() => card_param(&name, None).map(|mut p| {
                    p.value = ParamValue::Num(x);
                    p
                }),
                _ => Err(format!("option {name} must be a finite number")),
            },
            Value::String(s) => card_param(&name, Some(s)),
            _ => Err(format!(
                "option {name} must be a number, a string or a boolean"
            )),
        };
        match param {
            Ok(p) => {
                let mut diags: Vec<Diagnostic> = Vec::new();
                o.apply(std::slice::from_ref(&p), &mut diags);
                out.extend(diags.into_iter().map(|d| warning(d.message)));
            }
            Err(msg) => out.push(warning(msg)),
        }
    }
    out
}

/// The `Param` the parser makes of `.options NAME` (a bare flag) or
/// `.options NAME=TEXT`. `TEXT` must be one deck token that gives
/// exactly that one parameter; anything else is refused.
fn card_param(name: &str, text: Option<&str>) -> Result<Param, String> {
    let card = match text {
        None => name.to_string(),
        Some(t) => {
            let one_token = !t.is_empty()
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_'));
            if !one_token {
                return Err(format!("option {name}: bad value {t:?}"));
            }
            format!("{name}={t}")
        }
    };
    let parsed = dune_netlist::parse_str("options", &format!("opts\n.options {card}\n.end\n"));
    let clean = parsed.diags.iter().all(|d| d.severity != Severity::Error);
    match parsed.netlist.options.as_slice() {
        [p] if clean && p.name == name => Ok(p.clone()),
        _ => Err(format!(
            "option {name}: bad value {:?}",
            text.unwrap_or_default()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(json: &str) -> (Options, Vec<String>) {
        let Value::Object(map) = serde_json::from_str(json).unwrap() else {
            panic!()
        };
        let ov = Overrides {
            options: map,
            tmax_divisor: None,
        };
        let (o, d) = ov.apply(&Options::default());
        (o, d.into_iter().map(|d| d.message).collect())
    }

    #[test]
    fn values_follow_the_card() {
        let (o, d) = apply(
            r#"{"TRTOL": 1, "method": "GEAR", "maxord": "3", "reltol": "1m",
                "temp": 50, "ltrafit": true, "noopiter": true}"#,
        );
        assert!(d.is_empty(), "{d:?}");
        assert_eq!(o.trtol, 1.0);
        assert_eq!(o.method, dune_core::Method::Gear);
        assert_eq!(o.maxord, 3);
        assert_eq!(o.reltol, 1e-3);
        assert_eq!(o.temp, 50.0 + dune_core::consts::CTOK);
        assert!(o.ltra_fit && o.noopiter);
    }

    #[test]
    fn false_clears_flags() {
        let deck = Options {
            noopiter: true,
            ltra_fit: true,
            ..Options::default()
        };
        let Value::Object(map) =
            serde_json::from_str(r#"{"noopiter": false, "ltrafit": false}"#).unwrap()
        else {
            panic!()
        };
        let (o, d) = Overrides {
            options: map,
            tmax_divisor: Some(4.0),
        }
        .apply(&deck);
        assert!(d.is_empty());
        assert!(!o.noopiter && !o.ltra_fit);
        assert_eq!(o.tmax_divisor, Some(4.0));
    }

    #[test]
    fn bad_entries_are_warnings() {
        let (o, d) = apply(
            r#"{"trtol": "abc", "reltol": true, "bogus": 1, "method": "euler",
                "x y": 1, "gmin": null, "itl1": -3, "abstol": "1 reltol=5"}"#,
        );
        let def = Options::default();
        assert_eq!(
            (o.trtol, o.reltol, o.abstol, o.gmin, o.itl1, o.method),
            (
                def.trtol, def.reltol, def.abstol, def.gmin, def.itl1, def.method
            )
        );
        assert_eq!(d.len(), 8, "{d:?}");
        assert!(d.iter().all(|m| m.starts_with("opts.options: ")));
    }

    #[test]
    fn divisor_bounds() {
        assert!(check_divisor(None).is_ok());
        assert!(check_divisor(Some(1.0)).is_ok());
        for d in [0.5, 0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(check_divisor(Some(d)).unwrap_err().kind, "options");
        }
    }
}
