//! String and character literals (§1.1): every escape and name.

use super::{err, kind};
use crate::syntax::{FormKind, ReadErrorKind};

fn s(v: &str) -> FormKind {
    FormKind::Str(v.to_string())
}

fn bad_escape(src: &str) -> (String, &'static str) {
    match err(src) {
        ReadErrorKind::BadEscape { text, reason } => (text, reason),
        other => panic!("{src}: expected BadEscape, got {other:?}"),
    }
}

fn bad_unicode(src: &str) -> String {
    match err(src) {
        ReadErrorKind::BadUnicodeEscape { text, .. } => text,
        other => panic!("{src}: expected BadUnicodeEscape, got {other:?}"),
    }
}

#[test]
fn every_string_escape() {
    assert_eq!(
        kind(r#""\n\t\r\0\\\"\x41\u{48}\u{1F600}""#),
        s("\n\t\r\0\\\"AH😀")
    );
    assert_eq!(kind(r#""\x00\x7F\x7f""#), s("\0\u{7F}\u{7F}"));
    assert_eq!(kind(r#""\u{0}\u{10FFFF}\u{00000A}""#), s("\0\u{10FFFF}\n"));
    assert_eq!(kind("\"\""), s(""));
    assert_eq!(kind("\"é 数 😀\""), s("é 数 😀"));
    assert_eq!(kind("\"a\nb\""), s("a\nb"));
    assert_eq!(kind("\"a\r\nb\""), s("a\r\nb"));
    assert_eq!(kind("\"; not a comment\""), s("; not a comment"));
}

#[test]
fn bad_string_escapes() {
    assert_eq!(bad_escape(r#""\q""#).0, "\\q");
    assert_eq!(bad_escape(r#""\'""#).0, "\\'");
    assert_eq!(bad_escape(r#""\a""#).0, "\\a");
    assert_eq!(bad_escape(r#""\x4""#).0, "\\x4");
    assert_eq!(bad_escape(r#""\xZZ""#).0, "\\x");
    let (text, reason) = bad_escape(r#""\x80""#);
    assert_eq!(text, "\\x80");
    assert!(reason.contains("at most 7F"));
    assert_eq!(bad_escape(r#""\xFF""#).0, "\\xFF");
}

#[test]
fn bad_unicode_escapes() {
    let no_brace = concat!("\"\\u", "0041\"");
    assert_eq!(bad_unicode(no_brace), "\\u");
    assert_eq!(bad_unicode(r#""\u{}""#), "\\u{}");
    assert_eq!(bad_unicode(r#""\u{1234567}""#), "\\u{1234567}");
    assert_eq!(bad_unicode(r#""\u{D800}""#), "\\u{D800}");
    assert_eq!(bad_unicode(r#""\u{110000}""#), "\\u{110000}");
    assert_eq!(bad_unicode(r#""\u{12""#), "\\u{12");
    assert_eq!(bad_unicode(r#""\u{12G}""#), "\\u{12");
    assert_eq!(bad_unicode(r"\u{D800}"), "\\u{D800}");
}

#[test]
fn unterminated_strings() {
    for src in ["\"abc", "\"", "\"abc\\", "\"abc\\\"", "(f \"x)"] {
        assert_eq!(err(src), ReadErrorKind::UnterminatedString, "{src}");
    }
}

#[test]
fn every_character_spelling() {
    let cases = [
        (r"\a", 'a'),
        (r"\\", '\\'),
        (r"\newline", '\n'),
        (r"\space", ' '),
        (r"\tab", '\t'),
        (r"\return", '\r'),
        (r"\u{1F600}", '😀'),
        (r"\u{41}", 'A'),
        (r"\u", 'u'),
        (r"\n", 'n'),
        (r"\(", '('),
        (r"\)", ')'),
        (r#"\""#, '"'),
        (r"\;", ';'),
        (r"\,", ','),
        (r"\@", '@'),
        (r"\'", '\''),
        (r"\#", '#'),
        (r"\:", ':'),
        (r"\1", '1'),
        (r"\é", 'é'),
        (r"\数", '数'),
    ];
    for (src, c) in cases {
        assert_eq!(kind(src), FormKind::Chr(c), "{src}");
    }
    assert_eq!(super::show(r"(\a\b)"), r"(\a \b)");
    assert_eq!(super::show(r"[\( \)]"), r"[\( \)]");
}

#[test]
fn bad_character_literals() {
    for src in [r"\", r"\ab", r"\newlines", r"\12", "\\ ", "\\\n", r"\x41"] {
        match err(src) {
            ReadErrorKind::BadCharLiteral(_) => {}
            other => panic!("{src:?}: {other:?}"),
        }
    }
    assert_eq!(err("\\\u{7}"), ReadErrorKind::InvalidCharacter('\u{7}'));
}
