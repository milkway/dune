#!/usr/bin/env node
// Generates site/examples/index.html: every circuit of the dune-oracle
// corpus plus the pic figures, simulated by the local dune-wasm build.
//
//   (cd crates/dune-wasm/npm && ./build.sh)   # if pkg/ is stale
//   node tools/gen-examples.mjs
//
// Deterministic: no dates, no randomness, fixed ordering — two runs on
// the same tree write the same bytes. Every number on the page comes
// from `simulate`/`simulatePic`; nothing is hand-written.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const CORPUS = join(ROOT, 'corpus');
const NPM = join(ROOT, 'crates/dune-wasm/npm');
const OUT = join(ROOT, 'site/examples/index.html');

const { ready, simulate, extractPic, renderPic, simulatePic } = await import(join(NPM, 'index.js'));
await ready(readFileSync(join(NPM, 'pkg/dune_wasm_bg.wasm')));
const pkgVersion = JSON.parse(readFileSync(join(NPM, 'package.json'), 'utf8')).version;

// ---------------------------------------------------------------- corpus

const read = (p) => readFileSync(p, 'utf8');

/** corpus/SKIP: `<rel path> <reason>`. */
const skip = new Map(
  read(join(CORPUS, 'SKIP'))
    .split('\n')
    .filter((l) => l.trim() && !l.startsWith('#'))
    .map((l) => {
      const m = l.match(/^(\S+)\s+(.*)$/);
      return [m[1], m[2]];
    }),
);

/** corpus/XFAIL: `stem|plotname|max_margin|flags|reason` (reason may contain `|`). */
const xfail = new Map();
for (const l of read(join(CORPUS, 'XFAIL')).split('\n')) {
  if (!l.trim() || l.startsWith('#')) continue;
  const [stem, plotname, margin, flags, ...rest] = l.split('|');
  xfail.set(stem, { plotname, margin, flags, reason: rest.join('|') });
}

/** Tracked files only: the page documents the committed corpus (what CI
 *  validates), not work in progress sitting in the working tree. */
const tracked = (pattern) =>
  execFileSync('git', ['ls-files', '-z', '--', pattern], { cwd: ROOT })
    .toString()
    .split('\0')
    .filter(Boolean);

const cirs = (dir) =>
  tracked(`corpus/${dir}/*.cir`)
    .map((p) => p.slice('corpus/'.length))
    .sort();

const regress = cirs('regress');
const groups = [
  {
    id: 'basic',
    name: 'Basic',
    blurb: 'Small decks written for Dune to pin each analysis and linear element against ngspice-47.',
    files: cirs('basic'),
  },
  {
    id: 'regress',
    name: 'Regression & device probes',
    blurb: 'One deck per fixed bug or ported device/analysis (corpus/regress/), each a minimal circuit with an ngspice golden.',
    files: regress.filter((f) => !/\/(bsim3_|sp_)/.test(f)),
  },
  {
    id: 'berkeley',
    name: 'Berkeley SPICE3f5 examples',
    blurb: 'The example decks shipped with the SPICE3f5 source distribution, run unchanged.',
    files: cirs('spice3f5/examples'),
  },
  {
    id: 'bsim3',
    name: 'BSIM3v3',
    blurb: 'BSIM3v3 probes (corpus/regress/bsim3_*), compared against ngspice-47.',
    files: regress.filter((f) => f.includes('/bsim3_')),
  },
  {
    id: 'sp',
    name: 'S-parameters',
    blurb: 'ngspice-style .sp analyses (corpus/regress/sp_*).',
    files: regress.filter((f) => f.includes('/sp_')),
  },
];

// ---------------------------------------------------------------- html utils

const esc = (s) =>
  String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const slug = (s) => s.replace(/\.cir$|\.pic$/, '').replace(/[^A-Za-z0-9_-]+/g, '-');

const PREFIX = [
  [1e12, 'T'], [1e9, 'G'], [1e6, 'M'], [1e3, 'k'], [1, ''],
  [1e-3, 'm'], [1e-6, 'µ'], [1e-9, 'n'], [1e-12, 'p'], [1e-15, 'f'],
];

/** SI-prefixed number, `digits` significant digits, trailing zeros trimmed. */
function si(v, unit = '', digits = 3) {
  if (v === null || v === undefined || Number.isNaN(v)) return 'NaN';
  if (!Number.isFinite(v)) return v > 0 ? '+inf' : '-inf';
  const sp = unit ? ' ' : '';
  if (v === 0) return `0${sp}${unit}`;
  const a = Math.abs(v);
  if (a >= 1e15 || a < 1e-15) {
    return `${v.toExponential(digits - 1).replace(/\.?0+e/, 'e')}${sp}${unit}`;
  }
  let [scale, p] = PREFIX[PREFIX.length - 1];
  for (const [s, q] of PREFIX) {
    if (a >= s * (1 - 1e-12)) {
      scale = s;
      p = q;
      break;
    }
  }
  let m = (v / scale).toPrecision(digits);
  if (m.includes('.')) m = m.replace(/\.?0+$/, '');
  return `${m}${sp}${p}${unit}`;
}

// ---------------------------------------------------------------- vectors

function unitOf(v) {
  const n = v.name;
  if (/^S_\d+_\d+$/.test(n)) return '';
  if (/^Y_\d+_\d+$/.test(n)) return 'S';
  if (/^Z_\d+_\d+$/.test(n)) return 'Ω';
  switch (v.type) {
    case 'voltage': return 'V';
    case 'current': return 'A';
    case 'voltage-density': return 'V/√Hz';
    case 'current-density': return 'A/√Hz';
    case 'time': return 's';
    case 'frequency': return 'Hz';
    default: return '';
  }
}

const isComplex = (p) => p.flags.startsWith('complex');
const re = (x) => (Array.isArray(x) ? x[0] : x);
const mag = (x) => {
  if (!Array.isArray(x)) return x === null ? NaN : Math.abs(x);
  if (x[0] === null || x[1] === null) return NaN;
  return Math.hypot(x[0], x[1]);
};

/** Analysis keyword of a plot, as `.print`/`.plot` cards name it. */
function analysisKey(plotname) {
  if (plotname.startsWith('Transient')) return 'tran';
  if (plotname.startsWith('DC transfer')) return 'dc';
  if (plotname.startsWith('AC')) return 'ac';
  if (plotname.startsWith('Noise')) return 'noise';
  if (plotname.startsWith('DISTORTION')) return 'disto';
  if (plotname.startsWith('SP')) return 'sp';
  return null;
}

/** Vectors the deck's own `.plot`, then `.print`, cards ask for, per analysis. */
function deckRequests(deck) {
  const out = {};
  const lines = deck.split('\n');
  for (const card of ['.plot', '.print']) {
    for (const line of lines) {
      const t = line.trim();
      if (!t.toLowerCase().startsWith(card + ' ')) continue;
      const toks = t.split(/\s+/);
      const an = (toks[1] || '').toLowerCase();
      const rest = toks.slice(2).join(' ');
      const list = (out[an] ||= []);
      for (const m of rest.matchAll(/\b(v|vm|vdb|vr|i|im|idb|ir)\s*\(([^)]*)\)/gi)) {
        if (m[2].includes(',')) continue; // differential: not a stored vector
        const name = `${m[1][0].toLowerCase()}(${m[2].trim().toLowerCase()})`;
        if (!list.includes(name)) list.push(name);
      }
      for (const m of rest.matchAll(/"([io]noise)-spectrum"/gi)) {
        const name = `${m[1].toLowerCase()}_spectrum`;
        if (!list.includes(name)) list.push(name);
      }
    }
  }
  return out;
}

// ---------------------------------------------------------------- charts

const MAX_TRACES = 6;
const W = 600, H = 236, X1 = 568, Y0 = 14, Y1 = 196;
let X0 = 64; // set per chart from the widest y label
const BUCKETS = 240; // min+max per bucket → ≤ ~480 points per trace (≈1/px)

function niceStep(range, target = 5) {
  const raw = range / target;
  const p = 10 ** Math.floor(Math.log10(raw));
  const f = raw / p;
  return (f < 1.5 ? 1 : f < 3 ? 2 : f < 7 ? 5 : 10) * p;
}

function linTicks(lo, hi) {
  if (!(hi > lo)) {
    const d = lo === 0 ? 1 : Math.abs(lo) * 0.1;
    lo -= d;
    hi += d;
  }
  const st = niceStep(hi - lo);
  const a = Math.floor(lo / st + 1e-9) * st;
  const b = Math.ceil(hi / st - 1e-9) * st;
  const ticks = [];
  for (let k = 0; a + k * st <= b + st * 1e-6; k++) {
    const t = a + k * st;
    ticks.push(Math.abs(t) < st * 1e-9 ? 0 : t);
  }
  return { lo: a, hi: b, ticks };
}

function logTicks(lo, hi) {
  const a = Math.floor(Math.log10(lo) + 1e-9);
  let b = Math.ceil(Math.log10(hi) - 1e-9);
  if (b === a) b = a + 1;
  const n = b - a;
  const every = n > 12 ? 4 : n > 8 ? 3 : n > 5 ? 2 : 1;
  const ticks = [];
  for (let e = a; e <= b; e++) ticks.push({ v: 10 ** e, label: (e - a) % every === 0 });
  return { lo: 10 ** a, hi: 10 ** b, ticks };
}

const fx = (v) => v.toFixed(1);

/**
 * Min/max decimation per x bucket over one monotonic segment of
 * (x, y) pairs (already in screen space). Keeps endpoints and order.
 */
function decimate(pts) {
  if (pts.length <= 2 * BUCKETS) return pts;
  const out = [];
  const span = X1 - X0;
  let cur = -1, mn = null, mx = null;
  const flush = () => {
    if (mn === null) return;
    if (mn === mx) out.push(mn);
    else if (mn.i < mx.i) out.push(mn, mx);
    else out.push(mx, mn);
  };
  pts.forEach((p, i) => {
    const b = Math.min(BUCKETS - 1, Math.floor(((p[0] - X0) / span) * BUCKETS));
    const q = { p, i };
    if (b !== cur) {
      flush();
      cur = b;
      mn = mx = q;
    } else {
      if (p[1] < mn.p[1]) mn = q; // screen y: smaller = higher value
      if (p[1] > mx.p[1]) mx = q;
    }
  });
  flush();
  const res = out.map((q) => q.p);
  if (res[0] !== pts[0]) res.unshift(pts[0]);
  if (res[res.length - 1] !== pts[pts.length - 1]) res.push(pts[pts.length - 1]);
  return res;
}

/** [start, end) index ranges: a nested sweep restarts the scale, so split where x decreases. */
function segments(xs) {
  const segs = [];
  let s0 = 0;
  for (let k = 1; k < xs.length; k++) {
    if (xs[k] < xs[k - 1]) {
      segs.push([s0, k]);
      s0 = k;
    }
  }
  segs.push([s0, xs.length]);
  return segs;
}

/** One chart (single unit family). `series`: [{name, ys}] sharing `xs`. */
function chartSvg(xs, xUnit, xLog, series, yUnit, label) {
  const segs = segments(xs);

  const okX = (x) => Number.isFinite(x) && (!xLog || x > 0);
  const xv = xs.filter(okX);
  const xr = xLog ? logTicks(Math.min(...xv), Math.max(...xv)) : null;
  const xLo = xLog ? Math.min(...xv) : Math.min(...xv);
  const xHi = Math.max(...xv);

  const all = series.flatMap((s) => s.ys.filter((y, k) => Number.isFinite(y) && okX(xs[k])));
  const yMin = Math.min(...all), yMax = Math.max(...all);
  // Log magnitude only in the frequency domain (|H|, noise densities);
  // time and DC sweeps stay linear even when a node sits near 0.
  const yLog = xUnit === 'Hz' && yMin > 0 && yMax / yMin > 1e3;
  const yr = yLog ? logTicks(yMin, yMax) : linTicks(yMin, yMax);

  // Left margin from the widest y tick label (monospace, sized for the 14px phone labels ≈ 8.6 units/char).
  const yLabels = yLog
    ? yr.ticks.filter((t) => t.label).map((t) => si(t.v, yUnit))
    : yr.ticks.map((t) => si(t, yUnit));
  X0 = Math.round(Math.min(120, Math.max(40, 12 + 8.6 * Math.max(...yLabels.map((l) => l.length)))));

  const xmap = xLog
    ? (x) => X0 + ((Math.log10(x) - Math.log10(xLo)) / (Math.log10(xHi) - Math.log10(xLo) || 1)) * (X1 - X0)
    : (x) => X0 + ((x - xLo) / (xHi - xLo || 1)) * (X1 - X0);
  const ymap = yLog
    ? (y) => Y1 - ((Math.log10(y) - Math.log10(yr.lo)) / (Math.log10(yr.hi) - Math.log10(yr.lo))) * (Y1 - Y0)
    : (y) => Y1 - ((y - yr.lo) / (yr.hi - yr.lo)) * (Y1 - Y0);

  const grid = [];
  const text = [];
  if (yLog) {
    for (const t of yr.ticks) {
      const y = ymap(t.v);
      if (t.label || yr.ticks.length <= 9) grid.push(`<line x1="${X0}" y1="${fx(y)}" x2="${X1}" y2="${fx(y)}"/>`);
      if (t.label) text.push(`<text class="plot-axis" x="${X0 - 6}" y="${fx(y + 3.5)}" text-anchor="end">${esc(si(t.v, yUnit))}</text>`);
    }
  } else {
    for (const t of yr.ticks) {
      const y = ymap(t);
      grid.push(`<line x1="${X0}" y1="${fx(y)}" x2="${X1}" y2="${fx(y)}"/>`);
      text.push(`<text class="plot-axis" x="${X0 - 6}" y="${fx(y + 3.5)}" text-anchor="end">${esc(si(t, yUnit))}</text>`);
    }
  }
  if (xLog) {
    for (const t of xr.ticks) {
      if (t.v < xLo * (1 - 1e-9) || t.v > xHi * (1 + 1e-9)) continue;
      const x = xmap(t.v);
      grid.push(`<line x1="${fx(x)}" y1="${Y0}" x2="${fx(x)}" y2="${Y1}"/>`);
      if (t.label) text.push(`<text class="plot-axis" x="${fx(x)}" y="${Y1 + 15}" text-anchor="middle">${esc(si(t.v, xUnit))}</text>`);
    }
  } else {
    const st = niceStep(xHi - xLo || 1, 5);
    const first = Math.ceil(xLo / st - 1e-9) * st;
    for (let t = first; t <= xHi + st * 1e-6; t += st) {
      const tv = Math.abs(t) < st * 1e-9 ? 0 : t;
      const x = xmap(tv);
      grid.push(`<line x1="${fx(x)}" y1="${Y0}" x2="${fx(x)}" y2="${Y1}"/>`);
      text.push(`<text class="plot-axis" x="${fx(x)}" y="${Y1 + 15}" text-anchor="middle">${esc(si(tv, xUnit))}</text>`);
    }
  }
  // Frame edges, so a plot never floats.
  grid.push(`<line x1="${X0}" y1="${Y0}" x2="${X0}" y2="${Y1}"/>`, `<line x1="${X1}" y1="${Y0}" x2="${X1}" y2="${Y1}"/>`);

  const traces = series.map((s, si_) => {
    let d = '';
    for (const [a, b] of segs) {
      let run = [];
      const runs = [];
      for (let k = a; k < b; k++) {
        const x = xs[k], y = s.ys[k];
        if (!okX(x) || !Number.isFinite(y) || (yLog && y <= 0)) {
          if (run.length) runs.push(run);
          run = [];
          continue;
        }
        run.push([xmap(x), ymap(y)]);
      }
      if (run.length) runs.push(run);
      for (const r of runs) {
        const pts = decimate(r);
        d += 'M' + pts.map((p) => `${fx(p[0])} ${fx(p[1])}`).join('L');
      }
    }
    return `<path class="tr" style="stroke:var(--trace-${si_ + 1})" d="${d}"><title>${esc(s.name)}</title></path>`;
  });

  return (
    `<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(label)}">` +
    `<g class="plot-grid">${grid.join('')}</g>${text.join('')}${traces.join('')}</svg>`
  );
}

function legendHtml(series, extra) {
  const items = series.map(
    (s, k) => `<span><span class="sw" style="background:var(--trace-${k + 1})"></span>${esc(s.name)}</span>`,
  );
  if (extra > 0) items.push(`<span class="more">(+${extra} more ${extra === 1 ? 'vector' : 'vectors'})</span>`);
  return `<div class="legend">${items.join('')}</div>`;
}

const FAMILY_ORDER = ['V/√Hz', 'A/√Hz', 'V', '', 'A', 'S', 'Ω'];

/** Chart(s) of a swept plot: tran, dc, ac, noise spectrum, disto, sp. */
function renderSwept(p, requests) {
  const cplx = isComplex(p);
  const xs = p.columns[0].map((x) => (x === null ? NaN : re(x)));
  const scale = p.variables[0];
  const xUnit = unitOf(scale);
  const xv = xs.filter(Number.isFinite);
  const xLog = scale.type === 'frequency' && Math.min(...xv) > 0 && Math.max(...xv) / Math.min(...xv) > 10;
  const ys = (k) => p.columns[k].map((x) => (cplx ? mag(x) : x === null ? NaN : x));

  const vars = p.variables.map((v, k) => ({ ...v, k, lname: v.name.toLowerCase() })).slice(1);
  const segs = segments(xs);
  const key = analysisKey(p.plotname);
  let chosen = [];

  const req = (key && requests[key]) || [];
  for (const n of req) {
    const v = vars.find((x) => x.lname === n);
    if (v && !chosen.includes(v)) chosen.push(v);
  }
  if (!chosen.length) {
    let cands;
    if (key === 'noise') cands = vars.filter((v) => v.lname.endsWith('_spectrum'));
    else if (key === 'sp') cands = vars.filter((v) => /^S_\d+_\d+$/.test(v.name));
    else {
      cands = vars.filter((v) => !v.name.includes('#'));
      if (!cands.length) cands = vars;
      cands = cands.filter((v) => {
        const col = ys(v.k);
        const y = col.filter(Number.isFinite);
        if (!y.length) return false;
        // constant within every sweep segment (a fixed bias, or the outer
        // source of a nested sweep): nothing to see along x
        const flat = segs.every(([a, b]) => {
          const w = col.slice(a, b).filter(Number.isFinite);
          if (!w.length) return true;
          const lo = Math.min(...w), hi = Math.max(...w);
          return hi - lo <= 1e-9 * Math.max(Math.abs(hi), Math.abs(lo)) + 1e-18;
        });
        if (flat) return false;
        if (key === 'dc') {
          // a node that just follows (or mirrors) the swept source: v = ±x + c
          for (const sgn of [1, -1]) {
            const c = col[0] - sgn * xs[0];
            const tol = 1e-9 * Math.max(1, ...xs.map(Math.abs), Math.abs(c));
            if (xs.every((x, k) => Math.abs(col[k] - sgn * x - c) <= tol)) return false;
          }
        }
        return true;
      });
      if (!cands.length) cands = vars.filter((v) => !v.name.includes('#')).slice(0, 1);
    }
    // one unit family: the first present in FAMILY_ORDER
    const fam = FAMILY_ORDER.find((u) => cands.some((v) => unitOf(v) === u));
    chosen = cands.filter((v) => unitOf(v) === fam);
  }

  // Group the chosen vectors by unit (requests may mix V and A): one chart each.
  const fams = [];
  for (const v of chosen) {
    const u = unitOf(v);
    let f = fams.find((x) => x.unit === u);
    if (!f) fams.push((f = { unit: u, vars: [] }));
    f.vars.push(v);
  }
  const shown = fams.reduce((n, f) => n + Math.min(MAX_TRACES, f.vars.length), 0);
  const omitted = vars.length - shown;
  const parts = fams.map((f, i) => {
    const series = f.vars.slice(0, MAX_TRACES).map((v) => ({ name: v.name, ys: ys(v.k) }));
    const extra = i === fams.length - 1 ? omitted : 0;
    const label = `${p.plotname}: ${series.map((s) => s.name).join(', ')} versus ${scale.name}`;
    return chartSvg(xs, xUnit, xLog, series, f.unit, label) + legendHtml(series, extra);
  });
  const nested = xs.some((x, k) => k > 0 && x < xs[k - 1]);
  const tags = [];
  if (cplx) tags.push('magnitude');
  if (xLog) tags.push('log f');
  if (nested) tags.push('nested sweep');
  if (req.length && chosen.length) tags.push('vectors from the deck’s .plot/.print');
  return { tags, html: parts.join('') };
}

function num(v, unit, sci) {
  if (v === null || v === undefined) return 'NaN';
  if (sci) return v === 0 ? '0' : Number.isFinite(v) ? v.toExponential(4) : String(v);
  return si(v, unit, 5);
}

function cell(x, unit, cplx, sci) {
  if (!cplx) return esc(num(x, unit, sci));
  const [a, b] = x;
  if (a === null || b === null) return 'NaN';
  if (b === 0) return esc(num(a, unit, sci));
  const sg = b < 0 || Object.is(b, -0) ? '−' : '+';
  return `${esc(num(a, '', sci))} ${sg} j${esc(num(Math.abs(b), '', sci))}${unit ? ' ' + esc(unit) : ''}`;
}

/** Scalar plots (op, tf, sens, noise totals, one-point ac): a compact table. */
function renderTable(p) {
  const cplx = isComplex(p);
  // Sensitivities are d(output)/d(parameter): mixed units, wide range.
  const sens = p.plotname === 'Sensitivity Analysis';
  const u = (v) => (sens ? '' : unitOf(v));
  const isZero = (x) => (Array.isArray(x) ? x[0] === 0 && x[1] === 0 : x === 0);
  let head, rows;
  let zeros = 0;
  if (['frequency', 'time'].includes(p.variables[0].type)) {
    const sc = p.variables[0];
    head = `<tr><th>vector</th>${p.columns[0].map((x) => `<th>${esc(si(re(x), unitOf(sc), 4))}</th>`).join('')}</tr>`;
    rows = p.variables.slice(1).flatMap((v, k) => {
      const col = p.columns[k + 1];
      if (sens && col.every(isZero)) return zeros++, [];
      return [`<tr><td>${esc(v.name)}</td>${col.map((x) => `<td>${cell(x, u(v), cplx, sens)}</td>`).join('')}</tr>`];
    });
  } else {
    head = '<tr><th>vector</th><th>value</th></tr>';
    rows = p.variables.flatMap((v, k) => {
      if (sens && isZero(p.columns[k][0])) return zeros++, [];
      return [`<tr><td>${esc(v.name)}</td><td>${cell(p.columns[k][0], u(v), cplx, sens)}</td></tr>`];
    });
  }
  const note = zeros
    ? `<div class="tnote">${rows.length} nonzero of ${rows.length + zeros} sensitivities shown; the other ${zeros} are exactly 0${p.npoints > 1 ? ' at every point' : ''}.</div>`
    : '';
  return `${note}<div class="tbl"><table><thead>${head}</thead><tbody>${rows.join('')}</tbody></table></div>`;
}

function renderPz(p) {
  const rows = p.variables.map((v, k) => {
    const x = p.columns[k][0];
    const m = v.name.match(/(pole|zero)\((\d+)\)/);
    const what = m ? `${m[1]} ${m[2]}` : v.name;
    const [a, b] = Array.isArray(x) ? x : [x, 0];
    return `<tr><td>${esc(what)}</td><td>${esc(si(a, '', 6))}</td><td>${esc(si(b, '', 6))}</td></tr>`;
  });
  return `<div class="tbl"><table><thead><tr><th></th><th>real</th><th>imag</th></tr></thead><tbody>${rows.join('')}</tbody></table></div>`;
}

function renderPlot(p, requests) {
  let body, tags = [];
  const scalarNames = ['Operating Point', 'Transfer Function', 'Integrated Noise', 'Sensitivity Analysis'];
  if (p.plotname === 'Pole-Zero Analysis') body = renderPz(p);
  else if (p.npoints <= 1 || scalarNames.includes(p.plotname) || p.npoints < 4) body = renderTable(p);
  else ({ html: body, tags } = renderSwept(p, requests));
  const tagHtml = tags.map((t) => `<span class="ptag">${esc(t)}</span>`).join('');
  return `<div class="scope res"><div class="cap"><span>${esc(p.plotname)}</span>${tagHtml}</div>${body}</div>`;
}

// ---------------------------------------------------------------- examples

const stats = { total: 0, ok: 0, failed: [], byGroup: {} };

/** The deck with Dune's opt-in robust integrator asked for
 *  (`.options method=trbdf2`, D-0027) — only to show, next to an
 *  expected faithful-mode failure, what the robust mode computes. */
function robustDeck(deck) {
  // Before the `.end` card when the deck has one; some corpus decks
  // (ltra_3) end at EOF instead, so append there.
  if (/^[ \t]*\.end[ \t]*$/im.test(deck)) {
    return deck.replace(/^([ \t]*\.end[ \t]*)$/im, '.options method=trbdf2\n$1');
  }
  return `${deck.replace(/\s*$/, '')}\n.options method=trbdf2\n`;
}

/** INPevaluate's number: mantissa plus one scale suffix, rest ignored. */
function spiceNum(tok) {
  const m = /^([+-]?(?:[0-9]*\.)?[0-9]+(?:e[+-]?[0-9]+)?)(meg|mil|[tgkmunpf])?/i.exec(tok);
  if (!m) return NaN;
  const scale = { '': 1, t: 1e12, g: 1e9, meg: 1e6, k: 1e3, mil: 25.4e-6, m: 1e-3, u: 1e-6, n: 1e-9, p: 1e-12, f: 1e-15 };
  return parseFloat(m[1]) * scale[(m[2] || '').toLowerCase()];
}

const DISPLAY_PTS = 600; // ≥ this many timepoints across every tran chart

/**
 * The display grid: tran output lands on the internal timepoints, whose
 * ceiling is TMAX = min(tstep, span/50) by default — ~50 segments on
 * many corpus decks, visibly angular and clipping ringing peaks. For the
 * charts only, cap TMAX at span/600 (same engine, same contract, just
 * more timepoints). Decks already that dense, and cards whose arguments
 * this cannot parse, are left alone.
 */
function displayDeck(deck) {
  return deck.replace(/^([ \t]*)\.tran\b([^\r\n]*)/gim, (line, ind, rest) => {
    const toks = rest.trim().split(/\s+/).filter(Boolean);
    const uic = toks.length && /^uic$/i.test(toks[toks.length - 1]) ? toks.pop() : '';
    if (toks.length < 2 || toks.length > 4 || toks.some((t) => !Number.isFinite(spiceNum(t)))) return line;
    const [tstep, tstop] = [spiceNum(toks[0]), spiceNum(toks[1])];
    const tstart = toks.length > 2 ? spiceNum(toks[2]) : 0;
    const span = tstop - tstart;
    if (!(span > 0)) return line;
    const effTmax = toks.length > 3 ? spiceNum(toks[3]) : Math.min(tstep, span / 50);
    if (!(effTmax > span / DISPLAY_PTS)) return line; // already dense
    const tmax = (span / DISPLAY_PTS).toExponential(6);
    return `${ind}.tran ${toks[0]} ${toks[1]} ${toks.length > 2 ? toks[2] : '0'} ${tmax}${uic ? ' uic' : ''}`;
  });
}

/** Simulate on the display grid, falling back to the deck's own run. */
function simulateDisplay(deck, faithful = () => simulate(deck)) {
  const dense = displayDeck(deck);
  if (dense !== deck) {
    try {
      return simulate(dense);
    } catch {
      // A grid the deck does not survive (step-kill corner cases):
      // the deck's own grid, then.
    }
  }
  return faithful();
}

function runResults(deck, stem, run) {
  const requests = deckRequests(deck);
  const out = [];
  const xf = xfail.get(stem);
  let res, err;
  try {
    res = run();
  } catch (e) {
    err = e;
  }
  if (err) {
    const diags = (err.diagnostics || [])
      .map((d) => `${d.severity}${d.line ? ` (line ${d.line})` : ''}: ${d.message}`)
      .join('\n');
    out.push(
      `<div class="notebox err"><b>Simulation failed</b> <span class="ptag">${esc(err.kind || 'error')}</span>` +
        `<pre>${esc(err.message)}${diags ? '\n' + esc(diags) : ''}</pre>` +
        (xf
          ? `<p><b>Expected:</b> this circuit is in <code>corpus/XFAIL</code> (${esc(xf.plotname)}, flags <code>${esc(xf.flags)}</code>). ${esc(xf.reason)}</p>`
          : '<p><b>Not in corpus/XFAIL.</b></p>') +
        '</div>',
    );
    stats.failed.push({ stem, kind: err.kind, message: err.message, xfail: !!xf });
    // An expected faithful-mode failure: also show the same deck under
    // Dune's opt-in robust integrator, so the failure above reads as a
    // property of the faithful contract, not of the engine.
    if (xf) {
      try {
        const res2 = simulateDisplay(robustDeck(deck));
        out.push(
          `<div class="notebox"><b>Solved by the robust mode</b> <span class="ptag">.options method=trbdf2</span>` +
            `<p>The same deck with Dune's opt-in L-stable TR-BDF2 integrator (D-0027, ` +
            `<a href="https://github.com/milkway/dune/issues/6">#6</a>) appended: the trapezoidal ` +
            `ring that seeds the failure is damped and the run completes. Below is what it computes — ` +
            `closer to a fine-step reference than the faithful mode or ngspice's own golden on this deck. ` +
            `Not the 3f5 contract: the oracle comparison runs plain <code>trap</code>, so the expected ` +
            `failure above stays.</p></div>`,
        );
        for (const p of res2.plots) out.push(renderPlot(p, requests));
      } catch (e2) {
        out.push(`<div class="notebox"><b>Robust mode</b><pre>${esc(e2.message || String(e2))}</pre></div>`);
      }
    }
    return out.join('');
  }
  stats.ok++;
  if (xf) {
    out.push(
      `<div class="notebox"><b>Known-marginal vs the oracle</b> <span class="ptag">XFAIL · ${esc(xf.plotname)}</span><p>${esc(xf.reason)}</p></div>`,
    );
  }
  if (!res.plots.length) out.push('<div class="notebox">No plots.</div>');
  for (const p of res.plots) out.push(renderPlot(p, requests));
  const notes = [...res.notes, ...(res.diagnostics || []).map((d) => `${d.severity}: ${d.message}`)];
  if (notes.length) out.push(`<ul class="notes">${notes.map((n) => `<li>${esc(n)}</li>`).join('')}</ul>`);
  return out.join('');
}

const titleOf = (deck) => deck.split('\n')[0].trim() || '(untitled deck)';
const analysesOf = (deck) => {
  const seen = [];
  for (const l of deck.split('\n').slice(1)) {
    const m = l.trim().toLowerCase().match(/^\.(op|dc|ac|tran|pz|noise|disto|tf|sens|sp)\b/);
    if (m && !seen.includes(m[1])) seen.push(m[1]);
  }
  return seen;
};

function exampleHtml(rel, groupId) {
  const deck = read(join(CORPUS, rel));
  const stem = rel.split('/').pop().replace(/\.cir$/, '');
  const id = `${groupId}-${slug(stem)}`;
  stats.total++;
  const pills = analysesOf(deck).map((a) => `<span class="pill">.${a}</span>`).join('');
  const results = runResults(deck, stem, () => simulateDisplay(deck));
  return {
    id,
    stem,
    html:
      `<article class="ex" id="${id}"><header class="ex-head"><h3>${esc(titleOf(deck))}</h3>` +
      `<div class="ex-meta"><code>corpus/${esc(rel)}</code>${pills}<a class="anchor" href="#${id}" aria-label="link to ${esc(stem)}">#</a></div></header>` +
      `<div class="ex-grid"><div class="panel src"><div class="cap">${esc(stem)}.cir</div><pre><code>${esc(deck.replace(/\s+$/, ''))}</code></pre></div>` +
      `<div class="ex-res">${results}</div></div></article>`,
  };
}

function figureSvg(src) {
  return renderPic(src)
    .replace(/^<svg([^>]*?) width="[^"]*" height="[^"]*"/, '<svg$1')
    .replace(/(fill|stroke)="(black|rgb\(0,0,0\)|#000000|#000)"/g, '$1="currentColor"')
    .replace(/ id="s\d+"/g, '')
    .replace(/<g>\s*<\/g>\s*/g, '')
    .replace(/(\d+\.\d{2})\d+/g, '$1');
}

function picHtml(file) {
  const src = read(join(CORPUS, 'pic', file));
  const stem = file.replace(/\.pic$/, '');
  const id = `pic-${slug(stem)}`;
  stats.total++;
  const includes = {};
  for (const m of src.matchAll(/^\.include\s+(\S+)/gim)) includes[m[1]] = read(join(CORPUS, 'pic', m[1]));
  const ex = extractPic(src);
  const golden = read(join(CORPUS, 'pic/golden', `${stem}.cir`));
  if (ex.netlist !== golden) process.stderr.write(`warning: ${file}: extracted netlist differs from golden/${stem}.cir\n`);
  // Title: the first line of the figure's leading comment (continued up to the first line ending in '.' or ':').
  const comment = [];
  for (const l of src.split('\n').slice(1)) {
    if (!l.startsWith('#')) break;
    comment.push(l.replace(/^#\s*/, ''));
  }
  let title = comment[0] || stem;
  for (let k = 1; !/[.:]$/.test(title) && k < comment.length && !comment[k].startsWith('('); k++) {
    title += ' ' + comment[k];
  }
  title = title.replace(/[.:,]$/, '');
  const results = runResults(ex.netlist, stem, () => simulateDisplay(ex.netlist, () => simulatePic(src, { includes })));
  const incPanels = Object.entries(includes)
    .map(([n, t]) => `<div class="panel src"><div class="cap">${esc(n)} — pulled in by .include</div><pre><code>${esc(t.replace(/\s+$/, ''))}</code></pre></div>`)
    .join('');
  const pills = analysesOf('\n' + ex.netlist).map((a) => `<span class="pill">.${a}</span>`).join('');
  return {
    id,
    stem,
    html:
      `<article class="ex" id="${id}"><header class="ex-head"><h3>${esc(title)}</h3>` +
      `<div class="ex-meta"><code>corpus/pic/${esc(file)}</code>${pills}<a class="anchor" href="#${id}" aria-label="link to ${esc(stem)}">#</a></div></header>` +
      `<div class="ex-grid"><div class="panel src"><div class="cap">${esc(file)}</div><pre><code>${esc(src.replace(/\s+$/, ''))}</code></pre></div>` +
      `<div class="panel canvas"><div class="cap">the figure — rendered by Dune's rpic</div><div class="fig">${figureSvg(src)}</div></div></div>` +
      `<div class="ex-grid"><div class="ex-src"><div class="panel src"><div class="cap">dune pic ${esc(file)}</div><pre><code>${esc(ex.netlist.replace(/\s+$/, ''))}</code></pre></div>${incPanels}</div>` +
      `<div class="ex-res">${results}</div></div></article>`,
  };
}

// ---------------------------------------------------------------- page

const sections = [];
const toc = [];

// Figures first: they carry the full story (source, schematic, netlist,
// simulation), so they open the page.
const picFiles = tracked('corpus/pic/*.pic')
  .map((p) => p.split('/').pop())
  .filter((f) => f !== 'spice.pic')
  .sort();
const picItems = picFiles.map(picHtml);
stats.byGroup.pic = picItems.length;
const picGroup = { id: 'pic', name: 'Figures (pic)' };
toc.push({ g: picGroup, items: picItems });
sections.push(
  `<section id="pic"><div class="wrap"><h2>Figures (pic) <span class="count">${picItems.length}</span></h2>` +
    `<p class="sub">Schematics in rpic's pic language (<code>corpus/pic/</code>): the source, the figure Dune renders from it, the netlist its extractor derives, and the simulation of that netlist. <code>corpus/pic/spice.pic</code> is the prelude itself, not a figure.</p>` +
    `${picItems.map((i) => i.html).join('')}</div></section>`,
);

groups.forEach((g, gi) => {
  const items = g.files.filter((f) => !skip.has(f)).map((f) => exampleHtml(f, g.id));
  stats.byGroup[g.id] = items.length;
  const skipped = g.files.filter((f) => skip.has(f));
  const skipNote = skipped
    .map((f) => `<p class="note"><code>corpus/${esc(f)}</code> is excluded by <code>corpus/SKIP</code>: ${esc(skip.get(f))}.</p>`)
    .join('');
  toc.push({ g, items });
  sections.push(
    `<section id="${g.id}"${gi % 2 ? '' : ' class="alt"'}><div class="wrap"><h2>${esc(g.name)} <span class="count">${items.length}</span></h2>` +
      `<p class="sub">${esc(g.blurb)}</p>${skipNote}${items.map((i) => i.html).join('')}</div></section>`,
  );
});

const indexHtml = read(join(ROOT, 'site/index.html'));
const baseCss = indexHtml.match(/<style>([\s\S]*?)<\/style>/)[1];

const extraCss = `
  :root { --trace-3: #5b9cf0; --trace-4: #e8668a; --trace-5: #a98ef0; --trace-6: #c9c24a; }
  .page-head { padding: 52px 0 18px; }
  .page-head h1 { margin-top: 6px; }
  .toc { display: grid; gap: 14px; margin: 18px 0 8px; }
  .toc-group { display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 8px; }
  .toc-group > a.gname { font-weight: 650; color: var(--fg); margin-right: 6px; min-width: 11ch; }
  .toc-group .chip { font: 12px/1 ui-monospace, Menlo, monospace; padding: 6px 8px; border: 1px solid var(--border); border-radius: 999px; background: var(--card); color: var(--muted); }
  .toc-group .chip:hover { color: var(--fg); border-color: var(--brand); text-decoration: none; }
  .count { font-size: 14px; color: var(--muted); font-weight: 500; border: 1px solid var(--border); border-radius: 999px; padding: 2px 9px; vertical-align: middle; }
  .ex { margin: 0 0 34px; padding-top: 6px; scroll-margin-top: 70px; }
  .ex-head h3 { margin: 0 0 4px; font-size: 17px; overflow-wrap: anywhere; }
  .ex-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin-bottom: 12px; color: var(--muted); font-size: 13px; }
  .ex-meta code { overflow-wrap: anywhere; }
  .ex-meta .pill { padding: 4px 8px; font-size: 11.5px; }
  .ex-meta .anchor { color: var(--muted); margin-left: 2px; }
  .ex-grid { display: grid; grid-template-columns: minmax(0, .85fr) minmax(0, 1.15fr); gap: 14px; align-items: start; margin-bottom: 14px; }
  .ex-grid > * { min-width: 0; }
  .ex-src { display: grid; gap: 14px; min-width: 0; }
  .ex-res { display: grid; gap: 12px; min-width: 0; }
  .panel.src pre { max-height: 420px; overflow: auto; font-size: 12px; }
  .scope.res { box-shadow: none; }
  .scope .cap { display: flex; flex-wrap: wrap; gap: 6px 8px; align-items: center; padding: 8px 14px; border-bottom: 1px solid var(--border); color: #b5a687; font: 12px ui-monospace, Menlo, monospace; }
  .ptag { font: 11px/1 ui-monospace, Menlo, monospace; color: #8d7f63; border: 1px solid #41331c; border-radius: 999px; padding: 3px 7px; }
  .scope .plot-grid line { stroke: #342918; } /* the scope stays dark in light mode too */
  .tr { fill: none; stroke-width: 1.6; stroke-linejoin: round; stroke-linecap: round; }
  .legend { flex-wrap: wrap; gap: 6px 14px; color: #b5a687; }
  .legend .more { color: #8d7f63; }
  .tbl { max-height: 340px; overflow: auto; }
  .tbl table { border-collapse: collapse; width: 100%; font: 12px/1.45 ui-monospace, Menlo, monospace; color: #d9cdb4; }
  .tbl th, .tbl td { text-align: right; padding: 5px 12px; border-bottom: 1px solid #2c2213; white-space: nowrap; }
  .tbl th:first-child, .tbl td:first-child { text-align: left; color: #b5a687; }
  .tbl thead th { position: sticky; top: 0; background: var(--code-bg); color: #8d7f63; font-weight: 600; }
  .notebox { background: var(--card); border: 1px solid var(--border); border-left: 3px solid var(--brand); border-radius: 10px; padding: 10px 14px; font-size: 13.5px; color: var(--muted); }
  .notebox.err { border-left-color: #e8668a; }
  .notebox b { color: var(--fg); }
  .notebox p { margin: 6px 0 0; }
  .notebox pre { margin: 8px 0 0; padding: 8px 10px; background: var(--code-bg); color: #d9cdb4; border-radius: 8px; font-size: 12px; white-space: pre-wrap; overflow-wrap: anywhere; }
  .tnote { padding: 8px 14px 0; color: #8d7f63; font: 11.5px ui-monospace, Menlo, monospace; }
  .notes { margin: 0; padding: 0 0 0 18px; color: var(--muted); font-size: 12.5px; }
  .panel.canvas { background: var(--card); }
  .canvas .fig svg { max-width: 460px; }
  /* The schematic reads like ink on paper: a fixed light sheet in both
     themes, so the hairline strokes keep their contrast on the dark page. */
  .canvas .fig { min-height: 200px; display: flex; align-items: center; justify-content: center; padding: 18px 14px; background: #f7f1e3; color: #241c0e; }
  @media (max-width: 860px) {
    .ex-grid { grid-template-columns: minmax(0, 1fr); }
    .toc-group > a.gname { min-width: 0; width: 100%; }
    .page-head { padding: 34px 0 10px; }
  }
  @media (max-width: 600px) {
    .scope .plot-axis { font-size: 14px; }
  }
`;

const navLinks = [picGroup, ...groups].map((g) => `<a href="#${g.id}">${esc(g.name.split(' ')[0])}</a>`).join('\n      ');
const tocHtml = toc
  .map(
    ({ g, items }) =>
      `<div class="toc-group"><a class="gname" href="#${g.id}">${esc(g.name)}</a>${items
        .map((i) => `<a class="chip" href="#${i.id}">${esc(i.stem)}</a>`)
        .join('')}</div>`,
  )
  .join('\n');

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Examples — Dune</title>
<meta name="description" content="Every circuit of Dune's oracle corpus — the netlist and what Dune computes for it, plus pic schematics with their figures and extracted decks." />
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'%3E%3Ctext y='.9em' font-size='90'%3E%F0%9F%8F%9C%EF%B8%8F%3C/text%3E%3C/svg%3E" />
<!-- Generated by tools/gen-examples.mjs from corpus/ with dune-sim ${esc(pkgVersion)} (WebAssembly). Do not edit by hand. -->
<style>${baseCss}${extraCss}</style>
</head>
<body>

<header class="nav">
  <div class="wrap nav-inner">
    <a class="logo" href="../">dune<b>.</b></a>
    <nav class="nav-links">
      ${navLinks}
      <a href="https://github.com/milkway/dune">GitHub</a>
    </nav>
  </div>
</header>

<main>
  <section class="page-head">
    <div class="wrap">
      <a href="../">← dune home</a>
      <h1>Examples</h1>
      <p class="lead">Every circuit in the oracle corpus — ${stats.total - picItems.length} decks — plus ${picItems.length} pic schematics: the netlist, and what Dune computes for it. Each result was produced by Dune's WebAssembly build (dune-sim ${esc(pkgVersion)}) when this page was generated; the same decks are checked point by point against ngspice-47 in CI. Transient charts are re-run with the max internal step capped at 1/${DISPLAY_PTS} of the window, so the curves carry enough timepoints to draw smoothly — the deck shown, on its own grid, is what CI compares.</p>
      <p class="note">Charts show at most ${MAX_TRACES} vectors (the deck's own <code>.plot</code>/<code>.print</code> choice when it has one), decimated to min/max per pixel column; complex plots show magnitude. Known-marginal circuits (<code>corpus/XFAIL</code>) and failures are shown as they are.</p>
      <nav class="toc" aria-label="Contents">
${tocHtml}
      </nav>
    </div>
  </section>
${sections.join('\n')}
</main>

<footer>
  <div class="wrap foot-inner">
    <div><strong>dune</strong> · BSD-3-Clause · generated by <code>tools/gen-examples.mjs</code></div>
    <div class="foot-links">
      <a href="../">Home</a>
      <a href="https://github.com/milkway/dune">Repository</a>
      <a href="https://github.com/milkway/dune/blob/main/corpus/KNOWN_DIFFS.md">Known differences</a>
    </div>
  </div>
</footer>
</body>
</html>
`;

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, html);

const kb = (Buffer.byteLength(html) / 1024).toFixed(0);
console.log(`wrote ${OUT.slice(ROOT.length + 1)} (${kb} KiB): ${stats.total} circuits, ${stats.ok} simulated`);
console.log('per group:', JSON.stringify(stats.byGroup));
for (const f of stats.failed) {
  console.log(`failed: ${f.stem} [${f.kind}] ${f.message}${f.xfail ? ' (in XFAIL)' : ' (NOT in XFAIL)'}`);
}
