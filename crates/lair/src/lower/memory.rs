//! Memory and atomic instructions.

use llvm_sys::core::*;
use llvm_sys::target::LLVMABIAlignmentOfType;
use llvm_sys::{LLVMAtomicOrdering, LLVMAtomicRMWBinOp};

use super::expr::V;
use super::func::Fx;
use crate::error::{Error, Result};
use crate::llvm::NONAME;
use lir::ast::{Expr, Kind, Ordering, RmwOp, Scope};

fn ordering(o: Ordering) -> LLVMAtomicOrdering {
    use LLVMAtomicOrdering::*;
    match o {
        Ordering::Unordered => LLVMAtomicOrderingUnordered,
        Ordering::Monotonic => LLVMAtomicOrderingMonotonic,
        Ordering::Acquire => LLVMAtomicOrderingAcquire,
        Ordering::Release => LLVMAtomicOrderingRelease,
        Ordering::AcqRel => LLVMAtomicOrderingAcquireRelease,
        Ordering::SeqCst => LLVMAtomicOrderingSequentiallyConsistent,
    }
}

fn rmw_op(op: RmwOp) -> LLVMAtomicRMWBinOp {
    use LLVMAtomicRMWBinOp::*;
    match op {
        RmwOp::Xchg => LLVMAtomicRMWBinOpXchg,
        RmwOp::Add => LLVMAtomicRMWBinOpAdd,
        RmwOp::Sub => LLVMAtomicRMWBinOpSub,
        RmwOp::And => LLVMAtomicRMWBinOpAnd,
        RmwOp::Nand => LLVMAtomicRMWBinOpNand,
        RmwOp::Or => LLVMAtomicRMWBinOpOr,
        RmwOp::Xor => LLVMAtomicRMWBinOpXor,
        RmwOp::Max => LLVMAtomicRMWBinOpMax,
        RmwOp::Min => LLVMAtomicRMWBinOpMin,
        RmwOp::UMax => LLVMAtomicRMWBinOpUMax,
        RmwOp::UMin => LLVMAtomicRMWBinOpUMin,
        RmwOp::FAdd => LLVMAtomicRMWBinOpFAdd,
        RmwOp::FSub => LLVMAtomicRMWBinOpFSub,
        RmwOp::FMax => LLVMAtomicRMWBinOpFMax,
        RmwOp::FMin => LLVMAtomicRMWBinOpFMin,
    }
}

fn single(s: Scope) -> i32 {
    i32::from(s == Scope::SingleThread)
}

/// `volatile` and `(align N)` on an access or an alloca (spec/lir.md
/// §6.5); without `align` the builder's ABI alignment stands.
///
/// # Safety
/// `inst` is a load, store or alloca of the current builder.
unsafe fn access_options(inst: V, volatile: bool, align: Option<u32>) {
    if volatile {
        LLVMSetVolatile(inst, 1);
    }
    if let Some(a) = align {
        LLVMSetAlignment(inst, a);
    }
}

impl<'l, 'f> Fx<'l, 'f> {
    pub fn memory(&mut self, e: &'f Expr) -> Result<Option<V>> {
        let (b, n) = (self.b, NONAME.as_ptr());
        // SAFETY: operands were lowered in this function's context and
        // the checker guaranteed their types (spec/lir.md §6.5).
        unsafe {
            Ok(Some(match &e.kind {
                Kind::Alloca { ty, count, align } => {
                    let a = match count {
                        None => LLVMBuildAlloca(b, self.lx.ty(ty), n),
                        Some(c) => {
                            let c = self.val(c)?;
                            LLVMBuildArrayAlloca(b, self.lx.ty(ty), c, n)
                        }
                    };
                    access_options(a, false, *align);
                    a
                }
                Kind::Load {
                    ty,
                    ptr,
                    volatile,
                    align,
                } => {
                    let p = self.val(ptr)?;
                    let l = LLVMBuildLoad2(b, self.lx.ty(ty), p, n);
                    access_options(l, *volatile, *align);
                    l
                }
                Kind::Store {
                    value,
                    ptr,
                    volatile,
                    align,
                } => {
                    let (v, p) = (self.val(value)?, self.val(ptr)?);
                    let st = LLVMBuildStore(b, v, p);
                    access_options(st, *volatile, *align);
                    return Ok(None);
                }
                Kind::Gep {
                    inbounds,
                    ty,
                    ptr,
                    indices,
                } => {
                    let p = self.val(ptr)?;
                    let mut idx = indices
                        .iter()
                        .map(|i| self.val(i))
                        .collect::<Result<Vec<_>>>()?;
                    let build = if *inbounds {
                        LLVMBuildInBoundsGEP2
                    } else {
                        LLVMBuildGEP2
                    };
                    build(b, self.lx.ty(ty), p, idx.as_mut_ptr(), idx.len() as u32, n)
                }
                _ => return self.atomic(e),
            }))
        }
    }

    /// Mark a plain load or store atomic, aligned to its type.
    unsafe fn make_atomic(
        &self,
        inst: V,
        ty: llvm_sys::prelude::LLVMTypeRef,
        o: Ordering,
        s: Scope,
    ) {
        LLVMSetOrdering(inst, ordering(o));
        LLVMSetAtomicSingleThread(inst, single(s));
        LLVMSetAlignment(inst, LLVMABIAlignmentOfType(self.lx.td, ty));
    }

    fn atomic(&mut self, e: &'f Expr) -> Result<Option<V>> {
        let (b, n) = (self.b, NONAME.as_ptr());
        // SAFETY: as in `memory`; orderings and types were checked
        // against LLVM's rules (spec/lir.md §6.6).
        unsafe {
            Ok(match &e.kind {
                Kind::AtomicLoad(s, o, t, p) => {
                    let p = self.val(p)?;
                    let ty = self.lx.ty(t);
                    let v = LLVMBuildLoad2(b, ty, p, n);
                    self.make_atomic(v, ty, *o, *s);
                    Some(v)
                }
                Kind::AtomicStore(s, o, v, p) => {
                    let (v, p) = (self.val(v)?, self.val(p)?);
                    let st = LLVMBuildStore(b, v, p);
                    self.make_atomic(st, LLVMTypeOf(v), *o, *s);
                    None
                }
                Kind::AtomicRmw(op, s, o, p, v) => {
                    let (p, v) = (self.val(p)?, self.val(v)?);
                    Some(LLVMBuildAtomicRMW(
                        b,
                        rmw_op(*op),
                        p,
                        v,
                        ordering(*o),
                        single(*s),
                    ))
                }
                Kind::CmpXchg {
                    weak,
                    scope,
                    success,
                    failure,
                    ptr,
                    expected,
                    new,
                } => {
                    let (p, x, y) = (self.val(ptr)?, self.val(expected)?, self.val(new)?);
                    let v = LLVMBuildAtomicCmpXchg(
                        b,
                        p,
                        x,
                        y,
                        ordering(*success),
                        ordering(*failure),
                        single(*scope),
                    );
                    LLVMSetWeak(v, i32::from(*weak));
                    Some(v)
                }
                Kind::Fence(s, o) => {
                    LLVMBuildFence(b, ordering(*o), single(*s), n);
                    None
                }
                _ => return Err(Error::Internal(format!("unexpected form at {:?}", e.pos))),
            })
        }
    }
}
