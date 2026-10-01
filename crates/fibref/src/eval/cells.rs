//! Cells, atoms and weak references (types §6.7, §8.6, §8.7): `@`,
//! `set!`, `swap!`, `weak`. Atoms need no lock here: the executor runs
//! one thread at a time and switches only at scheduling points (see
//! `sched`), so reading and retaining is one step, and so is `swap!`'s
//! compare and store; its compare fails when `f` changed the atom, or
//! another thread did while `f` ran.

use crate::heap::Kind;
use crate::own::{option_rep, OptionRep};
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
    /// reference upgraded: `(some x)` retained, or `nil`, represented
    /// as `e`'s static `Option` type says (§8.1: a heap enum for a
    /// `dyn` target, §8.7; nothing for another object).
    pub fn deref_place(&mut self, e: &'p Expr, p: &'p Place) -> R<Val> {
        let c = self.place_cell(p)?;
        let rep = self
            .p
            .expr_types
            .get(&e.id)
            .and_then(|t| option_rep(&self.p.globals, t));
        let at = match rep {
            Some(OptionRep::HeapEnum) => Placement::Heap,
            Some(OptionRep::Pointer) => Placement::Unboxed,
            _ => Placement::Undecided,
        };
        self.deref_val(&c, at)
    }

    /// `@c` on a value; on an atom or a weak reference, a scheduling
    /// point first. `at` places an upgrade's `Option`. On a task it is
    /// `join` (types §2.9, owner's rule 2026-10-01): the task's result
    /// retained for the caller, the scheduling point and the waiting
    /// those of `join`.
    pub fn deref_val(&mut self, c: &Val, at: Placement) -> R<Val> {
        let id = c.expect_obj("the operand of @")?;
        match self.objs.get(&self.heap, id)? {
            Obj::Task(_) => return self.join(c),
            Obj::Atom(_) | Obj::Weak(_) => self.tick()?,
            _ => {}
        }
        if let Obj::Weak(target) = self.objs.get(&self.heap, id)? {
            let target = *target;
            self.heap.read(id, 0)?;
            let Some(t) = self.heap.upgrade(target)? else {
                return self.option_value(Some(0), Vec::new(), at);
            };
            // The upgrade's count passes to a heap enum's store (E2);
            // an unboxed `some` keeps it as the payload's own.
            let t = Val::Obj(t);
            let v = self.option_value(Some(1), vec![t.clone()], at)?;
            if !matches!(v, Val::Some(_)) {
                self.give_back(std::slice::from_ref(&t))?;
            }
            return Ok(v);
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

    /// `(swap! a f)` (§8.6): compare and retry. Each attempt acquires
    /// `old` as a snapshot, hands `f` a count of its own on `old` and on
    /// itself (the closure convention, §8.4: the callee owns `env` and
    /// every object argument, and a heap closure's body releases `env`
    /// at its exit) and runs it. If the atom still holds `old`, `f`'s
    /// result is stored (share-marked if the atom is shared, which the
    /// heap does) and returned with the count the call gave it, and the
    /// atom's count on `old` and the snapshot are released. If `f`
    /// changed the atom (itself, through a thread it waited for, or
    /// another thread that ran at a scheduling point inside `f`), the
    /// snapshot and the result
    /// are released and `f` runs again on the new content. Each attempt
    /// begins with a scheduling point; none falls between the compare
    /// and the store.
    pub fn swap(&mut self, a: &Val, f: &Val, pos: &Pos) -> R<Val> {
        let atom = a.expect_obj("the atom of swap!")?;
        loop {
            self.tick()?;
            let old = self.slot(atom)?;
            self.retain(&old)?;
            self.retain(&old)?;
            self.retain(f)?;
            let target = self.value_target(f, std::slice::from_ref(&old))?;
            let jump = Jump {
                target,
                args: vec![old.clone()],
                pos: pos.clone(),
                placement: Placement::Heap,
            };
            let new = self.invoke(jump)?;
            if same(&self.slot(atom)?, &old) {
                self.write_slot(atom, new.clone())?;
                self.release(&old)?;
                return Ok(new);
            }
            self.release(&old)?;
            self.release(&new)?;
        }
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

/// `load == old` in `swap!` (§8.6): the same object, or the same bits
/// of a scalar, as a `cmpxchg` on the atom's word compares (so a NaN
/// equals itself and `0.0` does not equal `-0.0`). The snapshot keeps
/// `old` alive, so its identity cannot be reused meanwhile.
fn same(a: &Val, b: &Val) -> bool {
    match (a, b) {
        (Val::Float(x, w), Val::Float(y, v)) => x.to_bits() == y.to_bits() && w == v,
        (Val::Some(x), Val::Some(y)) => same(x, y),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::{Heap, Kind};
    use crate::types::ty::Scalar;

    #[test]
    fn swap_compares_identity_and_bits() {
        let nan = Val::Float(f64::NAN, Scalar::F64);
        assert!(same(&nan, &nan.clone()));
        let zero = Val::Float(0.0, Scalar::F64);
        assert!(!same(&zero, &Val::Float(-0.0, Scalar::F64)));
        assert!(same(&Val::Int(3, Scalar::I64), &Val::Int(3, Scalar::I64)));
        assert!(!same(&Val::Int(3, Scalar::I64), &Val::Int(4, Scalar::I64)));
        let mut heap = Heap::new();
        let mut obj = || heap.alloc(Kind::Immutable, vec![]).map(Val::Obj);
        let (a, b) = (obj().expect("alloc"), obj().expect("alloc"));
        assert!(same(&a, &a.clone()));
        assert!(!same(&a, &b));
        let some = |v: &Val| Val::Some(Box::new(v.clone()));
        assert!(same(&some(&a), &some(&a)));
        assert!(!same(&some(&a), &Val::None));
    }
}
