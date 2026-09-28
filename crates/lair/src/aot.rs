//! Ahead-of-time compilation: object files, assembly, LLVM IR and
//! executables linked with `cc` (spec/lir.md §11).

use std::path::Path;
use std::process::Command;
use std::ptr;

use llvm_sys::core::{LLVMDisposeMemoryBuffer, LLVMGetBufferSize, LLVMGetBufferStart};
use llvm_sys::target_machine::{LLVMCodeGenFileType, LLVMTargetMachineEmitToMemoryBuffer};

use crate::error::{Error, Result};
use crate::llvm::passes::optimize;
use crate::llvm::take_message;
use crate::llvm::target::Machine;
use crate::lower::lower;
use lir::Module;

/// What [`emit`] produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Object,
    Assembly,
    LlvmIr,
}

/// AOT settings.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// 0 to 3: the IR pipeline `default<On>` and the code generator level.
    pub opt_level: u8,
    /// Libraries to link (`-l`).
    pub libs: Vec<String>,
}

/// Compile a checked module for the host.
pub fn emit(m: &Module, name: &str, out: Output, opts: &Options) -> Result<Vec<u8>> {
    let host = Machine::host(opts.opt_level)?;
    let owned = lower(m, name, &host.triple, &host.data_layout)?;
    // SAFETY: the module and target machine are alive; the buffer is
    // copied and disposed.
    unsafe {
        optimize(owned.module(), host.tm, opts.opt_level)?;
        let kind = match out {
            Output::LlvmIr => return Ok(owned.ir().into_bytes()),
            Output::Object => LLVMCodeGenFileType::LLVMObjectFile,
            Output::Assembly => LLVMCodeGenFileType::LLVMAssemblyFile,
        };
        let mut msg = ptr::null_mut();
        let mut buf = ptr::null_mut();
        if LLVMTargetMachineEmitToMemoryBuffer(host.tm, owned.module(), kind, &mut msg, &mut buf)
            != 0
        {
            return Err(Error::Backend(format!(
                "code generation failed: {}",
                take_message(msg)
            )));
        }
        let bytes = std::slice::from_raw_parts(
            LLVMGetBufferStart(buf).cast::<u8>(),
            LLVMGetBufferSize(buf),
        )
        .to_vec();
        LLVMDisposeMemoryBuffer(buf);
        Ok(bytes)
    }
}

/// Compile a module that satisfies the `main` rule and link it with
/// `cc` into an executable at `path`.
pub fn build_executable(m: &Module, name: &str, path: &Path, opts: &Options) -> Result<()> {
    lir::check_main(m)?;
    let obj = emit(m, name, Output::Object, opts)?;
    let obj_path = path.with_file_name(format!(
        "{}.lair.o",
        path.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::write(&obj_path, obj)
        .map_err(|e| Error::Backend(format!("cannot write {}: {e}", obj_path.display())))?;
    let mut cmd = Command::new("cc");
    cmd.arg(&obj_path).arg("-o").arg(path);
    // C links libm only on request; LLVM lowers frem to fmod.
    cmd.arg("-lm");
    for l in &opts.libs {
        cmd.arg(format!("-l{l}"));
    }
    let out = cmd.output();
    let _ = std::fs::remove_file(&obj_path);
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(Error::Backend(format!(
            "linker failed: {}",
            String::from_utf8_lossy(&o.stderr)
        ))),
        Err(e) => Err(Error::Backend(format!("cannot run cc: {e}"))),
    }
}
