//! `(Array a)` (types §2.13): made, read, copied with one slot changed,
//! sliced, and updated in place through a cell by `array-set!` (in
//! place when the cell's array is unique, a copy when a snapshot shares
//! it). Every index is a literal below the array's literal length, so
//! nothing traps.

use crate::ast::{Arg, Expr, Kind, Pat};
use crate::ty::Ty;

use super::{objects, observe, Ctx, Gen, Var, VarKind};

fn let1(name: &str, init: Expr, body: Expr) -> Expr {
    Expr::new(
        body.ty.clone(),
        Kind::Let(vec![(Pat::Bind(name.to_string()), init)], Box::new(body)),
    )
}

fn plus(a: Expr, b: Expr) -> Expr {
    Expr::call(Ty::Int, "+", vec![a, b])
}

/// `(array-get a i)`, folded.
fn get(g: &mut Gen, cx: &Ctx, a: Expr, i: i64, d: u32) -> Expr {
    let t = a.ty.inner().cloned().unwrap_or(Ty::Int);
    let e = Expr::call(t, "array-get", vec![a, Expr::int(i)]);
    observe::observe(g, cx, e, d)
}

/// An `i64` from an array of a random element type and length.
pub fn array_int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let t = [Ty::Int, Ty::Str, Ty::Wrap, Ty::Pt][g.rng.below(4)].clone();
    let at = Ty::Array(Box::new(t.clone()));
    let k = g.rng.range(1, 4);
    let x = g.expr(cx, &t, d - 1);
    let init = Expr::call(at.clone(), "array", vec![Expr::int(k), x]);
    if g.rng.chance(50) {
        functional(g, cx, init, k, d)
    } else {
        in_place(g, cx, init, k, d)
    }
}

/// `(let ((a init) (b (array-with a i x))) (+ (array-len (array-copy b
/// i j)) fold(array-get b ..) fold(array-get a ..)))`.
fn functional(g: &mut Gen, cx: &Ctx, init: Expr, k: i64, d: u32) -> Expr {
    let at = init.ty.clone();
    let t = at.inner().cloned().unwrap_or(Ty::Int);
    let (a, b) = (g.fresh("arr"), g.fresh("arr"));
    let ca = cx.with(Var::new(a.clone(), at.clone(), VarKind::Let));
    let x = g.expr(&ca, &t, d - 1);
    let i = g.rng.range(0, k - 1);
    let with = Expr::call(
        at.clone(),
        "array-with",
        vec![Expr::var(&a, at.clone()), Expr::int(i), x],
    );
    let cb = ca.with(Var::new(b.clone(), at.clone(), VarKind::Let));
    let lo = g.rng.range(0, k);
    let hi = g.rng.range(lo, k);
    let slice = Expr::call(
        at.clone(),
        "array-copy",
        vec![Expr::var(&b, at.clone()), Expr::int(lo), Expr::int(hi)],
    );
    let len = Expr::call(Ty::Int, "array-len", vec![slice]);
    let (j1, j2) = (g.rng.range(0, k - 1), g.rng.range(0, k - 1));
    let from_b = get(g, &cb, Expr::var(&b, at.clone()), j1, d - 1);
    let from_a = get(g, &cb, Expr::var(&a, at), j2, d - 1);
    let body = plus(len, plus(from_b, from_a));
    let1(&a, init, let1(&b, with, body))
}

/// `(let ((c (cell init)) [(snap @c)]) (do (array-set! &c i x) (+
/// fold(array-get @c j) [fold(array-get snap j)])))`: a unique write in
/// place, or, with a snapshot sharing the array, a copy (§6.6).
fn in_place(g: &mut Gen, cx: &Ctx, init: Expr, k: i64, d: u32) -> Expr {
    let at = init.ty.clone();
    let t = at.inner().cloned().unwrap_or(Ty::Int);
    let ct = Ty::cell(at.clone());
    let c = g.fresh("ac");
    let mut inner = cx.with(Var::new(c.clone(), ct.clone(), VarKind::Let));
    let snap = g.rng.chance(50).then(|| g.fresh("snap"));
    if let Some(s) = &snap {
        inner = inner.with(Var::new(s.clone(), at.clone(), VarKind::Let));
    }
    let x = g.expr(&objects::cell_content_ctx(&inner, &t), &t, d - 1);
    let i = g.rng.range(0, k - 1);
    let set = Expr::new(
        Ty::Unit,
        Kind::Call(
            "array-set!".into(),
            vec![Arg::InOut(c.clone()), Arg::Val(Expr::int(i)), Arg::Val(x)],
        ),
    );
    let read = objects::deref(&at, Expr::var(&c, ct.clone()));
    let j = g.rng.range(0, k - 1);
    let mut sum = get(g, &inner, read.clone(), j, d - 1);
    if let Some(s) = &snap {
        let old = get(g, &inner, Expr::var(s, at.clone()), i, d - 1);
        sum = plus(sum, old);
    }
    let mut body = Expr::new(Ty::Int, Kind::Do(vec![set, sum]));
    if let Some(s) = snap {
        body = let1(&s, read, body);
    }
    let1(&c, Expr::call(ct, "cell", vec![init]), body)
}
