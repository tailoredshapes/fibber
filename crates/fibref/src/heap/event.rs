//! The trace: one event per heap operation, in the order they happened.
//!
//! The compiler is checked against the interpreter by comparing results
//! and memory audits (`spec/method.md`, rule 6); this trace is the
//! interpreter's side of that comparison.

use super::value::{Kind, ObjId};

/// One heap operation. Every public operation on [`Heap`](super::Heap)
/// that touches an object appends exactly the events documented on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A new object with count 1 for the caller.
    Alloc { id: ObjId, kind: Kind },
    /// Count went up by one.
    Retain { id: ObjId, count_after: usize },
    /// Count went down by one; a `count_after` of zero is followed by
    /// `Free` of the same id.
    Release { id: ObjId, count_after: usize },
    /// The object was freed. Releases of the references it held follow.
    Free { id: ObjId },
    /// A field was read. No count change.
    Read { id: ObjId, field: usize },
    /// A field was written. Preceded by the retain of the new value and
    /// followed by the release of the old one, when those are references.
    Write { id: ObjId, field: usize },
    /// A weak reference to the object was made. No count change.
    Weak { id: ObjId },
    /// A weak reference was upgraded. If `live`, a `Retain` follows.
    Upgrade { id: ObjId, live: bool },
    /// The object became shared across threads (§7).
    Shared { id: ObjId },
}

impl Event {
    /// The object the event is about.
    pub fn id(self) -> ObjId {
        match self {
            Event::Alloc { id, .. }
            | Event::Retain { id, .. }
            | Event::Release { id, .. }
            | Event::Free { id }
            | Event::Read { id, .. }
            | Event::Write { id, .. }
            | Event::Weak { id }
            | Event::Upgrade { id, .. }
            | Event::Shared { id } => id,
        }
    }
}
