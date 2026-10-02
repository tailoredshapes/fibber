//! `fibc`: the fibber compiler (spec/compiler.md; ROADMAP M4).
//!
//! The front end is `fibref`'s ([`front`]): the checked program and
//! its ownership plan. [`compile`] lowers the plan to one lIR module
//! (types §8) that `lair` checks and compiles. [`harness`] runs every
//! case both ways and compares results and free traces ([`trace`]),
//! spec/method.md rule 6.
//!
//! The emitted text is a function of the program alone (stage 2 and
//! stage 3 must emit identical lIR, spec/bootstrap.md). So no hash order
//! reaches it: every `HashMap` and `HashSet` of this crate is only
//! looked up or inserted into (the indexes of `statics`, `objects` and
//! `mono`, the tables of `lower`, `program`, `ir` and `macros`), or is
//! folded into an unordered set (`lower::ops::value_sites`), or is
//! sorted before it is read (`trace::tally`). The two walks that could
//! reach the text, in `resume`, are over a sorted list and over an
//! insertion-ordered list; `tests/emit.rs` compiles the async cases ten
//! times in one process (each std hasher is seeded differently) and
//! asserts one text.

pub mod compile;
pub mod defs;
pub mod emit_dump;
pub mod front;
pub mod harness;
pub mod inits;
pub mod ir;
pub mod layout;
pub mod lower;
pub mod macros;
pub mod mono;
pub mod names;
pub mod objects;
pub mod program;
pub mod resume;
pub mod statics;
pub mod trace;
pub mod value;
