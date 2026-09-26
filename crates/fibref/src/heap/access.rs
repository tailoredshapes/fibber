//! Audited access to objects: reading and writing fields, weak
//! references, and inspection for tests and the interpreter.

use super::cascade::PendingWrite;
use super::error::{AuditError, Op};
use super::event::Event;
use super::value::{Kind, ObjId, Value};
use super::Heap;

impl Heap {
    /// Reads a field. The value is returned without retaining it: the
    /// caller decides whether the reference escapes (§3). Event: `Read`.
    pub fn read(&mut self, id: ObjId, field: usize) -> Result<Value, AuditError> {
        let object = self.live_object(id, Op::Read)?;
        let value = *object
            .fields
            .get(field)
            .ok_or(AuditError::BadField { id, index: field })?;
        self.trace.push(Event::Read { id, field });
        Ok(value)
    }

    /// Writes the slot of a `Cell` or `Atom` (§6, §7). The new value is
    /// retained if it is a `Ref`; the old value is released afterwards
    /// if it was one, which may free it and whatever it held. Writing
    /// into a shared `Atom` makes whatever the value lets the other
    /// thread reach shared (§7): the target of a `Ref`, or of a `Weak`
    /// whose target is live, exactly as `mark_shared` decides it. So it
    /// fails with `SharedCell` if a `Cell` is reachable that way.
    ///
    /// The old value's release is planned before anything changes
    /// (`cascade`): if it would reach a freed object (a `Ref` that
    /// dangles after an earlier over-release) the write fails with
    /// `ReleaseOfFreed`, and the new value is neither retained nor
    /// stored.
    ///
    /// Events: `Retain` of the new value (if a `Ref`), `Write`, then the
    /// release events of the old value (if it was a `Ref`).
    pub fn write(&mut self, id: ObjId, field: usize, value: Value) -> Result<(), AuditError> {
        let object = self.live_object(id, Op::Write)?;
        if !object.kind.is_mutable() {
            return Err(AuditError::WriteToImmutable { id });
        }
        let old = *object
            .fields
            .get(field)
            .ok_or(AuditError::BadField { id, index: field })?;
        let shared = object.shared;
        self.check_storable(value)?;
        let cascade = match old.as_ref() {
            Some(target) => {
                let pending = PendingWrite { id, field, value };
                self.plan_release(target, Some(pending))?
            }
            None => Vec::new(),
        };
        if shared {
            if let Some(target) = self.crossing_target(value) {
                self.share_from(target)?;
            }
        }
        if let Some(target) = value.as_ref() {
            self.bump(target);
        }
        self.objects[id.index()].fields[field] = value;
        self.trace.push(Event::Write { id, field });
        self.apply_release(&cascade);
        Ok(())
    }

    /// Makes a weak reference to a live object (§6). No count change.
    /// Event: `Weak`.
    pub fn weak(&mut self, id: ObjId) -> Result<Value, AuditError> {
        self.live_object(id, Op::Weak)?;
        self.trace.push(Event::Weak { id });
        Ok(Value::Weak(id))
    }

    /// Turns a weak reference back into a counted one (§6): `Some` with
    /// the object retained if it is still live, `None` if it has been
    /// freed. Ids are never reused, so a stale id cannot name a newer
    /// object. Events: `Upgrade`, then `Retain` when live.
    pub fn upgrade(&mut self, id: ObjId) -> Result<Option<ObjId>, AuditError> {
        let live = self.object(id)?.live;
        self.trace.push(Event::Upgrade { id, live });
        if live {
            self.bump(id);
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }

    /// The reference count of a live object.
    pub fn count(&self, id: ObjId) -> Result<usize, AuditError> {
        Ok(self.live_object(id, Op::Inspect)?.count)
    }

    /// Whether the object exists and has not been freed.
    pub fn is_live(&self, id: ObjId) -> bool {
        self.object(id).map(|o| o.live).unwrap_or(false)
    }

    /// The kind of a live object.
    pub fn kind(&self, id: ObjId) -> Result<Kind, AuditError> {
        Ok(self.live_object(id, Op::Inspect)?.kind)
    }

    /// Whether a live object has crossed a thread boundary (§7), so its
    /// count operations are the atomic kind.
    pub fn is_shared(&self, id: ObjId) -> Result<bool, AuditError> {
        Ok(self.live_object(id, Op::Inspect)?.shared)
    }

    /// The number of fields of a live object.
    pub fn field_count(&self, id: ObjId) -> Result<usize, AuditError> {
        Ok(self.live_object(id, Op::Inspect)?.fields.len())
    }
}
