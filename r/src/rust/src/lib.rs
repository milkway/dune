//! savvy layer of the `spice` R package: SEXP in, SEXP out. The
//! logic lives in [`engine`]; nothing here panics on user input, and
//! a bug-panic becomes an R error (`panic = "unwind"`, caught by the
//! savvy wrappers).
//!
//! The functions are internal (not exported from the R namespace):
//! the R side (`R/run.R`, `R/raw.R`) validates arguments, builds the
//! S3 objects and raises errors with `cli::cli_abort()`. Failures of
//! a run come back as data (`ok = FALSE`, `kind`, `message`,
//! diagnostics) rather than as R errors, so the R side can classify
//! them.

pub mod engine;

use std::collections::BTreeMap;
use std::path::Path;

use dune_raw::{Data, Plot, VarType, Variable};
use num_complex::Complex64;
use savvy::{
    ListSexp, OwnedIntegerSexp, OwnedListSexp, OwnedLogicalSexp, OwnedRealSexp, OwnedStringSexp,
    RealSexp, Sexp, StringSexp, TypedSexp, savvy,
};

use engine::{Diag, OptValue, Source, SweepRunOutput};

fn strings<I, S>(it: I) -> savvy::Result<Sexp>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    // Not `try_from_iter`: in savvy 0.11 it reads element 0 of an
    // empty iterator (`new_len = last_index + 1`), an R error.
    let v: Vec<S> = it.into_iter().collect();
    if v.is_empty() {
        return Ok(OwnedStringSexp::new(0)?.into());
    }
    Ok(OwnedStringSexp::try_from_slice(&v)?.into())
}

fn string(s: &str) -> savvy::Result<Sexp> {
    Ok(OwnedStringSexp::try_from_scalar(s)?.into())
}

fn logical(b: bool) -> savvy::Result<Sexp> {
    Ok(OwnedLogicalSexp::try_from_scalar(b)?.into())
}

fn diag_list(diags: &[Diag]) -> savvy::Result<Sexp> {
    let mut out = OwnedListSexp::new(2, true)?;
    out.set_name_and_value(0, "severity", strings(diags.iter().map(|d| d.severity))?)?;
    out.set_name_and_value(
        1,
        "message",
        strings(diags.iter().map(|d| d.message.as_str()))?,
    )?;
    Ok(out.into())
}

fn real_columns<'a>(cols: impl ExactSizeIterator<Item = Vec<f64>> + 'a) -> savvy::Result<Sexp> {
    let mut out = OwnedListSexp::new(cols.len(), false)?;
    for (i, c) in cols.enumerate() {
        out.set_value(i, OwnedRealSexp::try_from_slice(c)?)?;
    }
    Ok(out.into())
}

/// One rawfile plot as a plain R list (see `R/plot.R`, `new_plot()`).
/// `scale` is 1-based, `NA` when the plot has none.
fn plot_to_r(p: &Plot, scale: Option<usize>) -> savvy::Result<Sexp> {
    let mut out = OwnedListSexp::new(13, true)?;
    out.set_name_and_value(0, "title", string(&p.title)?)?;
    out.set_name_and_value(1, "date", string(&p.date)?)?;
    out.set_name_and_value(2, "plotname", string(&p.name)?)?;
    out.set_name_and_value(3, "padded", logical(p.padded)?)?;
    out.set_name_and_value(4, "commands", strings(&p.commands)?)?;
    out.set_name_and_value(5, "options", strings(&p.options)?)?;
    let dims: Sexp = match &p.dimensions {
        Some(d) => {
            OwnedRealSexp::try_from_slice(d.iter().map(|&x| x as f64).collect::<Vec<_>>())?.into()
        }
        None => OwnedRealSexp::new(0)?.into(),
    };
    out.set_name_and_value(6, "dimensions", dims)?;
    out.set_name_and_value(
        7,
        "var_names",
        strings(p.variables.iter().map(|v| &v.name))?,
    )?;
    out.set_name_and_value(
        8,
        "var_types",
        strings(p.variables.iter().map(|v| v.vtype.name()))?,
    )?;
    let mut sc = OwnedIntegerSexp::new(1)?;
    match scale {
        Some(i) => sc[0] = i as i32 + 1,
        None => sc.set_na(0)?,
    }
    out.set_name_and_value(9, "scale", sc)?;
    out.set_name_and_value(10, "complex", logical(p.is_complex())?)?;
    match &p.data {
        Data::Real(cols) => {
            out.set_name_and_value(11, "re", real_columns(cols.iter().cloned())?)?;
            out.set_name_and_value(12, "im", savvy::NullSexp)?;
        }
        Data::Complex(cols) => {
            out.set_name_and_value(
                11,
                "re",
                real_columns(cols.iter().map(|c| c.iter().map(|z| z.re).collect()))?,
            )?;
            out.set_name_and_value(
                12,
                "im",
                real_columns(cols.iter().map(|c| c.iter().map(|z| z.im).collect()))?,
            )?;
        }
    }
    Ok(out.into())
}

fn plots_to_r<'a>(
    plots: impl ExactSizeIterator<Item = (&'a Plot, Option<usize>)>,
) -> savvy::Result<Sexp> {
    let mut out = OwnedListSexp::new(plots.len(), false)?;
    for (i, (p, s)) in plots.enumerate() {
        out.set_value(i, plot_to_r(p, s)?)?;
    }
    Ok(out.into())
}

fn includes_map(names: &StringSexp, texts: &StringSexp) -> BTreeMap<String, String> {
    names
        .iter()
        .zip(texts.iter())
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn user_options(names: &StringSexp, values: ListSexp) -> savvy::Result<Vec<(String, OptValue)>> {
    let mut user = Vec::with_capacity(names.len());
    for (name, value) in names.iter().zip(values.values_iter()) {
        let v = match value.into_typed() {
            TypedSexp::Real(x) if x.len() == 1 => OptValue::Num(x.as_slice()[0]),
            TypedSexp::Logical(x) if x.len() == 1 => OptValue::Bool(x.to_vec()[0]),
            TypedSexp::String(x) if x.len() == 1 => {
                OptValue::Str(x.iter().next().unwrap_or_default().to_string())
            }
            _ => return Err(savvy::Error::new(format!("bad value for option `{name}`"))),
        };
        user.push((name.to_string(), v));
    }
    Ok(user)
}

/// Run a deck. `deck` is a path when `is_path`, netlist text
/// otherwise; includes are parallel name/text vectors (text decks
/// only); options are names plus a list of length-1 values.
/// @noRd
#[savvy]
fn ffi_run_deck(
    deck: &str,
    is_path: bool,
    include_names: StringSexp,
    include_texts: StringSexp,
    opt_names: StringSexp,
    opt_values: ListSexp,
) -> savvy::Result<Sexp> {
    let includes = includes_map(&include_names, &include_texts);
    let user = user_options(&opt_names, opt_values)?;
    let source = if is_path {
        Source::Path(Path::new(deck))
    } else {
        Source::Text {
            text: deck,
            includes: &includes,
        }
    };

    let result = engine::run(source, &user);
    let mut out = OwnedListSexp::new(6, true)?;
    match result {
        Ok(r) => {
            out.set_name_and_value(0, "ok", logical(true)?)?;
            out.set_name_and_value(1, "title", string(&r.title)?)?;
            out.set_name_and_value(
                2,
                "plots",
                plots_to_r(r.plots.iter().map(|(p, s)| (p, *s)))?,
            )?;
            out.set_name_and_value(3, "notes", strings(&r.notes)?)?;
            out.set_name_and_value(4, "diagnostics", diag_list(&r.diagnostics)?)?;
            out.set_name_and_value(5, "kind", savvy::NullSexp)?;
        }
        Err(f) => {
            out.set_name_and_value(0, "ok", logical(false)?)?;
            out.set_name_and_value(1, "message", string(&f.message)?)?;
            out.set_name_and_value(2, "plots", savvy::NullSexp)?;
            out.set_name_and_value(3, "notes", OwnedStringSexp::new(0)?)?;
            out.set_name_and_value(4, "diagnostics", diag_list(&f.diagnostics)?)?;
            out.set_name_and_value(5, "kind", string(f.kind)?)?;
        }
    }
    Ok(out.into())
}

/// Sweep a deck (issue #10): the deck, includes and options as in
/// `ffi_run_deck`; `params` the swept names, `values` the runs as a
/// column-major matrix (one row per run, one column per parameter),
/// `threads` 1 for the sequential path, 0 for one per core. Returns
/// `list(ok, title, params, runs, diagnostics, kind, message)`; each
/// run is `list(ok, plots, notes)` or `list(ok = FALSE, kind,
/// message)`.
/// @noRd
#[savvy]
#[allow(clippy::too_many_arguments)]
fn ffi_sweep_deck(
    deck: &str,
    is_path: bool,
    include_names: StringSexp,
    include_texts: StringSexp,
    opt_names: StringSexp,
    opt_values: ListSexp,
    params: StringSexp,
    values: RealSexp,
    threads: i32,
) -> savvy::Result<Sexp> {
    let includes = includes_map(&include_names, &include_texts);
    let user = user_options(&opt_names, opt_values)?;
    let names: Vec<String> = params.iter().map(str::to_string).collect();
    let k = names.len();
    let flat = values.as_slice();
    if k == 0 || !flat.len().is_multiple_of(k) || threads < 0 {
        return Err(savvy::Error::new("bad sweep shape"));
    }
    let n = flat.len() / k;
    let rows: Vec<Vec<f64>> = (0..n)
        .map(|r| (0..k).map(|c| flat[c * n + r]).collect())
        .collect();
    let source = if is_path {
        Source::Path(Path::new(deck))
    } else {
        Source::Text {
            text: deck,
            includes: &includes,
        }
    };

    let result = engine::sweep(source, &user, &names, &rows, threads as usize);
    let mut out = OwnedListSexp::new(7, true)?;
    match result {
        Ok(r) => {
            out.set_name_and_value(0, "ok", logical(true)?)?;
            out.set_name_and_value(1, "title", string(&r.title)?)?;
            out.set_name_and_value(2, "params", strings(&r.params)?)?;
            let mut runs = OwnedListSexp::new(r.runs.len(), false)?;
            for (i, run) in r.runs.iter().enumerate() {
                let mut item = OwnedListSexp::new(3, true)?;
                match run {
                    SweepRunOutput::Ok { plots, notes } => {
                        item.set_name_and_value(0, "ok", logical(true)?)?;
                        item.set_name_and_value(
                            1,
                            "plots",
                            plots_to_r(plots.iter().map(|(p, s)| (p, *s)))?,
                        )?;
                        item.set_name_and_value(2, "notes", strings(notes)?)?;
                    }
                    SweepRunOutput::Failed { kind, message } => {
                        item.set_name_and_value(0, "ok", logical(false)?)?;
                        item.set_name_and_value(1, "kind", string(kind)?)?;
                        item.set_name_and_value(2, "message", string(message)?)?;
                    }
                }
                runs.set_value(i, item)?;
            }
            out.set_name_and_value(3, "runs", runs)?;
            out.set_name_and_value(4, "diagnostics", diag_list(&r.diagnostics)?)?;
            out.set_name_and_value(5, "kind", savvy::NullSexp)?;
            out.set_name_and_value(6, "message", savvy::NullSexp)?;
        }
        Err(f) => {
            out.set_name_and_value(0, "ok", logical(false)?)?;
            out.set_name_and_value(1, "title", savvy::NullSexp)?;
            out.set_name_and_value(2, "params", savvy::NullSexp)?;
            out.set_name_and_value(3, "runs", savvy::NullSexp)?;
            out.set_name_and_value(4, "diagnostics", diag_list(&f.diagnostics)?)?;
            out.set_name_and_value(5, "kind", string(f.kind)?)?;
            out.set_name_and_value(6, "message", string(&f.message)?)?;
        }
    }
    Ok(out.into())
}

/// The option names `ffi_run_deck` accepts.
/// @noRd
#[savvy]
fn ffi_option_names() -> savvy::Result<Sexp> {
    strings(engine::OPTION_NAMES)
}

/// Read a rawfile: `list(plots, warnings)`. The 3f5 reader takes the
/// first vector of each plot as its scale; so do we.
/// @noRd
#[savvy]
fn ffi_read_raw(path: &str) -> savvy::Result<Sexp> {
    let raw = engine::read_raw(Path::new(path)).map_err(savvy::Error::new)?;
    let mut out = OwnedListSexp::new(2, true)?;
    out.set_name_and_value(
        0,
        "plots",
        plots_to_r(
            raw.plots
                .iter()
                .map(|p| (p, (!p.variables.is_empty()).then_some(0))),
        )?,
    )?;
    out.set_name_and_value(1, "warnings", strings(&raw.warnings)?)?;
    Ok(out.into())
}

fn field(l: &ListSexp, k: &str) -> savvy::Result<Sexp> {
    l.get(k)
        .ok_or_else(|| savvy::Error::new(format!("plot is missing `{k}`")))
}

fn field_strings(l: &ListSexp, k: &str) -> savvy::Result<Vec<String>> {
    let s = StringSexp::try_from(field(l, k)?)?;
    Ok(s.iter().map(str::to_string).collect())
}

fn field_string(l: &ListSexp, k: &str) -> savvy::Result<String> {
    Ok(field_strings(l, k)?.into_iter().next().unwrap_or_default())
}

fn field_columns(l: &ListSexp, k: &str) -> savvy::Result<Vec<Vec<f64>>> {
    let cols = ListSexp::try_from(field(l, k)?)?;
    cols.values_iter()
        .map(|c| Ok(RealSexp::try_from(c)?.to_vec()))
        .collect()
}

fn plot_from_r(l: &ListSexp) -> savvy::Result<Plot> {
    let names = field_strings(l, "var_names")?;
    let types = field_strings(l, "var_types")?;
    if names.len() != types.len() {
        return Err(savvy::Error::new(
            "var_names and var_types differ in length",
        ));
    }
    let re = field_columns(l, "re")?;
    let im_sexp = field(l, "im")?;
    let data = if im_sexp.is_null() {
        Data::Real(re)
    } else {
        let im = field_columns(l, "im")?;
        if im.len() != re.len() || im.iter().zip(&re).any(|(a, b)| a.len() != b.len()) {
            return Err(savvy::Error::new("re and im columns differ in shape"));
        }
        Data::Complex(
            re.iter()
                .zip(&im)
                .map(|(r, i)| {
                    r.iter()
                        .zip(i)
                        .map(|(&a, &b)| Complex64::new(a, b))
                        .collect()
                })
                .collect(),
        )
    };
    let dims = RealSexp::try_from(field(l, "dimensions")?)?;
    let dimensions = if dims.is_empty() {
        None
    } else {
        Some(
            dims.iter()
                .map(|&d| {
                    if d.is_finite() && d >= 0.0 && d.fract() == 0.0 {
                        Ok(d as usize)
                    } else {
                        Err(savvy::Error::new("dimensions must be whole numbers"))
                    }
                })
                .collect::<savvy::Result<Vec<_>>>()?,
        )
    };
    let padded = savvy::LogicalSexp::try_from(field(l, "padded")?)?
        .to_vec()
        .first()
        .copied()
        .unwrap_or(true);
    Ok(Plot {
        title: field_string(l, "title")?,
        date: field_string(l, "date")?,
        name: field_string(l, "plotname")?,
        padded,
        commands: field_strings(l, "commands")?,
        options: field_strings(l, "options")?,
        dimensions,
        variables: names
            .into_iter()
            .zip(types)
            .map(|(n, t)| Variable::new(n, VarType::from_name(&t)))
            .collect(),
        data,
    })
}

/// Write plots (each a list as `plot_to_r` makes) to a rawfile.
/// @noRd
#[savvy]
fn ffi_write_raw(path: &str, plots: ListSexp, binary: bool) -> savvy::Result<()> {
    let plots = plots
        .values_iter()
        .map(|p| plot_from_r(&ListSexp::try_from(p)?))
        .collect::<savvy::Result<Vec<_>>>()?;
    engine::write_raw(Path::new(path), plots, binary).map_err(savvy::Error::new)
}
