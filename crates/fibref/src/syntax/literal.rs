//! String and character literals (spec/syntax.md §1.1).
//!
//! Strings: any characters up to the closing `"` (raw newlines included,
//! kept byte for byte, so a CRLF inside a string stays `\r\n`), with the
//! escapes `\n \t \r \0 \\ \" \xNN \u{H..}`. `\xNN` must be at most `7F`:
//! a string is UTF-8 and a lone byte above that is not a character.
//! `\u{..}` takes one to six hex digits naming a Unicode scalar value.
//!
//! Characters: `\` followed by one character, which is taken literally
//! (`\(`, `\"`, `\\`, `\;` are the characters themselves); when that
//! character is a symbol constituent the whole constituent run is the
//! literal, so it must be one character or one of the names `newline`,
//! `space`, `tab`, `return`; `\u{..}` is as in strings.

use super::chars::{is_constituent, is_forbidden, is_space};
use super::cursor::{Cursor, Mark};
use super::error::{ReadError, ReadErrorKind};
use super::form::FormKind;

fn error(cur: &Cursor<'_>, mark: Mark, kind: ReadErrorKind) -> ReadError {
    ReadError {
        kind,
        pos: cur.pos_from(mark),
    }
}

/// Reads a string literal; the cursor is on the opening `"`.
pub(crate) fn lex_string(cur: &mut Cursor<'_>) -> Result<FormKind, ReadError> {
    let start = cur.mark();
    cur.bump();
    let mut out = String::new();
    loop {
        match cur.peek() {
            None => return Err(error(cur, start, ReadErrorKind::UnterminatedString)),
            Some('"') => {
                cur.bump();
                return Ok(FormKind::Str(out));
            }
            Some('\\') => {
                if let Some(c) = lex_escape(cur)? {
                    out.push(c);
                }
            }
            Some(c) => {
                cur.bump();
                out.push(c);
            }
        }
    }
}

/// Reads one escape; `None` when the input ends right after the `\`.
fn lex_escape(cur: &mut Cursor<'_>) -> Result<Option<char>, ReadError> {
    let mark = cur.mark();
    cur.bump();
    let Some(c) = cur.bump() else {
        return Ok(None);
    };
    let decoded = match c {
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '0' => '\0',
        '\\' => '\\',
        '"' => '"',
        'x' => lex_hex_escape(cur, mark)?,
        'u' => lex_unicode(cur, mark)?,
        _ => {
            let kind = ReadErrorKind::BadEscape {
                text: cur.text_from(mark).to_string(),
                reason: "the escapes are \\n \\t \\r \\0 \\\\ \\\" \\xNN \\u{HHHH}",
            };
            return Err(error(cur, mark, kind));
        }
    };
    Ok(Some(decoded))
}

/// `\xNN` after the `x`: exactly two hex digits, at most `7F`.
fn lex_hex_escape(cur: &mut Cursor<'_>, mark: Mark) -> Result<char, ReadError> {
    let mut value = 0u32;
    for _ in 0..2 {
        match cur.peek().and_then(|c| c.to_digit(16)) {
            Some(d) => {
                cur.bump();
                value = value * 16 + d;
            }
            None => return Err(bad_hex(cur, mark, "\\x takes exactly two hex digits")),
        }
    }
    match char::from_u32(value) {
        Some(c) if value <= 0x7F => Ok(c),
        _ => Err(bad_hex(
            cur,
            mark,
            "\\xNN must be at most 7F, one ASCII byte; write \\u{...} for other characters",
        )),
    }
}

fn bad_hex(cur: &Cursor<'_>, mark: Mark, reason: &'static str) -> ReadError {
    let text = cur.text_from(mark).to_string();
    error(cur, mark, ReadErrorKind::BadEscape { text, reason })
}

/// `\u{H..}` after the `u`: one to six hex digits naming a scalar value.
pub(crate) fn lex_unicode(cur: &mut Cursor<'_>, mark: Mark) -> Result<char, ReadError> {
    let bad = |cur: &Cursor<'_>, reason| {
        let text = cur.text_from(mark).to_string();
        error(cur, mark, ReadErrorKind::BadUnicodeEscape { text, reason })
    };
    if cur.peek() != Some('{') {
        return Err(bad(cur, "expected { after \\u"));
    }
    cur.bump();
    let digits_mark = cur.mark();
    cur.bump_while(|c| c.is_ascii_hexdigit());
    let digits = cur.text_from(digits_mark);
    if cur.peek() != Some('}') {
        return Err(bad(cur, "expected hex digits and a closing }"));
    }
    cur.bump();
    if digits.is_empty() || digits.len() > 6 {
        return Err(bad(cur, "expected one to six hex digits"));
    }
    u32::from_str_radix(digits, 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| {
            bad(
                cur,
                "not a Unicode scalar value (a surrogate, or above 10FFFF)",
            )
        })
}

/// Reads a character literal; the cursor is on the `\`.
pub(crate) fn lex_char(cur: &mut Cursor<'_>) -> Result<FormKind, ReadError> {
    let mark = cur.mark();
    cur.bump();
    let bad = |cur: &Cursor<'_>| {
        let text = cur.text_from(mark).to_string();
        error(cur, mark, ReadErrorKind::BadCharLiteral(text))
    };
    let c = match cur.peek() {
        None => return Err(bad(cur)),
        Some(c) if is_forbidden(c) => {
            let at = cur.mark();
            cur.bump();
            return Err(error(cur, at, ReadErrorKind::InvalidCharacter(c)));
        }
        Some(c) => c,
    };
    cur.bump();
    if is_space(c) {
        return Err(bad(cur));
    }
    if c == 'u' && cur.peek() == Some('{') {
        return lex_unicode(cur, mark).map(FormKind::Chr);
    }
    if !is_constituent(c) {
        return Ok(FormKind::Chr(c));
    }
    cur.bump_while(is_constituent);
    // Cannot panic: the text starts with the one-byte `\`.
    let name = &cur.text_from(mark)[1..];
    let named = match name {
        "newline" => '\n',
        "space" => ' ',
        "tab" => '\t',
        "return" => '\r',
        _ if name.chars().count() == 1 => c,
        _ => return Err(bad(cur)),
    };
    Ok(FormKind::Chr(named))
}
