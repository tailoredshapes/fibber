//! Thread crossing (§7): marking an object and everything reachable
//! from it as shared.

use std::collections::HashSet;

use super::error::{AuditError, Op};
use super::event::Event;
use super::value::{Kind, ObjId, Value};
use super::Heap;

impl Heap {
    /// Marks `id` and everything reachable from it as shared (§7):
    /// from now on their count operations are the atomic kind. Fails
    /// with `SharedCell` if any reachable object is a `Cell` (an `Atom`
    /// may cross), in which case nothing is marked.
    ///
    /// Reachability follows `Ref` fields and also `Weak` fields whose
    /// target is live, because the other thread could upgrade the weak
    /// reference and then hold the target. A `Ref` to a freed object (a
    /// field left dangling by an earlier over-release) is `UseAfterFree`
    /// with `Op::Share`; it is found before any `SharedCell` and nothing
    /// is marked. Events: one `Shared` per object newly marked, in the
    /// order the search reached them.
    pub fn mark_shared(&mut self, id: ObjId) -> Result<(), AuditError> {
        self.live_object(id, Op::Share)?;
        self.share_from(id)
    }

    /// `mark_shared` on an object already known to be live.
    pub(super) fn share_from(&mut self, root: ObjId) -> Result<(), AuditError> {
        let reached = self.reachable_from(root)?;
        for &id in &reached {
            if self.objects[id.index()].kind == Kind::Cell {
                return Err(AuditError::SharedCell { id });
            }
        }
        for id in reached {
            let object = &mut self.objects[id.index()];
            if !object.shared {
                object.shared = true;
                self.trace.push(Event::Shared { id });
            }
        }
        Ok(())
    }

    /// Every live object reachable from `root` (including `root`), in
    /// the order a depth-first search reaches them, or `UseAfterFree`
    /// for the first `Ref` field met whose target is freed. Iterative.
    fn reachable_from(&self, root: ObjId) -> Result<Vec<ObjId>, AuditError> {
        let mut seen: HashSet<ObjId> = HashSet::new();
        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            let object = &self.objects[id.index()];
            for value in object.fields.iter().rev() {
                if let Value::Ref(target) = *value {
                    self.live_object(target, Op::Share)?;
                }
                if let Some(target) = self.crossing_target(*value) {
                    stack.push(target);
                }
            }
        }
        Ok(order)
    }

    /// The live object a field value lets another thread reach, if any:
    /// the target of a `Ref`, or of a `Weak` whose target is still live
    /// (the other thread could upgrade it). Both routes across a thread
    /// boundary, `mark_shared` and `write` into a shared `Atom`, decide
    /// crossing with this one function so they cannot disagree.
    pub(super) fn crossing_target(&self, value: Value) -> Option<ObjId> {
        match value {
            Value::Ref(id) => Some(id),
            Value::Weak(id) if self.is_live(id) => Some(id),
            _ => None,
        }
    }
}
