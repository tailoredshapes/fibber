//! The scalar and reference values the heap stores, and the kinds of
//! object it allocates (`spec/ownership.md` §1, §6, §7).

use std::fmt;

/// The identity of a heap object: an index into the heap's object table.
///
/// An id is never reused within one [`Heap`](super::Heap): a freed
/// object's slot stays freed, so a stale id (for example inside a
/// `Value::Weak`) can never name a newer object by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjId(usize);

impl ObjId {
    /// Builds an id from its table index. Only the heap creates ids.
    pub(super) fn from_index(index: usize) -> ObjId {
        ObjId(index)
    }

    /// The table index behind this id.
    pub fn index(self) -> usize {
        self.0
    }
}

impl fmt::Display for ObjId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// The identity of a stack scope (`spec/types.md` §6.11): an index into
/// the heap's scope table. Never reused, so an id kept after its scope
/// ended can only name that ended scope.
///
/// Ids are handed out in opening order, so a larger id is a scope
/// opened later. Scopes need not nest (`scope`): any open one may end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeId(usize);

impl ScopeId {
    /// Builds an id from its table index. Only the heap creates ids.
    pub(super) fn from_index(index: usize) -> ScopeId {
        ScopeId(index)
    }

    /// The table index behind this id.
    pub fn index(self) -> usize {
        self.0
    }
}

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "scope {}", self.0)
    }
}

/// A value: a scalar (copied, no identity; §1) or a reference to an
/// object.
///
/// `Ref` is a counted reference: storing one in an object is +1 on the
/// target, and the object releases it when freed (§2). `Weak` is not
/// counted (§6): it names an object that may already be gone, and only
/// [`Heap::upgrade`](super::Heap::upgrade) can turn it back into a `Ref`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Nil,
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    Ref(ObjId),
    Weak(ObjId),
}

impl Value {
    /// The object this value counts, if it is a counted reference.
    pub fn as_ref(self) -> Option<ObjId> {
        match self {
            Value::Ref(id) => Some(id),
            _ => None,
        }
    }

    /// The object this value names without counting it, if it is weak.
    pub fn as_weak(self) -> Option<ObjId> {
        match self {
            Value::Weak(id) => Some(id),
            _ => None,
        }
    }
}

/// What an object is, as far as memory is concerned.
///
/// The heap models memory, not types: a struct, a vector, a closure
/// environment and a string are all `Immutable` objects with N fields.
/// A string is an `Immutable` object whose fields are `Value::Int`
/// code points, so it holds no `Ref` and takes part in no cycle. The
/// interpreter keeps whatever type tag it needs beside the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Fixed fields, never written after allocation (§1). Can only refer
    /// to objects that already existed, so cannot be on a cycle.
    Immutable,
    /// Exactly one writable slot (§6). May not cross a thread (§7).
    Cell,
    /// Exactly one writable slot that may be shared across threads (§7).
    Atom,
}

impl Kind {
    /// Whether objects of this kind may be written after allocation.
    pub fn is_mutable(self) -> bool {
        matches!(self, Kind::Cell | Kind::Atom)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Kind::Immutable => "immutable",
            Kind::Cell => "cell",
            Kind::Atom => "atom",
        };
        f.write_str(name)
    }
}
