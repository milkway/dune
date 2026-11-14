# Changelog — spice3 (Python)

## Unreleased

- **`.four` cards** (Fourier analysis and THD, ngspice-47 semantics)
  come back in `Result.fourier`: a polars frame with one row per
  harmonic of each vector (`card`, `vector`, `harmonic`, `frequency`,
  `magnitude`, `phase` in degrees, `norm_magnitude`, `norm_phase`,
  `thd` in percent, `gridsize`, `polydegree`, `nperiods`, `error`;
  dtypes in `spice.FOURIER_SCHEMA`). The deck's `.options` `nfreqs`,
  `nperiods`, `polydegree` and `fourgridsize` apply. A vector that
  cannot be analysed is a row with `error` set (`nan` numbers, null
  integers). `spice.fourier(plot, freq, vectors, *, nfreqs, nperiods,
  polydegree, gridsize)` analyses any transient plot the same way, as
  `spice.measure()` measures one. Sweep runs and session runs carry
  their `fourier` too.

## 0.7.1

Metadata only; the code is that of 0.7.0.

- The package carries a `NOTICE` file crediting SPICE3f5, ngspice and
  BSIM3v3, with their copyright and licence notices, and the `LICENSE`
  file; both ship in the sdist and in the wheels. The description and
  the README say that Dune derives from Berkeley SPICE3f5 and, in
  parts, ngspice, and that it is an independent project.
- Author and maintainer e-mail: leite@castlab.org.
- Figures are compiled with rpic 0.19.0 (0.7.0 was built with 0.16.1).

## 0.7.0

- **Short names for plots**: `result.plot("tran")`, `"ac"`, `"op"`,
  `"dc"`, `"noise"`, `"inoise"`, `"tf"`, `"pz"`, `"sens"`, `"sp"`,
  `"pss"` and `"pss_spectrum"` stand for the rawfile plot names
  (`"Transient Analysis"`, ...), which keep working.
- **`Plot.df(vars)`** keeps the named vectors and the scale. Names
  are matched without regard to case, and a bare node name stands for
  its voltage: `plot.df("out")` is `plot.df(["v(out)"])`. The same
  rule applies to `Plot.column()` and `name in plot`.
- **`Plot.df(polar=True)`** adds `db:<name>`, `mag:<name>` and
  `phase:<name>` (degrees) for each vector of a complex plot.
- **`Result.op()`** gives the operating point as a dict, optionally
  restricted to some vectors: `result.op("out")`.

## 0.6.1

- A `u()`/`sgn()` whose argument crosses zero and comes back inside
  one timestep is no longer skipped: the pulse is found when the
  point before the step shows it coming (a comparator on the crest of
  a sine, 14 µs wide on a 50 µs grid, gives every pulse at its
  width). A pulse that leaves no trace on its neighbouring timepoints
  still needs a smaller maximum step.

## 0.6.0

- **LTRA lines find the corners of the waves they carry.** SPICE's
  rule for planting a breakpoint where a delayed corner arrives
  almost never fires with the model's default `rel=1`; the corner
  then falls between timepoints and is rounded a little more at
  every reflection. On a lossless line with a known exact answer the
  error over 60 ns of reflections goes from 1.0 % to 0.16 %.
- **`trytocompact`** no longer loses accuracy as the step shrinks:
  a history point is dropped only while the line through the last
  kept point still passes by every point dropped since.

## 0.5.0

- **LTRA lines**: the step is also limited by the error of reading
  the delayed wave between history points (the lossy-line test deck
  is 40 % closer to the converged answer for 2 % more timepoints),
  and with `trytocompact` a corner of the wave is never compacted
  away, which ends the breakpoint cascade of refined runs.
- A B source that feeds its own argument (a comparator with positive
  feedback) is located in about a third fewer timepoints.

## 0.4.0

- **B-source jumps are located.** A `u()` or `sgn()` whose argument
  crosses zero flips at that instant (to `reltol` times the maximum
  step) when the expression jumps there, like the switches.
- **LTRA lines**: a model without an interpolation flag interpolates
  linearly (the quadratic rule overshoots at corners; on the two
  line decks of the SPICE3 examples the error at the deck's step
  falls by more than half), and a settled line no longer plants
  breakpoints on numerical residue. `compat="spice3"` restores both.

## 0.3.0

- **Switch events are located.** An `S`/`W` switch flips at the
  instant its control crosses the threshold, to within `reltol` times
  the maximum step, instead of at whatever timepoint first sees it
  crossed; `compat="spice3"` restores the SPICE behaviour.
- **`spice.pss()` shoots with TR-BDF2 by default**, like `.tran`:
  about half the period error of the trapezoidal rule at the same
  `points`. Only the start-up transient of an autonomous oscillator
  stays on the trapezoidal rule, which lets the oscillation grow at
  any step, whatever `method=` says; `method=` decides the shooting.
- **Tighter step control under TR-BDF2.** `trtol` (7) is SPICE's
  allowance for an error estimate that overestimates; TR-BDF2's own
  estimate does not, so the default profile holds it to `trtol/7`.
  About 3 % more timepoints on the test corpus for roughly a third
  less error at the deck's step. `trtol=` still scales it;
  `compat="spice3"` and `method="trap"`/`"gear"` are unchanged.
- An autonomous `spice.pss()` over transmission lines converges in
  about a third of the iterations.
- **`ltrabkrate` defaults to 10** (the cap on the slope breakpoints
  of LTRA lines per maximum step): a transient whose line
  echoes cascade into breakpoints now ends instead of exhausting its
  step budget, and no result changes where there is no cascade.
  `ltrabkrate=0`, or `compat="spice3"`, schedules every request as
  SPICE does.

## 0.2.0

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
