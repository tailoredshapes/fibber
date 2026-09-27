//! The raw-pointer builtins and externs (syntax §3.15, types §6.13).
//! `raw` is an address with no count; `raw-retained` keeps the count
//! its operand arrived with (a store, E2) for the foreign side, and
//! `release-raw` gives it back.

use crate::types::ast::ExternId;

use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;

impl Interp<'_> {
    /// The builtins that are only allowed inside `unsafe`.
    pub fn unsafe_builtin(&mut self, name: &str, a: &[Val]) -> R<Val> {
        let arg = |i: usize| {
            a.get(i)
                .ok_or_else(|| RunError::internal(format!("{name} without argument {i}")))
        };
        let ptr = |i: usize| -> R<u64> {
            match arg(i)? {
                Val::Ptr(p) => Ok(*p),
                v => Err(RunError::internal(format!("{name}: not a pointer: {v:?}"))),
            }
        };
        let int = |i: usize| arg(i)?.as_int();
        use crate::types::ty::Scalar::*;
        let (len, width) = match name.rsplit('-').next() {
            Some("i8") => (1, I8),
            Some("i16") => (2, I16),
            Some("i32") => (4, I32),
            Some("i64") => (8, I64),
            _ => (8, Ptr),
        };
        match name {
            "ptr+" => Ok(Val::Ptr(ptr(0)?.wrapping_add(int(1)? as u64))),
            "alloc" => Ok(Val::Ptr(self.raw.alloc(int(0)?)?)),
            "free" => self.raw.free(ptr(0)?).map(|()| Val::Unit),
            "load-ptr" => Ok(Val::Ptr(self.raw.load(ptr(0)?, 8)? as u64)),
            "store-ptr" => {
                let p = ptr(1)?;
                self.raw.store(ptr(0)?, 8, p as i64).map(|()| Val::Unit)
            }
            n if n.starts_with("load-") => Ok(Val::Int(self.raw.load(ptr(0)?, len)?, width)),
            n if n.starts_with("store-") => {
                self.raw.store(ptr(0)?, len, int(1)?).map(|()| Val::Unit)
            }
            "raw" | "raw-retained" => {
                let id = arg(0)?.expect_obj(name)?;
                Ok(Val::Ptr(self.raw.object_address(id)))
            }
            "release-raw" => {
                let id = self.raw.address_object(ptr(0)?).ok_or_else(|| {
                    RunError::trap("release-raw of a pointer that raw-retained did not give")
                })?;
                self.heap.release(id)?;
                Ok(Val::Unit)
            }
            _ => Err(RunError::internal(format!("no builtin {name}"))),
        }
    }

    /// A call to an `extern`: only the prelude's `write` exists here.
    pub fn call_extern(&mut self, x: ExternId, a: &[Val]) -> R<Val> {
        let ext = self.p.globals.ext(x);
        match (ext.name.as_str(), a) {
            ("write", [fd, Val::Ptr(p), n]) => {
                let w = self.raw.write(fd.as_int()?, *p, n.as_int()?)?;
                Ok(Val::Int(w, crate::types::ty::Scalar::I64))
            }
            (name, _) => Err(RunError::unsupported(format!(
                "extern {name} is not available in the reference interpreter"
            ))),
        }
    }
}
