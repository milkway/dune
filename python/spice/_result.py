"""Simulation results: :class:`Result` (one run) and :class:`Plot` (one analysis plot)."""

from __future__ import annotations

from collections.abc import Iterator, Sequence
from typing import TYPE_CHECKING, Any, Literal, NamedTuple, overload

import numpy as np
import numpy.typing as npt
import polars as pl

from ._errors import Diagnostic

if TYPE_CHECKING:
    import pandas  # type: ignore[import-untyped,unused-ignore]

__all__ = ["Column", "Plot", "Result", "Variable"]

Column = npt.NDArray[np.float64] | npt.NDArray[np.complex128]
"""A plot vector: ``float64`` in real plots, ``complex128`` in complex ones."""


class Variable(NamedTuple):
    """One vector of a plot: its name and its 3f5 type name.

    ``kind`` is the rawfile ``Variables:`` type (``"time"``,
    ``"frequency"``, ``"voltage"``, ``"current"``, ``"pole"``,
    ``"onoise-spectrum"``, ...).
    """

    name: str
    kind: str


class Plot:
    """One analysis plot, as a rawfile holds it.

    Most analyses give one plot; ``.noise`` gives the spectral density plus,
    when the band has more than one frequency, the integrated totals;
    ``.disto`` gives 2 or 3 plots. The vectors keep the SPICE names
    (``v(2)``, ``i(vin)``, ...) and are looked up case-insensitively, as in
    the SPICE front end.

    Columns are read-only NumPy arrays that own no copy: they wrap the
    buffers the engine produced (see :meth:`column`).
    """

    __slots__ = ("_columns", "_complex", "_scale", "_variables", "plotname", "title")

    plotname: str
    """The rawfile ``Plotname:`` (``"Transient Analysis"``, ``"AC Analysis"``, ...)."""
    title: str
    """The deck's title line."""

    def __init__(self, native: dict[str, Any]) -> None:
        self.title = native["title"]
        self.plotname = native["plotname"]
        self._variables = tuple(Variable(n, k) for n, k in native["variables"])
        self._scale: int | None = native["scale"]
        self._complex: bool = native["complex"]
        columns: list[Column] = native["columns"]
        for c in columns:
            c.flags.writeable = False
        self._columns = tuple(columns)

    @property
    def variables(self) -> tuple[Variable, ...]:
        """Every vector of the plot, in rawfile order."""
        return self._variables

    @property
    def names(self) -> list[str]:
        """The vector names, in rawfile order."""
        return [v.name for v in self._variables]

    @property
    def scale(self) -> str | None:
        """Name of the abscissa (``time``, ``frequency``, ``v(v-sweep)``, ...).

        ``None`` for plots without one (operating point, ``.pz``, ``.tf``,
        integrated noise...).
        """
        return None if self._scale is None else self._variables[self._scale].name

    @property
    def is_complex(self) -> bool:
        """True when the vectors are complex (``.ac``, ``.pz``, ``.disto``...)."""
        return self._complex

    @property
    def npoints(self) -> int:
        """Number of points (rows) of every vector."""
        return len(self._columns[0]) if self._columns else 0

    def _index(self, name: str) -> int:
        key = name.lower()
        for i, v in enumerate(self._variables):
            if v.name.lower() == key:
                return i
        raise KeyError(f"no vector {name!r} in plot {self.plotname!r}; vectors: {self.names}")

    def __contains__(self, name: object) -> bool:
        if not isinstance(name, str):
            return False
        try:
            self._index(name)
        except KeyError:
            return False
        return True

    def column(self, name: str) -> Column:
        """One vector as a NumPy array, without copying.

        The array is ``float64`` in a real plot and ``complex128`` in a
        complex one (the scale of an ``.ac`` plot included: its imaginary
        part is zero, as in the rawfile). It is a zero-copy view of the
        engine's buffer and is read-only (``flags.writeable`` is False) so
        that the plot and every frame built from it stay consistent; use
        ``.copy()`` for a mutable array.

        Raises:
            KeyError: when the plot has no such vector (the name is matched
                case-insensitively).
        """
        return self._columns[self._index(name)]

    def _frame_columns(self) -> dict[str, npt.NDArray[Any]]:
        out: dict[str, npt.NDArray[Any]] = {}
        order = list(range(len(self._variables)))
        if self._scale is not None:
            order.remove(self._scale)
            order.insert(0, self._scale)
        for i in order:
            name = self._variables[i].name
            col = self._columns[i]
            if not self._complex:
                out[name] = col
            elif i == self._scale:
                out[name] = col.real
            else:
                out[f"re:{name}"] = col.real
                out[f"im:{name}"] = col.imag
        return out

    @overload
    def df(self, *, pandas: Literal[False] = False) -> pl.DataFrame: ...
    @overload
    def df(self, *, pandas: Literal[True]) -> pandas.DataFrame: ...
    def df(self, *, pandas: bool = False) -> pl.DataFrame | pandas.DataFrame:
        """The plot as a wide data frame: one row per point.

        Column order: the scale first (when the plot has one), then every
        other vector in rawfile order, named as in the plot. A complex
        vector becomes two ``Float64`` columns, ``re:<name>`` and
        ``im:<name>`` (polars has no complex dtype); the scale of a complex
        plot (``frequency``) is real by construction and stays one column
        with its own name. Non-finite values (NaN, inf) are kept as such.

        Args:
            pandas: Return a :class:`pandas.DataFrame` with the same columns
                instead of a polars one (needs the ``pandas`` extra).

        The polars frame is built from the read-only NumPy columns; polars
        may or may not copy them (it does for the strided real/imaginary
        parts of complex vectors).
        """
        cols = self._frame_columns()
        if pandas:
            try:
                import pandas as pd
            except ImportError as e:  # pragma: no cover - depends on the env
                raise ImportError(
                    "Plot.df(pandas=True) needs pandas: pip install 'spice[pandas]'"
                ) from e
            return pd.DataFrame({k: np.asarray(v) for k, v in cols.items()}, copy=True)
        return pl.DataFrame([pl.Series(k, v) for k, v in cols.items()])

    def __len__(self) -> int:
        return self.npoints

    def __repr__(self) -> str:
        kind = "complex" if self._complex else "real"
        return (
            f"<spice.Plot {self.plotname!r}: {len(self._variables)} vectors x "
            f"{self.npoints} points, {kind}>"
        )


class Result(Sequence[Plot]):
    """Everything one :func:`spice.simulate` run produced.

    A sequence of :class:`Plot`, in rawfile order (the order of the
    analysis cards in the deck); ``len(result)``, ``result[i]`` and
    iteration work as on a list.
    """

    __slots__ = ("_plots", "diagnostics", "notes", "title")

    title: str
    """The deck's title line."""
    notes: list[str]
    """Informational remarks from elaboration and from each analysis, in order."""
    diagnostics: list[Diagnostic]
    """Non-fatal parser and ``.options`` diagnostics (warnings)."""

    def __init__(self, native: dict[str, Any]) -> None:
        self.title = native["title"]
        self._plots = [Plot(p) for p in native["plots"]]
        self.notes = list(native["notes"])
        self.diagnostics = [Diagnostic._from_native(d) for d in native["diagnostics"]]

    @property
    def plots(self) -> list[Plot]:
        """Every plot of the run, in rawfile order."""
        return list(self._plots)

    def plot(self, key: str | int) -> Plot:
        """One plot, by position or by plotname.

        A name is matched case-insensitively against :attr:`Plot.plotname`
        (e.g. ``"AC Analysis"``); when several plots share it, the first one
        is returned.

        Raises:
            KeyError: no plot has that name.
            IndexError: the position is out of range.
        """
        if isinstance(key, int):
            return self._plots[key]
        wanted = key.lower()
        for p in self._plots:
            if p.plotname.lower() == wanted:
                return p
        raise KeyError(
            f"no plot named {key!r}; plots: {[p.plotname for p in self._plots]}"
        )

    @overload
    def __getitem__(self, index: int) -> Plot: ...
    @overload
    def __getitem__(self, index: slice) -> list[Plot]: ...
    def __getitem__(self, index: int | slice) -> Plot | list[Plot]:
        return self._plots[index]

    def __len__(self) -> int:
        return len(self._plots)

    def __iter__(self) -> Iterator[Plot]:
        return iter(self._plots)

    def __repr__(self) -> str:
        names = ", ".join(repr(p.plotname) for p in self._plots)
        return f"<spice.Result {self.title!r}: [{names}]>"
