//! Stack scopes (`spec/types.md` §6.11): scope-local objects are
//! allocated `STACK`, receive no count operation, and end with the
//! scope that the compiled code ends them in, their drop releasing what
//! they hold.
//!
//! The caller drives the scopes explicitly: [`Heap::open_scope`] opens
//! one, and any open scope may end, in any order. A scope is never a
//! function's activation: it is the lifetime of the stack objects of
//! a `let` or `match` binding, a temporary of a step, or a private `&`
//! cell (§6.11). An `async` body's scopes belong to its task and stay
//! open across an `await`.
//!
//! Scopes need not nest because the compiled program's do not (§8.2):
//! every `STACK` allocation site has its own `alloca` in the function's
//! entry block, and a scope's end is an inline drop at the point the
//! plan names, so a temporary of a step may end while an object made
//! after it, bound by a `let` of that step, lives on (syntax §2). What
//! the audit must catch is a use of an object after its end
//! (`StackUseAfterScope`) and a reference that could outlive its
//! target: into a heap object (`StackRefInHeap`), or into a stack
//! object whose scope was opened before the target's
//! (`StackRefIntoOuterScope`, `store`).

use super::error::AuditError;
use super::event::Event;
use super::object::Object;
use super::store::Holder;
use super::value::{Kind, ObjId, ScopeId, Value};
use super::Heap;

/// One scope ever opened.
#[derive(Debug, Default)]
pub(super) struct Scope {
    /// The stack objects allocated in it, in allocation order.
    objects: Vec<ObjId>,
    ended: bool,
}

impl Heap {
    /// Opens a scope. Event: `ScopeOpen`.
    pub fn open_scope(&mut self) -> ScopeId {
        let scope = ScopeId::from_index(self.scopes.len());
        self.scopes.push(Scope::default());
        self.open.push(scope);
        self.trace.push(Event::ScopeOpen { scope });
        scope
    }

    /// Allocates a `STACK` object of `scope` (§6.11, §8.2): count 0, no
    /// count operation ever, ended by [`Heap::end_scope`]. `scope` must
    /// be open. Every `Ref` in `fields` to a counted object is retained
    /// (the drop releases it); a `Ref` may also name a stack object of
    /// this scope or one opened before it, never of a later one (`store`).
    /// Events: `AllocStack`, then one `Retain` per `Ref` field to a
    /// counted object, in field order.
    pub fn alloc_in_scope(
        &mut self,
        scope: ScopeId,
        kind: Kind,
        fields: Vec<Value>,
    ) -> Result<ObjId, AuditError> {
        self.check_open(scope)?;
        self.check_new(kind, &fields, Holder::Stack(scope))?;
        let object = Object::on_stack(kind, fields, scope);
        let id = self.install(object, |id| Event::AllocStack { id, kind, scope });
        self.scopes[scope.index()].objects.push(id);
        Ok(id)
    }

    /// Ends `scope`, which must be open (not necessarily the one opened
    /// last, §8.2): each of its
    /// objects, most recently allocated first (the reverse of the order
    /// in which the scope's bindings were made, §6.11), is ended and its
    /// drop releases the counted `Ref`s it holds, in field order, with
    /// their cascades. The whole drop is planned first (`cascade`): if
    /// it would release a freed object, the result is `ReleaseOfFreed`
    /// and nothing changes. An ended object is never freed; any later
    /// use of it is `StackUseAfterScope`.
    ///
    /// Events: `ScopeEnd`, then for each object `Drop` followed by the
    /// release events of what it held.
    pub fn end_scope(&mut self, scope: ScopeId) -> Result<(), AuditError> {
        self.check_open(scope)?;
        let dropped: Vec<ObjId> = self.scopes[scope.index()]
            .objects
            .iter()
            .rev()
            .copied()
            .collect();
        let cascades = self.plan_drops(&dropped)?;
        self.open.retain(|s| *s != scope);
        self.scopes[scope.index()].ended = true;
        self.trace.push(Event::ScopeEnd { scope });
        for (id, cascade) in dropped.into_iter().zip(cascades) {
            self.objects[id.index()].free();
            self.trace.push(Event::Drop { id });
            self.apply_release(&cascade);
        }
        Ok(())
    }

    /// The scopes still open, in the order they were opened.
    pub fn open_scopes(&self) -> &[ScopeId] {
        &self.open
    }

    /// `scope` was opened by this heap and has not ended.
    fn check_open(&self, scope: ScopeId) -> Result<(), AuditError> {
        match self.scopes.get(scope.index()) {
            None => Err(AuditError::UnknownScope { scope }),
            Some(state) if state.ended => Err(AuditError::ScopeEnded { scope }),
            Some(_) => Ok(()),
        }
    }
}
