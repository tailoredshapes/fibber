//! The colour-parameterised struct `(Hook k :colour)` (types §1.3): a
//! `(Hook :send)` holds a closure whose captures are all sendable and
//! may cross a thread; a `(Hook :local)` may hold one over a cell.

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::{funcs, objects, observe, Ctx, Gen, Region, Var, VarKind};

/// A closure sendable in any context: `inc1`, a `send` variable, or a
/// literal whose body sees only sendable captures (types §5.4).
fn send_fn(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let vars: Vec<Var> = cx
        .vars_where(|t| *t == Ty::fn_i())
        .into_iter()
        .filter(|v| v.send_fn)
        .cloned()
        .collect();
    match g.rng.below(3) {
        0 => Expr::new(Ty::fn_i(), Kind::Global("inc1".into())),
        1 if !vars.is_empty() => {
            let v = &vars[g.rng.below(vars.len())];
            Expr::var(&v.name, Ty::fn_i())
        }
        _ => {
            let tcx = cx.for_region(Region::Task, false);
            funcs::literal(g, &tcx, &[Ty::Int], &Ty::Int, true, d)
        }
    }
}

/// `(fn (a: i64) (+ a @c))` over the cell variable `c`.
fn over_cell(g: &mut Gen, c: &str) -> Expr {
    let a = g.fresh("a");
    let read = objects::deref(&Ty::Int, Expr::var(c, Ty::cell(Ty::Int)));
    let body = Expr::call(Ty::Int, "+", vec![Expr::var(&a, Ty::Int), read]);
    Expr::new(Ty::fn_i(), Kind::Fn(vec![(a, Ty::Int)], Box::new(body)))
}

/// `(Hook f tag)` at colour `send` or `local`; a local one's closure
/// is often over a cell (in scope, or made for it by a `let` around).
pub fn hook(g: &mut Gen, cx: &Ctx, send: bool, d: u32) -> Expr {
    let sub = d.saturating_sub(1);
    let tag = g.expr(cx, &Ty::Int, sub);
    let ht = Ty::Hook(send);
    if send {
        let f = send_fn(g, cx, sub);
        return Expr::call(ht, "Hook", vec![f, tag]);
    }
    if cx.cell_free || !g.rng.chance(50) {
        let f = g.expr(cx, &Ty::fn_i(), sub);
        return Expr::call(ht, "Hook", vec![f, tag]);
    }
    let cells: Vec<Var> = cx
        .vars_of(&Ty::cell(Ty::Int))
        .into_iter()
        .cloned()
        .collect();
    if let Some(c) = g.rng.pick(&cells) {
        let f = over_cell(g, &c.name);
        return Expr::call(ht, "Hook", vec![f, tag]);
    }
    let c = g.fresh("hc");
    let init = Expr::call(Ty::cell(Ty::Int), "cell", vec![Expr::int(g.small())]);
    let f = over_cell(g, &c);
    let h = Expr::call(ht.clone(), "Hook", vec![f, tag]);
    Expr::new(ht, Kind::Let(vec![(Pat::Bind(c), init)], Box::new(h)))
}

/// `(Hook inc1 k)`.
pub fn hook_leaf(g: &mut Gen, send: bool) -> Expr {
    let f = Expr::new(Ty::fn_i(), Kind::Global("inc1".into()));
    Expr::call(Ty::Hook(send), "Hook", vec![f, Expr::int(g.small())])
}

/// `(+ ((. h f) k) (. h tag))` over a binding of `e`, or the same
/// through a destructuring `(Hook f t)`.
pub fn observe_hook(g: &mut Gen, e: Expr) -> Expr {
    let k = Expr::int(g.small());
    if g.rng.chance(50) {
        let (f, t) = (g.fresh("hf"), g.fresh("ht"));
        let call = Expr::new(
            Ty::Int,
            Kind::Apply(Box::new(Expr::var(&f, Ty::fn_i())), vec![k]),
        );
        let body = Expr::call(Ty::Int, "+", vec![call, Expr::var(&t, Ty::Int)]);
        let pat = Pat::Ctor("Hook".into(), vec![Pat::Bind(f), Pat::Bind(t)]);
        return Expr::new(Ty::Int, Kind::Let(vec![(pat, e)], Box::new(body)));
    }
    let h = g.fresh("h");
    let hv = Expr::var(&h, e.ty.clone());
    let field = |n: &str, t: Ty| Expr::new(t, Kind::Field(Box::new(hv.clone()), n.into()));
    let call = Expr::new(
        Ty::Int,
        Kind::Apply(Box::new(field("f", Ty::fn_i())), vec![k]),
    );
    let body = Expr::call(Ty::Int, "+", vec![call, field("tag", Ty::Int)]);
    Expr::new(Ty::Int, Kind::Let(vec![(Pat::Bind(h), e)], Box::new(body)))
}

/// `(let ((h (Hook :send ..))) (join (spawn (fn () fold(h)))))`: a
/// `:send` instance crossing a thread, its `Send` following its colour
/// argument (types §1.3, §5.1).
pub fn cross(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let h = g.fresh("hk");
    let init = hook(g, cx, true, d - 1);
    let inner = cx.with(Var::new(h.clone(), Ty::Hook(true), VarKind::Let));
    let tcx = inner.for_region(Region::Task, false);
    let seen = observe::observe(g, &tcx, Expr::var(&h, Ty::Hook(true)), d - 1);
    let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(seen)));
    let joined = Expr::call(
        Ty::Int,
        "join",
        vec![Expr::call(Ty::task(Ty::Int), "spawn", vec![f])],
    );
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(h), init)], Box::new(joined)),
    )
}
