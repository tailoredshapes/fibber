//! Arithmetic, comparisons, conversions, select, vectors, aggregates.

use llvm_sys::core::*;
use llvm_sys::prelude::{LLVMTypeRef, LLVMValueRef};
use llvm_sys::{LLVMIntPredicate, LLVMOpcode, LLVMRealPredicate};

use super::expr::V;
use super::func::Fx;
use crate::error::{Error, Result};
use crate::llvm::NONAME;
use lir::ast::{BinOp, CastOp, Expr, FPred, IPred, Kind, OvfOp, UnOp};

fn bin_opcode(op: BinOp) -> LLVMOpcode {
    use BinOp::*;
    use LLVMOpcode::*;
    match op {
        Add => LLVMAdd,
        Sub => LLVMSub,
        Mul => LLVMMul,
        SDiv => LLVMSDiv,
        UDiv => LLVMUDiv,
        SRem => LLVMSRem,
        URem => LLVMURem,
        FAdd => LLVMFAdd,
        FSub => LLVMFSub,
        FMul => LLVMFMul,
        FDiv => LLVMFDiv,
        FRem => LLVMFRem,
        And => LLVMAnd,
        Or => LLVMOr,
        Xor => LLVMXor,
        Shl => LLVMShl,
        LShr => LLVMLShr,
        AShr => LLVMAShr,
    }
}

fn cast_opcode(op: CastOp) -> LLVMOpcode {
    use CastOp::*;
    use LLVMOpcode::*;
    match op {
        Trunc => LLVMTrunc,
        ZExt => LLVMZExt,
        SExt => LLVMSExt,
        FpTrunc => LLVMFPTrunc,
        FpExt => LLVMFPExt,
        FpToSi => LLVMFPToSI,
        FpToUi => LLVMFPToUI,
        SiToFp => LLVMSIToFP,
        UiToFp => LLVMUIToFP,
        PtrToInt => LLVMPtrToInt,
        IntToPtr => LLVMIntToPtr,
        Bitcast => LLVMBitCast,
        // The saturating conversions are intrinsics, not casts.
        FpToSiSat | FpToUiSat => LLVMFPToSI,
    }
}

fn ovf_name(op: OvfOp) -> &'static str {
    match op {
        OvfOp::SAdd => "llvm.sadd.with.overflow",
        OvfOp::SSub => "llvm.ssub.with.overflow",
        OvfOp::SMul => "llvm.smul.with.overflow",
    }
}

fn ipred(p: IPred) -> LLVMIntPredicate {
    use IPred::*;
    use LLVMIntPredicate::*;
    match p {
        Eq => LLVMIntEQ,
        Ne => LLVMIntNE,
        Slt => LLVMIntSLT,
        Sle => LLVMIntSLE,
        Sgt => LLVMIntSGT,
        Sge => LLVMIntSGE,
        Ult => LLVMIntULT,
        Ule => LLVMIntULE,
        Ugt => LLVMIntUGT,
        Uge => LLVMIntUGE,
    }
}

fn fpred(p: FPred) -> LLVMRealPredicate {
    use FPred::*;
    use LLVMRealPredicate::*;
    match p {
        Oeq => LLVMRealOEQ,
        One => LLVMRealONE,
        Olt => LLVMRealOLT,
        Ole => LLVMRealOLE,
        Ogt => LLVMRealOGT,
        Oge => LLVMRealOGE,
        Ord => LLVMRealORD,
        Ueq => LLVMRealUEQ,
        Une => LLVMRealUNE,
        Ult => LLVMRealULT,
        Ule => LLVMRealULE,
        Ugt => LLVMRealUGT,
        Uge => LLVMRealUGE,
        Uno => LLVMRealUNO,
    }
}

impl<'l, 'f> Fx<'l, 'f> {
    /// Every value-producing instruction that is not memory, a call or
    /// control flow.
    pub fn arith(&mut self, e: &'f Expr) -> Result<V> {
        let (b, n) = (self.b, NONAME.as_ptr());
        // SAFETY: operands were lowered in this function's context and
        // the checker guaranteed their types (spec/lir.md §6).
        unsafe {
            Ok(match &e.kind {
                Kind::Bin(op, x, y) => {
                    let (x, y) = (self.val(x)?, self.val(y)?);
                    LLVMBuildBinOp(b, bin_opcode(*op), x, y, n)
                }
                Kind::Un(UnOp::FNeg, x) => LLVMBuildFNeg(b, self.val(x)?, n),
                Kind::Un(UnOp::Ctpop, x) => {
                    let x = self.val(x)?;
                    self.intrinsic("llvm.ctpop", &[LLVMTypeOf(x)], &mut [x])
                }
                Kind::Overflow(op, x, y) => {
                    let (x, y) = (self.val(x)?, self.val(y)?);
                    self.intrinsic(ovf_name(*op), &[LLVMTypeOf(x)], &mut [x, y])
                }
                Kind::Cast(op @ (CastOp::FpToSiSat | CastOp::FpToUiSat), t, x) => {
                    let name = if *op == CastOp::FpToSiSat {
                        "llvm.fptosi.sat"
                    } else {
                        "llvm.fptoui.sat"
                    };
                    let x = self.val(x)?;
                    self.intrinsic(name, &[self.lx.ty(t), LLVMTypeOf(x)], &mut [x])
                }
                Kind::ICmp(p, x, y) => {
                    let (x, y) = (self.val(x)?, self.val(y)?);
                    LLVMBuildICmp(b, ipred(*p), x, y, n)
                }
                Kind::FCmp(p, x, y) => {
                    let (x, y) = (self.val(x)?, self.val(y)?);
                    LLVMBuildFCmp(b, fpred(*p), x, y, n)
                }
                Kind::Cast(op, t, x) => {
                    let x = self.val(x)?;
                    LLVMBuildCast(b, cast_opcode(*op), x, self.lx.ty(t), n)
                }
                Kind::Select(c, x, y) => {
                    let (c, x, y) = (self.val(c)?, self.val(x)?, self.val(y)?);
                    LLVMBuildSelect(b, c, x, y, n)
                }
                _ => return self.vector_aggregate(e),
            })
        }
    }

    fn vector_aggregate(&mut self, e: &'f Expr) -> Result<V> {
        let (b, n) = (self.b, NONAME.as_ptr());
        // SAFETY: as in `arith`.
        unsafe {
            Ok(match &e.kind {
                Kind::ExtractElement(v, i) => {
                    let (v, i) = (self.val(v)?, self.val(i)?);
                    LLVMBuildExtractElement(b, v, i, n)
                }
                Kind::InsertElement(v, x, i) => {
                    let (v, x, i) = (self.val(v)?, self.val(x)?, self.val(i)?);
                    LLVMBuildInsertElement(b, v, x, i, n)
                }
                Kind::Shuffle(x, y, m) => {
                    let (x, y, m) = (self.val(x)?, self.val(y)?, self.val(m)?);
                    LLVMBuildShuffleVector(b, x, y, m, n)
                }
                Kind::ExtractValue(a, idx) => {
                    let mut v = self.val(a)?;
                    for i in idx {
                        v = LLVMBuildExtractValue(b, v, *i as u32, n);
                    }
                    v
                }
                Kind::InsertValue(a, x, idx) => {
                    let (a, x) = (self.val(a)?, self.val(x)?);
                    self.insert_path(a, x, idx)
                }
                _ => return Err(Error::Internal(format!("unexpected form at {:?}", e.pos))),
            })
        }
    }

    /// `insertvalue` along a path of field indices.
    fn insert_path(&mut self, agg: LLVMValueRef, x: LLVMValueRef, idx: &[i128]) -> LLVMValueRef {
        let n = NONAME.as_ptr();
        match idx {
            [] => x,
            [i, rest @ ..] => {
                // SAFETY: the path was checked against the aggregate's type.
                unsafe {
                    let inner = LLVMBuildExtractValue(self.b, agg, *i as u32, n);
                    let inner = if rest.is_empty() {
                        x
                    } else {
                        self.insert_path(inner, x, rest)
                    };
                    LLVMBuildInsertValue(self.b, agg, inner, *i as u32, n)
                }
            }
        }
    }

    /// `(trap)`: `llvm.trap`.
    pub fn trap(&mut self) -> Result<()> {
        self.intrinsic("llvm.trap", &[], &mut []);
        Ok(())
    }

    /// Call an intrinsic overloaded on `overloads` with `args`.
    fn intrinsic(&mut self, name: &str, overloads: &[LLVMTypeRef], args: &mut [V]) -> V {
        let mut tys = overloads.to_vec();
        // SAFETY: the checker admitted only types the intrinsic exists for.
        unsafe {
            let id = LLVMLookupIntrinsicID(name.as_ptr().cast(), name.len());
            let n = tys.len();
            let f = LLVMGetIntrinsicDeclaration(self.lx.module, id, tys.as_mut_ptr(), n);
            let fty = LLVMIntrinsicGetType(self.lx.ctx, id, tys.as_mut_ptr(), n);
            LLVMBuildCall2(
                self.b,
                fty,
                f,
                args.as_mut_ptr(),
                args.len() as u32,
                NONAME.as_ptr(),
            )
        }
    }
}
