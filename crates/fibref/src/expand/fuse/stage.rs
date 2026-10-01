//! One stage of a chain, rewritten: `(map f c)` in the collection position
//! of a terminal consumer, or of another stage, becomes the recipe
//! `(fib.seq/Mapped c f)` (source first, `lib/fib/seq/recipes.fib`).
//!
//! A stage is rewritten only when the rewrite cannot change what the
//! program does:
//!
//! - its head is the library's (`Env::library`);
//! - the function or count argument is *atomic*: a symbol, a literal or a
//!   `fn` form, whose evaluation has no effect and does not depend on its
//!   place in the argument order (the recipe holds its source before its
//!   parameter, the call the other way round). A stage with any other
//!   argument (`(take (dec n) c)`) keeps its lazy form, and so does the
//!   chain below it: the lazy seq is a source like any other, read once;
//! - `remove` also needs its predicate to be a symbol, or a plain `fn`
//!   literal whose last body form can be wrapped: the recipe is `Filtered`
//!   over the predicate's complement, `(fn (g) (not (truthy? (p g))))` for
//!   a symbol `p` and a fresh `g`, or the literal with its last form
//!   wrapped; `truthy?` is `fib.core`'s, so the module must see `fib.core`.

use std::collections::HashSet;

use crate::syntax::{Form, FormKind, Pos};

use super::super::build::{call, list};
use super::super::collections::prelude_name;
use super::super::core::Role;
use super::super::ctx::ExpandCtx;
use super::tables::{split_head, stage_named, terminal, Facade, Shape, Stage, Terminal};

/// What a module is, for the fusion rewrite: the names that are not the
/// library's (its own definitions, what the non-library modules it uses
/// export) and which implicit modules it sees.
pub(crate) struct ModuleEnv {
    pub shadow: HashSet<String>,
    pub seq: bool,
    pub coll: bool,
    pub core: bool,
}

/// What the rewrite knows about the top-level form it is walking.
pub(crate) struct Env<'a> {
    pub module: &'a ModuleEnv,
    /// **B**: the names the form binds anywhere.
    pub bound: HashSet<String>,
}

impl Env<'_> {
    fn sees(&self, facade: Facade) -> bool {
        match facade {
            Facade::Seq => self.module.seq,
            Facade::Coll => self.module.coll,
        }
    }

    /// The bare name of `head` when it is the library's name of `facade`:
    /// written `fib.seq/base` it always is; bare, it is when the module
    /// sees the facade and the name is not bound by the form, defined by
    /// the module or exported by a non-library module it uses.
    fn library<'h>(&self, head: &'h str, facade: Facade) -> Option<&'h str> {
        let (qualified, base) = split_head(head);
        match qualified {
            Some(q) => (q == facade).then_some(base),
            None => {
                let free = !self.bound.contains(base) && !self.module.shadow.contains(base);
                (free && self.sees(facade)).then_some(base)
            }
        }
    }

    /// The row of **T** this call is, if its head is the library's.
    pub(super) fn terminal_call(&self, items: &[Form]) -> Option<&'static Terminal> {
        let head = items.first()?.as_sym()?;
        let row = terminal(split_head(head).1, items.len() - 1)?;
        self.library(head, row.facade).map(|_| row)
    }

    /// The row of **A** this call is, if its head is the library's, it has
    /// two arguments and they are ones the rewrite can take.
    fn stage_call(&self, items: &[Form]) -> Option<&'static Stage> {
        let head = items.first()?.as_sym()?;
        let row = self.library(head, Facade::Seq).and_then(stage_named)?;
        let ok = items.len() == 3
            && match row.shape {
                Shape::Two => true,
                Shape::Param => atomic(&items[1]),
                Shape::Complement => {
                    self.module.core && (items[1].as_sym().is_some() || plain_fn(&items[1]))
                }
            };
        ok.then_some(row)
    }
}

/// Whether the evaluation of `form` has no effect and can move: a symbol,
/// a literal, or a `fn` form.
fn atomic(form: &Form) -> bool {
    match &form.kind {
        FormKind::List(items) => items.first().and_then(Form::as_sym) == Some("fn"),
        FormKind::Vec(_) | FormKind::Map(_) => false,
        _ => true,
    }
}

/// `(fn (params) body+)`, with no name, `->` or `:where`.
fn plain_fn(form: &Form) -> bool {
    let Some(items) = form.as_list() else {
        return false;
    };
    let annotated = items.get(2).is_some_and(|f| match &f.kind {
        FormKind::Sym(s) => s == "->",
        FormKind::Kw(k) => k == "where",
        _ => false,
    });
    items.len() >= 3
        && items[0].as_sym() == Some("fn")
        && items[1].as_list().is_some()
        && !annotated
}

/// `(fib.prelude/not (fib.core/truthy? form))`.
fn complement(form: Form, pos: &Pos) -> Form {
    let truthy = call("fib.core/truthy?", vec![form], pos);
    call(&prelude_name("not"), vec![truthy], pos)
}

/// The predicate of `Filtered` that is the complement of `p`, a symbol or
/// a plain `fn` literal (`Env::stage_call` checked).
fn complement_of(p: Form, ctx: &ExpandCtx, pos: &Pos) -> Form {
    match p.kind {
        FormKind::List(mut items) => {
            if let Some(last) = items.pop() {
                items.push(complement(last, pos));
            }
            Form::new(FormKind::List(items), p.pos)
        }
        _ => {
            let g = ctx.gensym("fuse", pos);
            let body = complement(list(vec![p, g.clone()], pos), pos);
            call("fn", vec![list(vec![g], pos), body], pos)
        }
    }
}

/// The roles of a recipe call `(R c x)`: the head is kept, `c` is the next
/// stage or the source, `x` the other collection of a `Cat` (a stage too)
/// or an expression.
fn recipe_role(shape: Shape) -> Role {
    let last = if shape == Shape::Two {
        Role::Fuse
    } else {
        Role::Expr
    };
    Role::items(vec![Role::Keep, Role::Fuse, last], Role::Expr)
}

/// The rewrite of `form` as a stage in the collection position of a
/// terminal consumer or of another stage: the recipe call and the roles of
/// its items; `form` itself when it is not a stage the rewrite can take.
pub(crate) fn rewrite(env: &Env, ctx: &ExpandCtx, form: Form) -> Result<(Form, Role), Form> {
    let Some(row) = form.as_list().and_then(|items| env.stage_call(items)) else {
        return Err(form);
    };
    let pos = form.pos.clone();
    let FormKind::List(items) = form.kind else {
        return Err(form);
    };
    match <[Form; 3]>::try_from(items) {
        Ok([_, p, c]) => {
            let args = match row.shape {
                Shape::Param => vec![c, p],
                Shape::Two => vec![p, c],
                Shape::Complement => vec![c, complement_of(p, ctx, &pos)],
            };
            Ok((call(row.recipe, args, &pos), recipe_role(row.shape)))
        }
        Err(items) => Err(Form::new(FormKind::List(items), pos)),
    }
}
