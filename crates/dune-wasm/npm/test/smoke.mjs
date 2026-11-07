// Node smoke test of the built package (run ./build.sh first).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  Session,
  extractPic,
  prelude,
  ready,
  renderPic,
  simulate,
  simulatePic,
  simulatePicTyped,
  simulateTyped,
} from '../index.js';

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

// renderPic: dune's own compile; the map's ids are in the SVG.
const svg = renderPic(rc);
assert.match(svg, /^<svg|<svg /);
for (const e of ex.map.elements) {
  for (let n = e.shapes.start; n < e.shapes.end; n++) assert.ok(svg.includes(`id="s${n}"`), `s${n}`);
}
for (const n of ex.map.nets.flatMap((x) => x.shapes)) assert.ok(svg.includes(`id="s${n}"`), `net s${n}`);
assert.match(renderPic('.PS\nR1: resistor((0,0),(1,0),1k)\n.PE\n'), /<svg/, 'unmarked figures render');
assert.throws(() => renderPic('.PS\nline from (0,0) to\n.PE\n'), (e) => e.kind === 'pic' && e.diagnostics[0].kind === 'compile');
assert.throws(() => renderPic(rc, { snap: 1 }), (e) => e.kind === 'options');
assert.equal(typeof prelude(), 'string', 'prelude() still exported');

// simulateTyped: same data as the JSON columns, as Float64Arrays.
const acDeck = 'rc\nv1 1 0 dc 1 ac 1\nr1 1 2 1k\nc1 2 0 1u\n.op\n.ac dec 2 10 1k\n.end\n';
const js = simulate(acDeck);
const ty = simulateTyped(acDeck);
assert.deepEqual(ty.notes, js.notes);
assert.equal(ty.plots.length, js.plots.length);
for (const [k, tp] of ty.plots.entries()) {
  const jp = js.plots[k];
  assert.deepEqual({ ...tp, columns: null }, { ...jp, columns: null });
  const complex = tp.flags.startsWith('complex');
  assert.equal(tp.columns.length, tp.variables.length);
  for (const [i, col] of tp.columns.entries()) {
    assert.ok(col instanceof Float64Array);
    assert.equal(col.length, (complex ? 2 : 1) * tp.npoints);
    const flat = complex ? jp.columns[i].flat() : jp.columns[i];
    assert.deepEqual(Array.from(col), flat);
  }
}
// Own buffers: transferable, and they survive later calls (memory growth).
const tr = simulatePicTyped(rc);
assert.equal(tr.pic.netlist, ex.netlist);
const before = Array.from(tr.plots[0].columns[1]);
const cols0 = tr.plots[0].columns.map((c) => c.buffer);
assert.equal(new Set(cols0).size, cols0.length, 'one buffer per column');
simulatePicTyped(rc);
assert.deepEqual(Array.from(tr.plots[0].columns[1]), before);
const moved = structuredClone(tr, { transfer: cols0 });
assert.equal(tr.plots[0].columns[1].length, 0, 'transferred (detached at the source)');
assert.deepEqual(Array.from(moved.plots[0].columns[1]), before);
assert.throws(() => simulateTyped('t\nq1 1 1 0\n.op\n.end\n'), (e) => typeof e.kind === 'string');
assert.throws(() => simulatePicTyped(rc, { snap: -1 }), (e) => e.kind === 'options');

// Session: alter + run is simulate() of the altered deck.
const sDeck = 'rc\nv1 1 0 dc 1 ac 1 pulse(0 1 0 1u 1u 1m 2m)\nr1 1 2 1k\nc1 2 0 1u\nd1 2 0 dm\n.model dm d\n.op\n.ac dec 3 10 1k\n.tran 20u 1m\n.end\n';
assert.throws(() => Session.fromDeck('t\nr1 1 0\n.end\n'), (e) => e.kind === 'elaborate');
const ses = Session.fromDeck(sDeck);
assert.deepEqual(ses.run(), simulate(sDeck));
ses.alter('R1', 2200).alter('v1', 'pulse', [0, 2, 0, 1e-6, 1e-6, 1e-3, 2e-3]);
ses.temp = 50;
const sAlt = sDeck.replace('r1 1 2 1k', 'r1 1 2 2200').replace('pulse(0 1', 'pulse(0 2').replace('.op', '.options temp=50\n.op');
assert.deepEqual(ses.run(), simulate(sAlt));
const st = ses.runTyped();
assert.ok(st.plots[0].columns[0] instanceof Float64Array);
assert.equal(ses.param('r1'), 2200);
assert.deepEqual(Array.from(ses.param('v1', 'pulse')), [0, 2, 0, 1e-6, 1e-6, 1e-3, 2e-3]);
assert.deepEqual(ses.alterable('c1'), ['capacitance', 'ic']);
assert.throws(() => ses.alter('r1', 0), (e) => e.kind === 'alter');
assert.throws(() => ses.alter('v1', 'pulse', ['x']), (e) => e.kind === 'alter');
assert.throws(() => ses.alter('zz', 1), (e) => e.kind === 'alter');
ses.warmStart = true;
ses.alter('r1', 2300).run();
ses.alter('r1', 2400).run();
assert.ok(ses.warmStats.hits >= 1, JSON.stringify(ses.warmStats));
ses.free();
// From a figure, by refdes.
const ps = Session.fromPic(rc);
assert.equal(ps.pic.netlist, ex.netlist);
ps.alter('C1', 2e-6);
const psAlt = simulate(ex.netlist.replace('C1 Vout 0 1u', 'C1 Vout 0 2u'));
assert.deepEqual(ps.run(), psAlt);
ps.free();

console.log('dune-sim smoke: ok');
