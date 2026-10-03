//! A character cursor over the source that tracks line, column and byte
//! offset (the representation is documented on [`Pos`]).

use std::sync::Arc;

use super::pos::Pos;

/// A saved cursor location, turned into a [`Pos`] once the end is known.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mark {
    off: usize,
    line: usize,
    col: usize,
}

/// The reading position in one source string.
pub(crate) struct Cursor<'a> {
    src: &'a str,
    off: usize,
    line: usize,
    col: usize,
    file: Arc<str>,
}

/// U+FEFF: skipped once at offset 0, an invalid character anywhere else.
pub(crate) const BOM: char = '\u{FEFF}';

impl<'a> Cursor<'a> {
    /// A cursor at the start of `src`, past a leading byte order mark.
    pub(crate) fn new(src: &'a str, file: &str) -> Self {
        let off = if src.starts_with(BOM) {
            BOM.len_utf8()
        } else {
            0
        };
        Cursor {
            src,
            off,
            line: 1,
            col: 1,
            file: Arc::from(file),
        }
    }

    /// The next character, not consumed.
    pub(crate) fn peek(&self) -> Option<char> {
        self.src[self.off..].chars().next()
    }

    /// The character `n` places ahead (0 is [`Cursor::peek`]).
    pub(crate) fn peek_nth(&self, n: usize) -> Option<char> {
        self.src[self.off..].chars().nth(n)
    }

    /// Consumes and returns the next character.
    pub(crate) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.off += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// Consumes characters while `keep` holds.
    pub(crate) fn bump_while(&mut self, keep: impl Fn(char) -> bool) {
        while self.peek().is_some_and(&keep) {
            self.bump();
        }
    }

    /// The current location.
    pub(crate) fn mark(&self) -> Mark {
        Mark {
            off: self.off,
            line: self.line,
            col: self.col,
        }
    }

    /// The position from `mark` to the current location.
    pub(crate) fn pos_from(&self, mark: Mark) -> Pos {
        Pos {
            file: Arc::clone(&self.file),
            line: mark.line,
            col: mark.col,
            start: mark.off,
            end: self.off,
        }
    }

    /// The source text from `mark` to the current location.
    pub(crate) fn text_from(&self, mark: Mark) -> &'a str {
        &self.src[mark.off..self.off]
    }
}
