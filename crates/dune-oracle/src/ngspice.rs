//! Running the pinned ngspice oracle (D-0006).
//!
//! The oracle version is pinned in `corpus/ORACLE` (first line, e.g.
//! `ngspice-47`). Everything that *runs* ngspice checks the installed
//! version against the pin first; `--ci` never runs ngspice (it works
//! from the versioned golden rawfiles alone).

use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

/// Read the pinned oracle name from `corpus/ORACLE` (e.g. `ngspice-47`).
pub fn pinned_version(corpus_root: &Path) -> Result<String> {
    let path = corpus_root.join("ORACLE");
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {} (the oracle pin)", path.display()))?;
    let pin = text.lines().next().unwrap_or("").trim().to_string();
    if pin.is_empty() {
        bail!("{} is empty; expected e.g. `ngspice-47`", path.display());
    }
    Ok(pin)
}

/// Check that the installed ngspice matches the pin; returns the pin.
pub fn check_version(corpus_root: &Path) -> Result<String> {
    let pin = pinned_version(corpus_root)?;
    let out = Command::new("ngspice")
        .arg("--version")
        .output()
        .context("running `ngspice --version` (is ngspice installed? `brew install ngspice`)")?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let found = stdout
        .split_whitespace()
        .find(|w| w.starts_with("ngspice-"))
        .unwrap_or("ngspice-?")
        .to_string();
    if found != pin {
        bail!(
            "installed {found} but corpus/ORACLE pins {pin}; install the \
             pinned version or add a KNOWN_DIFFS.md entry and repin"
        );
    }
    Ok(pin)
}

/// Run `ngspice -b -D filetype=ascii -r <out_raw> <cir>` with the
/// circuit's directory as working directory (so relative `.include`
/// paths resolve the way `spice3 -b` resolved them).
pub fn run(cir: &Path, out_raw: &Path) -> Result<Output> {
    let dir = cir.parent().unwrap_or(Path::new("."));
    let out = Command::new("ngspice")
        .args(["-b", "-D", "filetype=ascii", "-r"])
        .arg(out_raw)
        .arg(cir)
        .current_dir(dir)
        .output()
        .context("spawning ngspice")?;
    if !out.status.success() {
        bail!(
            "ngspice failed on {} ({}):\n{}",
            cir.display(),
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(out)
}
