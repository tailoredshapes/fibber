//! The model on vector patterns, guards and protocol dispatch.

use super::*;
use crate::ast::{Clause, Expr, ImplDef, Kind, Method, Pat, Rest};
use crate::ty::{Proto, Ty};

fn vec_of(xs: &[i64]) -> Expr {
    let items = xs.iter().map(|x| Expr::int(*x)).collect();
    Expr::new(Ty::vec(Ty::Int), Kind::VecLit(items))
}

fn prog(impls: Vec<ImplDef>, main: Expr) -> Program {
    Program {
        defs: Vec::new(),
        impls,
        funs: Vec::new(),
        main,
    }
}

fn int(h: &str, args: Vec<Expr>) -> Expr {
    Expr::call(Ty::Int, h, args)
}

fn var(n: &str, t: Ty) -> Expr {
    Expr::var(n, t)
}

/// `(let ((c (cell 0))) (+ (match v ([x & r] :when (do (set! c 1) (> x 5)) 1)
/// ([x & r] (count r)) ([] 0)) (* 10 @c)))` on `[1 2 3]`: the guard
/// runs once, is false, and the next clause binds its own rest.
#[test]
fn a_false_guard_falls_through_after_its_effect() {
    let ct = Ty::cell(Ty::Int);
    let rest =
        |x: &str, r: &str| Pat::Vector(vec![Pat::Bind(x.into())], Some(Rest::Bind(r.into())));
    let set = Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(var("c", ct.clone())), Box::new(Expr::int(1))),
    );
    let test = Expr::call(Ty::Bool, ">", vec![var("x", Ty::Int), Expr::int(5)]);
    let guard = Expr::new(Ty::Bool, Kind::Do(vec![set, test]));
    let clauses = vec![
        Clause {
            pat: rest("x", "r"),
            guard: Some(guard),
            body: Expr::int(1),
        },
        Clause {
            pat: rest("y", "s"),
            guard: None,
            body: int("count", vec![var("s", Ty::vec(Ty::Int))]),
        },
        Clause {
            pat: Pat::Vector(Vec::new(), None),
            guard: None,
            body: Expr::int(0),
        },
    ];
    let m = Expr::new(Ty::Int, Kind::GMatch(Box::new(vec_of(&[1, 2, 3])), clauses));
    let read = Expr::new(Ty::Int, Kind::Deref(Box::new(var("c", ct.clone()))));
    let body = int("+", vec![m, int("*", vec![Expr::int(10), read])]);
    let init = Expr::call(ct, "cell", vec![Expr::int(0)]);
    let main = Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind("c".into()), init)], Box::new(body)),
    );
    assert_eq!(expected(&prog(Vec::new(), main)), Ok(12));
}

/// `[0 y]` needs the literal; `[x y z & _]` any length from three.
#[test]
fn vector_patterns_match_by_length_and_literal() {
    let clauses = |v: &[i64]| {
        let cl = vec![
            (
                Pat::Vector(vec![Pat::Lit(0), Pat::Bind("y".into())], None),
                var("y", Ty::Int),
            ),
            (
                Pat::Vector(vec![Pat::Wild; 3], Some(Rest::Wild)),
                Expr::int(100),
            ),
            (Pat::Wild, Expr::int(7)),
        ];
        Expr::new(Ty::Int, Kind::Match(Box::new(vec_of(v)), cl))
    };
    let main = |v: &[i64]| prog(Vec::new(), clauses(v));
    assert_eq!(expected(&main(&[0, 4])), Ok(4));
    assert_eq!(expected(&main(&[1, 4])), Ok(7));
    assert_eq!(expected(&main(&[1, 2, 3, 4])), Ok(100));
}

fn pt(x: i64, y: i64) -> Expr {
    Expr::call(Ty::Pt, "Pt", vec![Expr::int(x), Expr::int(y)])
}

fn method(name: &str, body: Expr) -> Method {
    Method {
        name: name.into(),
        params: Vec::new(),
        body,
    }
}

/// `score` is the `impl`'s; `bonus` and `tier` are the defaults
/// `(+ (score self) k)` and `(+ (rank self) (score self))` unless the
/// `impl` gives them; a `dyn` dispatches on the value inside it.
#[test]
fn methods_dispatch_to_impls_and_defaults() {
    let x = Expr::new(
        Ty::Int,
        Kind::Field(Box::new(var("self", Ty::Pt)), "x".into()),
    );
    let impls = vec![
        ImplDef {
            proto: Proto::Score,
            target: Ty::Pt,
            methods: vec![method("score", x)],
        },
        ImplDef {
            proto: Proto::Rank,
            target: Ty::Pt,
            methods: vec![method("rank", Expr::int(100))],
        },
        ImplDef {
            proto: Proto::Score,
            target: Ty::Shape,
            methods: vec![
                method("score", Expr::int(1)),
                method("bonus", Expr::int(50)),
            ],
        },
    ];
    let d = Expr::new(
        Ty::Dyn(Proto::Rank, false),
        Kind::Dyn(Proto::Rank, false, Box::new(pt(3, 4))),
    );
    let circle = Expr::call(Ty::Shape, "Circle", vec![Expr::int(0)]);
    let main = int(
        "+",
        vec![
            int("tier", vec![d]),
            int(
                "+",
                vec![
                    int("bonus", vec![pt(5, 0), Expr::int(1000)]),
                    int("bonus", vec![circle, Expr::int(9)]),
                ],
            ),
        ],
    );
    assert_eq!(expected(&prog(impls, main)), Ok(103 + 1005 + 50));
}
