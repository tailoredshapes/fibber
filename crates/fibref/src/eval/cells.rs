//! Cells, atoms and weak references (types §6.7, §8.6, §8.7): `@`,
//! `set!`, `swap!`, `weak`. Atoms need no lock here: the deterministic
//! executor never runs two threads at once (see `task`), so reading
//! and retaining is one step, and `swap!`'s compare always succeeds.

use crate::heap::Kind;
use crate::syntax::Pos;
use crate::types::ast::{Expr, Place};

use super::alloc::Placement;
use super::call::Jump;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl<'p> Interp<'p> {
    /// The cell a place names: an `&` parameter's private cell, or the
    /// value of an operand.
    fn place_cell(&mut self, p: &'p Place) -> R<Val> {
        match p {
            Place::Amp(b) => self.local(*b),
            Place::Expr(e) => self.val(e),
        }
    }

    /// `@p` (§6.7): a cell or atom's content, acquired (+1); a weak
    /// reference upgraded: `(some x)` retained, or `nil`.
    pub fn deref_place(&mut self, p: &'p Place) -> R<Val> {
        let c = self.place_cell(p)?;
        self.deref_val(&c)
    }

    /// `@c` on a value.
    pub fn deref_val(&mut self, c: &Val) -> R<Val> {
        let id = c.expect_obj("the operand of @")?;
        if let Obj::Weak(target) = self.objs.get(id)? {
            let target = *target;
            self.heap.read(id, 0)?;
            return Ok(match self.heap.upgrade(target)? {
                Some(t) => Val::Some(Box::new(Val::Obj(t))),
                None => Val::None,
            });
        }
        let v = self.slot(id)?;
        self.retain(&v)?;
        Ok(v)
    }

    /// `(set! p v)`: the value's store retain is in the plan; the store
    /// and the release of the old content are the primitive's.
    pub fn set_place(&mut self, p: &'p Place, v: &'p Expr) -> R<Val> {
        let c = self.place_cell(p)?;
        let v = self.val(v)?;
        self.set_cell(&c, &v)
    }

    /// `(swap! a f)` (§8.6): `old` acquired as a snapshot, handed to `f`
    /// with a count of its own (the closure convention), `f`'s result
    /// stored (share-marked if the atom is shared, which the heap does)
    /// and returned with the count the call gave it; the atom's count
    /// on `old` and the snapshot released.
    pub fn swap(&mut self, a: &Val, f: &Val, pos: &Pos) -> R<Val> {
        let atom = a.expect_obj("the atom of swap!")?;
        let old = self.slot(atom)?;
        self.retain(&old)?;
        self.retain(&old)?;
        let target = self.value_target(f, std::slice::from_ref(&old))?;
        let jump = Jump {
            target,
            args: vec![old.clone()],
            pos: pos.clone(),
            placement: Placement::Heap,
        };
        let new = self.invoke(jump)?;
        self.write_slot(atom, new.clone())?;
        self.release(&old)?;
        Ok(new)
    }

    /// `(weak x)` (§6.7, §8.7): `x`'s box, retained, or a new one; no
    /// count on `x`. A box of an immortal object is always new and never
    /// registered. Unlike the compiled runtime's, whose table keeps a box
    /// until its target dies, a box here is freed when its own count
    /// reaches zero; a later `weak` of the same object then makes a new
    /// one (no program can tell the two apart).
    pub fn weak(&mut self, x: &Val) -> R<Val> {
        let target = match x {
            Val::Obj(id) => *id,
            v => {
                return Err(RunError::unsupported(format!(
                    "weak of {v:?}: the interpreter makes weak references to objects only"
                )))
            }
        };
        if let Some(b) = self.weak_boxes.get(&target).copied() {
            if self.heap.is_live(b) {
                self.heap.weak(target)?;
                self.heap.retain(b)?;
                return Ok(Val::Obj(b));
            }
        }
        let w = self.heap.weak(target)?;
        let id = self.alloc(Kind::Immutable, vec![w], Obj::Weak(target), Placement::Heap)?;
        if !self.heap.is_immortal(target)? {
            self.weak_boxes.insert(target, id);
        }
        Ok(Val::Obj(id))
    }
}
