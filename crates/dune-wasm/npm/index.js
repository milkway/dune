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

/** Extract the deck of a `spice = 1` pic figure: `{ netlist, diagnostics, map }`. */
/** The pic prelude; render SVG from `prelude() + "\n" + src` so overlay ids match. */
export function prelude() {
  ensure();
  return wasm.prelude();
}

export function extractPic(src, opts) {
  ensure();
  return wasm.extract_pic(src, opts);
}

/** Extract a figure's deck and run it: `simulate`'s result plus `pic`. */
export function simulatePic(src, opts) {
  ensure();
  return wasm.simulate_pic(src, opts);
}
