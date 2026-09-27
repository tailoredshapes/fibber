//! Every [`ReadErrorKind`], each with the position it reports and the
//! text of its message.

use crate::syntax::{read_all, ReadError, ReadErrorKind, MAX_DEPTH};

fn error(src: &str) -> ReadError {
    match read_all(src, "e.fib") {
        Ok(forms) => panic!("{src:?} read as {forms:?}"),
        Err(e) => e,
    }
}

/// Asserts the kind, the `line:col`, the spanned text and the message.
fn check(src: &str, kind: ReadErrorKind, at: (usize, usize), text: &str, message: &str) {
    let e = error(src);
    assert_eq!(e.kind, kind, "{src:?}");
    assert_eq!((e.pos.line, e.pos.col), at, "{src:?}");
    assert_eq!(&src[e.pos.start..e.pos.end], text, "{src:?}");
    let shown = e.to_string();
    let prefix = format!("e.fib:{}:{}: ", at.0, at.1);
    assert!(shown.starts_with(&prefix), "{shown}");
    assert!(shown.contains(message), "{shown:?} lacks {message:?}");
}

#[test]
fn string_errors() {
    check(
        "(a\n \"abc",
        ReadErrorKind::UnterminatedString,
        (2, 2),
        "\"abc",
        "unterminated string",
    );
    let bad = ReadErrorKind::BadEscape {
        text: "\\q".into(),
        reason: "the escapes are \\n \\t \\r \\0 \\\\ \\\" \\xNN \\u{HHHH}",
    };
    check(" \"a\\qb\"", bad, (1, 4), "\\q", "bad escape \\q in string");
    let bad = ReadErrorKind::BadUnicodeEscape {
        text: "\\u{D800}".into(),
        reason: "not a Unicode scalar value (a surrogate, or above 10FFFF)",
    };
    check(
        "\"\\u{D800}\"",
        bad,
        (1, 2),
        "\\u{D800}",
        "bad unicode escape",
    );
}

#[test]
fn character_errors() {
    check(
        "x \\foo",
        ReadErrorKind::BadCharLiteral("\\foo".into()),
        (1, 3),
        "\\foo",
        "bad character literal \\foo",
    );
    check(
        "a\u{A0}b",
        ReadErrorKind::InvalidCharacter('\u{A0}'),
        (1, 2),
        "\u{A0}",
        "U+00A0",
    );
    check(
        "a \u{FEFF}",
        ReadErrorKind::InvalidCharacter('\u{FEFF}'),
        (1, 3),
        "\u{FEFF}",
        "U+FEFF",
    );
    check(
        "\u{0}",
        ReadErrorKind::InvalidCharacter('\0'),
        (1, 1),
        "\0",
        "U+0000",
    );
    check(
        "\u{2028}",
        ReadErrorKind::InvalidCharacter('\u{2028}'),
        (1, 1),
        "\u{2028}",
        "outside a string",
    );
}

#[test]
fn number_errors() {
    let k = ReadErrorKind::IntegerOutOfRange {
        text: "300i8".into(),
        width: crate::syntax::IntWidth::I8,
    };
    check(
        "(f 300i8)",
        k,
        (1, 4),
        "300i8",
        "does not fit i8 (-128 to 127)",
    );
    let k = ReadErrorKind::FloatOutOfRange {
        text: "1e999".into(),
        width: crate::syntax::FltWidth::F64,
    };
    check("1e999", k, (1, 1), "1e999", "too large for f64");
    let k = ReadErrorKind::InvalidNumber {
        text: "12ab".into(),
        reason: "trailing characters are not a width suffix (i8 i16 i32 i64)",
    };
    check("  12ab", k, (1, 3), "12ab", "invalid number 12ab");
}

#[test]
fn name_errors() {
    check(
        "a/b/c",
        ReadErrorKind::InvalidSymbol("a/b/c".into()),
        (1, 1),
        "a/b/c",
        "invalid symbol a/b/c",
    );
    check(
        "x ::k",
        ReadErrorKind::InvalidKeyword("::k".into()),
        (1, 3),
        "::k",
        "invalid keyword ::k",
    );
    check(
        "#{1}",
        ReadErrorKind::UnknownDispatch(Some('{')),
        (1, 1),
        "#{",
        "only #_ is defined",
    );
    check(
        "x #",
        ReadErrorKind::UnknownDispatch(None),
        (1, 3),
        "#",
        "# at end of input",
    );
}

#[test]
fn prefix_errors() {
    for (src, p, at) in [
        ("'", "'", (1, 1)),
        ("(a ')", "'", (1, 4)),
        ("`", "`", (1, 1)),
        ("(@)", "@", (1, 2)),
        ("@ x", "@", (1, 1)),
        ("[,@]", ",@", (1, 2)),
        (",@ x", ",@", (1, 1)),
        ("(x ';c\n)", "'", (1, 4)),
    ] {
        let e = error(src);
        assert_eq!(e.kind, ReadErrorKind::PrefixWithoutForm(p), "{src:?}");
        assert_eq!((e.pos.line, e.pos.col), at, "{src:?}");
        assert!(e
            .to_string()
            .contains("must be followed immediately by a form"));
    }
    check(
        "(a #_)",
        ReadErrorKind::DiscardWithoutForm,
        (1, 4),
        "#_",
        "#_ must be followed by a form",
    );
    check(
        "#_",
        ReadErrorKind::DiscardWithoutForm,
        (1, 1),
        "#_",
        "#_ must be followed by a form",
    );
    check(
        "(f &(. x y))",
        ReadErrorKind::InOutNotSymbol,
        (1, 4),
        "&(. x y)",
        "& must be followed by a symbol",
    );
    for src in ["&1", "&:k", "&nil", "&\"s\"", "&@x", "&&x", "&'x"] {
        assert_eq!(error(src).kind, ReadErrorKind::InOutNotSymbol, "{src}");
    }
}

#[test]
fn delimiter_errors() {
    check(
        "(a\n (b",
        ReadErrorKind::Unclosed { open: '(' },
        (2, 2),
        "(",
        "unclosed (",
    );
    check(
        "[a",
        ReadErrorKind::Unclosed { open: '[' },
        (1, 1),
        "[",
        "unclosed [",
    );
    check(
        "{",
        ReadErrorKind::Unclosed { open: '{' },
        (1, 1),
        "{",
        "unclosed {",
    );
    check(
        "a)",
        ReadErrorKind::UnexpectedClose { found: ')' },
        (1, 2),
        ")",
        "unexpected ) with nothing open",
    );
    check(
        "]",
        ReadErrorKind::UnexpectedClose { found: ']' },
        (1, 1),
        "]",
        "unexpected ]",
    );
}

#[test]
fn mismatched_close_and_odd_map() {
    let k = ReadErrorKind::MismatchedClose {
        open: '[',
        open_line: 1,
        open_col: 2,
        found: ')',
    };
    check("([a)]", k, (1, 4), ")", ") does not close [ opened at 1:2");
    check(
        "{1 2 3}",
        ReadErrorKind::OddMapEntries { count: 3 },
        (1, 1),
        "{1 2 3}",
        "has 3 forms",
    );
}

#[test]
fn depth_limit() {
    let ok = format!("{}{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    assert_eq!(read_all(&ok, "t").expect("at the limit").len(), 1);
    let deep = "(".repeat(MAX_DEPTH + 1);
    let e = error(&deep);
    assert_eq!(e.kind, ReadErrorKind::TooDeep { limit: MAX_DEPTH });
    assert_eq!(e.pos.col, MAX_DEPTH + 1);
    assert!(e.to_string().contains("nested deeper than 1000 levels"));
}
