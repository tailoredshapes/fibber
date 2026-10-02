//! `if-not`, `when-not`, `some`, `declare` and the presence macros
//! `if-some`, `when-some`, `when-first` (stdlib §4.3, §4.4; tranche 2 X3).
//!
//! ```text
//! (if-not c a b)            ⟹ (if (fib.prelude/not c) a b)     one-armed: the unit else
//! (when-not c body..)       ⟹ (if c () (do body..))
//! (some p c)                ⟹ (fib.seq/some-p p c)
//! (some x)                  declined: the constructor of `Option`
//! (declare n..)             ⟹ (do)
//! (if-some [x e] a b)       ⟹ (match e ((fib.prelude/some x) a) (_ b))
//! (when-some [x e] body..)  ⟹ (match e ((fib.prelude/some x) body..) (_ ()))
//! (when-first [x c] body..) ⟹ (match (fib.seq/first c) ((fib.prelude/some x) body..) (_ ()))
//! ```
//!
//! Decided here (the page is silent): the one-armed `if-not` writes its
//! unit else as `when` does, because the core `if` takes two arms until
//! L20; a binding is `[pattern e]` or `(pattern e)`, as the table writes
//! it and as `if-let` does; `some` of another count is the arity error
//! `macro some takes 1 or 2 argument(s), got N`; a name of `declare` that
//! is not a symbol is malformed.

use crate::syntax::{Form, FormKind, Pos};

use super::fold::declined;
use super::logic::{body_form, option_match, when};
use super::Outcome;
use crate::expand::build::{call, check_arity, malformed, unit};
use crate::expand::collections::prelude_name;
use crate::expand::error::{ExpandError, ExpandErrorKind};

/// `(if-not c a b)` and the one-armed `(if-not c a)`.
pub(super) fn if_not(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("if-not", &items, 2, Some(3), pos)?;
    let mut it = items.into_iter().skip(1);
    let test = it.next().unwrap_or_else(|| unit(pos));
    let then = it.next().unwrap_or_else(|| unit(pos));
    let other = it.next().unwrap_or_else(|| unit(pos));
    let negated = call(&prelude_name("not"), vec![test], pos);
    Ok(call("if", vec![negated, then, other], pos))
}

/// `(when-not c body..)`: `unless`, which the table keeps as its alias.
pub(super) fn when_not(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    when(items, pos, true, "when-not")
}

/// `(some p c)` is `fib.seq/some-p`; `(some x)` is the constructor and is
/// declined; any other count is an error.
pub(super) fn some(items: Vec<Form>, pos: Pos) -> Result<Outcome, ExpandError> {
    match items.len() {
        2 => Ok(declined(items, &pos)),
        3 => {
            let args: Vec<Form> = items.into_iter().skip(1).collect();
            Ok(Outcome::Expanded(call("fib.seq/some-p", args, &pos)))
        }
        n => {
            let kind = ExpandErrorKind::MacroArity {
                name: "some".to_string(),
                expected: "1 or 2".to_string(),
                found: n.saturating_sub(1),
            };
            Err(ExpandError::new(kind, &pos))
        }
    }
}

/// `(declare n..)`: nothing, since every name is bound before any body is
/// checked (syntax §3.1). Each name must be a symbol.
pub(super) fn declare(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    match items.iter().skip(1).find(|n| n.as_sym().is_none()) {
        Some(bad) => Err(malformed("declare", "a name is a symbol", &bad.pos)),
        None => Ok(call("do", Vec::new(), pos)),
    }
}

/// The `(pattern e)` of a binding written `[pattern e]` or `(pattern e)`.
pub(super) fn pair_of(name: &str, form: &Form) -> Result<(Form, Form), ExpandError> {
    match &form.kind {
        FormKind::List(p) | FormKind::Vec(p) if p.len() == 2 => Ok((p[0].clone(), p[1].clone())),
        _ => Err(malformed(
            name,
            "the binding is [pattern expression]",
            &form.pos,
        )),
    }
}

/// `(if-some [x e] a b)`; the else is `()` when there is none.
pub(super) fn if_some(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("if-some", &items, 2, Some(3), pos)?;
    let (x, e) = pair_of("if-some", &items[1])?;
    let mut it = items.into_iter().skip(2);
    let then = it.next().unwrap_or_else(|| unit(pos));
    let other = it.next().unwrap_or_else(|| unit(pos));
    Ok(option_match(x, e, then, other, pos))
}

/// `(when-some [x e] body..)`.
pub(super) fn when_some(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("when-some", &items, 1, None, pos)?;
    let (x, e) = pair_of("when-some", &items[1])?;
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    Ok(option_match(x, e, body, unit(pos), pos))
}

/// `(when-first [x c] body..)`: `when-some` over `(fib.seq/first c)`.
pub(super) fn when_first(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("when-first", &items, 1, None, pos)?;
    let (x, c) = pair_of("when-first", &items[1])?;
    let first = call("fib.seq/first", vec![c], pos);
    let body = body_form(items.into_iter().skip(2).collect(), pos);
    Ok(option_match(x, first, body, unit(pos), pos))
}
