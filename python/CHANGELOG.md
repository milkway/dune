# Changelog — spice3 (Python)

## 0.1.0

First functional release, aligned with Dune v0.1.0.

- `spice.simulate()` over SPICE3 decks (text, path or a figure
  extraction): op, dc, ac, tran, pz, noise, disto, tf, sens, sp, pss,
  with the full SPICE3f5 device set plus BSIM3v3; results as `Plot`s
  with NumPy columns (zero-copy) and polars frames (`pandas` optional).
- Rawfile read/write, `.measure`, interactive sessions (`alter`,
  `rerun`), parallel sweeps, typed options (`py.typed`, `_spice.pyi`).
- `spice.pic`: netlist extraction from rpic figures and their SVG
  (`$...$` labels typeset with the `texlabels` renderer).
- Distribution: abi3 wheels (CPython >= 3.10) for Linux x86_64,
  Windows x86_64 and macOS arm64/x86_64, plus an sdist that builds on
  its own (needs a Rust toolchain >= 1.88).

## 0.0.1

Placeholder upload that reserved the `spice3` name on PyPI.
