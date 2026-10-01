//! Ahead-of-time compilation: object files, assembly, LLVM IR and
//! executables linked with `cc` (spec/lir.md §11).

mod link;

use std::path::Path;
use std::ptr;

use llvm_sys::core::{LLVMDisposeMemoryBuffer, LLVMGetBufferSize, LLVMGetBufferStart};
use llvm_sys::target_machine::{LLVMCodeGenFileType, LLVMTargetMachineEmitToMemoryBuffer};

use crate::error::{Error, Result};
use crate::llvm::passes::optimize;
use crate::llvm::take_message;
use crate::llvm::target::Machine;
use crate::lower::lower;
use lir::Module;

pub use link::library_dir;

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
    /// Libraries to link (`-l`), after the module's own code.
    pub libs: Vec<String>,
    /// Directories that hold those libraries (`-L`). Each is also an
    /// rpath, an absolute one ([`library_dir`] says how a name becomes
    /// it), so the executable finds its libraries without
    /// `LD_LIBRARY_PATH`. Used by [`build_executable`] only.
    pub lib_dirs: Vec<String>,
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
/// `cc` into an executable at `path`, against libm, libpthread, and
/// `opts.libs` searched for in `opts.lib_dirs`. A directory that is
/// missing, is no directory or cannot be an rpath is an error before
/// anything is compiled.
pub fn build_executable(m: &Module, name: &str, path: &Path, opts: &Options) -> Result<()> {
    lir::check_main(m)?;
    let dirs = opts
        .lib_dirs
        .iter()
        .map(|d| library_dir(d).map_err(Error::Backend))
        .collect::<Result<Vec<_>>>()?;
    let obj = emit(m, name, Output::Object, opts)?;
    link::link(&obj, path, &dirs, &opts.libs)
}
