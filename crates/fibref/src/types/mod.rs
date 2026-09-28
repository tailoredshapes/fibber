//! Name resolution and type inference for fibber (spec/types.md §1–§5;
//! syntax §3, §4.3, §5).
//!
//! [`check_program`] takes a module's forms after expansion and the
//! prelude's (from [`prelude_forms`]) and returns a [`TypedProgram`] or
//! every error found. The pipeline is §3.5:
//!
//! 1. the builtin module (`init`): the built-in `Option` and `Form`, the
//!    built-in protocols (`Deref`, `Num`, `Eq`, `Ord`, `Bits`, `Hash`,
//!    `Show`) with their instances, and the builtins of the table in
//!    [`builtins`];
//! 2. lowering (`lower`), per module: every name declared, field types,
//!    protocol signatures, `impl` heads (instances with their declared
//!    contexts, coherence, the Paterson condition), then bodies to the
//!    resolved AST of [`ast`], with resolution errors at positions;
//! 3. inference (`infer`), per module: the SCCs of `defun`s and `def`s
//!    in dependency order, each solved (unification, the deferred
//!    worklist, colours) and generalised; then `impl` bodies against
//!    their signatures; then macros; then `main : (fn () i64)`.
//!
//! Modules are the prelude (`fib.prelude`, written in fibber under
//! `lib/`) and one user module; user names shadow prelude names, which
//! shadow the builtins (syntax §5, kept minimal).
//!
//! Not here: the ownership pass (§6), monomorphisation (§4.3) and the
//! syntactic `&` checks of §6.5 and §6.9, which belong to the next
//! stage; the typed program carries what they need.

pub mod annot;
pub mod ast;
pub mod builtins;
pub mod decls;
pub mod display;
pub mod error;
pub mod infer;
pub mod init;
pub mod lower;
pub mod names;
pub mod program;
pub mod scheme;
pub mod store;
pub mod ty;

#[cfg(test)]
mod tests;

use crate::expand::{expand_prelude, expand_program, ExpandCtx, NoRunner};
use crate::syntax::{read_all, Form};

pub use error::{ErrorKind, TypeError};
pub use program::TypedProgram;

use decls::ModuleId;
use infer::Checker;

/// The prelude's library source (`lib/prelude.fib`), written in fibber.
pub const PRELUDE_LIB: &str = include_str!("../../../../lib/prelude.fib");

/// The prelude's forms after expansion: the expander's own prelude
/// (`List` and the derived instances of `Option` and `List`) followed by
/// [`PRELUDE_LIB`]. Registers the prelude's types in `ctx`, so a user
/// module expanded afterwards in the same `ctx` can `derive` over them.
pub fn prelude_forms(ctx: &mut ExpandCtx) -> Result<Vec<Form>, String> {
    let mut forms = expand_prelude(ctx).map_err(|e| e.to_string())?;
    let lib = read_all(PRELUDE_LIB, "lib/prelude.fib").map_err(|e| e.to_string())?;
    forms.extend(expand_program(lib, ctx, &mut NoRunner).map_err(|e| e.to_string())?);
    ctx.end_module();
    Ok(forms)
}

/// Checks a program: its module's forms after expansion, against the
/// prelude's forms after expansion. Requires `main : (fn () i64)`.
pub fn check_program(forms: &[Form], prelude: &[Form]) -> Result<TypedProgram, Vec<TypeError>> {
    check(forms, prelude, true)
}

/// Checks a module that need not define `main` (the prelude alone, a
/// library).
pub fn check_library(forms: &[Form], prelude: &[Form]) -> Result<TypedProgram, Vec<TypeError>> {
    check(forms, prelude, false)
}

/// The stack the checker runs on. Its passes recurse over the AST, and
/// the expander admits nesting up to `expand::MAX_EXPAND_DEPTH` (2000)
/// levels, which each pass must handle; a few hundred kilobytes per
/// hundred levels in a debug build.
pub const CHECK_STACK: usize = 256 * 1024 * 1024;

fn check(forms: &[Form], prelude: &[Form], main: bool) -> Result<TypedProgram, Vec<TypeError>> {
    let pos = init::builtin_pos();
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("fibref-check".into())
            .stack_size(CHECK_STACK)
            .spawn_scoped(scope, || check_here(forms, prelude, main));
        match worker.map(|h| h.join()) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(vec![TypeError::other(&pos, "the checker panicked")]),
            Err(e) => Err(vec![TypeError::other(
                &pos,
                format!("cannot start the checker: {e}"),
            )]),
        }
    })
}

fn check_here(
    forms: &[Form],
    prelude: &[Form],
    main: bool,
) -> Result<TypedProgram, Vec<TypeError>> {
    infer_lowered(lower_program(forms, prelude)?, main)
}

/// A program after lowering (§3.5 steps 1–3) and before inference:
/// every name resolved, every body in the AST of [`ast`], nothing
/// typed. The syntactic `&` checks of §6.5 and §6.9, which §3.5 step 3
/// runs before typing, read it (`own::syntactic`).
pub struct Lowered {
    /// Every definition and binding site.
    pub globals: decls::Globals,
    prelude: lower::ModuleItems,
    user: lower::ModuleItems,
}

/// Steps 1–3 of §3.5 for the prelude and the user module. Runs on the
/// calling thread, whose stack must be [`CHECK_STACK`] deep for deeply
/// nested programs ([`check_program`] makes a thread of its own).
pub fn lower_program(forms: &[Form], prelude: &[Form]) -> Result<Lowered, Vec<TypeError>> {
    let mut g = init::new_globals().map_err(|e| vec![e])?;
    let (prelude, prelude_private) = lower::strip_private(prelude);
    let (forms, user_private) = lower::strip_private(forms);
    let pd = lower::declare(&mut g, ModuleId::Prelude, &prelude)?;
    lower::mark_private(&mut g, ModuleId::Prelude, &prelude_private);
    init::finish_builtins(&mut g).map_err(|e| vec![e])?;
    let prelude_items = lower::define(&mut g, pd)?;
    let ud = lower::declare(&mut g, ModuleId::User, &forms)?;
    lower::mark_private(&mut g, ModuleId::User, &user_private);
    let user_items = lower::define(&mut g, ud)?;
    Ok(Lowered {
        globals: g,
        prelude: prelude_items,
        user: user_items,
    })
}

/// Steps 4–7 of §3.5 on a lowered program; with `main`, requires `main
/// : (fn () i64)`. Same stack requirement as [`lower_program`].
pub fn infer_lowered(l: Lowered, main: bool) -> Result<TypedProgram, Vec<TypeError>> {
    let g = l.globals;
    let (env, tables, units) = {
        let mut ck = Checker::new(&g);
        ck.module(&l.prelude);
        ck.module(&l.user);
        if main {
            ck.check_main();
        }
        if !ck.errors.is_empty() {
            return Err(ck.errors);
        }
        (ck.env, ck.t, ck.order)
    };
    Ok(TypedProgram::new(g, env, tables, units))
}

/// Why [`check_source`] failed.
#[derive(Clone, Debug)]
pub enum SourceError {
    /// The prelude did not read or expand (a bug in `lib/`).
    Prelude(String),
    /// The source did not read.
    Read(String),
    /// The source did not expand (a user macro is pending without the
    /// evaluator).
    Expand(String),
    /// Resolution and type errors, in order.
    Type(Vec<TypeError>),
}

/// Reads, expands and checks `source` as the user module, with the
/// prelude: the whole front end, for tests and tools.
pub fn check_source(source: &str, file: &str) -> Result<TypedProgram, SourceError> {
    let mut ctx = ExpandCtx::new();
    let prelude = prelude_forms(&mut ctx).map_err(SourceError::Prelude)?;
    let forms = read_all(source, file).map_err(|e| SourceError::Read(e.to_string()))?;
    let forms = expand_program(forms, &mut ctx, &mut NoRunner)
        .map_err(|e| SourceError::Expand(e.to_string()))?;
    check_program(&forms, &prelude).map_err(SourceError::Type)
}
