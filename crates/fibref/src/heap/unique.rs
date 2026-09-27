//! Unique writes (`spec/types.md` §6.6, §8.2): `set-field!` and
//! `array-set!` write an `Immutable` object in place when `fib.unique?`
//! holds, and otherwise copy. The copy is the interpreter's business
//! (an ordinary `alloc` and `write`); the heap checks that an in-place
//! write happens only under the test, so an interpreter that writes
//! where the compiled program would copy is caught.
//!
//! The primitive is given the **place**, the cell whose content it
//! updates (a variable's cell, or the private cell of an `&`
//! parameter, which is a stack cell, §6.6), not the object: the object
//! is the place's content, read without retaining. That is what makes
//! the audit's cycle rule hold. A unique write closes a cycle only
//! through a cell: the object it writes has count 1, and that one
//! counted reference is held by the cell of the place written, so every
//! path of counted references from another object to it passes through
//! that cell, and so does every cycle the write closes (§6.6). Given
//! only an object, the heap could not tell whether its one count is held
//! by a cell or by an immutable object reaching it, and a write through
//! the latter would close a cycle of immutable objects (`ImmutableCycle`,
//! which §6.7 says a program cannot make). The one path the count
//! cannot see is the value itself being the object (the caller holds
//! it, so under counting its count is at least 2, §2); that is refused
//! as `StoresItself`.

use super::error::{AuditError, Op, Uniqueness};
use super::event::Event;
use super::object::Object;
use super::store::Holder;
use super::value::{Kind, ObjId, Value};
use super::Heap;

impl Heap {
    /// `fib.unique?` (§8.2) on a live object: none of `SHARED`,
    /// `IMMORTAL`, `STACK`, and a count of exactly 1. The interpreter
    /// calls it to decide between writing in place and copying.
    pub fn is_unique(&self, id: ObjId) -> Result<bool, AuditError> {
        Ok(not_unique(self.live_object(id, Op::Inspect)?).is_none())
    }

    /// Writes field `field` of the `Immutable` object held by the cell
    /// `place`, in place (§6.6). Legal iff `place` is a live `Cell`
    /// holding a `Ref` to a live `Immutable` object on which
    /// [`Heap::is_unique`] holds, `value` is not a `Ref` to that object,
    /// and `value` may be stored into a heap object (`store`: not a
    /// stack object). Otherwise `NotUnique` (or the ordinary access
    /// error), and nothing changes. Returns the object written.
    ///
    /// The same discipline as [`Heap::write`]: the old value's release
    /// cascade is planned before anything changes, then the new value is
    /// retained, stored, and the old one released. Events: `Retain` of
    /// the new value (if a counted `Ref`), `WriteUnique`, then the
    /// release events of the old value.
    pub fn write_unique(
        &mut self,
        place: ObjId,
        field: usize,
        value: Value,
    ) -> Result<ObjId, AuditError> {
        let id = self.unique_content(place)?;
        if field >= self.objects[id.index()].fields.len() {
            return Err(AuditError::BadField { id, index: field });
        }
        if value.as_ref() == Some(id) {
            let why = Uniqueness::StoresItself;
            return Err(AuditError::NotUnique { id, why });
        }
        self.check_storable(value, Holder::Heap)?;
        let event = Event::WriteUnique { place, id, field };
        self.store_field(id, field, value, false, event)?;
        Ok(id)
    }

    /// The object in `place` if a unique write may change it in place.
    fn unique_content(&self, place: ObjId) -> Result<ObjId, AuditError> {
        let cell = self.live_object(place, Op::Write)?;
        if cell.kind != Kind::Cell {
            let why = Uniqueness::PlaceNotCell;
            return Err(AuditError::NotUnique { id: place, why });
        }
        let Some(id) = cell.fields.first().and_then(|value| value.as_ref()) else {
            let why = Uniqueness::ContentNotImmutable;
            return Err(AuditError::NotUnique { id: place, why });
        };
        let object = self.live_object(id, Op::Write)?;
        if object.kind != Kind::Immutable {
            let why = Uniqueness::ContentNotImmutable;
            return Err(AuditError::NotUnique { id, why });
        }
        match not_unique(object) {
            Some(why) => Err(AuditError::NotUnique { id, why }),
            None => Ok(id),
        }
    }
}

/// Why `fib.unique?` is false on `object`, testing the flags before the
/// count as §8.2 does, or `None` if it is true.
fn not_unique(object: &Object) -> Option<Uniqueness> {
    if object.shared {
        Some(Uniqueness::Shared)
    } else if object.immortal {
        Some(Uniqueness::Immortal)
    } else if object.scope.is_some() {
        Some(Uniqueness::Stack)
    } else if object.count != 1 {
        Some(Uniqueness::Count(object.count))
    } else {
        None
    }
}
