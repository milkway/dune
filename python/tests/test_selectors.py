"""Short plot names, variable selection, polar columns and ``Result.op``."""

import math

import numpy as np
import pytest

import spice

DECK = """rc low-pass
vin in 0 dc 2 ac 1 pulse(0 1 0 1u)
r1 in out 1k
c1 out 0 1u
.op
.ac dec 5 10 10k
.tran 0.1m 5m
.end
"""


@pytest.fixture(scope="module")
def res() -> spice.Result:
    return spice.simulate(DECK)


def test_short_names_select_the_plot(res: spice.Result) -> None:
    assert res.plot("op").plotname == "Operating Point"
    assert res.plot("ac").plotname == "AC Analysis"
    assert res.plot("TRAN") is res.plot("Transient Analysis")
    with pytest.raises(KeyError, match="short names"):
        res.plot("noise")


def test_a_bare_node_name_is_its_voltage(res: spice.Result) -> None:
    tran = res.plot("tran")
    assert "out" in tran
    assert tran.column("out") is tran.column("v(out)")
    with pytest.raises(KeyError):
        tran.column("nowhere")


def test_df_keeps_the_selected_vectors(res: spice.Result) -> None:
    tran = res.plot("tran")
    assert tran.df("out").columns == ["time", "v(out)"]
    assert tran.df(["v(out)", "IN"]).columns == ["time", "v(in)", "v(out)"]
    assert tran.df().columns == tran.df(None).columns
    with pytest.raises(KeyError):
        tran.df(["v(out)", "nowhere"])


def test_polar_columns_of_a_complex_plot(res: spice.Result) -> None:
    ac = res.plot("ac")
    plain = ac.df("out")
    assert plain.columns == ["frequency", "re:v(out)", "im:v(out)"]
    df = ac.df("out", polar=True)
    assert df.columns == [
        "frequency",
        "re:v(out)",
        "im:v(out)",
        "db:v(out)",
        "mag:v(out)",
        "phase:v(out)",
    ]
    v = ac.column("v(out)")
    np.testing.assert_array_equal(df["mag:v(out)"].to_numpy(), np.abs(v))
    np.testing.assert_allclose(df["db:v(out)"].to_numpy(), 20 * np.log10(np.abs(v)))
    np.testing.assert_allclose(df["phase:v(out)"].to_numpy(), np.degrees(np.angle(v)))
    # The RC pole: -3 dB and -45 degrees at 1/(2 pi RC).
    f = df["frequency"].to_numpy()
    k = int(np.argmin(np.abs(f - 1 / (2 * math.pi * 1e-3))))
    assert abs(df["db:v(out)"][k] + 3.0) < 0.5
    assert abs(df["phase:v(out)"][k] + 45.0) < 5.0
    # A real plot ignores the flag.
    assert res.plot("tran").df("out", polar=True).columns == ["time", "v(out)"]


def test_op_is_a_dict(res: spice.Result) -> None:
    op = res.op()
    assert op["v(in)"] == 2.0
    assert op["v(out)"] == pytest.approx(2.0)
    assert list(res.op("out")) == ["v(out)"]
    assert list(res.op(["v(out)", "in"])) == ["v(in)", "v(out)"]
    with pytest.raises(KeyError):
        res.op("nowhere")
    with pytest.raises(KeyError, match="short names"):
        spice.simulate("t\nv1 1 0 1\nr1 1 0 1k\n.tran 1m 2m\n.end\n").op()
