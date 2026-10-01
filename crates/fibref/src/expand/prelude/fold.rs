//! The variadic folds of the operators (stdlib design §6.3, §4.1, §4.3;
//! tranche 1 R6a): `+ - * < > <= >= =`, `max min` and `bit-and bit-or
//! bit-xor`. Each is a builtin or a library function of two arguments, so
//! the macro answers the calls the binary function cannot and **declines**
//! (leaves the call alone) when it is the binary call: the common case
//! costs nothing and a program's own binding of the name still serves it.
//!
//! Decided here (the plan's R6a text; the page where it is silent):
//! - `(+)` is `0` and `(*)` is `1`, an `i64` until L19 lets a literal adopt
//!   its context's type; `(+ a)` and `(* a)` are `a`; `(+ a b c)` is the
//!   left fold `(fib.prelude/+ (fib.prelude/+ a b) c)`, so `a`, `b`, `c`
//!   are evaluated in order and an overflow traps at the first sum.
//! - `(-)` is the arity error `macro - takes at least 1 argument(s), got
//!   0`; `(- a)` is `(fib.prelude/neg a)`; more arguments fold left.
//! - `(< a b c)` is `(and (< a b) (< b c))`, short-circuiting, with every
//!   operand evaluated once and before the first test, as the call of a
//!   function would: an operand that is not a symbol, a literal or a field
//!   path `(. x f)` is bound first, `(let ((#cmp.N e)) ..)`, in order. The
//!   plan's text binds every operand. Reading what has no effect where it
//!   stands is the same program with fewer locals, and it keeps a literal
//!   in the operand position that L19 will let adopt its context's numeric
//!   type, which a `let` would fix first. (Probed on the interpreter: a
//!   `let` of an owned symbol or of a field of a borrowed struct is accepted
//!   as the direct read is, and a comparison of a `:borrow` parameter of a
//!   non-scalar type is refused with the `let` and without it, `declared
//!   :borrow but escapes`, as the binary call is: a property of protocol
//!   calls, not of this macro.) `(< a)` is `(let ((#cmp.N a)) true)` and the
//!   zero-argument call is declined (the checker's arity error). The same
//!   for `> <= >= =`.
//! - `(max a b c)` is `(fib.core/max (fib.core/max a b) c)`, `min` and the
//!   bit operations (`fib.prelude/bit-and`) likewise; with fewer than
//!   three arguments the call is declined, so the checker's arity error is
//!   the one a program sees.
//!
//! Heads are qualified (`fib.prelude/NAME`, `fib.core/NAME`), so a
//! binding of `+` or `max` in scope is not what the expansion calls.
//! Generated forms carry the call's position, the arguments their own.

use crate::syntax::{Form, FormKind, Pos};

use super::Outcome;
use crate::expand::build::{boolean, call, check_arity, int, list, unit};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// What a fold does with a call of one argument.
#[derive(Clone, Copy)]
pub(super) enum Alone {
    /// The argument itself: `(conj c)` is `c`.
    Itself,
    /// Nothing: the call is declined and the function's arity error
    /// stands.
    Declined,
}

/// The call, unchanged.
pub(super) fn declined(items: Vec<Form>, pos: &Pos) -> Outcome {
    Outcome::Declined(Form::new(FormKind::List(items), pos.clone()))
}

/// `(head (head a b) c)`: the arguments folded from the left by the
/// binary function `head`.
///
/// The callers pass at least one argument; with none the fold is `()`,
/// so that the function is total.
pub(super) fn left_fold(head: &str, args: Vec<Form>, pos: &Pos) -> Form {
    let fold = |acc, arg| call(head, vec![acc, arg], pos);
    args.into_iter().reduce(fold).unwrap_or_else(|| unit(pos))
}

/// A fold of the binary function `head` over the call's arguments: the
/// call itself is declined below three arguments (below two for `alone`
/// of [`Alone::Itself`], which answers one argument with itself).
pub(super) fn fold(mut items: Vec<Form>, pos: &Pos, head: &str, alone: Alone) -> Outcome {
    match (items.len().saturating_sub(1), alone) {
        (1, Alone::Itself) => match items.pop() {
            Some(arg) => Outcome::Expanded(arg),
            None => declined(items, pos),
        },
        (n, _) if n >= 3 => {
            let args = items.into_iter().skip(1).collect();
            Outcome::Expanded(left_fold(head, args, pos))
        }
        _ => declined(items, pos),
    }
}

/// `max`, `min`: a fold of the library's function through `fib.core`.
pub(super) fn extremum(items: Vec<Form>, pos: &Pos, name: &str) -> Outcome {
    fold(items, pos, &format!("fib.core/{name}"), Alone::Declined)
}

/// `bit-and`, `bit-or`, `bit-xor`: a fold of the builtin.
pub(super) fn bits(items: Vec<Form>, pos: &Pos, name: &str) -> Outcome {
    fold(items, pos, &prelude_name(name), Alone::Declined)
}

/// `+` and `*`: `identity` for no argument, the argument itself for one.
pub(super) fn sum_or_product(items: Vec<Form>, pos: &Pos, name: &str, identity: i64) -> Outcome {
    match items.len().saturating_sub(1) {
        0 => Outcome::Expanded(int(identity, pos)),
        _ => fold(items, pos, &prelude_name(name), Alone::Itself),
    }
}

/// `-`: `(-)` is an error, `(- a)` is `(fib.prelude/neg a)`.
pub(super) fn difference(mut items: Vec<Form>, pos: &Pos) -> Result<Outcome, ExpandError> {
    check_arity("-", &items, 1, None, pos)?;
    if items.len() == 2 {
        if let Some(arg) = items.pop() {
            return Ok(Outcome::Expanded(call(
                &prelude_name("neg"),
                vec![arg],
                pos,
            )));
        }
    }
    Ok(fold(items, pos, &prelude_name("-"), Alone::Declined))
}

/// Whether evaluating `form` twice is the same as once and borrows: a
/// symbol, a literal or a field path `(. x f)` of such.
fn is_simple(form: &Form) -> bool {
    match &form.kind {
        FormKind::Vec(_) | FormKind::Map(_) => false,
        FormKind::List(items) => match items.split_first() {
            Some((head, rest)) if head.as_sym() == Some(".") => {
                !rest.is_empty() && rest.iter().all(is_simple)
            }
            _ => false,
        },
        _ => true,
    }
}

/// `(and (op o1 o2) (op o2 o3) ..)` over the operands, an operand that is
/// not simple bound first, in order, to a gensym.
fn chain(ctx: &ExpandCtx, op: &str, args: Vec<Form>, pos: &Pos) -> Form {
    let mut bindings = Vec::new();
    let mut operands = Vec::new();
    for arg in args {
        if is_simple(&arg) {
            operands.push(arg);
        } else {
            let t = ctx.gensym("cmp", pos);
            bindings.push(list(vec![t.clone(), arg], pos));
            operands.push(t);
        }
    }
    let tests = operands
        .windows(2)
        .map(|w| call(op, w.to_vec(), pos))
        .collect();
    let body = call("and", tests, pos);
    match bindings.is_empty() {
        true => body,
        false => call("let", vec![list(bindings, pos), body], pos),
    }
}

/// `< > <= >= =`: a chain from three arguments, `true` after evaluating
/// the one argument of a call of one, the binary call declined.
pub(super) fn comparison(ctx: &ExpandCtx, mut items: Vec<Form>, pos: &Pos, name: &str) -> Outcome {
    match items.len().saturating_sub(1) {
        1 => match items.pop() {
            Some(arg) => Outcome::Expanded(evaluate_then_true(ctx, arg, pos)),
            None => declined(items, pos),
        },
        n if n >= 3 => {
            let args = items.into_iter().skip(1).collect();
            Outcome::Expanded(chain(ctx, &prelude_name(name), args, pos))
        }
        _ => declined(items, pos),
    }
}

/// `(let ((#cmp.N arg)) true)`: `arg` evaluated once, then `true`.
fn evaluate_then_true(ctx: &ExpandCtx, arg: Form, pos: &Pos) -> Form {
    let t = ctx.gensym("cmp", pos);
    let binding = list(vec![list(vec![t, arg], pos)], pos);
    call("let", vec![binding, boolean(true, pos)], pos)
}
