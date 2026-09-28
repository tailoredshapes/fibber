//! Top-level helper functions: plain ones (which may return their
//! parameters or parts of them), self and mutual tail recursion with
//! object accumulators, closure makers and async functions.

use crate::ast::{Expr, FunDef, Kind, Param};
use crate::ty::Ty;

use super::{funcs, tasks, Ctx, Gen, Region};

/// The depth of a helper's body.
fn body_depth(d: u32) -> u32 {
    d.saturating_sub(1).clamp(1, 3)
}

fn param(name: &str, ty: Ty) -> Param {
    Param {
        name: name.to_string(),
        ty,
        inout: false,
    }
}

/// A call of a helper returning `ret`: an existing one, or a new one.
pub fn helper_call(g: &mut Gen, cx: &Ctx, ret: &Ty, d: u32) -> Option<Expr> {
    let existing: Vec<usize> = (0..g.funs.len())
        .filter(|&i| {
            let ps = &g.funs[i].params;
            &g.funs[i].ret == ret && ps.iter().all(|p| !p.inout && !matches!(p.ty, Ty::Gen(..)))
        })
        .collect();
    let idx = match g.rng.pick(&existing) {
        Some(&i) if g.rng.chance(50) || g.funs.len() >= g.max_funs => i,
        _ if g.funs.len() < g.max_funs => make_helper(g, ret, d)?,
        _ => return None,
    };
    Some(call(g, cx, idx, d))
}

/// A call of helper `idx` with generated arguments; a recursion count
/// parameter named `n` gets a small literal.
pub fn call(g: &mut Gen, cx: &Ctx, idx: usize, d: u32) -> Expr {
    let f = g.funs[idx].clone();
    let args = f
        .params
        .iter()
        .map(|p| {
            if p.name == "n" {
                Expr::int(g.rng.range(0, 5))
            } else {
                g.expr(cx, &p.ty, d - 1)
            }
        })
        .collect();
    Expr::call(f.ret.clone(), &f.name, args)
}

/// Makes a helper returning `ret` and returns its index.
fn make_helper(g: &mut Gen, ret: &Ty, d: u32) -> Option<usize> {
    let hd = body_depth(d);
    let recursive = !matches!(ret, Ty::Unit | Ty::Task(_) | Ty::Atom(_));
    let kind = match ret {
        Ty::Task(_) => 3,
        Ty::Func(..) if g.rng.chance(50) => 4,
        _ if recursive => [0, 0, 1, 1, 2][g.rng.below(5)],
        _ => 0,
    };
    let f = match kind {
        1 => tail_self(g, ret, hd),
        2 => return Some(mutual(g, ret, hd)),
        3 => async_fn(g, ret, hd),
        4 => maker(g, ret, hd),
        _ => plain(g, ret, hd),
    };
    g.funs.push(f);
    Some(g.funs.len() - 1)
}

fn random_params(g: &mut Gen, max: usize, prefix: &str) -> Vec<Param> {
    let top = Ctx::top(Vec::new(), false);
    (0..g.rng.below(max + 1))
        .map(|i| {
            let t = g.any_type(&top);
            param(&format!("{prefix}{i}"), t)
        })
        .collect()
}

fn plain(g: &mut Gen, ret: &Ty, hd: u32) -> FunDef {
    let name = g.fresh("plain");
    let params = random_params(g, 3, "p");
    let body = g.expr(&g.body_ctx(&params), ret, hd);
    FunDef {
        name,
        params,
        ret: ret.clone(),
        body,
    }
}

/// `(defun tailN (n acc extra) (if (<= n 0) last (tailN (- n 1) step extra')))`.
fn tail_self(g: &mut Gen, ret: &Ty, hd: u32) -> FunDef {
    let name = g.fresh("tail");
    let mut params = vec![param("n", Ty::Int), param("acc", ret.clone())];
    params.extend(random_params(g, 1, "e"));
    let body = countdown(g, &params, ret, &name, hd);
    FunDef {
        name,
        params,
        ret: ret.clone(),
        body,
    }
}

/// `(if (<= n 0) last (callee (- n 1) step ...))`, the recursive call in
/// tail position, passing the other parameters on or new values.
fn countdown(g: &mut Gen, params: &[Param], ret: &Ty, callee: &str, hd: u32) -> Expr {
    let cx = g.body_ctx(params);
    let n = Expr::var("n", Ty::Int);
    let test = Expr::call(Ty::Bool, "<=", vec![n.clone(), Expr::int(0)]);
    let last = g.expr(&cx, ret, hd);
    let mut args = vec![Expr::call(Ty::Int, "-", vec![n, Expr::int(1)])];
    for p in &params[1..] {
        let pass_on = p.name != "acc" && g.rng.chance(60);
        args.push(if pass_on {
            Expr::var(&p.name, p.ty.clone())
        } else {
            g.expr(&cx, &p.ty, hd)
        });
    }
    let rec = Expr::call(ret.clone(), callee, args);
    Expr::new(
        ret.clone(),
        Kind::If(Box::new(test), Box::new(last), Box::new(rec)),
    )
}

/// Two functions that tail-call each other with an accumulator.
fn mutual(g: &mut Gen, ret: &Ty, hd: u32) -> usize {
    let (a, b) = (g.fresh("muta"), g.fresh("mutb"));
    let params = vec![param("n", Ty::Int), param("acc", ret.clone())];
    let body_a = countdown(g, &params, ret, &b, hd);
    let body_b = countdown(g, &params, ret, &a, hd);
    g.funs.push(FunDef {
        name: b,
        params: params.clone(),
        ret: ret.clone(),
        body: body_b,
    });
    g.funs.push(FunDef {
        name: a,
        params,
        ret: ret.clone(),
        body: body_a,
    });
    g.funs.len() - 1
}

/// `(defun asyncN (p ...) -> (Task T) (async ...))`: sendable parameters
/// only (the task captures them, syntax §3.14).
fn async_fn(g: &mut Gen, ret: &Ty, hd: u32) -> FunDef {
    let name = g.fresh("async");
    let params: Vec<Param> = (0..g.rng.below(3))
        .map(|i| param(&format!("p{i}"), g.send_type()))
        .collect();
    let inner = ret.inner().cloned().unwrap_or(Ty::Int);
    let cx = g.body_ctx(&params).for_region(Region::Task, true);
    let body = tasks::async_body(g, &cx, &inner, hd);
    FunDef {
        name,
        params,
        ret: ret.clone(),
        body,
    }
}

/// A function that returns a closure over its parameters.
fn maker(g: &mut Gen, ret: &Ty, hd: u32) -> FunDef {
    let name = g.fresh("make");
    let params = random_params(g, 2, "p");
    let cx = g.body_ctx(&params);
    let body = match ret {
        Ty::Func(ps, r) => funcs::literal(g, &cx, ps, r, true, hd + 1),
        _ => g.expr(&cx, ret, hd),
    };
    FunDef {
        name,
        params,
        ret: ret.clone(),
        body,
    }
}
