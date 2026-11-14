//! Basic `.param` support (an ngspice extension; PLAN.md Phase 1
//! lists "`.param` básico").
//!
//! Scope of "basic", documented in `docs/dune-netlist.md`:
//! `.param name=value` defines a global numeric constant; the value
//! and any `{…}` reference may use numbers (with SPICE scale
//! factors), previously defined parameter names and the operators
//! `+ - * / ^` with unary minus. Parentheses and whitespace inside
//! braces are *not* supported, because `( ) ,` and blanks are field
//! separators at the lexer level (a brace expression must be one
//! token). Anything richer waits until a corpus circuit needs it.

use std::collections::HashMap;

/// Longest accepted brace expression, so hostile input cannot make
/// the evaluator crawl.
const MAX_EXPR_LEN: usize = 512;

#[derive(Debug, Default)]
pub struct ParamEnv {
    map: HashMap<String, f64>,
}

impl ParamEnv {
    pub fn define(&mut self, name: &str, value: f64) {
        self.map.insert(name.to_ascii_lowercase(), value);
    }

    pub fn get(&self, name: &str) -> Option<f64> {
        self.map.get(name).copied()
    }

    /// Evaluate a `{expr}` token (braces included, already lowercased).
    pub fn eval_brace(&self, token: &str) -> Result<f64, String> {
        let inner = token
            .strip_prefix('{')
            .and_then(|t| t.strip_suffix('}'))
            .ok_or_else(|| {
                format!(
                    "malformed parameter expression `{token}` (note: basic .param \
                     expressions cannot contain spaces, commas or parentheses)"
                )
            })?;
        self.eval_expr(inner)
    }

    /// Evaluate a `{expr}` token where its value is consumed (an
    /// element, model or control card). A `.param` may hold an
    /// infinite or NaN value and pass it through other `.param`s
    /// unnoticed, but a non-finite result reaching a consumer is an
    /// error — ngspice-47 behaves the same way (`.param z={1/0}` alone
    /// runs; `v1 1 0 {z}` stops with "unknown parameter (inf)";
    /// `{1/z}` is 0 and runs).
    pub fn eval_use(&self, token: &str) -> Result<f64, String> {
        let v = self.eval_brace(token)?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err(format!(
                "parameter expression `{token}` evaluates to {v}, not a finite number"
            ))
        }
    }

    /// Evaluate a brace-free expression: numbers, parameter names,
    /// `+ - * / ^`, unary minus.
    pub fn eval_expr(&self, expr: &str) -> Result<f64, String> {
        if expr.len() > MAX_EXPR_LEN {
            return Err("parameter expression too long".into());
        }
        let mut lx = Lexer {
            bytes: expr.as_bytes(),
            pos: 0,
        };
        let v = self.sum(&mut lx)?;
        match lx.peek() {
            None => Ok(v),
            Some(_) => Err(format!("trailing garbage in expression `{expr}`")),
        }
    }

    fn sum(&self, lx: &mut Lexer) -> Result<f64, String> {
        let mut acc = self.product(lx)?;
        while let Some(op @ (b'+' | b'-')) = lx.peek() {
            lx.bump();
            let rhs = self.product(lx)?;
            acc = if op == b'+' { acc + rhs } else { acc - rhs };
        }
        Ok(acc)
    }

    fn product(&self, lx: &mut Lexer) -> Result<f64, String> {
        let mut acc = self.power(lx)?;
        while let Some(op @ (b'*' | b'/')) = lx.peek() {
            lx.bump();
            let rhs = self.power(lx)?;
            acc = if op == b'*' { acc * rhs } else { acc / rhs };
        }
        Ok(acc)
    }

    fn power(&self, lx: &mut Lexer) -> Result<f64, String> {
        // Unary sign binds looser than `^` (`-2^2` is -4) and `^` is
        // LEFT-associative (`2^3^2` is 64): both observed in
        // ngspice-47 (probes P05 and D_POW of `docs/pdk-parser.md`).
        // Recursion is bounded by MAX_EXPR_LEN.
        match lx.peek() {
            Some(b'-') => {
                lx.bump();
                return Ok(-self.power(lx)?);
            }
            Some(b'+') => {
                lx.bump();
                return self.power(lx);
            }
            _ => {}
        }
        let mut acc = self.atom(lx)?;
        while lx.peek() == Some(b'^') {
            lx.bump();
            acc = acc.powf(self.exponent(lx)?);
        }
        Ok(acc)
    }

    /// The right operand of `^`: an atom, optionally signed
    /// (`2^-1` is 0.5).
    fn exponent(&self, lx: &mut Lexer) -> Result<f64, String> {
        match lx.peek() {
            Some(b'-') => {
                lx.bump();
                Ok(-self.exponent(lx)?)
            }
            Some(b'+') => {
                lx.bump();
                self.exponent(lx)
            }
            _ => self.atom(lx),
        }
    }

    fn atom(&self, lx: &mut Lexer) -> Result<f64, String> {
        match lx.peek() {
            Some(b) if b.is_ascii_digit() || b == b'.' => {
                let text = lx.take_number();
                crate::scan::evaluate(text).ok_or_else(|| format!("bad number `{text}`"))
            }
            Some(b) if b.is_ascii_alphabetic() || b == b'_' => {
                let name = lx.take_ident().to_ascii_lowercase();
                self.get(&name)
                    .ok_or_else(|| format!("unknown parameter `{name}`"))
            }
            Some(b) => Err(format!("unexpected `{}` in expression", b as char)),
            None => Err("expression ends where a value was expected".into()),
        }
    }
}

struct Lexer<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    /// A number: digits/dot, optional exponent with sign, optional
    /// scale-factor/unit letters (consumed greedily, like the deck
    /// lexer does).
    fn take_number(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' {
                self.bump();
            } else {
                break;
            }
        }
        // Exponent: e/d with optional sign.
        if let Some(b'e' | b'E' | b'd' | b'D') = self.peek() {
            let mark = self.pos;
            self.bump();
            if let Some(b'+' | b'-') = self.peek() {
                self.bump();
            }
            if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    self.bump();
                }
            } else {
                // Not an exponent after all ("2e" could be "2*e"?
                // no: treat the letters as unit suffix below).
                self.pos = mark;
            }
        }
        // Scale factor / unit letters (3u, 10kohm).
        while self.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
            self.bump();
        }
        std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("")
    }

    fn take_ident(&mut self) -> &'a str {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            self.bump();
        }
        std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> ParamEnv {
        let mut e = ParamEnv::default();
        e.define("rload", 10e3);
        e.define("two", 2.0);
        e
    }

    #[test]
    fn numbers_and_names() {
        let e = env();
        assert_eq!(e.eval_expr("3"), Ok(3.0));
        assert_eq!(e.eval_expr("rload"), Ok(1e4));
        assert_eq!(e.eval_expr("2k"), Ok(2000.0));
        assert!((e.eval_expr("1.5e-3").unwrap() - 1.5e-3).abs() < 1e-18);
    }

    #[test]
    fn arithmetic_and_precedence() {
        let e = env();
        assert_eq!(e.eval_expr("1+two*3"), Ok(7.0));
        assert_eq!(e.eval_expr("rload/two"), Ok(5e3));
        assert_eq!(e.eval_expr("-two^two"), Ok(-4.0)); // sign binds looser than ^
        assert_eq!(e.eval_expr("2^3^2"), Ok(64.0)); // left-assoc, as ngspice-47
        assert_eq!(e.eval_expr("two^-1"), Ok(0.5));
        assert_eq!(e.eval_expr("1-two-two"), Ok(-3.0)); // left-assoc
    }

    #[test]
    fn braces_and_errors() {
        let e = env();
        assert_eq!(e.eval_brace("{rload*two}"), Ok(2e4));
        assert!(
            e.eval_brace("{oops}")
                .unwrap_err()
                .contains("unknown parameter")
        );
        assert!(e.eval_brace("no-braces").is_err());
        assert!(e.eval_expr("1+)").is_err());
        assert!(e.eval_expr("").is_err());
        let mut z = env();
        z.define("z", f64::INFINITY);
        assert_eq!(z.eval_brace("{1/z}"), Ok(0.0));
        assert_eq!(z.eval_use("{1/z}"), Ok(0.0));
        assert!(z.eval_use("{z}").unwrap_err().contains("inf, not a finite"));
        assert!(z.eval_use("{z*0}").unwrap_err().contains("NaN"));
        let long = "1+".repeat(600) + "1";
        assert!(e.eval_expr(&long).is_err());
    }

    #[test]
    fn exponent_vs_unit_suffix() {
        let e = env();
        assert_eq!(e.eval_expr("2e3"), Ok(2000.0));
        // "2e" alone: 'e' is a unit letter, value 2.
        assert_eq!(e.eval_expr("2e"), Ok(2.0));
        assert_eq!(e.eval_expr("10kohm+1"), Ok(10001.0));
    }
}
