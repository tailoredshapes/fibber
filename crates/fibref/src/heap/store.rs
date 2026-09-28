//! What may be stored where (`spec/types.md` §6.7, §6.11, §8.2), and
//! the one place that installs a new object and retains its fields.
//!
//! A `Ref` names its target with a count, except when the target is
//! `IMMORTAL` or `STACK`, whose retain and release are no-ops (§8.2);
//! [`Heap::counted_target`] is that test. The rules on holders:
//!
//! - a heap object may hold a `Ref` to any live heap or immortal object,
//!   never to a stack object (`StackRefInHeap`), since it could outlive
//!   the stack object's scope;
//! - a stack object of scope `s` may also hold a `Ref` to a stack object
//!   of `s` or of a scope opened before `s`, never of a scope opened
//!   after `s` (`StackRefIntoOuterScope`): the objects a stack object
//!   legitimately refers to (a stack closure's alias captures, a
//!   private cell's copied-in content) exist before it, and one made
//!   after it could end first and leave it pointing at a dead slot;
//! - an immortal object holds only immortal objects (a constant graph
//!   is closed, §6.7): anything else is `ImmortalHoldsMortal`;
//! - no one holds a `Weak` to a stack object (`WeakToStack`).

use super::error::{AuditError, Op};
use super::event::Event;
use super::object::Object;
use super::value::{Kind, ObjId, ScopeId, Value};
use super::Heap;

/// What kind of object would hold a stored value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Holder {
    /// A counted heap object.
    Heap,
    /// An `IMMORTAL` object.
    Immortal,
    /// A `STACK` object of this scope.
    Stack(ScopeId),
}

impl Object {
    /// The holder this object is, for the storing rules.
    pub(super) fn holder(&self) -> Holder {
        match self.scope {
            Some(scope) => Holder::Stack(scope),
            None if self.immortal => Holder::Immortal,
            None => Holder::Heap,
        }
    }
}

impl Heap {
    /// Whether `value` may be stored into an object that is `holder`:
    /// a `Ref` must name a live object the holder may hold, a `Weak` a
    /// known object that is not on the stack, and scalars always may.
    pub(super) fn check_storable(&self, value: Value, holder: Holder) -> Result<(), AuditError> {
        match value {
            Value::Ref(id) => {
                let target = self.live_object(id, Op::Store)?;
                check_ref_target(id, target, holder)
            }
            Value::Weak(id) => {
                let target = self.object(id)?;
                if target.scope.is_some() {
                    Err(AuditError::WeakToStack { id })
                } else if holder == Holder::Immortal && !target.immortal {
                    Err(AuditError::ImmortalHoldsMortal { id })
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }

    /// Validates a new object's fields: a `Cell` or `Atom` has exactly
    /// one, and each must be storable into `holder`.
    pub(super) fn check_new(
        &self,
        kind: Kind,
        fields: &[Value],
        holder: Holder,
    ) -> Result<(), AuditError> {
        if kind.is_mutable() && fields.len() != 1 {
            return Err(AuditError::WrongSlotCount {
                kind,
                given: fields.len(),
            });
        }
        for value in fields {
            self.check_storable(*value, holder)?;
        }
        Ok(())
    }

    /// The object `value` holds a count on, if any: the target of a
    /// `Ref` that is neither `IMMORTAL` nor `STACK`. A `Ref` to a freed
    /// heap object still counts, so releasing it is caught.
    pub(super) fn counted_target(&self, value: Value) -> Option<ObjId> {
        value
            .as_ref()
            .filter(|id| self.objects.get(id.index()).is_some_and(Object::counted))
    }

    /// Retains what storing `value` counts: +1 and a `Retain` event for
    /// a `Ref` to a counted object, nothing otherwise (§8.2).
    pub(super) fn retain_stored(&mut self, value: Value) {
        if let Some(target) = self.counted_target(value) {
            self.bump(target);
        }
    }

    /// Adds an already validated object: pushes `event(id)`, then one
    /// `Retain` per field that is a `Ref` to a counted object, in field
    /// order. Returns the new id.
    pub(super) fn install(&mut self, object: Object, event: impl FnOnce(ObjId) -> Event) -> ObjId {
        let id = ObjId::from_index(self.objects.len());
        self.trace.push(event(id));
        let fields = object.fields.clone();
        self.objects.push(object);
        for value in fields {
            self.retain_stored(value);
        }
        id
    }
}

/// Whether a `holder` may hold a `Ref` to the live object `target`.
fn check_ref_target(id: ObjId, target: &Object, holder: Holder) -> Result<(), AuditError> {
    if target.immortal {
        return Ok(());
    }
    match (holder, target.scope) {
        (Holder::Immortal, _) => Err(AuditError::ImmortalHoldsMortal { id }),
        (Holder::Heap, Some(_)) => Err(AuditError::StackRefInHeap { id }),
        // Both scopes are open (the holder and the target are live);
        // scope ids are handed out in opening order.
        (Holder::Stack(outer), Some(inner)) if inner > outer => {
            Err(AuditError::StackRefIntoOuterScope { id, scope: outer })
        }
        _ => Ok(()),
    }
}
