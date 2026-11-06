"""Rich notebook representations: an SVG preview of a plot, HTML tables.

Hand-rolled SVG and HTML (no plotting dependency). The preview is a
quick look, not a figure: one y axis, linear magnitude for complex
vectors, at most :data:`MAX_TRACES` traces of at most about
:data:`MAX_POINTS` points each. For anything else, plot
:meth:`spice.Plot.df` with the library of your choice.
"""

from __future__ import annotations

import math
from collections.abc import Sequence
from html import escape
from typing import TYPE_CHECKING

import numpy as np
import numpy.typing as npt

if TYPE_CHECKING:
    from ._result import Plot, Result

__all__ = [
    "MAX_POINTS",
    "MAX_TRACES",
    "PALETTE",
    "decimate",
    "has_preview",
    "plot_html",
    "plot_svg",
    "result_html",
]

FloatArray = npt.NDArray[np.float64]
IntArray = npt.NDArray[np.intp]

MAX_TRACES = 8
"""Most traces an SVG preview draws; the rest are counted in a note."""

MAX_POINTS = 400
"""Rough cap on the points of one trace in an SVG preview (see :func:`decimate`)."""

MAX_ROWS = 20
"""Most vectors an HTML table lists."""

PALETTE = ("#4477AA", "#EE6677", "#228833", "#CCBB44", "#66CCEE", "#AA3377")
"""Trace colors (Paul Tol's colorblind-safe "bright" scheme), cycled."""

_AXIS = "#666"
_TEXT = "#888"
_FONT = "system-ui, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"

_W, _H = 640, 360
_LEFT, _RIGHT, _TOP, _BOTTOM = 64, 488, 34, 312
_LEGEND_X = 504

_UNITS = {"time": "s", "frequency": "Hz", "voltage": "V", "current": "A"}

_SI = ((1e12, "T"), (1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""),
       (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p"), (1e-15, "f"))


# ---------------------------------------------------------------- numbers


def _si(value: float, step: float) -> str:
    """A tick label with an SI prefix, precise enough to tell ticks ``step`` apart."""
    if value == 0.0:
        return "0"
    ref = max(abs(value), abs(step))
    factor, prefix = _SI[-1]
    for f, p in _SI:
        if ref >= f * 0.9999:
            factor, prefix = f, p
            break
    if ref >= 1e15:
        return f"{value:.3g}"
    scaled_step = abs(step) / factor if step else 1.0
    decimals = max(0, min(6, -math.floor(math.log10(scaled_step) + 1e-9)))
    text = f"{value / factor:.{decimals}f}"
    if "." in text:
        text = text.rstrip("0").rstrip(".")
    if text in ("-0", ""):
        text = "0"
    return text.replace("-", "−") + prefix


def _nice_ticks(lo: float, hi: float, target: int = 5) -> list[float]:
    """Round-number ticks (1, 2, 5 x 10^k) inside ``[lo, hi]``, 4 to 6 of them when possible."""
    span = hi - lo
    if not span > 0 or not math.isfinite(span):
        return [lo]
    best: list[float] = []
    for want in (target, target + 1, target - 1, target + 2, target - 2):
        raw = span / max(want - 1, 1)
        mag = 10.0 ** math.floor(math.log10(raw))
        for m in (1.0, 2.0, 2.5, 5.0, 10.0):
            step = m * mag
            if step >= raw * 0.999:
                break
        first = math.ceil(lo / step - 1e-9) * step
        ticks = []
        k = 0
        while first + k * step <= hi + step * 1e-9:
            t = first + k * step
            ticks.append(0.0 if abs(t) < step * 1e-9 else t)
            k += 1
        if 4 <= len(ticks) <= 6:
            return ticks
        if not best or abs(len(ticks) - target) < abs(len(best) - target):
            best = ticks
    return best


def _log_ticks(lo: float, hi: float) -> list[float]:
    """Decade ticks of ``[10**lo, 10**hi]`` (in log10 units), thinned to at most 6."""
    first, last = math.ceil(lo - 1e-9), math.floor(hi + 1e-9)
    decades = list(range(first, last + 1))
    stride = max(1, math.ceil(len(decades) / 6))
    return [float(d) for d in decades[::stride]]


def decimate(y: FloatArray, max_points: int = MAX_POINTS) -> IntArray:
    """Indices of ``y`` to keep so a polyline of about ``max_points`` points looks the same.

    Min/max decimation: the points are split into ``max_points // 2``
    consecutive buckets and each keeps its lowest and highest sample (in
    index order), so peaks and ringing survive. The first and last points
    are always kept. A series of at most ``max_points`` points is kept
    whole. Non-finite samples are kept so the trace still breaks there.

    Returns:
        Sorted, unique indices; at most ``max_points + 2`` of them.
    """
    n = len(y)
    if n <= max_points:
        return np.arange(n, dtype=np.intp)
    buckets = max(1, max_points // 2)
    edges = np.linspace(0, n, buckets + 1).astype(np.intp)
    keep: list[int] = [0, n - 1]
    finite = np.isfinite(y)
    for a, b in zip(edges[:-1], edges[1:], strict=True):
        if b <= a:
            continue
        seg = y[a:b]
        ok = finite[a:b]
        if not ok.all():
            keep.extend(int(a + i) for i in np.flatnonzero(~ok)[:2])
            if not ok.any():
                continue
            seg = np.where(ok, seg, np.nan)
            keep.append(int(a + np.nanargmin(seg)))
            keep.append(int(a + np.nanargmax(seg)))
        else:
            keep.append(int(a + np.argmin(seg)))
            keep.append(int(a + np.argmax(seg)))
    out: IntArray = np.unique(np.asarray(keep, dtype=np.intp))
    return out


# ------------------------------------------------------------------- SVG


def _fmt(v: float) -> str:
    return f"{v:.1f}"


def _trace_path(xs: FloatArray, ys: FloatArray) -> str:
    """SVG path data; a non-finite point breaks the line."""
    parts: list[str] = []
    pen_down = False
    for x, y in zip(xs.tolist(), ys.tolist(), strict=True):
        if not (math.isfinite(x) and math.isfinite(y)):
            pen_down = False
            continue
        parts.append(f"{'L' if pen_down else 'M'}{_fmt(x)} {_fmt(y)}")
        pen_down = True
    return "".join(parts)


def _dash(i: int) -> str:
    """Traces past the palette's length are dashed, so 8 traces stay distinct."""
    return ' stroke-dasharray="5 3"' if i >= len(PALETTE) else ""


def _clip_label(text: str, width: int = 16) -> str:
    return text if len(text) <= width else text[: width - 1] + "…"


def _axis_label(name: str, kind: str, magnitude: bool = False) -> str:
    unit = _UNITS.get(kind)
    label = f"|{name}|" if magnitude else name
    return f"{label} ({unit})" if unit else label


def has_preview(plot: Plot) -> bool:
    """Whether :func:`plot_svg` draws ``plot``: a scale, 2+ points, something to draw."""
    scale = plot.scale
    if scale is None or plot.npoints < 2 or len(plot.variables) < 2:
        return False
    return bool(np.isfinite(plot.column(scale).real).any())


def plot_svg(plot: Plot) -> str | None:
    """The SVG preview of :meth:`spice.Plot._repr_svg_` (``None`` when there is none).

    See :meth:`spice.Plot._repr_svg_` for what is drawn.
    """
    if not has_preview(plot) or plot.scale is None:
        return None
    variables = plot.variables
    scale_idx = plot._scale
    scale_name, scale_kind = variables[scale_idx] if scale_idx is not None else (plot.scale, "")
    x = np.asarray(plot.column(scale_name).real, dtype=np.float64)
    others = [v for i, v in enumerate(variables) if i != scale_idx]
    kind = others[0].kind
    same = [v for v in others if v.kind == kind]
    shown = same[:MAX_TRACES]
    hidden_same = len(same) - len(shown)
    hidden_other = len(others) - len(same)

    # x axis: log10 for a frequency scale spanning more than a decade.
    xfin = x[np.isfinite(x)]
    log_x = bool(
        scale_kind == "frequency"
        and xfin.size > 0
        and xfin.min() > 0
        and xfin.max() / xfin.min() > 10.0
    )
    with np.errstate(divide="ignore", invalid="ignore"):
        xv: FloatArray = np.where(x > 0, np.log10(np.where(x > 0, x, 1.0)), np.nan) if log_x else x
    xv_fin = xv[np.isfinite(xv)]
    x_lo, x_hi = float(xv_fin.min()), float(xv_fin.max())
    if x_hi == x_lo:
        x_lo, x_hi = x_lo - 0.5, x_hi + 0.5

    ys: list[FloatArray] = []
    for v in shown:
        col = plot.column(v.name)
        y = np.abs(col) if plot.is_complex else col
        ys.append(np.asarray(y, dtype=np.float64))
    finite = [y[np.isfinite(y)] for y in ys]
    finite = [f for f in finite if f.size]
    if finite:
        y_lo = min(float(f.min()) for f in finite)
        y_hi = max(float(f.max()) for f in finite)
    else:
        y_lo, y_hi = 0.0, 1.0
    if y_hi == y_lo:
        pad = abs(y_lo) * 0.1 or 1.0
        y_lo, y_hi = y_lo - pad, y_hi + pad
    pad = (y_hi - y_lo) * 0.04
    y_lo, y_hi = y_lo - pad, y_hi + pad

    def px(u: FloatArray | float) -> FloatArray:
        return _LEFT + (np.asarray(u, dtype=np.float64) - x_lo) / (x_hi - x_lo) * (_RIGHT - _LEFT)

    def py(u: FloatArray | float) -> FloatArray:
        return _BOTTOM - (np.asarray(u, dtype=np.float64) - y_lo) / (y_hi - y_lo) * (_BOTTOM - _TOP)

    out: list[str] = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{_W}" height="{_H}" '
        f'viewBox="0 0 {_W} {_H}" font-family="{_FONT}" font-size="11">',
        f"<title>{escape(plot.plotname)}</title>",
        f'<text x="{_LEFT}" y="20" fill="{_TEXT}" font-size="13">{escape(plot.plotname)}</text>',
    ]

    # Grid, ticks and tick labels.
    grid: list[str] = []
    labels: list[str] = []
    if log_x:
        xticks = _log_ticks(x_lo, x_hi)
        xtext = [_si(10.0**t, 10.0**t) for t in xticks]
    else:
        xticks = _nice_ticks(x_lo, x_hi)
        xstep = xticks[1] - xticks[0] if len(xticks) > 1 else x_hi - x_lo
        xtext = [_si(t, xstep) for t in xticks]
    for t, s in zip(xticks, xtext, strict=True):
        u = float(px(t))
        grid.append(f'<line x1="{_fmt(u)}" y1="{_TOP}" x2="{_fmt(u)}" y2="{_BOTTOM}"/>')
        labels.append(
            f'<line x1="{_fmt(u)}" y1="{_BOTTOM}" x2="{_fmt(u)}" y2="{_BOTTOM + 4}" '
            f'stroke="{_AXIS}"/>'
            f'<text x="{_fmt(u)}" y="{_BOTTOM + 17}" text-anchor="middle">{escape(s)}</text>'
        )
    yticks = _nice_ticks(y_lo, y_hi)
    ystep = yticks[1] - yticks[0] if len(yticks) > 1 else y_hi - y_lo
    for t in yticks:
        u = float(py(t))
        grid.append(f'<line x1="{_LEFT}" y1="{_fmt(u)}" x2="{_RIGHT}" y2="{_fmt(u)}"/>')
        labels.append(
            f'<line x1="{_LEFT - 4}" y1="{_fmt(u)}" x2="{_LEFT}" y2="{_fmt(u)}" stroke="{_AXIS}"/>'
            f'<text x="{_LEFT - 7}" y="{_fmt(u + 3.5)}" text-anchor="end">'
            f"{escape(_si(t, ystep))}</text>"
        )
    out.append(f'<g stroke="{_TEXT}" stroke-opacity="0.18" stroke-width="1">{"".join(grid)}</g>')
    out.append(
        f'<path d="M{_LEFT} {_TOP}V{_BOTTOM}H{_RIGHT}" fill="none" '
        f'stroke="{_AXIS}" stroke-width="1"/>'
    )
    out.append(f'<g fill="{_TEXT}">{"".join(labels)}</g>')

    # Axis titles.
    xlabel = _axis_label(scale_name, scale_kind)
    ylabel = _axis_label(kind, kind, magnitude=plot.is_complex)
    mid_x, mid_y = (_LEFT + _RIGHT) / 2, (_TOP + _BOTTOM) / 2
    out.append(
        f'<g fill="{_TEXT}" font-size="12">'
        f'<text x="{_fmt(mid_x)}" y="{_H - 10}" text-anchor="middle">{escape(xlabel)}</text>'
        f'<text transform="translate(16 {_fmt(mid_y)}) rotate(-90)" text-anchor="middle">'
        f"{escape(ylabel)}</text></g>"
    )

    # Traces.
    out.append(
        f'<g fill="none" stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round">'
    )
    for i, y in enumerate(ys):
        keep = decimate(y)
        d = _trace_path(px(xv[keep]), py(y[keep]))
        out.append(
            f'<path d="{d}" stroke="{PALETTE[i % len(PALETTE)]}"{_dash(i)}>'
            f"<title>{escape(shown[i].name)}</title></path>"
        )
    out.append("</g>")

    # Legend.
    leg: list[str] = []
    for i, v in enumerate(shown):
        yy = _TOP + 8 + 18 * i
        name = f"|{v.name}|" if plot.is_complex else v.name
        leg.append(
            f'<line x1="{_LEGEND_X}" y1="{yy}" x2="{_LEGEND_X + 16}" y2="{yy}" '
            f'stroke="{PALETTE[i % len(PALETTE)]}" stroke-width="2.5"{_dash(i)}/>'
            f'<text x="{_LEGEND_X + 22}" y="{yy + 4}">{escape(_clip_label(name))}</text>'
        )
    notes: list[str] = []
    if hidden_same:
        notes.append(f"(+{hidden_same} more)")
    if hidden_other:
        kinds = sorted({v.kind for v in others if v.kind != kind})
        what = kinds[0] if len(kinds) == 1 else "other"
        notes.append(f"(+{hidden_other} {what}, not drawn)")
    for j, note in enumerate(notes):
        yy = _TOP + 8 + 18 * (len(shown) + j)
        leg.append(f'<text x="{_LEGEND_X}" y="{yy + 4}" font-style="italic">{escape(note)}</text>')
    out.append(f'<g fill="{_TEXT}">{"".join(leg)}</g>')
    out.append("</svg>")
    return "\n".join(out)


# ------------------------------------------------------------------ HTML


def _num(v: complex | float) -> str:
    if isinstance(v, complex):
        sign = "-" if v.imag < 0 or (v.imag == 0 and math.copysign(1.0, v.imag) < 0) else "+"
        return f"{v.real:.6g} {sign} {abs(v.imag):.6g}j"
    return f"{v:.6g}"


def _summary(values: npt.NDArray[np.float64] | npt.NDArray[np.complex128], complex_: bool) -> str:
    if values.size == 0:
        return ""
    if values.size == 1:
        item = values[0]
        return _num(complex(item) if complex_ else float(item.real))
    mag = np.abs(values) if complex_ else values
    fin = mag[np.isfinite(mag)]
    if fin.size == 0:
        return "non-finite"
    rng = f"[{float(fin.min()):.4g}, {float(fin.max()):.4g}]"
    return f"|·| in {rng}" if complex_ else rng


_TABLE = 'style="border-collapse:collapse;font-variant-numeric:tabular-nums"'
_CELL = 'style="padding:1px 8px;text-align:left"'
_NUMCELL = 'style="padding:1px 8px;text-align:right"'


def _row(cells: Sequence[str], header: bool = False, numeric: Sequence[bool] = ()) -> str:
    tag = "th" if header else "td"
    out = []
    for i, c in enumerate(cells):
        style = _NUMCELL if (i < len(numeric) and numeric[i]) else _CELL
        out.append(f"<{tag} {style}>{c}</{tag}>")
    return "<tr>" + "".join(out) + "</tr>"


def plot_html(plot: Plot) -> str:
    """The table of :meth:`spice.Plot._repr_html_`: one row per vector."""
    kind = "complex" if plot.is_complex else "real"
    scale = f", scale <code>{escape(plot.scale)}</code>" if plot.scale else ""
    n, nv = plot.npoints, len(plot.variables)
    head = (
        f"<b>{escape(plot.plotname)}</b> &#183; {nv} vector{'' if nv == 1 else 's'} "
        f"&#215; {n} point{'' if n == 1 else 's'}, {kind}{scale}"
    )
    rows = [_row(["vector", "type", "value" if n == 1 else "range"], header=True)]
    for v in plot.variables[:MAX_ROWS]:
        col = plot.column(v.name)
        rows.append(
            _row(
                [
                    f"<code>{escape(v.name)}</code>",
                    escape(v.kind),
                    escape(_summary(col, plot.is_complex)),
                ],
                numeric=(False, False, True),
            )
        )
    more = ""
    if nv > MAX_ROWS:
        more = f"<div><i>&#8230; and {nv - MAX_ROWS} more</i></div>"
    table = f"<table {_TABLE}>{''.join(rows)}</table>"
    return f'<div class="spice-plot"><div>{head}</div>{table}{more}</div>'


def result_html(result: Result) -> str:
    """The table of :meth:`spice.Result._repr_html_`: a row per plot, notes, diagnostics."""
    n = len(result)
    head = f"<b>{escape(result.title)}</b> &#183; {n} plot{'' if n == 1 else 's'}"
    parts = [f'<div class="spice-result"><div>{head}</div>']
    if len(result):
        rows = [_row(["plot", "variables", "points", "type", "scale"], header=True)]
        for p in result:
            rows.append(
                _row(
                    [
                        escape(p.plotname),
                        str(len(p.variables)),
                        str(p.npoints),
                        "complex" if p.is_complex else "real",
                        f"<code>{escape(p.scale)}</code>" if p.scale else "&#8212;",
                    ],
                    numeric=(False, True, True, False, False),
                )
            )
        parts.append(f"<table {_TABLE}>{''.join(rows)}</table>")
    if result.notes:
        items = "".join(f"<li>{escape(n)}</li>" for n in result.notes)
        parts.append(f'<div>Notes:</div><ul style="margin:0">{items}</ul>')
    if result.diagnostics:
        items = "".join(f"<li><code>{escape(str(d))}</code></li>" for d in result.diagnostics)
        parts.append(f'<div>Diagnostics:</div><ul style="margin:0">{items}</ul>')
    parts.append("</div>")
    return "".join(parts)
