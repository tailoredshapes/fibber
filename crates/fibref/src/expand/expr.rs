//! The expression walker: expands macros outermost-first, walks the
//! expression positions of core forms and calls, and rewrites literal
//! collections (§1.4, §3.16, §4.3).
//!
//! This file decides what to do with one form; `walk` does it for a
//! whole tree with an explicit stack.

use crate::syntax::{Form, FormKind, Pos};

use super::build::{head_name, malformed};
use super::collections::{map_literal, vec_literal};
use super::core::{expr_plan, Role};
use super::ctx::ExpandCtx;
use super::error::{ExpandError, ExpandErrorKind as K};
use super::fuse::{self, Env};
use super::heads::{is_core, is_definition, primitive_operand};
use super::prelude::{self, Outcome};
use super::quasi;
use super::runner::MacroRunner;

/// The state of one expansion: the context, the runner for user macros
/// and the current nesting depth; in the fusion walk, what it knows of the
/// form it rewrites (`fuse`).
pub(crate) struct Expander<'a> {
    pub(crate) ctx: &'a mut ExpandCtx,
    runner: &'a mut dyn MacroRunner,
    depth: usize,
    pub(crate) fuse: Option<Env<'a>>,
}

impl<'a> Expander<'a> {
    pub(crate) fn new(ctx: &'a mut ExpandCtx, runner: &'a mut dyn MacroRunner) -> Self {
        Expander {
            ctx,
            runner,
            depth: 0,
            fuse: None,
        }
    }

    /// An expander for the fusion walk of one top-level form: the forms it
    /// walks are expanded already, and it rewrites the chains in them.
    pub(crate) fn fusing(
        ctx: &'a mut ExpandCtx,
        runner: &'a mut dyn MacroRunner,
        env: Env<'a>,
    ) -> Self {
        Expander {
            fuse: Some(env),
            ..Expander::new(ctx, runner)
        }
    }

    /// Enters one level of nesting, failing past the depth limit.
    pub(crate) fn enter(&mut self, pos: &Pos) -> Result<(), ExpandError> {
        self.depth += 1;
        let limit = self.ctx.limits.max_depth;
        if self.depth > limit {
            return Err(ExpandError::new(K::TooDeep { limit }, pos));
        }
        Ok(())
    }

    /// Leaves the level entered last.
    pub(crate) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    /// Runs the user macro `name` on the call `form`.
    fn run_user(&mut self, name: &str, form: Form) -> Result<Form, ExpandError> {
        let pos = form.pos.clone();
        let args: Vec<Form> = match form.kind {
            FormKind::List(items) => items.into_iter().skip(1).collect(),
            _ => Vec::new(),
        };
        let Some(def) = self.ctx.macro_def(name) else {
            return Err(malformed(name, "not a macro", &pos));
        };
        if !def.accepts(args.len()) {
            let kind = K::MacroArity {
                name: name.to_string(),
                expected: def.arity_text(),
                found: args.len(),
            };
            return Err(ExpandError::new(kind, &pos));
        }
        let def = def.clone();
        self.ctx.call_pos = pos;
        self.runner.run(&def, args, self.ctx)
    }
}

/// One call of a prelude macro, counted as a step. A call written
/// `fib.prelude/NAME` counts a step only when it expands: the expansions
/// name the prelude's binary builtins that way (`fib.prelude/+`), and the
/// macro declines them.
fn prelude_step(ex: &mut Expander, name: &str, form: Form) -> Result<Outcome, ExpandError> {
    let pos = form.pos.clone();
    let qualified = prelude::is_qualified(name);
    if !qualified {
        ex.ctx.step(&pos)?;
    }
    let outcome = prelude::expand(ex.ctx, form)?;
    if qualified && matches!(outcome, Outcome::Expanded(_)) {
        ex.ctx.step(&pos)?;
    }
    Ok(outcome)
}

/// Expands `form` while its head is a macro (a user macro, which shadows
/// a prelude macro of the same name, then a prelude macro that the module
/// does not define itself, or written `fib.prelude/NAME`) or
/// `quasiquote`, counting each expansion as a step and admitting its
/// result (`ExpandCtx::admit`: its size and its literals). Returns the first
/// form that is not a macro call, or a `for-each` the prelude declined.
pub(crate) fn expand_head(ex: &mut Expander, mut form: Form) -> Result<Form, ExpandError> {
    loop {
        let Some(name) = head_name(&form).map(str::to_string) else {
            return Ok(form);
        };
        let pos = form.pos.clone();
        if name == "quasiquote" {
            ex.ctx.step(&pos)?;
            form = quasi::rewrite(form)?;
        } else if let Some((first, second)) = ex.ctx.macro_ambiguity(&name) {
            let (first, second) = (first.to_string(), second.to_string());
            return Err(ExpandError::new(
                K::AmbiguousMacro {
                    name,
                    first,
                    second,
                },
                &pos,
            ));
        } else if ex.ctx.macro_def(&name).is_some() {
            ex.ctx.step(&pos)?;
            form = ex.run_user(&name, form)?;
        } else if prelude::macro_of(ex.ctx, &name).is_some() {
            match prelude_step(ex, &name, form)? {
                Outcome::Expanded(f) => form = f,
                Outcome::Declined(f) => return Ok(f),
            }
        } else {
            return Ok(form);
        }
        ex.ctx.admit(&mut form, &pos)?;
    }
}

/// What to do with a form after its items are walked.
pub(crate) enum Finish {
    Same,
    Nil,
    VecLiteral,
    MapLiteral,
}

/// Expands macros at the head of a form in expression (or, with
/// `is_arg`, argument) position and plans the walk of what remains; in the
/// fusion walk (`fuse`) the form is expanded already and `role` says
/// whether it is a collection position.
pub(crate) fn plan_expr(
    ex: &mut Expander,
    form: Form,
    role: &Role,
) -> Result<(Form, Role, Finish), ExpandError> {
    if let Some(env) = &ex.fuse {
        return fuse::plan(env, ex.ctx, form, role);
    }
    let is_arg = *role == Role::Arg;
    let form = expand_head(ex, form)?;
    let (role, finish) = plan(&form, is_arg)?;
    Ok((form, role, finish))
}

/// The role of a form in expression position that is not a macro call.
fn plan(form: &Form, is_arg: bool) -> Result<(Role, Finish), ExpandError> {
    let all = || Role::after(0, Role::Expr);
    Ok(match &form.kind {
        FormKind::Sym(s) if s == "nil" => (Role::Keep, Finish::Nil),
        FormKind::Vec(_) => (all(), Finish::VecLiteral),
        FormKind::Map(items) if items.len() % 2 == 1 => {
            return Err(malformed("map literal", "odd number of forms", &form.pos));
        }
        FormKind::Map(_) => (all(), Finish::MapLiteral),
        FormKind::List(items) if !items.is_empty() => {
            (list_plan(items, &form.pos, is_arg)?, Finish::Same)
        }
        _ => (Role::Keep, Finish::Same),
    })
}

/// Applies `finish` to a walked form.
pub(crate) fn finish_form(form: Form, finish: Finish) -> Form {
    let pos = form.pos;
    match (finish, form.kind) {
        (Finish::Nil, _) => Form::new(FormKind::Nil, pos),
        (Finish::VecLiteral, FormKind::Vec(items)) => vec_literal(items, &pos),
        (Finish::MapLiteral, FormKind::Map(items)) => map_literal(items, &pos),
        (_, kind) => Form::new(kind, pos),
    }
}

/// The role of a non-empty list in expression position.
pub(crate) fn list_plan(items: &[Form], pos: &Pos, is_arg: bool) -> Result<Role, ExpandError> {
    let name = match &items[0].kind {
        FormKind::Nil => return Err(ExpandError::new(K::NilCalled, pos)),
        FormKind::Sym(s) => s.as_str(),
        _ => return Ok(call_role(None)),
    };
    match name {
        "nil" => Err(ExpandError::new(K::NilCalled, pos)),
        "unquote" => Err(ExpandError::new(
            K::UnquoteOutsideQuasiquote { head: "unquote" },
            pos,
        )),
        "unquote-splicing" => {
            let head = "unquote-splicing";
            Err(ExpandError::new(K::UnquoteOutsideQuasiquote { head }, pos))
        }
        "&" => {
            let ok = is_arg && items.len() == 2 && items[1].as_sym().is_some_and(|s| s != "nil");
            if ok {
                Ok(Role::Keep)
            } else {
                Err(ExpandError::new(K::InOutOutsideArgument, pos))
            }
        }
        n if is_definition(n) => {
            let head = n.to_string();
            Err(ExpandError::new(K::DefinitionInExpression { head }, pos))
        }
        n if is_core(n) => expr_plan(n, items, pos),
        n => Ok(call_role(primitive_operand(n))),
    }
}

/// A call: the head in expression position, every argument in argument
/// position except the operand at `skip`, which is left alone.
fn call_role(skip: Option<usize>) -> Role {
    let mut first = vec![Role::Expr];
    if let Some(skip) = skip {
        first.resize(skip, Role::Arg);
        first.push(Role::Keep);
    }
    Role::items(first, Role::Arg)
}
