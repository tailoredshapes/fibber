//! `Form` values (syntax §3.16): the built-in enum, as heap objects.
//! A quoted literal is an immortal graph made once (§3.16, types §8.3);
//! macro arguments are built the same way, and a macro's result is read
//! back into a reader [`Form`]. Also `concat`, `gensym` and the
//! expansion-time reflection calls.

use crate::syntax::{FltWidth, Form, FormKind, IntWidth, Pos};
use crate::types::ast::Expr;
use crate::types::decls::Shape;
use crate::types::ty::{Scalar, TypeId};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

impl<'p> Interp<'p> {
    fn form_type(&self) -> R<TypeId> {
        self.p
            .globals
            .form
            .ok_or_else(|| RunError::internal("Form is not declared"))
    }

    fn form_variant(&self, name: &str) -> R<usize> {
        let t = self.form_type()?;
        match &self.p.globals.ty(t).shape {
            Shape::Enum(vs) => vs.iter().position(|v| v.name == name),
            Shape::Struct(_) => None,
        }
        .ok_or_else(|| RunError::internal(format!("Form has no variant {name}")))
    }

    /// `'f`: the immortal graph of `f`, made once per literal.
    pub fn quote(&mut self, e: &Expr, f: &Form) -> R<Val> {
        if let Some(id) = self.statics.lits.get(&e.id) {
            return Ok(Val::Obj(*id));
        }
        let v = self.form_value(f, Placement::Immortal)?;
        let id = v.expect_obj("a quoted form")?;
        self.statics.lits.insert(e.id, id);
        Ok(v)
    }

    /// The `Form` value of `f`, every object in it placed at `at`; with
    /// heap placement the caller owns one count on the result.
    pub fn form_value(&mut self, f: &Form, at: Placement) -> R<Val> {
        let t = self.form_type()?;
        let (name, fields) = match &f.kind {
            FormKind::Sym(s) => ("Sym", vec![self.new_str(s.clone(), at)?]),
            FormKind::Kw(k) => ("Kw", vec![self.new_str(k.clone(), at)?]),
            FormKind::Int { v, width } => {
                let w = Val::Kw(self.keyword(width.suffix()));
                ("Int", vec![Val::Int(*v, Scalar::I64), w])
            }
            FormKind::Flt { v, width } => {
                let suffix = if *width == FltWidth::F32 {
                    "f32"
                } else {
                    "f64"
                };
                let w = Val::Kw(self.keyword(suffix));
                ("Flt", vec![Val::Float(*v, Scalar::F64), w])
            }
            FormKind::Str(s) => ("Str", vec![self.new_str(s.clone(), at)?]),
            FormKind::Chr(c) => ("Chr", vec![Val::Char(*c)]),
            FormKind::Bool(b) => ("Bool", vec![Val::Bool(*b)]),
            FormKind::Nil => ("Nil", Vec::new()),
            FormKind::List(items) => ("List", vec![self.items_value(items, at)?]),
            FormKind::Vec(items) => ("Vec", vec![self.items_value(items, at)?]),
            FormKind::Map(items) => ("Map", vec![self.items_value(items, at)?]),
        };
        let variant = self.form_variant(name)?;
        let v = self.node(t, variant, fields, at)?;
        if let (true, Some(id)) = (self.recording_inputs, v.obj()) {
            self.input_forms.insert(id, f.clone());
        }
        Ok(v)
    }

    fn items_value(&mut self, items: &[Form], at: Placement) -> R<Val> {
        let mut vals = Vec::with_capacity(items.len());
        for f in items {
            vals.push(self.form_value(f, at)?);
        }
        let v = self.build_vec(&vals, at)?;
        if at == Placement::Heap {
            self.give_back(&vals)?;
        }
        Ok(v)
    }

    /// The reader form a `Form` value denotes: an input form with its own
    /// positions, or a new one at `pos`.
    pub fn value_form(&mut self, v: &Val, pos: &Pos) -> R<Form> {
        let id = v.expect_obj("a Form")?;
        // The side table of input forms is object content too (rule 2).
        self.heap.check_access(id, crate::heap::Op::Read)?;
        if let Some(f) = self.input_forms.get(&id) {
            return Ok(f.clone());
        }
        let tag = match self.objs.get(&self.heap, id)? {
            Obj::Variant { tag, .. } => *tag as usize,
            o => return Err(RunError::internal(format!("not a Form: {o:?}"))),
        };
        let t = self.form_type()?;
        let name = match &self.p.globals.ty(t).shape {
            Shape::Enum(vs) => vs.get(tag).map(|v| v.name.clone()).unwrap_or_default(),
            Shape::Struct(_) => String::new(),
        };
        let kind = self.form_kind(id, &name, pos)?;
        Ok(Form::new(kind, pos.clone()))
    }

    fn form_kind(&mut self, id: crate::heap::ObjId, name: &str, pos: &Pos) -> R<FormKind> {
        let text = |me: &mut Self| -> R<String> {
            let f = me.field(id, 0)?;
            Ok(me.string(&f)?.to_string())
        };
        Ok(match name {
            "Sym" => FormKind::Sym(text(self)?),
            "Kw" => FormKind::Kw(text(self)?),
            "Str" => FormKind::Str(text(self)?),
            "Int" => {
                let v = self.field(id, 0)?.as_int()?;
                let w = self.field(id, 1)?;
                let w = self.keyword_text(&w)?;
                let width = IntWidth::from_suffix(&w)
                    .ok_or_else(|| RunError::trap(format!("an Int form of width :{w}")))?;
                FormKind::Int { v, width }
            }
            "Flt" => {
                let Val::Float(v, _) = self.field(id, 0)? else {
                    return Err(RunError::internal("a Flt form without a float"));
                };
                let w = self.field(id, 1)?;
                let w = self.keyword_text(&w)?;
                let width = FltWidth::from_suffix(&w)
                    .ok_or_else(|| RunError::trap(format!("a Flt form of width :{w}")))?;
                FormKind::Flt { v, width }
            }
            "Chr" => match self.field(id, 0)? {
                Val::Char(c) => FormKind::Chr(c),
                v => return Err(RunError::internal(format!("a Chr form holding {v:?}"))),
            },
            "Bool" => FormKind::Bool(self.field(id, 0)?.as_bool()?),
            "Nil" => FormKind::Nil,
            list => self.list_kind(id, list, pos)?,
        })
    }

    /// A `List`, `Vec` or `Map` form's items, read back.
    fn list_kind(&mut self, id: crate::heap::ObjId, list: &str, pos: &Pos) -> R<FormKind> {
        let items = self.field(id, 0)?;
        let mut forms = Vec::new();
        for item in self.read_vec(&items)? {
            forms.push(self.value_form(&item, pos)?);
        }
        Ok(match list {
            "List" => FormKind::List(forms),
            "Vec" => FormKind::Vec(forms),
            _ => FormKind::Map(forms),
        })
    }

    fn keyword_text(&self, v: &Val) -> R<String> {
        match v {
            Val::Kw(k) => Ok(self.keyword_name(*k)?.to_string()),
            v => Err(RunError::internal(format!("not a keyword: {v:?}"))),
        }
    }

    /// The `concat` form (§3.16): each operand, then a new vector.
    pub fn concat(&mut self, es: &'p [Expr]) -> R<Val> {
        let mut parts = Vec::with_capacity(es.len());
        for x in es {
            parts.push(self.val(x)?);
        }
        self.concat_vals(&parts)
    }

    /// `gensym` and the reflection calls of §3.16.
    pub fn expansion_builtin(&mut self, name: &str, arg: &Val, pos: &Pos) -> R<Val> {
        if name == "gensym" {
            let prefix = self.string(arg)?.to_string();
            let sym = match self.ctx {
                Some(ctx) => ctx.gensym(&prefix, pos),
                None => {
                    self.gensyms += 1;
                    let text = format!("#{prefix}.{}", self.gensyms);
                    Form::new(FormKind::Sym(text), pos.clone())
                }
            };
            return self.form_value(&sym, Placement::Heap);
        }
        let ctx = self.ctx.ok_or_else(|| {
            RunError::unsupported(format!("{name} is available only at expansion time"))
        })?;
        let f = self.value_form(arg, pos)?;
        let out = ctx.reflect(name, &f, pos).map_err(|e| {
            let msg = e.to_string();
            self.expand_error = Some(e);
            RunError::trap(msg)
        })?;
        self.reflection_value(&out)
    }

    /// The value of a reflection call's answer at the type §3.16 gives
    /// it: `struct?` and `enum?` a `bool`, the others a `(Vec Form)`
    /// (the caller owns one count on it), never the `Form` that
    /// `ExpandCtx::reflect` answers with.
    fn reflection_value(&mut self, out: &Form) -> R<Val> {
        match &out.kind {
            FormKind::Bool(b) => Ok(Val::Bool(*b)),
            FormKind::Vec(items) => self.items_value(items, Placement::Heap),
            k => Err(RunError::internal(format!("a reflection answer {k:?}"))),
        }
    }
}
