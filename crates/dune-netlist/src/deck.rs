//! Physical deck reading: files → logical cards.
//!
//! Mirrors `inp_readall` in `corpus/spice3f5/src/lib/fte/inpcom.c`:
//! the first non-blank line of the root file is the title; `.include`
//! at column 1 splices another file in before anything else happens;
//! then whole-line comments are dropped and `+` continuation lines are
//! joined (with a single space) onto the previous card — comments in
//! between do not break a continuation chain, and the title line can
//! never be continued.
//!
//! Deliberate differences from the C, all documented in
//! `docs/dune-netlist.md`:
//! - relative `.include` paths resolve against the including file's
//!   directory (the C uses the process working directory);
//! - `$` and `;` preceded by whitespace start an end-of-line comment
//!   (ngspice superset; the C only knows whole-line comments);
//! - trailing `\r` is stripped (DOS decks);
//! - include depth and total size are bounded so hostile decks cannot
//!   hang or exhaust memory (principle 4 of `CLAUDE.md`).

use std::path::{Path, PathBuf};

use crate::diag::Diagnostic;
use crate::span::{SourceMap, Span};

/// Maximum `.include` nesting; also what bounds include cycles.
const MAX_INCLUDE_DEPTH: usize = 16;
/// Cap on the total bytes loaded across all files of one deck.
const MAX_TOTAL_BYTES: usize = 256 << 20;

/// Loads the text of an `.include`d file. The default implementation
/// reads the file system; a WASM host can supply its own.
pub trait IncludeLoader {
    fn load(&mut self, path: &Path) -> Result<String, String>;
}

/// [`IncludeLoader`] over `std::fs`.
pub struct FsLoader;

impl IncludeLoader for FsLoader {
    fn load(&mut self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
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
    let text = std::fs::read_to_string(path).map_err(|source| crate::Error::Io {
        path: path.to_path_buf(),
        message: source.to_string(),
    })?;
    Ok(read_with_loader(path, text, &mut FsLoader))
}

/// Read a deck from a string, resolving `.include` with `loader`
/// (relative to `name`'s directory).
pub fn read_with_loader(name: &Path, text: String, loader: &mut dyn IncludeLoader) -> Deck {
    let mut r = Reader {
        sources: SourceMap::new(),
        physical: Vec::new(),
        diags: Vec::new(),
        loaded_bytes: 0,
        loader,
    };
    r.load(name.to_path_buf(), text, 0);
    r.finish()
}

/// A physical line that survived file reading and include splicing.
struct Physical {
    text: String,
    src: Span,
}

struct Reader<'a> {
    sources: SourceMap,
    physical: Vec<Physical>,
    diags: Vec<Diagnostic>,
    loaded_bytes: usize,
    loader: &'a mut dyn IncludeLoader,
}

impl Reader<'_> {
    /// Register `text` as a file and splice its lines (and includes)
    /// into `physical`.
    fn load(&mut self, name: PathBuf, text: String, depth: usize) {
        self.loaded_bytes = self.loaded_bytes.saturating_add(text.len());
        if self.loaded_bytes > MAX_TOTAL_BYTES {
            self.diags.push(Diagnostic::global_error(format!(
                "deck exceeds {} MiB of source text; giving up",
                MAX_TOTAL_BYTES >> 20
            )));
            return;
        }
        let Some(file) = self.sources.add(name.clone(), text) else {
            self.diags
                .push(Diagnostic::global_error("source file too large"));
            return;
        };
        // Walk the file's lines by byte offset so spans are exact.
        let text = &self.sources.file(file).text;
        let mut line_ranges = Vec::new();
        let mut start = 0usize;
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_ranges.push((start, i));
                start = i + 1;
            }
        }
        if start < text.len() {
            line_ranges.push((start, text.len()));
        }

        for (s, mut e) in line_ranges {
            let text = &self.sources.file(file).text;
            // Strip trailing '\r' (DOS decks) and trailing blanks.
            while e > s && matches!(text.as_bytes()[e - 1], b'\r' | b' ' | b'\t') {
                e -= 1;
            }
            // Blank lines never become cards (the C's readline folds
            // them away).
            if text[s..e].trim().is_empty() {
                continue;
            }
            // `.include` is only recognized at column 1, like the C.
            if starts_with_ignore_case(&text[s..e], ".include") {
                let line = text[s..e].to_string();
                let src = Span::new(file, s, e);
                self.handle_include(&line, src, &name, depth);
                continue;
            }
            // End-of-line comments: `$` or `;` at the start of a
            // field (ngspice superset). The title line is printed
            // verbatim, so it is never stripped.
            let is_title = depth == 0 && self.physical.is_empty();
            if !is_title {
                let cut = inline_comment_start(&text[s..e]);
                e = s + cut;
                while e > s && matches!(text.as_bytes()[e - 1], b' ' | b'\t') {
                    e -= 1;
                }
            }
            if text[s..e].trim().is_empty() {
                continue;
            }
            // Re-borrow immutably for the push below.
            let line = self.sources.file(file).text[s..e].to_string();
            self.physical.push(Physical {
                text: line,
                src: Span::new(file, s, e),
            });
        }
    }

    fn handle_include(&mut self, line: &str, src: Span, includer: &Path, depth: usize) {
        if depth >= MAX_INCLUDE_DEPTH {
            self.diags.push(Diagnostic::error(
                format!(".include nested deeper than {MAX_INCLUDE_DEPTH} levels (cycle?)"),
                src,
            ));
            return;
        }
        let mut fields = line.split_whitespace();
        fields.next(); // `.include`
        let Some(fname) = fields.next() else {
            self.diags
                .push(Diagnostic::error(".include filename missing", src));
            return;
        };
        let path = {
            let p = Path::new(fname);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                includer.parent().unwrap_or(Path::new(".")).join(p)
            }
        };
        match self.loader.load(&path) {
            Ok(text) => self.load(path, text, depth + 1),
            Err(e) => self.diags.push(Diagnostic::error(
                format!("cannot open .include file {}: {e}", path.display()),
                src,
            )),
        }
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

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
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
        fn load(&mut self, _: &Path) -> Result<String, String> {
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

    #[test]
    fn include_depth_is_bounded() {
        struct SelfLoader;
        impl IncludeLoader for SelfLoader {
            fn load(&mut self, _: &Path) -> Result<String, String> {
                Ok(".include self.cir\n".into())
            }
        }
        let d = read_with_loader(
            Path::new("test.cir"),
            "t\n.include self.cir\n".into(),
            &mut SelfLoader,
        );
        assert!(d.diags.iter().any(|g| g.message.contains("nested deeper")));
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
    fn crlf_is_stripped() {
        let d = read("t\r\nr1 1 0 1k\r\n");
        assert_eq!(d.title, "t");
        assert_eq!(d.cards[0].text, "r1 1 0 1k");
    }
}
