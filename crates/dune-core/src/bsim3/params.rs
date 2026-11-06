//! The BSIM3v3.3.0 model-parameter table (`b3.c` `BSIM3mPTable`,
//! `b3mpar.c`) and the model-level defaulting of `BSIM3setup`
//! (`b3set.c`). Generated from the ngspice-47 sources: one entry per
//! real-valued model field, its deck keywords (aliases share the
//! field, like the C's shared `BSIM3_MOD_*` codes) and the constant
//! default `b3set.c` assigns when the parameter is not given
//! (`None`: no default, or one computed in [`Bsim3Model::setup`]).

/// Declares [`Bsim3Model`]'s real parameters, the given-flags and
/// the keyword setter/getter from one table.
macro_rules! bsim3_params {
    ($( $(#[$doc:meta])* $field:ident : $($kw:literal)|+ = $def:expr ; )*) => {
        /// `.model … nmos/pmos(level=8|49 …)`: a BSIM3v3.3.0 model
        /// card, ngspice-47's (`bsim3def.h` `sBSIM3model`). Parameters
        /// are set by deck keyword through [`Bsim3Model::set`]; like
        /// the C, every parameter remembers whether it was given,
        /// since `b3set.c`/`b3temp.c` branch on that.
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct Bsim3Model {
            pub(crate) nmos: bool,
            pub(crate) cap_mod: i32,
            pub(crate) mob_mod: i32,
            pub(crate) noi_mod: i32,
            pub(crate) nqs_mod: i32,
            pub(crate) acnqs_mod: i32,
            pub(crate) acm_mod: i32,
            pub(crate) calcacm: i32,
            pub(crate) param_chk: i32,
            pub(crate) bin_unit: i32,
            pub(crate) version: Option<String>,
            /// `BSIM3cox = EPSOX / tox` (set up by `b3set.c`).
            pub(crate) cox: f64,
            $( $(#[$doc])* pub(crate) $field: f64, )*
            pub(crate) given: Bsim3Given,
        }

        /// The C's `BSIM3xxxGiven` flags.
        #[derive(Clone, Debug, Default, PartialEq)]
        pub(crate) struct Bsim3Given {
            pub cap_mod: bool,
            pub mob_mod: bool,
            pub noi_mod: bool,
            pub nqs_mod: bool,
            pub acnqs_mod: bool,
            pub acm_mod: bool,
            pub calcacm: bool,
            pub param_chk: bool,
            pub bin_unit: bool,
            $( pub $field: bool, )*
        }

        impl Bsim3Model {
            /// `BSIM3mParam` for the `IF_REAL` codes.
            fn set_real(&mut self, kw: &str, v: f64) -> bool {
                match kw {
                    $( $($kw)|+ => {
                        self.$field = v;
                        self.given.$field = true;
                    } )*
                    _ => return false,
                }
                true
            }

            /// `BSIM3mAsk` for the `IF_REAL` codes.
            fn get_real(&self, kw: &str) -> Option<f64> {
                match kw {
                    $( $($kw)|+ => Some(self.$field), )*
                    _ => None,
                }
            }

            /// The constant `if (!given) x = default;` lines of
            /// `BSIM3setup`.
            fn constant_defaults(&mut self) {
                $( {
                    let d: Option<f64> = $def;
                    if let Some(d) = d && !self.given.$field {
                        self.$field = d;
                    }
                } )*
            }
        }
    };
}

bsim3_params! {
    /// Gate oxide thickness in meters (`BSIM3tox`)
    tox: "tox" = Some(150.0e-10);
    /// Gate oxide thickness used in extraction (`BSIM3toxm`)
    toxm: "toxm" = None;
    /// Drain/Source and channel coupling capacitance (`BSIM3cdsc`)
    cdsc: "cdsc" = Some(2.4e-4);
    /// Body-bias dependence of cdsc (`BSIM3cdscb`)
    cdscb: "cdscb" = Some(0.0);
    /// Drain-bias dependence of cdsc (`BSIM3cdscd`)
    cdscd: "cdscd" = Some(0.0);
    /// Interface state capacitance (`BSIM3cit`)
    cit: "cit" = Some(0.0);
    /// Subthreshold swing Coefficient (`BSIM3nfactor`)
    nfactor: "nfactor" = Some(1.0);
    /// Junction depth in meters (`BSIM3xj`)
    xj: "xj" = Some(0.15e-6);
    /// Saturation velocity at tnom (`BSIM3vsat`)
    vsat: "vsat" = Some(8.0e4);
    /// Temperature coefficient of vsat (`BSIM3at`)
    at: "at" = Some(3.3e4);
    /// Non-uniform depletion width effect coefficient. (`BSIM3a0`)
    a0: "a0" = Some(1.0);
    /// Gate bias  coefficient of Abulk. (`BSIM3ags`)
    ags: "ags" = Some(0.0);
    /// Non-saturation effect coefficient (`BSIM3a1`)
    a1: "a1" = Some(0.0);
    /// Non-saturation effect coefficient (`BSIM3a2`)
    a2: "a2" = Some(1.0);
    /// Body-bias coefficient of non-uniform depletion width effect. (`BSIM3keta`)
    keta: "keta" = Some(-0.047);
    /// Substrate doping concentration (`BSIM3nsub`)
    nsub: "nsub" = Some(6.0e16);
    /// Channel doping concentration (`BSIM3npeak`)
    npeak: "nch" = Some(1.7e17);
    /// Poly-gate doping concentration (`BSIM3ngate`)
    ngate: "ngate" = Some(0.0);
    /// Vth body coefficient (`BSIM3gamma1`)
    gamma1: "gamma1" = None;
    /// Vth body coefficient (`BSIM3gamma2`)
    gamma2: "gamma2" = None;
    /// Vth transition body Voltage (`BSIM3vbx`)
    vbx: "vbx" = None;
    /// Maximum body voltage (`BSIM3vbm`)
    vbm: "vbm" = Some(-3.0);
    /// Doping depth (`BSIM3xt`)
    xt: "xt" = Some(1.55e-7);
    /// Bulk effect coefficient 1 (`BSIM3k1`)
    k1: "k1" = None;
    /// Temperature coefficient of Vth (`BSIM3kt1`)
    kt1: "kt1" = Some(-0.11);
    /// Temperature coefficient of Vth (`BSIM3kt1l`)
    kt1l: "kt1l" = Some(0.0);
    /// Body-coefficient of kt1 (`BSIM3kt2`)
    kt2: "kt2" = Some(0.022);
    /// Bulk effect coefficient 2 (`BSIM3k2`)
    k2: "k2" = None;
    /// Narrow width effect coefficient (`BSIM3k3`)
    k3: "k3" = Some(80.0);
    /// Body effect coefficient of k3 (`BSIM3k3b`)
    k3b: "k3b" = Some(0.0);
    /// Narrow width effect parameter (`BSIM3w0`)
    w0: "w0" = Some(2.5e-6);
    /// Lateral non-uniform doping effect (`BSIM3nlx`)
    nlx: "nlx" = Some(1.74e-7);
    /// Short channel effect coeff. 0 (`BSIM3dvt0`)
    dvt0: "dvt0" = Some(2.2);
    /// Short channel effect coeff. 1 (`BSIM3dvt1`)
    dvt1: "dvt1" = Some(0.53);
    /// Short channel effect coeff. 2 (`BSIM3dvt2`)
    dvt2: "dvt2" = Some(-0.032);
    /// Narrow Width coeff. 0 (`BSIM3dvt0w`)
    dvt0w: "dvt0w" = Some(0.0);
    /// Narrow Width effect coeff. 1 (`BSIM3dvt1w`)
    dvt1w: "dvt1w" = Some(5.3e6);
    /// Narrow Width effect coeff. 2 (`BSIM3dvt2w`)
    dvt2w: "dvt2w" = Some(-0.032);
    /// DIBL coefficient of output resistance (`BSIM3drout`)
    drout: "drout" = Some(0.56);
    /// DIBL coefficient in the subthreshold region (`BSIM3dsub`)
    dsub: "dsub" = None;
    /// Threshold voltage (`BSIM3vth0`)
    vth0: "vth0" | "vtho" = None;
    /// Linear gate dependence of mobility (`BSIM3ua`)
    ua: "ua" = Some(2.25e-9);
    /// Temperature coefficient of ua (`BSIM3ua1`)
    ua1: "ua1" = Some(4.31e-9);
    /// Quadratic gate dependence of mobility (`BSIM3ub`)
    ub: "ub" = Some(5.87e-19);
    /// Temperature coefficient of ub (`BSIM3ub1`)
    ub1: "ub1" = Some(-7.61e-18);
    /// Body-bias dependence of mobility (`BSIM3uc`)
    uc: "uc" = None;
    /// Temperature coefficient of uc (`BSIM3uc1`)
    uc1: "uc1" = None;
    /// Low-field mobility at Tnom (`BSIM3u0`)
    u0: "u0" = None;
    /// Temperature coefficient of mobility (`BSIM3ute`)
    ute: "ute" = Some(-1.5);
    /// Threshold voltage offset (`BSIM3voff`)
    voff: "voff" = Some(-0.08);
    /// Parameter measurement temperature (`BSIM3tnom`)
    tnom: "tnom" = None;
    /// Gate-source overlap capacitance per width (`BSIM3cgso`)
    cgso: "cgso" = None;
    /// Gate-drain overlap capacitance per width (`BSIM3cgdo`)
    cgdo: "cgdo" = None;
    /// Gate-bulk overlap capacitance per length (`BSIM3cgbo`)
    cgbo: "cgbo" = None;
    /// Channel charge partitioning (`BSIM3xpart`)
    xpart: "xpart" = Some(0.0);
    /// Non-quasi-static Elmore Constant Parameter (`BSIM3elm`)
    elm: "elm" = Some(5.0);
    /// Effective Vds parameter (`BSIM3delta`)
    delta: "delta" = Some(0.01);
    /// Source-drain sheet resistance (`BSIM3sheetResistance`)
    sheet_resistance: "rsh" = Some(0.0);
    /// Source-drain resistance per width (`BSIM3rdsw`)
    rdsw: "rdsw" = Some(0.0);
    /// Gate-bias effect on parasitic resistance (`BSIM3prwg`)
    prwg: "prwg" = Some(0.0);
    /// Body-effect on parasitic resistance (`BSIM3prwb`)
    prwb: "prwb" = Some(0.0);
    /// Temperature coefficient of parasitic resistance (`BSIM3prt`)
    prt: "prt" = Some(0.0);
    /// Subthreshold region DIBL coefficient (`BSIM3eta0`)
    eta0: "eta0" = Some(0.08);
    /// Subthreshold region DIBL coefficient (`BSIM3etab`)
    etab: "etab" = Some(-0.07);
    /// Channel length modulation Coefficient (`BSIM3pclm`)
    pclm: "pclm" = Some(1.3);
    /// Drain-induced barrier lowering coefficient (`BSIM3pdibl1`)
    pdibl1: "pdiblc1" = Some(0.39);
    /// Drain-induced barrier lowering coefficient (`BSIM3pdibl2`)
    pdibl2: "pdiblc2" = Some(0.0086);
    /// Body-effect on drain-induced barrier lowering (`BSIM3pdiblb`)
    pdiblb: "pdiblcb" = Some(0.0);
    /// Substrate current body-effect coefficient (`BSIM3pscbe1`)
    pscbe1: "pscbe1" = Some(4.24e8);
    /// Substrate current body-effect coefficient (`BSIM3pscbe2`)
    pscbe2: "pscbe2" = Some(1.0e-5);
    /// Gate dependence of output resistance parameter (`BSIM3pvag`)
    pvag: "pvag" = Some(0.0);
    /// Source/drain junction reverse saturation current density (`BSIM3jctSatCurDensity`)
    jct_sat_cur_density: "js" = Some(1.0E-4);
    /// Sidewall junction reverse saturation current density (`BSIM3jctSidewallSatCurDensity`)
    jct_sidewall_sat_cur_density: "jsw" = Some(0.0);
    /// Source/drain junction built-in potential (`BSIM3bulkJctPotential`)
    bulk_jct_potential: "pb" = Some(1.0);
    /// Source/drain junction emission coefficient (`BSIM3jctEmissionCoeff`)
    jct_emission_coeff: "nj" = Some(1.0);
    /// Junction current temperature exponent (`BSIM3jctTempExponent`)
    jct_temp_exponent: "xti" = Some(3.0);
    /// Source/drain bottom junction capacitance grading coefficient (`BSIM3bulkJctBotGradingCoeff`)
    bulk_jct_bot_grading_coeff: "mj" = Some(0.5);
    /// Source/drain sidewall junction capacitance built in potential (`BSIM3sidewallJctPotential`)
    sidewall_jct_potential: "pbsw" = Some(1.0);
    /// Source/drain sidewall junction capacitance grading coefficient (`BSIM3bulkJctSideGradingCoeff`)
    bulk_jct_side_grading_coeff: "mjsw" = Some(0.33);
    /// Source/drain (gate side) sidewall junction capacitance built in potential (`BSIM3GatesidewallJctPotential`)
    gatesidewall_jct_potential: "pbswg" = None;
    /// Source/drain (gate side) sidewall junction capacitance grading coefficient (`BSIM3bulkJctGateSideGradingCoeff`)
    bulk_jct_gate_side_grading_coeff: "mjswg" = None;
    /// Source/drain bottom junction capacitance per unit area (`BSIM3unitAreaJctCap`)
    unit_area_jct_cap: "cj" = Some(5.0E-4);
    /// Flat Band Voltage parameter for capmod=0 only (`BSIM3vfbcv`)
    vfbcv: "vfbcv" = Some(-1.0);
    /// Flat Band Voltage (`BSIM3vfb`)
    vfb: "vfb" = None;
    /// Source/drain sidewall junction capacitance per unit periphery (`BSIM3unitLengthSidewallJctCap`)
    unit_length_sidewall_jct_cap: "cjsw" = Some(5.0E-10);
    /// Source/drain (gate side) sidewall junction capacitance per unit width (`BSIM3unitLengthGateSidewallJctCap`)
    unit_length_gate_sidewall_jct_cap: "cjswg" = None;
    /// Temperature coefficient of pb (`BSIM3tpb`)
    tpb: "tpb" = Some(0.0);
    /// Temperature coefficient of cj (`BSIM3tcj`)
    tcj: "tcj" = Some(0.0);
    /// Temperature coefficient of pbsw (`BSIM3tpbsw`)
    tpbsw: "tpbsw" = Some(0.0);
    /// Temperature coefficient of cjsw (`BSIM3tcjsw`)
    tcjsw: "tcjsw" = Some(0.0);
    /// Temperature coefficient of pbswg (`BSIM3tpbswg`)
    tpbswg: "tpbswg" = Some(0.0);
    /// Temperature coefficient of cjswg (`BSIM3tcjswg`)
    tcjswg: "tcjswg" = Some(0.0);
    /// Exponential coefficient for finite charge thickness (`BSIM3acde`)
    acde: "acde" = Some(1.0);
    /// Coefficient for gate-bias dependent surface potential (`BSIM3moin`)
    moin: "moin" = Some(15.0);
    /// C-V turn-on/off parameter (`BSIM3noff`)
    noff: "noff" = Some(1.0);
    /// C-V lateral-shift parameter (`BSIM3voffcv`)
    voffcv: "voffcv" = Some(0.0);
    /// lint offset for noise calculation (`BSIM3lintnoi`)
    lintnoi: "lintnoi" = Some(0.0);
    /// Length reduction parameter (`BSIM3Lint`)
    lint: "lint" = Some(0.0);
    /// Length reduction parameter (`BSIM3Ll`)
    ll: "ll" = Some(0.0);
    /// Length reduction parameter for CV (`BSIM3Llc`)
    llc: "llc" = None;
    /// Length reduction parameter (`BSIM3Lln`)
    lln: "lln" = Some(1.0);
    /// Length reduction parameter (`BSIM3Lw`)
    lw: "lw" = Some(0.0);
    /// Length reduction parameter for CV (`BSIM3Lwc`)
    lwc: "lwc" = None;
    /// Length reduction parameter (`BSIM3Lwn`)
    lwn: "lwn" = Some(1.0);
    /// Length reduction parameter (`BSIM3Lwl`)
    lwl: "lwl" = Some(0.0);
    /// Length reduction parameter for CV (`BSIM3Lwlc`)
    lwlc: "lwlc" = None;
    /// Minimum length for the model (`BSIM3Lmin`)
    lmin: "lmin" = Some(0.0);
    /// Maximum length for the model (`BSIM3Lmax`)
    lmax: "lmax" = Some(1.0);
    /// Length correction parameter (`BSIM3xl`)
    xl: "xl" = Some(0.0);
    /// Width correction parameter (`BSIM3xw`)
    xw: "xw" = Some(0.0);
    /// Width dependence of rds (`BSIM3wr`)
    wr: "wr" = Some(1.0);
    /// Width reduction parameter (`BSIM3Wint`)
    wint: "wint" = Some(0.0);
    /// Width reduction parameter (`BSIM3dwg`)
    dwg: "dwg" = Some(0.0);
    /// Width reduction parameter (`BSIM3dwb`)
    dwb: "dwb" = Some(0.0);
    /// Width reduction parameter (`BSIM3Wl`)
    wl: "wl" = Some(0.0);
    /// Width reduction parameter for CV (`BSIM3Wlc`)
    wlc: "wlc" = None;
    /// Width reduction parameter (`BSIM3Wln`)
    wln: "wln" = Some(1.0);
    /// Width reduction parameter (`BSIM3Ww`)
    ww: "ww" = Some(0.0);
    /// Width reduction parameter for CV (`BSIM3Wwc`)
    wwc: "wwc" = None;
    /// Width reduction parameter (`BSIM3Wwn`)
    wwn: "wwn" = Some(1.0);
    /// Width reduction parameter (`BSIM3Wwl`)
    wwl: "wwl" = Some(0.0);
    /// Width reduction parameter for CV (`BSIM3Wwlc`)
    wwlc: "wwlc" = None;
    /// Minimum width for the model (`BSIM3Wmin`)
    wmin: "wmin" = Some(0.0);
    /// Maximum width for the model (`BSIM3Wmax`)
    wmax: "wmax" = Some(1.0);
    /// Abulk narrow width parameter (`BSIM3b0`)
    b0: "b0" = Some(0.0);
    /// Abulk narrow width parameter (`BSIM3b1`)
    b1: "b1" = Some(0.0);
    /// New C-V model parameter (`BSIM3cgsl`)
    cgsl: "cgsl" = Some(0.0);
    /// New C-V model parameter (`BSIM3cgdl`)
    cgdl: "cgdl" = Some(0.0);
    /// New C-V model parameter (`BSIM3ckappa`)
    ckappa: "ckappa" = Some(0.6);
    /// Fringe capacitance parameter (`BSIM3cf`)
    cf: "cf" = None;
    /// Vdsat parameter for C-V model (`BSIM3clc`)
    clc: "clc" = Some(0.1e-6);
    /// Vdsat parameter for C-V model (`BSIM3cle`)
    cle: "cle" = Some(0.6);
    /// Delta W for C-V model (`BSIM3dwc`)
    dwc: "dwc" = None;
    /// Delta L for C-V model (`BSIM3dlc`)
    dlc: "dlc" = None;
    /// ACM Parameter: Distance Gate - contact (`BSIM3hdif`)
    hdif: "hdif" = Some(0.0);
    /// ACM Parameter: Length of LDD Gate-Source/Drain (`BSIM3ldif`)
    ldif: "ldif" = Some(0.0);
    /// ACM Parameter: Length of LDD under Gate (`BSIM3ld`)
    ld: "ld" = Some(0.0);
    /// ACM Parameter: Resistance of LDD drain side (`BSIM3rd`)
    rd: "rd" = Some(0.0);
    /// ACM Parameter: Resistance of LDD source side (`BSIM3rs`)
    rs: "rs" = Some(0.0);
    /// ACM Parameter: Resistance contact drain side (`BSIM3rdc`)
    rdc: "rdc" = Some(0.0);
    /// ACM Parameter: Resistance contact source side (`BSIM3rsc`)
    rsc: "rsc" = Some(0.0);
    /// ACM Parameter: Width shrink factor (`BSIM3wmlt`)
    wmlt: "wmlt" = Some(1.0);
    /// substrate current model parameter (`BSIM3alpha0`)
    alpha0: "alpha0" = Some(0.0);
    /// substrate current model parameter (`BSIM3alpha1`)
    alpha1: "alpha1" = Some(0.0);
    /// substrate current model parameter (`BSIM3beta0`)
    beta0: "beta0" = Some(30.0);
    /// Diode limiting current (`BSIM3ijth`)
    ijth: "ijth" = Some(0.1);
    /// Length dependence of cdsc (`BSIM3lcdsc`)
    lcdsc: "lcdsc" = Some(0.0);
    /// Length dependence of cdscb (`BSIM3lcdscb`)
    lcdscb: "lcdscb" = Some(0.0);
    /// Length dependence of cdscd (`BSIM3lcdscd`)
    lcdscd: "lcdscd" = Some(0.0);
    /// Length dependence of cit (`BSIM3lcit`)
    lcit: "lcit" = Some(0.0);
    /// Length dependence of nfactor (`BSIM3lnfactor`)
    lnfactor: "lnfactor" = Some(0.0);
    /// Length dependence of xj (`BSIM3lxj`)
    lxj: "lxj" = Some(0.0);
    /// Length dependence of vsat (`BSIM3lvsat`)
    lvsat: "lvsat" = Some(0.0);
    /// Length dependence of at (`BSIM3lat`)
    lat: "lat" = Some(0.0);
    /// Length dependence of a0 (`BSIM3la0`)
    la0: "la0" = Some(0.0);
    /// Length dependence of ags (`BSIM3lags`)
    lags: "lags" = Some(0.0);
    /// Length dependence of a1 (`BSIM3la1`)
    la1: "la1" = Some(0.0);
    /// Length dependence of a2 (`BSIM3la2`)
    la2: "la2" = Some(0.0);
    /// Length dependence of keta (`BSIM3lketa`)
    lketa: "lketa" = Some(0.0);
    /// Length dependence of nsub (`BSIM3lnsub`)
    lnsub: "lnsub" = Some(0.0);
    /// Length dependence of nch (`BSIM3lnpeak`)
    lnpeak: "lnch" = Some(0.0);
    /// Length dependence of ngate (`BSIM3lngate`)
    lngate: "lngate" = Some(0.0);
    /// Length dependence of gamma1 (`BSIM3lgamma1`)
    lgamma1: "lgamma1" = None;
    /// Length dependence of gamma2 (`BSIM3lgamma2`)
    lgamma2: "lgamma2" = None;
    /// Length dependence of vbx (`BSIM3lvbx`)
    lvbx: "lvbx" = None;
    /// Length dependence of vbm (`BSIM3lvbm`)
    lvbm: "lvbm" = Some(0.0);
    /// Length dependence of xt (`BSIM3lxt`)
    lxt: "lxt" = Some(0.0);
    /// Length dependence of k1 (`BSIM3lk1`)
    lk1: "lk1" = None;
    /// Length dependence of kt1 (`BSIM3lkt1`)
    lkt1: "lkt1" = Some(0.0);
    /// Length dependence of kt1l (`BSIM3lkt1l`)
    lkt1l: "lkt1l" = Some(0.0);
    /// Length dependence of kt2 (`BSIM3lkt2`)
    lkt2: "lkt2" = Some(0.0);
    /// Length dependence of k2 (`BSIM3lk2`)
    lk2: "lk2" = None;
    /// Length dependence of k3 (`BSIM3lk3`)
    lk3: "lk3" = Some(0.0);
    /// Length dependence of k3b (`BSIM3lk3b`)
    lk3b: "lk3b" = Some(0.0);
    /// Length dependence of w0 (`BSIM3lw0`)
    lw0: "lw0" = Some(0.0);
    /// Length dependence of nlx (`BSIM3lnlx`)
    lnlx: "lnlx" = Some(0.0);
    /// Length dependence of dvt0 (`BSIM3ldvt0`)
    ldvt0: "ldvt0" = Some(0.0);
    /// Length dependence of dvt1 (`BSIM3ldvt1`)
    ldvt1: "ldvt1" = Some(0.0);
    /// Length dependence of dvt2 (`BSIM3ldvt2`)
    ldvt2: "ldvt2" = Some(0.0);
    /// Length dependence of dvt0w (`BSIM3ldvt0w`)
    ldvt0w: "ldvt0w" = Some(0.0);
    /// Length dependence of dvt1w (`BSIM3ldvt1w`)
    ldvt1w: "ldvt1w" = Some(0.0);
    /// Length dependence of dvt2w (`BSIM3ldvt2w`)
    ldvt2w: "ldvt2w" = Some(0.0);
    /// Length dependence of drout (`BSIM3ldrout`)
    ldrout: "ldrout" = Some(0.0);
    /// Length dependence of dsub (`BSIM3ldsub`)
    ldsub: "ldsub" = Some(0.0);
    /// Length dependence of vth0 (`BSIM3lvth0`)
    lvth0: "lvth0" | "lvtho" = Some(0.0);
    /// Length dependence of ua (`BSIM3lua`)
    lua: "lua" = Some(0.0);
    /// Length dependence of ua1 (`BSIM3lua1`)
    lua1: "lua1" = Some(0.0);
    /// Length dependence of ub (`BSIM3lub`)
    lub: "lub" = Some(0.0);
    /// Length dependence of ub1 (`BSIM3lub1`)
    lub1: "lub1" = Some(0.0);
    /// Length dependence of uc (`BSIM3luc`)
    luc: "luc" = Some(0.0);
    /// Length dependence of uc1 (`BSIM3luc1`)
    luc1: "luc1" = Some(0.0);
    /// Length dependence of u0 (`BSIM3lu0`)
    lu0: "lu0" = Some(0.0);
    /// Length dependence of ute (`BSIM3lute`)
    lute: "lute" = Some(0.0);
    /// Length dependence of voff (`BSIM3lvoff`)
    lvoff: "lvoff" = Some(0.0);
    /// Length dependence of elm (`BSIM3lelm`)
    lelm: "lelm" = Some(0.0);
    /// Length dependence of delta (`BSIM3ldelta`)
    ldelta: "ldelta" = Some(0.0);
    /// Length dependence of rdsw (`BSIM3lrdsw`)
    lrdsw: "lrdsw" = Some(0.0);
    /// Length dependence of prwg (`BSIM3lprwg`)
    lprwg: "lprwg" = Some(0.0);
    /// Length dependence of prwb (`BSIM3lprwb`)
    lprwb: "lprwb" = Some(0.0);
    /// Length dependence of prt (`BSIM3lprt`)
    lprt: "lprt" = Some(0.0);
    /// Length dependence of eta0 (`BSIM3leta0`)
    leta0: "leta0" = Some(0.0);
    /// Length dependence of etab (`BSIM3letab`)
    letab: "letab" = Some(-0.0);
    /// Length dependence of pclm (`BSIM3lpclm`)
    lpclm: "lpclm" = Some(0.0);
    /// Length dependence of pdiblc1 (`BSIM3lpdibl1`)
    lpdibl1: "lpdiblc1" = Some(0.0);
    /// Length dependence of pdiblc2 (`BSIM3lpdibl2`)
    lpdibl2: "lpdiblc2" = Some(0.0);
    /// Length dependence of pdiblcb (`BSIM3lpdiblb`)
    lpdiblb: "lpdiblcb" = Some(0.0);
    /// Length dependence of pscbe1 (`BSIM3lpscbe1`)
    lpscbe1: "lpscbe1" = Some(0.0);
    /// Length dependence of pscbe2 (`BSIM3lpscbe2`)
    lpscbe2: "lpscbe2" = Some(0.0);
    /// Length dependence of pvag (`BSIM3lpvag`)
    lpvag: "lpvag" = Some(0.0);
    /// Length dependence of wr (`BSIM3lwr`)
    lwr: "lwr" = Some(0.0);
    /// Length dependence of dwg (`BSIM3ldwg`)
    ldwg: "ldwg" = Some(0.0);
    /// Length dependence of dwb (`BSIM3ldwb`)
    ldwb: "ldwb" = Some(0.0);
    /// Length dependence of b0 (`BSIM3lb0`)
    lb0: "lb0" = Some(0.0);
    /// Length dependence of b1 (`BSIM3lb1`)
    lb1: "lb1" = Some(0.0);
    /// Length dependence of cgsl (`BSIM3lcgsl`)
    lcgsl: "lcgsl" = Some(0.0);
    /// Length dependence of cgdl (`BSIM3lcgdl`)
    lcgdl: "lcgdl" = Some(0.0);
    /// Length dependence of ckappa (`BSIM3lckappa`)
    lckappa: "lckappa" = Some(0.0);
    /// Length dependence of cf (`BSIM3lcf`)
    lcf: "lcf" = Some(0.0);
    /// Length dependence of clc (`BSIM3lclc`)
    lclc: "lclc" = Some(0.0);
    /// Length dependence of cle (`BSIM3lcle`)
    lcle: "lcle" = Some(0.0);
    /// Length dependence of alpha0 (`BSIM3lalpha0`)
    lalpha0: "lalpha0" = Some(0.0);
    /// Length dependence of alpha1 (`BSIM3lalpha1`)
    lalpha1: "lalpha1" = Some(0.0);
    /// Length dependence of beta0 (`BSIM3lbeta0`)
    lbeta0: "lbeta0" = Some(0.0);
    /// Length dependence of vfbcv (`BSIM3lvfbcv`)
    lvfbcv: "lvfbcv" = Some(0.0);
    /// Length dependence of vfb (`BSIM3lvfb`)
    lvfb: "lvfb" = Some(0.0);
    /// Length dependence of acde (`BSIM3lacde`)
    lacde: "lacde" = Some(0.0);
    /// Length dependence of moin (`BSIM3lmoin`)
    lmoin: "lmoin" = Some(0.0);
    /// Length dependence of noff (`BSIM3lnoff`)
    lnoff: "lnoff" = Some(0.0);
    /// Length dependence of voffcv (`BSIM3lvoffcv`)
    lvoffcv: "lvoffcv" = Some(0.0);
    /// Width dependence of cdsc (`BSIM3wcdsc`)
    wcdsc: "wcdsc" = Some(0.0);
    /// Width dependence of cdscb (`BSIM3wcdscb`)
    wcdscb: "wcdscb" = Some(0.0);
    /// Width dependence of cdscd (`BSIM3wcdscd`)
    wcdscd: "wcdscd" = Some(0.0);
    /// Width dependence of cit (`BSIM3wcit`)
    wcit: "wcit" = Some(0.0);
    /// Width dependence of nfactor (`BSIM3wnfactor`)
    wnfactor: "wnfactor" = Some(0.0);
    /// Width dependence of xj (`BSIM3wxj`)
    wxj: "wxj" = Some(0.0);
    /// Width dependence of vsat (`BSIM3wvsat`)
    wvsat: "wvsat" = Some(0.0);
    /// Width dependence of at (`BSIM3wat`)
    wat: "wat" = Some(0.0);
    /// Width dependence of a0 (`BSIM3wa0`)
    wa0: "wa0" = Some(0.0);
    /// Width dependence of ags (`BSIM3wags`)
    wags: "wags" = Some(0.0);
    /// Width dependence of a1 (`BSIM3wa1`)
    wa1: "wa1" = Some(0.0);
    /// Width dependence of a2 (`BSIM3wa2`)
    wa2: "wa2" = Some(0.0);
    /// Width dependence of keta (`BSIM3wketa`)
    wketa: "wketa" = Some(0.0);
    /// Width dependence of nsub (`BSIM3wnsub`)
    wnsub: "wnsub" = Some(0.0);
    /// Width dependence of nch (`BSIM3wnpeak`)
    wnpeak: "wnch" = Some(0.0);
    /// Width dependence of ngate (`BSIM3wngate`)
    wngate: "wngate" = Some(0.0);
    /// Width dependence of gamma1 (`BSIM3wgamma1`)
    wgamma1: "wgamma1" = None;
    /// Width dependence of gamma2 (`BSIM3wgamma2`)
    wgamma2: "wgamma2" = None;
    /// Width dependence of vbx (`BSIM3wvbx`)
    wvbx: "wvbx" = None;
    /// Width dependence of vbm (`BSIM3wvbm`)
    wvbm: "wvbm" = Some(0.0);
    /// Width dependence of xt (`BSIM3wxt`)
    wxt: "wxt" = Some(0.0);
    /// Width dependence of k1 (`BSIM3wk1`)
    wk1: "wk1" = None;
    /// Width dependence of kt1 (`BSIM3wkt1`)
    wkt1: "wkt1" = Some(0.0);
    /// Width dependence of kt1l (`BSIM3wkt1l`)
    wkt1l: "wkt1l" = Some(0.0);
    /// Width dependence of kt2 (`BSIM3wkt2`)
    wkt2: "wkt2" = Some(0.0);
    /// Width dependence of k2 (`BSIM3wk2`)
    wk2: "wk2" = None;
    /// Width dependence of k3 (`BSIM3wk3`)
    wk3: "wk3" = Some(0.0);
    /// Width dependence of k3b (`BSIM3wk3b`)
    wk3b: "wk3b" = Some(0.0);
    /// Width dependence of w0 (`BSIM3ww0`)
    ww0: "ww0" = Some(0.0);
    /// Width dependence of nlx (`BSIM3wnlx`)
    wnlx: "wnlx" = Some(0.0);
    /// Width dependence of dvt0 (`BSIM3wdvt0`)
    wdvt0: "wdvt0" = Some(0.0);
    /// Width dependence of dvt1 (`BSIM3wdvt1`)
    wdvt1: "wdvt1" = Some(0.0);
    /// Width dependence of dvt2 (`BSIM3wdvt2`)
    wdvt2: "wdvt2" = Some(0.0);
    /// Width dependence of dvt0w (`BSIM3wdvt0w`)
    wdvt0w: "wdvt0w" = Some(0.0);
    /// Width dependence of dvt1w (`BSIM3wdvt1w`)
    wdvt1w: "wdvt1w" = Some(0.0);
    /// Width dependence of dvt2w (`BSIM3wdvt2w`)
    wdvt2w: "wdvt2w" = Some(0.0);
    /// Width dependence of drout (`BSIM3wdrout`)
    wdrout: "wdrout" = Some(0.0);
    /// Width dependence of dsub (`BSIM3wdsub`)
    wdsub: "wdsub" = Some(0.0);
    /// Width dependence of vth0 (`BSIM3wvth0`)
    wvth0: "wvth0" | "wvtho" = Some(0.0);
    /// Width dependence of ua (`BSIM3wua`)
    wua: "wua" = Some(0.0);
    /// Width dependence of ua1 (`BSIM3wua1`)
    wua1: "wua1" = Some(0.0);
    /// Width dependence of ub (`BSIM3wub`)
    wub: "wub" = Some(0.0);
    /// Width dependence of ub1 (`BSIM3wub1`)
    wub1: "wub1" = Some(0.0);
    /// Width dependence of uc (`BSIM3wuc`)
    wuc: "wuc" = Some(0.0);
    /// Width dependence of uc1 (`BSIM3wuc1`)
    wuc1: "wuc1" = Some(0.0);
    /// Width dependence of u0 (`BSIM3wu0`)
    wu0: "wu0" = Some(0.0);
    /// Width dependence of ute (`BSIM3wute`)
    wute: "wute" = Some(0.0);
    /// Width dependence of voff (`BSIM3wvoff`)
    wvoff: "wvoff" = Some(0.0);
    /// Width dependence of elm (`BSIM3welm`)
    welm: "welm" = Some(0.0);
    /// Width dependence of delta (`BSIM3wdelta`)
    wdelta: "wdelta" = Some(0.0);
    /// Width dependence of rdsw (`BSIM3wrdsw`)
    wrdsw: "wrdsw" = Some(0.0);
    /// Width dependence of prwg (`BSIM3wprwg`)
    wprwg: "wprwg" = Some(0.0);
    /// Width dependence of prwb (`BSIM3wprwb`)
    wprwb: "wprwb" = Some(0.0);
    /// Width dependence of prt (`BSIM3wprt`)
    wprt: "wprt" = Some(0.0);
    /// Width dependence of eta0 (`BSIM3weta0`)
    weta0: "weta0" = Some(0.0);
    /// Width dependence of etab (`BSIM3wetab`)
    wetab: "wetab" = Some(0.0);
    /// Width dependence of pclm (`BSIM3wpclm`)
    wpclm: "wpclm" = Some(0.0);
    /// Width dependence of pdiblc1 (`BSIM3wpdibl1`)
    wpdibl1: "wpdiblc1" = Some(0.0);
    /// Width dependence of pdiblc2 (`BSIM3wpdibl2`)
    wpdibl2: "wpdiblc2" = Some(0.0);
    /// Width dependence of pdiblcb (`BSIM3wpdiblb`)
    wpdiblb: "wpdiblcb" = Some(0.0);
    /// Width dependence of pscbe1 (`BSIM3wpscbe1`)
    wpscbe1: "wpscbe1" = Some(0.0);
    /// Width dependence of pscbe2 (`BSIM3wpscbe2`)
    wpscbe2: "wpscbe2" = Some(0.0);
    /// Width dependence of pvag (`BSIM3wpvag`)
    wpvag: "wpvag" = Some(0.0);
    /// Width dependence of wr (`BSIM3wwr`)
    wwr: "wwr" = Some(0.0);
    /// Width dependence of dwg (`BSIM3wdwg`)
    wdwg: "wdwg" = Some(0.0);
    /// Width dependence of dwb (`BSIM3wdwb`)
    wdwb: "wdwb" = Some(0.0);
    /// Width dependence of b0 (`BSIM3wb0`)
    wb0: "wb0" = Some(0.0);
    /// Width dependence of b1 (`BSIM3wb1`)
    wb1: "wb1" = Some(0.0);
    /// Width dependence of cgsl (`BSIM3wcgsl`)
    wcgsl: "wcgsl" = Some(0.0);
    /// Width dependence of cgdl (`BSIM3wcgdl`)
    wcgdl: "wcgdl" = Some(0.0);
    /// Width dependence of ckappa (`BSIM3wckappa`)
    wckappa: "wckappa" = Some(0.0);
    /// Width dependence of cf (`BSIM3wcf`)
    wcf: "wcf" = Some(0.0);
    /// Width dependence of clc (`BSIM3wclc`)
    wclc: "wclc" = Some(0.0);
    /// Width dependence of cle (`BSIM3wcle`)
    wcle: "wcle" = Some(0.0);
    /// Width dependence of alpha0 (`BSIM3walpha0`)
    walpha0: "walpha0" = Some(0.0);
    /// Width dependence of alpha1 (`BSIM3walpha1`)
    walpha1: "walpha1" = Some(0.0);
    /// Width dependence of beta0 (`BSIM3wbeta0`)
    wbeta0: "wbeta0" = Some(0.0);
    /// Width dependence of vfbcv (`BSIM3wvfbcv`)
    wvfbcv: "wvfbcv" = Some(0.0);
    /// Width dependence of vfb (`BSIM3wvfb`)
    wvfb: "wvfb" = Some(0.0);
    /// Width dependence of acde (`BSIM3wacde`)
    wacde: "wacde" = Some(0.0);
    /// Width dependence of moin (`BSIM3wmoin`)
    wmoin: "wmoin" = Some(0.0);
    /// Width dependence of noff (`BSIM3wnoff`)
    wnoff: "wnoff" = Some(0.0);
    /// Width dependence of voffcv (`BSIM3wvoffcv`)
    wvoffcv: "wvoffcv" = Some(0.0);
    /// Cross-term dependence of cdsc (`BSIM3pcdsc`)
    pcdsc: "pcdsc" = Some(0.0);
    /// Cross-term dependence of cdscb (`BSIM3pcdscb`)
    pcdscb: "pcdscb" = Some(0.0);
    /// Cross-term dependence of cdscd (`BSIM3pcdscd`)
    pcdscd: "pcdscd" = Some(0.0);
    /// Cross-term dependence of cit (`BSIM3pcit`)
    pcit: "pcit" = Some(0.0);
    /// Cross-term dependence of nfactor (`BSIM3pnfactor`)
    pnfactor: "pnfactor" = Some(0.0);
    /// Cross-term dependence of xj (`BSIM3pxj`)
    pxj: "pxj" = Some(0.0);
    /// Cross-term dependence of vsat (`BSIM3pvsat`)
    pvsat: "pvsat" = Some(0.0);
    /// Cross-term dependence of at (`BSIM3pat`)
    pat: "pat" = Some(0.0);
    /// Cross-term dependence of a0 (`BSIM3pa0`)
    pa0: "pa0" = Some(0.0);
    /// Cross-term dependence of ags (`BSIM3pags`)
    pags: "pags" = Some(0.0);
    /// Cross-term dependence of a1 (`BSIM3pa1`)
    pa1: "pa1" = Some(0.0);
    /// Cross-term dependence of a2 (`BSIM3pa2`)
    pa2: "pa2" = Some(0.0);
    /// Cross-term dependence of keta (`BSIM3pketa`)
    pketa: "pketa" = Some(0.0);
    /// Cross-term dependence of nsub (`BSIM3pnsub`)
    pnsub: "pnsub" = Some(0.0);
    /// Cross-term dependence of nch (`BSIM3pnpeak`)
    pnpeak: "pnch" = Some(0.0);
    /// Cross-term dependence of ngate (`BSIM3pngate`)
    pngate: "pngate" = Some(0.0);
    /// Cross-term dependence of gamma1 (`BSIM3pgamma1`)
    pgamma1: "pgamma1" = None;
    /// Cross-term dependence of gamma2 (`BSIM3pgamma2`)
    pgamma2: "pgamma2" = None;
    /// Cross-term dependence of vbx (`BSIM3pvbx`)
    pvbx: "pvbx" = None;
    /// Cross-term dependence of vbm (`BSIM3pvbm`)
    pvbm: "pvbm" = Some(0.0);
    /// Cross-term dependence of xt (`BSIM3pxt`)
    pxt: "pxt" = Some(0.0);
    /// Cross-term dependence of k1 (`BSIM3pk1`)
    pk1: "pk1" = None;
    /// Cross-term dependence of kt1 (`BSIM3pkt1`)
    pkt1: "pkt1" = Some(0.0);
    /// Cross-term dependence of kt1l (`BSIM3pkt1l`)
    pkt1l: "pkt1l" = Some(0.0);
    /// Cross-term dependence of kt2 (`BSIM3pkt2`)
    pkt2: "pkt2" = Some(0.0);
    /// Cross-term dependence of k2 (`BSIM3pk2`)
    pk2: "pk2" = None;
    /// Cross-term dependence of k3 (`BSIM3pk3`)
    pk3: "pk3" = Some(0.0);
    /// Cross-term dependence of k3b (`BSIM3pk3b`)
    pk3b: "pk3b" = Some(0.0);
    /// Cross-term dependence of w0 (`BSIM3pw0`)
    pw0: "pw0" = Some(0.0);
    /// Cross-term dependence of nlx (`BSIM3pnlx`)
    pnlx: "pnlx" = Some(0.0);
    /// Cross-term dependence of dvt0 (`BSIM3pdvt0`)
    pdvt0: "pdvt0" = Some(0.0);
    /// Cross-term dependence of dvt1 (`BSIM3pdvt1`)
    pdvt1: "pdvt1" = Some(0.0);
    /// Cross-term dependence of dvt2 (`BSIM3pdvt2`)
    pdvt2: "pdvt2" = Some(0.0);
    /// Cross-term dependence of dvt0w (`BSIM3pdvt0w`)
    pdvt0w: "pdvt0w" = Some(0.0);
    /// Cross-term dependence of dvt1w (`BSIM3pdvt1w`)
    pdvt1w: "pdvt1w" = Some(0.0);
    /// Cross-term dependence of dvt2w (`BSIM3pdvt2w`)
    pdvt2w: "pdvt2w" = Some(0.0);
    /// Cross-term dependence of drout (`BSIM3pdrout`)
    pdrout: "pdrout" = Some(0.0);
    /// Cross-term dependence of dsub (`BSIM3pdsub`)
    pdsub: "pdsub" = Some(0.0);
    /// Cross-term dependence of vth0 (`BSIM3pvth0`)
    pvth0: "pvth0" | "pvtho" = Some(0.0);
    /// Cross-term dependence of ua (`BSIM3pua`)
    pua: "pua" = Some(0.0);
    /// Cross-term dependence of ua1 (`BSIM3pua1`)
    pua1: "pua1" = Some(0.0);
    /// Cross-term dependence of ub (`BSIM3pub`)
    pub: "pub" = Some(0.0);
    /// Cross-term dependence of ub1 (`BSIM3pub1`)
    pub1: "pub1" = Some(0.0);
    /// Cross-term dependence of uc (`BSIM3puc`)
    puc: "puc" = Some(0.0);
    /// Cross-term dependence of uc1 (`BSIM3puc1`)
    puc1: "puc1" = Some(0.0);
    /// Cross-term dependence of u0 (`BSIM3pu0`)
    pu0: "pu0" = Some(0.0);
    /// Cross-term dependence of ute (`BSIM3pute`)
    pute: "pute" = Some(0.0);
    /// Cross-term dependence of voff (`BSIM3pvoff`)
    pvoff: "pvoff" = Some(0.0);
    /// Cross-term dependence of elm (`BSIM3pelm`)
    pelm: "pelm" = Some(0.0);
    /// Cross-term dependence of delta (`BSIM3pdelta`)
    pdelta: "pdelta" = Some(0.0);
    /// Cross-term dependence of rdsw (`BSIM3prdsw`)
    prdsw: "prdsw" = Some(0.0);
    /// Cross-term dependence of prwg (`BSIM3pprwg`)
    pprwg: "pprwg" = Some(0.0);
    /// Cross-term dependence of prwb (`BSIM3pprwb`)
    pprwb: "pprwb" = Some(0.0);
    /// Cross-term dependence of prt (`BSIM3pprt`)
    pprt: "pprt" = Some(0.0);
    /// Cross-term dependence of eta0 (`BSIM3peta0`)
    peta0: "peta0" = Some(0.0);
    /// Cross-term dependence of etab (`BSIM3petab`)
    petab: "petab" = Some(0.0);
    /// Cross-term dependence of pclm (`BSIM3ppclm`)
    ppclm: "ppclm" = Some(0.0);
    /// Cross-term dependence of pdiblc1 (`BSIM3ppdibl1`)
    ppdibl1: "ppdiblc1" = Some(0.0);
    /// Cross-term dependence of pdiblc2 (`BSIM3ppdibl2`)
    ppdibl2: "ppdiblc2" = Some(0.0);
    /// Cross-term dependence of pdiblcb (`BSIM3ppdiblb`)
    ppdiblb: "ppdiblcb" = Some(0.0);
    /// Cross-term dependence of pscbe1 (`BSIM3ppscbe1`)
    ppscbe1: "ppscbe1" = Some(0.0);
    /// Cross-term dependence of pscbe2 (`BSIM3ppscbe2`)
    ppscbe2: "ppscbe2" = Some(0.0);
    /// Cross-term dependence of pvag (`BSIM3ppvag`)
    ppvag: "ppvag" = Some(0.0);
    /// Cross-term dependence of wr (`BSIM3pwr`)
    pwr: "pwr" = Some(0.0);
    /// Cross-term dependence of dwg (`BSIM3pdwg`)
    pdwg: "pdwg" = Some(0.0);
    /// Cross-term dependence of dwb (`BSIM3pdwb`)
    pdwb: "pdwb" = Some(0.0);
    /// Cross-term dependence of b0 (`BSIM3pb0`)
    pb0: "pb0" = Some(0.0);
    /// Cross-term dependence of b1 (`BSIM3pb1`)
    pb1: "pb1" = Some(0.0);
    /// Cross-term dependence of cgsl (`BSIM3pcgsl`)
    pcgsl: "pcgsl" = Some(0.0);
    /// Cross-term dependence of cgdl (`BSIM3pcgdl`)
    pcgdl: "pcgdl" = Some(0.0);
    /// Cross-term dependence of ckappa (`BSIM3pckappa`)
    pckappa: "pckappa" = Some(0.0);
    /// Cross-term dependence of cf (`BSIM3pcf`)
    pcf: "pcf" = Some(0.0);
    /// Cross-term dependence of clc (`BSIM3pclc`)
    pclc: "pclc" = Some(0.0);
    /// Cross-term dependence of cle (`BSIM3pcle`)
    pcle: "pcle" = Some(0.0);
    /// Cross-term dependence of alpha0 (`BSIM3palpha0`)
    palpha0: "palpha0" = Some(0.0);
    /// Cross-term dependence of alpha1 (`BSIM3palpha1`)
    palpha1: "palpha1" = Some(0.0);
    /// Cross-term dependence of beta0 (`BSIM3pbeta0`)
    pbeta0: "pbeta0" = Some(0.0);
    /// Cross-term dependence of vfbcv (`BSIM3pvfbcv`)
    pvfbcv: "pvfbcv" = Some(0.0);
    /// Cross-term dependence of vfb (`BSIM3pvfb`)
    pvfb: "pvfb" = Some(0.0);
    /// Cross-term dependence of acde (`BSIM3pacde`)
    pacde: "pacde" = Some(0.0);
    /// Cross-term dependence of moin (`BSIM3pmoin`)
    pmoin: "pmoin" = Some(0.0);
    /// Cross-term dependence of noff (`BSIM3pnoff`)
    pnoff: "pnoff" = Some(0.0);
    /// Cross-term dependence of voffcv (`BSIM3pvoffcv`)
    pvoffcv: "pvoffcv" = Some(0.0);
    /// Flicker noise parameter (`BSIM3oxideTrapDensityA`)
    oxide_trap_density_a: "noia" = None;
    /// Flicker noise parameter (`BSIM3oxideTrapDensityB`)
    oxide_trap_density_b: "noib" = None;
    /// Flicker noise parameter (`BSIM3oxideTrapDensityC`)
    oxide_trap_density_c: "noic" = None;
    /// Flicker noise parameter (`BSIM3em`)
    em: "em" = Some(4.1e7);
    /// Flicker noise frequency exponent (`BSIM3ef`)
    ef: "ef" = Some(1.0);
    /// Flicker noise exponent (`BSIM3af`)
    af: "af" = Some(1.0);
    /// Flicker noise coefficient (`BSIM3kf`)
    kf: "kf" = Some(0.0);
    /// maximum voltage G-S branch (`BSIM3vgsMax`)
    vgs_max: "vgs_max" = Some(1e99);
    /// maximum voltage G-D branch (`BSIM3vgdMax`)
    vgd_max: "vgd_max" = Some(1e99);
    /// maximum voltage G-B branch (`BSIM3vgbMax`)
    vgb_max: "vgb_max" = Some(1e99);
    /// maximum voltage D-S branch (`BSIM3vdsMax`)
    vds_max: "vds_max" = Some(1e99);
    /// maximum voltage B-S branch (`BSIM3vbsMax`)
    vbs_max: "vbs_max" = Some(1e99);
    /// maximum voltage B-D branch (`BSIM3vbdMax`)
    vbd_max: "vbd_max" = Some(1e99);
    /// maximum voltage G-S branch (`BSIM3vgsrMax`)
    vgsr_max: "vgsr_max" = Some(1e99);
    /// maximum voltage G-D branch (`BSIM3vgdrMax`)
    vgdr_max: "vgdr_max" = Some(1e99);
    /// maximum voltage G-B branch (`BSIM3vgbrMax`)
    vgbr_max: "vgbr_max" = Some(1e99);
    /// maximum voltage B-S branch (`BSIM3vbsrMax`)
    vbsr_max: "vbsr_max" = Some(1e99);
    /// maximum voltage B-D branch (`BSIM3vbdrMax`)
    vbdr_max: "vbdr_max" = Some(1e99);

}

/// `EPSOX` of `b3set.c`/`b3temp.c`/`b3ld.c`.
pub(crate) const EPSOX: f64 = 3.453133e-11;
/// `EPSSI`.
pub(crate) const EPSSI: f64 = 1.03594e-10;
/// The `PI` of `b3set.c` (truncated, as in the C).
#[allow(clippy::approx_constant)]
const PI_B3: f64 = 3.141592654;

impl Bsim3Model {
    /// An empty card of the given polarity: nothing given, every
    /// default still to be applied by `BSIM3setup`.
    pub fn new(nmos: bool) -> Self {
        Bsim3Model {
            nmos,
            ..Default::default()
        }
    }

    /// `true` for an `nmos` card.
    pub fn is_nmos(&self) -> bool {
        self.nmos
    }

    /// Set one model parameter by its deck keyword (lowercase, the
    /// `b3.c` names and aliases: `vth0`/`vtho`, `nch`, `rsh`, …),
    /// marking it given (`BSIM3mParam`). The integer selectors
    /// (`capmod`, `mobmod`, `noimod`, `nqsmod`, `acnqsmod`, `acm`,
    /// `calcacm`, `paramchk`, `binunit`) truncate like `IF_INTEGER`;
    /// `tnom` is in °C. Unknown keywords are an error.
    pub fn set(&mut self, name: &str, value: f64) -> Result<(), String> {
        let iv = value as i32;
        let g = &mut self.given;
        match name {
            "capmod" => (self.cap_mod, g.cap_mod) = (iv, true),
            "mobmod" => (self.mob_mod, g.mob_mod) = (iv, true),
            "noimod" => (self.noi_mod, g.noi_mod) = (iv, true),
            "nqsmod" => (self.nqs_mod, g.nqs_mod) = (iv, true),
            "acnqsmod" => (self.acnqs_mod, g.acnqs_mod) = (iv, true),
            "acm" => (self.acm_mod, g.acm_mod) = (iv, true),
            "calcacm" => (self.calcacm, g.calcacm) = (iv, true),
            "paramchk" => (self.param_chk, g.param_chk) = (iv, true),
            "binunit" => (self.bin_unit, g.bin_unit) = (iv, true),
            // `b3mpar.c`: TNOM is stored in kelvin.
            "tnom" => {
                self.tnom = value + crate::consts::CTOK;
                self.given.tnom = true;
            }
            _ => {
                if !self.set_real(name, value) {
                    return Err(format!("unknown BSIM3 model parameter `{name}`"));
                }
            }
        }
        Ok(())
    }

    /// `VERSION=`: kept as given (only 3.3.x is implemented).
    pub fn set_version(&mut self, version: &str) {
        self.version = Some(version.to_string());
    }

    /// Read one real model parameter back by keyword (`tnom` in °C).
    pub fn get(&self, name: &str) -> Option<f64> {
        match name {
            "tnom" => Some(self.tnom - crate::consts::CTOK),
            _ => self.get_real(name),
        }
    }

    /// Model-level `BSIM3setup` (`b3set.c`): the defaults, in the C's
    /// order where one default reads another. `tnom` is
    /// `CKTnomTemp` (kelvin). Idempotent: only parameters not given
    /// are touched.
    pub(crate) fn setup(&mut self, tnom: f64) {
        let nmos = self.nmos;
        if !self.given.mob_mod {
            self.mob_mod = 1;
        }
        if !self.given.bin_unit {
            self.bin_unit = 1;
        }
        if !self.given.param_chk {
            self.param_chk = 0;
        }
        if !self.given.cap_mod {
            self.cap_mod = 3;
        }
        if !self.given.acm_mod {
            self.acm_mod = 0;
        }
        if !self.given.calcacm {
            self.calcacm = 0;
        }
        if !self.given.noi_mod {
            self.noi_mod = 1;
        }
        // nqsMod/acnqsMod outside {0, 1} fall back to 0 (with a
        // warning in the C).
        if !self.given.nqs_mod || (self.nqs_mod != 0 && self.nqs_mod != 1) {
            self.nqs_mod = 0;
        }
        if !self.given.acnqs_mod || (self.acnqs_mod != 0 && self.acnqs_mod != 1) {
            self.acnqs_mod = 0;
        }
        self.constant_defaults();
        let g = self.given.clone();
        self.cox = EPSOX / self.tox;
        if !g.toxm {
            self.toxm = self.tox;
        }
        if !g.dsub {
            self.dsub = self.drout;
        }
        if !g.vth0 {
            self.vth0 = if nmos { 0.7 } else { -0.7 };
        }
        if !g.uc {
            self.uc = if self.mob_mod == 3 { -0.0465 } else { -0.0465e-9 };
        }
        if !g.uc1 {
            self.uc1 = if self.mob_mod == 3 { -0.056 } else { -0.056e-9 };
        }
        if !g.u0 {
            self.u0 = if nmos { 0.067 } else { 0.025 };
        }
        if !g.tnom {
            self.tnom = tnom;
        }
        if !g.llc {
            self.llc = self.ll;
        }
        if !g.lwc {
            self.lwc = self.lw;
        }
        if !g.lwlc {
            self.lwlc = self.lwl;
        }
        if !g.wlc {
            self.wlc = self.wl;
        }
        if !g.wwc {
            self.wwc = self.ww;
        }
        if !g.wwlc {
            self.wwlc = self.wwl;
        }
        if !g.dwc {
            self.dwc = self.wint;
        }
        if !g.dlc {
            self.dlc = self.lint;
        }
        if !g.cf {
            self.cf = 2.0 * EPSOX / PI_B3 * f64::ln(1.0 + 0.4e-6 / self.tox);
        }
        if !g.cgdo {
            self.cgdo = if g.dlc && self.dlc > 0.0 {
                self.dlc * self.cox - self.cgdl
            } else {
                0.6 * self.xj * self.cox
            };
        }
        if !g.cgso {
            self.cgso = if g.dlc && self.dlc > 0.0 {
                self.dlc * self.cox - self.cgsl
            } else {
                0.6 * self.xj * self.cox
            };
        }
        if !g.cgbo {
            self.cgbo = 2.0 * self.dwc * self.cox;
        }
        if !g.unit_length_gate_sidewall_jct_cap {
            self.unit_length_gate_sidewall_jct_cap = self.unit_length_sidewall_jct_cap;
        }
        if !g.gatesidewall_jct_potential {
            self.gatesidewall_jct_potential = self.sidewall_jct_potential;
        }
        if !g.bulk_jct_gate_side_grading_coeff {
            self.bulk_jct_gate_side_grading_coeff = self.bulk_jct_side_grading_coeff;
        }
        if !g.oxide_trap_density_a {
            self.oxide_trap_density_a = if nmos { 1e20 } else { 9.9e18 };
        }
        if !g.oxide_trap_density_b {
            self.oxide_trap_density_b = if nmos { 5e4 } else { 2.4e3 };
        }
        if !g.oxide_trap_density_c {
            self.oxide_trap_density_c = if nmos { -1.4e-12 } else { 1.4e-12 };
        }
    }

    /// `BSIM3type`: +1 NMOS, −1 PMOS.
    pub(crate) fn type_(&self) -> f64 {
        if self.nmos { 1.0 } else { -1.0 }
    }

    /// Series resistances present (`BSIM3setup` makes the prime
    /// nodes when `rsh·squares != 0`, ACM off).
    pub(crate) fn needs_prime(&self, squares: f64) -> bool {
        self.sheet_resistance * squares != 0.0
    }
}
