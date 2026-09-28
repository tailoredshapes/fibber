//! lIR types to LLVM types.

use llvm_sys::core::{
    LLVMArrayType2, LLVMDoubleTypeInContext, LLVMFloatTypeInContext, LLVMFunctionType,
    LLVMIntTypeInContext, LLVMPointerTypeInContext, LLVMStructTypeInContext, LLVMVectorType,
    LLVMVoidTypeInContext,
};
use llvm_sys::prelude::LLVMTypeRef;

use super::Lx;
use lir::types::{FnType, Type};

impl Lx {
    pub fn ty(&self, t: &Type) -> LLVMTypeRef {
        // SAFETY: the context is alive; named structs were created
        // before any type refers to them.
        unsafe {
            match t {
                Type::Int(n) => LLVMIntTypeInContext(self.ctx, *n),
                Type::Float => LLVMFloatTypeInContext(self.ctx),
                Type::Double => LLVMDoubleTypeInContext(self.ctx),
                Type::Ptr => LLVMPointerTypeInContext(self.ctx, 0),
                Type::Vector(n, e) => LLVMVectorType(self.ty(e), *n),
                Type::Array(n, e) => LLVMArrayType2(self.ty(e), *n),
                Type::Named(s) => self.structs[s],
                Type::Anon(fs) => {
                    let mut f: Vec<LLVMTypeRef> = fs.iter().map(|x| self.ty(x)).collect();
                    LLVMStructTypeInContext(self.ctx, f.as_mut_ptr(), f.len() as u32, 0)
                }
            }
        }
    }

    pub fn ret_ty(&self, t: &Option<Type>) -> LLVMTypeRef {
        match t {
            Some(t) => self.ty(t),
            // SAFETY: the context is alive.
            None => unsafe { LLVMVoidTypeInContext(self.ctx) },
        }
    }

    pub fn fn_ty(&self, f: &FnType) -> LLVMTypeRef {
        let mut ps: Vec<LLVMTypeRef> = f.params.iter().map(|p| self.ty(p)).collect();
        // SAFETY: every type belongs to this context.
        unsafe {
            LLVMFunctionType(
                self.ret_ty(&f.ret),
                ps.as_mut_ptr(),
                ps.len() as u32,
                i32::from(f.varargs),
            )
        }
    }
}
