# Contributing to Dune

Contributions are welcome: bug reports, circuits that Dune gets wrong,
fixes, device models, documentation.

## Before you write code

- **A bug or a wrong result**: open an issue with the smallest deck that
  shows it and, if you can, what ngspice prints for the same deck.
- **A fix**: a pull request is enough.
- **Anything that adds or changes public API** (the Rust crates, the C
  ABI, the wasm, Python or R packages), a command-line flag, a netlist
  extension or a default: open an issue first. These are discussed
  before they are written, because the three language packages mirror
  each other and a change in one has to land in all of them.

## What a pull request has to pass

CI runs these; run them locally before you push.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo run -p dune-oracle -- --ci        # the corpus against the ngspice-47 goldens
cargo build -p dune-wasm --target wasm32-unknown-unknown --release
```

`tools/preflight.sh` runs the whole set on a clean export of `HEAD`.

## Rules of the project

- **SPICE3f5 is the contract.** Netlist syntax, `.options` defaults,
  analysis semantics and the rawfile format follow it. A deliberate
  deviation is recorded in [`corpus/KNOWN_DIFFS.md`](corpus/KNOWN_DIFFS.md).
- **ngspice is the oracle.** A numerical change is done when
  `dune-oracle` is green with the SPICE tolerances (`reltol=1e-3`,
  `abstol=1e-12`, `vntol=1e-6`), not with tolerances chosen to pass.
- **Every fixed bug gets a regression deck**: a minimal `.cir` in
  `corpus/regress/`.
- **Hostile input never hangs or panics.** A malformed netlist yields a
  diagnostic with a source span.
- No `unsafe` outside `dune-ffi` and `dune-wasm`; `f64` everywhere;
  model parameter names as in SPICE3f5 (`IS`, `BF`, `VTO`…); rustdoc on
  every public item, with the equation or the reference it came from.
- Small commits, one subject each, message in the imperative
  (`netlist: parse .subckt with hierarchical node names`).

## Licences and the origin of code

Your contribution is licensed under the terms of the file it lands in:
BSD-3-Clause for the workspace (see [LICENSE](LICENSE)), ECL-2.0 for
the BSIM4 module.

Dune is a translation of other people's code, so the origin of every
piece matters:

- If you port from an upstream source, say which file and which release
  in the pull request, and add it to [NOTICE](NOTICE). The upstream
  licence has to be compatible with BSD-3-Clause.
- **Do not port, or write while reading, GPL, LGPL or AGPL code.** In
  ngspice, stay within the files under its Modified BSD licence:
  `frontend/numparam` (LGPL) and the other exceptions listed in its
  `COPYING` are off limits. The parameter language of Dune was written
  from the manual and from the observed behaviour of ngspice for this
  reason.
- A new dependency has to pass `cargo deny check licenses`
  ([`deny.toml`](deny.toml): permissive licences only) and must not
  break the `wasm32` build of `dune-core`, `dune-netlist`, `dune-raw`
  or `dune-pic`.
