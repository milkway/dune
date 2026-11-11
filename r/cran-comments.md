# CRAN comments — spice 0.1.0 (new submission)

## R CMD check results

0 errors | 0 warnings | 1 note (Linux, Windows); 2 notes on the local
macOS machine (see the test environments below)

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

All with `R CMD check --as-cran` on the source tarball, unpacked away
from the development repository (offline Rust build from the vendored
sources), rustc 1.99.1:

* local: macOS 26 (aarch64-apple-darwin), R 4.6.0 — 0 errors,
  0 warnings, 2 notes (the incoming-feasibility note above; "HTML
  version of manual" from the old `tidy` shipped with macOS, not a
  package issue).
* GitHub Actions: ubuntu-latest (x86_64-pc-linux-gnu), R 4.6.2 —
  0 errors, 0 warnings, 1 note (incoming feasibility).
* GitHub Actions: windows-latest (x86_64-w64-mingw32, Rtools45),
  R 4.6.2 ucrt — 0 errors, 0 warnings, 1 note (incoming feasibility).

One test file (`test-corpus.R`, the engine's whole circuit corpus) is
skipped outside the development repository: the tarball carries only
the corpus files the other tests read.

## Reverse dependencies

None: this is a new package.
