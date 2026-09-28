//! The interpreter's side of the comparison (compiler.md §5 step 2):
//! a case run in `fibref`, giving its outcome and its canonical trace.

use fibref::cases::{Outcome, Value};
use fibref::eval::pipeline::{abort_errors, summary};
use fibref::eval::{run_checked, RunErrorKind, STACK_BYTES};

use crate::front::{check, Front};
use crate::trace::Trace;

/// What the interpreter said: the outcome and, when it ran, the trace
/// of its heap (empty when it did not run).
pub struct InterpRun {
    pub outcome: Outcome,
    pub trace: Trace,
}

/// Runs `source` in the reference interpreter on a thread with the
/// stack the evaluator needs.
pub fn run(source: &str, file: &str) -> InterpRun {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibc-interp".into())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, || run_here(source, file));
        match worker.map(|h| h.join()) {
            Ok(Ok(run)) => run,
            Ok(Err(_)) => failed("internal error: the evaluator panicked"),
            Err(e) => failed(&format!("cannot start the evaluator: {e}")),
        }
    })
}

fn failed(message: &str) -> InterpRun {
    InterpRun {
        outcome: Outcome::Failed {
            message: message.to_string(),
        },
        trace: Trace::default(),
    }
}

fn run_here(source: &str, file: &str) -> InterpRun {
    let checked = match check(source, file) {
        Front::Checked(c) => c,
        other => {
            return match other.outcome() {
                Err(outcome) => InterpRun {
                    outcome,
                    trace: Trace::default(),
                },
                // `outcome` returns `Ok` only for `Front::Checked`, which
                // the match above took.
                Ok(_) => failed("internal error: front end state"),
            };
        }
    };
    let (result, report) = run_checked(&checked);
    let trace = Trace::from_events(&report.trace);
    let outcome = match result {
        Ok(n) => Outcome::Compiled {
            result: Value::Int(n),
            audit: summary(&report),
        },
        Err(e) if e.kind == RunErrorKind::Trap => Outcome::Trapped {
            message: e.to_string(),
            errors: abort_errors(&report),
        },
        Err(e) => Outcome::Failed {
            message: e.to_string(),
        },
    };
    InterpRun { outcome, trace }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_gives_its_result_and_trace() {
        let r = run(
            "(defstruct B (v: i64))\n(defun main () -> i64 (let ((b (B 4))) (. b v)))",
            "t",
        );
        assert!(
            matches!(
                r.outcome,
                Outcome::Compiled {
                    result: Value::Int(4),
                    ..
                }
            ),
            "{:?}",
            r.outcome
        );
        // The struct is scope-local (types §6.11): a stack allocation.
        assert_eq!(r.trace.render(), "S 1 o\nD 1\n");
    }
}
