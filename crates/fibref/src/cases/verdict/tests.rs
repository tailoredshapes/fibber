//! One test per row of the table in the module docs, plus each audit
//! mismatch that the harness must catch.

use super::*;
use crate::cases::header::HeaderErrorKind;
use std::path::PathBuf;

fn accept(result: i64, audit: AuditExpect) -> Header {
    Header {
        spec: "§4".to_string(),
        verdict: Verdict::Accept {
            result: Expected::Int(result),
            audit,
        },
    }
}

fn reject(error: &str) -> Header {
    Header {
        spec: "§5".to_string(),
        verdict: Verdict::Reject {
            error: error.to_string(),
        },
    }
}

fn compiled(result: i64, audit: AuditSummary) -> Outcome {
    Outcome::Compiled {
        result: Value::Int(result),
        audit,
    }
}

fn rejected(message: &str) -> Outcome {
    Outcome::Rejected {
        message: message.to_string(),
    }
}

fn leak_cycle() -> AuditSummary {
    AuditSummary {
        clean: false,
        leak_cycles: 1,
        leaks: 0,
        errors: vec![],
    }
}

fn fail_containing(status: Status, needles: &[&str]) {
    match &status {
        Status::Fail(why) => {
            for needle in needles {
                assert!(why.contains(needle), "{why:?} lacks {needle:?}");
            }
        }
        other => panic!("expected Fail, got {other:?}"),
    }
}

#[test]
fn accept_matching_result_and_clean_audit_passes() {
    let status = judge(
        &accept(5, AuditExpect::Clean),
        &compiled(5, AuditSummary::clean()),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn accept_matching_result_and_leak_cycle_passes() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, leak_cycle()),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn accept_wrong_result_fails_and_says_which() {
    let status = judge(
        &accept(5, AuditExpect::Clean),
        &compiled(6, AuditSummary::clean()),
    );
    fail_containing(status, &["result", "expected 5", "got 6"]);
}

#[test]
fn accept_clean_expected_but_leak_fails() {
    let summary = AuditSummary {
        clean: false,
        leaks: 2,
        ..AuditSummary::default()
    };
    let status = judge(&accept(5, AuditExpect::Clean), &compiled(5, summary));
    fail_containing(status, &["audit", "expected clean", "leaks=2"]);
}

#[test]
fn accept_clean_expected_but_leak_cycle_fails() {
    let status = judge(&accept(5, AuditExpect::Clean), &compiled(5, leak_cycle()));
    fail_containing(status, &["audit", "expected clean", "leak-cycles=1"]);
}

#[test]
fn accept_clean_flag_with_errors_is_not_clean() {
    let summary = AuditSummary {
        clean: true,
        errors: vec!["use after free".to_string()],
        ..AuditSummary::default()
    };
    let status = judge(&accept(5, AuditExpect::Clean), &compiled(5, summary));
    fail_containing(status, &["audit", "use after free"]);
}

#[test]
fn accept_leak_cycle_expected_but_clean_fails() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, AuditSummary::clean()),
    );
    fail_containing(status, &["audit", "expected leak-cycle", "leak-cycles=0"]);
}

#[test]
fn accept_leak_cycle_with_other_leaks_fails() {
    let summary = AuditSummary {
        leaks: 1,
        ..leak_cycle()
    };
    let status = judge(&accept(1, AuditExpect::LeakCycle), &compiled(1, summary));
    fail_containing(status, &["audit", "leaks=1"]);
}

#[test]
fn accept_leak_cycle_with_errors_fails() {
    let summary = AuditSummary {
        errors: vec!["count went negative".to_string()],
        ..leak_cycle()
    };
    let status = judge(&accept(1, AuditExpect::LeakCycle), &compiled(1, summary));
    fail_containing(status, &["audit", "count went negative"]);
}

#[test]
fn accept_with_both_mismatches_reports_both() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(2, AuditSummary::clean()),
    );
    fail_containing(status, &["result", "audit"]);
}

#[test]
fn accept_but_rejected_fails() {
    let status = judge(&accept(5, AuditExpect::Clean), &rejected("type error"));
    fail_containing(status, &["expected accept", "type error"]);
}

#[test]
fn reject_with_matching_message_passes() {
    let status = judge(
        &reject("passed to more than one & parameter"),
        &rejected("main.fib:8: x passed to more than one & parameter in call to bar"),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn reject_with_other_message_fails_showing_both() {
    let status = judge(
        &reject("cell cannot cross"),
        &rejected("unknown symbol pmap"),
    );
    fail_containing(
        status,
        &["\"cell cannot cross\"", "\"unknown symbol pmap\""],
    );
}

#[test]
fn reject_but_compiled_fails() {
    let status = judge(
        &reject("cell cannot cross"),
        &compiled(3, AuditSummary::clean()),
    );
    fail_containing(status, &["expected reject", "compiled", "result 3"]);
}

#[test]
fn unsupported_is_pending_for_either_verdict() {
    let outcome = Outcome::Unsupported {
        reason: "no interpreter yet".to_string(),
    };
    let pending = Status::Pending("no interpreter yet".to_string());
    assert_eq!(judge(&accept(5, AuditExpect::Clean), &outcome), pending);
    assert_eq!(judge(&reject("x"), &outcome), pending);
}

#[test]
fn status_labels_and_display() {
    assert_eq!(Status::Pass.to_string(), "pass");
    assert_eq!(Status::Fail("why".to_string()).to_string(), "FAIL: why");
    assert_eq!(
        Status::Pending("later".to_string()).to_string(),
        "PENDING: later"
    );
    let header_error = Status::HeaderError(HeaderError {
        path: PathBuf::from("x.fib"),
        line: 2,
        kind: HeaderErrorKind::MissingKey("spec"),
    });
    assert_eq!(
        header_error.to_string(),
        "HEADER: line 2: missing required header key `spec`"
    );
}

#[test]
fn whitespace_only_expected_text_never_passes() {
    // Nearly every message contains a space; a header built in code
    // with `" "` would be a case that cannot fail.
    for expected in [" ", "\t", "  ", "\n"] {
        let status = judge(&reject(expected), &rejected("borrowed value does not live"));
        fail_containing(status, &["expected error text is empty or only whitespace"]);
    }
}

#[test]
fn empty_expected_text_never_passes() {
    // `message.contains("")` is always true; a pass that cannot fail is
    // not a pass, so the judge refuses an empty expected text outright.
    let status = judge(&reject(""), &rejected("anything at all"));
    fail_containing(status, &["expected error text is empty"]);
    let status = judge(&reject(""), &compiled(1, AuditSummary::clean()));
    assert!(matches!(status, Status::Fail(_)), "{status:?}");
    // Unsupported is still Pending: the evaluator never decided the case.
    let pending = Outcome::Unsupported {
        reason: "no interpreter yet".to_string(),
    };
    assert_eq!(
        judge(&reject(""), &pending),
        Status::Pending("no interpreter yet".to_string())
    );
}

#[test]
fn reject_mismatch_shows_both_strings_verbatim() {
    // Quotes and backslashes must appear as written, not Debug-escaped.
    let expected = r#"unbound symbol "foo""#;
    let got = r"no such file: C:\cases\x.fib";
    let status = judge(&reject(expected), &rejected(got));
    fail_containing(status, &[expected, got]);
    let status = judge(&reject(expected), &compiled(3, AuditSummary::clean()));
    fail_containing(status, &[expected, "result 3"]);
}
