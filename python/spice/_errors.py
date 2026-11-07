"""Errors and diagnostics of a simulation run."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Literal

__all__ = ["Diagnostic", "ErrorKind", "Severity", "SpiceError"]

Severity = Literal["error", "warning"]
"""Severity of a :class:`Diagnostic`."""

ErrorKind = Literal["parse", "elaborate", "analysis", "measure", "alter", "internal"]
"""Stage at which a run failed (:attr:`SpiceError.kind`).

* ``"parse"`` -- the deck has errors (syntax, a missing ``.include``, ...);
  :attr:`SpiceError.diagnostics` lists all of them, warnings included.
* ``"elaborate"`` -- the deck parsed but does not build into a circuit
  (unknown model, bad device parameter, ...).
* ``"analysis"`` -- an analysis failed (no convergence, timestep too
  small, singular matrix, ...).
* ``"measure"`` -- a :func:`spice.measure` specification is malformed, or
  the plot cannot be measured.
* ``"alter"`` -- :meth:`Session.alter` refused a value (unknown element or
  parameter, a value out of range); the circuit is unchanged.
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
