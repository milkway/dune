"""``options={"seed": ...}`` and the deck's random functions (D-0075).

The option is ``.options seed=`` written in the deck, bit for bit, in every
entry point that takes options (the same mirror the R tests check).
"""

from __future__ import annotations

import pytest

import spice

# Mismatch through a .param (redrawn at each reference), a model and a
# subcircuit, plus a value that reads temper.
MISMATCH = """mismatch
{card}
.param rv=agauss(1k, 100, 3) q=aunif(1, 0.05)
.model dm d is={{1e-14*q}}
.subckt seg a b
r1 a b {{rv}}
.ends
v1 1 0 dc 2
x1 1 2 seg
x2 2 3 seg
d1 3 0 dm
r9 3 0 {{rv + temper}}
.op
.dc v1 0 2 0.5
.end
"""

FIGURE = """.PS
spice = 1
V1: vsource((0,1), (0,0), 1)
R1: resistor((0,1), (1.5,1), "{{rv}}")
R2: resistor((1.5,1), (1.5,0), 1k)
line from R2.N to V1.N
ground((0.7,0))
.param rv=agauss(1k, 100, 3)
{card}
.op
.PE
"""


def bits(res: spice.Result) -> list[tuple[str, list[bytes]]]:
    return [(p.plotname, [p.column(n).tobytes() for n in p.names]) for p in res]


def deck(card: str = "") -> str:
    return MISMATCH.format(card=card)


def test_nominal_by_default() -> None:
    res = spice.simulate(deck())
    assert res.seed is None
    assert res.random_mode == "nominal"
    assert any("nominal value" in d.message for d in res.diagnostics)
    # Nominal: every random function at its first argument.
    fixed = deck().replace("agauss(1k, 100, 3)", "1k").replace("aunif(1, 0.05)", "1")
    assert bits(res) == bits(spice.simulate(fixed))


def test_same_seed_same_tables_another_seed_other_numbers() -> None:
    a = spice.simulate(deck(), options={"seed": 9})
    b = spice.simulate(deck(), options={"seed": 9})
    c = spice.simulate(deck(), options={"seed": 10})
    assert bits(a) == bits(b)
    assert bits(a) != bits(c)
    assert (a.seed, a.random_mode) == (9, "seeded")
    assert a.plot("op").df().equals(b.plot("op").df())
    assert not any("nominal value" in d.message for d in a.diagnostics)


@pytest.mark.parametrize("seed", [1, 9, 2**32 - 1])
def test_the_option_is_the_deck_card(seed: int) -> None:
    want = spice.simulate(deck(f".options seed={seed}"))
    assert want.seed == seed
    got = spice.simulate(deck(), options={"seed": seed})
    assert bits(got) == bits(want)
    assert got.seed == want.seed and got.random_mode == want.random_mode
    # The option wins over the deck's own seed, as a later card would not.
    over = spice.simulate(deck(".options seed=3"), options={"seed": seed})
    assert bits(over) == bits(want)
    # Every entry point: session, sweep (options only), pss below.
    assert bits(spice.Session(deck(), options={"seed": seed}).run()) == bits(want)
    sw = spice.sweep(deck(), {"v1": [2.0]}, options={"seed": seed})
    assert bits(sw[0]) == bits(want)
    assert sw[0].seed == seed


def test_random_returns_its_seed_and_rerunning_with_it_repeats_the_run() -> None:
    res = spice.simulate(deck(), options={"seed": "random"})
    assert isinstance(res.seed, int) and 1 <= res.seed <= 2**32 - 1
    assert res.random_mode == "seeded"
    assert any(f"seed {res.seed}" in n for n in res.notes)
    again = spice.simulate(deck(), options={"seed": res.seed})
    assert bits(again) == bits(res)
    # The same through a session.
    sess = spice.Session(deck(), options={"seed": "RANDOM"}).run()
    assert sess.seed is not None
    assert bits(spice.simulate(deck(), options={"seed": sess.seed})) == bits(sess)


def test_a_deck_asking_for_a_random_seed_gets_one() -> None:
    res = spice.simulate(deck(".options seed=random"))
    assert res.seed is not None and res.random_mode == "seeded"
    assert not any("seed=random" in d.message for d in res.diagnostics)
    assert bits(spice.simulate(deck(f".options seed={res.seed}"))) == bits(res)
    # The caller's integer seed wins over the deck's request.
    fixed = spice.simulate(deck(".options seed=random"), options={"seed": 4})
    assert fixed.seed == 4
    assert bits(fixed) == bits(spice.simulate(deck(".options seed=4")))


def test_a_deck_without_random_functions_reports_the_seed_it_ran_with() -> None:
    plain = "div\nv1 1 0 dc 2\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n"
    assert spice.simulate(plain).seed is None
    res = spice.simulate(plain, options={"seed": 12})
    assert (res.seed, res.random_mode) == (12, "seeded")
    assert bits(res) == bits(spice.simulate(plain))


def test_the_seed_reaches_the_transient_noise() -> None:
    noisy = "n\nvn 1 0 dc 0 trnoise(10m 1u 0 0)\nr1 1 0 1k\n{card}\n.tran 1u 10u\n.end\n"
    got = spice.simulate(noisy.format(card=""), options={"seed": 77})
    assert bits(got) == bits(spice.simulate(noisy.format(card=".options seed=77")))


def test_pss_takes_the_seed() -> None:
    rc = """rc
.param rv=agauss(1k, 100, 3)
v1 1 0 sin(0 1 1k)
r1 1 2 {rv}
c1 2 0 100n
.end
"""
    want = spice.pss(rc.replace(".param", ".options seed=5\n.param"), freq=1e3, tstab=2e-3)
    got = spice.pss(rc, options={"seed": 5}, freq=1e3, tstab=2e-3)
    assert bits(got) == bits(want)
    assert got.seed == 5
    assert spice.pss(rc, freq=1e3, tstab=2e-3).seed is None


def test_a_figure_takes_the_seed() -> None:
    ex = spice.pic.extract(FIGURE.format(card=""))
    want = spice.simulate(spice.pic.extract(FIGURE.format(card=".options seed=6")))
    got = spice.simulate(ex, options={"seed": 6})
    assert bits(got) == bits(want)
    assert got.seed == 6
