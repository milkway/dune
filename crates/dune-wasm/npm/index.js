// dune-sim — the Dune SPICE3 simulator in WebAssembly.
//
// Browser:  await ready();                        // wasm fetched next to the module
// Vite:     await ready(wasmUrl);                 // import wasmUrl from '…/pkg/dune_wasm_bg.wasm?url'
// Node:     await ready(fs.readFileSync(url));     // pass the .wasm bytes
//
// Every call runs to completion on the calling thread: run it in a Web
// Worker you can terminate (a runaway deck cannot be interrupted from
// inside the wasm).
import init, * as glue from './pkg/dune_wasm.js';

let initPromise = null;
let wasm = null;

/**
 * Initialize the WebAssembly module (idempotent; concurrent calls share
 * one init). A failed init (bad bytes, fetch error) clears the slot so
 * `ready()` can be retried.
 */
export function ready(wasmInput) {
  if (initPromise) return initPromise;
  const attempt = init(wasmInput === undefined ? undefined : { module_or_path: wasmInput })
    .then(() => {
      wasm = glue;
    })
    .catch((e) => {
      if (initPromise === attempt) initPromise = null;
      throw e;
    });
  initPromise = attempt;
  return initPromise;
}

function ensure() {
  if (!wasm) throw new Error('dune-sim: call `await ready()` first');
}

/** Run a SPICE3 deck; returns `{ plots, notes, diagnostics }`. Throws a `SimError`. */
export function simulate(netlist, opts) {
  ensure();
  return wasm.simulate(netlist, opts);
}

/**
 * Run a SPICE3 deck like `simulate`, but each plot's `columns` is an Array of
 * `Float64Array`s (interleaved re,im in complex plots; NaN/±inf kept). The
 * arrays own their buffers (copies, not views of wasm memory): list
 * `plot.columns.map((c) => c.buffer)` as transferables to `postMessage`.
 */
export function simulateTyped(netlist, opts) {
  ensure();
  return wasm.simulate_typed(netlist, opts);
}

/**
 * The pic prelude. Superseded by `renderPic`; kept for embedders that render
 * `prelude() + "\n" + src` with their own rpic (blanking `.`-lines before rpic 0.13).
 */
export function prelude() {
  ensure();
  return wasm.prelude();
}

/** Extract the deck of a `spice = 1` pic figure: `{ netlist, diagnostics, map }`. */
export function extractPic(src, opts) {
  ensure();
  return wasm.extract_pic(src, opts);
}

/** Extract a figure's deck and run it: `simulate`'s result plus `pic`. */
export function simulatePic(src, opts) {
  ensure();
  return wasm.simulate_pic(src, opts);
}

/** `simulatePic` with typed-array `columns`, as in `simulateTyped`. */
export function simulatePicTyped(src, opts) {
  ensure();
  return wasm.simulate_pic_typed(src, opts);
}

/**
 * SVG of the figure, compiled with the dune prelude by dune's own rpic — the
 * same compile the overlay map indexes, so `<g id="sN">` lines up with
 * `extractPic(src).map` regardless of the embedder's rpic version. Supersedes
 * rendering `prelude() + src` yourself. No `spice = 1` needed. Throws kind
 * `pic` (one `compile` diagnostic) when rpic rejects the figure.
 */
export function renderPic(src, opts) {
  ensure();
  return wasm.render_pic(src, opts);
}
