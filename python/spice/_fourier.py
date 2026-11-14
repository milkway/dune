"""``.four`` results and :func:`spice.fourier`: harmonics and THD of a transient."""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING, Any

import polars as pl

from . import _spice
from ._errors import SpiceError

if TYPE_CHECKING:
    from ._result import Plot

__all__ = ["FOURIER_SCHEMA", "fourier"]

FOURIER_SCHEMA: dict[str, Any] = {
    "card": pl.Int64,
    "vector": pl.String,
    "harmonic": pl.Int64,
    "frequency": pl.Float64,
    "magnitude": pl.Float64,
    "phase": pl.Float64,
    "norm_magnitude": pl.Float64,
    "norm_phase": pl.Float64,
    "thd": pl.Float64,
    "gridsize": pl.Int64,
    "polydegree": pl.Int64,
    "nperiods": pl.Int64,
    "error": pl.String,
}
"""Column names and dtypes of :attr:`spice.Result.fourier` and :func:`spice.fourier`."""


def _frame(columns: dict[str, list[Any]] | None) -> pl.DataFrame:
    """The native column dict as the documented polars frame."""
    if columns is None:
        return pl.DataFrame(schema=FOURIER_SCHEMA)
    return pl.DataFrame({k: columns[k] for k in FOURIER_SCHEMA}, schema=FOURIER_SCHEMA)


def _count(value: object, name: str, minimum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an int, not {type(value).__name__}")
    if value < minimum:
        raise ValueError(f"{name} must be >= {minimum}, got {value}")
    return value


def fourier(
    plot: Plot,
    freq: float,
    vectors: str | Sequence[str],
    *,
    nfreqs: int = 10,
    nperiods: int = 1,
    polydegree: int = 1,
    gridsize: int = 200,
) -> pl.DataFrame:
    """Fourier analysis of a transient plot, as a ``.four`` card computes it.

    The harmonics and the total harmonic distortion of transient waveforms
    (ngspice-47's ``fourier``, the SPICE3f5 one plus ``nperiods``)::

        tran = spice.simulate(deck).plot("tran")
        spice.fourier(tran, 1e3, ["v(in)", "v(out)"], nfreqs=5)

    The last ``nperiods`` periods of the run (``nperiods / freq`` seconds,
    ending at the last time point) are resampled onto ``gridsize *
    nperiods`` uniform points by piecewise polynomial interpolation of
    degree ``polydegree`` (``0`` takes the raw samples as if they were
    uniform), and the first ``nfreqs`` harmonics of ``freq``, DC included,
    come from a discrete Fourier transform. Phases are in degrees, referred
    to a sine (a sine at zero phase reads 0, a cosine 90); the normalized
    columns divide by the fundamental and subtract its phase (not wrapped
    into +/-180). The THD, in percent, is ``100 * sqrt(sum(norm_magnitude**2
    for harmonic >= 2))``. The engine does the computing, exactly as for
    the deck's own ``.four`` cards (:attr:`spice.Result.fourier`); nothing
    is computed in Python.

    Args:
        plot: A ``.tran`` plot, or the time plot of a :func:`spice.pss`
            result.
        freq: The fundamental frequency, in hertz (positive).
        vectors: Output expressions as on a ``.four`` or ``.print`` card:
            ``"v(out)"``, ``"v(a,b)"``, ``"i(vin)"``, a vector name as the
            plot has it, or ``"all"`` (every vector of the plot, the time
            scale included). A single expression may be given as a string.
        nfreqs: Harmonics computed, DC included (``>= 1``; the ``.options``
            variable ``nfreqs``).
        nperiods: Periods analysed (``>= 1``; ``nperiods``).
        polydegree: Interpolation degree (``>= 0``; ``polydegree``).
        gridsize: Interpolation points per period (``>= 1``; the
            ``.options`` variable ``fourgridsize``).

    Returns:
        A polars frame with one row per harmonic of each vector and the
        columns of :data:`FOURIER_SCHEMA`: ``card`` (null here; for the
        deck's cards, the card's 0-based position among its ``.four``
        cards), ``vector``, ``harmonic`` (0 for DC), ``frequency``,
        ``magnitude`` (the signed mean for DC), ``phase`` (degrees),
        ``norm_magnitude``, ``norm_phase``, ``thd`` (percent, repeated on
        each of a vector's rows), ``gridsize`` (points transformed),
        ``polydegree``, ``nperiods`` and ``error``. A vector that cannot be
        analysed (unknown, or a run shorter than ``nperiods / freq``) is one
        row with ``error`` set, ``nan`` numbers and null integers -- not an
        exception. ``.to_pandas()`` gives a pandas frame.

    Raises:
        SpiceError: ``kind="fourier"`` when the plot is not a transient or an
            expression is malformed.
        TypeError, ValueError: malformed arguments.
    """
    if isinstance(vectors, str):
        vectors = [vectors]
    vectors = list(vectors)
    if not vectors or not all(isinstance(v, str) for v in vectors):
        raise TypeError("vectors must be a str or a non-empty sequence of str")
    if isinstance(freq, bool) or not isinstance(freq, int | float):
        raise TypeError(f"freq must be a number, not {type(freq).__name__}")
    freq = float(freq)
    if not (freq > 0.0 and freq != float("inf")):
        raise ValueError(f"freq must be a positive finite number, got {freq}")
    counts = (
        _count(nfreqs, "nfreqs", 1),
        _count(nperiods, "nperiods", 1),
        _count(polydegree, "polydegree", 0),
        _count(gridsize, "gridsize", 1),
    )
    try:
        native = _spice._fourier(
            plot.plotname,
            [(v.name, v.kind) for v in plot.variables],
            plot._scale,
            list(plot._columns),
            freq,
            vectors,
            *counts,
        )
    except _spice.NativeError as e:
        kind, message = e.args[:2]
        raise SpiceError(message, kind=kind) from None
    return _frame(native)
