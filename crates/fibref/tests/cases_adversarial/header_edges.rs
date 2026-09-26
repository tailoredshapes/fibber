//! Header parsing, round 2: the exact shape of a header line, line
//! numbers under CRLF, values that look almost like integers, an empty
//! `spec`, and forbidden keys with empty values.

use fibref::cases::{AuditExpect, Expected, HeaderErrorKind, Verdict};

use super::header::{error_of, parse};
use super::support::{accept_header, reject_header};

fn accept(result: i64, audit: AuditExpect) -> Verdict {
    Verdict::Accept {
        result: Expected::Int(result),
        audit,
    }
}

// ---- where the header ends ---------------------------------------------------

#[test]
fn a_bare_double_semicolon_line_ends_the_header() {
    // `;;` alone is a comment without a key, so the `result: 2` after it
    // is prose and the header keeps result 1.
    let source = format!("{};;\n;; result: 2\n", accept_header(1));
    assert_eq!(
        parse(&source).map(|h| h.verdict),
        Ok(accept(1, AuditExpect::Clean))
    );
}

#[test]
fn a_whitespace_only_line_ends_the_header() {
    let source = format!("{}   \t\n;; audit: leak-cycle\n", accept_header(1));
    assert_eq!(
        parse(&source).map(|h| h.verdict),
        Ok(accept(1, AuditExpect::Clean))
    );
}

#[test]
fn a_key_line_without_a_colon_ends_the_header() {
    // `;; expect accept` (typo: no colon) is not of the form `;; key: value`,
    // so the header ends at line 1 and `expect` is missing, not bad.
    let source = ";; spec: §4\n;; expect accept\n;; result: 1\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("expect"));
    assert_eq!(line, 1, "the header is only line 1");
}

#[test]
fn leading_whitespace_before_the_semicolons_is_not_a_header_line() {
    // Interpretation: the form is exactly `;; key: value`; an indented
    // comment is not a header line, so a file starting with one has no
    // header. See the spec questions in the report.
    let source = format!("  {}", accept_header(1));
    assert_eq!(error_of(&source), (1, HeaderErrorKind::Empty));
}

#[test]
fn three_semicolons_is_not_a_header_line() {
    // Interpretation: `;;; spec: §4` is a different comment style, not
    // `;; key: value`, so the header is empty.
    let source = ";;; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    assert_eq!(error_of(source), (1, HeaderErrorKind::Empty));
}

#[test]
fn a_space_between_key_and_colon_is_not_a_key_line() {
    // Interpretation: `result : 1` has a key containing a space, which
    // is not a key, so the header ends there and `result` is missing.
    let source = ";; spec: §4\n;; expect: accept\n;; result : 1\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("result"));
    assert!(
        (1..=2).contains(&line),
        "reported outside the header: {line}"
    );
}

// ---- CRLF line numbers -------------------------------------------------------

#[test]
fn crlf_bad_value_is_reported_at_the_right_line() {
    let source = ";; spec: §4\r\n;; expect: accept\r\n;; result: 1\r\n;; audit: dirty\r\n";
    assert_eq!(
        error_of(source),
        (
            4,
            HeaderErrorKind::BadValue {
                key: "audit",
                value: "dirty".to_string()
            }
        )
    );
}

#[test]
fn crlf_duplicate_key_is_reported_at_the_right_line() {
    let source = ";; spec: §5\r\n;; expect: reject\r\n;; error: x\r\n;; error: y\r\n";
    assert_eq!(
        error_of(source),
        (4, HeaderErrorKind::DuplicateKey("error".to_string()))
    );
}

#[test]
fn crlf_forbidden_key_is_reported_at_the_right_line() {
    let source = ";; spec: §5\r\n;; expect: reject\r\n;; error: x\r\n;; audit: clean\r\n";
    assert_eq!(
        error_of(source),
        (4, HeaderErrorKind::ForbiddenKey("audit"))
    );
}

#[test]
fn crlf_unknown_key_is_reported_at_the_right_line() {
    let source = ";; spec: §5\r\n;; expect: reject\r\n;; errr: x\r\n";
    assert_eq!(
        error_of(source),
        (3, HeaderErrorKind::UnknownKey("errr".to_string()))
    );
}

#[test]
fn crlf_value_with_trailing_spaces_before_the_line_ending_is_trimmed() {
    let source = ";; spec: §4  \r\n;; expect: accept \r\n;; result: 3 \r\n;; audit: clean\t\r\n";
    let header = parse(source).expect("parses");
    assert_eq!(header.spec, "§4");
    assert_eq!(header.verdict, accept(3, AuditExpect::Clean));
}

// ---- values that look almost like integers ------------------------------------

fn bad_result(value: &str) {
    let source = format!(";; spec: §4\n;; expect: accept\n;; result: {value}\n;; audit: clean\n");
    let (line, kind) = error_of(&source);
    assert_eq!(line, 3, "{value:?}");
    assert_eq!(
        kind,
        HeaderErrorKind::BadValue {
            key: "result",
            value: value.to_string()
        },
        "{value:?} must be a bad result value"
    );
}

#[test]
fn result_with_a_trailing_comment_is_a_bad_value() {
    bad_result("1 ; one");
}

#[test]
fn result_in_hexadecimal_is_a_bad_value() {
    bad_result("0x10");
}

#[test]
fn result_with_digit_separators_is_a_bad_value() {
    bad_result("1_000");
}

#[test]
fn result_with_internal_whitespace_is_a_bad_value() {
    bad_result("1 2");
}

#[test]
fn result_with_exponent_is_a_bad_value() {
    bad_result("1e3");
}

#[test]
fn result_true_is_a_bad_value() {
    bad_result("true");
}

#[test]
fn negative_zero_and_leading_zeros_are_integers() {
    for (text, value) in [("-0", 0), ("007", 7), ("-007", -7)] {
        let source =
            format!(";; spec: §4\n;; expect: accept\n;; result: {text}\n;; audit: clean\n");
        assert_eq!(
            parse(&source).map(|h| h.verdict),
            Ok(accept(value, AuditExpect::Clean)),
            "{text:?}"
        );
    }
}

// ---- other values -------------------------------------------------------------

#[test]
fn expect_with_trailing_words_is_a_bad_value() {
    let source = ";; spec: §4\n;; expect: accept please\n;; result: 1\n;; audit: clean\n";
    assert_eq!(
        error_of(source),
        (
            2,
            HeaderErrorKind::BadValue {
                key: "expect",
                value: "accept please".to_string()
            }
        )
    );
}

#[test]
fn audit_leak_cycle_spelled_with_underscore_is_a_bad_value() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: leak_cycle\n";
    assert_eq!(
        error_of(source),
        (
            4,
            HeaderErrorKind::BadValue {
                key: "audit",
                value: "leak_cycle".to_string()
            }
        )
    );
}

#[test]
fn empty_spec_is_a_header_error() {
    // Interpretation: `spec` names the section that decides the case
    // (cases/ownership/README.md); an empty value names nothing, so it
    // is a bad value, like an empty `error`. See the spec questions.
    let source = ";; spec:\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 1);
    assert_eq!(
        kind,
        HeaderErrorKind::BadValue {
            key: "spec",
            value: String::new()
        }
    );
}

// ---- forbidden keys with empty values ---------------------------------------

#[test]
fn a_forbidden_key_is_forbidden_even_with_an_empty_value() {
    let source = format!("{};; error:\n", accept_header(1));
    assert_eq!(
        error_of(&source),
        (5, HeaderErrorKind::ForbiddenKey("error"))
    );
    let source = format!("{};; result:\n", reject_header("x"));
    assert_eq!(
        error_of(&source),
        (4, HeaderErrorKind::ForbiddenKey("result"))
    );
}

#[test]
fn accept_missing_result_and_carrying_error_is_an_error_either_way() {
    // Both faults are present; the contract only says it is a header
    // error, so either kind is acceptable, but it must be one of them.
    let source = ";; spec: §4\n;; expect: accept\n;; audit: clean\n;; error: x\n";
    let (line, kind) = error_of(source);
    assert!(
        matches!(
            kind,
            HeaderErrorKind::MissingKey("result") | HeaderErrorKind::ForbiddenKey("error")
        ),
        "{kind:?}"
    );
    assert!((1..=4).contains(&line), "line {line} is outside the header");
}

#[test]
fn duplicate_key_is_reported_before_a_later_unknown_key() {
    let source = ";; spec: a\n;; spec: b\n;; bogus: c\n";
    assert_eq!(
        error_of(source),
        (2, HeaderErrorKind::DuplicateKey("spec".to_string()))
    );
}

#[test]
fn two_keys_on_one_line_are_one_key_with_a_long_value() {
    // `;; spec: §4 ;; expect: accept` is one header line whose value is
    // everything after `spec:`; `expect` is then missing.
    let source = ";; spec: §4 ;; expect: accept\n;; result: 1\n;; audit: clean\n";
    let (_, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("expect"));
}
