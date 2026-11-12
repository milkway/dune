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
