//! The syntactic `&` checks that §3.5 step 3 runs on the lowered
//! program before typing (types §6.5, §6.9; syntax §3.13 rules 1 and 3):
//!
//! - **distinct variables** (case 12): the `&` arguments of one call
//!   name distinct variables, else `variable x passed to more than one
//!   & parameter in call to f`;
//! - **async functions** (case 14): a `defun` with an `&` parameter `v`
//!   is an async function if an `async` form in its body (inside nested
//!   `fn`s too) mentions `v` (`@v`, `&v`, the target of `set!` or
//!   `set-field!`), or an `async` form is in tail position of its body;
//!   then `& parameter in async function: v in f`.
//!
//! Rule 5 (`& parameter v used as a value in f`) is enforced by
//! lowering (`types::lower`), which cannot resolve such a name to an
//! expression; it therefore precedes these two.

use crate::types::ast::{Arg, BindingKind, Expr, ExprKind, GlobalRef, Place};
use crate::types::decls::Globals;

use super::error::{OwnError, OwnErrorKind};

/// Runs both checks over every body of the program, in definition
/// order: `defun`s (and macros), `impl` methods, `def`s.
pub fn check(g: &Globals) -> Vec<OwnError> {
    let mut errors = Vec::new();
    for f in &g.funs {
        errors.extend(async_function(f));
        distinct_amps(g, &f.body, &mut errors);
    }
    for inst in &g.instances {
        for m in &inst.methods {
            distinct_amps(g, &m.body, &mut errors);
        }
    }
    for d in &g.defs {
        distinct_amps(g, &d.init, &mut errors);
    }
    errors
}

/// Calls `f` on `e` and every expression inside it, `fn` and `async`
/// bodies included, in pre-order.
pub(super) fn visit(e: &Expr, f: &mut dyn FnMut(&Expr)) {
    f(e);
    e.children(&mut |c| visit(c, f));
}

/// The direct sub-expressions of `e`, in evaluation order, with `e`'s
/// lifetime (what [`Expr::children`] visits).
pub(super) fn children_of(e: &Expr) -> Vec<&Expr> {
    let mut out: Vec<&Expr> = Vec::new();
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Global(_) | ExprKind::Quote(_) => {}
        ExprKind::Guarded(_) | ExprKind::And(_) | ExprKind::Or(_) | ExprKind::Elided => {}
        ExprKind::Call(h, args) => {
            out.push(h);
            out.extend(args.iter().filter_map(|a| match a {
                Arg::Expr(x) => Some(x),
                Arg::Amp(..) => None,
            }));
        }
        ExprKind::Fn(lit) => out.push(&lit.body),
        ExprKind::Let(bs, body) => {
            out.extend(bs.iter().map(|(_, x)| x));
            out.push(body);
        }
        ExprKind::If(c, t, f) => out.extend([c.as_ref(), t.as_ref(), f.as_ref()]),
        ExprKind::Do(es) | ExprKind::Recur(es) | ExprKind::Concat(es) => out.extend(es.iter()),
        ExprKind::Match(x, cls) => {
            out.push(x);
            for c in cls {
                out.extend(c.guard.as_ref());
                out.push(&c.body);
            }
        }
        ExprKind::Loop(vs, body) => {
            out.extend(vs.iter().map(|(_, x)| x));
            out.push(body);
        }
        ExprKind::Deref(p, _) => out.extend(place_expr(p)),
        ExprKind::Set(p, v) => {
            out.extend(place_expr(p));
            out.push(v);
        }
        ExprKind::Field(x, _, _)
        | ExprKind::SetField(_, _, x)
        | ExprKind::Async(x, _)
        | ExprKind::Await(x)
        | ExprKind::Unsafe(x)
        | ExprKind::Dyn(_, _, _, x)
        | ExprKind::Convert(_, _, x) => out.push(x),
    }
    out
}

fn place_expr(p: &Place) -> Option<&Expr> {
    match p {
        Place::Expr(x) => Some(x),
        Place::Amp(_) => None,
    }
}

/// The name a call's head is printed as in messages.
pub(super) fn head_name(g: &Globals, head: &Expr) -> String {
    match &head.kind {
        ExprKind::Global(GlobalRef::Fun(f)) => g.fun(*f).name.clone(),
        ExprKind::Global(GlobalRef::Builtin(b)) => crate::types::builtins::BUILTINS
            .get(b.0 as usize)
            .map(|s| s.name.to_string())
            .unwrap_or_default(),
        ExprKind::Global(GlobalRef::Method(p, i)) => g.proto(*p).methods[*i].name.clone(),
        ExprKind::Global(GlobalRef::Extern(x)) => g.ext(*x).name.clone(),
        ExprKind::Global(GlobalRef::Def(d)) => g.def(*d).name.clone(),
        ExprKind::Global(GlobalRef::Ctor(t, v)) => {
            let def = g.ty(*t);
            match (v, &def.shape) {
                (Some(i), crate::types::decls::Shape::Enum(vs)) => vs[*i].name.clone(),
                _ => def.name.clone(),
            }
        }
        ExprKind::Local(b) => g.binding(*b).name.clone(),
        _ => format!("the function at {}", head.pos),
    }
}

fn distinct_amps(g: &Globals, body: &Expr, errors: &mut Vec<OwnError>) {
    visit(body, &mut |e| {
        let ExprKind::Call(head, args) = &e.kind else {
            return;
        };
        let mut seen = Vec::new();
        let mut reported = Vec::new();
        for a in args {
            let Arg::Amp(b, _) = a else { continue };
            if seen.contains(b) && !reported.contains(b) {
                reported.push(*b);
                let msg = format!(
                    "variable {} passed to more than one & parameter in call to {}",
                    g.binding(*b).name,
                    head_name(g, head)
                );
                errors.push(OwnError::new(OwnErrorKind::AmpTwice, &e.pos, msg));
            }
            seen.push(*b);
        }
    });
}

/// The `&` parameters of `f` that make it an async function, each with
/// the position of the `async` form responsible.
fn async_function(f: &crate::types::decls::FunDef) -> Vec<OwnError> {
    let amps: Vec<_> = f.params.iter().filter(|p| p.amp).collect();
    if amps.is_empty() {
        return Vec::new();
    }
    let tail = tail_async(&f.body);
    let mut out = Vec::new();
    for p in amps {
        let pos = mentioning_async(&f.body, p.binding).or_else(|| tail.clone());
        if let Some(pos) = pos {
            let msg = format!("& parameter in async function: {} in {}", p.name, f.name);
            out.push(OwnError::new(OwnErrorKind::AmpInAsync, &pos, msg));
        }
    }
    out
}

/// The position of the first `async` form in `body` that mentions the
/// `&` parameter `v`.
fn mentioning_async(body: &Expr, v: crate::types::ast::BindingId) -> Option<crate::syntax::Pos> {
    let mut found = None;
    visit(body, &mut |e| {
        if found.is_none() {
            if let ExprKind::Async(inner, _) = &e.kind {
                if mentions(inner, v) {
                    found = Some(e.pos.clone());
                }
            }
        }
    });
    found
}

/// Whether `v` occurs in `e` as `@v`, `&v`, or the target of `set!` or
/// `set-field!`.
fn mentions(e: &Expr, v: crate::types::ast::BindingId) -> bool {
    let mut hit = false;
    visit(e, &mut |x| {
        hit |= match &x.kind {
            ExprKind::Deref(Place::Amp(b), _) | ExprKind::Set(Place::Amp(b), _) => *b == v,
            ExprKind::SetField(b, _, _) => *b == v,
            ExprKind::Call(_, args) => args.iter().any(|a| matches!(a, Arg::Amp(b, _) if *b == v)),
            _ => false,
        };
    });
    hit
}

/// The position of an `async` form in tail position of `e` (syntax
/// §3.1 (ii): the last step of a `do`, a branch of a tail `if`/`match`,
/// the body of a tail `let` or `loop`, transitively; not inside a `fn`).
fn tail_async(e: &Expr) -> Option<crate::syntax::Pos> {
    match &e.kind {
        ExprKind::Async(..) => Some(e.pos.clone()),
        ExprKind::Do(es) => es.last().and_then(tail_async),
        ExprKind::If(_, t, f) => tail_async(t).or_else(|| tail_async(f)),
        ExprKind::Let(_, body) | ExprKind::Loop(_, body) => tail_async(body),
        ExprKind::Match(_, cls) => cls.iter().find_map(|c| tail_async(&c.body)),
        _ => None,
    }
}

/// Whether the binding `b` is an `&` parameter.
pub(super) fn is_amp(g: &Globals, b: crate::types::ast::BindingId) -> bool {
    g.binding(b).kind == BindingKind::AmpParam
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expand::{expand_program, ExpandCtx, NoRunner};
    use crate::syntax::read_all;
    use crate::types::{lower_program, prelude_forms};

    fn errors(src: &str) -> Vec<String> {
        let mut ctx = ExpandCtx::new();
        let prelude = prelude_forms(&mut ctx).expect("prelude");
        let forms = read_all(src, "t.fib").expect("reads");
        let forms = expand_program(forms, &mut ctx, &mut NoRunner).expect("expands");
        let l = lower_program(&forms, &prelude).expect("lowers");
        check(&l.globals).into_iter().map(|e| e.message).collect()
    }

    #[test]
    fn the_same_variable_twice_is_reported_once_per_call() {
        let es = errors(
            "(defun bar (&a &b) (push! &a 1))
             (defun main () -> i64 (let ((x (cell [3]))) (do (bar &x &x) 0)))",
        );
        assert_eq!(
            es,
            vec!["variable x passed to more than one & parameter in call to bar"]
        );
    }

    #[test]
    fn two_names_for_one_cell_pass_the_check() {
        let es = errors(
            "(defun bar (&a &b) (push! &a 1))
             (defun main () -> i64 (let ((x (cell [3]))) (let ((y x)) (do (bar &x &y) 0))))",
        );
        assert!(es.is_empty(), "{es:?}");
    }

    #[test]
    fn an_async_mentioning_the_parameter_or_in_tail_position_is_async() {
        let mention = errors("(defun fill2 (&buf) (do (async (push! &buf 1)) ()))");
        assert_eq!(mention, vec!["& parameter in async function: buf in fill2"]);
        let tail = errors("(defun fill3 (&buf) (do (push! &buf 1) (async 5)))");
        assert_eq!(tail, vec!["& parameter in async function: buf in fill3"]);
        let nested = errors("(defun f (&v) (do (async (fn () @v)) 0))");
        assert_eq!(nested, vec!["& parameter in async function: v in f"]);
    }

    #[test]
    fn a_driven_task_that_does_not_mention_the_parameter_is_not_async() {
        let es = errors(
            "(defun bump (&v) (let ((n (block-on (async (do (await (yield)) 1))))) (push! &v n)))",
        );
        assert!(es.is_empty(), "{es:?}");
    }
}
