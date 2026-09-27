//! `list`, `plet` (§3.12), `->`, `->>`, `doto` (call rewriting) and
//! `assert`, `dbg` (over `if` and `trap`), §4.4.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{call, check_arity, list, malformed, string, sym, unit};
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::{ExpandError, ExpandErrorKind};

/// `(list a b)` ⟹ `(cons a (cons b empty))`; `(list)` ⟹ `empty`.
pub(super) fn list_macro(items: Vec<Form>, pos: &Pos) -> Form {
    let mut acc = sym("empty", pos);
    for item in items.into_iter().skip(1).rev() {
        acc = call("cons", vec![item, acc], pos);
    }
    acc
}

/// `(plet ((s1 e1) ...) body)` ⟹ `(let ((t1 (spawn (fn () e1))) ...)
/// (let ((s1 (join t1)) ...) body))`, `t1 ...` gensyms (§3.12).
pub(super) fn plet(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("plet", &items, 2, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let bindings = it.next().unwrap_or_else(|| unit(pos));
    let body: Vec<Form> = it.collect();
    let pairs = match bindings.as_list() {
        Some(pairs) if !pairs.is_empty() => pairs,
        _ => return Err(malformed("plet", "expected ((sym expr)+)", &bindings.pos)),
    };
    let mut spawns = Vec::new();
    let mut joins = Vec::new();
    for pair in pairs {
        let (s, e) = match pair.as_list() {
            Some([s, e]) if s.as_sym().is_some() => (s, e),
            _ => return Err(malformed("plet", "a binding is (sym expr)", &pair.pos)),
        };
        let t = ctx.gensym(s.as_sym().unwrap_or("t"), pos);
        let thunk = list(vec![sym("fn", pos), unit(pos), e.clone()], pos);
        spawns.push(list(vec![t.clone(), call("spawn", vec![thunk], pos)], pos));
        joins.push(list(vec![s.clone(), call("join", vec![t], pos)], pos));
    }
    let mut inner = vec![sym("let", pos), list(joins, pos)];
    inner.extend(body);
    let outer = vec![sym("let", pos), list(spawns, pos), list(inner, pos)];
    Ok(list(outer, pos))
}

/// Puts `x` into the step: first argument when `first`, else last. A
/// symbol step `f` is the call `(f x)`.
fn thread_step(step: Form, x: Form, first: bool) -> Result<Form, ExpandError> {
    let pos = step.pos;
    match step.kind {
        FormKind::Sym(_) => {
            let f = Form::new(step.kind, pos.clone());
            Ok(list(vec![f, x], &pos))
        }
        FormKind::List(mut items) if !items.is_empty() => {
            if first {
                items.insert(1, x);
            } else {
                items.push(x);
            }
            Ok(Form::new(FormKind::List(items), pos))
        }
        _ => Err(ExpandError::new(ExpandErrorKind::ThreadStep, &pos)),
    }
}

/// `(-> x (f a) g)` ⟹ `(g (f x a))`; `->>` puts `x` last instead.
pub(super) fn thread(items: Vec<Form>, pos: &Pos, first: bool) -> Result<Form, ExpandError> {
    let name = if first { "->" } else { "->>" };
    check_arity(name, &items, 1, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let mut acc = it.next().unwrap_or_else(|| unit(pos));
    for step in it {
        acc = thread_step(step, acc, first)?;
    }
    Ok(acc)
}

/// `(doto x (f a) g)` ⟹ `(let ((t x)) (f t a) (g t) t)`, `t` a gensym.
pub(super) fn doto(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("doto", &items, 1, None, pos)?;
    let t = ctx.gensym("doto", pos);
    let mut it = items.into_iter().skip(1);
    let x = it.next().unwrap_or_else(|| unit(pos));
    let binding = list(vec![list(vec![t.clone(), x], pos)], pos);
    let mut out = vec![sym("let", pos), binding];
    for step in it {
        out.push(thread_step(step, t.clone(), true)?);
    }
    out.push(t);
    Ok(list(out, pos))
}

/// `(assert c)` / `(assert c msg)` ⟹ `(if c () (trap msg))`, the default
/// message naming the call's position and the test. `dbg` is the same
/// with a `dbg` message (see the module docs of `expand`).
pub(super) fn assert(name: &str, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity(name, &items, 1, Some(2), pos)?;
    let mut it = items.into_iter().skip(1);
    let test = it.next().unwrap_or_else(|| unit(pos));
    let message = match it.next() {
        Some(m) => m,
        None => string(&format!("{name} failed at {pos}: {test}"), pos),
    };
    let fail = call("trap", vec![message], pos);
    Ok(call("if", vec![test, unit(pos), fail], pos))
}
