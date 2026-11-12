# spice3

`pip install spice3` → `import spice`.

Circuit simulation for Python, powered by **Dune** — a Rust engine
derived from Berkeley **SPICE3f5** and validated circuit-by-circuit
against **ngspice** as numerical oracle.

> **Status: pre-alpha.** This release reserves the package name while
> the engine's first public version is being finished. The API below
> works today but may still change.

```python
from pathlib import Path
import spice

result = spice.simulate(Path("rc.cir"))
plot = result.plot("tran")   # or "Transient Analysis"
df = plot.df()               # polars DataFrame, scale column first
df = plot.df("2")            # only v(2): a bare node is its voltage
v2 = plot.column("v(2)")     # zero-copy, read-only numpy array
```

- Every SPICE3f5 analysis: `.op`, `.dc`, `.tran`, `.ac`, `.pz`,
  `.noise`, `.disto`, `.tf`, `.sens` — plus the periodic steady state
  of driven circuits (`.pss`, shooting–Newton; `spice.pss()`).
- All SPICE3f5 devices, including BJT (Gummel–Poon), MOS levels
  1/2/3/6, BSIM1/BSIM2, JFET, MESFET, switches and both
  transmission lines.
- Results as [polars](https://pola.rs) DataFrames (pandas optional)
  and zero-copy NumPy arrays; complete type annotations (`py.typed`).
- Schematics as decks: `spice.pic.extract()` reads the netlist of an
  [rpic](https://github.com/milkway/rpic-lang) figure drawn with the
  dune prelude, `spice.simulate()` runs it and `spice.pic.render()`
  draws it as SVG (with typeset `$...$` labels).
- Faithful netlist semantics: decks that run under `spice3 -b` or
  `ngspice -b` run here, with structured, span-carrying errors.

![How a simulation runs: the deck is parsed and built into a circuit; for every point of an analysis the devices are linearized, the linear system is solved and convergence is tested, iterating until it holds; an accepted point goes to the results.](https://milkway.github.io/dune/diagrams/simulation.svg)

The same engine sits under the R package, the JavaScript package, the
C library and the command line; a deck gives the same numbers in all
of them.

![Architecture of Dune: a pic figure goes through dune-pic and a SPICE deck through dune-netlist into dune-core, the engine; dune-raw writes rawfiles; the command line and the Python, R, JavaScript and C interfaces all sit on that one engine.](https://milkway.github.io/dune/diagrams/architecture.svg)

Site: [milkway.github.io/dune](https://milkway.github.io/dune/). 
[Dune repository](https://github.com/milkway/dune). BSD-3-Clause,
© André Leite and The Regents of the University of California.
