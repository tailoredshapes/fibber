//! `when`, `unless`, `cond` (over the one-armed `if`) and `if-let`,
//! `when-let` (over `match`), §4.4. `and` and `or` are core forms the
//! checker elaborates (stdlib §7 L20), not macros.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::brackets::seq_items;
use crate::expand::build::{boolean, call, check_arity, list, malformed, sym, unit};
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

/// `(when c body...)` ⟹ `(if c body)`, the one-armed `if` (stdlib §7 L20:
/// unit for a unit body, `(Option T)` otherwise); `(unless c body...)` and
/// `(when-not c body...)` ⟹ `(if (not c) body)`; `name` is the one the
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
    let test = if negate {
        call(&prelude_name("not"), vec![test], pos)
    } else {
        test
    };
    let body = body_form(it.collect(), pos);
    Ok(call("if", vec![test, body], pos))
}

/// `(cond t1 e1 t2 e2 ...)`, Clojure's flat form: nested `if`s, tried in
/// order. A test that is a keyword (`:else`) is always true and must be the
/// last one, as is a test that is the literal `true`. Without one, the last test is a one-armed `if`: falling off
/// the end is `nil` (when the bodies are values, `some` around each) or
/// `()` (when they are unit), stdlib §7 L20; `(cond)` is `()`. An odd
/// number of forms is a test with no expression.
pub(super) fn cond(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    let mut forms: Vec<Form> = items.into_iter().skip(1).collect();
    if forms.len() % 2 == 1 {
        let at = forms.last().map_or(pos, |f| &f.pos);
        return Err(malformed("cond", "a test with no expression", at));
    }
    let mut acc: Option<Form> = None;
    let mut last = true;
    while let (Some(expr), Some(test)) = (forms.pop(), forms.pop()) {
        if matches!(test.kind, FormKind::Kw(_) | FormKind::Bool(true)) {
            if !last {
                return Err(malformed(
                    "cond",
                    "a keyword test must be the last",
                    &test.pos,
                ));
            }
            acc = Some(expr);
        } else {
            let mut args = vec![test, expr];
            args.extend(acc);
            acc = Some(call("if", args, pos));
        }
        last = false;
    }
    Ok(acc.unwrap_or_else(|| unit(pos)))
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

/// `(when-let (p e) body...)` ⟹ `(match e ((fib.prelude/some p) (if true
/// body)) (_ (fib.prelude/elide)))`: the body is one-armed, so the value is
/// unit for a unit body and `(Option T)` otherwise, and `elide`, which the
/// checker reads, takes the type of the other arm and is `()` or `nil`
/// accordingly (stdlib §7 L20).
pub(super) fn when_let(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("when-let", &items, 1, None, pos)?;
    let (x, e) = option_binding("when-let", &items[1])?;
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    let guarded = call("if", vec![boolean(true, pos), body], pos);
    let elide = call(&prelude_name("elide"), vec![], pos);
    Ok(option_match(x, e, guarded, elide, pos))
}
