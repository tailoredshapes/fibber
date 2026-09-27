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
//! Beside counted heap objects there are two uncounted kinds, as in the
//! compiled program's header flags (`spec/types.md` §8.2): `IMMORTAL`
//! objects ([`Heap::alloc_immortal`]: literals and `def` values, never
//! freed and not audited at exit) and `STACK` objects
//! ([`Heap::alloc_in_scope`]: scope-local objects, §6.11, ended with
//! their scope). An `Immutable` object may be written in place only by
//! [`Heap::write_unique`], under the `fib.unique?` test (§6.6).
//!
//! Files: `value` (ids, values, kinds), `event` (the trace), `error`
//! (audit errors), `object` (one object and its flags), this file
//! (alloc, retain, release), `store` (what may be stored where),
//! `cascade` (the planned release cascade), `access` (read, write, weak,
//! upgrade, inspection), `unique` (write-unique), `scope` (stack
//! scopes), `shared` (thread crossing), `audit` (leak classification),
//! `graph` (the live-object graph the audit reads) and `scc` (the graph
//! search the audit uses).

mod access;
mod audit;
mod cascade;
mod error;
mod event;
mod graph;
mod object;
mod scc;
mod scope;
mod shared;
mod store;
#[cfg(test)]
mod tests;
mod unique;
mod value;

pub use audit::{AuditReport, DanglingRef, Leak, LeakClass};
pub use error::{AuditError, Op, Uniqueness};
pub use event::Event;
pub use value::{Kind, ObjId, ScopeId, Value};

use object::Object;
use scope::Scope;
use store::Holder;

/// The audited heap. Owns every object and the trace of what was done
/// to them.
#[derive(Debug, Default)]
pub struct Heap {
    objects: Vec<Object>,
    trace: Vec<Event>,
    /// Every scope ever opened, indexed by `ScopeId`.
    scopes: Vec<Scope>,
    /// The open scopes, outermost first.
    open: Vec<ScopeId>,
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
    /// Every `Ref` in `fields` is retained: storing is +1 (§2, §3.2),
    /// except a `Ref` to an immortal object, whose count operations are
    /// no-ops (§8.2). A `Cell` or `Atom` has exactly one field; no field
    /// may name a stack object (`store`). Events: `Alloc`, then one
    /// `Retain` per `Ref` field to a counted object, in field order.
    /// Nothing is allocated or retained if any field fails validation.
    pub fn alloc(&mut self, kind: Kind, fields: Vec<Value>) -> Result<ObjId, AuditError> {
        self.check_new(kind, &fields, Holder::Heap)?;
        Ok(self.install(Object::new(kind, fields), |id| Event::Alloc { id, kind }))
    }

    /// Allocates an `IMMORTAL` object (`spec/types.md` §8.2): a literal,
    /// a `def` value, or an object reachable from one, with count 0. It
    /// is never freed, never reported at exit, and `fib.unique?` is
    /// false on it. Its kind is `Immutable` (a constant builds only
    /// immutable objects, syntax §3.19) and its fields name only
    /// immortal objects (a constant graph is closed, §6.7), so building
    /// one retains nothing. Event: `AllocImmortal`.
    ///
    /// **Count operations on an immortal are no-ops and are not
    /// traced.** `retain` and `release` return `Ok(0)` and change
    /// nothing; storing a `Ref` to it anywhere is fine and retains
    /// nothing, and freeing a holder releases nothing for it. §8.2
    /// makes `fib.retain` and `fib.release` return on the `IMMORTAL`
    /// flag before touching the count, "nothing ever changes it", and
    /// "the audit does not track" immortal objects (§6.7, §8.2); a
    /// `Retain` event would claim a count change that never happens,
    /// and how many no-op calls the compiled code makes depends on
    /// which counts it elides (§6.12), so tracing them would make the
    /// comparison of `spec/method.md` rule 6 differ where the two agree
    /// on every count and every free. Accesses (`read`, `weak`,
    /// `upgrade`) are traced as on any object.
    pub fn alloc_immortal(&mut self, kind: Kind, fields: Vec<Value>) -> Result<ObjId, AuditError> {
        if kind != Kind::Immutable {
            return Err(AuditError::MutableImmortal { kind });
        }
        self.check_new(kind, &fields, Holder::Immortal)?;
        let object = Object::immortal(kind, fields);
        Ok(self.install(object, |id| Event::AllocImmortal { id, kind }))
    }

    /// Count += 1. Returns the count after. Event: `Retain`.
    ///
    /// On an immortal or a live stack object a no-op returning 0, with no
    /// event: types §8.2 makes retain/release no-ops on `IMMORTAL` and
    /// `STACK` objects, and §6.11 relies on it when a stack object is
    /// passed to an owned parameter whose callee releases it.
    pub fn retain(&mut self, id: ObjId) -> Result<usize, AuditError> {
        if !self.check_count_op(id, Op::Retain)? {
            return Ok(0);
        }
        self.live_object(id, Op::Retain)?;
        Ok(self.bump(id))
    }

    /// Whether a count operation `op` on `id` counts: `false` for an
    /// immortal object or a live stack object (a no-op, types §8.2),
    /// `StackUseAfterScope` once a stack object's scope has ended.
    fn check_count_op(&self, id: ObjId, op: Op) -> Result<bool, AuditError> {
        let object = self.object(id)?;
        match object.scope {
            Some(_) if !object.live => Err(AuditError::StackUseAfterScope { id, op }),
            Some(_) => Ok(false),
            None => Ok(!object.immortal),
        }
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
    ///
    /// Immortal and stack objects as for [`Heap::retain`]; a `Ref` to
    /// either held by a freed object is not released.
    pub fn release(&mut self, id: ObjId) -> Result<usize, AuditError> {
        if !self.check_count_op(id, Op::Release)? {
            return Ok(0);
        }
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

    /// The object behind `id`, which must be live for `op`: not freed
    /// (`UseAfterFree`) and, for a stack object, its scope not ended
    /// (`StackUseAfterScope`).
    fn live_object(&self, id: ObjId, op: Op) -> Result<&Object, AuditError> {
        let object = self.object(id)?;
        if object.live {
            Ok(object)
        } else if object.scope.is_some() {
            Err(AuditError::StackUseAfterScope { id, op })
        } else {
            Err(AuditError::UseAfterFree { id, op })
        }
    }

    /// Every live counted object's id, in allocation order: immortal
    /// and stack objects are not audited at exit.
    fn live_counted_ids(&self) -> Vec<ObjId> {
        self.objects
            .iter()
            .enumerate()
            .filter(|(_, object)| object.live && object.counted())
            .map(|(index, _)| ObjId::from_index(index))
            .collect()
    }
}
