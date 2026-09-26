//! Header parsing through the public API: every error kind, with the
//! file path and line number, plus the boundaries of a valid header.

use std::path::{Path, PathBuf};

use fibref::cases::{
    parse_header, AuditExpect, Expected, Header, HeaderError, HeaderErrorKind, Verdict,
};

use super::support::{accept_header, reject_header};

const FILE: &str = "some/dir/case.fib";

pub(super) fn parse(source: &str) -> Result<Header, HeaderError> {
    parse_header(Path::new(FILE), source)
}

/// Asserts a header error and returns its (line, kind), checking the path.
pub(super) fn error_of(source: &str) -> (usize, HeaderErrorKind) {
    match parse(source) {
        Err(e) => {
            assert_eq!(
                e.path,
                PathBuf::from(FILE),
                "error must carry the file path"
            );
            assert!(e.line >= 1, "line must be 1-based, got {}", e.line);
            assert!(
                e.to_string().contains(FILE),
                "Display must name the file: {e}"
            );
            assert!(
                e.to_string().contains(&format!(":{}:", e.line)),
                "Display must name the line: {e}"
            );
            (e.line, e.kind)
        }
        Ok(h) => panic!("expected a header error for {source:?}, parsed {h:?}"),
    }
}

fn accept(result: i64, audit: AuditExpect) -> Verdict {
    Verdict::Accept {
        result: Expected::Int(result),
        audit,
    }
}

// ---- valid headers and where they end ------------------------------------

#[test]
fn valid_accept_header_with_trailing_comment_lines() {
    let source = format!(
        "{};; This comment has no key, so the header ended above.\n\
         ;; error: prose that looks like a key is not a key\n\
         ;; result: 999\n\
         ;; expect: reject\n\
         (defun main () -> i64 1)\n",
        accept_header(1)
    );
    assert_eq!(
        parse(&source),
        Ok(Header {
            spec: "§4".to_string(),
            verdict: accept(1, AuditExpect::Clean),
        })
    );
}

#[test]
fn header_ends_at_blank_line_and_later_keys_are_ignored() {
    // A blank line ends the header; the duplicate keys after it must be
    // ignored, not reported as DuplicateKey.
    let source = format!("{}\n;; result: 2\n;; audit: leak-cycle\n", accept_header(1));
    assert_eq!(
        parse(&source).map(|h| h.verdict),
        Ok(accept(1, AuditExpect::Clean))
    );
}

#[test]
fn header_ends_at_code_and_later_keys_are_ignored() {
    let source = format!(
        "{}(defun main () 1)\n;; result: 5\n;; unknown-key: x\n",
        reject_header("no such thing")
    );
    assert_eq!(
        parse(&source).map(|h| h.verdict),
        Ok(Verdict::Reject {
            error: "no such thing".to_string()
        })
    );
}

#[test]
fn a_required_key_after_the_header_ended_is_missing() {
    // `result` appears only after a prose comment ended the header, so
    // it does not count.
    let source = ";; spec: §4\n;; expect: accept\n;; audit: clean\n;; prose\n;; result: 1\n";
    let (line, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("result"));
    assert!(
        line <= 3,
        "missing key must be reported inside the header, got line {line}"
    );
}

#[test]
fn crlf_line_endings_parse_and_values_are_trimmed() {
    let source = ";; spec: §4\r\n;; expect: accept\r\n;; result: 42\r\n;; audit: clean\r\n;; prose\r\n(defun main () 42)\r\n";
    assert_eq!(
        parse(source),
        Ok(Header {
            spec: "§4".to_string(),
            verdict: accept(42, AuditExpect::Clean),
        })
    );
}

#[test]
fn crlf_reject_error_text_has_no_carriage_return() {
    let source = ";; spec: §5\r\n;; expect: reject\r\n;; error: passed twice\r\n";
    assert_eq!(
        parse(source).map(|h| h.verdict),
        Ok(Verdict::Reject {
            error: "passed twice".to_string()
        })
    );
}

#[test]
fn values_keep_internal_spacing_and_colons() {
    let source =
        ";; spec:   §5: the & rule\n;; expect: reject\n;; error:  cell  crosses  thread: x\n";
    assert_eq!(
        parse(source),
        Ok(Header {
            spec: "§5: the & rule".to_string(),
            verdict: Verdict::Reject {
                error: "cell  crosses  thread: x".to_string()
            },
        })
    );
}

#[test]
fn keys_may_come_in_any_order() {
    let source = ";; audit: leak-cycle\n;; result: -3\n;; expect: accept\n;; spec: §6\n";
    assert_eq!(
        parse(source).map(|h| h.verdict),
        Ok(accept(-3, AuditExpect::LeakCycle))
    );
}

#[test]
fn tab_after_the_semicolons_is_whitespace() {
    let source = ";;\tspec: §4\n;;\texpect: accept\n;;\tresult: 1\n;;\taudit: clean\n";
    assert_eq!(
        parse(source).map(|h| h.verdict),
        Ok(accept(1, AuditExpect::Clean))
    );
}

#[test]
fn header_without_trailing_newline_parses() {
    let source = ";; spec: §5\n;; expect: reject\n;; error: boom";
    assert_eq!(
        parse(source).map(|h| h.verdict),
        Ok(Verdict::Reject {
            error: "boom".to_string()
        })
    );
}

#[test]
fn i64_extremes_parse() {
    let source = format!(
        ";; spec: §4\n;; expect: accept\n;; result: {}\n;; audit: clean\n",
        i64::MIN
    );
    assert_eq!(
        parse(&source).map(|h| h.verdict),
        Ok(accept(i64::MIN, AuditExpect::Clean))
    );
}

// ---- accept: missing and forbidden keys -----------------------------------

#[test]
fn accept_missing_result() {
    let source = ";; spec: §4\n;; expect: accept\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("result"));
    assert!((1..=3).contains(&line), "line {line} is outside the header");
}

#[test]
fn accept_missing_audit() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\n";
    let (line, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("audit"));
    assert!((1..=3).contains(&line), "line {line} is outside the header");
}

#[test]
fn accept_with_error_key_is_forbidden() {
    let source = format!("{};; error: not allowed here\n", accept_header(1));
    assert_eq!(
        error_of(&source),
        (5, HeaderErrorKind::ForbiddenKey("error"))
    );
}

#[test]
fn missing_expect() {
    let source = ";; spec: §4\n;; result: 1\n;; audit: clean\n";
    let (_, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("expect"));
}

#[test]
fn missing_spec() {
    let source = ";; expect: reject\n;; error: boom\n";
    let (_, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("spec"));
}

// ---- reject: missing and forbidden keys -----------------------------------

#[test]
fn reject_with_result_key_is_forbidden() {
    let source = ";; spec: §5\n;; expect: reject\n;; error: boom\n;; result: 1\n";
    assert_eq!(
        error_of(source),
        (4, HeaderErrorKind::ForbiddenKey("result"))
    );
}

#[test]
fn reject_with_audit_key_is_forbidden() {
    let source = ";; spec: §5\n;; expect: reject\n;; audit: clean\n;; error: boom\n";
    assert_eq!(
        error_of(source),
        (3, HeaderErrorKind::ForbiddenKey("audit"))
    );
}

#[test]
fn reject_missing_error() {
    let source = ";; spec: §5\n;; expect: reject\n";
    let (_, kind) = error_of(source);
    assert_eq!(kind, HeaderErrorKind::MissingKey("error"));
}

#[test]
fn reject_with_empty_error() {
    let source = ";; spec: §5\n;; expect: reject\n;; error:\n";
    assert_eq!(
        error_of(source),
        (
            3,
            HeaderErrorKind::BadValue {
                key: "error",
                value: String::new()
            }
        )
    );
}

#[test]
fn reject_with_whitespace_only_error() {
    let source = ";; spec: §5\n;; expect: reject\n;; error:    \t \n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 3);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "error", .. }),
        "{kind:?}"
    );
}

// ---- bad values -------------------------------------------------------------

#[test]
fn unknown_audit_value() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: dirty\n";
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
fn audit_value_is_case_sensitive() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: Clean\n";
    let (_, kind) = error_of(source);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "audit", .. }),
        "{kind:?}"
    );
}

#[test]
fn unknown_expect_value() {
    let source = ";; spec: §4\n;; expect: maybe\n;; result: 1\n;; audit: clean\n";
    assert_eq!(
        error_of(source),
        (
            2,
            HeaderErrorKind::BadValue {
                key: "expect",
                value: "maybe".to_string()
            }
        )
    );
}

#[test]
fn expect_value_is_case_sensitive() {
    let source = ";; spec: §4\n;; expect: Accept\n;; result: 1\n;; audit: clean\n";
    let (_, kind) = error_of(source);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "expect", .. }),
        "{kind:?}"
    );
}

#[test]
fn non_integer_result() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: one\n;; audit: clean\n";
    assert_eq!(
        error_of(source),
        (
            3,
            HeaderErrorKind::BadValue {
                key: "result",
                value: "one".to_string()
            }
        )
    );
}

#[test]
fn float_result_is_not_an_integer() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1.0\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 3);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "result", .. }),
        "{kind:?}"
    );
}

#[test]
fn result_out_of_i64_range() {
    let source =
        ";; spec: §4\n;; expect: accept\n;; result: 9223372036854775808\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 3);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "result", .. }),
        "{kind:?}"
    );
}

#[test]
fn empty_result_value() {
    let source = ";; spec: §4\n;; expect: accept\n;; result:\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 3);
    assert!(
        matches!(kind, HeaderErrorKind::BadValue { key: "result", .. }),
        "{kind:?}"
    );
}

// ---- keys ------------------------------------------------------------------

#[test]
fn unknown_key_inside_the_header() {
    let source =
        ";; spec: §4\n;; verdict: accept\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    assert_eq!(
        error_of(source),
        (2, HeaderErrorKind::UnknownKey("verdict".to_string()))
    );
}

#[test]
fn key_is_case_sensitive() {
    let source = ";; Spec: §4\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
    let (line, kind) = error_of(source);
    assert_eq!(line, 1);
    assert!(
        matches!(
            kind,
            HeaderErrorKind::UnknownKey(_) | HeaderErrorKind::Empty
        ),
        "{kind:?}"
    );
}

#[test]
fn duplicate_key_is_reported_at_the_second_occurrence() {
    let source = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; result: 1\n;; audit: clean\n";
    assert_eq!(
        error_of(source),
        (4, HeaderErrorKind::DuplicateKey("result".to_string()))
    );
}

#[test]
fn duplicate_spec_is_an_error_even_with_same_value() {
    let source = ";; spec: §4\n;; spec: §4\n;; expect: reject\n;; error: x\n";
    assert_eq!(
        error_of(source),
        (2, HeaderErrorKind::DuplicateKey("spec".to_string()))
    );
}
