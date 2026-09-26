//! Release cascades (`spec/ownership.md` §2): an object whose count
//! reaches zero is freed and releases every `Ref` it holds, which may
//! free more objects.
//!
//! A cascade is planned before it is applied. The plan walks the objects
//! in exactly the order the cascade will release them and fails, having
//! changed nothing, if the cascade would release an object that is
//! already freed: a `Ref` left dangling by an earlier over-release, or
//! an object the cascade itself frees and then reaches again through
//! its own fields. So a failing release, and a write whose old value's
//! release would fail, leave the heap exactly as it was ([`AuditError`]).
//! Both walks use an explicit worklist, never recursion, so a list of
//! any length is fine.

use std::collections::HashMap;

use super::error::AuditError;
use super::event::Event;
use super::value::{ObjId, Value};
use super::Heap;

/// A write that has been decided but not yet done. A write retains its
/// new value and stores it before releasing the old one, but plans that
/// release first, so the plan reads the heap as if the write had
/// happened: the slot holds `value`, and `value`'s target, if a `Ref`,
/// counts one more.
#[derive(Debug, Clone, Copy)]
pub(super) struct PendingWrite {
    pub(super) id: ObjId,
    pub(super) field: usize,
    pub(super) value: Value,
}

impl Heap {
    /// The order in which releasing `root` once would release objects,
    /// depth first in field order (the order a recursive implementation
    /// would produce), or the error the cascade would hit. Counts and
    /// fields are read as they are now, adjusted for `pending`.
    pub(super) fn plan_release(
        &self,
        root: ObjId,
        pending: Option<PendingWrite>,
    ) -> Result<Vec<ObjId>, AuditError> {
        let mut counts: HashMap<ObjId, usize> = HashMap::new();
        if let Some(id) = pending.and_then(|p| p.value.as_ref()) {
            counts.insert(id, self.object(id)?.count + 1);
        }
        let mut order = Vec::new();
        let mut worklist = vec![root];
        while let Some(id) = worklist.pop() {
            let object = self.object(id)?;
            let count = counts.entry(id).or_insert(object.count);
            if !object.live || *count == 0 {
                return Err(AuditError::ReleaseOfFreed { id });
            }
            *count -= 1;
            order.push(id);
            if *count == 0 {
                worklist.extend(self.held_refs_reversed(id, pending));
            }
        }
        Ok(order)
    }

    /// The `Ref`s in `id`'s fields as the cascade will see them, in
    /// reverse so that a worklist pops them in field order.
    fn held_refs_reversed(&self, id: ObjId, pending: Option<PendingWrite>) -> Vec<ObjId> {
        let object = &self.objects[id.index()];
        object
            .fields
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(field, &value)| match pending {
                Some(p) if p.id == id && p.field == field => p.value.as_ref(),
                _ => value.as_ref(),
            })
            .collect()
    }

    /// Applies a plan from [`Heap::plan_release`]: count -= 1 on each
    /// object in order, freeing it at zero. Events: `Release`, then
    /// `Free` when the count reached zero.
    pub(super) fn apply_release(&mut self, order: &[ObjId]) {
        for &id in order {
            // The plan checked that every object here is live with a
            // count above zero when its turn comes, so this cannot
            // underflow.
            let object = &mut self.objects[id.index()];
            object.count -= 1;
            let count_after = object.count;
            self.trace.push(Event::Release { id, count_after });
            if count_after == 0 {
                self.objects[id.index()].free();
                self.trace.push(Event::Free { id });
            }
        }
    }
}
