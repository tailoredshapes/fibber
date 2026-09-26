//! One slot of the heap's object table.

use super::value::{Kind, Value};

/// A heap object: its kind, count, fields and audit flags.
///
/// A freed object keeps its slot (ids are never reused) but drops its
/// fields, so nothing can be read through it and it holds no counts.
#[derive(Debug)]
pub(super) struct Object {
    pub(super) kind: Kind,
    /// Reference count. Meaningless once `live` is false.
    pub(super) count: usize,
    pub(super) fields: Vec<Value>,
    /// Whether the object has crossed a thread boundary (§7), meaning
    /// its count operations are the atomic kind.
    pub(super) shared: bool,
    pub(super) live: bool,
}

impl Object {
    pub(super) fn new(kind: Kind, fields: Vec<Value>) -> Object {
        Object {
            kind,
            count: 1,
            fields,
            shared: false,
            live: true,
        }
    }

    /// Marks the object freed and drops its fields. The references among
    /// them are released by the cascade that planned this free.
    pub(super) fn free(&mut self) {
        self.live = false;
        self.count = 0;
        self.fields = Vec::new();
    }
}
