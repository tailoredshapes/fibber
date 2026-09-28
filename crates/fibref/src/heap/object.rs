//! One slot of the heap's object table.

use super::value::{Kind, ScopeId, Value};

/// A heap object: its kind, count, fields and audit flags.
///
/// A freed object keeps its slot (ids are never reused) but drops its
/// fields, so nothing can be read through it and it holds no counts.
///
/// The header flags of `spec/types.md` §8.2 are `shared` (`SHARED`),
/// `immortal` (`IMMORTAL`), `scope` (`STACK`, with the scope that ends
/// the object) and `has_weak` (`HAS-WEAK`). A `Value::Weak` names its
/// target directly and needs no box, so `HAS-WEAK` matters only to
/// `fib.unique?`, which refuses an object that ever had a weak
/// reference (§6.6, ownership.md §5). An `IMMORTAL` or `STACK` object
/// has count 0 and nothing ever changes it (§8.2).
#[derive(Debug)]
pub(super) struct Object {
    pub(super) kind: Kind,
    /// Reference count. Meaningless once `live` is false, and always 0
    /// on an immortal or stack object.
    pub(super) count: usize,
    pub(super) fields: Vec<Value>,
    /// Whether the object has crossed a thread boundary (§7), meaning
    /// its count operations are the atomic kind.
    pub(super) shared: bool,
    /// `IMMORTAL`: static data (a literal, a `def` value). Never freed,
    /// never counted, not audited at exit.
    pub(super) immortal: bool,
    /// `STACK`: the scope whose end ends this object (§6.11). A stack
    /// object's `live` turns false when its scope ends, not when it is
    /// released, which it never is.
    pub(super) scope: Option<ScopeId>,
    /// `HAS-WEAK`: a weak reference to it was taken; never cleared.
    /// Left unset on an immortal object, whose header `weak` does not
    /// write (§8.7).
    pub(super) has_weak: bool,
    pub(super) live: bool,
}

impl Object {
    /// A counted heap object with count 1 for the caller.
    pub(super) fn new(kind: Kind, fields: Vec<Value>) -> Object {
        Object {
            kind,
            count: 1,
            fields,
            shared: false,
            immortal: false,
            scope: None,
            has_weak: false,
            live: true,
        }
    }

    /// An `IMMORTAL` object: count 0, never freed.
    pub(super) fn immortal(kind: Kind, fields: Vec<Value>) -> Object {
        Object {
            count: 0,
            immortal: true,
            ..Object::new(kind, fields)
        }
    }

    /// A `STACK` object of `scope`: count 0, ended with its scope.
    pub(super) fn on_stack(kind: Kind, fields: Vec<Value>, scope: ScopeId) -> Object {
        Object {
            count: 0,
            scope: Some(scope),
            ..Object::new(kind, fields)
        }
    }

    /// Whether count operations on this object count: it is neither
    /// `IMMORTAL` nor `STACK`. True of a freed heap object too, so that
    /// releasing one is still caught as a double free.
    pub(super) fn counted(&self) -> bool {
        !self.immortal && self.scope.is_none()
    }

    /// Marks the object freed (or, for a stack object, ended) and drops
    /// its fields. The references among them are released by the
    /// cascade that planned this free.
    pub(super) fn free(&mut self) {
        self.live = false;
        self.count = 0;
        self.fields = Vec::new();
    }
}
