//! `list`, `plet` (§3.12), `->`, `->>`, `doto` (call rewriting) and
//! `assert`, `dbg` (over `if` and `trap`), §4.4. The heads and constants
//! they emit that are not core forms are `fib.prelude/NAME`.

use crate::syntax::{Form, FormKind, Pos};

use crate::expand::build::{call, check_arity, list, malformed, string, sym, unit};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::{ExpandError, ExpandErrorKind};

/// `(list a b)` ⟹ `(fib.prelude/Cons a (fib.prelude/Cons b
/// fib.prelude/Empty))`; `(list)` ⟹ `fib.prelude/Empty`. The variants are
/// qualified so that a function or a method named `cons` or `empty` in
/// scope (stdlib design C-3, C-4) is not what the list is built with.
pub(super) fn list_macro(items: Vec<Form>, pos: &Pos) -> Form {
    let cons = prelude_name("Cons");
    let mut acc = sym(&prelude_name("Empty"), pos);
    for item in items.into_iter().skip(1).rev() {
        acc = call(&cons, vec![item, acc], pos);
    }
    acc
}

/// `(plet ((s1 e1) ...) body)` ⟹ `(let ((t1 (fib.prelude/spawn (fn ()
/// e1))) ...) (let ((s1 (fib.prelude/join t1)) ...) body))`, `t1 ...`
/// gensyms (§3.12).
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
        // `(s expr)` or `(s: T expr)` (§1.5): the annotation types `s`.
        let (name, e) = match pair.as_list() {
            Some([s, e]) if s.as_sym().is_some() => (vec![s.clone()], e),
            Some([s, t, e]) if crate::expand::core::annotated_name(s) => {
                (vec![s.clone(), t.clone()], e)
            }
            _ => {
                let reason = "a binding is (sym expr) or (sym: type expr)";
                return Err(malformed("plet", reason, &pair.pos));
            }
        };
        let base = name[0].as_sym().map(|s| s.trim_end_matches(':'));
        let t = ctx.gensym(base.unwrap_or("t"), pos);
        let thunk = list(vec![sym("fn", pos), unit(pos), e.clone()], pos);
        let spawn = call(&prelude_name("spawn"), vec![thunk], pos);
        spawns.push(list(vec![t.clone(), spawn], pos));
        let mut join = name;
        join.push(call(&prelude_name("join"), vec![t], pos));
        joins.push(list(join, pos));
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

/// `(assert c)` / `(assert c msg)` ⟹ `(if c () (fib.prelude/trap msg))`, the default
/// message naming the call's position and the test.
pub(super) fn assert(items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    let name = "assert";
    check_arity(name, &items, 1, Some(2), pos)?;
    let mut it = items.into_iter().skip(1);
    let test = it.next().unwrap_or_else(|| unit(pos));
    let message = match it.next() {
        Some(m) => m,
        None => string(&format!("{name} failed at {pos}: {test}"), pos),
    };
    let fail = call(&prelude_name("trap"), vec![message], pos);
    Ok(call("if", vec![test, unit(pos), fail], pos))
}

/// `(dbg e)` ⟹ `(let ((t e)) (fib.prelude/eprintln (fib.prelude/str-concat
/// "dbg POS: e = " (fib.prelude/show t))) t)`: evaluates `e` once, prints
/// it with `Show` to standard error with the call's position and the
/// form as written, and returns it (§4.4).
pub(super) fn dbg(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    check_arity("dbg", &items, 1, Some(1), pos)?;
    let e = items.into_iter().nth(1).unwrap_or_else(|| unit(pos));
    let t = ctx.gensym("dbg", pos);
    let label = string(&format!("dbg {pos}: {e} = "), pos);
    let shown = call(&prelude_name("show"), vec![t.clone()], pos);
    let text = call(&prelude_name("str-concat"), vec![label, shown], pos);
    let print = call(&prelude_name("eprintln"), vec![text], pos);
    let binding = list(vec![list(vec![t.clone(), e], pos)], pos);
    Ok(call("let", vec![binding, print, t], pos))
}
