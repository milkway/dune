// Types of the JSON shapes dune-wasm returns (crates/dune-wasm/src/schema.rs
// is the reference; keep the two in step).

/** Initialize the wasm. Browser: no argument or the `.wasm` URL; Node: its bytes. */
export function ready(
  wasmInput?: string | URL | Request | Response | BufferSource | WebAssembly.Module,
): Promise<void>;

/** Run a SPICE3 deck. Throws a {@link SimError}. */
export function simulate(netlist: string, opts?: SimOptions | null): Rawfile;

/**
 * Extract the SPICE deck of an rpic figure marked `spice = 1`. Extraction
 * diagnostics (errors included) are in the result; throws a {@link SimError}
 * of kind `pic` only when there is no deck at all.
 */
export function extractPic(src: string, opts?: ExtractOptions | null): PicExtraction;

/**
 * Extract a figure's deck and run it. Throws kind `pic` when extraction has
 * errors; warnings ride along in `pic.diagnostics`.
 */
export function simulatePic(src: string, opts?: PicSimOptions | null): PicRun;

/**
 * `simulate` with typed-array columns: each plot's `columns` is one
 * `Float64Array` per variable — the values in a real plot (`npoints` long),
 * interleaved `re, im, …` in a complex one (`2·npoints`); NaN/±inf are kept.
 * The arrays are copies in JS-owned buffers (not views of wasm memory, which
 * growth would detach): structured-cloneable, and transferable via
 * `postMessage(run, run.plots.flatMap((p) => p.columns.map((c) => c.buffer)))`.
 */
export function simulateTyped(netlist: string, opts?: SimOptions | null): TypedRawfile;

/** `simulatePic` with typed-array columns, as in {@link simulateTyped}. */
export function simulatePicTyped(src: string, opts?: PicSimOptions | null): TypedPicRun;

/**
 * Render an rpic figure to SVG with dune's own rpic and the dune prelude —
 * the very compile {@link extractPic}'s overlay map indexes, so the
 * `<g id="sN">` groups line up with the map whatever rpic the embedder ships.
 * Supersedes rendering `prelude() + "\n" + src` yourself (and blanking the
 * `.`-lines for older rpic). Does not require `spice = 1`. Throws a
 * {@link SimError} of kind `pic` (one `compile` diagnostic) when rpic rejects
 * the figure, or `options`.
 */
export function renderPic(src: string, opts?: RenderOptions | null): string;

/**
 * A live circuit for interactive re-solves: parsed, extracted and elaborated
 * once; {@link Session.alter} changes an element value and
 * {@link Session.run} re-simulates every analysis card. With
 * {@link Session.warmStart} off (the default) a run returns exactly what
 * {@link simulate} returns for the altered deck. Call {@link Session.free}
 * (or `using`) when done.
 */
export class Session {
  private constructor();
  /** Open a SPICE3 deck. Throws a {@link SimError}. */
  static fromDeck(netlist: string, opts?: SimOptions | null): Session;
  /** Open an rpic figure; elements are altered by their refdes. */
  static fromPic(src: string, opts?: PicSimOptions | null): Session;
  /** Set the element's principal value (R, C, L, a source's DC, a gain). */
  alter(refdes: string, value: number): this;
  /**
   * Set a named parameter (ngspice names): `resistance`, `capacitance`,
   * `inductance`, `ic`, `dc`, `acmag`, `acphase` (degrees), `gain`, `w`, `l`
   * (meters), `temp` (°C), `area`; or a source's transient function
   * `pulse` | `sin` | `exp` | `pwl` | `sffm` with the card's arguments.
   * Throws a {@link SimError} of kind `alter`.
   */
  alter(refdes: string, param: string, value: number | number[] | Float64Array): this;
  /** Current value; `null` when never given (e.g. W left to `.options defw`). */
  param(refdes: string, param?: string): number | Float64Array | null;
  /** The parameter names {@link Session.alter} accepts for `refdes`. */
  alterable(refdes: string): string[];
  /** Circuit temperature, °C; wins over `options.temp` once set. */
  temp: number;
  /**
   * {@link SimOptions.options} of the session. Assigning replaces them for the
   * next runs without reopening (`null` or `{}` restore the deck's options);
   * invalid entries are warnings in each run's `diagnostics`. Throws a
   * {@link SimError} of kind `options` when not an object.
   */
  get options(): EngineOptions;
  set options(options: EngineOptions | null | undefined);
  /**
   * {@link SimOptions.tmaxDivisor} of the session (`undefined` when unset).
   * Assign a finite number ≥ 1, or `null`/`undefined` to clear; anything else
   * throws a {@link SimError} of kind `options` and changes nothing.
   */
  get tmaxDivisor(): number | undefined;
  set tmaxDivisor(d: number | null | undefined);
  /**
   * Continue each operating point from the previous one (off by default):
   * faster, and equal to a fresh run to the Newton tolerances rather than
   * bit for bit.
   */
  warmStart: boolean;
  readonly warmStats: { hits: number; misses: number };
  /** The extraction of a session opened with {@link Session.fromPic}. */
  readonly pic: PicExtraction | undefined;
  /** {@link simulate}'s result for the current circuit, `measurements` and `fourier` re-evaluated. */
  run(): Rawfile;
  /** {@link simulateTyped}'s result for the current circuit. */
  runTyped(): TypedRawfile;
  free(): void;
  [Symbol.dispose](): void;
}

/** Options of {@link renderPic}: none yet (unknown keys are an error). */
export interface RenderOptions {}

/**
 * Engine options by `.options` key (case-insensitive): `reltol`, `abstol`,
 * `vntol`, `gmin`, `itl1`, `itl2`, `itl4`, `gminsteps`, `srcsteps`, `temp` and
 * `tnom` (°C), `pivtol`, `pivrel`, `noopiter`, `trtol`, `chgtol`, `method`
 * (`"trap"` | `"gear"` | `"trbdf2"`; unset, a `.tran` uses `"trbdf2"`),
 * `compat` (`"dune"` | `"spice3"`: the SPICE3f5 defaults, i.e. a
 * trapezoidal `.tran`, switches flipping on the time grid, no cap on
 * LTRA breakpoints), `maxord`, `ltrabkrate` (unset: 10; 0: no cap), `ltrafit`,
 * `nopage`, `defl`, `defw`, `defad`, `defas`, `trytocompact`, `keepopinfo` (the small-signal
 * analyses emit their operating point first), `seed` (transient-noise seed, an
 * integer ≥ 1; no `"random"`), `notrnoise`, `devnoise` and `devnt` (device
 * noise in `.tran` and its grid step, s). A number is taken exactly; a string
 * as the card's text (`"1u"`); `true` is the bare flag, `false` clears it.
 */
export type EngineOptions = Record<string, number | string | boolean>;

export interface SimOptions {
  /** Files the deck may `.include`, by the path its card names. */
  includes?: Record<string, string>;
  /**
   * Engine options applied after the deck's `.options` cards, same keys and
   * units. An invalid entry is a warning in the result's `diagnostics`
   * (message prefixed `opts.options:`), as a bad `.options` card is — not a
   * throw.
   */
  options?: EngineOptions;
  /**
   * Divide every `.tran`'s effective maximum step (the card's TMAX, else
   * `min(TSTEP, (TSTOP-TSTART)/50)`) by this finite number ≥ 1: the run is the
   * deck's with the card's TMAX edited to TMAX/d, bit for bit. Not an
   * `options` key, because TMAX belongs to the analysis card. Out of range
   * throws a {@link SimError} of kind `options`.
   */
  tmaxDivisor?: number | null;
}

export interface ExtractOptions {
  /** Distance (pic inches) under which two terminals are one point; default 0.001. */
  snap?: number;
}

export interface PicSimOptions extends SimOptions, ExtractOptions {}

export interface Rawfile {
  plots: Plot[];
  /**
   * The deck's `.measure` cards evaluated on this run: each card against
   * every plot of its analysis, in plot order then card order. `[]` when the
   * deck has none. Same rows as Python's `Result.measurements` / R's
   * `res$measurements` (D-0029).
   */
  measurements: Measurement[];
  /**
   * The deck's `.four` cards evaluated on the run's last transient plot
   * (with the `nfreqs`, `nperiods`, `polydegree` and `fourgridsize` of its
   * `.options`): one row per harmonic of each vector, in card order then
   * vector order. `[]` when the deck has none. Same rows as Python's
   * `Result.fourier` / R's `res$fourier` (D-0061).
   */
  fourier: FourierRow[];
  notes: string[];
  /** Netlist parser and `.options` warnings (file `deck.cir`). */
  diagnostics: Diagnostic[];
  /**
   * One entry per `.pss` card, in deck order: the shooting's numbers that
   * its two plots do not carry. `[]` when the deck has none.
   */
  pss: PssInfo[];
}

/**
 * The periodic steady state of one `.pss` card — what Python's
 * `spice.PssResult` and R's `spice_pss` add to the two plots.
 */
export interface PssInfo {
  /** Index in `plots` of the card's time-domain plot; its spectrum is `plot + 1`. */
  plot: number;
  /**
   * The period `T` (s): the drive's for a driven circuit, the shooting's
   * result for an autonomous oscillator (`T` is a Newton unknown there).
   */
  period: number;
  /** Start of the reported period (s). */
  t0: number;
  /**
   * The period mismatch `max_i |x_i(t0+T) - x_i(t0)| / tol_i` of each
   * shooting iteration (the last one ≤ STEADYCOEFF); `null` for an iteration
   * whose transient failed and was backtracked (an infinite residual).
   */
  residuals: (number | null)[];
  /**
   * The period of each iteration, aligned with `residuals`: constant for a
   * driven circuit, the Newton iterate of an autonomous one.
   */
  periods: number[];
}

/**
 * One `.measure` result (ngspice-47 `.meas` semantics). A measurement that
 * does not happen is a row with `error`, never a thrown error.
 */
export interface Measurement {
  /** Result name, lowercased as on the card. */
  name: string;
  analysis: 'tran' | 'ac' | 'dc' | 'sp';
  /** Index of the measured plot in `plots`. */
  plot: number;
  /**
   * The result: a time for `WHEN` and `TRIG/TARG` (the delay), a value for
   * `FIND`, `MIN`, `AVG`…, the scale value for `MIN_AT`/`MAX_AT`. `null` when
   * the measurement failed (Python's `nan`) or the result is not finite.
   */
  value: number | null;
  /**
   * The extras ngspice prints beside it, in its order: `trig`/`targ`, `at`
   * (`MIN`/`MAX`), `with` (`MIN_AT`/`MAX_AT`), `from`/`to` (`AVG`, `RMS`,
   * `INTEG`, `PP`). `{}` for `WHEN`, `FIND`, `DERIV` and failures.
   */
  details: Partial<Record<'trig' | 'targ' | 'at' | 'with' | 'from' | 'to', number | null>>;
  /**
   * Why it failed (`out of interval (WHEN)`, an unknown vector…). Absent on
   * success (Python's `None`): a row failed exactly when it has this key.
   */
  error?: string;
}

/**
 * One harmonic of one `.four` vector (ngspice-47 `fourier` semantics). A
 * vector without a table (no transient plot, an unknown vector, a run
 * shorter than the periods) is one row with every field after `vector`
 * `null` and `error` set, never a thrown error.
 */
export interface FourierRow {
  /** Position of the card among the deck's `.four` cards (0 for the first). */
  card: number;
  /** The vector as ngspice names it: `v(out)`, `v(a,b)`, `i(v1)`. */
  vector: string;
  /** 0 for DC, then 1 to `nfreqs - 1`. */
  harmonic: number | null;
  /** `harmonic * f` (Hz). */
  frequency: number | null;
  /** The harmonic's amplitude (the signed mean for DC). */
  magnitude: number | null;
  /** Phase in degrees, referred to a sine (0 for DC). */
  phase: number | null;
  /** `magnitude` over the fundamental's (0 for DC). */
  norm_magnitude: number | null;
  /** `phase` minus the fundamental's, not wrapped (0 for DC). */
  norm_phase: number | null;
  /** The vector's total harmonic distortion (%), repeated on each of its rows. */
  thd: number | null;
  /** Points transformed (`fourgridsize * nperiods`, or the samples with `polydegree` 0). */
  gridsize: number | null;
  polydegree: number | null;
  nperiods: number | null;
  /** Why the vector has no table. Absent on success. */
  error?: string;
}

export interface Plot {
  title: string;
  date: string;
  /** `Operating Point`, `Transient Analysis`, `AC Analysis`, … */
  plotname: string;
  /** `real` | `complex`, plus ` unpadded` when not padded. */
  flags: string;
  commands: string[];
  options: string[];
  dimensions: number[] | null;
  /** The first variable is the scale. */
  variables: { name: string; type: string }[];
  npoints: number;
  /** One column per variable; complex plots hold `[re, im]` pairs; NaN/±inf are `null`. */
  columns: (number | null)[][] | [number | null, number | null][][];
}

/** A {@link Plot} whose data are typed arrays (see {@link simulateTyped}). */
export interface TypedPlot extends Omit<Plot, 'columns'> {
  /** One per variable: values (real plot) or interleaved `re, im` (complex plot). */
  columns: Float64Array[];
}

export interface TypedRawfile extends Omit<Rawfile, 'plots'> {
  plots: TypedPlot[];
}

export interface TypedPicRun extends TypedRawfile {
  pic: PicExtraction;
}

export interface Diagnostic {
  severity: 'error' | 'warning';
  /**
   * Present on pic diagnostics only: `dangling-pin`, `floating-net`,
   * `missing-value`, `duplicate-refdes`, `unknown-element`, `missing-pin`,
   * `unlabeled-element`, `node-name-conflict`, `prelude`, `compile`,
   * `not-marked`.
   */
  kind?: string;
  message: string;
  /**
   * Netlist diagnostics: the deck file (`deck.cir` or an include). Pic
   * diagnostics: `null` is the figure itself, `"spice.pic"` the dune prelude,
   * `"circuits"` rpic's std library.
   */
  file: string | null;
  /** 1-based; `null` when the diagnostic has no position. */
  line: number | null;
  column: number | null;
  /** Pic diagnostics only: exclusive end column. */
  end_column?: number | null;
}

export interface PicExtraction {
  /** SPICE3 deck, title line first, ending in `.end`. */
  netlist: string;
  diagnostics: Diagnostic[];
  map: OverlayMap;
}

/**
 * Netlist names → shapes of the figure compiled WITH the dune prelude
 * (shape N is the SVG group `<g id="sN">`).
 */
export interface OverlayMap {
  /** In card order; ids `s{start}` … `s{end-1}`. */
  elements: OverlayElement[];
  /** Named nets (`0` first), with their wires, dots and ground symbols. */
  nets: { name: string; shapes: number[] }[];
  /** The frame of {@link renderPic}'s SVG: maps pins to its user units. */
  canvas: OverlayCanvas;
}

/** One emitted element (card order). */
export interface OverlayElement {
  refdes: string;
  shapes: { start: number; end: number };
  pins: OverlayPin[];
  /**
   * Where the element's value/model argument is in the USER's source — the
   * text to replace to write a new value back (`1k`, `DC 5 AC 1`,
   * `SIN(0 1 1k)`; a quoted argument gives the range inside the quotes).
   * Unchanged by a display text (the argument after the value). `null` when
   * there is no span to trust: no value argument, a `K` (`coupling`), an
   * inline block, or an element whose outermost call is a user wrapper macro
   * (its value comes from the drawing or from a wrapper argument).
   */
  value: OverlayValueSpan | null;
  /**
   * The alterable parameters spelled inside `value`, in source order, named
   * as `Session.alter(refdes, param, …)` takes them:
   * - `V`/`I`: `dc`, `acmag`, `acphase` — the number only (`AC 1 90` →
   *   `acmag` = `1`, `acphase` = `90`); `pulse`, `sin`, `exp`, `pwl`, `sffm`,
   *   `trnoise`, `trrandom` — the whole spelling, keyword and parentheses
   *   included (`PULSE(0 5 0 1u 1u 10m 20m)`);
   * - `R`/`C`/`L`/`E`/`G`: the principal (leading number); `ic` (`IC=`, C/L);
   * - `F`/`H`: `gain` (`Vname gain`); `D`/`Q`/`J`: `area`, `temp`;
   *   `M`: `w`, `l`, `temp` (`W=…`).
   * Only what is spelled is listed (no `DC` field → no `dc` entry); empty when
   * `value` is `null`.
   */
  params: OverlayParamSpan[];
  /**
   * The parameter `Session.alter(refdes, value)` changes: `resistance`,
   * `capacitance`, `inductance`, `dc` (V/I), `gain` (E/F/G/H), `area`
   * (D/Q/J); `null` for letters without one (M, S, W, X, T, O, K).
   */
  principal: string | null;
}

/**
 * A range of the user's figure source: 1-based, end exclusive, like the
 * diagnostics. Columns count Unicode **code points**, not UTF-16 units:
 * index a line with `Array.from(line)` (or a code-point walk), not
 * `line[i]`, when it may hold characters outside the BMP. Lines split at `\n`.
 * A value split by `\` continuations spans lines (`end_line > line`).
 */
export interface SourceRange {
  line: number;
  col: number;
  end_line: number;
  end_col: number;
}

export interface OverlayValueSpan {
  span: SourceRange;
  /** The source text in `span`, verbatim (`\` continuations kept). */
  text: string;
}

export interface OverlayParamSpan {
  /** `alter` parameter name: `dc`, `acmag`, `acphase`, `pulse`, `resistance`, … */
  name: string;
  span: SourceRange;
  text: string;
}

/**
 * The page frame of {@link renderPic}'s SVG (rpic's scene `canvas`, at full
 * precision). A pin `(x, y)` (pic inches, y up) is at SVG user units
 *
 *     sx = (x + origin[0]) * 96
 *     sy = (h - (y + origin[1])) * 96
 *
 * exactly; the `viewBox` is `0 0 w*96 h*96`. Note `+ origin`, and that
 * `rescale` does not enter: pins are already past the whole-picture rescale.
 */
export interface OverlayCanvas {
  /** Page width and height in inches, margins and stroke padding included. */
  w: number;
  h: number;
  /** The pic origin, in inches from the page's lower-left corner. */
  origin: [number, number];
  /** Whole-picture rescale (`.PS <width>`, `maxpswid`/`maxpsht`); 1 if none. Informational. */
  rescale: number;
}

/**
 * One terminal of an element, in card order. Pins are drawn inside the
 * element, so a net whose elements touch pin to pin has no `shapes` of
 * its own: probe it at its pins.
 */
export interface OverlayPin {
  /** Prelude pin name: `P`, `N`, `C`, `B`, `E`, `D`, `G`, `S`, `Inp`, `Cp`, `P1`, … */
  name: string;
  /** The net it is on (a `name` of {@link OverlayMap.nets}). */
  net: string;
  /** Terminal point in pic inches, the frame of rpic's scene (y up); {@link OverlayMap.canvas} maps it to the SVG. */
  x: number;
  y: number;
}

export interface PicRun extends Rawfile {
  pic: PicExtraction;
}

export interface SimError extends Error {
  /**
   * `pss`: a `.pss` card whose shooting did not converge in its iterations
   * (the history is in `iterations`, `residuals` and `period`); every other
   * engine failure is `analysis`.
   */
  kind: 'options' | 'parse' | 'elaborate' | 'analysis' | 'pss' | 'pic' | 'alter' | 'internal';
  diagnostics: Diagnostic[];
  /** `simulatePic` failures after extraction: the extraction, netlist included. */
  pic?: PicExtraction;
  /** Kind `pss` only: the shooting iterations run. */
  iterations?: number;
  /**
   * Kind `pss` only: the mismatch of each iteration in units of the
   * tolerance, as in {@link PssInfo.residuals} (`null` for an infinite one).
   */
  residuals?: (number | null)[];
  /**
   * Kind `pss` only: the last period iterate of an autonomous oscillator;
   * `null` for a driven circuit, whose period is fixed.
   */
  period?: number | null;
}

/**
 * The pic prelude. Superseded by {@link renderPic}; kept for embedders that
 * render `prelude() + "\n" + src` with their own rpic (circuits library on;
 * before rpic 0.13, blank the `.`-lines first) so overlay ids match.
 */
export function prelude(): string;
