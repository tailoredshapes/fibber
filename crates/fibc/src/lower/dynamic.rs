//! Dynamic dispatch (types §4.4, §8.5): `(dyn P e)` builds the
//! two-word value `{ obj, vtable }`; a method call through it loads
//! the slot and calls indirectly with `obj` as `self`; a supertrait's
//! method goes through the supertrait's vtable slot.

use fibref::own::program::BodyKey;
use fibref::types::ast::Expr;
use fibref::types::ty::{Con, Pred, ProtoId, Ty};

use super::call::Target;
use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::names::mangle;

impl<'a> Cx<'_, 'a> {
    /// `(dyn P e)`: the value with the vtable of `(P, head type of e)`,
    /// or an upcast of a `dyn` value through its supertrait slot.
    pub fn dyn_of(&mut self, p: ProtoId, b: &'a Expr) -> R<V> {
        let v = self.value(b)?;
        let t = self.ty(b)?;
        if let Ty::Con(Con::Dyn(q, _), _) = &t {
            if *q == p {
                return Ok(v);
            }
            let obj = self
                .b
                .val(&format!("(extractvalue {} 0)", v.text()), LirTy::Ptr);
            let vt = self.super_vtable(&v, *q, p)?;
            return Ok(self
                .b
                .val(&format!("{{ {} {} }}", obj.text(), vt.text()), LirTy::Dyn));
        }
        let vt = self.vtable_for(p, &t)?;
        Ok(self.b.val(&format!("{{ {} {vt} }}", v.text()), LirTy::Dyn))
    }

    /// The vtable global of `(p, head)`: one slot per method of `p`,
    /// then one per supertrait in the order of the transitive closure.
    fn vtable_for(&mut self, p: ProtoId, head: &Ty) -> R<String> {
        let g = self.p.g();
        let name = format!("{}.vt.{}", g.proto(p).name, mangle(g, head));
        if self.p.statics.has_vtable(&name) {
            return Ok(self.p.statics.vtable(&name, Vec::new()));
        }
        let pred = Pred::Proto(p, vec![head.clone()]);
        let (index, args) = self.instance_of_pred(&pred)?;
        let inst = &g.instances[index];
        let proto = g.proto(p);
        if inst.methods.is_empty() {
            return Err(Unsupported("a dyn of a native instance".into()));
        }
        let mut slots = Vec::new();
        for i in 0..proto.methods.len() {
            let m = inst
                .methods
                .iter()
                .position(|im| im.index == i)
                .ok_or_else(|| Unsupported("an instance missing a method".into()))?;
            let f = self.p.request(BodyKey::Method(index, m), args.clone());
            slots.push(format!("@{f}"));
        }
        for sup in super_closure(g, &pred) {
            let Pred::Proto(q, _) = sup else {
                continue;
            };
            slots.push(self.vtable_for(q, head)?);
        }
        Ok(self.p.statics.vtable(&name, slots))
    }

    /// Supertrait `q`'s vtable, loaded from a `(dyn p)` value.
    fn super_vtable(&mut self, v: &V, p: ProtoId, q: ProtoId) -> R<V> {
        let g = self.p.g();
        let n = g.proto(p).methods.len();
        let pos = super_closure(g, &Pred::Proto(p, vec![Ty::unit()]))
            .iter()
            .position(|s| matches!(s, Pred::Proto(r, _) if *r == q))
            .ok_or_else(|| {
                Unsupported(format!(
                    "{} is not a supertrait of {}",
                    g.proto(q).name,
                    g.proto(p).name
                ))
            })?;
        let vt = self
            .b
            .val(&format!("(extractvalue {} 1)", v.text()), LirTy::Ptr);
        let slot = self.b.val(
            &format!("(getelementptr ptr {} (i64 {}))", vt.text(), n + pos),
            LirTy::Ptr,
        );
        Ok(self.load(LirTy::Ptr, slot.text()))
    }

    /// The target of method `i` of protocol `q` called on the `dyn`
    /// receiver `recv` of type `t`.
    pub fn dyn_target(&mut self, recv: &V, t: &Ty, q: ProtoId, i: usize) -> R<Target> {
        let Ty::Con(Con::Dyn(p, _), _) = t else {
            return Err(Unsupported("a dyn call on a non-dyn receiver".into()));
        };
        let p = *p;
        let obj = self
            .b
            .val(&format!("(extractvalue {} 0)", recv.text()), LirTy::Ptr);
        let vt = if p == q {
            self.b
                .val(&format!("(extractvalue {} 1)", recv.text()), LirTy::Ptr)
        } else {
            self.super_vtable(recv, p, q)?
        };
        let slot = self.b.val(
            &format!("(getelementptr ptr {} (i64 {i}))", vt.text()),
            LirTy::Ptr,
        );
        let code = self.load(LirTy::Ptr, slot.text());
        Ok(Target::Code {
            code: code.text().to_string(),
            this: obj.text().to_string(),
        })
    }
}

/// Every supertrait constraint `p` entails, transitively, depth first,
/// each protocol once (§4.1 rule 2; `fibref` `types/lower/supers.rs`).
pub fn super_closure(g: &fibref::types::decls::Globals, p: &Pred) -> Vec<Pred> {
    let mut out: Vec<Pred> = Vec::new();
    let mut work = vec![p.clone()];
    while let Some(Pred::Proto(q, args)) = work.pop() {
        for s in &g.proto(q).supers {
            let inst = s.map_tys(&mut |t| t.subst_gen(&args, &[]));
            if !out.contains(&inst) {
                out.push(inst.clone());
                work.push(inst);
            }
        }
    }
    out
}
