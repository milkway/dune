//! Source positions for diagnostics.
//!
//! A deck may pull in other files through `.include`, so a [`Span`]
//! names a file (by [`FileId`] into the [`SourceMap`]) plus a byte
//! range in that file's text. Line/column resolution is done lazily,
//! only when a diagnostic is rendered.

use std::path::{Path, PathBuf};

/// Index of a file inside a [`SourceMap`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId(pub(crate) u32);

/// A byte range inside one source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub(crate) fn new(file: FileId, start: usize, end: usize) -> Self {
        // Files larger than 4 GiB are rejected at load time, so the
        // casts cannot truncate.
        Span {
            file,
            start: start as u32,
            end: end as u32,
        }
    }

    /// A span covering both `self` and `other` when they are in the
    /// same file; otherwise `self` (good enough for diagnostics).
    pub fn to(self, other: Span) -> Span {
        if self.file == other.file {
            Span {
                file: self.file,
                start: self.start.min(other.start),
                end: self.end.max(other.end),
            }
        } else {
            self
        }
    }
}

/// One loaded source file.
#[derive(Debug)]
pub struct SourceFile {
    pub name: PathBuf,
    pub text: String,
    /// Byte offset of the start of each line, computed on demand.
    line_starts: std::cell::OnceCell<Vec<u32>>,
}

/// All files that took part in a parse (root deck + includes).
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

/// A resolved human-readable position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    /// 1-based.
    pub line: u32,
    /// 1-based byte column.
    pub column: u32,
}

impl SourceMap {
    pub fn new() -> Self {
        SourceMap::default()
    }

    /// Register a file's text; its bytes are addressed by the returned
    /// [`FileId`]. Fails for texts of 4 GiB or more (spans are `u32`).
    pub(crate) fn add(&mut self, name: impl Into<PathBuf>, text: String) -> Option<FileId> {
        if text.len() >= u32::MAX as usize || self.files.len() >= u32::MAX as usize {
            return None;
        }
        let id = FileId(self.files.len() as u32);
        self.files.push(SourceFile {
            name: name.into(),
            text,
            line_starts: std::cell::OnceCell::new(),
        });
        Some(id)
    }

    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.0 as usize]
    }

    /// The source text a span points at.
    pub fn snippet(&self, span: Span) -> &str {
        let text = &self.file(span.file).text;
        text.get(span.start as usize..span.end as usize)
            .unwrap_or("")
    }

    /// Resolve a span's start to file/line/column.
    pub fn locate(&self, span: Span) -> Location {
        let file = self.file(span.file);
        let starts = file.line_starts.get_or_init(|| {
            let mut v = vec![0u32];
            for (i, b) in file.text.bytes().enumerate() {
                if b == b'\n' {
                    v.push(i as u32 + 1);
                }
            }
            v
        });
        let line = match starts.binary_search(&span.start) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        Location {
            file: file.name.clone(),
            line: line as u32 + 1,
            column: span.start - starts[line] + 1,
        }
    }
}

impl SourceFile {
    pub fn path(&self) -> &Path {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_lines_and_columns() {
        let mut map = SourceMap::new();
        let id = map.add("t.cir", "title\nr1 1 0 1k\n.end\n".into()).unwrap();
        let loc = map.locate(Span::new(id, 6, 8));
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 1);
        let loc = map.locate(Span::new(id, 9, 10));
        assert_eq!(loc.line, 2);
        assert_eq!(loc.column, 4);
        assert_eq!(map.snippet(Span::new(id, 6, 8)), "r1");
    }

    #[test]
    fn locate_at_file_start_and_end() {
        let mut map = SourceMap::new();
        let id = map.add("t.cir", "a\nb".into()).unwrap();
        assert_eq!(map.locate(Span::new(id, 0, 1)).line, 1);
        let loc = map.locate(Span::new(id, 2, 3));
        assert_eq!((loc.line, loc.column), (2, 1));
    }
}
