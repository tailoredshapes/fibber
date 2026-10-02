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
use crate::expand::build::{call, check_arity, list, sym, unit};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// `(fn (v) (f v x ..))` with `v` a gensym.
fn closure(ctx: &ExpandCtx, f: Form, extra: Vec<Form>, pos: &Pos) -> Form {
    let v = ctx.gensym("v", pos);
    let mut body = vec![f, v.clone()];
    body.extend(extra);
    let params = list(vec![v], pos);
    list(vec![sym("fn", pos), params, list(body, pos)], pos)
}

/// `(vswap! v f a ..)` (the row `vswap!`, stdlib §4.9): not atomic, as
/// Clojure's, and the value is the new one.
///
/// ```text
/// (let ((c v)) (let ((n (f @c a ..))) (do (reset! c n) n)))
/// ```
///
/// `c` and `n` are gensyms, `@c` and `reset!` are written
/// `fib.prelude/deref` and `fib.prelude/reset!`. The new value is read
/// after the `reset!`, so it is for a `Copy` value (a number, a bool);
/// a value that the `reset!` moves is the ownership checker's error.
pub(super) fn vswap(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("vswap!", &items, 2, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let v = it.next().unwrap_or_else(|| unit(pos));
    let f = it.next().unwrap_or_else(|| unit(pos));
    let (c, n) = (ctx.gensym("c", pos), ctx.gensym("n", pos));
    let current = call(&prelude_name("deref"), vec![c.clone()], pos);
    let apply = list([vec![f, current], it.collect()].concat(), pos);
    let reset = call(&prelude_name("reset!"), vec![c.clone(), n.clone()], pos);
    let done = call("do", vec![reset, n.clone()], pos);
    let inner = call(
        "let",
        vec![list(vec![list(vec![n, apply], pos)], pos), done],
        pos,
    );
    let outer = list(vec![list(vec![c, v], pos)], pos);
    Ok(call("let", vec![outer, inner], pos))
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
