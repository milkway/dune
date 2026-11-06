//! Render pic figures with the dune prelude to SVG:
//! `cargo run -p dune-pic --example preview -- OUT_DIR FIGURE.pic...`
//! writes `OUT_DIR/<stem>.svg` for each figure (what the overlay map's
//! shape ids refer to).

use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = PathBuf::from(args.next().ok_or("usage: preview OUT_DIR FIGURE.pic...")?);
    std::fs::create_dir_all(&out)?;
    for fig in args {
        let src = std::fs::read_to_string(&fig)?;
        let svg = dune_pic::render_svg(&src, &dune_pic::ExtractOptions::default())?;
        let stem = Path::new(&fig).file_stem().ok_or("bad file name")?;
        let dest = out.join(stem).with_extension("svg");
        std::fs::write(&dest, svg)?;
        println!("{}", dest.display());
    }
    Ok(())
}
