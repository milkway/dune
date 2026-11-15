"""Decks that read ``temper`` and the seed sweep (D-0075).

A deck whose values read ``temper`` is re-expanded by the engine whenever
the circuit temperature changes, so an ``options={"temp": t}`` override, a
:attr:`Session.temp` and a swept ``temp`` all give, bit for bit, the deck
with ``.options temp=t`` written in it. Mirrors ``r/tests/testthat/test-temper.R``
and the wasm unit test.
"""

from __future__ import annotations

import polars as pl
import pytest

import spice

# temper in a model value, a device value (a behavioural R) and a
# subcircuit parameter.
TEMPER = """temper
{card}
.model dm d is={{1e-14*(1 + 0.005*(temper - 27))}}
.subckt load a
.param rl={{1k + 10*temper}}
r1 a 0 {{rl}}
.ends
v1 1 0 dc 2
r1 1 2 {{1k*(1 + 0.004*(temper - 27))}}
d1 2 0 dm
x1 2 load
.op
.dc v1 0 2 0.5
.end
"""

MISMATCH = """mismatch
.param rv=agauss(1k, 100, 3)
v1 1 0 dc 1
r1 1 2 {rv}
r2 2 0 {1k + temper}
.op
.end
"""


def bits(res: spice.Result) -> list[tuple[str, list[bytes]]]:
    return [(p.plotname, [p.column(n).tobytes() for n in p.names]) for p in res]


def deck(card: str = "") -> str:
    return TEMPER.format(card=card)


@pytest.mark.parametrize("temp", [-40.0, 27.0, 85.0])
def test_a_temp_override_is_the_deck_card(temp: float) -> None:
    want = spice.simulate(deck(f".options temp={temp:g}"))
    got = spice.simulate(deck(), options={"temp": temp})
    assert bits(got) == bits(want)
    sess = spice.Session(deck())
    sess.temp = temp
    assert bits(sess.run()) == bits(want)
    sw = spice.sweep(deck(), {"temp": [temp]})
    assert bits(sw[0]) == bits(want)


def test_the_temperature_reaches_the_temper_values() -> None:
    cold = spice.simulate(deck(), options={"temp": -40})
    hot = spice.simulate(deck(), options={"temp": 85})
    assert bits(cold) != bits(hot)
    # The session follows each new temperature, and back.
    sess = spice.Session(deck())
    for temp in (85.0, -40.0, 27.0):
        sess.temp = temp
        assert bits(sess.run()) == bits(spice.simulate(deck(f".options temp={temp:g}")))


def test_a_seed_sweep_is_a_monte_carlo_of_the_deck() -> None:
    mc = spice.sweep(MISMATCH, {"seed": range(1, 9)})
    assert mc.names == ["seed"]
    assert mc.params["seed"].dtype == pl.Int64
    assert mc.params["seed"].to_list() == list(range(1, 9))
    for s, res in zip(range(1, 9), mc, strict=True):
        assert (res.seed, res.random_mode) == (s, "seeded")
        assert bits(res) == bits(spice.simulate(MISMATCH, options={"seed": s}))
    frame = mc.df("op")
    assert frame.columns[:2] == ["run", "seed"]
    assert frame["seed"].dtype == pl.Int64
    # Eight seeds, eight different draws.
    assert frame["v(2)"].n_unique() == 8


def test_a_seed_sweep_composes_with_temp_and_threads() -> None:
    grid = spice.sweep(MISMATCH, {"seed": [3, 4], "temp": [0.0, 50.0]})
    assert grid.params.select("seed", "temp").rows() == [(3, 0.0), (3, 50.0), (4, 0.0), (4, 50.0)]
    for res, (s, t) in zip(grid, grid.params.select("seed", "temp").rows(), strict=True):
        want = spice.simulate(MISMATCH, options={"seed": s, "temp": t})
        assert bits(res) == bits(want)
    par = spice.sweep(MISMATCH, {"seed": [3, 4], "temp": [0.0, 50.0]}, threads=3)
    assert [bits(r) for r in par] == [bits(r) for r in grid]
    # A swept seed wins over options["seed"].
    over = spice.sweep(MISMATCH, {"seed": [3]}, options={"seed": 99})
    assert over[0].seed == 3
    assert bits(over[0]) == bits(spice.simulate(MISMATCH, options={"seed": 3}))


def test_a_seed_sweep_without_random_functions_says_so() -> None:
    plain = "div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n"
    res = spice.sweep(plain, {"seed": [1, 2]})
    assert all(any("every run is the same circuit" in n for n in r.notes) for r in res)
    assert bits(res[0]) == bits(res[1])


@pytest.mark.parametrize("bad", [0, -1, 1.5, 2**32])
def test_a_bad_seed_in_a_sweep_is_a_value_error(bad: float) -> None:
    with pytest.raises(ValueError, match="not a seed"):
        spice.sweep(MISMATCH, {"seed": [1, bad]})
