//! The model on vector patterns, guards, protocol dispatch, the
//! colour-parameterised enum, float comparisons and `rem`, and weak
//! references and atoms holding a `dyn`.

use super::*;
use crate::ast::{Clause, Expr, ImplDef, Kind, Method, Pat, Rest};
use crate::ty::{NumTy, Proto, Ty};

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
            colour_var: false,
        },
        ImplDef {
            proto: Proto::Rank,
            target: Ty::Pt,
            methods: vec![method("rank", Expr::int(100))],
            colour_var: false,
        },
        ImplDef {
            proto: Proto::Score,
            target: Ty::Shape,
            methods: vec![
                method("score", Expr::int(1)),
                method("bonus", Expr::int(50)),
            ],
            colour_var: false,
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

/// `(do (spawn (fn () (rem 1 0))) 0)`: a task whose handle is dropped
/// still runs (syntax §3.12), so its trap is the program's.
#[test]
fn a_discarded_spawn_still_runs_and_its_trap_counts() {
    let bad = int("rem", vec![Expr::int(1), Expr::int(0)]);
    let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(bad)));
    let spawn = Expr::call(Ty::task(Ty::Int), "spawn", vec![f]);
    let main = Expr::new(Ty::Int, Kind::Do(vec![spawn, Expr::int(0)]));
    assert_eq!(
        expected(&prog(Vec::new(), main)),
        Err(ModelError::Trap("integer rem by zero".into()))
    );
}

/// `sum-vec` is the preamble's loop of `+`, so a sum past `i64` traps.
#[test]
fn sum_vec_traps_on_overflow() {
    let big = Expr::int(i64::MAX / 2 + 1);
    let v = Expr::new(Ty::vec(Ty::Int), Kind::VecLit(vec![big.clone(), big]));
    assert_eq!(
        expected(&prog(Vec::new(), int("sum-vec", vec![v]))),
        Err(ModelError::Trap("integer overflow in + at i64".into()))
    );
}

/// `(match j ((Idle) 1) ((Ready r) (join (spawn r))))`: the
/// colour-parameterised enum's variants, its closure run on a thread.
#[test]
fn jobs_match_by_variant_and_their_closures_run() {
    let run = |j: Expr| {
        let r = Expr::var("r", Ty::fn_0());
        let t = Expr::call(Ty::task(Ty::Int), "spawn", vec![r]);
        let clauses = vec![
            (Pat::Ctor("Idle".into(), Vec::new()), Expr::int(1)),
            (
                Pat::Ctor("Ready".into(), vec![Pat::Bind("r".into())]),
                int("join", vec![t]),
            ),
        ];
        prog(
            Vec::new(),
            Expr::new(Ty::Int, Kind::Match(Box::new(j), clauses)),
        )
    };
    let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(Expr::int(7))));
    let ready = Expr::call(Ty::Job(true), "Ready", vec![f]);
    assert_eq!(expected(&run(ready)), Ok(7));
    let idle = Expr::new(Ty::Job(false), Kind::Var("Idle".into()));
    assert_eq!(expected(&run(idle)), Ok(1));
}

fn f64_lit(x: f64) -> Expr {
    Expr::new(Ty::Num(NumTy::F64), Kind::Flt(x, NumTy::F64))
}

fn f64_op(h: &str, a: Expr, b: Expr) -> Expr {
    Expr::call(Ty::Num(NumTy::F64), h, vec![a, b])
}

/// Comparisons with a NaN are false but `!=` (types §2.12, Decided);
/// `rem` on floats is `fmod`, with the sign of the dividend.
#[test]
fn float_comparisons_are_ieee_and_rem_is_fmod() {
    let nan = || f64_op("/", f64_lit(0.0), f64_lit(0.0));
    let bit = |h: &str, a: Expr, b: Expr| {
        let c = Expr::call(Ty::Bool, h, vec![a, b]);
        Expr::new(
            Ty::Int,
            Kind::If(Box::new(c), Box::new(Expr::int(1)), Box::new(Expr::int(0))),
        )
    };
    let cases = [
        (bit("<=", nan(), f64_lit(1.0)), 0),
        (bit(">=", nan(), f64_lit(1.0)), 0),
        (bit("=", nan(), nan()), 0),
        (bit("!=", nan(), nan()), 1),
        (
            bit("<", f64_lit(1.0), f64_op("/", f64_lit(1.0), f64_lit(0.0))),
            1,
        ),
    ];
    for (e, want) in cases {
        assert_eq!(expected(&prog(Vec::new(), e)), Ok(want));
    }
    let rem = |a: f64, b: f64| {
        let r = f64_op("rem", f64_lit(a), f64_lit(b));
        let scaled = f64_op("*", r, f64_lit(10.0));
        let e = Expr::new(Ty::Int, Kind::Conv("fptosi".into(), None, Box::new(scaled)));
        expected(&prog(Vec::new(), e))
    };
    assert_eq!(rem(7.5, 2.0), Ok(15));
    assert_eq!(rem(-7.5, 2.0), Ok(-15));
    assert_eq!(rem(7.5, -2.0), Ok(15));
    assert_eq!(rem(1.0, 0.0), Ok(0));
}

/// A `(dyn Score :send)` held by a weak reference and by an atom: the
/// upgrade finds it (it is bound around both) and the atom reads it.
#[test]
fn weak_and_atom_of_a_dyn_reach_its_object() {
    let x = Expr::new(
        Ty::Int,
        Kind::Field(Box::new(var("self", Ty::Pt)), "x".into()),
    );
    let impls = vec![ImplDef {
        proto: Proto::Score,
        target: Ty::Pt,
        methods: vec![method("score", x)],
        colour_var: false,
    }];
    let dt = Ty::Dyn(Proto::Score, true);
    let d = Expr::new(
        dt.clone(),
        Kind::Dyn(Proto::Score, true, Box::new(pt(3, 4))),
    );
    let w = Expr::call(Ty::weak(dt.clone()), "weak", vec![var("d", dt.clone())]);
    let a = Expr::call(Ty::atom(dt.clone()), "atom", vec![var("d", dt.clone())]);
    let up = Expr::new(
        Ty::opt(dt.clone()),
        Kind::Deref(Box::new(var("w", Ty::weak(dt.clone())))),
    );
    let clauses = vec![
        (
            Pat::Some(Box::new(Pat::Bind("y".into()))),
            int("score", vec![var("y", dt.clone())]),
        ),
        (Pat::Nil, Expr::int(0)),
    ];
    let m = Expr::new(Ty::Int, Kind::Match(Box::new(up), clauses));
    let read = Expr::new(dt.clone(), Kind::Deref(Box::new(var("a", Ty::atom(dt)))));
    let body = int("+", vec![m, int("score", vec![read])]);
    let binds = vec![
        (Pat::Bind("d".into()), d),
        (Pat::Bind("w".into()), w),
        (Pat::Bind("a".into()), a),
    ];
    let main = Expr::new(Ty::Int, Kind::Let(binds, Box::new(body)));
    assert_eq!(expected(&prog(impls, main)), Ok(6));
}
