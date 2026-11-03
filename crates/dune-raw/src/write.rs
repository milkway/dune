use std::io::{self, Write};

use crate::plot::{Data, Plot};

/// Data section encoding. Binary is raw little-endian `f64`s — the 3f5
/// writes native doubles, and every platform Dune targets (x86-64,
/// aarch64, wasm32) is little-endian.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Ascii,
    Binary,
}

/// `%.*e` with the given precision, as C's `printf` renders it
/// (`1.000000000000000e+03`): sign on the exponent, at least two
/// exponent digits. Rust's `{:e}` writes `1e3`, so this reshapes it.
fn fmt_e(x: f64, prec: usize) -> String {
    let s = format!("{x:.prec$e}");
    match s.rfind('e') {
        Some(pos) => {
            let (mant, exp) = s.split_at(pos);
            let exp: i32 = exp[1..].parse().expect("exponent is an integer");
            format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
        }
        None => s, // inf / NaN; never produced by a simulation we write
    }
}

/// The default ASCII precision: `DEFPREC` (15 significant decimals past
/// the point) in `rawfile.c`.
const DEFPREC: usize = 15;

impl Plot {
    /// Write this plot exactly as the 3f5 `raw_write` does.
    ///
    /// The caller is responsible for the scale being `variables[0]`
    /// (the C moves it to the front; here it is an invariant of the
    /// in-memory plot).
    pub fn write(&self, w: &mut impl Write, format: Format) -> io::Result<()> {
        let npoints = self.num_points();
        let complex = self.is_complex();

        writeln!(w, "Title: {}", self.title)?;
        writeln!(w, "Date: {}", self.date)?;
        writeln!(w, "Plotname: {}", self.name)?;
        writeln!(
            w,
            "Flags: {}{}",
            if complex { "complex" } else { "real" },
            if self.padded { "" } else { " unpadded" }
        )?;
        writeln!(w, "No. Variables: {}", self.variables.len())?;
        writeln!(w, "No. Points: {npoints}")?;
        if let Some(dims) = &self.dimensions
            && dims.len() > 1
        {
            writeln!(w, "Dimensions: {}", dimstring(dims))?;
        }
        for c in &self.commands {
            writeln!(w, "Command: {c}")?;
        }
        for o in &self.options {
            writeln!(w, "Option: {o}")?;
        }

        writeln!(w, "Variables:")?;
        for (i, v) in self.variables.iter().enumerate() {
            write!(w, "\t{i}\t{}\t{}", v.name, v.vtype.name())?;
            if let Some(min) = v.min {
                write!(w, " min={}", fmt_e(min, 6))?;
            }
            if let Some(max) = v.max {
                write!(w, " max={}", fmt_e(max, 6))?;
            }
            if let Some(color) = &v.color {
                write!(w, " color={color}")?;
            }
            if let Some(grid) = v.grid {
                write!(w, " grid={grid}")?;
            }
            // Faithful to the C, which prints the *grid* type under
            // `plot=` (`rawfile.c:139`); kept until a corpus circuit
            // proves it matters, then it becomes a KNOWN_DIFFS entry.
            if let Some(_plottype) = v.plottype {
                write!(w, " plot={}", v.grid.unwrap_or(0))?;
            }
            if let Some(dims) = &v.dims {
                write!(w, " dims={}", dimstring(dims))?;
            }
            writeln!(w)?;
        }

        match format {
            Format::Binary => {
                writeln!(w, "Binary:")?;
                for i in 0..npoints {
                    self.write_point(w, i, |w, re, im| {
                        w.write_all(&re.to_le_bytes())?;
                        if complex {
                            w.write_all(&im.to_le_bytes())?;
                        }
                        Ok(())
                    })?;
                }
            }
            Format::Ascii => {
                writeln!(w, "Values:")?;
                for i in 0..npoints {
                    write!(w, " {i}")?;
                    // The C prints a literal `,0.0` imaginary part for a
                    // real vector inside a complex plot; that distinction
                    // does not survive a read (every vector of a complex
                    // plot is complex afterwards), so a full-precision
                    // zero is written instead, as ngspice itself does.
                    self.write_point(w, i, |w, re, im| {
                        if complex {
                            writeln!(w, "\t{},{}", fmt_e(re, DEFPREC), fmt_e(im, DEFPREC))
                        } else {
                            writeln!(w, "\t{}", fmt_e(re, DEFPREC))
                        }
                    })?;
                    writeln!(w)?;
                }
            }
        }
        Ok(())
    }

    /// Emit point `i` of every vector through `emit(re, im)`, honoring
    /// per-vector lengths and zero padding, like the inner loop of
    /// `raw_write`.
    fn write_point(
        &self,
        w: &mut impl Write,
        i: usize,
        mut emit: impl FnMut(&mut dyn Write, f64, f64) -> io::Result<()>,
    ) -> io::Result<()> {
        match &self.data {
            Data::Real(cols) => {
                for col in cols {
                    if i < col.len() {
                        emit(w, col[i], 0.0)?;
                    } else if self.padded {
                        emit(w, 0.0, 0.0)?;
                    }
                }
            }
            Data::Complex(cols) => {
                for col in cols {
                    if i < col.len() {
                        emit(w, col[i].re, col[i].im)?;
                    } else if self.padded {
                        emit(w, 0.0, 0.0)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// `d1,d2,…` (the C `dimstring`).
fn dimstring(dims: &[usize]) -> String {
    dims.iter()
        .map(|d| d.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{VarType, Variable};

    #[test]
    fn c_style_exponent() {
        assert_eq!(fmt_e(1000.0, 15), "1.000000000000000e+03");
        assert_eq!(fmt_e(-2.5e-7, 15), "-2.500000000000000e-07");
        assert_eq!(fmt_e(0.0, 15), "0.000000000000000e+00");
        assert_eq!(fmt_e(1.0, 6), "1.000000e+00");
        assert_eq!(fmt_e(3.5e-112, 15), "3.500000000000000e-112");
    }

    #[test]
    fn ascii_matches_3f5_shape() {
        let plot = Plot {
            title: "t".into(),
            date: "d".into(),
            name: "Operating Point".into(),
            padded: true,
            commands: vec![],
            options: vec![],
            dimensions: None,
            variables: vec![
                Variable::new("v(1)", VarType::Voltage),
                Variable::new("i(v1)", VarType::Current),
            ],
            data: Data::Real(vec![vec![1.0], vec![-0.001]]),
        };
        let mut buf = Vec::new();
        plot.write(&mut buf, Format::Ascii).unwrap();
        let expected = "Title: t\n\
             Date: d\n\
             Plotname: Operating Point\n\
             Flags: real\n\
             No. Variables: 2\n\
             No. Points: 1\n\
             Variables:\n\
             \t0\tv(1)\tvoltage\n\
             \t1\ti(v1)\tcurrent\n\
             Values:\n \
             0\t1.000000000000000e+00\n\
             \t-1.000000000000000e-03\n\n";
        assert_eq!(String::from_utf8(buf).unwrap(), expected);
    }
}
