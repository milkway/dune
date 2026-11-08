# Dune

**SPICE, reborn in Rust.** Dune is a faithful rewrite of Berkeley's
SPICE3f5 circuit simulator: the same netlists, the same defaults, the
same numbers — validated circuit by circuit against ngspice — with a
clean engine you can call from Python, R, JavaScript or C.

**Site: [milkway.github.io/dune](https://milkway.github.io/dune/)** ·
[Examples gallery](https://milkway.github.io/dune/examples/) — every
corpus circuit, simulated · [Known differences](corpus/KNOWN_DIFFS.md)

[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.24052213.svg)](https://doi.org/10.5281/zenodo.24052213)

## What's inside

- **Every SPICE3f5 device and analysis**: R C L K, V/I sources, E F G H,
  diode, BJT (Gummel–Poon), MOS levels 1–6, BSIM1/2, JFET, MESFET,
  switches, ideal and lossy (LTRA) transmission lines, `.subckt` — and
  `.OP .DC .AC .TRAN .PZ .NOISE .DISTO .TF .SENS`. Plus BSIM3v3 and
  S-parameter analysis (`.sp`) from modern ngspice.
- **Oracle-tested**: nothing ships until every corpus circuit matches
  pinned ngspice-47 goldens point by point, at SPICE's own tolerances
  (`reltol`/`abstol`/`vntol`). Deliberate divergences are documented in
  [`corpus/KNOWN_DIFFS.md`](corpus/KNOWN_DIFFS.md), each behind a
  measured gate.
- **Schematics in, waveforms out**: Dune reads figures written in
  [rpic](https://rpic.dev)'s pic language and extracts runnable decks
  (`dune pic figure.pic`); the same engine powers the Simulate panel in
  rpic Studio, all client-side WebAssembly.
- **Rawfiles** in SPICE3's own binary and ASCII formats, so existing
  viewers read them unchanged.
- **Opt-in robust transient** (`.options method=trbdf2`): an L-stable
  TR-BDF2 integrator for decks the classic trapezoidal rule rings on.
  The faithful behavior stays byte-identical by default.

## Install

```sh
# Python — PyPI (polars DataFrames, zero-copy numpy; module `import spice`)
pip install spice3

# JavaScript / WebAssembly — npm (runs in the browser)
npm install @strategicprojects/dune-sim

# R — tidy tibbles (CRAN submission planned)
Rscript -e 'remotes::install_github("milkway/dune", subdir = "r")'

# CLI and Rust library — from source (Rust 1.88+)
git clone https://github.com/milkway/dune && cd dune && cargo build --release
```

```python
import spice

run = spice.simulate("""
step into a series rlc
V1 in 0 PULSE(0 1 0 1n 1n 1 2)
R1 in n1 60
L1 n1 out 1m
C1 out 0 1n
.tran 0.1u 40u
.end
""")
run.plots[0].frame()   # polars DataFrame: time, v(in), v(out)…
```

## Repository layout

| Path | What |
|---|---|
| `crates/dune-core` | The engine: devices, analyses, solver |
| `crates/dune-netlist` | SPICE3f5-faithful netlist parser |
| `crates/dune-raw` | Rawfile reader/writer |
| `crates/dune-pic` | Netlist extraction from pic figures |
| `crates/dune-cli` | The `dune` command |
| `crates/dune-wasm` | WebAssembly bindings (`@strategicprojects/dune-sim`) |
| `crates/dune-ffi` | C ABI |
| `python/`, `r/` | The `spice3` (PyPI) and `spice` (R) packages |
| `corpus/` | SPICE3f5 sources (the reference), test decks, ngspice goldens |
| `site/` | The website and examples gallery |

## License

BSD-3-Clause — © 2026 André Leite; portions derived from SPICE3f5,
© 1985–1991 The Regents of the University of California. See
[LICENSE](LICENSE).
