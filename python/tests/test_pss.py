"""spice.pss: the .pss card's engine call, with its period and shooting history."""

from __future__ import annotations

import math
from pathlib import Path

import numpy as np
import pytest

import spice

RC = """rc low-pass, periodic steady state
v1 1 0 sin(0 1 1k)
r1 1 2 1k
c1 2 0 1u
.end
"""

#: The card the bindings' defaults spell out (128 points, 10 harmonics).
RC_CARD = ".pss 1k 5m 2 128 10\n"


def with_card(deck: str, card: str) -> str:
    return deck.replace(".end\n", card + ".end\n")


def assert_same_plots(got: spice.Result, want: spice.Result) -> None:
    """Same plots and vectors, bit for bit, same notes and warnings."""
    assert len(got) == len(want)
    assert got.notes == want.notes
    assert got.diagnostics == want.diagnostics
    for g, w in zip(got, want, strict=True):
        assert g.plotname == w.plotname
        assert g.variables == w.variables
        assert g.is_complex == w.is_complex
        for name in w.names:
            a, b = g.column(name), w.column(name)
            assert a.dtype == b.dtype
            assert a.tobytes() == b.tobytes(), f"{w.plotname}: {name}"


def test_pss_is_the_card_bit_for_bit() -> None:
    card = spice.simulate(with_card(RC, RC_CARD))
    res = spice.pss(RC, freq=1e3, tstab=5e-3, osc_node="2")
    assert isinstance(res, spice.PssResult)
    assert isinstance(res, spice.Result)
    assert_same_plots(res, card)
    assert [p.plotname for p in res] == [spice.PSS_TIME_PLOT, spice.PSS_FREQ_PLOT]
    assert res.measurements == []


@pytest.mark.parametrize(
    ("name", "kwargs"),
    [
        ("pss_rc.cir", {"freq": 1e3, "tstab": 5e-3, "osc_node": "2"}),
        ("pss_rlc.cir", {"freq": 1.5e3, "tstab": 0.0, "osc_node": "3", "points": 256}),
        ("pss_rect.cir", {"freq": 50e3, "tstab": 20e-6, "osc_node": "2", "points": 256}),
        ("pss_pulse.cir", {"freq": 1e6, "tstab": 2e-6, "osc_node": "3", "points": 200}),
    ],
)
def test_corpus_decks_match_simulate(corpus: Path, name: str, kwargs: dict[str, object]) -> None:
    # The deck files with their .pss cards against the same parameters as
    # arguments (the rectifier and the pulse train take several Newton steps).
    path = corpus / "pss" / name
    got = spice.pss(path, **kwargs)  # type: ignore[arg-type]
    assert_same_plots(got, spice.simulate(path))
    assert got.residuals[-1] <= 1


def test_result_fields() -> None:
    res = spice.pss(RC, freq=1e3, tstab=5e-3)
    assert res.period == pytest.approx(1e-3, rel=1e-15)
    assert res.t0 == pytest.approx(5e-3, rel=1e-12)
    # Linear: one Newton step.
    assert len(res.residuals) == 2
    assert res.residuals[0] > 1 >= res.residuals[-1]
    t = res.time
    assert t is res[0] and res.spectrum is res[1]
    assert not t.is_complex and t.npoints == 129
    assert t.column("time")[0] == pytest.approx(res.t0)
    assert t.column("time")[-1] == pytest.approx(res.t0 + res.period)
    s = res.spectrum
    assert s.is_complex and s.npoints == 10 and s.scale == "frequency"
    assert s.column("frequency").real[1] == pytest.approx(1e3)
    # Closed form: v(2) = |H| sin(wt + arg H), H = 1/(1 + j 2 pi).
    h = 1 / (1 + 2j * math.pi)
    x1 = s.column("v(2)")[1]
    assert abs(x1) == pytest.approx(abs(h), rel=5e-4)
    # sin(wt + phi) = Re(-j e^{j phi} e^{jwt}): the phasor is -j H.
    assert x1 == pytest.approx(-1j * h, rel=5e-4)
    assert abs(s.column("v(2)")[0]) < 1e-6
    assert "pss: period" in " ".join(res.notes)
    assert "PssResult" in repr(res)


def test_freq_zero_takes_the_sources() -> None:
    a = spice.pss(RC, tstab=5e-3)
    b = spice.pss(RC, freq=1e3, tstab=5e-3)
    assert a.period == b.period
    assert a.time.column("v(2)").tobytes() == b.time.column("v(2)").tobytes()
    assert any("from the sources" in n for n in a.notes)


def test_deck_analysis_cards_are_not_run() -> None:
    deck = with_card(RC, ".tran 10u 1m\n.op\n")
    res = spice.pss(deck, freq=1e3)
    assert [p.plotname for p in res] == [spice.PSS_TIME_PLOT, spice.PSS_FREQ_PLOT]


def test_convergence_failure_carries_the_history() -> None:
    with pytest.raises(spice.PssConvergenceError) as info:
        spice.pss(RC, freq=1e3, tstab=5e-3, max_iter=1)
    e = info.value
    assert isinstance(e, spice.SpiceError)
    assert e.kind == "pss"
    assert e.iterations == 1
    assert len(e.residuals) == 1 and e.residuals[0] > 1
    assert "did not converge" in str(e)
    # The same failure through a .pss card in simulate() and in a Session.
    deck = with_card(RC, ".pss 1k 5m 2 128 10 1\n")
    with pytest.raises(spice.PssConvergenceError) as card:
        spice.simulate(deck)
    assert card.value.residuals == e.residuals
    with pytest.raises(spice.PssConvergenceError):
        spice.Session(deck).run()


def test_other_failures_stay_spice_errors() -> None:
    no_source = "dc only\nv1 1 0 dc 1\nr1 1 0 1k\n.end\n"
    with pytest.raises(spice.SpiceError) as info:
        spice.pss(no_source)
    assert info.value.kind == "analysis"
    assert not isinstance(info.value, spice.PssConvergenceError)
    with pytest.raises(spice.SpiceError, match="oscnode"):
        spice.pss(RC, freq=1e3, osc_node="nope")
    with pytest.raises(spice.SpiceError) as parse:
        spice.pss("t\nq1 1 1 0\n.end\n")
    assert parse.value.kind == "parse"


@pytest.mark.parametrize(
    ("kwargs", "exc"),
    [
        ({"freq": -1.0}, ValueError),
        ({"freq": math.inf}, ValueError),
        ({"freq": "1k"}, TypeError),
        ({"tstab": -1e-3}, ValueError),
        ({"points": 3}, ValueError),
        ({"points": 128.0}, TypeError),
        ({"points": True}, TypeError),
        ({"harmonics": 0}, ValueError),
        ({"harmonics": 65}, ValueError),
        ({"max_iter": 0}, ValueError),
        ({"steady_coeff": 0.0}, ValueError),
        ({"uic": 1}, TypeError),
        ({"osc_node": 2}, TypeError),
        ({"options": {"reltl": 1e-4}}, ValueError),
        ({"tmax_divisor": 0.5}, ValueError),
    ],
)
def test_bad_arguments(kwargs: dict[str, object], exc: type[Exception]) -> None:
    with pytest.raises(exc):
        spice.pss(RC, **kwargs)  # type: ignore[arg-type]


def test_options_reach_the_shooting() -> None:
    loose = spice.pss(RC, freq=1e3, tstab=5e-3)
    tight = spice.pss(RC, freq=1e3, tstab=5e-3, points=1024, options={"reltol": 1e-7})
    h = abs(1 / (1 + 2j * math.pi))

    def err(r: spice.PssResult) -> float:
        return float(abs(abs(r.spectrum.column("v(2)")[1]) - h) / h)

    assert err(tight) < err(loose) / 10


def test_measure_the_pss_plots() -> None:
    res = spice.pss(RC, freq=1e3, tstab=5e-3)
    h = abs(1 / (1 + 2j * math.pi))
    pp = spice.measure(res.time, "pp v(2)", name="vpp")
    assert pp.ok and pp.analysis == "tran" and pp.name == "vpp"
    assert pp.value == pytest.approx(2 * h, rel=1e-3)
    when = spice.measure(res.time, "when v(2)=0 rise=1")
    assert when.ok and res.t0 <= when.value <= res.t0 + res.period
    h1 = spice.measure(res.spectrum, "find vm(2) at=1k")
    assert h1.ok and h1.analysis == "ac"
    assert h1.value == pytest.approx(abs(res.spectrum.column("v(2)")[1]), rel=1e-12)


def test_session_runs_a_pss_deck() -> None:
    deck = with_card(RC, RC_CARD)
    s = spice.Session(deck)
    assert_same_plots(s.run(), spice.simulate(deck))
    s.alter("r1", "resistance", 2e3)
    assert_same_plots(s.run(), spice.simulate(deck.replace("r1 1 2 1k", "r1 1 2 2k")))


def test_columns_are_numpy() -> None:
    res = spice.pss(RC, freq=1e3)
    assert res.time.column("v(2)").dtype == np.float64
    assert res.spectrum.column("v(2)").dtype == np.complex128
    df = res.spectrum.df()
    assert df.columns[:3] == ["frequency", "re:v(1)", "im:v(1)"]
