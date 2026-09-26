//! The instrumented heap.
//!
//! Every allocation, retain, release, free and access of a fibber object
//! goes through this module so that a run can be audited
//! (`spec/method.md`, rule 2). It implements the plain counting
//! semantics of `spec/ownership.md` §2 literally and optimises nothing:
//!
//! - binding or storing a reference adds one ([`Heap::alloc`],
//!   [`Heap::write`], [`Heap::retain`]);
//! - a reference going away subtracts one ([`Heap::release`]);
//! - an object is freed when its count reaches zero, releasing
//!   everything it refers to.
//!
//! Every misuse is an [`AuditError`]: use-after-free, double free, a
//! write to an immutable object, a bad field, a cell crossing a thread.
//! Every operation appends an [`Event`] to the trace, which is what the
//! compiler is later compared against (`spec/method.md`, rule 6). At the
//! end of a run [`Heap::finish`] reports every still-live object as a
//! leak, classified so that the one permitted leak, a cycle through
//! cells (§6), is told apart from every other failure.
//!
//! Files: `value` (ids, values, kinds), `event` (the trace), `error`
//! (audit errors), this file (alloc, retain, release), `cascade` (the
//! planned release cascade), `access` (read, write, weak, upgrade,
//! inspection), `shared` (thread crossing), `audit` (leak
//! classification), `graph` (the live-object graph the audit reads) and
//! `scc` (the graph search the audit uses).

mod access;
mod audit;
mod cascade;
mod error;
mod event;
mod graph;
mod object;
mod scc;
mod shared;
#[cfg(test)]
mod tests;
mod value;

pub use audit::{AuditReport, DanglingRef, Leak, LeakClass};
pub use error::{AuditError, Op};
pub use event::Event;
pub use value::{Kind, ObjId, Value};

use object::Object;

/// The audited heap. Owns every object and the trace of what was done
/// to them.
#[derive(Debug, Default)]
pub struct Heap {
    objects: Vec<Object>,
    trace: Vec<Event>,
}

impl Heap {
    /// An empty heap.
    pub fn new() -> Heap {
        Heap::default()
    }

    /// The events so far, in order.
    pub fn trace(&self) -> &[Event] {
        &self.trace
    }

    /// Allocates an object with count 1 for the caller.
    ///
    /// Every `Ref` in `fields` is retained: storing is +1 (§2, §3.2). A
    /// `Cell` or `Atom` has exactly one field. Events: `Alloc`, then one
    /// `Retain` per `Ref` field in field order. Nothing is allocated or
    /// retained if any field fails validation.
    pub fn alloc(&mut self, kind: Kind, fields: Vec<Value>) -> Result<ObjId, AuditError> {
        if kind.is_mutable() && fields.len() != 1 {
            return Err(AuditError::WrongSlotCount {
                kind,
                given: fields.len(),
            });
        }
        for value in &fields {
            self.check_storable(*value)?;
        }
        let id = ObjId::from_index(self.objects.len());
        self.trace.push(Event::Alloc { id, kind });
        let retained: Vec<ObjId> = fields.iter().filter_map(|v| v.as_ref()).collect();
        self.objects.push(Object::new(kind, fields));
        for target in retained {
            self.bump(target);
        }
        Ok(id)
    }

    /// Whether `value` may be stored into an object: a `Ref` must name
    /// a live object, a `Weak` a known one, and scalars always may.
    fn check_storable(&self, value: Value) -> Result<(), AuditError> {
        match value {
            Value::Ref(id) => self.live_object(id, Op::Store).map(|_| ()),
            Value::Weak(id) => self.object(id).map(|_| ()),
            _ => Ok(()),
        }
    }

    /// Count += 1. Returns the count after. Event: `Retain`.
    pub fn retain(&mut self, id: ObjId) -> Result<usize, AuditError> {
        self.live_object(id, Op::Retain)?;
        Ok(self.bump(id))
    }

    /// Count += 1 on an object already known to be live.
    fn bump(&mut self, id: ObjId) -> usize {
        let object = &mut self.objects[id.index()];
        object.count += 1;
        let count_after = object.count;
        self.trace.push(Event::Retain { id, count_after });
        count_after
    }

    /// Count -= 1. Returns the count after. At zero the object is freed
    /// and every `Ref` it held is released in turn, which may free more
    /// objects; this is done with an explicit worklist, not recursion,
    /// so a list of any length frees without exhausting the stack.
    ///
    /// The whole cascade is planned before anything changes (`cascade`).
    /// If it would release an object that is already freed, because a
    /// `Ref` dangles after an earlier over-release or because the
    /// cascade reaches an object it has itself just freed, the result
    /// is `ReleaseOfFreed` naming that object and nothing is released.
    ///
    /// Events: `Release`, then for a count of zero `Free` followed by the
    /// events of releasing each held `Ref` in field order, depth first
    /// (the order a recursive implementation would produce).
    pub fn release(&mut self, id: ObjId) -> Result<usize, AuditError> {
        let order = self.plan_release(id, None)?;
        self.apply_release(&order);
        Ok(self.objects[id.index()].count)
    }

    /// The object behind `id`, live or freed.
    fn object(&self, id: ObjId) -> Result<&Object, AuditError> {
        self.objects
            .get(id.index())
            .ok_or(AuditError::UnknownId { id })
    }

    /// The object behind `id`, which must be live for `op`.
    fn live_object(&self, id: ObjId, op: Op) -> Result<&Object, AuditError> {
        let object = self.object(id)?;
        if object.live {
            Ok(object)
        } else {
            Err(AuditError::UseAfterFree { id, op })
        }
    }

    /// Every live object's id, in allocation order.
    fn live_ids(&self) -> Vec<ObjId> {
        self.objects
            .iter()
            .enumerate()
            .filter(|(_, object)| object.live)
            .map(|(index, _)| ObjId::from_index(index))
            .collect()
    }
}
