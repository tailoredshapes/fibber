//! `judge_labelled`: an `open` case is OPEN while the program fails as
//! the label says, a failure the day it passes, and a failure of the
//! tools themselves is never excused.

use super::*;

fn accept() -> Header {
    Header {
        spec: "§5.5".to_string(),
        verdict: Verdict::Accept {
            result: Expected::Int(7),
            audit: AuditExpect::Clean,
            allocs: None,
        },
    }
}

fn items() -> Vec<String> {
    vec!["L20".to_string(), "C9".to_string()]
}

fn ran(result: i64, audit: AuditSummary) -> Outcome {
    Outcome::Compiled {
        result: Value::Int(result),
        audit,
    }
}

fn judged(open: &[String], outcome: &Outcome) -> Status {
    judge_labelled(&accept(), open, outcome, None)
}

fn audit_error() -> AuditSummary {
    AuditSummary {
        clean: false,
        errors: vec!["results differ: interpreter 7, compiled 8".to_string()],
        ..AuditSummary::default()
    }
}

#[test]
fn without_items_it_is_judge_counted() {
    for outcome in [
        ran(7, AuditSummary::clean()),
        ran(8, AuditSummary::clean()),
        Outcome::Rejected {
            message: "cannot unify".into(),
        },
    ] {
        assert_eq!(
            judged(&[], &outcome),
            judge_counted(&accept(), &outcome, None)
        );
    }
}

#[test]
fn a_refused_program_is_open_and_names_the_items_and_the_message() {
    let outcome = Outcome::Rejected {
        message: "cannot unify (Option i64) with bool".into(),
    };
    match judged(&items(), &outcome) {
        Status::Open(why) => {
            assert!(why.starts_with("L20 C9: "), "{why}");
            assert!(why.contains("cannot unify (Option i64) with bool"), "{why}");
        }
        other => panic!("expected Open, got {other:?}"),
    }
}

#[test]
fn a_wrong_answer_with_a_clean_audit_and_a_trap_are_open() {
    assert!(matches!(
        judged(&items(), &ran(8, AuditSummary::clean())),
        Status::Open(_)
    ));
    let trapped = Outcome::Trapped {
        message: "nth: index out of range".into(),
        errors: vec![],
    };
    assert!(matches!(judged(&items(), &trapped), Status::Open(_)));
}

#[test]
fn a_pass_is_a_failure_the_item_landed() {
    let status = judged(&items(), &ran(7, AuditSummary::clean()));
    assert_eq!(status, Status::OpenPassed);
    assert_eq!(status.label(), "FAIL");
    assert_eq!(status.detail(), "the item landed: remove `open`");
}

#[test]
fn a_failure_of_the_tools_is_not_excused() {
    let disagree = Outcome::Failed {
        message: "the two sides disagree: interpreter result 7, compiled rejected".into(),
    };
    assert!(matches!(judged(&items(), &disagree), Status::Fail(_)));
    assert!(matches!(
        judged(&items(), &ran(7, audit_error())),
        Status::Fail(_)
    ));
    let leak = AuditSummary {
        clean: false,
        leaks: 2,
        ..AuditSummary::default()
    };
    assert!(matches!(judged(&items(), &ran(8, leak)), Status::Fail(_)));
    let bad_trap = Outcome::Trapped {
        message: "x".into(),
        errors: vec!["trap messages differ".into()],
    };
    assert!(matches!(judged(&items(), &bad_trap), Status::Fail(_)));
}

#[test]
fn pending_stays_pending() {
    let outcome = Outcome::Unsupported {
        reason: "later".into(),
    };
    assert_eq!(judged(&items(), &outcome), Status::Pending("later".into()));
}

#[test]
fn open_is_not_a_pass_and_has_its_own_label() {
    let status = Status::Open("L20: why".into());
    assert_eq!(status.label(), "OPEN");
    assert_eq!(status.detail(), "L20: why");
    assert_eq!(status.to_string(), "OPEN: L20: why");
}
