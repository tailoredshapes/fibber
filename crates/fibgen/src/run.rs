//! Running a generated program through the reference interpreter with a
//! time limit, and classifying what came back.

use std::sync::mpsc;
use std::time::Duration;

use fibref::cases::{AuditSummary, Outcome, Value};
use fibref::roots::Roots;

use crate::model::ModelError;

/// What the interpreter did with a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observed {
    /// It finished within the limit.
    Done(Outcome),
    /// It did not finish within the limit (the run is abandoned).
    Timeout,
}

/// Runs `source` through `fibref::eval::run_source` on its own thread and
/// waits at most `limit` for it.
pub fn run_with_limit(source: &str, limit: Duration) -> Observed {
    run_with_limit_in(source, limit, &Roots::default())
}

/// [`run_with_limit`] with the library found under `roots` first (the
/// built-in copy of `lib/` last), which is how a scratch copy of the
/// library with a planted fault is judged.
pub fn run_with_limit_in(source: &str, limit: Duration, roots: &Roots) -> Observed {
    let (tx, rx) = mpsc::channel();
    let src = source.to_string();
    let roots = roots.clone();
    let spawned = std::thread::Builder::new()
        .name("fibgen-run".into())
        .spawn(move || {
            let (outcome, _) = fibref::eval::run_source_in(&src, "<gen>", &[], &roots);
            // The receiver is gone only after a timeout; nothing to do then.
            let _ = tx.send(outcome);
        });
    if let Err(e) = spawned {
        return Observed::Done(Outcome::Failed {
            message: format!("fibgen: cannot start a run thread: {e}"),
        });
    }
    match rx.recv_timeout(limit) {
        Ok(o) => Observed::Done(o),
        Err(_) => Observed::Timeout,
    }
}

/// The class of one program's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Class {
    /// Accepted, ran, clean audit, the model's result.
    Ok,
    /// Accepted and trapped with the message the model predicts (an
    /// operation left unguarded on purpose, types §2.12), no audit error
    /// before the trap.
    ExpectedTrap,
    /// Accepted and ran, but the audit is not clean. FINDING.
    AuditFailure,
    /// Accepted and ran clean, but the result differs from the model's. FINDING.
    Mismatch,
    /// Accepted, but the run stopped (an audit error that stopped it, a
    /// plan gap, an internal error, a trap the generator did not intend). FINDING.
    RunFailed,
    /// The evaluator panicked. FINDING.
    Panic,
    /// The run did not finish in time. FINDING.
    Hang,
    /// Rejected, though the generator built it to be well-typed and
    /// ownership-legal: POSSIBLE-OVERREJECTION.
    Rejected,
    /// The evaluator reported the program as unsupported (Pending).
    Unsupported,
    /// The model could not evaluate the program: a generator bug, not a
    /// finding about the interpreter.
    ModelGap,
}

impl Class {
    /// Whether the class is a finding against the interpreter.
    pub fn is_finding(self) -> bool {
        matches!(
            self,
            Class::AuditFailure | Class::Mismatch | Class::RunFailed | Class::Panic | Class::Hang
        )
    }

    /// The label used in reports.
    pub fn label(self) -> &'static str {
        match self {
            Class::Ok => "ok",
            Class::ExpectedTrap => "ok (trapped as the model predicts)",
            Class::AuditFailure => "FINDING audit-failure",
            Class::Mismatch => "FINDING result-mismatch",
            Class::RunFailed => "FINDING run-failed",
            Class::Panic => "FINDING panic",
            Class::Hang => "FINDING hang",
            Class::Rejected => "POSSIBLE-OVERREJECTION",
            Class::Unsupported => "pending (unsupported)",
            Class::ModelGap => "generator/model gap",
        }
    }
}

/// A classified result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The class.
    pub class: Class,
    /// A key that groups results failing the same way (numbers and
    /// positions removed), for deduplication and minimisation.
    pub key: String,
    /// What was observed, in full.
    pub detail: String,
}

/// Whether an audit summary is what an `accept ... audit: clean` header requires.
fn clean(a: &AuditSummary) -> bool {
    a.clean && a.leaks == 0 && a.leak_cycles == 0 && a.errors.is_empty()
}

/// Classifies `obs` against the model's `expected` result.
pub fn classify(obs: &Observed, expected: &Result<i64, ModelError>) -> Verdict {
    let v = |class, key: String, detail: String| Verdict { class, key, detail };
    let outcome = match obs {
        Observed::Timeout => {
            return v(
                Class::Hang,
                "hang".into(),
                "no result within the time limit".into(),
            )
        }
        Observed::Done(o) => o,
    };
    match outcome {
        Outcome::Rejected { message } => v(Class::Rejected, normalise(message), message.clone()),
        Outcome::Unsupported { reason } => v(Class::Unsupported, normalise(reason), reason.clone()),
        Outcome::Failed { message } if message.contains("evaluator panicked") => {
            v(Class::Panic, "panic".into(), message.clone())
        }
        // A trap the model predicts, with its message: what the spec
        // requires (method.md rule 3, `trap`), if nothing went wrong in
        // the heap before it.
        Outcome::Trapped { message, errors } if predicted(expected, message) => {
            if errors.is_empty() {
                v(Class::ExpectedTrap, normalise(message), message.clone())
            } else {
                let key = format!("audit before a trap: {}", normalise(&errors.join("; ")));
                v(Class::AuditFailure, key, format!("{message}; {errors:?}"))
            }
        }
        Outcome::Failed { message } | Outcome::Trapped { message, .. } => {
            v(Class::RunFailed, normalise(message), message.clone())
        }
        Outcome::Compiled {
            result: Value::Int(n),
            audit,
        } => {
            let detail = format!("result {n}, audit {audit}, model {expected:?}");
            if !clean(audit) {
                return v(Class::AuditFailure, audit_key(audit), detail);
            }
            match expected {
                Ok(e) if e == n => v(Class::Ok, String::new(), detail),
                Ok(_) => v(Class::Mismatch, "mismatch".into(), detail),
                Err(ModelError::Trap(_)) => v(Class::Mismatch, "model traps".into(), detail),
                Err(_) => v(Class::ModelGap, format!("{expected:?}"), detail),
            }
        }
    }
}

/// Whether the model predicts the trap `message` reports.
fn predicted(expected: &Result<i64, ModelError>, message: &str) -> bool {
    matches!(expected, Err(ModelError::Trap(m)) if message.contains(&format!("trap: {m}")))
}

fn audit_key(a: &AuditSummary) -> String {
    let mut parts = Vec::new();
    if a.leaks > 0 {
        parts.push("leak".to_string());
    }
    if a.leak_cycles > 0 {
        parts.push("leak-cycle".to_string());
    }
    for e in &a.errors {
        let e = normalise(e);
        if !parts.contains(&e) {
            parts.push(e);
        }
    }
    format!("audit: {}", parts.join("; "))
}

/// Whether `head` is a source position `file:line:col`.
fn is_position(head: &str) -> bool {
    let mut parts = head.rsplitn(3, ':');
    let col = parts.next().unwrap_or("");
    let line = parts.next().unwrap_or("");
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    numeric(col) && numeric(line) && parts.next().is_some()
}

/// `msg` without its leading position, object ids and numbers, cut to
/// 100 characters.
pub fn normalise(msg: &str) -> String {
    let msg = match msg.split_once(": ") {
        Some((head, tail)) if is_position(head) => tail,
        _ => msg,
    };
    let mut out = String::new();
    let mut last_digit = false;
    for c in msg.chars() {
        if c.is_ascii_digit() {
            if !last_digit {
                out.push('N');
            }
            last_digit = true;
        } else {
            out.push(c);
            last_digit = false;
        }
    }
    out.chars().take(100).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compiled(n: i64, audit: AuditSummary) -> Observed {
        Observed::Done(Outcome::Compiled {
            result: Value::Int(n),
            audit,
        })
    }

    #[test]
    fn clean_and_equal_is_ok() {
        assert_eq!(
            classify(&compiled(3, AuditSummary::clean()), &Ok(3)).class,
            Class::Ok
        );
    }

    #[test]
    fn wrong_result_is_a_mismatch() {
        assert_eq!(
            classify(&compiled(4, AuditSummary::clean()), &Ok(3)).class,
            Class::Mismatch
        );
        // A trap the model predicts: the interpreter must trap the same
        // way (then it is the generator's program, not a finding).
        let overflow = Err(ModelError::Trap("integer overflow in * at i64".into()));
        let v = classify(&compiled(4, AuditSummary::clean()), &overflow);
        assert_eq!((v.class, v.key.as_str()), (Class::Mismatch, "model traps"));
        let trapped = Observed::Done(Outcome::Trapped {
            message: "<gen>:1:2: trap: integer overflow in * at i64".into(),
            errors: Vec::new(),
        });
        assert_eq!(classify(&trapped, &overflow).class, Class::ExpectedTrap);
        let other = Err(ModelError::Trap("integer rem by zero".into()));
        assert_eq!(classify(&trapped, &other).class, Class::RunFailed);
    }

    #[test]
    fn a_leak_is_an_audit_failure_even_with_the_right_result() {
        let audit = AuditSummary {
            clean: false,
            leak_cycles: 0,
            leaks: 2,
            errors: Vec::new(),
        };
        let v = classify(&compiled(3, audit), &Ok(3));
        assert_eq!(
            (v.class, v.key.as_str()),
            (Class::AuditFailure, "audit: leak")
        );
    }

    #[test]
    fn rejection_and_panic_and_hang_are_told_apart() {
        let rej = Observed::Done(Outcome::Rejected {
            message: "<gen>:3:4: cannot unify i64 with str".into(),
        });
        let v = classify(&rej, &Ok(0));
        assert_eq!(
            (v.class, v.key.as_str()),
            (Class::Rejected, "cannot unify iN with str")
        );
        let panic = Observed::Done(Outcome::Failed {
            message: "internal error: the evaluator panicked".into(),
        });
        assert_eq!(classify(&panic, &Ok(0)).class, Class::Panic);
        assert_eq!(classify(&Observed::Timeout, &Ok(0)).class, Class::Hang);
    }

    #[test]
    fn positions_are_dropped_from_keys() {
        assert_eq!(
            normalise("lib/prelude.fib:12:3: audit error: X"),
            "audit error: X"
        );
        assert_eq!(
            normalise("audit error: StackRefInHeap { id: ObjId(7) }"),
            "audit error: StackRefInHeap { id: ObjId(N) }"
        );
    }

    #[test]
    fn run_with_limit_runs_a_program() {
        let o = run_with_limit("(defun main () -> i64 (+ 1 2))", Duration::from_secs(60));
        assert_eq!(classify(&o, &Ok(3)).class, Class::Ok);
    }
}
