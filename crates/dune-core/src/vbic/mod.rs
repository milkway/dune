//! VBIC — ngspice-47's Vertical Bipolar Inter-Company model, version
//! 1.2 (`spicelib/devices/vbic/`: `vbicload.c`, `vbictemp.c`,
//! `vbicsetup.c`, `vbicmpar.c`, `vbicparam.c`, `vbicacld.c`,
//! `vbicpzld.c`, `vbicnoise.c`, `vbicconv.c`, `vbictrunc.c`,
//! `vbicgetic.c`). The 3f5 has no VBIC, so ngspice-47 is both the
//! contract and the oracle.
//!
//! Upstream authorship (Modified BSD, as the rest of ngspice):
//! Copyright 1990 Regents of the University of California; Author:
//! 1985 Thomas L. Quarles; Model Author: 1995 Colin McAndrew
//! Motorola; Spice3 Implementation: 2003 Dietmar Warning DAnalyse
//! GmbH.
//!
//! A four-terminal bipolar transistor (collector, base, emitter,
//! substrate) with an optional thermal node: the intrinsic transistor
//! with the Gummel-Poon-like charge `qb` (or the SGP form, `QBM`),
//! Kull's quasi-saturation epi resistance `RCI` (`VO`, `GAMM`,
//! `HRCF`), weak avalanche (`AVC1`/`AVC2`), the parasitic substrate
//! PNP (`ISP`, `WSP`, `IKP`) with its own junctions and base
//! resistance `RBP`, smoothed depletion charges (`AJE`/`AJC`/`AJS`,
//! reach-through `VRT`/`ART`), diffusion charges with the bias- and
//! current-dependent transit time, B-E breakdown (`VBBE`…), excess
//! phase through a two-node delay network (`TD`), and self-heating
//! through the thermal node (`RTH`, `CTH`). [`model`] holds the card
//! and the temperature mapping, `fj` the device equations
//! (mechanically translated from the C), `load` the Newton load; this
//! file the instance, its small-signal loads, noise and convergence
//! test. Notes on the port: `docs/vbic.md`.

mod fj;
mod load;
mod model;
#[cfg(test)]
mod tests;

pub use model::VbicModel;
use model::{NUM_PARAMS, P_AFN, P_BFN, P_KFN, P_TD};
pub(crate) use model::{P_RBX, P_RCX, P_RE, P_RS};

use crate::matrix::{AcCtx, MatrixBuilder, Slot, TRASH, Var};

// State offsets (`vbicdefs.h`).
const VBEI: usize = 0;
const VBEX: usize = 1;
const VBCI: usize = 2;
const VBCX: usize = 3;
const VBEP: usize = 4;
const VRCI: usize = 5;
const VRBI: usize = 6;
const VRBP: usize = 7;
const VBCP: usize = 8;
const VXF1: usize = 9;
const VXF2: usize = 10;
const IBE: usize = 11;
const IBE_VBEI: usize = 12;
const IBEX: usize = 13;
const IBEX_VBEX: usize = 14;
const ICIEI: usize = 15;
const ICIEI_VBEI: usize = 16;
const ICIEI_VBCI: usize = 17;
const ICIEI_VRTH: usize = 18;
const ICIEI_VXF2: usize = 19;
const IBC: usize = 20;
const IBC_VBCI: usize = 21;
const IBC_VBEI: usize = 22;
const IBEP: usize = 23;
const IBEP_VBEP: usize = 24;
const IRCI: usize = 25;
const IRCI_VRCI: usize = 26;
const IRCI_VBCI: usize = 27;
const IRCI_VBCX: usize = 28;
const IRBI: usize = 29;
const IRBI_VRBI: usize = 30;
const IRBI_VBEI: usize = 31;
const IRBI_VBCI: usize = 32;
const IRBP: usize = 33;
const IRBP_VRBP: usize = 34;
const IRBP_VBEP: usize = 35;
const IRBP_VBCI: usize = 36;
const QBE: usize = 37;
const CQBE: usize = 38;
const CQBECI: usize = 39;
const QBEX: usize = 40;
const CQBEX: usize = 41;
const QBC: usize = 42;
const CQBC: usize = 43;
const QBCX: usize = 44;
const CQBCX: usize = 45;
const QBEP: usize = 46;
const CQBEP: usize = 47;
const CQBEPCI: usize = 48;
const QBEO: usize = 49;
const CQBEO: usize = 50;
const GQBEO: usize = 51;
const QBCO: usize = 52;
const CQBCO: usize = 53;
const GQBCO: usize = 54;
const IBCP: usize = 55;
const IBCP_VBCP: usize = 56;
const ICCP: usize = 57;
const ICCP_VBEP: usize = 58;
const ICCP_VBCI: usize = 59;
const ICCP_VBCP: usize = 60;
const QBCP: usize = 61;
const CQBCP: usize = 62;
const IRCX_VRCX: usize = 63;
const IRBX_VRBX: usize = 64;
const IRS_VRS: usize = 65;
const IRE_VRE: usize = 66;
const QCTH: usize = 67;
const CQCTH: usize = 68;
const VRTH: usize = 69;
const ICTH_VRTH: usize = 70;
const QXF1: usize = 71;
const CQXF1: usize = 72;
const GQXF1: usize = 73;
const IXF1: usize = 74;
const IXF1_VBEI: usize = 75;
const IXF1_VBCI: usize = 76;
const IXF1_VXF1: usize = 77;
const IXF1_VXF2: usize = 78;
const IXF1_VRTH: usize = 79;
const QXF2: usize = 80;
const CQXF2: usize = 81;
const GQXF2: usize = 82;
const IXF2: usize = 83;
const IXF2_VXF1: usize = 84;
const IXF2_VXF2: usize = 85;
/// `VBICnumStates`.
pub(crate) const VBIC_NUM_STATES: usize = 86;

// Matrix slots in `VBICsetup`'s TSTALLOC order (C = collector, B =
// base, E = emitter, S = substrate, CX/CI/BX/BI/EI/BP/SI the internal
// nodes, T the thermal node, XF1/XF2 the excess-phase nodes).
const C_C: usize = 0;
const B_B: usize = 1;
const E_E: usize = 2;
const S_S: usize = 3;
const CX_CX: usize = 4;
const CI_CI: usize = 5;
const BX_BX: usize = 6;
const BI_BI: usize = 7;
const EI_EI: usize = 8;
const BP_BP: usize = 9;
const SI_SI: usize = 10;
const B_E: usize = 11;
const E_B: usize = 12;
const B_C: usize = 13;
const C_B: usize = 14;
const C_CX: usize = 15;
const B_BX: usize = 16;
const E_EI: usize = 17;
const S_SI: usize = 18;
const CX_CI: usize = 19;
const CX_BX: usize = 20;
const CX_BI: usize = 21;
const CX_BP: usize = 22;
const CI_BI: usize = 23;
const CI_EI: usize = 24;
const BX_BI: usize = 25;
const BX_EI: usize = 26;
const BX_BP: usize = 27;
const BX_SI: usize = 28;
const BI_EI: usize = 29;
const BP_SI: usize = 30;
const CX_C: usize = 31;
const BX_B: usize = 32;
const EI_E: usize = 33;
const SI_S: usize = 34;
const CI_CX: usize = 35;
const BI_CX: usize = 36;
const BP_CX: usize = 37;
const BX_CI: usize = 38;
const BI_CI: usize = 39;
const EI_CI: usize = 40;
const BP_CI: usize = 41;
const BI_BX: usize = 42;
const EI_BX: usize = 43;
const BP_BX: usize = 44;
const SI_BX: usize = 45;
const EI_BI: usize = 46;
const BP_BI: usize = 47;
const SI_CI: usize = 48;
const SI_BI: usize = 49;
const SI_BP: usize = 50;
// Self-heating.
const C_T: usize = 51;
const B_T: usize = 52;
const E_T: usize = 53;
const S_T: usize = 54;
const CI_T: usize = 55;
const CX_T: usize = 56;
const BI_T: usize = 57;
const BX_T: usize = 58;
const BP_T: usize = 59;
const EI_T: usize = 60;
const SI_T: usize = 61;
const T_C: usize = 62;
const T_CI: usize = 63;
const T_CX: usize = 64;
const T_BI: usize = 65;
const T_B: usize = 66;
const T_BX: usize = 67;
const T_BP: usize = 68;
const T_E: usize = 69;
const T_EI: usize = 70;
const T_S: usize = 71;
const T_SI: usize = 72;
const T_T: usize = 73;
const XF1_T: usize = 74;
// Excess phase.
const XF1_XF1: usize = 75;
const XF1_BI: usize = 76;
const XF1_EI: usize = 77;
const XF1_CI: usize = 78;
const XF1_XF2: usize = 79;
const XF2_XF1: usize = 80;
const XF2_XF2: usize = 81;
const EI_XF2: usize = 82;
const CI_XF2: usize = 83;
// Dune's pin of a thermal node named by an instance without
// self-heating.
const PIN: usize = 84;
const NUM_SLOTS: usize = 85;

/// The noise generators of `vbicnoise.c`, in its order.
const VBIC_NOISE_SUFFIXES: &[&str] = &[
    "_rc",
    "_rci",
    "_rb",
    "_rbi",
    "_re",
    "_rbp",
    "_rs",
    "_ic",
    "_ib",
    "_ibep",
    "_iccp",
    "_1overfbe",
    "_1overfbep",
    "",
];

/// The unknowns of one VBIC instance.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct VbicVars {
    pub coll: Var,
    pub base: Var,
    pub emit: Var,
    pub subs: Var,
    /// The thermal node (5th terminal; ground when the card names
    /// four or fewer nodes).
    pub temp: Var,
    /// Internal nodes; `coll_cx`, `base_bx`, `emit_ei`, `subs_si` are
    /// the external node when `RCX`/`RBX`/`RE`/`RS` is 0.
    pub coll_cx: Var,
    pub coll_ci: Var,
    pub base_bx: Var,
    pub base_bi: Var,
    pub emit_ei: Var,
    pub base_bp: Var,
    pub subs_si: Var,
    /// Excess-phase nodes (0 without `TD`).
    pub xf1: Var,
    pub xf2: Var,
    /// A thermal node named by an instance without self-heating that
    /// no other device stamps: nothing in the C stamps it, its sparse
    /// matrix never sees it and it reads 0; Dune pins it to 0 with a
    /// unit diagonal (0 = none).
    pub pin: Var,
}

/// The self-heating derivatives the load keeps on the instance for
/// the AC and pole-zero loads (`here->VBICibe_Vrth` … `VBICith_Vrs`)
/// and the thermal capacitances of the small-signal pass
/// (`VBICcapcth`, `VBICcapqbeth` …).
#[derive(Clone, Copy, Debug, Default)]
#[allow(non_snake_case)]
struct Thermal {
    Ibe_Vrth: f64,
    Ibex_Vrth: f64,
    Ibc_Vrth: f64,
    Ibep_Vrth: f64,
    Ircx_Vrth: f64,
    Irci_Vrth: f64,
    Irbx_Vrth: f64,
    Irbi_Vrth: f64,
    Ire_Vrth: f64,
    Irbp_Vrth: f64,
    Ibcp_Vrth: f64,
    Iccp_Vrth: f64,
    Irs_Vrth: f64,
    Irth_Vrth: f64,
    Ith_Vrth: f64,
    Ith_Vbei: f64,
    Ith_Vbci: f64,
    Ith_Vcei: f64,
    Ith_Vbex: f64,
    Ith_Vbep: f64,
    Ith_Vbcp: f64,
    Ith_Vcep: f64,
    Ith_Vrci: f64,
    Ith_Vbcx: f64,
    Ith_Vrbi: f64,
    Ith_Vrbp: f64,
    Ith_Vrcx: f64,
    Ith_Vrbx: f64,
    Ith_Vre: f64,
    Ith_Vrs: f64,
    capcth: f64,
    capqbeth: f64,
    capqbexth: f64,
    capqbcth: f64,
    capqbcxth: f64,
    capqbepth: f64,
    capqbcpth: f64,
}

/// One VBIC instance (`VBICinstance`).
pub(crate) struct VbicI {
    v: VbicVars,
    /// `VBICtype`: +1 NPN, −1 PNP.
    typ: f64,
    /// The parameter block `VBICload` builds (`model::load_block`).
    p: [f64; NUM_PARAMS],
    /// `AREA·M` (`SCALE`).
    scale: f64,
    m: f64,
    off: bool,
    ic_vbe: f64,
    ic_vce: f64,
    ic_given: crate::devices::IcGiven,
    selfheat: bool,
    excess: bool,
    /// `VBICtemp`, kelvin.
    temp: f64,
    /// `VBICtVcrit`.
    tvcrit: f64,
    /// The temperature offset of the thermal-noise sources
    /// (`vbicnoise.c`: `TEMP − CKTtemp` when `TEMP` was given, else
    /// `DTEMP`).
    noise_dtemp: f64,
    /// `TD` as given on the card (`model->VBICdelayTimeF`).
    td: f64,
    kfn: f64,
    afn: f64,
    bfn: f64,
    st: usize,
    s: [Slot; NUM_SLOTS],
    th: Thermal,
}

/// What [`VbicI::new`] needs besides the unknowns.
pub(crate) struct VbicSetup {
    pub model: VbicModel,
    pub area: f64,
    pub m: f64,
    pub off: bool,
    pub ic: (f64, f64),
    /// `VBICtemp` (kelvin): `TEMP` (or `CKTtemp`) plus `DTEMP`.
    pub temp: f64,
    pub noise_dtemp: f64,
}

impl VbicI {
    /// `VBICsetup` + `VBICtemp` for one instance.
    pub(crate) fn new(v: VbicVars, su: VbicSetup, st: usize) -> Self {
        let model = &su.model;
        let p = model::load_block(model, su.temp);
        let vt = su.temp * crate::consts::KOVERQ;
        let tvcrit =
            vt * f64::ln(vt / (crate::consts::ROOT2 * model.p(model::P_IS) * su.area * su.m));
        VbicI {
            v,
            typ: model.typ(),
            p,
            scale: su.area * su.m,
            m: su.m,
            off: su.off,
            ic_vbe: su.ic.0,
            ic_vce: su.ic.1,
            ic_given: crate::devices::IcGiven::ALL,
            selfheat: model.selfheat(),
            excess: model.excess_phase(),
            temp: su.temp,
            tvcrit,
            noise_dtemp: su.noise_dtemp,
            td: model.p(P_TD),
            kfn: model.p(P_KFN),
            afn: model.p(P_AFN),
            bfn: model.p(P_BFN),
            st,
            s: [TRASH; NUM_SLOTS],
            th: Thermal::default(),
        }
    }

    /// The small-signal stamps shared by `VBICacLoad` and
    /// `VBICpzLoad`: `g` takes the conductances, `c` the capacitances
    /// (the AC load multiplies them by ω into the imaginary part, the
    /// pole-zero load by `s`).
    #[allow(clippy::too_many_lines, non_snake_case)]
    fn small_signal(
        &self,
        states: &crate::state::States,
        mut g: impl FnMut(Slot, f64),
        mut c: impl FnMut(Slot, f64),
    ) {
        let st = self.st;
        let s0 = |o: usize| states.get(0, st + o);
        let s = &self.s;
        let Ibe_Vbei = s0(IBE_VBEI);
        let Ibex_Vbex = s0(IBEX_VBEX);
        let Iciei_Vbei = s0(ICIEI_VBEI);
        let Iciei_Vbci = s0(ICIEI_VBCI);
        let Iciei_Vrth = s0(ICIEI_VRTH);
        let Iciei_Vxf2 = s0(ICIEI_VXF2);
        let Ibc_Vbci = s0(IBC_VBCI);
        let Ibc_Vbei = s0(IBC_VBEI);
        let Ibep_Vbep = s0(IBEP_VBEP);
        let Irci_Vrci = s0(IRCI_VRCI);
        let Irci_Vbci = s0(IRCI_VBCI);
        let Irci_Vbcx = s0(IRCI_VBCX);
        let Irbi_Vrbi = s0(IRBI_VRBI);
        let Irbi_Vbei = s0(IRBI_VBEI);
        let Irbi_Vbci = s0(IRBI_VBCI);
        let Irbp_Vrbp = s0(IRBP_VRBP);
        let Irbp_Vbep = s0(IRBP_VBEP);
        let Irbp_Vbci = s0(IRBP_VBCI);
        let Ibcp_Vbcp = s0(IBCP_VBCP);
        let Iccp_Vbep = s0(ICCP_VBEP);
        let Iccp_Vbci = s0(ICCP_VBCI);
        let Iccp_Vbcp = s0(ICCP_VBCP);
        let Ircx_Vrcx = s0(IRCX_VRCX);
        let Irbx_Vrbx = s0(IRBX_VRBX);
        let Irs_Vrs = s0(IRS_VRS);
        let Ire_Vre = s0(IRE_VRE);
        let Ixf1_Vbei = s0(IXF1_VBEI);
        let Ixf1_Vbci = s0(IXF1_VBCI);
        let Ixf1_Vxf1 = s0(IXF1_VXF1);
        let Ixf1_Vxf2 = s0(IXF1_VXF2);
        let Ixf1_Vrth = s0(IXF1_VRTH);
        let Ixf2_Vxf1 = s0(IXF2_VXF1);
        let Ixf2_Vxf2 = s0(IXF2_VXF2);

        // The real part.
        // Ibe
        g(s[BI_BI], Ibe_Vbei);
        g(s[BI_EI], -Ibe_Vbei);
        g(s[EI_BI], -Ibe_Vbei);
        g(s[EI_EI], Ibe_Vbei);
        // Ibex
        g(s[BX_BX], Ibex_Vbex);
        g(s[BX_EI], -Ibex_Vbex);
        g(s[EI_BX], -Ibex_Vbex);
        g(s[EI_EI], Ibex_Vbex);
        // Iciei
        g(s[CI_BI], Iciei_Vbei);
        g(s[CI_EI], -Iciei_Vbei);
        g(s[CI_BI], Iciei_Vbci);
        g(s[CI_CI], -Iciei_Vbci);
        g(s[EI_BI], -Iciei_Vbei);
        g(s[EI_EI], Iciei_Vbei);
        g(s[EI_BI], -Iciei_Vbci);
        g(s[EI_CI], Iciei_Vbci);
        if self.excess {
            g(s[CI_XF2], Iciei_Vxf2);
            g(s[EI_XF2], -Iciei_Vxf2);
        }
        // Ibc
        g(s[BI_BI], Ibc_Vbci);
        g(s[BI_CI], -Ibc_Vbci);
        g(s[BI_BI], Ibc_Vbei);
        g(s[BI_EI], -Ibc_Vbei);
        g(s[CI_BI], -Ibc_Vbci);
        g(s[CI_CI], Ibc_Vbci);
        g(s[CI_BI], -Ibc_Vbei);
        g(s[CI_EI], Ibc_Vbei);
        // Ibep
        g(s[BX_BX], Ibep_Vbep);
        g(s[BX_BP], -Ibep_Vbep);
        g(s[BP_BX], -Ibep_Vbep);
        g(s[BP_BP], Ibep_Vbep);
        // Rcx
        g(s[C_C], Ircx_Vrcx);
        g(s[CX_CX], Ircx_Vrcx);
        g(s[CX_C], -Ircx_Vrcx);
        g(s[C_CX], -Ircx_Vrcx);
        // Rci
        g(s[CX_CX], Irci_Vrci);
        g(s[CX_CI], -Irci_Vrci);
        g(s[CX_BI], Irci_Vbci);
        g(s[CX_CI], -Irci_Vbci);
        g(s[CX_BI], Irci_Vbcx);
        g(s[CX_CX], -Irci_Vbcx);
        g(s[CI_CX], -Irci_Vrci);
        g(s[CI_CI], Irci_Vrci);
        g(s[CI_BI], -Irci_Vbci);
        g(s[CI_CI], Irci_Vbci);
        g(s[CI_BI], -Irci_Vbcx);
        g(s[CI_CX], Irci_Vbcx);
        // Rbx
        g(s[B_B], Irbx_Vrbx);
        g(s[BX_BX], Irbx_Vrbx);
        g(s[BX_B], -Irbx_Vrbx);
        g(s[B_BX], -Irbx_Vrbx);
        // Rbi
        g(s[BX_BX], Irbi_Vrbi);
        g(s[BX_BI], -Irbi_Vrbi);
        g(s[BX_BI], Irbi_Vbei);
        g(s[BX_EI], -Irbi_Vbei);
        g(s[BX_BI], Irbi_Vbci);
        g(s[BX_CI], -Irbi_Vbci);
        g(s[BI_BX], -Irbi_Vrbi);
        g(s[BI_BI], Irbi_Vrbi);
        g(s[BI_BI], -Irbi_Vbei);
        g(s[BI_EI], Irbi_Vbei);
        g(s[BI_BI], -Irbi_Vbci);
        g(s[BI_CI], Irbi_Vbci);
        // Re
        g(s[E_E], Ire_Vre);
        g(s[EI_EI], Ire_Vre);
        g(s[EI_E], -Ire_Vre);
        g(s[E_EI], -Ire_Vre);
        // Rbp
        g(s[BP_BP], Irbp_Vrbp);
        g(s[BP_CX], -Irbp_Vrbp);
        g(s[BP_BX], Irbp_Vbep);
        g(s[BP_BP], -Irbp_Vbep);
        g(s[BP_BI], Irbp_Vbci);
        g(s[BP_CI], -Irbp_Vbci);
        g(s[CX_BP], -Irbp_Vrbp);
        g(s[CX_CX], Irbp_Vrbp);
        g(s[CX_BX], -Irbp_Vbep);
        g(s[CX_BP], Irbp_Vbep);
        g(s[CX_BI], -Irbp_Vbci);
        g(s[CX_CI], Irbp_Vbci);
        // Ibcp
        g(s[SI_SI], Ibcp_Vbcp);
        g(s[SI_BP], -Ibcp_Vbcp);
        g(s[BP_SI], -Ibcp_Vbcp);
        g(s[BP_BP], Ibcp_Vbcp);
        // Iccp
        g(s[BX_BX], Iccp_Vbep);
        g(s[BX_BP], -Iccp_Vbep);
        g(s[BX_BI], Iccp_Vbci);
        g(s[BX_CI], -Iccp_Vbci);
        g(s[BX_SI], Iccp_Vbcp);
        g(s[BX_BP], -Iccp_Vbcp);
        g(s[SI_BX], -Iccp_Vbep);
        g(s[SI_BP], Iccp_Vbep);
        g(s[SI_BI], -Iccp_Vbci);
        g(s[SI_CI], Iccp_Vbci);
        g(s[SI_SI], -Iccp_Vbcp);
        g(s[SI_BP], Iccp_Vbcp);
        // Rs
        g(s[S_S], Irs_Vrs);
        g(s[SI_SI], Irs_Vrs);
        g(s[SI_S], -Irs_Vrs);
        g(s[S_SI], -Irs_Vrs);

        if self.excess {
            // Ixf1 (no `VBICtype` factor in the C, see docs/vbic.md)
            g(s[XF1_BI], Ixf1_Vbei);
            g(s[XF1_EI], -Ixf1_Vbei);
            g(s[XF1_BI], Ixf1_Vbci);
            g(s[XF1_CI], -Ixf1_Vbci);
            g(s[XF1_XF2], Ixf1_Vxf2);
            g(s[XF1_XF1], Ixf1_Vxf1);
            // Ixf2
            g(s[XF2_XF2], Ixf2_Vxf2);
            g(s[XF2_XF1], Ixf2_Vxf1);
        }

        if self.selfheat {
            let t = &self.th;
            // Ibe
            g(s[BI_T], t.Ibe_Vrth);
            g(s[EI_T], -t.Ibe_Vrth);
            // Ibex
            g(s[BX_T], t.Ibex_Vrth);
            g(s[EI_T], -t.Ibex_Vrth);
            // Iciei
            g(s[CI_T], Iciei_Vrth);
            g(s[EI_T], -Iciei_Vrth);
            // Ibc
            g(s[BI_T], t.Ibc_Vrth);
            g(s[CI_T], -t.Ibc_Vrth);
            // Ibep
            g(s[BX_T], t.Ibep_Vrth);
            g(s[BP_T], -t.Ibep_Vrth);
            // Rcx
            g(s[C_T], t.Ircx_Vrth);
            g(s[CX_T], -t.Ircx_Vrth);
            // Irci
            g(s[CX_T], t.Irci_Vrth);
            g(s[CI_T], -t.Irci_Vrth);
            // Rbx
            g(s[B_T], t.Irbx_Vrth);
            g(s[BX_T], -t.Irbx_Vrth);
            // Irbi
            g(s[BX_T], t.Irbi_Vrth);
            g(s[BI_T], -t.Irbi_Vrth);
            // Re
            g(s[E_T], t.Ire_Vrth);
            g(s[EI_T], -t.Ire_Vrth);
            // Irbp
            g(s[BP_T], t.Irbp_Vrth);
            g(s[CX_T], -t.Irbp_Vrth);
            // Ibcp
            g(s[SI_T], t.Ibcp_Vrth);
            g(s[BP_T], -t.Ibcp_Vrth);
            // Iccp
            g(s[BX_T], t.Iccp_Vrth);
            g(s[SI_T], -t.Iccp_Vrth);
            // Rs
            g(s[S_T], t.Irs_Vrth);
            g(s[SI_T], -t.Irs_Vrth);
            if self.excess {
                g(s[XF1_T], Ixf1_Vrth);
            }
            // Rth
            g(s[T_T], t.Irth_Vrth);
            // Ith (the Vbcp/Vcep columns land on the external
            // substrate node, as in the C)
            g(s[T_T], -t.Ith_Vrth);
            g(s[T_BI], -t.Ith_Vbei);
            g(s[T_EI], t.Ith_Vbei);
            g(s[T_BI], -t.Ith_Vbci);
            g(s[T_CI], t.Ith_Vbci);
            g(s[T_CI], -t.Ith_Vcei);
            g(s[T_EI], t.Ith_Vcei);
            g(s[T_BX], -t.Ith_Vbex);
            g(s[T_EI], t.Ith_Vbex);
            g(s[T_BX], -t.Ith_Vbep);
            g(s[T_BP], t.Ith_Vbep);
            g(s[T_S], -t.Ith_Vbcp);
            g(s[T_BP], t.Ith_Vbcp);
            g(s[T_BX], -t.Ith_Vcep);
            g(s[T_S], t.Ith_Vcep);
            g(s[T_CX], -t.Ith_Vrci);
            g(s[T_CI], t.Ith_Vrci);
            g(s[T_BI], -t.Ith_Vbcx);
            g(s[T_CX], t.Ith_Vbcx);
            g(s[T_BX], -t.Ith_Vrbi);
            g(s[T_BI], t.Ith_Vrbi);
            g(s[T_BP], -t.Ith_Vrbp);
            g(s[T_CX], t.Ith_Vrbp);
            g(s[T_C], -t.Ith_Vrcx);
            g(s[T_CX], t.Ith_Vrcx);
            g(s[T_B], -t.Ith_Vrbx);
            g(s[T_BX], t.Ith_Vrbx);
            g(s[T_E], -t.Ith_Vre);
            g(s[T_EI], t.Ith_Vre);
            g(s[T_S], -t.Ith_Vrs);
            g(s[T_SI], t.Ith_Vrs);
        }

        // The complex part.
        let XQbe_Vbei = s0(CQBE);
        let XQbe_Vbci = s0(CQBECI);
        let XQbex_Vbex = s0(CQBEX);
        let XQbc_Vbci = s0(CQBC);
        let XQbcx_Vbcx = s0(CQBCX);
        let XQbep_Vbep = s0(CQBEP);
        let XQbep_Vbci = s0(CQBEPCI);
        let XQbcp_Vbcp = s0(CQBCP);
        let XQbeo_Vbe = s0(CQBEO);
        let XQbco_Vbc = s0(CQBCO);
        let XQxf1_Vxf1 = s0(CQXF1);
        let XQxf2_Vxf2 = s0(CQXF2);
        // Qbe
        c(s[BI_BI], XQbe_Vbei);
        c(s[BI_EI], -XQbe_Vbei);
        c(s[BI_BI], XQbe_Vbci);
        c(s[BI_CI], -XQbe_Vbci);
        c(s[EI_BI], -XQbe_Vbei);
        c(s[EI_EI], XQbe_Vbei);
        c(s[EI_BI], -XQbe_Vbci);
        c(s[EI_CI], XQbe_Vbci);
        // Qbex
        c(s[BX_BX], XQbex_Vbex);
        c(s[BX_EI], -XQbex_Vbex);
        c(s[EI_BX], -XQbex_Vbex);
        c(s[EI_EI], XQbex_Vbex);
        // Qbc
        c(s[BI_BI], XQbc_Vbci);
        c(s[BI_CI], -XQbc_Vbci);
        c(s[CI_BI], -XQbc_Vbci);
        c(s[CI_CI], XQbc_Vbci);
        // Qbcx
        c(s[BI_BI], XQbcx_Vbcx);
        c(s[BI_CX], -XQbcx_Vbcx);
        c(s[CX_BI], -XQbcx_Vbcx);
        c(s[CX_CX], XQbcx_Vbcx);
        // Qbep
        c(s[BX_BX], XQbep_Vbep);
        c(s[BX_BP], -XQbep_Vbep);
        c(s[BX_BI], XQbep_Vbci);
        c(s[BX_CI], -XQbep_Vbci);
        c(s[BP_BX], -XQbep_Vbep);
        c(s[BP_BP], XQbep_Vbep);
        c(s[BP_BI], -XQbep_Vbci);
        c(s[BP_CI], XQbep_Vbci);
        // Qbcp
        c(s[SI_SI], XQbcp_Vbcp);
        c(s[SI_BP], -XQbcp_Vbcp);
        c(s[BP_SI], -XQbcp_Vbcp);
        c(s[BP_BP], XQbcp_Vbcp);
        // Qbeo
        c(s[B_B], XQbeo_Vbe);
        c(s[E_E], XQbeo_Vbe);
        c(s[B_E], -XQbeo_Vbe);
        c(s[E_B], -XQbeo_Vbe);
        // Qbco
        c(s[B_B], XQbco_Vbc);
        c(s[C_C], XQbco_Vbc);
        c(s[B_C], -XQbco_Vbc);
        c(s[C_B], -XQbco_Vbc);
        if self.excess {
            c(s[XF1_XF1], XQxf1_Vxf1);
            c(s[XF2_XF2], XQxf2_Vxf2);
        }
        if self.selfheat {
            let t = &self.th;
            c(s[T_T], t.capcth);
            c(s[BI_T], t.capqbeth);
            c(s[EI_T], -t.capqbeth);
            c(s[BX_T], t.capqbexth);
            c(s[EI_T], -t.capqbexth);
            c(s[BI_T], t.capqbcth);
            c(s[CI_T], -t.capqbcth);
            c(s[BI_T], t.capqbcxth);
            c(s[CX_T], -t.capqbcxth);
            c(s[BX_T], t.capqbepth);
            c(s[BP_T], -t.capqbepth);
            c(s[SI_T], t.capqbcpth);
            c(s[BP_T], -t.capqbcpth);
        }
        if self.v.pin != 0 {
            g(s[PIN], 1.0);
        }
    }
}

impl crate::devices::Device for VbicI {
    fn par_load(&self) -> bool {
        true
    }

    /// `VBICgetic` (`vbicgetic.c`): VBE and VCE not given on the card
    /// from the external nodes.
    fn set_ic(&mut self, rhs: &[f64]) {
        use crate::devices::getic;
        let g = self.ic_given;
        getic(g, 0, &mut self.ic_vbe, rhs, self.v.base, self.v.emit);
        getic(g, 1, &mut self.ic_vce, rhs, self.v.coll, self.v.emit);
    }

    fn set_ic_given(&mut self, given: crate::devices::IcGiven) {
        self.ic_given = given;
    }

    fn alloc(&mut self, mb: &mut MatrixBuilder) {
        let v = self.v;
        let (c, b, e, s) = (v.coll, v.base, v.emit, v.subs);
        let (cx, ci, bx, bi, ei, bp, si) = (
            v.coll_cx, v.coll_ci, v.base_bx, v.base_bi, v.emit_ei, v.base_bp, v.subs_si,
        );
        let mut sl = [TRASH; NUM_SLOTS];
        let base = [
            (c, c),
            (b, b),
            (e, e),
            (s, s),
            (cx, cx),
            (ci, ci),
            (bx, bx),
            (bi, bi),
            (ei, ei),
            (bp, bp),
            (si, si),
            (b, e),
            (e, b),
            (b, c),
            (c, b),
            (c, cx),
            (b, bx),
            (e, ei),
            (s, si),
            (cx, ci),
            (cx, bx),
            (cx, bi),
            (cx, bp),
            (ci, bi),
            (ci, ei),
            (bx, bi),
            (bx, ei),
            (bx, bp),
            (bx, si),
            (bi, ei),
            (bp, si),
            (cx, c),
            (bx, b),
            (ei, e),
            (si, s),
            (ci, cx),
            (bi, cx),
            (bp, cx),
            (bx, ci),
            (bi, ci),
            (ei, ci),
            (bp, ci),
            (bi, bx),
            (ei, bx),
            (bp, bx),
            (si, bx),
            (ei, bi),
            (bp, bi),
            (si, ci),
            (si, bi),
            (si, bp),
        ];
        for (k, (r, cc)) in base.iter().enumerate() {
            sl[k] = mb.slot(*r, *cc);
        }
        if self.selfheat {
            let t = v.temp;
            let th = [
                (c, t),
                (b, t),
                (e, t),
                (s, t),
                (ci, t),
                (cx, t),
                (bi, t),
                (bx, t),
                (bp, t),
                (ei, t),
                (si, t),
                (t, c),
                (t, ci),
                (t, cx),
                (t, bi),
                (t, b),
                (t, bx),
                (t, bp),
                (t, e),
                (t, ei),
                (t, s),
                (t, si),
                (t, t),
            ];
            for (k, (r, cc)) in th.iter().enumerate() {
                sl[C_T + k] = mb.slot(*r, *cc);
            }
            if self.excess {
                sl[XF1_T] = mb.slot(v.xf1, t);
            }
        }
        if self.excess {
            let (x1, x2) = (v.xf1, v.xf2);
            let xf = [
                (x1, x1),
                (x1, bi),
                (x1, ei),
                (x1, ci),
                (x1, x2),
                (x2, x1),
                (x2, x2),
                (ei, x2),
                (ci, x2),
            ];
            for (k, (r, cc)) in xf.iter().enumerate() {
                sl[XF1_XF1 + k] = mb.slot(*r, *cc);
            }
        }
        if v.pin != 0 {
            sl[PIN] = mb.slot(v.pin, v.pin);
        }
        self.s = sl;
    }

    fn load(&mut self, ctx: &mut crate::matrix::LoadCtx) {
        self.do_load(ctx);
    }

    /// `VBICtrunc`: the eight junction and overlap charges (not the
    /// thermal or excess-phase charges, as in the C).
    fn trunc(&self, t: &crate::integ::TerrCtx, time_step: &mut f64) {
        for q in [QBE, QBEX, QBC, QBCX, QBEP, QBEO, QBCO, QBCP] {
            crate::integ::terr(self.st + q, t, time_step);
        }
    }

    /// `VBICconvTest` (`vbicconv.c`): the ten branch currents against
    /// their linearization, with the voltages read from `CKTrhsOld`
    /// like the C (the iterate the load saw, before limiting).
    #[allow(non_snake_case)]
    fn conv_test(&self, ctx: &crate::matrix::ConvCtx) -> bool {
        let typ = self.typ;
        let st = self.st;
        let v = &self.v;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let r = ctx.sol;
        let Vbei = typ * (r[v.base_bi] - r[v.emit_ei]);
        let Vbex = typ * (r[v.base_bx] - r[v.emit_ei]);
        let Vbci = typ * (r[v.base_bi] - r[v.coll_ci]);
        let Vbcx = typ * (r[v.base_bi] - r[v.coll_cx]);
        let Vbep = typ * (r[v.base_bx] - r[v.base_bp]);
        let Vrci = typ * (r[v.coll_cx] - r[v.coll_ci]);
        let Vrbi = typ * (r[v.base_bx] - r[v.base_bi]);
        let Vrbp = typ * (r[v.base_bp] - r[v.coll_cx]);
        let Vbcp = typ * (r[v.subs_si] - r[v.base_bp]);
        let delvbei = Vbei - s0(VBEI);
        let delvbex = Vbex - s0(VBEX);
        let delvbci = Vbci - s0(VBCI);
        let delvbcx = Vbcx - s0(VBCX);
        let delvbep = Vbep - s0(VBEP);
        let delvrci = Vrci - s0(VRCI);
        let delvrbi = Vrbi - s0(VRBI);
        let delvrbp = Vrbp - s0(VRBP);
        let delvbcp = Vbcp - s0(VBCP);
        let ibehat = s0(IBE) + s0(IBE_VBEI) * delvbei;
        let ibexhat = s0(IBEX) + s0(IBEX_VBEX) * delvbex;
        let icieihat = s0(ICIEI) + s0(ICIEI_VBEI) * delvbei + s0(ICIEI_VBCI) * delvbci;
        let ibchat = s0(IBC) + s0(IBC_VBEI) * delvbei + s0(IBC_VBCI) * delvbci;
        let ibephat = s0(IBEP) + s0(IBEP_VBEP) * delvbep;
        let ircihat =
            s0(IRCI) + s0(IRCI_VRCI) * delvrci + s0(IRCI_VBCX) * delvbcx + s0(IRCI_VBCI) * delvbci;
        let irbihat =
            s0(IRBI) + s0(IRBI_VRBI) * delvrbi + s0(IRBI_VBEI) * delvbei + s0(IRBI_VBCI) * delvbci;
        let irbphat =
            s0(IRBP) + s0(IRBP_VRBP) * delvrbp + s0(IRBP_VBEP) * delvbep + s0(IRBP_VBCI) * delvbci;
        let ibcphat = s0(IBCP) + s0(IBCP_VBCP) * delvbcp;
        let iccphat =
            s0(ICCP) + s0(ICCP_VBEP) * delvbep + s0(ICCP_VBCI) * delvbci + s0(ICCP_VBCP) * delvbcp;
        for (hat, now) in [
            (ibehat, s0(IBE)),
            (ibexhat, s0(IBEX)),
            (icieihat, s0(ICIEI)),
            (ibchat, s0(IBC)),
            (ibephat, s0(IBEP)),
            (ircihat, s0(IRCI)),
            (irbihat, s0(IRBI)),
            (irbphat, s0(IRBP)),
            (ibcphat, s0(IBCP)),
            (iccphat, s0(ICCP)),
        ] {
            let tol = ctx.reltol * f64::max(hat.abs(), now.abs()) + ctx.abstol;
            if (hat - now).abs() > tol {
                return false;
            }
        }
        true
    }

    /// `VBICacLoad` (`vbicacld.c`).
    fn load_ac(&mut self, ctx: &mut AcCtx) {
        let omega = ctx.omega;
        let (g, b) = (&mut *ctx.g, &mut *ctx.b);
        self.small_signal(
            ctx.states,
            |sl, v| g[sl.0] += v,
            |sl, v| b[sl.0] += v * omega,
        );
    }

    /// `VBICpzLoad` (`vbicpzld.c`): the AC stamps with `s` in place
    /// of `jω`.
    fn load_pz(&mut self, ctx: &mut crate::matrix::PzCtx) -> Result<(), &'static str> {
        let sv = ctx.s;
        let (g, b) = (&mut *ctx.g, &mut *ctx.b);
        // Both closures write the real array: split it by hand.
        let cell = std::cell::RefCell::new(g);
        self.small_signal(
            ctx.states,
            |sl, v| cell.borrow_mut()[sl.0] += v,
            |sl, v| {
                cell.borrow_mut()[sl.0] += v * sv.re;
                b[sl.0] += v * sv.im;
            },
        );
        Ok(())
    }

    /// `VBICnoise` (`vbicnoise.c`): thermal noise of the seven
    /// resistances at `CKTtemp` plus the instance offset (the `RBP`
    /// source sits across the emitter resistance and the `RS` source
    /// is left out of the density total, both as in the C), shot
    /// noise of `Iciei`, `Ibe`, `Ibep` and `Iccp`, and flicker noise
    /// of `Ibe` and `Ibep`.
    fn noise_suffixes(&self) -> Option<&'static [&'static str]> {
        Some(VBIC_NOISE_SUFFIXES)
    }

    fn noise_dens(&self, ctx: &crate::matrix::NoiseCtx) -> Vec<f64> {
        const N_MINLOG: f64 = 1e-38;
        let st = self.st;
        let v = &self.v;
        let s0 = |o: usize| ctx.states.get(0, st + o);
        let temp = ctx.temp + self.noise_dtemp;
        let rc = ctx.thermal_at(v.coll_cx, v.coll, s0(IRCX_VRCX), temp);
        let rci = ctx.thermal_at(v.coll_cx, v.coll_ci, s0(IRCI_VRCI), temp);
        let rb = ctx.thermal_at(v.base_bx, v.base, s0(IRBX_VRBX), temp);
        let rbi = ctx.thermal_at(v.base_bx, v.base_bi, s0(IRBI_VRBI), temp);
        let re = ctx.thermal_at(v.emit_ei, v.emit, s0(IRE_VRE), temp);
        let rbp = ctx.thermal_at(v.emit_ei, v.emit, s0(IRBP_VRBP), temp);
        let rs = ctx.thermal_at(v.subs_si, v.subs, s0(IRS_VRS), temp);
        let ic = ctx.shot(v.coll_ci, v.emit_ei, s0(ICIEI));
        let ib = ctx.shot(v.base_bi, v.emit_ei, s0(IBE));
        let ibep = ctx.shot(v.base_bx, v.base_bp, s0(IBEP));
        let iccp = ctx.shot(v.base_bx, v.subs_si, s0(ICCP));
        let m = self.m;
        let fl =
            |i: f64| m * self.kfn * f64::exp(self.afn * f64::ln(f64::max((i / m).abs(), N_MINLOG)));
        let flbe = ctx.flicker(v.base_bi, v.emit_ei, fl(s0(IBE)), self.bfn);
        let flbep = ctx.flicker(v.base_bx, v.base_bp, fl(s0(IBEP)), self.bfn);
        let total = rc + rci + rb + rbi + re + rbp + ic + ib + ibep + flbe + flbep;
        vec![
            rc, rci, rb, rbi, re, rbp, rs, ic, ib, ibep, iccp, flbe, flbep, total,
        ]
    }
}
