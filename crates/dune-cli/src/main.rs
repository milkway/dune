//! The `dune` batch-mode simulator, flag-compatible with `spice3 -b`:
//! `dune -b input.cir [-r output.raw]`. With `-r` the results go to
//! a binary SPICE3 rawfile and `.print` cards are skipped, like the
//! 3f5; without it, `.print` cards render in the spice3 column
//! format (`print.rs`). Every analysis card runs through
//! `Circuit::run`, the dispatch the oracle and the wasm build share.
//! `.measure` cards print their results in ngspice's format
//! (`measure.rs`) after the analyses, in both modes — unlike
//! `ngspice -b -r`, which refuses measurements next to a rawfile;
//! a measurement never changes what is simulated or written.
//! `.four` cards print ngspice's Fourier tables (`four.rs`) in deck
//! order among the `.print` output, and — like `.measure`, unlike
//! `ngspice -b -r` ("ignored since rawfile was produced") — also in
//! rawfile mode, after the measurements.
//! `dune pic FIGURE.pic` extracts the netlist of an rpic figure
//! (`dune-pic`); `dune pic --run` simulates it the same way, and
//! `--svg OUT.svg` also writes the drawing. With the default
//! `texlabels` feature the binary registers rpic's RaTeX math
//! renderer, so a `$...$` label (in a figure that sets
//! `texlabels = 1`) is typeset; without it the label stays literal,
//! as in the wasm build.

#![forbid(unsafe_code)]

mod four;
mod measure;
mod print;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use dune_core::{Circuit, Options, OutExpr, Solution};
use dune_netlist::Severity;
use dune_netlist::ir::{Output, OutputKind};
use dune_raw::{Format, Rawfile};

const USAGE: &str = "usage: dune -b INPUT.cir [-r OUTPUT.raw]\n       \
                     dune pic [--run] FIGURE.pic [-r OUTPUT.raw] [--svg OUTPUT.svg]";

fn main() -> Result<()> {
    // rpic's math hook is process-wide and first-call-wins; nothing
    // else in this process registers one.
    #[cfg(feature = "texlabels")]
    rpic_core::set_math_renderer(rpic_render::math::render_math);
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().map(String::as_str) == Some("pic") {
        args.next();
        return pic_main(args);
    }
    let mut input: Option<PathBuf> = None;
    let mut rawfile: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "-b" => {} // batch is the only mode there is, so far
            "-r" => {
                rawfile = Some(PathBuf::from(args.next().context("-r needs a file name")?));
            }
            flag if flag.starts_with('-') => bail!("unknown flag `{flag}`\n{USAGE}"),
            _ => {
                if input.replace(PathBuf::from(&a)).is_some() {
                    bail!("more than one input file\n{USAGE}");
                }
            }
        }
    }
    let Some(input) = input else {
        bail!("no input file\n{USAGE}");
    };

    let parsed = dune_netlist::parse_file(&input)?;
    run_deck(&parsed, &input, rawfile.as_deref())
}

/// `dune pic FIGURE.pic` prints the netlist extracted from an rpic
/// figure (diagnostics to stderr); `--run` simulates it like `-b`;
/// `--svg FILE` also writes the figure's SVG (`dune_pic::render_svg`),
/// before the extraction, so a figure with errors still draws.
fn pic_main(args: impl Iterator<Item = String>) -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut rawfile: Option<PathBuf> = None;
    let mut svg: Option<PathBuf> = None;
    let mut run = false;
    let mut args = args;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--run" => run = true,
            "-r" => {
                rawfile = Some(PathBuf::from(args.next().context("-r needs a file name")?));
            }
            "--svg" => {
                svg = Some(PathBuf::from(
                    args.next().context("--svg needs a file name")?,
                ));
            }
            flag if flag.starts_with('-') => bail!("unknown flag `{flag}`\n{USAGE}"),
            _ => {
                if input.replace(PathBuf::from(&a)).is_some() {
                    bail!("more than one input file\n{USAGE}");
                }
            }
        }
    }
    let Some(input) = input else {
        bail!("no input figure\n{USAGE}");
    };
    if rawfile.is_some() && !run {
        bail!("-r only makes sense with --run\n{USAGE}");
    }
    let src =
        std::fs::read_to_string(&input).with_context(|| format!("reading {}", input.display()))?;
    let pic_err = |e: dune_pic::PicError| match &e {
        dune_pic::PicError::Compile { span: Some(s), .. } => {
            anyhow::anyhow!("{}:{s}: {e}", input.display())
        }
        _ => anyhow::anyhow!("{}: {e}", input.display()),
    };
    if let Some(svg) = &svg {
        let drawing =
            dune_pic::render_svg(&src, &dune_pic::ExtractOptions::default()).map_err(pic_err)?;
        std::fs::write(svg, drawing).with_context(|| format!("writing {}", svg.display()))?;
    }
    let ex = dune_pic::extract(&src, &dune_pic::ExtractOptions::default()).map_err(pic_err)?;
    for d in &ex.diagnostics {
        match &d.span {
            Some(s) if s.file.is_none() => eprintln!("{}:{d}", input.display()),
            _ => eprintln!("{}: {d}", input.display()),
        }
    }
    if !ex.is_clean() {
        if !run {
            print!("{}", ex.netlist);
        }
        bail!("figure has errors");
    }
    if !run {
        print!("{}", ex.netlist);
        return Ok(());
    }
    // `.include` in a directive resolves next to the figure, as it
    // would next to a deck.
    let deck_name = input.with_extension("cir");
    let parsed =
        dune_netlist::parse_with_loader(&deck_name, ex.netlist, &mut dune_netlist::deck::FsLoader);
    run_deck(&parsed, &input, rawfile.as_deref())
}

/// Simulate a parsed deck: every analysis card through `Circuit::run`,
/// results to a rawfile (`-r`) or as `.print` columns.
fn run_deck(
    parsed: &dune_netlist::ParseOutcome,
    input: &Path,
    rawfile: Option<&Path>,
) -> Result<()> {
    // Diagnostics go to stderr, errors are fatal like the 3f5's deck
    // errors.
    let mut fatal = false;
    for d in &parsed.diags {
        eprintln!("{}", d.render(&parsed.sources));
        fatal |= d.severity == Severity::Error;
    }
    if fatal {
        bail!("deck has errors");
    }

    let (circuit, notes) = Circuit::from_netlist(&parsed.netlist)
        .map_err(|e| anyhow::anyhow!("{}: {e}", input.display()))?;
    for n in &notes {
        eprintln!("note: {n}");
    }
    let mut opt_diags = Vec::new();
    let mut options = Options::default();
    options.apply(&parsed.netlist.options, &mut opt_diags);
    let four_params = dune_core::FourParams::from_options(&parsed.netlist.options, &mut opt_diags);
    for d in &opt_diags {
        eprintln!("{}", d.render(&parsed.sources));
    }

    let mut solutions: Vec<Solution> = Vec::new();
    for analysis in &parsed.netlist.analyses {
        let (sols, notes) = circuit.run(analysis, &options)?;
        for n in &notes {
            eprintln!("note: {n}");
        }
        solutions.extend(sols);
    }
    if solutions.is_empty() {
        eprintln!("note: no analyses in the deck; nothing to do");
        return Ok(());
    }

    // `.measure`: pure post-processing of the plots, printed before
    // any `.print` output, as `ngspice -b` does.
    let measures = &parsed.netlist.measures;
    if !measures.is_empty() {
        let outcomes = dune_core::measure_all(measures, &solutions);
        let (out, err) = measure::render(measures, &solutions, &outcomes);
        eprint!("{err}");
        print!("{out}");
    }

    // `.four`: pure post-processing too, on the last transient plot.
    let fourier = dune_core::four_all(&parsed.netlist.outputs, &solutions, &four_params);

    match rawfile {
        Some(path) => {
            // Rawfile mode: like `spice3 -b -r`, everything goes to
            // the rawfile and `.print` cards are not rendered.
            let (out, err) = four::render_all(&fourier);
            eprint!("{err}");
            print!("{out}");
            let raw = Rawfile {
                plots: solutions
                    .iter()
                    .map(|s| s.to_plot(&parsed.netlist.title, ""))
                    .collect(),
                warnings: Vec::new(),
            };
            let mut f = std::fs::File::create(path)
                .with_context(|| format!("creating {}", path.display()))?;
            // Binary is the 3f5's default rawfile format.
            raw.write(&mut f, Format::Binary)?;
        }
        None => {
            run_prints(&parsed.netlist, &solutions, &options, &fourier)?;
        }
    }
    Ok(())
}

/// Render the deck's `.print` cards in the spice3 column format;
/// fall back to a bare value listing when the deck has none (so an
/// `.op`-only deck still shows its numbers).
fn run_prints(
    nl: &dune_netlist::ir::Netlist,
    solutions: &[Solution],
    options: &Options,
    fourier: &[dune_core::FourOutcome],
) -> Result<()> {
    let page = print::PageSetup {
        width: nl.width_out.unwrap_or(print::DEF_WIDTH),
        height: print::DEF_HEIGHT,
        nobreak: options.nopage,
    };
    let mut printed = false;
    for (card, output) in nl.outputs.iter().enumerate() {
        let Output {
            kind,
            analysis,
            vars,
            ..
        } = output;
        match kind {
            OutputKind::Print => {}
            OutputKind::Plot => {
                eprintln!("note: .plot (ascii graphs) is not implemented yet; use .print");
                continue;
            }
            // `ft_cktcoms` runs `.four` in deck order with `.print`.
            OutputKind::Four { .. } => {
                let mine: Vec<&dune_core::FourOutcome> =
                    fourier.iter().filter(|o| o.card == card).collect();
                let (out, err) = four::render_card(&mine);
                eprint!("{err}");
                print!("{out}");
                printed = true;
                continue;
            }
            // `.save` narrows what gets saved; Dune currently saves
            // everything.
            OutputKind::Save => continue,
        }
        let plotname = match analysis.as_deref() {
            Some("tran") => "Transient Analysis",
            Some("dc") => "DC transfer characteristic",
            Some("ac") => "AC Analysis",
            // The period in the time domain; the harmonics go to the
            // rawfile (`-r`).
            Some("pss") => dune_core::PSS_TIME_PLOT,
            Some(other) => {
                eprintln!("note: .print {other}: analysis not implemented yet");
                continue;
            }
            None => continue,
        };
        let Some(solution) = solutions.iter().find(|s| s.plotname == plotname) else {
            eprintln!(
                "note: .print {}: no such analysis in this run",
                analysis.as_deref().unwrap_or("")
            );
            continue;
        };
        let exprs: Vec<OutExpr> = vars.iter().map(OutExpr::from_ir).collect();
        match solution.select(&exprs) {
            Ok(sel) => {
                let mut out = String::new();
                print::print_columns(&mut out, &nl.title, &solution.plotname, "", &sel, &page);
                print!("{out}");
                printed = true;
            }
            Err(e) => eprintln!("warning: .print: {e}"),
        }
    }
    if !printed {
        for s in solutions {
            if s.num_points() == 1 {
                println!("{}", s.plotname);
                for (name, _) in &s.variables {
                    if let Some(col) = s.column(name) {
                        println!("{:<24} = {}", name, dune_raw::fmt_e(col[0], 6));
                    }
                }
            }
        }
    }
    Ok(())
}
