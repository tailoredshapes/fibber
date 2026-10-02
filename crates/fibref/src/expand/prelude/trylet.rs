//! `try-let` (stdlib §2.10, §4.3; L12, tranche 2 X3): the block macro that
//! threads a `Result`, because the language has no early return.
//!
//! ```text
//! (try-let ((x e) (y f)) body)
//!   ⟹ (match e ((fib.prelude/Ok x) (match f ((fib.prelude/Ok y) body)
//!                                           ((fib.prelude/Err g) (fib.prelude/Err g))))
//!             ((fib.prelude/Err g) (fib.prelude/Err g)))
//! ```
//!
//! each `g` its own gensym: each `Ok` payload is bound in turn, the first
//! `Err` is the value of the whole form (rebuilt, so its type is that of
//! `body`), and no binding is `body`. A binding list is `((x e) ..)`, as
//! the table writes it, or the flat bracket `[x e y f]`; the body may be
//! several forms, as `let`'s. A pattern for `x` is any pattern of `Ok`'s
//! payload.

use crate::syntax::{Form, FormKind, Pos};

use super::logic::body_form;
use crate::expand::build::{call, check_arity, list, malformed, sym};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// The `(pattern expression)` pairs of a binding list.
fn bindings(form: &Form) -> Result<Vec<(Form, Form)>, ExpandError> {
    let bad = || {
        malformed(
            "try-let",
            "the bindings are ((pattern expression) ..)",
            &form.pos,
        )
    };
    match &form.kind {
        FormKind::List(items) => items
            .iter()
            .map(|b| match b.as_list() {
                Some([x, e]) => Ok((x.clone(), e.clone())),
                _ => Err(bad()),
            })
            .collect(),
        FormKind::Vec(items) if items.len() % 2 == 0 => Ok(items
            .chunks(2)
            .map(|c| (c[0].clone(), c[1].clone()))
            .collect()),
        _ => Err(bad()),
    }
}

/// `(match e ((Ok x) rest) ((Err g) (Err g)))`.
fn step(ctx: &ExpandCtx, x: Form, e: Form, rest: Form, pos: &Pos) -> Form {
    let g = ctx.gensym("err", pos);
    let err = |arg: &Form| call(&prelude_name("Err"), vec![arg.clone()], pos);
    let ok = list(vec![call(&prelude_name("Ok"), vec![x], pos), rest], pos);
    let bad = list(vec![err(&g), err(&g)], pos);
    list(vec![sym("match", pos), e, ok, bad], pos)
}

/// `(try-let ((x e) ..) body..)`.
pub(super) fn try_let(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("try-let", &items, 2, None, pos)?;
    let pairs = bindings(&items[1])?;
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    Ok(pairs
        .into_iter()
        .rev()
        .fold(body, |rest, (x, e)| step(ctx, x, e, rest, pos)))
}
