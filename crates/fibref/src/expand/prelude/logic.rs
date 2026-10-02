//! `when`, `unless`, `cond`, `and`, `or` (over `if`) and `if-let`,
//! `when-let` (over `match`), §4.4.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::brackets::seq_items;
use crate::expand::build::{boolean, call, check_arity, list, malformed, string, sym, unit};
use crate::expand::collections::prelude_name;
use crate::expand::error::ExpandError;

/// The body forms as one expression: the form itself when there is one,
/// `(do body...)` otherwise (`(do)`, unit, when there are none).
pub(crate) fn body_form(mut body: Vec<Form>, pos: &Pos) -> Form {
    if body.len() == 1 {
        if let Some(only) = body.pop() {
            return only;
        }
    }
    call("do", body, pos)
}

/// `(when c body...)` ⟹ `(if c body ())`; `(unless c body...)` and
/// `(when-not c body...)` ⟹ `(if c () body)`; `name` is the one the
/// arity error names.
pub(super) fn when(
    items: Vec<Form>,
    pos: &Pos,
    negate: bool,
    name: &str,
) -> Result<Form, ExpandError> {
    check_arity(name, &items, 1, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let test = it.next().unwrap_or_else(|| unit(pos));
    let body = body_form(it.collect(), pos);
    let (then, other) = if negate {
        (unit(pos), body)
    } else {
        (body, unit(pos))
    };
    Ok(call("if", vec![test, then, other], pos))
}

/// `(and)` ⟹ `true`, `(and a)` ⟹ `a`, `(and a b ...)` ⟹ `(if a (and b
/// ...) false)`; `or` dually with `(if a true (or b ...))`, the inner
/// call written `fib.prelude/and` (`fib.prelude/or`), which reaches the
/// macro even in a module that defines an `and` of its own or a user
/// `defmacro and` (R14).
pub(super) fn and_or(items: Vec<Form>, pos: &Pos, is_and: bool) -> Form {
    let name = &prelude_name(if is_and { "and" } else { "or" });
    let mut args: Vec<Form> = items.into_iter().skip(1).collect();
    if args.len() <= 1 {
        return args.pop().unwrap_or_else(|| boolean(is_and, pos));
    }
    let rest = args.split_off(1);
    let first = args.pop().unwrap_or_else(|| boolean(is_and, pos));
    let rest = call(name, rest, pos);
    let (then, other) = if is_and {
        (rest, boolean(false, pos))
    } else {
        (boolean(true, pos), rest)
    };
    call("if", vec![first, then, other], pos)
}

/// `(cond t1 e1 t2 e2 ...)`, Clojure's flat form: nested `if`s, tried in
/// order. A test that is a keyword (`:else`) is always true and must be the
/// last one; without one, falling off the end is
/// `(fib.prelude/trap "cond: no clause matched at POS")`, which has every
/// type, so `(cond)` is that trap. An odd number of forms is a test with no
/// expression.
pub(super) fn cond(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    let mut forms: Vec<Form> = items.into_iter().skip(1).collect();
    if forms.len() % 2 == 1 {
        let at = forms.last().map_or(pos, |f| &f.pos);
        return Err(malformed("cond", "a test with no expression", at));
    }
    let message = format!("cond: no clause matched at {pos}");
    let mut acc = call(&prelude_name("trap"), vec![string(&message, pos)], pos);
    let mut last = true;
    while let (Some(expr), Some(test)) = (forms.pop(), forms.pop()) {
        if matches!(test.kind, FormKind::Kw(_)) {
            if !last {
                return Err(malformed(
                    "cond",
                    "a keyword test must be the last",
                    &test.pos,
                ));
            }
            acc = expr;
        } else {
            acc = call("if", vec![test, expr, acc], pos);
        }
        last = false;
    }
    Ok(acc)
}

/// The `(x e)` or `[x e]` binding of `if-let`/`when-let`.
fn option_binding(name: &str, form: &Form) -> Result<(Form, Form), ExpandError> {
    match seq_items(form) {
        Some([x, e]) => Ok((x.clone(), e.clone())),
        _ => Err(malformed(
            name,
            "the binding is [pattern expression]",
            &form.pos,
        )),
    }
}

/// `(match e ((fib.prelude/some p) then) (_ other))`. The else is a
/// wildcard, not `nil`: the pattern `p` may be refutable (`[a b]` matches
/// a vector of two and no other), and the else is taken on a mismatch of
/// the pattern as well as on `nil` (stdlib §2.4, §7 E13). With a `nil`
/// clause the match would be non-exhaustive (`missing (some [])`) for every
/// pattern that does not cover its type.
pub(super) fn option_match(x: Form, e: Form, then: Form, other: Form, pos: &Pos) -> Form {
    let some = list(vec![call(&prelude_name("some"), vec![x], pos), then], pos);
    let rest = list(vec![sym("_", pos), other], pos);
    list(vec![sym("match", pos), e, some, rest], pos)
}

/// `(if-let (p e) a b)` ⟹ `(match e ((fib.prelude/some p) a) (_ b))`;
/// `(if-let (p e) a)` has `()` as its else (§4.4, stdlib §2.4).
pub(super) fn if_let(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("if-let", &items, 2, Some(3), pos)?;
    let (x, e) = option_binding("if-let", &items[1])?;
    let mut it = items.into_iter().skip(2);
    let then = it.next().unwrap_or_else(|| unit(pos));
    let other = it.next().unwrap_or_else(|| unit(pos));
    Ok(option_match(x, e, then, other, pos))
}

/// `(when-let (p e) body...)` ⟹ `(match e ((fib.prelude/some p) body)
/// (_ ()))`.
pub(super) fn when_let(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("when-let", &items, 1, None, pos)?;
    let (x, e) = option_binding("when-let", &items[1])?;
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    Ok(option_match(x, e, body, unit(pos), pos))
}
