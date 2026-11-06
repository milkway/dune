"""Failures: parse/elaborate/analysis stages, options, includes, files."""

from __future__ import annotations

from pathlib import Path

import pytest

import spice


def test_parse_error_has_position() -> None:
    with pytest.raises(spice.SpiceError) as info:
        spice.simulate("title\nr1 1 0 1k\nq1 1 2 3 nomodel\n.op\n.end\n")
    err = info.value
    assert err.kind == "parse"
    assert str(err) == "deck.cir:3:16: error: missing model name"
    (d,) = err.diagnostics
    assert (d.severity, d.file, d.line, d.column) == ("error", "deck.cir", 3, 16)
    assert str(d) == str(err)


def test_missing_include_is_parse_error() -> None:
    with pytest.raises(spice.SpiceError) as info:
        spice.simulate("t\n.include models.lib\nr1 1 0 1k\n.op\n.end\n")
    err = info.value
    assert err.kind == "parse"
    assert "models.lib" in err.message and "not among the supplied includes" in err.message
    assert err.diagnostics[0].line == 2


def test_string_deck_never_reads_disk(examples: Path) -> None:
    # The include exists on disk, but a string deck only sees the map.
    deck = f"t\n.include {examples / 'rc.cir'}\n.end\n"
    with pytest.raises(spice.SpiceError, match="not among the supplied includes"):
        spice.simulate(deck)


def test_elaborate_error() -> None:
    with pytest.raises(spice.SpiceError) as info:
        spice.simulate("t\nr1 1 0\n.op\n.end\n")
    assert info.value.kind == "elaborate"
    assert "r1" in str(info.value)
    assert info.value.diagnostics == ()


def test_analysis_error() -> None:
    with pytest.raises(spice.SpiceError) as info:
        spice.simulate("t\nr1 1 0 1k\nv1 1 0 1\n.tran 1 0\n.end\n")
    assert info.value.kind == "analysis"


def test_unknown_option_lists_valid_names() -> None:
    with pytest.raises(ValueError, match="unknown option 'reltool'") as info:
        spice.simulate("t\nr1 1 0 1k\n.op\n.end\n", options={"reltool": 1e-4})
    for name in spice.OPTION_NAMES:
        assert name in str(info.value)


@pytest.mark.parametrize(
    ("options", "exc"),
    [
        ({"reltol": "tight"}, TypeError),
        ({"reltol": True}, TypeError),
        ({"reltol": float("nan")}, ValueError),
        ({"itl1": 2.5}, TypeError),
        ({"itl1": -1}, ValueError),
        ({"method": "euler"}, ValueError),
        ({"noopiter": 1}, TypeError),
    ],
)
def test_bad_option_values(options: dict[str, spice.OptionValue], exc: type[Exception]) -> None:
    with pytest.raises(exc):
        spice.simulate("t\nr1 1 0 1k\n.op\n.end\n", options=options)


def test_missing_file(tmp_path: Path) -> None:
    with pytest.raises(FileNotFoundError):
        spice.simulate(tmp_path / "nope.cir")


def test_includes_with_file_is_refused(examples: Path) -> None:
    with pytest.raises(ValueError, match="includes"):
        spice.simulate(examples / "rc.cir", includes={"a": "b"})


def test_bad_netlist_type() -> None:
    with pytest.raises(TypeError):
        spice.simulate(b"t\n.end\n")  # type: ignore[arg-type]


def test_file_deck_resolves_includes_on_disk(tmp_path: Path) -> None:
    (tmp_path / "lib").mkdir()
    (tmp_path / "lib" / "r.lib").write_text("r1 1 2 1k\nr2 2 0 3k\n")
    deck = tmp_path / "top.cir"
    deck.write_text("t\n.include lib/r.lib\nv1 1 0 4\n.op\n.end\n")
    op = spice.simulate(deck)[0]
    assert op.column("v(2)")[0] == pytest.approx(3.0)


def test_str_naming_an_existing_file_is_rejected(examples: Path) -> None:
    """A str is deck text; catching the path-as-str slip early."""
    import pytest

    import spice

    with pytest.raises(ValueError, match="pathlib.Path"):
        spice.simulate(str(examples / "rc.cir"))
