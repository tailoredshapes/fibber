//! Tests for header parsing: one per error kind, plus valid headers.

use super::*;

fn parse(source: &str) -> Result<Header, HeaderError> {
    parse_header(Path::new("t.fib"), source)
}

fn kind_at(source: &str) -> (usize, HeaderErrorKind) {
    match parse(source) {
        Err(e) => {
            assert_eq!(e.path, PathBuf::from("t.fib"));
            (e.line, e.kind)
        }
        Ok(h) => panic!("expected a header error, parsed {h:?}"),
    }
}

const ACCEPT: &str = ";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: clean\n";
const REJECT: &str = ";; spec: §5\n;; expect: reject\n;; error:  passed twice\n";

#[test]
fn valid_accept_header() {
    assert_eq!(
        parse(ACCEPT),
        Ok(Header {
            spec: "§4".to_string(),
            verdict: Verdict::Accept {
                result: Expected::Int(1),
                audit: AuditExpect::Clean,
            },
        })
    );
}

#[test]
fn valid_reject_header_trims_value() {
    assert_eq!(
        parse(REJECT),
        Ok(Header {
            spec: "§5".to_string(),
            verdict: Verdict::Reject {
                error: "passed twice".to_string()
            },
        })
    );
}

#[test]
fn accept_with_leak_cycle_and_negative_result() {
    let source = ";; spec: §6 (proposed)\n;; expect: accept\n;; result: -7\n;; audit: leak-cycle\n";
    assert_eq!(
        parse(source).map(|h| h.verdict),
        Ok(Verdict::Accept {
            result: Expected::Int(-7),
            audit: AuditExpect::LeakCycle,
        })
    );
}

#[test]
fn trailing_comment_lines_are_not_header() {
    // The comment after the header looks like a key: value pair in the
    // second line, but the header already ended at the first plain comment.
    let source = format!("{ACCEPT};; A function returns memory owned by its argument.\n;; error: this is prose, not a key\n(defun main () 1)\n");
    assert!(parse(&source).is_ok(), "{:?}", parse(&source));
}

#[test]
fn header_ends_at_blank_line() {
    let source = format!("{ACCEPT}\n;; error: after a blank line\n");
    assert!(parse(&source).is_ok(), "{:?}", parse(&source));
}

#[test]
fn header_ends_at_code() {
    let source = format!("{REJECT}(defun main () 1)\n;; result: 3\n");
    assert!(parse(&source).is_ok(), "{:?}", parse(&source));
}

#[test]
fn prose_with_a_colon_is_not_a_key() {
    assert_eq!(header_line(";; Note that x: y"), None);
    assert_eq!(header_line(";; spec: §4"), Some(("spec", "§4")));
    assert_eq!(header_line(";;expect:accept"), Some(("expect", "accept")));
    assert_eq!(header_line(";; : no key"), None);
    assert_eq!(header_line("; one semicolon: no"), None);
}

#[test]
fn empty_header() {
    assert_eq!(kind_at(""), (1, HeaderErrorKind::Empty));
    assert_eq!(kind_at("(defun main () 1)\n"), (1, HeaderErrorKind::Empty));
    assert_eq!(kind_at("\n;; spec: §4\n"), (1, HeaderErrorKind::Empty));
}

#[test]
fn unknown_key() {
    assert_eq!(
        kind_at(";; spec: §4\n;; verdict: accept\n"),
        (2, HeaderErrorKind::UnknownKey("verdict".to_string()))
    );
}

#[test]
fn duplicate_key() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: accept\n;; spec: §5\n"),
        (3, HeaderErrorKind::DuplicateKey("spec".to_string()))
    );
}

#[test]
fn missing_spec() {
    assert_eq!(
        kind_at(";; expect: accept\n;; result: 1\n;; audit: clean\n"),
        (3, HeaderErrorKind::MissingKey("spec"))
    );
}

#[test]
fn missing_expect() {
    assert_eq!(
        kind_at(";; spec: §4\n;; result: 1\n"),
        (2, HeaderErrorKind::MissingKey("expect"))
    );
}

#[test]
fn accept_missing_result() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: accept\n;; audit: clean\n"),
        (3, HeaderErrorKind::MissingKey("result"))
    );
}

#[test]
fn accept_missing_audit() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: accept\n;; result: 1\n"),
        (3, HeaderErrorKind::MissingKey("audit"))
    );
}

#[test]
fn reject_missing_error() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: reject\n"),
        (2, HeaderErrorKind::MissingKey("error"))
    );
}

#[test]
fn accept_forbids_error() {
    assert_eq!(
        kind_at(&format!("{ACCEPT};; error: boom\n")),
        (5, HeaderErrorKind::ForbiddenKey("error"))
    );
}

#[test]
fn reject_forbids_result_and_audit() {
    assert_eq!(
        kind_at(&format!("{REJECT};; result: 1\n")),
        (4, HeaderErrorKind::ForbiddenKey("result"))
    );
    assert_eq!(
        kind_at(&format!("{REJECT};; audit: clean\n")),
        (4, HeaderErrorKind::ForbiddenKey("audit"))
    );
}

#[test]
fn bad_expect_value() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: maybe\n"),
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
fn bad_result_value() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: accept\n;; result: one\n;; audit: clean\n"),
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
fn bad_audit_value() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: accept\n;; result: 1\n;; audit: dirty\n"),
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
fn empty_error_value() {
    assert_eq!(
        kind_at(";; spec: §4\n;; expect: reject\n;; error:   \n"),
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
fn unreadable_file() {
    let err = read_header(Path::new("/nonexistent/dir/case.fib")).unwrap_err();
    assert_eq!(err.line, 0);
    assert!(matches!(err.kind, HeaderErrorKind::Unreadable(_)), "{err}");
}

#[test]
fn error_display_names_file_and_line() {
    let err = parse(";; spec: §4\n;; nope: 1\n").unwrap_err();
    assert_eq!(err.to_string(), "t.fib:2: unknown header key `nope`");
}

#[test]
fn empty_spec_is_a_bad_value() {
    // `spec` names the section that decides the case; empty names nothing.
    assert_eq!(
        kind_at(";; spec:\n;; expect: accept\n;; result: 1\n;; audit: clean\n"),
        (
            1,
            HeaderErrorKind::BadValue {
                key: "spec",
                value: String::new()
            }
        )
    );
    assert_eq!(
        kind_at(";; spec:   \n;; expect: reject\n;; error: x\n"),
        (
            1,
            HeaderErrorKind::BadValue {
                key: "spec",
                value: String::new()
            }
        )
    );
}

const TRAP: &str = ";; spec: types §2.11\n;; expect: trap\n;; trap:   integer / by zero\n";

#[test]
fn valid_trap_header() {
    assert_eq!(
        parse(TRAP),
        Ok(Header {
            spec: "types §2.11".to_string(),
            verdict: Verdict::Trap {
                trap: "integer / by zero".to_string()
            },
        })
    );
}

#[test]
fn trap_header_needs_its_text_and_forbids_the_others() {
    let missing = ";; spec: §2\n;; expect: trap\n";
    assert_eq!(kind_at(missing), (2, HeaderErrorKind::MissingKey("trap")));
    for (extra, key) in [
        (";; result: 1\n", "result"),
        (";; audit: clean\n", "audit"),
        (";; error: x\n", "error"),
    ] {
        let src = format!("{TRAP}{extra}");
        assert_eq!(kind_at(&src), (4, HeaderErrorKind::ForbiddenKey(key)));
    }
    let on_accept = format!("{ACCEPT};; trap: x\n");
    assert_eq!(
        kind_at(&on_accept),
        (5, HeaderErrorKind::ForbiddenKey("trap"))
    );
    let on_reject = format!("{REJECT};; trap: x\n");
    assert_eq!(
        kind_at(&on_reject),
        (4, HeaderErrorKind::ForbiddenKey("trap"))
    );
}
