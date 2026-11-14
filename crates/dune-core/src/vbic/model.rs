//! The VBIC model card (`vbic.c` keyword table, `vbicmpar.c`), the
//! model defaults of `VBICsetup` (`vbicsetup.c`) and the temperature
//! mapping `vbic_4T_et_cf_t` (`vbictemp.c`).

/// Number of entries of the C's parameter block `VBICtnom …
/// VBICrefVersion` (`vbicdefs.h`), which `VBICtemp` and `VBICload`
/// `memcpy` into their `p[108]` arrays.
pub(crate) const NUM_PARAMS: usize = 108;

// Indices into the parameter block the Rust side reads by name.
pub(crate) const P_RCX: usize = 1;
pub(crate) const P_RBX: usize = 6;
pub(crate) const P_RE: usize = 8;
pub(crate) const P_RS: usize = 9;
pub(crate) const P_IS: usize = 11;
pub(crate) const P_TD: usize = 62;
pub(crate) const P_KFN: usize = 63;
pub(crate) const P_AFN: usize = 64;
pub(crate) const P_BFN: usize = 65;
pub(crate) const P_RTH: usize = 83;
pub(crate) const P_CTH: usize = 84;
pub(crate) const P_DTEMP: usize = 105;

/// `R_MIN` of `vbicmpar.c`: a *given* resistance below it is raised
/// to it.
const R_MIN: f64 = 0.01;

/// The keyword table (`VBICmPTable`, aliases included) as
/// `(keyword, index into the parameter block)`, in the C's order.
const KEYWORDS: [(&str, usize); 116] = [
    ("tnom", 0),
    ("tref", 0),
    ("rcx", 1),
    ("rci", 2),
    ("vo", 3),
    ("v0", 3),
    ("gamm", 4),
    ("hrcf", 5),
    ("rbx", 6),
    ("rbi", 7),
    ("re", 8),
    ("rs", 9),
    ("rbp", 10),
    ("is", 11),
    ("nf", 12),
    ("nr", 13),
    ("fc", 14),
    ("cbeo", 15),
    ("cbe0", 15),
    ("cje", 16),
    ("pe", 17),
    ("me", 18),
    ("aje", 19),
    ("cbco", 20),
    ("cbc0", 20),
    ("cjc", 21),
    ("qco", 22),
    ("qc0", 22),
    ("cjep", 23),
    ("pc", 24),
    ("mc", 25),
    ("ajc", 26),
    ("cjcp", 27),
    ("ps", 28),
    ("ms", 29),
    ("ajs", 30),
    ("ibei", 31),
    ("wbe", 32),
    ("nei", 33),
    ("iben", 34),
    ("nen", 35),
    ("ibci", 36),
    ("nci", 37),
    ("ibcn", 38),
    ("ncn", 39),
    ("avc1", 40),
    ("avc2", 41),
    ("isp", 42),
    ("wsp", 43),
    ("nfp", 44),
    ("ibeip", 45),
    ("ibenp", 46),
    ("ibcip", 47),
    ("ncip", 48),
    ("ibcnp", 49),
    ("ncnp", 50),
    ("vef", 51),
    ("ver", 52),
    ("ikf", 53),
    ("ikr", 54),
    ("ikp", 55),
    ("tf", 56),
    ("qtf", 57),
    ("xtf", 58),
    ("vtf", 59),
    ("itf", 60),
    ("tr", 61),
    ("td", 62),
    ("kfn", 63),
    ("afn", 64),
    ("bfn", 65),
    ("xre", 66),
    ("xrbi", 67),
    ("xrci", 68),
    ("xrs", 69),
    ("xvo", 70),
    ("xv0", 70),
    ("ea", 71),
    ("eaie", 72),
    ("eaic", 73),
    ("eais", 74),
    ("eane", 75),
    ("eanc", 76),
    ("eans", 77),
    ("xis", 78),
    ("xii", 79),
    ("xin", 80),
    ("tnf", 81),
    ("tavc", 82),
    ("rth", 83),
    ("cth", 84),
    ("vrt", 85),
    ("art", 86),
    ("ccso", 87),
    ("qbm", 88),
    ("nkf", 89),
    ("xikf", 90),
    ("xrcx", 91),
    ("xrc", 91),
    ("xrbx", 92),
    ("xrb", 92),
    ("xrbp", 93),
    ("isrr", 94),
    ("xisr", 95),
    ("dear", 96),
    ("eap", 97),
    ("vbbe", 98),
    ("nbbe", 99),
    ("ibbe", 100),
    ("tvbbe1", 101),
    ("tvbbe2", 102),
    ("tnbbe", 103),
    ("ebbe", 104),
    ("dtemp", 105),
    ("vers", 106),
    ("vref", 107),
];

/// The `VBICsetup` defaults of the parameter block, by index.
const DEFAULTS: [f64; NUM_PARAMS] = [
    27.0,  // 0 TNOM (°C)
    0.0,   // 1 RCX
    0.1,   // 2 RCI
    0.0,   // 3 VO
    0.0,   // 4 GAMM
    1.0,   // 5 HRCF
    0.0,   // 6 RBX
    0.1,   // 7 RBI
    0.0,   // 8 RE
    0.0,   // 9 RS
    0.1,   // 10 RBP
    1e-16, // 11 IS
    1.0,   // 12 NF
    1.0,   // 13 NR
    0.9,   // 14 FC
    0.0,   // 15 CBEO
    0.0,   // 16 CJE
    0.75,  // 17 PE
    0.33,  // 18 ME
    -0.5,  // 19 AJE
    0.0,   // 20 CBCO
    0.0,   // 21 CJC
    0.0,   // 22 QCO
    0.0,   // 23 CJEP
    0.75,  // 24 PC
    0.33,  // 25 MC
    -0.5,  // 26 AJC
    0.0,   // 27 CJCP
    0.75,  // 28 PS
    0.33,  // 29 MS
    -0.5,  // 30 AJS
    1e-18, // 31 IBEI
    1.0,   // 32 WBE
    1.0,   // 33 NEI
    0.0,   // 34 IBEN
    2.0,   // 35 NEN
    1e-16, // 36 IBCI
    1.0,   // 37 NCI
    0.0,   // 38 IBCN
    2.0,   // 39 NCN
    0.0,   // 40 AVC1
    0.0,   // 41 AVC2
    0.0,   // 42 ISP
    1.0,   // 43 WSP
    1.0,   // 44 NFP
    0.0,   // 45 IBEIP
    0.0,   // 46 IBENP
    0.0,   // 47 IBCIP
    1.0,   // 48 NCIP
    0.0,   // 49 IBCNP
    2.0,   // 50 NCNP
    0.0,   // 51 VEF
    0.0,   // 52 VER
    0.0,   // 53 IKF
    0.0,   // 54 IKR
    0.0,   // 55 IKP
    0.0,   // 56 TF
    0.0,   // 57 QTF
    0.0,   // 58 XTF
    0.0,   // 59 VTF
    0.0,   // 60 ITF
    0.0,   // 61 TR
    0.0,   // 62 TD
    0.0,   // 63 KFN
    1.0,   // 64 AFN
    1.0,   // 65 BFN
    0.0,   // 66 XRE
    0.0,   // 67 XRBI
    0.0,   // 68 XRCI
    0.0,   // 69 XRS
    0.0,   // 70 XVO
    1.12,  // 71 EA
    1.12,  // 72 EAIE
    1.12,  // 73 EAIC
    1.12,  // 74 EAIS
    1.12,  // 75 EANE
    1.12,  // 76 EANC
    1.12,  // 77 EANS
    3.0,   // 78 XIS
    3.0,   // 79 XII
    3.0,   // 80 XIN
    0.0,   // 81 TNF
    0.0,   // 82 TAVC
    0.0,   // 83 RTH
    0.0,   // 84 CTH
    0.0,   // 85 VRT
    0.1,   // 86 ART
    0.0,   // 87 CCSO
    0.0,   // 88 QBM
    0.5,   // 89 NKF
    0.0,   // 90 XIKF
    0.0,   // 91 XRCX
    0.0,   // 92 XRBX
    0.0,   // 93 XRBP
    1.0,   // 94 ISRR
    0.0,   // 95 XISR
    0.0,   // 96 DEAR
    1.12,  // 97 EAP
    0.0,   // 98 VBBE
    1.0,   // 99 NBBE
    1e-6,  // 100 IBBE
    0.0,   // 101 TVBBE1
    0.0,   // 102 TVBBE2
    0.0,   // 103 TNBBE
    0.0,   // 104 EBBE
    0.0,   // 105 DTEMP
    1.2,   // 106 VERS
    0.0,   // 107 VREF
];

/// The SOA keywords (`VBE_MAX` … `VSUBFWD`, aliases included) with
/// their slot in [`VbicModel`]'s inert SOA block and the defaults.
const SOA_KEYWORDS: [(&str, usize); 11] = [
    ("vbe_max", 0),
    ("bvbe", 0),
    ("vbc_max", 1),
    ("bvbc", 1),
    ("vce_max", 2),
    ("bvce", 2),
    ("vsub_max", 3),
    ("bvsub", 3),
    ("vbefwd", 4),
    ("vbcfwd", 5),
    ("vsubfwd", 6),
];
const SOA_DEFAULTS: [f64; 7] = [1e99, 1e99, 1e99, 1e99, 0.2, 0.2, 0.2];

/// `.model NAME NPN|PNP (LEVEL=4 …)` (or `LEVEL=9`): ngspice-47's
/// VBIC 1.2 bipolar transistor (`vbic.c`, Colin McAndrew's model as
/// implemented in Spice3 by Dietmar Warning), with ngspice's keywords
/// and the defaults `VBICsetup` gives them.
///
/// The parameters are set and read by keyword ([`VbicModel::set`],
/// [`VbicModel::get`]); the keyword table is `vbicmpar.c`'s, aliases
/// included (`TREF`, `V0`, `CBE0`, `CBC0`, `QC0`, `XV0`, `XRC`, `XRB`,
/// `BVBE`, `BVBC`, `BVCE`, `BVSUB`). `TNOM` is in °C and defaults to
/// 27 regardless of `.options tnom`, as in the C. A *given* `RCX`,
/// `RCI`, `RBX`, `RBI`, `RE`, `RS` or `RBP` below 0.01 Ω is raised to
/// 0.01 Ω (`R_MIN`); `RCX`, `RBX`, `RE` and `RS` left at their default
/// 0 add no internal node. Self-heating is on when `RTH > 0` is given
/// (and `SELFT`, when given, is 1); excess phase when `TD > 0` is
/// given. The SOA limits (`VBE_MAX` …) are accepted and inert.
#[derive(Clone, Debug)]
pub struct VbicModel {
    /// NPN (`true`, also the C's fallback) or PNP.
    pub npn: bool,
    p: [f64; NUM_PARAMS],
    given: [bool; NUM_PARAMS],
    selft: Option<i64>,
    soa: [f64; 7],
}

impl VbicModel {
    /// A model card with every parameter at its `VBICsetup` default.
    pub fn new(npn: bool) -> Self {
        VbicModel {
            npn,
            p: DEFAULTS,
            given: [false; NUM_PARAMS],
            selft: None,
            soa: SOA_DEFAULTS,
        }
    }

    /// Set one model parameter by its ngspice keyword
    /// (`VBICmParam`). `npn`/`pnp` are the type flags (nonzero
    /// selects); `selft` is an integer parameter (rounded like
    /// `INPgetValue`: `floor(0.5 + v)`). Unknown keywords are an
    /// `Err`.
    pub fn set(&mut self, key: &str, v: f64) -> Result<(), String> {
        match key {
            "npn" => {
                if v != 0.0 {
                    self.npn = true;
                }
                return Ok(());
            }
            "pnp" => {
                if v != 0.0 {
                    self.npn = false;
                }
                return Ok(());
            }
            "selft" => {
                self.selft = Some((0.5 + v).floor() as i64);
                return Ok(());
            }
            _ => {}
        }
        if let Some(&(_, k)) = SOA_KEYWORDS.iter().find(|(n, _)| *n == key) {
            self.soa[k] = v;
            return Ok(());
        }
        let Some(&(_, k)) = KEYWORDS.iter().find(|(n, _)| *n == key) else {
            return Err(format!("unrecognized parameter ({key})"));
        };
        let mut v = v;
        // `R_MIN` on the seven ohmic resistances.
        if matches!(k, 1 | 2 | 6 | 7 | 8 | 9 | 10) && v < R_MIN {
            v = R_MIN;
        }
        self.p[k] = v;
        self.given[k] = true;
        Ok(())
    }

    /// Read one model parameter by keyword (the values [`set`]
    /// stored, or the defaults); `npn`/`pnp` read 1 or 0, `selft` its
    /// value (0 when not given). `None` for an unknown keyword.
    ///
    /// [`set`]: VbicModel::set
    pub fn get(&self, key: &str) -> Option<f64> {
        match key {
            "npn" => return Some(if self.npn { 1.0 } else { 0.0 }),
            "pnp" => return Some(if self.npn { 0.0 } else { 1.0 }),
            "selft" => return Some(self.selft.unwrap_or(0) as f64),
            _ => {}
        }
        if let Some(&(_, k)) = SOA_KEYWORDS.iter().find(|(n, _)| *n == key) {
            return Some(self.soa[k]);
        }
        KEYWORDS
            .iter()
            .find(|(n, _)| *n == key)
            .map(|&(_, k)| self.p[k])
    }

    /// The type as the C's `VBICtype` (+1 NPN, −1 PNP).
    pub(crate) fn typ(&self) -> f64 {
        if self.npn { 1.0 } else { -1.0 }
    }

    /// One entry of the parameter block.
    pub(crate) fn p(&self, k: usize) -> f64 {
        self.p[k]
    }

    /// `VBIC_selfheat` (`vbicsetup.c`): with `SELFT` given it must be
    /// 1, and `RTH` must be given and positive.
    pub(crate) fn selfheat(&self) -> bool {
        let rth_on = self.given[P_RTH] && self.p[P_RTH] > 0.0;
        match self.selft {
            Some(s) => s == 1 && rth_on,
            None => rth_on,
        }
    }

    /// `VBIC_excessPhase`: `TD` given and positive.
    pub(crate) fn excess_phase(&self) -> bool {
        self.given[P_TD] && self.p[P_TD] > 0.0
    }

    /// The parameter block `VBICload` copies (`memcpy(&p,
    /// &model->VBICtnom, …)`) after `VBICsetup` touched it: `CTH` is
    /// raised to 1e-12 when `RTH` is given (`vbicsetup.c`, done there
    /// on the model while it walks the instances).
    pub(crate) fn block(&self) -> [f64; NUM_PARAMS] {
        let mut p = self.p;
        if self.given[P_RTH] && p[P_CTH] < 1e-12 {
            p[P_CTH] = 1e-12;
        }
        p
    }
}

/// `vbic_4T_et_cf_t` (`vbictemp.c`): the parameter block mapped from
/// `TNOM` to the ambient temperature `tamb` (°C), statement for
/// statement. The C leaves the entries it neither copies nor maps
/// uninitialised; every index is covered, so none is.
#[allow(clippy::too_many_lines)]
pub(crate) fn temp_map(pnom: &[f64; NUM_PARAMS], tamb: f64) -> [f64; NUM_PARAMS] {
    let mut p = [0.0; NUM_PARAMS];
    // Direct copy p<-pnom for temperature independent parameters.
    for k in [
        5, 14, 15, 18, 19, 20, 22, 25, 26, 29, 30, 32, 33, 35, 37, 39, 40, 43, 44, 48, 50, 51, 52,
        54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76,
        77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 95, 96, 97, 100, 101,
        102, 103, 105, 106, 107,
    ] {
        p[k] = pnom[k];
    }

    // Temperature mappings for model parameters.
    let tini = 2.731500e+02 + pnom[0];
    let tdev = (2.731500e+02 + tamb) + pnom[105];
    let vtv = 1.380662e-23 * tdev / 1.602189e-19;
    let r_t = tdev / tini;
    let d_t = tdev - tini;
    p[53] = pnom[53] * r_t.powf(pnom[90]);
    p[1] = pnom[1] * r_t.powf(pnom[91]);
    p[2] = pnom[2] * r_t.powf(pnom[68]);
    p[6] = pnom[6] * r_t.powf(pnom[92]);
    p[7] = pnom[7] * r_t.powf(pnom[67]);
    p[8] = pnom[8] * r_t.powf(pnom[66]);
    p[9] = pnom[9] * r_t.powf(pnom[69]);
    p[10] = pnom[10] * r_t.powf(pnom[93]);
    // Saturation currents: pnom[i]·(rT^xti·exp(−ea·(1−rT)/Vtv))^(1/n).
    let sat = |i: usize, xti: usize, ea: usize, n: usize| {
        let xvar2 = r_t.powf(pnom[xti]);
        let xvar3 = -pnom[ea] * (1.0 - r_t) / vtv;
        let xvar4 = xvar3.exp();
        let xvar1 = xvar2 * xvar4;
        let xvar5 = 1.0 / pnom[n];
        let xvar6 = xvar1.powf(xvar5);
        pnom[i] * xvar6
    };
    p[11] = sat(11, 78, 71, 12);
    p[94] = sat(94, 95, 96, 13);
    p[42] = sat(42, 78, 97, 44);
    p[31] = sat(31, 79, 72, 33);
    p[34] = sat(34, 80, 75, 35);
    p[36] = sat(36, 79, 73, 37);
    p[38] = sat(38, 80, 76, 39);
    p[45] = sat(45, 79, 73, 37);
    p[46] = sat(46, 80, 76, 39);
    p[47] = sat(47, 79, 74, 48);
    p[49] = sat(49, 80, 77, 50);
    p[12] = pnom[12] * (1.0 + d_t * pnom[81]);
    p[13] = pnom[13] * (1.0 + d_t * pnom[81]);
    p[41] = pnom[41] * (1.0 + d_t * pnom[82]);
    p[98] = pnom[98] * (1.0 + d_t * (pnom[101] + d_t * pnom[102]));
    p[99] = pnom[99] * (1.0 + d_t * pnom[103]);
    // Built-in potentials.
    let pot = |pi: usize, ea: usize| {
        let xvar2 = 0.5 * pnom[pi] * r_t / vtv;
        let xvar3 = xvar2.exp();
        let xvar4 = -0.5 * pnom[pi] * r_t / vtv;
        let xvar5 = xvar4.exp();
        let xvar1 = xvar3 - xvar5;
        let xvar6 = xvar1.ln();
        let psiio = 2.0 * (vtv / r_t) * xvar6;
        let xvar1 = r_t.ln();
        let psiin = psiio * r_t - 3.0 * vtv * xvar1 - pnom[ea] * (r_t - 1.0);
        let xvar2 = -psiin / vtv;
        let xvar3 = xvar2.exp();
        let xvar1 = 0.5 * (1.0 + (1.0 + 4.0 * xvar3).sqrt());
        let xvar4 = xvar1.ln();
        psiin + 2.0 * vtv * xvar4
    };
    p[17] = pot(17, 72);
    p[24] = pot(24, 73);
    p[28] = pot(28, 74);
    p[16] = pnom[16] * (pnom[17] / p[17]).powf(pnom[18]);
    p[21] = pnom[21] * (pnom[24] / p[24]).powf(pnom[25]);
    p[23] = pnom[23] * (pnom[24] / p[24]).powf(pnom[25]);
    p[27] = pnom[27] * (pnom[28] / p[28]).powf(pnom[29]);
    let xvar1 = r_t.powf(pnom[78]);
    let xvar2 = -pnom[71] * (1.0 - r_t) / vtv;
    let xvar3 = xvar2.exp();
    p[4] = pnom[4] * xvar1 * xvar3;
    p[3] = pnom[3] * r_t.powf(pnom[70]);
    let xvar1 = -p[98] / (p[99] * vtv);
    p[104] = xvar1.exp();
    p[0] = tamb + p[105];
    p
}

/// The temperature-dependent instance values `VBICtemp` stores
/// (`here->VBICt…`), as the parameter block `VBICload` builds from
/// them: the model block with `p[0]` set to the device temperature
/// (°C, model `DTEMP` included) and the 33 mapped entries replaced.
/// `EBBE` (`p[104]`) is *not* replaced — `VBICtemp` drops the mapped
/// value, so the load sees the card's.
pub(crate) fn load_block(model: &VbicModel, temp: f64) -> [f64; NUM_PARAMS] {
    use crate::consts::CTOK;
    let pnom = model.block();
    let tamb = temp - CTOK;
    let t = temp_map(&pnom, tamb);
    let mut p = pnom;
    p[0] = temp - CTOK + p[P_DTEMP];
    for k in [
        1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 16, 17, 21, 23, 24, 27, 28, 31, 34, 36, 38, 41, 42,
        45, 46, 47, 49, 53, 94, 98, 99,
    ] {
        p[k] = t[k];
    }
    p
}
