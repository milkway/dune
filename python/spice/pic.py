"""``spice.pic``: SPICE netlists from rpic figures, and their SVG.

A figure is ordinary pic drawn with the dune ``spice.pic`` prelude
(``resistor()``, ``vsource()``, ``npn()``, ...), compiled by rpic; it opts
in with ``spice = 1``. :func:`extract` reads its netlist -- one card per
labeled element (the label is the refdes), nodes from the wires, ground
blocks and named places, the figure's column-1 ``.`` lines as the deck's
control cards -- and :func:`render` draws it. The extraction simulates
directly:

>>> import spice
>>> fig = '''.PS
... spice = 1
... In: (0,1); Out: (2,1)
... V1: vsource(In, (0,0), DC 5)
... R1: resistor(In, Out, 1k)
... R2: resistor(Out, (2,0), 1k)
... line from R2.N to V1.N; ground(V1.N)
... .op
... .PE
... '''
>>> ex = spice.pic.extract(fig)
>>> print(ex.netlist, end="")
* netlist extracted by dune-pic
V1 In 0 DC 5
R1 In Out 1k
R2 Out 0 1k
.op
.end
>>> [(p.refdes, p.name, p.net) for p in ex.map.pins][:2]
[('V1', 'P', 'In'), ('V1', 'N', '0')]
>>> float(spice.simulate(ex)[0].column("v(out)")[0])
2.5
>>> spice.pic.render(ex).startswith("<svg")
True

Extraction problems in a figure that compiles (a dangling pin, an element
without a value, ...) are not exceptions: the netlist is still emitted and
the problems are in :attr:`PicExtraction.diagnostics`. Simulating an
extraction with an error diagnostic raises :class:`PicError`, as
``dune pic --run`` refuses it. A figure that yields no netlist at all
(rpic rejects it, or it is not marked ``spice = 1``) raises
:class:`PicError` from :func:`extract` itself.
"""

from __future__ import annotations

import errno
import os
from collections.abc import Mapping
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal

from . import _spice
from ._errors import SpiceError

__all__ = [
    "TEXLABELS",
    "Canvas",
    "Element",
    "Net",
    "OverlayMap",
    "ParamSpan",
    "PicDiagKind",
    "PicDiagnostic",
    "PicError",
    "PicExtraction",
    "Pin",
    "SourceRange",
    "ValueSpan",
    "extract",
    "render",
]

TEXLABELS: bool = bool(_spice.TEXLABELS)
"""Whether this build typesets ``$...$`` figure labels (the ``texlabels`` feature)."""

PicDiagKind = Literal[
    "dangling-pin",
    "floating-net",
    "missing-value",
    "duplicate-refdes",
    "unknown-element",
    "missing-pin",
    "unlabeled-element",
    "node-name-conflict",
    "prelude",
    "compile",
    "not-marked",
]
"""What a :class:`PicDiagnostic` is about.

The first nine are extraction diagnostics (:attr:`PicExtraction.diagnostics`);
``"compile"`` (rpic rejected the figure) and ``"not-marked"`` (no
``spice = 1``) only appear in a :class:`PicError` raised by :func:`extract`
or :func:`render`.
"""


@dataclass(frozen=True, slots=True)
class PicDiagnostic:
    """A figure diagnostic, with its span in the figure source.

    ``file`` is ``None`` for the figure itself, ``"spice.pic"`` for the
    prelude and ``"circuits"`` for rpic's circuit library; ``line``,
    ``column`` (1-based) and ``end_column`` (exclusive) are ``None`` when the
    diagnostic has no position.
    """

    severity: Literal["error", "warning"]
    kind: PicDiagKind
    message: str
    file: str | None
    line: int | None
    column: int | None
    end_column: int | None

    def __str__(self) -> str:
        at = ""
        if self.line is not None:
            at = f"{self.line}:{self.column}: "
            if self.file is not None:
                at = f"{self.file}:{at}"
        return f"{at}{self.severity}: {self.message}"

    @classmethod
    def _from_native(cls, d: dict[str, Any]) -> PicDiagnostic:
        return cls(
            severity=d["severity"],
            kind=d["kind"],
            message=d["message"],
            file=d["file"],
            line=d["line"],
            column=d["column"],
            end_column=d["end_column"],
        )


class PicError(SpiceError):
    """A figure that yields no netlist or no drawing, or a run of one with errors.

    A :class:`spice.SpiceError` with ``kind == "pic"``. Raised by
    :func:`extract` and :func:`render` when rpic rejects the figure (its
    diagnostic has kind ``"compile"``) or, for :func:`extract`, when it is not
    marked ``spice = 1`` (``"not-marked"``); and by :func:`spice.simulate`
    (and :class:`spice.Session`, :func:`spice.sweep`, :func:`spice.pss`) for
    an extraction whose diagnostics include an error.

    Attributes:
        pic_diagnostics: The figure diagnostics: the one compile or
            not-marked diagnostic, or every extraction diagnostic.
        line: The figure line of the first error, or ``None``.
        column: Its column, or ``None``.
        extraction: The refused extraction, for a run; ``None`` from
            :func:`extract`/:func:`render`.
    """

    pic_diagnostics: tuple[PicDiagnostic, ...]
    line: int | None
    column: int | None
    extraction: PicExtraction | None

    def __init__(
        self,
        message: str,
        *,
        pic_diagnostics: tuple[PicDiagnostic, ...] = (),
        extraction: PicExtraction | None = None,
    ) -> None:
        super().__init__(message, kind="pic")
        self.pic_diagnostics = pic_diagnostics
        self.extraction = extraction
        first = next((d for d in pic_diagnostics if d.severity == "error"), None)
        self.line = None if first is None else first.line
        self.column = None if first is None else first.column


@dataclass(frozen=True, slots=True)
class SourceRange:
    """A range of the figure source: 1-based, end exclusive, columns in characters."""

    line: int
    col: int
    end_line: int
    end_col: int

    @classmethod
    def _from_native(cls, d: dict[str, Any]) -> SourceRange:
        return cls(line=d["line"], col=d["col"], end_line=d["end_line"], end_col=d["end_col"])


@dataclass(frozen=True, slots=True)
class ValueSpan:
    """Where an element's value argument is written in the figure.

    ``text`` is the source in ``span``, verbatim (``4.7k``, ``SIN(0 1 1k)``;
    the inside of a quoted argument) -- what an editor replaces to write a
    new value back.
    """

    span: SourceRange
    text: str


@dataclass(frozen=True, slots=True)
class ParamSpan:
    """An alterable parameter spelled inside a value (``dc``, ``acmag``, ``pulse``, ...).

    ``name`` is the parameter as :meth:`spice.Session.alter` names it.
    """

    name: str
    span: SourceRange
    text: str


@dataclass(frozen=True, slots=True)
class Pin:
    """An element terminal: its prelude name, its node and its point.

    ``x``, ``y`` are pic inches in the drawing's frame;
    :meth:`Canvas.to_svg` maps them to the SVG of :func:`render`.
    """

    refdes: str
    name: str
    net: str
    x: float
    y: float


@dataclass(frozen=True, slots=True)
class Element:
    """One card's shapes in the drawing.

    Attributes:
        refdes: The card name (the block label).
        shapes: The element's shape numbers (shape ``N`` is the SVG group
            ``<g id="sN">``).
        pins: Its terminals, in card order (none for a ``K``).
        value: Where its value argument is written, or ``None`` when there
            is no range an edit can trust (no value argument, a user wrapper
            macro, a ``K``).
        params: The alterable parameters spelled inside :attr:`value`.
        principal: The parameter ``Session.alter(refdes, value)`` changes
            (``resistance``, ``dc``, ``gain``, ...), ``None`` for ``M``,
            ``S``, ``W``, ``X``, ``T``, ``O``, ``K``.
    """

    refdes: str
    shapes: range
    pins: tuple[Pin, ...]
    value: ValueSpan | None
    params: tuple[ParamSpan, ...]
    principal: str | None

    @property
    def ids(self) -> str:
        """The shapes as rpic object ids, ``"sN..sM"`` (inclusive)."""
        return f"s{self.shapes.start}..s{max(self.shapes.stop - 1, 0)}"


@dataclass(frozen=True, slots=True)
class Net:
    """A named net and its drawn shapes (wires, dots, ground blocks)."""

    name: str
    shapes: tuple[int, ...]


@dataclass(frozen=True, slots=True)
class Canvas:
    """The SVG page frame of :func:`render`.

    ``w`` and ``h`` are the page size in inches (the ``viewBox`` is
    ``0 0 w*96 h*96``), ``origin`` the pic origin in inches from the page's
    lower-left corner, ``rescale`` the whole-picture scale factor
    (informational: the pins already carry it).
    """

    w: float
    h: float
    origin: tuple[float, float]
    rescale: float

    def to_svg(self, x: float, y: float) -> tuple[float, float]:
        """SVG user units of the drawing point ``(x, y)`` (pic inches)."""
        return ((x + self.origin[0]) * 96.0, (self.h - (y + self.origin[1])) * 96.0)


@dataclass(frozen=True, slots=True)
class OverlayMap:
    """Map from netlist names back to the drawing of :func:`render`."""

    elements: tuple[Element, ...]
    """Emitted elements, in card order."""
    nets: tuple[Net, ...]
    """Named nets, in order of first appearance (``0`` first if present)."""
    canvas: Canvas
    """The frame that maps pins to the SVG."""

    @property
    def pins(self) -> tuple[Pin, ...]:
        """Every element's pins, in card order."""
        return tuple(p for e in self.elements for p in e.pins)

    def element(self, refdes: str) -> Element:
        """The element named ``refdes`` (case-insensitive, as SPICE names are)."""
        key = refdes.lower()
        for e in self.elements:
            if e.refdes.lower() == key:
                return e
        raise KeyError(refdes)

    @classmethod
    def _from_native(cls, m: dict[str, Any]) -> OverlayMap:
        elements = []
        for e in m["elements"]:
            v = e["value"]
            start, stop = e["shapes"]
            elements.append(
                Element(
                    refdes=e["refdes"],
                    shapes=range(start, stop),
                    pins=tuple(
                        Pin(refdes=e["refdes"], name=p["name"], net=p["net"], x=p["x"], y=p["y"])
                        for p in e["pins"]
                    ),
                    value=None
                    if v is None
                    else ValueSpan(span=SourceRange._from_native(v["span"]), text=v["text"]),
                    params=tuple(
                        ParamSpan(
                            name=p["name"], span=SourceRange._from_native(p["span"]), text=p["text"]
                        )
                        for p in e["params"]
                    ),
                    principal=e["principal"],
                )
            )
        c = m["canvas"]
        return cls(
            elements=tuple(elements),
            nets=tuple(Net(name=n["name"], shapes=tuple(n["shapes"])) for n in m["nets"]),
            canvas=Canvas(
                w=c["w"], h=c["h"], origin=(c["origin"][0], c["origin"][1]), rescale=c["rescale"]
            ),
        )


@dataclass(frozen=True, slots=True, repr=False)
class PicExtraction:
    """The extraction of a figure (:func:`extract`): a deck :func:`spice.simulate` runs.

    Attributes:
        netlist: The SPICE3 netlist, ending in ``.end``.
        diagnostics: The extraction problems, errors and warnings.
        map: Netlist names back to the drawing.
        includes: The files the netlist's ``.include`` cards read, keyed by
            the path the card names: read from disk next to a figure file at
            extraction, or the ``includes`` given with figure text.
        source: The figure text (what :func:`render` draws).
        path: The figure file, or ``None`` for figure text.
    """

    netlist: str
    diagnostics: tuple[PicDiagnostic, ...]
    map: OverlayMap
    includes: Mapping[str, str]
    source: str
    path: str | None

    def __repr__(self) -> str:
        n_err = sum(d.severity == "error" for d in self.diagnostics)
        n_warn = len(self.diagnostics) - n_err
        what = "figure text" if self.path is None else repr(self.path)
        return (
            f"<PicExtraction of {what}: {len(self.map.elements)} elements, "
            f"{len(self.map.nets)} named nets, {n_err} errors, {n_warn} warnings>"
        )


def _figure_source(
    src: str | os.PathLike[str], includes: Mapping[str, str] | None
) -> tuple[str | None, str | None]:
    """``(text, path)`` of a figure argument, as :func:`spice.simulate` reads decks."""
    if isinstance(src, str):
        if "\n" not in src and Path(src).is_file():
            raise ValueError(
                f"{src!r} is figure TEXT here, but it names an existing "
                "file; pass pathlib.Path(...) to read a figure file"
            )
        return src, None
    if isinstance(src, os.PathLike):
        if includes is not None:
            raise ValueError(
                "includes applies to figure text; a figure file reads its "
                ".include files from disk, next to itself"
            )
        path = os.fspath(src)
        if not Path(path).exists():
            raise FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), path)
        return None, path
    raise TypeError(f"src must be a str (figure text) or a path, not {type(src).__name__}")


def _raise_native(e: _spice.NativeError) -> Exception:
    kind, message, diags = e.args[:3]
    if kind == "io":
        return OSError(message)
    if kind == "options":
        return ValueError(message)
    if kind == "pic":
        return PicError(
            message, pic_diagnostics=tuple(PicDiagnostic._from_native(d) for d in diags)
        )
    return SpiceError(message, kind="internal")


def extract(
    src: str | os.PathLike[str],
    *,
    snap: float | None = None,
    includes: Mapping[str, str] | None = None,
) -> PicExtraction:
    """Extract the SPICE3 netlist of a pic figure.

    Args:
        src: The figure. A :class:`str` is the figure text itself (never a
            file name); a :class:`os.PathLike` (e.g. :class:`pathlib.Path`) is
            a ``.pic`` file -- as :func:`spice.simulate` reads decks.
        snap: Distance, in pic inches, under which two terminals are the
            same point; ``None`` for the default (0.001). Positive, finite.
        includes: For figure text, the files its netlist's ``.include``
            cards read, keyed by the path the card names (as for
            :func:`spice.simulate`). A figure file reads them from disk,
            relative to the figure's directory (as ``dune pic --run`` does),
            at extraction time; ``includes`` is not allowed with one.

    Returns:
        The netlist, its diagnostics, the overlay map and the includes.
        Diagnostics, errors included, never raise here.

    Raises:
        PicError: rpic rejects the figure, or it is not marked ``spice = 1``.
        ValueError: a bad ``snap``; ``includes`` with a figure file.
        OSError: the figure file cannot be read.
    """
    text, path = _figure_source(src, includes)
    try:
        native = _spice._pic_extract(
            text, path, None if includes is None else dict(includes), snap
        )
    except _spice.NativeError as e:
        raise _raise_native(e) from None
    return PicExtraction(
        netlist=native["netlist"],
        diagnostics=tuple(PicDiagnostic._from_native(d) for d in native["diagnostics"]),
        map=OverlayMap._from_native(native["map"]),
        includes=dict(native["includes"]),
        source=native["source"],
        path=path,
    )


def render(src: str | os.PathLike[str] | PicExtraction) -> str:
    """Draw a figure as SVG: the drawing :attr:`PicExtraction.map` refers to.

    The figure need not be marked ``spice = 1``, and one whose extraction has
    errors still draws. Labels written entirely in ``$...$`` (``"$T_1$"``)
    are typeset as math when the figure sets ``texlabels = 1`` and
    :data:`TEXLABELS` is true (the default build); otherwise they stay
    literal. Typesetting never changes the netlist.

    Args:
        src: A :class:`PicExtraction` (its :attr:`~PicExtraction.source` is
            drawn), or the figure as for :func:`extract`.

    Returns:
        The SVG document.

    Raises:
        PicError: rpic rejects the figure.
        OSError: the figure file cannot be read.
    """
    text: str | None
    path: str | None
    if isinstance(src, PicExtraction):
        text, path = src.source, None
    else:
        text, path = _figure_source(src, None)
    try:
        return _spice._pic_render(text, path)
    except _spice.NativeError as e:
        raise _raise_native(e) from None


def _runnable(ex: PicExtraction) -> tuple[str, dict[str, str]]:
    """The deck text and includes of an extraction, or :class:`PicError` if it has errors."""
    first = next((d for d in ex.diagnostics if d.severity == "error"), None)
    if first is not None:
        at = "" if first.line is None else f"{first.line}:{first.column}: "
        raise PicError(
            f"the figure has extraction errors; its netlist is not runnable: "
            f"{at}{first.message}",
            pic_diagnostics=ex.diagnostics,
            extraction=ex,
        )
    return ex.netlist, dict(ex.includes)
