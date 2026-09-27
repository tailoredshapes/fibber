//! The fibber reader: text to [`Form`]s (spec/syntax.md §1, §3.16).
//!
//! [`read_all`] reads every top-level form of a source text. It knows
//! nothing about special forms (§1): `x: T` reads as the two symbols
//! `x:` and `T`, `-> T` as `->` and `T`, `:borrow` as a keyword, `&v` as
//! `(& v)`, and `[..]`/`{..}` as `Vec`/`Map` forms (the rewrite of §1.4
//! happens after expansion, not here).
//!
//! Errors are values ([`ReadError`]), never panics, each with a
//! [`Pos`]. Reading uses an explicit stack capped at [`MAX_DEPTH`], so
//! no input overflows the native stack. `Display` on a [`Form`] prints
//! text that reads back to an equal form.
//!
//! Where §1 is silent this reader chooses (each pinned by a test):
//! CR is whitespace and `\r\n` one line end; a BOM is skipped at offset
//! 0 only; control characters and Unicode whitespace other than space,
//! tab, LF, CR are errors outside strings and comments; `'` and `` ` ``
//! may be separated from their form by whitespace or comments, while
//! `@`, `&`, `,` and `,@` must touch it; `&` not followed by a form is
//! the reserved symbol `&`, and `&` applied to a non-symbol is a read
//! error; a run of commas followed by a form is nested unquotes; hex and
//! binary literals are values, so `0xFFi8` is out of range; `_` may only
//! stand between two digits; `1f32` is invalid (no `.` or exponent); a
//! float that overflows its width is an error, one that underflows
//! rounds; `\xNN` is at most `7F`; `\u{..}` takes one to six digits.

mod chars;
mod cursor;
mod error;
mod form;
mod lexer;
mod literal;
mod number;
mod pos;
mod print;
mod reader;

#[cfg(test)]
mod tests;

pub use error::{ReadError, ReadErrorKind};
pub use form::{FltWidth, Form, FormKind, IntWidth};
pub use number::check_literal;
pub use pos::Pos;
pub use reader::MAX_DEPTH;

/// Reads every top-level form of `source`, attributing positions to
/// `file`. Stops at the first error.
pub fn read_all(source: &str, file: &str) -> Result<Vec<Form>, ReadError> {
    reader::read_all(source, file)
}
