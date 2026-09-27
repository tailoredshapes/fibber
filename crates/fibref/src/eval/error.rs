//! Why a run stopped. A run never panics on a program: every failure is
//! one of these, with the position of the innermost expression being
//! evaluated when it happened.

use std::fmt;

use crate::heap::AuditError;
use crate::syntax::Pos;

/// What kind of failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunErrorKind {
    /// `(trap msg)` (or a primitive's trap: index out of range, division
    /// by zero, a match with no clause).
    Trap,
    /// The audited heap refused an operation (spec/method.md rule 2):
    /// a bug in the interpreter or in the plan it follows.
    Audit(AuditError),
    /// The ownership plan lacks something the evaluator needs; the
    /// evaluator reports it rather than inventing a rule.
    PlanGap,
    /// Evaluation used up its stack budget (non-tail recursion too deep).
    Depth,
    /// Something the interpreter cannot do (an unknown `extern`, a
    /// schedule the deterministic executor cannot run).
    Unsupported,
    /// An inconsistency in the evaluator itself.
    Internal,
}

/// A failed run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError {
    /// What kind.
    pub kind: RunErrorKind,
    /// The message.
    pub message: String,
    /// Where, once known.
    pub pos: Option<Pos>,
}

impl RunError {
    fn new(kind: RunErrorKind, message: impl Into<String>) -> Self {
        RunError {
            kind,
            message: message.into(),
            pos: None,
        }
    }

    /// A trap with `message`.
    pub fn trap(message: impl Into<String>) -> Self {
        RunError::new(RunErrorKind::Trap, message)
    }

    /// A gap in the ownership plan.
    pub fn gap(message: impl Into<String>) -> Self {
        RunError::new(RunErrorKind::PlanGap, message)
    }

    /// Something the interpreter does not support.
    pub fn unsupported(message: impl Into<String>) -> Self {
        RunError::new(RunErrorKind::Unsupported, message)
    }

    /// An internal inconsistency.
    pub fn internal(message: impl Into<String>) -> Self {
        RunError::new(RunErrorKind::Internal, message)
    }

    /// The stack budget, in bytes, was used up.
    pub fn depth(budget: usize) -> Self {
        RunError::new(
            RunErrorKind::Depth,
            format!(
                "evaluation used more than {} MiB of stack: non-tail recursion too deep",
                budget / (1024 * 1024)
            ),
        )
    }

    /// Sets the position if none is set yet (the innermost one wins).
    pub fn at(mut self, pos: &Pos) -> Self {
        if self.pos.is_none() {
            self.pos = Some(pos.clone());
        }
        self
    }
}

impl From<AuditError> for RunError {
    fn from(e: AuditError) -> Self {
        RunError::new(RunErrorKind::Audit(e), format!("{e:?}"))
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.kind {
            RunErrorKind::Trap => "trap",
            RunErrorKind::Audit(_) => "audit error",
            RunErrorKind::PlanGap => "gap in the ownership plan",
            RunErrorKind::Depth => "too deep",
            RunErrorKind::Unsupported => "unsupported",
            RunErrorKind::Internal => "internal error",
        };
        match &self.pos {
            Some(p) => write!(f, "{p}: {what}: {}", self.message),
            None => write!(f, "{what}: {}", self.message),
        }
    }
}

impl std::error::Error for RunError {}

/// The evaluator's result type.
pub type R<T> = Result<T, RunError>;
