//! Small builders for the tests of the pipeline's printer, model,
//! generator and coverage: lambdas over `x`, literal vectors, programs.

use crate::ast::{Expr, Kind, Pat, Program};
use crate::pipe::{Chain, Pipe, Shape, Src, Stage, Term};
use crate::ty::Ty;

pub fn x() -> Expr {
    Expr::var("x", Ty::Int)
}

pub fn a() -> Expr {
    Expr::var("a", Ty::Int)
}

pub fn n(v: i64) -> Expr {
    Expr::int(v)
}

pub fn op(h: &str, l: Expr, r: Expr) -> Expr {
    Expr::call(Ty::Int, h, vec![l, r])
}

pub fn cmp(h: &str, l: Expr, r: Expr) -> Expr {
    Expr::call(Ty::Bool, h, vec![l, r])
}

fn func(params: &[&str], ret: Ty, body: Expr) -> Expr {
    let ps = params.iter().map(|p| (p.to_string(), Ty::Int)).collect();
    let ty = Ty::Func(vec![Ty::Int; params.len()], Box::new(ret));
    Expr::new(ty, Kind::Fn(ps, Box::new(body)))
}

/// `(fn (x: i64) body)`.
pub fn lam(body: Expr) -> Expr {
    func(&["x"], Ty::Int, body)
}

/// `(fn (x: i64) pred)`.
pub fn plam(body: Expr) -> Expr {
    func(&["x"], Ty::Bool, body)
}

/// `(fn (x: i64) [..])`.
pub fn vlam(items: Vec<Expr>) -> Expr {
    func(&["x"], Ty::vec(Ty::Int), vec_of(items))
}

/// `(fn (a: i64 x: i64) body)`.
pub fn step(body: Expr) -> Expr {
    func(&["a", "x"], Ty::Int, body)
}

pub fn vec_of(items: Vec<Expr>) -> Expr {
    Expr::new(Ty::vec(Ty::Int), Kind::VecLit(items))
}

/// `[v ..]`.
pub fn vecs(items: &[i64]) -> Expr {
    vec_of(items.iter().map(|v| n(*v)).collect())
}

/// A source vector.
pub fn src(items: &[i64]) -> Src {
    Src::Vec(vecs(items))
}

/// `body` after `(set! calls (+ @calls w))`.
pub fn counted(w: i64, body: Expr) -> Expr {
    let cell = || Expr::var("calls", Ty::cell(Ty::Int));
    let read = Expr::new(Ty::Int, Kind::Deref(Box::new(cell())));
    let set = Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(cell()), Box::new(op("+", read, n(w)))),
    );
    Expr::new(body.ty.clone(), Kind::Do(vec![set, body]))
}

/// A pipeline over `items`, in the shape `shape`, one terminal.
pub fn pipe(items: &[i64], stages: Vec<Stage>, term: Term, shape: Shape) -> Pipe {
    Pipe {
        chain: Chain {
            src: src(items),
            stages,
        },
        terms: vec![term],
        shape,
    }
}

/// A program whose `main` is the pipeline.
pub fn program(p: Pipe) -> Program {
    Program {
        defs: Vec::new(),
        impls: Vec::new(),
        funs: Vec::new(),
        main: Expr::new(Ty::Int, Kind::Pipe(Box::new(p))),
    }
}

/// A program that binds the call counter and returns the pipeline's
/// answer times 1000 plus the calls.
pub fn counting(p: Pipe) -> Program {
    let pipe = Expr::new(Ty::Int, Kind::Pipe(Box::new(p)));
    let calls = Expr::new(
        Ty::Int,
        Kind::Deref(Box::new(Expr::var("calls", Ty::cell(Ty::Int)))),
    );
    let both = op("+", op("*", Expr::var("r", Ty::Int), n(1000)), calls);
    let inner = Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind("r".into()), pipe)], Box::new(both)),
    );
    let cell = Expr::call(Ty::cell(Ty::Int), "cell", vec![n(0)]);
    let main = Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind("calls".into()), cell)], Box::new(inner)),
    );
    Program {
        defs: Vec::new(),
        impls: Vec::new(),
        funs: Vec::new(),
        main,
    }
}
