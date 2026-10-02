//! `fibref`: the reference interpreter and memory audit for fibber.
//!
//! This crate is the executable form of `spec/`. It follows the rules in
//! `spec/ownership.md` literally and optimises nothing (`spec/method.md`,
//! rule 1). Every heap operation goes through the instrumented heap in
//! [`heap`] (rule 2), and the programs in `cases/` are run against the
//! verdicts fixed in their headers by [`cases`] (rule 3).

#![forbid(unsafe_code)]

pub mod cases;
pub mod cmdline;
pub mod dump;
pub mod eval;
pub mod expand;
pub mod expand_dump;
pub mod heap;
pub mod modules;
pub mod own;
pub mod roots;
pub mod syntax;
pub mod types;
pub mod types_dump;

pub use heap::{
    AuditError, AuditReport, Event, Heap, Kind, Leak, LeakClass, ObjId, Op, ScopeId, Uniqueness,
    Value,
};
