"""spice.simulate over real corpus decks."""

from __future__ import annotations

import math
import re
from pathlib import Path

import numpy as np
import polars as pl
import pytest

import spice


def test_version() -> None:
    assert spice.__version__ == "0.0.1"


def test_rc_transient(examples: Path) -> None:
    res = spice.simulate(examples / "rc.cir")
    assert res.title == "Basic RC circuit"
    assert len(res) == 1
    tran = res.plot("Transient Analysis")
    assert tran is res[0] and tran is res.plot(0)
    assert tran.scale == "time"
    assert not tran.is_complex
    assert tran.names == ["time", "v(1)", "v(2)", "i(vin)"]
    assert [v.kind for v in tran.variables] == ["time", "voltage", "voltage", "current"]
    t = tran.column("time")
    assert t.dtype == np.float64
    assert t[0] == 0.0 and math.isclose(t[-1], 7.0)
    assert np.all(np.diff(t) > 0)
    # Step into RC = 1 s: v(2) = 1 - e^-t once the 0-width pulse is up.
    v2 = tran.column("V(2)")  # case-insensitive
    assert math.isclose(v2[-1], 1 - math.exp(-t[-1]), rel_tol=0.02)
    assert "v(2)" in tran and "v(9)" not in tran


def test_rc_frame_shape(examples: Path) -> None:
    tran = spice.simulate(examples / "rc.cir")[0]
    df = tran.df()
    assert isinstance(df, pl.DataFrame)
    assert df.columns == ["time", "v(1)", "v(2)", "i(vin)"]
    assert df.height == tran.npoints == len(tran)
    assert all(dt == pl.Float64 for dt in df.dtypes)
    np.testing.assert_array_equal(df["v(2)"].to_numpy(), tran.column("v(2)"))


def test_pandas_frame(examples: Path) -> None:
    pd = pytest.importorskip("pandas")
    tran = spice.simulate(examples / "rc.cir")[0]
    pdf = tran.df(pandas=True)
    assert isinstance(pdf, pd.DataFrame)
    assert list(pdf.columns) == tran.df().columns
    assert len(pdf) == tran.npoints


def _diffpair_all(examples: Path) -> str:
    """diffpair.cir with its commented-out .tf/.dc/.ac cards switched on."""
    text = (examples / "diffpair.cir").read_text()
    return re.sub(r"(?m)^\*(\.(tf|dc|ac) )", r"\1", text)


def test_diffpair_multi_analysis(examples: Path) -> None:
    res = spice.simulate(_diffpair_all(examples))
    assert [p.plotname for p in res] == [
        "Transfer Function",
        "DC transfer characteristic",
        "AC Analysis",
        "Transient Analysis",
    ]
    tf, dc, ac, tran = res
    assert tf.scale is None and tf.npoints == 1
    assert dc.scale == "v(v-sweep)" and dc.npoints == 101
    assert ac.is_complex and ac.scale == "frequency"
    assert tran.scale == "time" and tran.column("time")[-1] == pytest.approx(500e-9)
    # Symmetric pair: v(4) and v(5) cross at the zero-input point.
    sweep = dc.column("v(v-sweep)")
    mid = int(np.argmin(np.abs(sweep)))
    assert dc.column("v(4)")[mid] == pytest.approx(dc.column("v(5)")[mid], rel=1e-3)
    # The unimplemented `lvlcod` option comes back as a positioned warning.
    (w,) = res.diagnostics
    assert w.severity == "warning" and "lvlcod" in w.message
    assert (w.file, w.line) == ("deck.cir", 3)


def test_ac_complex_columns(corpus: Path) -> None:
    ac = spice.simulate(corpus / "basic" / "acrc.cir").plot("ac analysis")
    assert ac.is_complex
    v2 = ac.column("v(2)")
    assert v2.dtype == np.complex128
    f = ac.column("frequency")
    assert f.dtype == np.complex128 and np.all(f.imag == 0)
    # First-order low pass, fc = 1/(2 pi 1k 159.155n) = 1 kHz.
    h = 1 / (1 + 1j * f.real / 1e3)
    np.testing.assert_allclose(v2, h, rtol=1e-4)

    df = ac.df()
    assert df.columns == [
        "frequency",
        "re:v(1)",
        "im:v(1)",
        "re:i(vin)",
        "im:i(vin)",
        "re:v(2)",
        "im:v(2)",
    ]
    assert df.shape == (ac.npoints, 7)
    assert all(dt == pl.Float64 for dt in df.dtypes)
    np.testing.assert_array_equal(df["frequency"].to_numpy(), f.real)
    np.testing.assert_array_equal(df["re:v(2)"].to_numpy(), v2.real)
    np.testing.assert_array_equal(df["im:v(2)"].to_numpy(), v2.imag)


def test_noise_two_plots(examples: Path) -> None:
    res = spice.simulate(examples / "resnoise.cir")
    density, integrated = res
    assert density.plotname == "Noise Spectral Density Curves"
    assert integrated.plotname == "Integrated Noise"
    assert density.scale == "frequency" and density.npoints == 41
    assert integrated.scale is None and integrated.npoints == 1
    # 1k resistor at 27 C: sqrt(4kTR) = 4.07 nV/sqrt(Hz), flat.
    onoise = density.column("onoise_spectrum")
    np.testing.assert_allclose(onoise, 4.0713e-9, rtol=1e-4)
    assert density.df().shape == (41, len(density.variables))
    assert integrated.df().shape == (1, len(integrated.variables))


def test_pole_zero_has_no_scale(examples: Path) -> None:
    pz = spice.simulate(examples / "simplepz.cir")[0]
    assert pz.scale is None and pz.is_complex
    assert pz.df().columns == [
        "re:v(pole(1))",
        "im:v(pole(1))",
        "re:v(zero(1))",
        "im:v(zero(1))",
    ]


def test_string_deck_with_includes() -> None:
    deck = "divider\n.include parts/r.lib\nv1 1 0 10\n.op\n.end\n"
    lib = "r1 1 2 1k\nr2 2 0 1k\n"
    op = spice.simulate(deck, includes={"parts/r.lib": lib})[0]
    assert op.plotname == "Operating Point"
    assert op.column("v(2)")[0] == pytest.approx(5.0)
    assert op.df().shape == (1, len(op.variables))


def test_options_override(examples: Path) -> None:
    deck = (examples / "rc.cir").read_text()
    base = spice.simulate(deck)[0]
    fine = spice.simulate(deck, options={"RELTOL": 1e-6, "method": "gear", "maxord": 2})[0]
    assert fine.npoints > base.npoints


def test_options_temp_is_celsius() -> None:
    deck = "diode\nd1 1 0 dmod\ni1 0 1 1m\n.model dmod d\n.op\n.end\n"
    cold = spice.simulate(deck)[0].column("v(1)")[0]
    hot = spice.simulate(deck, options={"temp": 127.0})[0].column("v(1)")[0]
    assert hot < cold  # forward drop falls ~2 mV/K


def test_no_analyses_note() -> None:
    res = spice.simulate("empty\nr1 1 0 1k\n.end\n")
    assert len(res) == 0
    assert res.notes == ["no analyses in the deck; nothing to do"]


def test_iteration_and_lookup_errors(examples: Path) -> None:
    res = spice.simulate(examples / "resnoise.cir")
    assert [p.plotname for p in res.plots] == [p.plotname for p in res]
    with pytest.raises(KeyError, match="Integrated Noise"):
        res.plot("AC Analysis")
    with pytest.raises(IndexError):
        res.plot(5)
    with pytest.raises(KeyError, match="onoise_spectrum"):
        res[0].column("v(42)")
