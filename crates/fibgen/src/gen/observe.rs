//! Folding a value of any type into an `i64`, so that what a program
//! built is visible in its result (a wrong answer is detectable).

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::{derive, hooks, jobs, nums, objects, protos, Ctx, Gen, Var, VarKind};

/// An `i64` expression that evaluates `e` once and folds its value.
pub fn observe(g: &mut Gen, cx: &Ctx, e: Expr, d: u32) -> Expr {
    let ty = e.ty.clone();
    match &ty {
        Ty::Int => e,
        Ty::Bool => if_int(e, Expr::int(1), Expr::int(0)),
        Ty::Unit => Expr::new(Ty::Int, Kind::Do(vec![e, Expr::int(g.small())])),
        Ty::Str => Expr::call(Ty::Int, "str-len", vec![e]),
        Ty::List | Ty::Vec(_) if g.rng.chance(40) => Expr::call(Ty::Int, "count", vec![e]),
        Ty::Vec(t) if **t == Ty::Int => Expr::call(Ty::Int, "sum-vec", vec![e]),
        Ty::Vec(t) => fold_vec(g, cx, e, t, d),
        Ty::List => list_head(g, e),
        Ty::Pt | Ty::Wrap | Ty::Holder => fold_struct(g, cx, e, d),
        Ty::Shape => fold_shape(g, cx, e, d),
        Ty::Boxed(t) | Ty::Cell(t) | Ty::Atom(t) | Ty::Task(t) => {
            let inner = match ty {
                Ty::Boxed(_) => Expr::call((**t).clone(), "unbox", vec![e]),
                Ty::Task(_) => Expr::call((**t).clone(), "join", vec![e]),
                _ => objects::deref(t, e),
            };
            observe(g, cx, inner, d)
        }
        Ty::Opt(t) => fold_opt(g, cx, e, t, d),
        Ty::Func(ps, _) => {
            let args = ps.iter().map(|_| Expr::int(g.small())).collect();
            Expr::new(Ty::Int, Kind::Apply(Box::new(e), args))
        }
        Ty::Weak(t) => {
            let o = Expr::new(Ty::opt((**t).clone()), Kind::Deref(Box::new(e)));
            fold_opt(g, cx, o, t, d)
        }
        Ty::Dyn(p, _) | Ty::Gen(p, _) => protos::call_method(g, cx, e, *p, d),
        Ty::Hook(_) => hooks::observe_hook(g, e),
        Ty::Job(_) => jobs::observe_job(g, e),
        Ty::Num(_) => nums::to_i64(g, e),
        Ty::Array(_) => Expr::call(Ty::Int, "array-len", vec![e]),
        Ty::Derived(_) => derive::observe(g, e),
    }
}

fn if_int(c: Expr, t: Expr, f: Expr) -> Expr {
    Expr::new(Ty::Int, Kind::If(Box::new(c), Box::new(t), Box::new(f)))
}

fn var(name: &str, ty: &Ty, kind: VarKind) -> Var {
    Var {
        name: name.to_string(),
        ty: ty.clone(),
        kind,
        send_fn: false,
    }
}

/// `(match l ((Cons h _) h) (_ k))`.
fn list_head(g: &mut Gen, e: Expr) -> Expr {
    let h = g.fresh("h");
    let pat = Pat::Ctor("Cons".into(), vec![Pat::Bind(h.clone()), Pat::Wild]);
    let clauses = vec![
        (pat, Expr::var(&h, Ty::Int)),
        (Pat::Wild, Expr::int(g.small())),
    ];
    Expr::new(Ty::Int, Kind::Match(Box::new(e), clauses))
}

/// A loop summing the folds of every element.
fn fold_vec(g: &mut Gen, cx: &Ctx, e: Expr, t: &Ty, d: u32) -> Expr {
    let vt = Ty::vec(t.clone());
    let (v, i, s) = (g.fresh("v"), g.fresh("i"), g.fresh("s"));
    let inner = cx
        .with(var(&v, &vt, VarKind::Let))
        .with(var(&i, &Ty::Int, VarKind::Loop))
        .with(var(&s, &Ty::Int, VarKind::Loop));
    let iv = Expr::var(&i, Ty::Int);
    let count = Expr::call(Ty::Int, "count", vec![Expr::var(&v, vt.clone())]);
    let test = Expr::call(Ty::Bool, "<", vec![iv.clone(), count]);
    let elem = Expr::call(t.clone(), "nth", vec![Expr::var(&v, vt), iv.clone()]);
    let folded = observe(g, &inner, elem, d);
    let step = Expr::call(Ty::Int, "+", vec![Expr::var(&s, Ty::Int), folded]);
    let next = Expr::call(Ty::Int, "+", vec![iv, Expr::int(1)]);
    let recur = Expr::new(Ty::Int, Kind::Recur(vec![next, step]));
    let body = if_int(test, recur, Expr::var(&s, Ty::Int));
    let lp = Expr::new(
        Ty::Int,
        Kind::Loop(vec![(i, Expr::int(0)), (s, Expr::int(0))], Box::new(body)),
    );
    Expr::new(Ty::Int, Kind::Let(vec![(Pat::Bind(v), e)], Box::new(lp)))
}

/// A struct folded field by field, through a destructuring `let` or
/// through `.` on a binding.
fn fold_struct(g: &mut Gen, cx: &Ctx, e: Expr, d: u32) -> Expr {
    let ty = e.ty.clone();
    let (head, ftys, fnames): (&str, [Ty; 2], [&str; 2]) = match ty {
        Ty::Pt => ("Pt", [Ty::Int, Ty::Int], ["x", "y"]),
        Ty::Wrap => ("Wrap", [Ty::Str, Ty::vec(Ty::Int)], ["s", "v"]),
        _ => ("Holder", [Ty::fn_i(), Ty::cell(Ty::Int)], ["f", "c"]),
    };
    let (a, b) = (g.fresh("f"), g.fresh("f"));
    if g.rng.chance(50) {
        let inner =
            cx.with(var(&a, &ftys[0], VarKind::Pattern))
                .with(var(&b, &ftys[1], VarKind::Pattern));
        let body = combine(
            g,
            &inner,
            Expr::var(&a, ftys[0].clone()),
            Expr::var(&b, ftys[1].clone()),
            d,
        );
        let pat = Pat::Ctor(head.into(), vec![Pat::Bind(a), Pat::Bind(b)]);
        return Expr::new(Ty::Int, Kind::Let(vec![(pat, e)], Box::new(body)));
    }
    let s = g.fresh("st");
    let inner = cx.with(var(&s, &ty, VarKind::Let));
    let field = |i: usize| {
        let sv = Expr::var(&s, ty.clone());
        Expr::new(
            ftys[i].clone(),
            Kind::Field(Box::new(sv), fnames[i].to_string()),
        )
    };
    let body = combine(g, &inner, field(0), field(1), d);
    Expr::new(Ty::Int, Kind::Let(vec![(Pat::Bind(s), e)], Box::new(body)))
}

/// `(+ fold(a) fold(b))`, or `(f @c)` for a Holder's two fields.
fn combine(g: &mut Gen, cx: &Ctx, a: Expr, b: Expr, d: u32) -> Expr {
    if a.ty.is_fn() {
        let c = objects::deref(&Ty::Int, b);
        return Expr::new(Ty::Int, Kind::Apply(Box::new(a), vec![c]));
    }
    let x = observe(g, cx, a, d);
    let y = observe(g, cx, b, d);
    Expr::call(Ty::Int, "+", vec![x, y])
}

fn fold_shape(g: &mut Gen, cx: &Ctx, e: Expr, d: u32) -> Expr {
    let (r, p, q, n, w) = (
        g.fresh("r"),
        g.fresh("p"),
        g.fresh("q"),
        g.fresh("n"),
        g.fresh("w"),
    );
    let circle = (
        Pat::Ctor("Circle".into(), vec![Pat::Bind(r.clone())]),
        Expr::var(&r, Ty::Int),
    );
    let rc = cx
        .with(var(&p, &Ty::Pt, VarKind::Pattern))
        .with(var(&q, &Ty::Pt, VarKind::Pattern));
    let rect_body = combine(g, &rc, Expr::var(&p, Ty::Pt), Expr::var(&q, Ty::Pt), d);
    let rect = (
        Pat::Ctor("Rect".into(), vec![Pat::Bind(p), Pat::Bind(q)]),
        rect_body,
    );
    let nc =
        cx.with(var(&n, &Ty::Str, VarKind::Pattern))
            .with(var(&w, &Ty::Wrap, VarKind::Pattern));
    let named_body = combine(g, &nc, Expr::var(&n, Ty::Str), Expr::var(&w, Ty::Wrap), d);
    let named = (
        Pat::Ctor("Named".into(), vec![Pat::Bind(n), Pat::Bind(w)]),
        named_body,
    );
    let mut clauses = vec![circle, rect, named];
    if g.rng.chance(30) {
        clauses.truncate(1 + g.rng.below(2));
        clauses.push((Pat::Wild, Expr::int(g.small())));
    }
    Expr::new(Ty::Int, Kind::Match(Box::new(e), clauses))
}

fn fold_opt(g: &mut Gen, cx: &Ctx, e: Expr, t: &Ty, d: u32) -> Expr {
    let x = g.fresh("x");
    let inner = cx.with(var(&x, t, VarKind::Pattern));
    let some_body = observe(g, &inner, Expr::var(&x, t.clone()), d);
    let some = (Pat::Some(Box::new(Pat::Bind(x))), some_body);
    let nil = (Pat::Nil, Expr::int(g.small()));
    let clauses = if g.rng.chance(50) {
        vec![some, nil]
    } else {
        vec![nil, some]
    };
    Expr::new(Ty::Int, Kind::Match(Box::new(e), clauses))
}
