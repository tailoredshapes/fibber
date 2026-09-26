//! Tests for the instrumented heap, one file per concern.

mod dangling;
mod errors;
mod free;
mod leaks;
mod masked;
mod shared;
mod trace;
mod weak;

use super::{Heap, Kind, ObjId, Value};

/// An immutable object holding `fields`.
pub(super) fn imm(heap: &mut Heap, fields: Vec<Value>) -> ObjId {
    heap.alloc(Kind::Immutable, fields)
        .expect("alloc immutable")
}

/// A cell holding `value`.
pub(super) fn cell(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Cell, vec![value]).expect("alloc cell")
}

/// An atom holding `value`.
pub(super) fn atom(heap: &mut Heap, value: Value) -> ObjId {
    heap.alloc(Kind::Atom, vec![value]).expect("alloc atom")
}

impl Heap {
    /// Test back door: overwrites a field with no audit, no count change
    /// and no event. The public API cannot build an immutable cycle
    /// (§1), so the test that proves the detector fires needs this.
    pub(super) fn forge_field(&mut self, id: ObjId, index: usize, value: Value) {
        self.objects[id.index()].fields[index] = value;
    }
}
