"""``spice.sweep``: run a deck over a set of parameter assignments (grid or Monte Carlo)."""

from __future__ import annotations

import itertools
import os
from collections.abc import Iterator, Mapping, Sequence
from typing import TYPE_CHECKING, Any, Literal, cast, overload

import numpy as np
import numpy.typing as npt
import polars as pl

from . import _spice
from ._errors import Diagnostic, ErrorKind, SpiceError
from ._result import Result
from ._simulate import OptionValue, _deck_source, _native_errors
from .pic import PicExtraction

if TYPE_CHECKING:
    import pandas  # type: ignore[import-untyped,unused-ignore]

__all__ = ["SweepMode", "SweepResult", "sweep"]

SweepMode = Literal["grid", "zip"]
"""How :func:`sweep` combines its parameter values: Cartesian product or row by row."""

RUN_COLUMN = "run"
"""Name of the run-index column :meth:`SweepResult.df` puts first."""


def sweep(
    netlist: str | os.PathLike[str] | PicExtraction,
    params: Mapping[str, npt.ArrayLike],
    *,
    mode: SweepMode = "grid",
    includes: Mapping[str, str] | None = None,
    options: Mapping[str, OptionValue] | None = None,
    threads: int = 1,
    tmax_divisor: float | None = None,
) -> SweepResult:
    """Run a SPICE3 deck once per parameter assignment.

    The deck stays pure SPICE3 (there is no ``.step`` card): the sweep lives
    here. It is parsed once; each run writes its values into the parsed
    deck, builds the circuit afresh and runs every analysis card, exactly as
    :func:`simulate` would on the deck edited by hand -- bit for bit.

    The simulator holds no random generator: a Monte Carlo is a ``"zip"``
    sweep over draws you make, so it is reproducible from your own seed::

        rng = np.random.default_rng(1)
        spice.sweep(deck, {"r1": rng.normal(1e3, 50, 200),
                           "temp": rng.uniform(0, 70, 200)}, mode="zip")

    Args:
        netlist: The deck, as in :func:`simulate` (deck text or a path).
        params: Swept quantity -> its values (any 1-D array-like of numbers;
            a polars ``DataFrame.to_dict()`` works). Keys, matched
            case-insensitively, are ``"temp"`` (degrees Celsius, as
            ``.options temp=``); an element name -- ``"r1"``, ``"c1"``,
            ``"l1"`` for its value, ``"vin"``/``"iin"`` for a source's DC
            level; or the ngspice form ``"@name[param]"``:
            ``@r1[resistance]``, ``@c1[capacitance]``, ``@l1[inductance]``,
            ``@vin[dc]``, ``@vin[acmag]``, ``@vin[acphase]`` (degrees),
            ``@m1[w]``, ``@m1[l]`` (MOSFET width/length, m). Elements inside
            subcircuits go by their expanded names (``r.x1.r1``). Model
            parameters are not sweepable yet.
        mode: ``"grid"`` runs every combination (the Cartesian product, the
            first parameter varying slowest, as :func:`itertools.product`);
            ``"zip"`` runs row by row and needs equal lengths.
        includes: As in :func:`simulate`.
        options: As in :func:`simulate`; applied over the deck's
            ``.options`` for every run (a swept ``temp`` overrides
            ``options["temp"]``).
        threads: Worker threads: ``1`` (the default) runs sequentially;
            ``0`` uses one per core; ``n`` uses ``n``. Results are identical
            either way -- every run is an independent circuit. The GIL is
            released while the runs execute.
        tmax_divisor: As in :func:`simulate`, for every run.

    Returns:
        A :class:`SweepResult`: one :class:`Result` per run plus tidy frames
        with the swept values prepended.

    Raises:
        ValueError: an unknown element or parameter, a parameter given
            twice, unequal lengths in ``"zip"`` mode, a non-finite value, a
            negative ``threads``; and what :func:`simulate` raises for
            ``options``/``includes``/``tmax_divisor``.
        SpiceError: the deck does not parse, or a run fails to build or
            simulate (the message names the run and its values).
        OSError: the deck file cannot be read.
    """
    text, path, inc = _deck_source(netlist, includes)
    if mode not in ("grid", "zip"):
        raise ValueError(f"mode must be 'grid' or 'zip', not {mode!r}")
    if isinstance(threads, bool) or not isinstance(threads, int) or threads < 0:
        raise ValueError(f"threads must be a non-negative int, not {threads!r}")
    if not isinstance(params, Mapping) or not params:
        raise ValueError("params must be a non-empty mapping of name -> values")

    names = list(params)
    columns: list[npt.NDArray[np.float64]] = []
    for name in names:
        if not isinstance(name, str):
            raise TypeError(f"parameter names must be str, not {type(name).__name__}")
        try:
            col = np.asarray(params[name], dtype=np.float64)
        except (TypeError, ValueError) as e:
            raise TypeError(f"values of {name!r} must be numbers") from e
        if col.ndim == 0:
            col = col.reshape(1)
        if col.ndim != 1:
            raise ValueError(f"values of {name!r} must be 1-D, got shape {col.shape}")
        if col.size == 0:
            raise ValueError(f"values of {name!r} are empty")
        columns.append(col)
    if mode == "zip":
        lengths = {len(c) for c in columns}
        if len(lengths) != 1:
            sizes = ", ".join(f"{n}={len(c)}" for n, c in zip(names, columns, strict=True))
            raise ValueError(f"zip mode needs equal lengths, got {sizes}")
        rows = np.column_stack(columns).tolist()
    else:
        rows = [list(r) for r in itertools.product(*(c.tolist() for c in columns))]

    with _native_errors():
        native = _spice._sweep(
            text,
            path,
            inc,
            None if options is None else dict(options),
            names,
            rows,
            threads,
            tmax_divisor,
        )
    keys: list[str] = native["params"]
    for i, run in enumerate(native["runs"]):
        if not run["ok"]:
            values = ", ".join(f"{k}={v:g}" for k, v in zip(keys, rows[i], strict=True))
            raise SpiceError(
                f"run {i} ({values}): {run['message']}",
                kind=cast(ErrorKind, run["kind"]),
            )
    return SweepResult(native, rows)


class SweepResult(Sequence[Result]):
    """Everything one :func:`spice.sweep` produced.

    A sequence of :class:`Result`, one per run, in run order (``res[i]`` is
    run ``i``); :attr:`params` holds the values each run assigned and
    :meth:`df` stacks one plot of every run into a single tidy frame.
    """

    __slots__ = ("_keys", "_results", "_values", "diagnostics", "title")

    title: str
    """The deck's title line."""
    diagnostics: list[Diagnostic]
    """Non-fatal parser and ``.options`` diagnostics of the deck (parsed once)."""

    def __init__(self, native: dict[str, Any], rows: list[list[float]]) -> None:
        self.title = native["title"]
        self.diagnostics = [Diagnostic._from_native(d) for d in native["diagnostics"]]
        self._keys: tuple[str, ...] = tuple(native["params"])
        self._values = np.asarray(rows, dtype=np.float64).reshape(len(rows), len(self._keys))
        self._results = [
            Result(
                {
                    "title": self.title,
                    "plots": r["plots"],
                    "notes": r["notes"],
                    "measurements": r["measurements"],
                    "diagnostics": [],
                }
            )
            for r in native["runs"]
        ]

    @property
    def names(self) -> list[str]:
        """The swept parameters, in canonical form (``"r1"``, ``"@m1[w]"``, ``"temp"``)."""
        return list(self._keys)

    @property
    def params(self) -> pl.DataFrame:
        """One row per run: ``run`` (0-based) then the value of every swept parameter."""
        cols = [pl.Series(RUN_COLUMN, np.arange(len(self._results), dtype=np.int64))]
        cols += [pl.Series(k, self._values[:, j]) for j, k in enumerate(self._keys)]
        return pl.DataFrame(cols)

    @property
    def plotnames(self) -> list[str]:
        """The plot names of a run (every run of a deck has the same plots)."""
        return [p.plotname for p in self._results[0]] if self._results else []

    @overload
    def df(self, plot: str | int = 0, *, pandas: Literal[False] = False) -> pl.DataFrame: ...
    @overload
    def df(self, plot: str | int = 0, *, pandas: Literal[True]) -> pandas.DataFrame: ...
    def df(
        self, plot: str | int = 0, *, pandas: bool = False
    ) -> pl.DataFrame | pandas.DataFrame:
        """One plot of every run, stacked into one tidy frame.

        The columns are ``run`` (``Int64``, 0-based), one ``Float64`` column
        per swept parameter (named as in :attr:`names`), then the plot's own
        columns exactly as :meth:`Plot.df` gives them; rows are run 0's
        points, then run 1's, and so on. Group by ``run`` (or by a
        parameter) to get one curve per run.

        Args:
            plot: The plot, by position or plotname (as :meth:`Result.plot`).
            pandas: Return a :class:`pandas.DataFrame` instead (needs pandas).
        """
        frames = []
        for i, res in enumerate(self._results):
            frame = res.plot(plot).df()
            lead = [pl.lit(i, dtype=pl.Int64).alias(RUN_COLUMN)]
            lead += [
                pl.lit(float(self._values[i, j]), dtype=pl.Float64).alias(k)
                for j, k in enumerate(self._keys)
            ]
            frames.append(frame.select([*lead, pl.all()]))
        out = pl.concat(frames, how="vertical") if frames else pl.DataFrame()
        if pandas:
            try:
                import pandas as pd
            except ImportError as e:  # pragma: no cover - depends on the env
                raise ImportError(
                    "SweepResult.df(pandas=True) needs pandas: pip install 'spice3[pandas]'"
                ) from e
            # Column by column through NumPy, as Plot.df: no pyarrow needed.
            return pd.DataFrame({c: out[c].to_numpy() for c in out.columns}, copy=True)
        return out

    @overload
    def __getitem__(self, index: int) -> Result: ...
    @overload
    def __getitem__(self, index: slice) -> list[Result]: ...
    def __getitem__(self, index: int | slice) -> Result | list[Result]:
        return self._results[index]

    def __len__(self) -> int:
        return len(self._results)

    def __iter__(self) -> Iterator[Result]:
        return iter(self._results)

    def __repr__(self) -> str:
        return (
            f"<spice.SweepResult {self.title!r}: {len(self._results)} runs over "
            f"{list(self._keys)}, plots {self.plotnames}>"
        )
