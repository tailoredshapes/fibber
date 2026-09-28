//! Lowering one function: blocks in dominator order, phis fixed up at
//! the end.

use std::collections::HashMap;

use llvm_sys::core::{
    LLVMAddIncoming, LLVMAppendBasicBlockInContext, LLVMCreateBuilderInContext, LLVMDisposeBuilder,
    LLVMGetBasicBlockTerminator, LLVMGetInstructionParent, LLVMGetNumSuccessors, LLVMGetParam,
    LLVMGetSuccessor, LLVMPositionBuilderAtEnd,
};
use llvm_sys::prelude::{LLVMBasicBlockRef, LLVMBuilderRef, LLVMValueRef};

use super::expr::name_value;
use super::Lx;
use crate::error::{Error, Result};
use crate::llvm::cstr;
use lir::ast::{Binding, Expr, Function, Kind};

/// Per-function lowering state.
pub struct Fx<'l, 'f> {
    pub lx: &'l mut Lx,
    pub b: LLVMBuilderRef,
    pub blocks: Vec<LLVMBasicBlockRef>,
    pub labels: HashMap<&'f str, usize>,
    pub names: HashMap<&'f str, LLVMValueRef>,
    pub phis: Vec<(LLVMValueRef, &'f [Binding])>,
}

impl Lx {
    pub fn function(&mut self, f: &Function) -> Result<()> {
        let func = self.funcs[&f.name].0;
        let order = lir::check::block_order(f).map_err(|d| Error::Internal(d.to_string()))?;
        // SAFETY: the builder is disposed at the end of this function.
        let b = unsafe { LLVMCreateBuilderInContext(self.ctx) };
        let blocks = f
            .blocks
            .iter()
            .map(|blk| {
                let n = cstr(&blk.label);
                // SAFETY: the function belongs to this context.
                unsafe { LLVMAppendBasicBlockInContext(self.ctx, func, n.as_ptr()) }
            })
            .collect();
        let mut fx = Fx {
            lx: self,
            b,
            blocks,
            labels: f
                .blocks
                .iter()
                .enumerate()
                .map(|(i, x)| (x.label.as_str(), i))
                .collect(),
            names: HashMap::new(),
            phis: Vec::new(),
        };
        let r = fx.body(f, func, &order);
        // SAFETY: created above.
        unsafe { LLVMDisposeBuilder(b) };
        r
    }
}

impl<'l, 'f> Fx<'l, 'f> {
    fn body(&mut self, f: &'f Function, func: LLVMValueRef, order: &[usize]) -> Result<()> {
        for (i, (name, _)) in f.params.iter().enumerate() {
            // SAFETY: the function has exactly these parameters.
            let v = unsafe { LLVMGetParam(func, i as u32) };
            name_value(v, name);
            self.names.insert(name, v);
        }
        for &blk in order {
            // SAFETY: the block belongs to this function.
            unsafe { LLVMPositionBuilderAtEnd(self.b, self.blocks[blk]) };
            for e in &f.blocks[blk].body {
                self.expr(e)?;
            }
        }
        let phis = std::mem::take(&mut self.phis);
        for (phi, incoming) in phis {
            for inc in incoming {
                let mut v = self.incoming(&inc.value)?;
                let mut bb = self.blocks[self.labels[inc.name.as_str()]];
                // LLVM wants one entry per edge: a `br` or `switch` naming
                // the phi's block twice gets the same value twice.
                // SAFETY: one value and one block, both of this function.
                unsafe {
                    for _ in 0..edges(bb, LLVMGetInstructionParent(phi)) {
                        LLVMAddIncoming(phi, &mut v, &mut bb, 1);
                    }
                }
            }
        }
        Ok(())
    }

    fn incoming(&mut self, e: &Expr) -> Result<LLVMValueRef> {
        match &e.kind {
            Kind::Local(n) => self
                .names
                .get(n.as_str())
                .copied()
                .ok_or_else(|| Error::Internal(format!("phi value {n} not lowered"))),
            _ => self.lx.constant(e),
        }
    }
}

/// How many edges of `from`'s terminator lead to `to`.
///
/// # Safety
/// Both blocks belong to a function whose blocks are all terminated.
unsafe fn edges(from: LLVMBasicBlockRef, to: LLVMBasicBlockRef) -> u32 {
    let term = LLVMGetBasicBlockTerminator(from);
    if term.is_null() {
        return 1;
    }
    let n = LLVMGetNumSuccessors(term);
    (0..n)
        .filter(|&i| LLVMGetSuccessor(term, i) == to)
        .count()
        .max(1) as u32
}
