//! What the harness runs a case through, and what comes back.
//!
//! The reference interpreter implements [`Evaluator`]
//! (`crate::eval::Interpreter`); later the compiler does too, so that
//! both are checked against the same cases (`spec/method.md`, rule 6).
//! [`PendingEvaluator`] answers [`Outcome::Unsupported`] for everything,
//! which the harness reports as Pending, never as a pass; the harness's
//! own tests use it.

use std::fmt;

/// A value `main` returned. Only integers exist today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// A signed 64-bit integer.
    Int(i64),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
        }
    }
}

/// What the instrumented heap reported at the end of a run
/// (`spec/method.md`, rule 2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditSummary {
    /// Nothing was live at exit and no check fired.
    pub clean: bool,
    /// Allocations still live at exit that are cycles through cells
    /// (`spec/ownership.md` §6); reported separately from every other failure.
    pub leak_cycles: usize,
    /// Allocations still live at exit that are not on a documented leak path.
    pub leaks: usize,
    /// Every check that fired: use-after-free, double free, negative count.
    pub errors: Vec<String>,
}

impl AuditSummary {
    /// A summary with nothing to report.
    pub fn clean() -> Self {
        AuditSummary {
            clean: true,
            ..AuditSummary::default()
        }
    }
}

impl fmt::Display for AuditSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "clean={} leak-cycles={} leaks={} errors={}",
            self.clean,
            self.leak_cycles,
            self.leaks,
            self.errors.len()
        )?;
        for error in &self.errors {
            write!(f, " [{error}]")?;
        }
        Ok(())
    }
}

/// The result of running one case through an [`Evaluator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The program type-checked and ran to completion.
    Compiled { result: Value, audit: AuditSummary },
    /// The program was refused with a compile error.
    Rejected { message: String },
    /// The program compiled and its run trapped (types §2.12): a `trap`
    /// call or a primitive's trap. A trap aborts the program, so what
    /// is live then is not a leak and the scopes open then are not
    /// errors; `errors` are the audit failures that still count at an
    /// abort (a live object holding a reference to a freed one).
    Trapped {
        message: String,
        errors: Vec<String>,
    },
    /// The program compiled but its run failed otherwise: a memory-audit
    /// error that stopped the run, or an interpreter limit. Never a
    /// pass, whatever the header says.
    Failed { message: String },
    /// The evaluator cannot decide this program yet. Reported as Pending.
    Unsupported { reason: String },
}

/// Something that can run a fibber program: the reference interpreter,
/// and later the compiler.
pub trait Evaluator {
    /// Runs the whole source of one case file, header comments included.
    fn run(&self, source: &str) -> Outcome;
}

/// An evaluator that supports nothing, so that the harness runs before
/// the interpreter exists. Every case it sees is Pending.
#[derive(Debug, Default, Clone, Copy)]
pub struct PendingEvaluator;

impl Evaluator for PendingEvaluator {
    fn run(&self, _source: &str) -> Outcome {
        Outcome::Unsupported {
            reason: "no interpreter yet".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_evaluator_supports_nothing() {
        assert_eq!(
            PendingEvaluator.run("(defun main () 1)"),
            Outcome::Unsupported {
                reason: "no interpreter yet".to_string()
            }
        );
    }

    #[test]
    fn clean_summary_reports_nothing() {
        let summary = AuditSummary::clean();
        assert!(summary.clean);
        assert_eq!((summary.leak_cycles, summary.leaks), (0, 0));
        assert!(summary.errors.is_empty());
        assert_eq!(
            summary.to_string(),
            "clean=true leak-cycles=0 leaks=0 errors=0"
        );
    }

    #[test]
    fn summary_display_lists_errors() {
        let summary = AuditSummary {
            clean: false,
            leak_cycles: 0,
            leaks: 1,
            errors: vec!["double free of #3".to_string()],
        };
        assert_eq!(
            summary.to_string(),
            "clean=false leak-cycles=0 leaks=1 errors=1 [double free of #3]"
        );
    }
}
