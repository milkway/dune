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

/** The pic prelude; render SVG from `prelude() + "\n" + src` so overlay ids match. */
export function prelude(): string;
