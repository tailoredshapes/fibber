//! The host target: initialisation, target machines, data layouts.

use std::ffi::CString;
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
            // FIB_TARGET_CPU names the CPU to generate code for instead of the host's, with no extra features: a release
            // is built for a baseline such as `x86-64-v2` so that it runs on machines other than the one that built it.
            let requested = requested_cpu(std::env::var("FIB_TARGET_CPU").ok());
            let (cpu, features) = match &requested {
                Some(name) => match (CString::new(name.as_str()), CString::new("")) {
                    (Ok(c), Ok(f)) => (Chosen::Named(c), Chosen::Named(f)),
                    _ => {
                        take_message(triple_c);
                        return Err(Error::Backend(format!(
                            "FIB_TARGET_CPU is not a CPU name: {name:?}"
                        )));
                    }
                },
                None => (
                    Chosen::Host(LLVMGetHostCPUName()),
                    Chosen::Host(LLVMGetHostCPUFeatures()),
                ),
            };
            let tm = LLVMCreateTargetMachine(
                target,
                triple_c,
                cpu.as_ptr(),
                features.as_ptr(),
                level,
                LLVMRelocMode::LLVMRelocPIC,
                LLVMCodeModel::LLVMCodeModelDefault,
            );
            cpu.dispose();
            features.dispose();
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

/// The CPU a build is asked for through FIB_TARGET_CPU: none for an unset, empty or `host` value.
fn requested_cpu(var: Option<String>) -> Option<String> {
    var.filter(|v| !v.is_empty() && v != "host")
}

/// A CPU name or feature string: LLVM's own (to dispose) or ours.
enum Chosen {
    Host(*mut std::os::raw::c_char),
    Named(CString),
}

impl Chosen {
    fn as_ptr(&self) -> *const std::os::raw::c_char {
        match self {
            Chosen::Host(p) => *p,
            Chosen::Named(c) => c.as_ptr(),
        }
    }

    /// Disposes LLVM's string; ours is dropped.
    fn dispose(self) {
        if let Chosen::Host(p) = self {
            // SAFETY: `p` came from LLVMGetHostCPUName/Features and is disposed once.
            unsafe { take_message(p) };
        }
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        // SAFETY: created in `host`, owned here.
        unsafe { LLVMDisposeTargetMachine(self.tm) }
    }
}

#[cfg(test)]
mod tests {
    use super::requested_cpu;

    #[test]
    fn an_unset_empty_or_host_request_asks_for_the_host() {
        assert_eq!(requested_cpu(None), None);
        assert_eq!(requested_cpu(Some(String::new())), None);
        assert_eq!(requested_cpu(Some("host".into())), None);
    }

    #[test]
    fn a_named_cpu_is_taken_as_it_is() {
        assert_eq!(
            requested_cpu(Some("x86-64-v2".into())),
            Some("x86-64-v2".into())
        );
    }
}
