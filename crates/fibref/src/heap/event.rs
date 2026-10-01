//! The trace: one event per heap operation, in the order they happened.
//!
//! The compiler is checked against the interpreter by comparing results
//! and memory audits (`spec/method.md`, rule 6); this trace is the
//! interpreter's side of that comparison.

use super::value::{Kind, ObjId, ScopeId};

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
    /// A new `IMMORTAL` object (a literal, a `def` value; §8.2): count 0,
    /// never freed. Its fields name only immortal objects, so no
    /// `Retain` follows.
    AllocImmortal { id: ObjId, kind: Kind },
    /// A new `STACK` object of `scope` (§6.11): count 0. A `Retain`
    /// follows for each field that is a `Ref` to a counted object.
    AllocStack {
        id: ObjId,
        kind: Kind,
        scope: ScopeId,
    },
    /// An in-place write to field `field` of the unique `Immutable`
    /// object `id` held by the cell `place` (§6.6). Preceded and
    /// followed exactly as `Write` is.
    WriteUnique {
        place: ObjId,
        id: ObjId,
        field: usize,
    },
    /// A stack scope was opened (§6.11).
    ScopeOpen { scope: ScopeId },
    /// A stack scope ended. One `Drop` per object allocated in it
    /// follows, most recent first.
    ScopeEnd { scope: ScopeId },
    /// A `STACK` object's drop ran at its scope's end: the releases of
    /// the counted references it held follow, in field order.
    Drop { id: ObjId },
    /// A counted object became `IMMORTAL` (`fib.immortalise`, §8.2: a
    /// `def` value and everything reachable from it). Its count is 0
    /// from now on and it is never freed.
    Immortalised { id: ObjId },
}

impl Event {
    /// The object the event is about, or `None` for the scope events,
    /// which are about no object.
    pub fn id(self) -> Option<ObjId> {
        let id = match self {
            Event::Alloc { id, .. }
            | Event::Retain { id, .. }
            | Event::Release { id, .. }
            | Event::Free { id }
            | Event::Read { id, .. }
            | Event::Write { id, .. }
            | Event::Weak { id }
            | Event::Upgrade { id, .. }
            | Event::Shared { id }
            | Event::AllocImmortal { id, .. }
            | Event::AllocStack { id, .. }
            | Event::WriteUnique { id, .. }
            | Event::Drop { id }
            | Event::Immortalised { id } => id,
            Event::ScopeOpen { .. } | Event::ScopeEnd { .. } => return None,
        };
        Some(id)
    }
}

/// The events of `events` that the free trace is made of
/// (`spec/compiler.md` §4): those after the last `Immortalised`. What
/// happens before it is the evaluation of the `def`s (syntax §3.19),
/// whose objects the compiled program holds as static data, so neither
/// side traces it.
pub fn traced(events: &[Event]) -> &[Event] {
    let start = events
        .iter()
        .rposition(|e| matches!(e, Event::Immortalised { .. }))
        .map_or(0, |i| i + 1);
    &events[start..]
}

/// The number of heap objects the run allocated: the `A` lines of its
/// free trace (`spec/compiler.md` §4), which the case header key
/// `allocs` bounds. A stack object (`S` line) is not one.
pub fn trace_allocs(events: &[Event]) -> u64 {
    traced(events)
        .iter()
        .filter(|e| matches!(e, Event::Alloc { .. }))
        .count() as u64
}
