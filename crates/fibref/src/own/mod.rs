//! The ownership checker (spec/types.md §6): the pass that runs after
//! inference and decides, from the syntax tree, the types, the capture
//! sets and the callees' summaries alone, every count operation of the
//! program; and its printer (§9, `fibref explain`).
//!
//! Pipeline ([`check_source`], [`check_forms`]): read, expand, lower
//! (types §3.5 steps 1–3), then the syntactic `&` checks of §6.5 and
//! §6.9 ([`syntactic`], still step 3, before any type error), then
//! inference (steps 4–7), then the ownership pass ([`analyse`], step
//! 5f for every unit in checking order).
//!
//! The pass, per unit ([`unit`]): the tail sites of §6.10; the least
//! fixpoint of §6.4 over count kinds, escape summaries and heap
//! closures ([`facts`], [`classify`]); the admission of tail calls; the
//! scope-local bindings of §6.11 ([`stack`]); and the operations,
//! emitted by the same walk ([`walk`]) that computes the modes of §6.1
//! and §6.2. The result is an [`OwnedProgram`] (see [`program`] for how
//! to read it). The walk is one function of the unit's facts: every run
//! computes modes, events and operations, and the driver reads what it
//! needs from each.

pub mod error;
pub mod explain;
pub mod program;
pub mod syntactic;

mod classify;
mod facts;
mod objects;
mod stack;
mod taken;
mod top;
mod unit;
mod walk;

#[cfg(test)]
mod tests;

pub use error::{CheckError, OwnError, OwnErrorKind};
pub use objects::is_object;
pub use program::OwnedProgram;
pub use taken::{methods_taken, value_taken};
pub use top::analyse;

use crate::expand::{expand_program, ExpandCtx, NoRunner};
use crate::syntax::{read_all, Form};
use crate::types::{infer_lowered, lower_program, prelude_forms, TypedProgram, CHECK_STACK};

/// A program that passed the whole front end.
#[derive(Clone, Debug)]
pub struct Checked {
    /// Its types (§3).
    pub typed: TypedProgram,
    /// Its ownership decisions (§6).
    pub owned: OwnedProgram,
}

/// Reads, expands, types and checks the ownership of `source` as the
/// user module, with the prelude.
pub fn check_source(source: &str, file: &str) -> Result<Checked, CheckError> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).map_err(CheckError::Prelude)?;
    let forms = read_all(source, file).map_err(|e| CheckError::Read(e.to_string()))?;
    let forms = expand_program(forms, &mut ctx, &mut NoRunner)
        .map_err(|e| CheckError::Expand(e.to_string()))?;
    check_forms(&forms, &prelude)
}

/// Checks a module's forms after expansion against the prelude's:
/// lowering, the `&` checks, inference, the ownership pass. Runs on a
/// thread with a [`CHECK_STACK`]-deep stack, as the checker does.
pub fn check_forms(forms: &[Form], prelude: &[Form]) -> Result<Checked, CheckError> {
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibref-own".into())
            .stack_size(CHECK_STACK)
            .spawn_scoped(scope, || check_here(forms, prelude));
        match worker.map(|h| h.join()) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CheckError::Internal("the checker panicked".into())),
            Err(e) => Err(CheckError::Internal(format!(
                "cannot start the checker: {e}"
            ))),
        }
    })
}

fn check_here(forms: &[Form], prelude: &[Form]) -> Result<Checked, CheckError> {
    let lowered = lower_program(forms, prelude).map_err(CheckError::Type)?;
    let amp = syntactic::check(&lowered.globals);
    if !amp.is_empty() {
        return Err(CheckError::Own(amp));
    }
    let typed = infer_lowered(lowered, true).map_err(CheckError::Type)?;
    let owned = analyse(&typed).map_err(CheckError::Own)?;
    Ok(Checked { typed, owned })
}
