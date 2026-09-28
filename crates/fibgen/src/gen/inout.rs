//! Helpers with `&` parameters (syntax §3.13) and their calls: copy-in,
//! write-back, forwarding at tail calls, distinct `&` arguments, aliases.

use crate::ast::{Arg, Expr, FunDef, Kind, Param, Pat};
use crate::ty::{inout_contents, Ty};

use super::helpers::call;
use super::{effects, objects, observe, Ctx, Gen, Var, VarKind};

fn cell_var(name: &str, content: &Ty) -> Var {
    Var::new(name.to_string(), Ty::cell(content.clone()), VarKind::Let)
}

/// An existing `&` helper (index), or a new one.
fn pick_or_make(g: &mut Gen, d: u32) -> Option<usize> {
    let ios: Vec<usize> = (0..g.funs.len())
        .filter(|&i| g.funs[i].params.iter().any(|p| p.inout))
        .collect();
    match g.rng.pick(&ios) {
        Some(&i) if g.rng.chance(50) || g.funs.len() >= g.max_funs => Some(i),
        _ if g.funs.len() < g.max_funs => Some(make(g, d)),
        _ => None,
    }
}

/// `(defun ioN (&v: T ... p n) -> unit|i64 ...)`: statements over the
/// `&` parameters, then a result; sometimes a countdown whose recursive
/// call forwards every `&` parameter (a tail call, §3.13 forwarding).
fn make(g: &mut Gen, d: u32) -> usize {
    let name = g.fresh("io");
    let (params, recursive) = io_params(g);
    let ret = if g.rng.chance(50) { Ty::Unit } else { Ty::Int };
    let hd = d.saturating_sub(1).clamp(1, 3);
    let cx = g.body_ctx(&params);
    let mut steps: Vec<Expr> = (0..g.rng.range(1, 3))
        .map(|_| inout_step(g, &cx, &params, hd))
        .collect();
    let last = g.expr(&cx, &ret, hd);
    let body = if recursive {
        steps.push(self_call(&name, &params, &ret));
        let again = Expr::new(ret.clone(), Kind::Do(steps));
        let test = Expr::call(Ty::Bool, "<=", vec![Expr::var("n", Ty::Int), Expr::int(0)]);
        Expr::new(
            ret.clone(),
            Kind::If(Box::new(test), Box::new(last), Box::new(again)),
        )
    } else {
        steps.push(last);
        Expr::new(ret.clone(), Kind::Do(steps))
    };
    g.funs.push(FunDef {
        name,
        params,
        ret,
        body,
    });
    g.funs.len() - 1
}

fn param(name: &str, ty: Ty, inout: bool) -> Param {
    Param {
        name: name.to_string(),
        ty,
        inout,
    }
}

/// One or two `&` parameters, sometimes a plain one, and for a
/// countdown a last parameter `n`; whether it is a countdown.
fn io_params(g: &mut Gen) -> (Vec<Param>, bool) {
    let contents = inout_contents();
    let mut params = Vec::new();
    for i in 0..g.rng.range(1, 2) {
        let t = contents[g.rng.below(contents.len())].clone();
        params.push(param(&format!("v{i}"), t, true));
    }
    if g.rng.chance(40) {
        let t = g.any_type(&Ctx::top(Vec::new(), false));
        params.push(param("p0", t, false));
    }
    let recursive = g.rng.chance(35);
    if recursive {
        params.push(param("n", Ty::Int, false));
    }
    (params, recursive)
}

/// `(ioN &v0 .. p0 (- n 1))`.
fn self_call(name: &str, params: &[Param], ret: &Ty) -> Expr {
    let args = params
        .iter()
        .map(|p| match (p.inout, p.name.as_str()) {
            (true, _) => Arg::InOut(p.name.clone()),
            (false, "n") => Arg::Val(Expr::call(
                Ty::Int,
                "-",
                vec![Expr::var("n", Ty::Int), Expr::int(1)],
            )),
            (false, _) => Arg::Val(Expr::var(&p.name, p.ty.clone())),
        })
        .collect();
    Expr::new(ret.clone(), Kind::Call(name.into(), args))
}

/// A statement that updates one of the `&` parameters.
fn inout_step(g: &mut Gen, cx: &Ctx, params: &[Param], d: u32) -> Expr {
    let ios: Vec<&Param> = params.iter().filter(|p| p.inout).collect();
    let v = ios[g.rng.below(ios.len())];
    let target = Expr::var(&v.name, Ty::cell(v.ty.clone()));
    match g.rng.below(4) {
        0 if v.ty == Ty::vec(Ty::Int) => {
            let x = g.expr(cx, &Ty::Int, d);
            Expr::new(
                Ty::Unit,
                Kind::Call(
                    "push!".into(),
                    vec![Arg::InOut(v.name.clone()), Arg::Val(x)],
                ),
            )
        }
        0 | 1 => {
            let val = g.expr(cx, &v.ty, d);
            Expr::new(Ty::Unit, Kind::Set(Box::new(target), Box::new(val)))
        }
        _ => effects::stmt(g, cx, d),
    }
}

/// Chooses `&` arguments for `f` from `cx` (distinct variables), making
/// fresh `let` cells for the rest; returns the bindings to wrap around
/// the call, the call, and the cells it updates.
/// `let` bindings to wrap around a call.
type Lets = Vec<(Pat, Expr)>;

/// The bindings to wrap around a call, the call, the cells it updates.
type CallParts = (Lets, Expr, Vec<Var>);

fn args_for(g: &mut Gen, cx: &Ctx, idx: usize, fresh_ok: bool, d: u32) -> Option<CallParts> {
    let f = g.funs[idx].clone();
    let (lets, cells) = choose_cells(g, cx, &f, fresh_ok, d)?;
    let plain = call(g, &cx.with_all(cells.clone()), idx, d);
    let Kind::Call(_, vals) = plain.kind else {
        return None;
    };
    let mut names = cells.iter().map(|c| c.name.clone());
    let args = f
        .params
        .iter()
        .zip(vals)
        .map(|(p, v)| {
            if p.inout {
                names.next().map(Arg::InOut)
            } else {
                Some(v)
            }
        })
        .collect::<Option<Vec<Arg>>>()?;
    let call = Expr::new(f.ret.clone(), Kind::Call(f.name.clone(), args));
    Some((lets, call, cells))
}

/// One distinct cell variable per `&` parameter of `f`: one in scope,
/// or (when `fresh_ok`) a new `let` cell, sometimes a second name for a
/// cell chosen before (case 67). `None` if the scope has too few.
fn choose_cells(
    g: &mut Gen,
    cx: &Ctx,
    f: &FunDef,
    fresh_ok: bool,
    d: u32,
) -> Option<(Lets, Vec<Var>)> {
    let mut lets = Vec::new();
    let mut cells: Vec<Var> = Vec::new();
    for p in f.params.iter().filter(|p| p.inout) {
        let cands: Vec<Var> = cx
            .cell_args(&p.ty)
            .into_iter()
            .filter(|v| cells.iter().all(|c| c.name != v.name))
            .cloned()
            .collect();
        let chosen = match g.rng.pick(&cands) {
            Some(v) if !fresh_ok || g.rng.chance(60) => v.clone(),
            _ if fresh_ok => {
                let c = g.fresh("c");
                let ct = Ty::cell(p.ty.clone());
                let prev = cells.iter().find(|v| v.ty == ct && v.kind == VarKind::Let);
                let init = match prev {
                    Some(prev) if g.rng.chance(30) => Expr::var(&prev.name, ct.clone()),
                    _ => Expr::call(ct, "cell", vec![g.expr(cx, &p.ty, d - 1)]),
                };
                lets.push((Pat::Bind(c.clone()), init));
                cell_var(&c, &p.ty)
            }
            _ => return None,
        };
        cells.push(chosen);
    }
    Some((lets, cells))
}

/// A statement calling an `&` helper on cells already in scope.
pub fn stmt_call(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let idx = pick_or_make(g, d)?;
    let (_, call, _) = args_for(g, cx, idx, false, d)?;
    if call.ty == Ty::Unit {
        return Some(call);
    }
    Some(Expr::new(
        Ty::Unit,
        Kind::Do(vec![call, Expr::new(Ty::Unit, Kind::Unit)]),
    ))
}

/// An `i64`: an `&` helper called on cells (fresh `let` cells where
/// needed, sometimes two names for one cell, case 67), then the result
/// and a cell's new content folded.
pub fn inout_int(g: &mut Gen, cx: &Ctx, d: u32) -> Option<Expr> {
    let idx = pick_or_make(g, d)?;
    let (lets, call, cells) = args_for(g, cx, idx, true, d)?;
    let c = &cells[g.rng.below(cells.len())];
    let content = match &c.ty {
        Ty::Cell(t) => (**t).clone(),
        t => t.clone(),
    };
    let read = objects::deref(&content, Expr::var(&c.name, Ty::cell(content.clone())));
    let inner = cx.with_all(cells.clone());
    let seen = observe::observe(g, &inner, read, d);
    let body = if call.ty == Ty::Int {
        Expr::call(Ty::Int, "+", vec![call, seen])
    } else {
        Expr::new(Ty::Int, Kind::Do(vec![call, seen]))
    };
    if lets.is_empty() {
        return Some(body);
    }
    Some(Expr::new(Ty::Int, Kind::Let(lets, Box::new(body))))
}
