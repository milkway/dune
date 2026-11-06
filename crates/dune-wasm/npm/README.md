# @strategicprojects/dune-sim

> Package name is provisional.

The [Dune](https://github.com/milkway/dune) circuit simulator — a Rust port of
Berkeley SPICE3f5 — compiled to WebAssembly. Run a SPICE3 netlist, or extract
the netlist of an [rpic](https://github.com/milkway/rpic-lang) schematic figure
(`spice = 1`) and simulate it. Browser and Node.

```js
import { ready, simulate, extractPic, simulatePic } from '@strategicprojects/dune-sim';

await ready(); // Node: await ready(fs.readFileSync(require.resolve('…/pkg/dune_wasm_bg.wasm')))

const deck = simulate('divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n');
// { plots: [{ plotname: 'Operating Point', variables, columns, … }], notes, diagnostics }

const figure = `.PS
spice = 1
Vin: (0,1.5); Vout: (2.5,1.5)
V1: vsource(Vin, (0,0), PULSE(0 5 0 1u 1u 10m 20m))
R1: resistor(Vin, Vout, 1k)
C1: capacitor(Vout, (2.5,0), 1u)
line from V1.N to C1.N
ground((1.25,0))
.tran 20u 5m
.PE`;
const { netlist, diagnostics, map } = extractPic(figure);
const run = simulatePic(figure); // simulate's result + { pic: { netlist, diagnostics, map } }
```

With Vite, pass the wasm URL explicitly:

```js
import wasmUrl from '@strategicprojects/dune-sim/pkg/dune_wasm_bg.wasm?url';
await ready(wasmUrl);
```

## API

- `ready(wasm?)` — initialize once (idempotent; a failed init can be retried).
- `simulate(netlist, { includes? })` → `{ plots, notes, diagnostics }`. Plots
  mirror the SPICE rawfile: `variables` (first = scale) and `columns`
  (numbers, or `[re, im]` pairs in complex plots; NaN/±inf as `null`).
  `includes` maps `.include` paths to file text (there is no file system).
- `extractPic(src, { snap? })` → `{ netlist, diagnostics, map }`. `map`
  relates netlist names to the shapes (`<g id="sN">`) of the figure as
  rpic compiles it with the dune prelude: `elements[{ refdes, shapes:
  { start, end } }]` and `nets[{ name, shapes: [n] }]`.
- `simulatePic(src, { snap?, includes? })` → `simulate`'s result plus
  `pic: { netlist, diagnostics, map }`.

Errors are thrown as `Error`s with `kind` (`options`, `parse`, `elaborate`,
`analysis`, `pic`, `internal`), `diagnostics` (with line/column) and, from
`simulatePic`, `pic` (the extraction) once a netlist exists. Pic diagnostics
carry a `kind` (`dangling-pin`, `missing-value`, …) and `end_column`; `file:
null` is the figure itself. Types are in `index.d.ts`.

Simulation runs to completion on the calling thread: call it from a Web
Worker you can terminate.

## Building

```sh
crates/dune-wasm/npm/build.sh   # from the dune repository
cd crates/dune-wasm/npm && npm test
```

License: BSD-3-Clause.

## rpic version note

The embedded pic prelude relies on troff-style `.`-line passthrough,
available in rpic ≥ 0.13. An embedder rendering `prelude() + source`
with an older rpic must blank the `.`-directive lines itself (keeping
line count) before compiling.
