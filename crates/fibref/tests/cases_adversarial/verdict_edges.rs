//! Judging, round 2: the expected error text is a literal substring
//! (not a pattern, not case-folded, not whitespace-normalised), Fail
//! details show the strings a reader needs verbatim, and integer
//! results compare exactly at the extremes.

use fibref::cases::{
    judge, AuditExpect, AuditSummary, Expected, Header, HeaderError, HeaderErrorKind, Outcome,
    Status, Value, Verdict,
};
use std::path::PathBuf;

fn accept(result: i64, audit: AuditExpect) -> Header {
    Header {
        spec: "§4".to_string(),
        verdict: Verdict::Accept {
            result: Expected::Int(result),
            audit,
            allocs: None,
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

fn fail_detail(status: Status) -> String {
    match status {
        Status::Fail(detail) => detail,
        other => panic!("expected Fail, got {other:?}"),
    }
}

// ---- literal substring ---------------------------------------------------------

#[test]
fn expected_text_is_a_literal_substring_not_a_pattern() {
    // `.` must not match any character, and `[x]` must not be a class.
    assert!(matches!(
        judge(&reject("a.b"), &rejected("got axb")),
        Status::Fail(_)
    ));
    assert_eq!(judge(&reject("a.b"), &rejected("got a.b")), Status::Pass);
    assert!(matches!(
        judge(&reject("[x]"), &rejected("got x")),
        Status::Fail(_)
    ));
    assert_eq!(judge(&reject("[x]"), &rejected("got [x]")), Status::Pass);
    assert!(matches!(
        judge(&reject("a*"), &rejected("aaa")),
        Status::Fail(_)
    ));
}

#[test]
fn expected_text_at_the_start_or_end_of_the_message_passes() {
    assert_eq!(
        judge(&reject("cell cannot"), &rejected("cell cannot be shared")),
        Status::Pass
    );
    assert_eq!(
        judge(&reject("be shared"), &rejected("cell cannot be shared")),
        Status::Pass
    );
}

#[test]
fn expected_text_split_by_a_newline_in_the_message_fails() {
    assert!(matches!(
        judge(
            &reject("cell cannot be shared"),
            &rejected("cell cannot\nbe shared")
        ),
        Status::Fail(_)
    ));
}

#[test]
fn expected_text_differing_by_one_character_fails() {
    assert!(matches!(
        judge(
            &reject("& parameter in async function"),
            &rejected("& parameter in async functions")
        ),
        Status::Pass
    ));
    assert!(matches!(
        judge(
            &reject("& parameter in async function"),
            &rejected("& parameter in async fnction")
        ),
        Status::Fail(_)
    ));
}

#[test]
fn non_ascii_expected_text_matches_by_exact_bytes() {
    assert_eq!(judge(&reject("§5"), &rejected("see §5")), Status::Pass);
    assert!(matches!(
        judge(&reject("§5"), &rejected("see S5")),
        Status::Fail(_)
    ));
}

#[test]
fn expected_text_longer_than_the_message_fails() {
    assert!(matches!(
        judge(&reject("cell cannot be shared"), &rejected("cell")),
        Status::Fail(_)
    ));
}

#[test]
fn empty_expected_error_does_not_pass_vacuously() {
    // Interpretation: the contract requires `error` to be a non-empty
    // substring. A Header built with an empty one (the fields are
    // public) must not turn every rejection into a Pass; a pass that
    // cannot fail is worse than none (spec/method.md). See the spec
    // questions in the report.
    let status = judge(&reject(""), &rejected("anything at all"));
    assert_ne!(
        status,
        Status::Pass,
        "empty expected text matched vacuously"
    );
}

// ---- what a Fail shows ---------------------------------------------------------

#[test]
fn reject_mismatch_detail_shows_both_strings_verbatim_when_they_contain_quotes() {
    // Interpretation: "show both strings" means the reader can find the
    // strings as written; `"` and `\` inside them must not be escaped
    // into something the compiler never printed. See the spec questions.
    let expected = r#"unbound symbol "foo""#;
    let got = r"no such file: C:\cases\x.fib";
    let detail = fail_detail(judge(&reject(expected), &rejected(got)));
    assert!(
        detail.contains(expected),
        "detail must show the expected text verbatim: {detail}"
    );
    assert!(
        detail.contains(got),
        "detail must show the message verbatim: {detail}"
    );
}

#[test]
fn reject_compiled_detail_shows_the_expected_text_and_the_result() {
    let detail = fail_detail(judge(
        &reject("cell cannot be shared"),
        &compiled(42, AuditSummary::clean()),
    ));
    assert!(detail.contains("cell cannot be shared"), "{detail}");
    assert!(detail.contains("42"), "{detail}");
}

#[test]
fn accept_rejected_detail_shows_the_whole_message() {
    let detail = fail_detail(judge(
        &accept(1, AuditExpect::Clean),
        &rejected("error at 3:1: unbound symbol conj"),
    ));
    assert!(
        detail.contains("error at 3:1: unbound symbol conj"),
        "{detail}"
    );
}

#[test]
fn accept_audit_mismatch_detail_names_the_expected_audit_and_the_counts() {
    let summary = AuditSummary {
        clean: false,
        leak_cycles: 3,
        leaks: 2,
        errors: vec!["double free of #7".to_string()],
    };
    let detail = fail_detail(judge(&accept(1, AuditExpect::Clean), &compiled(1, summary)));
    assert!(
        detail.contains("clean"),
        "must name what was expected: {detail}"
    );
    assert!(detail.contains('3'), "must show leak-cycles: {detail}");
    assert!(detail.contains('2'), "must show leaks: {detail}");
    assert!(
        detail.contains("double free of #7"),
        "must show the audit error: {detail}"
    );
}

#[test]
fn accept_leak_cycle_mismatch_detail_names_leak_cycle() {
    let detail = fail_detail(judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, AuditSummary::clean()),
    ));
    assert!(detail.contains("leak-cycle"), "{detail}");
}

#[test]
fn accept_result_mismatch_detail_does_not_blame_the_audit() {
    let detail = fail_detail(judge(
        &accept(1, AuditExpect::Clean),
        &compiled(2, AuditSummary::clean()),
    ));
    assert!(detail.contains("result"), "{detail}");
    assert!(
        !detail.contains("audit"),
        "audit matched, must not be blamed: {detail}"
    );
}

#[test]
fn accept_audit_mismatch_detail_does_not_blame_the_result() {
    let summary = AuditSummary {
        clean: false,
        leak_cycles: 0,
        leaks: 1,
        errors: vec![],
    };
    let detail = fail_detail(judge(&accept(1, AuditExpect::Clean), &compiled(1, summary)));
    assert!(detail.contains("audit"), "{detail}");
    assert!(
        !detail.contains("result"),
        "result matched, must not be blamed: {detail}"
    );
}

// ---- integers ------------------------------------------------------------------

#[test]
fn integer_results_compare_exactly_at_the_extremes() {
    assert_eq!(
        judge(
            &accept(i64::MAX, AuditExpect::Clean),
            &compiled(i64::MAX, AuditSummary::clean())
        ),
        Status::Pass
    );
    assert_eq!(
        judge(
            &accept(i64::MIN, AuditExpect::Clean),
            &compiled(i64::MIN, AuditSummary::clean())
        ),
        Status::Pass
    );
    let detail = fail_detail(judge(
        &accept(i64::MAX, AuditExpect::Clean),
        &compiled(i64::MIN, AuditSummary::clean()),
    ));
    assert!(detail.contains(&i64::MAX.to_string()), "{detail}");
    assert!(detail.contains(&i64::MIN.to_string()), "{detail}");
}

// ---- status rendering ----------------------------------------------------------

#[test]
fn header_error_status_display_names_line_and_reason() {
    let status = Status::HeaderError(HeaderError {
        path: PathBuf::from("cases/x.fib"),
        line: 3,
        kind: HeaderErrorKind::UnknownKey("bogus".to_string()),
    });
    let text = status.to_string();
    assert!(text.contains('3'), "{text}");
    assert!(text.contains("bogus"), "{text}");
    assert_ne!(status.label(), Status::Pass.label());
    assert_ne!(status.label(), Status::Fail(String::new()).label());
    assert_ne!(status.label(), Status::Pending(String::new()).label());
}

#[test]
fn pending_reason_is_kept_verbatim() {
    let outcome = Outcome::Unsupported {
        reason: "weak references: §6 not implemented".to_string(),
    };
    assert_eq!(
        judge(&accept(1, AuditExpect::Clean), &outcome),
        Status::Pending("weak references: §6 not implemented".to_string())
    );
    assert_eq!(
        judge(&reject("x"), &outcome),
        Status::Pending("weak references: §6 not implemented".to_string())
    );
}
