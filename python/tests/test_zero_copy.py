"""NumPy columns wrap the engine's buffers: no copy, read-only."""

from __future__ import annotations

from pathlib import Path

import numpy as np
import pytest

import spice


def test_real_column_is_a_readonly_view(examples: Path) -> None:
    tran = spice.simulate(examples / "rc.cir")[0]
    a = tran.column("v(2)")
    b = tran.column("v(2)")
    assert a is b  # same array every time, nothing rebuilt
    assert not a.flags.owndata  # backed by the Rust Vec, not a NumPy copy
    assert not a.flags.writeable
    assert a.flags.c_contiguous
    with pytest.raises(ValueError, match="read-only"):
        a[0] = 42.0
    c = a.copy()
    c[0] = 42.0  # a copy is mutable and leaves the plot alone
    assert tran.column("v(2)")[0] == 0.0


def test_complex_column_is_a_readonly_view(corpus: Path) -> None:
    ac = spice.simulate(corpus / "basic" / "acrc.cir")[0]
    z = ac.column("v(2)")
    assert z.dtype == np.complex128
    assert not z.flags.owndata and not z.flags.writeable and z.flags.c_contiguous
    with pytest.raises(ValueError, match="read-only"):
        z[0] = 0


def test_polars_frame_shares_real_columns(examples: Path) -> None:
    tran = spice.simulate(examples / "rc.cir")[0]
    s = tran.df()["v(2)"].to_numpy()
    # polars adopted the buffer as is (observed with polars >= 1.0; not
    # promised by the API, so only real columns are checked).
    assert np.shares_memory(s, tran.column("v(2)"))
