//! The host target: initialisation, target machines, data layouts.

use std::ptr;
use std::sync::Once;

use llvm_sys::target::{
    LLVMCopyStringRepOfTargetData, LLVM_InitializeNativeAsmParser, LLVM_InitializeNativeAsmPrinter,
    LLVM_InitializeNativeTarget,
};
use llvm_sys::target_machine::{
    LLVMCodeGenOptLevel, LLVMCodeModel, LLVMCreateTargetDataLayout, LLVMCreateTargetMachine,
    LLVMDisposeTargetMachine, LLVMGetDefaultTargetTriple, LLVMGetHostCPUFeatures,
    LLVMGetHostCPUName, LLVMGetTargetFromTriple, LLVMRelocMode, LLVMTargetMachineRef,
};

use super::take_message;
use crate::error::{Error, Result};

static INIT: Once = Once::new();

/// Initialise LLVM's native target once per process (LLVM's own
/// registry is process-global).
pub fn init() {
    // SAFETY: the initialisers are idempotent and run once.
    INIT.call_once(|| unsafe {
        LLVM_InitializeNativeTarget();
        LLVM_InitializeNativeAsmPrinter();
        LLVM_InitializeNativeAsmParser();
    });
}

/// A target machine for the host, disposed on drop.
pub struct Machine {
    pub tm: LLVMTargetMachineRef,
    pub triple: String,
    pub data_layout: String,
}

impl Machine {
    /// The host machine at code generation level `opt` (0 to 3),
    /// position-independent so that `cc` can link a PIE.
    pub fn host(opt: u8) -> Result<Machine> {
        init();
        let level = match opt {
            0 => LLVMCodeGenOptLevel::LLVMCodeGenLevelNone,
            1 => LLVMCodeGenOptLevel::LLVMCodeGenLevelLess,
            2 => LLVMCodeGenOptLevel::LLVMCodeGenLevelDefault,
            _ => LLVMCodeGenOptLevel::LLVMCodeGenLevelAggressive,
        };
        // SAFETY: every string LLVM returns is disposed by take_message;
        // the target machine is owned by the returned value.
        unsafe {
            let triple_c = LLVMGetDefaultTargetTriple();
            let mut target = ptr::null_mut();
            let mut msg = ptr::null_mut();
            if LLVMGetTargetFromTriple(triple_c, &mut target, &mut msg) != 0 {
                let m = take_message(msg);
                take_message(triple_c);
                return Err(Error::Backend(format!("no target for the host: {m}")));
            }
            let cpu = LLVMGetHostCPUName();
            let features = LLVMGetHostCPUFeatures();
            let tm = LLVMCreateTargetMachine(
                target,
                triple_c,
                cpu,
                features,
                level,
                LLVMRelocMode::LLVMRelocPIC,
                LLVMCodeModel::LLVMCodeModelDefault,
            );
            take_message(cpu);
            take_message(features);
            let triple = take_message(triple_c);
            let td = LLVMCreateTargetDataLayout(tm);
            let data_layout = take_message(LLVMCopyStringRepOfTargetData(td));
            llvm_sys::target::LLVMDisposeTargetData(td);
            Ok(Machine {
                tm,
                triple,
                data_layout,
            })
        }
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        // SAFETY: created in `host`, owned here.
        unsafe { LLVMDisposeTargetMachine(self.tm) }
    }
}
