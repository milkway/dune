//! SPICE netlist extraction from rpic figures (PLAN §2.5; D-0003,
//! D-0004, D-0019, D-0020).
//!
//! A figure is ordinary pic, compiled by [`rpic_core`] as a library,
//! with the `spice.pic` prelude (embedded; `corpus/pic/spice.pic`)
//! loaded after rpic's std circuit library. The prelude wraps every
//! D-0019 element in a labeled block:
//!
//! ```text
//! .PS
//! spice = 1
//! In: (0,1); Out: (2,1)
//! V1: vsource(In, (0,0), DC 5)
//! R1: resistor(In, Out, 1k)
//! C1: capacitor(Out, (2,0), 1u)
//! line from C1.N to V1.N; ground(V1.N)
//! .tran 10u 5m
//! .PE
//! ```
//!
//! extracts to `V1 In 0 DC 5`, `R1 In Out 1k`, `C1 Out 0 1u`,
//! `.tran 10u 5m`, `.end`.
//!
//! Conventions (the contract between the prelude and this crate):
//!
//! * **refdes = block label**; its first letter picks the card type
//!   (D-0004 §1), restricted to the D-0019 set `R C L V I D Q M J S W X`
//!   plus the transmission lines `T` (ideal) and `O` (LTRA), the
//!   controlled sources `E F G H` and the mutual inductance `K`
//!   (D-0026);
//! * **pins** are `circle invis rad 0` shapes labeled `P`/`N`,
//!   `C`/`B`/`E` (+ the optional substrate `S`), `D`/`G`/`S`/`B`,
//!   `Inp`/`Inm`/`Out`, `Cp`/`Cn`, `P1`/`N1`/`P2`/`N2` that
//!   are direct children of the element block (D-0020 §c); a `K`
//!   block has no pins, only a `Mutual` marker;
//! * **value/model = the macro argument after the pin positions**
//!   (D-0004 §2: the 3rd of a two-terminal, the 5th of the four-point
//!   `E`/`G`, the 2nd of an element centered on an `At` anchor; all
//!   three arguments of a `K`, `L1 L2 k`), read verbatim from the call
//!   in the user's source (`1e-9` stays `1e-9`). The prelude also draws it
//!   (the first text shape in the block); that drawing is presentation,
//!   read only as a fallback when the block has no usable call (an
//!   inline block, or a wrapper macro whose arguments are not the
//!   element's) and, normalized by pic, to check that the call's
//!   argument really is the element's value;
//! * **wires** are the visible `line`/`arrow` paths outside element
//!   and ground blocks; **dots** are explicit junctions; connectivity
//!   rules are in [`connect`];
//! * **nets** (D-0004 §3): a `ground(P)` block makes node `0`; a named
//!   place (`Vout: (2,1)`, `Vin: R1.P`) that coincides with a net
//!   names it — places that share a label are one SPICE node; the rest
//!   are `n1, n2, …` in order of first appearance (lowest shape index);
//! * **directives** (D-0004 §4): column-1 `.` lines (rpic's
//!   `Drawing.passthrough`, the troff rule), with their column-1 `+`
//!   continuations, pass through to the netlist in source order; a user
//!   `.end` is dropped and one is appended.
//!
//! The figure must set `spice = 1` (D-0004 §5) or extraction refuses.

#![forbid(unsafe_code)]

mod connect;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::ops::Range;

use rpic_core::ir::{Drawing, MacroCall, Shape};
use rpic_core::{CompileOptions, IncludePolicy};

use crate::connect::{Pt, UnionFind, join_geometry};

/// The embedded dune prelude (`corpus/pic/spice.pic`).
pub const PRELUDE: &str = include_str!("../../../corpus/pic/spice.pic");

/// Default snap tolerance, in pic inches (PLAN §2.5).
pub const DEFAULT_SNAP: f64 = 1e-3;

/// Marker label of the prelude's `ground(P)` block.
const GROUND_MARKER: &str = "Gnd";
/// Marker label of the anchor of centered elements, whose value is the
/// 2nd macro argument instead of the 3rd.
const ANCHOR_MARKER: &str = "At";
/// Marker of the prelude's `coupling(L1, L2, k)` block (a `K` card,
/// which has no pins).
const COUPLING_MARKER: &str = "Mutual";
/// Every pin-marker label the prelude uses on elements, plus the
/// `At` anchor of centered elements and the coupling marker.
const ELEMENT_MARKERS: &[&str] = &[
    "P", "N", "Cp", "Cn", "C", "B", "E", "D", "G", "S", "Inp", "Inm", "Out", "P1", "N1", "P2",
    "N2", "At", "Mutual",
];
/// Pins of a `Q` drawn with its substrate (`npn4`/`pnp4`).
const BJT4_PINS: &[&str] = &["C", "B", "E", "S"];
/// Marker of a block whose drawn text is a display, not the value.
const DISPLAY_MARKER: &str = "Disp";
/// Prelude macros that take an optional display text after the
/// value/model, drawn instead of it (two-terminals, `vcvs`/`vccs` and
/// the elements centered on an `At` anchor).
const DISPLAY_MACROS: &[&str] = &[
    "resistor",
    "capacitor",
    "inductor",
    "diode",
    "vsource",
    "acsource",
    "battery",
    "isource",
    "cccs",
    "ccvs",
    "vcvs",
    "vccs",
    "npn",
    "pnp",
    "npn4",
    "pnp4",
    "nmos",
    "pmos",
    "nmos4",
    "pmos4",
    "njfet",
    "pjfet",
    "opamp",
];
/// The supported card letters, for messages.
const LETTERS: &str = "R C L V I D Q M J S W X T O E F G H K";

/// Options for [`extract`].
#[derive(Debug, Clone, Copy)]
pub struct ExtractOptions<'a> {
    /// Distance (pic inches) under which two terminals are the same
    /// point. Default [`DEFAULT_SNAP`].
    pub snap: f64,
    /// Prelude loaded after rpic's std circuit library; `None` uses the
    /// embedded [`PRELUDE`].
    pub prelude: Option<&'a str>,
}

impl Default for ExtractOptions<'_> {
    fn default() -> Self {
        Self {
            snap: DEFAULT_SNAP,
            prelude: None,
        }
    }
}

/// A position in the figure source (1-based line/column, end
/// exclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PicSpan {
    /// `None` = the user's figure; `Some("spice.pic")` = the prelude;
    /// `Some("circuits")` = rpic's std library.
    pub file: Option<String>,
    pub line: u32,
    pub col: u32,
    pub end_col: u32,
}

impl fmt::Display for PicSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}:")?;
        }
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PicSeverity {
    /// The netlist is emitted but is wrong or incomplete (an element
    /// was skipped or lacks its value).
    Error,
    /// The netlist is emitted and probably what was meant.
    Warning,
}

/// What a [`PicDiagnostic`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PicDiagKind {
    /// An element pin no other element pin (or ground) connects to.
    DanglingPin,
    /// Wires (or a named node) that reach no element pin.
    FloatingNet,
    /// An element block with no (or an empty) value text.
    MissingValue,
    /// Two elements with the same refdes (SPICE names are
    /// case-insensitive).
    DuplicateRefdes,
    /// A refdes whose first letter is outside the supported set.
    UnknownElement,
    /// An element block without one of the pins its letter needs.
    MissingPin,
    /// An element block without a label.
    UnlabeledElement,
    /// One net carrying two different names (or a name and ground).
    NodeNameConflict,
    /// Something in the source that defeats the prelude.
    Prelude,
}

/// A structured extraction diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub struct PicDiagnostic {
    pub severity: PicSeverity,
    pub kind: PicDiagKind,
    pub message: String,
    /// Where in the figure, when known.
    pub span: Option<PicSpan>,
}

impl fmt::Display for PicDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sev = match self.severity {
            PicSeverity::Error => "error",
            PicSeverity::Warning => "warning",
        };
        match &self.span {
            Some(s) => write!(f, "{s}: {sev}: {}", self.message),
            None => write!(f, "{sev}: {}", self.message),
        }
    }
}

/// Why no netlist could be produced at all.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PicError {
    /// rpic rejected the figure.
    #[error("pic: {message}")]
    Compile {
        message: String,
        span: Option<PicSpan>,
    },
    /// The figure does not opt in with `spice = 1` (D-0004 §5).
    #[error("figure not marked `spice = 1`; dune-pic only extracts figures that opt in")]
    NotMarked,
}

/// One element's shapes in the compiled drawing (ids `sN` of the SVG).
#[derive(Debug, Clone, PartialEq)]
pub struct ElementOverlay {
    pub refdes: String,
    /// Half-open range of shape indices of the element's block.
    pub shapes: Range<usize>,
    /// The element's terminals, in card order (the nodes of its card);
    /// empty for a `K`, which has none.
    pub pins: Vec<PinOverlay>,
}

/// One terminal of an element: where it is drawn and the net it is on.
///
/// Element pins are invisible markers inside the element's own shapes,
/// so a net whose elements touch pin to pin has no wire of its own in
/// [`NetOverlay::shapes`]; the pins still give a probe target for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PinOverlay {
    /// The pin's prelude name (`P`, `N`, `C`, `B`, `E`, `D`, `G`, `S`,
    /// `Inp`, `Cp`, `P1`, …; D-0020).
    pub name: String,
    /// The node name on the card, as in [`NetOverlay::name`].
    pub net: String,
    /// The terminal point, in pic inches, in the frame of the compiled
    /// drawing's shapes (y up, after the whole-picture rescale) — the
    /// frame of rpic's scene, which maps it to SVG pixels.
    pub x: f64,
    pub y: f64,
}

impl ElementOverlay {
    /// The range as rpic object ids, `"sN..sM"` (inclusive).
    pub fn ids(&self) -> String {
        format!(
            "s{}..s{}",
            self.shapes.start,
            self.shapes.end.saturating_sub(1)
        )
    }
}

/// One net's drawn shapes: its wires and dots, and the ground blocks
/// on it (named places draw nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct NetOverlay {
    pub name: String,
    pub shapes: Vec<usize>,
}

/// Map from netlist names back to rpic objects, for result overlays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OverlayMap {
    /// Emitted elements, in card order.
    pub elements: Vec<ElementOverlay>,
    /// Named nets, in order of first appearance (`0` first if present).
    pub nets: Vec<NetOverlay>,
}

/// The result of [`extract`].
#[derive(Debug, Clone, PartialEq)]
pub struct Extraction {
    /// SPICE3 netlist text, ending in `.end`.
    pub netlist: String,
    pub map: OverlayMap,
    pub diagnostics: Vec<PicDiagnostic>,
}

impl Extraction {
    /// True when no diagnostic is an error.
    pub fn is_clean(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|d| d.severity != PicSeverity::Error)
    }
}

/// Compile a figure with the prelude and render it to SVG — the
/// drawing whose shape ids [`OverlayMap`] refers to. Does not require
/// `spice = 1`.
pub fn render_svg(src: &str, opts: &ExtractOptions) -> Result<String, PicError> {
    let c = compile(src, opts)?;
    Ok(rpic_core::to_svg(&c.drawing))
}

/// Extract a SPICE3 netlist from a pic figure.
pub fn extract(src: &str, opts: &ExtractOptions) -> Result<Extraction, PicError> {
    let c = compile(src, opts)?;
    let marked = c
        .drawing
        .vars
        .iter()
        .any(|(k, v)| k == "spice" && *v == 1.0);
    if !marked {
        return Err(PicError::NotMarked);
    }
    let snap = if opts.snap.is_finite() && opts.snap > 0.0 {
        opts.snap
    } else {
        DEFAULT_SNAP
    };
    Ok(Extractor::new(src, &c, snap).run())
}

/// A compiled figure.
struct Compiled {
    drawing: Drawing,
    /// The user's directives in source order: each passthrough entry
    /// with its `+` continuations, one per line.
    directives: Vec<String>,
    /// Lines the prelude occupies ahead of the user source.
    prelude_lines: u32,
}

fn compile(src: &str, opts: &ExtractOptions) -> Result<Compiled, PicError> {
    let prelude = opts.prelude.unwrap_or(PRELUDE);
    let mut full = String::with_capacity(prelude.len() + src.len() + 1);
    full.push_str(prelude);
    if !prelude.ends_with('\n') {
        full.push('\n');
    }
    let prelude_lines = full.matches('\n').count() as u32;
    full.push_str(src);
    let copts = CompileOptions {
        circuits: true,
        includes: IncludePolicy::Deny,
        ..Default::default()
    };
    let drawing = rpic_core::compile_with_diagnostics(&full, &copts).map_err(|e| {
        let info = &e.info;
        let span = info.line.map(|line| {
            map_span(
                prelude_lines,
                info.file.as_deref(),
                line,
                info.col.unwrap_or(1),
                info.end_col.unwrap_or(info.col.unwrap_or(1)),
            )
        });
        PicError::Compile {
            message: info.message.clone(),
            span,
        }
    })?;
    // Directives: the user's passthrough entries (a prelude has none of
    // its own to contribute). rpic folds the column-1 `+` continuations
    // of a request into its entry, one line per `\n`.
    let directives = drawing
        .passthrough
        .iter()
        .filter(|l| l.span.file.is_none() && l.span.line > prelude_lines)
        .map(|l| directive_text(&l.text))
        .collect();
    Ok(Compiled {
        drawing,
        directives,
        prelude_lines,
    })
}

/// A passthrough entry as a netlist line: the request trimmed at the
/// end, each `+` continuation line trimmed.
fn directive_text(text: &str) -> String {
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or("").trim_end().to_string();
    for cont in lines {
        out.push('\n');
        out.push_str(cont.trim());
    }
    out
}

/// Map a position in the prelude+user stream back to its source.
fn map_span(prelude_lines: u32, file: Option<&str>, line: u32, col: u32, end_col: u32) -> PicSpan {
    match file {
        Some(f) => PicSpan {
            file: Some(f.to_string()),
            line,
            col,
            end_col,
        },
        None if line > prelude_lines => PicSpan {
            file: None,
            line: line - prelude_lines,
            col,
            end_col,
        },
        None => PicSpan {
            file: Some("spice.pic".into()),
            line,
            col,
            end_col,
        },
    }
}

/// Pins of each supported card letter (D-0019 + `T`/`O` + D-0026), in
/// card order. A `Q` may also carry the substrate ([`BJT4_PINS`]); a
/// `K` has none.
fn pins_for(letter: char) -> Option<&'static [&'static str]> {
    Some(match letter {
        'R' | 'C' | 'L' | 'V' | 'I' | 'D' | 'W' | 'F' | 'H' => &["P", "N"],
        'S' | 'E' | 'G' => &["P", "N", "Cp", "Cn"],
        'K' => &[],
        'Q' => &["C", "B", "E"],
        'M' => &["D", "G", "S", "B"],
        'J' => &["D", "G", "S"],
        'X' => &["Inp", "Inm", "Out"],
        'T' | 'O' => &["P1", "N1", "P2", "N2"],
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum BlockKind {
    Plain,
    Element,
    Ground,
}

/// What a terminal is.
#[derive(Clone, Copy)]
enum Term {
    /// Pin `pin` (index into the element's pin list) of element `elem`.
    Pin {
        elem: usize,
        pin: usize,
    },
    Ground,
    /// A named place (index into the extractor's place list).
    Place(usize),
    /// A vertex of a wire.
    Wire,
    Dot,
}

struct Terminal {
    at: Pt,
    /// The shape the terminal belongs to; [`NO_SHAPE`] for places,
    /// which draw nothing (so they never decide net order).
    shape: usize,
    term: Term,
}

struct Element {
    refdes: String,
    pins: &'static [&'static str],
    /// Terminal index of each pin, in card order.
    terms: Vec<usize>,
    value: String,
    block: usize,
    /// The element's call site in the user source.
    span: Option<PicSpan>,
}

/// [`Terminal::shape`] of a named place.
const NO_SHAPE: usize = usize::MAX;

/// A named place that takes part in net naming.
struct Place {
    label: String,
    span: Option<PicSpan>,
}

struct Extractor<'a> {
    src: &'a str,
    d: &'a Drawing,
    directives: &'a [String],
    prelude_lines: u32,
    snap: f64,
    diags: Vec<PicDiagnostic>,
    places: Vec<Place>,
}

impl<'a> Extractor<'a> {
    fn new(src: &'a str, c: &'a Compiled, snap: f64) -> Self {
        Self {
            src,
            d: &c.drawing,
            directives: &c.directives,
            prelude_lines: c.prelude_lines,
            snap,
            diags: Vec::new(),
            places: Vec::new(),
        }
    }

    fn diag(
        &mut self,
        severity: PicSeverity,
        kind: PicDiagKind,
        message: String,
        span: Option<PicSpan>,
    ) {
        self.diags.push(PicDiagnostic {
            severity,
            kind,
            message,
            span,
        });
    }

    fn span(&self, s: &rpic_core::Span) -> PicSpan {
        map_span(
            self.prelude_lines,
            s.file.as_deref(),
            s.line,
            s.col,
            s.end_col,
        )
    }

    fn shape_span(&self, i: usize) -> Option<PicSpan> {
        let s = self.d.shape_spans.get(i)?.as_ref()?;
        Some(self.span(s))
    }

    /// The label and center of shape `i` if it is a labeled marker
    /// (`circle invis rad 0`).
    fn marker(&self, i: usize) -> Option<(&'a str, Pt)> {
        let d = self.d;
        let label = d.shape_labels.get(i)?.as_deref()?;
        match &d.shapes[i] {
            Shape::Circle {
                c, r, dot, style, ..
            } if *r == 0.0 && !*dot && style.invis => Some((label, (c.x, c.y))),
            _ => None,
        }
    }

    /// Direct-child markers of block `b`.
    fn markers_of(&self, b: usize) -> Vec<(usize, &'a str, Pt)> {
        let (lo, hi) = self.d.blocks[b].shapes;
        (lo..hi.min(self.d.shapes.len()))
            .filter(|&i| self.d.shape_blocks.get(i).copied().flatten() == Some(b))
            .filter_map(|i| self.marker(i).map(|(l, p)| (i, l, p)))
            .collect()
    }

    fn run(mut self) -> Extraction {
        let d = self.d;
        if self.src.lines().any(|l| {
            let t = l.trim();
            t.starts_with("copy") && t.contains("\"circuits\"")
        }) {
            self.diag(
                PicSeverity::Warning,
                PicDiagKind::Prelude,
                "`copy \"circuits\"` reloads rpic's std library over the dune prelude, \
                 turning elements back into plain drawings; dune-pic loads the library itself"
                    .into(),
                None,
            );
        }

        // 1. Classify blocks.
        let kinds: Vec<BlockKind> = (0..d.blocks.len())
            .map(|b| {
                let ms = self.markers_of(b);
                if ms.iter().any(|m| m.1 == GROUND_MARKER) {
                    BlockKind::Ground
                } else if ms.iter().any(|m| ELEMENT_MARKERS.contains(&m.1)) {
                    BlockKind::Element
                } else {
                    BlockKind::Plain
                }
            })
            .collect();
        // Inside a special block (at any depth)?
        let in_special = |mut b: Option<usize>| {
            while let Some(bi) = b {
                if kinds[bi] != BlockKind::Plain {
                    return true;
                }
                b = d.blocks[bi].parent;
            }
            false
        };
        // A shape is internal when any enclosing block is special.
        let internal = |i: usize| in_special(d.shape_blocks.get(i).copied().flatten());

        let mut terms: Vec<Terminal> = Vec::new();
        let mut elements: Vec<Element> = Vec::new();

        // 2. Elements and grounds, in source order.
        let mut order: Vec<usize> = (0..d.blocks.len()).collect();
        order.sort_by_key(|&b| d.blocks[b].shapes.0);
        let mut seen: HashMap<String, String> = HashMap::new();
        for &b in &order {
            let ms = self.markers_of(b);
            match kinds[b] {
                BlockKind::Plain => {}
                BlockKind::Ground => {
                    for &(i, l, p) in &ms {
                        if l == GROUND_MARKER {
                            terms.push(Terminal {
                                at: p,
                                shape: i,
                                term: Term::Ground,
                            });
                        }
                    }
                }
                BlockKind::Element => {
                    if let Some(e) = self.element(b, &ms, &mut terms, &mut seen, elements.len()) {
                        elements.push(e);
                    }
                }
            }
        }

        self.check_references(&elements);

        // Named places (D-0004 §3): the user's `Label: position`
        // statements, not those of the prelude, the std library or the
        // inside of an element.
        for p in &d.places {
            let span = p.span.as_ref().map(|s| self.span(s));
            if span.as_ref().is_some_and(|s| s.file.is_some()) || in_special(p.block) {
                continue;
            }
            if let Some(refdes) = seen.get(&p.label.to_ascii_lowercase()) {
                self.diag(
                    PicSeverity::Warning,
                    PicDiagKind::NodeNameConflict,
                    format!(
                        "place `{}` has the name of element `{refdes}`; it names no net",
                        p.label
                    ),
                    span,
                );
                continue;
            }
            terms.push(Terminal {
                at: (p.at.x, p.at.y),
                shape: NO_SHAPE,
                term: Term::Place(self.places.len()),
            });
            self.places.push(Place {
                label: p.label.clone(),
                span,
            });
        }

        // 3. Wires and dots.
        let mut segments: Vec<(Pt, Pt, usize)> = Vec::new();
        for (i, s) in d.shapes.iter().enumerate() {
            if internal(i) {
                continue;
            }
            match s {
                Shape::Path { pts, style, .. } if !style.invis && !pts.is_empty() => {
                    let first = terms.len();
                    for p in pts {
                        terms.push(Terminal {
                            at: (p.x, p.y),
                            shape: i,
                            term: Term::Wire,
                        });
                    }
                    for w in pts.windows(2) {
                        segments.push(((w[0].x, w[0].y), (w[1].x, w[1].y), first));
                    }
                }
                Shape::Circle { c, dot: true, .. } => terms.push(Terminal {
                    at: (c.x, c.y),
                    shape: i,
                    term: Term::Dot,
                }),
                _ => {}
            }
        }

        // 4. Union-find.
        let mut uf = UnionFind::new(terms.len());
        for w in terms.windows(2).enumerate() {
            let (k, pair) = w;
            if let (Term::Wire, Term::Wire) = (pair[0].term, pair[1].term)
                && pair[0].shape == pair[1].shape
            {
                uf.union(k, k + 1);
            }
        }
        let points: Vec<Pt> = terms.iter().map(|t| t.at).collect();
        join_geometry(&mut uf, &points, &segments, self.snap);
        // Nets that share a place name are one node (net labels
        // connect).
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for (k, t) in terms.iter().enumerate() {
            if let Term::Place(n) = t.term {
                let key = self.places[n].label.to_ascii_lowercase();
                match by_name.get(&key) {
                    Some(&other) => uf.union(k, other),
                    None => {
                        by_name.insert(key, k);
                    }
                }
            }
        }

        // 5. Gather nets.
        let roots: Vec<usize> = (0..terms.len()).map(|k| uf.find(k)).collect();
        let mut nets: BTreeMap<usize, Vec<usize>> = BTreeMap::new(); // root -> terms
        for (k, &r) in roots.iter().enumerate() {
            nets.entry(r).or_default().push(k);
        }

        // 6. Name nets.
        let user_names: Vec<String> = self
            .places
            .iter()
            .map(|p| p.label.to_ascii_lowercase())
            .collect();
        let mut name_of: HashMap<usize, String> = HashMap::new();
        // Nets in order of first appearance.
        let mut net_order: Vec<(usize, usize)> = nets
            .iter()
            .map(|(&r, ks)| (ks.iter().map(|&k| terms[k].shape).min().unwrap_or(0), r))
            .collect();
        net_order.sort();
        for &(_, r) in &net_order {
            let ks = &nets[&r];
            let grounded = ks.iter().any(|&k| matches!(terms[k].term, Term::Ground));
            // Place names in definition order.
            let mut names: Vec<usize> = ks
                .iter()
                .filter_map(|&k| match terms[k].term {
                    Term::Place(n) => Some(n),
                    _ => None,
                })
                .collect();
            names.sort_unstable();
            let mut distinct: Vec<String> = Vec::new();
            for n in names {
                let l = &self.places[n].label;
                if !distinct.iter().any(|m| m.eq_ignore_ascii_case(l)) {
                    distinct.push(l.clone());
                }
            }
            if grounded {
                if let Some(n) = distinct.first() {
                    let span = self.label_span(n);
                    self.diag(
                        PicSeverity::Warning,
                        PicDiagKind::NodeNameConflict,
                        format!("place `{n}` is on the ground net; it is named `0`"),
                        span,
                    );
                }
                name_of.insert(r, "0".into());
            } else if let Some(first) = distinct.first() {
                if distinct.len() > 1 {
                    let span = self.label_span(&distinct[1]);
                    self.diag(
                        PicSeverity::Warning,
                        PicDiagKind::NodeNameConflict,
                        format!(
                            "one net named {}; using `{first}`",
                            distinct
                                .iter()
                                .map(|n| format!("`{n}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        span,
                    );
                }
                name_of.insert(r, first.clone());
            }
        }
        let mut auto = 0usize;
        for &(_, r) in &net_order {
            if name_of.contains_key(&r) {
                continue;
            }
            let has_pin = nets[&r]
                .iter()
                .any(|&k| matches!(terms[k].term, Term::Pin { .. }));
            if !has_pin {
                continue;
            }
            let name = loop {
                auto += 1;
                let n = format!("n{auto}");
                if !user_names.contains(&n) {
                    break n;
                }
            };
            name_of.insert(r, name);
        }

        // 7. Electrical checks.
        for &(_, r) in &net_order {
            let ks = &nets[&r];
            let pins: Vec<usize> = ks
                .iter()
                .copied()
                .filter(|&k| matches!(terms[k].term, Term::Pin { .. }))
                .collect();
            let grounded = ks.iter().any(|&k| matches!(terms[k].term, Term::Ground));
            if pins.is_empty() {
                let drawn = ks
                    .iter()
                    .copied()
                    .find(|&k| matches!(terms[k].term, Term::Wire | Term::Dot));
                if let Some(k) = drawn {
                    let what = match name_of.get(&r) {
                        Some(n) => format!("net `{n}`"),
                        None => "wire".into(),
                    };
                    let span = self.shape_span(terms[k].shape).filter(|s| s.file.is_none());
                    self.diag(
                        PicSeverity::Warning,
                        PicDiagKind::FloatingNet,
                        format!("{what} connects to no element pin"),
                        span,
                    );
                } else if let Some(n) = ks.iter().find_map(|&k| match terms[k].term {
                    Term::Place(n) => Some(n),
                    _ => None,
                }) {
                    // Only places: a label on empty space.
                    let p = &self.places[n];
                    let msg = format!("place `{}` names no net: no wire or pin is there", p.label);
                    let span = p.span.clone();
                    self.diag(PicSeverity::Warning, PicDiagKind::FloatingNet, msg, span);
                }
                continue;
            }
            if pins.len() == 1 && !grounded {
                let Term::Pin { elem, pin } = terms[pins[0]].term else {
                    unreachable!()
                };
                let e = &elements[elem];
                let span = e.span.clone();
                let net = name_of.get(&r).cloned().unwrap_or_default();
                let msg = format!(
                    "pin {}.{} is dangling: net `{net}` reaches no other element pin",
                    e.refdes, e.pins[pin]
                );
                self.diag(PicSeverity::Warning, PicDiagKind::DanglingPin, msg, span);
            }
        }

        // A Q card's 4th name is the substrate only when it is not a
        // model name (INP2Q looks it up in the model table).
        for e in &elements {
            if e.pins.len() == 4 && e.refdes.to_ascii_uppercase().starts_with('Q') {
                let sub = &name_of[&roots[e.terms[3]]];
                let model = e.value.split_whitespace().next().unwrap_or("");
                if sub.eq_ignore_ascii_case(model) {
                    let msg = format!(
                        "`{}`: substrate net `{sub}` has the name of the model; \
                         SPICE would read it as the model and the next word as the model name",
                        e.refdes
                    );
                    let span = e.span.clone();
                    self.diag(
                        PicSeverity::Warning,
                        PicDiagKind::NodeNameConflict,
                        msg,
                        span,
                    );
                }
            }
        }

        // 8. Emit.
        let mut netlist = String::from("* netlist extracted by dune-pic\n");
        let mut map = OverlayMap::default();
        for e in &elements {
            netlist.push_str(&e.refdes);
            for &k in &e.terms {
                netlist.push(' ');
                netlist.push_str(&name_of[&roots[k]]);
            }
            if !e.value.is_empty() {
                netlist.push(' ');
                netlist.push_str(&e.value);
            }
            netlist.push('\n');
            let (lo, hi) = d.blocks[e.block].shapes;
            let pins = e
                .pins
                .iter()
                .zip(&e.terms)
                .map(|(&name, &k)| PinOverlay {
                    name: name.to_string(),
                    net: name_of[&roots[k]].clone(),
                    x: terms[k].at.0,
                    y: terms[k].at.1,
                })
                .collect();
            map.elements.push(ElementOverlay {
                refdes: e.refdes.clone(),
                shapes: lo..hi,
                pins,
            });
        }
        for dir in self.directives {
            if dir.eq_ignore_ascii_case(".end") {
                continue;
            }
            netlist.push_str(dir);
            netlist.push('\n');
        }
        netlist.push_str(".end\n");

        // Overlay of nets: wires, dots and ground blocks.
        let mut net_shapes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (k, t) in terms.iter().enumerate() {
            match t.term {
                Term::Wire | Term::Dot => net_shapes.entry(roots[k]).or_default().push(t.shape),
                Term::Ground => {
                    let b = d.shape_blocks[t.shape].expect("marker inside its block");
                    let (lo, hi) = d.blocks[b].shapes;
                    net_shapes.entry(roots[k]).or_default().extend(lo..hi);
                }
                Term::Pin { .. } | Term::Place(_) => {}
            }
        }
        let mut emitted: Vec<String> = Vec::new();
        let mut ordered: Vec<(usize, usize)> = net_order.clone();
        ordered.sort_by_key(|&(first, r)| (name_of.get(&r).map(|n| n != "0"), first));
        for (_, r) in ordered {
            let Some(name) = name_of.get(&r) else {
                continue;
            };
            let mut shapes = net_shapes.remove(&r).unwrap_or_default();
            shapes.sort_unstable();
            shapes.dedup();
            // Nets merged by name share one entry.
            if let Some(pos) = emitted.iter().position(|n| n == name) {
                map.nets[pos].shapes.extend(shapes);
                map.nets[pos].shapes.sort_unstable();
                map.nets[pos].shapes.dedup();
                continue;
            }
            emitted.push(name.clone());
            map.nets.push(NetOverlay {
                name: name.clone(),
                shapes,
            });
        }

        Extraction {
            netlist,
            map,
            diagnostics: self.diags,
        }
    }

    /// Validate an element block and register its pin terminals.
    fn element(
        &mut self,
        b: usize,
        ms: &[(usize, &'a str, Pt)],
        terms: &mut Vec<Terminal>,
        seen: &mut HashMap<String, String>,
        index: usize,
    ) -> Option<Element> {
        let d = self.d;
        let Some(refdes) = d.blocks[b].label.clone() else {
            let value = self.value(b, ms, ' ').unwrap_or_default();
            let at = ms.first().map(|m| m.2).unwrap_or((0.0, 0.0));
            let msg = format!(
                "element without a label (refdes) at ({:.3}, {:.3}){}; skipped — write `R1: resistor(…)`",
                at.0,
                at.1,
                if value.is_empty() {
                    String::new()
                } else {
                    format!(", value `{value}`")
                }
            );
            let span = self.block_span(b);
            self.diag(PicSeverity::Error, PicDiagKind::UnlabeledElement, msg, span);
            return None;
        };
        let span = self.block_span(b);
        let letter = refdes.chars().next().unwrap_or(' ').to_ascii_uppercase();
        let Some(mut pins) = pins_for(letter) else {
            self.diag(
                PicSeverity::Error,
                PicDiagKind::UnknownElement,
                format!(
                    "`{refdes}`: element letter `{letter}` is not supported ({LETTERS}); skipped"
                ),
                span,
            );
            return None;
        };
        if letter == 'Q' && ms.iter().any(|m| m.1 == "S") {
            pins = BJT4_PINS;
        }
        if letter == 'K' && !ms.iter().any(|m| m.1 == COUPLING_MARKER) {
            self.diag(
                PicSeverity::Error,
                PicDiagKind::MissingPin,
                format!(
                    "`{refdes}`: a `K` element is drawn with `coupling(L1, L2, k)`, \
                     not this macro; skipped"
                ),
                span,
            );
            return None;
        }
        let value = self.value(b, ms, letter);
        let key = refdes.to_ascii_lowercase();
        if let Some(prev) = seen.get(&key) {
            self.diag(
                PicSeverity::Error,
                PicDiagKind::DuplicateRefdes,
                format!("refdes `{refdes}` already used by `{prev}`; skipped"),
                span,
            );
            return None;
        }
        let mut pin_terms = Vec::with_capacity(pins.len());
        let mut missing = Vec::new();
        for (pi, &pin) in pins.iter().enumerate() {
            match ms.iter().find(|m| m.1 == pin) {
                Some(&(i, _, p)) => {
                    pin_terms.push(terms.len());
                    terms.push(Terminal {
                        at: p,
                        shape: i,
                        term: Term::Pin {
                            elem: index,
                            pin: pi,
                        },
                    });
                }
                None => missing.push(pin),
            }
        }
        if !missing.is_empty() {
            // Undo the pins registered for this element.
            terms.truncate(terms.len() - pin_terms.len());
            self.diag(
                PicSeverity::Error,
                PicDiagKind::MissingPin,
                format!(
                    "`{refdes}`: a `{letter}` element needs pins {}, but the drawn element \
                     lacks {} (wrong refdes letter for this macro?); skipped",
                    pins.join(", "),
                    missing.join(", ")
                ),
                span,
            );
            return None;
        }
        seen.insert(key, refdes.clone());
        let Some(value) = value else {
            self.diag(
                PicSeverity::Error,
                PicDiagKind::MissingValue,
                format!(
                    "`{refdes}` draws a display text instead of its value, but its call \
                     is not the element's own macro (a wrapper?), so the value cannot \
                     be read; the card is incomplete"
                ),
                span.clone(),
            );
            return Some(Element {
                refdes,
                pins,
                terms: pin_terms,
                value: String::new(),
                block: b,
                span,
            });
        };
        if value.is_empty() {
            // A source without a value is 0 V / 0 A in SPICE; anything
            // else is incomplete.
            let (sev, what) = if matches!(letter, 'V' | 'I') {
                (PicSeverity::Warning, "it is a 0 V/0 A source")
            } else {
                (PicSeverity::Error, "the card is incomplete")
            };
            self.diag(
                sev,
                PicDiagKind::MissingValue,
                format!("`{refdes}` has no value/model text; {what}"),
                span.clone(),
            );
        }
        Some(Element {
            refdes,
            pins,
            terms: pin_terms,
            value,
            block: b,
            span,
        })
    }

    /// Elements whose value names other elements: an `F`/`H` must
    /// start with the refdes of a `V` source (the controlling branch), a
    /// `K` with the refdes of two `L` elements. A card without them is
    /// invalid, so these are errors (reported as [`PicDiagKind::MissingValue`]:
    /// the value lacks what the card needs). An empty value was already
    /// reported.
    fn check_references(&mut self, elements: &[Element]) {
        let letter_of = |name: &str| {
            elements
                .iter()
                .find(|e| e.refdes.eq_ignore_ascii_case(name))
                .and_then(|e| e.refdes.chars().next())
                .map(|c| c.to_ascii_uppercase())
        };
        for e in elements {
            let letter = e.refdes.chars().next().unwrap_or(' ').to_ascii_uppercase();
            let toks: Vec<&str> = e.value.split_whitespace().collect();
            if toks.is_empty() {
                continue;
            }
            let (want, refs, what): (char, &[&str], &str) = match letter {
                'F' | 'H' => (
                    'V',
                    &toks[..1],
                    "a controlling voltage source (`Vname gain`)",
                ),
                'K' => (
                    'L',
                    &toks[..toks.len().min(2)],
                    "two inductors (`L1, L2, k`)",
                ),
                _ => continue,
            };
            for r in refs {
                let problem = match letter_of(r) {
                    Some(l) if l == want => continue,
                    Some(_) => format!("`{r}` is not a `{want}` element"),
                    None if r
                        .chars()
                        .next()
                        .is_some_and(|c| c.eq_ignore_ascii_case(&want)) =>
                    {
                        format!("no element `{r}` in the figure")
                    }
                    None => format!("`{r}` is not a `{want}` refdes"),
                };
                self.diag(
                    PicSeverity::Error,
                    PicDiagKind::MissingValue,
                    format!(
                        "`{}` needs {what}: {problem}; the card is invalid",
                        e.refdes
                    ),
                    e.span.clone(),
                );
            }
            if letter == 'K' && toks.len() < 3 {
                self.diag(
                    PicSeverity::Error,
                    PicDiagKind::MissingValue,
                    format!("`{}` has no coupling coefficient", e.refdes),
                    e.span.clone(),
                );
            }
        }
    }

    /// Where block `b` was asked for: its outermost macro call (or its
    /// inline `[`) — in the user's source for every element the figure
    /// writes, even though its shapes point into the prelude.
    fn block_span(&self, b: usize) -> Option<PicSpan> {
        self.d.blocks[b].span.as_ref().map(|s| self.span(s))
    }

    /// The value/model of element block `b` (D-0004 §2): the macro
    /// argument after the pin positions — index 1 for elements centered
    /// on an `At` anchor, 4 for the four-point `E`/`G`, 2 otherwise —
    /// verbatim from the user's source. A `K` (`coupling(L1, L2, k)`)
    /// takes all three arguments, `L1 L2 k`; its drawing shows only
    /// `k=…`, so it has no drawn fallback.
    ///
    /// A call to one of the [`DISPLAY_MACROS`] with one more argument,
    /// non-empty, carries a display text that the prelude draws instead
    /// of the value: the value still comes from its own argument, and
    /// the check below compares the drawing with the display. A
    /// mismatch then falls back on the drawn text, which is the display
    /// — wrong, but it only happens when the call is not the element's
    /// (and the extraction would be wrong without the display too).
    /// A drawing that shows a display (`Disp` marker) without such a
    /// call — a wrapper macro passing one — has no trustworthy value:
    /// `None`.
    ///
    /// The call is trusted only when its argument is what the prelude
    /// drew (both normalized by pic), which rules out a user wrapper
    /// macro whose arguments are not the element's (`BlockInfo.call` is
    /// the *outermost* call). Fallbacks, in order: rpic's normalized
    /// argument (an argument itself split over lines, see
    /// [`Self::raw_arg`]); the drawn text (no call — an inline block —
    /// or a mismatch).
    fn value(&self, b: usize, ms: &[(usize, &'a str, Pt)], letter: char) -> Option<String> {
        let coupling = letter == 'K';
        let drawn = self.value_text(b);
        // A drawn display is not the value: nothing to fall back on.
        let shows_display = ms.iter().any(|m| m.1 == DISPLAY_MARKER);
        let fallback = if shows_display {
            None
        } else if coupling {
            Some(String::new())
        } else {
            Some(drawn.clone())
        };
        let k = match letter {
            'E' | 'G' => 4,
            _ if ms.iter().any(|m| m.1 == ANCHOR_MARKER) => 1,
            _ => 2,
        };
        let Some(call) = &self.d.blocks[b].call else {
            return fallback;
        };
        let Some(arg) = call.args.get(k) else {
            return fallback;
        };
        if coupling && call.args.len() != 3 {
            return fallback;
        }
        let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let spliced = unquote(arg);
        // A display text (one argument past the value, non-empty) is
        // what the prelude drew, so it is what the call must match.
        // Only a call to the prelude macro itself has one: a wrapper's
        // extra argument says nothing about the element.
        let display = (DISPLAY_MACROS.contains(&call.name.as_str()) && call.args.len() == k + 2)
            .then(|| words(unquote(&call.args[k + 1])))
            .filter(|d| !d.is_empty());
        if shows_display && display.is_none() {
            // The display came through a wrapper: its arguments are
            // not the element's.
            return None;
        }
        let expect = if coupling {
            format!("k={}", words(spliced))
        } else if let Some(d) = display {
            d
        } else {
            words(spliced)
        };
        if !drawn.is_empty() && expect != words(&drawn) {
            return fallback;
        }
        let pick = |i: usize| unquote(&self.raw_arg(call, i)).trim().to_string();
        Some(if coupling {
            (0..3).map(pick).collect::<Vec<_>>().join(" ")
        } else {
            pick(k)
        })
    }

    /// Argument `i` of `call` as the user wrote it (`MacroCall.raw_args`:
    /// `1e-9` stays `1e-9`) when it is one line of the user's source;
    /// otherwise — an argument split by `\` continuations, or one that
    /// is not the user's text — rpic's normalized argument.
    fn raw_arg(&self, call: &MacroCall, i: usize) -> String {
        match call.raw_args.get(i) {
            Some(raw)
                if call.raw_args.len() == call.args.len()
                    && raw.span.file.is_none()
                    && raw.span.line > self.prelude_lines
                    && raw.end_line == raw.span.line =>
            {
                raw.text.clone()
            }
            _ => call.args[i].clone(),
        }
    }

    /// The first text shape inside block `b` (any depth), lines joined
    /// by spaces, trimmed.
    fn value_text(&self, b: usize) -> String {
        let (lo, hi) = self.d.blocks[b].shapes;
        for i in lo..hi.min(self.d.shapes.len()) {
            if let Shape::Text { text, .. } = &self.d.shapes[i] {
                let joined: Vec<&str> = text.iter().map(|t| t.s.trim()).collect();
                return joined.join(" ").trim().to_string();
            }
        }
        String::new()
    }

    /// Where the first place named `label` is defined.
    fn label_span(&self, label: &str) -> Option<PicSpan> {
        self.places
            .iter()
            .find(|p| p.label.eq_ignore_ascii_case(label))
            .and_then(|p| p.span.clone())
    }
}

/// A lone string argument splices as its content (pic's
/// quote-at-use-site idiom), so `"DX"` is the value `DX`.
fn unquote(arg: &str) -> &str {
    let t = arg.trim();
    match t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        Some(inner) if !inner.contains('"') => inner,
        _ => arg,
    }
}
