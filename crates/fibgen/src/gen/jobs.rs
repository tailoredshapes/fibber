//! The colour-parameterised enum `(Job k :colour)` (types §1.3): its
//! `Ready` variant holds a `(fn k () i64)`, so a `(Job :send)` holds a
//! closure whose captures are all sendable and may cross a thread, and a
//! `(Job :local)` may hold one over a cell. `Idle` has no field and is
//! either colour.

use crate::ast::{Expr, Kind, Pat};
use crate::ty::Ty;

use super::{funcs, objects, observe, Ctx, Gen, Region, Var, VarKind};

/// A `(fn () i64)` sendable in any context: a sendable variable, a
/// constant literal, or a literal whose body sees only sendable
/// captures (types §5.4).
fn send_fn(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let vars: Vec<Var> = cx
        .vars_where(|t| *t == Ty::fn_0())
        .into_iter()
        .filter(|v| v.send_fn)
        .cloned()
        .collect();
    match g.rng.below(3) {
        0 => funcs::const_fn(g, 0),
        1 if !vars.is_empty() => {
            let v = &vars[g.rng.below(vars.len())];
            Expr::var(&v.name, Ty::fn_0())
        }
        _ => {
            let tcx = cx.for_region(Region::Task, false);
            funcs::literal(g, &tcx, &[], &Ty::Int, true, d)
        }
    }
}

/// `(fn () (+ k @c))` over the cell variable `c`.
fn over_cell(g: &mut Gen, c: &str) -> Expr {
    let read = objects::deref(&Ty::Int, Expr::var(c, Ty::cell(Ty::Int)));
    let body = Expr::call(Ty::Int, "+", vec![Expr::int(g.small()), read]);
    Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(body)))
}

/// `Idle` at the colour `send` or `local`.
pub fn idle(send: bool) -> Expr {
    Expr::new(Ty::Job(send), Kind::Var("Idle".into()))
}

/// A `(Job :send)` or `(Job :local)`: `Idle` sometimes, else `(Ready f)`
/// whose closure, for a local one, is often over a cell.
pub fn job(g: &mut Gen, cx: &Ctx, send: bool, d: u32) -> Expr {
    let jt = Ty::Job(send);
    if g.rng.chance(20) {
        return idle(send);
    }
    let sub = d.saturating_sub(1);
    if send {
        let f = send_fn(g, cx, sub);
        return Expr::call(jt, "Ready", vec![f]);
    }
    if cx.cell_free || !g.rng.chance(50) {
        let f = g.expr(cx, &Ty::fn_0(), sub);
        return Expr::call(jt, "Ready", vec![f]);
    }
    let cells: Vec<Var> = cx
        .vars_of(&Ty::cell(Ty::Int))
        .into_iter()
        .cloned()
        .collect();
    if let Some(c) = g.rng.pick(&cells) {
        let f = over_cell(g, &c.name);
        return Expr::call(jt, "Ready", vec![f]);
    }
    let c = g.fresh("jc");
    let init = Expr::call(Ty::cell(Ty::Int), "cell", vec![Expr::int(g.small())]);
    let f = over_cell(g, &c);
    let j = Expr::call(jt.clone(), "Ready", vec![f]);
    Expr::new(jt, Kind::Let(vec![(Pat::Bind(c), init)], Box::new(j)))
}

/// `Idle` or `(Ready (fn () k))`, capturing nothing.
pub fn job_leaf(g: &mut Gen, send: bool) -> Expr {
    if g.rng.chance(30) {
        return idle(send);
    }
    let f = funcs::const_fn(g, 0);
    Expr::call(Ty::Job(send), "Ready", vec![f])
}

/// `(match e ((Idle) k) ((Ready r) run))`, the clauses in either order;
/// `run` is `(r)`, or, on a `(Job :send)` now and then, `(join (spawn
/// r))`: the closure bound out of a `:send` instance is sendable (types
/// §1.3, patterns see a field's colour under the scrutinee's argument).
pub fn observe_job(g: &mut Gen, e: Expr) -> Expr {
    let send = e.ty == Ty::Job(true);
    let r = g.fresh("r");
    let rv = Expr::var(&r, Ty::fn_0());
    let run = if send && g.rng.chance(40) {
        let t = Expr::call(Ty::task(Ty::Int), "spawn", vec![rv]);
        Expr::call(Ty::Int, "join", vec![t])
    } else {
        Expr::new(Ty::Int, Kind::Apply(Box::new(rv), Vec::new()))
    };
    let ready = (Pat::Ctor("Ready".into(), vec![Pat::Bind(r)]), run);
    let idle = (Pat::Ctor("Idle".into(), Vec::new()), Expr::int(g.small()));
    let clauses = if g.rng.chance(50) {
        vec![idle, ready]
    } else {
        vec![ready, idle]
    };
    Expr::new(Ty::Int, Kind::Match(Box::new(e), clauses))
}

/// A `(Job :send)` crossing a thread: `(let ((jb job)) (join (spawn (fn
/// () fold(jb)))))`, its `Send` following its colour argument (§5.1).
/// Now and then a `(Job :local)` is made and run here beside it.
pub fn cross(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let jb = g.fresh("jb");
    let init = job(g, cx, true, d - 1);
    let inner = cx.with(Var::new(jb.clone(), Ty::Job(true), VarKind::Let));
    let tcx = inner.for_region(Region::Task, false);
    let seen = observe::observe(g, &tcx, Expr::var(&jb, Ty::Job(true)), d - 1);
    let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(seen)));
    let t = Expr::call(Ty::task(Ty::Int), "spawn", vec![f]);
    let mut body = Expr::call(Ty::Int, "join", vec![t]);
    if g.rng.chance(40) {
        let local = job(g, &inner, false, d - 1);
        let here = observe_job(g, local);
        body = Expr::call(Ty::Int, "+", vec![body, here]);
    }
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(jb), init)], Box::new(body)),
    )
}
