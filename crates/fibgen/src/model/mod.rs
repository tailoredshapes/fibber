//! An independent model of what a generated program must return.
//!
//! It is a plain value interpreter over [`crate::ast`]: no counts, no
//! heap audit, no ownership plan. It follows the value semantics the
//! spec fixes for the constructs the generator emits: strict left-to-right
//! evaluation (syntax §2), copy-in/copy-out `&` with write-backs in
//! parameter order (§3.13), cells shared by reference, atoms replaced by
//! `swap!`/`reset!`, lazily run `async` tasks, integer arithmetic that
//! traps on overflow at every width and IEEE floats (types §2.12),
//! protocol dispatch with defaults (types §4), vector patterns and
//! guards (syntax §3.6), and macro calls expanded by
//! [`crate::macros::expand`]. The interpreter's result must equal the
//! model's (rule 4 of the task: differential sanity).

mod builtins;
mod eval;
mod nums;
mod patterns;
mod protos;
mod value;

pub use value::V;

use crate::ast::Program;
use eval::Machine;

/// Why the model could not produce a result. A generated program never
/// reaches either on purpose; one that does is a generator bug and is
/// counted, not reported as a finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelError {
    /// The program would trap (index out of range and the like).
    Trap(String),
    /// The model met something it does not implement.
    Unsupported(String),
    /// The model used up its step budget.
    Budget,
}

/// The stack the model runs on: deep programs recurse deeply.
const MODEL_STACK: usize = 256 * 1024 * 1024;

/// The value `main` must return, computed on a thread with
/// [`MODEL_STACK`] of stack.
pub fn expected(p: &Program) -> Result<i64, ModelError> {
    expected_traced(p).0
}

/// What the model's run of a program did beyond its result: labels for
/// the coverage table of things only a run shows (a guard that was
/// false, a default method that ran).
pub type Trace = Vec<&'static str>;

/// [`expected`], and the labels of what the run did.
pub fn expected_traced(p: &Program) -> (Result<i64, ModelError>, Trace) {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibgen-model".into())
            .stack_size(MODEL_STACK)
            .spawn_scoped(scope, || expected_here(p));
        match worker.map(|h| h.join()) {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => (
                Err(ModelError::Unsupported("the model panicked".into())),
                Vec::new(),
            ),
            Err(e) => (
                Err(ModelError::Unsupported(format!(
                    "cannot start the model: {e}"
                ))),
                Vec::new(),
            ),
        }
    })
}

fn expected_here(p: &Program) -> (Result<i64, ModelError>, Trace) {
    let mut m = Machine::new(p);
    let r = run_main(&mut m, p);
    (r, m.trace.iter().copied().collect())
}

fn run_main(m: &mut Machine, p: &Program) -> Result<i64, ModelError> {
    for d in &p.defs {
        let v = m.eval(&d.init, &eval::Env::default())?;
        m.globals.insert(d.name.clone(), v);
    }
    match m.eval(&p.main, &eval::Env::default())? {
        V::Int(n) => Ok(n),
        other => Err(ModelError::Unsupported(format!("main returned {other:?}"))),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_new;
