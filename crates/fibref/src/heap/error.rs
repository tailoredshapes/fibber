//! Audit errors: every misuse of the heap is returned as one of these,
//! never a panic and never silently ignored (`spec/method.md`, rule 2).

use std::fmt;

use super::value::{Kind, ObjId, ScopeId};

/// The operation that was attempted on an object, for error reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `retain`.
    Retain,
    /// `release`.
    Release,
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
            Op::Release => "release",
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
    /// A `write_unique` that `fib.unique?` refuses (§6.6, §8.2): writing
    /// in place would change an object another binding can see, or
    /// static or stack data. `id` is the object `why` is about.
    NotUnique { id: ObjId, why: Uniqueness },
    /// Any operation on a `STACK` object after its scope ended (§6.11),
    /// told apart from `UseAfterFree`: the compiled program would touch
    /// a frame slot that its site may already have reused.
    StackUseAfterScope { id: ObjId, op: Op },
    /// A `Ref` to the `STACK` object `id` stored into a heap object
    /// (§6.11): the heap object could outlive the scope.
    StackRefInHeap { id: ObjId },
    /// A `Ref` to the `STACK` object `id` stored into a `STACK` object
    /// of `scope`, which was opened before `id`'s own and so may
    /// outlive it (`store`).
    StackRefIntoOuterScope { id: ObjId, scope: ScopeId },
    /// A weak reference to the `STACK` object `id` (§6.7, §6.11: `weak`
    /// forces its operand onto the heap).
    WeakToStack { id: ObjId },
    /// `mark_shared` reached the `STACK` object `id`: a stack object is
    /// never passed to another thread (§6.11).
    SharedStack { id: ObjId },
    /// An `IMMORTAL` object would hold `id`, which is not immortal: a
    /// constant graph is closed (§6.7, §8.2).
    ImmortalHoldsMortal { id: ObjId },
    /// An `IMMORTAL` object of a mutable kind: constants build only
    /// immutable objects (syntax §3.19).
    MutableImmortal { kind: Kind },
    /// A scope id this heap never opened.
    UnknownScope { scope: ScopeId },
    /// A scope that has already ended, ended again or allocated into.
    ScopeEnded { scope: ScopeId },
}

/// Why `write_unique` refused: the first failing test of `fib.unique?`
/// (§8.2), flags before count, or a place the primitive cannot take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uniqueness {
    /// The place is not a live `Cell` (§6.6: the place of `set-field!`
    /// and `array-set!` is a variable's cell or an `&` private cell).
    PlaceNotCell,
    /// The place does not hold a `Ref` to an `Immutable` object.
    ContentNotImmutable,
    /// `SHARED`: another thread may see it.
    Shared,
    /// `IMMORTAL`: static data, no count.
    Immortal,
    /// `STACK`: no count to test.
    Stack,
    /// `HAS-WEAK`: a weak reference may observe it, so it is never
    /// changed in place (ownership.md §5; types §6.6).
    HasWeak,
    /// The count is not exactly 1.
    Count(usize),
    /// The value written is the object itself: the caller holds it, so
    /// its count cannot be 1 under counting (§2) and the write would
    /// close a cycle through no cell.
    StoresItself,
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
            _ => self.fmt_scoped(f),
        }
    }
}

impl AuditError {
    /// `Display` for the variants of immortal, stack and unique writes.
    fn fmt_scoped(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuditError::NotUnique { id, why } => {
                write!(f, "unique write refused on {id}: {why:?}")
            }
            AuditError::StackUseAfterScope { id, op } => {
                write!(f, "stack use after scope: {op} of stack object {id}")
            }
            AuditError::StackRefInHeap { id } => {
                write!(f, "stack object {id} stored into a heap object")
            }
            AuditError::StackRefIntoOuterScope { id, scope } => {
                write!(
                    f,
                    "stack object {id} stored into an object of earlier {scope}"
                )
            }
            AuditError::WeakToStack { id } => write!(f, "weak reference to stack object {id}"),
            AuditError::SharedStack { id } => write!(f, "stack object {id} would be shared"),
            AuditError::ImmortalHoldsMortal { id } => {
                write!(f, "immortal object would hold non-immortal {id}")
            }
            AuditError::MutableImmortal { kind } => write!(f, "an immortal {kind}"),
            AuditError::UnknownScope { scope } => write!(f, "unknown {scope}"),
            AuditError::ScopeEnded { scope } => write!(f, "{scope} has already ended"),
            _ => write!(f, "{self:?}"),
        }
    }
}

impl std::error::Error for AuditError {}
