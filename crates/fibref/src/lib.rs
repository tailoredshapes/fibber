//! `fibref`: the reference interpreter and memory audit for fibber.
//!
//! This crate is the executable form of `spec/`. It follows the rules in
//! `spec/ownership.md` literally and optimises nothing (`spec/method.md`,
//! rule 1). Every heap operation goes through the instrumented heap in
//! [`heap`] (rule 2), and the programs in `cases/` are run against the
//! verdicts fixed in their headers by [`cases`] (rule 3).

pub mod cases;
pub mod expand;
pub mod heap;
pub mod own;
pub mod syntax;
pub mod types;

pub use heap::{
    AuditError, AuditReport, Event, Heap, Kind, Leak, LeakClass, ObjId, Op, ScopeId, Uniqueness,
    Value,
};
