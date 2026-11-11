# @strategicprojects/dune-sim

> Package name is provisional.

The [Dune](https://github.com/milkway/dune) circuit simulator — a Rust port of
Berkeley SPICE3f5 — compiled to WebAssembly. Run a SPICE3 netlist, or extract
the netlist of an [rpic](https://github.com/milkway/rpic-lang) schematic figure
(`spice = 1`) and simulate it. Browser and Node.

```js
import { ready, simulate, extractPic, simulatePic, renderPic } from '@strategicprojects/dune-sim';

await ready(); // Node: await ready(fs.readFileSync(require.resolve('…/pkg/dune_wasm_bg.wasm')))

const deck = simulate('divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n');
// { plots: [{ plotname: 'Operating Point', variables, columns, … }], measurements: [], notes, diagnostics }

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
const svg = renderPic(figure);   // <g id="sN"> ids match map
```

With Vite, pass the wasm URL explicitly:

```js
import wasmUrl from '@strategicprojects/dune-sim/pkg/dune_wasm_bg.wasm?url';
await ready(wasmUrl);
```

## API

- `ready(wasm?)` — initialize once (idempotent; a failed init can be retried).
- `simulate(netlist, { includes? })` → `{ plots, measurements, notes, diagnostics, pss }`. Plots
  mirror the SPICE rawfile: `variables` (first = scale) and `columns`
  (numbers, or `[re, im]` pairs in complex plots; NaN/±inf as `null`).
  `includes` maps `.include` paths to file text (there is no file system).
  A `.pss` card gives two plots on this same path: the period
  (`Time Domain Periodic Steady State Analysis`, real, like a `.tran`) and
  its complex harmonics (`Frequency Domain Periodic Steady State Analysis`,
  `[re, im]` pairs, like an `.ac`), and the result's `pss` list gets one
  entry per card: `{ plot, period, t0, residuals, periods }` — the period
  (the shooting's result for an autonomous oscillator), where it starts,
  and the residual and period of each shooting iteration (`[]` without a
  `.pss` card). A shooting that does not converge throws kind `pss` with
  `iterations`, `residuals` and `period` (the last period iterate of an
  autonomous oscillator, `null` for a driven circuit). Python's
  `spice.pss()` and R's `spice_pss()` carry the same numbers.
  `measurements` holds the deck's `.measure` results — the rows Python's
  `Result.measurements` and R's `res$measurements` carry, one per card and
  measured plot:

  ```js
  { name: 'trise', analysis: 'tran', plot: 0, value: 2.1972e-9,
    details: { trig: 1.106e-9, targ: 3.303e-9 } }
  { name: 'never', analysis: 'tran', plot: 0, value: null, details: {},
    error: 'out of interval (WHEN)' }
  ```

  `value` is `null` when the measurement failed (or is not finite);
  `error` is present only on failure; `details` is always an object
  (`trig`/`targ`, `at`, `with`, `from`/`to`, in ngspice's order). A
  `Session` re-measures on every `run()`.
- `extractPic(src, { snap? })` → `{ netlist, diagnostics, map }`. `map`
  relates netlist names to the shapes (`<g id="sN">`) of the figure as
  rpic compiles it with the dune prelude: `elements[{ refdes, shapes:
  { start, end } }]` and `nets[{ name, shapes: [n] }]`.
- `simulatePic(src, { snap?, includes? })` → `simulate`'s result plus
  `pic: { netlist, diagnostics, map }`.
- Engine overrides, in the `opts` of every `simulate*` and `Session.from*`
  (0.3.3): `options` — `.options` keys applied after the deck's cards
  (`{ trtol: 1, method: 'gear', temp: 50 }`, °C; a bad entry is a warning
  in `diagnostics`, not a throw; every key a `.options` card takes,
  including `method` (a `.tran` defaults to Dune's `'trbdf2'`),
  `compat: 'spice3'` (the SPICE3f5 defaults: a trapezoidal `.tran`,
  switches flipping on the time grid, no cap on LTRA breakpoints),
  `ltrafit`, `ltrabkrate`,
  `keepopinfo`, `seed`, `notrnoise`, `devnoise` and `devnt`) — and
  `tmaxDivisor` — a number ≥ 1 that
  divides every `.tran`'s maximum step (the card's TMAX, else
  `min(TSTEP, span/50)`). `{ options: { trtol: 1 }, tmaxDivisor: 10 }` is
  bit for bit the deck with `.options trtol=1` and the `.tran` card's TMAX
  at a tenth. On a `Session`, `session.options = {…}` and
  `session.tmaxDivisor = n` change them for the next `run()` without
  reopening (`null` restores the deck's): a "fine steps" toggle.
- `renderPic(src)` → SVG text of the figure, compiled with the dune
  prelude by dune's own rpic — the same compile `map` indexes, so the
  `<g id="sN">` groups always line up with it, whatever rpic version the
  page ships. Any figure rpic accepts renders (`spice = 1` is not
  required); a figure rpic rejects throws kind `pic` with one `compile`
  diagnostic. Supersedes rendering `prelude() + "\n" + src` with your own
  rpic; `prelude()` stays exported for that.
- `simulateTyped(netlist, opts?)` / `simulatePicTyped(src, opts?)` → the
  same results, but each plot's `columns` is an `Array` of `Float64Array`s:
  the values in a real plot, interleaved `re, im, re, im, …` (length
  `2 * npoints`) in a complex one; NaN/±inf are kept as such. Each array
  owns its buffer (a copy, not a view of wasm memory, which memory growth
  would detach), so it outlives later calls and moves out of a Worker
  without a copy:

  ```js
  const run = simulateTyped(deck);
  postMessage(run, run.plots.flatMap((p) => p.columns.map((c) => c.buffer)));
  ```

  On a 100k-point, 6-vector `.tran` (Node 24, Apple-silicon Mac):
  `simulate` 230 ms vs `simulateTyped` 178 ms; handing the result over
  costs 15 ms to structured-clone the JSON object (48 ms to
  `JSON.stringify` it) vs 0.8 ms to clone the typed one, ~0 ms to
  transfer it (`npm run bench`).

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
