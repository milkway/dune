"""``tmax_divisor=`` (issue #16): the deck with the card's TMAX divided, bit for bit."""

from __future__ import annotations

import pytest

import spice

RC = """rc
V1 Vin 0 PULSE(0 5 0 1u 1u 10m 20m)
R1 Vin Vout 1k
C1 Vout 0 1u
D1 Vout 0 dm
.model dm d is=1e-14
{card}
.end
"""
PLAIN = RC.format(card=".tran 20u 5m")
# Default TMAX min(20u, 5m/50) = 20u; a tenth is 2u.
EDITED = RC.format(card=".options trtol=1\n.tran 20u 5m 0 2u")


def bits(res: spice.Result) -> list[bytes]:
    return [p.column(n).tobytes() for p in res for n in p.names]


def test_simulate_is_the_edited_deck() -> None:
    fine = spice.simulate(PLAIN, options={"trtol": 1}, tmax_divisor=10)
    assert bits(fine) == bits(spice.simulate(EDITED))
    plain = spice.simulate(PLAIN)
    assert len(fine[0].column("time")) > len(plain[0].column("time"))
    assert bits(spice.simulate(PLAIN, tmax_divisor=None)) == bits(plain)
    assert bits(spice.simulate(PLAIN, tmax_divisor=1)) == bits(plain)


def test_sweep_is_the_edited_deck() -> None:
    sw = spice.sweep(PLAIN, {"r1": [1e3, 2e3]}, options={"trtol": 1}, tmax_divisor=10)
    want = spice.sweep(EDITED, {"r1": [1e3, 2e3]})
    for got, ref in zip(sw, want, strict=True):
        assert bits(got) == bits(ref)


def test_session_toggles_without_reopening() -> None:
    s = spice.Session(PLAIN, options={"trtol": 1})
    assert s.tmax_divisor is None
    s.tmax_divisor = 10
    assert s.tmax_divisor == 10.0
    assert bits(s.run()) == bits(spice.simulate(EDITED))
    s.tmax_divisor = None
    assert bits(s.run()) == bits(spice.simulate(PLAIN, options={"trtol": 1}))
    opened = spice.Session(PLAIN, options={"trtol": 1}, tmax_divisor=10)
    assert bits(opened.run()) == bits(spice.simulate(EDITED))


@pytest.mark.parametrize("bad", [0.5, 0, -1, float("nan"), float("inf")])
def test_out_of_range_is_value_error(bad: float) -> None:
    with pytest.raises(ValueError, match="tmax_divisor"):
        spice.simulate(PLAIN, tmax_divisor=bad)
    with pytest.raises(ValueError, match="tmax_divisor"):
        spice.sweep(PLAIN, {"r1": [1e3]}, tmax_divisor=bad)
    s = spice.Session(PLAIN, tmax_divisor=4)
    with pytest.raises(ValueError, match="tmax_divisor"):
        s.tmax_divisor = bad
    assert s.tmax_divisor == 4.0


@pytest.mark.parametrize("bad", [True, "10", [10]])
def test_wrong_type_is_type_error(bad: object) -> None:
    with pytest.raises(TypeError, match="tmax_divisor"):
        spice.simulate(PLAIN, tmax_divisor=bad)  # type: ignore[arg-type]


def test_not_an_option_key() -> None:
    with pytest.raises(ValueError, match="unknown option"):
        spice.simulate(PLAIN, options={"tmaxdivisor": 10})
