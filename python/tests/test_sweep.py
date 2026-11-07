"""spice.sweep: grids, Monte Carlo draws, bit-for-bit agreement with simulate."""

from __future__ import annotations

import itertools
import re
from pathlib import Path

import numpy as np
import polars as pl
import pytest

import spice

DIVIDER = """divider
v1 1 0 dc 2
r1 1 2 1k
r2 2 0 1k
.op
.dc v1 0 2 1
.end
"""


def _diffpair(examples: Path) -> str:
    text = (examples / "diffpair.cir").read_text()
    return re.sub(r"(?m)^\*(\.(tf|dc|ac) )", r"\1", text)


def _edited(deck: str, rc1: float, vcc: float, temp: float) -> str:
    """The deck as edited by hand for one run (repr round-trips the f64)."""
    return (
        deck.replace("rc1 4 8 10k", f"rc1 4 8 {rc1!r}")
        .replace("vcc 8 0 12", f"vcc 8 0 {vcc!r}")
        .replace(".tran 5ns", f".options temp={temp!r}\n.tran 5ns")
    )


def test_grid_shape_and_order() -> None:
    res = spice.sweep(DIVIDER, {"R2": [1e3, 3e3], "v1": [1.0, 2.0, 4.0]})
    assert isinstance(res, spice.SweepResult)
    assert len(res) == 6
    assert res.names == ["r2", "v1"]
    p = res.params
    assert p.columns == ["run", "r2", "v1"]
    # itertools.product order: first parameter slowest.
    assert p["r2"].to_list() == [1e3, 1e3, 1e3, 3e3, 3e3, 3e3]
    assert p["v1"].to_list() == [1.0, 2.0, 4.0] * 2
    assert res.plotnames == ["Operating Point", "DC transfer characteristic"]
    op = res.df("Operating Point")
    assert op.columns[:3] == ["run", "r2", "v1"]
    assert op.height == 6
    want = p["v1"] * p["r2"] / (1e3 + p["r2"])
    np.testing.assert_allclose(op["v(2)"].to_numpy(), want.to_numpy(), rtol=1e-12)
    dc = res.df(1)
    assert dc.height == 6 * 3
    assert dc["run"].to_list() == [r for r in range(6) for _ in range(3)]


def test_zip_needs_equal_lengths() -> None:
    with pytest.raises(ValueError, match="equal lengths"):
        spice.sweep(DIVIDER, {"r2": [1e3, 2e3], "v1": [1.0]}, mode="zip")
    res = spice.sweep(DIVIDER, {"r2": [1e3, 2e3], "v1": [1.0, 3.0]}, mode="zip")
    assert res.params["v1"].to_list() == [1.0, 3.0]


def test_bad_parameters() -> None:
    with pytest.raises(ValueError, match="no element `r9`"):
        spice.sweep(DIVIDER, {"r9": [1.0]})
    with pytest.raises(ValueError, match="no sweepable parameter `w`"):
        spice.sweep(DIVIDER, {"@r1[w]": [1.0]})
    with pytest.raises(ValueError, match="given twice"):
        spice.sweep(DIVIDER, {"r1": [1.0], "@R1[resistance]": [2.0]})
    with pytest.raises(ValueError, match="not finite"):
        spice.sweep(DIVIDER, {"r1": [np.nan]})
    with pytest.raises(ValueError, match="mode"):
        spice.sweep(DIVIDER, {"r1": [1.0]}, mode="product")  # type: ignore[arg-type]
    with pytest.raises(ValueError, match="threads"):
        spice.sweep(DIVIDER, {"r1": [1.0]}, threads=-1)


def test_failing_run_names_its_values() -> None:
    with pytest.raises(spice.SpiceError, match=r"run 1 \(r2=0\)") as e:
        spice.sweep(DIVIDER, {"r2": [1e3, 0.0]})
    assert e.value.kind == "elaborate"


def test_sweep_matches_individual_runs_bit_for_bit(examples: Path) -> None:
    deck = _diffpair(examples)
    grid = {"rc1": [8e3, 12.5e3], "vcc": [10.0, 12.0], "temp": [-40.0, 85.0]}
    res = spice.sweep(deck, grid)
    assert len(res) == 8
    for run, (rc1, vcc, temp) in zip(res, itertools.product(*grid.values()), strict=True):
        single = spice.simulate(_edited(deck, rc1, vcc, temp))
        assert [p.plotname for p in run] == [p.plotname for p in single]
        for got, want in zip(run, single, strict=True):
            assert got.names == want.names
            for name in got.names:
                a, b = got.column(name), want.column(name)
                # Bitwise, not approximately.
                assert a.tobytes() == b.tobytes(), (got.plotname, name)


def test_threads_give_identical_bits(examples: Path) -> None:
    deck = _diffpair(examples)
    rng = np.random.default_rng(10)
    draws = {"rc1": rng.normal(10e3, 500, 16), "temp": rng.uniform(0, 70, 16)}
    seq = spice.sweep(deck, draws, mode="zip")
    for threads in (0, 4):
        par = spice.sweep(deck, draws, mode="zip", threads=threads)
        for name in seq.plotnames:
            assert seq.df(name).equals(par.df(name))


def test_monte_carlo_is_reproducible_from_the_seed(examples: Path) -> None:
    deck = examples / "rc.cir"

    def mc(seed: int) -> pl.DataFrame:
        rng = np.random.default_rng(seed)
        n = 25
        draws = {"r": rng.normal(1.0, 0.05, n), "c": rng.normal(1.0, 0.05, n)}
        return spice.sweep(deck, draws, mode="zip").df("Transient Analysis")

    a, b, c = mc(42), mc(42), mc(7)
    assert a.equals(b)
    assert not a.equals(c)
    assert a.columns[:3] == ["run", "r", "c"]
    # One time constant spread per run: final v(2) depends on R*C.
    final = a.group_by("run", maintain_order=True).last()
    tau = final["r"] * final["c"]
    want = 1 - (np.exp(0.1 / tau) - 1) / (0.1 / tau) * np.exp(-final["time"] / tau)
    np.testing.assert_allclose(final["v(2)"].to_numpy(), np.asarray(want), rtol=2e-3)


def test_ac_and_mosfet_geometry(examples: Path) -> None:
    res = spice.sweep(
        examples / "mosmem.cir", {"@m1[w]": [200e-6, 300e-6], "@m3[L]": [5e-6]}
    )
    assert res.names == ["@m1[w]", "@m3[l]"]
    tr = res.df("Transient Analysis")
    assert tr.columns[:3] == ["run", "@m1[w]", "@m3[l]"]
    runs = tr.partition_by("run")
    assert not np.array_equal(runs[0]["v(5)"].to_numpy(), runs[1]["v(5)"].to_numpy())

    rc = "rc\nvin 1 0 dc 0 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.ac dec 5 10 10k\n.end\n"
    ac = spice.sweep(rc, {"@vin[acmag]": [1.0, 2.0]}).df("AC Analysis")
    r0, r1 = ac.partition_by("run")
    np.testing.assert_array_equal(r1["re:v(2)"].to_numpy(), 2 * r0["re:v(2)"].to_numpy())


def test_pandas_frame() -> None:
    pd = pytest.importorskip("pandas")
    pdf = spice.sweep(DIVIDER, {"r2": [1e3, 2e3]}).df(0, pandas=True)
    assert isinstance(pdf, pd.DataFrame)
    assert list(pdf.columns[:2]) == ["run", "r2"]
