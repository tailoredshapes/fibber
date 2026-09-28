//! Errors of the pipeline: the checker's diagnostics, or a failure of
//! LLVM, the JIT or the linker.

use std::fmt;

use lir::Diagnostic;

#[derive(Debug)]
pub enum Error {
    /// The module is not valid lIR (spec/lir.md §10).
    Invalid(Vec<Diagnostic>),
    /// LLVM rejected a module lIR's checker accepted: a checker bug.
    Internal(String),
    /// The JIT failed to add a module or find a symbol.
    Jit(String),
    /// Emitting code, writing a file or linking failed.
    Backend(String),
}

impl Error {
    /// Render with a file name in front of each diagnostic.
    pub fn render(&self, file: &str) -> String {
        match self {
            Error::Invalid(ds) => ds
                .iter()
                .map(|d| d.in_file(file))
                .collect::<Vec<_>>()
                .join("\n"),
            other => format!("{file}: {other}"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Invalid(ds) => {
                let lines: Vec<String> = ds.iter().map(|d| d.to_string()).collect();
                f.write_str(&lines.join("\n"))
            }
            Error::Internal(m) => write!(
                f,
                "internal error: LLVM verifier rejected a checked module: {m}"
            ),
            Error::Jit(m) => write!(f, "error: jit: {m}"),
            Error::Backend(m) => write!(f, "error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<Diagnostic> for Error {
    fn from(d: Diagnostic) -> Self {
        Error::Invalid(vec![d])
    }
}

pub type Result<T> = std::result::Result<T, Error>;
