//! Expression lowering: the dispatch, names, struct values, let, phi.

use llvm_sys::core::{
    LLVMBuildInsertValue, LLVMBuildPhi, LLVMGetPoison, LLVMGetValueName2, LLVMIsConstant,
    LLVMSetValueName2, LLVMStructTypeInContext, LLVMTypeOf,
};
use llvm_sys::prelude::{LLVMTypeRef, LLVMValueRef};

use super::func::Fx;
use crate::error::{Error, Result};
use crate::llvm::NONAME;
use lir::ast::{Binding, Expr, Kind};

pub type V = LLVMValueRef;

/// Give an unnamed instruction the lIR name, for readable IR.
pub fn name_value(v: V, name: &str) {
    // SAFETY: v is a live value; names are copied by LLVM.
    unsafe {
        if LLVMIsConstant(v) != 0 {
            return;
        }
        let mut len = 0;
        LLVMGetValueName2(v, &mut len);
        if len == 0 {
            LLVMSetValueName2(v, name.as_ptr().cast(), name.len());
        }
    }
}

impl<'l, 'f> Fx<'l, 'f> {
    /// Lower `e`; `None` for void values and terminators.
    pub fn expr(&mut self, e: &'f Expr) -> Result<Option<V>> {
        let some = |v| Ok(Some(v));
        match &e.kind {
            Kind::Local(n) => match self.names.get(n.as_str()) {
                Some(v) => some(*v),
                None => Err(Error::Internal(format!("name {n} not lowered"))),
            },
            Kind::Global(_)
            | Kind::Int(..)
            | Kind::Float(..)
            | Kind::Null
            | Kind::Zero(_)
            | Kind::Vector(..) => some(self.lx.constant(e)?),
            Kind::Str(bytes) => some(self.lx.string(bytes)),
            Kind::Struct(name, fields) => some(self.struct_value(e, name, fields)?),
            Kind::Array(t, elems) => {
                let ty = self.lx.ty(t);
                some(self.aggregate_value(e, ty, elems)?)
            }
            Kind::Trap => self.trap().map(|_| None),
            Kind::Let(binds, body) => self.let_form(binds, body),
            Kind::Phi(t, inc) => {
                let ty = self.lx.ty(t);
                // SAFETY: the builder is positioned in the phi's block,
                // before any other instruction (checked).
                let phi = unsafe { LLVMBuildPhi(self.b, ty, NONAME.as_ptr()) };
                self.phis.push((phi, inc));
                some(phi)
            }
            Kind::Call { .. } => self.call(e),
            Kind::Ret(_)
            | Kind::Br(_)
            | Kind::CondBr(..)
            | Kind::Switch(..)
            | Kind::Unreachable => self.control(e).map(|_| None),
            Kind::Alloca { .. }
            | Kind::Load { .. }
            | Kind::Store { .. }
            | Kind::Gep { .. }
            | Kind::AtomicLoad(..)
            | Kind::AtomicStore(..)
            | Kind::AtomicRmw(..)
            | Kind::CmpXchg { .. }
            | Kind::Fence(..) => self.memory(e),
            _ => self.arith(e).map(Some),
        }
    }

    /// A value operand.
    pub fn val(&mut self, e: &'f Expr) -> Result<V> {
        self.expr(e)?
            .ok_or_else(|| Error::Internal(format!("void value at {:?}", e.pos)))
    }

    fn let_form(&mut self, binds: &'f [Binding], body: &'f [Expr]) -> Result<Option<V>> {
        for b in binds {
            let v = self.val(&b.value)?;
            name_value(v, &b.name);
            self.names.insert(&b.name, v);
        }
        let mut last = None;
        for e in body {
            last = self.expr(e)?;
        }
        Ok(last)
    }

    fn struct_value(
        &mut self,
        e: &'f Expr,
        name: &Option<String>,
        fields: &'f [Expr],
    ) -> Result<V> {
        if let Some(n) = name {
            let ty = self.lx.structs[n];
            return self.aggregate_value(e, ty, fields);
        }
        if fields.iter().all(lir::check::is_constant) {
            return self.lx.constant(e);
        }
        let vals = fields
            .iter()
            .map(|f| self.val(f))
            .collect::<Result<Vec<_>>>()?;
        // SAFETY: the values belong to this context.
        let mut ts: Vec<LLVMTypeRef> = vals.iter().map(|v| unsafe { LLVMTypeOf(*v) }).collect();
        let ty =
            unsafe { LLVMStructTypeInContext(self.lx.ctx, ts.as_mut_ptr(), ts.len() as u32, 0) };
        Ok(self.insert_all(ty, vals))
    }

    /// A named struct or array value: a constant, or `insertvalue`s of
    /// its elements into a value of type `ty`.
    fn aggregate_value(&mut self, e: &'f Expr, ty: LLVMTypeRef, elems: &'f [Expr]) -> Result<V> {
        if elems.iter().all(lir::check::is_constant) {
            return self.lx.constant(e);
        }
        let vals = elems
            .iter()
            .map(|f| self.val(f))
            .collect::<Result<Vec<_>>>()?;
        Ok(self.insert_all(ty, vals))
    }

    fn insert_all(&mut self, ty: LLVMTypeRef, vals: Vec<V>) -> V {
        // SAFETY: every element is overwritten, so the poison start is
        // never observed.
        let mut agg = unsafe { LLVMGetPoison(ty) };
        for (i, v) in vals.into_iter().enumerate() {
            agg = unsafe { LLVMBuildInsertValue(self.b, agg, v, i as u32, NONAME.as_ptr()) };
        }
        agg
    }
}
