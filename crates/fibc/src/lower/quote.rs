//! Quoted forms (types §8.3, syntax §3.16): a `(quote f)` is an
//! `IMMORTAL` graph of `Form`, `Vec`, `Array` and `str` constants
//! referring to each other by address, laid out as the interpreter's
//! `form_value` builds it (eval/forms.rs), and `(concat v..)` builds
//! a fresh vector of the parts' elements through the runtime.

use std::fmt::Write;

use fibref::syntax::{FltWidth, Form, FormKind};
use fibref::types::decls::Shape;
use fibref::types::ty::{Con, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};
use crate::layout::IMMORTAL;

impl<'a> Cx<'_, 'a> {
    /// `(quote f)`: the address of its immortal graph.
    pub fn quote(&mut self, e: &fibref::types::ast::Expr, f: &Form) -> R<V> {
        if let Some(name) = self.p.quotes.get(&e.id) {
            return Ok(V::Val(name.clone(), LirTy::Ptr));
        }
        let name = self.form_constant(f)?;
        self.p.quotes.insert(e.id, name.clone());
        Ok(V::Val(name, LirTy::Ptr))
    }

    /// The constant of one form.
    fn form_constant(&mut self, f: &Form) -> R<String> {
        let g = self.p.g();
        let form_id = g
            .form
            .ok_or_else(|| Unsupported("the prelude defines no Form".into()))?;
        let form_ty = Ty::nominal(form_id, Vec::new());
        let (tid, sname) = self.p.object(&form_ty)?;
        let g = self.p.g();
        let Shape::Enum(vs) = &g.ty(form_id).shape else {
            return Err(Unsupported("Form is not an enum".into()));
        };
        let (vname, fields): (&str, Vec<String>) = match &f.kind {
            FormKind::Sym(s) => ("Sym", vec![self.p.statics.string(s, 0)]),
            FormKind::Kw(k) => ("Kw", vec![self.p.statics.string(k, 0)]),
            FormKind::Int { v, width } => {
                let w = self.p.statics.keyword(width.suffix());
                ("Int", vec![format!("(i64 {v})"), format!("(i64 {w})")])
            }
            FormKind::Flt { v, width } => {
                let suffix = if *width == FltWidth::F32 {
                    "f32"
                } else {
                    "f64"
                };
                let w = self.p.statics.keyword(suffix);
                (
                    "Flt",
                    vec![
                        format!("(double {})", super::float_text(*v)),
                        format!("(i64 {w})"),
                    ],
                )
            }
            FormKind::Str(s) => ("Str", vec![self.p.statics.string(s, 0)]),
            FormKind::Chr(c) => ("Chr", vec![format!("(i32 {})", u32::from(*c))]),
            FormKind::Bool(b) => ("Bool", vec![format!("(i1 {})", i32::from(*b))]),
            FormKind::Nil => ("Nil", vec![]),
            FormKind::List(items) => ("List", vec![self.forms_vec(items, &form_ty)?]),
            FormKind::Vec(items) => ("Vec", vec![self.forms_vec(items, &form_ty)?]),
            FormKind::Map(items) => ("Map", vec![self.forms_vec(items, &form_ty)?]),
        };
        let tag = vs
            .iter()
            .position(|v| v.name == vname)
            .ok_or_else(|| Unsupported(format!("Form has no variant {vname}")))?;
        let name = self.p.fresh_quote();
        let vsname = format!("{sname}.v{tag}");
        let _ = writeln!(
            self.p.quote_text,
            "(constant internal {} %struct.{vsname} (%struct.{vsname} (i64 0) (i32 {tid}) (i32 {IMMORTAL}) (i32 {tag}) {}))",
            &name[1..],
            fields.join(" ")
        );
        Ok(name)
    }

    /// An immortal `(Vec Form)` of up to 32 items: `VecEmpty`, or a
    /// `VecOf` whose tail array holds them all (the layout `build_vec`
    /// makes for so few, eval/vecs.rs).
    fn forms_vec(&mut self, items: &[Form], form_ty: &Ty) -> R<String> {
        let g = self.p.g();
        let vec_id = g
            .vec
            .ok_or_else(|| Unsupported("the prelude defines no Vec".into()))?;
        let vec_ty = Ty::nominal(vec_id, vec![form_ty.clone()]);
        let (vtid, vsname) = self.p.object(&vec_ty)?;
        let name = self.p.fresh_quote();
        if items.is_empty() {
            let _ = writeln!(
                self.p.quote_text,
                "(constant internal {} %struct.{vsname}.v0 (%struct.{vsname}.v0 (i64 0) (i32 {vtid}) (i32 {IMMORTAL}) (i32 0)))",
                &name[1..]
            );
            return Ok(name);
        }
        if items.len() > 32 {
            return Err(Unsupported("a quoted vector of more than 32 items".into()));
        }
        let mut elems = Vec::new();
        for it in items {
            elems.push(self.form_constant(it)?);
        }
        let (atid, _) = self.p.object(&Ty::Con(Con::Array, vec![form_ty.clone()]))?;
        let arr = self.p.fresh_quote();
        let n = elems.len();
        let _ = writeln!(
            self.p.quote_text,
            "(constant internal {} {{ i64 i32 i32 i64 [{n} x ptr] }} {{ (i64 0) (i32 {atid}) (i32 {IMMORTAL}) (i64 {n}) ([{n} x ptr] {}) }})",
            &arr[1..],
            elems.join(" ")
        );
        let _ = writeln!(
            self.p.quote_text,
            "(constant internal {} %struct.{vsname}.v1 (%struct.{vsname}.v1 (i64 0) (i32 {vtid}) (i32 {IMMORTAL}) (i32 1) (i64 {n}) (i64 5) (ptr null) {arr}))",
            &name[1..]
        );
        Ok(name)
    }

    /// `(concat v..)`: a fresh vector of every part's elements, built
    /// as `build_vec` builds it. `e` is the whole call, whose type is
    /// the vector's even when there are no parts (`` `() `` is `(List
    /// (concat))`).
    pub fn concat(
        &mut self,
        e: &fibref::types::ast::Expr,
        parts: &'a [fibref::types::ast::Expr],
    ) -> R<V> {
        let t = self.ty(e)?;
        let elem = match &t {
            Ty::Con(Con::Nominal(_), args) => args
                .first()
                .cloned()
                .ok_or_else(|| Unsupported("concat of a non-Vec".into()))?,
            _ => return Err(Unsupported("concat of a non-Vec".into())),
        };
        let mut vals = Vec::new();
        for p in parts {
            vals.push(self.value(p)?);
        }
        let ids = self.vec_tids(&t, &elem)?;
        let n = self.b.val("(i64 0)", LirTy::I64);
        let mut total = n;
        for v in &vals {
            let len = self
                .b
                .val(&format!("(call @fib.vec-len {})", v.text()), LirTy::I64);
            total = self.b.val(
                &format!("(add {} {})", total.text(), len.text()),
                LirTy::I64,
            );
        }
        let scratch = self.b.val(
            &format!("(call @malloc (mul {} (i64 {})))", total.text(), ids.esize),
            LirTy::Ptr,
        );
        let mut at = V::int(LirTy::I64, 0);
        for v in &vals {
            at = self.b.val(
                &format!(
                    "(call @fib.vec-gather {} {} {} (i64 {}))",
                    v.text(),
                    scratch.text(),
                    at.text(),
                    ids.esize
                ),
                LirTy::I64,
            );
        }
        let r = self.b.val(
            &format!(
                "(call @fib.vec-build {} {} (i64 {}) (i8 {}) (i32 {}) (i32 {}) (i32 {}) (i32 {}))",
                scratch.text(),
                total.text(),
                ids.esize,
                ids.counted,
                ids.tvec,
                ids.tnode,
                ids.tarr,
                ids.tnarr
            ),
            LirTy::Ptr,
        );
        self.b.stmt(&format!("(call @free {})", scratch.text()));
        Ok(r)
    }
}
