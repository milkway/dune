# spice (development version)

* **Temperature sweeps re-evaluate the deck:** `temper` and the
  `tc1`/`tc2` of behavioural R, C and L follow every point of a
  `.dc temp` sweep, a `temp` option given after parsing and the
  session's temperature (before, `.dc temp` with `temper` was an
  error and other changes used the value read once).
* **Seeded random functions:** `options = list(seed = n)` is now exactly
  `.options seed=n` in the deck, in `spice_run()`, `spice_sweep()`,
  `spice_session()`, `spice_pss()` and figure runs: it draws the deck's
  `agauss`, `gauss`, `aunif`, `unif` and `limit` from the seed (before,
  it only seeded the transient noise). Without a seed they keep their
  nominal value. `seed = "random"`, and a deck with `.options
  seed=random`, draw a seed with R's random number generator, so
  `set.seed()` makes it reproducible. Results gain `seed` (the seed the
  run was drawn with, `NA` when nominal) and `random_mode` (`"nominal"`
  or `"seeded"`); `options = list(seed = res$seed)` repeats a run.
* **Monte Carlo of the deck's own random functions:** `spice_sweep()`
  takes `seed` as a swept parameter: run `i` is the deck with `.options
  seed=` its seed (`seed = 1:n` for `n` samples), composing with the
  other parameters in grid and zip mode, in parallel, bit for bit. The
  swept seed is a column of `params` and of `spice_tidy()`'s long
  tibble; on a deck without random functions each run notes that the
  seed only changes the transient noise (or nothing).
* A deck that reads `temper` follows the `temp` option,
  `spice_set_temp()` and a swept `temp` exactly as `.options temp=`
  written in it (now tested).
* `seed` must now be a whole number from 1 to `2^32 - 1` (was below
  `2^53`): ngspice reads a seed as an unsigned 32-bit integer, so a
  larger one only aliased a smaller one.

* **Models from ngspice-47:** BSIM4 4.8.3 (`LEVEL=14|54`; cards that
  declare version 4.0-4.7 run on the 4.8.3 equations, with a note), the
  VDMOS power MOSFET, the VBIC bipolar transistor (`LEVEL=4|9`, with
  self-heating), the KSPICE lossy and coupled transmission lines
  (elements `Y` and `P`), and the ngspice diode beyond the SPICE3 card
  (`LEVEL=3` geometry, sidewall junction, recombination, tunnelling,
  high injection, `TLEV`/`TLEVC`).
* **Open PDK model libraries are read directly:** `.lib`/`.endl`
  sections, the parameter expression language (`{...}`, `'...'`,
  functions, `?:`), subcircuit parameters, `.model` binning, `m` on
  `X` lines and on devices, `.option scale`, `.func`,
  `.if`/`.elseif`/`.else`/`.endif`, `temper`, and behavioural R, C and
  L. The gf180mcu and sky130 inverters run from the PDK files and
  match ngspice-47 within 1 mV.
* The `.temp` card is applied (it was ignored).
* `.pz`: a voltage source with an AC value is opened, as in SPICE3;
  differential ports keep their negative node; the search no longer
  loses poles when it converges onto a real root.
* The branch currents of a `Y` or `P` line are named `i(y1#1)`,
  `i(y1#2)`, ... (ngspice repeats `i(y1)`).
* The package now also contains code under the Educational Community
  License 2.0 (BSIM4); `inst/COPYRIGHTS` reproduces the notices.
* `.four` cards (Fourier analysis and THD, ngspice-47 semantics) come
  back in `res$fourier`: a long tibble with one row per harmonic of
  each vector (`card`, `vector`, `harmonic`, `frequency`, `magnitude`,
  `phase` in degrees, `norm_magnitude`, `norm_phase`, `thd` in
  percent, `gridsize`, `polydegree`, `nperiods`, `error`). The deck's
  `.options` `nfreqs`, `nperiods`, `polydegree` and `fourgridsize`
  apply. A vector that cannot be analysed is a row with `error` set.
  `spice_fourier()` analyses any transient plot the same way, as
  `spice_measure()` measures one. Sweep runs and session reruns carry
  their `fourier` too.

# spice 0.1.0

First release, aligned with Dune v0.1.0.

* `spice_run()` runs 'SPICE3' decks (text, a path or a `spice_pic()`
  extraction) through the 'Dune' engine: operating point, DC sweep,
  transient, AC, pole-zero, noise, distortion, transfer function,
  sensitivity and periodic steady state (`spice_pss()`), with the full
  'SPICE3f5' device set plus BSIM3v3. Results are long tibbles
  (`spice_tidy()`, `autoplot()`).
* Selecting from a result reads as a pipeline:
  `res |> spice_tidy("tran", "out")`. `plots` takes a short name of
  the analysis (`"op"`, `"dc"`, `"tran"`, `"ac"`, `"noise"`, ...)
  besides labels and positions; `vars` keeps the named variables,
  without regard to case, a bare node name standing for its voltage.
  The tidy table of a complex plot (`.ac`) has `mag`, `db` and `phase`
  (degrees) next to `re` and `im`. `spice_wide()` gives one plot with
  a column per variable, and `spice_op()` the operating point as a
  named vector.
* `spice_read_raw()` reads 'SPICE3' rawfiles; `spice_measure()`,
  `spice_session()` (`spice_alter()`, `spice_rerun()`), `spice_sweep()`
  and a 'knitr' engine (`spice_knitr()`) round out the workflow.
* Vignettes: `vignette("spice")` runs a deck end to end, and
  `vignette("defaults")` shows where the engine departs from 'SPICE3'
  by default and how `compat = "spice3"` turns it back. Four worked
  examples with their results computed in advance: an amplifier
  (`"amplifier"`), oscillators and periodic steady state
  (`"oscillators"`), sweeps and tolerances (`"sweeps"`) and
  transmission lines (`"lines"`).
* `spice_pic()` and `spice_pic_svg()` extract netlists from 'rpic'
  figures and draw them.
* A `.tran` integrates with TR-BDF2 by default, not the trapezoidal
  rule: it is closer to the converged answer at the same step on the
  whole test corpus and does not ring. The option `compat = "spice3"`
  (`.options compat=spice3` in a deck) restores the 'SPICE3f5'
  defaults. `spice_pss()` shoots with the same default; only the
  start-up transient of an autonomous oscillator stays on the
  trapezoidal rule, which lets the oscillation grow at any step.
* A switch (`S`, `W`) flips at the instant its control crosses the
  threshold, located to `reltol` times the maximum step, not at
  whatever timepoint first sees it crossed (`compat = "spice3"`
  restores that). A `u()` or `sgn()` in a 'B' source flips at the
  instant its argument crosses zero, too. A pulse narrower than the
  timestep is found when the point before the step shows it coming. 'LTRA' lines without an
  interpolation flag interpolate linearly. Their step is also
  limited by the error of reading the delayed wave between
  history points. A corner of a wave gets a timepoint where it
  arrives one delay later (the 'SPICE' rule almost never plants
  it), so reflections no longer round it. Under TR-BDF2 the step control is held to `trtol / 7`
  (its error estimate does not overestimate as the one `trtol`
  was made for), about 3% more timepoints for a third less error.
  The slope breakpoints of 'LTRA' lines are
  capped at 10 per maximum step (`ltrabkrate = 0` schedules every
  request, as 'SPICE' does).
* On Windows every simulation runs on a worker thread, so that a
  threaded `spice_sweep()` agrees bit for bit with the sequential one
  and with `spice_run()`: the 'Rtools' C runtime rounds `exp()` and
  `pow()` according to the thread's x87 precision mode, which differs
  between the R thread and new threads.
* The Rust sources are vendored in `src/rust/vendor.tar.xz` and the
  installation builds offline, with at most two jobs.
