//! Simulator options with the 3f5 defaults
//! (`lib/ckt/cktinit.c`, `lib/ckt/cktntask.c`; names from
//! `spice.txt` §SIMULATOR VARIABLES). Temperatures arrive in °C on
//! the card and live in kelvin here, like the C.

use dune_netlist::Diagnostic;
use dune_netlist::ir::{Param, ParamValue};

use crate::consts::{CTOK, REFTEMP};
use crate::integ::Method;

#[derive(Clone, Debug)]
pub struct Options {
    /// `RELTOL`, default 1e-3.
    pub reltol: f64,
    /// `ABSTOL`, default 1e-12 (amperes).
    pub abstol: f64,
    /// `VNTOL`, default 1e-6 (volts).
    pub vntol: f64,
    /// `GMIN`, default 1e-12.
    pub gmin: f64,
    /// `ITL1`: DC iteration limit, default 100.
    pub itl1: usize,
    /// `ITL2`: DC transfer-curve iteration limit, default 50.
    pub itl2: usize,
    /// `ITL4`: transient timepoint iteration limit, default 10
    /// (`CKTtranMaxIter`).
    pub itl4: usize,
    /// Gmin steps in `CKTop`, default 10 (`TSKnumGminSteps`).
    pub gminsteps: usize,
    /// Source steps in `CKTop`, default 10 (`TSKnumSrcSteps`).
    pub srcsteps: usize,
    /// Operating temperature in kelvin (`TEMP`, default 27 °C).
    pub temp: f64,
    /// Nominal parameter-measurement temperature (`TNOM`, 27 °C).
    pub tnom: f64,
    /// `PIVTOL`, default 1e-13.
    pub pivtol: f64,
    /// `PIVREL`, default 1e-3.
    pub pivrel: f64,
    /// `NOOPITER`: go directly to gmin stepping.
    pub noopiter: bool,
    /// `TRTOL`: truncation-error overestimation factor, default 7.
    pub trtol: f64,
    /// `CHGTOL`: charge tolerance, default 1e-14.
    pub chgtol: f64,
    /// `METHOD`: `trap` (default) or `gear`.
    pub method: Method,
    /// `MAXORD` for Gear, default 2.
    pub maxord: usize,
    /// `NOPAGE`: suppress page breaks in printed output.
    pub nopage: bool,
    /// `DEFL`/`DEFW`: default MOS channel length/width, meters
    /// (`CKTdefaultMosL/W`, 1e-4).
    pub defl: f64,
    pub defw: f64,
    /// `DEFAD`/`DEFAS`: default MOS drain/source diffusion area, m²
    /// (`CKTdefaultMosAD/AS`, 0).
    pub defad: f64,
    pub defas: f64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            reltol: 1e-3,
            abstol: 1e-12,
            vntol: 1e-6,
            gmin: 1e-12,
            itl1: 100,
            itl2: 50,
            itl4: 10,
            gminsteps: 10,
            srcsteps: 10,
            temp: REFTEMP,
            tnom: REFTEMP,
            pivtol: 1e-13,
            pivrel: 1e-3,
            noopiter: false,
            trtol: 7.0,
            chgtol: 1e-14,
            method: Method::Trapezoidal,
            maxord: 2,
            nopage: false,
            defl: 1e-4,
            defw: 1e-4,
            defad: 0.0,
            defas: 0.0,
        }
    }
}

impl Options {
    /// Apply `.options` parameters from the IR. Unknown or
    /// not-yet-implemented options produce a warning, never an error
    /// (the 3f5 front end also lets unknown variables through).
    pub fn apply(&mut self, params: &[Param], diags: &mut Vec<Diagnostic>) {
        for p in params {
            let num = match &p.value {
                ParamValue::Num(v) => Some(*v),
                _ => None,
            };
            macro_rules! num_opt {
                ($field:ident) => {
                    match num {
                        Some(v) => self.$field = v,
                        None => diags.push(Diagnostic::warning(
                            format!("option {} needs a numeric value", p.name),
                            p.span,
                        )),
                    }
                };
            }
            match p.name.as_str() {
                "reltol" => num_opt!(reltol),
                "abstol" => num_opt!(abstol),
                "vntol" => num_opt!(vntol),
                "gmin" => num_opt!(gmin),
                "pivtol" => num_opt!(pivtol),
                "pivrel" => num_opt!(pivrel),
                "itl1" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl1 = v;
                    }
                }
                "itl2" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl2 = v;
                    }
                }
                "itl4" => {
                    if let Some(v) = count(num, p, diags) {
                        self.itl4 = v;
                    }
                }
                "maxord" => {
                    if let Some(v) = count(num, p, diags) {
                        self.maxord = v;
                    }
                }
                "trtol" => num_opt!(trtol),
                "chgtol" => num_opt!(chgtol),
                "method" => match &p.value {
                    ParamValue::Str(s) if s == "gear" => self.method = Method::Gear,
                    ParamValue::Str(s) if s == "trap" || s == "trapezoidal" => {
                        self.method = Method::Trapezoidal;
                    }
                    _ => diags.push(Diagnostic::warning(
                        "method must be trap or gear".to_string(),
                        p.span,
                    )),
                },
                "gminsteps" => {
                    if let Some(v) = count(num, p, diags) {
                        self.gminsteps = v;
                    }
                }
                "srcsteps" | "itl6" => {
                    if let Some(v) = count(num, p, diags) {
                        self.srcsteps = v;
                    }
                }
                "temp" => match num {
                    Some(v) => self.temp = v + CTOK,
                    None => diags.push(Diagnostic::warning(
                        "option temp needs a value in °C".to_string(),
                        p.span,
                    )),
                },
                "tnom" => match num {
                    Some(v) => self.tnom = v + CTOK,
                    None => diags.push(Diagnostic::warning(
                        "option tnom needs a value in °C".to_string(),
                        p.span,
                    )),
                },
                "noopiter" => self.noopiter = true,
                "nopage" => self.nopage = true,
                // `cktsopt.c` has a copy-paste bug that makes `defas`
                // set the drain area; ngspice fixed it and so do we
                // (`corpus/KNOWN_DIFFS.md`).
                "defl" => num_opt!(defl),
                "defw" => num_opt!(defw),
                "defad" => num_opt!(defad),
                "defas" => num_opt!(defas),
                // Recognized 3f5 options that only matter to later
                // phases (output formatting, statistics, devices).
                "itl3" | "itl5" | "acct" | "list" | "nomod" | "node" | "opts" | "bypass"
                | "badmos3" | "keepopinfo" => {}
                other => diags.push(Diagnostic::warning(
                    format!("option {other} is not implemented yet; ignored"),
                    p.span,
                )),
            }
        }
    }
}

fn count(num: Option<f64>, p: &Param, diags: &mut Vec<Diagnostic>) -> Option<usize> {
    match num {
        Some(v) if v >= 0.0 && v.is_finite() && v < 1e9 => Some(v as usize),
        _ => {
            diags.push(Diagnostic::warning(
                format!("option {} needs a non-negative count", p.name),
                p.span,
            ));
            None
        }
    }
}
