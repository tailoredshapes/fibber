//! Character classes of spec/syntax.md §1.1 and §1.2.

use super::cursor::BOM;

/// Whitespace that separates tokens: space, tab, newline (§1.1), and CR
/// so that CRLF line endings read like LF ones.
pub(crate) fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Characters rejected outside strings and comments: control characters
/// and Unicode whitespace other than [`is_space`], and a BOM after the
/// start of the text. §1.1 lists only space, tab and newline as
/// whitespace, so these would otherwise be invisible symbol constituents.
pub(crate) fn is_forbidden(c: char) -> bool {
    ((c.is_control() || c.is_whitespace()) && !is_space(c)) || c == BOM
}

/// The characters §1.1 excludes from symbols besides whitespace:
/// `( ) [ ] { } " ; ' `` ` `` , @ \`.
pub(crate) fn is_terminating(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '[' | ']' | '{' | '}' | '"' | ';' | '\'' | '`' | ',' | '@' | '\\'
    )
}

/// A character that may continue a symbol, keyword or number token.
pub(crate) fn is_constituent(c: char) -> bool {
    !is_space(c) && !is_terminating(c) && !is_forbidden(c)
}

/// §1.2: "a character that can start a form", other than a comma:
/// anything but whitespace, a closing delimiter, `;`, a forbidden
/// character or the end. Commas are decided by the lexer
/// (`Lexer::starts_form`), since a comma starts a form exactly when it
/// is an unquote, that is when what follows it starts a form.
pub(crate) fn starts_form_char(c: Option<char>) -> bool {
    match c {
        Some(c) => !is_space(c) && !is_forbidden(c) && !matches!(c, ')' | ']' | '}' | ';'),
        None => false,
    }
}

/// §1.1: `/` once between two non-empty parts, or `/` alone.
pub(crate) fn valid_slashes(name: &str) -> bool {
    if name == "/" {
        return true;
    }
    match name.split_once('/') {
        None => true,
        Some((ns, rest)) => !ns.is_empty() && !rest.is_empty() && !rest.contains('/'),
    }
}
