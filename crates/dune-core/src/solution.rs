//! Analysis results in the shape of a rawfile plot (D-0008): named
//! variables with `dune_raw::VarType`s and real or complex columns,
//! so the oracle, the CLI and the bindings all read the same thing.

use dune_raw::{Data, Plot, VarType, Variable};
use num_complex::Complex64;

#[derive(Clone, Debug)]
pub struct Solution {
    /// The `Plotname:` (e.g. `Operating Point`).
    pub plotname: String,
    pub variables: Vec<(String, VarType)>,
    /// One column per variable, all the same length; complex for
    /// `.ac`, real for everything else (mirrors `dune_raw::Data`).
    pub data: Data,
    /// Index of the abscissa (`time`, `frequency`, `v(v-sweep)`) in
    /// `variables`; `None` for the operating point.
    pub scale: Option<usize>,
}

impl Solution {
    pub fn num_points(&self) -> usize {
        self.data.num_points()
    }

    /// Real column by variable name (case-insensitive, like the 3f5
    /// front end); `None` when missing or complex.
    pub fn column(&self, name: &str) -> Option<&[f64]> {
        let i = self.index(name)?;
        match &self.data {
            Data::Real(cols) => Some(&cols[i]),
            Data::Complex(_) => None,
        }
    }

    /// Complex column by variable name.
    pub fn column_complex(&self, name: &str) -> Option<&[Complex64]> {
        let i = self.index(name)?;
        match &self.data {
            Data::Complex(cols) => Some(&cols[i]),
            Data::Real(_) => None,
        }
    }

    pub(crate) fn index(&self, name: &str) -> Option<usize> {
        self.variables
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// Package as a rawfile plot (padded).
    pub fn to_plot(&self, title: &str, date: &str) -> Plot {
        Plot {
            title: title.to_string(),
            date: date.to_string(),
            name: self.plotname.clone(),
            padded: true,
            commands: Vec::new(),
            options: Vec::new(),
            dimensions: None,
            variables: self
                .variables
                .iter()
                .map(|(n, t)| Variable::new(n.clone(), t.clone()))
                .collect(),
            data: self.data.clone(),
        }
    }
}
