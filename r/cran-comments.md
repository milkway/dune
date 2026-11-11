# CRAN comments — spice 0.1.0 (new submission)

## R CMD check results

0 errors | 0 warnings | 1 note

* New submission; size of tarball (~11 MB), from the vendored Rust
  sources (see below).

## Rust code (CRAN policy compliance)

The package statically links compiled Rust code (via 'savvy'):

* `SystemRequirements: Cargo (Rust's package manager), rustc (>= 1.88.0)`;
  `configure` reports the `cargo`/`rustc` versions.
* All Rust sources are vendored in `src/rust/vendor.tar.xz`: the
  crates.io dependencies (`cargo vendor`, pinned by `Cargo.lock`) and
  the simulation engine crates (part of this package, not on
  crates.io). The build runs offline (`--offline -j 2`, at most two
  jobs), with `CARGO_HOME` and the target directory inside the package
  directory; both are removed after the build.
* Authorship of the vendored crates is acknowledged in `Authors@R`
  (`cph`) and itemized in `inst/AUTHORS`; full license texts ship
  inside the vendored sources.

## Test environments

* local macOS (R 4.6, rustc stable)
