// Node smoke test of the built package (run ./build.sh first).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { extractPic, ready, simulate, simulatePic } from '../index.js';

const wasm = readFileSync(new URL('../pkg/dune_wasm_bg.wasm', import.meta.url));
const corpus = (name) => readFileSync(new URL(`../../../../corpus/pic/${name}`, import.meta.url), 'utf8');

assert.throws(() => simulate('t\n.end\n'), /ready\(\)/, 'calls before ready() throw');
await assert.rejects(() => ready(Buffer.from('not wasm')), undefined, 'garbage bytes reject');
const p1 = ready(wasm);
assert.equal(p1, ready(wasm), 'concurrent ready() share one init');
await p1;

// simulate: a divider.
const op = simulate('divider\nv1 1 0 dc 10\nr1 1 2 1k\nr2 2 0 1k\n.op\n.end\n');
const i2 = op.plots[0].variables.findIndex((v) => v.name === 'v(2)');
assert.ok(Math.abs(op.plots[0].columns[i2][0] - 5) < 1e-9);

// extractPic: rc.pic matches its golden deck; map shapes.
const rc = corpus('rc.pic');
const ex = extractPic(rc);
assert.equal(ex.netlist, corpus('golden/rc.cir'));
assert.deepEqual(ex.map.elements.map((e) => e.refdes), ['V1', 'R1', 'C1']);
assert.equal(typeof ex.map.elements[0].shapes.start, 'number');
assert.equal(ex.map.nets[0].name, '0');

// simulatePic: the RC charges to 5(1 - e^-5) V by 5 ms.
const run = simulatePic(rc, { snap: 0.001 });
assert.equal(run.pic.netlist, ex.netlist);
const tran = run.plots[0];
assert.equal(tran.plotname, 'Transient Analysis');
const iv = tran.variables.findIndex((v) => v.name.toLowerCase() === 'v(vout)');
const vEnd = tran.columns[iv][tran.npoints - 1];
assert.ok(Math.abs(vEnd - 4.966) < 0.01, `v(vout) at 5 ms = ${vEnd}`);

// Errors: kind/diagnostics on the thrown Error.
const unmarked = (() => {
  try {
    extractPic('.PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n');
  } catch (e) {
    return e;
  }
})();
assert.ok(unmarked instanceof Error);
assert.equal(unmarked.kind, 'pic');
assert.equal(unmarked.diagnostics[0].kind, 'not-marked');

const missing = (() => {
  try {
    simulatePic('.PS\nspice = 1\nV1: vsource((0,1),(0,0), 1)\nR1: resistor((0,1),(0,0))\nground((0,0))\n.op\n.PE\n');
  } catch (e) {
    return e;
  }
})();
assert.equal(missing.kind, 'pic');
assert.ok(missing.diagnostics.some((d) => d.kind === 'missing-value'));
assert.match(missing.pic.netlist, /^R1 /m);

// Warnings ride along.
const dangling = simulatePic('.PS\nspice = 1\nV1: vsource((0,1),(0,0), 1)\nR1: resistor((0,1),(1,1),1k)\nground((0,0))\n.op\n.PE\n');
const d = dangling.pic.diagnostics.find((x) => x.kind === 'dangling-pin');
assert.equal(d.severity, 'warning');
assert.deepEqual([d.file, d.line, d.column], [null, 4, 5]);

assert.throws(() => simulatePic(rc, { snap: -1 }), (e) => e.kind === 'options');

console.log('dune-sim smoke: ok');
