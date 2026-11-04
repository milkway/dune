//! Corpus discovery and inventory (PLAN.md §3, Phase 0).
//!
//! Circuits live under `corpus/` in four roots: `spice3f5/examples`
//! (upstream examples, untouched), `berkeley` (quarles_bench + qxdir,
//! still missing — see `corpus/README.md`), `basic` (small decks
//! written for Dune) and `regress` (one minimal `.cir` per fixed
//! bug). `corpus/SKIP` lists circuits the oracle must
//! not run, one per line: `<path relative to corpus/> <reason…>`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Subdirectories of `corpus/` that are scanned for `.cir` files.
const ROOTS: &[&str] = &["spice3f5/examples", "berkeley", "basic", "regress"];

/// Analysis cards recognized by the inventory scan (3f5 grammar).
const ANALYSES: &[&str] = &[
    ".op", ".dc", ".ac", ".tran", ".pz", ".noise", ".disto", ".sens", ".tf",
];

/// One corpus circuit.
#[derive(Debug)]
pub struct Circuit {
    /// File stem, unique across the corpus; names the golden rawfile.
    pub stem: String,
    /// Absolute path to the `.cir` file.
    pub path: PathBuf,
    /// Path relative to `corpus/`, for display.
    pub rel: String,
    /// Analysis cards found in the deck, in order of first appearance.
    pub analyses: Vec<String>,
    /// Device letters used by the deck's element cards (lowercase).
    pub devices: Vec<char>,
    /// Reason from `corpus/SKIP`, when listed there.
    pub skip: Option<String>,
}

#[derive(Debug)]
pub struct Corpus {
    pub root: PathBuf,
    pub circuits: Vec<Circuit>,
}

impl Corpus {
    /// Locate `corpus/` (next to the workspace manifest or under the
    /// current directory) and inventory it.
    pub fn load() -> Result<Self> {
        let root = find_root()?;
        let skips = read_skip_file(&root)?;

        let mut circuits = Vec::new();
        for sub in ROOTS {
            let dir = root.join(sub);
            if dir.is_dir() {
                collect_cir_files(&dir, &mut circuits)?;
            }
        }
        circuits.sort_by(|a, b| a.path.cmp(&b.path));

        let mut seen = std::collections::HashMap::new();
        for c in &mut circuits {
            c.rel = c
                .path
                .strip_prefix(&root)
                .unwrap_or(&c.path)
                .display()
                .to_string();
            c.skip = skips.get(&c.rel).cloned();
            if let Some(prev) = seen.insert(c.stem.clone(), c.rel.clone()) {
                bail!(
                    "duplicate circuit stem `{}` ({} and {}): golden \
                     rawfiles are named by stem, rename one of them",
                    c.stem,
                    prev,
                    c.rel
                );
            }
        }
        Ok(Corpus { root, circuits })
    }

    /// Where the golden rawfile of `c` lives (`corpus/golden/<stem>.raw`).
    pub fn golden_path(&self, c: &Circuit) -> PathBuf {
        self.root.join("golden").join(format!("{}.raw", c.stem))
    }

    /// Find a circuit by stem or by path (exact, or suffix match).
    pub fn find(&self, arg: &str) -> Option<&Circuit> {
        let norm = arg.trim_end_matches(".cir");
        self.circuits
            .iter()
            .find(|c| c.stem == norm)
            .or_else(|| {
                self.circuits
                    .iter()
                    .find(|c| Path::new(arg) == c.path || c.rel == arg)
            })
            .or_else(|| {
                self.circuits
                    .iter()
                    .find(|c| c.path.ends_with(arg) || arg.ends_with(&c.rel))
            })
    }
}

fn find_root() -> Result<PathBuf> {
    // `cargo run` executes from the workspace root; fall back to the
    // manifest location so the binary also works from elsewhere.
    let cwd = Path::new("corpus");
    if cwd.is_dir() {
        return Ok(cwd.canonicalize()?);
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus")
        .canonicalize();
    match manifest {
        Ok(p) if p.is_dir() => Ok(p),
        _ => bail!("corpus/ not found (run from the workspace root)"),
    }
}

fn collect_cir_files(dir: &Path, out: &mut Vec<Circuit>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_cir_files(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("cir"))
        {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (analyses, devices) = scan_deck(&path)?;
            out.push(Circuit {
                stem,
                path,
                rel: String::new(),
                analyses,
                devices,
                skip: None,
            });
        }
    }
    Ok(())
}

/// Collect the analysis cards (in order of first appearance) and the
/// element letters of a deck. A plain line scan is enough for the
/// inventory; the real parser is `dune-netlist`. The first line is
/// the title and `.control` blocks are commands, so both are skipped.
fn scan_deck(path: &Path) -> Result<(Vec<String>, Vec<char>)> {
    let text = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let text = String::from_utf8_lossy(&text);
    let mut analyses = Vec::new();
    let mut devices = Vec::new();
    let mut in_control = false;
    for line in text.lines().skip(1) {
        let line = line.trim_start();
        let first = line.chars().next().unwrap_or('*');
        if first == '.' {
            let card = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            match card.as_str() {
                ".control" => in_control = true,
                ".endc" => in_control = false,
                _ if !in_control
                    && ANALYSES.contains(&card.as_str())
                    && !analyses.contains(&card) =>
                {
                    analyses.push(card);
                }
                _ => {}
            }
            continue;
        }
        if in_control || !first.is_ascii_alphabetic() {
            continue; // comments, continuations, control commands
        }
        let letter = first.to_ascii_lowercase();
        if !devices.contains(&letter) {
            devices.push(letter);
        }
    }
    devices.sort_unstable();
    Ok((analyses, devices))
}

fn read_skip_file(root: &Path) -> Result<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();
    let path = root.join("SKIP");
    if !path.is_file() {
        return Ok(map);
    }
    let text = fs::read_to_string(&path)?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (rel, reason) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        map.insert(rel.to_string(), reason.trim().to_string());
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_analyses_and_devices() {
        let dir = std::env::temp_dir().join(format!("dune-oracle-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.cir");
        fs::write(
            &p,
            "title line, not a device\n.OPTIONS reltol=1e-3\n .TRAN 1n 1u\nr1 1 0 1k\n             + 100\nQ3 1 2 3 mod\n.control\nrun\n.endc\n.ac dec 10 1 1meg\n.tran 2n 2u\n.end\n",
        )
        .unwrap();
        let (a, d) = scan_deck(&p).unwrap();
        assert_eq!(a, vec![".tran", ".ac"]);
        // `run` inside .control and the `+` continuation are not
        // devices; the title line is skipped.
        assert_eq!(d, vec!['q', 'r']);
        fs::remove_dir_all(&dir).ok();
    }
}
