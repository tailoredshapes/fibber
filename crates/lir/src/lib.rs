//! `lir`: the lIR language of spec/lir.md without LLVM.
//!
//! [`parse`] reads a module into the AST of [`ast`]; [`check`] runs the
//! whole-module checker of spec/lir.md §10 over it. `lair` lowers only
//! modules that [`check`] accepted (method.md rule 7).

pub mod ast;
pub mod check;
pub mod diag;
pub mod parse;
pub mod sexp;
pub mod types;

pub use ast::Module;
pub use check::{check, check_main};
pub use diag::{Diagnostic, Pos};
pub use types::{Cc, FnType, Type};

/// Parse `src` into a module (spec/lir.md §1 to §4).
pub fn parse(src: &str) -> Result<Module, Diagnostic> {
    let forms = sexp::read(src)?;
    parse::module(&forms)
}

/// Parse and check `src`: every diagnostic, or the checked module.
pub fn parse_and_check(src: &str) -> Result<Module, Vec<Diagnostic>> {
    let module = parse(src).map_err(|d| vec![d])?;
    check(&module)?;
    Ok(module)
}
