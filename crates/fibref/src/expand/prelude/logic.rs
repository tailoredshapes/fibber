//! `when`, `unless`, `cond`, `and`, `or` (over `if`) and `if-let`,
//! `when-let` (over `match`), §4.4.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{boolean, call, check_arity, list, malformed, string, sym, unit};
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

/// `(when c body...)` ⟹ `(if c body ())`; `(unless c body...)` ⟹ `(if
/// c () body)`.
pub(super) fn when(items: Vec<Form>, pos: &Pos, negate: bool) -> Result<Form, ExpandError> {
    let name = if negate { "unless" } else { "when" };
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
/// ...) false)`; `or` dually with `(if a true (or b ...))`.
pub(super) fn and_or(items: Vec<Form>, pos: &Pos, is_and: bool) -> Form {
    let name = if is_and { "and" } else { "or" };
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

/// Whether a `cond` clause test is the catch-all `else` or `:else`.
fn is_else(test: &Form) -> bool {
    match &test.kind {
        FormKind::Sym(s) => s == "else",
        FormKind::Kw(k) => k == "else",
        _ => false,
    }
}

/// `(cond (test body+)*)`: nested `if`s, tried in order. A final clause
/// whose test is `else` or `:else` is the default; without one, falling
/// off the end is `(trap "cond: no clause matched at POS")`, which has
/// every type, so `(cond)` is that trap.
pub(super) fn cond(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    let clauses: Vec<Form> = items.into_iter().skip(1).collect();
    if let Some(bad) = clauses
        .iter()
        .find(|c| c.as_list().is_none_or(|p| p.len() < 2))
    {
        return Err(malformed("cond", "a clause is (test body+)", &bad.pos));
    }
    let message = format!("cond: no clause matched at {pos}");
    let mut acc = call("trap", vec![string(&message, pos)], pos);
    for (i, clause) in clauses.into_iter().rev().enumerate() {
        let cpos = clause.pos;
        let mut parts = match clause.kind {
            FormKind::List(parts) if parts.len() >= 2 => parts,
            _ => return Err(malformed("cond", "a clause is (test body+)", &cpos)),
        };
        let body = body_form(parts.split_off(1), &cpos);
        let test = parts.pop().unwrap_or_else(|| boolean(true, &cpos));
        if is_else(&test) {
            if i != 0 {
                return Err(malformed("cond", "else must be the last clause", &cpos));
            }
            acc = body;
        } else {
            acc = call("if", vec![test, body, acc], pos);
        }
    }
    Ok(acc)
}

/// The `(x e)` binding of `if-let`/`when-let`.
fn option_binding(name: &str, form: &Form) -> Result<(Form, Form), ExpandError> {
    match form.as_list() {
        Some([x, e]) => Ok((x.clone(), e.clone())),
        _ => Err(malformed(
            name,
            "the binding is (pattern expression)",
            &form.pos,
        )),
    }
}

/// `(match e ((some x) then) (nil other))`.
fn option_match(x: Form, e: Form, then: Form, other: Form, pos: &Pos) -> Form {
    let some = list(vec![call("some", vec![x], pos), then], pos);
    let none = list(vec![Form::new(FormKind::Nil, pos.clone()), other], pos);
    list(vec![sym("match", pos), e, some, none], pos)
}

/// `(if-let (x e) a b)` ⟹ `(match e ((some x) a) (nil b))` (§4.4).
pub(super) fn if_let(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("if-let", &items, 3, Some(3), pos)?;
    let (x, e) = option_binding("if-let", &items[1])?;
    let mut it = items.into_iter().skip(2);
    let then = it.next().unwrap_or_else(|| unit(pos));
    let other = it.next().unwrap_or_else(|| unit(pos));
    Ok(option_match(x, e, then, other, pos))
}

/// `(when-let (x e) body...)` ⟹ `(match e ((some x) body) (nil ()))`.
pub(super) fn when_let(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("when-let", &items, 1, None, pos)?;
    let (x, e) = option_binding("when-let", &items[1])?;
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    Ok(option_match(x, e, body, unit(pos), pos))
}
