"""``.measure`` cards and spice.measure (the engine measures; Python wraps)."""

from __future__ import annotations

import math
from pathlib import Path

import pytest

import spice

RC = """rc step
v1 in 0 pulse(0 1 1n 1p 1p 20n 40n)
r1 in out 1k
c1 out 0 1p
.tran 10p 10n
.meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1
.meas tran vmax max v(out)
.meas tran never when v(out)=5
.end
"""


def test_deck_measurements() -> None:
    res = spice.simulate(RC)
    names = [m.name for m in res.measurements]
    assert names == ["trise", "vmax", "never"]
    trise = res.measurement("TRISE")
    assert trise.ok and trise.analysis == "tran" and trise.plot == 0
    # 10-90 % rise of a 1 ns RC: tau*ln 9 (simulation error ~1e-5).
    assert math.isclose(trise.value, 1e-9 * math.log(9), rel_tol=1e-4)
    assert set(trise.details) == {"trig", "targ"}
    assert math.isclose(trise.details["targ"] - trise.details["trig"], trise.value)
    assert float(res.measurement("vmax")) == res.measurement("vmax").value
    assert res.measurement("vmax").details["at"] == 10e-9
    never = res.measurement("never")
    assert not never.ok and math.isnan(never.value)
    assert never.error == "out of interval (WHEN)"
    assert dict(never.details) == {}
    with pytest.raises(KeyError, match="trise"):
        res.measurement("nope")


def test_measure_api_matches_the_cards() -> None:
    res = spice.simulate(RC)
    tran = res.plot("Transient Analysis")
    m = spice.measure(tran, "trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1")
    assert m.value == res.measurement("trise").value
    assert m.name == "measure" and m.plot is None and m.analysis == "tran"
    d = spice.measure(tran, "deriv v(out) at=2n", name="slope")
    assert d.name == "slope"
    k = 1e-9 / 1e-12 * math.expm1(1e-3)
    assert math.isclose(d.value, k / 1e-9 * math.exp(-1.0), rel_tol=1e-3)
    assert spice.measure(tran, "when v(out)=7").error == "out of interval (WHEN)"
    assert spice.measure(tran, "max v(9)").error is not None


def test_measure_errors() -> None:
    res = spice.simulate(RC + "")
    tran = res[0]
    with pytest.raises(spice.SpiceError) as e:
        spice.measure(tran, "max v(out) bogus=1")
    assert e.value.kind == "measure"
    assert "no such parameter" in str(e.value) and "column 12" in str(e.value)
    with pytest.raises(TypeError):
        spice.measure(tran, 3)  # type: ignore[arg-type]
    op = spice.simulate("op\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n")[0]
    with pytest.raises(spice.SpiceError, match="cannot be measured"):
        spice.measure(op, "max v(1)")


def test_ac_parts(corpus: Path) -> None:
    res = spice.simulate(corpus / "measure" / "meas_ac.cir")
    ac = res.plot("AC Analysis")
    # The complex columns go back to the engine; vdb/vp/vm are its parts.
    f3 = spice.measure(ac, "when vdb(out)=-3")
    assert f3.value == res.measurement("f3db").value
    ph = spice.measure(ac, "find vp(out) at=1k")
    assert -math.pi / 2 < ph.value < 0


def test_measure_rawfile_of_ngspice(corpus: Path) -> None:
    """Same numbers as ngspice's own .meas on ngspice's own plot."""
    golden = corpus / "measure" / "golden"
    expect = {}
    for line in (golden / "meas_rlc.meas").read_text().splitlines():
        name, rest = line.split("=", 1)
        expect[name.strip()] = float(rest.split()[0])
    deck = (corpus / "measure" / "meas_rlc.cir").read_text()
    res = spice.simulate(deck)
    tran = res[0]
    # Re-measuring Dune's own plot with the card bodies gives the cards'
    # values; ngspice's numbers on its own plot agree to the simulation.
    for line in deck.splitlines():
        if not line.startswith(".meas"):
            continue
        _, _, name, body = line.split(maxsplit=3)
        m = spice.measure(tran, body, name=name)
        assert m.value == res.measurement(name).value
        assert math.isclose(m.value, expect[name], rel_tol=1e-4), name
