//! Card tokenizer, a faithful port of `INPgetTok`, `INPgetUTok` and
//! `INPevaluate` (`corpus/spice3f5/src/lib/inp/inpgtok.c`, `inpeval.c`).
//!
//! Fields on an element line are separated by blanks, tabs, commas,
//! equal signs and parentheses — all equivalent, all discarded. A
//! number is `[±]digits[.digits][(e|d)[±]digits][scale][junk]`: the
//! scale factors are `t g meg k mil m u n p f` (case-insensitive,
//! `mil` = 25.4e-6) and any letters after the number or scale factor
//! are ignored (`10kOhm` is 10e3). A value token is cut like
//! `INPgetUTok` cuts it: at separators and, following the C's
//! sign-state machine, at `*`, `/`, `^` and at interior `+`/`-` that
//! cannot be part of a sign or exponent.

use crate::deck::Card;
use crate::span::Span;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Token<'c> {
    pub text: &'c str,
    pub start: usize,
    pub end: usize,
}

impl Token<'_> {
    /// Lowercased copy (the 3f5 lowercases the whole deck; we keep
    /// the source intact and lowercase identifiers at token level).
    pub fn lc(&self) -> String {
        self.text.to_ascii_lowercase()
    }

    pub fn eq_ic(&self, other: &str) -> bool {
        self.text.eq_ignore_ascii_case(other)
    }
}

fn is_sep(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'=' | b'(' | b')' | b',')
}

#[derive(Clone)]
pub(crate) struct Scanner<'c> {
    card: &'c Card,
    pos: usize,
}

impl<'c> Scanner<'c> {
    pub fn new(card: &'c Card) -> Self {
        Scanner { card, pos: 0 }
    }

    pub fn span(&self, t: &Token) -> Span {
        self.card.span_of(t.start, t.end)
    }

    /// Span at the current position (for "expected more fields" errors).
    pub fn here(&self) -> Span {
        let p = self.pos.min(self.card.text.len());
        self.card.span_of(p.saturating_sub(1), p)
    }

    fn skip_seps(&mut self) {
        let bytes = self.card.text.as_bytes();
        while self.pos < bytes.len() && is_sep(bytes[self.pos]) {
            self.pos += 1;
        }
    }

    /// `INPgetTok`: the next field, cut at separators only. A
    /// parameter expression is one field whatever it contains: from a
    /// `{` to its matching `}` (ngspice's `inp_spawn_brace` counts
    /// nesting), and a field that starts with `'` runs to the next
    /// `'` (ngspice turns `'…'` into `{…}` before anything else,
    /// `inp_change_quotes`). An unclosed one runs to the end of the
    /// card, where the evaluator reports it.
    pub fn next_tok(&mut self) -> Option<Token<'c>> {
        self.skip_seps();
        let bytes = self.card.text.as_bytes();
        if self.pos >= bytes.len() {
            return None;
        }
        let start = self.pos;
        if bytes[start] == b'\'' {
            self.pos = match bytes[start + 1..].iter().position(|&b| b == b'\'') {
                Some(k) => start + 1 + k + 1,
                None => bytes.len(),
            };
        }
        while self.pos < bytes.len() && !is_sep(bytes[self.pos]) {
            if bytes[self.pos] == b'{' {
                self.pos = skip_brace(bytes, self.pos);
            } else {
                self.pos += 1;
            }
        }
        Some(Token {
            text: &self.card.text[start..self.pos],
            start,
            end: self.pos,
        })
    }

    /// A bare parameter value right after an assignment `=`
    /// (`w=wv`, `rsh=rv*2`, `noia=max(a, b)`): ngspice wraps such a
    /// value in `{…}` (`inp_fix_param_values`, Modified BSD), taking
    /// everything up to the first blank outside parentheses. Here a
    /// `,` or `)` outside parentheses also ends it (the `)` closing a
    /// `.model` parameter list). Consumes nothing and returns `None`
    /// unless the separators before the next field contain an `=`.
    pub fn assign_value(&mut self) -> Option<Token<'c>> {
        let bytes = self.card.text.as_bytes();
        let mut p = self.pos;
        let mut after_eq = None;
        while p < bytes.len() && is_sep(bytes[p]) {
            if bytes[p] == b'=' {
                if bytes.get(p + 1) == Some(&b'=') {
                    return None;
                }
                after_eq = Some(p + 1);
            }
            p += 1;
        }
        let mut p = after_eq?;
        while p < bytes.len() && matches!(bytes[p], b' ' | b'\t') {
            p += 1;
        }
        let start = p;
        let mut depth = 0usize;
        while p < bytes.len() {
            match bytes[p] {
                b' ' | b'\t' | b',' | b')' if depth == 0 => break,
                b'(' => depth += 1,
                b')' => depth -= 1,
                b'{' => {
                    p = skip_brace(bytes, p);
                    continue;
                }
                _ => {}
            }
            p += 1;
        }
        if p == start {
            return None;
        }
        self.pos = p;
        Some(Token {
            text: &self.card.text[start..p],
            start,
            end: p,
        })
    }

    /// Continue scanning from byte offset `pos` of the card text.
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.card.text.len());
    }

    pub fn peek_tok(&self) -> Option<Token<'c>> {
        self.clone().next_tok()
    }

    /// `INPevaluate` with `gobble`: if a number starts here, consume
    /// its whole value token (`INPgetUTok` cut) and return the value.
    /// Otherwise consume nothing.
    pub fn try_value(&mut self) -> Option<(f64, Token<'c>)> {
        let mut probe = self.clone();
        probe.skip_seps();
        let bytes = probe.card.text.as_bytes();
        if probe.pos >= bytes.len() {
            return None;
        }
        let start = probe.pos;
        let end = start + utok_len(&bytes[start..]);
        let text = &self.card.text[start..end];
        let value = evaluate(text)?;
        self.pos = end;
        Some((value, Token { text, start, end }))
    }
}

/// Index just past the `}` matching the `{` at `bytes[at]`, or the
/// end of the text when it is never closed.
pub(crate) fn skip_brace(bytes: &[u8], at: usize) -> usize {
    match_brace(bytes, at).unwrap_or(bytes.len())
}

/// Index just past the `}` matching the `{` at `bytes[at]`.
pub(crate) fn match_brace(bytes: &[u8], at: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(at) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Length of the value token starting at `bytes[0]` (`INPgetUTok`'s
/// sign-state machine; quotes are not handled because no element card
/// takes quoted values).
fn utok_len(bytes: &[u8]) -> usize {
    let mut signstate = 0u8;
    for (i, &b) in bytes.iter().enumerate() {
        if is_sep(b) {
            return i;
        }
        match b {
            b'+' if signstate == 1 || signstate == 3 => return i,
            b'-' => {
                if signstate == 1 || signstate == 3 {
                    return i;
                }
                signstate += 1;
                continue;
            }
            b'*' | b'/' | b'^' => return i,
            _ => {}
        }
        if b.is_ascii_digit() || b == b'.' {
            signstate = if signstate > 1 { 3 } else { 1 };
        } else if (b == b'e' || b == b'E') && signstate == 1 {
            signstate = 2;
        } else {
            signstate = 3;
        }
    }
    bytes.len()
}

/// `INPevaluate` on one token: `None` when the token does not start
/// with a number (the C's "error" return).
pub(crate) fn evaluate(token: &str) -> Option<f64> {
    evaluate_prefix(token.as_bytes()).map(|(v, _)| v)
}

/// `INPevaluate` reading a number at the start of `b` and reporting
/// where it ends: the value and the byte length consumed, scale
/// factor included (`meg`/`mil` count 3 bytes, any other scale
/// letter 1; letters after it are not consumed). `None` when no
/// number starts here, as for [`evaluate`].
///
/// The C's non-gobbling mode (`gobble == 0`, used by the B-source
/// lexer) leaves the cursor *on* the scale letter, so `2k` ends the
/// expression there and the `k` becomes an unknown card parameter;
/// consuming it is Dune's superset (`docs/dune-netlist.md`).
pub(crate) fn evaluate_prefix(b: &[u8]) -> Option<(f64, usize)> {
    let mut i = 0usize;
    let mut mantis: f64 = 0.0;
    let mut expo1: i32 = 0;
    let mut expo2: i32 = 0;
    let mut sign: f64 = 1.0;
    let mut expsgn: i32 = 1;

    if i < b.len() && b[i] == b'+' {
        i += 1;
    }
    if i < b.len() && b[i] == b'-' {
        i += 1;
        sign = -1.0;
    }
    if i >= b.len() || (!b[i].is_ascii_digit() && b[i] != b'.') {
        return None; // just a sign, or not a number at all
    }
    while i < b.len() && b[i].is_ascii_digit() {
        mantis = 10.0 * mantis + f64::from(b[i] - b'0');
        i += 1;
    }
    if i >= b.len() {
        return Some((mantis * sign, i));
    }
    if b[i] == b':' {
        // The C's hack for subcircuit node numbering: not a number.
        return None;
    }
    if b[i] == b'.' {
        i += 1;
        if i >= b.len() {
            return Some((mantis * sign, i));
        }
        while i < b.len() && b[i].is_ascii_digit() {
            mantis = 10.0 * mantis + f64::from(b[i] - b'0');
            expo1 -= 1;
            i += 1;
        }
    }
    if i < b.len() && matches!(b[i], b'E' | b'e' | b'D' | b'd') {
        i += 1;
        if i < b.len() && b[i] == b'+' {
            i += 1;
        }
        if i < b.len() && b[i] == b'-' {
            i += 1;
            expsgn = -1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            expo2 = expo2
                .saturating_mul(10)
                .saturating_add(i32::from(b[i] - b'0'));
            i += 1;
        }
    }
    // Scale factor; everything after it is ignored.
    if i < b.len() {
        let scale_len = match b[i].to_ascii_lowercase() {
            b't' => {
                expo1 += 12;
                1
            }
            b'g' => {
                expo1 += 9;
                1
            }
            b'k' => {
                expo1 += 3;
                1
            }
            b'u' => {
                expo1 -= 6;
                1
            }
            b'n' => {
                expo1 -= 9;
                1
            }
            b'p' => {
                expo1 -= 12;
                1
            }
            b'f' => {
                expo1 -= 15;
                1
            }
            b'm' => {
                let two = b.get(i + 1).map(u8::to_ascii_lowercase);
                let three = b.get(i + 2).map(u8::to_ascii_lowercase);
                match (two, three) {
                    (Some(b'e'), Some(b'g')) => {
                        expo1 += 6;
                        3
                    }
                    (Some(b'i'), Some(b'l')) => {
                        expo1 -= 6;
                        mantis *= 25.4;
                        3
                    }
                    _ => {
                        expo1 -= 3;
                        1
                    }
                }
            }
            _ => 0,
        };
        i += scale_len;
    }
    // `pow`, not repeated multiplication, to land on the same
    // doubles the C does.
    Some((
        sign * mantis * pow10(expo1.saturating_add(expsgn * expo2)),
        i,
    ))
}

/// `pow(10.0, k)` correctly rounded, on every target.
///
/// glibc's and Apple's `pow` return the correctly rounded power of
/// ten for every integer `k` (checked on Apple Silicon for
/// -400..=400), but the soft `libm` behind `f64::powf` on
/// `wasm32-unknown-unknown` is off by an ulp for some (`10^-5`, so
/// `4.00000E+000` read as 3.9999999999999996). Rust's decimal
/// parser is correctly rounded everywhere, so it gives the C's
/// doubles on all targets. Outside ±400 the result is 0 or inf
/// either way.
fn pow10(k: i32) -> f64 {
    if (-400..=400).contains(&k) {
        format!("1e{k}")
            .parse()
            .unwrap_or_else(|_| 10f64.powf(f64::from(k)))
    } else {
        10f64.powf(f64::from(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow10_is_correctly_rounded() {
        // Literals are correctly rounded by rustc; the soft libm on
        // wasm32 missed these through `powf`.
        assert_eq!(pow10(-5), 1e-5);
        assert_eq!(pow10(-23), 1e-23);
        assert_eq!(pow10(22), 1e22);
        assert_eq!(pow10(308), 1e308);
        assert_eq!(pow10(-323), 1e-323);
        assert_eq!(evaluate("4.00000E+000"), Some(4.0));
        assert_eq!(pow10(-1000), 0.0);
        assert_eq!(pow10(1000), f64::INFINITY);
        assert_eq!(pow10(i32::MIN), 0.0);
    }
    use crate::deck::{Card, read_with_loader};
    use std::path::Path;

    fn card(line: &str) -> Card {
        struct NoInc;
        impl crate::deck::IncludeLoader for NoInc {
            fn load(&mut self, _: &Path, _: u64) -> Result<String, String> {
                Err("none".into())
            }
        }
        let mut deck = read_with_loader(Path::new("t.cir"), format!("t\n{line}\n"), &mut NoInc);
        deck.cards.remove(0)
    }

    #[test]
    fn separators_vanish() {
        let c = card("v1 (1,2) dc=5");
        let mut s = Scanner::new(&c);
        let toks: Vec<String> = std::iter::from_fn(|| s.next_tok().map(|t| t.lc())).collect();
        assert_eq!(toks, ["v1", "1", "2", "dc", "5"]);
    }

    /// The port mirrors the C's arithmetic (`mantis * pow(10, expo)`),
    /// which may differ from a decimal literal in the last ulp, so
    /// non-integer comparisons are relative.
    fn approx(token: &str, expect: f64) {
        let got = evaluate(token).unwrap_or_else(|| panic!("{token} did not parse"));
        assert!(
            (got - expect).abs() <= expect.abs() * 1e-12,
            "{token}: got {got:e}, expected ~{expect:e}"
        );
    }

    #[test]
    fn numbers_match_the_manual() {
        // "1000, 1000.0, 1000Hz, 1e3, 1.0e3, 1KHz, and 1K all
        // represent the same number" (spice.txt §2.1).
        for t in ["1000", "1000.0", "1000Hz", "1e3", "1.0e3", "1KHz", "1K"] {
            assert_eq!(evaluate(t), Some(1000.0), "{t}");
        }
        approx("2.2meg", 2.2e6);
        approx("10u", 1e-5);
        approx("1e-12", 1e-12);
        approx("10kohm", 1e4);
        approx("1mil", 25.4e-6);
        // "M, MA, MSec, and MMhos all represent the same scale
        // factor" (milli).
        for t in ["5M", "5MA", "5MSec", "5MMhos", "5m"] {
            approx(t, 5e-3);
        }
        // FORTRAN-style exponent.
        assert_eq!(evaluate("1d3"), Some(1000.0));
        approx("-1.5e-2", -0.015);
        assert_eq!(evaluate("+3"), Some(3.0));
        // Quirks the C has: "." and "1." are numbers.
        assert_eq!(evaluate("."), Some(0.0));
        assert_eq!(evaluate("1."), Some(1.0));
        // Not numbers.
        assert_eq!(evaluate("-"), None);
        assert_eq!(evaluate("abc"), None);
        assert_eq!(evaluate("1:2"), None);
        assert_eq!(evaluate(""), None);
    }

    #[test]
    fn try_value_consumes_whole_token_or_nothing() {
        let c = card("r1 1 0 10kohm w=5u");
        let mut s = Scanner::new(&c);
        assert!(s.try_value().is_none()); // "r1" is not a number
        s.next_tok(); // r1
        assert_eq!(s.try_value().map(|v| v.0), Some(1.0));
        assert_eq!(s.try_value().map(|v| v.0), Some(0.0));
        let kohm = s.try_value().unwrap().0; // 10kohm, gobbled
        assert!((kohm - 1e4).abs() < 1e-6);
        assert!(s.try_value().is_none()); // "w"
        assert_eq!(s.next_tok().unwrap().lc(), "w");
        let w = s.try_value().unwrap().0;
        assert!((w - 5e-6).abs() <= 5e-18);
        assert!(s.peek_tok().is_none());
    }

    #[test]
    fn utok_cuts_at_operators_like_the_c() {
        let c = card("x 3-2");
        let mut s = Scanner::new(&c);
        s.next_tok();
        // INPgetUTok cuts "3-2" at the interior '-': value is 3,
        // "-2" stays for the next token.
        assert_eq!(s.try_value().map(|v| v.0), Some(3.0));
        assert_eq!(s.next_tok().unwrap().text, "-2");
        // But exponent signs are not cuts.
        let c = card("x 1e+5 1e-5");
        let mut s = Scanner::new(&c);
        s.next_tok();
        assert_eq!(s.try_value().map(|v| v.0), Some(1e5));
        assert_eq!(s.try_value().map(|v| v.0), Some(1e-5));
    }

    #[test]
    fn spans_point_at_the_source() {
        let c = card("r1 1 0 1k");
        let mut s = Scanner::new(&c);
        let t = s.next_tok().unwrap();
        assert_eq!((t.start, t.end), (0, 2));
    }
}
