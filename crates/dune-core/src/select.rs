//! Output selection: resolve `.print`/`.plot`/`.save`-style output
//! expressions against a [`Solution`] and hand back plain named
//! columns.
//!
//! This is the one layer every consumer shares (D-0008): the CLI's
//! spice3-format `.print` renders a [`Selection`]; the Python, R and
//! WASM bindings turn the same [`Selection`] into polars/numpy,
//! tibbles and JSON column arrays. No consumer ever parses formatted
//! text, and all of them see identical numbers.

use dune_raw::VarType;

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

    /// Apply to a real-valued sample (complex comes with `.ac`).
    fn apply(self, x: f64) -> f64 {
        match self {
            Part::Real => x,
            Part::Imag => 0.0,
            Part::Mag => x.abs(),
            Part::Phase => f64::atan2(0.0, x),
            Part::Db => 20.0 * x.abs().log10(),
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

/// One selected column: pure data, ready for a numpy array, an R
/// vector, a JSON `Float64Array` or the CLI formatter.
#[derive(Clone, Debug)]
pub struct OutColumn {
    pub label: String,
    pub unit: VarType,
    pub data: Vec<f64>,
}

/// A resolved selection. `scale` is the plot's abscissa (`time`,
/// `v(v-sweep)`, …) when the analysis has one, whether or not it was
/// asked for — consumers building tables want it alongside.
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
                            data: self.columns[i].clone(),
                        });
                    }
                }
                OutExpr::Name(n) => {
                    let (i, (name, unit)) = self.lookup(n)?;
                    columns.push(OutColumn {
                        label: name.clone(),
                        unit: unit.clone(),
                        data: self.columns[i].clone(),
                    });
                }
                OutExpr::Voltage {
                    node,
                    reference,
                    part,
                } => {
                    let a = self.voltage_data(node)?;
                    let mut data = match reference {
                        None => a.to_vec(),
                        Some(r) if r == "0" => a.to_vec(),
                        Some(r) => {
                            let b = self.voltage_data(r)?;
                            a.iter().zip(b).map(|(x, y)| x - y).collect()
                        }
                    };
                    if let Some(p) = part {
                        for v in &mut data {
                            *v = p.apply(*v);
                        }
                    }
                    columns.push(OutColumn {
                        label: e.label(),
                        unit: VarType::Voltage,
                        data,
                    });
                }
                OutExpr::Current { source, part } => {
                    let (i, _) = self.lookup(&format!("i({source})"))?;
                    let mut data = self.columns[i].clone();
                    if let Some(p) = part {
                        for v in &mut data {
                            *v = p.apply(*v);
                        }
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
            data: self.columns[i].clone(),
        });
        Ok(Selection { scale, columns })
    }

    fn lookup(&self, name: &str) -> Result<(usize, &(String, VarType)), Error> {
        self.variables
            .iter()
            .enumerate()
            .find(|(_, (n, _))| n.eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::UnknownVector(name.to_string()))
    }

    /// `v(<node>)` by node name, with `v(v-sweep)`-style names also
    /// reachable as plain vectors.
    fn voltage_data(&self, node: &str) -> Result<&[f64], Error> {
        let wrapped = format!("v({node})");
        if let Ok((i, _)) = self.lookup(&wrapped) {
            return Ok(&self.columns[i]);
        }
        let (i, _) = self.lookup(node)?;
        Ok(&self.columns[i])
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
            columns: vec![
                vec![0.0, 1.0, 2.0],
                vec![1.0, 1.0, 1.0],
                vec![0.25, 0.5, -1.0],
                vec![-1e-3, -2e-3, -3e-3],
            ],
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
        assert_eq!(sel.columns[0].data, vec![0.25, 0.5, -1.0]);
        assert_eq!(sel.columns[1].label, "v(1,2)");
        assert_eq!(sel.columns[1].data, vec![0.75, 0.5, 2.0]);
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
        assert_eq!(sel.columns[0].data, vec![0.25, 0.5, 1.0]);
        assert_eq!(sel.columns[1].data, vec![0.0, 0.0, 0.0]); // 20·log10(1)
        assert_eq!(sel.columns[2].data[2], std::f64::consts::PI); // phase of -1
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
        assert_eq!(sel.columns[0].data, vec![0.25, 0.5, -1.0]);
    }
}
