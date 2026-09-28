//! Methods and builtins as function values (types §8.4, eval/closure.rs
//! `Clo::Impl`, `Clo::Builtin`): an immortal closure whose code is the
//! method's all-owned body, or a generated wrapper that runs the
//! native operation under the closure convention and releases the
//! arguments it does not own.

use std::sync::OnceLock;

use fibref::own::program::{BodyKey, BodyOwn};
use fibref::types::ast::{BuiltinId, Expr};
use fibref::types::builtins::Escape;
use fibref::types::infer::Resolution;
use fibref::types::ty::{ProtoId, Ty};

use super::{Cx, Local, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::names::mangle;

/// The plan of a generated wrapper: it has no expressions of its own.
fn empty_plan() -> &'static BodyOwn {
    static EMPTY: OnceLock<BodyOwn> = OnceLock::new();
    EMPTY.get_or_init(BodyOwn::default)
}

impl<'a> Cx<'_, 'a> {
    /// A protocol method as a value: its implementation at the
    /// resolved instance, all-owned.
    pub fn method_value(&mut self, e: &Expr, _p: ProtoId, i: usize) -> R<V> {
        let (index, args) = match self.p.c.typed.resolutions.get(&e.id) {
            Some(Resolution::Instance { index, args }) => (
                *index,
                args.iter().map(|t| self.inst.subst(t)).collect::<Vec<_>>(),
            ),
            _ => {
                return Err(Unsupported(
                    "a method value without a resolved instance".into(),
                ))
            }
        };
        let g = self.p.g();
        let inst = &g.instances[index];
        let proto = g.proto(inst.proto);
        let code = if inst.methods.is_empty() {
            let (pname, mname) = (proto.name.clone(), proto.methods[i].name.clone());
            let owned: Vec<bool> = proto.methods[i].params.iter().map(|q| q.owned).collect();
            let name = format!("l.native.{pname}.{mname}.{}", mangle(g, &self.ty(e)?));
            self.wrapper(e, &name, move |cx, vals, tys, rt| {
                let v = cx.native_method(&pname, &mname, vals, tys, rt)?;
                for (j, a) in vals.iter().enumerate() {
                    if !owned.get(j).copied().unwrap_or(false) {
                        cx.release(a);
                    }
                }
                Ok(v)
            })?
        } else {
            let m = inst
                .methods
                .iter()
                .position(|im| im.index == i)
                .ok_or_else(|| Unsupported("an instance without the method".into()))?;
            self.p.request(BodyKey::MethodOwned(index, m), args)
        };
        let (tid, _) = self.p.closure_object(&code, Vec::new());
        Ok(V::Val(self.p.statics.closure(&code, tid), LirTy::Ptr))
    }

    /// A builtin as a value: a wrapper running it, then releasing its
    /// borrowed arguments (eval/native.rs `release_borrowed`).
    pub fn builtin_value(&mut self, e: &Expr, b: BuiltinId) -> R<V> {
        let sig = self.p.c.typed.builtins[b.0 as usize];
        if matches!(
            sig.name,
            "cell"
                | "atom"
                | "array"
                | "array-with"
                | "array-copy"
                | "str-bytes"
                | "weak"
                | "spawn"
                | "join"
                | "trap"
                | "concat"
        ) {
            return Err(Unsupported(format!("{} as a value", sig.name)));
        }
        let name = format!(
            "l.builtin.{}.{}",
            sig.name,
            mangle(self.p.g(), &self.ty(e)?)
        );
        let code = self.wrapper(e, &name, move |cx, vals, tys, _| {
            let v = cx
                .builtin(sig.name, e, vals, tys)?
                .ok_or_else(|| Unsupported("a builtin wrapper that does not return".into()))?;
            for (esc, a) in sig.escapes.iter().zip(vals) {
                if matches!(esc, Escape::Borrow | Escape::Weak | Escape::Raw) {
                    cx.release(a);
                }
            }
            Ok(v)
        })?;
        let (tid, _) = self.p.closure_object(&code, Vec::new());
        Ok(V::Val(self.p.statics.closure(&code, tid), LirTy::Ptr))
    }

    /// Emits (once) the closure-convention function `name` whose
    /// parameters and result are those of the function type of `e`,
    /// with `body` computing the result from the parameters.
    fn wrapper(
        &mut self,
        e: &Expr,
        name: &str,
        body: impl FnOnce(&mut Cx<'_, 'a>, &[V], &[Ty], &Ty) -> R<V>,
    ) -> R<String> {
        if self.p.has_helper(name) {
            return Ok(name.to_string());
        }
        let Ty::Fn(_, params, ret) = self.ty(e)? else {
            return Err(Unsupported(
                "a function value without a function type".into(),
            ));
        };
        let mut sig = vec![(LirTy::Ptr, "env".to_string())];
        let mut vals = Vec::new();
        for (i, t) in params.iter().enumerate() {
            match self.p.lir(t)? {
                Some(l) => {
                    let pn = format!("p{i}");
                    sig.push((l, pn.clone()));
                    vals.push(V::Val(pn, l));
                }
                None => vals.push(V::Unit),
            }
        }
        let inst = self.inst.clone();
        let mut cx = Cx::new(&mut *self.p, inst, empty_plan(), name, sig);
        cx.b.ret = cx.p.lir(&ret)?;
        cx.env = Some(V::Val("env".into(), LirTy::Ptr));
        cx.locals
            .insert(fibref::types::ast::BindingId(u32::MAX), Local::Val(V::Unit));
        let v = body(&mut cx, &vals, &params, &ret)?;
        match &v {
            V::Unit => cx.b.term("(ret)"),
            V::Val(s, _) => cx.b.term(&format!("(ret {s})")),
        }
        let text = cx.b.render();
        self.p.add_helper(name, text);
        Ok(name.to_string())
    }
}
