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
  /** Circuit temperature, °C. */
  temp: number;
  /**
   * Continue each operating point from the previous one (off by default):
   * faster, and equal to a fresh run to the Newton tolerances rather than
   * bit for bit.
   */
  warmStart: boolean;
  readonly warmStats: { hits: number; misses: number };
  /** The extraction of a session opened with {@link Session.fromPic}. */
  readonly pic: PicExtraction | undefined;
  /** {@link simulate}'s result for the current circuit, `measurements` re-evaluated. */
  run(): Rawfile;
  /** {@link simulateTyped}'s result for the current circuit. */
  runTyped(): TypedRawfile;
  free(): void;
  [Symbol.dispose](): void;
}

/** Options of {@link renderPic}: none yet (unknown keys are an error). */
export interface RenderOptions {}

export interface SimOptions {
  /** Files the deck may `.include`, by the path its card names. */
  includes?: Record<string, string>;
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
  notes: string[];
  /** Netlist parser and `.options` warnings (file `deck.cir`). */
  diagnostics: Diagnostic[];
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
  elements: { refdes: string; shapes: { start: number; end: number } }[];
  /** Named nets (`0` first), with their wires, dots and ground symbols. */
  nets: { name: string; shapes: number[] }[];
}

export interface PicRun extends Rawfile {
  pic: PicExtraction;
}

export interface SimError extends Error {
  kind: 'options' | 'parse' | 'elaborate' | 'analysis' | 'pic' | 'alter' | 'internal';
  diagnostics: Diagnostic[];
  /** `simulatePic` failures after extraction: the extraction, netlist included. */
  pic?: PicExtraction;
}

/**
 * The pic prelude. Superseded by {@link renderPic}; kept for embedders that
 * render `prelude() + "\n" + src` with their own rpic (circuits library on;
 * before rpic 0.13, blank the `.`-lines first) so overlay ids match.
 */
export function prelude(): string;
