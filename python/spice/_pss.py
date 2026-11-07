"""``spice.pss``: the periodic steady state of a driven circuit."""

from __future__ import annotations

import math
import os
from collections.abc import Mapping
from typing import Any

from . import _spice
from ._result import Plot, Result
from ._simulate import OptionValue, _deck_source, _native_errors
from .pic import PicExtraction

__all__ = ["PSS_FREQ_PLOT", "PSS_TIME_PLOT", "PssResult", "pss"]

PSS_TIME_PLOT = "Time Domain Periodic Steady State Analysis"
"""Plotname of the period in time (ngspice-47's name)."""
PSS_FREQ_PLOT = "Frequency Domain Periodic Steady State Analysis"
"""Plotname of the harmonics (ngspice-47's name)."""


class PssResult(Result):
    """What :func:`spice.pss` produced: a :class:`~spice.Result` of two plots.

    ``result[0]`` (:attr:`time`) and ``result[1]`` (:attr:`spectrum`) are the
    plots a ``.pss`` card gives in :func:`spice.simulate`, value for value;
    this class adds the period, where it starts and the shooting history.
    :attr:`~spice.Result.measurements` is always empty (a deck's ``.measure``
    cards do not apply to ``.pss`` plots; measure them with
    :func:`spice.measure`).
    """

    __slots__ = ("period", "residuals", "t0")

    period: float
    """The period ``T`` in seconds (``1/freq``)."""
    t0: float
    """Start of the reported period: the first multiple of ``T`` at or after
    ``tstab`` (at least one period of plain transient comes before it)."""
    residuals: tuple[float, ...]
    """The period mismatch ``max_i |x_i(t0+T) - x_i(t0)| / tol_i`` of each
    shooting iteration (``tol_i = reltol * peak_i + vntol`` for nodes,
    ``+ abstol`` for branches); the last one is ``<= steady_coeff``. A
    linear circuit converges in one Newton step: two entries."""

    def __init__(self, native: dict[str, Any]) -> None:
        super().__init__(native)
        self.period = float(native["period"])
        self.t0 = float(native["t0"])
        self.residuals = tuple(float(r) for r in native["residuals"])

    @property
    def time(self) -> Plot:
        """One period in time: ``points + 1`` equispaced rows from ``t0`` to
        ``t0 + period`` (scale ``time``; node voltages and branch currents as
        in a ``.tran``). Real."""
        return self[0]

    @property
    def spectrum(self) -> Plot:
        """The complex harmonics ``X_k``, ``k = 0 ... harmonics - 1``, at
        ``k / period`` (scale ``frequency``): ``X_0`` is the mean and ``x(t) =
        X_0 + sum Re(X_k exp(j k w t))``, the phasor convention of ``.ac``
        with phases referred to ``t = 0``. ``abs()`` of a column is
        ngspice's (magnitude-only) plot."""
        return self[1]

    def __repr__(self) -> str:
        return (
            f"<spice.PssResult {self.title!r}: period {self.period:g} s from "
            f"t0={self.t0:g} s, {len(self.residuals)} shooting iterations, "
            f"residual {self.residuals[-1]:.3g}>"
        )


def _number(name: str, v: object) -> float:
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise TypeError(f"{name} must be a number, not {type(v).__name__}")
    x = float(v)
    if not math.isfinite(x):
        raise ValueError(f"{name} must be finite, got {x}")
    return x


def _count(name: str, v: object, low: int, high: int) -> int:
    if isinstance(v, bool) or not isinstance(v, int):
        raise TypeError(f"{name} must be an int, not {type(v).__name__}")
    if not low <= v <= high:
        raise ValueError(f"{name} must be between {low} and {high}, got {v}")
    return v


def pss(
    netlist: str | os.PathLike[str] | PicExtraction,
    *,
    freq: float = 0.0,
    tstab: float = 0.0,
    osc_node: str | None = None,
    points: int = 128,
    harmonics: int = 10,
    max_iter: int = 50,
    steady_coeff: float = 1.0,
    uic: bool = False,
    includes: Mapping[str, str] | None = None,
    options: Mapping[str, OptionValue] | None = None,
    tmax_divisor: float | None = None,
) -> PssResult:
    """The periodic steady state of a driven circuit, by shooting-Newton.

    The arguments are those of the card ``.pss FREQ TSTAB OSCNODE POINTS
    HARMS SCITER STEADYCOEFF UIC``, and the result is, value for value, what
    that card gives in :func:`spice.simulate` -- the same engine call --
    plus the period, ``t0`` and the shooting history. The deck supplies the
    circuit and its ``.options``; its own analysis cards are not run.

    The method is Dune's (ngspice-47's ``.pss`` is a free-running oscillator
    search that detunes from a driving source): Newton on the period map
    ``x(t0) -> x(t0 + T)`` with matrix-free GMRES, the Jacobian products by
    directional differences over a frozen-grid replay of the period
    transient. A linear circuit converges in one step. ``points`` is the
    accuracy knob: the error left is the integrator's phase error,
    ``(2 pi / points)**2 / 12`` of the amplitude with the default
    trapezoidal rule. Autonomous oscillators and transmission lines are not
    supported yet. The simulation releases the GIL.

    >>> import spice
    >>> res = spice.pss(\"\"\"rc low-pass
    ... v1 1 0 sin(0 1 1k)
    ... r1 1 2 1k
    ... c1 2 0 1u
    ... .end
    ... \"\"\", freq=1e3, tstab=2e-3)
    >>> res.period, res.t0
    (0.001, 0.002)
    >>> res.residuals[-1] <= 1
    True
    >>> h1 = res.spectrum.column("v(2)")[1]       # first harmonic, a phasor
    >>> round(float(abs(h1)), 3)                   # |H| = 1/sqrt(1 + (2 pi)^2)
    0.157
    >>> spice.measure(res.time, "pp v(2)").value > 0.31
    True

    Args:
        netlist: The deck, as for :func:`spice.simulate` (a ``str`` is deck
            text, an :class:`os.PathLike` a deck file).
        freq: The fundamental in Hz; ``0`` (the default) takes it from the
            periodic sources -- the greatest common divisor of their
            frequencies (``SIN``/``SFFM`` with an explicit frequency,
            ``PULSE`` ``1/PER``).
        tstab: Stabilization time: a plain transient runs to the first
            multiple of the period at or after ``tstab`` (at least one
            period) before the shooting starts there.
        osc_node: ngspice's oscillation node, for autonomous circuits (not
            solved yet): checked to exist and reported in the notes only.
        points: Samples per period, ``4 ... 10_000_000``: the time plot has
            ``points + 1`` rows, and the period transient steps at most
            ``period / points``.
        harmonics: Rows of the spectrum, the DC included (ngspice's count),
            ``1 ... points // 2``.
        max_iter: Most shooting iterations (at least 1).
        steady_coeff: Scale of the convergence tolerance, ``> 0``: ``1``
            (the default) is the SPICE tolerances (``reltol``, ``vntol``,
            ``abstol``) on the period mismatch.
        uic: Start the stabilization transient from the initial conditions,
            as ``.tran ... uic``.
        includes: As for :func:`spice.simulate`.
        options: As for :func:`spice.simulate`.
        tmax_divisor: As for :func:`spice.simulate` (it divides the maximum
            step of the stabilization transient).

    Returns:
        The two plots (:attr:`PssResult.time`, :attr:`PssResult.spectrum`),
        the notes (method summary, Newton and GMRES counts) and the period.

    Raises:
        PssConvergenceError: ``max_iter`` shooting iterations did not converge
            (a :class:`SpiceError` of kind ``"pss"`` with the residual
            history).
        SpiceError: the deck does not parse or build, or the analysis fails
            otherwise (kind ``"analysis"``: no periodic source with
            ``freq=0``, an unknown ``osc_node``, a transmission line, a
            transient that does not converge...).
        ValueError: an argument out of range; bad ``options`` or
            ``includes``, as for :func:`spice.simulate`.
        TypeError: an argument of the wrong type.
        OSError: the deck file cannot be read.
    """
    f = _number("freq", freq)
    if f < 0:
        raise ValueError(f"freq must be positive, or 0 to take it from the sources; got {f}")
    ts = _number("tstab", tstab)
    if ts < 0:
        raise ValueError(f"tstab must not be negative, got {ts}")
    n = _count("points", points, 4, 10_000_000)
    h = _count("harmonics", harmonics, 1, n // 2)
    it = _count("max_iter", max_iter, 1, 1_000_000_000)
    sc = _number("steady_coeff", steady_coeff)
    if sc <= 0:
        raise ValueError(f"steady_coeff must be positive, got {sc}")
    if not isinstance(uic, bool):
        raise TypeError(f"uic must be a bool, not {type(uic).__name__}")
    if osc_node is not None and not isinstance(osc_node, str):
        raise TypeError(f"osc_node must be a str or None, not {type(osc_node).__name__}")
    text, path, inc = _deck_source(netlist, includes)
    with _native_errors():
        native = _spice._pss(
            text,
            path,
            inc,
            None if options is None else dict(options),
            tmax_divisor,
            f,
            ts,
            osc_node,
            n,
            h,
            it,
            sc,
            uic,
        )
    return PssResult(native)
