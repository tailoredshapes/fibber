//! S-expressions: the lexical layer of spec/lir.md §1.

mod lexer;
mod reader;

pub use reader::read;

use crate::diag::Pos;

/// Lists nested deeper than this are an error (spec/lir.md §1).
pub const MAX_DEPTH: usize = 512;

/// One form of the source text.
#[derive(Clone, Debug, PartialEq)]
pub enum Sexp {
    /// `( … )`
    List(Vec<Sexp>, Pos),
    /// `{ … }`
    Brace(Vec<Sexp>, Pos),
    /// A symbol, number, `@name` or `%struct.name`.
    Atom(String, Pos),
    /// `"…"`, its bytes after escapes.
    Str(Vec<u8>, Pos),
    /// `<N x T>`, normalised to `N` and the element's text.
    VecType(String, String, Pos),
}

impl Sexp {
    pub fn pos(&self) -> Pos {
        match self {
            Sexp::List(_, p)
            | Sexp::Brace(_, p)
            | Sexp::Atom(_, p)
            | Sexp::Str(_, p)
            | Sexp::VecType(_, _, p) => *p,
        }
    }

    /// The atom's text, if this is an atom.
    pub fn atom(&self) -> Option<&str> {
        match self {
            Sexp::Atom(a, _) => Some(a),
            _ => None,
        }
    }

    /// A short description for error messages: the atom, or the head
    /// of a list.
    pub fn describe(&self) -> String {
        match self {
            Sexp::Atom(a, _) => a.clone(),
            Sexp::List(items, _) => match items.first() {
                Some(h) => h.describe(),
                None => "()".into(),
            },
            Sexp::Brace(..) => "{ … }".into(),
            Sexp::Str(..) => "a string".into(),
            Sexp::VecType(n, e, _) => format!("<{n} x {e}>"),
        }
    }
}
