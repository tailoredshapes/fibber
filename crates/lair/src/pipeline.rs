//! The shared front half of every path: parse, check, and for a program
//! the `main` rule (spec/lir.md §7.2).

use crate::error::{Error, Result};
use crate::llvm::target::Machine;
use crate::lower::lower;
use lir::Module;

/// Parse and check a module.
pub fn check_source(src: &str) -> Result<Module> {
    lir::parse_and_check(src).map_err(Error::Invalid)
}

/// Parse and check a module that will run as a program.
pub fn for_executable(src: &str) -> Result<Module> {
    let m = check_source(src)?;
    lir::check_main(&m)?;
    Ok(m)
}

/// The verified, unoptimised LLVM IR of a module for the host.
pub fn emit_llvm(src: &str, name: &str) -> Result<String> {
    let m = check_source(src)?;
    let host = Machine::host(0)?;
    Ok(lower(&m, name, &host.triple, &host.data_layout)?.ir())
}
