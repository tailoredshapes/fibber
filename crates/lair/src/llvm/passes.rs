//! LLVM's new pass manager: the `default<On>` pipelines.

use llvm_sys::error::{LLVMDisposeErrorMessage, LLVMErrorRef, LLVMGetErrorMessage};
use llvm_sys::prelude::LLVMModuleRef;
use llvm_sys::target_machine::LLVMTargetMachineRef;
use llvm_sys::transforms::pass_builder::{
    LLVMCreatePassBuilderOptions, LLVMDisposePassBuilderOptions, LLVMRunPasses,
};

use super::cstr;
use crate::error::{Error, Result};

/// The message of an LLVM error, consuming it; `None` for success.
///
/// # Safety
/// `e` is null or an error LLVM returned to the caller.
pub unsafe fn error_message(e: LLVMErrorRef) -> Option<String> {
    if e.is_null() {
        return None;
    }
    let m = LLVMGetErrorMessage(e);
    let s = std::ffi::CStr::from_ptr(m).to_string_lossy().into_owned();
    LLVMDisposeErrorMessage(m);
    Some(s)
}

/// Run `default<On>` over the module; level 0 runs nothing.
///
/// # Safety
/// `module` and `tm` are alive (`tm` may be null).
pub unsafe fn optimize(module: LLVMModuleRef, tm: LLVMTargetMachineRef, level: u8) -> Result<()> {
    if level == 0 {
        return Ok(());
    }
    let passes = cstr(&format!("default<O{}>", level.min(3)));
    let opts = LLVMCreatePassBuilderOptions();
    let e = LLVMRunPasses(module, passes.as_ptr(), tm, opts);
    LLVMDisposePassBuilderOptions(opts);
    match error_message(e) {
        None => Ok(()),
        Some(m) => Err(Error::Backend(format!("optimisation failed: {m}"))),
    }
}
