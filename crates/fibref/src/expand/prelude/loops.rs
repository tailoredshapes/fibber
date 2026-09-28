//! `while`, `dotimes` and `for-each` over a literal `range`, over
//! `loop`/`recur` (§3.18, §4.4), with exactly the table's expansions:
//!
//! ```text
//! (while c body)          ⟹ (loop () (if c (do body (recur)) ()))
//! (dotimes (i n) body)    ⟹ (let ((m n)) (loop ((i 0)) (if (< i m) (do body (recur (+ i 1))) ())))
//! (for-each (range a b) (fn (i) body))
//!                         ⟹ (let ((s a) (m b)) (loop ((i s)) (if (< i m) (do body (recur (+ i 1))) ())))
//! (range a b)             ⟹ (range-between a b)
//! ```
//!
//! with `s` and `m` gensyms; `(range n)` is `(range 0 n)` in the loop.
//! The bounds are evaluated once, left to right, before the loop
//! variable exists. Any other `for-each`, and `(range n)` outside it,
//! is the library function.

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

/// The counting loop shared by `dotimes` and `for-each`: `bounds` are
/// the `(gensym expr)` pairs bound, in order, before the loop; `start`
/// and `limit` name the bound values.
struct Counting {
    i: Form,
    bounds: Vec<Form>,
    start: Form,
    limit: Form,
}

fn counting_loop(c: Counting, body: Vec<Form>, pos: &Pos) -> Form {
    let next = call("+", vec![c.i.clone(), int(1, pos)], pos);
    let mut steps = body;
    steps.push(call("recur", vec![next], pos));
    let test = call("<", vec![c.i.clone(), c.limit], pos);
    let branch = call("if", vec![test, call("do", steps, pos), unit(pos)], pos);
    let vars = list(vec![list(vec![c.i, c.start], pos)], pos);
    let lp = call("loop", vec![vars, branch], pos);
    call("let", vec![list(c.bounds, pos), lp], pos)
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
    let c = Counting {
        i,
        bounds: vec![list(vec![m.clone(), n], pos)],
        start: int(0, pos),
        limit: m,
    };
    Ok(counting_loop(c, body, pos))
}

/// The `a b` of a literal `(range a b)`, or `0 n` of `(range n)`
/// (Decided, owner, 2026-09-27: `(range n)` is `(range 0 n)`).
fn literal_range(form: &Form) -> Option<(Form, Form)> {
    match form.as_list()? {
        [head, a, b] if head.as_sym() == Some("range") => Some((a.clone(), b.clone())),
        [head, n] if head.as_sym() == Some("range") => Some((int(0, &form.pos), n.clone())),
        _ => None,
    }
}

/// `(range a b)` ⟹ `(range-between a b)`; `(range n)` is declined and
/// stays a call of the library function `range` (§4.4, §4.5). There is
/// no arity overloading, so the two-argument form is this rewrite.
pub(super) fn range(items: Vec<Form>, pos: Pos) -> Result<Outcome, ExpandError> {
    check_arity("range", &items, 1, Some(2), &pos)?;
    if let [_, a, b] = items.as_slice() {
        let call = call("range-between", vec![a.clone(), b.clone()], &pos);
        return Ok(Outcome::Expanded(call));
    }
    Ok(Outcome::Declined(Form::new(FormKind::List(items), pos)))
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
    let start = ctx.gensym("s", &pos);
    let m = ctx.gensym("m", &pos);
    let c = Counting {
        i,
        bounds: vec![
            list(vec![start.clone(), a], &pos),
            list(vec![m.clone(), b], &pos),
        ],
        start,
        limit: m,
    };
    Ok(Outcome::Expanded(counting_loop(c, body, &pos)))
}
