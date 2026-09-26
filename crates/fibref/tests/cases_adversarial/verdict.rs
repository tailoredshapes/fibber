//! Judging a case: every header/outcome combination from
//! `spec/method.md` rule 3, including the audit edge cases.

use fibref::cases::{
    judge, AuditExpect, AuditSummary, Expected, Header, Outcome, Status, Value, Verdict,
};

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

fn unsupported() -> Outcome {
    Outcome::Unsupported {
        reason: "no interpreter yet".to_string(),
    }
}

fn leak_cycle(cycles: usize, leaks: usize) -> AuditSummary {
    AuditSummary {
        clean: false,
        leak_cycles: cycles,
        leaks,
        errors: vec![],
    }
}

/// Asserts a Fail whose detail mentions every string in `mentions`.
fn assert_fail(status: &Status, mentions: &[&str]) {
    match status {
        Status::Fail(detail) => {
            for needle in mentions {
                assert!(
                    detail.contains(needle),
                    "Fail detail must say {needle:?}, got {detail:?}"
                );
            }
        }
        other => panic!("expected Fail mentioning {mentions:?}, got {other:?}"),
    }
}

// ---- accept ------------------------------------------------------------------

#[test]
fn accept_compiled_matching_result_and_clean_audit_passes() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(7, AuditSummary::clean()),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn accept_compiled_wrong_result_fails_and_says_result() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(8, AuditSummary::clean()),
    );
    assert_fail(&status, &["result", "7", "8"]);
}

#[test]
fn accept_compiled_correct_result_but_leak_cycle_when_clean_expected_fails() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(7, leak_cycle(1, 0)),
    );
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_clean_with_plain_leak_fails() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(7, leak_cycle(0, 1)),
    );
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_clean_with_an_audit_error_fails() {
    let summary = AuditSummary {
        clean: false,
        leak_cycles: 0,
        leaks: 0,
        errors: vec!["use after free of #2".to_string()],
    };
    let status = judge(&accept(7, AuditExpect::Clean), &compiled(7, summary));
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_clean_with_clean_flag_false_and_nothing_else_fails() {
    // The evaluator says "not clean" without saying why; the harness
    // must believe it, not reconstruct clean from the counts.
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(7, leak_cycle(0, 0)),
    );
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_clean_with_clean_flag_true_but_errors_listed_fails() {
    // An inconsistent summary is never a pass.
    let summary = AuditSummary {
        clean: true,
        leak_cycles: 0,
        leaks: 0,
        errors: vec!["double free of #1".to_string()],
    };
    let status = judge(&accept(7, AuditExpect::Clean), &compiled(7, summary));
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_wrong_result_and_wrong_audit_names_both() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &compiled(8, leak_cycle(0, 1)),
    );
    assert_fail(&status, &["result", "audit"]);
}

#[test]
fn accept_leak_cycle_with_only_cycles_passes() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, leak_cycle(2, 0)),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn accept_leak_cycle_with_cycles_and_plain_leaks_fails() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, leak_cycle(1, 1)),
    );
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_leak_cycle_with_cycles_and_errors_fails() {
    let summary = AuditSummary {
        clean: false,
        leak_cycles: 1,
        leaks: 0,
        errors: vec!["count went negative on #4".to_string()],
    };
    let status = judge(&accept(1, AuditExpect::LeakCycle), &compiled(1, summary));
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_leak_cycle_with_a_clean_audit_fails() {
    // The case expects a leak; a clean run means the rule did not fire.
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(1, AuditSummary::clean()),
    );
    assert_fail(&status, &["audit"]);
}

#[test]
fn accept_leak_cycle_with_wrong_result_fails_even_when_audit_matches() {
    let status = judge(
        &accept(1, AuditExpect::LeakCycle),
        &compiled(2, leak_cycle(1, 0)),
    );
    assert_fail(&status, &["result"]);
}

#[test]
fn accept_rejected_fails_and_shows_the_message() {
    let status = judge(
        &accept(7, AuditExpect::Clean),
        &rejected("type error: no such function"),
    );
    assert_fail(&status, &["type error: no such function"]);
}

#[test]
fn accept_unsupported_is_pending_not_pass() {
    let status = judge(&accept(7, AuditExpect::Clean), &unsupported());
    assert_eq!(status, Status::Pending("no interpreter yet".to_string()));
}

// ---- reject ------------------------------------------------------------------

#[test]
fn reject_rejected_with_exact_message_passes() {
    let status = judge(&reject("passed twice"), &rejected("passed twice"));
    assert_eq!(status, Status::Pass);
}

#[test]
fn reject_rejected_with_expected_text_inside_a_longer_message_passes() {
    let status = judge(
        &reject("passed to more than one & parameter"),
        &rejected("error at 7:5: binding `x` passed to more than one & parameter in call to bar"),
    );
    assert_eq!(status, Status::Pass);
}

#[test]
fn reject_rejected_differing_only_in_case_fails_and_shows_both() {
    let status = judge(
        &reject("passed to more than one & parameter"),
        &rejected("Passed To More Than One & Parameter"),
    );
    assert_fail(
        &status,
        &[
            "passed to more than one & parameter",
            "Passed To More Than One & Parameter",
        ],
    );
}

#[test]
fn reject_rejected_with_unrelated_message_fails_and_shows_both() {
    let status = judge(
        &reject("cell crosses thread"),
        &rejected("unbound symbol foo"),
    );
    assert_fail(&status, &["cell crosses thread", "unbound symbol foo"]);
}

#[test]
fn reject_rejected_with_empty_message_fails() {
    let status = judge(&reject("cell crosses thread"), &rejected(""));
    assert_fail(&status, &["cell crosses thread"]);
}

#[test]
fn reject_rejected_with_expected_text_split_by_whitespace_fails() {
    // Substring match is literal: extra whitespace inside breaks it.
    let status = judge(
        &reject("cell crosses thread"),
        &rejected("cell  crosses thread"),
    );
    assert_fail(&status, &["cell crosses thread"]);
}

#[test]
fn reject_compiled_fails() {
    let status = judge(
        &reject("cell crosses thread"),
        &compiled(1, AuditSummary::clean()),
    );
    assert_fail(&status, &["cell crosses thread"]);
}

#[test]
fn reject_compiled_with_dirty_audit_still_fails_as_compiled() {
    let status = judge(
        &reject("cell crosses thread"),
        &compiled(1, leak_cycle(1, 1)),
    );
    assert!(matches!(status, Status::Fail(_)), "{status:?}");
}

#[test]
fn reject_unsupported_is_pending_not_pass() {
    let status = judge(&reject("cell crosses thread"), &unsupported());
    assert_eq!(status, Status::Pending("no interpreter yet".to_string()));
}

// ---- status rendering ---------------------------------------------------------

#[test]
fn pass_has_no_detail_and_others_do() {
    assert_eq!(Status::Pass.detail(), "");
    assert_eq!(Status::Fail("why".to_string()).detail(), "why");
    assert_eq!(Status::Pending("why".to_string()).detail(), "why");
}

#[test]
fn labels_distinguish_every_status() {
    let labels = [
        Status::Pass.label(),
        Status::Fail(String::new()).label(),
        Status::Pending(String::new()).label(),
    ];
    assert_ne!(labels[0], labels[1]);
    assert_ne!(labels[1], labels[2]);
    assert_ne!(labels[0], labels[2]);
    assert!(
        labels[2].to_ascii_uppercase().contains("PENDING"),
        "pending must be labelled as such: {}",
        labels[2]
    );
}
