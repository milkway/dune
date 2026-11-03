//! Re-encode a rawfile: `recode <in.raw> <out.raw> ascii|binary`.
//! Development aid for checking interchange with ngspice.

use dune_raw::{Format, Rawfile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output), Some(fmt)) = (args.next(), args.next(), args.next()) else {
        return Err("usage: recode <in.raw> <out.raw> ascii|binary".into());
    };
    let format = match fmt.as_str() {
        "ascii" => Format::Ascii,
        "binary" => Format::Binary,
        other => return Err(format!("unknown format {other:?}").into()),
    };
    let raw = Rawfile::read_from(&input)?;
    for w in &raw.warnings {
        eprintln!("warning: {w}");
    }
    let mut out = std::fs::File::create(&output)?;
    raw.write(&mut out, format)?;
    Ok(())
}
