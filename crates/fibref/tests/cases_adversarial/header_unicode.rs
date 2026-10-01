//! Round 3: headers that are byte-for-byte unusual. A UTF-8 byte order
//! mark, CR-only line endings, invisible Unicode in values, non-ASCII
//! digits, a NUL byte and a very long line. The contract says a header
//! line has the form `;; key: value` and parsing never panics; these
//! pin what "of that form" means at the edges.

use std::path::Path;

use fibref::cases::{parse_header, AuditExpect, Expected, HeaderErrorKind, Verdict};

fn path() -> &'static Path {
    Path::new("cases/unicode.fib")
}

fn accept(result: i64, audit: AuditExpect) -> Verdict {
    Verdict::Accept {
        result: Expected::Int(result),
        audit,
        allocs: None,
    }
}

#[test]
fn a_byte_order_mark_before_the_first_semicolons_is_not_a_header_line() {
    // Interpretation: the form is `;; key: value` from byte 0. A BOM is
    // a character before `;;`, so line 1 is not a header line and the
    // header is empty. Pinned so a BOM'd file fails loudly, not oddly.
    let source = "\u{feff};; spec: §4\n;; expect: reject\n;; error: x\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.kind, HeaderErrorKind::Empty);
}

#[test]
fn cr_only_line_endings_are_one_line_and_do_not_panic() {
    // Interpretation: only LF and CRLF are line endings. A classic-Mac
    // file is one long line whose spec value swallows the rest, so the
    // next required key is missing at line 1. It must not parse as a
    // valid header and must not panic.
    let source = ";; spec: §4\r;; expect: accept\r;; result: 1\r;; audit: clean\r";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 1, "{err}");
    assert_eq!(err.kind, HeaderErrorKind::MissingKey("expect"), "{err}");
}

#[test]
fn a_zero_width_space_in_an_enum_value_is_a_bad_value() {
    // U+200B is not whitespace, so trimming does not remove it and the
    // value is not `accept`.
    let source = ";; spec: §4\n;; expect: accept\u{200b}\n;; result: 1\n;; audit: clean\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 2, "{err}");
    assert_eq!(
        err.kind,
        HeaderErrorKind::BadValue {
            key: "expect",
            value: "accept\u{200b}".to_string()
        }
    );
}

#[test]
fn a_trailing_byte_order_mark_in_a_value_is_a_bad_value() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\u{feff}\n;; audit: clean\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 3, "{err}");
    assert!(
        matches!(err.kind, HeaderErrorKind::BadValue { key: "result", .. }),
        "{err}"
    );
}

#[test]
fn no_break_spaces_around_a_value_are_trimmed() {
    // Interpretation: "trimmed value" means Unicode whitespace, which
    // is what `str::trim` does; U+00A0 counts. Pinned either way so a
    // change is visible.
    let source = ";; spec: §4\n;; expect: accept\n;; result:\u{a0}7\u{a0}\n;; audit: clean\n";
    let header = parse_header(path(), source).unwrap();
    assert_eq!(header.verdict, accept(7, AuditExpect::Clean));
}

#[test]
fn non_ascii_digits_are_not_an_integer() {
    for value in ["٣", "５", "①", "²"] {
        let source =
            format!(";; spec: §4\n;; expect: accept\n;; result: {value}\n;; audit: clean\n");
        let err = parse_header(path(), &source).unwrap_err();
        assert_eq!(err.line, 3, "{value}: {err}");
        assert_eq!(
            err.kind,
            HeaderErrorKind::BadValue {
                key: "result",
                value: value.to_string()
            },
            "{value}"
        );
    }
}

#[test]
fn a_leading_plus_sign_is_an_integer() {
    // Interpretation: `+5` is the i64 5, as Rust's integer parser reads
    // it. Pinned so a stricter parser is a visible change.
    let source = ";; spec: §4\n;; expect: accept\n;; result: +5\n;; audit: clean\n";
    let header = parse_header(path(), source).unwrap();
    assert_eq!(header.verdict, accept(5, AuditExpect::Clean));
}

#[test]
fn no_space_after_the_semicolons_is_still_a_header_line() {
    // Interpretation: the space after `;;` and after `:` is layout, not
    // syntax. `;;expect:accept` carries the same key and value.
    let source = ";;spec:§4\n;;expect:accept\n;;result:1\n;;audit:leak-cycle\n";
    let header = parse_header(path(), source).unwrap();
    assert_eq!(header.spec, "§4");
    assert_eq!(header.verdict, accept(1, AuditExpect::LeakCycle));
}

#[test]
fn a_numeric_key_on_line_one_is_an_unknown_key_not_an_empty_header() {
    let source = ";; 42: the answer\n;; spec: §4\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.kind, HeaderErrorKind::UnknownKey("42".to_string()));
}

#[test]
fn a_non_ascii_key_ends_the_header() {
    // `spéc` has a non-ASCII letter, so it is not a key and line 1 is
    // not of the form; the header is empty.
    let source = ";; spéc: §4\n;; expect: reject\n;; error: x\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 1);
    assert_eq!(err.kind, HeaderErrorKind::Empty);
}

#[test]
fn a_nul_byte_in_free_text_does_not_panic_and_is_kept() {
    let source = ";; spec: §4\u{0}here\n;; expect: reject\n;; error: bad\u{0}thing\n";
    let header = parse_header(path(), source).unwrap();
    assert_eq!(header.spec, "§4\u{0}here");
    assert_eq!(
        header.verdict,
        Verdict::Reject {
            error: "bad\u{0}thing".to_string()
        }
    );
}

#[test]
fn a_megabyte_long_spec_line_parses() {
    let long = "x".repeat(1 << 20);
    let source = format!(";; spec: {long}\n;; expect: reject\n;; error: e\n");
    let header = parse_header(path(), &source).unwrap();
    assert_eq!(header.spec.len(), 1 << 20);
}

#[test]
fn a_megabyte_of_code_after_the_header_is_not_scanned_for_keys() {
    let code = ";; error: other\n".repeat(1 << 16);
    let source = format!(";; spec: §4\n;; expect: reject\n;; error: e\n\n{code}");
    let header = parse_header(path(), &source).unwrap();
    assert_eq!(
        header.verdict,
        Verdict::Reject {
            error: "e".to_string()
        }
    );
}

#[test]
fn form_feed_and_vertical_tab_are_not_line_endings() {
    // A form feed inside line 1 stays inside line 1: the spec value
    // holds it, and line 2 is still line 2.
    let source = ";; spec: §4\u{c}more\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    let header = parse_header(path(), source).unwrap();
    assert_eq!(header.spec, "§4\u{c}more");
    let source = ";; spec: §4\u{b};; expect: reject\n;; error: x\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.kind, HeaderErrorKind::MissingKey("expect"), "{err}");
}

#[test]
fn error_text_with_only_a_no_break_space_is_empty_after_trimming() {
    // A whitespace-only error text is refused from files; U+00A0 is
    // whitespace too, so it must be refused the same way, not accepted
    // as a one-character substring that every message with a space
    // would contain.
    let source = ";; spec: §4\n;; expect: reject\n;; error:\u{a0}\n";
    let err = parse_header(path(), source).unwrap_err();
    assert_eq!(err.line, 3, "{err}");
    assert!(
        matches!(err.kind, HeaderErrorKind::BadValue { key: "error", .. }),
        "{err}"
    );
}
