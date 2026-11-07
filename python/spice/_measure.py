"""``.measure`` results and :func:`spice.measure`: scalar measurements of a plot."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass, field
from types import MappingProxyType
from typing import TYPE_CHECKING, Any, Literal

from . import _spice
from ._errors import SpiceError

if TYPE_CHECKING:
    from ._result import Plot

__all__ = ["MeasureAnalysis", "Measurement", "measure"]

MeasureAnalysis = Literal["tran", "ac", "dc", "sp"]
"""The analysis a measurement applies to (the ``.measure`` card's first word)."""


@dataclass(frozen=True, slots=True)
class Measurement:
    """One measurement: a ``.measure`` card of the deck, or a :func:`measure` call.

    The semantics are ngspice-47's ``.meas`` (the SPICE3f5 has none): linear
    interpolation between points, crossings counted by ``RISE``/``FALL``/
    ``CROSS``, ``AVG`` by trapezoids, ``RMS``/``INTEG`` by Simpson's rules
    with interpolated window edges. Measurements read the plot only; they
    never change a simulation.

    Attributes:
        name: The result name (lowercased, as on the card).
        analysis: ``"tran"``, ``"ac"``, ``"dc"`` or ``"sp"``.
        value: The result (what ngspice prints first): a time for ``WHEN``
            and ``TRIG/TARG`` (the delay), a value for ``FIND``, ``MIN``,
            ``AVG``... and the *scale value* for ``MIN_AT``/``MAX_AT``.
            ``nan`` when the measurement failed.
        details: The extra numbers ngspice prints beside it, by its labels:
            ``trig``/``targ`` (delays), ``at`` (``MIN``/``MAX``: where),
            ``with`` (``MIN_AT``/``MAX_AT``: the extremum), ``from``/``to``
            (``AVG``, ``RMS``, ``INTEG``, ``PP``: the window). Empty for
            ``WHEN``, ``FIND``, ``DERIV`` and failures.
        error: Why the measurement failed (``"out of interval (WHEN)"``: the
            event never happens in the window; an unknown vector...), or
            ``None``.
        plot: For a deck's measurements, the index of the measured plot in
            the :class:`~spice.Result`; ``None`` for :func:`measure`.
    """

    name: str
    analysis: MeasureAnalysis
    value: float
    details: Mapping[str, float] = field(default_factory=lambda: MappingProxyType({}))
    error: str | None = None
    plot: int | None = None

    @property
    def ok(self) -> bool:
        """True when the measurement produced a value."""
        return self.error is None

    def __float__(self) -> float:
        return self.value

    def __repr__(self) -> str:
        if self.error is not None:
            return f"<spice.Measurement {self.name}: failed, {self.error}>"
        extra = "".join(f" {k}={v:.6g}" for k, v in self.details.items())
        return f"<spice.Measurement {self.name} = {self.value:.6g}{extra}>"

    @classmethod
    def _from_native(cls, d: dict[str, Any]) -> Measurement:
        return cls(
            name=d["name"],
            analysis=d["analysis"],
            value=float(d["value"]),
            details=MappingProxyType(dict(d["details"])),
            error=d["error"],
            plot=d["plot"],
        )


def measure(plot: Plot, spec: str, *, name: str = "measure") -> Measurement:
    """Measure one plot, with a ``.measure`` card's body as the specification.

    ``spec`` is what follows ``.meas <analysis> <name>`` on a card (the
    ngspice-47 grammar); the analysis comes from the plot itself::

        tran = spice.simulate(deck).plot("Transient Analysis")
        spice.measure(tran, "trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1")
        spice.measure(tran, "when v(out)=0.5 cross=last")
        spice.measure(tran, "find v(out) at=2n")
        spice.measure(tran, "deriv v(out) when v(out)=0.5")
        spice.measure(tran, "max v(out) from=1n to=5n")   # avg rms integ pp min_at...

    Values take SPICE suffixes (``2n``, ``1meg``). ``DERIV`` is a Dune
    extension (ngspice-47 does not implement it). The engine does the
    measuring; nothing is computed in Python.

    Args:
        plot: A ``.tran``, ``.ac``, ``.dc`` or ``.sp`` plot.
        spec: The measurement, e.g. ``"when v(2)=0.5 rise=2"``.
        name: The :attr:`Measurement.name` to give the result.

    Returns:
        The measurement; check :attr:`Measurement.ok` -- an event that never
        happens is a result (``value`` nan, ``error`` set), not an exception.

    Raises:
        SpiceError: ``kind="measure"`` when ``spec`` is malformed (the message
            names the offending word's column) or the plot is of a kind that
            cannot be measured (operating point, noise, ...).
    """
    if not isinstance(spec, str):
        raise TypeError(f"spec must be a str, not {type(spec).__name__}")
    try:
        native = _spice._measure(
            plot.plotname,
            [(v.name, v.kind) for v in plot.variables],
            plot._scale,
            list(plot._columns),
            spec,
            name,
        )
    except _spice.NativeError as e:
        kind, message, _ = e.args
        raise SpiceError(message, kind=kind) from None
    return Measurement._from_native(native)
