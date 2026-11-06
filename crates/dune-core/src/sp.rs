//! `.sp` — ngspice's S-parameter analysis (`analysis/span.c`,
//! `cktspdum.c`, the RFSPICE parts of `vsrc/` and `nevalsrc.c`;
//! Alessio Cacciatori 2021). It postdates the 3f5, so ngspice-47 is
//! both the contract and the oracle.
//!
//! Ports are V sources carrying `PORTNUM N <Z0 Z>`: in every
//! analysis such a source drives its `+` node through the series
//! reference impedance `Z0` (internal node `<name>#res`). Per
//! frequency `.sp` does one AC load with every V-source stimulus off
//! (`MODESP`) and one complex factorization, then for each port `j`
//! solves with 1 V on port `j`'s branch and records, for every port
//! `i`, the Kurokawa power waves
//!
//! ```text
//! a_i = (V_i + Z0_i·I_i) / (2·√Z0_i)    b_i = (V_i − Z0_i·I_i) / (2·√Z0_i)
//! ```
//!
//! (`V_i` across the external terminals, `I_i` into the network) as
//! column `j` of `A` and `B` (`CKTspCalcPowerWave`). Then
//! `S = B·A⁻¹`, and with `Z0 = diag(Z0_i)`, `Gn = diag(1/√Z0_i)`
//! (`CKTspCalcSMatrix`):
//!
//! ```text
//! Z = Gn⁻¹ · (E − S)⁻¹ · (S·Z0 + Z0) · Gn
//! Y = Gn⁻¹ · (S·Z0 + Z0)⁻¹ · (E − S) · Gn
//! ```
//!
//! With `DONOISE = 1` the analysis adds the short-circuit noise
//! current correlation matrix `Cy` (A²/Hz): each thermal/shot
//! generator `(n1, n2, PSD)` of the devices is carried to the ports
//! through the adjoint solutions (unit current into each port's `+`
//! node, ports terminated in `Z0`), giving the terminated port
//! voltages `v`, whose Norton currents are `i = v/Z0 + Y·v`;
//! `Cy += i·iᴴ`. Flicker noise never reaches `Cy` (it goes through
//! `NevalSrc`'s `N_GAIN` path, which records nothing under `.sp`).
//! For two ports the noise parameters follow from `Cy/(4kT)`
//! (Maas, *Noise*): `NF` and `NFmin` in dB at the port-1 reference
//! admittance, `SOpt` and `Rn`.

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::ac::AcSweep;
use crate::circuit::{Circuit, PortInfo, VarKind, saved_node};
use crate::engine::{AcStepType, Engine, MAX_OUTPUT_VALUES, sweep_budget};
use crate::error::Error;
use crate::options::Options;
use crate::solution::Solution;

/// Result of an S-parameter analysis: one complex plot `SP Analysis`
/// over `frequency` holding the node voltages and branch currents of
/// the last port's excitation, then `S_i_j`, `Y_i_j`, `Z_i_j`
/// (`i` the responding port, `j` the driven one) and, with noise,
/// `i(Cy_i_j)` plus, for two ports, `NF`, `SOpt`, `NFmin`, `Rn` —
/// named and typed as ngspice-47's rawfile writes them.
#[derive(Clone, Debug)]
pub struct Sp {
    pub solution: Solution,
    pub notes: Vec<String>,
}

type CMat = Vec<Vec<Complex64>>;

impl Circuit {
    /// Run `.sp DEC|OCT|LIN N FSTART FSTOP <DONOISE>` over the
    /// circuit's RF ports (V sources with [`crate::RfPort`]); at
    /// least two ports are required.
    pub fn sp(
        &self,
        opts: &Options,
        sweep: AcSweep,
        points: usize,
        fstart: f64,
        fstop: f64,
        donoise: bool,
    ) -> Result<Sp, Error> {
        // `SPsetParm` refuses negative frequencies; `SPan` refuses a
        // non-positive start on log sweeps.
        if !(fstart.is_finite() && fstop.is_finite()) || fstart < 0.0 || fstop < 0.0 {
            return Err(Error::Build(".sp frequencies are inconsistent".into()));
        }
        if sweep != AcSweep::Linear && fstart <= 0.0 {
            return Err(Error::Build(".sp start frequency must be positive".into()));
        }
        let built = self.build(opts)?;
        let notes = built.notes.clone();
        let ports = built.ports.clone();
        match ports.len() {
            0 => {
                return Err(Error::Build(
                    "no RF port is present, cannot run sp analysis".into(),
                ));
            }
            1 => {
                return Err(Error::Build(
                    "only one RF port is found, sp needs at least two".into(),
                ));
            }
            _ => {}
        }
        let np = ports.len();
        let mut engine = Engine::new(built);

        let n = points.max(1);
        let step_type = match sweep {
            AcSweep::Decade => AcStepType::Decade,
            AcSweep::Octave => AcStepType::Octave,
            AcSweep::Linear => AcStepType::Linear,
        };
        // `span.c` keeps the 3f5 stepping: `10^(1/n)` per decade even
        // where ngspice-47's `.ac` re-spaces the points onto FSTOP.
        let freq_delta = match sweep {
            AcSweep::Decade => f64::exp(f64::ln(10.0) / n as f64),
            AcSweep::Octave => f64::exp(f64::ln(2.0) / n as f64),
            AcSweep::Linear => {
                if n - 1 > 1 {
                    (fstop - fstart) / (n - 1) as f64
                } else {
                    0.0
                }
            }
        };
        let extra = np
            .saturating_mul(np)
            .saturating_mul(if donoise { 4 } else { 3 })
            + 4;
        let row_len = engine.vars.len().saturating_add(extra);
        sweep_budget(step_type, n, fstart.max(1e-300), fstop, row_len, "sp")?;

        engine.sp_prepare(opts)?;

        let freq_tol = match sweep {
            AcSweep::Linear => freq_delta * opts.reltol,
            _ => freq_delta * fstop * opts.reltol,
        };

        let mut cols = Columns::new(&engine, np, donoise);
        let mut freq = fstart;
        while freq <= fstop + freq_tol {
            if (cols.rows + 1).saturating_mul(row_len) > MAX_OUTPUT_VALUES {
                return Err(Error::Build(
                    "sp sweep produces too many points; refusing".into(),
                ));
            }
            let (forward, adjoint) = engine.sp_point(freq, &ports, donoise)?;
            let (a, b) = power_waves(&ports, &forward);
            let (s, y, z) = network_matrices(&a, &b, &ports);
            let noise = donoise.then(|| {
                let gens = engine.sp_noise_generators(freq, opts.temp);
                let cy = noise_correlation(&gens, &adjoint, &y, &ports);
                let params = (np == 2).then(|| noise_params(&cy, &y, &ports, opts.temp));
                (cy, params)
            });
            let last = forward.last().map(Vec::as_slice).unwrap_or(&[]);
            cols.push(freq, last, &s, &y, &z, noise.as_ref());

            match sweep {
                AcSweep::Decade | AcSweep::Octave => {
                    freq *= freq_delta;
                    if freq_delta == 1.0 {
                        break;
                    }
                }
                AcSweep::Linear => {
                    freq += freq_delta;
                    if freq_delta == 0.0 {
                        break;
                    }
                }
            }
        }

        let (variables, columns) = cols.finish();
        Ok(Sp {
            solution: Solution {
                plotname: "SP Analysis".to_string(),
                variables,
                data: Data::Complex(columns),
                scale: Some(0),
            },
            notes,
        })
    }
}

/// `CKTspCalcPowerWave` over every excitation: `A[i][j]`, `B[i][j]`
/// are port `i`'s incident and scattered waves with port `j` driven.
/// `I_i = −I(branch)` is the current into the network (the branch
/// current flows from the internal node through the source).
fn power_waves(ports: &[PortInfo], forward: &[Vec<Complex64>]) -> (CMat, CMat) {
    let np = ports.len();
    let mut a = vec![vec![Complex64::new(0.0, 0.0); np]; np];
    let mut b = a.clone();
    for (col, sol) in forward.iter().enumerate() {
        for (row, p) in ports.iter().enumerate() {
            let ki = 0.5 / p.z0.sqrt();
            let i = -sol[p.br];
            let v = sol[p.pos] - sol[p.neg];
            a[row][col] = (v + i * p.z0) * ki;
            b[row][col] = (v - i * p.z0) * ki;
        }
    }
    (a, b)
}

/// `CKTspCalcSMatrix`: S, Y and Z in the C's operation order.
fn network_matrices(a: &CMat, b: &CMat, ports: &[PortInfo]) -> (CMat, CMat, CMat) {
    let np = ports.len();
    let diag = |f: &dyn Fn(&PortInfo) -> f64| -> CMat {
        let mut m = zeros(np);
        for (i, p) in ports.iter().enumerate() {
            m[i][i] = Complex64::new(f(p), 0.0);
        }
        m
    };
    let zref = diag(&|p| p.z0);
    let gn = diag(&|p| 2.0 * (0.5 / p.z0.sqrt()));
    let gninv = diag(&|p| 1.0 / (2.0 * (0.5 / p.z0.sqrt())));
    let eye = diag(&|_| 1.0);

    let s = mul(b, &inverse(a));
    let sz0_z0 = add(&mul(&s, &zref), &zref);
    let e_s = sub(&eye, &s);
    let z = mul(&gninv, &mul(&inverse(&e_s), &mul(&sz0_z0, &gn)));
    let y = mul(&gninv, &mul(&inverse(&sz0_z0), &mul(&e_s, &gn)));
    (s, y, z)
}

/// `NevalSrc` under `DOING_SP`, summed over every generator: the
/// short-circuit noise-current correlation matrix (A²/Hz).
fn noise_correlation(
    gens: &[crate::matrix::NoiseGen],
    adjoint: &[Vec<Complex64>],
    y: &CMat,
    ports: &[PortInfo],
) -> CMat {
    let np = ports.len();
    let mut cy = zeros(np);
    let mut v = vec![Complex64::new(0.0, 0.0); np];
    let mut cur = v.clone();
    for &(n1, n2, density) in gens {
        let inoise = density.sqrt();
        for (s, adj) in adjoint.iter().enumerate() {
            v[s] = (adj[n1] - adj[n2]) * inoise;
        }
        for d in 0..np {
            let mut c = v[d] * (1.0 / ports[d].z0);
            for s in 0..np {
                c += y[d][s] * v[s];
            }
            cur[d] = c;
        }
        for d in 0..np {
            for s in 0..np {
                cy[d][s] += cur[d] * cur[s].conj();
            }
        }
    }
    cy
}

/// Two-port noise parameters (`CKTspnoise`, `N_CALC`).
#[derive(Clone, Copy, Debug)]
struct NoiseParams {
    /// Noise figure at the port-1 reference admittance, dB.
    nf: f64,
    /// Optimum source reflection coefficient.
    sopt: Complex64,
    /// Minimum noise figure, dB.
    nfmin: f64,
    /// Equivalent noise resistance, Ω.
    rn: f64,
}

fn noise_params(cy: &CMat, y: &CMat, ports: &[PortInfo], temp: f64) -> NoiseParams {
    let knorm = 4.0 * crate::consts::BOLTZ * temp;
    let mut t: CMat = cy
        .iter()
        .map(|r| r.iter().map(|c| c * (1.0 / knorm)).collect())
        .collect();
    let y21mod = y[1][0].norm_sqr();
    let mut rn = t[1][1].re / y21mod;
    if rn.abs() < 1e-30 {
        rn = 1e-30;
    }
    if t[1][1].re.abs() < 1e-30 && t[1][1].im.abs() < 1e-30 {
        t[1][1].re = 1e-30;
    }
    let ycor = y[0][0] - (t[0][1] / t[1][1]) * y[1][0];
    let y11_ycor = (y[0][0] - ycor).norm_sqr();
    let gu = t[0][0].re - rn * y11_ycor;
    let ysopt = Complex64::new((ycor.re * ycor.re + gu / rn).sqrt(), -ycor.im);
    let y0 = Complex64::new(1.0 / ports[0].z0, 0.0);
    let sopt = (y0 - ysopt) / (y0 + ysopt);
    let fmin = 1.0 + 2.0 * rn * (ycor.re + ysopt.re);
    let nf = fmin + (rn / y0.re) * (y0 - ysopt).norm_sqr();
    NoiseParams {
        nf: 10.0 * nf.log10(),
        sopt,
        nfmin: 10.0 * fmin.log10(),
        rn,
    }
}

/// The plot's columns, filled row by row (`CKTspDump`).
struct Columns {
    variables: Vec<(String, VarType)>,
    /// Var index per saved node/branch column.
    saved: Vec<usize>,
    columns: Vec<Vec<Complex64>>,
    np: usize,
    rows: usize,
}

impl Columns {
    fn new(engine: &Engine, np: usize, donoise: bool) -> Self {
        let mut variables = vec![("frequency".to_string(), VarType::Frequency)];
        let mut saved = Vec::new();
        let vars = &engine.vars;
        for i in 1..vars.len() {
            let name = &vars.names[i];
            match vars.kinds[i] {
                VarKind::Node if saved_node(name) => {
                    variables.push((format!("v({name})"), VarType::Voltage));
                    saved.push(i);
                }
                VarKind::Branch => {
                    variables.push((format!("i({name})"), VarType::Current));
                    saved.push(i);
                }
                _ => {}
            }
        }
        // `guess_type` in ngspice's `outitf.c` types the `.sp`
        // vectors by prefix, and its rawfile writer wraps the
        // current-typed `Cy_` ones in `i(…)`.
        let matrix = |variables: &mut Vec<(String, VarType)>, prefix: &str, t: VarType| {
            for i in 1..=np {
                for j in 1..=np {
                    variables.push((format!("{prefix}_{i}_{j}"), t.clone()));
                }
            }
        };
        matrix(&mut variables, "S", VarType::SParam);
        matrix(&mut variables, "Y", VarType::Other("admittance".into()));
        matrix(&mut variables, "Z", VarType::Other("impedance".into()));
        if donoise {
            for i in 1..=np {
                for j in 1..=np {
                    variables.push((format!("i(Cy_{i}_{j})"), VarType::Current));
                }
            }
            if np == 2 {
                variables.push(("NF".into(), VarType::Other("decibel".into())));
                variables.push(("SOpt".into(), VarType::NoType));
                variables.push(("NFmin".into(), VarType::Other("decibel".into())));
                variables.push(("Rn".into(), VarType::Other("impedance".into())));
            }
        }
        let columns = vec![Vec::new(); variables.len()];
        Columns {
            variables,
            saved,
            columns,
            np,
            rows: 0,
        }
    }

    fn push(
        &mut self,
        freq: f64,
        sol: &[Complex64],
        s: &CMat,
        y: &CMat,
        z: &CMat,
        noise: Option<&(CMat, Option<NoiseParams>)>,
    ) {
        let mut k = 0;
        let mut put = |cols: &mut Vec<Vec<Complex64>>, v: Complex64| {
            cols[k].push(v);
            k += 1;
        };
        put(&mut self.columns, Complex64::new(freq, 0.0));
        for &i in &self.saved {
            put(&mut self.columns, sol[i]);
        }
        let mut mats: Vec<&CMat> = vec![s, y, z];
        if let Some((cy, _)) = noise {
            mats.push(cy);
        }
        for m in mats {
            for row in m.iter().take(self.np) {
                for &v in row.iter().take(self.np) {
                    put(&mut self.columns, v);
                }
            }
        }
        if let Some((_, Some(p))) = noise {
            put(&mut self.columns, Complex64::new(p.nf, 0.0));
            put(&mut self.columns, p.sopt);
            put(&mut self.columns, Complex64::new(p.nfmin, 0.0));
            put(&mut self.columns, Complex64::new(p.rn, 0.0));
        }
        self.rows += 1;
    }

    fn finish(self) -> (Vec<(String, VarType)>, Vec<Vec<Complex64>>) {
        (self.variables, self.columns)
    }
}

fn zeros(n: usize) -> CMat {
    vec![vec![Complex64::new(0.0, 0.0); n]; n]
}

fn mul(a: &CMat, b: &CMat) -> CMat {
    let n = a.len();
    let mut c = zeros(n);
    for i in 0..n {
        for k in 0..n {
            let aik = a[i][k];
            for j in 0..n {
                c[i][j] += aik * b[k][j];
            }
        }
    }
    c
}

fn add(a: &CMat, b: &CMat) -> CMat {
    a.iter()
        .zip(b)
        .map(|(r, s)| r.iter().zip(s).map(|(x, y)| x + y).collect())
        .collect()
}

fn sub(a: &CMat, b: &CMat) -> CMat {
    a.iter()
        .zip(b)
        .map(|(r, s)| r.iter().zip(s).map(|(x, y)| x - y).collect())
        .collect()
}

/// `cinverse` (`maths/dense/dense.c`) computes `adj(A)/det(A)`; this
/// is the same inverse by Gauss–Jordan elimination with partial
/// pivoting (cofactor expansion is factorial in the port count). An
/// exactly singular matrix gives all-NaN, as the C's division by a
/// zero determinant does.
fn inverse(a: &CMat) -> CMat {
    let n = a.len();
    let mut m = a.clone();
    let mut inv = zeros(n);
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = Complex64::new(1.0, 0.0);
    }
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&x, &y| m[x][col].norm().total_cmp(&m[y][col].norm()))
            .unwrap_or(col);
        if m[piv][col].norm() == 0.0 {
            return vec![vec![Complex64::new(f64::NAN, f64::NAN); n]; n];
        }
        m.swap(col, piv);
        inv.swap(col, piv);
        let d = m[col][col].inv();
        for j in 0..n {
            m[col][j] *= d;
            inv[col][j] *= d;
        }
        for r in 0..n {
            if r != col {
                let f = m[r][col];
                if f != Complex64::new(0.0, 0.0) {
                    for j in 0..n {
                        let (mc, ic) = (m[col][j], inv[col][j]);
                        m[r][j] -= f * mc;
                        inv[r][j] -= f * ic;
                    }
                }
            }
        }
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::{Resistor, RfPort, Vsource};

    fn port(name: &str, node: &str, num: u32, z0: f64) -> Vsource {
        Vsource {
            name: name.into(),
            nodes: [node.into(), "0".into()],
            dc: Some(0.0),
            wave: None,
            ac: None,
            distof1: None,
            distof2: None,
            port: Some(RfPort { num, z0 }),
        }
    }

    fn close(a: Complex64, b: Complex64, tol: f64) -> bool {
        (a - b).norm() <= tol * (1.0 + b.norm())
    }

    /// A matched 50 Ω T-pad with voltage ratio K = 2 (6.02 dB):
    /// series arms R1 = Z0·(K−1)/(K+1), shunt R2 = 2·Z0·K/(K²−1).
    /// Closed form: S11 = S22 = 0, S21 = S12 = 1/K; Z11 = Z22 =
    /// R1 + R2, Z12 = Z21 = R2; Y = Z⁻¹.
    #[test]
    fn matched_t_attenuator() {
        let (z0, k) = (50.0, 2.0);
        let r1 = z0 * (k - 1.0) / (k + 1.0);
        let r2 = 2.0 * z0 * k / (k * k - 1.0);
        let mut ckt = Circuit::new("t-pad");
        // Port 2 first: the ports are ordered by index, not by card.
        ckt.add(port("v2", "out", 2, z0)).unwrap();
        ckt.add(port("v1", "in", 1, z0)).unwrap();
        for (name, a, b, r) in [
            ("ra", "in", "mid", r1),
            ("rb", "mid", "out", r1),
            ("rc", "mid", "0", r2),
        ] {
            ckt.add(Resistor {
                name: name.into(),
                nodes: [a.into(), b.into()],
                value: r,
            })
            .unwrap();
        }
        let sp = ckt
            .sp(&Options::default(), AcSweep::Decade, 1, 1e3, 1e6, true)
            .unwrap();
        let sol = &sp.solution;
        assert_eq!(sol.plotname, "SP Analysis");
        assert_eq!(sol.num_points(), 4);
        let at = |name: &str| sol.column_complex(name).unwrap()[2];
        let c = |re: f64| Complex64::new(re, 0.0);
        for (name, want) in [
            ("S_1_1", 0.0),
            ("S_2_2", 0.0),
            ("S_2_1", 1.0 / k),
            ("S_1_2", 1.0 / k),
            ("Z_1_1", r1 + r2),
            ("Z_2_2", r1 + r2),
            ("Z_1_2", r2),
            ("Z_2_1", r2),
        ] {
            assert!(close(at(name), c(want), 1e-12), "{name}: {}", at(name));
        }
        let det = (r1 + r2) * (r1 + r2) - r2 * r2;
        assert!(close(at("Y_1_1"), c((r1 + r2) / det), 1e-12));
        assert!(close(at("Y_2_1"), c(-r2 / det), 1e-12));
        // Noise: a passive network's Cy is 4kT·Re(Y) (Bosma).
        let four_kt = 4.0 * crate::consts::BOLTZ * Options::default().temp;
        assert!(close(at("i(Cy_1_1)"), c(four_kt * (r1 + r2) / det), 1e-9));
        assert!(close(at("i(Cy_2_1)"), c(-four_kt * r2 / det), 1e-9));
        // A matched passive attenuator's noise figure is its loss.
        let nf = at("NF").re;
        assert!((nf - 20.0 * k.log10()).abs() < 1e-9, "NF = {nf}");
        // The node vectors are those of the last port's excitation
        // (port 2 driven by 1 V behind 50 Ω into a matched load).
        assert!(close(at("v(out)"), c(0.5), 1e-12));
        assert!(close(at("v(v2#res)"), c(1.0), 1e-12));
    }

    #[test]
    fn port_checks() {
        let mut ckt = Circuit::new("ports");
        ckt.add(port("v1", "a", 1, 50.0)).unwrap();
        ckt.add(Resistor {
            name: "r1".into(),
            nodes: ["a".into(), "0".into()],
            value: 50.0,
        })
        .unwrap();
        let opts = Options::default();
        let e = ckt
            .sp(&opts, AcSweep::Linear, 2, 1.0, 2.0, false)
            .unwrap_err();
        assert!(e.to_string().contains("only one RF port"), "{e}");
        ckt.add(port("v3", "a", 3, 50.0)).unwrap();
        let e = ckt
            .sp(&opts, AcSweep::Linear, 2, 1.0, 2.0, false)
            .unwrap_err();
        assert!(e.to_string().contains("v3: incorrect port ordering"), "{e}");
        // The check runs for every analysis, as `VSRCtemp` does.
        assert!(ckt.op(&opts).is_err());
    }

    #[test]
    fn inverse_roundtrip_and_singular() {
        let m: CMat = vec![
            vec![Complex64::new(0.0, 1.0), Complex64::new(2.0, 0.0)],
            vec![Complex64::new(3.0, -1.0), Complex64::new(0.5, 0.25)],
        ];
        let p = mul(&m, &inverse(&m));
        for (i, row) in p.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!(close(*v, Complex64::new(want, 0.0), 1e-14));
            }
        }
        let z = zeros(2);
        assert!(inverse(&z)[0][0].re.is_nan());
    }
}
