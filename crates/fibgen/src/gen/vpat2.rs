//! Guards and clause bodies (syntax §3.6): a guard may read the
//! clause's variables and write cells, and runs only when its pattern
//! matched; a rest vector may leave its clause as the clause's value.

use crate::ast::{Clause, Expr, Kind, Pat};
use crate::ty::Ty;

use super::control::clause_patterns;
use super::vpat::{vector_clauses, wrap_in_wrap};
use super::{effects, objects, Ctx, Gen, Var, VarKind};

/// The lengths a vector pattern with irrefutable elements matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lens {
    /// Exactly `n` elements: `[p1 .. pn]`.
    Exact(usize),
    /// At least `k`: `[p1 .. pk & r]`.
    AtLeast(usize),
    /// Any: `_` or a variable.
    All,
}

/// A planned clause: its pattern, its variables, and the lengths it
/// covers when it is full (`None` for a refutable one).
pub type Planned = (Pat, Vec<Var>, Option<Lens>);

/// Puts a vector pattern inside another pattern; returns it and the
/// variables the outer part binds.
pub type Wrapper<'w> = &'w dyn Fn(&mut Gen, Pat) -> (Pat, Vec<Var>);

/// Where a `match`'s clauses are made: its result type, the cell its
/// guards count themselves in (if any), the depth.
pub struct Site<'s> {
    /// The type of every clause's body.
    pub ty: &'s Ty,
    /// The counter cell's name.
    pub counter: Option<&'s str>,
    /// The depth.
    pub d: u32,
}

/// `(op v k)` over an `i64` pattern variable, or `(op (count r) k)`
/// over a vector one; `None` when the pattern binds neither.
fn var_test(g: &mut Gen, vars: &[Var]) -> Option<Expr> {
    let usable: Vec<&Var> = vars
        .iter()
        .filter(|v| v.ty == Ty::Int || matches!(v.ty, Ty::Vec(_)))
        .collect();
    let v = *g.rng.pick(&usable)?;
    let mut x = Expr::var(&v.name, v.ty.clone());
    if v.ty != Ty::Int {
        x = Expr::call(Ty::Int, "count", vec![x]);
    }
    let op = ["<", "=", ">", ">="][g.rng.below(4)];
    let k = Expr::int(g.rng.range(0, 4));
    Some(Expr::call(Ty::Bool, op, vec![x, k]))
}

/// A guard: a test of the clause's variables, any `bool`, or either
/// after a statement (which may write a cell, store the clause's rest,
/// or `await`); with `counter`, `(do (set! c (+ @c 1)) ..)` first.
fn guard(g: &mut Gen, cx: &Ctx, vars: &[Var], counter: Option<&str>, d: u32) -> Expr {
    let sub = d.saturating_sub(1);
    let test = match var_test(g, vars) {
        Some(t) if g.rng.chance(60) => t,
        _ => g.expr(cx, &Ty::Bool, sub),
    };
    let mut steps = Vec::new();
    if let Some(c) = counter {
        let ct = Ty::cell(Ty::Int);
        let read = objects::deref(&Ty::Int, Expr::var(c, ct.clone()));
        let inc = Expr::call(Ty::Int, "+", vec![read, Expr::int(1)]);
        steps.push(Expr::new(
            Ty::Unit,
            Kind::Set(Box::new(Expr::var(c, ct)), Box::new(inc)),
        ));
    }
    if g.rng.chance(35) {
        steps.push(effects::stmt(g, cx, sub));
    }
    if steps.is_empty() {
        return test;
    }
    steps.push(test);
    Expr::new(Ty::Bool, Kind::Do(steps))
}

/// A clause over `pat` binding `vars`: its guard when `guarded`, and a
/// body of `ty` (sometimes a pattern variable of that type itself, so a
/// rest or an element leaves as the clause's value).
pub fn clause_for(
    g: &mut Gen,
    cx: &Ctx,
    site: &Site,
    pat: Pat,
    vars: Vec<Var>,
    guarded: bool,
) -> Clause {
    let (ty, counter, d) = (site.ty, site.counter, site.d);
    let inner = cx.with_all(vars.clone());
    let guard = guarded.then(|| guard(g, &inner, &vars, counter, d));
    let same: Vec<&Var> = vars.iter().filter(|v| &v.ty == ty).collect();
    let body = match g.rng.pick(&same) {
        Some(v) if g.rng.chance(40) => Expr::var(&v.name, ty.clone()),
        _ => g.expr(&inner, ty, d.saturating_sub(1)),
    };
    Clause { pat, guard, body }
}

/// `(let ((c (cell 0))) (+ (match v ..) (* 10 @c)))`: every guard
/// counts itself in `c` before its test, so how many guards ran, and
/// in which order, shows in the result (case 134).
pub fn counted_match(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let c = g.fresh("tries");
    let ct = Ty::cell(Ty::Int);
    let inner = cx.with(Var::new(c.clone(), ct.clone(), VarKind::Let));
    let (st, elem, wrap) = if g.rng.chance(25) {
        (Ty::Wrap, Ty::Int, true)
    } else {
        let e = [Ty::Int, Ty::Str, Ty::Wrap][g.rng.below(3)].clone();
        (Ty::vec(e.clone()), e, false)
    };
    let s = g.expr(&inner, &st, d - 1);
    let site = Site {
        ty: &Ty::Int,
        counter: Some(&c),
        d,
    };
    let clauses = if wrap {
        vector_clauses(g, &inner, &elem, &site, &wrap_in_wrap)
    } else {
        vector_clauses(g, &inner, &elem, &site, &|_, p| (p, Vec::new()))
    };
    let m = Expr::new(Ty::Int, Kind::GMatch(Box::new(s), clauses));
    let read = objects::deref(&Ty::Int, Expr::var(&c, ct.clone()));
    let tens = Expr::call(Ty::Int, "*", vec![Expr::int(10), read]);
    let body = Expr::call(Ty::Int, "+", vec![m, tens]);
    let init = Expr::call(ct, "cell", vec![Expr::int(0)]);
    Expr::new(
        Ty::Int,
        Kind::Let(vec![(Pat::Bind(c), init)], Box::new(body)),
    )
}

/// A `match` over an enum, an `Option`, a list or an integer with one
/// or two guarded clauses first (never redundant: nothing unguarded is
/// above them), then exhaustive unguarded clauses; on an integer,
/// distinct literal clauses before the catch-all.
pub fn guarded_match(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let st = [
        Ty::Shape,
        Ty::opt(Ty::Int),
        Ty::opt(Ty::Wrap),
        Ty::List,
        Ty::Int,
    ][g.rng.below(5)]
    .clone();
    let s = g.expr(cx, &st, d - 1);
    let mut plan: Vec<(Pat, Vec<Var>, bool)> = Vec::new();
    for _ in 0..g.rng.range(1, 2) {
        let mut ps = clause_patterns(g, &st);
        if st == Ty::Int && g.rng.chance(50) {
            ps = vec![(Pat::Lit(g.rng.range(0, 3)), Vec::new())];
        }
        let (p, vs) = ps.swap_remove(g.rng.below(ps.len()));
        plan.push((p, vs, true));
    }
    if st == Ty::Int {
        let first = g.rng.range(-2, 1);
        for i in 0..g.rng.range(0, 2) {
            plan.push((Pat::Lit(first + 2 * i), Vec::new(), false));
        }
    }
    for (p, vs) in clause_patterns(g, &st) {
        plan.push((p, vs, false));
    }
    let site = Site {
        ty,
        counter: None,
        d,
    };
    let clauses = plan
        .into_iter()
        .map(|(p, vs, guarded)| clause_for(g, cx, &site, p, vs, guarded))
        .collect();
    Expr::new(ty.clone(), Kind::GMatch(Box::new(s), clauses))
}
