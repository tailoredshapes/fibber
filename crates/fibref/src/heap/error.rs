//! Audit errors: every misuse of the heap is returned as one of these,
//! never a panic and never silently ignored (`spec/method.md`, rule 2).

use std::fmt;

use super::value::{Kind, ObjId};

/// The operation that was attempted on an object, for error reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `retain`.
    Retain,
    /// `read`.
    Read,
    /// `write`.
    Write,
    /// `weak`.
    Weak,
    /// `mark_shared`, or reached while marking.
    Share,
    /// Stored as a field of a new allocation.
    Store,
    /// `count`, `kind` or `is_shared`.
    Inspect,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Op::Retain => "retain",
            Op::Read => "read",
            Op::Write => "write",
            Op::Weak => "weak",
            Op::Share => "share",
            Op::Store => "store",
            Op::Inspect => "inspect",
        };
        f.write_str(name)
    }
}

/// A memory-audit failure. The heap's state after an `Err` is exactly
/// what it was before the failing call: every operation validates its
/// arguments before it changes anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditError {
    /// The id was never allocated by this heap.
    UnknownId { id: ObjId },
    /// An operation other than release on an object already freed.
    UseAfterFree { id: ObjId, op: Op },
    /// A release of an object already freed: the count would go below
    /// zero. This is the double free of `spec/method.md` rule 2. It
    /// also names an object a release cascade would reach after freeing
    /// it, or through a `Ref` left dangling by an earlier over-release;
    /// the cascade is then refused whole and nothing is released.
    ReleaseOfFreed { id: ObjId },
    /// A write to an `Immutable` object (§1).
    WriteToImmutable { id: ObjId },
    /// A field index past the end of the object.
    BadField { id: ObjId, index: usize },
    /// A `Cell` that is not an `Atom` would become shared (§7).
    SharedCell { id: ObjId },
    /// A `Cell` or `Atom` allocated with other than exactly one slot.
    WrongSlotCount { kind: Kind, given: usize },
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditError::UnknownId { id } => write!(f, "unknown object {id}"),
            AuditError::UseAfterFree { id, op } => {
                write!(f, "use after free: {op} of freed object {id}")
            }
            AuditError::ReleaseOfFreed { id } => {
                write!(f, "double free: release of freed object {id}")
            }
            AuditError::WriteToImmutable { id } => {
                write!(f, "write to immutable object {id}")
            }
            AuditError::BadField { id, index } => {
                write!(f, "object {id} has no field {index}")
            }
            AuditError::SharedCell { id } => {
                write!(f, "cell {id} would cross a thread boundary")
            }
            AuditError::WrongSlotCount { kind, given } => {
                write!(f, "a {kind} has exactly one slot, not {given}")
            }
        }
    }
}

impl std::error::Error for AuditError {}
