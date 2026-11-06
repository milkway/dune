"""Notebook reprs: the SVG preview of a plot and the HTML tables."""

from __future__ import annotations

import re
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np

import spice
from spice._repr import MAX_POINTS, MAX_TRACES, PALETTE, decimate

SVG = "{http://www.w3.org/2000/svg}"


def _svg(plot: spice.Plot) -> ET.Element:
    text = plot._repr_svg_()
    assert text is not None
    root = ET.fromstring(text)
    assert root.tag == f"{SVG}svg"
    assert root.get("viewBox") == "0 0 640 360"
    return root


def _traces(root: ET.Element) -> list[ET.Element]:
    return [p for p in root.iter(f"{SVG}path") if p.find(f"{SVG}title") is not None]


def _npoints(path: ET.Element) -> int:
    return len(re.findall(r"[ML]", path.get("d", "")))


def _texts(root: ET.Element) -> list[str]:
    return [t.text or "" for t in root.iter(f"{SVG}text")]


def test_tran_preview(examples: Path) -> None:
    tran = spice.simulate(examples / "rc.cir").plot("Transient Analysis")
    root = _svg(tran)
    traces = _traces(root)
    # v(1) and v(2) drawn; i(vin) is a current, not drawn on the voltage axis.
    assert ["".join(t.itertext()) for t in traces] == ["v(1)", "v(2)"]
    assert [t.get("stroke") for t in traces] == list(PALETTE[:2])
    texts = _texts(root)
    assert "time (s)" in texts and "voltage (V)" in texts
    assert "(+1 current, not drawn)" in texts
    assert all(_npoints(t) == tran.npoints for t in traces)
    # No background: transparent on light and dark notebooks.
    assert root.find(f"{SVG}rect") is None
    # A drawn plot leaves the HTML table out, so notebooks show the SVG.
    assert tran._repr_html_() is None


def test_ac_preview_is_log_magnitude(corpus: Path) -> None:
    ac = spice.simulate(corpus / "basic" / "acrc.cir").plot("AC Analysis")
    root = _svg(ac)
    texts = _texts(root)
    assert any(t.startswith("|") for t in texts)  # |v(..)| legend and axis
    # Decade ticks on a log axis.
    freq = ac.column("frequency").real
    assert freq.max() / freq.min() > 10
    assert any(t.endswith("k") or t in ("10", "100") for t in texts)


def test_trace_cap_and_more_note() -> None:
    lines = ["many nodes", "v1 1 0 dc 0"]
    lines += [f"r{i} {i} {i + 1} 1k" for i in range(1, 12)]
    lines += ["r12 12 0 1k", ".dc v1 0 1 0.1", ".end"]
    dc = spice.simulate("\n".join(lines) + "\n")[0]
    root = _svg(dc)
    traces = _traces(root)
    assert len(traces) == MAX_TRACES
    voltages = [v for v in dc.variables[1:] if v.kind == "voltage"]
    assert f"(+{len(voltages) - MAX_TRACES} more)" in _texts(root)
    # Past the 6-color palette, traces are dashed.
    assert traces[0].get("stroke-dasharray") is None
    assert traces[len(PALETTE)].get("stroke-dasharray") is not None


def test_decimation_bounds_trace_size() -> None:
    deck = "\n".join(
        [
            "long ringing",
            "vin 1 0 sin(0 1 1k)",
            "r1 1 2 1k",
            "c1 2 0 10n",
            ".tran 1u 20m 0 1u",
            ".end",
        ]
    )
    tran = spice.simulate(deck + "\n")[0]
    assert tran.npoints > 4 * MAX_POINTS
    for t in _traces(_svg(tran)):
        assert _npoints(t) <= MAX_POINTS + 2


def test_decimate_keeps_extremes_and_ends() -> None:
    y = np.zeros(10_000)
    y[1234] = 5.0
    y[8765] = -3.0
    keep = decimate(y)
    assert len(keep) <= MAX_POINTS + 2
    assert np.all(np.diff(keep) > 0)
    assert {0, 1234, 8765, 9999} <= set(keep.tolist())
    short = np.arange(10.0)
    np.testing.assert_array_equal(decimate(short), np.arange(10))


def test_decimate_keeps_breaks() -> None:
    y = np.sin(np.linspace(0, 50, 5000))
    y[2500] = np.nan
    assert 2500 in decimate(y).tolist()


def test_no_scale_falls_back_to_html(examples: Path) -> None:
    pz = spice.simulate(examples / "simplepz.cir")[0]
    assert pz.scale is None
    assert pz._repr_svg_() is None
    html = pz._repr_html_()
    assert html is not None
    ET.fromstring(html)  # well-formed
    assert "pole" in html and "complex" in html


def test_op_html_shows_values() -> None:
    op = spice.simulate("divider\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n")[0]
    assert op._repr_svg_() is None
    html = op._repr_html_()
    assert html is not None
    root = ET.fromstring(html)
    cells = [c.text or "".join(c.itertext()) for c in root.iter("td")]
    assert "v(2)" in cells and "1" in cells and "2" in cells


def test_result_html(examples: Path) -> None:
    res = spice.simulate(examples / "resnoise.cir")
    html = res._repr_html_()
    root = ET.fromstring(html)
    rows = list(root.iter("tr"))
    assert len(rows) == 1 + len(res)
    header = [c.text for c in rows[0]]
    assert header == ["plot", "variables", "points", "type", "scale"]
    first = ["".join(c.itertext()) for c in rows[1]]
    assert first[0] == res[0].plotname
    assert first[1:4] == [str(len(res[0].variables)), str(res[0].npoints), "real"]


def test_result_html_notes_and_diagnostics_are_escaped() -> None:
    res = spice.simulate("t <&>\nr1 1 0 1k\n.options bogus=1\n.end\n")
    html = res._repr_html_()
    ET.fromstring(html)
    assert "t &lt;&amp;&gt;" in html
    assert "Notes:" in html and "Diagnostics:" in html
    assert "bogus" in html
