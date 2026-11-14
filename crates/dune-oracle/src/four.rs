//! `.four` against ngspice-47 (`corpus/four/`, `tools/four-golden.sh`).
//!
//! Every deck has two goldens from the pinned ngspice: `<deck>.four`,
//! its batch-mode Fourier tables verbatim at 17 significant digits,
//! and `<deck>.raw`, the transient plot those tables were computed
//! from. Two checks, both in `--ci`:
//!
//! - **port**: Dune's `.four` code transforms ngspice's own plot. Same
//!   samples, so only roundoff may differ (operation order, FMA
//!   contraction in the C build, the platform `sin`/`cos`): the
//!   ruler below is applied with the roundoff bound of `port_tau`.
//! - **simulation**: Dune simulates the deck (SPICE3 profile, like
//!   every ngspice comparison) and transforms its own plot. Each
//!   sample of the two waveforms may differ by the transient ruler of
//!   the rawfile comparison (D-0010), `τ = 2·(TRTOL·reltol·S + floor)`
//!   with `S` the vector's scale over both runs — for `v(a,b)` the sum
//!   of the two nodes' rulers.
//!
//! From a per-sample bound `τ` everything follows by linearity of the
//! DFT, no tolerance is invented:
//!
//! - DC is a mean: `|ΔDC| ≤ τ`;
//! - the harmonic vectors `z_k = (2/N)Σ v_i e^{iθ_ik}` obey Bessel's
//!   inequality, `Σ_{k≥1} |Δz_k|² ≤ (2/N)Σ Δv_i² ≤ 2τ²`, as long as
//!   the harmonics are distinct bins below Nyquist
//!   (`(nfreqs−1)·nperiods < N/2`; otherwise `|Δz_k| ≤ 2τ` each). So
//!   with `E = √2·τ`: `|Δmag_k| ≤ E`;
//! - the phase of `z_k` moves by at most `asin(E/|z_k|)`; when
//!   `E ≥ |z_k|` the harmonic is inside its own error ball and its
//!   phase is noise — **not compared** (counted in the report). This
//!   is the case of every harmonic of a pure sine;
//! - `nmag_k = |z_k|/|z_1|`: `≤ E·√(1+nmag_k²)/(|z_1|−E)`, and
//!   `THD/100 = ‖z_{2..}‖/|z_1|`: `≤ E·√(1+(THD/100)²)/(|z_1|−E)`
//!   (needs `|z_1| > E`); `nphase_k`: the sum of the two phase bounds;
//! - phases are compared modulo 360°.
//!
//! Branch currents are reported, never gating, for the reason the
//! transient comparison gives (D-0011): their own-scale ruler cannot
//! be reconstructed from the rawfile.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use dune_core::{Circuit, FourOutcome, FourParams, Fourier, Options, Solution, four_all};
use dune_netlist::Severity;
use dune_netlist::ir::{Netlist, OutVar, OutputKind};
use dune_raw::Rawfile;

use crate::compare::{TRTOL, Tolerances};

/// Rounding steps charged per sample by `port_tau`: the two
/// products and the division of the C's intercept, its evaluation
/// `c0 + c1·t`, and the accumulation.
const PORT_ROUNDINGS: f64 = 4.0;

/// One table as ngspice printed it.
#[derive(Debug)]
struct Table {
    var: String,
    gridsize: usize,
    polydegree: usize,
    nperiods: usize,
    /// `[freq, mag, phase, nmag, nphase]` per harmonic.
    rows: Vec<[f64; 5]>,
}

impl Table {
    /// THD recomputed from the 17-digit normalised magnitudes (the
    /// printed one has `%g`'s 6 digits).
    fn thd(&self) -> f64 {
        100.0
            * self.rows[2.min(self.rows.len())..]
                .iter()
                .map(|r| r[3] * r[3])
                .sum::<f64>()
                .sqrt()
    }
}

fn parse_golden(text: &str) -> Result<Vec<Table>> {
    let mut out: Vec<Table> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Fourier analysis for ") {
            out.push(Table {
                var: rest.trim_end_matches(':').to_string(),
                gridsize: 0,
                polydegree: 0,
                nperiods: 0,
                rows: Vec::new(),
            });
            continue;
        }
        let Some(t) = out.last_mut() else { continue };
        if let Some(rest) = line.trim_start().strip_prefix("No. Harmonics:") {
            let field = |key: &str| -> Result<usize> {
                let s = rest
                    .split(key)
                    .nth(1)
                    .with_context(|| format!("no `{key}` in `{line}`"))?;
                let n: String = s.trim().chars().take_while(char::is_ascii_digit).collect();
                Ok(n.parse()?)
            };
            t.gridsize = field("Gridsize:")?;
            t.polydegree = field("Interpolation Degree:")?;
            t.nperiods = field("No. Periods:")?;
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() == 6 && words[0].parse::<usize>().is_ok() {
            let mut row = [0.0; 5];
            for (r, w) in row.iter_mut().zip(&words[1..]) {
                *r = w.parse().with_context(|| format!("bad number `{w}`"))?;
            }
            t.rows.push(row);
        }
    }
    Ok(out)
}

/// Worst margin per compared quantity.
#[derive(Clone, Copy, Debug, Default)]
pub struct Margins {
    pub dc: f64,
    pub mag: f64,
    pub phase: f64,
    pub nmag: f64,
    pub nphase: f64,
    pub thd: f64,
    /// Phases not compared: harmonics inside their own error ball.
    pub phase_skipped: usize,
    pub phase_compared: usize,
}

impl Margins {
    fn worst(&self) -> (f64, &'static str) {
        [
            (self.dc, "dc"),
            (self.mag, "mag"),
            (self.phase, "phase"),
            (self.nmag, "nmag"),
            (self.nphase, "nphase"),
            (self.thd, "thd"),
        ]
        .into_iter()
        .fold((0.0, "-"), |a, b| if b.0 > a.0 { b } else { a })
    }

    fn merge(&mut self, o: &Margins) {
        self.dc = self.dc.max(o.dc);
        self.mag = self.mag.max(o.mag);
        self.phase = self.phase.max(o.phase);
        self.nmag = self.nmag.max(o.nmag);
        self.nphase = self.nphase.max(o.nphase);
        self.thd = self.thd.max(o.thd);
        self.phase_skipped += o.phase_skipped;
        self.phase_compared += o.phase_compared;
    }
}

/// Angle difference folded into [0, 180].
fn angle_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Compare one Dune table with ngspice's under the per-sample bound
/// `tau` (module docs); structural mismatches are `Err`.
fn compare_table(golden: &Table, cand: &Fourier, tau: f64) -> Result<Margins, String> {
    if golden.rows.len() != cand.harmonics.len()
        || golden.gridsize != cand.gridsize
        || golden.polydegree != cand.polydegree
        || golden.nperiods != cand.nperiods
    {
        return Err(format!(
            "shape differs: ngspice {} harmonics, grid {}, degree {}, periods {}; \
             dune {}, {}, {}, {}",
            golden.rows.len(),
            golden.gridsize,
            golden.polydegree,
            golden.nperiods,
            cand.harmonics.len(),
            cand.gridsize,
            cand.polydegree,
            cand.nperiods
        ));
    }
    let nh = golden.rows.len();
    let bessel = (nh.saturating_sub(1) * golden.nperiods) * 2 < golden.gridsize;
    let e = if bessel { 2f64.sqrt() * tau } else { 2.0 * tau };
    let ratio = |d: f64, allow: f64| {
        if allow > 0.0 {
            d / allow
        } else if d == 0.0 {
            0.0
        } else {
            f64::INFINITY
        }
    };
    let asin_deg = |x: f64| x.min(1.0).asin().to_degrees();
    let mut m = Margins::default();
    for (k, (g, c)) in golden.rows.iter().zip(&cand.harmonics).enumerate() {
        let freq_tol = 1e-12 * g[0].abs();
        if (g[0] - c.freq).abs() > freq_tol {
            return Err(format!("harmonic {k}: frequency {} vs {}", g[0], c.freq));
        }
        if k == 0 {
            m.dc = m.dc.max(ratio((g[1] - c.mag).abs(), tau));
            continue;
        }
        m.mag = m.mag.max(ratio((g[1] - c.mag).abs(), e));
        let m1 = golden.rows.get(1).map_or(0.0, |r| r[1]);
        if e < g[1] {
            m.phase_compared += 1;
            m.phase = m
                .phase
                .max(ratio(angle_diff(g[2], c.phase), asin_deg(e / g[1])));
            if e < m1 {
                let allow = asin_deg(e / g[1]) + asin_deg(e / m1);
                m.nphase = m.nphase.max(ratio(angle_diff(g[4], c.norm_phase), allow));
            }
        } else {
            m.phase_skipped += 1;
        }
        if e < m1 {
            let allow = e * (1.0 + g[3] * g[3]).sqrt() / (m1 - e);
            m.nmag = m.nmag.max(ratio((g[3] - c.norm_mag).abs(), allow));
        }
    }
    if let Some(m1) = golden.rows.get(1).map(|r| r[1]).filter(|&m1| e < m1) {
        let thd = golden.thd();
        let allow = 100.0 * e * (1.0 + (thd / 100.0).powi(2)).sqrt() / (m1 - e);
        m.thd = ratio((thd - cand.thd).abs(), allow);
    }
    Ok(m)
}

/// Scale `max|v|` of a plot vector over both plots.
fn vec_scale(sols: &[&Solution], name: &str) -> f64 {
    sols.iter()
        .filter_map(|s| s.column(name))
        .flat_map(|c| c.iter())
        .fold(0.0f64, |m, v| m.max(v.abs()))
}

/// The per-sample simulation ruler of one `.four` vector, and whether
/// it gates (node voltages do, branch currents do not).
fn sim_tau(var: &OutVar, sols: &[&Solution], tol: &Tolerances) -> (f64, bool) {
    let ruler = |s: f64, floor: f64| 2.0 * (TRTOL * tol.reltol * s + floor);
    match var {
        OutVar::Voltage {
            node, reference, ..
        } => {
            let mut t = ruler(vec_scale(sols, &format!("v({node})")), tol.vntol);
            if let Some(r) = reference {
                t += ruler(vec_scale(sols, &format!("v({r})")), tol.vntol);
            }
            (t, true)
        }
        OutVar::Current { vsrc, .. } => (
            ruler(vec_scale(sols, &format!("i({vsrc})")), tol.abstol),
            false,
        ),
        OutVar::Raw(n) => (ruler(vec_scale(sols, n), tol.abstol), false),
    }
}

/// The port ruler of one vector on ngspice's own plot: a roundoff
/// bound per sample, `PORT_ROUNDINGS·ε·S·(N + t_end/h_min)`. The
/// linear fit is evaluated as `c0 + c1·t` with
/// `c0 = (x1·y0 − x0·y1)/(x1 − x0)`, which cancels up to `t/h` digits
/// of a waveform of scale `S` (`h` the plot's smallest positive step),
/// and the DFT sums `N` products of size `≤ S`.
fn port_tau(var: &OutVar, gsol: &Solution, n: usize) -> f64 {
    let sols = [gsol];
    let s = match var {
        OutVar::Voltage {
            node, reference, ..
        } => {
            vec_scale(&sols, &format!("v({node})"))
                + reference
                    .as_ref()
                    .map_or(0.0, |r| vec_scale(&sols, &format!("v({r})")))
        }
        OutVar::Current { vsrc, .. } => vec_scale(&sols, &format!("i({vsrc})")),
        OutVar::Raw(n) => vec_scale(&sols, n),
    };
    let time = gsol.column("time").unwrap_or(&[]);
    let t_end = time.iter().fold(0.0f64, |m, t| m.max(t.abs()));
    let h_min = time
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&h| h > 0.0)
        .fold(f64::INFINITY, f64::min);
    let cancel = if h_min.is_finite() {
        t_end / h_min
    } else {
        0.0
    };
    PORT_ROUNDINGS * f64::EPSILON * s * (n as f64 + cancel)
}

/// Parse a deck and run its analyses in the SPICE3 profile.
fn simulate(path: &Path) -> Result<(Netlist, Vec<Solution>), String> {
    let parsed = dune_netlist::parse_file(path).map_err(|e| e.to_string())?;
    let errors: Vec<String> = parsed
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.render(&parsed.sources))
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    let (circuit, _) = Circuit::from_netlist(&parsed.netlist).map_err(|e| e.to_string())?;
    let mut options = Options::spice3();
    options.apply(&parsed.netlist.options, &mut Vec::new());
    let mut sols = Vec::new();
    for a in &parsed.netlist.analyses {
        let (s, _) = circuit.run(a, &options).map_err(|e| e.to_string())?;
        sols.extend(s);
    }
    Ok((parsed.netlist, sols))
}

fn golden_solution(raw: &Path) -> Result<Solution> {
    let raw = Rawfile::read_from(raw).with_context(|| format!("reading {}", raw.display()))?;
    let Some(p) = raw.plots.into_iter().next() else {
        bail!("no plot in the golden rawfile");
    };
    Ok(Solution {
        plotname: p.name,
        variables: p.variables.into_iter().map(|v| (v.name, v.vtype)).collect(),
        data: p.data,
        scale: Some(0),
    })
}

/// The decks of `corpus/four/`, sorted.
fn decks(root: &Path) -> Result<Vec<PathBuf>> {
    let dir = root.join("four");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "cir"))
        .collect();
    out.sort();
    Ok(out)
}

/// One checked table.
pub struct Row {
    pub deck: String,
    pub var: String,
    /// `port` or `dune`.
    pub cand: &'static str,
    pub gating: bool,
    pub result: Result<Margins, String>,
}

/// The ngspice tables of a deck paired with Dune's outcomes, which
/// must come in the same order and with the same names.
fn pair<'a>(
    deck: &str,
    golden: &'a [Table],
    outcomes: &'a [FourOutcome],
) -> Result<Vec<(&'a Table, &'a FourOutcome)>, String> {
    if golden.len() != outcomes.len() {
        return Err(format!(
            "{deck}: ngspice printed {} tables, dune computed {}",
            golden.len(),
            outcomes.len()
        ));
    }
    golden
        .iter()
        .zip(outcomes)
        .map(|(g, o)| {
            if g.var.eq_ignore_ascii_case(&o.var) {
                Ok((g, o))
            } else {
                Err(format!(
                    "{deck}: table for `{}` vs dune's `{}`",
                    g.var, o.var
                ))
            }
        })
        .collect()
}

/// Run both checks on every deck of `corpus/four/`.
pub fn check_corpus(root: &Path) -> Result<Vec<Row>> {
    let tol = Tolerances::default();
    let mut rows = Vec::new();
    for path in decks(root)? {
        let deck = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let gdir = root.join("four").join("golden");
        let text = std::fs::read_to_string(gdir.join(format!("{deck}.four")))
            .with_context(|| format!("golden tables of {deck} (run tools/four-golden.sh)"))?;
        let golden = parse_golden(&text)?;
        let gsol = golden_solution(&gdir.join(format!("{deck}.raw")))?;
        let fail = |cand, msg: String| Row {
            deck: deck.clone(),
            var: "-".into(),
            cand,
            gating: true,
            result: Err(msg),
        };
        let (nl, sols) = match simulate(&path) {
            Ok(x) => x,
            Err(e) => {
                rows.push(fail("dune", format!("dune failed: {e}")));
                continue;
            }
        };
        let params = FourParams::from_options(&nl.options, &mut Vec::new());
        // The vars in outcome order, for the rulers.
        let vars: Vec<&OutVar> = nl
            .outputs
            .iter()
            .filter(|o| matches!(o.kind, OutputKind::Four { .. }))
            .flat_map(|o| o.vars.iter())
            .collect();
        let tran: Vec<&Solution> = sols
            .iter()
            .filter(|s| s.plotname == "Transient Analysis")
            .collect();
        let both: Vec<&Solution> = tran.iter().copied().chain([&gsol]).collect();
        for (cand, outcomes) in [
            (
                "port",
                four_all(&nl.outputs, std::slice::from_ref(&gsol), &params),
            ),
            ("dune", four_all(&nl.outputs, &sols, &params)),
        ] {
            let pairs = match pair(&deck, &golden, &outcomes) {
                Ok(p) => p,
                Err(e) => {
                    rows.push(fail(cand, e));
                    continue;
                }
            };
            for (i, (g, o)) in pairs.into_iter().enumerate() {
                let var = vars.get(i).copied();
                let (tau, gating) = match (cand, var) {
                    ("port", Some(v)) => (port_tau(v, &gsol, g.gridsize), true),
                    (_, Some(v)) => sim_tau(v, &both, &tol),
                    _ => (0.0, true),
                };
                let result = match &o.result {
                    Ok(f) => compare_table(g, f, tau),
                    Err(e) => Err(format!("dune: {e}")),
                };
                rows.push(Row {
                    deck: deck.clone(),
                    var: o.var.clone(),
                    cand,
                    gating,
                    result,
                });
            }
        }
    }
    Ok(rows)
}

/// Print the `.four` section of `--ci`; returns whether it passed.
pub fn ci(root: &Path) -> Result<bool> {
    let rows = check_corpus(root)?;
    println!("\n.four (corpus/four, ngspice-47 tables; margin = worst |Δ| / derived bound)");
    println!(
        "{:<14} {:<26} {:<8} {:>8} {:<7} {:>11}  result",
        "deck", "vector", "cand", "margin", "worst", "phases"
    );
    let mut ok = true;
    let mut totals = [Margins::default(), Margins::default()];
    for r in &rows {
        match &r.result {
            Ok(m) => {
                let (w, q) = m.worst();
                let pass = w <= 1.0;
                let verdict = match (pass, r.gating) {
                    (true, _) => "PASS",
                    (false, true) => "FAIL",
                    (false, false) => "REPORT (branch current, not gating)",
                };
                ok &= pass || !r.gating;
                if r.gating {
                    totals[usize::from(r.cand == "dune")].merge(m);
                }
                println!(
                    "{:<14} {:<26} {:<8} {:>8.3} {:<7} {:>5}/{:<5}  {verdict}{}",
                    r.deck,
                    r.var,
                    r.cand,
                    w,
                    q,
                    m.phase_compared,
                    m.phase_compared + m.phase_skipped,
                    if r.gating { "" } else { " (current)" }
                );
            }
            Err(e) => {
                ok = false;
                println!("{:<14} {:<26} {:<8} FAIL: {e}", r.deck, r.var, r.cand);
            }
        }
    }
    for (name, t) in [("port", &totals[0]), ("dune", &totals[1])] {
        println!(
            "worst {name}: dc {:.3e} mag {:.3e} phase {:.3e} nmag {:.3e} nphase {:.3e} thd {:.3e} \
             ({} phases compared, {} inside their error ball)",
            t.dc, t.mag, t.phase, t.nmag, t.nphase, t.thd, t.phase_compared, t.phase_skipped
        );
    }
    Ok(ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
    }

    #[test]
    fn port_matches_ngspice_on_its_own_plots() {
        let rows = check_corpus(&root()).unwrap();
        let port: Vec<&Row> = rows.iter().filter(|r| r.cand == "port").collect();
        assert!(port.len() >= 9, "{} port rows", port.len());
        for r in port {
            let m = r
                .result
                .as_ref()
                .unwrap_or_else(|e| panic!("{}: {e}", r.deck));
            assert!(m.worst().0 <= 1.0, "{} {}: {m:?}", r.deck, r.var);
        }
    }

    #[test]
    fn golden_parser_reads_ngspice_tables() {
        let text = "Fourier analysis for v(2):\n  No. Harmonics: 3, THD: 1 %, Gridsize: 200, \
                    Interpolation Degree: 1, No. Periods: 1\n\nHarmonic Frequency Magnitude\n\
                    -------- ---------\n 0 0 -0.5 0 0 0\n 1 1000 1 -90 1 0\n 2 2000 0.01 3 0.01 93\n";
        let t = parse_golden(text).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].var, "v(2)");
        assert_eq!((t[0].gridsize, t[0].polydegree, t[0].nperiods), (200, 1, 1));
        assert_eq!(t[0].rows[2], [2000.0, 0.01, 3.0, 0.01, 93.0]);
        assert!((t[0].thd() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn angles_compare_modulo_360() {
        assert_eq!(angle_diff(179.0, -179.0), 2.0);
        assert_eq!(angle_diff(-270.0, 90.0), 0.0);
    }
}
