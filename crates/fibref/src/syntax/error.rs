//! Read errors. Every malformed input is one of these values, with the
//! position of the offending text; the reader never panics on input.

use std::fmt;

use super::form::{FltWidth, IntWidth};
use super::pos::Pos;

/// A read error: what went wrong and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadError {
    /// What went wrong.
    pub kind: ReadErrorKind,
    /// The offending text (for an unclosed delimiter or string, where it
    /// was opened).
    pub pos: Pos,
}

/// Every way reading can fail (spec/syntax.md §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadErrorKind {
    /// A `"` with no closing `"` before the end of input (§1.1 string).
    UnterminatedString,
    /// A `\` escape in a string that §1.1 does not list, or a malformed
    /// `\xNN` (`text` is the escape as written).
    BadEscape {
        /// The escape as written, e.g. `\q`.
        text: String,
        /// Why it is not accepted.
        reason: &'static str,
    },
    /// A malformed `\u{...}` in a string or character literal.
    BadUnicodeEscape {
        /// The escape as written.
        text: String,
        /// Why it is not accepted.
        reason: &'static str,
    },
    /// A character literal §1.1 does not define (`\foo`, `\` at the end).
    BadCharLiteral(String),
    /// A character that may not appear outside strings and comments
    /// (control characters other than tab, LF, CR; Unicode whitespace
    /// other than space; a BOM after the start).
    InvalidCharacter(char),
    /// A token that starts like a number (a digit, or `-` and a digit) but
    /// is not one of §1.1's integer or float spellings.
    InvalidNumber {
        /// The token.
        text: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// An integer literal that does not fit its width (§1.1: "a read
    /// error, never a wrap").
    IntegerOutOfRange {
        /// The token.
        text: String,
        /// The width it had to fit.
        width: IntWidth,
    },
    /// A float literal too large for its width (it would be infinite).
    FloatOutOfRange {
        /// The token.
        text: String,
        /// The width it had to fit.
        width: FltWidth,
    },
    /// A symbol with `/` other than exactly once between two non-empty
    /// parts, or alone.
    InvalidSymbol(String),
    /// A keyword with an empty or malformed name (`:`, `::a`, `:a/`).
    InvalidKeyword(String),
    /// `#` followed by anything but `_`, `(` or `{` (the dispatches §1.2
    /// defines).
    UnknownDispatch(Option<char>),
    /// `#(` inside the body of another `#(` (§1.2).
    NestedFn,
    /// `%&` in a `#(` body: rest parameters are not read yet (§1.2).
    UnsupportedRest,
    /// A `%N` in a `#(` body that is not a parameter: `%0` or an index
    /// with a leading zero, or one above 255 (§1.2).
    BadFnParam {
        /// The symbol as written.
        text: String,
        /// Why it is not accepted.
        reason: &'static str,
    },
    /// A prefix reader macro (§1.2) with no form after it; for `@`, `&`,
    /// `~` and `~@`, also one not immediately followed by a form.
    PrefixWithoutForm(&'static str),
    /// `#_` with no form after it before a closing delimiter or the end.
    DiscardWithoutForm,
    /// `&` applied to a form that is not a symbol (§1.2: "`x` must be a
    /// symbol").
    InOutNotSymbol,
    /// An opening delimiter with no matching close before the end.
    Unclosed {
        /// The opening delimiter.
        open: char,
    },
    /// A closing delimiter with nothing open.
    UnexpectedClose {
        /// The closing delimiter.
        found: char,
    },
    /// A closing delimiter of the wrong kind.
    MismatchedClose {
        /// The delimiter that is open.
        open: char,
        /// Line of the open delimiter.
        open_line: usize,
        /// Column of the open delimiter.
        open_col: usize,
        /// The closing delimiter found.
        found: char,
    },
    /// A map literal with an odd number of forms (§1.1).
    OddMapEntries {
        /// How many forms it had.
        count: usize,
    },
    /// Nesting deeper than [`MAX_DEPTH`](super::MAX_DEPTH).
    TooDeep {
        /// The limit.
        limit: usize,
    },
}

impl fmt::Display for ReadErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ReadErrorKind as K;
        match self {
            K::UnterminatedString => write!(f, "unterminated string"),
            K::BadEscape { text, reason } => write!(f, "bad escape {text} in string: {reason}"),
            K::BadUnicodeEscape { text, reason } => write!(f, "bad unicode escape {text}: {reason}"),
            K::BadCharLiteral(t) => write!(
                f,
                "bad character literal {t}: expected \\c for one character, \\newline, \\space, \\tab, \\return or \\u{{HHHH}}"
            ),
            K::InvalidCharacter(c) => {
                write!(f, "invalid character U+{:04X} outside a string or comment", u32::from(*c))
            }
            K::InvalidNumber { text, reason } => write!(f, "invalid number {text}: {reason}"),
            K::IntegerOutOfRange { text, width } => {
                let (lo, hi) = width.range();
                let w = width.suffix();
                write!(f, "integer literal {text} does not fit {w} ({lo} to {hi})")
            }
            K::FloatOutOfRange { text, width } => {
                write!(f, "float literal {text} is too large for {}", width.suffix())
            }
            K::InvalidSymbol(s) => {
                write!(f, "invalid symbol {s}: `/` may appear once, between a namespace and a name, or alone")
            }
            K::InvalidKeyword(s) => write!(f, "invalid keyword {s}"),
            K::UnknownDispatch(Some(c)) => write!(f, "unknown reader syntax #{c}: only #_, #(, #{{ are defined"),
            K::UnknownDispatch(None) => write!(f, "unknown reader syntax # at end of input"),
            K::NestedFn => write!(f, "#( may not be nested inside #("),
            K::UnsupportedRest => write!(f, "%& (a rest parameter) is not supported in #("),
            K::BadFnParam { text, reason } => write!(f, "bad parameter {text} in #(: {reason}"),
            K::PrefixWithoutForm(p) => write!(f, "{p} must be followed immediately by a form"),
            K::DiscardWithoutForm => write!(f, "#_ must be followed by a form"),
            K::InOutNotSymbol => write!(f, "& must be followed by a symbol"),
            K::Unclosed { open } => write!(f, "unclosed {open}"),
            K::UnexpectedClose { found } => write!(f, "unexpected {found} with nothing open"),
            K::MismatchedClose { open, open_line, open_col, found } => write!(
                f,
                "{found} does not close {open} opened at {open_line}:{open_col}"
            ),
            K::OddMapEntries { count } => {
                write!(f, "map literal has {count} forms; keys and values must pair up")
            }
            K::TooDeep { limit } => write!(f, "forms nested deeper than {limit} levels"),
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.pos, self.kind)
    }
}

impl std::error::Error for ReadError {}
