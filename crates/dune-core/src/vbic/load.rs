//! `VBICload` (`vbicload.c`, lines 41-1480): bias resolution and
//! limiting, the device equations (`fj`), excess phase, gmin, charge
//! integration and the stamps, statement for statement.

use super::fj::{FjBias, vbic_4t_et_cf_fj};
use super::*;
use crate::diode::pnjlim;
use crate::matrix::{AnKind, InitMode, LoadCtx};
use crate::vdmos::limitlog;

impl VbicI {
    #[allow(clippy::too_many_lines, non_snake_case)]
    pub(super) fn do_load(&mut self, ctx: &mut LoadCtx) {
        let st = self.st;
        let typ = self.typ;
        let v = self.v;
        let selfheat = self.selfheat;
        let excess = self.excess;
        let vt = self.temp * crate::consts::KOVERQ;
        let s0 = |ctx: &LoadCtx, o: usize| ctx.states.get(0, st + o);
        let s1 = |ctx: &LoadCtx, o: usize| ctx.states.get(1, st + o);
        let sol = ctx.sol;

        let mut gbcx = 0.0;
        let mut cbcx = 0.0;
        let mut gqbeo = 0.0;
        let mut gqbco = 0.0;
        let mut Icth = 0.0;
        let mut Icth_Vrth = 0.0;
        // `Vrth` is declared once with `= 0.0` at the top of the C
        // function and only assigned for self-heating instances; Dune
        // starts every instance at 0 (docs/vbic.md, Fidelidade).
        let mut Vrth = 0.0;
        let Vxf1;
        let Vxf2;

        let SCALE = self.scale;
        let mut icheck = true;
        let (Vbe, Vbc, mut Vbei, mut Vbex, mut Vbci, mut Vbcx, mut Vbep);
        let (Vrci, Vrbi, Vrbp, Vrcx, Vrbx, Vre, mut Vbcp, Vrs);
        let rhs_v = |a: Var, b: Var| typ * (sol[a] - sol[b]);
        let smsig = ctx.mode == InitMode::Smsig;
        if smsig {
            Vbe = rhs_v(v.base, v.emit);
            Vbc = rhs_v(v.base, v.coll);
            Vbei = s0(ctx, VBEI);
            Vbex = s0(ctx, VBEX);
            Vbci = s0(ctx, VBCI);
            Vbcx = s0(ctx, VBCX);
            Vbep = s0(ctx, VBEP);
            Vrci = s0(ctx, VRCI);
            Vrbi = s0(ctx, VRBI);
            Vrbp = s0(ctx, VRBP);
            Vrcx = rhs_v(v.coll, v.coll_cx);
            Vrbx = rhs_v(v.base, v.base_bx);
            Vre = rhs_v(v.emit, v.emit_ei);
            Vbcp = s0(ctx, VBCP);
            Vrs = rhs_v(v.subs, v.subs_si);
            if selfheat {
                Vrth = s0(ctx, VRTH);
            }
            Vxf1 = sol[v.xf1];
            Vxf2 = sol[v.xf2];
        } else if ctx.mode == InitMode::Tran {
            Vbe = rhs_v(v.base, v.emit);
            Vbc = rhs_v(v.base, v.coll);
            Vbei = s1(ctx, VBEI);
            Vbex = s1(ctx, VBEX);
            Vbci = s1(ctx, VBCI);
            Vbcx = s1(ctx, VBCX);
            Vbep = s1(ctx, VBEP);
            Vrci = s1(ctx, VRCI);
            Vrbi = s1(ctx, VRBI);
            Vrbp = s1(ctx, VRBP);
            Vrcx = rhs_v(v.coll, v.coll_cx);
            Vrbx = rhs_v(v.base, v.base_bx);
            Vre = rhs_v(v.emit, v.emit_ei);
            Vbcp = s1(ctx, VBCP);
            Vrs = rhs_v(v.subs, v.subs_si);
            if selfheat {
                Vrth = s1(ctx, VRTH);
            }
            Vxf1 = sol[v.xf1];
            Vxf2 = sol[v.xf2];
        } else if ctx.mode == InitMode::Jct && ctx.kind == AnKind::TranOp && ctx.uic {
            Vbe = typ * self.ic_vbe;
            Vbei = Vbe;
            Vbex = Vbe;
            let Vce = typ * self.ic_vce;
            Vbc = Vbe - Vce;
            Vbci = Vbc;
            Vbcx = Vbc;
            Vbep = 0.0;
            Vbcp = 0.0;
            Vrci = 0.0;
            Vrbi = 0.0;
            Vrbp = 0.0;
            Vrcx = 0.0;
            Vrbx = 0.0;
            Vre = 0.0;
            Vrs = 0.0;
            Vrth = 0.0;
            Icth = 0.0;
            Icth_Vrth = 0.0;
            Vxf1 = 0.0;
            Vxf2 = 0.0;
        } else if ctx.mode == InitMode::Jct && !self.off {
            Vbe = typ * self.tvcrit;
            Vbei = Vbe;
            Vbex = Vbe;
            Vbc = 0.0;
            Vbcx = 0.0;
            Vbep = 0.0;
            Vbci = -typ * self.tvcrit;
            Vbcp = Vbc - Vbe;
            Vrci = 0.0;
            Vrbi = 0.0;
            Vrbp = 0.0;
            Vrcx = 0.0;
            Vrbx = 0.0;
            Vre = 0.0;
            Vrs = 0.0;
            Vrth = 0.0;
            Icth = 0.0;
            Icth_Vrth = 0.0;
            Vxf1 = 0.0;
            Vxf2 = 0.0;
        } else if ctx.mode == InitMode::Jct || (ctx.mode == InitMode::Fix && self.off) {
            Vbe = 0.0;
            Vbei = Vbe;
            Vbex = Vbe;
            Vbc = 0.0;
            Vbci = Vbc;
            Vbcx = Vbc;
            Vbep = 0.0;
            Vbcp = 0.0;
            Vrci = 0.0;
            Vrbi = 0.0;
            Vrbp = 0.0;
            Vrcx = 0.0;
            Vrbx = 0.0;
            Vre = 0.0;
            Vrs = 0.0;
            Vrth = 0.0;
            Icth = 0.0;
            Icth_Vrth = 0.0;
            Vxf1 = 0.0;
            Vxf2 = 0.0;
        } else {
            if ctx.mode == InitMode::Pred {
                let xfact = ctx.pred_xfact;
                let pred = |ctx: &LoadCtx, o: usize| {
                    (1.0 + xfact) * s1(ctx, o) - xfact * ctx.states.get(2, st + o)
                };
                Vbei = pred(ctx, VBEI);
                Vbex = pred(ctx, VBEX);
                Vbci = pred(ctx, VBCI);
                Vbcx = pred(ctx, VBCX);
                Vbep = pred(ctx, VBEP);
                Vrci = pred(ctx, VRCI);
                Vrbi = pred(ctx, VRBI);
                Vrbp = pred(ctx, VRBP);
                Vbcp = pred(ctx, VBCP);
                let copy = |ctx: &mut LoadCtx, o: usize| {
                    let x = ctx.states.get(1, st + o);
                    ctx.states.set(0, st + o, x);
                };
                if selfheat {
                    Vrth = pred(ctx, VRTH);
                    copy(ctx, VRTH);
                }
                // The C predicts `Vxf2` here; the unconditional read
                // of `CKTrhsOld` below overwrites it (and the
                // self-heating `Vrth` the same way).
                for o in [
                    VBEI, VBEX, VBCI, VBCX, VBEP, VRCI, VRBI, VRBP, VBCP, IBE, IBE_VBEI, IBEX,
                    IBEX_VBEX, IBC, IBC_VBCI, IBC_VBEI, IBEP, IBEP_VBEP, IRCI, IRCI_VRCI,
                    IRCI_VBCI, IRCI_VBCX, IRBI, IRBI_VRBI, IRBI_VBEI, IRBI_VBCI, IRBP, IRBP_VRBP,
                    IRBP_VBEP, IRBP_VBCI, IBCP, IBCP_VBCP, ICCP, ICCP_VBEP, ICCP_VBCI, ICCP_VBCP,
                    GQBEO, GQBCO, IRCX_VRCX, IRBX_VRBX, IRS_VRS, IRE_VRE,
                ] {
                    copy(ctx, o);
                }
                if selfheat {
                    copy(ctx, QCTH);
                }
                for o in [ICIEI, ICIEI_VBEI, ICIEI_VBCI, ICIEI_VXF2] {
                    copy(ctx, o);
                }
                if excess {
                    for o in [
                        VXF1, QXF1, CQXF1, GQXF1, IXF1_VBEI, IXF1_VBCI, IXF1_VXF2, IXF1_VXF1, VXF2,
                        QXF2, CQXF2, GQXF2, IXF2_VXF1, IXF2_VXF2,
                    ] {
                        copy(ctx, o);
                    }
                    if selfheat {
                        copy(ctx, IXF1_VRTH);
                    }
                }
            } else {
                // Compute new nonlinear branch voltages.
                Vbei = rhs_v(v.base_bi, v.emit_ei);
                Vbex = rhs_v(v.base_bx, v.emit_ei);
                Vbci = rhs_v(v.base_bi, v.coll_ci);
                Vbcx = rhs_v(v.base_bi, v.coll_cx);
                Vbep = rhs_v(v.base_bx, v.base_bp);
                Vrci = rhs_v(v.coll_cx, v.coll_ci);
                Vrbi = rhs_v(v.base_bx, v.base_bi);
                Vrbp = rhs_v(v.base_bp, v.coll_cx);
                Vbcp = rhs_v(v.subs_si, v.base_bp);
                if selfheat {
                    Vrth = sol[v.temp];
                }
            }
            // (The `del…`/`…hat` values the C forms here only feed its
            // device bypass, which Dune does not have — `CKTbypass`
            // is 0 by default in ngspice-47 too.)
            Vbe = rhs_v(v.base, v.emit);
            Vbc = rhs_v(v.base, v.coll);
            Vrcx = rhs_v(v.coll, v.coll_cx);
            Vrbx = rhs_v(v.base, v.base_bx);
            Vre = rhs_v(v.emit, v.emit_ei);
            Vrs = rhs_v(v.subs, v.subs_si);
            if selfheat {
                Vrth = sol[v.temp];
            }
            Vxf1 = sol[v.xf1];
            Vxf2 = sol[v.xf2];

            // Limit nonlinear branch voltages.
            let (mut ichk1, mut ichk2, mut ichk3, mut ichk4, mut ichk5) =
                (true, true, true, true, true);
            let mut ichk6 = false;
            Vbei = pnjlim(Vbei, s0(ctx, VBEI), vt, self.tvcrit, &mut icheck);
            Vbex = pnjlim(Vbex, s0(ctx, VBEX), vt, self.tvcrit, &mut ichk1);
            Vbci = pnjlim(Vbci, s0(ctx, VBCI), vt, self.tvcrit, &mut ichk2);
            Vbcx = pnjlim(Vbcx, s0(ctx, VBCX), vt, self.tvcrit, &mut ichk3);
            Vbep = pnjlim(Vbep, s0(ctx, VBEP), vt, self.tvcrit, &mut ichk4);
            Vbcp = pnjlim(Vbcp, s0(ctx, VBCP), vt, self.tvcrit, &mut ichk5);
            if selfheat {
                ichk6 = true;
                Vrth = limitlog(Vrth, s0(ctx, VRTH), 100.0, &mut ichk6);
            }
            if ichk1 || ichk2 || ichk3 || ichk4 || ichk5 || ichk6 {
                icheck = true;
            }
        }

        // Determine dc current and derivatives.
        let Vcei = Vbei - Vbci;
        let Vcep = Vbep - Vbcp;
        let o = vbic_4t_et_cf_fj(
            &self.p,
            &FjBias {
                Vrth,
                Vbei,
                Vbex,
                Vbci,
                Vbep,
                Vbcp,
                Vrcx,
                Vbcx,
                Vrci,
                Vrbx,
                Vrbi,
                Vre,
                Vrbp,
                Vrs,
                Vbe,
                Vbc,
                Vcei,
                Vcep,
            },
            SCALE,
        );
        let mut Ibe = o.Ibe;
        let mut Ibe_Vbei = o.Ibe_Vbei;
        let mut Ibex = o.Ibex;
        let mut Ibex_Vbex = o.Ibex_Vbex;
        let mut Ibc = o.Ibc;
        let mut Ibc_Vbci = o.Ibc_Vbci;
        let Ibc_Vbei = o.Ibc_Vbei;
        let mut Ibep = o.Ibep;
        let mut Ibep_Vbep = o.Ibep_Vbep;
        let mut Irci = o.Irci;
        let mut Irci_Vrci = o.Irci_Vrci;
        let mut Irci_Vbci = o.Irci_Vbci;
        let mut Irci_Vbcx = o.Irci_Vbcx;
        let mut Ibcp = o.Ibcp;
        let mut Ibcp_Vbcp = o.Ibcp_Vbcp;
        let (Itzf, Itzf_Vbei, Itzf_Vbci, Itzf_Vrth) =
            (o.Itzf, o.Itzf_Vbei, o.Itzf_Vbci, o.Itzf_Vrth);
        let (Itzr, Itzr_Vbei, Itzr_Vbci, Itzr_Vrth) =
            (o.Itzr, o.Itzr_Vbei, o.Itzr_Vbci, o.Itzr_Vrth);

        // Excess Phase calculation.
        let (mut Ixf1, mut Ixf1_Vxf1, Ixf1_Vxf2, Ixf1_Vbei, Ixf1_Vbci, Ixf1_Vrth);
        let (mut Ixf2, Ixf2_Vxf1, mut Ixf2_Vxf2);
        let (Qxf1, Qxf1_Vxf1, Qxf2, Qxf2_Vxf2);
        let (Itxf, Itxf_Vbei, Itxf_Vbci, Itxf_Vrth, Itxf_Vxf2);
        if excess {
            Ixf1 = Vxf2 - Itzf;
            Ixf1_Vxf1 = 0.0;
            Ixf1_Vxf2 = 1.0;
            let Ixf1_ditf = -1.0;
            Ixf1_Vbei = Ixf1_ditf * Itzf_Vbei;
            Ixf1_Vbci = Ixf1_ditf * Itzf_Vbci;
            Ixf1_Vrth = Ixf1_ditf * Itzf_Vrth;

            Ixf2 = Vxf2 - Vxf1;
            Ixf2_Vxf2 = 1.0;
            Ixf2_Vxf1 = -1.0;

            Qxf1 = self.td * Vxf1;
            Qxf1_Vxf1 = self.td;
            Qxf2 = self.td * Vxf2 / 3.0;
            Qxf2_Vxf2 = self.td / 3.0;

            Itxf = Vxf2;
            Itxf_Vbei = 0.0;
            Itxf_Vbci = 0.0;
            Itxf_Vrth = 0.0;
            Itxf_Vxf2 = 1.0;
        } else {
            Ixf1 = Vxf1;
            Ixf1_Vxf1 = 1.0;
            Ixf1_Vxf2 = 0.0;
            Ixf1_Vbei = 0.0;
            Ixf1_Vbci = 0.0;
            Ixf1_Vrth = 0.0;

            Ixf2 = Vxf2;
            Ixf2_Vxf2 = 1.0;
            Ixf2_Vxf1 = 0.0;

            Qxf1 = 0.0;
            Qxf1_Vxf1 = 0.0;
            Qxf2 = 0.0;
            Qxf2_Vxf2 = 0.0;

            Itxf = Itzf;
            Itxf_Vbei = Itzf_Vbei;
            Itxf_Vbci = Itzf_Vbci;
            Itxf_Vrth = Itzf_Vrth;
            Itxf_Vxf2 = 0.0;
        }

        let Iciei = Itxf - Itzr;
        let Iciei_Vbei = Itxf_Vbei - Itzr_Vbei;
        let Iciei_Vbci = Itxf_Vbci - Itzr_Vbci;
        let Iciei_Vrth = Itxf_Vrth - Itzr_Vrth;
        let Iciei_Vxf2 = Itxf_Vxf2;

        let gmin = ctx.gmin;
        Ibe += gmin * Vbei;
        Ibe_Vbei += gmin;
        Ibex += gmin * Vbex;
        Ibex_Vbex += gmin;
        Ibc += gmin * Vbci;
        Ibc_Vbci += gmin;
        Ibep += gmin * Vbep;
        Ibep_Vbep += gmin;
        Irci += gmin * Vrci;
        Irci_Vrci += gmin;
        Irci += gmin * Vbci;
        Irci_Vbci += gmin;
        Irci += gmin * Vbcx;
        Irci_Vbcx += gmin;
        Ibcp += gmin * Vbcp;
        Ibcp_Vbcp += gmin;

        let tranop_uic = ctx.kind == AnKind::TranOp && ctx.uic;
        if matches!(ctx.kind, AnKind::DcSweep | AnKind::Tran) || tranop_uic || smsig {
            // Charge storage elements.
            let set0 = |ctx: &mut LoadCtx, o: usize, x: f64| ctx.states.set(0, st + o, x);
            set0(ctx, QBE, o.Qbe);
            set0(ctx, QBEX, o.Qbex);
            set0(ctx, QBC, o.Qbc);
            set0(ctx, QBCX, o.Qbcx);
            set0(ctx, QBEP, o.Qbep);
            set0(ctx, QBEO, o.Qbeo);
            set0(ctx, QBCO, o.Qbco);
            set0(ctx, QBCP, o.Qbcp);
            if selfheat {
                set0(ctx, QCTH, o.Qcth);
            }
            if excess {
                set0(ctx, QXF1, Qxf1);
                set0(ctx, CQXF1, Qxf1_Vxf1);
                set0(ctx, QXF2, Qxf2);
                set0(ctx, CQXF2, Qxf2_Vxf2);
            }
            if selfheat {
                self.th.capcth = o.Qcth_Vrth;
            }

            // Store small-signal parameters.
            if !tranop_uic {
                if smsig {
                    set0(ctx, CQBE, o.Qbe_Vbei);
                    set0(ctx, CQBECI, o.Qbe_Vbci);
                    set0(ctx, CQBEX, o.Qbex_Vbex);
                    set0(ctx, CQBC, o.Qbc_Vbci);
                    set0(ctx, CQBCX, o.Qbcx_Vbcx);
                    set0(ctx, CQBEP, o.Qbep_Vbep);
                    set0(ctx, CQBEPCI, o.Qbep_Vbci);
                    set0(ctx, CQBEO, o.Qbeo_Vbe);
                    set0(ctx, CQBCO, o.Qbco_Vbc);
                    set0(ctx, CQBCP, o.Qbcp_Vbcp);
                    set0(ctx, CQXF1, Qxf1_Vxf1);
                    set0(ctx, CQXF2, Qxf2_Vxf2);
                    if selfheat {
                        let t = &mut self.th;
                        t.capcth = o.Qcth_Vrth;
                        t.capqbeth = o.Qbe_Vrth;
                        t.capqbexth = o.Qbex_Vrth;
                        t.capqbcth = o.Qbc_Vrth;
                        t.capqbcxth = o.Qbcx_Vrth;
                        t.capqbepth = o.Qbep_Vrth;
                        t.capqbcpth = o.Qbcp_Vrth;
                    }
                    return; // go to 1000
                }
                // Transient analysis.
                let init_tran = ctx.mode == InitMode::Tran;
                let copy01 = |ctx: &mut LoadCtx, o: usize| {
                    let x = ctx.states.get(0, st + o);
                    ctx.states.set(1, st + o, x);
                };
                if init_tran {
                    for q in [QBE, QBEX, QBC, QBCX, QBEP, QBEO, QBCO, QBCP] {
                        copy01(ctx, q);
                    }
                    if selfheat {
                        copy01(ctx, QCTH);
                    }
                    if excess {
                        copy01(ctx, QXF1);
                        copy01(ctx, QXF2);
                    }
                }
                // No integration if dc sweep, but keep evaluating
                // capacitances.
                if ctx.kind != AnKind::DcSweep {
                    let (geq, _) = ctx.integrate(o.Qbe_Vbei, st + QBE);
                    Ibe_Vbei += geq;
                    Ibe += s0(ctx, CQBE);

                    let (geq, _) = ctx.integrate(o.Qbex_Vbex, st + QBEX);
                    Ibex_Vbex += geq;
                    Ibex += s0(ctx, CQBEX);

                    let (geq, _) = ctx.integrate(o.Qbc_Vbci, st + QBC);
                    Ibc_Vbci += geq;
                    Ibc += s0(ctx, CQBC);

                    let (geq, _) = ctx.integrate(o.Qbcx_Vbcx, st + QBCX);
                    gbcx = geq;
                    cbcx = s0(ctx, CQBCX);

                    let (geq, _) = ctx.integrate(o.Qbep_Vbep, st + QBEP);
                    Ibep_Vbep += geq;
                    Ibep += s0(ctx, CQBEP);

                    let (geq, _) = ctx.integrate(o.Qbcp_Vbcp, st + QBCP);
                    Ibcp_Vbcp += geq;
                    Ibcp += s0(ctx, CQBCP);

                    if selfheat {
                        let (geq, _) = ctx.integrate(o.Qcth_Vrth, st + QCTH);
                        Icth_Vrth = geq;
                        Icth = s0(ctx, CQCTH);
                    }
                    if excess {
                        let (geq, _) = ctx.integrate(Qxf1_Vxf1, st + QXF1);
                        Ixf1_Vxf1 += geq;
                        Ixf1 += s0(ctx, CQXF1);
                        let (geq, _) = ctx.integrate(Qxf2_Vxf2, st + QXF2);
                        Ixf2_Vxf2 += geq;
                        Ixf2 += s0(ctx, CQXF2);
                    }
                }

                if init_tran {
                    for q in [CQBE, CQBEX, CQBC, CQBCX, CQBEP, CQBCP] {
                        copy01(ctx, q);
                    }
                    if selfheat {
                        copy01(ctx, CQCTH);
                    }
                    if excess {
                        copy01(ctx, CQXF1);
                        copy01(ctx, CQXF2);
                    }
                }
            }
        }

        // Check convergence.
        if (ctx.mode != InitMode::Fix || !self.off) && icheck {
            ctx.noncon = true;
        }

        // Charge storage for outer b-e and b-c junctions.
        if ctx.kind == AnKind::Tran {
            let (g, _) = ctx.integrate(o.Qbeo_Vbe, st + QBEO);
            gqbeo = g;
            let (g, _) = ctx.integrate(o.Qbco_Vbc, st + QBCO);
            gqbco = g;
            if ctx.mode == InitMode::Tran {
                for q in [CQBEO, CQBCO] {
                    let x = s0(ctx, q);
                    ctx.states.set(1, st + q, x);
                }
            }
        }

        {
            let set0 = |ctx: &mut LoadCtx, o: usize, x: f64| ctx.states.set(0, st + o, x);
            set0(ctx, VRTH, Vrth);
            set0(ctx, VBEI, Vbei);
            set0(ctx, VBEX, Vbex);
            set0(ctx, VBCI, Vbci);
            set0(ctx, VBCX, Vbcx);
            set0(ctx, VBEP, Vbep);
            set0(ctx, VRCI, Vrci);
            set0(ctx, VRBI, Vrbi);
            set0(ctx, VRBP, Vrbp);
            set0(ctx, VBCP, Vbcp);
            if excess {
                set0(ctx, VXF1, Vxf1);
                set0(ctx, VXF2, Vxf2);
            }
            set0(ctx, IBE, Ibe);
            set0(ctx, IBE_VBEI, Ibe_Vbei);
            set0(ctx, IBEX, Ibex);
            set0(ctx, IBEX_VBEX, Ibex_Vbex);
            set0(ctx, ICIEI, Iciei);
            set0(ctx, ICIEI_VBEI, Iciei_Vbei);
            set0(ctx, ICIEI_VBCI, Iciei_Vbci);
            set0(ctx, ICIEI_VRTH, Iciei_Vrth);
            set0(ctx, ICIEI_VXF2, Iciei_Vxf2);
            set0(ctx, IBC, Ibc);
            set0(ctx, IBC_VBCI, Ibc_Vbci);
            set0(ctx, IBC_VBEI, Ibc_Vbei);
            set0(ctx, IBEP, Ibep);
            set0(ctx, IBEP_VBEP, Ibep_Vbep);
            set0(ctx, IRCI, Irci);
            set0(ctx, IRCI_VRCI, Irci_Vrci);
            set0(ctx, IRCI_VBCI, Irci_Vbci);
            set0(ctx, IRCI_VBCX, Irci_Vbcx);
            set0(ctx, IRBI, o.Irbi);
            set0(ctx, IRBI_VRBI, o.Irbi_Vrbi);
            set0(ctx, IRBI_VBEI, o.Irbi_Vbei);
            set0(ctx, IRBI_VBCI, o.Irbi_Vbci);
            set0(ctx, IRBP, o.Irbp);
            set0(ctx, IRBP_VRBP, o.Irbp_Vrbp);
            set0(ctx, IRBP_VBEP, o.Irbp_Vbep);
            set0(ctx, IRBP_VBCI, o.Irbp_Vbci);
            set0(ctx, IBCP, Ibcp);
            set0(ctx, IBCP_VBCP, Ibcp_Vbcp);
            set0(ctx, ICCP, o.Iccp);
            set0(ctx, ICCP_VBEP, o.Iccp_Vbep);
            set0(ctx, ICCP_VBCI, o.Iccp_Vbci);
            set0(ctx, ICCP_VBCP, o.Iccp_Vbcp);
            set0(ctx, GQBEO, gqbeo);
            set0(ctx, GQBCO, gqbco);
            set0(ctx, IRCX_VRCX, o.Ircx_Vrcx);
            set0(ctx, IRBX_VRBX, o.Irbx_Vrbx);
            set0(ctx, IRS_VRS, o.Irs_Vrs);
            set0(ctx, IRE_VRE, o.Ire_Vre);

            set0(ctx, IXF1, Ixf1);
            set0(ctx, IXF1_VBEI, Ixf1_Vbei);
            set0(ctx, IXF1_VBCI, Ixf1_Vbci);
            set0(ctx, IXF1_VXF2, Ixf1_Vxf2);
            set0(ctx, IXF1_VXF1, Ixf1_Vxf1);
            set0(ctx, IXF1_VRTH, Ixf1_Vrth);

            set0(ctx, IXF2, Ixf2);
            set0(ctx, IXF2_VXF1, Ixf2_Vxf1);
            set0(ctx, IXF2_VXF2, Ixf2_Vxf2);

            if selfheat {
                set0(ctx, CQCTH, Icth);
                set0(ctx, ICTH_VRTH, Icth_Vrth);
                self.th.Ibe_Vrth = o.Ibe_Vrth;
                self.th.Ibex_Vrth = o.Ibex_Vrth;
                self.th.Ibc_Vrth = o.Ibc_Vrth;
                self.th.Ibep_Vrth = o.Ibep_Vrth;
                self.th.Ircx_Vrth = o.Ircx_Vrth;
                self.th.Irci_Vrth = o.Irci_Vrth;
                self.th.Irbx_Vrth = o.Irbx_Vrth;
                self.th.Irbi_Vrth = o.Irbi_Vrth;
                self.th.Ire_Vrth = o.Ire_Vrth;
                self.th.Irbp_Vrth = o.Irbp_Vrth;
                self.th.Ibcp_Vrth = o.Ibcp_Vrth;
                self.th.Iccp_Vrth = o.Iccp_Vrth;
                self.th.Irs_Vrth = o.Irs_Vrth;
                self.th.Irth_Vrth = o.Irth_Vrth;
                self.th.Ith_Vrth = o.Ith_Vrth;
                self.th.Ith_Vbei = o.Ith_Vbei;
                self.th.Ith_Vbci = o.Ith_Vbci;
                self.th.Ith_Vcei = o.Ith_Vcei;
                self.th.Ith_Vbex = o.Ith_Vbex;
                self.th.Ith_Vbep = o.Ith_Vbep;
                self.th.Ith_Vbcp = o.Ith_Vbcp;
                self.th.Ith_Vcep = o.Ith_Vcep;
                self.th.Ith_Vrci = o.Ith_Vrci;
                self.th.Ith_Vbcx = o.Ith_Vbcx;
                self.th.Ith_Vrbi = o.Ith_Vrbi;
                self.th.Ith_Vrbp = o.Ith_Vrbp;
                self.th.Ith_Vrcx = o.Ith_Vrcx;
                self.th.Ith_Vrbx = o.Ith_Vrbx;
                self.th.Ith_Vre = o.Ith_Vre;
                self.th.Ith_Vrs = o.Ith_Vrs;
            }
        }

        // load: the current excitation vector and the matrix.
        let s = self.s;
        let (Irbi, Irbi_Vrbi, Irbi_Vbei, Irbi_Vbci) =
            (o.Irbi, o.Irbi_Vrbi, o.Irbi_Vbei, o.Irbi_Vbci);
        let (Irbp, Irbp_Vrbp, Irbp_Vbep, Irbp_Vbci) =
            (o.Irbp, o.Irbp_Vrbp, o.Irbp_Vbep, o.Irbp_Vbci);
        let (Iccp, Iccp_Vbep, Iccp_Vbci, Iccp_Vbcp) =
            (o.Iccp, o.Iccp_Vbep, o.Iccp_Vbci, o.Iccp_Vbcp);
        let (Ircx_Vrcx, Irbx_Vrbx, Ire_Vre, Irs_Vrs) =
            (o.Ircx_Vrcx, o.Irbx_Vrbx, o.Ire_Vre, o.Irs_Vrs);

        let rhs_current = typ * (s0(ctx, CQBEO) - Vbe * gqbeo);
        ctx.add_rhs(v.base, -rhs_current);
        ctx.add_rhs(v.emit, rhs_current);
        ctx.add(s[B_B], gqbeo);
        ctx.add(s[E_E], gqbeo);
        ctx.add(s[B_E], -gqbeo);
        ctx.add(s[E_B], -gqbeo);

        let rhs_current = typ * (s0(ctx, CQBCO) - Vbc * gqbco);
        ctx.add_rhs(v.base, -rhs_current);
        ctx.add_rhs(v.coll, rhs_current);
        ctx.add(s[B_B], gqbco);
        ctx.add(s[C_C], gqbco);
        ctx.add(s[B_C], -gqbco);
        ctx.add(s[C_B], -gqbco);

        ctx.add_rhs(v.base_bi, -cbcx);
        ctx.add_rhs(v.coll_cx, cbcx);
        ctx.add(s[BI_BI], gbcx);
        ctx.add(s[CX_CX], gbcx);
        ctx.add(s[BI_CX], -gbcx);
        ctx.add(s[CX_BI], -gbcx);

        // MNA at internal nodes. Ibe:
        let rhs_current = typ * (Ibe - Ibe_Vbei * Vbei);
        ctx.add_rhs(v.base_bi, -rhs_current);
        ctx.add(s[BI_BI], Ibe_Vbei);
        ctx.add(s[BI_EI], -Ibe_Vbei);
        ctx.add_rhs(v.emit_ei, rhs_current);
        ctx.add(s[EI_BI], -Ibe_Vbei);
        ctx.add(s[EI_EI], Ibe_Vbei);
        // Ibex
        let rhs_current = typ * (Ibex - Ibex_Vbex * Vbex);
        ctx.add_rhs(v.base_bx, -rhs_current);
        ctx.add(s[BX_BX], Ibex_Vbex);
        ctx.add(s[BX_EI], -Ibex_Vbex);
        ctx.add_rhs(v.emit_ei, rhs_current);
        ctx.add(s[EI_BX], -Ibex_Vbex);
        ctx.add(s[EI_EI], Ibex_Vbex);
        // Iciei
        let rhs_current = typ * (Iciei - Iciei_Vbei * Vbei - Iciei_Vbci * Vbci);
        ctx.add_rhs(v.coll_ci, -rhs_current);
        ctx.add(s[CI_BI], Iciei_Vbei);
        ctx.add(s[CI_EI], -Iciei_Vbei);
        ctx.add(s[CI_BI], Iciei_Vbci);
        ctx.add(s[CI_CI], -Iciei_Vbci);
        ctx.add_rhs(v.emit_ei, rhs_current);
        ctx.add(s[EI_BI], -Iciei_Vbei);
        ctx.add(s[EI_EI], Iciei_Vbei);
        ctx.add(s[EI_BI], -Iciei_Vbci);
        ctx.add(s[EI_CI], Iciei_Vbci);
        if excess {
            let rhs_current = typ * (-Iciei_Vxf2 * Vxf2);
            ctx.add_rhs(v.coll_ci, -rhs_current);
            ctx.add_rhs(v.emit_ei, rhs_current);
            // With respect to Vxf2 (no `VBICtype` factor in the C).
            ctx.add(s[CI_XF2], Iciei_Vxf2);
            ctx.add(s[EI_XF2], -Iciei_Vxf2);
        }
        // Ibc
        let rhs_current = typ * (Ibc - Ibc_Vbci * Vbci - Ibc_Vbei * Vbei);
        ctx.add_rhs(v.base_bi, -rhs_current);
        ctx.add(s[BI_BI], Ibc_Vbci);
        ctx.add(s[BI_CI], -Ibc_Vbci);
        ctx.add(s[BI_BI], Ibc_Vbei);
        ctx.add(s[BI_EI], -Ibc_Vbei);
        ctx.add_rhs(v.coll_ci, rhs_current);
        ctx.add(s[CI_BI], -Ibc_Vbci);
        ctx.add(s[CI_CI], Ibc_Vbci);
        ctx.add(s[CI_BI], -Ibc_Vbei);
        ctx.add(s[CI_EI], Ibc_Vbei);
        // Ibep
        let rhs_current = typ * (Ibep - Ibep_Vbep * Vbep);
        ctx.add_rhs(v.base_bx, -rhs_current);
        ctx.add(s[BX_BX], Ibep_Vbep);
        ctx.add(s[BX_BP], -Ibep_Vbep);
        ctx.add_rhs(v.base_bp, rhs_current);
        ctx.add(s[BP_BX], -Ibep_Vbep);
        ctx.add(s[BP_BP], Ibep_Vbep);
        // Rcx
        ctx.add(s[C_C], Ircx_Vrcx);
        ctx.add(s[CX_CX], Ircx_Vrcx);
        ctx.add(s[CX_C], -Ircx_Vrcx);
        ctx.add(s[C_CX], -Ircx_Vrcx);
        // Rci
        let rhs_current = typ * (Irci - Irci_Vrci * Vrci - Irci_Vbci * Vbci - Irci_Vbcx * Vbcx);
        ctx.add_rhs(v.coll_cx, -rhs_current);
        ctx.add(s[CX_CX], Irci_Vrci);
        ctx.add(s[CX_CI], -Irci_Vrci);
        ctx.add(s[CX_BI], Irci_Vbci);
        ctx.add(s[CX_CI], -Irci_Vbci);
        ctx.add(s[CX_BI], Irci_Vbcx);
        ctx.add(s[CX_CX], -Irci_Vbcx);
        ctx.add_rhs(v.coll_ci, rhs_current);
        ctx.add(s[CI_CX], -Irci_Vrci);
        ctx.add(s[CI_CI], Irci_Vrci);
        ctx.add(s[CI_BI], -Irci_Vbci);
        ctx.add(s[CI_CI], Irci_Vbci);
        ctx.add(s[CI_BI], -Irci_Vbcx);
        ctx.add(s[CI_CX], Irci_Vbcx);
        // Rbx
        ctx.add(s[B_B], Irbx_Vrbx);
        ctx.add(s[BX_BX], Irbx_Vrbx);
        ctx.add(s[BX_B], -Irbx_Vrbx);
        ctx.add(s[B_BX], -Irbx_Vrbx);
        // Rbi
        let rhs_current = typ * (Irbi - Irbi_Vrbi * Vrbi - Irbi_Vbei * Vbei - Irbi_Vbci * Vbci);
        ctx.add_rhs(v.base_bx, -rhs_current);
        ctx.add(s[BX_BX], Irbi_Vrbi);
        ctx.add(s[BX_BI], -Irbi_Vrbi);
        ctx.add(s[BX_BI], Irbi_Vbei);
        ctx.add(s[BX_EI], -Irbi_Vbei);
        ctx.add(s[BX_BI], Irbi_Vbci);
        ctx.add(s[BX_CI], -Irbi_Vbci);
        ctx.add_rhs(v.base_bi, rhs_current);
        ctx.add(s[BI_BX], -Irbi_Vrbi);
        ctx.add(s[BI_BI], Irbi_Vrbi);
        ctx.add(s[BI_BI], -Irbi_Vbei);
        ctx.add(s[BI_EI], Irbi_Vbei);
        ctx.add(s[BI_BI], -Irbi_Vbci);
        ctx.add(s[BI_CI], Irbi_Vbci);
        // Re
        ctx.add(s[E_E], Ire_Vre);
        ctx.add(s[EI_EI], Ire_Vre);
        ctx.add(s[EI_E], -Ire_Vre);
        ctx.add(s[E_EI], -Ire_Vre);
        // Rbp
        let rhs_current = typ * (Irbp - Irbp_Vrbp * Vrbp - Irbp_Vbep * Vbep - Irbp_Vbci * Vbci);
        ctx.add_rhs(v.base_bp, -rhs_current);
        ctx.add(s[BP_BP], Irbp_Vrbp);
        ctx.add(s[BP_CX], -Irbp_Vrbp);
        ctx.add(s[BP_BX], Irbp_Vbep);
        ctx.add(s[BP_BP], -Irbp_Vbep);
        ctx.add(s[BP_BI], Irbp_Vbci);
        ctx.add(s[BP_CI], -Irbp_Vbci);
        ctx.add_rhs(v.coll_cx, rhs_current);
        ctx.add(s[CX_BP], -Irbp_Vrbp);
        ctx.add(s[CX_CX], Irbp_Vrbp);
        ctx.add(s[CX_BX], -Irbp_Vbep);
        ctx.add(s[CX_BP], Irbp_Vbep);
        ctx.add(s[CX_BI], -Irbp_Vbci);
        ctx.add(s[CX_CI], Irbp_Vbci);
        // Ibcp
        let rhs_current = typ * (Ibcp - Ibcp_Vbcp * Vbcp);
        ctx.add_rhs(v.subs_si, -rhs_current);
        ctx.add(s[SI_SI], Ibcp_Vbcp);
        ctx.add(s[SI_BP], -Ibcp_Vbcp);
        ctx.add_rhs(v.base_bp, rhs_current);
        ctx.add(s[BP_SI], -Ibcp_Vbcp);
        ctx.add(s[BP_BP], Ibcp_Vbcp);
        // Iccp
        let rhs_current = typ * (Iccp - Iccp_Vbep * Vbep - Iccp_Vbci * Vbci - Iccp_Vbcp * Vbcp);
        ctx.add_rhs(v.base_bx, -rhs_current);
        ctx.add(s[BX_BX], Iccp_Vbep);
        ctx.add(s[BX_BP], -Iccp_Vbep);
        ctx.add(s[BX_BI], Iccp_Vbci);
        ctx.add(s[BX_CI], -Iccp_Vbci);
        ctx.add(s[BX_SI], Iccp_Vbcp);
        ctx.add(s[BX_BP], -Iccp_Vbcp);
        ctx.add_rhs(v.subs_si, rhs_current);
        ctx.add(s[SI_BX], -Iccp_Vbep);
        ctx.add(s[SI_BP], Iccp_Vbep);
        ctx.add(s[SI_BI], -Iccp_Vbci);
        ctx.add(s[SI_CI], Iccp_Vbci);
        ctx.add(s[SI_SI], -Iccp_Vbcp);
        ctx.add(s[SI_BP], Iccp_Vbcp);
        // Rs
        ctx.add(s[S_S], Irs_Vrs);
        ctx.add(s[SI_SI], Irs_Vrs);
        ctx.add(s[SI_S], -Irs_Vrs);
        ctx.add(s[S_SI], -Irs_Vrs);

        if excess {
            // Branch xf1-ground, Ixf1 (no `VBICtype` factor in the C).
            let rhs_current =
                Ixf1 - Ixf1_Vbci * Vbci - Ixf1_Vbei * Vbei - Ixf1_Vxf1 * Vxf1 - Ixf1_Vxf2 * Vxf2;
            ctx.add_rhs(v.xf1, -rhs_current);
            ctx.add(s[XF1_BI], Ixf1_Vbei);
            ctx.add(s[XF1_EI], -Ixf1_Vbei);
            ctx.add(s[XF1_BI], Ixf1_Vbci);
            ctx.add(s[XF1_CI], -Ixf1_Vbci);
            ctx.add(s[XF1_XF2], Ixf1_Vxf2);
            ctx.add(s[XF1_XF1], Ixf1_Vxf1);
            // Branch xf2-ground, Ixf2.
            let rhs_current = Ixf2 - Ixf2_Vxf1 * Vxf1 - Ixf2_Vxf2 * Vxf2;
            ctx.add_rhs(v.xf2, -rhs_current);
            ctx.add(s[XF2_XF2], Ixf2_Vxf2);
            ctx.add(s[XF2_XF1], Ixf2_Vxf1);
        }

        if selfheat {
            let t = v.temp;
            // Two-terminal branch currents' Vrth dependence: `rhs =
            // −dI/dVrth·Vrth` into (n1, n2), matrix (n1,T) / (n2,T).
            let th = |ctx: &mut LoadCtx, d: f64, n1: Var, n2: Var, s1: Slot, s2: Slot| {
                let rhs_current = -d * Vrth;
                ctx.add_rhs(n1, -rhs_current);
                ctx.add(s1, d);
                ctx.add_rhs(n2, rhs_current);
                ctx.add(s2, -d);
            };
            th(ctx, o.Ibe_Vrth, v.base_bi, v.emit_ei, s[BI_T], s[EI_T]);
            th(ctx, o.Ibex_Vrth, v.base_bx, v.emit_ei, s[BX_T], s[EI_T]);
            th(ctx, Iciei_Vrth, v.coll_ci, v.emit_ei, s[CI_T], s[EI_T]);
            th(ctx, o.Ibc_Vrth, v.base_bi, v.coll_ci, s[BI_T], s[CI_T]);
            th(ctx, o.Ibep_Vrth, v.base_bx, v.base_bp, s[BX_T], s[BP_T]);
            th(ctx, o.Ircx_Vrth, v.coll, v.coll_cx, s[C_T], s[CX_T]);
            th(ctx, o.Irci_Vrth, v.coll_cx, v.coll_ci, s[CX_T], s[CI_T]);
            th(ctx, o.Irbx_Vrth, v.base, v.base_bx, s[B_T], s[BX_T]);
            th(ctx, o.Irbi_Vrth, v.base_bx, v.base_bi, s[BX_T], s[BI_T]);
            th(ctx, o.Ire_Vrth, v.emit, v.emit_ei, s[E_T], s[EI_T]);
            th(ctx, o.Irbp_Vrth, v.base_bp, v.coll_cx, s[BP_T], s[CX_T]);
            th(ctx, o.Ibcp_Vrth, v.subs_si, v.base_bp, s[SI_T], s[BP_T]);
            th(ctx, o.Iccp_Vrth, v.base_bx, v.subs_si, s[BX_T], s[SI_T]);
            th(ctx, o.Irs_Vrth, v.subs, v.subs_si, s[S_T], s[SI_T]);
            // Rth
            ctx.add(s[T_T], o.Irth_Vrth);
            // Cth
            let rhs_current = Icth - Icth_Vrth * Vrth;
            ctx.add_rhs(t, -rhs_current);
            ctx.add(s[T_T], Icth_Vrth);
            // Ith (the Vbcp/Vcep columns land on the external
            // substrate node, as in the C)
            let rhs_current = -o.Ith
                - o.Ith_Vrth * Vrth
                - o.Ith_Vbei * Vbei
                - o.Ith_Vbci * Vbci
                - o.Ith_Vcei * Vcei
                - o.Ith_Vbex * Vbex
                - o.Ith_Vbep * Vbep
                - o.Ith_Vbcp * Vbcp
                - o.Ith_Vcep * Vcep
                - o.Ith_Vrci * Vrci
                - o.Ith_Vbcx * Vbcx
                - o.Ith_Vrbi * Vrbi
                - o.Ith_Vrbp * Vrbp
                - o.Ith_Vrcx * Vrcx
                - o.Ith_Vrbx * Vrbx
                - o.Ith_Vre * Vre
                - o.Ith_Vrs * Vrs;
            ctx.add_rhs(t, rhs_current);
            ctx.add(s[T_T], -o.Ith_Vrth);
            ctx.add(s[T_BI], -o.Ith_Vbei);
            ctx.add(s[T_EI], o.Ith_Vbei);
            ctx.add(s[T_BI], -o.Ith_Vbci);
            ctx.add(s[T_CI], o.Ith_Vbci);
            ctx.add(s[T_CI], -o.Ith_Vcei);
            ctx.add(s[T_EI], o.Ith_Vcei);
            ctx.add(s[T_BX], -o.Ith_Vbex);
            ctx.add(s[T_EI], o.Ith_Vbex);
            ctx.add(s[T_BX], -o.Ith_Vbep);
            ctx.add(s[T_BP], o.Ith_Vbep);
            ctx.add(s[T_S], -o.Ith_Vbcp);
            ctx.add(s[T_BP], o.Ith_Vbcp);
            ctx.add(s[T_BX], -o.Ith_Vcep);
            ctx.add(s[T_S], o.Ith_Vcep);
            ctx.add(s[T_CX], -o.Ith_Vrci);
            ctx.add(s[T_CI], o.Ith_Vrci);
            ctx.add(s[T_BI], -o.Ith_Vbcx);
            ctx.add(s[T_CX], o.Ith_Vbcx);
            ctx.add(s[T_BX], -o.Ith_Vrbi);
            ctx.add(s[T_BI], o.Ith_Vrbi);
            ctx.add(s[T_BP], -o.Ith_Vrbp);
            ctx.add(s[T_CX], o.Ith_Vrbp);
            ctx.add(s[T_C], -o.Ith_Vrcx);
            ctx.add(s[T_CX], o.Ith_Vrcx);
            ctx.add(s[T_B], -o.Ith_Vrbx);
            ctx.add(s[T_BX], o.Ith_Vrbx);
            ctx.add(s[T_E], -o.Ith_Vre);
            ctx.add(s[T_EI], o.Ith_Vre);
            ctx.add(s[T_S], -o.Ith_Vrs);
            ctx.add(s[T_SI], o.Ith_Vrs);
            if excess {
                // Ixf1 with respect to Vrth.
                let rhs_current = -Ixf1_Vrth * Vrth;
                ctx.add_rhs(v.xf1, -rhs_current);
                ctx.add(s[XF1_T], Ixf1_Vrth);
            }
        }
        if v.pin != 0 {
            ctx.add(s[PIN], 1.0);
        }
    }
}
