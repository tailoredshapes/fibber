//! Lowering a checked lIR module to an LLVM module (spec/lir.md §10:
//! the checker has run; LLVM's verifier runs after).

mod arith;
mod calls;
mod consts;
mod expr;
mod func;
mod memory;
mod types;

use std::collections::HashMap;
use std::ptr;

use llvm_sys::analysis::{LLVMVerifierFailureAction, LLVMVerifyModule};
use llvm_sys::core::{
    LLVMAddFunction, LLVMAddGlobal, LLVMSetFunctionCallConv, LLVMSetGlobalConstant,
    LLVMSetInitializer, LLVMStructCreateNamed, LLVMStructSetBody,
};
use llvm_sys::prelude::{LLVMContextRef, LLVMModuleRef, LLVMTypeRef, LLVMValueRef};
use llvm_sys::target::{LLVMCreateTargetData, LLVMDisposeTargetData, LLVMTargetDataRef};

use crate::error::{Error, Result};
use crate::llvm::{cstr, take_message, Owned};
use lir::ast::{Item, Module};
use lir::types::{Cc, FnType};

/// LLVM's number for `tailcc` (`CallingConv::Tail`).
pub const TAILCC: u32 = 18;

pub fn cc_number(cc: Cc) -> u32 {
    match cc {
        Cc::C => 0,
        Cc::Tail => TAILCC,
    }
}

/// Module-level lowering state.
pub struct Lx {
    pub ctx: LLVMContextRef,
    pub module: LLVMModuleRef,
    pub td: LLVMTargetDataRef,
    pub structs: HashMap<String, LLVMTypeRef>,
    pub struct_fields: HashMap<String, Vec<lir::Type>>,
    pub funcs: HashMap<String, (LLVMValueRef, FnType)>,
    pub globals: HashMap<String, LLVMValueRef>,
}

/// Lower a module that `lir::check` accepted, for the given triple and
/// data layout, and run the LLVM verifier on the result.
pub fn lower(m: &Module, name: &str, triple: &str, data_layout: &str) -> Result<Owned> {
    let owned = Owned::new(name, triple, data_layout);
    let dl = cstr(data_layout);
    // SAFETY: the target data is disposed below, after its last use.
    let td = unsafe { LLVMCreateTargetData(dl.as_ptr()) };
    let mut lx = Lx {
        ctx: owned.ctx(),
        module: owned.module(),
        td,
        structs: HashMap::new(),
        struct_fields: HashMap::new(),
        funcs: HashMap::new(),
        globals: HashMap::new(),
    };
    let r = lx.module_items(m);
    // SAFETY: created above; nothing refers to it any more.
    unsafe { LLVMDisposeTargetData(td) };
    r?;
    verify(&owned)?;
    Ok(owned)
}

fn verify(owned: &Owned) -> Result<()> {
    let mut msg = ptr::null_mut();
    // SAFETY: the module is alive; the message is ours to dispose.
    let failed = unsafe {
        LLVMVerifyModule(
            owned.module(),
            LLVMVerifierFailureAction::LLVMReturnStatusAction,
            &mut msg,
        )
    };
    let text = unsafe { take_message(msg) };
    if failed != 0 {
        return Err(Error::Internal(text));
    }
    Ok(())
}

impl Lx {
    fn module_items(&mut self, m: &Module) -> Result<()> {
        self.declare_structs(m);
        for item in &m.items {
            match item {
                Item::Declare(d) => self.add_function(&d.name, &d.ty),
                Item::Define(f) => self.add_function(&f.name, &f.ty),
                Item::Global(g) => {
                    let t = self.ty(&g.ty);
                    let n = cstr(&g.name);
                    // SAFETY: module and type are alive.
                    let gv = unsafe { LLVMAddGlobal(self.module, t, n.as_ptr()) };
                    self.globals.insert(g.name.clone(), gv);
                }
                Item::Struct(_) => {}
            }
        }
        for item in &m.items {
            if let Item::Global(g) = item {
                let init = self.constant(&g.init)?;
                let gv = self.globals[&g.name];
                // SAFETY: gv and init belong to this module's context.
                unsafe {
                    LLVMSetInitializer(gv, init);
                    LLVMSetGlobalConstant(gv, i32::from(g.constant));
                }
            }
        }
        for item in &m.items {
            if let Item::Define(f) = item {
                self.function(f)?;
            }
        }
        Ok(())
    }

    fn declare_structs(&mut self, m: &Module) {
        for item in &m.items {
            if let Item::Struct(s) = item {
                let n = cstr(&format!("struct.{}", s.name));
                // SAFETY: the context is alive.
                let t = unsafe { LLVMStructCreateNamed(self.ctx, n.as_ptr()) };
                self.structs.insert(s.name.clone(), t);
                self.struct_fields.insert(s.name.clone(), s.fields.clone());
            }
        }
        for item in &m.items {
            if let Item::Struct(s) = item {
                let mut fields: Vec<LLVMTypeRef> = s.fields.iter().map(|f| self.ty(f)).collect();
                // SAFETY: every field type exists in this context.
                unsafe {
                    LLVMStructSetBody(
                        self.structs[&s.name],
                        fields.as_mut_ptr(),
                        fields.len() as u32,
                        0,
                    )
                };
            }
        }
    }

    fn add_function(&mut self, name: &str, ty: &FnType) {
        let fty = self.fn_ty(ty);
        let n = cstr(name);
        // SAFETY: module and type are alive.
        let f = unsafe { LLVMAddFunction(self.module, n.as_ptr(), fty) };
        unsafe { LLVMSetFunctionCallConv(f, cc_number(ty.cc)) };
        self.funcs.insert(name.to_string(), (f, ty.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llvm::target::Machine;

    /// The LLVM verifier is the backstop behind lIR's checker: a module
    /// that skipped the checker and is invalid is an internal error,
    /// not a crash or silently wrong code.
    #[test]
    fn the_llvm_verifier_fires_on_an_unchecked_invalid_module() {
        let src = "(define (main i32) () (block entry (ret (i64 3))))";
        let m = lir::parse(src).unwrap();
        assert!(lir::check(&m).is_err(), "the checker rejects it");
        let host = Machine::host(0).unwrap();
        match lower(&m, "t", &host.triple, &host.data_layout) {
            Err(Error::Internal(msg)) => assert!(msg.contains("return type"), "{msg}"),
            Err(e) => panic!("{e}"),
            Ok(_) => panic!("the verifier accepted an invalid module"),
        }
    }
}
