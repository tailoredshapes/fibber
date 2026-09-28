//! Minimising a failing program: greedy delta debugging over the typed
//! tree. Every candidate keeps each node's type (a node is replaced by
//! the smallest value of its type or by one of its children of the same
//! type, a `let` binding, `do` step or vector element is dropped), and a
//! candidate is kept only if the caller's predicate says it still fails
//! the same way.

use std::collections::HashSet;

use crate::ast::{Expr, Kind, Pat, Program};
use crate::ty::Ty;

/// The smallest value of `ty`, mentioning no variable.
pub fn smallest(ty: &Ty) -> Expr {
    let e = |k| Expr::new(ty.clone(), k);
    let call = |h: &str, args: Vec<Expr>| Expr::call(ty.clone(), h, args);
    let s = |x: &str| Expr::new(Ty::Str, Kind::Str(x.into()));
    match ty {
        Ty::Unit => e(Kind::Unit),
        Ty::Int => Expr::int(0),
        Ty::Bool => e(Kind::Bool(false)),
        Ty::Str => s(""),
        Ty::Opt(_) => e(Kind::Nil),
        Ty::List => e(Kind::Empty),
        Ty::Vec(_) => e(Kind::VecLit(Vec::new())),
        Ty::Pt => call("Pt", vec![Expr::int(0), Expr::int(0)]),
        Ty::Wrap => call("Wrap", vec![s(""), smallest(&Ty::vec(Ty::Int))]),
        Ty::Holder => call(
            "Holder",
            vec![smallest(&Ty::fn_i()), smallest(&Ty::cell(Ty::Int))],
        ),
        Ty::Shape => call("Circle", vec![Expr::int(0)]),
        Ty::Boxed(t) => call("Box", vec![smallest(t)]),
        Ty::Cell(t) => call("cell", vec![smallest(t)]),
        Ty::Atom(t) => call("atom", vec![smallest(t)]),
        Ty::Task(t) => e(Kind::Async(Box::new(smallest(t)))),
        Ty::Weak(t) => e(Kind::WeakDead("dead".into(), Box::new(smallest(t)))),
        Ty::Func(ps, r) => {
            let params = ps
                .iter()
                .enumerate()
                .map(|(i, t)| (format!("z{i}"), t.clone()))
                .collect();
            e(Kind::Fn(params, Box::new(smallest(r))))
        }
    }
}

fn is_smallest(e: &Expr) -> bool {
    matches!(
        e.kind,
        Kind::Int(0)
            | Kind::Bool(false)
            | Kind::Unit
            | Kind::Nil
            | Kind::Empty
            | Kind::Var(_)
            | Kind::Global(_)
    ) || matches!(&e.kind, Kind::Str(s) if s.is_empty())
        || matches!(&e.kind, Kind::VecLit(v) if v.is_empty())
}

/// The candidates that replace node `e`, smallest first.
fn candidates(e: &Expr) -> Vec<Expr> {
    let mut out = Vec::new();
    if !is_smallest(e) {
        out.push(smallest(&e.ty));
    }
    for c in e.children() {
        if c.ty == e.ty {
            out.push(c.clone());
        } else if c.size() + 2 < e.size() {
            // Keep one part's evaluation, drop the rest: `(do c smallest)`.
            out.push(Expr::new(
                e.ty.clone(),
                Kind::Do(vec![c.clone(), smallest(&e.ty)]),
            ));
        }
    }
    out.extend(removals(e));
    out
}

/// `e` with one `let` binding, `do` step or vector element removed, or
/// a destructuring `let` pattern replaced by a name.
fn removals(e: &Expr) -> Vec<Expr> {
    let with = |k: Kind| Expr::new(e.ty.clone(), k);
    let mut out = Vec::new();
    match &e.kind {
        Kind::Let(bs, b) if bs.len() > 1 => {
            for i in 0..bs.len() {
                let mut bs2 = bs.clone();
                bs2.remove(i);
                out.push(with(Kind::Let(bs2, b.clone())));
            }
        }
        Kind::Let(bs, b) => {
            if let Some((Pat::Ctor(..) | Pat::As(..), init)) = bs.first() {
                let bind = vec![(Pat::Bind("shrunk".into()), init.clone())];
                out.push(with(Kind::Let(bind, b.clone())));
            }
        }
        Kind::Do(steps) if steps.len() > 1 => {
            for i in 0..steps.len() - 1 {
                let mut s2 = steps.clone();
                s2.remove(i);
                out.push(if s2.len() == 1 {
                    s2.remove(0)
                } else {
                    with(Kind::Do(s2))
                });
            }
        }
        Kind::VecLit(items) => {
            for i in 0..items.len() {
                let mut it = items.clone();
                it.remove(i);
                out.push(with(Kind::VecLit(it)));
            }
        }
        _ => {}
    }
    out
}

/// The node with pre-order number `idx` across the program's bodies.
fn node_mut(p: &mut Program, mut idx: usize) -> Option<&mut Expr> {
    let defs = p.defs.iter_mut().map(|d| &mut d.init);
    let bodies = defs
        .chain(p.funs.iter_mut().map(|f| &mut f.body))
        .chain(std::iter::once(&mut p.main));
    for b in bodies {
        let n = b.size();
        if idx < n {
            return find(b, idx);
        }
        idx -= n;
    }
    None
}

fn find(e: &mut Expr, idx: usize) -> Option<&mut Expr> {
    if idx == 0 {
        return Some(e);
    }
    let mut rest = idx - 1;
    for c in e.children_mut() {
        let n = c.size();
        if rest < n {
            return find(c, rest);
        }
        rest -= n;
    }
    None
}

/// `p` without the helpers `main` cannot reach.
pub fn prune(p: &Program) -> Program {
    let mut reach: HashSet<String> = HashSet::new();
    let mut todo: Vec<&Expr> = vec![&p.main];
    while let Some(e) = todo.pop() {
        e.walk(&mut |n| {
            if let Kind::Call(h, _) | Kind::Global(h) = &n.kind {
                if reach.insert(h.clone()) {
                    if let Some(f) = p.funs.iter().find(|f| &f.name == h) {
                        todo.push(&f.body);
                    }
                }
            }
        });
    }
    let funs: Vec<_> = p
        .funs
        .iter()
        .filter(|f| reach.contains(&f.name))
        .cloned()
        .collect();
    let mut used: HashSet<String> = HashSet::new();
    for b in funs
        .iter()
        .map(|f| &f.body)
        .chain([&p.main])
        .chain(p.defs.iter().map(|d| &d.init))
    {
        b.walk(&mut |n| {
            if let Kind::Var(v) | Kind::Global(v) = &n.kind {
                used.insert(v.clone());
            }
        });
    }
    let defs = p
        .defs
        .iter()
        .filter(|d| used.contains(&d.name))
        .cloned()
        .collect();
    Program {
        defs,
        funs,
        main: p.main.clone(),
    }
}

/// The pre-order numbers of every node, largest subtree first.
fn by_size(p: &Program) -> Vec<usize> {
    let mut sizes = Vec::new();
    for b in p.bodies() {
        b.walk(&mut |e| sizes.push(e.size()));
    }
    let mut idx: Vec<usize> = (0..sizes.len()).collect();
    idx.sort_by(|a, b| sizes[*b].cmp(&sizes[*a]).then(a.cmp(b)));
    idx
}

/// Shrinks `p` while `still_fails` holds, calling it at most `budget`
/// times; returns the smallest failing program found. Largest subtrees
/// are tried first; a candidate already tried is not run again.
pub fn shrink(
    p: &Program,
    budget: usize,
    still_fails: &mut dyn FnMut(&Program) -> bool,
) -> Program {
    let mut best = prune(p);
    let mut best_len = crate::print::program(&best).len();
    let mut tried: HashSet<String> = HashSet::new();
    let mut calls = 0;
    'outer: while calls < budget {
        for idx in by_size(&best) {
            let Some(node) = node_mut(&mut best.clone(), idx).map(|n| n.clone()) else {
                continue;
            };
            for cand in candidates(&node) {
                let mut trial = best.clone();
                if let Some(slot) = node_mut(&mut trial, idx) {
                    *slot = cand;
                }
                let trial = prune(&trial);
                let text = crate::print::program(&trial);
                // Only strictly shorter programs: the search terminates and
                // never trades one node for a bigger replacement.
                if text.len() >= best_len || !tried.insert(text) {
                    continue;
                }
                calls += 1;
                if still_fails(&trial) {
                    best_len = crate::print::program(&trial).len();
                    best = trial;
                    continue 'outer;
                }
                if calls >= budget {
                    break 'outer;
                }
            }
        }
        break;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::expected;

    fn plus(a: Expr, b: Expr) -> Expr {
        Expr::call(Ty::Int, "+", vec![a, b])
    }

    #[test]
    fn shrinks_to_the_part_that_matters() {
        // "Fails" while the result is still 7 and the program has a 7 in it.
        let main = plus(
            plus(Expr::int(3), Expr::int(4)),
            plus(Expr::int(0), Expr::int(0)),
        );
        let p = Program {
            defs: Vec::new(),
            funs: Vec::new(),
            main,
        };
        let small = shrink(&p, 1000, &mut |q| expected(q) == Ok(7));
        assert_eq!(expected(&small), Ok(7));
        assert!(small.size() <= 3, "{small:?}");
    }

    #[test]
    fn smallest_values_evaluate() {
        for t in crate::ty::universe() {
            let p = Program {
                defs: Vec::new(),
                funs: Vec::new(),
                main: Expr::new(Ty::Int, Kind::Do(vec![smallest(&t), Expr::int(1)])),
            };
            assert_eq!(expected(&p), Ok(1), "{t}");
        }
    }
}
