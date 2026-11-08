"""``spice.pic`` over the real figure corpus (``corpus/pic``).

Mirrored test for test by ``r/tests/testthat/test-pic.R`` (same figures,
same assertions): every figure extracts to its golden netlist byte for
byte, simulates exactly as its golden deck, and draws.
"""

from __future__ import annotations

import math
from pathlib import Path

import pytest

import spice
from spice.pic import PicError, PicExtraction

# The CLI's texlabels test figure (crates/dune-cli/tests/texlabels.rs):
# display labels in math.
MATH_FIGURE = r""".PS
spice = 1
texlabels = 1
B: (0.3,1.2)
Vcc: vsource((3.9,2.6), (3.9,0.3), 5, "$V_{CC}$")
Vb: vsource((0.3,1.2), (0.3,0.3), 0.8, "$G_1$")
Rb: resistor((0.3,1.2), (1.72,1.2), 10k)
Rc: resistor((2.36,2.6), (2.36,1.62), 1k)
Q1: npn((2.2,1.2), qn, "$T_1$")
line from Rc.P to Vcc.P
line from Q1.E to (Q1.E.x,0.3)
line from Vb.N to Vcc.N
dot at (Q1.E.x,0.3)
ground((1.2,0.3))
.model qn npn(bf=100)
.op
.PE
"""
MATH_LITERALS = ("$T_1$", "$G_1$", "$V_{CC}$")

# R1 has no value (an error), R2.N is dangling (a warning).
FLAWED_FIGURE = """.PS
spice = 1
V1: vsource((0,1), (0,0), DC 1)
R1: resistor((0,1), (1,1))
R2: resistor((1,1), (2,1), 1k)
line from R1.N to (1,0) then to V1.N; ground(V1.N)
.op
.PE
"""

# Line 4 names a label that does not exist: rpic rejects the figure.
BROKEN_FIGURE = """.PS
spice = 1
R1: resistor((0,0), (1,0), 1k)
line from Q9.C to (1,1)
.PE
"""

# R1.P sits 5 mil off V1.P: apart at the default snap (1 mil).
GAP_FIGURE = """.PS
spice = 1
V1: vsource((0,1), (0,0), DC 2)
R1: resistor((0,1.005), (1,1.005), 1k)
R2: resistor((1,1.005), (1,0), 1k)
line from R2.N to V1.N; ground(V1.N)
.op
.PE
"""


# Figures whose extraction warns by design: figure -> the nets of its
# dangling pins (warnings), in order. mos6inv leaves the deck's test
# nodes 11..43 and its output open, as the deck does (dune-pic's
# same_as_deck_dangling). Every other figure extracts clean.
EXPECTED_DANGLING: dict[str, list[str]] = {
    "mos6inv": [
        "N11", "N12", "N13", "N21", "N22", "N23",
        "N31", "N32", "N33", "N41", "N42", "N43", "N5",
    ],
}


def figures(corpus: Path) -> list[Path]:
    return sorted(p for p in (corpus / "pic").glob("*.pic") if p.name != "spice.pic")


def same_result(a: spice.Result, b: spice.Result) -> bool:
    return len(a) == len(b) and all(
        p.plotname == q.plotname and p.df().equals(q.df()) for p, q in zip(a, b, strict=True)
    )


def test_every_corpus_figure_extracts_to_its_golden_and_simulates_as_it(corpus: Path) -> None:
    figs = figures(corpus)
    assert len(figs) >= 29
    for fig in figs:
        golden = (corpus / "pic" / "golden" / f"{fig.stem}.cir").read_bytes()
        ex = spice.pic.extract(fig)
        dangling = EXPECTED_DANGLING.get(fig.stem, [])
        assert all(
            (d.severity, d.kind) == ("warning", "dangling-pin") for d in ex.diagnostics
        ), fig.name
        assert [d.message.split("`")[1] for d in ex.diagnostics] == dangling, fig.name
        assert ex.netlist.encode() == golden, fig.name
        assert ex.path == str(fig)
        # The golden deck, given the figure's includes (it sits in
        # golden/, away from them), runs exactly as the extraction.
        ours = spice.simulate(ex)
        theirs = spice.simulate(golden.decode(), includes=ex.includes or None)
        assert same_result(ours, theirs), fig.name
        svg = spice.pic.render(fig)
        assert svg.startswith("<svg") and svg.rstrip().endswith("</svg>"), fig.name
        assert spice.pic.render(ex) == svg


def test_map_of_rc(corpus: Path) -> None:
    ex = spice.pic.extract(corpus / "pic" / "rc.pic")
    m = ex.map
    assert [e.refdes for e in m.elements] == ["V1", "R1", "C1"]
    assert [(p.refdes, p.name, p.net) for p in m.pins] == [
        ("V1", "P", "Vin"),
        ("V1", "N", "0"),
        ("R1", "P", "Vin"),
        ("R1", "N", "Vout"),
        ("C1", "P", "Vout"),
        ("C1", "N", "0"),
    ]
    r1 = m.element("r1")
    assert r1.principal == "resistance"
    assert r1.value is not None and r1.value.text == "1k"
    assert (r1.value.span.line, r1.value.span.col) == (6, 25)
    v1 = m.element("V1")
    assert [(p.name, p.text) for p in v1.params] == [("pulse", "PULSE(0 5 0 1u 1u 10m 20m)")]
    assert r1.ids == f"s{r1.shapes.start}..s{r1.shapes.stop - 1}"
    assert m.nets[0].name == "0" and len(m.nets[0].shapes) > 0
    assert m.canvas.w > 0 and m.canvas.h > 0 and m.canvas.rescale == 1
    # The source line the value span points at holds the value.
    line = ex.source.splitlines()[r1.value.span.line - 1]
    assert line[r1.value.span.col - 1 : r1.value.span.end_col - 1] == "1k"
    # Pins land inside the SVG page.
    for p in m.pins:
        x, y = m.canvas.to_svg(p.x, p.y)
        assert 0 <= x <= m.canvas.w * 96 and 0 <= y <= m.canvas.h * 96


def test_opamp_include_resolves_next_to_the_figure(corpus: Path) -> None:
    fig = corpus / "pic" / "opamp.pic"
    ex = spice.pic.extract(fig)
    sub = (corpus / "pic" / "opamp_ideal.sub").read_text()
    assert ex.includes == {"opamp_ideal.sub": sub}
    dc = spice.simulate(ex)[0]
    vin, vout = dc.column("v(vin)"), dc.column("v(vout)")
    # Finite-gain inverting amplifier, A = 200k.
    gain = -10.0 / (1.0 + 11.0 / 200e3)
    for vi, vo in zip(vin, vout, strict=True):
        assert math.isclose(vo, gain * vi, rel_tol=1e-5, abs_tol=1e-9)
    # Figure text has no directory: its includes are given, or missing.
    text = fig.read_text()
    with pytest.raises(spice.SpiceError) as e:
        spice.simulate(spice.pic.extract(text))
    assert e.value.kind == "parse" and "opamp_ideal.sub" in str(e.value)
    given = spice.pic.extract(text, includes={"opamp_ideal.sub": sub})
    assert same_result(spice.simulate(given), spice.simulate(ex))
    with pytest.raises(ValueError):
        spice.simulate(ex, includes={"opamp_ideal.sub": sub})


def test_texlabels_typeset_math_labels() -> None:
    plain = MATH_FIGURE.replace("texlabels = 1\n", "")
    # Presentation only: the netlist does not change.
    assert spice.pic.extract(MATH_FIGURE).netlist == spice.pic.extract(plain).netlist
    lit = spice.pic.render(plain)
    assert all(s in lit for s in MATH_LITERALS)
    assert lit.count("<svg") == 1
    svg = spice.pic.render(MATH_FIGURE)
    if spice.pic.TEXLABELS:
        assert not any(s in svg for s in MATH_LITERALS)
        assert svg.count("<svg x=") >= 3
    else:
        assert all(s in svg for s in MATH_LITERALS)


def test_extraction_diagnostics_are_data_and_block_the_run() -> None:
    ex = spice.pic.extract(FLAWED_FIGURE)
    assert [(d.severity, d.kind, d.file, d.line, d.column) for d in ex.diagnostics] == [
        ("error", "missing-value", None, 4, 5),
        ("warning", "dangling-pin", None, 5, 5),
    ]
    assert all(d.end_column is not None for d in ex.diagnostics)
    assert "R1 n1 0\n" in ex.netlist
    with pytest.raises(PicError) as e:
        spice.simulate(ex)
    assert e.value.kind == "pic"
    assert (e.value.line, e.value.column) == (4, 5)
    assert e.value.extraction is ex
    with pytest.raises(PicError):
        spice.Session(ex)


def test_compile_errors_raise_with_their_position() -> None:
    with pytest.raises(PicError) as e:
        spice.pic.extract(BROKEN_FIGURE)
    assert isinstance(e.value, spice.SpiceError) and e.value.kind == "pic"
    assert (e.value.line, e.value.column) == (4, 11)
    (d,) = e.value.pic_diagnostics
    assert (d.kind, d.file, d.line) == ("compile", None, 4)
    assert "Q9" in str(e.value)
    with pytest.raises(PicError):
        spice.pic.render(BROKEN_FIGURE)


def test_unmarked_figures_draw_but_do_not_extract() -> None:
    fig = ".PS\nbox\n.PE\n"
    with pytest.raises(PicError) as e:
        spice.pic.extract(fig)
    assert e.value.pic_diagnostics[0].kind == "not-marked"
    assert e.value.line is None
    assert spice.pic.render(fig).startswith("<svg")


def test_snap_joins_near_terminals() -> None:
    apart = spice.pic.extract(GAP_FIGURE)
    assert any(d.kind == "dangling-pin" for d in apart.diagnostics)
    joined = spice.pic.extract(GAP_FIGURE, snap=0.01)
    assert joined.diagnostics == ()
    op = spice.simulate(joined)[0]
    assert math.isclose(float(op.column("v(n2)")[0]), 1.0, rel_tol=1e-9)


def test_bad_arguments(corpus: Path, tmp_path: Path) -> None:
    rc = corpus / "pic" / "rc.pic"
    with pytest.raises(ValueError):
        spice.pic.extract(GAP_FIGURE, snap=-1)
    with pytest.raises(ValueError):
        spice.pic.extract(rc, includes={})
    with pytest.raises(ValueError, match="figure TEXT"):
        spice.pic.extract(str(rc))
    with pytest.raises(FileNotFoundError):
        spice.pic.extract(tmp_path / "missing.pic")
    with pytest.raises(TypeError):
        spice.pic.extract(42)  # type: ignore[arg-type]
    assert isinstance(spice.pic.extract(rc), PicExtraction)
    assert "3 elements" in repr(spice.pic.extract(rc))
