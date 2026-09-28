//! Thin ownership wrappers over the LLVM C API. Every `unsafe` block of
//! the crate that is not lowering an instruction lives here or in
//! `jit.rs` and `aot.rs`.

pub mod passes;
pub mod target;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use llvm_sys::core::{
    LLVMContextCreate, LLVMContextDispose, LLVMDisposeMessage, LLVMDisposeModule,
    LLVMModuleCreateWithNameInContext, LLVMPrintModuleToString, LLVMSetDataLayout, LLVMSetTarget,
};
use llvm_sys::prelude::{LLVMContextRef, LLVMModuleRef};

/// A C string for LLVM; interior NULs (impossible in lIR names, which
/// are atoms) are replaced rather than panicking.
pub fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', "?")).unwrap_or_default()
}

/// The empty name LLVM takes for unnamed values.
pub const NONAME: &CStr = c"";

/// Take an LLVM-allocated message and free it.
///
/// # Safety
/// `msg` is null or a string LLVM allocated for the caller to dispose.
pub unsafe fn take_message(msg: *mut c_char) -> String {
    if msg.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(msg).to_string_lossy().into_owned();
    LLVMDisposeMessage(msg);
    s
}

/// An LLVM context with one module in it, disposed together. The
/// module may be released to a JIT with [`Owned::into_raw`].
pub struct Owned {
    ctx: LLVMContextRef,
    module: LLVMModuleRef,
}

impl Owned {
    /// A fresh context and an empty module with the given triple and
    /// data layout.
    pub fn new(name: &str, triple: &str, data_layout: &str) -> Owned {
        let n = cstr(name);
        let (t, dl) = (cstr(triple), cstr(data_layout));
        // SAFETY: the context is fresh and owned by the returned value;
        // the strings outlive the calls, which copy them.
        unsafe {
            let ctx = LLVMContextCreate();
            let module = LLVMModuleCreateWithNameInContext(n.as_ptr(), ctx);
            LLVMSetTarget(module, t.as_ptr());
            LLVMSetDataLayout(module, dl.as_ptr());
            Owned { ctx, module }
        }
    }

    pub fn ctx(&self) -> LLVMContextRef {
        self.ctx
    }

    pub fn module(&self) -> LLVMModuleRef {
        self.module
    }

    /// The module as LLVM IR text.
    pub fn ir(&self) -> String {
        // SAFETY: the module is alive; the string is ours to dispose.
        unsafe { take_message(LLVMPrintModuleToString(self.module)) }
    }

    /// Give up ownership: the caller disposes (or hands to the JIT)
    /// the context and the module.
    pub fn into_raw(self) -> (LLVMContextRef, LLVMModuleRef) {
        let parts = (self.ctx, self.module);
        std::mem::forget(self);
        parts
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: both were created in `new` and are still owned here.
        unsafe {
            LLVMDisposeModule(self.module);
            LLVMContextDispose(self.ctx);
        }
    }
}
