//! What each heap object is, beside the heap's own view of it.
//!
//! The heap models memory, not types (`heap::Kind`): a struct, a vector
//! node, a closure environment and a string are all `Immutable` objects
//! with fields. The interpreter keeps, per object id, what the object
//! is and its fields as [`Val`]s (with their widths, tags and `Option`
//! wrappers), while the heap holds the counted references
//! ([`Val::project`]). Every allocation goes through both; every access
//! is also made on the heap, which is what the audit sees.

use crate::heap::ObjId;
use crate::own::program::BodyKey;
use crate::types::ast::{BindingId, BuiltinId, ExprId, FunId};
use crate::types::ty::{ProtoId, TypeId};

use super::value::Val;

/// The values of a closure's or task's captures, by binding.
pub type Captures = Vec<(BindingId, Val)>;

/// An `async` task's code: its literal, the body whose tables created
/// it, its captures.
#[derive(Clone, Debug)]
pub struct AsyncBody {
    /// The `async` literal.
    pub lit: ExprId,
    /// The body whose plan has the literal's records.
    pub body: BodyKey,
    /// The captures.
    pub caps: Captures,
}

/// A closure's code and environment (types §8.4).
#[derive(Clone, Debug)]
pub enum Clo {
    /// A `fn` literal, with the body whose tables created it and every
    /// capture's value (heap fields hold only the counted ones, §6.5).
    Lambda {
        lit: ExprId,
        body: BodyKey,
        caps: Captures,
    },
    /// A named function used as a value: runs its all-owned body (§8.4).
    Fun(FunId),
    /// A builtin used as a value.
    Builtin(BuiltinId),
    /// A protocol method used as a value whose instance the checker
    /// left to the receiver (a bound or `dyn`): dispatched on the
    /// receiver's type at each call (§4.5).
    Method(ProtoId, usize),
    /// Method `usize` (of the protocol) of the instance resolved at the
    /// use (§4.2): runs that implementation's all-owned body (§8.4).
    Impl(usize, usize),
    /// A constructor used as a value.
    Ctor(TypeId, Option<usize>),
}

/// Where a task is in its life (§8.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    /// Created, not yet run.
    Pending,
    /// Being driven now (its driver claimed it).
    Running,
    /// Its result is stored.
    Done,
}

/// A task (`spawn` or `async`).
#[derive(Clone, Debug)]
pub struct TaskData {
    /// `Some` for an `async` task, `None` for a spawned thread's task.
    pub body: Option<AsyncBody>,
    /// Its state.
    pub state: TaskState,
    /// The atom that holds its result once done.
    pub result: ObjId,
}

/// One object.
#[derive(Clone, Debug)]
pub enum Obj {
    /// A string.
    Str(String),
    /// A struct.
    Struct { ty: TypeId, fields: Vec<Val> },
    /// A variant of an enum with fields.
    Variant {
        ty: TypeId,
        tag: u32,
        fields: Vec<Val>,
    },
    /// An `(Array T)`.
    Array(Vec<Val>),
    /// A cell (a private `&` cell among them).
    Cell(Val),
    /// An atom.
    Atom(Val),
    /// A weak box (§8.7): a `(Weak T)` value is the box.
    Weak(ObjId),
    /// A closure.
    Closure(Clo),
    /// A task.
    Task(Box<TaskData>),
}

/// The table of objects, indexed like the heap.
#[derive(Debug, Default)]
pub struct Objects {
    slots: Vec<Option<Obj>>,
}

impl Objects {
    /// Records what the new object `id` is.
    pub fn insert(&mut self, id: ObjId, obj: Obj) {
        let i = id.index();
        if self.slots.len() <= i {
            self.slots.resize_with(i + 1, || None);
        }
        self.slots[i] = Some(obj);
    }

    /// The object `id`.
    pub fn get(&self, id: ObjId) -> Result<&Obj, super::RunError> {
        self.slots
            .get(id.index())
            .and_then(Option::as_ref)
            .ok_or_else(|| super::RunError::internal(format!("no object {id}")))
    }

    /// The object `id`, to change.
    pub fn get_mut(&mut self, id: ObjId) -> Result<&mut Obj, super::RunError> {
        self.slots
            .get_mut(id.index())
            .and_then(Option::as_mut)
            .ok_or_else(|| super::RunError::internal(format!("no object {id}")))
    }
}
