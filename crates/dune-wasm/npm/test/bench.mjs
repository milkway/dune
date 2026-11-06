// JSON vs typed columns on a long .tran (run ./build.sh first):
//   node test/bench.mjs [points]       (default ~100k points)
// Times the simulate call and a postMessage-shaped hand-off
// (structuredClone; transfer lists for the typed variant).
import { readFileSync } from 'node:fs';
import { ready, simulate, simulateTyped } from '../index.js';

await ready(readFileSync(new URL('../pkg/dune_wasm_bg.wasm', import.meta.url)));

const points = Number(process.argv[2] ?? 100_000);
// RC ladder driven by a sine; tmax = tstep forces ~points steps.
const tstop = 1e-3;
const tstep = tstop / points;
const deck = `bench
v1 1 0 sin(0 1 10k)
r1 1 2 1k
c1 2 0 10n
r2 2 3 1k
c2 3 0 10n
r3 3 4 1k
c3 4 0 10n
.tran ${tstep} ${tstop} 0 ${tstep}
.end
`;

const time = (f) => {
  const t0 = performance.now();
  const v = f();
  return [v, performance.now() - t0];
};
const best = (n, f) => {
  let out;
  let ms = Infinity;
  for (let i = 0; i < n; i++) {
    const [v, t] = time(f);
    out = v;
    ms = Math.min(ms, t);
  }
  return [out, ms];
};

const N = 3;
const [js, tJs] = best(N, () => simulate(deck));
const [ty, tTy] = best(N, () => simulateTyped(deck));
const p = js.plots[0];
const [, tCloneJs] = best(N, () => structuredClone(js));
const [, tCloneTy] = best(N, () => structuredClone(ty));
const [, tTransfer] = best(N, () => {
  const r = simulateTyped(deck);
  return structuredClone(r, { transfer: r.plots.flatMap((q) => q.columns.map((c) => c.buffer)) });
});
const [, tStringify] = best(N, () => JSON.stringify(js));

console.log(`npoints ${p.npoints}, ${p.variables.length} vectors`);
console.log(`simulate (JSON)            ${tJs.toFixed(1)} ms`);
console.log(`simulateTyped              ${tTy.toFixed(1)} ms`);
console.log(`structuredClone JSON obj   ${tCloneJs.toFixed(1)} ms`);
console.log(`JSON.stringify JSON obj    ${tStringify.toFixed(1)} ms`);
console.log(`structuredClone typed      ${tCloneTy.toFixed(1)} ms (copy)`);
console.log(`simulateTyped + transfer   ${tTransfer.toFixed(1)} ms (total)`);
