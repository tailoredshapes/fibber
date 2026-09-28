//! `i64` productions over the protocols: method calls, calls of
//! protocol-bounded generic helpers, vectors of `dyn` values mapped
//! through a method, and `(dyn P :send)` values crossing a thread by
//! `spawn`, `plet`, `pmap` and `async` (types §2.15, §5.1).

use crate::ast::{Expr, Kind, Pat};
use crate::ty::{Proto, Ty};

use super::protos::{call_method, dyn_value, make_generic, receiver};
use super::{Ctx, Gen, Region, Var, VarKind};

/// An `i64` from one of the productions, or `None` in a program (or a
/// position) without protocols.
pub fn int_form(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    if !g.methods_ok {
        return None;
    }
    let p = if g.rng.chance(50) {
        Proto::Rank
    } else {
        Proto::Score
    };
    match g.rng.below(11) {
        // Score's methods through a (dyn Rank): its supertrait's
        // vtable entries (types §4.1 rule 3).
        10 => {
            let send = g.rng.chance(40);
            let recv = dyn_value(g, cx, Proto::Rank, send, d - 1);
            Some(call_method(g, cx, recv, Proto::Score, d))
        }
        0..=2 => {
            let recv = receiver(g, cx, p, d)?;
            Some(call_method(g, cx, recv, p, d))
        }
        3..=6 => generic_call(g, cx, d),
        7 => Some(map_dyn(g, cx, p, d)),
        _ => Some(cross(g, cx, p, d)),
    }
}

/// The index of the proto a generic helper is bounded by, if it is one.
fn generic_bound(g: &Gen, idx: usize) -> Option<Proto> {
    match g.funs[idx].params.first().map(|p| &p.ty) {
        Some(Ty::Gen(p, _)) => Some(*p),
        _ => None,
    }
}

/// A call of a protocol-bounded generic helper (an existing one or a
/// new one), its receiver of any type satisfying the bound: a concrete
/// implementing type, a `dyn` (whose vtable satisfies it, types §3.3),
/// or a variable bounded by an entailing protocol.
pub fn generic_call(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let existing: Vec<usize> = (0..g.funs.len())
        .filter(|&i| generic_bound(g, i).is_some())
        .collect();
    let idx = match g.rng.pick(&existing) {
        Some(&i) if g.rng.chance(60) || g.funs.len() >= g.max_funs => i,
        _ if g.funs.len() < g.max_funs => make_generic(g, d),
        _ => return None,
    };
    let p = generic_bound(g, idx)?;
    let f = g.funs[idx].clone();
    let recv = if g.rng.chance(45) {
        let q = if p == Proto::Rank || g.rng.chance(50) {
            Proto::Rank
        } else {
            Proto::Score
        };
        let send = g.rng.chance(40);
        dyn_value(g, cx, q, send, d.saturating_sub(1))
    } else {
        receiver(g, cx, p, d)?
    };
    let mut args = vec![recv];
    for q in &f.params[1..] {
        args.push(g.expr(cx, &q.ty, d.saturating_sub(1)));
    }
    Some(Expr::call(Ty::Int, &f.name, args))
}

/// `(fn (e: T) (m e ..))` with its body generated in `cx` as a
/// closure body (no `await`, no `&` parameters: syntax §3.13, §3.14).
fn method_fn(g: &mut Gen, cx: &Ctx, t: &Ty, p: Proto, d: u32) -> Expr {
    let e = g.fresh("e");
    let inner = cx
        .for_closure(true)
        .with(Var::new(e.clone(), t.clone(), VarKind::Param));
    let body = call_method(g, &inner, Expr::var(&e, t.clone()), p, d);
    let fty = Ty::Func(vec![t.clone()], Box::new(Ty::Int));
    Expr::new(fty, Kind::Fn(vec![(e, t.clone())], Box::new(body)))
}

/// `(sum-vec (map (fn (e: (dyn P)) (m e)) v))`: each element of a
/// heterogeneous vector called through its own vtable.
fn map_dyn(g: &mut Gen, cx: &Ctx, p: Proto, d: u32) -> Expr {
    let send = g.rng.chance(30);
    let et = Ty::Dyn(p, send);
    let n = g.rng.range(1, 3);
    let items = (0..n).map(|_| dyn_value(g, cx, p, send, d - 1)).collect();
    let v = Expr::new(Ty::vec(et.clone()), Kind::VecLit(items));
    let f = method_fn(g, cx, &et, p, d - 1);
    let mapped = Expr::call(Ty::vec(Ty::Int), "map", vec![f, v]);
    Expr::call(Ty::Int, "sum-vec", vec![mapped])
}

/// `(let ((dn (dyn P :send e))) cross)`, where `cross` uses `dn` on
/// another thread or in a task: `(join (spawn (fn () (m dn))))`, a
/// `plet` initialiser, a `pmap` over a vector holding it, or `(block-on
/// (async (m dn)))`.
fn cross(g: &mut Gen, cx: &Ctx, p: Proto, d: u32) -> Expr {
    let dt = Ty::Dyn(p, true);
    let dn = g.fresh("dn");
    let init = dyn_value(g, cx, p, true, d - 1);
    let inner = cx.with(Var::new(dn.clone(), dt.clone(), VarKind::Let));
    let body = crossing(g, &inner, Expr::var(&dn, dt), p, d.saturating_sub(1));
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(dn), init)], Box::new(body)),
    )
}

/// A use of `me`, a `(dyn P :send)` in scope in `cx`, on another thread
/// or in a task, directly or through an atom or a weak reference holding
/// it (types §1.6, §5.1: both are `Send` since the `dyn` is; case 110).
fn crossing(g: &mut Gen, cx: &Ctx, me: Expr, p: Proto, d: u32) -> Expr {
    let pool = if cx.region == Region::Task {
        Region::Task
    } else {
        Region::Pool
    };
    match g.rng.below(6) {
        4 => through_atom(g, cx, me, p, d),
        5 => through_weak(g, cx, me, p, d),
        0 => {
            let m = call_method(g, &cx.for_region(Region::Task, false), me, p, d);
            let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(m)));
            let t = Expr::call(Ty::task(Ty::Int), "spawn", vec![f]);
            Expr::call(Ty::Int, "join", vec![t])
        }
        1 => {
            let t = g.fresh("t");
            let m = call_method(g, &cx.for_region(pool, false), me, p, d);
            let after = cx.with(Var::new(t.clone(), Ty::Int, VarKind::Loop));
            let rest = g.expr(&after, &Ty::Int, d);
            let sum = Expr::call(Ty::Int, "+", vec![Expr::var(&t, Ty::Int), rest]);
            Expr::new(Ty::Int, Kind::Plet(vec![(t, m)], Box::new(sum)))
        }
        2 => {
            let dt = me.ty.clone();
            let other = dyn_value(g, cx, p, true, d);
            let v = Expr::new(Ty::vec(dt.clone()), Kind::VecLit(vec![me, other]));
            let f = method_fn(g, &cx.for_region(pool, false), &dt, p, d);
            let mapped = Expr::call(Ty::vec(Ty::Int), "pmap", vec![f, v]);
            Expr::call(Ty::Int, "sum-vec", vec![mapped])
        }
        _ => {
            let m = call_method(g, &cx.for_region(Region::Task, true), me, p, d);
            let t = Expr::new(Ty::task(Ty::Int), Kind::Async(Box::new(m)));
            Expr::call(Ty::Int, "block-on", vec![t])
        }
    }
}

/// `(join (spawn (fn () body)))`, `body` made in the task region of `cx`.
fn spawned(g: &mut Gen, cx: &Ctx, body: impl FnOnce(&mut Gen, &Ctx) -> Expr) -> Expr {
    let b = body(g, &cx.for_region(Region::Task, false));
    let f = Expr::new(Ty::fn_0(), Kind::Fn(Vec::new(), Box::new(b)));
    let t = Expr::call(Ty::task(Ty::Int), "spawn", vec![f]);
    Expr::call(Ty::Int, "join", vec![t])
}

/// `(let ((at (atom me))) [(reset! at other)] (join (spawn (fn () (m
/// @at)))))`: an `(Atom (dyn P :send))` crossing a thread. Only this
/// thread writes it, and before the spawn, so every schedule reads the
/// same value.
fn through_atom(g: &mut Gen, cx: &Ctx, me: Expr, p: Proto, d: u32) -> Expr {
    let dt = me.ty.clone();
    let at = g.fresh("at");
    let atv = Expr::var(&at, Ty::atom(dt.clone()));
    let read = Expr::new(dt.clone(), Kind::Deref(Box::new(atv.clone())));
    let call = spawned(g, cx, |g, tcx| call_method(g, tcx, read, p, d));
    let body = if g.rng.chance(40) {
        let other = dyn_value(g, cx, p, true, d.saturating_sub(1));
        let reset = Expr::call(Ty::Unit, "reset!", vec![atv, other]);
        Expr::new(Ty::Int, Kind::Do(vec![reset, call]))
    } else {
        call
    };
    let init = Expr::call(Ty::atom(dt), "atom", vec![me]);
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(at), init)], Box::new(body)),
    )
}

/// `(let ((w (weak me))) (join (spawn (fn () (match @w ((some x) (m x))
/// (nil k))))))`: a `(Weak (dyn P :send))` crossing a thread; `me` is a
/// variable bound around it, so the upgrade finds its target alive.
fn through_weak(g: &mut Gen, cx: &Ctx, me: Expr, p: Proto, d: u32) -> Expr {
    let dt = me.ty.clone();
    let wt = Ty::weak(dt.clone());
    let (w, x) = (g.fresh("w"), g.fresh("x"));
    let k = Expr::int(g.small());
    let wv = Expr::var(&w, wt.clone());
    let call = spawned(g, cx, |g, tcx| {
        let xc = tcx.with(Var::new(x.clone(), dt.clone(), VarKind::Pattern));
        let some = call_method(g, &xc, Expr::var(&x, dt.clone()), p, d);
        let read = Expr::new(Ty::opt(dt.clone()), Kind::Deref(Box::new(wv)));
        let clauses = vec![
            (Pat::Some(Box::new(Pat::Bind(x.clone()))), some),
            (Pat::Nil, k),
        ];
        Expr::new(Ty::Int, Kind::Match(Box::new(read), clauses))
    });
    let init = Expr::call(wt, "weak", vec![me]);
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(w), init)], Box::new(call)),
    )
}
