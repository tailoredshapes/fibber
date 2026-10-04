//! The JIT: compile lIR modules in-process and call their functions
//! (spec/lir.md §11).

mod names;
mod trampoline;

use std::ffi::CStr;
use std::ptr;

use llvm_sys::orc2::lljit::{
    LLVMOrcCreateLLJIT, LLVMOrcCreateLLJITBuilder, LLVMOrcDisposeLLJIT,
    LLVMOrcLLJITAddLLVMIRModule, LLVMOrcLLJITBuilderSetJITTargetMachineBuilder,
    LLVMOrcLLJITGetDataLayoutStr, LLVMOrcLLJITGetGlobalPrefix, LLVMOrcLLJITGetMainJITDylib,
    LLVMOrcLLJITGetTripleString, LLVMOrcLLJITLookup, LLVMOrcLLJITRef,
};
use llvm_sys::orc2::{
    LLVMOrcCreateDynamicLibrarySearchGeneratorForProcess, LLVMOrcCreateNewThreadSafeModule,
    LLVMOrcDisposeThreadSafeContext, LLVMOrcJITDylibAddGenerator,
    LLVMOrcJITTargetMachineBuilderCreateFromTargetMachine, LLVMOrcThreadSafeContextRef,
};
use llvm_sys::prelude::LLVMContextRef;
use llvm_sys::support::{LLVMLoadLibraryPermanently, LLVMSearchForAddressOfSymbol};

use crate::error::{Error, Result};
use crate::llvm::passes::{error_message, optimize};
use crate::llvm::target::Machine;
use crate::llvm::{cstr, target};
use crate::lower::lower;
use lir::ast::Module;
use lir::{Cc, FnType};
use names::Names;

extern "C" {
    // In LLVM 21's C API (llvm-c/Orc.h), not yet bound by llvm-sys 211.
    fn LLVMOrcCreateNewThreadSafeContextFromLLVMContext(
        ctx: LLVMContextRef,
    ) -> LLVMOrcThreadSafeContextRef;
}

/// JIT settings.
#[derive(Clone, Copy, Debug, Default)]
pub struct JitOptions {
    /// 0 runs no IR optimisation; 1 to 3 run `default<On>`.
    pub opt_level: u8,
}

/// A JIT session: modules added to it see each other's functions.
/// Function pointers it returns are valid until it is dropped.
pub struct Jit {
    jit: LLVMOrcLLJITRef,
    triple: String,
    data_layout: String,
    opts: JitOptions,
    /// What every module so far defined and exported.
    names: Names,
}

impl Jit {
    pub fn new(opts: JitOptions) -> Result<Jit> {
        target::init();
        let mut jit = ptr::null_mut();
        let builder = Self::builder(opts)?;
        // SAFETY: an LLJIT from the builder (which it consumes); its strings
        // are copied before use and the generator is owned by the JITDylib
        // once added.
        unsafe {
            if let Some(m) = error_message(LLVMOrcCreateLLJIT(&mut jit, builder)) {
                return Err(Error::Jit(m));
            }
            let jd = LLVMOrcLLJITGetMainJITDylib(jit);
            let mut gen = ptr::null_mut();
            let prefix = LLVMOrcLLJITGetGlobalPrefix(jit);
            let e = LLVMOrcCreateDynamicLibrarySearchGeneratorForProcess(
                &mut gen,
                prefix,
                None,
                ptr::null_mut(),
            );
            if let Some(m) = error_message(e) {
                LLVMOrcDisposeLLJIT(jit);
                return Err(Error::Jit(m));
            }
            LLVMOrcJITDylibAddGenerator(jd, gen);
            let triple = CStr::from_ptr(LLVMOrcLLJITGetTripleString(jit))
                .to_string_lossy()
                .into_owned();
            let data_layout = CStr::from_ptr(LLVMOrcLLJITGetDataLayoutStr(jit))
                .to_string_lossy()
                .into_owned();
            Ok(Jit {
                jit,
                triple,
                data_layout,
                opts,
                names: Names::default(),
            })
        }
    }

    /// The LLJIT builder for `opts`: null (LLVM's defaults, whose code
    /// generator runs at its default level) except at optimisation level 0,
    /// where the code generator runs at level None (FastISel, no DAG
    /// scheduling), so that a module is compiled fast: this is the path of
    /// `run`, of the macro runner and of the `def` evaluator, which compile
    /// large modules of which little runs.
    fn builder(opts: JitOptions) -> Result<llvm_sys::orc2::lljit::LLVMOrcLLJITBuilderRef> {
        if opts.opt_level != 0 {
            return Ok(ptr::null_mut());
        }
        let machine = Machine::host(0)?;
        // SAFETY: the JITTargetMachineBuilder takes the target machine, so
        // `machine` must not dispose it; the LLJIT builder takes the
        // JITTargetMachineBuilder, and `LLVMOrcCreateLLJIT` the builder.
        unsafe {
            let machine = std::mem::ManuallyDrop::new(machine);
            let jtmb = LLVMOrcJITTargetMachineBuilderCreateFromTargetMachine(machine.tm);
            let builder = LLVMOrcCreateLLJITBuilder();
            LLVMOrcLLJITBuilderSetJITTargetMachineBuilder(builder, jtmb);
            Ok(builder)
        }
    }

    /// Parse, check, lower, verify and add a module.
    pub fn add_source(&mut self, name: &str, src: &str) -> Result<()> {
        let m = crate::check_source(src)?;
        self.add_module(name, &m)
    }

    /// Add a module `lir::check` accepted.
    pub fn add_module(&mut self, name: &str, m: &Module) -> Result<()> {
        lir::check(m).map_err(Error::Invalid)?;
        self.names.check(m, in_process)?;
        let owned = lower(m, name, &self.triple, &self.data_layout)?;
        // SAFETY: the module is verified; ownership of its context and
        // module passes to the JIT through the thread-safe wrappers.
        unsafe {
            optimize(owned.module(), ptr::null_mut(), self.opts.opt_level)?;
            let (ctx, module) = owned.into_raw();
            let tsc = LLVMOrcCreateNewThreadSafeContextFromLLVMContext(ctx);
            let tsm = LLVMOrcCreateNewThreadSafeModule(module, tsc);
            LLVMOrcDisposeThreadSafeContext(tsc);
            let jd = LLVMOrcLLJITGetMainJITDylib(self.jit);
            if let Some(msg) = error_message(LLVMOrcLLJITAddLLVMIRModule(self.jit, jd, tsm)) {
                return Err(Error::Jit(msg));
            }
        }
        self.names.record(name, m);
        Ok(())
    }

    /// The lIR type of an exported function defined in this JIT.
    pub fn signature(&self, name: &str) -> Result<FnType> {
        self.names.function(name)
    }

    /// The address of a `ccc` entry to `name`: the function itself when
    /// it is `ccc`, else a trampoline this `Jit` generates and compiles
    /// once (spec/lir.md §11).
    pub fn c_entry(&mut self, name: &str) -> Result<usize> {
        let ty = self.signature(name)?;
        if ty.cc == Cc::C {
            return self.address(name);
        }
        let tramp = trampoline::name_of(name);
        if self.signature(&tramp).is_err() {
            self.add_source(&tramp, &trampoline::source(name, &ty))?;
        }
        self.address(&tramp)
    }

    /// The address of a defined function, compiling it if needed.
    pub fn address(&self, name: &str) -> Result<usize> {
        self.signature(name)?;
        let n = cstr(name);
        let mut addr = 0u64;
        // SAFETY: the JIT is alive; lookup materialises the symbol.
        let e = unsafe { LLVMOrcLLJITLookup(self.jit, &mut addr, n.as_ptr()) };
        // SAFETY: e is the error LLVMOrcLLJITLookup returned.
        if let Some(m) = unsafe { error_message(e) } {
            return Err(Error::Jit(m));
        }
        Ok(addr as usize)
    }

    /// A defined function as the function pointer type `F`; for a
    /// `tailcc` function, its `ccc` trampoline ([`Jit::c_entry`]).
    ///
    /// # Safety
    /// `F` must be an `extern "C"` function pointer type matching the
    /// parameters and result of the lIR type [`Jit::signature`]
    /// reports, and must not be called after the `Jit` is dropped.
    pub unsafe fn function<F: Copy>(&mut self, name: &str) -> Result<F> {
        if std::mem::size_of::<F>() != std::mem::size_of::<usize>() {
            return Err(Error::Jit("F is not a function pointer type".into()));
        }
        let addr = self.c_entry(name)?;
        Ok(std::mem::transmute_copy::<usize, F>(&addr))
    }
}

/// Whether the host process (the C library and what it loaded)
/// defines `name`.
fn in_process(name: &str) -> bool {
    let n = cstr(name);
    // SAFETY: loading "no library" makes the process's own symbols
    // searchable; the name outlives the search.
    unsafe {
        LLVMLoadLibraryPermanently(ptr::null());
        !LLVMSearchForAddressOfSymbol(n.as_ptr()).is_null()
    }
}

impl Drop for Jit {
    fn drop(&mut self) {
        // SAFETY: created in `new`; disposing frees all JIT'd code.
        unsafe {
            error_message(LLVMOrcDisposeLLJIT(self.jit));
        }
    }
}
