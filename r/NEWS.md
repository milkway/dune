# spice 0.1.0

First release, aligned with Dune v0.1.0.

* `spice_run()` runs 'SPICE3' decks (text, a path or a `spice_pic()`
  extraction) through the 'Dune' engine: operating point, DC sweep,
  transient, AC, pole-zero, noise, distortion, transfer function,
  sensitivity and periodic steady state (`spice_pss()`), with the full
  'SPICE3f5' device set plus BSIM3v3. Results are long tibbles
  (`spice_tidy()`, `autoplot()`).
* `spice_read_raw()` reads 'SPICE3' rawfiles; `spice_measure()`,
  `spice_session()` (`spice_alter()`, `spice_rerun()`), `spice_sweep()`
  and a 'knitr' engine (`spice_knitr()`) round out the workflow.
* `spice_pic()` and `spice_pic_svg()` extract netlists from 'rpic'
  figures and draw them.
* A `.tran` integrates with TR-BDF2 by default, not the trapezoidal
  rule: it is closer to the converged answer at the same step on the
  whole test corpus and does not ring. The option `compat = "spice3"`
  (`.options compat=spice3` in a deck) restores the 'SPICE3f5'
  defaults. `spice_pss()` keeps the trapezoidal rule unless `method`
  is given.
* On Windows every simulation runs on a worker thread, so that a
  threaded `spice_sweep()` agrees bit for bit with the sequential one
  and with `spice_run()`: the 'Rtools' C runtime rounds `exp()` and
  `pow()` according to the thread's x87 precision mode, which differs
  between the R thread and new threads.
* The Rust sources are vendored in `src/rust/vendor.tar.xz` and the
  installation builds offline, with at most two jobs.
