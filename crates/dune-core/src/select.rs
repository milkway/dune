//! Output selection: resolve `.print`/`.plot`/`.save`-style output
//! expressions against a [`Solution`] and hand back plain named
//! columns.
//!
//! This is the one layer every consumer shares (D-0008): the CLI's
//! spice3-format `.print` renders a [`Selection`]; the Python, R and
//! WASM bindings turn the same [`Selection`] into polars/numpy,
//! tibbles and JSON column arrays. No consumer ever parses formatted
//! text, and all of them see identical numbers.

use dune_raw::{Data, VarType};
use num_complex::Complex64;

use crate::error::Error;
use crate::solution::Solution;

/// The derived part of a vector (`VR VI VM VP VDB` in `spice.txt`
/// §.PRINT; `cx_*` in the 3f5 front end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Real,
    Imag,
    Mag,
    /// Phase in radians (the 3f5 default; `set units=degrees` is a
    /// front-end nicety that can live in the consumers).
    Phase,
    /// `20·log10(mag)` (`cx_db`).
    Db,
}

/// One output expression: what a `.print` card, a Python
/// `result["v(2)"]` or an R `dune_run(...)$nodes` selection asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum OutExpr {
    /// `V(node)` / `V(a,b)` with an optional part (`VM(out)`, …).
    Voltage {
        node: String,
        reference: Option<String>,
        part: Option<Part>,
    },
    /// `I(vname)` with an optional part.
    Current { source: String, part: Option<Part> },
    /// A vector by its exact name (`time`, `v(v-sweep)`, …).
    Name(String),
    /// Every vector of the solution.
    All,
}

impl Part {
    fn prefix(self) -> &'static str {
        match self {
            Part::Real => "r",
            Part::Imag => "i",
            Part::Mag => "m",
            Part::Phase => "p",
            Part::Db => "db",
        }
    }

    fn parse(s: &str) -> Option<Option<Part>> {
        Some(match s {
            "" => None,
            "r" => Some(Part::Real),
            "i" => Some(Part::Imag),
            "m" => Some(Part::Mag),
            "p" => Some(Part::Phase),
            "db" => Some(Part::Db),
            _ => return None,
        })
    }

    /// Apply to one complex sample (`cx_real`, `cx_imag`, `cx_mag`,
    /// `cx_ph`, `cx_db`); real data is the `im == 0` case.
    fn apply(self, z: Complex64) -> f64 {
        match self {
            Part::Real => z.re,
            Part::Imag => z.im,
            Part::Mag => z.norm(),
            Part::Phase => z.im.atan2(z.re),
            Part::Db => 20.0 * z.norm().log10(),
        }
    }
}

impl OutExpr {
    /// Parse `v(2)`, `v(a,b)`, `vm(out)`, `i(vin)`, `idb(v1)`,
    /// `all`, or a bare vector name. Case-insensitive, like the 3f5
    /// front end.
    pub fn parse(text: &str) -> Result<OutExpr, Error> {
        let t = text.trim().to_ascii_lowercase();
        if t == "all" {
            return Ok(OutExpr::All);
        }
        let Some(open) = t.find('(') else {
            if t.is_empty() {
                return Err(Error::Build("empty output expression".into()));
            }
            return Ok(OutExpr::Name(t));
        };
        let Some(inner) = t[open..]
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
        else {
            return Err(Error::Build(format!(
                "malformed output expression `{text}`"
            )));
        };
        let head = &t[..open];
        let (kind, part_text) = match head.as_bytes().first() {
            Some(b'v') => ('v', &head[1..]),
            Some(b'i') => ('i', &head[1..]),
            _ => return Ok(OutExpr::Name(t.clone())),
        };
        let Some(part) = Part::parse(part_text) else {
            return Ok(OutExpr::Name(t.clone()));
        };
        let args: Vec<&str> = inner.split(',').map(str::trim).collect();
        match (kind, args.as_slice()) {
            ('v', [node]) if !node.is_empty() => Ok(OutExpr::Voltage {
                node: node.to_string(),
                reference: None,
                part,
            }),
            ('v', [node, reference]) if !node.is_empty() && !reference.is_empty() => {
                Ok(OutExpr::Voltage {
                    node: node.to_string(),
                    reference: Some(reference.to_string()),
                    part,
                })
            }
            ('i', [source]) if !source.is_empty() => Ok(OutExpr::Current {
                source: source.to_string(),
                part,
            }),
            _ => Err(Error::Build(format!(
                "malformed output expression `{text}`"
            ))),
        }
    }

    /// Convert a parsed `.print`/`.save` output variable.
    pub fn from_ir(ov: &dune_netlist::ir::OutVar) -> OutExpr {
        use dune_netlist::ir::{AcSuffix, OutVar};
        let part = |s: &Option<AcSuffix>| {
            s.as_ref().map(|x| match x {
                AcSuffix::Real => Part::Real,
                AcSuffix::Imag => Part::Imag,
                AcSuffix::Mag => Part::Mag,
                AcSuffix::Phase => Part::Phase,
                AcSuffix::Db => Part::Db,
            })
        };
        match ov {
            OutVar::Voltage {
                node,
                reference,
                suffix,
            } => OutExpr::Voltage {
                node: node.clone(),
                reference: reference.clone(),
                part: part(suffix),
            },
            OutVar::Current { vsrc, suffix } => OutExpr::Current {
                source: vsrc.clone(),
                part: part(suffix),
            },
            OutVar::Raw(s) if s == "all" => OutExpr::All,
            OutVar::Raw(s) => OutExpr::Name(s.clone()),
        }
    }

    /// The canonical label (`vm(2)`, `v(a,b)`, `i(vin)`).
    pub fn label(&self) -> String {
        match self {
            OutExpr::Voltage {
                node,
                reference,
                part,
            } => {
                let p = part.map(Part::prefix).unwrap_or("");
                match reference {
                    Some(r) => format!("v{p}({node},{r})"),
                    None => format!("v{p}({node})"),
                }
            }
            OutExpr::Current { source, part } => {
                let p = part.map(Part::prefix).unwrap_or("");
                format!("i{p}({source})")
            }
            OutExpr::Name(n) => n.clone(),
            OutExpr::All => "all".into(),
        }
    }
}

/// Column data: real for `.op`/`.dc`/`.tran`, complex for `.ac`
/// vectors selected without a part.
#[derive(Clone, Debug, PartialEq)]
pub enum ColData {
    Real(Vec<f64>),
    Complex(Vec<Complex64>),
}

impl ColData {
    pub fn len(&self) -> usize {
        match self {
            ColData::Real(v) => v.len(),
            ColData::Complex(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The real view, when real.
    pub fn real(&self) -> Option<&[f64]> {
        match self {
            ColData::Real(v) => Some(v),
            ColData::Complex(_) => None,
        }
    }

    fn as_complex(&self) -> Vec<Complex64> {
        match self {
            ColData::Real(v) => v.iter().map(|x| Complex64::new(*x, 0.0)).collect(),
            ColData::Complex(v) => v.clone(),
        }
    }

    fn sub(&self, other: &ColData) -> ColData {
        match (self, other) {
            (ColData::Real(a), ColData::Real(b)) => {
                ColData::Real(a.iter().zip(b).map(|(x, y)| x - y).collect())
            }
            _ => {
                let a = self.as_complex();
                let b = other.as_complex();
                ColData::Complex(a.iter().zip(&b).map(|(x, y)| x - y).collect())
            }
        }
    }

    fn part(&self, p: Part) -> ColData {
        match self {
            ColData::Real(v) => {
                ColData::Real(v.iter().map(|x| p.apply(Complex64::new(*x, 0.0))).collect())
            }
            ColData::Complex(v) => ColData::Real(v.iter().map(|z| p.apply(*z)).collect()),
        }
    }
}

/// One selected column: pure data, ready for a numpy array, an R
/// vector, a JSON column or the CLI formatter.
#[derive(Clone, Debug)]
pub struct OutColumn {
    pub label: String,
    pub unit: VarType,
    pub data: ColData,
}

/// A resolved selection. `scale` is the plot's abscissa (`time`,
/// `frequency`, `v(v-sweep)`) when the analysis has one, whether or
/// not it was asked for — consumers building tables want it
/// alongside. The scale is real by construction, also in complex
/// plots (the 3f5's `pl_scale` is a real vector).
#[derive(Clone, Debug)]
pub struct Selection {
    pub scale: Option<OutColumn>,
    pub columns: Vec<OutColumn>,
}

impl Solution {
    /// Resolve output expressions into columns.
    pub fn select(&self, exprs: &[OutExpr]) -> Result<Selection, Error> {
        let mut columns = Vec::new();
        for e in exprs {
            match e {
                OutExpr::All => {
                    for (i, (name, unit)) in self.variables.iter().enumerate() {
                        columns.push(OutColumn {
                            label: name.clone(),
                            unit: unit.clone(),
                            data: self.col(i),
                        });
                    }
                }
                OutExpr::Name(n) => {
                    let i = self.index_or_err(n)?;
                    columns.push(OutColumn {
                        label: self.variables[i].0.clone(),
                        unit: self.variables[i].1.clone(),
                        data: self.col(i),
                    });
                }
                OutExpr::Voltage {
                    node,
                    reference,
                    part,
                } => {
                    let a = self.voltage_col(node)?;
                    let mut data = match reference {
                        None => a,
                        Some(r) if r == "0" => a,
                        Some(r) => a.sub(&self.voltage_col(r)?),
                    };
                    if let Some(p) = part {
                        data = data.part(*p);
                    }
                    columns.push(OutColumn {
                        label: e.label(),
                        unit: VarType::Voltage,
                        data,
                    });
                }
                OutExpr::Current { source, part } => {
                    let i = self.index_or_err(&format!("i({source})"))?;
                    let mut data = self.col(i);
                    if let Some(p) = part {
                        data = data.part(*p);
                    }
                    columns.push(OutColumn {
                        label: e.label(),
                        unit: VarType::Current,
                        data,
                    });
                }
            }
        }
        let scale = self.scale.map(|i| OutColumn {
            label: self.variables[i].0.clone(),
            unit: self.variables[i].1.clone(),
            // The abscissa is real by construction; strip the zero
            // imaginary part complex plots carry.
            data: match self.col(i) {
                ColData::Complex(v) => ColData::Real(v.iter().map(|z| z.re).collect()),
                real => real,
            },
        });
        Ok(Selection { scale, columns })
    }

    fn col(&self, i: usize) -> ColData {
        match &self.data {
            Data::Real(cols) => ColData::Real(cols[i].clone()),
            Data::Complex(cols) => ColData::Complex(cols[i].clone()),
        }
    }

    fn index_or_err(&self, name: &str) -> Result<usize, Error> {
        self.index(name)
            .ok_or_else(|| Error::UnknownVector(name.to_string()))
    }

    /// `v(<node>)` by node name, with `v(v-sweep)`-style names also
    /// reachable as plain vectors.
    fn voltage_col(&self, node: &str) -> Result<ColData, Error> {
        let wrapped = format!("v({node})");
        if let Some(i) = self.index(&wrapped) {
            return Ok(self.col(i));
        }
        let i = self.index_or_err(node)?;
        Ok(self.col(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sol() -> Solution {
        Solution {
            plotname: "Transient Analysis".into(),
            variables: vec![
                ("time".into(), VarType::Time),
                ("v(1)".into(), VarType::Voltage),
                ("v(2)".into(), VarType::Voltage),
                ("i(vin)".into(), VarType::Current),
            ],
            data: Data::Real(vec![
                vec![0.0, 1.0, 2.0],
                vec![1.0, 1.0, 1.0],
                vec![0.25, 0.5, -1.0],
                vec![-1e-3, -2e-3, -3e-3],
            ]),
            scale: Some(0),
        }
    }

    fn sol_ac() -> Solution {
        Solution {
            plotname: "AC Analysis".into(),
            variables: vec![
                ("frequency".into(), VarType::Frequency),
                ("v(2)".into(), VarType::Voltage),
            ],
            data: Data::Complex(vec![
                vec![Complex64::new(1.0, 0.0), Complex64::new(10.0, 0.0)],
                vec![Complex64::new(1.0, 0.0), Complex64::new(0.0, -0.5)],
            ]),
            scale: Some(0),
        }
    }

    #[test]
    fn parse_forms() {
        assert_eq!(
            OutExpr::parse("V(2)").unwrap(),
            OutExpr::Voltage {
                node: "2".into(),
                reference: None,
                part: None
            }
        );
        assert_eq!(
            OutExpr::parse("vdb(out,ref)").unwrap().label(),
            "vdb(out,ref)"
        );
        assert_eq!(
            OutExpr::parse("I(Vin)").unwrap(),
            OutExpr::Current {
                source: "vin".into(),
                part: None
            }
        );
        assert_eq!(OutExpr::parse("all").unwrap(), OutExpr::All);
        assert_eq!(
            OutExpr::parse("time").unwrap(),
            OutExpr::Name("time".into())
        );
        assert!(OutExpr::parse("v(").is_err());
        assert!(OutExpr::parse("").is_err());
        // Unknown head letters fall back to a plain name.
        assert_eq!(
            OutExpr::parse("par(x)").unwrap(),
            OutExpr::Name("par(x)".into())
        );
    }

    #[test]
    fn select_simple_and_differential() {
        let s = sol();
        let sel = s
            .select(&[
                OutExpr::parse("v(2)").unwrap(),
                OutExpr::parse("v(1,2)").unwrap(),
                OutExpr::parse("i(vin)").unwrap(),
            ])
            .unwrap();
        assert_eq!(sel.columns[0].data.real().unwrap(), &[0.25, 0.5, -1.0]);
        assert_eq!(sel.columns[1].label, "v(1,2)");
        assert_eq!(sel.columns[1].data.real().unwrap(), &[0.75, 0.5, 2.0]);
        assert_eq!(sel.columns[2].unit, VarType::Current);
        // The scale rides along without being asked for.
        assert_eq!(sel.scale.as_ref().unwrap().label, "time");
    }

    #[test]
    fn parts_on_real_data() {
        let s = sol();
        let sel = s
            .select(&[
                OutExpr::parse("vm(2)").unwrap(),
                OutExpr::parse("vdb(1)").unwrap(),
                OutExpr::parse("vp(2)").unwrap(),
            ])
            .unwrap();
        assert_eq!(sel.columns[0].data.real().unwrap(), &[0.25, 0.5, 1.0]);
        assert_eq!(sel.columns[1].data.real().unwrap(), &[0.0, 0.0, 0.0]);
        assert_eq!(sel.columns[2].data.real().unwrap()[2], std::f64::consts::PI);
    }

    #[test]
    fn parts_on_complex_data() {
        let s = sol_ac();
        let sel = s
            .select(&[
                OutExpr::parse("v(2)").unwrap(),
                OutExpr::parse("vm(2)").unwrap(),
                OutExpr::parse("vdb(2)").unwrap(),
                OutExpr::parse("vp(2)").unwrap(),
            ])
            .unwrap();
        // No part: the complex column comes through complex.
        assert!(matches!(sel.columns[0].data, ColData::Complex(_)));
        let m = sel.columns[1].data.real().unwrap();
        assert_eq!(m, &[1.0, 0.5]);
        let db = sel.columns[2].data.real().unwrap();
        assert!((db[1] - 20.0 * 0.5f64.log10()).abs() < 1e-12);
        let ph = sel.columns[3].data.real().unwrap();
        assert!((ph[1] + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // The frequency scale is delivered real.
        assert_eq!(
            sel.scale.as_ref().unwrap().data.real().unwrap(),
            &[1.0, 10.0]
        );
    }

    #[test]
    fn all_and_errors() {
        let s = sol();
        let sel = s.select(&[OutExpr::All]).unwrap();
        assert_eq!(sel.columns.len(), 4);
        let err = s.select(&[OutExpr::parse("v(99)").unwrap()]).unwrap_err();
        assert!(matches!(err, Error::UnknownVector(_)), "{err}");
    }

    #[test]
    fn v_zero_reference_is_identity() {
        let s = sol();
        let sel = s.select(&[OutExpr::parse("v(2,0)").unwrap()]).unwrap();
        assert_eq!(sel.columns[0].data.real().unwrap(), &[0.25, 0.5, -1.0]);
    }
}
