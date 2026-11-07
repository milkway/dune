//! Source spans of an element's value argument and of the alterable
//! parameters spelled inside it (issue #17): what an editor rewrites to
//! write a new value back into the figure.
//!
//! Everything here works on the argument's raw text
//! (`MacroCall.raw_args`) as a sequence of chars, so offsets are char
//! offsets and positions are 1-based `(line, col)` with columns in
//! chars, the unit of rpic's spans.

use crate::{ParamSpanOverlay, PicRange};

/// The name `alter(refdes, value)` targets for an element of `letter`
/// (`dune_core::Circuit::alter` with an empty parameter: the
/// ngspice `IFparm` name of the principal value), `None` for letters
/// without one.
pub(crate) fn principal(letter: char) -> Option<&'static str> {
    Some(match letter {
        'R' => "resistance",
        'C' => "capacitance",
        'L' => "inductance",
        'V' | 'I' => "dc",
        'E' | 'F' | 'G' | 'H' => "gain",
        'D' | 'Q' | 'J' | 'Z' => "area",
        _ => return None,
    })
}

/// Transient functions of an independent source, by their `alter`
/// names (the card keyword, lowercased).
const WAVES: &[&str] = &["pulse", "sin", "exp", "pwl", "sffm", "trnoise", "trrandom"];

/// A position table for `chars` laid out from `(line, col)`: entry `i`
/// is where char `i` starts (entry `len` is just past the last).
pub(crate) fn positions(chars: &[char], line: u32, col: u32) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(chars.len() + 1);
    let (mut l, mut c) = (line, col);
    for &ch in chars {
        out.push((l, c));
        if ch == '\n' {
            l += 1;
            c = 1;
        } else {
            c += 1;
        }
    }
    out.push((l, c));
    out
}

pub(crate) fn range(pos: &[(u32, u32)], start: usize, end: usize) -> PicRange {
    let (line, col) = pos[start];
    let (end_line, end_col) = pos[end];
    PicRange {
        line,
        col,
        end_line,
        end_col,
    }
}

/// The char range of the value inside a raw argument: the argument
/// trimmed, then unquoted when it is one string (pic's quote-at-use-site
/// idiom, as `crate::unquote`), then trimmed again — the range whose
/// text is the card's value.
pub(crate) fn value_range(chars: &[char]) -> (usize, usize) {
    let (mut a, mut b) = trim(chars, 0, chars.len());
    if b - a >= 2 && chars[a] == '"' && chars[b - 1] == '"' && !chars[a + 1..b - 1].contains(&'"') {
        (a, b) = trim(chars, a + 1, b - 1);
    }
    (a, b)
}

fn trim(chars: &[char], mut a: usize, mut b: usize) -> (usize, usize) {
    while a < b && chars[a].is_whitespace() {
        a += 1;
    }
    while b > a && chars[b - 1].is_whitespace() {
        b -= 1;
    }
    (a, b)
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Word,
    Open,
    Close,
    Eq,
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    kind: Kind,
    start: usize,
    end: usize,
}

/// SPICE field tokens of `chars[a..b]`: words, `(`, `)` and `=`;
/// blanks, commas and pic `\` line continuations separate.
fn tokenize(chars: &[char], a: usize, b: usize) -> Vec<Tok> {
    let sep = |i: usize| {
        let c = chars[i];
        c.is_whitespace()
            || c == ','
            || (c == '\\'
                && (chars.get(i + 1) == Some(&'\n')
                    || (chars.get(i + 1) == Some(&'\r') && chars.get(i + 2) == Some(&'\n'))))
    };
    let mut out = Vec::new();
    let mut i = a;
    while i < b {
        if sep(i) {
            i += 1;
            continue;
        }
        let kind = match chars[i] {
            '(' => Some(Kind::Open),
            ')' => Some(Kind::Close),
            '=' => Some(Kind::Eq),
            _ => None,
        };
        if let Some(kind) = kind {
            out.push(Tok {
                kind,
                start: i,
                end: i + 1,
            });
            i += 1;
            continue;
        }
        let start = i;
        while i < b && !sep(i) && !matches!(chars[i], '(' | ')' | '=') {
            i += 1;
        }
        out.push(Tok {
            kind: Kind::Word,
            start,
            end: i,
        });
    }
    out
}

/// A SPICE number field (`INPevaluate` accepts it): a digit, or a sign
/// or point followed by one.
fn is_number(chars: &[char], t: &Tok) -> bool {
    if t.kind != Kind::Word {
        return false;
    }
    let s = &chars[t.start..t.end];
    let rest = match s.first() {
        Some('+' | '-') => &s[1..],
        _ => s,
    };
    match rest {
        [d, ..] if d.is_ascii_digit() => true,
        ['.', d, ..] => d.is_ascii_digit(),
        _ => false,
    }
}

fn word(chars: &[char], t: &Tok) -> String {
    chars[t.start..t.end]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase()
}

/// The alterable parameters spelled in the value `chars[a..b]` of an
/// element of `letter`, as `(name, start, end)` char ranges of
/// `chars`, in source order. Scalar parameters cover their number
/// only (`AC 1 90` gives `acmag` = `1`, `acphase` = `90`); a transient
/// function covers its whole spelling, keyword and parentheses
/// included (`PULSE(0 5 1u)`).
pub(crate) fn params(
    letter: char,
    chars: &[char],
    a: usize,
    b: usize,
) -> Vec<(&'static str, usize, usize)> {
    let toks = tokenize(chars, a, b);
    let num = |i: usize| toks.get(i).filter(|t| is_number(chars, t));
    let mut out: Vec<(&'static str, usize, usize)> = Vec::new();
    let push = |out: &mut Vec<_>, name: &'static str, t: &Tok| {
        out.push((name, t.start, t.end));
    };
    match letter {
        'V' | 'I' => {
            let mut has_dc = false;
            let mut i = 0;
            while i < toks.len() {
                let t = toks[i];
                let w = word(chars, &t);
                if t.kind != Kind::Word {
                    i += 1;
                } else if w == "dc" {
                    i += 1;
                    if let Some(n) = num(i) {
                        push(&mut out, "dc", n);
                        has_dc = true;
                        i += 1;
                    }
                } else if w == "ac" || w == "distof1" || w == "distof2" {
                    i += 1;
                    let names: [Option<&'static str>; 2] = if w == "ac" {
                        [Some("acmag"), Some("acphase")]
                    } else {
                        [None, None]
                    };
                    for name in names {
                        let Some(n) = num(i) else { break };
                        if let Some(name) = name {
                            push(&mut out, name, n);
                        }
                        i += 1;
                    }
                } else if let Some(&name) = WAVES.iter().find(|&&f| f == w) {
                    let start = t.start;
                    let mut end = t.end;
                    i += 1;
                    if toks.get(i).is_some_and(|t| t.kind == Kind::Open) {
                        let mut depth = 0usize;
                        while i < toks.len() {
                            match toks[i].kind {
                                Kind::Open => depth += 1,
                                Kind::Close => depth -= 1,
                                _ => {}
                            }
                            end = toks[i].end;
                            i += 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    } else {
                        while let Some(n) = num(i) {
                            end = n.end;
                            i += 1;
                        }
                    }
                    out.push((name, start, end));
                } else if is_number(chars, &t) && !has_dc {
                    // A bare leading value is the DC value.
                    push(&mut out, "dc", &t);
                    has_dc = true;
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }
        'R' | 'C' | 'L' | 'E' | 'G' => {
            if let (Some(n), Some(p)) = (num(0), principal(letter)) {
                push(&mut out, p, n);
            }
            if matches!(letter, 'C' | 'L') {
                keywords(chars, &toks, &["ic"], &mut out);
            }
        }
        'F' | 'H' => {
            // `Vname gain`.
            if let Some(n) = num(1) {
                push(&mut out, "gain", n);
            }
        }
        'D' | 'Q' | 'J' => {
            // `model [area]`.
            if let Some(n) = num(1) {
                push(&mut out, "area", n);
            }
            keywords(chars, &toks, &["area", "temp"], &mut out);
        }
        'Z' => {
            // `model [area]`; a MESFET has no `temp` (`alterable`).
            if let Some(n) = num(1) {
                push(&mut out, "area", n);
            }
            keywords(chars, &toks, &["area"], &mut out);
        }
        'M' => keywords(chars, &toks, &["w", "l", "temp"], &mut out),
        _ => {}
    }
    out.sort_by_key(|p| p.1);
    // A name spelled twice: the last one is the card's (SPICE reads
    // keyword fields left to right), keep only that.
    let mut keep: Vec<(&'static str, usize, usize)> = Vec::new();
    for p in out.into_iter().rev() {
        if !keep.iter().any(|k| k.0 == p.0) {
            keep.push(p);
        }
    }
    keep.reverse();
    keep
}

/// `NAME = value` fields whose lowercased name is in `names`: the
/// value's range.
fn keywords(
    chars: &[char],
    toks: &[Tok],
    names: &[&'static str],
    out: &mut Vec<(&'static str, usize, usize)>,
) {
    for w in toks.windows(3) {
        if w[0].kind == Kind::Word && w[1].kind == Kind::Eq && is_number(chars, &w[2]) {
            let k = word(chars, &w[0]);
            if let Some(&name) = names.iter().find(|&&n| n == k) {
                out.push((name, w[2].start, w[2].end));
            }
        }
    }
}

/// [`params`] as overlay entries, positioned by `pos`.
pub(crate) fn param_overlays(
    letter: char,
    chars: &[char],
    a: usize,
    b: usize,
    pos: &[(u32, u32)],
) -> Vec<ParamSpanOverlay> {
    params(letter, chars, a, b)
        .into_iter()
        .map(|(name, s, e)| ParamSpanOverlay {
            name: name.to_string(),
            span: range(pos, s, e),
            text: chars[s..e].iter().collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(letter: char, s: &str) -> Vec<(&'static str, String)> {
        let c: Vec<char> = s.chars().collect();
        params(letter, &c, 0, c.len())
            .into_iter()
            .map(|(n, a, b)| (n, c[a..b].iter().collect()))
            .collect()
    }

    #[test]
    fn source_spellings() {
        assert_eq!(names('V', "5"), [("dc", "5".into())]);
        assert_eq!(
            names('V', "DC 5 AC 1 90"),
            [
                ("dc", "5".into()),
                ("acmag", "1".into()),
                ("acphase", "90".into())
            ]
        );
        assert_eq!(names('V', "ac 1"), [("acmag", "1".into())]);
        assert_eq!(names('I', "AC"), []);
        assert_eq!(
            names('V', "DC 1 PULSE(0 5 0 1u 1u 10m 20m) AC 2"),
            [
                ("dc", "1".into()),
                ("pulse", "PULSE(0 5 0 1u 1u 10m 20m)".into()),
                ("acmag", "2".into())
            ]
        );
        assert_eq!(names('V', "sin 0 1 1k"), [("sin", "sin 0 1 1k".into())]);
        assert_eq!(
            names('V', "PWL(0 0, 1m 1) DISTOF1 0.1 0"),
            [("pwl", "PWL(0 0, 1m 1)".into())]
        );
        assert_eq!(
            names('V', "PULSE(0 1e-3 \\\n  0 1e-9)"),
            [("pulse", "PULSE(0 1e-3 \\\n  0 1e-9)".into())]
        );
        assert_eq!(names('V', "-.5"), [("dc", "-.5".into())]);
    }

    #[test]
    fn other_letters() {
        assert_eq!(names('R', "4.7k"), [("resistance", "4.7k".into())]);
        assert_eq!(
            names('C', "1u IC=2"),
            [("capacitance", "1u".into()), ("ic", "2".into())]
        );
        assert_eq!(names('F', "Vsense 10"), [("gain", "10".into())]);
        assert_eq!(names('E', "2"), [("gain", "2".into())]);
        assert_eq!(names('Q', "QX AREA=2"), [("area", "2".into())]);
        assert_eq!(names('D', "DX 3"), [("area", "3".into())]);
        assert_eq!(
            names('M', "NM W = 1e-6 L=1u"),
            [("w", "1e-6".into()), ("l", "1u".into())]
        );
        assert_eq!(names('Z', "ZN 2 TEMP=50"), [("area", "2".into())]);
        assert_eq!(names('Z', "ZN AREA=1.5"), [("area", "1.5".into())]);
        assert_eq!(names('X', "OPAMP"), []);
        assert_eq!(names('R', "{rval}"), []);
    }

    #[test]
    fn quoted_values_and_positions() {
        let c: Vec<char> = "  \"DX \"".chars().collect();
        assert_eq!(value_range(&c), (3, 5));
        let c: Vec<char> = "ab\ncΩd".chars().collect();
        let pos = positions(&c, 4, 7);
        assert_eq!(pos[0], (4, 7));
        assert_eq!(pos[3], (5, 1));
        assert_eq!(pos[6], (5, 4));
    }
}
