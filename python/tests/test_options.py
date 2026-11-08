"""``options=`` keys of the transient-noise, device-noise and keepopinfo cards.

Each override is, bit for bit, the deck with the ``.options`` card edited
(the same mirror the wasm smoke test and the R tests check).
"""

from __future__ import annotations

import pytest

import spice

NOISY = """parity
vn 1 0 dc 0.5 trnoise(10m 1u 0 0)
r1 1 2 1k
c1 2 0 2n
{card}
.tran 1u 20u
.end
"""
AC = """keep
v1 1 0 dc 1 ac 1
r1 1 2 1k
c1 2 0 1n
{card}
.ac dec 2 1k 100k
.end
"""
LTRA = "ltra\nv1 1 0 pulse(0 1 1n 1n 1n 20n 100n)\nr1 1 2 50\no1 2 0 3 0 lin\nr2 3 0 50\n.model lin ltra r=0.1 l=9.13u c=3.65p len=1\n{card}\n.tran 0.5n 60n\n.end\n"


def bits(res: spice.Result) -> list[tuple[str, list[bytes]]]:
    return [(p.plotname, [p.column(n).tobytes() for n in p.names]) for p in res]


@pytest.mark.parametrize(
    ("deck", "options", "card"),
    [
        (AC, {"keepopinfo": True}, ".options keepopinfo"),
        (LTRA, {"trytocompact": True}, ".options trytocompact"),
        (NOISY, {"seed": 9}, ".options seed=9"),
        (NOISY, {"notrnoise": True}, ".options notrnoise"),
        (
            NOISY,
            {"DevNoise": True, "devnt": 2e-7, "seed": 5},
            ".options devnoise devnt=0.2u seed=5",
        ),
        (NOISY, {"method": "trbdf2"}, ".options method=trbdf2"),
    ],
)
def test_override_is_the_edited_deck(
    deck: str, options: dict[str, spice.OptionValue], card: str
) -> None:
    got = spice.simulate(deck.format(card=""), options=options)
    want = spice.simulate(deck.format(card=card))
    assert bits(got) == bits(want)
    assert bits(got) != bits(spice.simulate(deck.format(card="")))
    sess = spice.Session(deck.format(card=""), options=options)
    assert bits(sess.run()) == bits(want)


def test_keepopinfo_adds_the_operating_point() -> None:
    res = spice.simulate(AC.format(card=""), options={"keepopinfo": True})
    assert [p.plotname for p in res] == ["AC Operating Point", "AC Analysis"]


def test_false_clears_the_deck_flags() -> None:
    flagged = NOISY.format(card=".options notrnoise devnoise")
    got = spice.simulate(flagged, options={"notrnoise": False, "devnoise": False})
    assert bits(got) == bits(spice.simulate(NOISY.format(card="")))
    ac = spice.simulate(AC.format(card=".options keepopinfo"), options={"keepopinfo": False})
    assert [p.plotname for p in ac] == ["AC Analysis"]


def test_new_names_are_listed() -> None:
    for name in ("seed", "notrnoise", "devnoise", "devnt", "keepopinfo", "trytocompact"):
        assert name in spice.OPTION_NAMES


@pytest.mark.parametrize(
    ("options", "exc"),
    [
        ({"seed": 0}, ValueError),
        ({"seed": -2}, ValueError),
        ({"seed": 2**53}, ValueError),
        ({"seed": 1.5}, TypeError),
        ({"seed": True}, TypeError),
        ({"seed": "random"}, TypeError),
        ({"devnt": 0}, ValueError),
        ({"devnt": -1e-9}, ValueError),
        ({"devnt": float("inf")}, ValueError),
        ({"devnt": True}, TypeError),
        ({"devnoise": 1}, TypeError),
        ({"notrnoise": "yes"}, TypeError),
        ({"keepopinfo": 0}, TypeError),
    ],
)
def test_bad_values(options: dict[str, spice.OptionValue], exc: type[Exception]) -> None:
    with pytest.raises(exc):
        spice.simulate(NOISY.format(card=""), options=options)
