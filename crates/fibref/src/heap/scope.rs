//! Stack scopes (`spec/types.md` §6.11): scope-local objects are
//! allocated `STACK`, receive no count operation, and end with the
//! scope that the compiled code ends them in, their drop releasing what
//! they hold.
//!
//! The caller drives the scopes explicitly: [`Heap::open_scope`] opens
//! one inside the innermost open scope, and only the innermost open
//! scope may end. A scope is never a function's activation: it is the
//! `let` or `match` of a binding, the step of a temporary, or the call
//! of a private `&` cell (§6.11). An `async` body's scopes belong to its
//! task and stay open across an `await`. Tasks are not modelled here:
//! the heap keeps one strictly nested stack of scopes, so an
//! interpreter that interleaves tasks must keep their scopes nested in
//! the order it opens them, or ending one is `ScopeNotInnermost`.

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
    /// Opens a scope nested in the innermost open one (or outermost if
    /// none is open). Event: `ScopeOpen`.
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
    /// this scope or an outer one, never of an inner one (`store`).
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

    /// Ends `scope`, which must be the innermost open scope: each of its
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
        if let Some(&innermost) = self.open.last() {
            if innermost != scope {
                return Err(AuditError::ScopeNotInnermost { scope, innermost });
            }
        }
        let dropped: Vec<ObjId> = self.scopes[scope.index()]
            .objects
            .iter()
            .rev()
            .copied()
            .collect();
        let cascades = self.plan_drops(&dropped)?;
        self.open.pop();
        self.scopes[scope.index()].ended = true;
        self.trace.push(Event::ScopeEnd { scope });
        for (id, cascade) in dropped.into_iter().zip(cascades) {
            self.objects[id.index()].free();
            self.trace.push(Event::Drop { id });
            self.apply_release(&cascade);
        }
        Ok(())
    }

    /// The scopes still open, outermost first.
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
