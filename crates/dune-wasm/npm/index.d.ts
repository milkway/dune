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
  notes: string[];
  /** Netlist parser and `.options` warnings (file `deck.cir`). */
  diagnostics: Diagnostic[];
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
  kind: 'options' | 'parse' | 'elaborate' | 'analysis' | 'pic' | 'internal';
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
