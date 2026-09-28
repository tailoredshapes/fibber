//! The JIT: compile lIR modules in-process and call their functions
//! (spec/lir.md §11).

use std::collections::HashMap;
use std::ffi::CStr;
use std::ptr;

use llvm_sys::orc2::lljit::{
    LLVMOrcCreateLLJIT, LLVMOrcDisposeLLJIT, LLVMOrcLLJITAddLLVMIRModule,
    LLVMOrcLLJITGetDataLayoutStr, LLVMOrcLLJITGetGlobalPrefix, LLVMOrcLLJITGetMainJITDylib,
    LLVMOrcLLJITGetTripleString, LLVMOrcLLJITLookup, LLVMOrcLLJITRef,
};
use llvm_sys::orc2::{
    LLVMOrcCreateDynamicLibrarySearchGeneratorForProcess, LLVMOrcCreateNewThreadSafeModule,
    LLVMOrcDisposeThreadSafeContext, LLVMOrcJITDylibAddGenerator, LLVMOrcThreadSafeContextRef,
};
use llvm_sys::prelude::LLVMContextRef;
use llvm_sys::support::{LLVMLoadLibraryPermanently, LLVMSearchForAddressOfSymbol};

use crate::error::{Error, Result};
use crate::llvm::passes::{error_message, optimize};
use crate::llvm::{cstr, target};
use crate::lower::lower;
use lir::ast::{Item, Module};
use lir::{Diagnostic, FnType};

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
    /// Every function defined so far: its type and module.
    defined: HashMap<String, (FnType, String)>,
    /// Every global variable defined so far and its module.
    globals: HashMap<String, String>,
}

impl Jit {
    pub fn new(opts: JitOptions) -> Result<Jit> {
        target::init();
        let mut jit = ptr::null_mut();
        // SAFETY: a default LLJIT; its strings are copied before use and
        // the generator is owned by the JITDylib once added.
        unsafe {
            if let Some(m) = error_message(LLVMOrcCreateLLJIT(&mut jit, ptr::null_mut())) {
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
                defined: HashMap::new(),
                globals: HashMap::new(),
            })
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
        self.against_earlier(m)?;
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
        self.record(name, m);
        Ok(())
    }

    /// Cross-module rules (spec/lir.md §11): no second definition, and a
    /// declaration matches an earlier definition.
    fn against_earlier(&self, m: &Module) -> Result<()> {
        for item in &m.items {
            let (name, pos, decl) = match item {
                Item::Define(f) => (&f.name, f.pos, None),
                Item::Global(g) => (&g.name, g.pos, None),
                Item::Declare(d) => (&d.name, d.pos, Some(&d.ty)),
                Item::Struct(_) => continue,
            };
            let first = self
                .defined
                .get(name)
                .map(|(_, m)| m)
                .or(self.globals.get(name));
            match (first, decl) {
                (Some(first), None) => {
                    return Err(Diagnostic::new(
                        pos,
                        format!("duplicate definition of @{name} (first in module {first})"),
                    )
                    .into())
                }
                (None, Some(_)) => {
                    if !in_process(name) {
                        return Err(Diagnostic::new(pos, format!(
                            "undefined symbol @{name}: defined by no module of this JIT and not in the process"
                        ))
                        .into());
                    }
                }
                (Some(_), Some(ty)) => {
                    if let Some((def, module)) = self.defined.get(name) {
                        if def != ty {
                            return Err(Diagnostic::new(pos, format!(
                                "declaration of @{name} does not match its definition in module {module}"
                            ))
                            .into());
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn record(&mut self, module: &str, m: &Module) {
        for item in &m.items {
            match item {
                Item::Define(f) => {
                    self.defined
                        .insert(f.name.clone(), (f.ty.clone(), module.to_string()));
                }
                Item::Global(g) => {
                    self.globals.insert(g.name.clone(), module.to_string());
                }
                _ => {}
            }
        }
    }

    /// The lIR type of a function defined in this JIT.
    pub fn signature(&self, name: &str) -> Result<FnType> {
        match self.defined.get(name) {
            Some((t, _)) => Ok(t.clone()),
            None => Err(Error::Jit(format!(
                "no function @{name} is defined in this JIT"
            ))),
        }
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

    /// A defined function as the function pointer type `F`.
    ///
    /// # Safety
    /// `F` must be an `extern "C"` function pointer type matching the
    /// lIR type [`Jit::signature`] reports (a `tailcc` function cannot
    /// be called from Rust), and must not be called after the `Jit` is
    /// dropped.
    pub unsafe fn function<F: Copy>(&self, name: &str) -> Result<F> {
        if std::mem::size_of::<F>() != std::mem::size_of::<usize>() {
            return Err(Error::Jit("F is not a function pointer type".into()));
        }
        let addr = self.address(name)?;
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
