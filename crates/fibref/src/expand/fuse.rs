//! The fusion rewrite (stdlib design §2.1 rule 2, §7 E16; tranche 1 plan
//! R9): after a module is expanded, the collection argument of a terminal
//! consumer that is a chain of sequence functions becomes the recipes of
//! `lib/fib/seq/recipes.fib`, so the consumer walks the chain as one push
//! loop and nothing is allocated per element:
//!
//! ```text
//! (reduce g 0 (filter p (map f v)))  ⟹  (reduce g 0 (fib.seq/Filtered (fib.seq/Mapped v f) p))
//! ```
//!
//! **The rule.** Let **T** be the terminal consumers and **A** the sequence
//! functions that have a recipe (`tables`). In a call `(t args.. c)` of a
//! member of **T** the collection argument `c` becomes `F[c]`, where
//! `F[(a args.. c)]` for `a` in **A** is the recipe `(R c args..)` over
//! `F[c]` (`concat` rewrites both collections) and `F[x]` is `x` for
//! anything else, evaluated as written. A call of **A** anywhere else is
//! left as it is: the lazy form, a memoised seq. It is sound because a
//! recipe made this way occurs once, as the argument of the consumer or
//! stage that holds it, and a consumer walks its argument at most once.
//!
//! **Which calls are the library's** (stdlib design C-13: the expander has
//! macros and types by module, no scope, so the rule is conservative). A
//! call `(h args..)` is a library call when `h` is a bare name of the
//! tables that (1) the form does not bind anywhere (a `let` or `loop` name,
//! a parameter, a pattern variable, a `defun` name: **B**, `scan`), (2) the
//! module does not define at top level, (3) no non-library module the
//! module `:use`s exports, and (4) belongs to an implicit module the module
//! sees (`fib.seq`, or `fib.coll` for `vec`, `set` and `into`); or a head
//! written `fib.seq/h` or `fib.coll/h`, which always is. A module of the
//! library itself (an `ns` starting `fib.`) is not rewritten: its parts
//! define the functions and the recipes the rewrite names.
//!
//! **Decided here, where the page is silent** (each makes the rewrite
//! decline, never change what a program does; `stage` has the reasons):
//! - a stage is rewritten only when its function or count argument is a
//!   symbol, a literal or a `fn`: the recipe holds its source before its
//!   parameter, so an argument with an effect would run in another order;
//! - `remove` becomes `Filtered` over the complement of its predicate (the
//!   predicate of `Filtered` takes any `Truthy`, so the complement is
//!   `(not (truthy? (p g)))`), for a symbol or a plain `fn` literal, in a
//!   module that sees `fib.core`;
//! - `nth` reads `(nth c i)`: its collection is the first argument, the
//!   others take it last;
//! - a stage of the wrong arity (`(map f a b)`) is not a stage.
//!
//! **How.** A second walk of each top-level form that calls a terminal,
//! with the machinery of `walk`: the role `Role::Fuse` marks a collection
//! position, `Expander::fuse` says the walk is this pass, and the
//! planning of a form (`plan`) rewrites a stage and picks the roles of its
//! items; the first pass's macro expansion is not run again. The pass
//! uses the one `ExpandCtx` (a `remove` of a symbol takes one gensym,
//! `#fuse.N`) and counts depth as the first walk does: a form the walk
//! cannot finish (too deep) is kept as the first pass left it.

mod scan;
mod stage;
mod tables;

use crate::syntax::Form;

use super::core::Role;
use super::ctx::ExpandCtx;
use super::error::ExpandError;
use super::expr::{list_plan, Expander, Finish};
use super::private::{marker_index, put_marker, take_marker};
use super::runner::NoRunner;
use super::top::definition_role;
use super::walk::walk;

pub(crate) use scan::{defined_names, names_of};
pub(crate) use stage::{Env, ModuleEnv};
use tables::Terminal;

/// Whether a module is the library's own.
pub(crate) fn is_library(ns: &str) -> bool {
    ns.starts_with("fib.")
}

/// The roles of a terminal consumer's items: the head, the arguments, and
/// the collection (the first argument or the last) in the fusing role.
fn terminal_role(items: usize, t: &Terminal) -> Role {
    let at = if t.coll_first { 1 } else { items - 1 };
    let mut first = vec![Role::Expr];
    first.resize(at, Role::Arg);
    first.push(Role::Fuse);
    Role::items(first, Role::Arg)
}

/// How the fusing walk plans a form in expression position: a stage in a
/// collection position is rewritten, a terminal consumer has its
/// collection position marked, any other form is planned as the first
/// pass plans it. Forms are already expanded, so a literal collection or
/// `nil` is not looked at again.
pub(crate) fn plan(
    env: &Env,
    ctx: &ExpandCtx,
    form: Form,
    role: &Role,
) -> Result<(Form, Role, Finish), ExpandError> {
    let form = if *role == Role::Fuse {
        match stage::rewrite(env, ctx, form) {
            Ok((rewritten, roles)) => return Ok((rewritten, roles, Finish::Same)),
            Err(form) => form,
        }
    } else {
        form
    };
    let is_arg = matches!(role, Role::Arg | Role::Fuse);
    let items = match form.as_list() {
        Some(items) if !items.is_empty() => items,
        _ => return Ok((form, Role::Keep, Finish::Same)),
    };
    let roles = match env.terminal_call(items) {
        Some(t) => terminal_role(items.len(), t),
        None => list_plan(items, &form.pos, is_arg)?,
    };
    Ok((form, roles, Finish::Same))
}

/// What module `ctx` is expanding is, for the rewrite.
fn module_env(ctx: &ExpandCtx, defined: std::collections::HashSet<String>) -> ModuleEnv {
    let scope = ctx.scope();
    let sees = |ns: &str| scope.uses.iter().chain(&scope.implicit).any(|u| u == ns);
    let mut shadow = defined;
    for used in scope.uses.iter().filter(|u| !is_library(u)) {
        shadow.extend(ctx.exported_names(used));
    }
    ModuleEnv {
        shadow,
        seq: sees("fib.seq"),
        coll: sees("fib.coll"),
        core: sees("fib.core"),
    }
}

/// `form` with its `:private` marker back, if it had one.
fn restore(form: Form, marker: Option<(usize, Form)>) -> Form {
    match marker {
        Some((at, m)) => put_marker(form, at, m),
        None => form,
    }
}

/// One top-level form, rewritten; as it was if it calls no terminal, is
/// not a definition with expressions, or its walk fails.
fn fuse_form(form: Form, module: &ModuleEnv, ctx: &mut ExpandCtx) -> Form {
    if !scan::calls_a_terminal(&form) {
        return form;
    }
    let (body, marker) = match marker_index(&form) {
        Some(at) => {
            let (body, m) = take_marker(form, at);
            (body, m.map(|m| (at, m)))
        }
        None => (form, None),
    };
    let Some(role) = definition_role(&body) else {
        return restore(body, marker);
    };
    let env = Env {
        module,
        bound: scan::bound_names(&body),
    };
    let mut runner = NoRunner;
    let mut ex = Expander::fusing(ctx, &mut runner, env);
    match walk(&mut ex, body.clone(), role) {
        Ok(fused) => restore(fused, marker),
        Err(_) => restore(body, marker),
    }
}

/// The pass over one module's expanded forms: records the names the
/// module exports (for the modules that `:use` it), then rewrites each
/// top-level form unless the module is the library's or does not see
/// `fib.seq`.
pub(crate) fn run(forms: Vec<Form>, ctx: &mut ExpandCtx) -> Vec<Form> {
    let ns = ctx.scope().ns.clone();
    if is_library(&ns) {
        return forms;
    }
    let defined = defined_names(&forms);
    ctx.record_names(&ns, defined.public);
    let module = module_env(ctx, defined.all);
    if !module.seq {
        return forms;
    }
    forms
        .into_iter()
        .map(|form| fuse_form(form, &module, ctx))
        .collect()
}
