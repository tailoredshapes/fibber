//! The unix primitives (stdlib "sys primitives"): each a call of the
//! runtime function of `rt/sys.lir`, whose results the interpreter's
//! `eval/sys.rs` repeats.

use fibref::types::ty::{Con, Scalar, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};

impl Cx<'_, '_> {
    /// The `sys-` builtin `name` over the lowered arguments `a`.
    pub fn sys_builtin(&mut self, name: &str, a: &[V]) -> R<V> {
        let (f, ret) = match name {
            "sys-open" => ("fib.sys-open", LirTy::I64),
            "sys-close" => ("fib.sys-close", LirTy::I64),
            "sys-write" => ("fib.sys-write", LirTy::I64),
            "sys-seek" => ("fib.sys-seek", LirTy::I64),
            "sys-pipe" => ("fib.sys-pipe", LirTy::I64),
            "sys-dup" => ("fib.sys-dup", LirTy::I64),
            "sys-isatty" => ("fib.sys-isatty", LirTy::I1),
            "sys-unlink" => ("fib.sys-unlink", LirTy::I64),
            "sys-mkdir" => ("fib.sys-mkdir", LirTy::I64),
            "sys-rmdir" => ("fib.sys-rmdir", LirTy::I64),
            "sys-errno-text" => ("fib.sys-errno-text", LirTy::Ptr),
            "sys-getenv" => ("fib.sys-getenv", LirTy::Ptr),
            "sys-clock-now" => ("fib.sys-clock-now", LirTy::I64),
            "sys-wall-now" => ("fib.sys-wall-now", LirTy::I64),
            "sys-sleep" => ("fib.sys-sleep", LirTy::I64),
            "sys-read" => {
                // The array of the status and the bytes is an `(Array i8)`,
                // as `str-bytes` makes one.
                let i8s = Ty::Con(Con::Array, vec![Ty::scalar(Scalar::I8)]);
                let (tid, _) = self.p.object(&i8s)?;
                let mut v = a.to_vec();
                v.push(V::int(LirTy::I32, i64::from(tid)));
                return Ok(self.rt_call("fib.sys-read", &v, Some(LirTy::Ptr)));
            }
            other => return Err(Unsupported(format!("builtin {other}"))),
        };
        Ok(self.rt_call(f, a, Some(ret)))
    }
}
