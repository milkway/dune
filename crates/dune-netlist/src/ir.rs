//! Intermediate representation of a parsed deck (PLAN.md §2.1).
//!
//! Faithful to the 3f5 contract: identifiers are lowercased (the C
//! lowercases the whole deck), parameter names are the 3f5 keywords,
//! nodes are strings and `0` is ground. Everything carries a [`Span`].

use crate::span::Span;

/// A lowercased identifier (device, node, model or subcircuit name).
pub type Ident = String;

#[derive(Clone, Debug, Default)]
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
    /// `.measure` / `.meas` cards (ngspice grammar; pure
    /// post-processing, never part of the simulation).
    pub measures: Vec<Measure>,
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
        /// `PORTNUM N <Z0 Z>` (ngspice RF port, V sources only):
        /// present only when the card makes the source a port —
        /// `N > 0` and a positive reference impedance — as
        /// `VSRCtemp` decides it.
        port: Option<RfPort>,
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
    /// `M`: 4 nodes + model + geometry parameters (3 to 5 nodes for an
    /// ngspice VDMOS model: drain, gate, source, then the junction and
    /// case temperature nodes).
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
    /// `B`: non-linear dependent source (`ASRC`), `V=EXPR` or
    /// `I=EXPR`; when the card gives both, the last one wins, as in
    /// `INPdevParse` + `ASRCparam`.
    Behavioral { kind: SourceKind, expr: Expr },
    /// `X`: subcircuit call, only seen before expansion.
    SubcktCall { subname: Ident },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Voltage,
    Current,
}

/// A B-source expression: the 3f5 parse tree (`INPparseNode`,
/// `inpptree.h`) as `PTparse` builds it — no constant folding (only
/// the derivative trees fold, in `dune-core`) and every name already
/// resolved. Built by [`crate::parse_expr`].
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// `PT_CONSTANT`: a number or one of the constants `e`, `pi`.
    Num(f64),
    /// `v(node)` (`PT_VAR` of type `IF_NODE`). `v(a,b)` is parsed
    /// into `v(a) - v(b)`, as `mkfnode` does.
    V(Ident),
    /// `i(vname)` (`PT_VAR` of type `IF_INSTANCE`): the branch current
    /// of a device with a branch equation (V, E, H, L, a voltage-type
    /// B).
    I(Ident),
    /// `PT_PLUS` … `PT_POWER`.
    Bin(ExprOp, Box<Expr>, Box<Expr>),
    /// `PT_FUNCTION` (unary minus included, as `ExprFunc::Neg`).
    Call(ExprFunc, Box<Expr>),
}

/// The binary operators of the 3f5 expression grammar (`ops[]` in
/// `inpptree.c`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprOp {
    Plus,
    Minus,
    Times,
    Divide,
    Power,
}

/// The functions of `ptfuncs.c` (`funcs[]` in `inpptree.c`), with
/// their 3f5 semantics: `log` is the base-10 logarithm, `ln`/`log`/
/// `sqrt` act on `|x|`, `u` is the unit step (½ at 0), `uramp` the
/// unit ramp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprFunc {
    Abs,
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Cos,
    Cosh,
    Exp,
    Ln,
    Log,
    Sgn,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    U,
    Uramp,
    /// Unary minus (`PTF_UMINUS`, the function named `-`).
    Neg,
}

impl ExprFunc {
    /// The 3f5 function table, by name (`funcs[]`; `-` is reached
    /// only through the unary-minus token).
    pub const TABLE: [(&'static str, ExprFunc); 20] = [
        ("abs", ExprFunc::Abs),
        ("acos", ExprFunc::Acos),
        ("acosh", ExprFunc::Acosh),
        ("asin", ExprFunc::Asin),
        ("asinh", ExprFunc::Asinh),
        ("atan", ExprFunc::Atan),
        ("atanh", ExprFunc::Atanh),
        ("cos", ExprFunc::Cos),
        ("cosh", ExprFunc::Cosh),
        ("exp", ExprFunc::Exp),
        ("ln", ExprFunc::Ln),
        ("log", ExprFunc::Log),
        ("sgn", ExprFunc::Sgn),
        ("sin", ExprFunc::Sin),
        ("sinh", ExprFunc::Sinh),
        ("sqrt", ExprFunc::Sqrt),
        ("tan", ExprFunc::Tan),
        ("tanh", ExprFunc::Tanh),
        ("u", ExprFunc::U),
        ("uramp", ExprFunc::Uramp),
    ];

    /// The name the function is written with (`-` for [`ExprFunc::Neg`]).
    pub fn name(self) -> &'static str {
        if self == ExprFunc::Neg {
            return "-";
        }
        Self::TABLE
            .iter()
            .find(|(_, f)| *f == self)
            .map_or("?", |(n, _)| n)
    }
}

impl Expr {
    /// Visit every `v()` node name and `i()` device name, mutably
    /// (subcircuit expansion renames them). Depth is bounded by the
    /// parser, so the recursion is too.
    pub fn visit_refs_mut(
        &mut self,
        node: &mut dyn FnMut(&mut Ident),
        device: &mut dyn FnMut(&mut Ident),
    ) {
        match self {
            Expr::Num(_) => {}
            Expr::V(n) => node(n),
            Expr::I(d) => device(d),
            Expr::Bin(_, l, r) => {
                l.visit_refs_mut(node, device);
                r.visit_refs_mut(node, device);
            }
            Expr::Call(_, a) => a.visit_refs_mut(node, device),
        }
    }
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

/// An ngspice RF port on a V source (`PORTNUM N <Z0 Z>`, `vsrc.c`
/// under `RFSPICE`): the source sits behind a series reference
/// impedance `z0` (default 50 Ω) and `.sp` excites it as port `num`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RfPort {
    /// Port index, 1-based (`VSRCportNum`).
    pub num: u32,
    /// Reference impedance in ohms (`VSRCportZ0`), > 0.
    pub z0: f64,
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
    /// ngspice-47 `TRNOISE(NA NT NALPHA NAMP RTSAM RTSCAPT RTSEMT)`:
    /// transient noise (white, 1/f^alpha, random telegraph).
    TrNoise,
    /// ngspice-47 `TRRANDOM(TYPE TS TD PARAM1 PARAM2)`: a random
    /// value held for `TS` seconds.
    TrRandom,
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
    /// `.sens OUTVAR [FILTER…] [AC DEC|OCT|LIN N FSTART FSTOP | DC]`
    /// (the filters — glob patterns over the sensitivity vector
    /// names, `*` and `?` — are ngspice's extension of the 3f5 card).
    Sens {
        outvar: OutVar,
        filters: Vec<Ident>,
        ac: Option<(Variation, f64, f64, f64)>,
        span: Span,
    },
    /// `.sp DEC|OCT|LIN N FSTART FSTOP <DONOISE>` — ngspice's
    /// S-parameter analysis over the `PORTNUM` V sources
    /// (`inp2dot.c` `dot_sp`; `DONOISE` is the integer 1 to add the
    /// noise-correlation outputs).
    Sp {
        variation: Variation,
        points: f64,
        fstart: f64,
        fstop: f64,
        donoise: bool,
        span: Span,
    },
    /// `.pss FREQ TSTAB OSCNODE <POINTS <HARMS <SCITER <STEADYCOEFF>>>>
    /// <UIC>` — the periodic steady state, in ngspice-47's positional
    /// grammar (`inp2dot.c` `dot_pss`; there every field up to
    /// STEADYCOEFF is required). `FREQ = 0` asks Dune to take the
    /// fundamental from the periodic sources.
    Pss {
        freq: f64,
        tstab: f64,
        osc_node: Ident,
        points: Option<f64>,
        harmonics: Option<f64>,
        sc_iter: Option<f64>,
        steady_coeff: Option<f64>,
        uic: bool,
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

/// One `.measure` card (ngspice-47 `com_measure2.c` grammar — the
/// 3f5 has none, so ngspice is the contract here; see
/// `docs/dune-measure.md`).
#[derive(Clone, Debug, PartialEq)]
pub struct Measure {
    /// Which plot the card measures.
    pub analysis: MeasAnalysis,
    /// Result name (lowercased, like every deck identifier).
    pub name: Ident,
    /// The measurement itself.
    pub func: MeasFunc,
    /// The function keyword as written, lowercased (`trig`, `when`,
    /// `avg`…), for ngspice-style messages.
    pub keyword: String,
    /// The card text as written (continuations joined).
    pub text: String,
    pub span: Span,
}

/// The analysis a `.measure` card applies to (`chkAnalysisType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasAnalysis {
    Tran,
    Ac,
    Dc,
    /// ngspice's `.sp` (measured like `.ac`).
    Sp,
}

impl MeasAnalysis {
    /// The card keyword (`tran`, `ac`, `dc`, `sp`).
    pub fn keyword(self) -> &'static str {
        match self {
            MeasAnalysis::Tran => "tran",
            MeasAnalysis::Ac => "ac",
            MeasAnalysis::Dc => "dc",
            MeasAnalysis::Sp => "sp",
        }
    }
}

/// What a `.measure` computes.
#[derive(Clone, Debug, PartialEq)]
pub enum MeasFunc {
    /// `TRIG … TARG …` (also spelled `DELAY`): `targ − trig`.
    Delay { trig: MeasTrig, targ: MeasTrig },
    /// `WHEN out=val|out2`: the scale value of a crossing.
    When(MeasCross),
    /// `FIND out AT=x` / `FIND out WHEN …`: the interpolated value.
    Find { var: OutVar, at: MeasPoint },
    /// `DERIV out AT=x` / `DERIV out WHEN …`: d(out)/d(scale) —
    /// a Dune extension; ngspice-47 parses it but reports
    /// "currently not supported".
    Deriv { var: OutVar, at: MeasPoint },
    /// `AVG|MIN|MAX|MIN_AT|MAX_AT|PP|RMS|INTEG out FROM= TO=`.
    Stat {
        stat: MeasStat,
        var: OutVar,
        window: MeasWindow,
    },
}

/// One side of a `TRIG … TARG …` delay.
#[derive(Clone, Debug, PartialEq)]
pub enum MeasTrig {
    /// `TRIG AT=x`: a fixed scale value.
    At(f64),
    /// `TRIG out VAL=v RISE|FALL|CROSS=n …`.
    Cross(MeasCross),
}

/// Where `FIND`/`DERIV` evaluate.
#[derive(Clone, Debug, PartialEq)]
pub enum MeasPoint {
    At(f64),
    When(MeasCross),
}

/// A level crossing: `out` against a constant or a second vector.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasCross {
    pub var: OutVar,
    pub level: MeasLevel,
    pub edge: MeasEdge,
    pub window: MeasWindow,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeasLevel {
    Value(f64),
    /// `WHEN v(a)=v(b)`.
    Var(OutVar),
}

/// Which crossing counts (`RISE=`, `FALL=`, `CROSS=`; the last one
/// given wins, as in `measure_parse_stdParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasEdge {
    /// None given: the first crossing either way.
    Any,
    Rise(MeasCount),
    Fall(MeasCount),
    Cross(MeasCount),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasCount {
    /// The n-th (1-based).
    Nth(u32),
    /// `=LAST` (or a bare `LAST`, which means `CROSS=LAST`).
    Last,
}

/// `TD=`, `FROM=`, `TO=` as given (`None` = not on the card; the
/// per-analysis defaults are applied by the evaluator, like ngspice).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MeasWindow {
    pub td: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasStat {
    Min,
    Max,
    /// The scale value of the minimum.
    MinAt,
    /// The scale value of the maximum.
    MaxAt,
    /// Peak to peak.
    Pp,
    Avg,
    Rms,
    Integ,
}
