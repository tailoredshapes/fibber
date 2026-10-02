//! Closures: literals (escaping or not), named functions as values, and
//! calls through function values.

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::{objects, Ctx, Gen, Var, VarKind};

pub use super::helpers::helper_call;
pub use super::inout::inout_int;

/// A `fn` literal of type `(fn (params) ret)` whose body is generated in
/// `cx`; `escaping` says whether the closure may outlive the call, which
/// decides whether it may capture `&` parameters (syntax §3.13 rule 2).
pub fn literal(g: &mut Gen, cx: &Ctx, params: &[Ty], ret: &Ty, escaping: bool, d: u32) -> Expr {
    let mut inner = cx.for_closure(escaping);
    let mut ps = Vec::new();
    for t in params {
        let n = g.fresh("a");
        inner = inner.with(Var::new(n.clone(), t.clone(), VarKind::Param));
        ps.push((n, t.clone()));
    }
    let body = g.expr(&inner, ret, d.saturating_sub(1));
    let fty = Ty::Func(params.to_vec(), Box::new(ret.clone()));
    Expr::new(fty, Kind::Fn(ps, Box::new(body)))
}

/// An expression of a function type.
pub fn function(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let Ty::Func(ps, ret) = ty else {
        return g.leaf_value(cx, ty);
    };
    match g.rng.below(7) {
        0 if *ty == Ty::fn_i() => Expr::new(ty.clone(), Kind::Global("inc1".into())),
        2 if *ty == Ty::fn_i() => named(g, cx, d),
        1 if *ty == Ty::fn_i() => {
            objects::extract(g, cx, ty, d).unwrap_or_else(|| literal(g, cx, ps, ret, true, d))
        }
        _ => literal(g, cx, ps, ret, true, d),
    }
}

/// A function value that captures nothing: `inc1` or a constant literal.
pub fn const_fn(g: &mut Gen, arity: usize) -> Expr {
    if arity == 1 && g.rng.chance(50) {
        return Expr::new(Ty::fn_i(), Kind::Global("inc1".into()));
    }
    let k = Expr::int(g.small());
    if arity == 0 {
        return Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(k)));
    }
    let x = g.fresh("a");
    let body = Expr::call(Ty::Int, "+", vec![Expr::var(&x, Ty::Int), k]);
    Expr::new(Ty::fn_i(), Kind::Fn(vec![(x, Ty::Int)], Box::new(body)))
}

/// A call through a function value: `(f a)` or `(f)`.
pub fn apply(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    if g.rng.chance(30) {
        let f = g.expr(cx, &Ty::fn_0(), d - 1);
        return Expr::new(Ty::Int, Kind::Apply(Box::new(f), Vec::new()));
    }
    let f = g.expr(cx, &Ty::fn_i(), d - 1);
    let a = g.expr(cx, &Ty::Int, d - 1);
    Expr::new(Ty::Int, Kind::Apply(Box::new(f), vec![a]))
}

/// A closure that does not escape (types §6.5 uses (a) and (c)): called
/// at the literal, `((fn (x) ..) a)`, or bound by a `let` whose every use
/// is a call. It may capture `&` parameters.
pub fn immediate(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let f = literal(g, cx, &[Ty::Int], &Ty::Int, false, d);
    if g.rng.chance(50) {
        let a = g.expr(cx, &Ty::Int, d - 1);
        return Expr::new(Ty::Int, Kind::Apply(Box::new(f), vec![a]));
    }
    let n = g.fresh("k");
    let call = |x: i64| {
        Expr::new(
            Ty::Int,
            Kind::Apply(Box::new(Expr::var(&n, Ty::fn_i())), vec![Expr::int(x)]),
        )
    };
    let body = Expr::call(Ty::Int, "+", vec![call(1), call(2)]);
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(n.clone()), f)], Box::new(body)),
    )
}

/// `(vec (map (fn (x: i64) ..) v))`: `map` takes its function `:borrow`
/// (syntax §4.5), so the closure may be non-escaping; its answer is a lazy
/// sequence since the flip, which `vec` realises where the program needs
/// the vector.
pub fn map_form(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let escaping = g.rng.chance(50);
    let f = literal(g, cx, &[Ty::Int], &Ty::Int, escaping, d);
    let v = g.expr(cx, &Ty::vec(Ty::Int), d);
    let mapped = Expr::call(Ty::vec(Ty::Int), "map", vec![f, v]);
    Expr::call(Ty::vec(Ty::Int), "vec", vec![mapped])
}

/// `(fn (x: i64) stmt)` for `for-each`: non-escaping.
pub fn stmt_closure(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    literal(g, cx, &[Ty::Int], &Ty::Unit, false, d)
}

/// `(fn go (k: i64) (if (<= k 0) base (+ step (go' (rem (- k 1) 3)))))`:
/// a self-recursive closure, recursing at most three levels. The
/// recursive call goes through the self-name directly, or through a
/// value made from it (a box, a struct, a `let`), which makes the
/// closure escaping and a heap object (types §6.5; cases 59, 60).
pub fn named(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let (go, k) = (g.fresh("go"), g.fresh("k"));
    let inner = cx
        .for_closure(true)
        .with(Var::new(k.clone(), Ty::Int, VarKind::Param));
    let base = g.expr(&inner, &Ty::Int, d.saturating_sub(1));
    let step = g.expr(&inner, &Ty::Int, d.saturating_sub(1));
    let kv = Expr::var(&k, Ty::Int);
    let dec = Expr::call(Ty::Int, "-", vec![kv.clone(), Expr::int(1)]);
    let arg = Expr::call(Ty::Int, "rem", vec![dec, Expr::int(3)]);
    let me = Expr::var(&go, Ty::fn_i());
    let head = match g.rng.below(4) {
        0 => Expr::call(
            Ty::fn_i(),
            "unbox",
            vec![Expr::call(Ty::boxed(Ty::fn_i()), "Box", vec![me])],
        ),
        1 => {
            let h = Expr::call(
                Ty::Holder,
                "Holder",
                vec![
                    me,
                    Expr::call(Ty::cell(Ty::Int), "cell", vec![Expr::int(0)]),
                ],
            );
            Expr::new(Ty::fn_i(), Kind::Field(Box::new(h), "f".into()))
        }
        _ => me,
    };
    let rec = Expr::new(Ty::Int, Kind::Apply(Box::new(head), vec![arg]));
    let test = Expr::call(Ty::Bool, "<=", vec![kv, Expr::int(0)]);
    let body = Expr::new(
        Ty::Int,
        Kind::If(
            Box::new(test),
            Box::new(base),
            Box::new(Expr::call(Ty::Int, "+", vec![step, rec])),
        ),
    );
    Expr::new(
        Ty::fn_i(),
        Kind::FnNamed(go, vec![(k, Ty::Int)], Box::new(body)),
    )
}
