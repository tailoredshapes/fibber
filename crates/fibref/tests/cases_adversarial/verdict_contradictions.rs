//! Round 3: audit summaries whose fields contradict each other, and
//! expected texts that are technically non-empty but would match almost
//! anything. The contract: `audit: clean` means clean; `audit:
//! leak-cycle` means `leak_cycles > 0`, `leaks == 0` and no errors; a
//! reject case passes when the message contains the expected text; a
//! test that cannot fail is worse than no test.

use fibref::cases::{
    judge, AuditExpect, AuditSummary, Expected, Header, Outcome, Status, Value, Verdict,
};

fn accept(audit: AuditExpect) -> Header {
    Header {
        spec: "§4".to_string(),
        verdict: Verdict::Accept {
            result: Expected::Int(1),
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

fn compiled(audit: AuditSummary) -> Outcome {
    Outcome::Compiled {
        result: Value::Int(1),
        audit,
    }
}

fn summary(clean: bool, leak_cycles: usize, leaks: usize, errors: &[&str]) -> AuditSummary {
    AuditSummary {
        clean,
        leak_cycles,
        leaks,
        errors: errors.iter().map(|e| e.to_string()).collect(),
    }
}

fn is_fail(status: &Status) -> bool {
    matches!(status, Status::Fail(_))
}

#[test]
fn clean_flag_true_with_a_plain_leak_is_not_clean() {
    let status = judge(
        &accept(AuditExpect::Clean),
        &compiled(summary(true, 0, 1, &[])),
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn clean_flag_true_with_a_leak_cycle_is_not_clean() {
    let status = judge(
        &accept(AuditExpect::Clean),
        &compiled(summary(true, 1, 0, &[])),
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn clean_flag_false_with_nothing_counted_is_not_clean() {
    // The heap said "not clean" and gave no reason; the harness must
    // not overrule it.
    let status = judge(
        &accept(AuditExpect::Clean),
        &compiled(summary(false, 0, 0, &[])),
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn leak_cycle_with_clean_flag_true_and_one_cycle_passes_by_the_counts() {
    // Interpretation: the contract defines leak-cycle purely by the
    // counts (`leak_cycles > 0`, `leaks == 0`, no errors) and does not
    // mention the flag, so a contradictory `clean: true` is ignored.
    // Pinned so a change to trust the flag is visible.
    let status = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(true, 1, 0, &[])),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn leak_cycle_with_clean_flag_false_and_nothing_counted_fails() {
    let status = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(false, 0, 0, &[])),
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn leak_cycle_with_an_error_and_no_leaks_at_all_fails() {
    let status = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(false, 0, 0, &["use after free of #2"])),
    );
    assert!(is_fail(&status), "{status}");
    assert!(status.detail().contains("use after free of #2"), "{status}");
}

#[test]
fn leak_cycle_with_the_maximum_count_passes_and_with_maximum_leaks_fails() {
    let ok = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(false, usize::MAX, 0, &[])),
    );
    assert_eq!(ok, Status::Pass);
    let bad = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(false, usize::MAX, usize::MAX, &[])),
    );
    assert!(is_fail(&bad), "{bad}");
}

#[test]
fn an_empty_error_string_in_the_audit_is_still_an_error() {
    // An error entry with no text is still a check that fired.
    let status = judge(
        &accept(AuditExpect::Clean),
        &compiled(summary(true, 0, 0, &[""])),
    );
    assert!(is_fail(&status), "{status}");
    let status = judge(
        &accept(AuditExpect::LeakCycle),
        &compiled(summary(false, 1, 0, &[""])),
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn a_whitespace_only_expected_error_cannot_pass_vacuously() {
    // Interpretation: the judge already refuses an empty expected text
    // because every message contains it. A single space is the same
    // vacuity for every message with two words, and the header parser
    // trims it to empty from files; a header built in code with `" "`
    // must not be a case that cannot fail.
    for expected in [" ", "\t", "  "] {
        let status = judge(
            &reject(expected),
            &Outcome::Rejected {
                message: "borrowed value does not live long enough".to_string(),
            },
        );
        assert!(
            is_fail(&status),
            "expected {expected:?} must not pass by matching whitespace: {status}"
        );
    }
}

#[test]
fn expected_text_equal_to_the_whole_message_with_trailing_newline_passes() {
    let status = judge(
        &reject("cell crosses thread"),
        &Outcome::Rejected {
            message: "cell crosses thread\n".to_string(),
        },
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn expected_text_found_only_across_a_carriage_return_fails() {
    let status = judge(
        &reject("cell crosses thread"),
        &Outcome::Rejected {
            message: "cell crosses\r\nthread".to_string(),
        },
    );
    assert!(is_fail(&status), "{status}");
}

#[test]
fn accept_with_a_dirty_audit_and_a_wrong_result_reports_both_in_one_detail() {
    let header = accept(AuditExpect::Clean);
    let outcome = Outcome::Compiled {
        result: Value::Int(2),
        audit: summary(false, 0, 3, &["double free of #1"]),
    };
    let status = judge(&header, &outcome);
    let detail = status.detail();
    assert!(is_fail(&status), "{status}");
    for needle in ["expected 1", "got 2", "leaks=3", "double free of #1"] {
        assert!(detail.contains(needle), "missing {needle:?} in {detail}");
    }
}

#[test]
fn reject_compiled_with_a_leak_cycle_still_fails_as_compiled() {
    let status = judge(&reject("x"), &compiled(summary(false, 1, 0, &[])));
    assert!(is_fail(&status), "{status}");
    assert!(status.detail().contains("compiled"), "{status}");
}

#[test]
fn unsupported_with_an_empty_reason_is_still_pending() {
    let outcome = Outcome::Unsupported {
        reason: String::new(),
    };
    assert_eq!(
        judge(&accept(AuditExpect::Clean), &outcome),
        Status::Pending(String::new())
    );
    assert_eq!(
        judge(&reject("x"), &outcome),
        Status::Pending(String::new())
    );
    assert_eq!(judge(&reject(""), &outcome), Status::Pending(String::new()));
}
