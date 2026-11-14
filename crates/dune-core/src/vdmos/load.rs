//! `VDMOSload` (`vdmosload.c`): the channel, the gate charges, the
//! self-heating network and the body diode, statement for statement.

use super::model::{VdmosTemp, dev_cap, limitlog};
use super::*;
use crate::consts::{CTOK, E, KOVERQ};
use crate::diode::pnjlim;
use crate::matrix::{AnKind, InitMode, LoadCtx};
use crate::mos1::{fetlim, limvds};

impl VdmosI {
    #[allow(clippy::too_many_lines)]
    pub(super) fn do_load(&mut self, ctx: &mut LoadCtx) {
        let st = self.st;
        let typ = self.p.typ();
        let m = self.m;
        let v = self.v;
        let cgdmin = m * self.p.cgdmin;
        let cgdmax = m * self.p.cgdmax;
        let a = self.p.a;
        let cgs = m * self.p.cgs;
        let selfheat = self.selfheat;
        let mut check_th = selfheat;
        let smsig = ctx.mode == InitMode::Smsig;
        let s0 = |ctx: &LoadCtx, o: usize| Self::sget(ctx, 0, st + o);
        let s1 = |ctx: &LoadCtx, o: usize| Self::sget(ctx, 1, st + o);

        // Bias resolution.
        let mut xfact = 0.0;
        let (mut vgs, mut vds, mut del_temp);
        match ctx.mode {
            InitMode::Smsig => {
                vgs = s0(ctx, VGS);
                vds = s0(ctx, VDS);
                del_temp = s0(ctx, DELTEMP);
            }
            InitMode::Tran => {
                vgs = s1(ctx, VGS);
                vds = s1(ctx, VDS);
                del_temp = s1(ctx, DELTEMP);
            }
            InitMode::Jct if !self.off => {
                vds = typ * self.ic_vds;
                vgs = typ * self.ic_vgs;
                del_temp = 0.0;
                if vds == 0.0 && vgs == 0.0 && !(ctx.kind == AnKind::TranOp && ctx.uic) {
                    vgs = typ * self.p.vto.unwrap_or(0.0) + 0.1;
                    vds = 0.0;
                }
            }
            InitMode::Jct | InitMode::Fix if self.off => {
                vgs = 0.0;
                vds = 0.0;
                del_temp = 0.0;
            }
            _ => {
                if ctx.mode == InitMode::Pred {
                    xfact = ctx.pred_xfact;
                    let mut pred = |o: usize| {
                        let a1 = ctx.states.get(1, st + o);
                        ctx.states.set(0, st + o, a1);
                        (1.0 + xfact) * a1 - xfact * ctx.states.get(2, st + o)
                    };
                    vgs = pred(VGS);
                    vds = pred(VDS);
                    del_temp = pred(DELTEMP);
                } else {
                    vgs = typ * (ctx.sol[v.gp] - ctx.sol[v.sp]);
                    vds = typ * (ctx.sol[v.dp] - ctx.sol[v.sp]);
                    del_temp = if selfheat { ctx.sol[v.tj] } else { 0.0 };
                }
                let mut vgd = vgs - vds;
                let vgdo = s0(ctx, VGS) - s0(ctx, VDS);
                // Limiting.
                let von = typ * self.von;
                if s0(ctx, VDS) >= 0.0 {
                    vgs = fetlim(vgs, s0(ctx, VGS), von);
                    vds = vgs - vgd;
                    vds = limvds(vds, s0(ctx, VDS));
                } else {
                    vgd = fetlim(vgd, vgdo, von);
                    vds = vgs - vgd;
                    // `CKTfixLimit` is 0.
                    vds = -limvds(-vds, -s0(ctx, VDS));
                    vgs = vgd + vds;
                }
                del_temp = if selfheat {
                    limitlog(del_temp, s0(ctx, DELTEMP), 10.0, &mut check_th)
                } else {
                    0.0
                };
            }
        }

        let temp = if selfheat {
            let t = del_temp + CTOK;
            let mut notes = Vec::new();
            self.t = VdmosTemp::new(&self.p, m, t, self.reltol, &mut notes, &self.name);
            t
        } else {
            self.temp
        };
        self.temp_sh = temp;

        // Temperature derivatives for self-heating.
        let beta = self.t.t_transconductance;
        let tnom = self.p.tnom.unwrap_or(temp);
        let (dbeta_dt, rd0t, drd0t_dt, mut rd1t, mut drd1t_dt, dgsource_dt);
        if selfheat {
            dbeta_dt = beta * self.p.mu / temp;
            rd0t = self.t.drain_resistance;
            drd0t_dt = match self.p.texp0 {
                Some(texp0) => rd0t * texp0 / temp,
                None => self.p.rd / m * (self.p.trd1 + 2.0 * self.p.trd2 * (temp - tnom)),
            };
            rd1t = 0.0;
            drd1t_dt = 0.0;
            if self.p.qs() {
                rd1t = self.t.qs_resistance;
                drd1t_dt = rd1t * self.p.texp1 / temp;
            }
            let rst = 1.0 / self.t.source_conductance;
            let drst_dt = self.p.rs / m * (self.p.trs1 + 2.0 * self.p.trs2 * (temp - tnom));
            dgsource_dt = -drst_dt / (rst * rst);
        } else {
            dbeta_dt = 0.0;
            rd0t = self.t.drain_resistance;
            drd0t_dt = 0.0;
            rd1t = 0.0;
            drd1t_dt = 0.0;
            if self.p.qs() {
                rd1t = self.t.qs_resistance;
            }
            dgsource_dt = 0.0;
        }
        let _ = &mut rd1t;
        let _ = &mut drd1t_dt;

        let vgd = vgs - vds;
        let (vds_n, vgs_n);
        if vds >= 0.0 {
            self.mode = 1.0;
            vds_n = vds;
            vgs_n = vgs;
        } else {
            self.mode = -1.0;
            vds_n = -vds;
            vgs_n = vgd;
        }
        let mode = self.mode;

        // Drain current (Shichman-Hodges with the weak-inversion blend).
        let von = self.t.t_vth * typ;
        let mut vgst = (if mode == 1.0 { vgs } else { vgd }) - von;
        let vdsat = f64::max(vgst, 0.0);
        let slope = self.t.t_ksubthres;
        let lambda = self.p.lambda;
        let theta = self.p.theta;
        let shift = self.p.subshift;
        let mtr = self.p.mtriode;
        let vdss = vds * mtr * mode;
        let t0 = 1.0 + lambda * vds;
        let t1 = 1.0 + theta * vdsat;
        let betap = beta * t0 / t1;
        let dbetapdvgs = -beta * theta * t0 / (t1 * t1);
        let dbetapdvds = beta * lambda / t1;
        let t2 = f64::exp((vgst - shift) / slope);
        vgst = slope * f64::ln(1.0 + t2);
        let dvgstdvgs = t2 / (t2 + 1.0);
        let (cdrain, gm, gds);
        if vgst <= vdss {
            cdrain = betap * vgst * vgst * 0.5;
            gm = betap * vgst * dvgstdvgs + 0.5 * dbetapdvgs * vgst * vgst;
            gds = 0.5 * dbetapdvds * vgst * vgst;
        } else {
            cdrain = betap * vdss * (vgst - 0.5 * vdss);
            gm = betap * vdss * dvgstdvgs + vdss * dbetapdvgs * (vgst - 0.5 * vdss);
            gds = vdss * dbetapdvds * (vgst - 0.5 * vdss) + betap * mtr * (vgst - 0.5 * vdss)
                - 0.5 * vdss * betap * mtr;
        }
        let mut dids_dt = 0.0;
        if selfheat {
            // `dvgst_dT = type·TCVTH` with the C's sign (the threshold
            // rises with TCVTH, so vgst falls; kept as written).
            let dvgst_dt = typ * self.p.tcvth;
            let dvdsat_dt = if vgst > 0.0 { dvgst_dt } else { 0.0 };
            let dt1_dt = theta * dvdsat_dt;
            let dbetap_dt = t0 * (t1 * dbeta_dt - beta * dt1_dt) / (t1 * t1);
            dids_dt = if vgst <= vdss {
                0.5 * dbetap_dt * vgst * vgst + betap * vgst * dvgst_dt
            } else {
                vdss * (dbetap_dt * vgst + betap * dvgst_dt) - dbetap_dt * vdss * 0.5 * vdss
            };
        }

        self.von = typ * von;
        self.vdsat = typ * vdsat;
        self.gm = gm;
        self.gds = gds;
        self.cd = mode * cdrain;

        ctx.states.set(0, st + VGS, vgs);
        ctx.states.set(0, st + VDS, vds);
        ctx.states.set(0, st + DELTEMP, del_temp);

        // Quasi-saturation (d'Alessandro's model, simplified): the
        // drain resistance follows the external drain-source voltage
        // of the previous iterate, without a derivative stamp.
        let mut dgdrain_dt = 0.0;
        if self.p.qs() && mode == 1.0 {
            let vdsn = typ * (ctx.sol[v.d] - ctx.sol[v.s]);
            let vq = self.p.vq.unwrap_or(0.0).abs();
            let rd = rd0t + rd1t * (vdsn / (vdsn + vq));
            let drd_dt = drd0t_dt + drd1t_dt * (vdsn / (vdsn + vq));
            if rd > 0.0 {
                self.drain_conductance = 1.0 / rd + ctx.gmin;
                dgdrain_dt = -drd_dt / (rd * rd);
            } else {
                self.drain_conductance = 1.0 / rd0t;
                dgdrain_dt = -drd0t_dt / (rd0t * rd0t);
            }
        } else if rd0t > 0.0 {
            self.drain_conductance = 1.0 / rd0t;
            dgdrain_dt = -drd0t_dt / (rd0t * rd0t);
        }
        let gdrain = self.drain_conductance;
        let gsource = self.t.source_conductance;
        let ggate = self.t.gate_conductance;

        self.gmt = if selfheat { dids_dt } else { 0.0 };
        let (mut vrd, mut dith_dvrd, mut dird_dt) = (0.0, 0.0, 0.0);
        let (mut vrs, mut dith_dvrs, mut dirs_dt) = (0.0, 0.0, 0.0);
        let mut cth = 0.0;
        if selfheat {
            self.gtempg = typ * gm * vds_n;
            self.gtemp_t = self.gmt * vds_n;
            self.gtempd = typ * (gds * vds_n + cdrain);
            cth = cdrain * vds_n
                - typ * (self.gtempg * vgs_n + self.gtempd * vds_n)
                - self.gtemp_t * del_temp;
            vrd = ctx.sol[v.d] - ctx.sol[v.dp];
            dith_dvrd = gdrain * 2.0 * vrd;
            dird_dt = vrd * dgdrain_dt;
            cth += gdrain * vrd * vrd - dith_dvrd * vrd - dird_dt * vrd * del_temp;
            vrs = ctx.sol[v.s] - ctx.sol[v.sp];
            dith_dvrs = gsource * 2.0 * vrs;
            dirs_dt = vrs * dgsource_dt;
            cth += gsource * vrs * vrs - dith_dvrs * vrs - dirs_dt * vrs * del_temp;
        }

        // Gate capacitances (`DevCapVDMOS`) and the thermal capacitor.
        let (mut capgs, mut capgd, mut capth) = (0.0, 0.0, 0.0);
        let charge_kinds = matches!(ctx.kind, AnKind::DcSweep | AnKind::Tran | AnKind::TranOp);
        if charge_kinds || smsig {
            let (hgs, hgd) = dev_cap(vgd, cgdmin, cgdmax, a, cgs);
            ctx.states.set(0, st + CAPGS, hgs);
            ctx.states.set(0, st + CAPGD, hgd);
            ctx.states.set(0, st + CAPTH, m * self.p.cthj / 2.0);
            let vgs1 = s1(ctx, VGS);
            let vgd1 = vgs1 - s1(ctx, VDS);
            let del_temp1 = s1(ctx, DELTEMP);
            if ctx.kind == AnKind::TranOp || smsig {
                capgs = 2.0 * s0(ctx, CAPGS);
                capgd = 2.0 * s0(ctx, CAPGD);
                capth = 2.0 * s0(ctx, CAPTH);
            } else {
                capgs = s0(ctx, CAPGS) + s1(ctx, CAPGS);
                capgd = s0(ctx, CAPGD) + s1(ctx, CAPGD);
                capth = s0(ctx, CAPTH) + s1(ctx, CAPTH);
            }
            if matches!(ctx.mode, InitMode::Pred | InitMode::Tran) {
                for o in [QGS, QGD, QTH] {
                    let q = (1.0 + xfact) * s1(ctx, o) - xfact * Self::sget(ctx, 2, st + o);
                    ctx.states.set(0, st + o, q);
                }
            } else if ctx.kind == AnKind::Tran {
                let q = (vgs - vgs1) * capgs + s1(ctx, QGS);
                ctx.states.set(0, st + QGS, q);
                let q = (vgd - vgd1) * capgd + s1(ctx, QGD);
                ctx.states.set(0, st + QGD, q);
                let q = (del_temp - del_temp1) * capth + s1(ctx, QTH);
                ctx.states.set(0, st + QTH, q);
            } else {
                ctx.states.set(0, st + QGS, vgs * capgs);
                ctx.states.set(0, st + QGD, vgd * capgd);
                ctx.states.set(0, st + QTH, del_temp * capth);
            }
        }

        let (mut gcgs, mut ceqgs, mut gcgd, mut ceqgd, mut gc_tt, mut ceqqth) =
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        if ctx.mode != InitMode::Tran && ctx.kind == AnKind::Tran {
            if capgs == 0.0 {
                ctx.states.set(0, st + CQGS, 0.0);
            }
            if capgd == 0.0 {
                ctx.states.set(0, st + CQGD, 0.0);
            }
            if capth == 0.0 {
                ctx.states.set(0, st + CQTH, 0.0);
            }
            let (g, ceq) = ctx.integrate(capgs, st + QGS);
            gcgs = g;
            ceqgs = ceq;
            let (g, ceq) = ctx.integrate(capgd, st + QGD);
            gcgd = g;
            ceqgd = ceq;
            ceqgs = ceqgs - gcgs * vgs + ctx.ag[0] * s0(ctx, QGS);
            ceqgd = ceqgd - gcgd * vgd + ctx.ag[0] * s0(ctx, QGD);
            if selfheat {
                let (g, ceq) = ctx.integrate(capth, st + QTH);
                gc_tt = g;
                ceqqth = ceq;
            }
        }

        // Current vector.
        let (gmt_s, gttg, gttdp, gtt_t, gttsp) = if !selfheat {
            (0.0, 0.0, 0.0, 0.0, 0.0)
        } else if mode >= 0.0 {
            let (g, d) = (self.gtempg, self.gtempd);
            (typ * self.gmt, g, d, self.gtemp_t, -(g + d))
        } else {
            let (g, sp) = (self.gtempg, self.gtempd);
            (-typ * self.gmt, g, -(g + sp), self.gtemp_t, sp)
        };
        let (xnrm, xrev, cdreq) = if mode >= 0.0 {
            (1.0, 0.0, typ * (cdrain - gds * vds - gm * vgs))
        } else {
            (0.0, 1.0, -typ * (cdrain - gds * (-vds) - gm * vgd))
        };
        ctx.add_rhs(v.gp, -(typ * (ceqgs + ceqgd)));
        ctx.add_rhs(v.dp, -cdreq + typ * ceqgd);
        ctx.add_rhs(v.sp, cdreq + typ * ceqgs);
        if selfheat {
            ctx.add_rhs(v.d, dird_dt * del_temp);
            ctx.add_rhs(v.s, dirs_dt * del_temp);
            ctx.add_rhs(v.dp, gmt_s * del_temp - dird_dt * del_temp);
            ctx.add_rhs(v.sp, -gmt_s * del_temp - dirs_dt * del_temp);
            ctx.add_rhs(v.tj, cth - ceqqth);
            let mut vdev_temp = if self.temp_given {
                self.temp - CTOK
            } else {
                self.ckt_temp - CTOK
            };
            if ctx.kind == AnKind::TranOp {
                vdev_temp *= ctx.srcfact;
            }
            ctx.add_rhs(v.br, vdev_temp);
        }

        // Y matrix.
        let s = self.s;
        let gdss = self.ds_conductance;
        ctx.add(s[D_D], gdrain + gdss);
        ctx.add(s[G_G], ggate);
        ctx.add(s[S_S], gsource + gdss);
        ctx.add(s[DP_DP], gdrain + gds + xrev * gm + gcgd);
        ctx.add(s[SP_SP], gsource + gds + xnrm * gm + gcgs);
        ctx.add(s[GP_GP], ggate + (gcgd + gcgs));
        ctx.add(s[G_GP], -ggate);
        ctx.add(s[D_DP], -gdrain);
        ctx.add(s[GP_G], -ggate);
        ctx.add(s[GP_DP], -gcgd);
        ctx.add(s[GP_SP], -gcgs);
        ctx.add(s[S_SP], -gsource);
        ctx.add(s[DP_D], -gdrain);
        ctx.add(s[DP_GP], (xnrm - xrev) * gm - gcgd);
        ctx.add(s[DP_SP], -gds - xnrm * gm);
        ctx.add(s[SP_GP], -(xnrm - xrev) * gm - gcgs);
        ctx.add(s[SP_S], -gsource);
        ctx.add(s[SP_DP], -gds - xrev * gm);
        ctx.add(s[D_S], -gdss);
        ctx.add(s[S_D], -gdss);
        if selfheat {
            let gthjc = m / self.p.rthjc.unwrap_or(1.0);
            let gthca = m / self.p.rthca;
            ctx.add(s[D_T], dird_dt);
            ctx.add(s[DP_T], gmt_s - dird_dt);
            ctx.add(s[S_T], dirs_dt);
            ctx.add(s[SP_T], -gmt_s - dirs_dt);
            ctx.add(
                s[T_T],
                -gtt_t - dird_dt * vrd - dirs_dt * vrs + gthjc + gc_tt,
            );
            ctx.add(s[T_GP], -gttg);
            ctx.add(s[T_D], -dith_dvrd);
            ctx.add(s[T_S], -dith_dvrs);
            ctx.add(s[T_DP], -gttdp + dith_dvrd);
            ctx.add(s[T_SP], -gttsp + dith_dvrs);
            ctx.add(s[T_TC], -gthjc);
            ctx.add(s[TC_T], -gthjc);
            ctx.add(s[TC_TC], gthjc + gthca);
            ctx.add(s[TP_TP], gthca);
            ctx.add(s[TP_TC], -gthca);
            ctx.add(s[TC_TP], -gthca);
            ctx.add(s[BR_TP], 1.0);
            ctx.add(s[TP_BR], 1.0);
        }
        for k in 0..2 {
            if v.pins[k] != 0 {
                ctx.add(s[PIN0 + k], 1.0);
            }
        }

        // Body diode: reverse conduction and forward breakdown of the
        // transistor, anode at the source side (`VDIOposPrimeNode`),
        // cathode at the external drain.
        let revrec = self.revrec();
        let t = &self.t;
        let gbpr = t.t_conductance;
        let vt = KOVERQ * temp;
        let vte = self.p.n * vt;
        let vtebrk = self.p.nbv * vt;
        let vbrknp = t.t_brkdwn_v;
        let bv_given = self.p.bv.is_some();
        let mut check_dio = true;
        let (vd, vqp);
        match ctx.mode {
            InitMode::Smsig => {
                vd = s0(ctx, DVOLT);
                vqp = s0(ctx, DQP);
            }
            InitMode::Tran => {
                vd = s1(ctx, DVOLT);
                vqp = s1(ctx, DQP);
            }
            // `VDIOinitCond` is never set: 0.
            InitMode::Jct if ctx.kind == AnKind::TranOp && ctx.uic => {
                vd = 0.0;
                vqp = 0.0;
            }
            InitMode::Jct => {
                vd = t.t_vcrit;
                vqp = 0.0;
            }
            _ => {
                let mut vdn;
                if ctx.mode == InitMode::Pred {
                    for o in [DVOLT, DCURRENT, DCONDUCT, DIDIO_DT] {
                        let x = s1(ctx, o);
                        ctx.states.set(0, st + o, x);
                    }
                    vdn = ctx.pred(st + DVOLT);
                    vqp = ctx.pred(st + DQP);
                    for o in [RESCURRENT, RESCONDUCT, CQCSR, GQCSR] {
                        let x = s1(ctx, o);
                        ctx.states.set(0, st + o, x);
                    }
                } else {
                    vdn = typ * (ctx.sol[v.rp] - ctx.sol[v.d]);
                    vqp = typ * ctx.sol[v.qp];
                }
                // Limit the new junction voltage.
                if bv_given && vdn < f64::min(0.0, -vbrknp + 10.0 * vtebrk) {
                    let mut vdtemp = -(vdn + vbrknp);
                    vdtemp = pnjlim(
                        vdtemp,
                        -(s0(ctx, DVOLT) + vbrknp),
                        vtebrk,
                        t.t_vcrit,
                        &mut check_dio,
                    );
                    vdn = -(vdtemp + vbrknp);
                } else {
                    vdn = pnjlim(vdn, s0(ctx, DVOLT), vte, t.t_vcrit, &mut check_dio);
                }
                vd = vdn;
            }
        }

        // DC current and derivatives.
        let (cdb, gdb, didio_dt);
        if vd >= -3.0 * vte {
            let evd = f64::exp(vd / vte);
            cdb = t.t_sat_cur * (evd - 1.0);
            didio_dt = t.t_sat_cur_dt * (evd - 1.0) - t.t_sat_cur * vd * evd / (vte * temp);
            gdb = t.t_sat_cur * evd / vte;
        } else if !bv_given || vd >= -vbrknp {
            let arg = 3.0 * vte / (vd * E);
            let arg3 = arg * arg * arg;
            let darg3_dt = 3.0 * arg3 / temp;
            cdb = -t.t_sat_cur * (1.0 + arg3);
            didio_dt = -t.t_sat_cur_dt * (arg3 + 1.0) - t.t_sat_cur * darg3_dt;
            gdb = t.t_sat_cur * 3.0 * arg / vd;
        } else {
            let evrev = f64::exp(-(vbrknp + vd) / vtebrk);
            cdb = -t.t_sat_cur * evrev;
            didio_dt =
                t.t_sat_cur * (-vbrknp - vd) * evrev / vtebrk / temp - t.t_sat_cur_dt * evrev;
            gdb = t.t_sat_cur * evrev / vtebrk;
        }
        let mut cd = cdb + ctx.gmin * vd;
        let mut gd = gdb + ctx.gmin;
        let cdres = cd;
        let gdres = gd;
        let mut cqcsr = 0.0;
        let mut gqcsr = 0.0;

        let tranop_uic = ctx.kind == AnKind::TranOp && ctx.uic;
        if matches!(ctx.kind, AnKind::DcSweep | AnKind::Tran) || smsig || tranop_uic {
            let czero = t.t_jct_cap;
            let (deplcharge, deplcap);
            if vd < t.t_dep_cap {
                let arg = 1.0 - vd / t.t_jct_pot;
                let sarg = f64::exp(-t.t_grading * f64::ln(arg));
                deplcharge = t.t_jct_pot * czero * (1.0 - arg * sarg) / (1.0 - t.t_grading);
                deplcap = czero * sarg;
            } else {
                let czof2 = czero / t.t_f2;
                deplcharge = czero * t.t_f1
                    + czof2
                        * (t.t_f3 * (vd - t.t_dep_cap)
                            + (t.t_grading / (t.t_jct_pot + t.t_jct_pot))
                                * (vd * vd - t.t_dep_cap * t.t_dep_cap));
                deplcap = czof2 * (t.t_f3 + t.t_grading * vd / t.t_jct_pot);
            }
            let (capd, capsr);
            if revrec {
                ctx.states.set(0, st + DCAPCHARGE, deplcharge);
                capd = deplcap;
                ctx.states.set(0, st + SRCAPCHARGE, t.t_transit_time * vqp);
                capsr = t.t_transit_time;
            } else {
                let diffcharge = t.t_transit_time * cdb;
                let diffcap = t.t_transit_time * gdb;
                ctx.states.set(0, st + DCAPCHARGE, diffcharge + deplcharge);
                capd = diffcap + deplcap;
                ctx.states.set(0, st + SRCAPCHARGE, 0.0);
                capsr = 0.0;
            }
            if !tranop_uic {
                if smsig {
                    ctx.states.set(0, st + DCAPCURRENT, capd);
                    ctx.states.set(0, st + RESCONDUCT, gdres);
                    return;
                }
                let init_tran = ctx.mode == InitMode::Tran;
                if init_tran {
                    let q = s0(ctx, DCAPCHARGE);
                    ctx.states.set(1, st + DCAPCHARGE, q);
                }
                if ctx.kind != AnKind::DcSweep {
                    let (geq, _) = ctx.integrate(capd, st + DCAPCHARGE);
                    gd += geq;
                    cd += s0(ctx, DCAPCURRENT);
                    if init_tran {
                        let c = s0(ctx, DCAPCURRENT);
                        ctx.states.set(1, st + DCAPCURRENT, c);
                    }
                    if revrec {
                        if init_tran {
                            let q = s0(ctx, SRCAPCHARGE);
                            ctx.states.set(1, st + SRCAPCHARGE, q);
                        }
                        let (geq, _) = ctx.integrate(capsr, st + SRCAPCHARGE);
                        gqcsr = geq;
                        cqcsr = s0(ctx, SRCAPCURRENT);
                        if init_tran {
                            let c = s0(ctx, SRCAPCURRENT);
                            ctx.states.set(1, st + SRCAPCURRENT, c);
                        }
                    }
                }
            }
        }

        if check_th || check_dio {
            ctx.noncon = true;
        }

        ctx.states.set(0, st + DVOLT, vd);
        ctx.states.set(0, st + DCURRENT, cd);
        ctx.states.set(0, st + DCONDUCT, gd);
        ctx.states.set(0, st + DIDIO_DT, didio_dt);
        ctx.states.set(0, st + DQP, vqp);
        ctx.states.set(0, st + RESCURRENT, cdres);
        ctx.states.set(0, st + RESCONDUCT, gdres);
        ctx.states.set(0, st + CQCSR, cqcsr);
        ctx.states.set(0, st + GQCSR, gqcsr);

        let (mut ith, mut dith_dvdio, mut dith_dvrb, mut dirb_dt, mut dith_dt, mut vrb) =
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        if selfheat {
            vrb = ctx.sol[v.s] - ctx.sol[v.rp];
            ith = vd * cd + vrb * vrb * gbpr;
            dith_dvdio = cd + vd * gd;
            dith_dvrb = vrb * gbpr;
            dirb_dt = vrb * t.t_conductance_dt;
            dith_dt = vrb * dirb_dt + didio_dt * vd;
        }
        let cdeq = typ * (cd - gd * vd);
        ctx.add_rhs(v.d, cdeq);
        ctx.add_rhs(v.rp, -cdeq);
        if selfheat {
            ctx.add_rhs(v.rp, didio_dt * del_temp - dirb_dt * del_temp);
            ctx.add_rhs(v.d, -didio_dt * del_temp);
            ctx.add_rhs(v.s, dirb_dt * del_temp);
            ctx.add_rhs(
                v.tj,
                ith - typ * dith_dvdio * vd - dith_dvrb * vrb - dith_dt * del_temp,
            );
        }
        ctx.add(s[S_S], gbpr);
        ctx.add(s[D_D], gd);
        ctx.add(s[RP_RP], gd + gbpr);
        ctx.add(s[S_RP], -gbpr);
        ctx.add(s[D_RP], -gd);
        ctx.add(s[RP_S], -gbpr);
        ctx.add(s[RP_D], -gd);
        if selfheat {
            ctx.add(s[T_S], -dith_dvrb);
            ctx.add(s[T_RP], -dith_dvdio + dith_dvrb);
            ctx.add(s[T_D], dith_dvdio);
            ctx.add(s[T_T], -dith_dt);
            ctx.add(s[RP_T], didio_dt - dirb_dt);
            ctx.add(s[S_T], dirb_dt);
            ctx.add(s[D_T], -didio_dt);
        }
        if revrec {
            let vp = self.p.vp;
            let tt = t.t_transit_time;
            let fac = tt / vp;
            let dcrrdvd = fac * gdres;
            let ceqrr = -fac * cdres + cqcsr + dcrrdvd * vd - gqcsr * vqp;
            let grr = 1.0 / vp;
            ctx.add_rhs(v.qp, -(typ * ceqrr));
            ctx.add(s[QP_QP], grr + gqcsr);
            ctx.add(s[QP_RP], -dcrrdvd);
            ctx.add(s[QP_D], dcrrdvd);
            let qp_gain = (1.0 - vp) / tt;
            let geqrrd = qp_gain * gqcsr;
            let ceqrrd = typ * (qp_gain * cqcsr - geqrrd * vqp);
            ctx.add_rhs(v.rp, -ceqrrd);
            ctx.add_rhs(v.d, ceqrrd);
            ctx.add(s[RP_QP], geqrrd);
            ctx.add(s[D_QP], -geqrrd);
        }
    }
}
