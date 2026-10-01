//! Modules (syntax §5): a program is the file given and the modules its
//! `ns` clauses `:require`, `:use` or `:export-from`, found beside that
//! file and then under the library roots (`a.b` at `a/b.fib`; the order
//! is that of `roots.rs`), read once each in dependency order, the main
//! module last. A module's macros reach the modules that `:use` it
//! unqualified and the ones that `:require` it through the alias, a
//! `:private` macro its own module only (`ExpandCtx::macro_def`).
//!
//! The **implicit modules** (`IMPLICIT_LIB`) are loaded before all of
//! these and seen by every module of the program that is not the
//! library's own, as the prelude is: after its `:use`s, shadowed by them
//! and by its own definitions, and by qualified name from anywhere in it
//! (`fib.seq/x`). `spec` reads the `ns` form, `load` finds the files.

mod load;
mod spec;

use std::fmt;

use crate::expand::{expand_program, ExpandCtx, ExpandError, MacroRunner};
use crate::syntax::{Form, Pos, ReadError};

pub use load::{load, load_in, sees_implicit, try_load, try_load_in, try_load_with};
pub use spec::{spec_of, ModuleSpec};

/// The library modules every module of a program sees without naming
/// them (syntax §5), in the order they are loaded: empty until the
/// library is complete, so that nothing in an existing program changes
/// (stdlib design §6.2: `fib.core`, `fib.seq`, `fib.coll`, `fib.print`).
pub const IMPLICIT_LIB: &[&str] = &[];

/// A module as read: its spec, its file and its forms before expansion.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub spec: ModuleSpec,
    pub file: String,
    pub forms: Vec<Form>,
    /// Whether it is an implicit module or was read for one: the
    /// expansion dump leaves such a module out, as it does the prelude
    /// (spec/bootstrap.md §5.1).
    pub implicit: bool,
}

/// Why a program could not be loaded: what [`try_load`] reports, which
/// [`load`] shows as the text of its `Display` (the expansion dump,
/// spec/bootstrap.md §5, tells the kinds apart).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A module's text did not read.
    Read(ReadError),
    /// An `ns` form that does not parse: where it is and what is wrong.
    Spec { pos: Pos, what: String },
    /// A module is not at the file its name gives (missing, a directory,
    /// not UTF-8); `cause` is the operating system's words.
    Missing {
        ns: String,
        file: String,
        cause: String,
    },
    /// Modules that require each other: `path` leads from the main
    /// module to the one that `ns` requires again.
    Cycle { path: Vec<String>, ns: String },
    /// A file declares another `ns` than the one it is required as.
    Mismatch {
        file: String,
        declared: String,
        required: String,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Read(e) => write!(f, "{e}"),
            LoadError::Spec { pos, what } => write!(f, "{pos}: {what}"),
            LoadError::Missing { ns, file, cause } => {
                write!(f, "module {ns} is not at {file}: {cause}")
            }
            LoadError::Cycle { path, ns } => write!(
                f,
                "modules require each other in a cycle: {} -> {ns}",
                path.join(" -> ")
            ),
            LoadError::Mismatch {
                file,
                declared,
                required,
            } => write!(
                f,
                "{file} declares (ns {declared}) but is required as {required}"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Starts expanding the module `spec` describes in `ctx`: its `:use`s
/// and `:require` aliases are what a macro name reaches
/// (`ExpandCtx::begin_module`). [`expand_all`] does this for each module;
/// the expansion dump does it step by step.
pub fn begin_spec(ctx: &mut ExpandCtx, spec: &ModuleSpec) {
    let aliases = spec
        .requires
        .iter()
        .map(|(alias, ns)| (alias.clone(), ns.clone()))
        .collect();
    ctx.begin_module(&spec.ns, (&spec.uses, &spec.implicit), aliases);
    ctx.reexport(&spec.ns, &spec.exports);
}

/// Expands every loaded module in order in one context, each ended
/// for the next (syntax §5: private types), with the macro runner.
pub fn expand_all(
    loaded: Vec<Loaded>,
    ctx: &mut ExpandCtx,
    runner: &mut dyn MacroRunner,
) -> Result<Vec<(ModuleSpec, Vec<Form>)>, ExpandError> {
    let mut out = Vec::with_capacity(loaded.len());
    for l in loaded {
        begin_spec(ctx, &l.spec);
        let forms = expand_program(l.forms, ctx, runner)?;
        ctx.end_module();
        out.push((l.spec, forms));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    #[test]
    fn begin_spec_gives_the_context_the_uses_and_aliases_of_the_module() {
        let forms = read_all("(ns a (:require [b.c :as bc]) (:use d))", "t").expect("reads");
        let mut spec = spec_of(&forms, "main").expect("a spec");
        spec.implicit = vec!["fib.x".to_string()];
        let mut ctx = ExpandCtx::new();
        begin_spec(&mut ctx, &spec);
        let scope = ctx.scope();
        assert_eq!(scope.ns, "a");
        assert_eq!(scope.uses, vec!["d".to_string()]);
        assert_eq!(scope.implicit, vec!["fib.x".to_string()]);
        assert_eq!(scope.aliases.get("bc").map(String::as_str), Some("b.c"));
    }
}
