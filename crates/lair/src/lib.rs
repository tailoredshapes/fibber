//! `lair`: lIR to native code through LLVM (spec/lir.md §11).
//!
//! Every path runs the same pipeline: parse, check the whole module
//! (`lir::check`), lower, run the LLVM verifier. [`Jit`] compiles
//! modules in-process and returns callable functions; [`aot`] emits
//! object files and executables.

pub mod aot;
pub mod cases;
mod error;
pub mod fuzz;
mod jit;
mod llvm;
mod lower;
mod pipeline;

pub use error::{Error, Result};
pub use jit::{Jit, JitOptions};
pub use lir::{FnType, Type};
pub use pipeline::{check_source, emit_llvm, for_executable};
