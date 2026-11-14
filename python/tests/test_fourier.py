"""``.four`` cards and spice.fourier (the engine transforms; Python wraps).

Mirror of r/tests/testthat/test-fourier.R.
"""

from __future__ import annotations

import math
import re
from pathlib import Path
from typing import Any

import numpy as np
import polars as pl
import pytest

import spice
from spice._result import Plot

DECKS = ["four_sine", "four_square", "four_clip", "four_rect"]
COLUMNS = [
    "card",
    "vector",
    "harmonic",
    "frequency",
    "magnitude",
    "phase",
    "norm_magnitude",
    "norm_phase",
    "thd",
    "gridsize",
    "polydegree",
    "nperiods",
    "error",
]
RC = "rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 100n\n.tran 10u 5m\n.four 1k v(2)\n.end\n"


def read_golden(path: Path) -> list[dict[str, Any]]:
    """ngspice-47's tables (17 digits) in corpus/four/golden/*.four."""
    lines = path.read_text().splitlines()
    out = []
    for i, line in enumerate(lines):
        if not line.startswith("Fourier analysis for "):
            continue
        info = lines[i + 1]
        n = int(re.search(r"No. Harmonics: (\d+),", info).group(1))  # type: ignore[union-attr]
        rows = [[float(x) for x in r.split()] for r in lines[i + 5 : i + 5 + n]]
        out.append(
            {
                "vector": line[len("Fourier analysis for ") : -1],
                "thd": float(re.search(r"THD: (\S+) %", info).group(1)),  # type: ignore[union-attr]
                "gridsize": int(re.search(r"Gridsize: (\d+),", info).group(1)),  # type: ignore[union-attr]
                # harmonic, frequency, magnitude, phase, norm. mag, norm. phase
                "table": rows,
            }
        )
    return out


def read_real_raw(path: Path) -> Plot:
    """The one real plot of an ngspice binary rawfile, as a :class:`spice.Plot`.

    The package has no rawfile reader (the R one does); the layout of a real
    binary plot is a text header, ``Binary:`` and the points as rows of
    float64, one value per variable.
    """
    data = path.read_bytes()
    head, _, body = data.partition(b"Binary:\n")
    lines = head.decode().splitlines()
    nvars = int(next(ln for ln in lines if ln.startswith("No. Variables:")).split(":")[1])
    npoints = int(next(ln for ln in lines if ln.startswith("No. Points:")).split(":")[1])
    start = lines.index("Variables:") + 1
    # "\t<index>\t<name>\t<type>" per variable.
    variables = [tuple(ln.split()[1:3]) for ln in lines[start : start + nvars]]
    rows = np.frombuffer(body, dtype="<f8", count=nvars * npoints).reshape(npoints, nvars)
    plotname = next(ln for ln in lines if ln.startswith("Plotname:")).split(":", 1)[1].strip()
    return Plot(
        {
            "title": "",
            "plotname": plotname,
            "variables": variables,
            "scale": 0,
            "complex": False,
            "columns": [rows[:, i].copy() for i in range(nvars)],
        }
    )


def four_cards(cir: Path) -> list[tuple[float, list[str]]]:
    """The deck's ``.four`` cards: fundamental and vectors, in deck order."""
    cards = []
    for line in cir.read_text().splitlines():
        if line.startswith(".four"):
            w = line.split()
            cards.append((float(w[1].replace("k", "e3")), w[2:]))
    return cards


def four_options(cir: Path) -> dict[str, int]:
    """The deck's ``.options`` for ``.four``, as spice.fourier arguments."""
    opt = " ".join(ln for ln in cir.read_text().splitlines() if ln.startswith(".options"))

    def get(key: str, default: int) -> int:
        m = re.search(rf"\b{key}=(\d+)", opt)
        return int(m.group(1)) if m else default

    return {
        "nfreqs": get("nfreqs", 10),
        "nperiods": get("nperiods", 1),
        "polydegree": get("polydegree", 1),
        "gridsize": get("fourgridsize", 200),
    }


def test_deck_fourier(corpus: Path) -> None:
    res = spice.simulate(corpus / "four" / "four_sine.cir")
    f = res.fourier
    assert isinstance(f, pl.DataFrame)
    assert f.columns == COLUMNS
    assert f.schema == pl.Schema(spice.FOURIER_SCHEMA)
    assert f.height == 20
    assert f["card"].to_list() == [0] * 20
    assert f["vector"].to_list() == ["v(in)"] * 10 + ["v(out)"] * 10
    assert f["harmonic"].to_list() == list(range(10)) * 2
    assert f["frequency"].to_list() == [k * 1e3 for k in range(10)] * 2
    assert f["error"].null_count() == 20
    assert set(f["gridsize"]) == {200}
    assert set(f["polydegree"]) == {1}
    assert set(f["nperiods"]) == {1}
    # A 1 V sine: |V1| = 1, THD ~ 0; the RC (1 kHz, tau = 0.1 ms) lags.
    vin = f.filter(pl.col("vector") == "v(in)")
    assert math.isclose(vin["magnitude"][1], 1.0, rel_tol=1e-3)
    assert vin["thd"][0] < 1e-3
    assert vin["thd"].n_unique() == 1
    vout = f.filter(pl.col("vector") == "v(out)")
    wrc = 2 * math.pi * 1e3 * 1e-4
    assert math.isclose(vout["magnitude"][1], 1 / math.sqrt(1 + wrc**2), rel_tol=1e-3)
    assert math.isclose(vout["phase"][1], -math.degrees(math.atan(wrc)), rel_tol=1e-2)
    assert vout["norm_magnitude"][1] == 1.0
    assert vout["norm_phase"][1] == 0.0


def test_deck_without_four_has_an_empty_frame() -> None:
    res = spice.simulate("op\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n")
    assert res.fourier.height == 0
    assert res.fourier.schema == pl.Schema(spice.FOURIER_SCHEMA)


@pytest.mark.parametrize("deck", DECKS)
def test_fourier_api_matches_the_cards_bit_for_bit(corpus: Path, deck: str) -> None:
    cir = corpus / "four" / f"{deck}.cir"
    res = spice.simulate(cir)
    tran = res.plot("tran")
    opts = four_options(cir)
    for k, (freq, vectors) in enumerate(four_cards(cir)):
        got = spice.fourier(tran, freq, vectors, **opts)
        want = res.fourier.filter(pl.col("card") == k)
        assert got["card"].null_count() == got.height
        assert got.drop("card").equals(want.drop("card"), null_equal=True), (deck, k)


def test_deck_options_reach_the_four_cards(corpus: Path) -> None:
    f = spice.simulate(corpus / "four" / "four_rect.cir").fourier
    assert f["card"].unique(maintain_order=True).to_list() == [0, 1]
    assert f["vector"].unique(maintain_order=True).to_list() == ["v(out)", "i(v1)"]
    assert f.height == 32
    assert set(f["gridsize"]) == {1024}
    assert set(f["polydegree"]) == {2}
    assert set(f["nperiods"]) == {2}
    assert f.filter(pl.col("card") == 1)["frequency"][1] == 120.0


@pytest.mark.parametrize("deck", DECKS)
def test_fourier_of_ngspice_plot_matches_ngspice(corpus: Path, deck: str) -> None:
    """Same tables as ngspice's own .four on ngspice's own plot."""
    cir = corpus / "four" / f"{deck}.cir"
    golden = corpus / "four" / "golden"
    tran = read_real_raw(golden / f"{deck}.raw")
    opts = four_options(cir)
    cards = four_cards(cir)
    for g in read_golden(golden / f"{deck}.four"):
        freq = next(f for f, vs in cards if g["vector"] in vs)
        got = spice.fourier(tran, freq, g["vector"], **opts)
        _check_against_golden(got, g)


def _check_against_golden(got: pl.DataFrame, g: dict[str, Any]) -> None:
    tab = g["table"]
    label = g["vector"]
    assert got["harmonic"].to_list() == [int(r[0]) for r in tab], label
    assert got["gridsize"][0] == g["gridsize"], label
    m1 = tab[1][2]
    for row, want in zip(got.iter_rows(named=True), tab, strict=True):
        assert math.isclose(row["frequency"], want[1], rel_tol=1e-12), label
        assert abs(row["magnitude"] - want[2]) < 1e-10 * m1, label
        # Phases where the harmonic stands clear of roundoff.
        if row["norm_magnitude"] > 1e-3:
            assert abs(row["phase"] - want[3]) < 1e-6, label
    # The header prints the THD with 6 digits.
    assert math.isclose(got["thd"][0], g["thd"], rel_tol=1e-5), label


def test_failed_vector_is_a_row(corpus: Path) -> None:
    tran = spice.simulate(corpus / "four" / "four_sine.cir").plot("tran")
    f = spice.fourier(tran, 1e3, ["v(9)", "v(out)"], nfreqs=3)
    assert f.height == 4
    assert f["vector"].to_list() == ["v(9)"] + ["v(out)"] * 3
    bad = f.row(0, named=True)
    assert bad["error"] == "no such vector v(9)"
    assert math.isnan(bad["magnitude"]) and math.isnan(bad["thd"])
    assert bad["harmonic"] is None and bad["gridsize"] is None
    assert f["error"][1:].null_count() == 3
    # 5 ms of run, 10 periods asked for: too short.
    short = spice.fourier(tran, 1e3, "v(out)", nperiods=10)
    assert short["error"].to_list() == ["(10 * wavelength) longer than time span"]
    # A deck card that fails is a row too.
    deck = RC.replace(".tran 10u 5m", ".tran 10u 0.5m").replace("v(2)", "v(2) v(7)")
    rf = spice.simulate(deck).fourier
    assert rf["error"].to_list() == [
        "(1 * wavelength) longer than time span",
        "no such vector v(7)",
    ]
    assert rf["card"].to_list() == [0, 0]


def test_all_expands_to_every_vector(corpus: Path) -> None:
    tran = spice.simulate(corpus / "four" / "four_sine.cir").plot("tran")
    f = spice.fourier(tran, 1e3, "all", nfreqs=2)
    assert f["vector"].unique(maintain_order=True).to_list() == tran.names


def test_fourier_errors(corpus: Path) -> None:
    tran = spice.simulate(corpus / "four" / "four_sine.cir").plot("tran")
    with pytest.raises(ValueError):
        spice.fourier(tran, 0, "v(out)")
    with pytest.raises(TypeError):
        spice.fourier(tran, "1k", "v(out)")  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        spice.fourier(tran, 1e3, [])
    with pytest.raises(ValueError):
        spice.fourier(tran, 1e3, "v(out)", nfreqs=0)
    with pytest.raises(TypeError):
        spice.fourier(tran, 1e3, "v(out)", nperiods=1.5)  # type: ignore[arg-type]
    with pytest.raises(ValueError):
        spice.fourier(tran, 1e3, "v(out)", polydegree=-1)
    with pytest.raises(spice.SpiceError) as e:
        spice.fourier(tran, 1e3, "v(out")
    assert e.value.kind == "fourier"
    op = spice.simulate("op\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n")[0]
    with pytest.raises(spice.SpiceError, match="not a transient"):
        spice.fourier(op, 1e3, "v(1)")


def test_sweep_runs_and_sessions_carry_their_fourier() -> None:
    sw = spice.sweep(RC, {"r1": [1e3, 2e3]})
    m1 = [run.fourier["magnitude"][1] for run in sw]
    assert m1[1] < m1[0]
    s = spice.Session(RC)
    assert s.run().fourier.equals(spice.simulate(RC).fourier, null_equal=True)
