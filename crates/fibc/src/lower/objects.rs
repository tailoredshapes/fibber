//! Objects (types §8.3, §8.4): constructors on the heap or the stack,
//! field access, `Option`, closures and named-function values.

use fibref::own::program::{Alloc, ClosureOwn, Pass};
use fibref::types::ast::{Expr, ExprId, ExprKind, GlobalRef};
use fibref::types::decls::Shape;
use fibref::types::ty::{Con, Ty, TypeId};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::layout::{option_payload, option_rep, variants, OptRep};
use crate::mono::fun_key;
use crate::objects::{CAPTURE0, CLOSURE_CODE, ENUM_TAG, STRUCT_FIELD0, VARIANT_FIELD0};

impl<'a> Cx<'_, 'a> {
    /// The object of allocation site `e`: on the heap through
    /// `fib.alloc`, or a `STACK` object in the site's entry-block slot
    /// (§8.2), as the plan decided.
    pub fn alloc_at(&mut self, e: ExprId, tid: u32, sname: &str, size: u64) -> R<String> {
        match self.own.allocs.get(&e).copied() {
            Some(Alloc::Stack) => {
                let slot = match self.stack_slots.get(&e) {
                    Some(s) => s.clone(),
                    None => {
                        let s = self.b.entry_alloca(&format!("%struct.{sname}"), size);
                        self.stack_slots.insert(e, s.clone());
                        s
                    }
                };
                self.b
                    .stmt(&format!("(call @fib.stack-init {slot} (i32 {tid}))"));
                Ok(slot)
            }
            Some(Alloc::Nothing) => Err(Unsupported(
                "an allocation the plan says allocates nothing".into(),
            )),
            Some(Alloc::Heap) | None => Ok(self.heap_alloc(tid, size)),
        }
    }

    /// A counted heap object of `size` bytes, count 1.
    pub fn heap_alloc(&mut self, tid: u32, size: u64) -> String {
        self.b
            .val(
                &format!("(call @fib.alloc (i64 {size}) (i32 {tid}))"),
                LirTy::Ptr,
            )
            .text()
            .to_string()
    }

    /// A constructor call: the object with its fields stored.
    pub fn construct(&mut self, e: &Expr, id: TypeId, variant: Option<usize>, vals: &[V]) -> R<V> {
        let t = self.ty(e)?;
        let g = self.p.g();
        if id == g.option {
            return self.construct_option(e, variant, vals, &t);
        }
        let def = g.ty(id);
        if def.is_fieldless_enum() {
            return Ok(V::int(LirTy::I32, variant.unwrap_or(0) as i64));
        }
        let (tid, sname) = self.p.object(&t)?;
        let size = self.p.objects.get(tid).size();
        let (sname, base) = match (&def.shape, variant) {
            (Shape::Struct(_), _) => (sname, STRUCT_FIELD0),
            (Shape::Enum(_), Some(i)) => (format!("{sname}.v{i}"), VARIANT_FIELD0),
            (Shape::Enum(_), None) => {
                return Err(Unsupported("an enum constructor without a variant".into()))
            }
        };
        let p = self.alloc_at(e.id, tid, &sname, size)?;
        if let Some(i) = variant {
            let tagp = self.gep(&sname, &p, ENUM_TAG);
            self.b.stmt(&format!("(store (i32 {i}) {tagp})"));
        }
        self.store_fields(&sname, &p, base, vals);
        Ok(V::Val(p, LirTy::Ptr))
    }

    /// Stores values into consecutive fields from `base`, skipping
    /// `unit` values.
    pub fn store_fields(&mut self, sname: &str, p: &str, base: usize, vals: &[V]) {
        let mut slot = base;
        for v in vals {
            if matches!(v, V::Unit) {
                continue;
            }
            let f = self.gep(sname, p, slot);
            self.store(v, &f);
            slot += 1;
        }
    }

    fn construct_option(&mut self, e: &Expr, variant: Option<usize>, vals: &[V], t: &Ty) -> R<V> {
        let g = self.p.g();
        let payload = option_payload(g, t)
            .cloned()
            .ok_or_else(|| Unsupported("an Option constructor at a non-Option type".into()))?;
        match (option_rep(g, &payload)?, variant) {
            (OptRep::Null, Some(0)) => Ok(V::null()),
            (OptRep::Null, _) => Ok(vals
                .first()
                .cloned()
                .ok_or_else(|| Unsupported("some without a payload".into()))?),
            // A `nil` whose site the plan gave no allocation (a generic
            // payload) is the null pointer, as the interpreter makes it
            // (eval/option.rs: unboxed when the plan does not decide);
            // patterns accept both forms (`pattern.rs`).
            (OptRep::Boxed, Some(0)) if !self.own.allocs.contains_key(&e.id) => Ok(V::null()),
            (OptRep::Boxed, v) => {
                let tag = v.unwrap_or(0);
                let (tid, sname) = self.p.object(t)?;
                let size = self.p.objects.get(tid).size();
                let vs = format!("{sname}.v{tag}");
                let p = self.alloc_at(e.id, tid, &vs, size)?;
                let tagp = self.gep(&vs, &p, ENUM_TAG);
                self.b.stmt(&format!("(store (i32 {tag}) {tagp})"));
                self.store_fields(&vs, &p, VARIANT_FIELD0, vals);
                Ok(V::Val(p, LirTy::Ptr))
            }
        }
    }

    /// `(. x f)`: a field load, no count operation (§6.2).
    pub fn field_of(&mut self, x: &'a Expr, name: &str) -> R<V> {
        let v = self.value(x)?;
        let t = self.ty(x)?;
        let (id, args) = match &t {
            Ty::Con(Con::Nominal(id), args) => (*id, args.clone()),
            _ => return Err(Unsupported("field access on a non-struct".into())),
        };
        let g = self.p.g();
        let Shape::Struct(fs) = &g.ty(id).shape else {
            return Err(Unsupported("field access on an enum".into()));
        };
        let index = fs
            .iter()
            .position(|f| f.name == name)
            .ok_or_else(|| Unsupported(format!("no field {name}")))?;
        let tys = variants(g, id, &args)?.remove(0).1;
        let (_, sname) = self.p.object(&t)?;
        let mut slot = STRUCT_FIELD0;
        for ft in &tys[..index] {
            if self.p.lir(ft)?.is_some() {
                slot += 1;
            }
        }
        Ok(match self.p.lir(&tys[index])? {
            Some(l) => {
                let ptr = self.gep(&sname, v.text(), slot);
                self.load(l, &ptr)
            }
            None => V::Unit,
        })
    }

    /// The lIR types of a closure's capture slots.
    pub fn capture_tys(&self, own: &ClosureOwn) -> R<Vec<LirTy>> {
        own.captures
            .iter()
            .map(|c| {
                let t = self.binding_ty(c.binding)?;
                self.p
                    .lir(&t)?
                    .ok_or_else(|| Unsupported("a unit capture".into()))
            })
            .collect()
    }

    /// A `fn` literal: its closure object (§8.4), heap or stack as the
    /// plan says, captures taken as their passes say.
    pub fn make_fn(&mut self, e: &Expr) -> R<V> {
        let own = self
            .own
            .closures
            .get(&e.id)
            .cloned()
            .ok_or_else(|| Unsupported("no plan for a fn literal".into()))?;
        if own.is_async {
            return Err(Unsupported("async".into()));
        }
        let caps = self.capture_tys(&own)?;
        let code = self
            .p
            .request_closure(&self.inst.clone(), &self.name.clone(), e.id);
        let (tid, sname) = self.p.closure_object(&code, caps);
        let size = self.p.objects.get(tid).size();
        let mut vals = Vec::new();
        for c in &own.captures {
            let v = self.local(c.binding)?;
            if c.pass == Pass::Retain {
                self.retain(&v);
            }
            vals.push(v);
        }
        let p = self.alloc_at(e.id, tid, &sname, size)?;
        if self.own.allocs.get(&e.id) == Some(&Alloc::Stack) {
            self.closure_slots.insert(p.clone());
        }
        let codep = self.gep(&sname, &p, CLOSURE_CODE);
        self.b.stmt(&format!("(store @{code} {codep})"));
        self.store_fields(&sname, &p, CAPTURE0, &vals);
        Ok(V::Val(p, LirTy::Ptr))
    }

    /// A global name as a value: a named function's immortal closure
    /// (§8.4), a field-less variant's index, a `def`.
    pub fn global(&mut self, e: &Expr, g: GlobalRef) -> R<V> {
        use fibref::own::program::BodyKey;
        match g {
            GlobalRef::Fun(f) => {
                let gl = self.p.g();
                let scheme = self
                    .p
                    .c
                    .typed
                    .fun_schemes
                    .get(f.0 as usize)
                    .and_then(|s| s.as_ref())
                    .ok_or_else(|| Unsupported("a function without a scheme".into()))?;
                let tys: Vec<Ty> = match self.p.c.typed.instantiations.get(&e.id) {
                    Some(i) => i.tys.iter().map(|t| self.inst.subst(t)).collect(),
                    None => Vec::new(),
                };
                let key = fun_key(gl, scheme, &tys)?;
                let code = self.p.request(BodyKey::AllOwned(f), key);
                let (tid, _) = self.p.closure_object(&code, Vec::new());
                Ok(V::Val(self.p.statics.closure(&code, tid), LirTy::Ptr))
            }
            GlobalRef::Ctor(t, v) => self.ctor_value(e, t, v),
            GlobalRef::Method(p, i) => self.method_value(e, p, i),
            GlobalRef::Builtin(b) => self.builtin_value(e, b),
            GlobalRef::Def(d) => match self.p.def_values.get(&d) {
                Some((text, Some(l))) => Ok(V::Val(text.clone(), *l)),
                Some((_, None)) => Ok(V::Unit),
                // Made at run time (inits.rs): a count-free load of its
                // slot, which the init functions fill before `main`.
                None => match self.p.def_slots.get(&d).cloned() {
                    Some((slot, Some(l))) => Ok(self.load(l, &format!("@{slot}"))),
                    Some((_, None)) => Ok(V::Unit),
                    None => Err(Unsupported(format!(
                        "def {} is read where it has no value: it is not a constant \
                         and no init function makes it here",
                        self.p.g().def(d).name
                    ))),
                },
            },
            GlobalRef::Extern(_) => Err(Unsupported("an extern as a value".into())),
        }
    }

    fn ctor_value(&mut self, e: &Expr, t: TypeId, v: Option<usize>) -> R<V> {
        let g = self.p.g();
        let def = g.ty(t);
        let fieldless = match (&def.shape, v) {
            (Shape::Enum(vs), Some(i)) => vs.get(i).is_some_and(|x| x.fields.is_empty()),
            _ => false,
        };
        if t == g.option && v == Some(0) {
            return self.construct(e, t, v, &[]);
        }
        match v {
            Some(i) if fieldless && def.is_fieldless_enum() => Ok(V::int(LirTy::I32, i as i64)),
            Some(_) if fieldless => self.construct(e, t, v, &[]),
            _ => Err(Unsupported("a constructor as a value".into())),
        }
    }

    /// The expression a literal id names, for closure bodies.
    pub fn literal_expr(&self, lit: ExprId) -> R<&'a Expr> {
        self.lits
            .get(&lit)
            .copied()
            .ok_or_else(|| Unsupported(format!("no literal {lit:?}")))
    }

    /// Whether an expression is a `fn` literal.
    pub fn is_fn(e: &Expr) -> bool {
        matches!(e.kind, ExprKind::Fn(_))
    }
}
