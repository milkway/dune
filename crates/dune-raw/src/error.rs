/// Rawfile reading errors.
///
/// Fatal conditions mirror the 3f5 reader: a non-blank line that is not a
/// recognized keyword aborts the load ("strange line in rawfile"), and so
/// does truncated or unparsable data. Recoverable oddities become
/// [`crate::Rawfile::warnings`] instead.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("line {line}: {msg}")]
    Parse { line: usize, msg: String },
}

impl Error {
    pub(crate) fn parse(line: usize, msg: impl Into<String>) -> Self {
        Error::Parse {
            line,
            msg: msg.into(),
        }
    }
}
