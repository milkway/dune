//! Intermediate representation of a parsed deck (PLAN.md §2.1).
//!
//! Faithful to the 3f5 contract: identifiers are lowercased (the C
//! lowercases the whole deck), parameter names are the 3f5 keywords,
//! nodes are strings and `0` is ground. Everything carries a [`Span`].

use crate::span::Span;

/// A lowercased identifier (device, node, model or subcircuit name).
pub type Ident = String;

#[derive(Debug, Default)]
pub struct Netlist {
    pub title: String,
    /// Flat element list (subcircuits already expanded).
    pub elements: Vec<Element>,
    pub models: Vec<Model>,
    pub analyses: Vec<Analysis>,
    /// `.options`, as given; defaults live in `dune-core::consts`.
    pub options: Vec<Param>,
    /// `.print` / `.plot` / `.save` / `.four`.
    pub outputs: Vec<Output>,
    /// `.ic` and `.nodeset` entries.
    pub node_inits: Vec<NodeInit>,
    /// `.control` … `.endc` lines, verbatim and uninterpreted.
    pub control: Vec<String>,
    /// `.width out=N`: print column width (the `in=` half only ever
    /// mattered to punched-card decks and is ignored).
    pub width_out: Option<usize>,
}

/// One circuit element (an instance line).
#[derive(Clone, Debug)]
pub struct Element {
    /// Full lowercased name; the first letter is the device type.
    /// Subcircuit expansion produces dotted names (`x1.r1`).
    pub name: Ident,
    /// Connection nodes, in card order (empty for `K`).
    pub nodes: Vec<Ident>,
    pub device: Device,
    pub span: Span,
}

/// Extra per-card data, one variant per 3f5 device letter.
///
/// `params` hold the keyword parameters exactly as the card gave
/// them (3f5 keywords, lowercase); validation against each device's
/// real parameter table is `dune-core`'s job when the device is
/// implemented.
#[derive(Clone, Debug)]
pub enum Device {
    /// `R`: `RXXXXXXX N1 N2 VALUE` / semiconductor form with MNAME.
    Resistor {
        value: Option<f64>,
        model: Option<Ident>,
        params: Vec<Param>,
    },
    /// `C`: `CXXXXXXX N+ N- VALUE <IC=INCOND>` / semiconductor form.
    Capacitor {
        value: Option<f64>,
        model: Option<Ident>,
        params: Vec<Param>,
    },
    /// `L`: `LYYYYYYY N+ N- VALUE <IC=INCOND>`.
    Inductor {
        value: Option<f64>,
        params: Vec<Param>,
    },
    /// `K`: `KXXXXXXX LYYYYYYY LZZZZZZZ VALUE` (no nodes).
    Mutual { l1: Ident, l2: Ident, k: f64 },
    /// `V`/`I`: independent source.
    Source {
        kind: SourceKind,
        dc: Option<f64>,
        ac: Option<AcSpec>,
        tran: Option<TranFn>,
        /// `DISTOF1 <MAG <PHASE>>`.
        distof1: Option<DistoSpec>,
        distof2: Option<DistoSpec>,
    },
    /// `E`: VCVS, 4 nodes + gain.
    Vcvs { gain: Option<f64> },
    /// `G`: VCCS, 4 nodes + transconductance.
    Vccs { gain: Option<f64> },
    /// `F`: CCCS, 2 nodes + controlling V-source + gain.
    Cccs { vname: Ident, gain: Option<f64> },
    /// `H`: CCVS, 2 nodes + controlling V-source + transresistance.
    Ccvs { vname: Ident, gain: Option<f64> },
    /// `D`: `DXXXXXXX N+ N- MNAME <AREA> <OFF> <IC=VD> <TEMP=T>`.
    Diode {
        model: Ident,
        area: Option<f64>,
        off: bool,
        params: Vec<Param>,
    },
    /// `Q`: 3 or 4 nodes (optional substrate) + model.
    Bjt {
        model: Ident,
        area: Option<f64>,
        off: bool,
        params: Vec<Param>,
    },
    /// `J`: `JXXXXXXX ND NG NS MNAME <AREA> <OFF> <IC=…>`.
    Jfet {
        model: Ident,
        area: Option<f64>,
        off: bool,
        params: Vec<Param>,
    },
    /// `Z`: MESFET, same shape as `J`.
    Mesfet {
        model: Ident,
        area: Option<f64>,
        off: bool,
        params: Vec<Param>,
    },
    /// `M`: 4 nodes + model + geometry parameters.
    Mosfet { model: Ident, params: Vec<Param> },
    /// `S`: voltage-controlled switch, 4 nodes + model.
    SwitchV { model: Ident, state: Option<OnOff> },
    /// `W`: current-controlled switch, 2 nodes + V-source + model.
    SwitchI {
        vname: Ident,
        model: Ident,
        state: Option<OnOff>,
    },
    /// `T`: lossless transmission line, 4 nodes, `Z0=` etc.
    TlineIdeal { params: Vec<Param> },
    /// `O`: lossy transmission line, 4 nodes + LTRA model.
    TlineLossy { model: Ident },
    /// `U`: uniform distributed RC, 3 nodes + URC model.
    Urc { model: Ident, params: Vec<Param> },
    /// `B`: non-linear dependent source. The expression is kept as
    /// raw text until the corpus demands parse trees (`inpptree.c`).
    Behavioral {
        v_expr: Option<String>,
        i_expr: Option<String>,
    },
    /// `X`: subcircuit call, only seen before expansion.
    SubcktCall { subname: Ident },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Voltage,
    Current,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnOff {
    On,
    Off,
}

/// `AC <ACMAG <ACPHASE>>` — defaults are the 3f5's (magnitude 1,
/// phase 0 when omitted).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcSpec {
    pub mag: f64,
    pub phase: f64,
}

/// `DISTOF1/DISTOF2 <MAG <PHASE>>` (defaults 1 and 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DistoSpec {
    pub mag: f64,
    pub phase: f64,
}

/// A time-dependent source function. Arguments are stored exactly as
/// given; defaults (many depend on TSTEP/TSTOP) are applied by
/// `dune-core` at analysis time, as the 3f5 does.
#[derive(Clone, Debug, PartialEq)]
pub struct TranFn {
    pub kind: TranFnKind,
    pub args: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranFnKind {
    Pulse,
    Sin,
    Exp,
    Pwl,
    Sffm,
}

/// A keyword parameter from a card: `IC=3`, `OFF`, `IC=2,0.5,0,0.2`,
/// `METHOD=GEAR`.
#[derive(Clone, Debug)]
pub struct Param {
    /// Lowercased 3f5 keyword.
    pub name: Ident,
    pub value: ParamValue,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    /// Bare keyword (`off`, `uic`, `acct`).
    Flag,
    Num(f64),
    /// Consecutive numbers after the keyword (`ic=2,0.5,0,0.2`).
    NumList(Vec<f64>),
    /// Non-numeric value (`method=gear`).
    Str(String),
}

/// `.model MNAME TYPE(PNAME1=PVAL1 …)`.
#[derive(Clone, Debug)]
pub struct Model {
    pub name: Ident,
    /// 3f5 type name: `r c sw csw urc ltra d npn pnp njf pjf nmos
    /// pmos nmf pmf`.
    pub mtype: Ident,
    pub params: Vec<Param>,
    pub span: Span,
}

/// An analysis request (one dot card).
#[derive(Clone, Debug)]
pub enum Analysis {
    /// `.op`
    Op { span: Span },
    /// `.dc SRCNAM VSTART VSTOP VINCR [SRC2 START2 STOP2 INCR2]`
    Dc { sweeps: Vec<DcSweep>, span: Span },
    /// `.ac DEC|OCT|LIN N FSTART FSTOP`
    Ac {
        variation: Variation,
        points: f64,
        fstart: f64,
        fstop: f64,
        span: Span,
    },
    /// `.tran TSTEP TSTOP <TSTART <TMAX>> <UIC>`
    Tran {
        tstep: f64,
        tstop: f64,
        tstart: Option<f64>,
        tmax: Option<f64>,
        uic: bool,
        span: Span,
    },
    /// `.pz NODE1 NODE2 NODE3 NODE4 CUR|VOL POL|ZER|PZ`
    Pz {
        nodes: [Ident; 4],
        input: PzInput,
        which: PzWhich,
        span: Span,
    },
    /// `.noise V(OUT<,REF>) SRC DEC|LIN|OCT PTS FSTART FSTOP <PPS>`
    Noise {
        output: Ident,
        output_ref: Option<Ident>,
        src: Ident,
        variation: Variation,
        points: f64,
        fstart: f64,
        fstop: f64,
        pts_per_summary: Option<f64>,
        span: Span,
    },
    /// `.disto DEC|OCT|LIN NP FSTART FSTOP <F2OVERF1>`
    Disto {
        variation: Variation,
        points: f64,
        fstart: f64,
        fstop: f64,
        f2overf1: Option<f64>,
        span: Span,
    },
    /// `.sens OUTVAR` / `.sens OUTVAR AC DEC|OCT|LIN N FSTART FSTOP`
    Sens {
        outvar: OutVar,
        ac: Option<(Variation, f64, f64, f64)>,
        span: Span,
    },
    /// `.tf OUTVAR INSRC`
    Tf {
        outvar: OutVar,
        insrc: Ident,
        span: Span,
    },
}

/// One `.dc` sweep source: name, start, stop, increment.
#[derive(Clone, Debug)]
pub struct DcSweep {
    pub src: Ident,
    pub start: f64,
    pub stop: f64,
    pub step: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variation {
    Dec,
    Oct,
    Lin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PzInput {
    Current,
    Voltage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PzWhich {
    Poles,
    Zeros,
    Both,
}

/// `.print` / `.plot` / `.save` / `.four`.
#[derive(Clone, Debug)]
pub struct Output {
    pub kind: OutputKind,
    /// `DC`, `AC`, `TRAN`, `NOISE` or `DISTO` for print/plot.
    pub analysis: Option<Ident>,
    pub vars: Vec<OutVar>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum OutputKind {
    Print,
    Plot,
    Save,
    /// `.four FREQ OV1 …`
    Four {
        freq: f64,
    },
}

/// An output variable reference.
#[derive(Clone, Debug, PartialEq)]
pub enum OutVar {
    /// `V(N1<,N2>)` with an optional AC suffix (`VM`, `VP`, `VR`,
    /// `VI`, `VDB`).
    Voltage {
        node: Ident,
        reference: Option<Ident>,
        suffix: Option<AcSuffix>,
    },
    /// `I(VXXXXXXX)` with the same optional suffixes.
    Current {
        vsrc: Ident,
        suffix: Option<AcSuffix>,
    },
    /// Anything else (a raw vector name for `.save`, e.g. `all`).
    Raw(Ident),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcSuffix {
    Real,
    Imag,
    Mag,
    Phase,
    Db,
}

/// One `.ic`/`.nodeset` assignment.
#[derive(Clone, Debug)]
pub struct NodeInit {
    pub kind: NodeInitKind,
    pub node: Ident,
    pub value: f64,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeInitKind {
    /// `.ic`: enforced initial condition.
    Ic,
    /// `.nodeset`: initial guess only.
    Nodeset,
}
