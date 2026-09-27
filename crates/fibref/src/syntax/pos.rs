//! Source positions (spec/syntax.md §1.3).

use std::fmt;
use std::sync::Arc;

/// Where a form, or a read error, is in the source text.
///
/// Representation (the spec asks only for "file, line, column"; the rest
/// is this implementation's choice):
///
/// - `file` is the name passed to [`read_all`](super::read_all), shared
///   between all positions of one read;
/// - `line` and `col` are 1-based and locate the first character of the
///   form; `col` counts Unicode scalar values from the start of the line
///   (a tab, a CR and a multi-byte character each count as one column);
///   a line ends after `\n`, so `\r\n` ends a line once and a lone `\r`
///   does not end one;
/// - `start..end` is the half-open byte range of the form's text in the
///   source string exactly as it was passed in (a leading UTF-8 BOM is
///   counted in the offsets but not in the column).
///
/// The span of a list, vector or map runs from its opening to its closing
/// delimiter; the span of a prefix form (`'x`, `@x`, ...) runs from the
/// prefix character through the end of the form it applies to, and the
/// synthesised head symbol (`quote`, `deref`, ...) gets the span of the
/// prefix characters alone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    /// The file name given to the reader.
    pub file: Arc<str>,
    /// 1-based line of the first character.
    pub line: usize,
    /// 1-based column of the first character, in Unicode scalar values.
    pub col: usize,
    /// Byte offset of the first byte.
    pub start: usize,
    /// Byte offset one past the last byte.
    pub end: usize,
}

impl Pos {
    /// A position covering `self` through the end of `last`.
    pub fn through(&self, last: &Pos) -> Pos {
        Pos {
            end: last.end,
            ..self.clone()
        }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}
