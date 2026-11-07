"""``spice.Session``: alter a circuit and re-run it without re-parsing."""

from __future__ import annotations

import os
from collections.abc import Mapping, Sequence
from typing import NamedTuple, overload

import numpy as np
import numpy.typing as npt

from . import _spice
from ._result import Result
from ._simulate import OptionValue, _deck_source, _native_errors
from .pic import PicExtraction

__all__ = ["AlterValue", "Session", "WarmStats"]

AlterValue = float | Sequence[float] | npt.NDArray[np.float64]
"""A value for :meth:`Session.alter`: a number, or a source function's arguments."""


class WarmStats(NamedTuple):
    """Warm-start counters of a :class:`Session` (:attr:`Session.warm_stats`)."""

    hits: int
    """Operating points solved from the previous one."""
    misses: int
    """Warm attempts that did not converge (each analysis then re-ran cold)."""


class Session:
    """A live circuit: parsed and elaborated once, altered and re-run.

    The deck argument, ``includes``, ``options`` and ``tmax_divisor`` are
    those of :func:`simulate` (:attr:`tmax_divisor` can change between
    runs). :meth:`alter` changes one element parameter in place
    (the spice3 ``alter`` command, with ngspice's parameter names) and
    :meth:`run` runs every analysis card of the deck again, skipping the
    parse and the elaboration.

    With :attr:`warm_start` off (the default), :meth:`run` returns exactly
    what :func:`simulate` returns for the altered deck -- the same plots, bit
    for bit, the same notes and warnings. Turning it on continues each
    operating point from the previous run's: faster for nonlinear circuits,
    and equal to a fresh run to the Newton tolerances instead of bit for
    bit.

    >>> import spice
    >>> s = spice.Session(\"\"\"divider
    ... v1 1 0 dc 10
    ... r1 1 2 1k
    ... r2 2 0 1k
    ... .op
    ... .end
    ... \"\"\")
    >>> float(s.run()[0].column("v(2)")[0])
    5.0
    >>> float(s.alter("r2", 3e3).run()[0].column("v(2)")[0])
    7.5
    """

    __slots__ = ("_native",)

    def __init__(
        self,
        netlist: str | os.PathLike[str] | PicExtraction,
        *,
        includes: Mapping[str, str] | None = None,
        options: Mapping[str, OptionValue] | None = None,
        tmax_divisor: float | None = None,
    ) -> None:
        text, path, inc = _deck_source(netlist, includes)
        with _native_errors():
            self._native = _spice._Session(
                text,
                path,
                inc,
                None if options is None else dict(options),
                tmax_divisor,
            )

    @overload
    def alter(self, refdes: str, value: float, /) -> Session: ...
    @overload
    def alter(self, refdes: str, param: str, value: AlterValue, /) -> Session: ...
    def alter(self, refdes: str, *args: object) -> Session:
        """Change one parameter of element ``refdes``; returns the session.

        ``alter(refdes, value)`` sets the element's principal value: the
        resistance, capacitance or inductance, a source's DC level, a
        controlled source's gain, a diode/BJT/JFET/MESFET area.
        ``alter(refdes, param, value)`` sets a named one, with ngspice's
        names and the card's units: ``resistance``, ``capacitance``,
        ``inductance``, ``ic``, ``dc``, ``acmag``, ``acphase`` (degrees),
        ``gain``, ``w``/``l`` (meters, MOS of every level), ``temp`` (degrees
        Celsius; MOS 1/2/3/6, diode, BJT, JFET), ``area``; and a source's
        transient function ``pulse``, ``sin``, ``exp``, ``pwl`` or ``sffm``
        with the card's arguments as a sequence. :meth:`alterable` lists them
        per element.

        Names are case-insensitive; subcircuit instances go by their expanded
        name (``r.x1.r1``). The circuit's topology never changes.

        Raises:
            SpiceError: kind ``"alter"`` -- unknown element or parameter, a
                non-finite value, a zero resistance, a nonpositive W/L/area,
                a number where a sequence is due or vice versa. The circuit
                is left as it was.
            TypeError: a value that is neither a number nor a sequence.
        """
        param: object
        value: object
        if len(args) == 1:
            param, value = "", args[0]
        elif len(args) == 2:
            param, value = args
        else:
            raise TypeError("alter(refdes, value) or alter(refdes, param, value)")
        if not isinstance(param, str):
            raise TypeError(f"param must be a str, not {type(param).__name__}")
        with _native_errors():
            self._native.alter(refdes, param, value)
        return self

    def param(self, refdes: str, param: str = "") -> float | tuple[float, ...] | None:
        """The current value of an alterable parameter (names as :meth:`alter`).

        ``None`` for a value the element never got: a MOS ``W`` left to
        ``.options defw``, a source without ``AC``, a transient function the
        source does not have.
        """
        with _native_errors():
            v = self._native.param(refdes, param)
        return tuple(v) if isinstance(v, list) else v

    def alterable(self, refdes: str) -> tuple[str, ...]:
        """The parameter names :meth:`alter` accepts for ``refdes``."""
        with _native_errors():
            return tuple(self._native.alterable(refdes))

    @property
    def temp(self) -> float:
        """The circuit temperature in degrees Celsius (``.options temp``).

        Instance ``TEMP=`` values still win, as in SPICE3.
        """
        return float(self._native.temp())

    @temp.setter
    def temp(self, celsius: float) -> None:
        with _native_errors():
            self._native.set_temp(celsius)

    @property
    def tmax_divisor(self) -> float | None:
        """The divisor of every ``.tran`` card's maximum step (``None``: none).

        As the ``tmax_divisor`` argument of :func:`simulate`; assigning it
        (a finite number ``>= 1``, or ``None``) applies from the next
        :meth:`run` on, without re-elaborating -- a "fine steps" toggle.

        Raises:
            ValueError: below 1 or not finite (the value is left as it was).
            TypeError: not a number.
        """
        return self._native.tmax_divisor()

    @tmax_divisor.setter
    def tmax_divisor(self, d: float | None) -> None:
        self._native.set_tmax_divisor(d)

    @property
    def warm_start(self) -> bool:
        """Continue each operating point from the previous one (default off)."""
        return bool(self._native.warm_start())

    @warm_start.setter
    def warm_start(self, on: bool) -> None:
        self._native.set_warm_start(bool(on))

    @property
    def warm_stats(self) -> WarmStats:
        """Warm-start hits and misses so far."""
        return WarmStats(*self._native.warm_stats())

    def run(self) -> Result:
        """Run every analysis card of the deck on the current circuit.

        The result is :func:`simulate`'s for the altered deck. The
        simulation releases the GIL.

        Raises:
            SpiceError: kind ``"analysis"`` when an analysis fails.
        """
        with _native_errors():
            return Result(self._native.run())

    def __repr__(self) -> str:
        return f"<spice.Session warm_start={self.warm_start} temp={self.temp:g}>"
