//! Analysis results in the shape of a rawfile plot (D-0008): named
//! variables with `dune_raw::VarType`s and real/complex columns, so
//! the oracle, the CLI and the bindings all read the same thing.

use dune_raw::{Data, Plot, VarType, Variable};

#[derive(Clone, Debug)]
pub struct Solution {
    /// The `Plotname:` (e.g. `Operating Point`).
    pub plotname: String,
    pub variables: Vec<(String, VarType)>,
    /// One column per variable, all the same length.
    pub columns: Vec<Vec<f64>>,
    /// Index of the abscissa (`time`, `v(v-sweep)`, …) in
    /// `variables`; `None` for the operating point.
    pub scale: Option<usize>,
}

impl Solution {
    pub fn num_points(&self) -> usize {
        self.columns.first().map(Vec::len).unwrap_or(0)
    }

    /// Column by variable name (case-insensitive, like the 3f5
    /// front end).
    pub fn column(&self, name: &str) -> Option<&[f64]> {
        self.variables
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|i| self.columns[i].as_slice())
    }

    /// Package as a rawfile plot (real flags, padded).
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
            data: Data::Real(self.columns.clone()),
        }
    }
}
