// Slider loop: a fresh simulatePicTyped/simulateTyped per value against
// Session.alter + runTyped (run ./build.sh first):
//   node test/session-bench.mjs
import { readFileSync } from 'node:fs';
import { Session, ready, simulatePicTyped, simulateTyped } from '../index.js';

await ready(readFileSync(new URL('../pkg/dune_wasm_bg.wasm', import.meta.url)));
const corpus = (p) => readFileSync(new URL(`../../../../corpus/${p}`, import.meta.url), 'utf8');

const median = (n, f) => {
  for (let i = 0; i < 3; i++) f(i);
  const t = [];
  for (let i = 0; i < n; i++) {
    const t0 = performance.now();
    f(i);
    t.push(performance.now() - t0);
  }
  t.sort((a, b) => a - b);
  return t[n >> 1];
};

const row = (label, tFresh, tCold, tWarm) =>
  console.log(
    `| ${label} | ${tFresh.toFixed(3)} ms | ${tCold.toFixed(3)} ms | ${(tFresh / tCold).toFixed(2)}× | ${tWarm.toFixed(3)} ms | ${(tFresh / tWarm).toFixed(2)}× |`,
  );
console.log('| case (wasm, node) | fresh | session | speedup | session warm | speedup |');
console.log('|---|---|---|---|---|---|');

// rc.pic (.tran): the figure is re-extracted by every fresh run.
{
  const src = corpus('pic/rc.pic');
  const vals = ['1k', '2k'];
  const n = 30;
  const tFresh = median(n, (i) => simulatePicTyped(src.replace('1k)', `${vals[i % 2]})`)));
  const s = Session.fromPic(src);
  const tCold = median(n, (i) => s.alter('R1', [1e3, 2e3][i % 2]).runTyped());
  s.warmStart = true;
  const tWarm = median(n, (i) => s.alter('R1', [1e3, 2e3][i % 2]).runTyped());
  row('rc.pic .tran (figure)', tFresh, tCold, tWarm);
  s.free();
}

// Op-only decks: parse/elaboration and the operating point dominate.
for (const [label, path, from, to, el, param, a, b] of [
  ['bsim3_inv .op', 'regress/bsim3_inv.cir', 'mn3 7 6 8 0 n1 l=1u w=20u', 'mn3 7 6 8 0 n1 l=1u w=22u', 'mn3', 'w', 20e-6, 22e-6],
  ['mosamp2 .op', 'spice3f5/examples/mosamp2.cir', 'm7   5 20  8 32 m w=482.6u', 'm7   5 20  8 32 m w=500u', 'm7', 'w', 482.6e-6, 500e-6],
]) {
  const base = corpus(path)
    .split('\n')
    .filter((l) => !/^\s*\.(op|ac|tran|dc|print|plot|end)\b/i.test(l))
    .join('\n');
  const decks = [base, base.replace(from, to)].map((d) => `${d}\n.op\n.end\n`);
  const n = 100;
  const tFresh = median(n, (i) => simulateTyped(decks[i % 2]));
  const s = Session.fromDeck(decks[0]);
  const tCold = median(n, (i) => s.alter(el, param, [b, a][i % 2]).runTyped());
  s.warmStart = true;
  const tWarm = median(n, (i) => s.alter(el, param, [b, a][i % 2]).runTyped());
  row(label, tFresh, tCold, tWarm);
  s.free();
}
