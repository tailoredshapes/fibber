//! `fib.immortalise` (`spec/types.md` §8.2, syntax §3.19): the value of
//! a `def` is evaluated as ordinary counted objects and then made
//! static data, with everything reachable from it.

use std::collections::HashSet;

use super::error::{AuditError, Op};
use super::event::Event;
use super::value::{Kind, ObjId, Value};
use super::Heap;

impl Heap {
    /// Makes `root` and every object reachable from it through `Ref`
    /// fields `IMMORTAL`: count 0, never freed, not audited at exit,
    /// count operations no-ops (§8.2: "walk p through trace, stopping at
    /// IMMORTAL objects; on each: count := 0, flags |= IMMORTAL").
    ///
    /// A constant builds only immutable objects (syntax §3.19), so a
    /// reachable `Cell` or `Atom` is `MutableImmortal`, a reachable stack
    /// object `StackRefInHeap` (static data would point into a frame),
    /// a `Weak` field whose target is not immortal once the walk is done
    /// `ImmortalHoldsMortal`, and a freed object `UseAfterFree`. The
    /// whole graph is validated before anything changes: on `Err`
    /// nothing is marked. Events: one `Immortalised` per object marked,
    /// in the order the search reached them.
    pub fn immortalise(&mut self, root: ObjId) -> Result<(), AuditError> {
        let reached = self.immortal_graph(root)?;
        let set: HashSet<ObjId> = reached.iter().copied().collect();
        for &id in &reached {
            for value in &self.objects[id.index()].fields {
                if let Value::Weak(target) = *value {
                    let t = self.object(target)?;
                    if !t.immortal && !set.contains(&target) {
                        return Err(AuditError::ImmortalHoldsMortal { id: target });
                    }
                }
            }
        }
        for id in reached {
            let object = &mut self.objects[id.index()];
            object.immortal = true;
            object.count = 0;
            self.trace.push(Event::Immortalised { id });
        }
        Ok(())
    }

    /// The live counted objects reachable from `root` (included),
    /// stopping at immortal ones, depth first; or the first reason one
    /// of them cannot become static data.
    fn immortal_graph(&self, root: ObjId) -> Result<Vec<ObjId>, AuditError> {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let object = self.live_object(id, Op::Store)?;
            if object.immortal || !seen.insert(id) {
                continue;
            }
            if object.scope.is_some() {
                return Err(AuditError::StackRefInHeap { id });
            }
            if object.kind != Kind::Immutable {
                return Err(AuditError::MutableImmortal { kind: object.kind });
            }
            order.push(id);
            for value in object.fields.iter().rev() {
                if let Value::Ref(target) = *value {
                    stack.push(target);
                }
            }
        }
        Ok(order)
    }
}
