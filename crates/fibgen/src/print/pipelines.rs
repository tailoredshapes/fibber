//! Printing a [`Pipe`] (`crate::pipe`) as the forms a program writes:
//! threaded with `->>`, as nested calls, with the stages bound by `let`,
//! or bound once and read by two terminals. Every shape means the same
//! value, which is the point: the expander fuses the nested and the
//! threaded forms and cannot fuse a bound seq, so one pipeline reaches
//! both paths of the library.

use crate::ast::Kind;
use crate::pipe::{Chain, Operand, Pipe, Shape, Src, Stage, Term, NONE};

use super::{atom, expr, form, list, Sexp};

/// The name of the seq a `let` binds after the first `k` stages.
fn bound(k: usize) -> Sexp {
    atom(format!("sq{k}"))
}

/// `(digest e)`: the preamble's fold of a `Vec` to a number.
fn digest(e: Sexp) -> Sexp {
    form("digest", [e])
}

/// The source as a form.
fn src(s: &Src) -> Sexp {
    match s {
        Src::Vec(e) | Src::List(e) => expr(e),
        Src::Range(n) => form("range", [expr(n)]),
    }
}

/// The operand of a `concat`.
fn operand(o: &Operand) -> Sexp {
    match o {
        Operand::Vec(e) => expr(e),
        Operand::Lazy(c) => nested(c, c.stages.len(), src(&c.src)),
        Operand::Eager(c) => form("vec", [nested(c, c.stages.len(), src(&c.src))]),
    }
}

/// The call of `s` over `prev`.
fn stage_call(s: &Stage, prev: Sexp) -> Sexp {
    match s {
        Stage::Concat {
            other,
            first: false,
        } => form("concat", [prev, operand(other)]),
        _ => {
            let mut step = stage_step(s);
            step.push(prev);
            list(step)
        }
    }
}

/// The items of `s` before the seq it is applied to: `[map f]`.
fn stage_step(s: &Stage) -> Vec<Sexp> {
    let head = atom(s.name());
    match s {
        Stage::Map(e)
        | Stage::Filter(e)
        | Stage::Remove(e)
        | Stage::Take(e)
        | Stage::Drop(e)
        | Stage::TakeWhile(e)
        | Stage::Mapcat(e) => vec![head, expr(e)],
        Stage::Concat { other, .. } => vec![head, operand(other)],
    }
}

/// The first `k` stages of `c` as nested calls over `base`.
fn nested(c: &Chain, k: usize, base: Sexp) -> Sexp {
    c.stages
        .iter()
        .take(k)
        .fold(base, |prev, s| stage_call(s, prev))
}

/// The step of a `reduce`, with its body under the `reduced` test when
/// the term has a `stop`. The macro that reads `reduced` (R6b) takes a
/// literal `(fn (acc x) body)` with two bare parameters, so that is how
/// the step is written then.
fn reduce_fn(f: &crate::ast::Expr, stop: Option<i64>) -> Sexp {
    let (Some(limit), Kind::Fn(params, body)) = (stop, &f.kind) else {
        return expr(f);
    };
    let over = form(">", [atom("a"), atom(limit.to_string())]);
    let test = form("if", [over, form("reduced", [atom("a")]), expr(body)]);
    let bare = params.iter().map(|(p, _)| atom(p.clone())).collect();
    form("fn", [list(bare), test])
}

/// The calls of the terminal, each as the items before the seq (the
/// library's argument order puts the seq last, which `->>` relies on),
/// or `None` for `nth`, whose seq comes first.
fn term_steps(t: &Term) -> Option<Vec<Vec<Sexp>>> {
    let one = |items: Vec<Sexp>| Some(vec![items]);
    match t {
        Term::Reduce { f, init, stop } => {
            one(vec![atom("reduce"), reduce_fn(f, *stop), expr(init)])
        }
        Term::Reduce1(f) => one(vec![atom("reduce"), expr(f)]),
        Term::Count => one(vec![atom("count")]),
        Term::Vec => one(vec![atom("vec")]),
        Term::First => one(vec![atom("first")]),
        Term::Last => one(vec![atom("last")]),
        Term::Empty => one(vec![atom("empty?")]),
        Term::Sum => one(vec![atom("sum")]),
        Term::Sort => Some(vec![vec![atom("sort")], vec![atom("vec")]]),
        Term::Every(p) => one(vec![atom("every?"), expr(p)]),
        Term::FindFirst(p) => one(vec![atom("find-first"), expr(p)]),
        Term::SortBy(k) => Some(vec![vec![atom("sort-by"), expr(k)], vec![atom("vec")]]),
        Term::Nth(_) => None,
    }
}

/// The number the terminal's answer `s` stands for.
fn fold(t: &Term, s: Sexp) -> Sexp {
    let none = atom(NONE.to_string());
    let some_v = |body: Sexp| list(vec![list(vec![atom("some"), atom("v")]), body]);
    match t {
        Term::Vec | Term::Sort | Term::SortBy(_) => digest(s),
        Term::First => form(
            "match",
            [s, list(vec![atom("nil"), none]), some_v(atom("v"))],
        ),
        Term::Last => form("unwrap-or", [s, none]),
        Term::Empty | Term::Every(_) => form("if", [s, atom("1"), atom("0")]),
        Term::FindFirst(_) => {
            let plus = form("+", [atom("v"), atom("1")]);
            form("match", [s, list(vec![atom("nil"), none]), some_v(plus)])
        }
        Term::Reduce { .. } | Term::Reduce1(_) | Term::Count | Term::Sum | Term::Nth(_) => s,
    }
}

/// The terminal applied to the seq `s` as a call: its answer, folded.
fn term_call(t: &Term, s: Sexp) -> Sexp {
    let answer = match term_steps(t) {
        Some(steps) => steps.into_iter().fold(s, |prev, mut step| {
            step.push(prev);
            list(step)
        }),
        None => match t {
            Term::Nth(i) => form("nth", [s, expr(i)]),
            _ => s,
        },
    };
    fold(t, answer)
}

/// `(->> src step ..)`, or `None` when something cannot be threaded
/// last: `nth`, and a `concat` whose seq is its first argument.
fn thread(p: &Pipe) -> Option<Sexp> {
    let steps = term_steps(&p.terms[0])?;
    let mut items = vec![atom("->>"), src(&p.chain.src)];
    for s in &p.chain.stages {
        if matches!(s, Stage::Concat { first: false, .. }) {
            return None;
        }
        items.push(list(stage_step(s)));
    }
    items.extend(steps.into_iter().map(list));
    Some(fold(&p.terms[0], list(items)))
}

/// The first `k` stages bound by a `let`, the others nested over the
/// last name, under the terminal.
fn bound_stages(p: &Pipe, k: usize) -> Sexp {
    let c = &p.chain;
    let mut binds = Vec::new();
    let mut prev = src(&c.src);
    for (i, s) in c.stages.iter().take(k).enumerate() {
        binds.push(list(vec![bound(i + 1), stage_call(s, prev)]));
        prev = bound(i + 1);
    }
    let rest: Vec<Stage> = c.stages.iter().skip(k).cloned().collect();
    let tail = nested(
        &Chain {
            src: c.src.clone(),
            stages: rest,
        },
        usize::MAX,
        prev,
    );
    let call = term_call(&p.terms[0], tail);
    if binds.is_empty() {
        call
    } else {
        form("let", [list(binds), call])
    }
}

/// One seq bound to a name and read by two terminals, in order.
fn twice(p: &Pipe) -> Sexp {
    let c = &p.chain;
    let seq = nested(c, c.stages.len(), src(&c.src));
    let (a, b) = (&p.terms[0], &p.terms[1]);
    let first = form("*", [term_call(a, bound(0)), atom("7")]);
    let sum = form("+", [first, term_call(b, bound(0))]);
    form("let", [list(vec![list(vec![bound(0), seq])]), sum])
}

/// The pipeline as one `i64` form.
pub(crate) fn pipe(p: &Pipe) -> Sexp {
    if p.twice() {
        return twice(p);
    }
    let n = p.chain.stages.len();
    match p.shape {
        Shape::Thread => thread(p).unwrap_or_else(|| bound_stages(p, 0)),
        Shape::Nested => bound_stages(p, 0),
        Shape::Bound => bound_stages(p, n),
        Shape::Split(k) => bound_stages(p, k.min(n)),
    }
}

#[cfg(test)]
mod tests;
