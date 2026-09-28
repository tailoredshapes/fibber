//! The front end, reused from `fibref` (spec/compiler.md §1): read,
//! expand (user macros run by the interpreter's macro evaluator until
//! compiler.md §6 replaces it), type and check ownership. The result is
//! the plan the lowering consumes.

use fibref::cases::Outcome;
use fibref::eval::MacroEvaluator;
use fibref::expand::{expand_program, ExpandCtx};
use fibref::own::{check_forms, CheckError, Checked};
use fibref::syntax::read_all;
use fibref::types::prelude_forms;

/// What the front end said about a program.
pub enum Front {
    /// It passed: here is its plan.
    Checked(Box<Checked>),
    /// It was rejected with this message.
    Rejected(String),
    /// The front end itself failed (a prelude that does not expand, a
    /// panic): never a verdict on the program.
    Failed(String),
}

impl Front {
    /// The harness outcome a non-checked answer stands for.
    pub fn outcome(self) -> Result<Box<Checked>, Outcome> {
        match self {
            Front::Checked(c) => Ok(c),
            Front::Rejected(message) => Err(Outcome::Rejected { message }),
            Front::Failed(message) => Err(Outcome::Failed { message }),
        }
    }
}

/// Reads, expands, types and checks `source` (named `file` in
/// positions), exactly as `fibref run` does before it evaluates.
pub fn check(source: &str, file: &str) -> Front {
    let mut ctx = ExpandCtx::new();
    let prelude = match prelude_forms(&mut ctx) {
        Ok(p) => p,
        Err(m) => return Front::Failed(format!("the prelude does not expand: {m}")),
    };
    let forms = match read_all(source, file) {
        Ok(f) => f,
        Err(e) => return Front::Rejected(e.to_string()),
    };
    let mut runner = MacroEvaluator::new(&forms, prelude.clone());
    let forms = match expand_program(forms, &mut ctx, &mut runner) {
        Ok(f) => f,
        Err(e) => return Front::Rejected(e.to_string()),
    };
    match check_forms(&forms, &prelude) {
        Ok(c) => Front::Checked(Box::new(c)),
        Err(CheckError::Internal(m) | CheckError::Prelude(m)) => Front::Failed(m),
        Err(e) => Front::Rejected(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_checked() {
        assert!(matches!(
            check("(defun main () -> i64 1)", "t"),
            Front::Checked(_)
        ));
    }

    #[test]
    fn a_type_error_is_a_rejection() {
        match check("(defun main () -> i64 \"s\")", "t") {
            Front::Rejected(m) => assert!(!m.is_empty()),
            _ => panic!("expected a rejection"),
        }
    }
}
