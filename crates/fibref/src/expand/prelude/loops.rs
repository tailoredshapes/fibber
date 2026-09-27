//! `while`, `dotimes` and `for-each` over a literal `range`, over
//! `loop`/`recur` (§3.18, §4.4), with exactly the table's expansions:
//!
//! ```text
//! (while c body)          ⟹ (loop () (if c (do body (recur)) ()))
//! (dotimes (i n) body)    ⟹ (loop ((i 0) (m n)) (if (< i m) (do body (recur (+ i 1) m)) ()))
//! (for-each (range a b) (fn (i) body))
//!                         ⟹ (loop ((i a) (m b)) (if (< i m) (do body (recur (+ i 1) m)) ()))
//! ```
//!
//! with `m` a gensym. Any other `for-each` is the library function.

use crate::syntax::{Form, FormKind, Pos};

use super::Outcome;
use crate::expand::build::{call, check_arity, int, list, malformed, unit};
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// `(while c body...)`.
pub(super) fn while_loop(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("while", &items, 1, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let test = it.next().unwrap_or_else(|| unit(pos));
    let mut steps: Vec<Form> = it.collect();
    steps.push(call("recur", Vec::new(), pos));
    let body = call("if", vec![test, call("do", steps, pos), unit(pos)], pos);
    Ok(call("loop", vec![unit(pos), body], pos))
}

/// The counting loop shared by `dotimes` and `for-each`.
fn counting_loop(i: Form, from: Form, to: Form, body: Vec<Form>, m: Form, pos: &Pos) -> Form {
    let bindings = list(
        vec![
            list(vec![i.clone(), from], pos),
            list(vec![m.clone(), to], pos),
        ],
        pos,
    );
    let next = call("+", vec![i.clone(), int(1, pos)], pos);
    let mut steps = body;
    steps.push(call("recur", vec![next, m.clone()], pos));
    let test = call("<", vec![i, m], pos);
    let branch = call("if", vec![test, call("do", steps, pos), unit(pos)], pos);
    call("loop", vec![bindings, branch], pos)
}

/// `(dotimes (i n) body...)`.
pub(super) fn dotimes(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("dotimes", &items, 1, None, pos)?;
    let (i, n) = match items[1].as_list() {
        Some([i, n]) if i.as_sym().is_some() => (i.clone(), n.clone()),
        _ => {
            return Err(malformed(
                "dotimes",
                "expected (dotimes (sym count) body)",
                &items[1].pos,
            ))
        }
    };
    let body: Vec<Form> = items.into_iter().skip(2).collect();
    let m = ctx.gensym("m", pos);
    Ok(counting_loop(i, int(0, pos), n, body, m, pos))
}

/// The `a b` of a literal `(range a b)`.
fn literal_range(form: &Form) -> Option<(Form, Form)> {
    match form.as_list()? {
        [head, a, b] if head.as_sym() == Some("range") => Some((a.clone(), b.clone())),
        _ => None,
    }
}

/// The `i` and body of a literal `(fn (i) body+)`: no name, one
/// unannotated parameter, no result annotation.
fn literal_fn(form: &Form) -> Option<(Form, Vec<Form>)> {
    let items = form.as_list()?;
    if items.len() < 3 || items[0].as_sym() != Some("fn") {
        return None;
    }
    let param = match items[1].as_list()? {
        [p] => p,
        _ => return None,
    };
    let name = param.as_sym()?;
    if name.ends_with(':') || name == "nil" || items[2].as_sym() == Some("->") {
        return None;
    }
    Some((param.clone(), items[2..].to_vec()))
}

/// `(for-each (range a b) (fn (i) body...))` becomes the loop; any other
/// `for-each` call is declined and stays a call of the library function.
pub(super) fn for_each(
    ctx: &ExpandCtx,
    items: Vec<Form>,
    pos: Pos,
) -> Result<Outcome, ExpandError> {
    let shape = match items.as_slice() {
        [_, r, f] => literal_range(r).zip(literal_fn(f)),
        _ => None,
    };
    let Some(((a, b), (i, body))) = shape else {
        return Ok(Outcome::Declined(Form::new(FormKind::List(items), pos)));
    };
    let m = ctx.gensym("m", &pos);
    Ok(Outcome::Expanded(counting_loop(i, a, b, body, m, &pos)))
}
