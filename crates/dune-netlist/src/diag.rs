//! Structured diagnostics (the rpic philosophy, CLAUDE.md §Princípios 4):
//! malformed input produces an error or warning with a span, never a
//! panic. The parser keeps going after errors so one pass reports as
//! much as possible; callers decide whether errors are fatal.

use crate::span::{SourceMap, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The deck is unusable as written (3f5 would refuse or mangle it).
    Error,
    /// Suspicious but recoverable; parsing continued.
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    /// Where in the sources; `None` only for deck-level problems
    /// (empty file, missing `.end`, …).
    pub span: Option<Span>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn warning(message: impl Into<String>, span: Span) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn global_error(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span: None,
        }
    }

    pub fn global_warning(message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span: None,
        }
    }

    /// Render as `file:line:col: severity: message` (or without the
    /// position for deck-level diagnostics).
    pub fn render(&self, sources: &SourceMap) -> String {
        let sev = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match self.span {
            Some(span) => {
                let loc = sources.locate(span);
                format!(
                    "{}:{}:{}: {sev}: {}",
                    loc.file.display(),
                    loc.line,
                    loc.column,
                    self.message
                )
            }
            None => format!("{sev}: {}", self.message),
        }
    }
}
