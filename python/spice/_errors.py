"""Errors and diagnostics of a simulation run."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal

__all__ = ["Diagnostic", "ErrorKind", "PssConvergenceError", "Severity", "SpiceError"]

Severity = Literal["error", "warning"]
"""Severity of a :class:`Diagnostic`."""

ErrorKind = Literal[
    "parse", "elaborate", "analysis", "pss", "measure", "alter", "pic", "internal"
]
"""Stage at which a run failed (:attr:`SpiceError.kind`).

* ``"parse"`` -- the deck has errors (syntax, a missing ``.include``, ...);
  :attr:`SpiceError.diagnostics` lists all of them, warnings included.
* ``"elaborate"`` -- the deck parsed but does not build into a circuit
  (unknown model, bad device parameter, ...).
* ``"analysis"`` -- an analysis failed (no convergence, timestep too
  small, singular matrix, ...).
* ``"pss"`` -- a periodic steady state (:func:`spice.pss`, or a ``.pss``
  card) did not converge in its shooting iterations; the error is a
  :class:`PssConvergenceError`, carrying the residual history.
* ``"measure"`` -- a :func:`spice.measure` specification is malformed, or
  the plot cannot be measured.
* ``"alter"`` -- :meth:`Session.alter` refused a value (unknown element or
  parameter, a value out of range); the circuit is unchanged.
* ``"pic"`` -- a pic figure yields no netlist or drawing (rpic rejects it,
  or it is not marked ``spice = 1``), or an extraction with errors was
  run; the error is a :class:`spice.pic.PicError`, carrying the figure
  diagnostics and the ``line``/``column`` of the first error.
* ``"internal"`` -- a bug in the simulator; please report it with the deck.
"""


@dataclass(frozen=True, slots=True)
class Diagnostic:
    """A parser or ``.options`` message, with its position in the deck.

    ``file``, ``line`` and ``column`` are ``None`` for deck-level messages
    that have no position. ``line`` and ``column`` are 1-based; the column
    counts bytes. A deck passed as a string is named ``deck.cir``.
    """

    severity: Severity
    message: str
    file: str | None
    line: int | None
    column: int | None

    def __str__(self) -> str:
        if self.file is None:
            return f"{self.severity}: {self.message}"
        return f"{self.file}:{self.line}:{self.column}: {self.severity}: {self.message}"

    @classmethod
    def _from_native(cls, d: dict[str, Any]) -> Diagnostic:
        return cls(
            severity=d["severity"],
            message=d["message"],
            file=d["file"],
            line=d["line"],
            column=d["column"],
        )


class SpiceError(Exception):
    """A deck that does not parse, build or simulate.

    ``str(err)`` is the simulator's message; for a parse error it is the
    first error rendered as ``file:line:column: error: message``.

    Attributes:
        kind: The failing stage (see :data:`ErrorKind`).
        message: The message, as ``str(err)``.
        diagnostics: Every parser diagnostic (warnings included) for a
            ``"parse"`` error; empty otherwise.
    """

    kind: ErrorKind
    message: str
    diagnostics: tuple[Diagnostic, ...]

    def __init__(
        self,
        message: str,
        *,
        kind: ErrorKind,
        diagnostics: tuple[Diagnostic, ...] = (),
    ) -> None:
        super().__init__(message)
        self.kind = kind
        self.message = message
        self.diagnostics = diagnostics


class PssConvergenceError(SpiceError):
    """A periodic steady state whose shooting did not converge (kind ``"pss"``).

    Raised by :func:`spice.pss` and by any run of a deck with a ``.pss`` card
    (:func:`spice.simulate`, :meth:`spice.Session.run`) when ``max_iter``
    shooting iterations leave the period mismatch ``x(t0+T) - x(t0)`` above
    tolerance. A :class:`SpiceError` with ``kind == "pss"``, so code that
    catches :class:`SpiceError` keeps working.

    Attributes:
        iterations: The shooting iterations run.
        residuals: The mismatch of each iteration, in units of the
            convergence tolerance (converged would be ``<= steady_coeff``):
            what :attr:`spice.PssResult.residuals` holds on success. A
            residual that stalls well above 1 usually means a non-periodic
            circuit (an autonomous oscillator, a source that is not a
            harmonic of ``freq``) or too few ``points``.
    """

    iterations: int
    residuals: tuple[float, ...]

    def __init__(
        self,
        message: str,
        *,
        iterations: int,
        residuals: tuple[float, ...],
        diagnostics: tuple[Diagnostic, ...] = (),
    ) -> None:
        super().__init__(message, kind="pss", diagnostics=diagnostics)
        self.iterations = iterations
        self.residuals = residuals
