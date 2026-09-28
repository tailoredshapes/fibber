//! The cross-module namespace of a `Jit` (spec/lir.md §11): what each
//! module defined and exported, and the rules a later module's
//! declarations must satisfy.

use std::collections::HashMap;

use lir::ast::{Item, Module};
use lir::{Diagnostic, FnType, Type};

use crate::error::Result;

/// What a name resolves to across modules.
#[derive(Clone, Debug, PartialEq)]
pub enum Defined {
    Func(FnType),
    Var(Type),
}

#[derive(Default)]
pub struct Names {
    /// Every exported definition so far: what it is and its module.
    exported: HashMap<String, (Defined, String)>,
    /// `private` and `internal` definitions: their module, for messages.
    hidden: HashMap<String, String>,
}

/// What a declaration asks for.
enum Decl<'a> {
    Fn(&'a FnType),
    Var(&'a Type),
}

impl Names {
    /// An exported function's type, or the error for `name`.
    pub fn function(&self, name: &str) -> Result<FnType> {
        match self.exported.get(name) {
            Some((Defined::Func(t), _)) => Ok(t.clone()),
            Some((Defined::Var(_), _)) => {
                Err(jit_err(format!("@{name} is a global, not a function")))
            }
            None => match self.hidden.get(name) {
                Some(m) => Err(jit_err(format!("@{name} is private to module {m}"))),
                None => Err(jit_err(format!(
                    "no function @{name} is defined in this JIT"
                ))),
            },
        }
    }

    /// The rules of spec/lir.md §11 for a module about to be added: no
    /// second exported definition; a declaration matches the earlier
    /// definition, or names a symbol of the host process.
    pub fn check(&self, m: &Module, in_process: impl Fn(&str) -> bool) -> Result<()> {
        for item in &m.items {
            let (name, pos, decl) = match item {
                Item::Define(f) if f.mods.linkage.exported() => (&f.name, f.pos, None),
                Item::Global(g) if g.mods.linkage.exported() => (&g.name, g.pos, None),
                Item::Declare(d) => (&d.name, d.pos, Some(Decl::Fn(&d.ty))),
                Item::DeclareGlobal(g) => (&g.name, g.pos, Some(Decl::Var(&g.ty))),
                Item::Define(_) | Item::Global(_) | Item::Struct(_) => continue,
            };
            let first = self.exported.get(name);
            match (first, decl) {
                (Some((_, module)), None) => {
                    return Err(Diagnostic::new(
                        pos,
                        format!("duplicate definition of @{name} (first in module {module})"),
                    )
                    .into())
                }
                (None, Some(_)) if !in_process(name) => {
                    return Err(Diagnostic::new(pos, format!(
                        "undefined symbol @{name}: defined by no module of this JIT and not in the process"
                    ))
                    .into())
                }
                (Some((def, module)), Some(decl)) if !matches(def, &decl) => {
                    return Err(Diagnostic::new(
                        pos,
                        format!("declaration of @{name} does not match its definition in module {module}"),
                    )
                    .into())
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Record a module that was added.
    pub fn record(&mut self, module: &str, m: &Module) {
        for item in &m.items {
            let (name, exported, def) = match item {
                Item::Define(f) => (
                    &f.name,
                    f.mods.linkage.exported(),
                    Defined::Func(f.ty.clone()),
                ),
                Item::Global(g) => (
                    &g.name,
                    g.mods.linkage.exported(),
                    Defined::Var(g.ty.clone()),
                ),
                _ => continue,
            };
            if exported {
                self.exported
                    .insert(name.clone(), (def, module.to_string()));
            } else {
                self.hidden.insert(name.clone(), module.to_string());
            }
        }
    }
}

fn matches(def: &Defined, decl: &Decl<'_>) -> bool {
    match (def, decl) {
        (Defined::Func(a), Decl::Fn(b)) => a == *b,
        (Defined::Var(a), Decl::Var(b)) => a == *b,
        _ => false,
    }
}

fn jit_err(m: String) -> crate::error::Error {
    crate::error::Error::Jit(m)
}
