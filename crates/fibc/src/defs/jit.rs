//! The JIT backend of [`super`] (types §8.10, compiler.md §8 item 4):
//! each `def`'s initialiser is lowered as the body `d.NAME`, the
//! module so far (runtime, tables, the constants of the earlier `def`s,
//! every body) is JIT-compiled through `lair`, the initialiser is
//! called, and its value is read out of the JIT's memory by the type
//! table's layouts into constants. This is the mechanism the
//! self-hosted compiler will use, with no interpreter to fall back on.

use std::collections::HashMap;

use fibref::types::ty::{Con, Ty};
use lair::{Jit, JitOptions};

use super::{def_ids, Defs};
use crate::compile::{assemble_parts, emit_all, Unsupported};
use crate::ir::LirTy;
use crate::layout::{option_payload, option_rep, size_align, OptRep};
use crate::objects::ObjKind;
use crate::program::Program;

/// Evaluates every `def` of the user module through the JIT and
/// serialises its value.
pub fn emit_defs(p: &mut Program<'_>) -> Result<Defs, Unsupported> {
    let mut defs = Defs::default();
    for (k, d) in def_ids(p).into_iter().enumerate() {
        let t = p.c.typed.def_types[d.0 as usize]
            .clone()
            .ok_or_else(|| Unsupported("a def without a type".into()))?;
        let body = p.request(fibref::own::program::BodyKey::Def(d), Vec::new());
        emit_all(p)?;
        let lt = p.lir(&t)?;
        let entry = format!("fib.defentry.{k}");
        let init = format!("fib.definit.{k}");
        let mut text = assemble_parts(p, &defs.text);
        text.push_str(&match lt {
            Some(l) => format!(
                "(define ({entry} {}) () (block entry (ret (call @{body}))))\n",
                l.text()
            ),
            None => format!("(define ({entry} void) () (block entry (call @{body}) (ret)))\n"),
        });
        // Not `fib.init`: it would also read FIB_TRACE, and the objects
        // a `def` allocates at compile time are not in the trace of the
        // run (compiler.md §4), whatever the environment says.
        text.push_str(&format!(
            "(define ({init} void) () (block entry (call @fib.rq-init) (ret)))\n"
        ));
        // The statics a value may point to are `internal`, so each gets
        // an exported getter of its address, for sharing over copying.
        let known: Vec<String> = defs
            .constants
            .iter()
            .chain(p.statics.closure_names().iter())
            .cloned()
            .collect();
        text.push_str(&address_getters(k, &known));
        let fail = |e: lair::Error| Unsupported(format!("def {k}: {e}"));
        let mut jit = Jit::new(JitOptions::default()).map_err(fail)?;
        jit.add_source(&format!("def.{k}"), &text).map_err(fail)?;
        // SAFETY: `init` is defined just above as (fn void ()).
        let init_fn: extern "C" fn() = unsafe { jit.function(&init).map_err(fail)? };
        init_fn();
        let mut reader = Reader {
            names: known_addresses(&mut jit, k, &known)?,
        };
        let raw = call_entry(&mut jit, &entry, lt)?;
        let value = reader.value(&mut defs, p, raw, &t)?;
        // The initialisers after this one see the value (syntax §3.19).
        p.def_values.insert(d, (value.clone(), lt));
        defs.values.insert(d, (value, lt));
    }
    Ok(defs)
}

/// The exported getter of each static's address (`fib.defaddr.K.I`).
fn address_getters(k: usize, known: &[String]) -> String {
    let mut text = String::new();
    for (i, name) in known.iter().enumerate() {
        text.push_str(&format!(
            "(define (fib.defaddr.{k}.{i} ptr) () (block entry (ret {name})))\n"
        ));
    }
    text
}

/// What a `def`'s entry returned.
#[derive(Clone, Copy, Debug)]
enum Raw {
    Unit,
    Int(i64),
    F32(f32),
    F64(f64),
    Ptr(usize),
}

/// Calls the entry with the signature its lIR result type gives.
fn call_entry(jit: &mut Jit, entry: &str, lt: Option<LirTy>) -> Result<Raw, Unsupported> {
    let fail = |e: lair::Error| Unsupported(format!("def entry: {e}"));
    // SAFETY: the entry is defined by `emit_defs` with exactly the lIR
    // result type matched here, and no parameters; the JIT outlives
    // the call.
    unsafe {
        Ok(match lt {
            None => {
                let f: extern "C" fn() = jit.function(entry).map_err(fail)?;
                f();
                Raw::Unit
            }
            Some(LirTy::I1) => {
                let f: extern "C" fn() -> bool = jit.function(entry).map_err(fail)?;
                Raw::Int(i64::from(f()))
            }
            Some(LirTy::I8) => {
                let f: extern "C" fn() -> i8 = jit.function(entry).map_err(fail)?;
                Raw::Int(i64::from(f()))
            }
            Some(LirTy::I16) => {
                let f: extern "C" fn() -> i16 = jit.function(entry).map_err(fail)?;
                Raw::Int(i64::from(f()))
            }
            Some(LirTy::I32) => {
                let f: extern "C" fn() -> i32 = jit.function(entry).map_err(fail)?;
                Raw::Int(i64::from(f()))
            }
            Some(LirTy::I64) => {
                let f: extern "C" fn() -> i64 = jit.function(entry).map_err(fail)?;
                Raw::Int(f())
            }
            Some(LirTy::Float) => {
                let f: extern "C" fn() -> f32 = jit.function(entry).map_err(fail)?;
                Raw::F32(f())
            }
            Some(LirTy::Double) => {
                let f: extern "C" fn() -> f64 = jit.function(entry).map_err(fail)?;
                Raw::F64(f())
            }
            Some(LirTy::Ptr) => {
                let f: extern "C" fn() -> usize = jit.function(entry).map_err(fail)?;
                Raw::Ptr(f())
            }
            Some(LirTy::Dyn) => return Err(Unsupported("a def holding a dyn".into())),
            Some(LirTy::Raw) => return Err(Unsupported("a def holding a raw ptr".into())),
        })
    }
}

/// The JIT addresses of the static objects a value may point to, the
/// constants of the earlier `def`s and the immortal closures of named
/// functions, through the getters `emit_defs` exported for them.
fn known_addresses(
    jit: &mut Jit,
    k: usize,
    known: &[String],
) -> Result<HashMap<usize, String>, Unsupported> {
    let mut names = HashMap::new();
    let fail = |e: lair::Error| Unsupported(format!("def constants: {e}"));
    for (i, name) in known.iter().enumerate() {
        // SAFETY: the getter is defined by `emit_defs` as (fn ptr ()).
        let get: extern "C" fn() -> usize = unsafe {
            jit.function(&format!("fib.defaddr.{k}.{i}"))
                .map_err(fail)?
        };
        names.insert(get(), name.clone());
    }
    Ok(names)
}

/// Reads values out of the JIT's memory.
struct Reader {
    names: HashMap<usize, String>,
}

/// A raw read of `l` at `addr`.
///
/// SAFETY: the caller has an address inside an object the JIT
/// allocated, at a field of that type by the type table's layout.
unsafe fn read(addr: usize, l: LirTy) -> Result<Raw, Unsupported> {
    Ok(match l {
        LirTy::I1 => Raw::Int(i64::from(*(addr as *const u8) & 1)),
        LirTy::I8 => Raw::Int(i64::from(*(addr as *const i8))),
        LirTy::I16 => Raw::Int(i64::from(*(addr as *const i16))),
        LirTy::I32 => Raw::Int(i64::from(*(addr as *const i32))),
        LirTy::I64 => Raw::Int(*(addr as *const i64)),
        LirTy::Float => Raw::F32(*(addr as *const f32)),
        LirTy::Double => Raw::F64(*(addr as *const f64)),
        LirTy::Ptr => Raw::Ptr(*(addr as *const usize)),
        LirTy::Dyn => return Err(Unsupported("a def holding a dyn".into())),
        LirTy::Raw => return Err(Unsupported("a def holding a raw ptr".into())),
    })
}

impl Reader {
    /// The lIR text of a value of type `t`: a literal, or the address
    /// of its constant.
    fn value(
        &mut self,
        defs: &mut Defs,
        p: &mut Program<'_>,
        raw: Raw,
        t: &Ty,
    ) -> Result<String, Unsupported> {
        let l = p.lir(t)?;
        Ok(match (raw, l) {
            (Raw::Unit, _) | (_, None) => String::new(),
            (Raw::Int(n), Some(l)) => format!("({} {n})", l.text()),
            (Raw::F32(x), Some(l)) => {
                format!("({} {})", l.text(), crate::lower::float_text(f64::from(x)))
            }
            (Raw::F64(x), Some(l)) => format!("({} {})", l.text(), crate::lower::float_text(x)),
            (Raw::Ptr(a), Some(LirTy::Ptr)) => {
                if let Some(payload) = option_payload(p.g(), t).cloned() {
                    match (option_rep(p.g(), &payload)?, a) {
                        (OptRep::Null, 0) => "(ptr null)".into(),
                        (OptRep::Null, _) => self.object(defs, p, a, &payload)?,
                        // A boxed nil may be null (a generic payload gave
                        // the plan no allocation): the constant is the
                        // nil object either way, as the interpreter's.
                        (OptRep::Boxed, 0) => defs.emit_object(p, t, Some(0), &[])?,
                        (OptRep::Boxed, _) => self.object(defs, p, a, t)?,
                    }
                } else {
                    self.object(defs, p, a, t)?
                }
            }
            (Raw::Ptr(_), Some(l)) => {
                return Err(Unsupported(format!(
                    "a pointer where {} was expected",
                    l.text()
                )))
            }
        })
    }

    /// The constant for the object at `a`, of type `t`.
    fn object(
        &mut self,
        defs: &mut Defs,
        p: &mut Program<'_>,
        a: usize,
        t: &Ty,
    ) -> Result<String, Unsupported> {
        if a == 0 {
            return Ok("(ptr null)".into());
        }
        if let Some(n) = self.names.get(&a) {
            return Ok(n.clone());
        }
        // SAFETY: `a` is an object the JIT allocated or a static of the
        // module, with the header of types §8.2.
        let tid = unsafe { *((a + 8) as *const u32) };
        let info = p.objects.get(tid).clone();
        let name = match &info.kind {
            ObjKind::Str => {
                // SAFETY: a str is its header, the byte length, the bytes.
                let s = unsafe {
                    let n = *((a + 16) as *const i64) as usize;
                    let bytes = std::slice::from_raw_parts((a + 24) as *const u8, n);
                    String::from_utf8_lossy(bytes).into_owned()
                };
                p.statics.string(&s, 0)
            }
            ObjKind::Array(el) => {
                let elem = match t {
                    Ty::Con(Con::Array, args) => args[0].clone(),
                    _ => return Err(Unsupported("an array at a non-array type".into())),
                };
                let el = *el;
                // SAFETY: an array is its header, the length, the elements.
                let n = unsafe { *((a + 16) as *const i64) } as usize;
                let stride = size_align(el).0 as usize;
                let mut elems = Vec::new();
                for i in 0..n {
                    // SAFETY: element i lies inside the array's allocation.
                    let raw = unsafe { read(a + 24 + i * stride, el)? };
                    elems.push(self.value(defs, p, raw, &elem)?);
                }
                defs.emit_array(p, t, el, &elems)?
            }
            ObjKind::Struct(_) | ObjKind::Enum(_) => {
                let tag = match &info.kind {
                    // SAFETY: an enum's tag follows its header.
                    ObjKind::Enum(_) => Some(unsafe { *((a + 16) as *const i32) } as usize),
                    _ => None,
                };
                let tys = Defs::field_types(p, t, tag)?;
                let slots = info.slots(tag);
                let offsets = info.offsets(tag);
                let mut texts = Vec::new();
                let mut slot = 0;
                for (ft, st) in tys.iter().zip(&slots) {
                    match st {
                        None => texts.push(String::new()),
                        Some(l) => {
                            // SAFETY: the slot lies inside the object at
                            // the type table's offset.
                            let raw = unsafe { read(a + offsets[slot] as usize, *l)? };
                            texts.push(self.value(defs, p, raw, ft)?);
                            slot += 1;
                        }
                    }
                }
                defs.emit_object(p, t, tag, &texts)?
            }
            ObjKind::Closure(_) => return Err(Unsupported("a def holding a closure".into())),
            other => return Err(Unsupported(format!("a def holding a {other:?}"))),
        };
        self.names.insert(a, name.clone());
        Ok(name)
    }
}
