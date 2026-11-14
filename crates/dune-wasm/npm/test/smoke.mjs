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
// Pins: card order, nets of the map, terminal points in pic inches
// (R1 runs from Vin (0,1.5) to Vout (2.5,1.5)).
const r1 = ex.map.elements[1];
assert.deepEqual(r1.pins.map((p) => [p.name, p.net]), [['P', 'Vin'], ['N', 'Vout']]);
assert.ok(Math.abs(r1.pins[0].x) < 1e-9 && Math.abs(r1.pins[0].y - 1.5) < 1e-9);
assert.ok(Math.abs(r1.pins[1].x - 2.5) < 1e-9 && Math.abs(r1.pins[1].y - 1.5) < 1e-9);
// Write-back span of R1's value (`R1: resistor(Vin, Vout, 1k)`, line 6).
assert.equal(r1.value.text, '1k');
assert.deepEqual(r1.value.span, { line: 6, col: 25, end_line: 6, end_col: 27 });
assert.equal(r1.principal, 'resistance');
assert.equal(ex.map.elements[0].params[0].name, 'pulse');
assert.equal(typeof ex.map.canvas.origin[0], 'number');
const netNames = new Set(ex.map.nets.map((n) => n.name));
assert.ok(ex.map.elements.every((e) => e.pins.every((p) => netNames.has(p.net))));

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
// The canvas maps R1.P onto the SVG (a stroke endpoint, six decimals).
{
  const { origin, h } = ex.map.canvas;
  const f = (v) => String(Math.round(v * 1e6) / 1e6);
  const p = r1.pins[0];
  const pt = `${f((p.x + origin[0]) * 96)},${f((h - (p.y + origin[1])) * 96)}`;
  assert.ok(svg.includes(pt), pt);
}
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
// .pss: the period real (like .tran), the harmonics [re, im] pairs (like .ac).
const pss = simulate('rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.pss 1k 5m 2 128 10\n.end\n');
assert.deepEqual(pss.plots.map((p) => [p.flags, p.npoints, p.columns[1][1].length]), [['real', 129, undefined], ['complex', 10, 2]]);
// ...and its numbers: one `pss` entry per card (period, t0, history).
assert.equal(pss.pss.length, 1);
assert.deepEqual([pss.pss[0].plot, pss.pss[0].period, pss.pss[0].t0], [0, 1e-3, 5e-3]);
assert.equal(pss.pss[0].periods.length, pss.pss[0].residuals.length);
assert.ok(pss.pss[0].residuals.at(-1) <= 1);
assert.equal(pss.plots[0].columns[0][0], pss.pss[0].t0);
assert.deepEqual(simulateTyped('rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.pss 1k 5m 2 128 10\n.end\n').pss, pss.pss);
assert.deepEqual(simulate('d\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n').pss, []);
// A shooting cut short (SCITER 1): kind pss with the history; a driven
// circuit has no period iterate.
const pssCut = 'rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 1u\n.pss 1k 5m 2 128 10 1\n.end\n';
const isCut = (e) =>
  e.kind === 'pss' && e.iterations === 1 && e.residuals.length === 1 && e.residuals[0] > 1 &&
  e.period === null && /did not converge/.test(e.message);
assert.throws(() => simulate(pssCut), isCut);
assert.throws(() => simulateTyped(pssCut), isCut);
{
  const s = Session.fromDeck(pssCut);
  assert.throws(() => s.run(), isCut);
  s.free();
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

// .measure results (issue #14): flat rows on every result shape.
const mDeck = 'rc measured\nv1 in 0 pulse(0 1 1n 1p 1p 20n 40n)\nr1 in out 1k\nc1 out 0 1p\n.tran 10p 15n\n' +
  '.meas tran trise trig v(out) val=0.1 rise=1 targ v(out) val=0.9 rise=1\n' +
  '.meas tran never when v(out)=5\n.end\n';
const tau = 1e3 * 1e-12;
const mr = simulate(mDeck);
assert.equal(mr.measurements.length, 2);
const [trise, never] = mr.measurements;
assert.deepEqual(Object.keys(trise), ['name', 'analysis', 'plot', 'value', 'details']);
assert.deepEqual([trise.name, trise.analysis, trise.plot], ['trise', 'tran', 0]);
assert.ok(Math.abs(trise.value / (Math.log(9) * tau) - 1) < 1e-3, `trise = ${trise.value}`);
assert.deepEqual(Object.keys(trise.details), ['trig', 'targ']);
assert.equal(trise.details.targ - trise.details.trig, trise.value);
assert.ok(!('error' in trise));
assert.equal(never.value, null);
assert.deepEqual(never.details, {});
assert.equal(never.error, 'out of interval (WHEN)');
assert.deepEqual(simulate('d\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n').measurements, []);
assert.deepEqual(simulateTyped(mDeck).measurements, mr.measurements);
// Session: R doubled -> tau and trise doubled; every run re-measures.
const ms = Session.fromDeck(mDeck);
assert.deepEqual(ms.run().measurements, mr.measurements);
ms.alter('r1', 2e3);
const t2 = ms.run().measurements[0].value;
assert.ok(Math.abs(t2 / trise.value - 2) < 1e-3, `${trise.value} -> ${t2}`);
assert.ok(Math.abs(t2 / (Math.log(9) * 2 * tau) - 1) < 1e-3);
assert.equal(ms.runTyped().measurements[0].value, t2);
ms.free();

// .four results (D-0061): one row per harmonic, a failed vector is a row.
const fDeck = 'rc\nv1 1 0 sin(0 1 1k)\nr1 1 2 1k\nc1 2 0 100n\n.tran 10u 5m\n' +
  '.four 1k v(2) v(7)\n.end\n';
const fr = simulate(fDeck);
assert.equal(fr.fourier.length, 11);
const h1 = fr.fourier[1];
assert.deepEqual(Object.keys(h1), ['card', 'vector', 'harmonic', 'frequency', 'magnitude',
  'phase', 'norm_magnitude', 'norm_phase', 'thd', 'gridsize', 'polydegree', 'nperiods']);
assert.deepEqual([h1.card, h1.vector, h1.harmonic, h1.frequency], [0, 'v(2)', 1, 1000]);
const wrc = 2 * Math.PI * 1e3 * 1e-4;
assert.ok(Math.abs(h1.magnitude * Math.sqrt(1 + wrc * wrc) - 1) < 1e-3, `|V1| = ${h1.magnitude}`);
assert.equal(h1.norm_magnitude, 1);
assert.ok(!('error' in h1));
const bad = fr.fourier[10];
assert.equal(bad.vector, 'v(7)');
assert.equal(bad.error, 'no such vector v(7)');
assert.equal(bad.magnitude, null);
assert.deepEqual(simulate('d\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n').fourier, []);
assert.deepEqual(simulateTyped(fDeck).fourier, fr.fourier);
const fs = Session.fromDeck(fDeck);
assert.deepEqual(fs.run().fourier, fr.fourier);
fs.alter('r1', 2e3);
assert.ok(fs.run().fourier[1].magnitude < h1.magnitude);
fs.free();
// A figure's .meas line: rows at the top level of simulatePic's result.
const rcMeas = rc.replace('.tran 20u 5m', '.tran 20u 5m\n.meas tran trise trig v(vout) val=0.5 rise=1 targ v(vout) val=4.5 rise=1');
const pm = simulatePic(rcMeas).measurements;
assert.equal(pm.length, 1);
assert.ok(Math.abs(pm[0].value / (Math.log(9) * 1e-3) - 1) < 1e-2, `pic trise = ${pm[0].value}`);
assert.deepEqual(simulatePicTyped(rcMeas).measurements, pm);

// Engine overrides (issue #16): bit for bit the deck-side edit.
const fine = { options: { trtol: 1 }, tmaxDivisor: 10 };
const rcFineDeck = ex.netlist.replace('.tran 20u 5m', '.options trtol=1\n.tran 20u 5m 0 2u');
const bits = (r) => r.plots.map((p) => p.columns.map((c) => Array.from(new Uint32Array(c.buffer))));
const rcFine = simulatePicTyped(rc, fine);
assert.deepEqual(bits(rcFine), bits(simulateTyped(rcFineDeck)));
assert.ok(rcFine.plots[0].npoints > tr.plots[0].npoints, 'denser');
assert.deepEqual(simulatePic(rc, fine).plots, simulate(rcFineDeck).plots);
assert.deepEqual(simulate(rcFineDeck, {}).plots, simulate(ex.netlist, fine).plots);
assert.deepEqual(simulatePic(rc, {}).plots, simulatePic(rc).plots, 'defaults unchanged');
const warned = simulate('d\nv1 1 0 1\nr1 1 0 1k\n.op\n.end\n', { options: { trtol: 'abc', bogus: 1 } });
assert.equal(warned.diagnostics.filter((x) => x.message.startsWith('opts.options: ')).length, 2);
assert.throws(() => simulate(ex.netlist, { tmaxDivisor: 0.5 }), (e) => e.kind === 'options');
assert.throws(() => Session.fromPic(rc, { tmaxDivisor: -1 }), (e) => e.kind === 'options');
// Session: the "fine steps" toggle without reopening.
const fs2 = Session.fromPic(rc);
assert.deepEqual(fs2.options, {});
assert.equal(fs2.tmaxDivisor, undefined);
fs2.options = { trtol: 1 };
fs2.tmaxDivisor = 10;
assert.deepEqual(fs2.options, { trtol: 1 });
assert.equal(fs2.tmaxDivisor, 10);
assert.deepEqual(bits(fs2.runTyped()), bits(rcFine));
assert.throws(() => { fs2.tmaxDivisor = 0; }, (e) => e.kind === 'options');
assert.throws(() => { fs2.options = 3; }, (e) => e.kind === 'options');
assert.equal(fs2.tmaxDivisor, 10, 'a refused divisor changes nothing');
fs2.options = null;
fs2.tmaxDivisor = null;
assert.deepEqual(fs2.run(), simulate(ex.netlist));
assert.deepEqual(fs2.run().plots, simulatePic(rc).plots);
fs2.free();

// The noise and keepopinfo cards as overrides: each is the deck with the
// `.options` card edited, bit for bit (mirrors python/tests/test_options.py
// and r/tests/testthat/test-options.R).
const noisy = (card) =>
  `parity\nvn 1 0 dc 0.5 trnoise(10m 1u 0 0)\nr1 1 2 1k\nc1 2 0 2n\n${card}\n.tran 1u 20u\n.end\n`;
const keepDeck = (card) => `keep\nv1 1 0 dc 1 ac 1\nr1 1 2 1k\nc1 2 0 1n\n${card}\n.ac dec 2 1k 100k\n.end\n`;
for (const [deck, options, card] of [
  [keepDeck, { keepopinfo: true }, '.options keepopinfo'],
  [noisy, { seed: 9 }, '.options seed=9'],
  [noisy, { notrnoise: true }, '.options notrnoise'],
  [noisy, { DevNoise: true, devnt: 2e-7, seed: 5 }, '.options devnoise devnt=0.2u seed=5'],
  [noisy, { devnoise: true, devnt: '0.2u', seed: 5 }, '.options devnoise devnt=0.2u seed=5'],
  // TR-BDF2 is the `.tran` default (D-0046): the trapezoidal rule and
  // the SPICE3 profile are what change the deck now.
  [noisy, { method: 'trap' }, '.options method=trap'],
  [noisy, { compat: 'spice3' }, '.options compat=spice3'],
]) {
  const got = simulateTyped(deck(''), { options });
  assert.equal(got.diagnostics.length, 0, JSON.stringify(got.diagnostics));
  assert.deepEqual(bits(got), bits(simulateTyped(deck(card))), card);
  assert.notDeepEqual(bits(got), bits(simulateTyped(deck(''))), card);
  const ns = Session.fromDeck(deck(''), { options });
  assert.deepEqual(bits(ns.runTyped()), bits(got), card);
  ns.free();
}
assert.deepEqual(
  simulate(keepDeck(''), { options: { keepopinfo: true } }).plots.map((p) => p.plotname),
  ['AC Operating Point', 'AC Analysis'],
);
assert.deepEqual(
  bits(simulateTyped(noisy('.options notrnoise devnoise'), { options: { notrnoise: false, devnoise: false } })),
  bits(simulateTyped(noisy(''))),
  'false clears the deck flags',
);
const badNoise = simulate(noisy(''), { options: { seed: 0, devnt: -1, notrnoise: 'yes' } });
assert.equal(badNoise.diagnostics.filter((x) => x.message.startsWith('opts.options: ')).length, 3);

console.log('dune-sim smoke: ok');
