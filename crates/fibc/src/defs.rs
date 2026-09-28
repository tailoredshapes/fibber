//! `def` values as static data (types §8.10, syntax §3.19): each
//! initialiser is evaluated at compile time and the resulting graph is
//! emitted as `IMMORTAL` constants referring to each other by address.
//! The evaluation runs in the reference interpreter (compiler.md §8,
//! question 4: §8.10 asks for the JIT that runs macros; the
//! interpreter is the executable spec of the same values).

use std::collections::HashMap;
use std::fmt::Write;

use fibref::eval::object::{Clo, Obj};
use fibref::eval::pipeline::with_threads;
use fibref::eval::{Interp, Val};
use fibref::own::program::BodyKey;
use fibref::types::ast::DefId;
use fibref::types::infer::UnitRef;
use fibref::types::ty::{Con, Ty};
use fibref::ObjId;

use crate::compile::Unsupported;
use crate::ir::LirTy;
use crate::layout::{option_payload, option_rep, variants, OptRep, IMMORTAL};
use crate::objects::{ObjKind, STRUCT_FIELD0};
use crate::program::Program;

/// The constants of every `def`, and each `def`'s value as lIR text.
#[derive(Debug, Default)]
pub struct Defs {
    pub text: String,
    pub values: HashMap<DefId, (String, Option<LirTy>)>,
    names: HashMap<ObjId, String>,
    counter: usize,
}

/// Evaluates every `def` of the user module and serialises its value.
pub fn emit_defs(p: &mut Program<'_>) -> Result<Defs, Unsupported> {
    let c = p.c;
    let mut defs = Defs::default();
    let ids: Vec<DefId> = c
        .typed
        .units
        .iter()
        .filter_map(|u| match u {
            UnitRef::Def(d) => Some(*d),
            _ => None,
        })
        .collect();
    if ids.is_empty() {
        return Ok(defs);
    }
    with_threads(&c.typed, &c.owned, None, |it| -> Result<(), Unsupported> {
        it.eval_defs()
            .map_err(|e| Unsupported(format!("a def does not evaluate: {e}")))?;
        for d in ids {
            let v = it.defs[d.0 as usize]
                .clone()
                .ok_or_else(|| Unsupported("a def without a value".into()))?;
            let t = c.typed.def_types[d.0 as usize]
                .clone()
                .ok_or_else(|| Unsupported("a def without a type".into()))?;
            let text = defs.value(p, it, &v, &t)?;
            let lt = p.lir(&t)?;
            defs.values.insert(d, (text, lt));
        }
        Ok(())
    })?;
    Ok(defs)
}

impl Defs {
    /// The lIR text of a value of type `t`: a literal, or the address
    /// of its constant.
    fn value(
        &mut self,
        p: &mut Program<'_>,
        it: &Interp<'_>,
        v: &Val,
        t: &Ty,
    ) -> Result<String, Unsupported> {
        Ok(match v {
            Val::Unit => String::new(),
            Val::Bool(b) => format!("(i1 {})", i32::from(*b)),
            Val::Int(n, _) => {
                let l = p.lir(t)?.unwrap_or(LirTy::I64);
                format!("({} {n})", l.text())
            }
            Val::Float(x, _) => {
                let l = p.lir(t)?.unwrap_or(LirTy::Double);
                format!("({} {})", l.text(), crate::lower::float_text(*x))
            }
            Val::Char(c) => format!("(i32 {})", u32::from(*c)),
            Val::Kw(k) => {
                let name = it
                    .keyword_name(*k)
                    .map_err(|e| Unsupported(e.to_string()))?
                    .to_string();
                format!("(i64 {})", p.statics.keyword(&name))
            }
            Val::Ptr(x) => format!("(i64 {x})"),
            Val::Tag(_, i) => format!("(i32 {i})"),
            Val::None => "(ptr null)".into(),
            Val::Some(inner) => {
                let payload = option_payload(p.g(), t)
                    .cloned()
                    .ok_or_else(|| Unsupported("some at a non-Option type".into()))?;
                match option_rep(p.g(), &payload)? {
                    OptRep::Null => self.value(p, it, inner, &payload)?,
                    OptRep::Boxed => return Err(Unsupported("a def holding a boxed some".into())),
                }
            }
            Val::Obj(id) => self.object(p, it, *id, t)?,
        })
    }

    /// The address of the constant for object `id` of type `t`.
    fn object(
        &mut self,
        p: &mut Program<'_>,
        it: &Interp<'_>,
        id: ObjId,
        t: &Ty,
    ) -> Result<String, Unsupported> {
        if let Some(n) = self.names.get(&id) {
            return Ok(n.clone());
        }
        let obj = it
            .objs
            .get(&it.heap, id)
            .map_err(|e| Unsupported(e.to_string()))?
            .clone();
        let g = p.g();
        let name = match &obj {
            Obj::Str(s) => p.statics.string(s, 0),
            Obj::Closure(Clo::Fun(f)) => {
                let scheme = p.c.typed.fun_schemes[f.0 as usize]
                    .as_ref()
                    .ok_or_else(|| Unsupported("a def function without a scheme".into()))?;
                if scheme.n_vars > 0 {
                    return Err(Unsupported("a def naming a generic function".into()));
                }
                let code = p.request(BodyKey::AllOwned(*f), Vec::new());
                let (tid, _) = p.closure_object(&code, Vec::new());
                p.statics.closure(&code, tid)
            }
            Obj::Closure(_) => return Err(Unsupported("a def holding a closure".into())),
            Obj::Array(items) => {
                let elem = match t {
                    Ty::Con(Con::Array, args) => args[0].clone(),
                    _ => return Err(Unsupported("an array at a non-array type".into())),
                };
                let (tid, _) = p.object(t)?;
                let el = p
                    .lir(&elem)?
                    .ok_or_else(|| Unsupported("an array of unit".into()))?;
                let mut elems = Vec::new();
                for item in items {
                    elems.push(self.value(p, it, item, &elem)?);
                }
                let n = elems.len();
                let name = self.fresh();
                let _ = writeln!(
                    self.text,
                    "(constant internal {} {{ i64 i32 i32 i64 [{n} x {}] }} {{ (i64 0) (i32 {tid}) (i32 {IMMORTAL}) (i64 {n}) ([{n} x {}] {}) }})",
                    &name[1..],
                    el.text(),
                    el.text(),
                    elems.join(" ")
                );
                name
            }
            Obj::Struct { ty, fields } | Obj::Variant { ty, fields, .. } => {
                let tag = match &obj {
                    Obj::Variant { tag, .. } => Some(*tag as usize),
                    _ => None,
                };
                let args = match t {
                    Ty::Con(Con::Nominal(id), args) if id == ty => args.clone(),
                    _ => return Err(Unsupported("an object at another type".into())),
                };
                let (tid, sname) = p.object(t)?;
                let vs = variants(g, *ty, &args)?;
                let (sname, tys) = match tag {
                    None => (sname, vs[0].1.clone()),
                    Some(i) => (format!("{sname}.v{i}"), vs[i].1.clone()),
                };
                let mut texts = vec![
                    "(i64 0)".to_string(),
                    format!("(i32 {tid})"),
                    format!("(i32 {IMMORTAL})"),
                ];
                if let Some(i) = tag {
                    texts.push(format!("(i32 {i})"));
                }
                for (f, ft) in fields.iter().zip(&tys) {
                    let s = self.value(p, it, f, ft)?;
                    if !s.is_empty() {
                        texts.push(s);
                    }
                }
                let _ = STRUCT_FIELD0;
                let name = self.fresh();
                let _ = writeln!(
                    self.text,
                    "(constant internal {} %struct.{sname} (%struct.{sname} {}))",
                    &name[1..],
                    texts.join(" ")
                );
                name
            }
            other => return Err(Unsupported(format!("a def holding {other:?}"))),
        };
        self.names.insert(id, name.clone());
        Ok(name)
    }

    fn fresh(&mut self) -> String {
        self.counter += 1;
        format!("@def.{}", self.counter)
    }
}

/// Whether an object kind is one a def constant may hold.
pub fn _kind_ok(k: &ObjKind) -> bool {
    matches!(
        k,
        ObjKind::Str | ObjKind::Array(_) | ObjKind::Struct(_) | ObjKind::Enum(_)
    )
}
