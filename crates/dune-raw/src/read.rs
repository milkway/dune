//! Rawfile reader, mirroring `raw_read` in
//! `corpus/spice3f5/src/lib/fte/rawfile.c`: line-oriented header with
//! case-insensitive keywords, then a whitespace-insensitive ASCII value
//! stream or a raw little-endian `f64` stream. "Really all we require is
//! that there be one variables and one values section per plot and that
//! the variables precede the values."

use num_complex::Complex64;

use crate::plot::{Data, Plot, Variable};
use crate::{Error, Rawfile, VarType};

pub(crate) fn parse(bytes: &[u8]) -> Result<Rawfile, Error> {
    Reader::new(bytes).run()
}

/// Header fields accumulated before the data section closes a plot.
struct Pending {
    name: String,
    nvars: usize,
    npoints: usize,
    complex: bool,
    padded: bool,
    commands: Vec<String>,
    options: Vec<String>,
    dimensions: Option<Vec<usize>>,
    variables: Vec<Variable>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    line: usize,
    title: String,
    date: String,
    plots: Vec<Plot>,
    warnings: Vec<String>,
    cur: Option<Pending>,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            line: 0,
            // The C defaults when a plot appears before a Title/Date.
            title: "default title".into(),
            date: String::new(),
            plots: Vec::new(),
            warnings: Vec::new(),
            cur: None,
        }
    }

    fn run(mut self) -> Result<Rawfile, Error> {
        while let Some(line) = self.next_line() {
            let line = String::from_utf8_lossy(line).into_owned();
            let Some((keyword, rest)) = split_keyword(&line) else {
                if !line.trim().is_empty() {
                    return Err(Error::parse(
                        self.line,
                        format!("strange line in rawfile: {:?}", line.trim()),
                    ));
                }
                continue;
            };
            match keyword.to_ascii_lowercase().as_str() {
                "title" => self.title = rest.trim().to_owned(),
                "date" => self.date = rest.trim().to_owned(),
                "plotname" => self.start_plot(rest.trim().to_owned()),
                "flags" => self.parse_flags(rest),
                "no. variables" => {
                    self.with_plot(|p, n| p.nvars = n, parse_usize(rest), "No. Variables")?
                }
                "no. points" => {
                    self.with_plot(|p, n| p.npoints = n, parse_usize(rest), "No. Points")?
                }
                "dimensions" => self.parse_dimensions(rest),
                "command" => match &mut self.cur {
                    Some(p) => p.commands.push(rest.trim().to_owned()),
                    None => self.warn("misplaced Command: line"),
                },
                "option" => match &mut self.cur {
                    Some(p) => p.options.push(rest.trim().to_owned()),
                    None => self.warn("misplaced Option: line"),
                },
                "variables" => self.parse_variables(rest)?,
                "values" => self.parse_values()?,
                "binary" => self.parse_binary()?,
                _ => {
                    return Err(Error::parse(
                        self.line,
                        format!("strange line in rawfile: {:?}", line.trim()),
                    ));
                }
            }
        }
        if self.cur.is_some() {
            return Err(Error::parse(
                self.line,
                "plot header without a data section",
            ));
        }
        Ok(Rawfile {
            plots: self.plots,
            warnings: self.warnings,
        })
    }

    fn start_plot(&mut self, name: String) {
        if self.cur.is_some() {
            self.warn("new Plotname: before previous plot's data; dropping it");
        }
        self.cur = Some(Pending {
            name,
            nvars: 0,
            npoints: 0,
            complex: false,
            padded: true,
            commands: Vec::new(),
            options: Vec::new(),
            dimensions: None,
            variables: Vec::new(),
        });
    }

    fn with_plot<T>(
        &mut self,
        set: impl FnOnce(&mut Pending, T),
        value: Option<T>,
        what: &str,
    ) -> Result<(), Error> {
        let Some(v) = value else {
            return Err(Error::parse(self.line, format!("bad {what} line")));
        };
        match &mut self.cur {
            Some(p) => set(p, v),
            None => self.warn(format!("misplaced {what}: line")),
        }
        Ok(())
    }

    fn parse_flags(&mut self, rest: &str) {
        let mut complex = false;
        let mut padded = true;
        for tok in rest.split_ascii_whitespace() {
            if tok.eq_ignore_ascii_case("real") {
                complex = false;
            } else if tok.eq_ignore_ascii_case("complex") {
                complex = true;
            } else if tok.eq_ignore_ascii_case("unpadded") {
                padded = false;
            } else if tok.eq_ignore_ascii_case("padded") {
                padded = true;
            } else {
                self.warn(format!("unknown flag {tok}"));
            }
        }
        if let Some(p) = &mut self.cur {
            p.complex = complex;
            p.padded = padded;
        }
    }

    fn parse_dimensions(&mut self, rest: &str) {
        let Some(p) = &mut self.cur else {
            self.warn("misplaced Dimensions: line");
            return;
        };
        if p.npoints == 0 {
            self.warnings.push("misplaced Dimensions: line".into());
            return;
        }
        match parse_dims(rest.trim()).and_then(|d| checked_product(&d).map(|n| (d, n))) {
            Some((dims, n)) if n == p.npoints => {
                p.dimensions = Some(dims);
            }
            Some(_) => self
                .warnings
                .push("dimensions inconsistent with no. of points, ignored".into()),
            None => self
                .warnings
                .push("syntax error in dimensions, ignored".into()),
        }
    }

    fn parse_variables(&mut self, rest: &str) -> Result<(), Error> {
        let line = self.line;
        let (nvars, npoints) = match &self.cur {
            Some(p) => (p.nvars, p.npoints),
            None => return Err(Error::parse(line, "no plot name given")),
        };
        for i in 0..nvars {
            // The first variable may sit on the `Variables:` line itself.
            let text: String = if i == 0 && !rest.trim().is_empty() {
                rest.to_owned()
            } else {
                match self.next_line() {
                    Some(l) => String::from_utf8_lossy(l).into_owned(),
                    None => return Err(Error::parse(self.line, "unexpected EOF in Variables:")),
                }
            };
            let var = self.parse_variable_line(&text, npoints)?;
            self.cur
                .as_mut()
                .expect("checked above")
                .variables
                .push(var);
        }
        Ok(())
    }

    fn parse_variable_line(&mut self, text: &str, npoints: usize) -> Result<Variable, Error> {
        let mut toks = text.split_ascii_whitespace();
        let _index = toks
            .next()
            .ok_or_else(|| Error::parse(self.line, format!("bad var line {text:?}")))?;
        let name = toks
            .next()
            .ok_or_else(|| Error::parse(self.line, format!("bad var line {text:?}")))?;
        let vtype = VarType::from_name(
            toks.next()
                .ok_or_else(|| Error::parse(self.line, format!("bad var line {text:?}")))?,
        );
        // Fix names that start with a digit: `2` (voltage) -> `V(2)`.
        let name = match (
            name.starts_with(|c: char| c.is_ascii_digit()),
            vtype.abbrev(),
        ) {
            (true, Some(abbrev)) => format!("{abbrev}({name})"),
            _ => name.to_owned(),
        };
        let mut var = Variable::new(name, vtype);
        for tok in toks {
            if let Some(v) = tok.strip_prefix("min=") {
                match v.parse() {
                    Ok(x) => var.min = Some(x),
                    Err(_) => self.warn(format!("bad arg {tok}")),
                }
            } else if let Some(v) = tok.strip_prefix("max=") {
                match v.parse() {
                    Ok(x) => var.max = Some(x),
                    Err(_) => self.warn(format!("bad arg {tok}")),
                }
            } else if let Some(v) = tok.strip_prefix("color=") {
                var.color = Some(v.to_owned());
            } else if let Some(v) = tok.strip_prefix("scale=") {
                var.scale = Some(v.to_owned());
            } else if let Some(v) = tok.strip_prefix("grid=") {
                var.grid = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("plot=") {
                var.plottype = v.parse().ok();
            } else if let Some(v) = tok.strip_prefix("dims=") {
                // `fixdims`: invalid, overflowing or oversized dims fall
                // back to the plot default.
                match parse_dims(v).and_then(|d| checked_product(&d).map(|n| (d, n))) {
                    Some((dims, n)) if n <= npoints => var.dims = Some(dims),
                    _ => self.warn("syntax error in dimensions, ignored"),
                }
            } else {
                self.warn(format!("bad var param {tok}"));
            }
        }
        Ok(var)
    }

    /// Close the current plot after its data was read.
    fn finish_plot(&mut self, data: Data) {
        let p = self.cur.take().expect("caller checked");
        // Resolve scale= references like the C does, demoting dangling
        // ones to a warning.
        for v in &p.variables {
            if let Some(s) = &v.scale
                && !p.variables.iter().any(|o| o.name.eq_ignore_ascii_case(s))
            {
                self.warnings.push(format!("no such vector {s}"));
            }
        }
        self.plots.push(Plot {
            title: self.title.clone(),
            date: self.date.clone(),
            name: p.name,
            padded: p.padded,
            commands: p.commands,
            options: p.options,
            dimensions: p.dimensions,
            variables: p.variables,
            data,
        });
    }

    fn data_shape(&self) -> Result<(usize, Vec<usize>, bool, bool), Error> {
        let Some(p) = &self.cur else {
            return Err(Error::parse(self.line, "no plot name given"));
        };
        if p.variables.len() != p.nvars {
            return Err(Error::parse(
                self.line,
                format!(
                    "data section with {} of {} declared variables",
                    p.variables.len(),
                    p.nvars
                ),
            ));
        }
        let lengths = p.variables.iter().map(|v| v.length(p.npoints)).collect();
        Ok((p.npoints, lengths, p.complex, p.padded))
    }

    fn parse_values(&mut self) -> Result<(), Error> {
        let (npoints, lengths, complex, padded) = self.data_shape()?;
        let mut cols = self.new_columns(&lengths, complex);
        for i in 0..npoints {
            // Point index: read and discarded, like the C `fscanf(" %d")`.
            self.ascii_token_f64("point index")?;
            for (col, &len) in cols.iter_mut().zip(&lengths) {
                if i < len {
                    col.push(self.ascii_value(complex)?);
                } else if padded {
                    self.ascii_value(complex)?; // consume the padding
                }
            }
        }
        self.finish_plot(pack(cols, complex));
        Ok(())
    }

    fn parse_binary(&mut self) -> Result<(), Error> {
        let (npoints, lengths, complex, padded) = self.data_shape()?;
        // Every double promised by the header must fit in the remaining
        // input; checking up front rejects a hostile `No. Points` before
        // the read loop (which would otherwise spin on point counts that
        // consume no bytes, e.g. zero-length unpadded vectors).
        let width = if complex { 16 } else { 8 };
        let total: Option<usize> = lengths
            .iter()
            .map(|&len| if padded { npoints } else { len })
            .try_fold(0usize, |acc, n| acc.checked_add(n.checked_mul(width)?));
        match total {
            Some(t) if t <= self.data.len() - self.pos => {}
            _ => {
                return Err(Error::parse(
                    self.line,
                    "bad rawfile: truncated binary data",
                ));
            }
        }
        // A section that promises bytes for no vector at all (unpadded
        // zero-length vectors, or none declared) has nothing to read:
        // skip the loop instead of spinning `npoints` times over it.
        let npoints = if total == Some(0) { 0 } else { npoints };
        let mut cols = self.new_columns(&lengths, complex);
        for i in 0..npoints {
            for (col, &len) in cols.iter_mut().zip(&lengths) {
                if i < len {
                    let re = self.binary_f64()?;
                    let im = if complex { self.binary_f64()? } else { 0.0 };
                    col.push(Complex64::new(re, im));
                } else if padded {
                    self.binary_f64()?;
                    if complex {
                        self.binary_f64()?;
                    }
                }
            }
        }
        self.finish_plot(pack(cols, complex));
        Ok(())
    }

    /// Pre-allocate data columns. The reserve is bounded by what the
    /// remaining input could possibly hold, so hostile `No. Points` /
    /// `No. Variables` cannot force a large allocation up front: real
    /// data still grows the vectors as it arrives.
    fn new_columns(&self, lengths: &[usize], complex: bool) -> Vec<Vec<Complex64>> {
        let remaining = self.data.len() - self.pos;
        // At least 8 bytes per value in binary; ASCII values are wider.
        let possible = remaining / (if complex { 16 } else { 8 }) / lengths.len().max(1);
        lengths
            .iter()
            .map(|&n| Vec::with_capacity(n.min(possible)))
            .collect()
    }

    // ---- low-level cursor ----------------------------------------------

    /// Next line, without its terminator (`\n`, tolerating `\r\n`).
    fn next_line(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.data.len() {
            return None;
        }
        self.line += 1;
        let start = self.pos;
        let end = self.data[start..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(self.data.len(), |i| start + i);
        self.pos = (end + 1).min(self.data.len());
        let mut line = &self.data[start..end];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        Some(line)
    }

    fn skip_ascii_whitespace(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if b == b'\n' {
                self.line += 1;
            }
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// One whitespace-delimited float (also stopping at `,`), like the
    /// C `fscanf(" %lf")`.
    fn ascii_token_f64(&mut self, what: &str) -> Result<f64, Error> {
        self.skip_ascii_whitespace();
        let start = self.pos;
        while let Some(&b) = self.data.get(self.pos) {
            if b.is_ascii_whitespace() || b == b',' {
                break;
            }
            self.pos += 1;
        }
        let tok = std::str::from_utf8(&self.data[start..self.pos])
            .map_err(|_| Error::parse(self.line, format!("bad rawfile: non-ASCII {what}")))?;
        tok.parse()
            .map_err(|_| Error::parse(self.line, format!("bad rawfile: bad {what} {tok:?}")))
    }

    /// One value: a float, or `re,im` when the plot is complex.
    fn ascii_value(&mut self, complex: bool) -> Result<Complex64, Error> {
        let re = self.ascii_token_f64("value")?;
        if !complex {
            return Ok(Complex64::new(re, 0.0));
        }
        self.skip_ascii_whitespace();
        if self.data.get(self.pos) != Some(&b',') {
            return Err(Error::parse(
                self.line,
                "bad rawfile: expected `,` in complex value",
            ));
        }
        self.pos += 1;
        let im = self.ascii_token_f64("imaginary part")?;
        Ok(Complex64::new(re, im))
    }

    fn binary_f64(&mut self) -> Result<f64, Error> {
        let end = self.pos + 8;
        let bytes = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| Error::parse(self.line, "bad rawfile: truncated binary data"))?;
        self.pos = end;
        Ok(f64::from_le_bytes(bytes.try_into().expect("8 bytes")))
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}

/// `dims` product with overflow detection (a hostile `dims=` must not
/// wrap in release builds nor panic in debug builds).
fn checked_product(dims: &[usize]) -> Option<usize> {
    dims.iter().try_fold(1usize, |acc, &d| acc.checked_mul(d))
}

fn pack(cols: Vec<Vec<Complex64>>, complex: bool) -> Data {
    if complex {
        Data::Complex(cols)
    } else {
        Data::Real(
            cols.into_iter()
                .map(|c| c.into_iter().map(|z| z.re).collect())
                .collect(),
        )
    }
}

/// Split `Keyword: rest`; keywords may contain spaces and dots
/// (`No. Variables`), so split at the first `:`.
fn split_keyword(line: &str) -> Option<(&str, &str)> {
    let (k, rest) = line.split_once(':')?;
    let k = k.trim();
    // A keyword is short and alphabetic-ish; this keeps arbitrary junk
    // containing `:` flowing to the "strange line" error.
    if k.is_empty()
        || k.len() > 16
        || !k
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '.')
    {
        return None;
    }
    Some((k, rest))
}

/// `d1,d2,d3…` (the C `atodims`).
fn parse_dims(s: &str) -> Option<Vec<usize>> {
    if s.is_empty() {
        return None;
    }
    s.split(',').map(|d| d.trim().parse().ok()).collect()
}

/// Last integer on a header line, tolerating ngspice's padded
/// `No. Points: 13      ` (trailing blanks it back-patches in place).
fn parse_usize(rest: &str) -> Option<usize> {
    rest.trim().parse().ok()
}
