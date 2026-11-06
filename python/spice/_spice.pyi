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
) -> dict[str, Any]: ...
