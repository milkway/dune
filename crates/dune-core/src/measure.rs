//! `.measure`: scalar measurements over a computed plot — a pure
//! post-processing layer (data in, scalars out) that never touches
//! the simulation. The CLI prints it in ngspice's format; the
//! Python, R and JS bindings hand the same [`Measurement`]s back.
//!
//! The contract is ngspice-47's `frontend/com_measure2.c` (the 3f5
//! has no measurements): the same crossing state machine
//! (`com_measure_when`), linear interpolation (`measure_at`),
//! trapezoidal average without window interpolation
//! (`measure_minMaxAvg`) and the Simpson 3/8 – 1/3 – trapezoid
//! integral with interpolated window edges (`measure_rms_integral`),
//! with the analysis-specific windows (`TD`/`FROM`/`TO` defaults;
//! `.dc` windows are inclusive and may run backwards). Deliberate
//! departures, each in `corpus/KNOWN_DIFFS.md`:
//!
//! * a crossing inside the first segment of the window counts and is
//!   interpolated there (ngspice counts it during its state
//!   initialisation but never reports it, or extrapolates it from the
//!   next segment);
//! * `DERIV` is implemented (ngspice-47: "currently not supported");
//! * in `WHEN v(a)=v(b)` each side keeps its own part (`vdb`, `vp`…;
//!   ngspice applies the first side's part to both).
//!
//! Notes on the design are in `docs/dune-measure.md`.

use dune_netlist::ir::{
    MeasCount, MeasCross, MeasEdge, MeasFunc, MeasLevel, MeasPoint, MeasStat, MeasTrig, MeasWindow,
    Measure, OutVar,
};

use crate::select::{ColData, OutExpr};
use crate::solution::Solution;

/// The ngspice "unset" `AT` and the `.dc` window bounds.
const DC_FAR: f64 = 1e99;

/// What a successful measurement produced. `value` is the measure's
/// result (what ngspice prints first and stores as the parameter);
/// `detail` carries the extra numbers ngspice reports beside it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measurement {
    pub value: f64,
    pub detail: MeasDetail,
}

/// The extras ngspice prints after the value, per function.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeasDetail {
    /// `WHEN`, `FIND`, `DERIV`: nothing more.
    None,
    /// `TRIG/TARG`: `value = targ − trig`.
    Delay { trig: f64, targ: f64 },
    /// `MIN`/`MAX`: the scale value of the extremum (`at=`).
    At { at: f64 },
    /// `MIN_AT`/`MAX_AT`: `value` is the scale value, `with` the
    /// extremum (`with=`).
    With { with: f64 },
    /// `AVG`, `RMS`, `INTEG`, `PP`: the window ngspice reports. For
    /// `RMS`/`INTEG` it is the integrated interval (edges
    /// interpolated); for `AVG` `from` is the `FROM=` given (or its
    /// default) and `to` the last scale point read; for `PP` both
    /// are as given (`0` = unset in `.tran`/`.ac`) — all as ngspice
    /// prints them.
    Window { from: f64, to: f64 },
}

/// Which clause of the card a failure belongs to (ngspice's
/// `measure  NAME  FUNC(CLAUSE)` message).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasClause {
    Trig,
    Targ,
    When,
    At,
}

impl MeasClause {
    /// The upper-case tag ngspice prints.
    pub fn tag(self) -> &'static str {
        match self {
            MeasClause::Trig => "TRIG",
            MeasClause::Targ => "TARG",
            MeasClause::When => "WHEN",
            MeasClause::At => "AT",
        }
    }
}

/// Why a measurement has no value.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MeasureError {
    /// The event never happens in the plot (or the window is empty):
    /// ngspice's "out of interval".
    #[error("out of interval")]
    OutOfInterval { clause: MeasClause },
    /// The output variable is not in the plot.
    #[error("no such vector as `{0}`")]
    UnknownVector(String),
    /// Measurements need a `.tran`, `.ac`, `.dc` or `.sp` plot.
    #[error("measure limited to tran, dc, sp, or ac analysis (plot `{0}`)")]
    UnsupportedPlot(String),
}

/// The analysis family of a plot, which decides the window rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Tran,
    /// `.ac` and `.sp`.
    Ac,
    Dc,
}

fn plot_kind(sol: &Solution) -> Result<Kind, MeasureError> {
    let kind = match sol.plotname.as_str() {
        "Transient Analysis" => Kind::Tran,
        "AC Analysis" | "SP Analysis" => Kind::Ac,
        "DC transfer characteristic" => Kind::Dc,
        other => return Err(MeasureError::UnsupportedPlot(other.to_string())),
    };
    if sol.scale.is_none() {
        return Err(MeasureError::UnsupportedPlot(sol.plotname.clone()));
    }
    Ok(kind)
}

/// Does a `.measure` card of `analysis` apply to this plot?
pub fn applies_to(analysis: dune_netlist::ir::MeasAnalysis, sol: &Solution) -> bool {
    use dune_netlist::ir::MeasAnalysis as A;
    matches!(
        (analysis, sol.plotname.as_str()),
        (A::Tran, "Transient Analysis")
            | (A::Ac, "AC Analysis")
            | (A::Sp, "SP Analysis")
            | (A::Dc, "DC transfer characteristic")
    )
}

/// The plot's data, resolved once per measurement.
struct Plot<'a> {
    sol: &'a Solution,
    kind: Kind,
    x: Vec<f64>,
}

impl<'a> Plot<'a> {
    fn new(sol: &'a Solution) -> Result<Self, MeasureError> {
        let kind = plot_kind(sol)?;
        let sel = sol
            .select(&[])
            .map_err(|e| MeasureError::UnknownVector(e.to_string()))?;
        let x = match sel.scale.map(|c| c.data) {
            Some(ColData::Real(v)) => v,
            Some(ColData::Complex(v)) => v.iter().map(|z| z.re).collect(),
            None => return Err(MeasureError::UnsupportedPlot(sol.plotname.clone())),
        };
        Ok(Plot { sol, kind, x })
    }

    /// One real trace: complex vectors without a part read their
    /// real part (`get_value`'s default).
    fn trace(&self, var: &OutVar) -> Result<Vec<f64>, MeasureError> {
        let expr = OutExpr::from_ir(var);
        let label = expr.label();
        let sel = self
            .sol
            .select(std::slice::from_ref(&expr))
            .map_err(|_| MeasureError::UnknownVector(label.clone()))?;
        let Some(col) = sel.columns.into_iter().next() else {
            return Err(MeasureError::UnknownVector(label));
        };
        Ok(match col.data {
            ColData::Real(v) => v,
            ColData::Complex(v) => v.iter().map(|z| z.re).collect(),
        })
    }
}

impl Solution {
    /// Evaluate one measurement on this plot (pure: the solution is
    /// only read). `func` comes from a `.measure` card
    /// ([`dune_netlist::ir::Measure::func`]) or from
    /// [`dune_netlist::parse_measure_body`].
    pub fn measure(&self, func: &MeasFunc) -> Result<Measurement, MeasureError> {
        let p = Plot::new(self)?;
        match func {
            MeasFunc::Delay { trig, targ } => delay(&p, trig, targ),
            MeasFunc::When(c) => {
                let t = when(&p, c)?.ok_or(MeasureError::OutOfInterval {
                    clause: MeasClause::When,
                })?;
                Ok(Measurement {
                    value: t,
                    detail: MeasDetail::None,
                })
            }
            MeasFunc::Find { var, at } => {
                let y = p.trace(var)?;
                let x = point(&p, at)?;
                let v = value_at(&p, &y, x).ok_or(MeasureError::OutOfInterval {
                    clause: MeasClause::At,
                })?;
                Ok(Measurement {
                    value: v,
                    detail: MeasDetail::None,
                })
            }
            MeasFunc::Deriv { var, at } => {
                let y = p.trace(var)?;
                let x = point(&p, at)?;
                let d = derivative(&p.x, &y);
                let v = value_at(&p, &d, x).filter(|v| !v.is_nan()).ok_or(
                    MeasureError::OutOfInterval {
                        clause: MeasClause::At,
                    },
                )?;
                Ok(Measurement {
                    value: v,
                    detail: MeasDetail::None,
                })
            }
            MeasFunc::Stat { stat, var, window } => {
                let y = p.trace(var)?;
                stat_measure(&p, *stat, &y, window)
            }
        }
    }
}

/// One `.measure` card evaluated on one plot.
#[derive(Clone, Debug)]
pub struct MeasureOutcome {
    /// Index of the card in [`dune_netlist::ir::Netlist::measures`].
    pub card: usize,
    /// Index of the plot it was evaluated on.
    pub plot: usize,
    pub result: Result<Measurement, MeasureError>,
}

/// Evaluate a deck's `.measure` cards: each against every plot of
/// its analysis, in plot order then card order (what `dune -b`
/// prints and what the bindings return).
pub fn measure_all(measures: &[Measure], solutions: &[Solution]) -> Vec<MeasureOutcome> {
    let mut out = Vec::new();
    for (pi, sol) in solutions.iter().enumerate() {
        for (ci, m) in measures.iter().enumerate() {
            if applies_to(m.analysis, sol) {
                out.push(MeasureOutcome {
                    card: ci,
                    plot: pi,
                    result: sol.measure(&m.func),
                });
            }
        }
    }
    out
}

/// `TRIG … TARG …` (the `AT_DELAY` case of `get_measure2`).
fn delay(p: &Plot, trig: &MeasTrig, targ: &MeasTrig) -> Result<Measurement, MeasureError> {
    // "If there was a FROM propagate trig<->targ."
    let mut trig = trig.clone();
    let mut targ = targ.clone();
    if let (MeasTrig::Cross(a), MeasTrig::Cross(b)) = (&mut trig, &mut targ) {
        match (a.window.from, b.window.from) {
            (Some(f), None) => b.window.from = Some(f),
            (None, Some(f)) => a.window.from = Some(f),
            _ => {}
        }
    }
    let side = |t: &MeasTrig, clause| -> Result<f64, MeasureError> {
        match t {
            MeasTrig::At(x) => Ok(*x),
            MeasTrig::Cross(c) => when(p, c)?.ok_or(MeasureError::OutOfInterval { clause }),
        }
    };
    let t0 = side(&trig, MeasClause::Trig)?;
    let t1 = side(&targ, MeasClause::Targ)?;
    Ok(Measurement {
        value: t1 - t0,
        detail: MeasDetail::Delay { trig: t0, targ: t1 },
    })
}

/// The scale value `FIND`/`DERIV` read at.
fn point(p: &Plot, at: &MeasPoint) -> Result<f64, MeasureError> {
    match at {
        MeasPoint::At(x) => Ok(*x),
        MeasPoint::When(c) => when(p, c)?.ok_or(MeasureError::OutOfInterval {
            clause: MeasClause::At,
        }),
    }
}

/// The resolved window (ngspice's per-analysis sentinels: `TD = 0`,
/// `FROM = 0`, `TO = 0` meaning "no end" for `.tran`/`.ac`;
/// `FROM = −1e99`, `TO = 1e99`, swapped into order, for `.dc`).
#[derive(Clone, Copy)]
struct Win {
    td: f64,
    from: f64,
    to: f64,
}

fn window(kind: Kind, w: &MeasWindow) -> Win {
    match kind {
        Kind::Dc => {
            let mut from = w.from.unwrap_or(-DC_FAR);
            let mut to = w.to.unwrap_or(DC_FAR);
            if to < from {
                std::mem::swap(&mut from, &mut to);
            }
            Win { td: 0.0, from, to }
        }
        _ => Win {
            td: w.td.unwrap_or(0.0),
            from: w.from.unwrap_or(0.0),
            to: w.to.unwrap_or(0.0),
        },
    }
}

/// Where a scale point falls relative to the window.
enum Gate {
    Skip,
    Take,
    Stop,
}

/// The window test of `measure_minMaxAvg`/`com_measure_when`.
fn gate(kind: Kind, w: &Win, x: f64) -> Gate {
    match kind {
        Kind::Dc => {
            if x < w.from || x > w.to {
                Gate::Skip
            } else {
                Gate::Take
            }
        }
        _ => {
            if x < w.from {
                Gate::Skip
            } else if w.to != 0.0 && x > w.to {
                Gate::Stop
            } else {
                Gate::Take
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Above,
    Below,
}

/// `com_measure_when`: the scale value at which `var` crosses its
/// level, by the requested edge; `None` when it never does in the
/// window.
fn when(p: &Plot, c: &MeasCross) -> Result<Option<f64>, MeasureError> {
    let y = p.trace(&c.var)?;
    let y2 = match &c.level {
        MeasLevel::Value(_) => None,
        MeasLevel::Var(v) => Some(p.trace(v)?),
    };
    let level = match c.level {
        MeasLevel::Value(v) => v,
        MeasLevel::Var(_) => f64::NAN,
    };
    let w = window(p.kind, &c.window);
    // ngspice's counters: MEASURE_DEFAULT (-1) when unused.
    let (want_rise, want_fall, want_cross) = match c.edge {
        MeasEdge::Any => (None, None, None),
        MeasEdge::Rise(n) => (Some(n), None, None),
        MeasEdge::Fall(n) => (None, Some(n), None),
        MeasEdge::Cross(n) => (None, None, Some(n)),
    };
    let last_rise = want_rise == Some(MeasCount::Last);
    let last_fall = want_fall == Some(MeasCount::Last);
    let any_last = matches!(
        c.edge,
        MeasEdge::Rise(MeasCount::Last)
            | MeasEdge::Fall(MeasCount::Last)
            | MeasEdge::Cross(MeasCount::Last)
    );
    let nth = |want: Option<MeasCount>, n: u32| want == Some(MeasCount::Nth(n));

    let origin = p.x.first().copied().unwrap_or(0.0);
    let (mut rise, mut fall, mut cross) = (0u32, 0u32, 0u32);
    let mut section = Side::Above;
    let mut started = false;
    let (mut px, mut pv, mut pv2) = (0.0, 0.0, 0.0);
    let mut last: Option<f64> = None;

    for i in 0..y.len().min(p.x.len()) {
        let x = p.x[i];
        let v = y[i];
        let v2 = y2.as_ref().map_or(level, |y2| y2[i]);
        match p.kind {
            Kind::Tran if x < w.td => continue,
            Kind::Ac if x < 0.0 => continue,
            _ => {}
        }
        match gate(p.kind, &w, x) {
            Gate::Skip => continue,
            Gate::Stop => break,
            Gate::Take => {}
        }
        // `.dc`: a nested sweep jumping back to its origin starts a
        // new trace (ngspice resets the cross counter only).
        if started && p.kind == Kind::Dc && x == origin {
            started = false;
            cross = 0;
        }
        if !started {
            section = if v < v2 { Side::Below } else { Side::Above };
            started = true;
            (px, pv, pv2) = (x, v, v2);
            continue;
        }
        let mut pending = false;
        if section == Side::Below && v >= v2 {
            section = Side::Above;
            cross += 1;
            rise += 1;
            pending = !last_fall;
        } else if section == Side::Above && v <= v2 {
            section = Side::Below;
            cross += 1;
            fall += 1;
            pending = !last_rise;
        }
        let interp = || match &y2 {
            None => px + (level - pv) * (x - px) / (v - pv),
            Some(_) => px + (pv2 - pv) * (x - px) / (v - pv - v2 + pv2),
        };
        // The C checks the exact count at every point; counts only
        // move on transitions, so it fires on the one that reached it.
        if nth(want_cross, cross) || nth(want_rise, rise) || nth(want_fall, fall) {
            return Ok(finite(interp()));
        }
        if pending {
            if c.edge == MeasEdge::Any {
                return Ok(finite(interp()));
            }
            if any_last {
                last = Some(interp());
            }
        }
        (px, pv, pv2) = (x, v, v2);
    }
    Ok(last.and_then(finite))
}

/// A crossing interpolated across a flat stretch (`v == level` on
/// both ends: the C's `<=`/`>=` flip-flop on equality) is 0/0;
/// ngspice reports it as "out of interval", and so do we.
fn finite(x: f64) -> Option<f64> {
    (!x.is_nan()).then_some(x)
}

/// `measure_at`: linear interpolation of `y` at scale value `at`
/// (a `.dc` sweep may run backwards).
fn value_at(p: &Plot, y: &[f64], at: f64) -> Option<f64> {
    let n = y.len().min(p.x.len());
    for i in 1..n {
        let (px, x) = (p.x[i - 1], p.x[i]);
        let forward = px <= at && x >= at;
        let backward = p.kind == Kind::Dc && px >= at && x <= at;
        if forward || backward {
            let (pv, v) = (y[i - 1], y[i]);
            if x == px {
                return Some(v);
            }
            return Some(pv + (at - px) * (v - pv) / (x - px));
        }
    }
    None
}

/// Pointwise d(y)/d(x): the three-point formula on the (possibly
/// non-uniform) grid inside, one-sided differences at the ends — the
/// derivative of the parabola through each sample and its
/// neighbours, second-order accurate. `DERIV` interpolates it
/// linearly like `FIND` interpolates values.
fn derivative(x: &[f64], y: &[f64]) -> Vec<f64> {
    let n = x.len().min(y.len());
    let slope = |a: usize, b: usize| {
        let h = x[b] - x[a];
        if h == 0.0 {
            f64::NAN
        } else {
            (y[b] - y[a]) / h
        }
    };
    (0..n)
        .map(|i| {
            if n < 2 {
                return f64::NAN;
            }
            if i == 0 {
                return slope(0, 1);
            }
            if i == n - 1 {
                return slope(n - 2, n - 1);
            }
            let h0 = x[i] - x[i - 1];
            let h1 = x[i + 1] - x[i];
            if h0 == 0.0 {
                return slope(i, i + 1);
            }
            if h1 == 0.0 || h0 + h1 == 0.0 {
                return slope(i - 1, i);
            }
            (h0 * h0 * y[i + 1] - h1 * h1 * y[i - 1] + (h1 * h1 - h0 * h0) * y[i])
                / (h0 * h1 * (h0 + h1))
        })
        .collect()
}

fn stat_measure(
    p: &Plot,
    stat: MeasStat,
    y: &[f64],
    w: &MeasWindow,
) -> Result<Measurement, MeasureError> {
    let win = window(p.kind, w);
    let oob = MeasureError::OutOfInterval {
        clause: MeasClause::Trig,
    };
    match stat {
        MeasStat::Min | MeasStat::Max | MeasStat::MinAt | MeasStat::MaxAt => {
            let max = matches!(stat, MeasStat::Max | MeasStat::MaxAt);
            let (m, at) = extremum(p, y, &win, max).ok_or(oob)?;
            Ok(match stat {
                MeasStat::Min | MeasStat::Max => Measurement {
                    value: m,
                    detail: MeasDetail::At { at },
                },
                _ => Measurement {
                    value: at,
                    detail: MeasDetail::With { with: m },
                },
            })
        }
        MeasStat::Pp => {
            let (lo, _) = extremum(p, y, &win, false).ok_or(oob.clone())?;
            let (hi, _) = extremum(p, y, &win, true).ok_or(oob)?;
            Ok(Measurement {
                value: hi - lo,
                detail: MeasDetail::Window {
                    from: win.from,
                    to: win.to,
                },
            })
        }
        MeasStat::Avg => {
            let (avg, to) = average(p, y, &win);
            let avg = avg.ok_or(oob)?;
            Ok(Measurement {
                value: avg,
                detail: MeasDetail::Window { from: win.from, to },
            })
        }
        MeasStat::Rms | MeasStat::Integ => {
            let (v, from, to) = rms_integral(p, y, &win, stat == MeasStat::Rms);
            if v.is_nan() {
                return Err(oob);
            }
            Ok(Measurement {
                value: v,
                detail: MeasDetail::Window { from, to },
            })
        }
    }
}

/// `measure_minMaxAvg` for MIN/MAX: the last extremum wins ties
/// (`<=`/`>=`), as in the C. `(value, scale)`.
fn extremum(p: &Plot, y: &[f64], w: &Win, max: bool) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for (&x, &v) in p.x.iter().zip(y) {
        match gate(p.kind, w, x) {
            Gate::Skip => continue,
            Gate::Stop => break,
            Gate::Take => {}
        }
        best = match best {
            None => Some((v, x)),
            Some((m, _)) if (max && v >= m) || (!max && v <= m) => Some((v, x)),
            keep => keep,
        };
    }
    best
}

/// `measure_minMaxAvg` for AVG: trapezoids over the window points
/// (edges not interpolated), divided by their span. Also returns the
/// last scale value read — what ngspice prints as `to=`.
fn average(p: &Plot, y: &[f64], w: &Win) -> (Option<f64>, f64) {
    let mut sum = 0.0;
    let mut tsum = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    let mut last_x = 0.0;
    for (&x, &v) in p.x.iter().zip(y) {
        last_x = x;
        match gate(p.kind, w, x) {
            Gate::Skip => continue,
            Gate::Stop => break,
            Gate::Take => {}
        }
        if let Some((px, pv)) = prev {
            sum += 0.5 * (v + pv) * (x - px);
            tsum += x - px;
        }
        prev = Some((x, v));
    }
    match prev {
        // A single point: 0/0, NaN, "out of interval" in ngspice.
        Some(_) => {
            let v = sum / tsum;
            (Some(v).filter(|v| !v.is_nan()), last_x)
        }
        None => (None, last_x),
    }
}

/// ngspice's `AlmostEqualUlps` (`maths/misc/equality.c`).
fn almost_equal_ulps(a: f64, b: f64, max_ulps: i64) -> bool {
    if a == b {
        return true;
    }
    let ord = |x: f64| {
        let i = x.to_bits() as i64;
        if i < 0 { i64::MIN.wrapping_sub(i) } else { i }
    };
    ord(a).wrapping_sub(ord(b)).unsigned_abs() <= max_ulps as u64
}

/// `measure_rms_integral`: the window's samples, edges interpolated
/// onto `FROM`/`TO`, integrated by Simpson 3/8 where three equal
/// widths follow, Simpson 1/3 where two do, trapezoids elsewhere.
/// `(value, from, to)`; `value` NaN when the window is empty.
fn rms_integral(p: &Plot, y: &[f64], w: &Win, rms: bool) -> (f64, f64, f64) {
    let n = y.len().min(p.x.len());
    let sq = |v: f64| if rms { v * v } else { v };
    let lerp = |i: usize, at: f64| {
        let slope = (y[i] - y[i - 1]) / (p.x[i] - p.x[i - 1]);
        let yint = y[i - 1] - slope * p.x[i - 1];
        slope * at + yint
    };
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut from_at = f64::NAN;
    let mut to_val: Option<f64> = None;
    for (i, (&x0, &v0)) in p.x.iter().zip(y).enumerate() {
        let (mut x, mut v) = (x0, v0);
        if x < w.from {
            continue;
        }
        if w.to != 0.0 && x > w.to {
            if i > 0 && !almost_equal_ulps(x, w.to, 100) {
                v = lerp(i, w.to);
                x = w.to;
            }
            xs.push(x);
            ys.push(sq(v));
            to_val = Some(x);
            break;
        }
        if xs.is_empty() {
            if w.from != 0.0 && i > 0 && !almost_equal_ulps(x, w.from, 100) {
                v = lerp(i, w.from);
                x = w.from;
            }
            from_at = x;
        }
        xs.push(x);
        ys.push(sq(v));
    }
    let m = xs.len();
    // Widths, padded with two zeros like the C.
    let mut width: Vec<f64> = (0..m.saturating_sub(1))
        .map(|i| xs[i + 1] - xs[i])
        .collect();
    width.extend([0.0, 0.0]);
    let (mut s1, mut s2, mut s3) = (0.0, 0.0, 0.0);
    let mut i = 0;
    while i + 1 < m {
        if i + 3 < m
            && almost_equal_ulps(width[i], width[i + 1], 100)
            && almost_equal_ulps(width[i], width[i + 2], 100)
        {
            s1 += 3.0 * width[i] * (ys[i] + 3.0 * (ys[i + 1] + ys[i + 2]) + ys[i + 3]) / 8.0;
            i += 3;
        } else if i + 2 < m && almost_equal_ulps(width[i], width[i + 1], 100) {
            s2 += width[i] * (ys[i] + 4.0 * ys[i + 1] + ys[i + 2]) / 3.0;
            i += 2;
        } else {
            s3 += width[i] * (ys[i] + ys[i + 1]) / 2.0;
            i += 1;
        }
    }
    // ngspice re-reads the last scale value when no `TO` cut the
    // window (its `toVal < 0` test).
    let to = match to_val {
        Some(t) if t >= 0.0 => t,
        _ => p.x.get(n.wrapping_sub(1)).copied().unwrap_or(f64::NAN),
    };
    if m == 0 {
        return (f64::NAN, from_at, to);
    }
    let sum = s1 + s2 + s3;
    let v = if rms {
        (sum / (to - from_at)).sqrt()
    } else {
        sum
    };
    (v, from_at, to)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dune_netlist::parse_measure_body;
    use dune_raw::{Data, VarType};

    fn tran(t: Vec<f64>, y: Vec<f64>) -> Solution {
        Solution {
            plotname: "Transient Analysis".into(),
            variables: vec![
                ("time".into(), VarType::Time),
                ("v(1)".into(), VarType::Voltage),
            ],
            data: Data::Real(vec![t, y]),
            scale: Some(0),
        }
    }

    fn meas(s: &Solution, body: &str) -> Result<Measurement, MeasureError> {
        let (f, _) = parse_measure_body(body).unwrap();
        s.measure(&f)
    }

    fn val(s: &Solution, body: &str) -> f64 {
        meas(s, body)
            .unwrap_or_else(|e| panic!("{body}: {e}"))
            .value
    }

    /// A triangle 0→1→0→1→0 over t = 0..4.
    fn tri() -> Solution {
        let t: Vec<f64> = (0..=40).map(|i| f64::from(i) * 0.1).collect();
        let y = t
            .iter()
            .map(|&t| {
                let f = t - t.floor();
                if (t.floor() as i64) % 2 == 0 {
                    f
                } else {
                    1.0 - f
                }
            })
            .collect();
        tran(t, y)
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() <= 1e-12 * b.abs().max(1.0), "{a} vs {b}");
    }

    #[test]
    fn crossings_by_edge_and_count() {
        let s = tri();
        close(val(&s, "when v(1)=0.25"), 0.25);
        close(val(&s, "when v(1)=0.25 rise=2"), 2.25);
        close(val(&s, "when v(1)=0.25 fall=1"), 1.75);
        close(val(&s, "when v(1)=0.25 cross=3"), 2.25);
        close(val(&s, "when v(1)=0.25 cross=last"), 3.75);
        close(val(&s, "when v(1)=0.25 rise=last"), 2.25);
        close(val(&s, "when v(1)=0.25 last"), 3.75);
        close(val(&s, "when v(1)=0.25 td=1"), 1.75);
        close(val(&s, "when v(1)=0.75 from=1 to=1.5"), 1.25);
        assert!(meas(&s, "when v(1)=0.25 from=1 to=1.5").is_err());
    }

    #[test]
    fn out_of_interval_and_unknown_vectors() {
        let s = tri();
        assert_eq!(
            meas(&s, "when v(1)=5"),
            Err(MeasureError::OutOfInterval {
                clause: MeasClause::When
            })
        );
        assert!(matches!(
            meas(&s, "max v(9)"),
            Err(MeasureError::UnknownVector(_))
        ));
        assert_eq!(
            meas(&s, "trig v(1) val=0.5 rise=1 targ v(1) val=0.5 rise=9"),
            Err(MeasureError::OutOfInterval {
                clause: MeasClause::Targ
            })
        );
        let op = Solution {
            plotname: "Operating Point".into(),
            variables: vec![("v(1)".into(), VarType::Voltage)],
            data: Data::Real(vec![vec![1.0]]),
            scale: None,
        };
        assert!(matches!(
            meas(&op, "max v(1)"),
            Err(MeasureError::UnsupportedPlot(_))
        ));
    }

    #[test]
    fn first_segment_crossing_counts() {
        // ngspice misses this one (KNOWN_DIFFS); the crossing is in
        // [t0, t1].
        let s = tri();
        close(val(&s, "when v(1)=0.05"), 0.05);
        close(val(&s, "when v(1)=0.05 rise=1"), 0.05);
    }

    #[test]
    fn delay_find_and_stats() {
        let s = tri();
        let m = meas(&s, "trig v(1) val=0.5 rise=1 targ v(1) val=0.5 fall=1").unwrap();
        close(m.value, 1.0);
        assert!(matches!(m.detail, MeasDetail::Delay { .. }));
        close(val(&s, "trig at=0.2 targ v(1) val=0.5 rise=2"), 2.3);
        close(val(&s, "find v(1) at=1.25"), 0.75);
        close(val(&s, "find v(1) when v(1)=0.5 cross=2"), 0.5);
        close(val(&s, "deriv v(1) at=0.55"), 1.0);
        close(val(&s, "deriv v(1) at=1.55"), -1.0);
        close(val(&s, "max v(1)"), 1.0);
        close(val(&s, "min v(1) from=0.5 to=1.5"), 0.5);
        close(val(&s, "pp v(1)"), 1.0);
        close(val(&s, "avg v(1)"), 0.5);
        let m = meas(&s, "max_at v(1) to=2").unwrap();
        close(m.value, 1.0);
        assert_eq!(m.detail, MeasDetail::With { with: 1.0 });
    }

    #[test]
    fn simpson_is_exact_on_cubics() {
        // Simpson 3/8 and 1/3 integrate cubics exactly, so on
        // uniform samples of t³ and t the window integrals are
        // closed-form.
        // 12 widths: four Simpson 3/8 panels (a trapezoid would
        // close an odd remainder, as in the C).
        let t: Vec<f64> = (0..=12).map(|i| f64::from(i) / 10.0).collect();
        let s = Solution {
            plotname: "Transient Analysis".into(),
            variables: vec![
                ("time".into(), VarType::Time),
                ("v(1)".into(), VarType::Voltage),
                ("v(2)".into(), VarType::Voltage),
            ],
            data: Data::Real(vec![
                t.clone(),
                t.clone(),
                t.iter().map(|t| t * t * t).collect(),
            ]),
            scale: Some(0),
        };
        close(val(&s, "integ v(2)"), 1.2f64.powi(4) / 4.0);
        close(val(&s, "rms v(1)"), (1.44f64 / 3.0).sqrt());
        // Interpolated window edges.
        let m = meas(&s, "integ v(1) from=0.05 to=0.95").unwrap();
        close(m.value, 0.5 * (0.95 * 0.95 - 0.05 * 0.05));
        let MeasDetail::Window { from, to } = m.detail else {
            panic!()
        };
        close(from, 0.05);
        close(to, 0.95);
    }

    #[test]
    fn ulps_like_ngspice() {
        assert!(almost_equal_ulps(1.0, 1.0 + 50.0 * f64::EPSILON, 100));
        assert!(!almost_equal_ulps(1.0, 1.0 + 1e-10, 100));
        assert!(almost_equal_ulps(0.0, -0.0, 100));
        assert!(!almost_equal_ulps(1.0, -1.0, 100));
    }

    #[test]
    fn derivative_is_exact_on_parabolas() {
        let x = [0.0, 0.1, 0.3, 0.35, 0.8];
        let y: Vec<f64> = x.iter().map(|x| 3.0 * x * x - x).collect();
        let d = derivative(&x, &y);
        for i in 1..4 {
            close(d[i], 6.0 * x[i] - 1.0);
        }
    }
}
