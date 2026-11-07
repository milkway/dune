# Stub of the private native module (python/src/lib.rs).
from pathlib import Path
from typing import Any

__version__: str
OPTION_NAMES: list[str]

class NativeError(Exception): ...

def _simulate(
    text: str | None,
    path: str | Path | None,
    includes: dict[str, str] | None,
    options: dict[str, Any] | None,
    tmax_divisor: float | None = None,
) -> dict[str, Any]: ...

def _sweep(
    text: str | None,
    path: str | Path | None,
    includes: dict[str, str] | None,
    options: dict[str, Any] | None,
    params: list[str],
    rows: list[list[float]],
    threads: int,
    tmax_divisor: float | None = None,
) -> dict[str, Any]: ...

def _measure(
    plotname: str,
    variables: list[tuple[str, str]],
    scale: int | None,
    columns: list[Any],
    spec: str,
    name: str,
) -> dict[str, Any]: ...

def _pss(
    text: str | None,
    path: str | Path | None,
    includes: dict[str, str] | None,
    options: dict[str, Any] | None,
    tmax_divisor: float | None,
    freq: float,
    tstab: float,
    osc_node: str | None,
    points: int,
    harmonics: int,
    max_iter: int,
    steady_coeff: float,
    uic: bool,
) -> dict[str, Any]: ...

class _Session:
    def __init__(
        self,
        text: str | None,
        path: str | Path | None,
        includes: dict[str, str] | None,
        options: dict[str, Any] | None,
        tmax_divisor: float | None = None,
    ) -> None: ...
    def alter(self, refdes: str, param: str, value: object) -> None: ...
    def param(self, refdes: str, param: str) -> float | list[float] | None: ...
    def alterable(self, refdes: str) -> list[str]: ...
    def set_temp(self, celsius: float) -> None: ...
    def temp(self) -> float: ...
    def set_warm_start(self, on: bool) -> None: ...
    def warm_start(self) -> bool: ...
    def set_tmax_divisor(self, d: float | None = None) -> None: ...
    def tmax_divisor(self) -> float | None: ...
    def warm_stats(self) -> tuple[int, int]: ...
    def run(self) -> dict[str, Any]: ...

TEXLABELS: bool

def _pic_extract(
    text: str | None,
    path: str | Path | None,
    includes: dict[str, str] | None,
    snap: float | None = None,
) -> dict[str, Any]: ...

def _pic_render(text: str | None, path: str | Path | None) -> str: ...
