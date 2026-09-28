use super::*;
use crate::ast::{Arg, Expr, FunDef, Kind, Param, Pat};
use crate::ty::Ty;

fn int_call(h: &str, args: Vec<Expr>) -> Expr {
    Expr::call(Ty::Int, h, args)
}

fn inout_call(h: &str, cells: &[&str], ret: Ty) -> Expr {
    let args = cells.iter().map(|c| Arg::InOut(c.to_string())).collect();
    Expr::new(ret, Kind::Call(h.into(), args))
}

fn vec_of(xs: &[i64]) -> Expr {
    Expr::new(
        Ty::vec(Ty::Int),
        Kind::VecLit(xs.iter().map(|x| Expr::int(*x)).collect()),
    )
}

fn let1(name: &str, init: Expr, body: Expr) -> Expr {
    let ty = body.ty.clone();
    Expr::new(
        ty,
        Kind::Let(vec![(Pat::Bind(name.into()), init)], Box::new(body)),
    )
}

fn push_fn(name: &str, param: &str, x: i64) -> FunDef {
    let push = Expr::new(
        Ty::Unit,
        Kind::Call(
            "push!".into(),
            vec![Arg::InOut(param.into()), Arg::Val(Expr::int(x))],
        ),
    );
    FunDef {
        name: name.into(),
        params: vec![Param {
            name: param.into(),
            ty: Ty::vec(Ty::Int),
            inout: true,
        }],
        ret: Ty::Unit,
        body: push,
    }
}

/// `(defun bar (&a &b) (do (push! &a 1) (push! &b 2)))`.
fn bar() -> FunDef {
    let push = |p: &str, x: i64| {
        let args = vec![Arg::InOut(p.into()), Arg::Val(Expr::int(x))];
        Expr::new(Ty::Unit, Kind::Call("push!".into(), args))
    };
    let vt = Ty::vec(Ty::Int);
    let param = |n: &str| Param {
        name: n.into(),
        ty: vt.clone(),
        inout: true,
    };
    FunDef {
        name: "bar".into(),
        params: vec![param("a"), param("b")],
        ret: Ty::Unit,
        body: Expr::new(Ty::Unit, Kind::Do(vec![push("a", 1), push("b", 2)])),
    }
}

/// Case 67: two names for one cell at two `&` parameters; the later
/// write-back wins, so element 2 is the second push.
#[test]
fn later_write_back_wins() {
    let vt = Ty::vec(Ty::Int);
    let cell = Expr::call(Ty::cell(vt.clone()), "cell", vec![vec_of(&[3, 4])]);
    let read = Expr::new(
        vt.clone(),
        Kind::Deref(Box::new(Expr::var("x", Ty::cell(vt.clone())))),
    );
    let nth = int_call("nth", vec![read, Expr::int(2)]);
    let body = Expr::new(
        Ty::Int,
        Kind::Do(vec![inout_call("bar", &["x", "y"], Ty::Unit), nth]),
    );
    let main = let1("x", cell, let1("y", Expr::var("x", Ty::cell(vt)), body));
    assert_eq!(
        expected(&Program {
            defs: Vec::new(),
            funs: vec![bar()],
            main
        }),
        Ok(2)
    );
}

/// Case 17: `&v` and `@v` in one call; the plain argument is read before
/// the call writes anything.
#[test]
fn copy_in_happens_at_the_argument() {
    let vt = Ty::vec(Ty::Int);
    let f = push_fn("p", "v", 9);
    let cell = Expr::call(Ty::cell(vt.clone()), "cell", vec![vec_of(&[1, 2, 3])]);
    let read = || {
        Expr::new(
            vt.clone(),
            Kind::Deref(Box::new(Expr::var("c", Ty::cell(vt.clone())))),
        )
    };
    let body = Expr::new(
        Ty::Int,
        Kind::Do(vec![
            inout_call("p", &["c"], Ty::Unit),
            int_call("count", vec![read()]),
        ]),
    );
    assert_eq!(
        expected(&Program {
            defs: Vec::new(),
            funs: vec![f],
            main: let1("c", cell, body)
        }),
        Ok(4)
    );
}

#[test]
fn arithmetic_wraps_and_rem_by_zero_traps() {
    let big = int_call("*", vec![Expr::int(i64::MAX), Expr::int(2)]);
    assert_eq!(
        expected(&Program {
            defs: Vec::new(),
            funs: Vec::new(),
            main: big
        }),
        Ok(-2)
    );
    let rem = int_call("rem", vec![Expr::int(1), Expr::int(0)]);
    assert!(matches!(
        expected(&Program {
            defs: Vec::new(),
            funs: Vec::new(),
            main: rem
        }),
        Err(ModelError::Trap(_))
    ));
}

#[test]
fn loop_and_recur_rebind_all_variables() {
    // (loop ((i 0) (s 0)) (if (< i 4) (recur (+ i 1) (+ s i)) s)) = 6
    let i = Expr::var("i", Ty::Int);
    let s = Expr::var("s", Ty::Int);
    let test = Expr::call(Ty::Bool, "<", vec![i.clone(), Expr::int(4)]);
    let rec = Expr::new(
        Ty::Int,
        Kind::Recur(vec![
            int_call("+", vec![i.clone(), Expr::int(1)]),
            int_call("+", vec![s.clone(), i]),
        ]),
    );
    let body = Expr::new(
        Ty::Int,
        Kind::If(Box::new(test), Box::new(rec), Box::new(s)),
    );
    let lp = Expr::new(
        Ty::Int,
        Kind::Loop(
            vec![("i".into(), Expr::int(0)), ("s".into(), Expr::int(0))],
            Box::new(body),
        ),
    );
    assert_eq!(
        expected(&Program {
            defs: Vec::new(),
            funs: Vec::new(),
            main: lp
        }),
        Ok(6)
    );
}

#[test]
fn a_weak_reference_to_a_dead_target_reads_nil() {
    let wt = Ty::weak(Ty::Wrap);
    let target = Expr::call(
        Ty::Wrap,
        "Wrap",
        vec![Expr::new(Ty::Str, Kind::Str("a".into())), vec_of(&[])],
    );
    let w = Expr::new(wt.clone(), Kind::WeakDead("d".into(), Box::new(target)));
    let read = Expr::new(Ty::opt(Ty::Wrap), Kind::Deref(Box::new(w)));
    let m = Expr::new(
        Ty::Int,
        Kind::Match(
            Box::new(read),
            vec![
                (Pat::Some(Box::new(Pat::Wild)), Expr::int(1)),
                (Pat::Nil, Expr::int(0)),
            ],
        ),
    );
    assert_eq!(
        expected(&Program {
            defs: Vec::new(),
            funs: Vec::new(),
            main: m
        }),
        Ok(0)
    );
}
