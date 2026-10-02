//! `for` and `doseq` (stdlib §4.3, §4.4; tranche 2 X3, D2): the
//! comprehensions, Rust macros because stage 2a has no macro runner.
//!
//! `(for [x xs :when p :let [y e] z zs] body)` and `doseq` take a binding
//! vector of levels, `pattern source`, each followed by its modifiers
//! `:when p`, `:while p`, `:let [pattern e ..]`. A level is one nesting.
//! The expansions, innermost level first:
//!
//! ```text
//! (for [x xs] b)                ⟹ (fib.seq/map (fn (x) b) xs)
//! (for [x xs :when p] b)        ⟹ (fib.seq/map (fn (x) b) (fib.seq/filter (fn (x) p) xs))
//! (for [x xs :while p] b)       ⟹ (fib.seq/map (fn (x) b) (fib.seq/take-while (fn (x) p) xs))
//! (for [x xs :let [y e]] b)     ⟹ (fib.seq/map (fn (x) (let ((y e)) b)) xs)
//! (for [x xs :let [y e] :when p] b)
//!                               ⟹ (fib.seq/keep (fn (x) (let ((y e)) (if p (fib.prelude/some b) nil))) xs)
//! (for [x xs y ys] b)           ⟹ (fib.seq/mapcat (fn (x) (fib.seq/map (fn (y) b) ys)) xs)
//! (doseq [x xs ..] b..)         the same levels over (fib.prelude/for-each xs (fn (x) ..)),
//!                               a guard after a :let being (if p .. ())
//! ```
//!
//! A `:when` or `:while` before the level's first `:let` is applied to the
//! source (`filter`, `take-while`, in the order written, which is the order
//! Clojure tests them); one after a `:let` can see the let's names, so it
//! guards the level's body (`keep` over an `Option` for the innermost level
//! of a `for`, an `if` over the empty `take 0` of the inner sequence for
//! an outer one). A `:while` after a `:let` is not supported (`malformed`):
//! it ends the whole level, which no guard can say. `doseq` over a literal
//! `(range a b)` and a one-parameter `fn` reaches the counting loop of
//! `for-each` (§6.3). A `for` is lazy (an `LSeq`), a `doseq` is unit. The
//! pattern of a level is the parameter of the functions, so it may be any
//! pattern that `fn` takes.

use crate::syntax::{Form, FormKind, Pos};

use super::logic::body_form;
use crate::expand::build::{call, check_arity, int, list, malformed, unit};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// Which macro: the sequence it builds or the loop it runs.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    For,
    Doseq,
}

/// A modifier of a level.
enum Step {
    When(Form),
    While(Form),
    Let(Vec<Form>),
}

/// One `pattern source` with its modifiers.
struct Level {
    pat: Form,
    src: Form,
    steps: Vec<Step>,
}

/// A modifier from its keyword and argument.
fn step(name: &str, kw: &str, arg: &Form) -> Result<Step, ExpandError> {
    match (kw, &arg.kind) {
        ("when", _) => Ok(Step::When(arg.clone())),
        ("while", _) => Ok(Step::While(arg.clone())),
        ("let", FormKind::Vec(b) | FormKind::List(b)) if b.len() % 2 == 0 => Ok(Step::Let(
            b.chunks(2).map(|c| list(c.to_vec(), &arg.pos)).collect(),
        )),
        ("let", _) => Err(malformed(
            name,
            ":let takes [pattern expression ..]",
            &arg.pos,
        )),
        _ => Err(malformed(
            name,
            "a modifier is :when, :while or :let",
            &arg.pos,
        )),
    }
}

/// The levels of a binding vector.
fn levels(name: &str, form: &Form) -> Result<Vec<Level>, ExpandError> {
    let items = match &form.kind {
        FormKind::Vec(items) | FormKind::List(items) if !items.is_empty() => items,
        _ => {
            return Err(malformed(
                name,
                "the bindings are [pattern source ..]",
                &form.pos,
            ))
        }
    };
    let mut out: Vec<Level> = Vec::new();
    for pair in items.chunks(2) {
        let [head, arg] = pair else {
            return Err(malformed(
                name,
                "a binding is a pattern and a source",
                &pair[0].pos,
            ));
        };
        match &head.kind {
            FormKind::Kw(kw) => match out.last_mut() {
                Some(level) => level.steps.push(step(name, kw, arg)?),
                None => return Err(malformed(name, "a modifier follows a binding", &head.pos)),
            },
            _ => out.push(Level {
                pat: head.clone(),
                src: arg.clone(),
                steps: Vec::new(),
            }),
        }
    }
    Ok(out)
}

/// `(fn (pat) body)`.
fn lambda(pat: &Form, body: Form, pos: &Pos) -> Form {
    call("fn", vec![list(vec![pat.clone()], pos), body], pos)
}

/// The level's source with the modifiers before its first `:let` applied,
/// and the modifiers from the `:let` on.
fn leading(level: Level, pos: &Pos) -> (Form, Vec<Step>) {
    let Level { pat, src, steps } = level;
    let mut src = src;
    let mut rest = steps.into_iter().peekable();
    while let Some(s) = rest.next_if(|s| !matches!(s, Step::Let(_))) {
        let (head, test) = match s {
            Step::When(p) => ("fib.seq/filter", p),
            Step::While(p) => ("fib.seq/take-while", p),
            Step::Let(_) => break,
        };
        src = call(head, vec![lambda(&pat, test, pos), src], pos);
    }
    (src, rest.collect())
}

/// What a guard `:when p` makes of `acc`, the form of the rest of the
/// level: an `if` with the empty value of its kind for the else.
fn guard(ctx: &ExpandCtx, kind: Kind, inner: bool, p: Form, acc: Form, pos: &Pos) -> Form {
    match (kind, inner) {
        (Kind::Doseq, _) => call("if", vec![p, acc, unit(pos)], pos),
        (Kind::For, true) => call(
            "if",
            vec![p, acc, Form::new(FormKind::Nil, pos.clone())],
            pos,
        ),
        (Kind::For, false) => {
            let s = ctx.gensym("s", pos);
            let none = call("fib.seq/take", vec![int(0, pos), s.clone()], pos);
            let test = call("if", vec![p, s.clone(), none], pos);
            let binding = list(vec![list(vec![s, acc], pos)], pos);
            call("let", vec![binding, test], pos)
        }
    }
}

/// The body of a level's function: the leaf under the modifiers after the
/// first `:let`, innermost last.
fn wrap(
    ctx: &ExpandCtx,
    name: &str,
    kind: Kind,
    inner: bool,
    steps: Vec<Step>,
    leaf: Form,
    pos: &Pos,
) -> Result<Form, ExpandError> {
    let mut acc = leaf;
    for s in steps.into_iter().rev() {
        acc = match s {
            Step::When(p) => guard(ctx, kind, inner, p, acc, pos),
            Step::Let(pairs) => call("let", vec![list(pairs, pos), acc], pos),
            Step::While(p) => {
                let why = ":while after :let is not supported; write it before the :let";
                return Err(malformed(name, why, &p.pos));
            }
        };
    }
    Ok(acc)
}

/// One level around `acc`, the form of the levels inside it (the body for
/// the innermost).
fn level_form(
    ctx: &ExpandCtx,
    name: &str,
    kind: Kind,
    level: Level,
    acc: Form,
    inner: bool,
    pos: &Pos,
) -> Result<Form, ExpandError> {
    let pat = level.pat.clone();
    let (src, steps) = leading(level, pos);
    let guarded = steps.iter().any(|s| matches!(s, Step::When(_)));
    let keep = kind == Kind::For && inner && guarded;
    let leaf = if keep {
        call(&prelude_name("some"), vec![acc], pos)
    } else {
        acc
    };
    let f = lambda(&pat, wrap(ctx, name, kind, inner, steps, leaf, pos)?, pos);
    Ok(match (kind, inner, keep) {
        (Kind::Doseq, _, _) => call(&prelude_name("for-each"), vec![src, f], pos),
        (Kind::For, false, _) => call("fib.seq/mapcat", vec![f, src], pos),
        (Kind::For, true, true) => call("fib.seq/keep", vec![f, src], pos),
        (Kind::For, true, false) => call("fib.seq/map", vec![f, src], pos),
    })
}

/// `(for [x xs ..] body..)` and `(doseq [x xs ..] body..)`.
fn comprehension(
    ctx: &ExpandCtx,
    items: Vec<Form>,
    pos: &Pos,
    kind: Kind,
) -> Result<Form, ExpandError> {
    let name = if kind == Kind::For { "for" } else { "doseq" };
    check_arity(
        name,
        &items,
        if kind == Kind::For { 2 } else { 1 },
        None,
        pos,
    )?;
    let levels = levels(name, &items[1])?;
    let mut forms: Vec<Form> = items.into_iter().skip(2).collect();
    // a doseq body is for effect: its value is dropped, so a guard's unit else unifies with it
    let body = match kind {
        Kind::For => body_form(forms, pos),
        Kind::Doseq => {
            forms.push(unit(pos));
            call("do", forms, pos)
        }
    };
    let mut acc = body;
    for (i, level) in levels.into_iter().rev().enumerate() {
        acc = level_form(ctx, name, kind, level, acc, i == 0, pos)?;
    }
    Ok(acc)
}

/// `(for [x xs ..] body..)`.
pub(super) fn for_macro(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    comprehension(ctx, items, pos, Kind::For)
}

/// `(doseq [x xs ..] body..)`.
pub(super) fn doseq(ctx: &ExpandCtx, items: Vec<Form>, pos: &Pos) -> Result<Form, ExpandError> {
    comprehension(ctx, items, pos, Kind::Doseq)
}
