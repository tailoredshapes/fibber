//! Unit tests of the evaluator: small programs through the whole
//! pipeline, one file per concern.

mod atoms;
mod calls;
mod macros;
mod memory;
mod objects;
mod options;
mod patterns;
mod stack;
mod threads;
mod values;

use crate::cases::{Evaluator, Outcome, Value};
use crate::heap::{AuditReport, Event};

use super::{run_checked, Interpreter};

/// Runs `src` and returns the outcome.
fn run(src: &str) -> Outcome {
    Interpreter.run(src)
}

/// Asserts `src` runs to `n` with a clean audit.
fn clean(src: &str, n: i64) {
    match run(src) {
        Outcome::Compiled { result, audit } => {
            assert_eq!(result, Value::Int(n), "{src}");
            assert!(audit.clean, "{src}: {audit}");
        }
        other => panic!("{src}: {other:?}"),
    }
}

/// The message of a run that fails, by a trap (with a clean audit at
/// the abort, types §2.11) or otherwise.
fn failed(src: &str) -> String {
    match run(src) {
        Outcome::Failed { message } => message,
        Outcome::Trapped { message, errors } if errors.is_empty() => message,
        other => panic!("{src}: expected a failed run, got {other:?}"),
    }
}

/// `main`'s result and the heap's report, for tests that read the
/// trace. Runs on a thread with the evaluator's stack.
fn traced(src: &str) -> (i64, AuditReport) {
    let src = src.to_string();
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .stack_size(super::STACK_BYTES)
            .spawn_scoped(s, move || {
                let c = crate::own::check_source(&src, "<test>").expect("checks");
                let (r, report) = run_checked(&c);
                (r.expect("runs"), report)
            })
            .expect("thread")
            .join()
            .expect("no panic")
    })
}

/// How many events of the trace satisfy `f`.
fn count(report: &AuditReport, f: impl Fn(&Event) -> bool) -> usize {
    report.trace.iter().filter(|e| f(e)).count()
}
