"""``spice.simulate``: run a SPICE3 deck."""

from __future__ import annotations

import errno
import os
import secrets
from collections.abc import Iterator, Mapping
from contextlib import contextmanager
from pathlib import Path
from typing import Any, cast

from . import _spice
from ._errors import Diagnostic, ErrorKind, PssConvergenceError, SpiceError
from ._result import Result
from .pic import PicExtraction, _runnable

__all__ = ["OPTION_NAMES", "OptionValue", "simulate"]

OptionValue = float | int | bool | str
"""Value of a simulator option: a number, a count, a flag or a word (``method``, ``compat``)."""

OPTION_NAMES: tuple[str, ...] = tuple(_spice.OPTION_NAMES)
"""Every key :func:`simulate` accepts in ``options`` (the 3f5 ``.options`` names)."""

SEED_MAX = 2**32 - 1
"""The largest ``seed``: ngspice reads a seed as an unsigned 32-bit integer."""


def _random_seed() -> int:
    """A fresh seed in ``[1, SEED_MAX]`` for ``seed="random"`` (OS entropy, :mod:`secrets`).

    Drawn on every call that takes ``options``; the engine uses it only when
    the option, or the deck's ``.options seed=random``, asks for one.
    """
    return secrets.randbelow(SEED_MAX) + 1


def _deck_source(
    netlist: str | os.PathLike[str] | PicExtraction, includes: Mapping[str, str] | None
) -> tuple[str | None, str | None, dict[str, str] | None]:
    """``(text, path, includes)`` of a deck argument, with the checks of :func:`simulate`."""
    if isinstance(netlist, PicExtraction):
        if includes is not None:
            raise ValueError(
                "includes does not apply to a PicExtraction: its .include files "
                "come with the figure (pass them to spice.pic.extract)"
            )
        text, pic_inc = _runnable(netlist)
        return text, None, pic_inc
    inc = None if includes is None else dict(includes)
    if isinstance(netlist, str):
        # A str is deck text, never a file name -- but a one-line str
        # that names an existing file is a classic slip that would
        # otherwise simulate an empty deck titled with the path.
        if "\n" not in netlist and Path(netlist).is_file():
            raise ValueError(
                f"{netlist!r} is deck TEXT here, but it names an existing "
                "file; pass pathlib.Path(...) to simulate a deck file"
            )
        return netlist, None, inc
    if isinstance(netlist, os.PathLike):
        if includes is not None:
            raise ValueError(
                "includes applies to a netlist string; a deck file resolves .include on disk"
            )
        path = os.fspath(netlist)
        if not Path(path).exists():
            raise FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), path)
        return None, path, None
    raise TypeError(
        f"netlist must be a str (deck text), a path or a spice.pic.PicExtraction, "
        f"not {type(netlist).__name__}"
    )


def _from_native_error(e: _spice.NativeError) -> Exception:
    """The public exception for a ``NativeError(kind, message, diagnostics[, extra])``."""
    kind, message, diags = e.args[:3]
    if kind == "io":
        return OSError(message)
    diagnostics = tuple(Diagnostic._from_native(d) for d in diags)
    if kind == "pss":
        return _pss_error(message, e.args[3], diagnostics)
    return SpiceError(message, kind=cast(ErrorKind, kind), diagnostics=diagnostics)


def _pss_error(
    message: str, history: dict[str, Any], diagnostics: tuple[Diagnostic, ...] = ()
) -> PssConvergenceError:
    """The :class:`PssConvergenceError` of a native shooting history
    ``{"iterations", "residuals", "period"}``."""
    period = history["period"]
    return PssConvergenceError(
        message,
        iterations=int(history["iterations"]),
        residuals=tuple(float(r) for r in history["residuals"]),
        period=None if period is None else float(period),
        diagnostics=diagnostics,
    )


@contextmanager
def _native_errors() -> Iterator[None]:
    """Turn the native module's ``NativeError`` into :class:`SpiceError`."""
    try:
        yield
    except _spice.NativeError as e:
        raise _from_native_error(e) from None


def simulate(
    netlist: str | os.PathLike[str] | PicExtraction,
    *,
    includes: Mapping[str, str] | None = None,
    options: Mapping[str, OptionValue] | None = None,
    tmax_divisor: float | None = None,
) -> Result:
    """Parse a SPICE3 deck, build it and run every analysis card in it.

    This is what ``spice3 -b`` (or ``dune -b``) runs: each ``.op``, ``.dc``,
    ``.tran``, ``.ac``, ``.pz``, ``.noise``, ``.disto``, ``.tf``, ``.sens``
    and ``.pss`` card in deck order, each giving one or more :class:`Plot`
    (for the periodic steady state with its period and shooting history,
    see :func:`spice.pss`).
    The simulation releases the GIL.

    Args:
        netlist: The deck. A :class:`str` is the deck text itself (never a
            file name); its ``.include`` cards are served from ``includes``
            only, so it never touches the file system, and diagnostics name
            it ``deck.cir``. A :class:`os.PathLike` (e.g.
            :class:`pathlib.Path`) is a deck file; its ``.include`` cards
            resolve on disk, relative to the including file. A
            :class:`spice.pic.PicExtraction` (:func:`spice.pic.extract`) runs
            as its netlist, with the ``.include`` files that came with the
            figure; one whose diagnostics include an error raises
            :class:`spice.pic.PicError`.
        includes: For a string deck, the files it may ``.include``, keyed by
            the path the card names (resolved relative to the including
            file, the deck being ``deck.cir`` at the root). Not allowed with
            a deck file or a ``PicExtraction``.
        options: Simulator options applied over the deck's own ``.options``
            -- the 3f5 names in :data:`OPTION_NAMES`, matched
            case-insensitively: tolerances and steps as numbers (``reltol``,
            ``abstol``, ``vntol``, ``gmin``, ``pivtol``, ``pivrel``,
            ``trtol``, ``chgtol``, ``defl``, ``defw``, ``defad``,
            ``defas``), counts as ints (``itl1``, ``itl2``, ``itl4``,
            ``gminsteps``, ``srcsteps``, ``maxord``, ``ltrabkrate``),
            ``temp``/``tnom`` in degrees Celsius as on the card, ``method``
            as ``"trap"``, ``"gear"`` or ``"trbdf2"`` (unset, a ``.tran``
            integrates with ``"trbdf2"``), ``compat`` as ``"dune"`` or
            ``"spice3"`` (the SPICE3f5 defaults: a trapezoidal ``.tran``,
            switches flipping on the time grid, ``ltrabkrate`` 0),
            flags as bools
            (``noopiter``, ``nopage``, ``ltrafit``, ``keepopinfo``, ``trytocompact``,
            ``notrnoise``, ``devnoise``), ``seed`` (see below) and ``devnt``
            (the device-noise grid step, s) as a positive number. ``False``
            clears a flag the deck's card set.

            ``seed`` is exactly ``.options seed=`` written in the deck: an
            int from 1 to ``2**32 - 1`` draws the deck's random functions
            (``agauss``, ``gauss``, ``aunif``, ``unif``, ``limit``) from
            that seed -- the same seed, the same circuit, on every platform
            -- and seeds the transient noise (``TRNOISE``, ``devnoise``).
            ``"random"`` picks a fresh seed with :mod:`secrets` (as does a
            deck that says ``.options seed=random``); the seed used comes
            back in :attr:`Result.seed`, and passing it as ``seed`` repeats
            the run. Without a seed the random functions take their nominal
            value (:attr:`Result.random_mode` ``"nominal"``).
        tmax_divisor: Divide the maximum step of every ``.tran`` card -- its
            TMAX argument, else the default ``min(TSTEP, (TSTOP-TSTART)/50)``
            -- by this finite number ``>= 1``: the run is, bit for bit, that
            of the deck with the card's TMAX edited to ``TMAX/tmax_divisor``
            (with ``options={"trtol": 1}``, the "fine steps" view of a
            waveform). It is an argument rather than an ``options`` key
            because TMAX is a parameter of the analysis card, not an engine
            option. ``None`` (the default) leaves the cards as they are.

    Returns:
        The plots of every analysis, the run's notes and its warnings.

    Raises:
        SpiceError: the deck does not parse, does not build or an analysis
            fails; :attr:`SpiceError.kind` tells which (a ``.pss`` that does
            not converge raises its subclass :class:`PssConvergenceError`; a
            ``PicExtraction`` with extraction errors, :class:`spice.pic.PicError`).
        ValueError: an unknown option name (the message lists the valid
            ones) or an invalid option value; a ``tmax_divisor`` below 1 or
            not finite; ``includes`` with a deck file.
        TypeError: an option value (or ``tmax_divisor``) of the wrong type.
        OSError: the deck file cannot be read (``FileNotFoundError`` when it
            does not exist).
    """
    text, path, inc = _deck_source(netlist, includes)
    with _native_errors():
        native = _spice._simulate(
            text,
            path,
            inc,
            None if options is None else dict(options),
            tmax_divisor,
            _random_seed(),
        )
    return Result(native)
