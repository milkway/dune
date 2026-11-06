//! **Interim** workarounds for rpic-core 0.12.1 (D-0020).
//!
//! Everything in this module exists only because rpic-core 0.12.1
//! does not yet expose what D-0004 needs; each item names the rpic-lang
//! PR that retires it. When rpic-core ships those PRs, this module
//! shrinks to nothing and the extractor reads the corresponding
//! `Drawing` fields instead:
//!
//! * **W1** — [`strip_directives`]: rpic-core 0.12.1 rejects lines
//!   starting with `.` (lexer error), so D-0004 §4 directives
//!   (`.tran`, `.model`, …) are blanked here *before* compiling, line
//!   count preserved, and collected with exact spans. Retired by the
//!   passthrough PR (`Drawing.passthrough`, dpic-style).
//! * **W2** — named nodes come from the prelude's `Name: node(P)` block
//!   (see `corpus/pic/spice.pic`) instead of plain labeled places
//!   (D-0004 §3), because places do not reach the `Drawing`. Retired by
//!   the `Drawing.places` PR; the extractor-side seam is
//!   `NODE_MARKER` in `lib.rs`.
//! * **W3** — [`call_site`]: `BlockInfo` carries no span, and shapes
//!   expanded from a macro carry the *define* site, so an element's
//!   diagnostics locate its call site by scanning the user source for
//!   the `Label:` that introduced it. Retired by the `BlockInfo.span`
//!   PR.

use crate::PicSpan;

/// A SPICE directive lifted out of the figure source (W1).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Directive {
    /// The directive text: the `.` line, trimmed, followed by any `+`
    /// continuation lines (each on its own line, trimmed).
    pub text: String,
    /// Where the `.` line is in the user source.
    pub span: PicSpan,
}

/// W1: blank every line whose first non-blank character is `.` —
/// except the `.PS`/`.PE` delimiters, which rpic handles — and every
/// `+` continuation line that directly follows one. Returns the source
/// to hand to rpic (same number of lines, so rpic's positions stay
/// valid for the user source) and the directives in source order.
pub(crate) fn strip_directives(src: &str) -> (String, Vec<Directive>) {
    let mut out = String::with_capacity(src.len());
    let mut directives: Vec<Directive> = Vec::new();
    let mut in_directive = false;
    for (i, raw) in src.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let lead = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        let is_dot = trimmed.starts_with('.') && !is_ps_pe(trimmed);
        let is_cont = in_directive && trimmed.starts_with('+');
        if i > 0 {
            out.push('\n');
        }
        if is_dot {
            let col = char_col(line, lead);
            directives.push(Directive {
                text: trimmed.to_string(),
                span: PicSpan {
                    file: None,
                    line: i as u32 + 1,
                    col,
                    end_col: col + trimmed.chars().count() as u32,
                },
            });
            in_directive = true;
        } else if is_cont {
            if let Some(d) = directives.last_mut() {
                d.text.push('\n');
                d.text.push_str(trimmed);
            }
        } else {
            in_directive = false;
            out.push_str(raw);
        }
    }
    (out, directives)
}

/// `.PS`/`.PE` (optionally followed by arguments) are pic's own
/// delimiters, not directives.
fn is_ps_pe(trimmed: &str) -> bool {
    ["PS", "PE"].iter().any(|k| {
        trimmed[1..].strip_prefix(k).is_some_and(|rest| {
            rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace() || c == '<')
        })
    })
}

/// 1-based character column of byte offset `byte` in `line`.
fn char_col(line: &str, byte: usize) -> u32 {
    line[..byte].chars().count() as u32 + 1
}

/// W3: the span of the statement that introduced block label `label`
/// for the `nth` time (0-based) — the `nth` `label :` at the start of a
/// statement (line start or after `;`) in the user source. `None` when
/// the label is built some other way (a loop, a macro); the diagnostic
/// then carries no position.
pub(crate) fn call_site(src: &str, label: &str, nth: usize) -> Option<PicSpan> {
    let mut seen = 0usize;
    for (i, line) in src.lines().enumerate() {
        for (pos, stmt) in split_statements(line) {
            let t = stmt.trim_start();
            let lead = stmt.len() - t.len();
            if let Some(rest) = t.strip_prefix(label)
                && rest.trim_start().starts_with(':')
                && !rest.trim_start().starts_with(":=")
            {
                if seen < nth {
                    seen += 1;
                    continue;
                }
                let col = char_col(line, pos + lead);
                return Some(PicSpan {
                    file: None,
                    line: i as u32 + 1,
                    col,
                    end_col: col + label.chars().count() as u32,
                });
            }
        }
    }
    None
}

/// Statements of one line as `(byte offset, text)`, split on `;`
/// outside string literals.
fn split_statements(line: &str) -> Vec<(usize, &str)> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut in_str = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_str = !in_str,
            ';' | '#' if !in_str => {
                parts.push((start, &line[start..i]));
                if c == '#' {
                    return parts;
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push((start, &line[start..]));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanks_directives_and_keeps_line_count() {
        let src = ".PS\nspice = 1\n  .tran 1u 1m\n.model D1 D(IS=1e-14\n+ RS=1)\nline\n.PE\n";
        let (out, dirs) = strip_directives(src);
        assert_eq!(out.split('\n').count(), src.split('\n').count());
        assert_eq!(out, ".PS\nspice = 1\n\n\n\nline\n.PE\n");
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[0].text, ".tran 1u 1m");
        assert_eq!((dirs[0].span.line, dirs[0].span.col), (3, 3));
        assert_eq!(dirs[0].span.end_col, 3 + 11);
        assert_eq!(dirs[1].text, ".model D1 D(IS=1e-14\n+ RS=1)");
        assert_eq!(dirs[1].span.line, 4);
    }

    #[test]
    fn ps_pe_with_arguments_stay() {
        let (out, dirs) = strip_directives(".PS 3\n.PE\n.PSX\n");
        assert_eq!(out, ".PS 3\n.PE\n\n");
        assert_eq!(dirs[0].text, ".PSX");
    }

    #[test]
    fn plus_line_without_directive_is_pic() {
        let (out, dirs) = strip_directives("x = 1\n+ 2\n");
        assert_eq!(out, "x = 1\n+ 2\n");
        assert!(dirs.is_empty());
    }

    #[test]
    fn finds_call_site() {
        let src = "A: (0,0)\nline; R1: resistor(A, B, 1k) # R1: no\n";
        let sp = call_site(src, "R1", 0).unwrap();
        assert_eq!((sp.line, sp.col, sp.end_col), (2, 7, 9));
        assert!(call_site(src, "R1", 1).is_none());
        assert!(call_site(src, "R2", 0).is_none());
        // A label that is only a prefix of another does not match.
        assert!(call_site("R10: x\n", "R1", 0).is_none());
        assert_eq!(call_site("A: x\nA: y\n", "A", 1).unwrap().line, 2);
    }
}
