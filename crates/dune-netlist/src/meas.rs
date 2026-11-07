//! `.measure` / `.meas` cards: the ngspice-47 grammar
//! (`frontend/com_measure2.c`, `get_measure2` and the
//! `measure_parse_*` helpers; the 3f5 has no measurements).
//!
//! Words are cut like ngspice's `gettok` (blanks, and commas outside
//! parentheses, so `v(a,b)` stays one word) and an `=` with blanks
//! around it is glued to its neighbours (`measure_parse_line` glues
//! `val= 0.5`; we also accept `val =0.5` and `val = 0.5`). Then:
//!
//! ```text
//! .meas ANALYSIS NAME TRIG|DELAY out VAL= [TD= FROM= TO=] RISE|FALL|CROSS=n|LAST
//!                    | TRIG AT=x
//!                     TARG … (same shapes)
//! .meas ANALYSIS NAME WHEN out=val|out2 [TD= FROM= TO=] [RISE|FALL|CROSS=n|LAST]
//! .meas ANALYSIS NAME FIND|DERIV out AT=x
//! .meas ANALYSIS NAME FIND|DERIV out WHEN out2=val|out3 [… as WHEN]
//! .meas ANALYSIS NAME AVG|MIN|MAX|MIN_AT|MAX_AT|PP|RMS|INTEG out [FROM= TO=]
//! ```

use crate::diag::Diagnostic;
use crate::ir::{
    MeasAnalysis, MeasCount, MeasCross, MeasEdge, MeasFunc, MeasLevel, MeasPoint, MeasStat,
    MeasTrig, MeasWindow, Measure, OutVar,
};
use crate::span::Span;

/// A malformed measurement: the message and the byte range of the
/// offending word in the text that was parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct MeasSyntaxError {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

/// A remark on a well-formed measurement (a parameter ngspice
/// accepts but ignores), with its byte range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeasWarning {
    pub message: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
struct Word {
    /// Lowercased text, with glued `=` forms.
    text: String,
    start: usize,
    end: usize,
}

type PResult<T> = Result<T, MeasSyntaxError>;

fn err<T>(w: &Word, message: impl Into<String>) -> PResult<T> {
    Err(MeasSyntaxError {
        message: message.into(),
        start: w.start,
        end: w.end,
    })
}

/// `gettok` words with their byte ranges, `=` glued.
fn words(text: &str) -> Vec<Word> {
    let b = text.as_bytes();
    let mut raw: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b',') {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let start = i;
        let mut paren = 0i32;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            match b[i] {
                b'(' => paren += 1,
                b')' => paren -= 1,
                b',' if paren < 1 => break,
                _ => {}
            }
            i += 1;
        }
        if i > start {
            raw.push((start, i));
        }
    }
    let mut out: Vec<Word> = Vec::new();
    let mut k = 0;
    while k < raw.len() {
        let (start, mut end) = raw[k];
        let mut s = text[start..end].to_ascii_lowercase();
        k += 1;
        // Glue `a =b`, `a= b`, `a = b` into `a=b`.
        loop {
            let next_eq = k < raw.len() && text[raw[k].0..raw[k].1].starts_with('=');
            if s.ends_with('=') && k < raw.len() || next_eq && !s.contains('=') {
                s.push_str(&text[raw[k].0..raw[k].1].to_ascii_lowercase());
                end = raw[k].1;
                k += 1;
                if s.ends_with('=') && k < raw.len() {
                    continue;
                }
            }
            break;
        }
        out.push(Word {
            text: s,
            start,
            end,
        });
    }
    out
}

/// Parse a measurement body — the function keyword and what follows
/// it, as in `.meas tran NAME <body>` — e.g. `"trig v(1) val=0.5
/// rise=1 targ v(2) val=0.5 rise=1"` or `"max v(out) from=1n"`.
/// This is what the language bindings' direct measurement API takes.
///
/// On success also returns the remarks (parameters ngspice accepts
/// and ignores, such as `td=` on `avg`).
pub fn parse_measure_body(body: &str) -> Result<(MeasFunc, Vec<MeasWarning>), MeasSyntaxError> {
    let ws = words(body);
    let Some(first) = ws.first() else {
        return Err(MeasSyntaxError {
            message: "empty measurement".into(),
            start: 0,
            end: body.len(),
        });
    };
    let mut warnings = Vec::new();
    let func = parse_func(first, &ws[1..], &mut warnings)?;
    Ok((func, warnings))
}

/// Parse a whole `.measure` card (`.meas ANALYSIS NAME FUNCTION …`)
/// from its text; `span_of` maps byte ranges of `text` to source
/// spans.
pub(crate) fn parse_measure_card(
    text: &str,
    card_span: Span,
    span_of: &dyn Fn(usize, usize) -> Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<Measure> {
    let ws = words(text);
    let fail = |diags: &mut Vec<Diagnostic>, e: MeasSyntaxError| {
        diags.push(Diagnostic::error(
            format!(".measure: {}", e.message),
            span_of(e.start, e.end),
        ));
    };
    // ws[0] is `.meas`/`.measure`.
    let Some(an) = ws.get(1) else {
        diags.push(Diagnostic::error(
            ".measure needs an analysis (tran, ac, dc or sp), a name and a function",
            card_span,
        ));
        return None;
    };
    let analysis = match an.text.as_str() {
        "tran" => MeasAnalysis::Tran,
        "ac" => MeasAnalysis::Ac,
        "dc" => MeasAnalysis::Dc,
        "sp" => MeasAnalysis::Sp,
        other => {
            fail(
                diags,
                MeasSyntaxError {
                    message: format!("unrecognized analysis type `{other}` (tran, ac, dc or sp)"),
                    start: an.start,
                    end: an.end,
                },
            );
            return None;
        }
    };
    let (Some(name), Some(func_word)) = (ws.get(2), ws.get(3)) else {
        diags.push(Diagnostic::error(
            ".measure needs a result name and a function",
            card_span,
        ));
        return None;
    };
    if name.text.contains('=') {
        fail(
            diags,
            MeasSyntaxError {
                message: format!("`{}` is not a result name", name.text),
                start: name.start,
                end: name.end,
            },
        );
        return None;
    }
    let mut warnings = Vec::new();
    match parse_func(func_word, &ws[4..], &mut warnings) {
        Ok(func) => {
            for w in warnings {
                diags.push(Diagnostic::warning(
                    format!(".measure {}: {}", name.text, w.message),
                    span_of(w.start, w.end),
                ));
            }
            Some(Measure {
                analysis,
                name: name.text.clone(),
                func,
                keyword: func_word.text.clone(),
                text: text.to_string(),
                span: card_span,
            })
        }
        Err(e) => {
            fail(diags, e);
            None
        }
    }
}

/// `measure_function_type`, then the per-function parse.
fn parse_func(fw: &Word, rest: &[Word], warnings: &mut Vec<MeasWarning>) -> PResult<MeasFunc> {
    let st = match fw.text.as_str() {
        "trig" | "delay" | "targ" => return parse_delay(fw, rest),
        "when" => {
            let Some(cond) = rest.first() else {
                return err(fw, "when needs `out=value` or `out=out2`");
            };
            return Ok(MeasFunc::When(parse_when(cond, &rest[1..])?));
        }
        "find" | "deriv" | "derivative" => {
            let Some(var_w) = rest.first() else {
                return err(fw, format!("{} needs an output variable", fw.text));
            };
            let var = outvar(var_w)?;
            let at = parse_point(fw, var_w, &rest[1..])?;
            return Ok(if fw.text == "find" {
                MeasFunc::Find { var, at }
            } else {
                MeasFunc::Deriv { var, at }
            });
        }
        "avg" => Some(MeasStat::Avg),
        "min" => Some(MeasStat::Min),
        "max" => Some(MeasStat::Max),
        "min_at" => Some(MeasStat::MinAt),
        "max_at" => Some(MeasStat::MaxAt),
        "pp" => Some(MeasStat::Pp),
        "rms" => Some(MeasStat::Rms),
        "integ" | "integral" => Some(MeasStat::Integ),
        t if t.starts_with("param") || t.starts_with("expr") => {
            return err(
                fw,
                "`param`/`expr` measurements (expressions) are not supported yet",
            );
        }
        "err" | "err1" | "err2" | "err3" => {
            return err(
                fw,
                format!(
                    "function `{}` is not supported (ngspice-47 does not implement it either)",
                    fw.text
                ),
            );
        }
        _ => None,
    };
    let Some(stat) = st else {
        return err(fw, format!("no such function as `{}`", fw.text));
    };
    let Some(var_w) = rest.first() else {
        return err(fw, format!("{} needs an output variable", fw.text));
    };
    let var = outvar(var_w)?;
    let p = std_params(&rest[1..])?;
    for (key, w) in &p.given {
        if !matches!(key.as_str(), "from" | "to") {
            warnings.push(MeasWarning {
                message: format!("`{key}` has no effect on {} (as in ngspice)", fw.text),
                start: w.start,
                end: w.end,
            });
        }
    }
    Ok(MeasFunc::Stat {
        stat,
        var,
        window: p.window,
    })
}

/// `TRIG … TARG …`.
fn parse_delay(fw: &Word, rest: &[Word]) -> PResult<MeasFunc> {
    let Some(targ_at) = rest.iter().position(|w| w.text == "targ") else {
        return err(fw, "trig needs a matching targ clause");
    };
    let trig = parse_trigtarg(fw, &rest[..targ_at], "trig")?;
    let targ = parse_trigtarg(&rest[targ_at], &rest[targ_at + 1..], "targ")?;
    Ok(MeasFunc::Delay { trig, targ })
}

/// `measure_parse_trigtarg`: `out VAL= … RISE=n` or `AT=x`.
fn parse_trigtarg(kw: &Word, ws: &[Word], what: &str) -> PResult<MeasTrig> {
    let Some(first) = ws.first() else {
        return err(kw, format!("bad syntax of `{what}`: nothing follows it"));
    };
    let (var, params) = if first.text.starts_with("at") && first.text.contains('=') {
        (None, ws)
    } else {
        (Some(outvar(first)?), &ws[1..])
    };
    let p = std_params(params)?;
    if let Some(at) = p.at {
        return Ok(MeasTrig::At(at));
    }
    let Some(var) = var else {
        return err(first, format!("{what}: missing output variable"));
    };
    if p.edge == MeasEdge::Any {
        let w = ws.last().unwrap_or(first);
        return err(w, format!("{what}: at, rise, fall or cross must be given"));
    }
    Ok(MeasTrig::Cross(MeasCross {
        var,
        // ngspice's TMALLOC'd (zeroed) structure: VAL defaults to 0.
        level: MeasLevel::Value(p.val.unwrap_or(0.0)),
        edge: p.edge,
        window: p.window,
    }))
}

/// `measure_parse_when`: `out=val` or `out=out2`, then the standard
/// parameters.
fn parse_when(cond: &Word, rest: &[Word]) -> PResult<MeasCross> {
    let Some((lhs, rhs)) = cond.text.split_once('=') else {
        return err(
            cond,
            "bad syntax of when: expected `out=value` or `out=out2`",
        );
    };
    if lhs.is_empty() || rhs.is_empty() {
        return err(
            cond,
            "bad syntax of when: expected `out=value` or `out=out2`",
        );
    }
    let var = outvar_text(lhs, cond)?;
    let level = match crate::scan::evaluate(rhs) {
        Some(v) if !has_arith(rhs) => MeasLevel::Value(v),
        _ if has_arith(rhs) => {
            return err(cond, format!("expressions like `{rhs}` are not supported"));
        }
        _ => MeasLevel::Var(outvar_text(rhs, cond)?),
    };
    let p = std_params(rest)?;
    Ok(MeasCross {
        var,
        level,
        edge: p.edge,
        window: p.window,
    })
}

/// After `FIND out` / `DERIV out`: `AT=x` or `WHEN …`.
fn parse_point(fw: &Word, var_w: &Word, rest: &[Word]) -> PResult<MeasPoint> {
    let Some(next) = rest.first() else {
        return err(
            var_w,
            format!("{} needs `at=` or `when` after the variable", fw.text),
        );
    };
    if next.text == "when" {
        let Some(cond) = rest.get(1) else {
            return err(next, "when needs `out=value` or `out=out2`");
        };
        return Ok(MeasPoint::When(parse_when(cond, &rest[2..])?));
    }
    if next.text.starts_with("at=") {
        let p = std_params(rest)?;
        if let Some(at) = p.at {
            return Ok(MeasPoint::At(at));
        }
    }
    err(
        next,
        format!("{} needs `at=` or `when` after the variable", fw.text),
    )
}

/// `str_has_arith_char2`.
fn has_arith(s: &str) -> bool {
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    s.chars().any(|c| "*/<>?:|&^!%\\".contains(c))
}

fn outvar(w: &Word) -> PResult<OutVar> {
    if w.text.contains('=') {
        return err(w, format!("expected an output variable, got `{}`", w.text));
    }
    outvar_text(&w.text, w)
}

fn outvar_text(text: &str, w: &Word) -> PResult<OutVar> {
    let text = text.trim_matches(['"', '\'']);
    if text.is_empty() {
        return err(w, "missing output variable");
    }
    let mut d = Vec::new();
    let v = crate::dot::parse_outvar(text, Span::new(crate::span::FileId(0), 0, 0), &mut d);
    if let Some(first) = d.first() {
        return err(w, first.message.clone());
    }
    Ok(v)
}

/// What `measure_parse_stdParams` collects.
struct StdParams {
    edge: MeasEdge,
    val: Option<f64>,
    at: Option<f64>,
    window: MeasWindow,
    /// Every key given, with its word (for "no effect" remarks).
    given: Vec<(String, Word)>,
}

/// `measure_parse_stdParams`: `NAME=VALUE` words (`RISE FALL CROSS
/// VAL TD FROM TO AT`) and a bare `LAST` (= `CROSS=LAST`). The last
/// of RISE/FALL/CROSS given wins. Counts round like the C
/// (`floor(v + 0.5)`) and must be at least 1.
fn std_params(ws: &[Word]) -> PResult<StdParams> {
    let mut p = StdParams {
        edge: MeasEdge::Any,
        val: None,
        at: None,
        window: MeasWindow::default(),
        given: Vec::new(),
    };
    for w in ws {
        let Some((name, value)) = w.text.split_once('=') else {
            if w.text == "last" {
                p.edge = MeasEdge::Cross(MeasCount::Last);
                p.given.push(("cross".into(), w.clone()));
                continue;
            }
            return err(
                w,
                format!("bad syntax at `{}`: equal sign missing?", w.text),
            );
        };
        if value.is_empty() {
            return err(w, format!("missing value for `{name}`"));
        }
        let count = |w: &Word| -> PResult<MeasCount> {
            if value == "last" {
                return Ok(MeasCount::Last);
            }
            let Some(v) = crate::scan::evaluate(value) else {
                return err(w, format!("cannot evaluate `{value}` for `{name}`"));
            };
            let n = (v + 0.5).floor();
            if !(1.0..=f64::from(u32::MAX)).contains(&n) {
                return err(
                    w,
                    format!("`{name}` must be a count of at least 1 or `last`"),
                );
            }
            Ok(MeasCount::Nth(n as u32))
        };
        let num = |w: &Word| -> PResult<f64> {
            match crate::scan::evaluate(value) {
                Some(v) if v.is_finite() => Ok(v),
                _ => err(w, format!("cannot evaluate `{value}` for `{name}`")),
            }
        };
        match name {
            "rise" => p.edge = MeasEdge::Rise(count(w)?),
            "fall" => p.edge = MeasEdge::Fall(count(w)?),
            "cross" => p.edge = MeasEdge::Cross(count(w)?),
            "val" => p.val = Some(num(w)?),
            "td" => p.window.td = Some(num(w)?),
            "from" => p.window.from = Some(num(w)?),
            "to" => p.window.to = Some(num(w)?),
            "at" => p.at = Some(num(w)?),
            _ => return err(w, format!("no such parameter as `{name}`")),
        }
        p.given.push((name.to_string(), w.clone()));
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::OutVar;

    fn v(node: &str) -> OutVar {
        OutVar::Voltage {
            node: node.into(),
            reference: None,
            suffix: None,
        }
    }

    fn body(s: &str) -> MeasFunc {
        parse_measure_body(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
            .0
    }

    #[test]
    fn words_glue_equals_and_keep_parens() {
        let w: Vec<String> = words("WHEN v(a,b) = 0.5 val= 1 td =2n, rise=LAST")
            .into_iter()
            .map(|w| w.text)
            .collect();
        assert_eq!(w, ["when", "v(a,b)=0.5", "val=1", "td=2n", "rise=last"]);
    }

    #[test]
    fn delay_card() {
        let f = body("trig v(1) val=0.1 rise=1 targ v(2) val=0.9 fall=2 td=1n");
        let MeasFunc::Delay { trig, targ } = f else {
            panic!()
        };
        assert_eq!(
            trig,
            MeasTrig::Cross(MeasCross {
                var: v("1"),
                level: MeasLevel::Value(0.1),
                edge: MeasEdge::Rise(MeasCount::Nth(1)),
                window: MeasWindow::default(),
            })
        );
        let MeasTrig::Cross(t) = targ else { panic!() };
        assert_eq!(t.edge, MeasEdge::Fall(MeasCount::Nth(2)));
        assert!((t.window.td.unwrap() - 1e-9).abs() < 1e-24);
        let f = body("trig at=2n targ v(2) val=0.5 cross=last");
        let MeasFunc::Delay { trig, targ } = f else {
            panic!()
        };
        assert_eq!(trig, MeasTrig::At(2e-9));
        let MeasTrig::Cross(t) = targ else { panic!() };
        assert_eq!(t.edge, MeasEdge::Cross(MeasCount::Last));
    }

    #[test]
    fn when_find_deriv_and_stats() {
        let MeasFunc::When(c) = body("when v(out)=v(ref) cross=2 from=1u") else {
            panic!()
        };
        assert_eq!(c.level, MeasLevel::Var(v("ref")));
        assert_eq!(c.edge, MeasEdge::Cross(MeasCount::Nth(2)));
        let MeasFunc::Find { at, .. } = body("find i(vin) at=5u") else {
            panic!()
        };
        let MeasPoint::At(x) = at else { panic!() };
        assert!((x - 5e-6).abs() < 1e-20);
        let MeasFunc::Find { var, at } = body("find vdb(out) when vp(out)=-0.785 last") else {
            panic!()
        };
        assert!(matches!(
            var,
            OutVar::Voltage {
                suffix: Some(_),
                ..
            }
        ));
        let MeasPoint::When(c) = at else { panic!() };
        assert_eq!(c.edge, MeasEdge::Cross(MeasCount::Last));
        assert!(matches!(
            body("deriv v(2) at=1m"),
            MeasFunc::Deriv {
                at: MeasPoint::At(_),
                ..
            }
        ));
        let MeasFunc::Stat { stat, window, .. } = body("INTEGRAL v(2) from=1u to=2u") else {
            panic!()
        };
        assert_eq!(stat, MeasStat::Integ);
        assert_eq!(window.to, Some(2e-6));
        let (_, warns) = parse_measure_body("avg v(2) td=1u").unwrap();
        assert_eq!(warns.len(), 1, "{warns:?}");
    }

    #[test]
    fn malformed_bodies_point_at_the_word() {
        let cases = [
            ("trig v(1) val=0.5 rise=1", "targ"),
            (
                "trig v(1) val=0.5 targ v(2) val=1 rise=1",
                "rise, fall or cross",
            ),
            ("when v(1)", "out=value"),
            ("when v(1)=2*v(3)", "expressions"),
            ("find v(1)", "at="),
            ("max", "output variable"),
            ("max v(1) bogus=3", "no such parameter"),
            ("max v(1) from", "equal sign"),
            ("when v(1)=0.5 rise=0", "at least 1"),
            ("when v(1)=0.5 rise=x", "cannot evaluate"),
            ("frobnicate v(1)", "no such function"),
            ("param='a+b'", "not supported"),
            ("deriv v(1) at=", "missing value"),
            ("", "empty"),
        ];
        for (text, want) in cases {
            let e = parse_measure_body(text).expect_err(text);
            assert!(e.message.contains(want), "{text}: {}", e.message);
            assert!(e.end <= text.len() && e.start <= e.end, "{text}: {e:?}");
        }
        let e = parse_measure_body("max v(1) bogus=3").unwrap_err();
        assert_eq!((e.start, e.end), (9, 16));
    }
}
