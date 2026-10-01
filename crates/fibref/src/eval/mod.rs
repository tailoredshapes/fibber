//! The evaluator: the reference interpreter's run time (spec/types.md
//! §6.12). It runs a checked program over the audited heap and follows
//! the ownership checker's plan ([`crate::own::program`]) mechanically:
//! it performs exactly the count operations the plan lists, at the
//! points it lists them, plus those that are part of each primitive's
//! definition, and decides nothing about ownership itself. Where the
//! plan lacks something it needs, it stops with a
//! [`RunErrorKind::PlanGap`] rather than invent a rule.
//!
//! - [`value`], [`object`]: the value model. Scalars are unboxed;
//!   every object (string, struct, variant, array, closure, cell, atom,
//!   weak box, task, `Form`) is a heap object of the right `Kind`, with
//!   the interpreter's view of it in the object table. `Option` is
//!   unboxed with a real tag and allocates nothing (§8.1, §4.5).
//! - [`interp`]: a thread's state, frames, sites and the plan's
//!   operations; [`world`]: the state the threads share; [`alloc`]:
//!   heap, stack (§6.11) and immortal (§8.2) placement.
//! - [`expr`], [`pattern`], [`call`], [`closure`]: the core forms, calls
//!   and tail calls (a trampoline: a chain of tail calls runs in
//!   constant Rust stack, §6.10), closures and dispatch.
//! - [`native`], [`cells`], [`arrays`], [`strings`], [`arith`],
//!   [`unsafe_ops`], [`raw`]: the primitives.
//! - [`task`]: threads and tasks on a deterministic, fair executor;
//!   [`sched`]: its policy; [`threads`]: switching between threads.
//! - [`forms`], [`vecs`]: `Form` values and the prelude's `Vec`, built
//!   and read natively; [`macros`]: user macros at expansion time.
//! - [`pipeline`]: the whole run and [`Interpreter`], the case
//!   evaluator.

pub mod alloc;
pub mod arith;
pub mod arrays;
pub mod ast;
pub mod call;
pub mod cells;
pub mod closure;
pub mod error;
pub mod expr;
pub mod float_bits;
pub mod floattext;
pub mod forms;
pub mod fx;
pub mod interp;
pub mod io;
pub mod macros;
pub mod native;
pub mod object;
pub mod option;
pub mod pattern;
pub mod pipeline;
pub mod plan;
pub mod raw;
pub mod sched;
pub mod strings;
pub mod strtod;
pub mod task;
pub mod threads;
pub mod unsafe_ops;
pub mod value;
pub mod vec_view;
pub mod vecs;
pub mod world;

#[cfg(test)]
mod tests;

pub use error::{RunError, RunErrorKind, R};
pub use interp::{Interp, MACRO_STACK_BUDGET, STACK_BUDGET};
pub use macros::MacroEvaluator;
pub use pipeline::{
    run_checked, run_checked_with, run_source, run_source_in, run_source_with, summary,
    Interpreter, STACK_BYTES,
};
pub use value::Val;
