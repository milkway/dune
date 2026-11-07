"""spice.Session: alter + run equals simulate() of the altered deck."""

from __future__ import annotations

from pathlib import Path
from typing import Any

import numpy as np
import pytest

import spice

DECK = """session deck
vin 1 0 dc 1 ac 1 pulse(0 1 1u 1u 1u 4u 10u)
r1 1 2 1k
c1 2 0 1n
l1 2 3 1u
r2 3 0 50
d1 2 0 dm
m1 4 2 0 0 nm l=2u w=10u
rd 5 4 10k
vdd 5 0 dc 5
.model dm d is=1e-14 cjo=1p
.model nm nmos level=1 vto=0.7 kp=50u cgso=1n cgdo=1n
.op
.dc vin 0 2 0.5
.ac dec 5 1k 1meg
.tran 0.2u 10u
.end
"""


def card(mantissa: int, k: int = 0) -> float:
    """The double SPICE3's ``INPevaluate`` reads for ``<mantissa>e<k>``.

    The card parser is the 3f5's, byte for byte: integer mantissa times a
    correctly rounded power of ten -- two roundings, so ``1.5`` on a card is
    ``10u`` is ``10 * 1e-6 == 9.999999999999999e-06``, not ``1e-05``. An alteration equals
    the altered card when it carries the card's double.
    """
    return mantissa * float(f"1e{k}")


def assert_same(got: spice.Result, want: spice.Result) -> None:
    """Same plots and vectors, bit for bit, same notes and warnings."""
    assert len(got) == len(want)
    assert got.notes == want.notes
    assert got.diagnostics == want.diagnostics
    for g, w in zip(got, want, strict=True):
        assert g.plotname == w.plotname
        assert g.variables == w.variables
        for name in w.names:
            a, b = g.column(name), w.column(name)
            assert a.dtype == b.dtype
            assert a.tobytes() == b.tobytes(), f"{w.plotname}: {name}"


def test_unaltered_run_is_simulate() -> None:
    assert_same(spice.Session(DECK).run(), spice.simulate(DECK))


def test_alter_r_c_l_source_mos_temp_matches_the_altered_deck() -> None:
    s = spice.Session(DECK)
    s.alter("R1", 2048.0).alter("c1", "capacitance", 2e-9).alter("l1", 2e-6)
    s.alter("vin", "dc", card(15, -1)).alter("vin", "acphase", 30.0)
    s.alter("vin", "pulse", [0, 2, *(card(n, -6) for n in (1, 1, 1, 4, 10))])
    s.alter("m1", "w", card(20, -6)).alter("m1", "l", card(4, -6))
    s.temp = 60
    altered = (
        DECK.replace("r1 1 2 1k", "r1 1 2 2048")
        .replace("c1 2 0 1n", "c1 2 0 2n")
        .replace("l1 2 3 1u", "l1 2 3 2u")
        .replace("dc 1 ac 1 pulse(0 1", "dc 1.5 ac 1 30 pulse(0 2")
        .replace("l=2u w=10u", "l=4u w=20u")
        .replace(".op\n", ".options temp=60\n.op\n")
    )
    assert_same(s.run(), spice.simulate(altered))


def test_options_and_files_like_simulate(examples: Path) -> None:
    deck = examples / "rc.cir"
    s = spice.Session(deck, options={"reltol": 1e-4})
    assert_same(s.run(), spice.simulate(deck, options={"reltol": 1e-4}))


def test_reads_and_errors() -> None:
    s = spice.Session(DECK)
    assert s.param("r1") == 1e3
    assert s.param("vin", "pulse") == (0, 1, *(card(n, -6) for n in (1, 1, 1, 4, 10)))
    assert card(10, -6) != 1e-5
    assert s.param("vin", "sin") is None
    assert s.param("m1", "temp") is None
    assert s.alterable("m1") == ("w", "l", "temp")
    assert s.temp == pytest.approx(27.0)
    bad: list[tuple[Any, ...]] = [
        ("zz", 1.0),
        ("r1", 0.0),
        ("r1", "w", 1.0),
        ("vin", "pulse", 1.0),
    ]
    for args in bad:
        with pytest.raises(spice.SpiceError) as e:
            s.alter(*args)
        assert e.value.kind == "alter"
    with pytest.raises(TypeError):
        s.alter("r1", True)
    with pytest.raises(TypeError):
        s.alter("r1", "x")  # type: ignore[call-overload]
    with pytest.raises(spice.SpiceError) as e:
        s.temp = float("nan")
    assert e.value.kind == "alter"
    with pytest.raises(spice.SpiceError) as e:
        spice.Session("t\nr1 1 0\n.end\n")
    assert e.value.kind == "elaborate"
    assert_same(s.run(), spice.simulate(DECK))  # unchanged by the refusals


def test_warm_start_agrees_to_newton_tolerance() -> None:
    op = DECK.replace(".dc vin 0 2 0.5\n", "").replace(".tran 0.2u 10u\n", "")
    cold, warm = spice.Session(op), spice.Session(op)
    warm.warm_start = True
    assert warm.warm_start and not cold.warm_start
    for w in [12e-6, 14e-6, 9e-6]:
        a = cold.alter("m1", "w", w).run().plot("Operating Point")
        b = warm.alter("m1", "w", w).run().plot("Operating Point")
        for name in a.names:
            x, y = a.column(name), b.column(name)
            tol = 1e-12 if name.startswith("i(") else 1e-6
            assert np.allclose(y, x, rtol=2e-3, atol=2 * tol), name
    assert warm.warm_stats.hits >= 3
    assert cold.warm_stats == spice.WarmStats(0, 0)


def test_numpy_values() -> None:
    s = spice.Session(DECK)
    s.alter("r1", np.float64(2048.0))
    s.alter("vin", "pwl", np.array([0.0, 0.0, card(1, -6), 1.0]))
    assert s.param("vin", "pwl") == (0.0, 0.0, 1e-6, 1.0)
    want = spice.simulate(
        DECK.replace("r1 1 2 1k", "r1 1 2 2048").replace(
            "pulse(0 1 1u 1u 1u 4u 10u)", "pwl(0 0 1u 1)"
        )
    )
    assert_same(s.run(), want)


RISE = """rc rise
v1 1 0 pulse(0 1 0 1n 1n 10m 20m)
r1 1 2 1k
c1 2 0 1u
.tran 2u 8m
.measure tran trise trig v(2) val=0.1 rise=1 targ v(2) val=0.9 rise=1
.end
"""


def test_run_measures_the_deck_cards() -> None:
    s = spice.Session(RISE)
    r1 = s.run()
    t1 = r1.measurement("trise").value
    assert t1 == spice.simulate(RISE).measurement("trise").value
    assert t1 == pytest.approx(np.log(9) * 1e-3, rel=1e-2)
    r2 = s.alter("r1", 2e3).run()
    fresh = spice.simulate(RISE.replace("r1 1 2 1k", "r1 1 2 2k"))
    assert [m.value for m in r2.measurements] == [m.value for m in fresh.measurements]
    assert r2.measurement("trise").value / t1 == pytest.approx(2.0, rel=1e-2)
