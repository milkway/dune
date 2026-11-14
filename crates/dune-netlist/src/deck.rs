//! Physical deck reading: files → logical cards.
//!
//! Mirrors `inp_readall` in `corpus/spice3f5/src/lib/fte/inpcom.c`:
//! the first non-blank line of the root file is the title; `.include`
//! splices another file in before anything else happens; then
//! whole-line comments are dropped and `+` continuation lines are
//! joined (with a single space) onto the previous card — comments in
//! between do not break a continuation chain, and the title line can
//! never be continued.
//!
//! ngspice's library sections (`.lib FILE SECTION`, with the library
//! holding `.lib SECTION` … `.endl` blocks — `expand_section_ref` in
//! ngspice-47's `frontend/inpcom.c`) are resolved here too, on
//! physical lines: only the body of the requested section is spliced,
//! nested `.lib`/`.include` inside it resolve against the library
//! file, and each file is read once however many of its sections are
//! requested.
//!
//! Deliberate differences from the C, all documented in
//! `docs/dune-netlist.md`:
//! - relative `.include`/`.lib` paths resolve against the including
//!   file's directory (the 3f5 uses the process working directory;
//!   ngspice tries the working directory and the deck's directory
//!   first);
//! - `$` and `;` preceded by whitespace start an end-of-line comment
//!   (ngspice superset; the C only knows whole-line comments);
//! - trailing `\r` is stripped (DOS decks);
//! - `.include`/`.lib` nesting, cycles, bytes read and bytes spliced
//!   are bounded so hostile decks cannot hang or exhaust memory
//!   (principle 4 of `CLAUDE.md`); ngspice-47 segfaults on an
//!   `.include` cycle.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::diag::Diagnostic;
use crate::span::{FileId, SourceMap, Span};

/// Maximum `.include`/`.lib` nesting (one count for both).
const MAX_NESTING: usize = 16;
/// Cap on the bytes the loader hands over for one deck (each file is
/// read once, so this bounds distinct source text).
const MAX_READ_BYTES: usize = 256 << 20;
/// Cap on the bytes spliced into the deck: a section requested over
/// and over is read once but spliced each time, and nesting can
/// multiply that.
const MAX_SPLICED_BYTES: usize = 256 << 20;

/// Loads the text of a file named by `.include` or `.lib`. The default
/// implementation reads the file system; a WASM host can supply its
/// own. The reader asks for each file at most once per deck.
///
/// `limit` is the byte budget left for this deck: an implementation
/// must not return more than `limit` bytes, and must refuse sources
/// it cannot bound (pipes, device files) — otherwise a hostile
/// `.include /dev/zero` would hang the read before any cap applies.
pub trait IncludeLoader {
    fn load(&mut self, path: &Path, limit: u64) -> Result<String, String>;
}

/// [`IncludeLoader`] over `std::fs`.
pub struct FsLoader;

impl IncludeLoader for FsLoader {
    fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
        use std::io::Read;
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err("not a regular file".into());
        }
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut text = String::new();
        file.take(limit.saturating_add(1))
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() as u64 > limit {
            return Err(format!(
                "larger than the deck's remaining {limit}-byte budget"
            ));
        }
        Ok(text)
    }
}

/// One logical line: continuations joined, with enough bookkeeping to
/// map any byte range of `text` back to a source [`Span`].
#[derive(Debug)]
pub struct Card {
    pub text: String,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct Segment {
    /// Start of this segment inside `Card::text`.
    text_start: u32,
    len: u32,
    /// Where the segment came from.
    src: Span,
}

impl Card {
    fn new(text: String, src: Span) -> Self {
        let len = text.len() as u32;
        Card {
            text,
            segments: vec![Segment {
                text_start: 0,
                len,
                src,
            }],
        }
    }

    /// Append a continuation segment (joined with a single space).
    fn push_continuation(&mut self, more: &str, src: Span) {
        self.text.push(' ');
        let text_start = self.text.len() as u32;
        self.text.push_str(more);
        self.segments.push(Segment {
            text_start,
            len: more.len() as u32,
            src,
        });
    }

    /// Source span of `text[start..end]`. Ranges that straddle a
    /// continuation boundary are clipped to the segment holding
    /// `start` — good enough for diagnostics.
    pub fn span_of(&self, start: usize, end: usize) -> Span {
        let start = start as u32;
        let end = end as u32;
        let seg = self
            .segments
            .iter()
            .rev()
            .find(|s| s.text_start <= start)
            .unwrap_or(&self.segments[0]);
        let off = start.saturating_sub(seg.text_start).min(seg.len);
        let len = end.saturating_sub(start).min(seg.len - off);
        Span {
            file: seg.src.file,
            start: seg.src.start + off,
            end: seg.src.start + off + len,
        }
    }

    /// Span of the whole card (first segment's source line).
    pub fn span(&self) -> Span {
        self.segments[0].src
    }
}

/// The reader's output: a title plus logical cards, with everything
/// needed to render diagnostics.
#[derive(Debug)]
pub struct Deck {
    pub title: String,
    pub cards: Vec<Card>,
    pub sources: SourceMap,
    pub diags: Vec<Diagnostic>,
}

/// Read a deck from the file system (root file + includes).
pub fn read_file(path: &Path) -> Result<Deck, crate::Error> {
    let text = FsLoader
        .load(path, MAX_READ_BYTES as u64)
        .map_err(|message| crate::Error::Io {
            path: path.to_path_buf(),
            message,
        })?;
    Ok(read_with_loader(path, text, &mut FsLoader))
}

/// Read a deck from a string, resolving `.include` and `.lib` with
/// `loader` (relative to the including file's directory).
pub fn read_with_loader(name: &Path, text: String, loader: &mut dyn IncludeLoader) -> Deck {
    let mut r = Reader {
        sources: SourceMap::new(),
        physical: Vec::new(),
        diags: Vec::new(),
        read_bytes: 0,
        spliced_bytes: 0,
        aborted: false,
        loader,
        files: HashMap::new(),
        sections: HashMap::new(),
        stack: Vec::new(),
    };
    r.read_bytes = text.len();
    match r.sources.add(name.to_path_buf(), text) {
        Some(file) => {
            r.files.insert(normalize(name), file);
            r.stack.push((normalize(name), None));
            let lines = line_ranges(&r.sources.file(file).text);
            r.splice(file, name, &lines, 0, false);
        }
        None => r
            .diags
            .push(Diagnostic::global_error("source file too large")),
    }
    r.finish()
}

/// A physical line that survived file reading and include splicing.
struct Physical {
    text: String,
    src: Span,
}

/// One `.lib NAME` … `.endl` block of a library file.
struct Section {
    /// Lowercased (ngspice compares section names case-insensitively).
    name: String,
    /// The `.lib NAME` line.
    head: Span,
    /// Body lines (byte ranges into the library file).
    body: Vec<(usize, usize)>,
    /// The closing `.endl` line and the name it carries, if any.
    end: Option<(Span, Option<String>)>,
}

/// What the include stack remembers: a file, and the section of it
/// being spliced (`None` for a whole file).
type StackKey = (PathBuf, Option<String>);

struct Reader<'a> {
    sources: SourceMap,
    physical: Vec<Physical>,
    diags: Vec<Diagnostic>,
    /// Bytes handed over by the loader (each file is read once).
    read_bytes: usize,
    /// Bytes of physical lines spliced into the deck.
    spliced_bytes: usize,
    /// Set once a budget is exhausted: reading stops everywhere.
    aborted: bool,
    loader: &'a mut dyn IncludeLoader,
    /// Every file read so far, by lexically normalized path.
    files: HashMap<PathBuf, FileId>,
    /// Section index of each library file, built on first use.
    sections: HashMap<FileId, Vec<Section>>,
    /// Files and sections being spliced right now (cycle detection).
    stack: Vec<StackKey>,
}

/// A directive the reader acts on before any card exists.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Directive {
    Include,
    Lib,
    Endl,
}

/// One argument of a reader directive, quotes stripped; `start..end`
/// covers the token in the line (quotes included).
struct Arg {
    text: String,
    start: usize,
    end: usize,
}

impl Reader<'_> {
    /// Splice `lines` (byte ranges of `file`, read from `path`) into
    /// `physical`, acting on `.include`/`.lib`/`.endl` on the way.
    /// `in_section` tells a library section body from a plain file.
    fn splice(
        &mut self,
        file: FileId,
        path: &Path,
        lines: &[(usize, usize)],
        depth: usize,
        in_section: bool,
    ) {
        // After a misplaced `.lib NAME` (a section definition outside
        // a library file) the block it opens is skipped up to its
        // `.endl`, so one mistake gives one diagnostic.
        let mut skipping = false;
        for &(s, e) in lines {
            if self.aborted {
                return;
            }
            let line = self.sources.file(file).text[s..e].to_string();
            let directive = directive_of(&line);
            if skipping {
                skipping = directive.map(|(d, _)| d) != Some(Directive::Endl);
                continue;
            }
            if let Some((d, kw_end)) = directive {
                let src = Span::new(file, s, e);
                let args = match directive_args(&line, kw_end) {
                    Ok(args) => args,
                    Err((msg, a, b)) => {
                        self.diags
                            .push(Diagnostic::error(msg, Span::new(file, s + a, s + b)));
                        continue;
                    }
                };
                let arg_span = |a: &Arg| Span::new(file, s + a.start, s + a.end);
                match d {
                    Directive::Include => match args.first() {
                        None => self
                            .diags
                            .push(Diagnostic::error(".include filename missing", src)),
                        Some(a) => {
                            let target = resolve(path, &a.text);
                            self.include(&target, arg_span(a), src, depth);
                        }
                    },
                    Directive::Lib => match args.as_slice() {
                        [] => self.diags.push(Diagnostic::error(
                            ".lib needs a library file and a section name",
                            src,
                        )),
                        [_] if in_section => self.diags.push(Diagnostic::error(
                            "`.lib NAME` inside a library section: sections do not nest \
                             (this one ends at the first `.endl`, as in ngspice)",
                            src,
                        )),
                        [a] => {
                            self.diags.push(Diagnostic::error(
                                format!(
                                    "`.lib {}` opens a library section, which only a file \
                                     read with `.lib FILE SECTION` may define; to read a \
                                     library give both the file and the section",
                                    a.text
                                ),
                                src,
                            ));
                            skipping = true;
                        }
                        [f, sec, rest @ ..] => {
                            if let Some(extra) = rest.first() {
                                self.diags.push(Diagnostic::warning(
                                    "text after the .lib section name ignored",
                                    arg_span(extra),
                                ));
                            }
                            let target = resolve(path, &f.text);
                            self.lib(&target, arg_span(f), &sec.text, arg_span(sec), src, depth);
                        }
                    },
                    Directive::Endl => self
                        .diags
                        .push(Diagnostic::error(".endl without .lib", src)),
                }
                continue;
            }
            self.push_line(file, s, e, depth);
        }
    }

    /// An ordinary physical line: strip end-of-line comments and keep
    /// it for the logical pass.
    fn push_line(&mut self, file: FileId, s: usize, mut e: usize, depth: usize) {
        let text = &self.sources.file(file).text;
        // End-of-line comments: `$` or `;` at the start of a field
        // (ngspice superset). The title line is printed verbatim, so
        // it is never stripped.
        let is_title = depth == 0 && self.physical.is_empty();
        if !is_title {
            let cut = inline_comment_start(&text[s..e]);
            e = s + cut;
            while e > s && matches!(text.as_bytes()[e - 1], b' ' | b'\t') {
                e -= 1;
            }
        }
        if text[s..e].trim().is_empty() {
            return;
        }
        self.spliced_bytes = self.spliced_bytes.saturating_add(e - s + 1);
        if self.spliced_bytes > MAX_SPLICED_BYTES {
            self.abort(
                format!(
                    "deck grows past {} MiB of spliced source text \
                     (.include/.lib repeated too often?); giving up",
                    MAX_SPLICED_BYTES >> 20
                ),
                Span::new(file, s, e),
            );
            return;
        }
        let line = text[s..e].to_string();
        self.physical.push(Physical {
            text: line,
            src: Span::new(file, s, e),
        });
    }

    fn abort(&mut self, msg: String, span: Span) {
        if !self.aborted {
            self.aborted = true;
            self.diags.push(Diagnostic::error(msg, span));
        }
    }

    /// Nesting and cycle checks shared by `.include` and `.lib`.
    fn may_enter(&mut self, key: &StackKey, what: &str, src: Span, depth: usize) -> bool {
        if self.stack.contains(key) {
            let target = match &key.1 {
                Some(sec) => format!("section `{sec}` of {}", key.0.display()),
                None => key.0.display().to_string(),
            };
            self.diags.push(Diagnostic::error(
                format!("{what} cycle: {target} is already being read"),
                src,
            ));
            return false;
        }
        if depth >= MAX_NESTING {
            self.diags.push(Diagnostic::error(
                format!(".include/.lib nested deeper than {MAX_NESTING} levels"),
                src,
            ));
            return false;
        }
        true
    }

    /// Load `path` (once per deck) through the loader, charging its
    /// size to the read budget.
    fn open(&mut self, path: &Path, what: &str, at: Span) -> Option<FileId> {
        let key = normalize(path);
        if let Some(&id) = self.files.get(&key) {
            return Some(id);
        }
        let budget = MAX_READ_BYTES.saturating_sub(self.read_bytes);
        let text = match self.loader.load(path, budget as u64) {
            Ok(text) => text,
            Err(e) => {
                self.diags.push(Diagnostic::error(
                    format!("cannot open {what} file {}: {e}", path.display()),
                    at,
                ));
                return None;
            }
        };
        if text.len() > budget {
            // A loader that ignores its limit still cannot blow the cap.
            self.abort(
                format!(
                    "deck exceeds {} MiB of source text; giving up",
                    MAX_READ_BYTES >> 20
                ),
                at,
            );
            return None;
        }
        self.read_bytes += text.len();
        let Some(id) = self.sources.add(path.to_path_buf(), text) else {
            self.diags
                .push(Diagnostic::error("source file too large", at));
            return None;
        };
        self.files.insert(key, id);
        Some(id)
    }

    /// `.include FILE`: splice the whole file.
    fn include(&mut self, path: &Path, at: Span, src: Span, depth: usize) {
        let key = (normalize(path), None);
        if !self.may_enter(&key, ".include", src, depth) {
            return;
        }
        let Some(file) = self.open(path, ".include", at) else {
            return;
        };
        let lines = line_ranges(&self.sources.file(file).text);
        self.stack.push(key);
        self.splice(file, path, &lines, depth + 1, false);
        self.stack.pop();
    }

    /// `.lib FILE SECTION`: splice the body of one section of a
    /// library file.
    fn lib(
        &mut self,
        path: &Path,
        at: Span,
        section: &str,
        sec_span: Span,
        src: Span,
        depth: usize,
    ) {
        let name = section.to_ascii_lowercase();
        let key = (normalize(path), Some(name.clone()));
        if !self.may_enter(&key, ".lib", src, depth) {
            return;
        }
        let Some(file) = self.open(path, "library", at) else {
            return;
        };
        let index = self
            .sections
            .entry(file)
            .or_insert_with(|| index_sections(file, &self.sources.file(file).text));
        // ngspice takes the first definition of a name.
        let Some(sec) = index.iter().find(|s| s.name == name) else {
            self.diags.push(Diagnostic::error(
                format!(
                    "section `{section}` not found in library file {}",
                    path.display()
                ),
                sec_span,
            ));
            return;
        };
        let head = sec.head;
        let body = sec.body.clone();
        let end = sec.end.clone();
        match end {
            None => {
                self.diags.push(Diagnostic::error(
                    format!("library section `{section}` is never closed by `.endl`"),
                    head,
                ));
                return;
            }
            Some((end_span, Some(closing))) if closing != name => {
                // ngspice accepts any name on `.endl` (and so do
                // upstream `.ends`); worth a warning, not an error.
                self.diags.push(Diagnostic::warning(
                    format!(".endl {closing} closes .lib section {name}"),
                    end_span,
                ));
            }
            Some(_) => {}
        }
        self.stack.push(key);
        self.splice(file, path, &body, depth + 1, true);
        self.stack.pop();
    }

    /// Logical pass: title, comments, continuations.
    fn finish(mut self) -> Deck {
        let mut lines = self.physical.into_iter();
        let title = match lines.next() {
            Some(l) => l.text.trim_end().to_string(),
            None => {
                self.diags
                    .push(Diagnostic::global_error("empty input deck"));
                String::new()
            }
        };

        let mut cards: Vec<Card> = Vec::new();
        for line in lines {
            let trimmed = line.text.trim_start();
            let first = trimmed.chars().next().unwrap_or('\0');
            match first {
                // Whole-line comments. The C also treats `#` and `$`
                // in column 1 this way. Comments do NOT break a
                // continuation chain.
                '*' | '#' | '$' => continue,
                '+' => {
                    let rest = &trimmed[1..];
                    let lead = line.text.len() - trimmed.len() + 1;
                    let src = Span {
                        file: line.src.file,
                        start: line.src.start + lead as u32,
                        end: line.src.end,
                    };
                    match cards.last_mut() {
                        Some(card) => card.push_continuation(rest, src),
                        None => self.diags.push(Diagnostic::warning(
                            "illegal continuation line: ignored",
                            line.src,
                        )),
                    }
                }
                _ => cards.push(Card::new(line.text, line.src)),
            }
        }
        Deck {
            title,
            cards,
            sources: self.sources,
            diags: self.diags,
        }
    }
}

/// Byte ranges of the non-blank lines of `text`, trailing `\r` and
/// blanks stripped. Blank lines never become cards (the C's readline
/// folds them away).
fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut push = |s: usize, mut e: usize| {
        while e > s && matches!(bytes[e - 1], b'\r' | b' ' | b'\t') {
            e -= 1;
        }
        if !text[s..e].trim().is_empty() {
            out.push((s, e));
        }
    };
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            push(start, i);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        push(start, bytes.len());
    }
    out
}

/// Index the `.lib NAME` … `.endl` sections of a library file in one
/// linear pass. Lines outside sections are ignored, as in ngspice
/// (only section bodies are ever spliced); inside a section the first
/// `.endl` closes it.
fn index_sections(file: FileId, text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut open = false;
    for (s, e) in line_ranges(text) {
        let line = &text[s..e];
        let directive = directive_of(line);
        if !open {
            if let Some((Directive::Lib, kw_end)) = directive
                && let Ok(args) = directive_args(line, kw_end)
                && let [name] = args.as_slice()
            {
                out.push(Section {
                    name: name.text.to_ascii_lowercase(),
                    head: Span::new(file, s, e),
                    body: Vec::new(),
                    end: None,
                });
                open = true;
            }
            continue;
        }
        let Some(sec) = out.last_mut() else {
            continue;
        };
        if let Some((Directive::Endl, kw_end)) = directive {
            let closing = directive_args(line, kw_end)
                .ok()
                .and_then(|a| a.into_iter().next())
                .map(|a| a.text.to_ascii_lowercase());
            sec.end = Some((Span::new(file, s, e), closing));
            open = false;
        } else {
            sec.body.push((s, e));
        }
    }
    out
}

/// The reader directive a line starts with (leading blanks allowed,
/// keyword case-insensitive) and the byte offset where the keyword
/// ends. `.inc` is ngspice's alias of `.include`.
fn directive_of(line: &str) -> Option<(Directive, usize)> {
    let lead = line.len() - line.trim_start().len();
    let rest = &line[lead..];
    if !rest.starts_with('.') {
        return None;
    }
    let kw_len = rest
        .find(|c: char| c.is_ascii_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    let kw = &rest[..kw_len];
    let d = if kw.eq_ignore_ascii_case(".include") || kw.eq_ignore_ascii_case(".inc") {
        Directive::Include
    } else if kw.eq_ignore_ascii_case(".lib") {
        Directive::Lib
    } else if kw.eq_ignore_ascii_case(".endl") {
        Directive::Endl
    } else {
        return None;
    };
    Some((d, lead + kw_len))
}

/// The arguments of a reader directive from byte `from` on: blank
/// separated, each optionally in `"…"` or `'…'` (ngspice accepts both;
/// the quotes are not part of the name). A `;` field or a lone `$`
/// starts an end-of-line comment. An unclosed quote is an error with
/// the byte range of the offending token.
fn directive_args(line: &str, from: usize) -> Result<Vec<Arg>, (String, usize, usize)> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = from;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        let q = bytes[i];
        if q == b'"' || q == b'\'' {
            let Some(len) = line[i + 1..].find(q as char) else {
                return Err(("missing closing quote".into(), start, bytes.len()));
            };
            let text = &line[i + 1..i + 1 + len];
            i += len + 2;
            if text.is_empty() {
                return Err(("empty quoted name".into(), start, i));
            }
            out.push(Arg {
                text: text.to_string(),
                start,
                end: i,
            });
            continue;
        }
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let text = &line[start..i];
        if text.starts_with(';') || text == "$" {
            break;
        }
        out.push(Arg {
            text: text.to_string(),
            start,
            end: i,
        });
    }
    Ok(out)
}

/// A directive's file name relative to the including file's directory
/// (absolute names stay as they are).
fn resolve(includer: &Path, name: &str) -> PathBuf {
    let p = Path::new(name);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        includer.parent().unwrap_or(Path::new(".")).join(p)
    }
}

/// Lexical normalization (`.` dropped, `dir/..` folded) so that one
/// file reached by two spellings is read once and a cycle through
/// `sub/../x.inc` is still seen as a cycle. No file system access:
/// symbolic links are not resolved (the nesting limit covers them).
fn normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else if !matches!(
                    out.components().next_back(),
                    Some(Component::RootDir | Component::Prefix(_))
                ) {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Byte offset where an end-of-line comment begins (`$` or `;` at the
/// start of the line or right after whitespace), or `s.len()`.
fn inline_comment_start(s: &str) -> usize {
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if (b == b'$' || b == b';') && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
            return i;
        }
    }
    s.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Severity;

    fn read(text: &str) -> Deck {
        read_with_loader(Path::new("test.cir"), text.into(), &mut FailLoader)
    }

    struct FailLoader;
    impl IncludeLoader for FailLoader {
        fn load(&mut self, _: &Path, _: u64) -> Result<String, String> {
            Err("no includes in this test".into())
        }
    }

    #[test]
    fn title_comments_and_continuations() {
        let d = read("my title\n* comment\nr1 1 0\n+ 1k\n$ gone\nc1 2 0 1u\n");
        assert_eq!(d.title, "my title");
        assert_eq!(d.cards.len(), 2);
        assert_eq!(d.cards[0].text, "r1 1 0  1k");
        assert_eq!(d.cards[1].text, "c1 2 0 1u");
    }

    #[test]
    fn comment_does_not_break_continuation() {
        let d = read("t\nr1 1 0\n* note\n+1k\n");
        assert_eq!(d.cards.len(), 1);
        assert_eq!(d.cards[0].text, "r1 1 0 1k");
    }

    #[test]
    fn continuation_cannot_attach_to_title() {
        let d = read("t\n+ 1k\n");
        assert!(d.cards.is_empty());
        assert_eq!(d.diags.len(), 1);
        assert_eq!(d.diags[0].severity, Severity::Warning);
    }

    #[test]
    fn inline_dollar_and_semicolon_comments() {
        let d = read("t\nr1 1 0 1k $ load\nc1 1 0 1u ; bypass\nv$weird 1 0\n");
        assert_eq!(d.cards[0].text, "r1 1 0 1k");
        assert_eq!(d.cards[1].text, "c1 1 0 1u");
        // `$` inside a token is not a comment.
        assert_eq!(d.cards[2].text, "v$weird 1 0");
    }

    #[test]
    fn blank_lines_vanish_and_title_is_first_nonblank() {
        let d = read("\n\n  \nthe title\nr1 1 0 1k\n");
        assert_eq!(d.title, "the title");
        assert_eq!(d.cards.len(), 1);
    }

    #[test]
    fn missing_include_is_a_diagnostic_not_a_panic() {
        let d = read("t\n.include nope.cir\nr1 1 0 1k\n");
        assert_eq!(d.cards.len(), 1);
        assert!(d.diags[0].message.contains("nope.cir"));
    }

    /// Serves files from memory; counts loads per path.
    struct MapLoader {
        files: Vec<(&'static str, String)>,
        loads: Vec<String>,
    }

    impl MapLoader {
        fn new(files: &[(&'static str, &str)]) -> Self {
            MapLoader {
                files: files.iter().map(|(p, t)| (*p, t.to_string())).collect(),
                loads: Vec::new(),
            }
        }
    }

    impl IncludeLoader for MapLoader {
        fn load(&mut self, path: &Path, limit: u64) -> Result<String, String> {
            let key = path.to_string_lossy().into_owned();
            self.loads.push(key.clone());
            let text = self
                .files
                .iter()
                .find(|(p, _)| *p == key)
                .map(|(_, t)| t.clone())
                .ok_or_else(|| "no such file".to_string())?;
            if text.len() as u64 > limit {
                return Err("over budget".into());
            }
            Ok(text)
        }
    }

    fn read_map(deck: &str, loader: &mut MapLoader) -> Deck {
        read_with_loader(Path::new("top.cir"), deck.into(), loader)
    }

    fn texts(d: &Deck) -> Vec<&str> {
        d.cards.iter().map(|c| c.text.as_str()).collect()
    }

    fn errors(d: &Deck) -> Vec<&str> {
        d.diags
            .iter()
            .filter(|g| g.severity == Severity::Error)
            .map(|g| g.message.as_str())
            .collect()
    }

    #[test]
    fn include_depth_is_bounded() {
        // Every file includes a new, deeper one: no cycle, only depth.
        struct DeepLoader;
        impl IncludeLoader for DeepLoader {
            fn load(&mut self, _: &Path, _: u64) -> Result<String, String> {
                Ok(".include d/x.cir\n".into())
            }
        }
        let d = read_with_loader(
            Path::new("test.cir"),
            "t\n.include d/x.cir\n".into(),
            &mut DeepLoader,
        );
        assert!(d.diags.iter().any(|g| g.message.contains("nested deeper")));
    }

    #[test]
    fn include_cycle_is_a_spanned_error() {
        let mut l = MapLoader::new(&[
            ("a.inc", ".include sub/../b.inc\n"),
            ("sub/../b.inc", ".inc 'a.inc'\n"),
        ]);
        let d = read_map("t\n.include a.inc\nr1 1 0 1\n", &mut l);
        let e = errors(&d);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains(".include cycle"), "{e:?}");
        let span = d.diags[0].span.unwrap();
        assert_eq!(d.sources.file(span.file).name, Path::new("sub/../b.inc"));
        assert_eq!(texts(&d), ["r1 1 0 1"]);
    }

    #[test]
    fn lib_section_is_selected() {
        let lib = "* header\nrout 1 0 1\n.lib s1\nr1 1 0 1k\n.endl s1\n.LIB S2\nr2 1 0 2k\n.ENDL\n";
        let mut l = MapLoader::new(&[("m.lib", lib)]);
        let d = read_map("t\n.lib m.lib s2\nv1 1 0 1\n", &mut l);
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        assert_eq!(texts(&d), ["r2 1 0 2k", "v1 1 0 1"]);
    }

    #[test]
    fn lib_file_is_read_once_for_many_sections() {
        let lib = ".lib a\nra 1 0 1\n.endl\n.lib b\nrb 1 0 1\n.lib m.lib a\n.endl\n";
        let mut l = MapLoader::new(&[("m.lib", lib)]);
        let d = read_map(
            "t\n.lib m.lib b\n.lib 'm.lib' a\n.lib \"m.lib\" B\n",
            &mut l,
        );
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        assert_eq!(
            texts(&d),
            ["rb 1 0 1", "ra 1 0 1", "ra 1 0 1", "rb 1 0 1", "ra 1 0 1"]
        );
        assert_eq!(l.loads, ["m.lib"]);
    }

    #[test]
    fn nested_lib_and_include_resolve_against_the_library() {
        let lib = ".lib s\n  .lib 'other.lib' t\n  .include \"../inc/x.inc\"\n.endl\n";
        let mut l = MapLoader::new(&[
            ("pdk/m.lib", lib),
            ("pdk/other.lib", ".lib t\nrt 1 0 1\n.endl t\n"),
            ("pdk/../inc/x.inc", "rx 1 0 1\n"),
        ]);
        let d = read_map("t\n   .LIB pdk/m.lib S ; comment\n", &mut l);
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        assert_eq!(texts(&d), ["rt 1 0 1", "rx 1 0 1"]);
    }

    #[test]
    fn lib_cycle_and_same_section_twice() {
        let lib = ".lib s\n.lib m.lib s\nr1 1 0 1\n.endl\n";
        let mut l = MapLoader::new(&[("m.lib", lib)]);
        let d = read_map("t\n.lib m.lib s\n", &mut l);
        let e = errors(&d);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains(".lib cycle"), "{e:?}");
        // The same section twice in a row is not a cycle (ngspice P24).
        let lib = ".lib s\nr1 1 0 1\n.endl\n";
        let mut l = MapLoader::new(&[("m.lib", lib)]);
        let d = read_map("t\n.lib m.lib s\n.lib m.lib s\n", &mut l);
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        assert_eq!(d.cards.len(), 2);
    }

    #[test]
    fn lib_errors_are_spanned_diagnostics() {
        let mut l = MapLoader::new(&[(
            "m.lib",
            ".lib ok\nr1 1 0 1\n.endl wrong\n.lib open\nr2 1 0 1\n",
        )]);
        let cases: &[(&str, &str)] = &[
            (".lib m.lib nosuch", "section `nosuch` not found"),
            (".lib m.lib open", "never closed by `.endl`"),
            (".lib nofile.lib s", "cannot open library file"),
            (".lib", "needs a library file and a section name"),
            (".lib 'm.lib s", "missing closing quote"),
            (".lib \"\" s", "empty quoted name"),
            (".endl", ".endl without .lib"),
            (".lib sec\nr9 1 0 1\n.endl", "opens a library section"),
            (".include", ".include filename missing"),
        ];
        for (line, want) in cases {
            let d = read_map(&format!("t\n{line}\nv1 1 0 1\n"), &mut l);
            let e = errors(&d);
            assert_eq!(e.len(), 1, "{line}: {e:?}");
            assert!(e[0].contains(want), "{line}: {e:?}");
            assert!(d.diags.iter().all(|g| g.span.is_some()), "{line}");
            // A misplaced definition's body is skipped with it.
            assert_eq!(texts(&d), ["v1 1 0 1"], "{line}");
        }
        // A mismatched `.endl` name is accepted with a warning.
        let d = read_map("t\n.lib m.lib ok\n", &mut l);
        assert!(errors(&d).is_empty());
        assert!(d.diags[0].message.contains(".endl wrong closes"));
        assert_eq!(d.sources.snippet(d.diags[0].span.unwrap()), ".endl wrong");
    }

    #[test]
    fn nested_section_definition_ends_at_first_endl() {
        let lib = ".lib s\nr1 1 0 1\n.lib inner\nr2 1 0 1\n.endl\nr3 1 0 1\n.endl\n";
        let mut l = MapLoader::new(&[("m.lib", lib)]);
        let d = read_map("t\n.lib m.lib s\n", &mut l);
        let e = errors(&d);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("sections do not nest"));
        assert_eq!(texts(&d), ["r1 1 0 1", "r2 1 0 1"]);
    }

    #[test]
    fn spliced_bytes_are_bounded() {
        // A 256 KiB section requested 2000 times is read once, but
        // splicing it every time would build a 500 MiB deck.
        let big = format!("r1 1 0 1 {}\n", "x".repeat(64 << 10));
        let lib = format!(".lib t\n{big}{big}{big}{big}.endl\n");
        let deck = format!("t\n{}", ".lib m.lib t\n".repeat(2000));
        let mut l = MapLoader {
            files: vec![("m.lib", lib)],
            loads: Vec::new(),
        };
        let d = read_map(&deck, &mut l);
        let e = errors(&d);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("spliced source text"), "{e:?}");
        assert_eq!(l.loads.len(), 1);
    }

    #[test]
    fn indented_and_quoted_include() {
        let mut l = MapLoader::new(&[("dir/a b.inc", "ra 1 0 1\n")]);
        let d = read_map(
            "t\n   .INCLUDE \"dir/a b.inc\"\n\t.inc 'dir/a b.inc'\n",
            &mut l,
        );
        assert!(d.diags.is_empty(), "{:?}", d.diags);
        assert_eq!(texts(&d), ["ra 1 0 1", "ra 1 0 1"]);
    }

    #[test]
    fn spans_survive_continuation_joins() {
        let d = read("t\nr1 1 0\n+ 1k\n");
        let card = &d.cards[0];
        // "1k" sits in the continuation segment.
        let pos = card.text.find("1k").unwrap();
        let span = card.span_of(pos, pos + 2);
        assert_eq!(d.sources.snippet(span), "1k");
        // "r1" sits in the first segment.
        let span = card.span_of(0, 2);
        assert_eq!(d.sources.snippet(span), "r1");
    }

    #[test]
    fn multibyte_line_start_does_not_panic() {
        // `starts_with_ignore_case` must not slice mid-character.
        let d = read("t\nréseau 1 0 1k\n\u{e9}\u{e9}\u{e9} x y\n");
        assert_eq!(d.cards.len(), 2);
    }

    #[test]
    fn loader_budget_shrinks_with_each_include() {
        struct Probe(Vec<u64>);
        impl IncludeLoader for Probe {
            fn load(&mut self, _: &Path, limit: u64) -> Result<String, String> {
                self.0.push(limit);
                Ok("rinc 1 0 1k\n".into())
            }
        }
        let mut probe = Probe(Vec::new());
        let d = read_with_loader(
            Path::new("test.cir"),
            "t\n.include a.cir\n.include b.cir\n".into(),
            &mut probe,
        );
        assert_eq!(d.cards.len(), 2);
        assert_eq!(probe.0.len(), 2);
        // The second include's budget is smaller than the first's.
        assert!(probe.0[1] < probe.0[0]);
    }

    #[test]
    fn crlf_is_stripped() {
        let d = read("t\r\nr1 1 0 1k\r\n");
        assert_eq!(d.title, "t");
        assert_eq!(d.cards[0].text, "r1 1 0 1k");
    }
}
