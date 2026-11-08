//! Public circuit construction (D-0008): one typed spec per device,
//! added to a [`Circuit`]; name/node resolution happens at build
//! time so declaration order never matters (the deck's order does
//! not either).

use std::collections::HashMap;

use crate::diode::{DiodeModel, DiodeTemp};
use crate::error::Error;
use crate::matrix::Var;
use crate::options::Options;
use crate::trnoise::NoiseWave;

/// `RXXXXXXX N1 N2 VALUE` (ohms; nonzero).
#[derive(Clone, Debug)]
pub struct Resistor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
}

/// The temperature dependence of one resistor (`REStemp`,
/// `dev/res/restemp.c`): `R(T) = R(TNOM)·[1 + TC1·(T−TNOM) +
/// TC2·(T−TNOM)²]`. Temperatures in kelvin; `None` takes the
/// circuit's `TEMP`/`TNOM` (`.options`, or the `.dc TEMP` sweep
/// point).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResTemp {
    pub tc1: f64,
    pub tc2: f64,
    /// Model `TNOM`.
    pub tnom: Option<f64>,
    /// Instance `TEMP=`.
    pub temp: Option<f64>,
}

impl ResTemp {
    /// `factor` of `REStemp`, the divisor of the conductance.
    pub fn factor(&self, opts: &Options) -> f64 {
        let difference = self.temp.unwrap_or(opts.temp) - self.tnom.unwrap_or(opts.tnom);
        1.0 + self.tc1 * difference + self.tc2 * difference * difference
    }
}

/// `CXXXXXXX N+ N- VALUE <IC=INCOND>` (farads).
#[derive(Clone, Debug)]
pub struct Capacitor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
    /// Initial voltage, used under `.tran … UIC`.
    pub ic: Option<f64>,
}

/// `LYYYYYYY N+ N- VALUE <IC=INCOND>` (henries).
#[derive(Clone, Debug)]
pub struct Inductor {
    pub name: String,
    pub nodes: [String; 2],
    pub value: f64,
    /// Initial current, used under `.tran … UIC`.
    pub ic: Option<f64>,
}

/// `KXXXXXXX L1 L2 K`. No effect at the operating point.
#[derive(Clone, Debug)]
pub struct Mutual {
    pub name: String,
    pub l1: String,
    pub l2: String,
    pub k: f64,
}

/// `VXXXXXXX N+ N- <DC VALUE> <tranfn(…)>`.
#[derive(Clone, Debug)]
pub struct Vsource {
    pub name: String,
    pub nodes: [String; 2],
    /// `None` when no DC value was given (transient functions then
    /// also provide the DC-mode value, like `VSRCload`).
    pub dc: Option<f64>,
    pub wave: Option<crate::srcfn::Wave>,
    /// `AC <MAG <PHASE°>>`: the small-signal stimulus.
    pub ac: Option<(f64, f64)>,
    /// `DISTOF1 <MAG <PHASE°>>`: the `.disto` input at f1.
    pub distof1: Option<(f64, f64)>,
    /// `DISTOF2 <MAG <PHASE°>>`: the `.disto` input at f2.
    pub distof2: Option<(f64, f64)>,
    /// `PORTNUM N <Z0 Z>`: makes the source an ngspice RF port —
    /// in every analysis it then drives its `+` node through the
    /// series reference impedance (internal node `<name>#res`), and
    /// `.sp` excites it. `None` for an ordinary source.
    pub port: Option<RfPort>,
}

/// An ngspice RF port (`vsrc.c` under `RFSPICE`, ngspice-47):
/// `PORTNUM num Z0 z0`. The source is a port only when `num > 0`
/// and `z0 > 0` (`VSRCtemp`); otherwise the field is inert. The
/// ports of a circuit must be numbered `1..=N` without gaps or
/// duplicates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RfPort {
    /// Port index, 1-based (`VSRCportNum`).
    pub num: u32,
    /// Reference impedance in ohms (`VSRCportZ0`; 50 on a card that
    /// gives none).
    pub z0: f64,
}

impl RfPort {
    /// `VSRCisPort`.
    pub(crate) fn is_port(&self) -> bool {
        self.num > 0 && self.z0 > 0.0
    }
}

/// `IYYYYYYY N+ N- <DC VALUE> <tranfn(…)>` (flows through the source
/// from `+` to `-`).
#[derive(Clone, Debug)]
pub struct Isource {
    pub name: String,
    pub nodes: [String; 2],
    pub dc: Option<f64>,
    pub wave: Option<crate::srcfn::Wave>,
    /// `AC <MAG <PHASE°>>`.
    pub ac: Option<(f64, f64)>,
    /// `DISTOF1 <MAG <PHASE°>>`.
    pub distof1: Option<(f64, f64)>,
    /// `DISTOF2 <MAG <PHASE°>>`.
    pub distof2: Option<(f64, f64)>,
}

/// `EXXXXXXX N+ N- NC+ NC- VALUE` (VCVS).
#[derive(Clone, Debug)]
pub struct Vcvs {
    pub name: String,
    pub nodes: [String; 4],
    pub gain: f64,
}

/// `GXXXXXXX N+ N- NC+ NC- VALUE` (VCCS, mhos).
#[derive(Clone, Debug)]
pub struct Vccs {
    pub name: String,
    pub nodes: [String; 4],
    pub gain: f64,
}

/// `FXXXXXXX N+ N- VNAM VALUE` (CCCS).
#[derive(Clone, Debug)]
pub struct Cccs {
    pub name: String,
    pub nodes: [String; 2],
    pub ctrl: String,
    pub gain: f64,
}

/// `HXXXXXXX N+ N- VNAM VALUE` (CCVS, ohms).
#[derive(Clone, Debug)]
pub struct Ccvs {
    pub name: String,
    pub nodes: [String; 2],
    pub ctrl: String,
    pub gain: f64,
}

/// `QXXXXXXX NC NB NE <NS> MNAME <AREA> <OFF> <IC=VBE,VCE> <TEMP=T>`
/// with its model inline. `nodes[3]` is ground when no substrate was
/// given.
#[derive(Clone, Debug)]
pub struct Bjt {
    pub name: String,
    /// Collector, base, emitter, substrate.
    pub nodes: [String; 4],
    pub model: crate::bjt::BjtModel,
    pub area: f64,
    pub off: bool,
    /// `IC=VBE,VCE`.
    pub ic: (f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// `MXXXXXXX ND NG NS NB MNAME <L=> <W=> <AD=> <AS=> <PD=> <PS=>
/// <NRD=> <NRS=> <OFF> <IC=VDS,VGS,VBS> <TEMP=T>` with its level-1
/// model inline. `l`/`w`/`ad`/`as` fall back to the
/// `.options defl/defw/defad/defas` values at build time.
#[derive(Clone, Debug)]
pub struct Mos1 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::mos1::Mos1Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    /// Squares of drain/source diffusion (`NRD`/`NRS`, default 1).
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// Like [`Mos1`], but with the level-2 (Grove-Frohman) model. The
/// 3f5 names its prime nodes `m1#internal#drain`/`#internal#source`
/// (`mos2set.c`).
#[derive(Clone, Debug)]
pub struct Mos2 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::mos2::Mos2Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// Like [`Mos1`], but with the level-3 (semi-empirical
/// short-channel) model. Prime nodes are named like MOS2's,
/// `m1#internal#drain`/`#internal#source` (`mos3set.c`).
#[derive(Clone, Debug)]
pub struct Mos3 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::mos3::Mos3Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// Like [`Mos1`], but with the level-6 (Sakurai-Newton) model.
#[derive(Clone, Debug)]
pub struct Mos6 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::mos6::Mos6Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    /// `NRD`/`NRS`; the 3f5 forces 1 when 0 (`mos6temp.c`).
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// Like [`Mos1`], but with the BSIM1 (level-4) model.
#[derive(Clone, Debug)]
pub struct Bsim1 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::bsim1::Bsim1Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
}

/// Like [`Mos1`], but with the BSIM2 (level-5) model.
#[derive(Clone, Debug)]
pub struct Bsim2 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::bsim2::Bsim2Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: Option<f64>,
    pub as_: Option<f64>,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
}

/// A BSIM3v3.3.0 MOSFET (`LEVEL=8`/`49`, ngspice-47's instance
/// line `MXXX D G S B MNAME <L=> <W=> <AD=> <AS=> <PD=> <PS=> <NRD=>
/// <NRS=> <M=> <DELVTO=> <MULU0=> <OFF> <IC=VDS,VGS,VBS>`, `b3.c`).
/// L and W default to 5 µm (`b3set.c`), not the `.options` MOS
/// defaults.
#[derive(Clone, Debug)]
pub struct Bsim3 {
    pub name: String,
    /// Drain, gate, source, bulk.
    pub nodes: [String; 4],
    pub model: crate::bsim3::Bsim3Model,
    pub l: Option<f64>,
    pub w: Option<f64>,
    pub ad: f64,
    pub as_: f64,
    pub pd: f64,
    pub ps: f64,
    pub nrd: f64,
    pub nrs: f64,
    /// `M`: parallel multiplier (1).
    pub m: f64,
    /// `DELVTO`: threshold shift added to `VTH0` (0).
    pub delvto: f64,
    /// `MULU0`: low-field mobility multiplier (1).
    pub mulu0: f64,
    pub off: bool,
    /// `IC=VDS,VGS,VBS`.
    pub ic: (f64, f64, f64),
}

/// `DXXXXXXX N+ N- MNAME <AREA> <OFF>` with its model inline.
#[derive(Clone, Debug)]
pub struct Diode {
    pub name: String,
    pub nodes: [String; 2],
    pub model: DiodeModel,
    pub area: f64,
    pub off: bool,
    /// Instance temperature override, kelvin (`TEMP=` is °C on the
    /// card).
    pub temp: Option<f64>,
    /// `IC=VD`: initial junction voltage under `.tran … UIC`.
    pub ic: Option<f64>,
}

/// `JXXXXXXX ND NG NS MNAME <AREA> <OFF> <IC=VDS,VGS> <TEMP=T>` with
/// its model inline. Prime nodes past `RD`/`RS` are named
/// `<name>#drain`/`<name>#source` (`JFETsetup`'s `CKTmkVolt` calls).
#[derive(Clone, Debug)]
pub struct Jfet {
    pub name: String,
    /// Drain, gate, source.
    pub nodes: [String; 3],
    pub model: crate::jfet::JfetModel,
    pub area: f64,
    pub off: bool,
    /// `IC=VDS,VGS`.
    pub ic: (f64, f64),
    /// Instance temperature override, kelvin.
    pub temp: Option<f64>,
}

/// `ZXXXXXXX ND NG NS MNAME <AREA> <OFF> <IC=VDS,VGS>` (MESFET) with
/// its model inline; prime nodes as for [`Jfet`]. The 3f5 MESFET has
/// no instance temperature.
#[derive(Clone, Debug)]
pub struct Mes {
    pub name: String,
    /// Drain, gate, source.
    pub nodes: [String; 3],
    pub model: crate::mes::MesModel,
    pub area: f64,
    pub off: bool,
    /// `IC=VDS,VGS`.
    pub ic: (f64, f64),
}

/// `OXXXXXXX N1 N2 N3 N4 MNAME`: a lossy transmission line (LTRA)
/// between port 1 (`N1`, `N2`) and port 2 (`N3`, `N4`), with its
/// model inline. The two port currents are extra unknowns named
/// `<name>#i1`/`<name>#i2` (`LTRAsetup`'s `CKTmkVolt` calls).
#[derive(Clone, Debug)]
pub struct Ltra {
    pub name: String,
    /// Port 1 +/−, port 2 +/−.
    pub nodes: [String; 4],
    pub model: crate::ltra::LtraModel,
}

/// `TXXXXXXX N1 N2 N3 N4 Z0=VALUE <TD=VALUE | F=FREQ <NL=NRMLEN>>
/// <IC=V1,I1,V2,I2>`: an ideal (lossless) transmission line between
/// port 1 (`N1`, `N2`) and port 2 (`N3`, `N4`) (`dev/tra/`). Four
/// extra unknowns, named like `TRAsetup`'s `CKTmkVolt` calls:
/// `<name>#i1`, `<name>#i2` (port currents into `N1`/`N3`) and
/// `<name>#int1`, `<name>#int2` (the nodes between each port's `Z0`
/// and its delayed source).
#[derive(Clone, Debug)]
pub struct Tra {
    pub name: String,
    /// Port 1 +/−, port 2 +/−.
    pub nodes: [String; 4],
    /// `Z0` (or `ZO`), ohms; required.
    pub z0: f64,
    /// `TD`, seconds. When absent the delay is `NL/F`.
    pub td: Option<f64>,
    /// `F`: the frequency `NL` refers to (default 1 GHz).
    pub f: Option<f64>,
    /// `NL`: normalized electrical length at `F` (default 0.25).
    pub nl: Option<f64>,
    /// `IC=V1,I1,V2,I2` (also `V1=`, `I1=`, `V2=`, `I2=`): port
    /// voltages and currents used under `.tran … UIC`.
    pub ic: [f64; 4],
}

/// `SXXXXXXX N+ N- NC+ NC- MODEL <ON|OFF>`: voltage-controlled
/// switch with its model inline (`dev/sw/`).
#[derive(Clone, Debug)]
pub struct SwitchV {
    pub name: String,
    /// Switch +/−, control +/−.
    pub nodes: [String; 4],
    pub model: crate::switch::SwModel,
    /// `ON` (true) or `OFF`/nothing: the state the operating point
    /// starts from (`SWzero_stateGiven`).
    pub on: bool,
}

/// `WXXXXXXX N+ N- VNAM MODEL <ON|OFF>`: current-controlled switch
/// keyed by the branch current of voltage source `ctrl`, with its
/// model inline (`dev/csw/`).
#[derive(Clone, Debug)]
pub struct SwitchI {
    pub name: String,
    pub nodes: [String; 2],
    pub ctrl: String,
    pub model: crate::switch::CswModel,
    /// `ON` (true) or `OFF`/nothing (`CSWzero_stateGiven`).
    pub on: bool,
}

#[derive(Clone, Debug)]
pub enum Spec {
    Bjt(Box<Bjt>),
    Mos1(Box<Mos1>),
    Mos2(Box<Mos2>),
    Mos3(Box<Mos3>),
    Mos6(Box<Mos6>),
    Bsim1(Box<Bsim1>),
    Bsim2(Box<Bsim2>),
    Bsim3(Box<Bsim3>),
    Resistor(Resistor),
    Capacitor(Capacitor),
    Inductor(Inductor),
    Mutual(Mutual),
    Vsource(Vsource),
    Isource(Isource),
    Vcvs(Vcvs),
    Vccs(Vccs),
    Cccs(Cccs),
    Ccvs(Ccvs),
    Diode(Diode),
    Jfet(Box<Jfet>),
    Mes(Box<Mes>),
    Ltra(Box<Ltra>),
    Tra(Box<Tra>),
    SwitchV(SwitchV),
    SwitchI(SwitchI),
}

macro_rules! impl_into_spec {
    ($($t:ident),*) => {$(
        impl From<$t> for Spec {
            fn from(s: $t) -> Spec { Spec::$t(s) }
        }
    )*};
}
impl_into_spec!(
    Resistor, Capacitor, Inductor, Mutual, Vsource, Isource, Vcvs, Vccs, Cccs, Ccvs, Diode,
    SwitchV, SwitchI
);

impl From<Bjt> for Spec {
    fn from(s: Bjt) -> Spec {
        Spec::Bjt(Box::new(s))
    }
}

impl From<Mos1> for Spec {
    fn from(s: Mos1) -> Spec {
        Spec::Mos1(Box::new(s))
    }
}

impl From<Mos2> for Spec {
    fn from(s: Mos2) -> Spec {
        Spec::Mos2(Box::new(s))
    }
}

impl From<Mos3> for Spec {
    fn from(s: Mos3) -> Spec {
        Spec::Mos3(Box::new(s))
    }
}

impl From<Mos6> for Spec {
    fn from(s: Mos6) -> Spec {
        Spec::Mos6(Box::new(s))
    }
}

impl From<Bsim1> for Spec {
    fn from(s: Bsim1) -> Spec {
        Spec::Bsim1(Box::new(s))
    }
}

impl From<Bsim2> for Spec {
    fn from(s: Bsim2) -> Spec {
        Spec::Bsim2(Box::new(s))
    }
}

impl From<Bsim3> for Spec {
    fn from(s: Bsim3) -> Spec {
        Spec::Bsim3(Box::new(s))
    }
}

impl From<Jfet> for Spec {
    fn from(s: Jfet) -> Spec {
        Spec::Jfet(Box::new(s))
    }
}

impl From<Mes> for Spec {
    fn from(s: Mes) -> Spec {
        Spec::Mes(Box::new(s))
    }
}

impl From<Ltra> for Spec {
    fn from(s: Ltra) -> Spec {
        Spec::Ltra(Box::new(s))
    }
}

impl From<Tra> for Spec {
    fn from(s: Tra) -> Spec {
        Spec::Tra(Box::new(s))
    }
}

impl Spec {
    pub(crate) fn name(&self) -> &str {
        match self {
            Spec::Bjt(s) => &s.name,
            Spec::Mos1(s) => &s.name,
            Spec::Mos2(s) => &s.name,
            Spec::Mos3(s) => &s.name,
            Spec::Mos6(s) => &s.name,
            Spec::Bsim1(s) => &s.name,
            Spec::Bsim2(s) => &s.name,
            Spec::Bsim3(s) => &s.name,
            Spec::Resistor(s) => &s.name,
            Spec::Capacitor(s) => &s.name,
            Spec::Inductor(s) => &s.name,
            Spec::Mutual(s) => &s.name,
            Spec::Vsource(s) => &s.name,
            Spec::Isource(s) => &s.name,
            Spec::Vcvs(s) => &s.name,
            Spec::Vccs(s) => &s.name,
            Spec::Cccs(s) => &s.name,
            Spec::Ccvs(s) => &s.name,
            Spec::Diode(s) => &s.name,
            Spec::Jfet(s) => &s.name,
            Spec::Mes(s) => &s.name,
            Spec::Ltra(s) => &s.name,
            Spec::Tra(s) => &s.name,
            Spec::SwitchV(s) => &s.name,
            Spec::SwitchI(s) => &s.name,
        }
    }
}

/// A circuit under construction. Analyses live in `analysis/`
/// (Phase 1: [`Circuit::op`]).
///
/// Cloning copies the element specs, `.nodeset`s and `.ic`s; the
/// clone never shares a [`Session`](crate::Session)'s warm-start
/// state.
#[derive(Clone, Default)]
pub struct Circuit {
    pub title: String,
    pub(crate) specs: Vec<Spec>,
    /// `.nodeset` entries (node, volts).
    pub(crate) nodesets: Vec<(String, f64)>,
    /// `.ic` entries (node, volts), applied at the transient
    /// operating point.
    pub(crate) ics: Vec<(String, f64)>,
    /// Instance name → which `IC=` components its card gave (the
    /// C's `*icXGiven` bits), recorded by the netlist front end for
    /// the semiconductors' `DEVsetic`. A device without an entry
    /// (built from a typed spec) has every component given.
    pub(crate) ic_given: HashMap<String, crate::devices::IcGiven>,
    /// Instance name → the temperature dependence of a resistor
    /// card that named an `R` model or a `TEMP=` (`restemp.c`),
    /// recorded by the netlist front end. A resistor without an
    /// entry (typed spec, plain card) has `TC1 = TC2 = 0`: its
    /// conductance is `1/R` at every temperature, as in the 3f5.
    pub(crate) res_temp: HashMap<String, ResTemp>,
    /// The warm-start channel of a [`Session`](crate::Session):
    /// empty for a plain circuit (the engine then runs exactly the
    /// 3f5 path).
    pub(crate) warm: crate::session::WarmSlot,
}

impl Circuit {
    pub fn new(title: impl Into<String>) -> Self {
        Circuit {
            title: title.into(),
            ..Circuit::default()
        }
    }

    /// Add one device. Names and nodes are case-insensitive (the 3f5
    /// lowercases the whole deck).
    pub fn add(&mut self, spec: impl Into<Spec>) -> Result<(), Error> {
        let mut spec = spec.into();
        normalize(&mut spec);
        let name = spec.name();
        if name.is_empty() {
            return Err(Error::Build("device name cannot be empty".into()));
        }
        if self.specs.iter().any(|s| s.name() == name) {
            return Err(Error::Build(format!("device `{name}` already defined")));
        }
        if let Spec::Resistor(r) = &spec
            && (r.value == 0.0 || !r.value.is_finite())
        {
            // spice.txt §3.1.1: "may be positive or negative but
            // not zero".
            return Err(Error::Build(format!(
                "resistor `{}` cannot have zero resistance",
                r.name
            )));
        }
        self.specs.push(spec);
        Ok(())
    }

    /// `.nodeset v(node) = value`.
    pub fn nodeset(&mut self, node: impl Into<String>, value: f64) {
        self.nodesets
            .push((node.into().to_ascii_lowercase(), value));
    }

    /// `.ic v(node) = value` (enforced at the transient operating
    /// point; `.tran … UIC` ignores it in favor of device ICs).
    pub fn ic(&mut self, node: impl Into<String>, value: f64) {
        self.ics.push((node.into().to_ascii_lowercase(), value));
    }

    /// Record the `restemp.c` temperature data of resistor `name`
    /// (the netlist front end; see [`ResTemp`]).
    pub(crate) fn set_res_temp(&mut self, name: &str, t: ResTemp) {
        self.res_temp.insert(name.to_ascii_lowercase(), t);
    }

    /// Record which `IC=` components instance `name`'s card gave
    /// (the netlist front end; see [`Circuit::ic_given`]).
    pub(crate) fn set_ic_given(&mut self, name: &str, given: crate::devices::IcGiven) {
        self.ic_given.insert(name.to_ascii_lowercase(), given);
    }
}

fn normalize(spec: &mut Spec) {
    #[allow(clippy::ptr_arg)] // uniform `&mut String` for iter_mut().for_each
    fn lc(s: &mut String) {
        s.make_ascii_lowercase();
    }
    match spec {
        Spec::Resistor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Capacitor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Inductor(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mutual(s) => {
            lc(&mut s.name);
            lc(&mut s.l1);
            lc(&mut s.l2);
        }
        Spec::Vsource(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Isource(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Vcvs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Vccs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Cccs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
            lc(&mut s.ctrl);
        }
        Spec::Ccvs(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
            lc(&mut s.ctrl);
        }
        Spec::Diode(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Bjt(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mos1(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mos2(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mos3(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mos6(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Bsim1(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Bsim2(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Bsim3(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Jfet(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Mes(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Ltra(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::Tra(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::SwitchV(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
        }
        Spec::SwitchI(s) => {
            lc(&mut s.name);
            s.nodes.iter_mut().for_each(lc);
            lc(&mut s.ctrl);
        }
    }
}

/// The unknowns of the MNA system: index 0 is ground, then node
/// voltages and branch currents in order of creation (the C's
/// `CKTnode` list).
pub(crate) struct Vars {
    pub names: Vec<String>,
    pub kinds: Vec<VarKind>,
}

/// Whether a node unknown is written to the output plots. ngspice's
/// default save list (`outitf.c`, `beginPlot`) drops the device-
/// internal nodes by suffix — `#internal`, `#source`, `#drain`,
/// `#collector`, `#emitter`, `#base` — and keeps every other node,
/// including the LTRA port-current equations `o1#i1`/`o1#i2`.
pub(crate) fn saved_node(name: &str) -> bool {
    const HIDDEN: [&str; 6] = [
        "#internal",
        "#source",
        "#drain",
        "#collector",
        "#emitter",
        "#base",
    ];
    !HIDDEN.iter().any(|h| name.contains(h))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VarKind {
    Ground,
    Node,
    Branch,
}

impl Vars {
    fn new() -> Self {
        Vars {
            names: vec!["0".into()],
            kinds: vec![VarKind::Ground],
        }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    fn push(&mut self, name: String, kind: VarKind) -> Var {
        self.names.push(name);
        self.kinds.push(kind);
        self.names.len() - 1
    }
}

/// Fully resolved circuit: instances bound to variable indices.
/// One RF port as the `.sp` analysis sees it (`VSRCinstance` with
/// `VSRCisPort`): external terminals, branch equation and
/// reference impedance.
#[derive(Clone, Debug)]
pub(crate) struct PortInfo {
    pub name: String,
    pub num: u32,
    pub z0: f64,
    pub pos: Var,
    pub neg: Var,
    pub br: Var,
}

pub(crate) struct Built {
    pub vars: Vars,
    pub devices: Vec<Box<dyn crate::devices::Device>>,
    /// Element name per device index (noise generators are named
    /// `onoise_<name>_<suffix>`).
    pub names: Vec<String>,
    pub nodesets: Vec<(Var, f64)>,
    pub ics: Vec<(Var, f64)>,
    /// Independent V/I source element name → index in `devices`
    /// (what a `.dc` sweep drives) and whether it is a V source.
    pub sources: HashMap<String, (usize, bool)>,
    /// RF ports (`CKTrfPorts`), sorted by port number `1..=N`.
    pub ports: Vec<PortInfo>,
    /// Total device state slots (`CKTnumStates`).
    pub num_states: usize,
    /// Per device (same order as `devices`), the window of state
    /// offsets it allocated: `[start, end)`, nondecreasing in device
    /// order. The parallel load hands each worker the union of its
    /// devices' windows.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    pub state_ranges: Vec<(usize, usize)>,
    /// Diode temperature-adjust notes etc., surfaced by the caller.
    pub notes: Vec<String>,
    /// The circuit's warm-start channel (see [`Circuit::warm`]).
    pub warm: crate::session::WarmSlot,
}

impl Circuit {
    /// Resolve names and build device instances (the 3f5's setup
    /// pass).
    pub(crate) fn build(&self, opts: &Options) -> Result<Built, Error> {
        use crate::devices::*;

        let mut vars = Vars::new();
        let mut node_ids: HashMap<String, Var> = HashMap::new();
        node_ids.insert("0".into(), 0);
        let node = |vars: &mut Vars, node_ids: &mut HashMap<String, Var>, name: &str| -> Var {
            *node_ids
                .entry(name.to_string())
                .or_insert_with(|| vars.push(name.to_string(), VarKind::Node))
        };

        // Pass 1: node variables in order of first appearance,
        // branch variables per device, the name→branch map (F/H
        // control references), state-slot bases (`CKTnumStates`) and
        // the inductor info the mutuals need.
        let mut branches: HashMap<String, Var> = HashMap::new();
        let mut st = crate::state::StateAlloc::new();
        let mut state_base: Vec<usize> = Vec::with_capacity(self.specs.len());
        let mut state_ranges: Vec<(usize, usize)> = Vec::with_capacity(self.specs.len());
        let mut inductors: HashMap<String, (usize, f64)> = HashMap::new(); // name → (flux, L)
        for spec in &self.specs {
            let first = st.total();
            state_base.push(match spec {
                Spec::Bjt(_) => st.alloc(crate::bjt::BJT_NUM_STATES),
                Spec::Mos1(_) => st.alloc(crate::mos1::MOS1_NUM_STATES),
                Spec::Mos2(_) => st.alloc(crate::mos2::MOS2_NUM_STATES),
                Spec::Mos3(_) => st.alloc(crate::mos3::MOS3_NUM_STATES),
                Spec::Mos6(_) => st.alloc(crate::mos6::MOS6_NUM_STATES),
                Spec::Bsim1(_) => st.alloc(crate::bsim1::BSIM1_NUM_STATES),
                Spec::Bsim2(_) => st.alloc(crate::bsim2::BSIM2_NUM_STATES),
                Spec::Bsim3(_) => st.alloc(crate::bsim3::BSIM3_NUM_STATES),
                Spec::Capacitor(_) => st.alloc(2),
                Spec::Inductor(s) => {
                    let flux = st.alloc(2);
                    inductors.insert(s.name.clone(), (flux, s.value));
                    flux
                }
                Spec::Diode(_) => st.alloc(5),
                Spec::Jfet(_) => st.alloc(crate::jfet::FET3_NUM_STATES),
                Spec::Mes(_) => st.alloc(crate::mes::MES_NUM_STATES),
                Spec::SwitchV(_) | Spec::SwitchI(_) => st.alloc(crate::switch::SW_NUM_STATES),
                _ => 0,
            });
            state_ranges.push((first, st.total()));
            let node_names: &[String] = match spec {
                Spec::Bjt(s) => &s.nodes,
                Spec::Mos1(s) => &s.nodes,
                Spec::Mos2(s) => &s.nodes,
                Spec::Mos3(s) => &s.nodes,
                Spec::Mos6(s) => &s.nodes,
                Spec::Bsim1(s) => &s.nodes,
                Spec::Bsim2(s) => &s.nodes,
                Spec::Bsim3(s) => &s.nodes,
                Spec::Resistor(s) => &s.nodes,
                Spec::Capacitor(s) => &s.nodes,
                Spec::Inductor(s) => &s.nodes,
                Spec::Mutual(_) => &[],
                Spec::Vsource(s) => &s.nodes,
                Spec::Isource(s) => &s.nodes,
                Spec::Vcvs(s) => &s.nodes,
                Spec::Vccs(s) => &s.nodes,
                Spec::Cccs(s) => &s.nodes,
                Spec::Ccvs(s) => &s.nodes,
                Spec::Diode(s) => &s.nodes,
                Spec::Jfet(s) => &s.nodes,
                Spec::Mes(s) => &s.nodes,
                Spec::Ltra(s) => &s.nodes,
                Spec::Tra(s) => &s.nodes,
                Spec::SwitchV(s) => &s.nodes,
                Spec::SwitchI(s) => &s.nodes,
            };
            for n in node_names {
                node(&mut vars, &mut node_ids, n);
            }
            match spec {
                Spec::Vsource(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                    // `VSRCsetup` (RFSPICE): a port's internal node
                    // behind the reference impedance, made after the
                    // branch with `CKTmkVolt` (`v1#res`).
                    if s.port.is_some_and(|p| p.is_port()) {
                        node(&mut vars, &mut node_ids, &format!("{}#res", s.name));
                    }
                }
                Spec::Inductor(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                Spec::Vcvs(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                Spec::Ccvs(s) => {
                    let br = vars.push(s.name.clone(), VarKind::Branch);
                    branches.insert(s.name.clone(), br);
                }
                // `LTRAsetup`: the two port-current equations, made
                // with `CKTmkVolt` — voltage-typed unknowns for the
                // convergence test and the rawfile (`v(o1#i1)`).
                Spec::Ltra(s) => {
                    node(&mut vars, &mut node_ids, &format!("{}#i1", s.name));
                    node(&mut vars, &mut node_ids, &format!("{}#i2", s.name));
                }
                // `TRAsetup`: both branch equations, then both
                // internal nodes, all with `CKTmkVolt`.
                Spec::Tra(s) => {
                    for suffix in ["i1", "i2", "int1", "int2"] {
                        node(&mut vars, &mut node_ids, &format!("{}#{suffix}", s.name));
                    }
                }
                // `DIOsetup`: internal node past RS, named
                // `<name>#internal` like `CKTmkVolt` does.
                Spec::Diode(s) if s.model.rs != 0.0 => {
                    node(&mut vars, &mut node_ids, &format!("{}#internal", s.name));
                }
                // `JFETsetup`/`MESsetup`: source prime first, then
                // drain prime (`<name>#source`, `<name>#drain`).
                Spec::Jfet(s) => {
                    if s.model.rs != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                    if s.model.rd != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                }
                Spec::Mes(s) => {
                    if s.model.rs != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                    if s.model.rd != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                }
                // `BJTsetup`: one prime node per nonzero parasitic
                // resistance (`q1#collector` etc., `CKTmkVolt`).
                Spec::Bjt(s) => {
                    if s.model.rc != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#collector", s.name));
                    }
                    if s.model.rb != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#base", s.name));
                    }
                    if s.model.re != 0.0 {
                        node(&mut vars, &mut node_ids, &format!("{}#emitter", s.name));
                    }
                }
                // `MOS1setup`: prime nodes past the series
                // resistance (`m1#drain`/`m1#source`, `CKTmkVolt`).
                Spec::Mos1(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                }
                Spec::Mos2(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(
                            &mut vars,
                            &mut node_ids,
                            &format!("{}#internal#drain", s.name),
                        );
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(
                            &mut vars,
                            &mut node_ids,
                            &format!("{}#internal#source", s.name),
                        );
                    }
                }
                Spec::Mos3(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(
                            &mut vars,
                            &mut node_ids,
                            &format!("{}#internal#drain", s.name),
                        );
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(
                            &mut vars,
                            &mut node_ids,
                            &format!("{}#internal#source", s.name),
                        );
                    }
                }
                // `MOS6setup` names them like MOS1's (`m1#drain`).
                Spec::Mos6(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                }
                // `B1setup`/`B2setup` name them like MOS1's too.
                Spec::Bsim1(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                }
                Spec::Bsim2(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                }
                // `BSIM3setup`: `CKTmkVolt(…, "drain")` / `"source"`.
                Spec::Bsim3(s) => {
                    if s.model.needs_prime(s.nrd) {
                        node(&mut vars, &mut node_ids, &format!("{}#drain", s.name));
                    }
                    if s.model.needs_prime(s.nrs) {
                        node(&mut vars, &mut node_ids, &format!("{}#source", s.name));
                    }
                }
                _ => {}
            }
        }
        let num_states = st.total();

        // Pass 2: instances.
        let mut devices: Vec<Box<dyn Device>> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut sources: HashMap<String, (usize, bool)> = HashMap::new();
        let mut ports: Vec<PortInfo> = Vec::new();
        let mut notes = Vec::new();
        let get = |node_ids: &HashMap<String, Var>, n: &str| node_ids[n];
        // `BSIM3temp`'s per-model knot list: models (as given) with
        // their set-up copy, temperature part and the size-dependent
        // parameter sets already built, keyed by drawn (L, W).
        let mut bsim3_models: Vec<crate::bsim3::Bsim3Knots> = Vec::new();
        for (spec, &base) in self.specs.iter().zip(&state_base) {
            names.push(spec.name().to_string());
            match spec {
                Spec::Bjt(s) => {
                    let col = get(&node_ids, &s.nodes[0]);
                    let bse = get(&node_ids, &s.nodes[1]);
                    let emi = get(&node_ids, &s.nodes[2]);
                    let sub = get(&node_ids, &s.nodes[3]);
                    let prime = |rz: f64, suffix: &str, outer: Var| {
                        if rz != 0.0 {
                            node_ids[&format!("{}#{suffix}", s.name)]
                        } else {
                            outer
                        }
                    };
                    let primes = [
                        prime(s.model.rc, "collector", col),
                        prime(s.model.rb, "base", bse),
                        prime(s.model.re, "emitter", emi),
                    ];
                    let temp = s.temp.unwrap_or(opts.temp);
                    let bt = crate::bjt::BjtTemp::new(&s.name, &s.model, temp, opts.tnom);
                    notes.extend(bt.notes.iter().cloned());
                    devices.push(Box::new(crate::bjt::BjtI::new(
                        [col, bse, emi, sub],
                        primes,
                        bt,
                        s.area,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Mos1(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#source", s.name)]
                    } else {
                        so
                    };
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(opts.defl),
                        w: s.w.unwrap_or(opts.defw),
                        ad: s.ad.unwrap_or(opts.defad),
                        as_: s.as_.unwrap_or(opts.defas),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let mt = crate::mos1::Mos1Temp::new(&s.name, &s.model, &geom, temp, opts.tnom)
                        .map_err(Error::Build)?;
                    notes.extend(mt.notes.iter().cloned());
                    devices.push(Box::new(crate::mos1::Mos1I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        mt,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Mos2(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#internal#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#internal#source", s.name)]
                    } else {
                        so
                    };
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(opts.defl),
                        w: s.w.unwrap_or(opts.defw),
                        ad: s.ad.unwrap_or(opts.defad),
                        as_: s.as_.unwrap_or(opts.defas),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let mt = crate::mos2::Mos2Temp::new(&s.name, &s.model, &geom, temp, opts.tnom)
                        .map_err(Error::Build)?;
                    notes.extend(mt.notes.iter().cloned());
                    devices.push(Box::new(crate::mos2::Mos2I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        mt,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Mos3(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#internal#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#internal#source", s.name)]
                    } else {
                        so
                    };
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(opts.defl),
                        w: s.w.unwrap_or(opts.defw),
                        ad: s.ad.unwrap_or(opts.defad),
                        as_: s.as_.unwrap_or(opts.defas),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let mt = crate::mos3::Mos3Temp::new(&s.name, &s.model, &geom, temp, opts.tnom)
                        .map_err(Error::Build)?;
                    devices.push(Box::new(crate::mos3::Mos3I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        mt,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Mos6(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#source", s.name)]
                    } else {
                        so
                    };
                    // `mos6temp.c` forces zero squares back to 1.
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(opts.defl),
                        w: s.w.unwrap_or(opts.defw),
                        ad: s.ad.unwrap_or(opts.defad),
                        as_: s.as_.unwrap_or(opts.defas),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: if s.nrd == 0.0 { 1.0 } else { s.nrd },
                        nrs: if s.nrs == 0.0 { 1.0 } else { s.nrs },
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let mt = crate::mos6::Mos6Temp::new(&s.name, &s.model, &geom, temp, opts.tnom)
                        .map_err(Error::Build)?;
                    notes.extend(mt.notes.iter().cloned());
                    devices.push(Box::new(crate::mos6::Mos6I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        mt,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Bsim1(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#source", s.name)]
                    } else {
                        so
                    };
                    // `b1set.c`: L and W default to 5 µm, not the
                    // `.options` MOS defaults.
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(5e-6),
                        w: s.w.unwrap_or(5e-6),
                        ad: s.ad.unwrap_or(0.0),
                        as_: s.as_.unwrap_or(0.0),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                    };
                    let sz = crate::bsim1::Bsim1Size::new(&s.name, &s.model, &geom)
                        .map_err(Error::Build)?;
                    devices.push(Box::new(crate::bsim1::Bsim1I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        sz,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Bsim2(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#source", s.name)]
                    } else {
                        so
                    };
                    // `b2set.c`: L and W default to 5 µm, not the
                    // `.options` MOS defaults.
                    let geom = crate::mos1::MosGeom {
                        l: s.l.unwrap_or(5e-6),
                        w: s.w.unwrap_or(5e-6),
                        ad: s.ad.unwrap_or(0.0),
                        as_: s.as_.unwrap_or(0.0),
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                    };
                    let sz = crate::bsim2::Bsim2Size::new(&s.name, &s.model, &geom)
                        .map_err(Error::Build)?;
                    devices.push(Box::new(crate::bsim2::Bsim2I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        sz,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Bsim3(s) => {
                    let d = get(&node_ids, &s.nodes[0]);
                    let gt = get(&node_ids, &s.nodes[1]);
                    let so = get(&node_ids, &s.nodes[2]);
                    let b = get(&node_ids, &s.nodes[3]);
                    let dp = if s.model.needs_prime(s.nrd) {
                        node_ids[&format!("{}#drain", s.name)]
                    } else {
                        d
                    };
                    let sp = if s.model.needs_prime(s.nrs) {
                        node_ids[&format!("{}#source", s.name)]
                    } else {
                        so
                    };
                    let geom = crate::bsim3::Bsim3Geom {
                        l: s.l.unwrap_or(5e-6),
                        w: s.w.unwrap_or(5e-6),
                        ad: s.ad,
                        as_: s.as_,
                        pd: s.pd,
                        ps: s.ps,
                        nrd: s.nrd,
                        nrs: s.nrs,
                        m: s.m,
                        delvto: s.delvto,
                        mulu0: s.mulu0,
                    };
                    let (model, mt, pp) = crate::bsim3::Bsim3Knots::resolve(
                        &mut bsim3_models,
                        &s.name,
                        &s.model,
                        geom.l,
                        geom.w,
                        opts.temp,
                        opts.tnom,
                        &mut notes,
                    )
                    .map_err(Error::Build)?;
                    devices.push(Box::new(crate::bsim3::Bsim3I::new(
                        [d, gt, so, b],
                        [dp, sp],
                        model,
                        mt,
                        pp,
                        geom,
                        s.off,
                        s.ic,
                        base,
                    )));
                }
                Spec::Resistor(s) => {
                    let factor = self.res_temp.get(&s.name).map_or(1.0, |t| t.factor(opts));
                    devices.push(Box::new(ResistorI::with_factor(
                        s.value,
                        factor,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                    )));
                }
                Spec::Capacitor(s) => {
                    devices.push(Box::new(CapacitorI::new(
                        s.value,
                        s.ic,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        base,
                    )));
                }
                Spec::Inductor(s) => {
                    devices.push(Box::new(InductorI::new(
                        s.value,
                        s.ic.unwrap_or(0.0),
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        branches[&s.name],
                        base,
                    )));
                }
                Spec::Mutual(s) => {
                    for l in [&s.l1, &s.l2] {
                        if !inductors.contains_key(l) {
                            return Err(Error::Build(format!(
                                "`{}`: coupled inductor `{l}` does not exist",
                                s.name
                            )));
                        }
                    }
                    let (flux1, l1) = inductors[&s.l1];
                    let (flux2, l2) = inductors[&s.l2];
                    // `MUTfactor = k·sqrt(L1·L2)` (`mutsetup.c`).
                    devices.push(Box::new(MutualI::new(
                        s.k,
                        (l1 * l2).abs().sqrt(),
                        branches[&s.l1],
                        branches[&s.l2],
                        flux1,
                        flux2,
                    )));
                }
                Spec::Vsource(s) => {
                    sources.insert(s.name.clone(), (devices.len(), true));
                    let pos = get(&node_ids, &s.nodes[0]);
                    let neg = get(&node_ids, &s.nodes[1]);
                    let br = branches[&s.name];
                    let noise = NoiseWave::from_wave(s.wave.as_ref(), opts, &s.name)?;
                    let mut dev = VsourceI::new(s.dc, s.wave.clone(), s.ac, pos, neg, br)
                        .with_disto(s.distof1, s.distof2)
                        .with_noise(noise);
                    if let Some(p) = s.port.filter(RfPort::is_port) {
                        let res = get(&node_ids, &format!("{}#res", s.name));
                        dev = dev.with_port(res, p.z0);
                        ports.push(PortInfo {
                            name: s.name.clone(),
                            num: p.num,
                            z0: p.z0,
                            pos,
                            neg,
                            br,
                        });
                    }
                    devices.push(Box::new(dev));
                }
                Spec::Isource(s) => {
                    sources.insert(s.name.clone(), (devices.len(), false));
                    let noise = NoiseWave::from_wave(s.wave.as_ref(), opts, &s.name)?;
                    devices.push(Box::new(
                        IsourceI::new(
                            s.dc,
                            s.wave.clone(),
                            s.ac,
                            get(&node_ids, &s.nodes[0]),
                            get(&node_ids, &s.nodes[1]),
                        )
                        .with_disto(s.distof1, s.distof2)
                        .with_noise(noise),
                    ));
                }
                Spec::Vcvs(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    devices.push(Box::new(VcvsI::new(s.gain, n, branches[&s.name])));
                }
                Spec::Vccs(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    devices.push(Box::new(VccsI::new(s.gain, n)));
                }
                Spec::Cccs(s) => {
                    let ctrl = ctrl_branch(&branches, &s.name, &s.ctrl)?;
                    devices.push(Box::new(CccsI::new(
                        s.gain,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        ctrl,
                    )));
                }
                Spec::Ccvs(s) => {
                    let ctrl = ctrl_branch(&branches, &s.name, &s.ctrl)?;
                    devices.push(Box::new(CcvsI::new(
                        s.gain,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        branches[&s.name],
                        ctrl,
                    )));
                }
                Spec::Diode(s) => {
                    let pos = get(&node_ids, &s.nodes[0]);
                    let neg = get(&node_ids, &s.nodes[1]);
                    let pos_prime = if s.model.rs != 0.0 {
                        get(&node_ids, &format!("{}#internal", s.name))
                    } else {
                        pos
                    };
                    let temp = s.temp.unwrap_or(opts.temp);
                    let t = DiodeTemp::new(&s.name, &s.model, s.area, temp, opts.tnom, opts.reltol);
                    notes.extend(t.notes.iter().cloned());
                    devices.push(Box::new(crate::diode::DiodeI::new(
                        pos,
                        neg,
                        pos_prime,
                        t,
                        s.area,
                        s.off,
                        s.ic.unwrap_or(0.0),
                        base,
                        crate::diode::DiodeSensBase {
                            name: s.name.clone(),
                            model: s.model.clone(),
                            temp,
                            m: 1.0,
                            default_tnom: opts.tnom,
                            reltol: opts.reltol,
                        },
                    )));
                }
                Spec::Jfet(s) => {
                    let (nodes, primes) =
                        fet3_vars(&node_ids, &s.name, &s.nodes, s.model.rd, s.model.rs);
                    let temp = s.temp.unwrap_or(opts.temp);
                    let t = crate::jfet::JfetTemp::new(&s.name, &s.model, temp, opts.tnom);
                    notes.extend(t.notes.iter().cloned());
                    devices.push(Box::new(crate::jfet::JfetI::new(
                        nodes, primes, t, s.area, s.off, s.ic, base,
                    )));
                }
                Spec::Mes(s) => {
                    let (nodes, primes) =
                        fet3_vars(&node_ids, &s.name, &s.nodes, s.model.rd, s.model.rs);
                    let t = crate::mes::MesTemp::new(&s.model);
                    devices.push(Box::new(crate::mes::MesI::new(
                        nodes, primes, t, s.area, s.off, s.ic, base,
                    )));
                }
                Spec::Ltra(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    let br = [
                        get(&node_ids, &format!("{}#i1", s.name)),
                        get(&node_ids, &format!("{}#i2", s.name)),
                    ];
                    let p =
                        crate::ltra::LtraParams::new(&s.name, &s.model, opts.reltol, opts.abstol)
                            .map_err(Error::Build)?;
                    notes.extend(p.notes.iter().cloned());
                    devices.push(Box::new(
                        crate::ltra::LtraI::new(p, n, br).with_fit(opts.ltra_fit),
                    ));
                }
                Spec::Tra(s) => {
                    let n = [
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        get(&node_ids, &s.nodes[2]),
                        get(&node_ids, &s.nodes[3]),
                    ];
                    let v = |suffix: &str| get(&node_ids, &format!("{}#{suffix}", s.name));
                    let p = crate::tra::TraParams::new(&s.name, s.z0, s.td, s.f, s.nl, s.ic)
                        .map_err(Error::Build)?;
                    devices.push(Box::new(crate::tra::TraI::new(
                        p,
                        n,
                        [v("i1"), v("i2")],
                        [v("int1"), v("int2")],
                    )));
                }
                Spec::SwitchV(s) => {
                    let m = &s.model;
                    let p = crate::switch::SwitchParams::new(m.vt, m.vh, m.ron, m.roff, opts.gmin);
                    let ctrl = crate::switch::Control::Voltage {
                        pos: get(&node_ids, &s.nodes[2]),
                        neg: get(&node_ids, &s.nodes[3]),
                    };
                    devices.push(Box::new(crate::switch::SwitchInst::new(
                        p,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        ctrl,
                        s.on,
                        base,
                    )));
                }
                Spec::SwitchI(s) => {
                    // `CSWsetup`: "unknown controlling source".
                    let branch = ctrl_branch(&branches, &s.name, &s.ctrl)?;
                    let m = &s.model;
                    let p = crate::switch::SwitchParams::new(m.it, m.ih, m.ron, m.roff, opts.gmin);
                    devices.push(Box::new(crate::switch::SwitchInst::new(
                        p,
                        get(&node_ids, &s.nodes[0]),
                        get(&node_ids, &s.nodes[1]),
                        crate::switch::Control::Current { branch },
                        s.on,
                        base,
                    )));
                }
            }
        }

        // `*param.c`'s `*icXGiven` bits for `DEVsetic`.
        for (spec, dev) in self.specs.iter().zip(devices.iter_mut()) {
            if let Some(&g) = self.ic_given.get(spec.name()) {
                dev.set_ic_given(g);
            }
        }

        // Nodesets and ics must name existing nodes.
        let mut nodesets = Vec::new();
        for (name, v) in &self.nodesets {
            match node_ids.get(name) {
                Some(&var) if var != 0 => nodesets.push((var, *v)),
                Some(_) => notes.push(".nodeset on ground ignored".to_string()),
                None => notes.push(format!(".nodeset names unknown node `{name}`")),
            }
        }
        let mut ics = Vec::new();
        for (name, v) in &self.ics {
            match node_ids.get(name) {
                Some(&var) if var != 0 => ics.push((var, *v)),
                Some(_) => notes.push(".ic on ground ignored".to_string()),
                None => notes.push(format!(".ic names unknown node `{name}`")),
            }
        }

        // The mutual inductor adds no matrix row of its own but
        // keeps `devices` and `names` aligned by construction.
        let ports = check_ports(ports)?;
        Ok(Built {
            vars,
            devices,
            names,
            nodesets,
            ics,
            sources,
            ports,
            num_states,
            state_ranges,
            notes,
            warm: self.warm.share(),
        })
    }
}

/// `VSRCtemp`'s port checks (ngspice-47, RFSPICE): with N ports,
/// every index must lie in `1..=N` and none may repeat, so the ports
/// are exactly `1..=N`. The C walks the instances newest first (the
/// instance list is built by prepending), so the reported instance
/// is the last offending card. Returns the ports sorted by index
/// (`CKTrfPorts`).
fn check_ports(mut ports: Vec<PortInfo>) -> Result<Vec<PortInfo>, Error> {
    let count = ports.len();
    let mut seen = vec![false; count + 1];
    for p in ports.iter().rev() {
        let n = p.num as usize;
        if n > count {
            return Err(Error::Build(format!("{}: incorrect port ordering", p.name)));
        }
        if std::mem::replace(&mut seen[n], true) {
            return Err(Error::Build(format!("{}: duplicate port Index", p.name)));
        }
    }
    ports.sort_by_key(|p| p.num);
    Ok(ports)
}

/// Terminal and prime-node variables of a JFET/MESFET.
fn fet3_vars(
    node_ids: &HashMap<String, Var>,
    name: &str,
    nodes: &[String; 3],
    rd: f64,
    rs: f64,
) -> ([Var; 3], [Var; 2]) {
    let d = node_ids[&nodes[0]];
    let g = node_ids[&nodes[1]];
    let s = node_ids[&nodes[2]];
    let dp = if rd != 0.0 {
        node_ids[&format!("{name}#drain")]
    } else {
        d
    };
    let sp = if rs != 0.0 {
        node_ids[&format!("{name}#source")]
    } else {
        s
    };
    ([d, g, s], [dp, sp])
}

fn ctrl_branch(branches: &HashMap<String, Var>, name: &str, ctrl: &str) -> Result<Var, Error> {
    if !ctrl.starts_with('v') {
        return Err(Error::Build(format!(
            "`{name}`: controlling source `{ctrl}` must be a voltage source"
        )));
    }
    branches
        .get(ctrl)
        .copied()
        .ok_or_else(|| Error::Build(format!("`{name}`: controlling source `{ctrl}` not found")))
}
