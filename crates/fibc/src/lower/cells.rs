//! Cells (types §8.6): `@c`, `(set! c v)`, `(cell e)`, and
//! `set-field!`. Atoms and weak references come with threads; `@t` of
//! a task is `join` (threads.rs).

use fibref::types::ast::{BindingId, Expr, Place};
use fibref::types::decls::Shape;
use fibref::types::ty::{Con, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::layout::{option_rep, variants, OptRep};
use crate::objects::{ATOM_LOCK, ATOM_VALUE, CELL_VALUE, ENUM_TAG, STRUCT_FIELD0, VARIANT_FIELD0};

impl<'a> Cx<'_, 'a> {
    /// The cell (or atom) a place names and its type.
    fn place(&mut self, pl: &'a Place) -> R<(V, Ty)> {
        match pl {
            Place::Amp(b) => Ok((self.local(*b)?, self.binding_ty(*b)?)),
            Place::Expr(e) => Ok((self.value(e)?, self.ty(e)?)),
        }
    }

    /// `@p`: the content, retained (§8.6, never elided).
    pub fn deref_place(&mut self, pl: &'a Place) -> R<V> {
        let (c, t) = self.place(pl)?;
        self.deref_val(&c, &t)
    }

    pub fn deref_val(&mut self, c: &V, t: &Ty) -> R<V> {
        let (con, content) = match t {
            Ty::Con(con @ (Con::Cell | Con::Atom | Con::Weak | Con::Task), args) => {
                (*con, args[0].clone())
            }
            _ => return Err(Unsupported("deref of a non-cell".into())),
        };
        if con == Con::Task {
            return self.join_value(c, &content);
        }
        if con == Con::Weak {
            return self.upgrade(c, &content);
        }
        let (_, sname) = self.p.object(t)?;
        if con == Con::Atom {
            // §8.6: lock, load, retain, unlock: one atomic step.
            let lockp = self.gep(&sname, c.text(), ATOM_LOCK);
            self.b.stmt(&format!("(call @fib.lock {lockp})"));
            let vp = self.gep(&sname, c.text(), ATOM_VALUE);
            let v = match self.p.lir(&content)? {
                Some(l) => self.load(l, &vp),
                None => V::Unit,
            };
            self.retain(&v);
            self.b.stmt(&format!("(call @fib.unlock {lockp})"));
            return Ok(v);
        }
        let v = self.cell_load(&sname, c.text(), &content)?;
        self.retain(&v);
        Ok(v)
    }

    /// `@w` on a weak box: the target retained if alive, else `nil`
    /// (§8.7). For a `dyn` target the value is `{ box, vt }` and the
    /// result a heap enum, `(some { t, vt })` or the tag `nil` (§8.1),
    /// allocated as the interpreter allocates it.
    fn upgrade(&mut self, w: &V, target: &Ty) -> R<V> {
        let g = self.p.g();
        if option_rep(g, target)? == OptRep::Null {
            return Ok(self
                .b
                .val(&format!("(call @fib.upgrade {})", w.text()), LirTy::Ptr));
        }
        if w.ty() != Some(LirTy::Dyn) {
            return Err(Unsupported("a weak reference to an Option".into()));
        }
        let opt = Ty::nominal(g.option, vec![target.clone()]);
        let (tid, sname) = self.p.object(&opt)?;
        let size = self.p.objects.get(tid).size();
        let bx = self
            .b
            .val(&format!("(extractvalue {} 0)", w.text()), LirTy::Ptr);
        let vt = self
            .b
            .val(&format!("(extractvalue {} 1)", w.text()), LirTy::Ptr);
        let t = self
            .b
            .val(&format!("(call @fib.upgrade {})", bx.text()), LirTy::Ptr);
        let gone = self
            .b
            .val(&format!("(icmp eq {} (ptr null))", t.text()), LirTy::I1);
        let (lnil, lsome, ljoin) = (
            self.b.label("wnil"),
            self.b.label("wsome"),
            self.b.label("wup"),
        );
        self.b.term(&format!("(br {} {lnil} {lsome})", gone.text()));
        self.b.open(&lnil);
        let pn = self.option_object(tid, &sname, size, 0, None);
        self.b.term(&format!("(br {ljoin})"));
        self.b.open(&lsome);
        let d = self
            .b
            .val(&format!("{{ {} {} }}", t.text(), vt.text()), LirTy::Dyn);
        let ps = self.option_object(tid, &sname, size, 1, Some(&d));
        self.b.term(&format!("(br {ljoin})"));
        self.b.open(&ljoin);
        Ok(self.b.phi(LirTy::Ptr, &[(lnil, pn), (lsome, ps)]))
    }

    /// A heap enum `Option` object with `tag` and, for `some`, its
    /// consumed payload.
    fn option_object(
        &mut self,
        tid: u32,
        sname: &str,
        size: u64,
        tag: i32,
        payload: Option<&V>,
    ) -> String {
        let p = self.heap_alloc(tid, size);
        let vs = format!("{sname}.v{tag}");
        let tagp = self.gep(&vs, &p, ENUM_TAG);
        self.b.stmt(&format!("(store (i32 {tag}) {tagp})"));
        if let Some(v) = payload {
            let f = self.gep(&vs, &p, VARIANT_FIELD0);
            self.store(v, &f);
        }
        p
    }

    /// `(reset! a v)`: the consumed value share-marked if the atom is,
    /// stored under the lock, the old value released (§8.6).
    pub fn reset(&mut self, a: &V, t: &Ty, v: &V) -> R<V> {
        let content = match t {
            Ty::Con(Con::Atom, args) => args[0].clone(),
            _ => return Err(Unsupported("reset! on a non-atom".into())),
        };
        let (_, sname) = self.p.object(t)?;
        if let Some(w) = self.obj_word(v) {
            self.b
                .stmt(&format!("(call @fib.share-into {} {w})", a.text()));
        }
        let lockp = self.gep(&sname, a.text(), ATOM_LOCK);
        self.b.stmt(&format!("(call @fib.lock {lockp})"));
        let vp = self.gep(&sname, a.text(), ATOM_VALUE);
        let old = match self.p.lir(&content)? {
            Some(l) => self.load(l, &vp),
            None => V::Unit,
        };
        self.store(v, &vp);
        self.b.stmt(&format!("(call @fib.unlock {lockp})"));
        self.release(&old);
        Ok(V::Unit)
    }

    /// `(swap! a f)` (§8.6, as the interpreter runs it): snapshot the
    /// value, call `f` on it with the counts a call through a value
    /// takes, store the result if the atom is unchanged, else retry.
    pub fn swap(&mut self, a: &V, t: &Ty, f: &V) -> R<V> {
        let content = match t {
            Ty::Con(Con::Atom, args) => args[0].clone(),
            _ => return Err(Unsupported("swap! on a non-atom".into())),
        };
        let (_, sname) = self.p.object(t)?;
        let l = self
            .p
            .lir(&content)?
            .ok_or_else(|| Unsupported("swap! on an atom of unit".into()))?;
        let lockp = self.gep(&sname, a.text(), ATOM_LOCK);
        let vp = self.gep(&sname, a.text(), ATOM_VALUE);
        let (head, done) = (self.b.label("swap"), self.b.label("swapped"));
        self.b.term(&format!("(br {head})"));
        self.b.open(&head);
        self.b.stmt(&format!("(call @fib.lock {lockp})"));
        let old = self.load(l, &vp);
        self.retain(&old);
        self.b.stmt(&format!("(call @fib.unlock {lockp})"));
        self.retain(&old);
        self.retain(f);
        let target = super::call::Target::Value(f.clone());
        let new = self.emit_call(&target, std::slice::from_ref(&old), &content, false)?;
        self.b.stmt(&format!("(call @fib.lock {lockp})"));
        let cur = self.load(l, &vp);
        let same = self.same_bits(&cur, &old);
        let (lyes, lno) = (self.b.label("unchanged"), self.b.label("changed"));
        self.b.term(&format!("(br {} {lyes} {lno})", same.text()));
        self.b.open(&lyes);
        if let Some(w) = self.obj_word(&new) {
            self.b
                .stmt(&format!("(call @fib.share-into {} {w})", a.text()));
        }
        self.store(&new, &vp);
        self.retain(&new);
        self.b.stmt(&format!("(call @fib.unlock {lockp})"));
        self.release(&old);
        self.release(&old);
        self.b.term(&format!("(br {done})"));
        self.b.open(&lno);
        self.b.stmt(&format!("(call @fib.unlock {lockp})"));
        self.release(&old);
        self.release(&new);
        self.b.term(&format!("(br {head})"));
        self.b.open(&done);
        Ok(new)
    }

    /// Whether two values of one type are the same word (an object's
    /// address, a scalar's bits).
    fn same_bits(&mut self, x: &V, y: &V) -> V {
        let (a, b) = (x.text().to_string(), y.text().to_string());
        match x.ty() {
            Some(LirTy::Float) => {
                let ia = self.b.val(&format!("(bitcast i32 {a})"), LirTy::I32);
                let ib = self.b.val(&format!("(bitcast i32 {b})"), LirTy::I32);
                self.b
                    .val(&format!("(icmp eq {} {})", ia.text(), ib.text()), LirTy::I1)
            }
            Some(LirTy::Double) => {
                let ia = self.b.val(&format!("(bitcast i64 {a})"), LirTy::I64);
                let ib = self.b.val(&format!("(bitcast i64 {b})"), LirTy::I64);
                self.b
                    .val(&format!("(icmp eq {} {})", ia.text(), ib.text()), LirTy::I1)
            }
            Some(LirTy::Dyn) => {
                let oa = self.b.val(&format!("(extractvalue {a} 0)"), LirTy::Ptr);
                let ob = self.b.val(&format!("(extractvalue {b} 0)"), LirTy::Ptr);
                self.b
                    .val(&format!("(icmp eq {} {})", oa.text(), ob.text()), LirTy::I1)
            }
            _ => self.b.val(&format!("(icmp eq {a} {b})"), LirTy::I1),
        }
    }

    /// `(set! p v)`: the value consumed (its retain is in the plan),
    /// stored, the old content released.
    pub fn set_place(&mut self, pl: &'a Place, v: &'a Expr) -> R<V> {
        let (c, t) = self.place(pl)?;
        let new = self.value(v)?;
        let content = match &t {
            Ty::Con(Con::Cell, args) => args[0].clone(),
            Ty::Con(Con::Atom, _) => return Err(Unsupported("atoms".into())),
            _ => return Err(Unsupported("set! of a non-cell".into())),
        };
        let (_, sname) = self.p.object(&t)?;
        let old = self.cell_load(&sname, c.text(), &content)?;
        let field = self.gep(&sname, c.text(), CELL_VALUE);
        self.store(&new, &field);
        self.release(&old);
        Ok(V::Unit)
    }

    /// `(cell e)` or `(atom e)`: the slot object holding the consumed
    /// value.
    pub fn new_slot(&mut self, e: &Expr, v: &V, atom: bool) -> R<V> {
        let t = self.ty(e)?;
        let (tid, sname) = self.p.object(&t)?;
        let size = self.p.objects.get(tid).size();
        let p = self.alloc_at(e.id, tid, &sname, size)?;
        let field = if atom {
            let lock = self.gep(&sname, &p, CELL_VALUE);
            self.b.stmt(&format!("(store (i32 0) {lock})"));
            self.gep(&sname, &p, ATOM_VALUE)
        } else {
            self.gep(&sname, &p, CELL_VALUE)
        };
        self.store(v, &field);
        Ok(V::Val(p, LirTy::Ptr))
    }

    /// `(set-field! &s f v)`: a unique write when `fib.unique?` holds,
    /// else a changed copy stored into the cell (§8.3, §6.6).
    pub fn set_field(&mut self, b: BindingId, f: &str, v: &'a Expr) -> R<V> {
        let cell = self.local(b)?;
        let cell_ty = self.binding_ty(b)?;
        let x = self.value(v)?;
        let st = match &cell_ty {
            Ty::Con(Con::Cell, args) => args[0].clone(),
            _ => return Err(Unsupported("set-field! on a non-cell".into())),
        };
        let (id, args) = match &st {
            Ty::Con(Con::Nominal(id), args) => (*id, args.clone()),
            _ => return Err(Unsupported("set-field! on a non-struct".into())),
        };
        let g = self.p.g();
        let Shape::Struct(fs) = &g.ty(id).shape else {
            return Err(Unsupported("set-field! on an enum".into()));
        };
        let index = fs
            .iter()
            .position(|d| d.name == f)
            .ok_or_else(|| Unsupported(format!("no field {f}")))?;
        let tys = variants(g, id, &args)?.remove(0).1;
        let mut slot = STRUCT_FIELD0;
        for ft in &tys[..index] {
            if self.p.lir(ft)?.is_some() {
                slot += 1;
            }
        }
        let (_, cell_s) = self.p.object(&cell_ty)?;
        let (tid, sname) = self.p.object(&st)?;
        let size = self.p.objects.get(tid).size();
        let content = self.cell_load(&cell_s, cell.text(), &st)?;
        self.unique_write(
            &cell_s,
            cell.text(),
            &content,
            &sname,
            tid,
            size,
            slot,
            &x,
            &tys[index],
        )
    }

    /// The unique-write protocol shared by `set-field!` and
    /// `array-set!` on a fixed-layout object: in place when unique,
    /// else copy, write, store the copy, release the old.
    #[allow(clippy::too_many_arguments)]
    pub fn unique_write(
        &mut self,
        cell_s: &str,
        cell: &str,
        content: &V,
        sname: &str,
        tid: u32,
        size: u64,
        slot: usize,
        x: &V,
        elem_ty: &Ty,
    ) -> R<V> {
        let unique = self.b.val(
            &format!("(call @fib.unique? {})", content.text()),
            LirTy::I1,
        );
        let (li, lc, lj) = (
            self.b.label("inplace"),
            self.b.label("copy"),
            self.b.label("written"),
        );
        self.b.term(&format!("(br {} {li} {lc})", unique.text()));
        self.b.open(&li);
        let elt = self.p.lir(elem_ty)?;
        let field = self.gep(sname, content.text(), slot);
        if let Some(t) = elt {
            let old = self.load(t, &field);
            self.store(x, &field);
            self.release(&old);
        }
        self.b.term(&format!("(br {lj})"));
        self.b.open(&lc);
        let copy = self.b.val(
            &format!(
                "(call @fib.copy-object {} (i64 {size}) (i32 {tid}))",
                content.text()
            ),
            LirTy::Ptr,
        );
        let nfields = self.p.objects.get(tid).size() as usize;
        let _ = nfields;
        self.b
            .stmt(&format!("(call @trace.{tid} {} @fib.retain)", copy.text()));
        let cfield = self.gep(sname, copy.text(), slot);
        if let Some(t) = elt {
            let old = self.load(t, &cfield);
            self.store(x, &cfield);
            self.release(&old);
        }
        let cellf = self.gep(cell_s, cell, CELL_VALUE);
        self.store(&copy, &cellf);
        self.release(content);
        self.b.term(&format!("(br {lj})"));
        self.b.open(&lj);
        Ok(V::Unit)
    }
}
