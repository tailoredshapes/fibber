//! `fibc`: the fibber compiler (spec/compiler.md; ROADMAP M4).
//!
//! The front end is `fibref`'s ([`front`]): the checked program and
//! its ownership plan. [`compile`] lowers the plan to one lIR module
//! (types §8) that `lair` checks and compiles. [`harness`] runs every
//! case both ways and compares results and free traces ([`trace`]),
//! spec/method.md rule 6.

pub mod compile;
pub mod defs;
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
