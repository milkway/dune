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
* The Rust sources are vendored in `src/rust/vendor.tar.xz` and the
  installation builds offline, with at most two jobs.
