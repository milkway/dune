//! The ngspice `.measure` printout, rendered over
//! [`dune_core::MeasureOutcome`]s — the result lines of
//! `get_measure2` (`frontend/com_measure2.c`, the `out_line`
//! formats, quirks included: `WHEN`, `RMS` and `INTEG` put three
//! blanks after the `=`) and the block framing of `do_measure`
//! (`frontend/measure.c`). Values are `%.5e` (ngspice's default
//! `measure_get_precision`).
//!
//! One block per measured plot, headed `Measurements for <X>
//! Analysis`; failed measurements print nothing on stdout and the
//! ngspice error lines on stderr.

use dune_core::{MeasClause, MeasDetail, MeasureError, MeasureOutcome, Solution};
use dune_netlist::ir::{MeasFunc, MeasStat, Measure};
use dune_raw::fmt_e;

/// ngspice's default measurement precision.
const PRECISION: usize = 5;

fn e(x: f64) -> String {
    fmt_e(x, PRECISION)
}

/// The analysis word of a plot's block header.
fn analysis_title(plotname: &str) -> &'static str {
    match plotname {
        "Transient Analysis" => "Transient",
        "AC Analysis" => "AC",
        "SP Analysis" => "SP",
        _ => "DC",
    }
}

/// One result line (`out_line` of `get_measure2`).
fn line(m: &Measure, value: f64, detail: MeasDetail) -> String {
    let name = &m.name;
    match (&m.func, detail) {
        (MeasFunc::Delay { .. }, MeasDetail::Delay { trig, targ }) => format!(
            "{name:<20}=  {} targ=  {} trig=  {}\n",
            e(value),
            e(targ),
            e(trig)
        ),
        (MeasFunc::When(_), _) => format!("{name:<20}=   {}\n", e(value)),
        (
            MeasFunc::Stat {
                stat: MeasStat::Rms | MeasStat::Integ,
                ..
            },
            MeasDetail::Window { from, to },
        ) => format!(
            "{name:<20}=   {} from=  {} to=  {}\n",
            e(value),
            e(from),
            e(to)
        ),
        (_, MeasDetail::Window { from, to }) => format!(
            "{name:<20}=  {} from=  {} to=  {}\n",
            e(value),
            e(from),
            e(to)
        ),
        (_, MeasDetail::At { at }) => format!("{name:<20}=  {} at=  {}\n", e(value), e(at)),
        (_, MeasDetail::With { with }) => {
            format!("{name:<20}=  {} with=  {}\n", e(value), e(with))
        }
        _ => format!("{name:<20}=  {}\n", e(value)),
    }
}

/// ngspice's failure report (`measure_errMessage` plus `do_measure`'s
/// "failed!" line).
fn failure(m: &Measure, err: &MeasureError) -> String {
    let what = match err {
        MeasureError::OutOfInterval { clause } => {
            let clause = match (&m.func, clause) {
                // ngspice tags every statistic's window failure TRIG.
                (MeasFunc::Stat { .. }, _) => MeasClause::Trig,
                (_, c) => *c,
            };
            format!("{}({}) : out of interval", m.keyword, clause.tag())
        }
        other => format!("{} : {other}", m.keyword),
    };
    format!(
        "\nError: measure  {}  {what}\n {} failed!\n\n",
        m.name, m.text
    )
}

/// Render every outcome: `(stdout, stderr)` text.
pub fn render(
    measures: &[Measure],
    solutions: &[Solution],
    outcomes: &[MeasureOutcome],
) -> (String, String) {
    let mut out = String::new();
    let mut err = String::new();
    let mut i = 0;
    while i < outcomes.len() {
        let plot = outcomes[i].plot;
        out.push_str(&format!(
            "\n  Measurements for {} Analysis\n\n",
            analysis_title(&solutions[plot].plotname)
        ));
        while i < outcomes.len() && outcomes[i].plot == plot {
            let o = &outcomes[i];
            let m = &measures[o.card];
            match &o.result {
                Ok(r) => out.push_str(&line(m, r.value, r.detail)),
                Err(e) => err.push_str(&failure(m, e)),
            }
            i += 1;
        }
        out.push('\n');
    }
    (out, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_core::Measurement;

    fn card(text: &str) -> Measure {
        let out = dune_netlist::parse_str("t.cir", &format!("t\nr1 1 0 1\n{text}\n.end\n"));
        out.netlist.measures.into_iter().next().expect(text)
    }

    #[test]
    fn line_formats_match_ngspice() {
        let cases = [
            (
                ".meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1",
                MeasDetail::Delay {
                    trig: 1.10587e-9,
                    targ: 3.30307e-9,
                },
                2.19721e-9,
                "trise               =  2.19721e-09 targ=  3.30307e-09 trig=  1.10587e-09\n",
            ),
            (
                ".meas tran tw when v(out)=0.5",
                MeasDetail::None,
                1.69365e-9,
                "tw                  =   1.69365e-09\n",
            ),
            (
                ".meas ac pmid find vp(out) at=1e8",
                MeasDetail::None,
                -5.60982e-1,
                "pmid                =  -5.60982e-01\n",
            ),
            (
                ".meas tran vmax max v(out)",
                MeasDetail::At { at: 1e-8 },
                9.99877e-1,
                "vmax                =  9.99877e-01 at=  1.00000e-08\n",
            ),
            (
                ".meas tran vrms rms v(out)",
                MeasDetail::Window {
                    from: 0.0,
                    to: 1e-8,
                },
                8.66012e-1,
                "vrms                =   8.66012e-01 from=  0.00000e+00 to=  1.00000e-08\n",
            ),
            (
                ".meas tran vavg avg v(out)",
                MeasDetail::Window {
                    from: 0.0,
                    to: 1e-8,
                },
                7.99963e-1,
                "vavg                =  7.99963e-01 from=  0.00000e+00 to=  1.00000e-08\n",
            ),
            (
                ".meas tran tmaxat max_at v(out)",
                MeasDetail::With { with: 9.99877e-1 },
                1e-8,
                "tmaxat              =  1.00000e-08 with=  9.99877e-01\n",
            ),
        ];
        for (text, detail, value, want) in cases {
            assert_eq!(line(&card(text), value, detail), want, "{text}");
        }
    }

    #[test]
    fn failures_and_blocks() {
        let m = card(".meas tran bad when v(out)=5");
        let sol = Solution {
            plotname: "Transient Analysis".into(),
            variables: vec![],
            data: dune_core::Data::Real(vec![]),
            scale: None,
        };
        let outcomes = [
            MeasureOutcome {
                card: 0,
                plot: 0,
                result: Err(MeasureError::OutOfInterval {
                    clause: MeasClause::When,
                }),
            },
            MeasureOutcome {
                card: 0,
                plot: 0,
                result: Ok(Measurement {
                    value: 1.0,
                    detail: MeasDetail::None,
                }),
            },
        ];
        let (out, err) = render(&[m], &[sol], &outcomes);
        assert_eq!(
            err,
            "\nError: measure  bad  when(WHEN) : out of interval\n .meas tran bad when v(out)=5 failed!\n\n"
        );
        assert_eq!(
            out,
            "\n  Measurements for Transient Analysis\n\nbad                 =   1.00000e+00\n\n"
        );
    }
}
