//! `.noise` — small-signal noise analysis (`ckt/noisean.c`,
//! `ckt/cktnoise.c`, `ni/niniter.c::NInzIter`, `dev/*noise.c` in
//! their ngspice form). Per frequency: the ordinary AC system gives
//! the input-output gain; the **adjoint** (transposed) system with a
//! unit current at the output gives, in one solve, the transfer
//! from every internal noise generator to the output. Each device
//! evaluates its spectral densities against that adjoint solution;
//! the totals integrate over the sweep with `Nintegrate`'s
//! log-log-segment rule.

use dune_raw::{Data, VarType};

use crate::ac::AcSweep;
use crate::circuit::{Circuit, Spec};
use crate::engine::{AcStepType, Engine};
use crate::error::Error;
use crate::matrix::{AnKind, InitMode};
use crate::options::Options;
use crate::solution::Solution;

const N_MINGAIN: f64 = 1e-20;
const N_MINLOG: f64 = 1e-38;
const N_INTFTHRESH: f64 = 1e-10;
const N_INTUSELOG: f64 = 1e-10;

/// Result of a `.noise` analysis: the density sweep and, when the
/// band has more than one frequency, the integrated totals.
#[derive(Clone, Debug)]
pub struct Noise {
    pub density: Solution,
    pub integrated: Option<Solution>,
    pub notes: Vec<String>,
}

/// `Nintegrate`: area under a log-log line segment.
fn nintegrate(
    noiz_dens: f64,
    ln_ndens: f64,
    ln_nlst_dens: f64,
    del_ln_freq: f64,
    del_freq: f64,
    ln_freq: f64,
    ln_last_freq: f64,
) -> f64 {
    let limexp = |x: f64| {
        if x > 700.0 {
            f64::exp(700.0) * (1.0 + x - 700.0)
        } else {
            f64::exp(x)
        }
    };
    let mut exponent = (ln_ndens - ln_nlst_dens) / del_ln_freq;
    if exponent.abs() < N_INTFTHRESH {
        noiz_dens * del_freq
    } else {
        let a = limexp(ln_ndens - exponent * ln_freq);
        exponent += 1.0;
        if exponent.abs() < N_INTUSELOG {
            a * (ln_freq - ln_last_freq)
        } else {
            a * ((f64::exp(exponent * ln_freq) - f64::exp(exponent * ln_last_freq)) / exponent)
        }
    }
}

impl Circuit {
    /// Run `.noise V(OUT<,REF>) SRC DEC|OCT|LIN PTS FSTART FSTOP
    /// <PTS_PER_SUMMARY>`.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub fn noise(
        &self,
        opts: &Options,
        output: &str,
        output_ref: Option<&str>,
        src: &str,
        sweep: AcSweep,
        points: usize,
        fstart: f64,
        fstop: f64,
        pts_per_summary: Option<usize>,
    ) -> Result<Noise, Error> {
        let ok = fstart > 0.0 && fstart.is_finite() && fstop.is_finite() && fstop >= fstart;
        if !ok {
            return Err(Error::Build(".noise frequencies are inconsistent".into()));
        }
        let src_lc = src.to_ascii_lowercase();

        // The input source must exist and carry an AC value
        // (`noisean.c`'s `noacinput`); it also fixes the unit of the
        // input-referred noise.
        let mut input_is_vsrc = None;
        for spec in &self.specs {
            match spec {
                Spec::Vsource(v) if v.name == src_lc => {
                    if v.ac.is_none() {
                        return Err(Error::Build(".noise input source has no AC value".into()));
                    }
                    input_is_vsrc = Some(true);
                }
                Spec::Isource(i) if i.name == src_lc => {
                    if i.ac.is_none() {
                        return Err(Error::Build(".noise input source has no AC value".into()));
                    }
                    input_is_vsrc = Some(false);
                }
                _ => {}
            }
        }
        let Some(input_is_vsrc) = input_is_vsrc else {
            return Err(Error::Build(format!(
                ".noise input source `{src}` not in circuit"
            )));
        };

        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let mut engine = Engine::new(built);

        let var_of = |name: &str| -> Result<usize, Error> {
            let name = name.to_ascii_lowercase();
            if name == "0" {
                return Ok(0);
            }
            engine
                .vars
                .names
                .iter()
                .position(|n| *n == name)
                .ok_or_else(|| Error::Build(format!(".noise names unknown node `{name}`")))
        };
        let pos_out = var_of(output)?;
        let neg_out = match output_ref {
            Some(r) => var_of(r)?,
            None => 0,
        };

        let step_type = match sweep {
            AcSweep::Decade => AcStepType::Decade,
            AcSweep::Octave => AcStepType::Octave,
            AcSweep::Linear => AcStepType::Linear,
        };
        let n = points.max(1);
        let freq_delta = match step_type {
            AcStepType::Decade => f64::exp(f64::ln(10.0) / n as f64),
            AcStepType::Octave => f64::exp(f64::ln(2.0) / n as f64),
            // ngspice divides by NnumSteps−1 (the 3f5 used +1).
            AcStepType::Linear => (fstop - fstart) / (n.max(2) - 1) as f64,
        };
        let freq_tol = match step_type {
            AcStepType::Linear => freq_delta * opts.reltol,
            _ => freq_delta * fstop * opts.reltol,
        };
        let stps_sm = pts_per_summary.unwrap_or(0);

        // Operating point, then the small-signal pass.
        engine.set_kind(AnKind::DcOp, false);
        engine.op(opts)?;
        engine.load_all_public(InitMode::Smsig, opts);

        // Noisy devices, in device order.
        let noisy: Vec<(usize, &'static [&'static str])> = engine.noise_devices();

        // Vector names for the density plot.
        let mut dens_vars: Vec<(String, VarType)> = vec![("frequency".into(), VarType::Frequency)];
        if stps_sm != 0 {
            for &(d, suffixes) in &noisy {
                for sfx in suffixes {
                    dens_vars.push((
                        format!("onoise_{}{}", engine.names[d], sfx),
                        VarType::Other("voltage-density".into()),
                    ));
                }
            }
        }
        dens_vars.push((
            "onoise_spectrum".into(),
            VarType::Other("voltage-density".into()),
        ));
        dens_vars.push((
            "inoise_spectrum".into(),
            VarType::Other(if input_is_vsrc {
                "voltage-density".into()
            } else {
                "current-density".into()
            }),
        ));
        let mut dens_cols: Vec<Vec<f64>> = vec![Vec::new(); dens_vars.len()];

        // Integration state per noisy device per generator.
        let mut ln_lst_dens: Vec<Vec<f64>> =
            noisy.iter().map(|(_, s)| vec![0.0; s.len()]).collect();
        let mut out_noiz_dev: Vec<Vec<f64>> =
            noisy.iter().map(|(_, s)| vec![0.0; s.len()]).collect();
        let mut in_noiz_dev: Vec<Vec<f64>> =
            noisy.iter().map(|(_, s)| vec![0.0; s.len()]).collect();
        let mut out_noiz = 0.0f64;
        let mut in_noise = 0.0f64;

        let mut freq = fstart;
        let mut lst_freq = freq;
        let mut step = 0usize;
        while freq <= fstop + freq_tol {
            let (gain2, adj_re, adj_im) = engine.noise_point(freq, pos_out, neg_out)?;
            let gain_sq_inv = 1.0 / f64::max(gain2, N_MINGAIN);
            let ln_gain_inv = gain_sq_inv.ln();

            let del_freq = freq - lst_freq;
            let ln_freq = f64::ln(f64::max(freq, N_MINLOG));
            let ln_last_freq = f64::ln(f64::max(lst_freq, N_MINLOG));
            let del_ln_freq = ln_freq - ln_last_freq;
            let prt_summary = stps_sm != 0 && step % stps_sm == 0;

            let mut out_ndens = 0.0f64;
            let mut row: Vec<f64> = Vec::new();
            for (k, &(d, suffixes)) in noisy.iter().enumerate() {
                let dens = engine.noise_dens_of(d, freq, &adj_re, &adj_im, opts.temp);
                let nsrc = suffixes.len();
                debug_assert_eq!(dens.len(), nsrc);
                let ln_dens: Vec<f64> = dens
                    .iter()
                    .map(|v| f64::ln(f64::max(*v, N_MINLOG)))
                    .collect();
                // (the C takes log(total) without the floor)
                out_ndens += dens[nsrc - 1];

                if del_freq == 0.0 {
                    ln_lst_dens[k].copy_from_slice(&ln_dens);
                    if freq == fstart {
                        out_noiz_dev[k].fill(0.0);
                        in_noiz_dev[k].fill(0.0);
                    }
                } else {
                    // Each generator integrates on its own; the total
                    // entry only accumulates them. A one-generator
                    // device (the switch, `swnoise.c`) has its total
                    // as the only entry and integrates that.
                    let single = nsrc == 1;
                    for i in 0..nsrc {
                        if single || i != nsrc - 1 {
                            let t_out = nintegrate(
                                dens[i],
                                ln_dens[i],
                                ln_lst_dens[k][i],
                                del_ln_freq,
                                del_freq,
                                ln_freq,
                                ln_last_freq,
                            );
                            let t_in = nintegrate(
                                dens[i] * gain_sq_inv,
                                ln_dens[i] + ln_gain_inv,
                                ln_lst_dens[k][i] + ln_gain_inv,
                                del_ln_freq,
                                del_freq,
                                ln_freq,
                                ln_last_freq,
                            );
                            ln_lst_dens[k][i] = ln_dens[i];
                            out_noiz += t_out;
                            in_noise += t_in;
                            if single {
                                out_noiz_dev[k][0] += t_out;
                                in_noiz_dev[k][0] += t_in;
                            } else if stps_sm != 0 {
                                out_noiz_dev[k][i] += t_out;
                                out_noiz_dev[k][nsrc - 1] += t_out;
                                in_noiz_dev[k][i] += t_in;
                                in_noiz_dev[k][nsrc - 1] += t_in;
                            }
                        }
                    }
                }
                if prt_summary {
                    row.extend_from_slice(&dens);
                }
            }

            if stps_sm == 0 || prt_summary {
                // ngspice's default output reports √(V²/Hz) — the
                // `.options sqrnoise` form keeps the squares; we
                // emit the default, which is what the goldens hold.
                let mut full = vec![freq];
                full.extend(row.iter().map(|v| v.sqrt()));
                full.push(out_ndens.sqrt());
                full.push((out_ndens * gain_sq_inv).sqrt());
                debug_assert_eq!(full.len(), dens_cols.len());
                for (col, v) in dens_cols.iter_mut().zip(full) {
                    col.push(v);
                }
            }

            lst_freq = freq;
            match step_type {
                AcStepType::Decade | AcStepType::Octave => freq *= freq_delta,
                AcStepType::Linear => {
                    freq += freq_delta;
                    if freq_delta == 0.0 {
                        break;
                    }
                }
            }
            step += 1;
        }

        let density = Solution {
            plotname: "Noise Spectral Density Curves".to_string(),
            variables: dens_vars,
            data: Data::Real(dens_cols),
            scale: Some(0),
        };

        // Integrated totals (only over a real band).
        let integrated = if fstart != fstop {
            let (owrap, otype) = ("v", VarType::Voltage);
            let (iwrap, itype) = if input_is_vsrc {
                ("v", VarType::Voltage)
            } else {
                ("i", VarType::Current)
            };
            let mut vars: Vec<(String, VarType)> = Vec::new();
            let mut cols: Vec<Vec<f64>> = Vec::new();
            if stps_sm != 0 {
                for (k, &(d, suffixes)) in noisy.iter().enumerate() {
                    for (i, sfx) in suffixes.iter().enumerate() {
                        vars.push((
                            format!("{owrap}(onoise_total_{}{})", engine.names[d], sfx),
                            otype.clone(),
                        ));
                        cols.push(vec![out_noiz_dev[k][i].sqrt()]);
                        vars.push((
                            format!("{iwrap}(inoise_total_{}{})", engine.names[d], sfx),
                            itype.clone(),
                        ));
                        cols.push(vec![in_noiz_dev[k][i].sqrt()]);
                    }
                }
            }
            vars.push((format!("{owrap}(onoise_total)"), otype.clone()));
            cols.push(vec![out_noiz.sqrt()]);
            vars.push((format!("{iwrap}(inoise_total)"), itype.clone()));
            cols.push(vec![in_noise.sqrt()]);
            Some(Solution {
                plotname: "Integrated Noise".to_string(),
                variables: vars,
                data: Data::Real(cols),
                scale: None,
            })
        } else {
            None
        };

        Ok(Noise {
            density,
            integrated,
            notes,
        })
    }
}
