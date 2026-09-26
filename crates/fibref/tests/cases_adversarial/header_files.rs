//! Header parsing, continued from `header.rs`: files with no header at
//! all, `read_header` on real files, and one documented interpretation.

use fibref::cases::{read_header, HeaderErrorKind, Verdict};

use super::header::error_of;
use super::support::{accept_header, reject_header, TempDir};

// ---- no header ---------------------------------------------------------------

#[test]
fn empty_file_has_no_header() {
    assert_eq!(error_of(""), (1, HeaderErrorKind::Empty));
}

#[test]
fn file_starting_with_code_has_no_header() {
    assert_eq!(
        error_of("(defun main () 1)\n;; spec: §4\n;; expect: accept\n"),
        (1, HeaderErrorKind::Empty)
    );
}

#[test]
fn file_starting_with_blank_line_has_no_header() {
    let source = format!("\n{}", accept_header(1));
    assert_eq!(error_of(&source), (1, HeaderErrorKind::Empty));
}

#[test]
fn file_starting_with_plain_comment_has_no_header() {
    let source = format!(";; a case\n{}", accept_header(1));
    assert_eq!(error_of(&source), (1, HeaderErrorKind::Empty));
}

#[test]
fn single_semicolon_comment_is_not_a_header_line() {
    let source = "; spec: §4\n; expect: accept\n; result: 1\n; audit: clean\n";
    assert_eq!(error_of(source), (1, HeaderErrorKind::Empty));
}

// ---- read_header -------------------------------------------------------------

#[test]
fn read_header_of_missing_file_is_an_error_not_a_panic() {
    let dir = TempDir::new("read-missing");
    let path = dir.path().join("missing.fib");
    let err = read_header(&path).expect_err("missing file must be a header error");
    assert_eq!(err.path, path);
    assert!(
        matches!(err.kind, HeaderErrorKind::Unreadable(_)),
        "{:?}",
        err.kind
    );
}

#[test]
fn read_header_of_invalid_utf8_is_an_error_not_a_panic() {
    let dir = TempDir::new("read-utf8");
    let path = dir.write_bytes(
        "bad.fib",
        b";; spec: \xff\xfe\n;; expect: reject\n;; error: x\n",
    );
    let err = read_header(&path).expect_err("invalid UTF-8 must be a header error");
    assert!(
        matches!(err.kind, HeaderErrorKind::Unreadable(_)),
        "{:?}",
        err.kind
    );
}

#[test]
fn read_header_reads_a_real_file() {
    let dir = TempDir::new("read-ok");
    let path = dir.write("ok.fib", &reject_header("passed twice"));
    assert_eq!(
        read_header(&path).map(|h| h.verdict),
        Ok(Verdict::Reject {
            error: "passed twice".to_string()
        })
    );
}

// ---- interpretation: prose shaped like a key directly under the header ---

#[test]
fn prose_comment_shaped_like_a_key_directly_under_the_header_is_an_unknown_key() {
    // `;; Note: ...` has the form `;; key: value`, so by the contract the
    // header has not ended and `Note` is an unknown key. A case author who
    // writes such a comment gets a HeaderError, not a silently shorter
    // header. See the spec questions in the report.
    let source = format!(
        "{};; Note: the callee retains it on return\n",
        accept_header(1)
    );
    assert_eq!(
        error_of(&source),
        (5, HeaderErrorKind::UnknownKey("Note".to_string()))
    );
}
