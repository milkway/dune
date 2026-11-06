"""Shared fixtures: the repo's corpus, read in place."""

from __future__ import annotations

from pathlib import Path

import pytest


def _repo_root() -> Path:
    here = Path(__file__).resolve()
    for parent in here.parents:
        if (parent / "corpus").is_dir() and (parent / "crates").is_dir():
            return parent
    raise RuntimeError("cannot find the dune repo root (corpus/ + crates/) above tests/")


@pytest.fixture(scope="session")
def corpus() -> Path:
    """The repo's ``corpus/`` directory."""
    return _repo_root() / "corpus"


@pytest.fixture(scope="session")
def examples(corpus: Path) -> Path:
    """The Berkeley 3f5 example decks."""
    return corpus / "spice3f5" / "examples"
