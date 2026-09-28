//! An independent model of what a generated program must return.
//!
//! It is a plain value interpreter over [`crate::ast`]: no counts, no
//! heap audit, no ownership plan. It follows the value semantics the
//! spec fixes for the constructs the generator emits: strict left-to-right
//! evaluation (syntax §2), copy-in/copy-out `&` with write-backs in
//! parameter order (§3.13), cells shared by reference, atoms replaced by
//! `swap!`/`reset!`, lazily run `async` tasks, and wrapping integer
//! arithmetic (types §2.12). The interpreter's result must equal the
//! model's (rule 4 of the task: differential sanity).

mod builtins;
mod eval;
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
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibgen-model".into())
            .stack_size(MODEL_STACK)
            .spawn_scoped(scope, || expected_here(p));
        match worker.map(|h| h.join()) {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err(ModelError::Unsupported("the model panicked".into())),
            Err(e) => Err(ModelError::Unsupported(format!(
                "cannot start the model: {e}"
            ))),
        }
    })
}

fn expected_here(p: &Program) -> Result<i64, ModelError> {
    let mut m = Machine::new(p);
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
