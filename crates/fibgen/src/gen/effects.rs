//! Statements (`unit` expressions: `set!`, `push!`, `set-field!`, `&`
//! calls, atom updates, `for-each`, `await`), atom reads and weak
//! references.

use crate::ast::{Arg, Expr, Kind, Pat};
use crate::ty::Ty;

use super::{funcs, inout, objects, observe, tasks, Ctx, Gen, Region, Var, VarKind};

fn unit() -> Expr {
    Expr::new(Ty::Unit, Kind::Unit)
}

/// A `unit` expression with an effect.
pub fn stmt(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    if d == 0 {
        return unit();
    }
    let has = |t: Ty| !cx.cell_args(&t).is_empty();
    let weights = [
        3,                                                // set! on a cell
        if cx.has_inout() { 4 } else { 0 },               // set! on an & parameter
        if has(Ty::vec(Ty::Int)) { 5 } else { 0 },        // push!
        3,                                                // call an & helper
        if has(Ty::Pt) || has(Ty::Wrap) { 4 } else { 0 }, // set-field!
        if cx.region == Region::Main { 1 } else { 0 },    // reset!
        if cx.region != Region::Task { 2 } else { 0 },    // swap!, result discarded
        if cx.in_async { 2 } else { 0 },                  // (await (yield))
        2,                                                // for-each
        1,                                                // if
    ];
    let e = match g.rng.weighted(&weights) {
        Some(0) => set_cell(g, cx, d),
        Some(1) => set_inout(g, cx, d),
        Some(2) => push(g, cx, d),
        Some(3) => inout::stmt_call(g, cx, d),
        Some(4) => set_field(g, cx, d),
        Some(5) => reset(g, cx, d),
        Some(6) => swap(g, cx, d).map(|s| Expr::new(Ty::Unit, Kind::Do(vec![s, unit()]))),
        Some(7) => Some(tasks::await_yield()),
        Some(8) => Some(for_each(g, cx, d)),
        Some(9) => {
            let c = g.expr(cx, &Ty::Bool, d - 1);
            let (a, b) = (stmt(g, cx, d - 1), stmt(g, cx, d - 1));
            Some(Expr::new(
                Ty::Unit,
                Kind::If(Box::new(c), Box::new(a), Box::new(b)),
            ))
        }
        _ => None,
    };
    e.unwrap_or_else(unit)
}

fn set_cell(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let contents = [Ty::Int, Ty::vec(Ty::Int), Ty::Str, Ty::fn_i(), Ty::Pt];
    let t = contents[g.rng.below(contents.len())].clone();
    let ct = Ty::cell(t.clone());
    let target = match g.var_of(cx, &ct) {
        Some(v) => v,
        None if t == Ty::Int && g.rng.chance(50) => {
            let h = g.expr(cx, &Ty::Holder, d - 1);
            Expr::new(ct, Kind::Field(Box::new(h), "c".into()))
        }
        None => return None,
    };
    let v = g.expr(&objects::cell_content_ctx(cx, &t), &t, d - 1);
    Some(Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(target), Box::new(v)),
    ))
}

fn set_inout(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let ios: Vec<Var> = cx
        .vars
        .iter()
        .filter(|v| v.kind == VarKind::InOut)
        .cloned()
        .collect();
    let v = g.rng.pick(&ios)?.clone();
    let val = g.expr(cx, &v.ty, d - 1);
    let target = Expr::var(&v.name, Ty::cell(v.ty.clone()));
    Some(Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(target), Box::new(val)),
    ))
}

fn push(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let args = cx.cell_args(&Ty::vec(Ty::Int));
    let c = g.rng.pick(&args)?.name.clone();
    let x = g.expr(cx, &Ty::Int, d - 1);
    Some(Expr::new(
        Ty::Unit,
        Kind::Call("push!".into(), vec![Arg::InOut(c), Arg::Val(x)]),
    ))
}

/// `(set-field! &c f x)` on a cell (or `&` parameter) holding a `Pt` or
/// a `Wrap`: a unique update in place, or a copy when the struct is
/// shared (syntax §3.13).
fn set_field(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let mut args: Vec<(String, Ty)> = Vec::new();
    for t in [Ty::Pt, Ty::Wrap] {
        args.extend(
            cx.cell_args(&t)
                .into_iter()
                .map(|v| (v.name.clone(), t.clone())),
        );
    }
    let (c, t) = g.rng.pick(&args)?.clone();
    let (f, ft) = match (t, g.rng.chance(50)) {
        (Ty::Pt, true) => ("x", Ty::Int),
        (Ty::Pt, false) => ("y", Ty::Int),
        (_, true) => ("s", Ty::Str),
        (_, false) => ("v", Ty::vec(Ty::Int)),
    };
    let x = g.expr(cx, &ft, d - 1);
    Some(Expr::new(
        Ty::Unit,
        Kind::SetField(c, f.into(), Box::new(x)),
    ))
}

fn atom_var(g: &mut Gen, cx: &Ctx) -> Option<Var> {
    let atoms: Vec<Var> = cx
        .vars_where(|t| t.is_atom())
        .into_iter()
        .cloned()
        .collect();
    g.rng.pick(&atoms).cloned()
}

fn reset(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let a = atom_var(g, cx)?;
    let inner = a.ty.inner()?.clone();
    let v = g.expr(cx, &inner, d - 1);
    Some(Expr::call(
        Ty::Unit,
        "reset!",
        vec![Expr::var(&a.name, a.ty.clone()), v],
    ))
}

/// `(swap! a (fn (x) ...))` with a pure, commutative update, so that its
/// effect does not depend on the order concurrent updates run in.
fn swap(g: &mut Gen, cx: &Ctx, _d: u32) -> Option<Expr> {
    let a = atom_var(g, cx)?;
    let inner = a.ty.inner()?.clone();
    let x = g.fresh("old");
    let xv = Expr::var(&x, inner.clone());
    let k = Expr::int(g.rng.range(1, 5));
    let body = if inner == Ty::Int {
        Expr::call(Ty::Int, "+", vec![xv, k])
    } else {
        Expr::call(inner.clone(), "conj", vec![xv, k])
    };
    let fty = Ty::Func(vec![inner.clone()], Box::new(inner.clone()));
    let f = Expr::new(fty, Kind::Fn(vec![(x, inner.clone())], Box::new(body)));
    Some(Expr::call(
        inner,
        "swap!",
        vec![Expr::var(&a.name, a.ty.clone()), f],
    ))
}

/// An `i64` from an atom: `swap!`'s new value, or a write then a read.
pub fn atom_int(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    if !cx.atoms_readable() {
        return None;
    }
    if atom_var(g, cx).is_none() {
        let a = g.fresh("at");
        let t = if g.rng.chance(50) {
            Ty::Int
        } else {
            Ty::vec(Ty::Int)
        };
        let init = g.expr(cx, &t, d - 1);
        let at = Ty::atom(t);
        let inner = cx.with(Var::new(a.clone(), at.clone(), VarKind::Let));
        let body = atom_int(g, &inner, d)?;
        let bind = Expr::call(at, "atom", vec![init]);
        return Some(Expr::new(
            Ty::Int,
            Kind::Let(vec![(Pat::Bind(a), bind)], Box::new(body)),
        ));
    }
    if g.rng.chance(60) {
        let s = swap(g, cx, d)?;
        return Some(observe::observe(g, cx, s, d));
    }
    let r = reset(g, cx, d)?;
    let Kind::Call(_, args) = &r.kind else {
        return None;
    };
    let Some(Arg::Val(a)) = args.first() else {
        return None;
    };
    let read = objects::deref(a.ty.inner()?, a.clone());
    let obs = observe::observe(g, cx, read, d);
    Some(Expr::new(Ty::Int, Kind::Do(vec![r, obs])))
}

/// A weak reference that must upgrade (its target is bound by an
/// enclosing `let`) or must not (its target died with its `let`).
pub fn weak_int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let targets = [Ty::Wrap, Ty::Str, Ty::vec(Ty::Int), Ty::Pt, Ty::Shape];
    let t = targets[g.rng.below(targets.len())].clone();
    let wt = Ty::weak(t.clone());
    let (w, z) = (g.fresh("w"), g.fresh("z"));
    let alive = g.rng.chance(60);
    let (outer, inner) = if alive {
        let o = g.fresh("o");
        let init = g.expr(cx, &t, d - 1);
        let inner = cx.with(Var::new(o.clone(), t.clone(), VarKind::Let));
        let wk = Expr::call(wt.clone(), "weak", vec![Expr::var(&o, t.clone())]);
        (Some((o, init)), (wk, inner))
    } else {
        (None, (g.leaf_value(cx, &wt), cx.clone()))
    };
    let (wk, inner) = inner;
    let zc = inner.with(Var::new(z.clone(), t.clone(), VarKind::Pattern));
    let some_body = observe::observe(g, &zc, Expr::var(&z, t.clone()), d - 1);
    let read = Expr::new(Ty::opt(t.clone()), Kind::Deref(Box::new(Expr::var(&w, wt))));
    let clauses = vec![
        (Pat::Some(Box::new(Pat::Bind(z))), some_body),
        (Pat::Nil, Expr::int(g.small())),
    ];
    let m = Expr::new(Ty::Int, Kind::Match(Box::new(read), clauses));
    let body = Expr::new(Ty::Int, Kind::Let(vec![(Pat::Bind(w), wk)], Box::new(m)));
    match outer {
        Some((o, init)) => Expr::new(
            Ty::Int,
            Kind::Let(vec![(Pat::Bind(o), init)], Box::new(body)),
        ),
        None => body,
    }
}

/// `(for-each v (fn (x: i64) stmt))`: the closure goes to a `:borrow`
/// parameter, so it does not escape and may use `&` parameters (case 08).
fn for_each(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let v = g.expr(cx, &Ty::vec(Ty::Int), d - 1);
    let f = funcs::stmt_closure(g, cx, d);
    Expr::call(Ty::Unit, "for-each", vec![v, f])
}

/// `(let ((acc (cell 0))) (do (for-each v (fn (x) (set! acc (+ @acc x)))) @acc))`.
pub fn for_each_sum(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let (acc, x) = (g.fresh("acc"), g.fresh("x"));
    let ct = Ty::cell(Ty::Int);
    let v = g.expr(cx, &Ty::vec(Ty::Int), d - 1);
    let read = objects::deref(&Ty::Int, Expr::var(&acc, ct.clone()));
    let sum = Expr::call(Ty::Int, "+", vec![read.clone(), Expr::var(&x, Ty::Int)]);
    let set = Expr::new(
        Ty::Unit,
        Kind::Set(Box::new(Expr::var(&acc, ct.clone())), Box::new(sum)),
    );
    let f = Expr::new(
        Ty::Func(vec![Ty::Int], Box::new(Ty::Unit)),
        Kind::Fn(vec![(x, Ty::Int)], Box::new(set)),
    );
    let each = Expr::call(Ty::Unit, "for-each", vec![v, f]);
    let body = Expr::new(Ty::Int, Kind::Do(vec![each, read]));
    let init = Expr::call(ct, "cell", vec![Expr::int(0)]);
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(acc), init)], Box::new(body)),
    )
}
