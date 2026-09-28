//! Constants: literals, strings, addresses and struct constants.

use llvm_sys::core::{
    LLVMAddGlobal, LLVMArrayType2, LLVMConstInt, LLVMConstNamedStruct, LLVMConstNull,
    LLVMConstReal, LLVMConstStringInContext2, LLVMConstStructInContext, LLVMConstVector,
    LLVMInt8TypeInContext, LLVMSetGlobalConstant, LLVMSetInitializer, LLVMSetLinkage,
    LLVMSetUnnamedAddress,
};
use llvm_sys::prelude::LLVMValueRef;
use llvm_sys::{LLVMLinkage, LLVMUnnamedAddr};

use super::Lx;
use crate::error::{Error, Result};
use lir::ast::{Expr, Kind};
use lir::types::Type;

/// The low `bits` of `v`, as LLVM's constant constructor wants them.
pub fn low_bits(v: i128, bits: u32) -> u64 {
    let m: u128 = if bits >= 64 {
        u64::MAX as u128
    } else {
        (1u128 << bits) - 1
    };
    ((v as u128) & m) as u64
}

impl Lx {
    /// A constant expression (spec/lir.md §6.11, §9).
    pub fn constant(&mut self, e: &Expr) -> Result<LLVMValueRef> {
        // SAFETY: all values and types belong to this module's context.
        unsafe {
            Ok(match &e.kind {
                Kind::Int(t @ Type::Int(b), v) => LLVMConstInt(self.ty(t), low_bits(*v, *b), 0),
                Kind::Float(t, v) => LLVMConstReal(self.ty(t), *v),
                Kind::Null => LLVMConstNull(self.ty(&Type::Ptr)),
                Kind::Vector(_, es) => {
                    let mut vs = es
                        .iter()
                        .map(|x| self.constant(x))
                        .collect::<Result<Vec<_>>>()?;
                    LLVMConstVector(vs.as_mut_ptr(), vs.len() as u32)
                }
                Kind::Str(bytes) => self.string(bytes),
                Kind::Global(g) => self.address(g)?,
                Kind::Struct(name, fs) => {
                    let mut vs = fs
                        .iter()
                        .map(|x| self.constant(x))
                        .collect::<Result<Vec<_>>>()?;
                    match name {
                        Some(n) => {
                            LLVMConstNamedStruct(self.structs[n], vs.as_mut_ptr(), vs.len() as u32)
                        }
                        None => {
                            LLVMConstStructInContext(self.ctx, vs.as_mut_ptr(), vs.len() as u32, 0)
                        }
                    }
                }
                _ => return Err(Error::Internal(format!("not a constant at {:?}", e.pos))),
            })
        }
    }

    /// The address of a function or global.
    pub fn address(&self, g: &str) -> Result<LLVMValueRef> {
        if let Some((f, _)) = self.funcs.get(g) {
            return Ok(*f);
        }
        match self.globals.get(g) {
            Some(v) => Ok(*v),
            None => Err(Error::Internal(format!("unknown global @{g}"))),
        }
    }

    /// A private constant holding `bytes` and a NUL; its address.
    pub fn string(&mut self, bytes: &[u8]) -> LLVMValueRef {
        // SAFETY: the context and module are alive; the byte slice is
        // copied by LLVMConstStringInContext2.
        unsafe {
            let init = LLVMConstStringInContext2(self.ctx, bytes.as_ptr().cast(), bytes.len(), 0);
            let ty = LLVMArrayType2(LLVMInt8TypeInContext(self.ctx), bytes.len() as u64 + 1);
            let g = LLVMAddGlobal(self.module, ty, c".str".as_ptr());
            LLVMSetInitializer(g, init);
            LLVMSetGlobalConstant(g, 1);
            LLVMSetLinkage(g, LLVMLinkage::LLVMPrivateLinkage);
            LLVMSetUnnamedAddress(g, LLVMUnnamedAddr::LLVMGlobalUnnamedAddr);
            g
        }
    }
}
