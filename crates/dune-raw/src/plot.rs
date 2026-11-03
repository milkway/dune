use num_complex::Complex64;

use crate::VarType;

/// One variable (a `dvec` in the 3f5 sources) from the `Variables:` table.
///
/// The optional attributes mirror the "strange options" the 3f5 reader
/// accepts on a variable line: `min=`, `max=`, `color=`, `scale=`,
/// `grid=`, `plot=`, `dims=`.
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    pub name: String,
    pub vtype: VarType,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub color: Option<String>,
    /// Name of another vector used as this one's scale.
    pub scale: Option<String>,
    pub grid: Option<i64>,
    pub plottype: Option<i64>,
    /// Per-vector dimensions, only when different from the plot default.
    /// Their product is the vector's true length, which may be shorter
    /// than the plot's `No. Points` (the rest is padding when the plot
    /// is `padded`).
    pub dims: Option<Vec<usize>>,
}

impl Variable {
    /// A plain variable with no optional attributes.
    pub fn new(name: impl Into<String>, vtype: VarType) -> Self {
        Variable {
            name: name.into(),
            vtype,
            min: None,
            max: None,
            color: None,
            scale: None,
            grid: None,
            plottype: None,
            dims: None,
        }
    }

    /// The vector's true length in a plot with `npoints` rows
    /// (`fixdims` in the C: the product of `dims=` when valid).
    pub fn length(&self, npoints: usize) -> usize {
        match &self.dims {
            Some(d) => {
                let n: usize = d.iter().product();
                if n > npoints { npoints } else { n }
            }
            None => npoints,
        }
    }
}

/// Plot data, one column per variable, in `Variables:` order.
///
/// A plot is complex as a whole when *any* vector is complex (`Flags:
/// complex`); real vectors inside a complex plot are written with a zero
/// imaginary part, exactly as `raw_write` does.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    Real(Vec<Vec<f64>>),
    Complex(Vec<Vec<Complex64>>),
}

impl Data {
    pub fn num_vars(&self) -> usize {
        match self {
            Data::Real(c) => c.len(),
            Data::Complex(c) => c.len(),
        }
    }

    /// Length of the longest column (the plot's `No. Points`).
    pub fn num_points(&self) -> usize {
        match self {
            Data::Real(c) => c.iter().map(Vec::len).max().unwrap_or(0),
            Data::Complex(c) => c.iter().map(Vec::len).max().unwrap_or(0),
        }
    }
}

/// One plot: a header plus a block of data (`struct plot` in the C).
#[derive(Clone, Debug, PartialEq)]
pub struct Plot {
    pub title: String,
    pub date: String,
    /// The `Plotname:` line, e.g. `Transient Analysis`.
    pub name: String,
    /// `Flags: … padded`/`unpadded`: whether short vectors are padded
    /// with zeros up to `No. Points` in the data section.
    pub padded: bool,
    pub commands: Vec<String>,
    pub options: Vec<String>,
    /// `Dimensions:` of the plot, when multidimensional (e.g. a nested
    /// `.dc` sweep). Product must equal `No. Points`.
    pub dimensions: Option<Vec<usize>>,
    pub variables: Vec<Variable>,
    pub data: Data,
}

impl Plot {
    pub fn num_points(&self) -> usize {
        self.data.num_points()
    }

    pub fn is_complex(&self) -> bool {
        matches!(self.data, Data::Complex(_))
    }

    /// The scale vector: the first variable, by 3f5 convention (the
    /// writer moves the scale to the front before writing).
    pub fn scale(&self) -> Option<&Variable> {
        self.variables.first()
    }

    /// Index of a variable by name, case-insensitive (vector names in
    /// the 3f5 front end compare with `cieq`).
    pub fn variable_index(&self, name: &str) -> Option<usize> {
        self.variables
            .iter()
            .position(|v| v.name.eq_ignore_ascii_case(name))
    }

    /// Real column by variable name, if this is a real plot.
    pub fn real_values(&self, name: &str) -> Option<&[f64]> {
        let i = self.variable_index(name)?;
        match &self.data {
            Data::Real(c) => Some(&c[i]),
            Data::Complex(_) => None,
        }
    }

    /// Complex column by variable name, if this is a complex plot.
    pub fn complex_values(&self, name: &str) -> Option<&[Complex64]> {
        let i = self.variable_index(name)?;
        match &self.data {
            Data::Complex(c) => Some(&c[i]),
            Data::Real(_) => None,
        }
    }
}
