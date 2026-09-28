//! Calls, tail calls and control flow.

use llvm_sys::core::*;
use llvm_sys::prelude::LLVMValueRef;
use llvm_sys::LLVMTailCallKind;

use super::cc_number;
use super::expr::V;
use super::func::Fx;
use crate::error::{Error, Result};
use crate::llvm::NONAME;
use lir::ast::{Callee, Expr, Kind};

impl<'l, 'f> Fx<'l, 'f> {
    /// A call; a tail call is `musttail` followed by `ret` (spec/lir.md
    /// §7.3).
    pub fn call(&mut self, e: &'f Expr) -> Result<Option<V>> {
        let Kind::Call { callee, args, tail } = &e.kind else {
            return Err(Error::Internal("not a call".into()));
        };
        let (f, ty) = match callee {
            Callee::Direct(g) => {
                let (f, ty) = self.lx.funcs[g].clone();
                (f, ty)
            }
            Callee::Indirect(p, ty) => (self.val(p)?, ty.clone()),
        };
        let mut vals = args
            .iter()
            .map(|a| self.val(a))
            .collect::<Result<Vec<_>>>()?;
        let fty = self.lx.fn_ty(&ty);
        // SAFETY: arity and types were checked against `ty`, which is the
        // callee's own type for a direct call.
        unsafe {
            let c = LLVMBuildCall2(
                self.b,
                fty,
                f,
                vals.as_mut_ptr(),
                vals.len() as u32,
                NONAME.as_ptr(),
            );
            LLVMSetInstructionCallConv(c, cc_number(ty.cc));
            if !*tail {
                return Ok(ty.ret.as_ref().map(|_| c));
            }
            LLVMSetTailCallKind(c, LLVMTailCallKind::LLVMTailCallKindMustTail);
            match ty.ret {
                Some(_) => LLVMBuildRet(self.b, c),
                None => LLVMBuildRetVoid(self.b),
            };
        }
        Ok(None)
    }

    /// `ret`, `br`, `switch`, `unreachable`.
    pub fn control(&mut self, e: &'f Expr) -> Result<()> {
        let b = self.b;
        // SAFETY: labels, operands and types were checked.
        unsafe {
            match &e.kind {
                Kind::Ret(Some(v)) => {
                    let v = self.val(v)?;
                    LLVMBuildRet(b, v);
                }
                Kind::Ret(None) => {
                    LLVMBuildRetVoid(b);
                }
                Kind::Br(l) => {
                    LLVMBuildBr(b, self.block(l));
                }
                Kind::CondBr(c, t, f) => {
                    let c = self.val(c)?;
                    LLVMBuildCondBr(b, c, self.block(t), self.block(f));
                }
                Kind::Switch(v, d, cases) => {
                    let v = self.val(v)?;
                    let sw = LLVMBuildSwitch(b, v, self.block(d), cases.len() as u32);
                    for (c, l) in cases {
                        let cv: LLVMValueRef = self.lx.constant(c)?;
                        LLVMAddCase(sw, cv, self.block(l));
                    }
                }
                Kind::Unreachable => {
                    LLVMBuildUnreachable(b);
                }
                _ => return Err(Error::Internal(format!("not a terminator at {:?}", e.pos))),
            }
        }
        Ok(())
    }

    fn block(&self, label: &str) -> llvm_sys::prelude::LLVMBasicBlockRef {
        self.blocks[self.labels[label]]
    }
}
