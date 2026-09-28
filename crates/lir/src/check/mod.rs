//! The whole-module checker (spec/lir.md §10).

mod aggr;
mod arith;
mod calls;
mod cfg;
mod env;
mod expr;
mod fcx;
mod func;
mod globals;
mod memory;
mod phi;
mod walk;

pub use cfg::Cfg;
pub use env::{Env, Symbol};
pub use expr::is_constant;
pub use func::block_order;
pub use walk::{children, statements};

use crate::ast::{Item, Module};
use crate::diag::{Diagnostic, Pos};
use crate::types::{Cc, Type};

/// Check the whole module: every error of the module-level rules, else
/// the first error of each global and function.
pub fn check(m: &Module) -> Result<(), Vec<Diagnostic>> {
    let env = Env::build(m)?;
    let mut errs = Vec::new();
    for item in &m.items {
        let r = match item {
            Item::Global(g) => globals::check_global(&env, g),
            Item::Define(f) => func::check_function(&env, f),
            _ => Ok(()),
        };
        if let Err(e) = r {
            errs.push(e);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// The rule for a module that is run or linked into an executable
/// (spec/lir.md §7.2).
pub fn check_main(m: &Module) -> Result<(), Diagnostic> {
    let bad = |pos| {
        Err(Diagnostic::new(
            pos,
            "main must be (main i32) with no parameters or (i32 ptr)",
        ))
    };
    for item in &m.items {
        match item {
            Item::Define(f) if f.name == "main" => {
                let ok_params = f.ty.params.is_empty() || f.ty.params == [Type::Int(32), Type::Ptr];
                if f.ty.cc != Cc::C || f.ty.ret != Some(Type::Int(32)) || !ok_params {
                    return bad(f.pos);
                }
                if !f.mods.linkage.exported() {
                    return Err(Diagnostic::new(
                        f.pos,
                        "main must not be private or internal",
                    ));
                }
                return Ok(());
            }
            Item::Declare(d) if d.name == "main" => return bad(d.pos),
            _ => {}
        }
    }
    Err(Diagnostic::new(Pos { line: 1, col: 1 }, "no main function"))
}
