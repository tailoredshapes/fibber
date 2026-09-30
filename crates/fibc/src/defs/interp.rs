//! The interpreter backend of [`super`]: the reference interpreter
//! evaluates every `def` (`eval_defs`), and the resulting values are
//! read out of its heap. The executable spec of the same values the
//! JIT backend produces.

use std::collections::HashMap;

use fibref::eval::object::{Clo, Obj};
use fibref::eval::pipeline::with_threads;
use fibref::eval::{Interp, Val};
use fibref::own::program::BodyKey;
use fibref::types::ty::{Con, Ty};
use fibref::ObjId;

use super::{def_ids, Defs};
use crate::compile::Unsupported;
use crate::ir::LirTy;
use crate::layout::{option_payload, option_rep, OptRep};
use crate::program::Program;

/// Evaluates every `def` of the user module and serialises its value.
pub fn emit_defs(p: &mut Program<'_>) -> Result<Defs, Unsupported> {
    let c = p.c;
    let mut defs = Defs::default();
    let ids = def_ids(p);
    if ids.is_empty() {
        return Ok(defs);
    }
    let mut reader = Reader {
        names: HashMap::new(),
    };
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
            let text = reader.value(&mut defs, p, it, &v, &t)?;
            let lt = p.lir(&t)?;
            defs.values.insert(d, (text, lt));
        }
        Ok(())
    })?;
    Ok(defs)
}

/// The constants emitted for the interpreter's objects, by id.
struct Reader {
    names: HashMap<ObjId, String>,
}

impl Reader {
    /// The lIR text of a value of type `t`: a literal, or the address
    /// of its constant.
    fn value(
        &mut self,
        defs: &mut Defs,
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
                    OptRep::Null => self.value(defs, p, it, inner, &payload)?,
                    OptRep::Boxed => return Err(Unsupported("a def holding a boxed some".into())),
                }
            }
            Val::Obj(id) => self.object(defs, p, it, *id, t)?,
        })
    }

    /// The address of the constant for object `id` of type `t`.
    fn object(
        &mut self,
        defs: &mut Defs,
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
                let el = p
                    .lir(&elem)?
                    .ok_or_else(|| Unsupported("an array of unit".into()))?;
                let mut elems = Vec::new();
                for item in items {
                    elems.push(self.value(defs, p, it, item, &elem)?);
                }
                defs.emit_array(p, t, el, &elems)?
            }
            Obj::Struct { ty, fields } | Obj::Variant { ty, fields, .. } => {
                let tag = match &obj {
                    Obj::Variant { tag, .. } => Some(*tag as usize),
                    _ => None,
                };
                if !matches!(t, Ty::Con(Con::Nominal(id), _) if id == ty) {
                    return Err(Unsupported("an object at another type".into()));
                }
                let tys = Defs::field_types(p, t, tag)?;
                let mut texts = Vec::new();
                for (f, ft) in fields.iter().zip(&tys) {
                    texts.push(self.value(defs, p, it, f, ft)?);
                }
                defs.emit_object(p, t, tag, &texts)?
            }
            other => return Err(Unsupported(format!("a def holding {other:?}"))),
        };
        self.names.insert(id, name.clone());
        Ok(name)
    }
}
