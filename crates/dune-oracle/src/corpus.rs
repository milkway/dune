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
    ".op", ".dc", ".ac", ".tran", ".pz", ".noise", ".disto", ".sens", ".tf", ".sp",
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
    /// `LEVEL` of every `nmos`/`pmos` `.model` card that some
    /// M-card actually uses (default 1), so the support gate can
    /// tell MOS1/2/6 decks from MOS3 ones — decks routinely carry
    /// alternative model cards for levels they never instantiate
    /// (mos6inv.cir).
    pub mos_levels: Vec<u32>,
    /// Reason from `corpus/SKIP`, when listed there.
    pub skip: Option<String>,
}

#[derive(Debug)]
pub struct Corpus {
    pub root: PathBuf,
    pub circuits: Vec<Circuit>,
    pub xfails: Vec<Xfail>,
}

/// One `corpus/XFAIL` entry: an expected-marginal comparison.
#[derive(Debug)]
pub struct Xfail {
    pub stem: String,
    pub plot: String,
    pub max_margin: f64,
    /// `bistable`: the deck forks on last-ulp differences (platform
    /// libm, SIMD LU), so either outcome is legitimate — a PASS is
    /// reported but does not ask for the entry's removal (D-0022).
    pub bistable: bool,
    /// `accept-error=<text>`: a Dune run failing with an error that
    /// contains `<text>` is the known outcome of the other branch
    /// (e.g. the transient step budget), reported but not a CI
    /// failure.
    pub accept_error: Option<String>,
    pub reason: String,
}

impl Xfail {
    /// Does `err` (a failed Dune run of this deck) match the entry's
    /// accepted structured failure?
    pub fn accepts_error(&self, err: &str) -> bool {
        self.accept_error
            .as_deref()
            .is_some_and(|s| err.contains(s))
    }
}

impl Corpus {
    /// Locate `corpus/` (next to the workspace manifest or under the
    /// current directory) and inventory it.
    pub fn load() -> Result<Self> {
        let root = find_root()?;
        let skips = read_skip_file(&root)?;
        let xfails = read_xfail_file(&root)?;

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
        Ok(Corpus {
            root,
            circuits,
            xfails,
        })
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
            let (analyses, devices, mos_levels) = scan_deck(&path)?;
            out.push(Circuit {
                stem,
                path,
                rel: String::new(),
                analyses,
                devices,
                mos_levels,
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
fn scan_deck(path: &Path) -> Result<(Vec<String>, Vec<char>, Vec<u32>)> {
    let text = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let text = String::from_utf8_lossy(&text);
    let mut analyses = Vec::new();
    let mut devices = Vec::new();
    // (name, level) of every nmos/pmos model card, and the model
    // names the M-cards reference.
    let mut mos_models: Vec<(String, u32)> = Vec::new();
    let mut mos_used: Vec<String> = Vec::new();
    // ngspice VDMOS model names (implemented, no level), and the
    // tokens of every M card: a VDMOS card has 3 to 5 nodes, so the
    // model is resolved once every `.model` card is known.
    let mut vdmos_models: Vec<String> = Vec::new();
    let mut m_cards: Vec<Vec<String>> = Vec::new();
    let mut in_control = false;
    let mut in_mos_model = false;
    for line in text.lines().skip(1) {
        let line = line.trim_start();
        let first = line.chars().next().unwrap_or('*');
        // `.model` cards continue over `+` lines; `LEVEL` may sit on
        // any of them (mosmem.cir does exactly that).
        if first == '+' {
            if in_mos_model
                && !in_control
                && let (Some(l), Some(slot)) = (scan_level(line), mos_models.last_mut())
            {
                slot.1 = l;
            }
            continue;
        }
        in_mos_model = false;
        if first == '.' {
            let lower = line.to_ascii_lowercase();
            let card = lower.split_whitespace().next().unwrap_or("").to_string();
            match card.as_str() {
                ".control" => in_control = true,
                ".endc" => in_control = false,
                ".model" if !in_control => {
                    if lower
                        .split(|c: char| c.is_whitespace() || c == '(')
                        .nth(2)
                        .is_some_and(|t| t.starts_with("vdmos"))
                    {
                        let name = lower.split_whitespace().nth(1).unwrap_or("").to_string();
                        vdmos_models.push(name);
                    } else if lower
                        .split(|c: char| c.is_whitespace() || c == '(')
                        .any(|t| t == "nmos" || t == "pmos")
                    {
                        in_mos_model = true;
                        let name = lower.split_whitespace().nth(1).unwrap_or("").to_string();
                        mos_models.push((name, scan_level(&lower).unwrap_or(1)));
                    }
                }
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
            continue; // comments, control commands
        }
        let letter = first.to_ascii_lowercase();
        if letter == 'm' {
            m_cards.push(
                line.split_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect(),
            );
        }
        if !devices.contains(&letter) {
            devices.push(letter);
        }
    }
    for toks in &m_cards {
        if toks
            .iter()
            .skip(4)
            .take(3)
            .any(|t| vdmos_models.contains(t))
        {
            continue;
        }
        // `Mxxx d g s b MNAME …`: the model is the 6th token.
        if let Some(name) = toks.get(5)
            && !mos_used.contains(name)
        {
            mos_used.push(name.clone());
        }
    }
    devices.sort_unstable();
    // Levels of the models the deck instantiates; a reference the
    // scan cannot resolve is kept as level 0 (never supported), so
    // the gate stays conservative.
    let mut mos_levels: Vec<u32> = Vec::new();
    for name in &mos_used {
        let exact = mos_models.iter().find(|(n, _)| n == name).map(|(_, l)| *l);
        // A binned family `name.<digits>` (G4 of docs/pdk-parser.md):
        // every bin's level counts, the least supported one gating.
        let bins = || {
            mos_models.iter().filter(|(n, _)| {
                n.strip_prefix(name.as_str())
                    .and_then(|r| r.strip_prefix('.'))
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            })
        };
        match exact {
            Some(l) => mos_levels.push(l),
            None if bins().next().is_some() => mos_levels.extend(bins().map(|(_, l)| *l)),
            None => mos_levels.push(0),
        }
    }
    Ok((analyses, devices, mos_levels))
}

/// Find `level=<n>` (spaces allowed around `=`) in a lowercased
/// model-card line.
fn scan_level(line: &str) -> Option<u32> {
    let lower = line.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find("level") {
        let at = from + pos;
        from = at + 5;
        // Must be a whole token.
        if at > 0
            && lower[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let rest = lower[at + 5..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let digits: String = rest
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(v) = digits.parse::<f64>() {
            return Some(v as u32);
        }
    }
    None
}

fn read_xfail_file(root: &Path) -> Result<Vec<Xfail>> {
    let mut out = Vec::new();
    let path = root.join("XFAIL");
    if !path.is_file() {
        return Ok(out);
    }
    for line in fs::read_to_string(&path)?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(parse_xfail_line(line)?);
    }
    Ok(out)
}

/// `stem|plot|cap|flags|reason`; `flags` is `-` or a comma-separated
/// list of `bistable` and `accept-error=<text>` (the reason is free
/// text and may itself contain `|`).
fn parse_xfail_line(line: &str) -> Result<Xfail> {
    let parts: Vec<&str> = line.splitn(5, '|').collect();
    let [stem, plot, cap, flags, reason] = parts.as_slice() else {
        bail!("corpus/XFAIL: bad line `{line}` (want stem|plot|cap|flags|reason)");
    };
    let max_margin: f64 = cap
        .trim()
        .parse()
        .with_context(|| format!("corpus/XFAIL: bad margin in `{line}`"))?;
    let mut bistable = false;
    let mut accept_error = None;
    let flags = flags.trim();
    if flags != "-" {
        for flag in flags.split(',').map(str::trim) {
            if flag == "bistable" {
                bistable = true;
            } else if let Some(text) = flag.strip_prefix("accept-error=") {
                if text.trim().is_empty() {
                    bail!("corpus/XFAIL: empty accept-error in `{line}`");
                }
                accept_error = Some(text.trim().to_string());
            } else {
                bail!("corpus/XFAIL: unknown flag `{flag}` in `{line}`");
            }
        }
    }
    Ok(Xfail {
        stem: stem.trim().to_string(),
        plot: plot.trim().to_string(),
        max_margin,
        bistable,
        accept_error,
        reason: reason.trim().to_string(),
    })
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
        let (a, d, m) = scan_deck(&p).unwrap();
        assert_eq!(a, vec![".tran", ".ac"]);
        // `run` inside .control and the `+` continuation are not
        // devices; the title line is skipped.
        assert_eq!(d, vec!['q', 'r']);
        assert!(m.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_resolves_vdmos_cards_of_three_to_five_nodes() {
        let dir = std::env::temp_dir().join(format!("dune-oracle-vd-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("vd.cir");
        fs::write(
            &p,
            "t\nm1 d g s pw\nm2 d g s tj tc pw thermal\nm3 d g s b n4\n\
             .model pw vdmos(pchan kp=1)\n.model n4 nmos level=3\n.op\n.end\n",
        )
        .unwrap();
        let (_, _, m) = scan_deck(&p).unwrap();
        // The VDMOS cards need no level; m3 resolves to level 3.
        assert_eq!(m, vec![3]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_reads_mos_levels_across_continuations() {
        let dir = std::env::temp_dir().join(format!("dune-oracle-lvl-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("lvl.cir");
        fs::write(
            &p,
            "t\nm1 1 2 3 4 a\n.model a nmos(vto=1\n+ level=2)\n\
             .model b pmos level = 6\n.model c nmos(kp=1u)\n.op\n.end\n",
        )
        .unwrap();
        let (_, d, m) = scan_deck(&p).unwrap();
        assert_eq!(d, vec!['m']);
        // Only model `a` (level 2) is instantiated; the unused
        // level-6 and level-1 cards do not gate the deck.
        assert_eq!(m, vec![2]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_resolves_binned_families() {
        let dir = std::env::temp_dir().join(format!("dune-oracle-bin-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("bin.cir");
        fs::write(
            &p,
            "t\nm1 1 2 3 4 mm l=1u w=1u\nm2 1 2 3 4 nx l=1u w=1u\n\
             .model mm.1 nmos level=49\n.model mm.2 nmos level=54\n.model mm.x nmos\n\
             .op\n.end\n",
        )
        .unwrap();
        let (_, _, m) = scan_deck(&p).unwrap();
        // Every bin of `mm` counts (`mm.x` is no bin); `nx` is unknown.
        assert_eq!(m, vec![49, 54, 0]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn xfail_plain_entry() {
        let x = parse_xfail_line("schmitt|Transient Analysis|5|-|edges").unwrap();
        assert_eq!(x.stem, "schmitt");
        assert_eq!(x.plot, "Transient Analysis");
        assert_eq!(x.max_margin, 5.0);
        assert!(!x.bistable);
        assert!(x.accept_error.is_none());
        assert_eq!(x.reason, "edges");
    }

    #[test]
    fn xfail_flags() {
        let x = parse_xfail_line(
            "ltra_3|Transient Analysis|35|bistable, accept-error=internal steps|a|b",
        )
        .unwrap();
        assert!(x.bistable);
        assert_eq!(x.accept_error.as_deref(), Some("internal steps"));
        assert_eq!(x.reason, "a|b");
        assert!(x.accepts_error("transient exceeded 2000000 internal steps at t = 1e-8"));
        assert!(!x.accepts_error("timestep too small"));
    }

    #[test]
    fn xfail_rejects_bad_lines() {
        assert!(parse_xfail_line("a|b|5|reason only").is_err());
        assert!(parse_xfail_line("a|b|x|-|r").is_err());
        assert!(parse_xfail_line("a|b|5|flaky|r").is_err());
        assert!(parse_xfail_line("a|b|5|accept-error=|r").is_err());
    }
}
