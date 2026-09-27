//! The errors of the ownership checker (types §6.14), and of the whole
//! front end when it is run through [`super::check_source`].

use std::fmt;

use crate::syntax::Pos;
use crate::types::TypeError;

/// Which rule of §6.14 an ownership error comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OwnErrorKind {
    /// `variable x passed to more than one & parameter in call to f`
    /// (§6.5, case 12; before typing).
    AmpTwice,
    /// `& parameter in async function: v in f` (§6.9, case 14; before
    /// typing).
    AmpInAsync,
    /// `& parameter captured by escaping closure: v in f` (§6.5, case 18).
    AmpCaptured,
    /// `parameter p of f is declared :borrow but escapes` (§6.4).
    BorrowEscapes,
    /// `implementation of P/m for T makes parameter p escape; the
    /// protocol declares it :borrow` (§6.4).
    ImplEscapes,
}

/// One ownership error: where, which rule, the full message (canonical
/// part first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnError {
    /// Which rule.
    pub kind: OwnErrorKind,
    /// The form the rule is about.
    pub pos: Pos,
    /// The message.
    pub message: String,
}

impl OwnError {
    /// An error of `kind` at `pos`.
    pub fn new(kind: OwnErrorKind, pos: &Pos, message: impl Into<String>) -> Self {
        OwnError {
            kind,
            pos: pos.clone(),
            message: message.into(),
        }
    }
}

impl fmt::Display for OwnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.message)
    }
}

impl std::error::Error for OwnError {}

/// Why [`super::check_source`] failed, in pipeline order.
#[derive(Clone, Debug)]
pub enum CheckError {
    /// The prelude did not read or expand (a bug in `lib/`).
    Prelude(String),
    /// The source did not read.
    Read(String),
    /// The source did not expand.
    Expand(String),
    /// Lowering, resolution or type errors (§3), in order.
    Type(Vec<TypeError>),
    /// Ownership errors (§6): the syntactic `&` checks before typing,
    /// or the ownership pass after it.
    Own(Vec<OwnError>),
    /// The checker's thread could not run or panicked (a bug).
    Internal(String),
}

impl CheckError {
    /// Every message, in order.
    pub fn messages(&self) -> Vec<String> {
        match self {
            CheckError::Prelude(m)
            | CheckError::Read(m)
            | CheckError::Expand(m)
            | CheckError::Internal(m) => vec![m.clone()],
            CheckError::Type(es) => es.iter().map(|e| e.message.clone()).collect(),
            CheckError::Own(es) => es.iter().map(|e| e.message.clone()).collect(),
        }
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::Prelude(m) => write!(f, "prelude: {m}"),
            CheckError::Read(m) => write!(f, "read: {m}"),
            CheckError::Expand(m) => write!(f, "expand: {m}"),
            CheckError::Internal(m) => write!(f, "internal: {m}"),
            CheckError::Type(es) => {
                let lines: Vec<String> = es.iter().map(|e| e.to_string()).collect();
                write!(f, "{}", lines.join("\n"))
            }
            CheckError::Own(es) => {
                let lines: Vec<String> = es.iter().map(|e| e.to_string()).collect();
                write!(f, "{}", lines.join("\n"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn pos() -> Pos {
        Pos {
            file: Arc::from("f.fib"),
            line: 3,
            col: 7,
            start: 0,
            end: 1,
        }
    }

    #[test]
    fn display_names_the_position_then_the_message() {
        let e = OwnError::new(OwnErrorKind::AmpCaptured, &pos(), "text");
        assert_eq!(e.to_string(), "f.fib:3:7: text");
        let all = CheckError::Own(vec![e.clone(), e]);
        assert_eq!(all.messages(), vec!["text".to_string(), "text".to_string()]);
    }
}
