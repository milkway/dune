# Changelog — spice3 (Python)

## Unreleased

- **`.tran` integrates with TR-BDF2 by default** instead of the
  trapezoidal rule: closer to the converged answer at the same step
  on the whole test corpus, and it does not ring. The new option
  `compat="spice3"` (`.options compat=spice3` in a deck) restores the
  SPICE3f5 defaults; `method="trap"` restores just the method.
  `spice.pss()` keeps the trapezoidal rule unless `method` is given.

## 0.1.2

- `spice.pss()` refuses a deck with `devnoise` on (a `SpiceError`),
  as it already did for `TRNOISE`/`TRRANDOM` sources; the option used
  to be silently ignored by the shooting.

## 0.1.1

- The Linux wheel is now `manylinux2014` (glibc >= 2.17). The 0.1.0
  wheel was tagged `manylinux_2_39`, so pip fell back to the sdist
  (and needed a Rust toolchain) on anything older than Ubuntu 24.04.
- `spice.pss()` documentation: transmission lines are supported (the
  lossy RLC/RC lines of the O element need `.options ltrafit`); the
  0.1.0 docstring still said they were not.

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
