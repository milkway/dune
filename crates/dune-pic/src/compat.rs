//! The two source-text helpers dune-pic still needs on top of
//! rpic-core 0.13.0 (D-0020; the W1–W3 interims of 0.12.1 are retired:
//! directives come from `Drawing.passthrough`, net names from
//! `Drawing.places`, call sites from `BlockInfo.span`).
//!
//! * [`split_continuations`] — SPICE `+` continuation lines. rpic sets
//!   aside only the column-1 `.` line itself (the troff rule); the `+`
//!   lines that continue a `.model` are SPICE, not troff, and would be
//!   pic syntax errors. They are blanked here (line count preserved, so
//!   rpic's positions stay valid) and re-attached to their directive.
//! * [`raw_call_args`] — `MacroCall.args` is rpic's *normalized* token
//!   text (`1e-9` prints as `0.000000001`, `1.5e3` as `1500`), so the
//!   element value is re-read from the user's own text at the call span
//!   to pass it through verbatim (D-0004 §2).

/// A run of `+` lines that continues the column-1 `.` line on `head`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Continuation {
    /// 1-based line of the directive these lines continue.
    pub head: u32,
    /// The `+` lines, trimmed, in order.
    pub lines: Vec<String>,
}

/// Blank every column-1 `+` line that directly follows a column-1 `.`
/// line (or another such `+` line) — the same `.` rule rpic applies to
/// passthrough lines, `.PS`/`.PE` excluded. Returns the source to hand
/// to rpic (same number of lines) and the continuations found.
pub(crate) fn split_continuations(src: &str) -> (String, Vec<Continuation>) {
    let mut out = String::with_capacity(src.len());
    let mut conts: Vec<Continuation> = Vec::new();
    // The line of the directive the next `+` line would continue.
    let mut head: Option<u32> = None;
    for (i, raw) in src.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let n = i as u32 + 1;
        if i > 0 {
            out.push('\n');
        }
        if let Some(h) = head
            && line.starts_with('+')
        {
            match conts.last_mut() {
                Some(c) if c.head == h => c.lines.push(line.trim().to_string()),
                _ => conts.push(Continuation {
                    head: h,
                    lines: vec![line.trim().to_string()],
                }),
            }
            continue;
        }
        head = (line.starts_with('.') && !line.starts_with(".PS") && !line.starts_with(".PE"))
            .then_some(n);
        out.push_str(raw);
    }
    (out, conts)
}

/// The arguments of the macro call `name(…)` written in `text` (the
/// source slice a `BlockInfo.span` covers), verbatim and trimmed:
/// commas split only outside parentheses/brackets/braces and strings.
/// `None` when `text` is not a complete one-line `name(…)` call.
pub(crate) fn raw_call_args(text: &str, name: &str) -> Option<Vec<String>> {
    let rest = text.trim_start().strip_prefix(name)?.trim_start();
    let body = rest.strip_prefix('(')?;
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut prev = '\0';
    for c in body.chars() {
        if in_str {
            in_str = !(c == '"' && prev != '\\');
        } else {
            match c {
                '"' => in_str = true,
                '(' | '[' | '{' => depth += 1,
                ')' if depth == 0 => {
                    args.push(cur.trim().to_string());
                    return Some(args);
                }
                ')' | ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(std::mem::take(&mut cur).trim().to_string());
                    prev = c;
                    continue;
                }
                _ => {}
            }
        }
        cur.push(c);
        prev = c;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blanks_continuations_and_keeps_line_count() {
        let src = ".PS\n.model D1 D(IS=1e-14\n+ RS=1)\n+ N=2\nline\n+ 1\n.op\n.PE\n";
        let (out, conts) = split_continuations(src);
        assert_eq!(out.split('\n').count(), src.split('\n').count());
        assert_eq!(out, ".PS\n.model D1 D(IS=1e-14\n\n\nline\n+ 1\n.op\n.PE\n");
        assert_eq!(
            conts,
            [Continuation {
                head: 2,
                lines: vec!["+ RS=1)".into(), "+ N=2".into()]
            }]
        );
    }

    #[test]
    fn continuation_rule_is_column_one() {
        // An indented `.` is pic (rpic's rule too), so nothing follows it;
        // an indented `+` is not a continuation; `.PS` is not a directive.
        let (out, conts) = split_continuations("  .x\n+ 1\n.a\n  + 2\n.PS\n+ 3\n");
        assert_eq!(out, "  .x\n+ 1\n.a\n  + 2\n.PS\n+ 3\n");
        assert!(conts.is_empty());
    }

    #[test]
    fn raw_args_keep_the_source_text() {
        assert_eq!(
            raw_call_args("resistor(TL, TR, 1e-9)", "resistor").unwrap(),
            ["TL", "TR", "1e-9"]
        );
        assert_eq!(
            raw_call_args("vsource((0,1), B, PULSE(0 5 0, 1u) )", "vsource").unwrap(),
            ["(0,1)", "B", "PULSE(0 5 0, 1u)"]
        );
        assert_eq!(
            raw_call_args("m(\"a,)b\", {x,y}, [1,2])", "m").unwrap(),
            ["\"a,)b\"", "{x,y}", "[1,2]"]
        );
        assert!(raw_call_args("resistor(A, B,", "resistor").is_none());
        assert!(raw_call_args("capacitor(A, B, 1)", "resistor").is_none());
    }
}
