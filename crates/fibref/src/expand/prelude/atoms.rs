//! `swap!` with Clojure's extra arguments (stdlib design §4.1 and the row
//! `swap!`, tranche 1 R6a; Appendix A10 swap). The builtin takes the atom
//! and a function of the value; the macro answers the calls with more:
//!
//! ```text
//! (swap! a f x ..) ⟹ (fib.prelude/swap! a (fn (v) (f v x ..)))
//! ```
//!
//! with `v` a gensym. The call of two arguments is the builtin's and is
//! declined, so the macro and the builtin coexist (the expansion's own
//! head is `fib.prelude/swap!`, which is not a macro, so it is expanded
//! once). `f` may be any expression: a symbol, a literal `fn`, a call that
//! answers a function. The extra arguments are evaluated inside the
//! closure, each time the builtin calls it, which is more than once when
//! the swap is retried under contention (§4.1: "`f` may run more than
//! once"). A call of fewer than two arguments is declined: the checker's
//! arity error stands. Generated forms carry the call's position, the
//! arguments their own.

use crate::syntax::{Form, Pos};

use super::fold::declined;
use super::Outcome;
use crate::expand::build::{call, list, sym, unit};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;

/// `(fn (v) (f v x ..))` with `v` a gensym.
fn closure(ctx: &ExpandCtx, f: Form, extra: Vec<Form>, pos: &Pos) -> Form {
    let v = ctx.gensym("v", pos);
    let mut body = vec![f, v.clone()];
    body.extend(extra);
    let params = list(vec![v], pos);
    list(vec![sym("fn", pos), params, list(body, pos)], pos)
}

/// One call of `swap!`: the rewrite from three arguments, else declined.
pub(super) fn swap(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Outcome {
    if items.len() < 4 {
        return declined(items, pos);
    }
    let mut it = items.into_iter().skip(1);
    // At least three arguments: the unit forms are for totality.
    let atom = it.next().unwrap_or_else(|| unit(pos));
    let f = it.next().unwrap_or_else(|| unit(pos));
    let f = closure(ctx, f, it.collect(), pos);
    Outcome::Expanded(call(&prelude_name("swap!"), vec![atom, f], pos))
}
