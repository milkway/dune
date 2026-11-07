"""``spice.simulate``: run a SPICE3 deck."""

from __future__ import annotations

import errno
import os
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from pathlib import Path
from typing import cast

from . import _spice
from ._errors import Diagnostic, ErrorKind, SpiceError
from ._result import Result

__all__ = ["OPTION_NAMES", "OptionValue", "simulate"]

OptionValue = float | int | bool | str
"""Value of a simulator option: a number, a count, a flag or ``"trap"``/``"gear"``."""

OPTION_NAMES: tuple[str, ...] = tuple(_spice.OPTION_NAMES)
"""Every key :func:`simulate` accepts in ``options`` (the 3f5 ``.options`` names)."""


def _deck_source(
    netlist: str | os.PathLike[str], includes: Mapping[str, str] | None
) -> tuple[str | None, str | None]:
    """``(text, path)`` of a deck argument, with the checks of :func:`simulate`."""
    if isinstance(netlist, str):
        # A str is deck text, never a file name -- but a one-line str
        # that names an existing file is a classic slip that would
        # otherwise simulate an empty deck titled with the path.
        if "\n" not in netlist and Path(netlist).is_file():
            raise ValueError(
                f"{netlist!r} is deck TEXT here, but it names an existing "
                "file; pass pathlib.Path(...) to simulate a deck file"
            )
        return netlist, None
    if isinstance(netlist, os.PathLike):
        if includes is not None:
            raise ValueError(
                "includes applies to a netlist string; a deck file resolves .include on disk"
            )
        path = os.fspath(netlist)
        if not Path(path).exists():
            raise FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), path)
        return None, path
    raise TypeError(
        f"netlist must be a str (deck text) or a path, not {type(netlist).__name__}"
    )


@contextmanager
def _native_errors() -> Iterator[None]:
    """Turn the native module's ``NativeError`` into :class:`SpiceError`."""
    try:
        yield
    except _spice.NativeError as e:
        kind, message, diags = e.args
        if kind == "io":
            raise OSError(message) from None
        raise SpiceError(
            message,
            kind=cast(ErrorKind, kind),
            diagnostics=tuple(Diagnostic._from_native(d) for d in diags),
        ) from None


def simulate(
    netlist: str | os.PathLike[str],
    *,
    includes: Mapping[str, str] | None = None,
    options: Mapping[str, OptionValue] | None = None,
) -> Result:
    """Parse a SPICE3 deck, build it and run every analysis card in it.

    This is what ``spice3 -b`` (or ``dune -b``) runs: each ``.op``, ``.dc``,
    ``.tran``, ``.ac``, ``.pz``, ``.noise``, ``.disto``, ``.tf`` and
    ``.sens`` card in deck order, each giving one or more :class:`Plot`.
    The simulation releases the GIL.

    Args:
        netlist: The deck. A :class:`str` is the deck text itself (never a
            file name); its ``.include`` cards are served from ``includes``
            only, so it never touches the file system, and diagnostics name
            it ``deck.cir``. A :class:`os.PathLike` (e.g.
            :class:`pathlib.Path`) is a deck file; its ``.include`` cards
            resolve on disk, relative to the including file.
        includes: For a string deck, the files it may ``.include``, keyed by
            the path the card names (resolved relative to the including
            file, the deck being ``deck.cir`` at the root). Not allowed with
            a deck file.
        options: Simulator options applied over the deck's own ``.options``
            -- the 3f5 names in :data:`OPTION_NAMES`, matched
            case-insensitively: tolerances and steps as numbers (``reltol``,
            ``abstol``, ``vntol``, ``gmin``, ``pivtol``, ``pivrel``,
            ``trtol``, ``chgtol``, ``defl``, ``defw``, ``defad``,
            ``defas``), counts as ints (``itl1``, ``itl2``, ``itl4``,
            ``gminsteps``, ``srcsteps``, ``maxord``), ``temp``/``tnom`` in
            degrees Celsius as on the card, ``method`` as ``"trap"`` or
            ``"gear"``, ``noopiter``/``nopage`` as bools.

    Returns:
        The plots of every analysis, the run's notes and its warnings.

    Raises:
        SpiceError: the deck does not parse, does not build or an analysis
            fails; :attr:`SpiceError.kind` tells which.
        ValueError: an unknown option name (the message lists the valid
            ones) or an invalid option value; ``includes`` with a deck file.
        TypeError: an option value of the wrong type.
        OSError: the deck file cannot be read (``FileNotFoundError`` when it
            does not exist).
    """
    text, path = _deck_source(netlist, includes)
    with _native_errors():
        native = _spice._simulate(
            text,
            path,
            None if includes is None else dict(includes),
            None if options is None else dict(options),
        )
    return Result(native)
