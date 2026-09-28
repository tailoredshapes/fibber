//! Threads and tasks: `spawn`/`join`, `plet`, `pmap`, `async`/`await`,
//! `block-on` (syntax §3.12, §3.14). Code that runs elsewhere is
//! generated in a context holding only sendable captures (types §5).

use crate::ast::{Expr, Kind};
use crate::ty::Ty;

use super::{funcs, Ctx, Gen, Region, Var, VarKind};

/// `(await (yield))`.
pub fn await_yield() -> Expr {
    let y = Expr::call(Ty::task(Ty::Unit), "yield", Vec::new());
    Expr::new(Ty::Unit, Kind::Await(Box::new(y)))
}

/// The region for code started from `cx` on another thread.
fn pool_region(cx: &Ctx) -> Region {
    if cx.region == Region::Task {
        Region::Task
    } else {
        Region::Pool
    }
}

/// `(join t)` or `(block-on t)` of a task of `ty`.
pub fn join(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    let t = g.expr(cx, &Ty::task(ty.clone()), d - 1);
    let head = if g.rng.chance(50) { "join" } else { "block-on" };
    Some(Expr::call(ty.clone(), head, vec![t]))
}

/// `(await t)` inside an `async` body.
pub fn await_form(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Option<Expr> {
    if !cx.in_async {
        return None;
    }
    let t = g.expr(cx, &Ty::task(ty.clone()), d - 1);
    Some(Expr::new(ty.clone(), Kind::Await(Box::new(t))))
}

/// An expression of type `(Task t)`.
pub fn task(g: &mut Gen, cx: &Ctx, t: &Ty, d: u32) -> Expr {
    let tt = Ty::task(t.clone());
    if g.rng.chance(50) {
        let inner = cx.for_region(Region::Task, true);
        return async_body(g, &inner, t, d - 1);
    }
    let inner = cx.for_region(Region::Task, false);
    let f = funcs::literal(g, &inner, &[], t, true, d - 1);
    Expr::call(tt, "spawn", vec![f])
}

/// `(async body)`, the body sometimes suspending first; `cx` must
/// already be the task's context.
pub fn async_body(g: &mut Gen, cx: &Ctx, t: &Ty, d: u32) -> Expr {
    let mut body = g.expr(cx, t, d);
    if g.rng.chance(50) {
        body = Expr::new(t.clone(), Kind::Do(vec![await_yield(), body]));
    }
    Expr::new(Ty::task(t.clone()), Kind::Async(Box::new(body)))
}

/// `(spawn (fn () ..))` whose task is dropped: it still runs to
/// completion before `main` returns (case 26).
pub fn spawn_discarded(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let inner = cx.for_region(Region::Task, false);
    let f = funcs::literal(g, &inner, &[], &Ty::Int, true, d);
    Expr::call(Ty::task(Ty::Int), "spawn", vec![f])
}

/// `(plet ((a e) ..) body)`: the initialisers run on other threads.
pub fn plet(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let inner = cx.for_region(pool_region(cx), false);
    let mut binds = Vec::new();
    let mut vars = Vec::new();
    for _ in 0..g.rng.range(1, 3) {
        let t = g.send_type();
        let name = g.fresh("t");
        binds.push((name.clone(), g.expr(&inner, &t, d - 1)));
        vars.push(Var {
            name,
            ty: t,
            kind: VarKind::Loop,
            send_fn: false,
        });
    }
    let body = g.expr(&cx.with_all(vars), ty, d - 1);
    Expr::new(ty.clone(), Kind::Plet(binds, Box::new(body)))
}

/// `(pmap (fn (x: i64) ..) v)`: a sendable closure over each element.
pub fn pmap(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let inner = cx.for_region(pool_region(cx), false);
    let f = funcs::literal(g, &inner, &[Ty::Int], &Ty::Int, true, d);
    let v = g.expr(cx, &Ty::vec(Ty::Int), d);
    Expr::call(Ty::vec(Ty::Int), "pmap", vec![f, v])
}

/// `(sum-vec (pmap ..))`.
pub fn pmap_sum(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let p = pmap(g, cx, d - 1);
    Expr::call(Ty::Int, "sum-vec", vec![p])
}
