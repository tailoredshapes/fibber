//! Diagnostics: a message and the position of the form at fault.

use std::fmt;

/// A 1-based line and column in the source text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

/// One error, printed as `LINE:COL: error: MESSAGE` (spec/lir.md §10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub pos: Pos,
    pub message: String,
}

impl Diagnostic {
    pub fn new(pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            pos,
            message: message.into(),
        }
    }

    /// The diagnostic with a file name in front, as `lair` prints it.
    pub fn in_file(&self, file: &str) -> String {
        format!("{file}:{self}")
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: error: {}",
            self.pos.line, self.pos.col, self.message
        )
    }
}

/// Shorthand for results carrying one diagnostic.
pub type Result<T> = std::result::Result<T, Diagnostic>;

/// An error at `pos`.
pub fn err<T>(pos: Pos, message: impl Into<String>) -> Result<T> {
    Err(Diagnostic::new(pos, message))
}
