//! Protocols (syntax §3.10, types §4): the program's `impl`s of the
//! preamble's `Score` and `Rank` on its structs and enum, with defaults
//! sometimes taken and sometimes overridden; method calls on concrete
//! receivers, on `dyn` values and on the bounded variable of a generic
//! helper; `dyn` values, their upcasts and conversions.
//!
//! No `impl` body calls a method: they are made before anything else,
//! with `methods_ok` off, so dispatch never recurses.

use crate::ast::{Expr, FunDef, ImplDef, Kind, Method, Param};
use crate::ty::{Proto, Ty};

use super::{Ctx, Gen, Var, VarKind};

pub use super::protos2::int_form;

/// The types that may implement the protocols.
const TARGETS: [Ty; 4] = [Ty::Wrap, Ty::Holder, Ty::Shape, Ty::Hook(false)];

/// The program's `impl`s: none (a program without protocols), or `Score`
/// and `Rank` for `Pt` (so every `dyn` type has a value) and each for
/// some of the other types, `Rank` only where `Score` is (types §4.1
/// rule 1).
pub fn impls(g: &mut Gen) -> Vec<ImplDef> {
    if !g.rng.chance(70) {
        return Vec::new();
    }
    let mut out = vec![one_impl(g, Proto::Score, Ty::Pt)];
    out.push(one_impl(g, Proto::Rank, Ty::Pt));
    for t in TARGETS {
        if !g.rng.chance(55) {
            continue;
        }
        let score = one_impl(g, Proto::Score, t.clone());
        let score_covers_all = score.colour_var;
        out.push(score);
        if g.rng.chance(45) {
            let mut rank = one_impl(g, Proto::Rank, t);
            // An impl for (Hook k) needs its supertrait's for (Hook k)
            // too (types §4.1 rule 1).
            rank.colour_var &= score_covers_all;
            out.push(rank);
        }
    }
    out
}

/// Whether `body` uses `self` other than as the receiver of `.`: passes
/// it, binds it, matches on it or joins it with another value, all at
/// the generator's view of it, a `(Hook :local)`. Such a body is legal
/// only under a `(Hook :local)` head: in `(impl P (Hook k) ..)`, `self`
/// is a `(Hook k)` with `k` rigid (types §1.3). A field read is legal
/// under both, since a `k` closure may go wherever a local one may.
pub fn self_as_value(body: &Expr) -> bool {
    let (mut uses, mut fields) = (0, 0);
    body.walk(&mut |e| match &e.kind {
        Kind::Var(n) if n == "self" => uses += 1,
        Kind::Field(s, _) if matches!(&s.kind, Kind::Var(n) if n == "self") => fields += 1,
        _ => {}
    });
    uses > fields
}

/// One `impl`: the required method, and the defaulted one sometimes.
fn one_impl(g: &mut Gen, proto: Proto, target: Ty) -> ImplDef {
    let (req, dflt) = match proto {
        Proto::Score => ("score", "bonus"),
        Proto::Rank => ("rank", "tier"),
    };
    let mut methods = vec![method(g, &target, req, false)];
    if g.rng.chance(50) {
        methods.push(method(g, &target, dflt, dflt == "bonus"));
    }
    let colour_var =
        matches!(target, Ty::Hook(_)) && !methods.iter().any(|m| self_as_value(&m.body));
    ImplDef {
        proto,
        target,
        methods,
        colour_var,
    }
}

/// A method body over `self` (and `k` for `bonus`).
fn method(g: &mut Gen, target: &Ty, name: &str, with_k: bool) -> Method {
    let mut cx = g.body_ctx(&[]);
    cx = cx.with(Var::new("self", target.clone(), VarKind::Param));
    let mut params = Vec::new();
    if with_k {
        cx = cx.with(Var::new("k", Ty::Int, VarKind::Param));
        params.push(("k".to_string(), Ty::Int));
    }
    let depth = g.rng.range(1, 3) as u32;
    let body = g.expr(&cx, &Ty::Int, depth);
    Method {
        name: name.to_string(),
        params,
        body,
    }
}

/// The strongest protocol values of `t` support, if any.
pub fn supports(g: &Gen, t: &Ty) -> Option<Proto> {
    if !g.methods_ok {
        return None;
    }
    match t {
        Ty::Dyn(p, _) | Ty::Gen(p, _) => Some(*p),
        _ => {
            let has = |p: Proto| g.impls.iter().any(|i| i.proto == p && covers(i, t));
            [Proto::Rank, Proto::Score].into_iter().find(|p| has(*p))
        }
    }
}

/// Whether the `impl` `i` covers values of `t`: one for `(Hook k)`
/// covers both colours, one for `(Hook :local)` only local hooks (types
/// §1.3, §4.1).
fn covers(i: &ImplDef, t: &Ty) -> bool {
    match (&i.target, t) {
        (Ty::Hook(_), Ty::Hook(send)) => i.colour_var || !send,
        (target, t) => target == t,
    }
}

/// The concrete types with an `impl` of (something entailing) `p`,
/// sendable ones only if `send`.
pub fn impl_types(g: &Gen, p: Proto, send: bool) -> Vec<Ty> {
    let all = [
        Ty::Pt,
        Ty::Wrap,
        Ty::Holder,
        Ty::Shape,
        Ty::Hook(true),
        Ty::Hook(false),
    ];
    all.into_iter()
        .filter(|t| supports(g, t).is_some_and(|q| q.entails(p)))
        .filter(|t| !send || t.is_send())
        .collect()
}

/// `(dyn P [:send] e)` over a value of a type implementing `P`, or a
/// conversion: `(dyn Score d)` of a `(dyn Score :send)` (types §2.15) or
/// an upcast of a `(dyn Rank ..)` (§4.1 rule 3).
pub fn dyn_value(g: &mut Gen, cx: &Ctx, p: Proto, send: bool, d: u32) -> Expr {
    let ty = Ty::Dyn(p, send);
    let sub = d.saturating_sub(1);
    if p == Proto::Score && g.rng.chance(35) {
        let from = match (send, g.rng.below(3)) {
            (false, 0) => Some(Ty::Dyn(Proto::Score, true)),
            (false, 1) => Some(Ty::Dyn(Proto::Rank, false)),
            (false, _) => Some(Ty::Dyn(Proto::Rank, true)),
            (true, 0) => Some(Ty::Dyn(Proto::Rank, true)),
            _ => None,
        };
        if let Some(from) = from {
            let src = g.expr(cx, &from, sub);
            return Expr::new(ty, Kind::Dyn(p, send, Box::new(src)));
        }
    }
    let ts = impl_types(g, p, send);
    let Some(t) = g.rng.pick(&ts).cloned() else {
        return dyn_leaf(g, p, send);
    };
    let e = g.expr(cx, &t, sub);
    Expr::new(ty, Kind::Dyn(p, send, Box::new(e)))
}

/// `(dyn P [:send] (Pt a b))`: `Pt` implements both protocols whenever
/// the program has any `impl`.
pub fn dyn_leaf(g: &mut Gen, p: Proto, send: bool) -> Expr {
    let pt = Expr::call(
        Ty::Pt,
        "Pt",
        vec![Expr::int(g.small()), Expr::int(g.small())],
    );
    Expr::new(Ty::Dyn(p, send), Kind::Dyn(p, send, Box::new(pt)))
}

/// A call of one of `p`'s methods on `recv`.
pub fn call_method(g: &mut Gen, cx: &Ctx, recv: Expr, p: Proto, d: u32) -> Expr {
    let ms = p.methods();
    // Rank's own methods half the time, its supertrait's the rest.
    let m = if p == Proto::Rank && g.rng.chance(50) {
        ms[2 + g.rng.below(2)]
    } else {
        ms[g.rng.below(ms.len())]
    };
    let mut args = vec![recv];
    if m == "bonus" {
        args.push(g.expr(cx, &Ty::Int, d.saturating_sub(1)));
    }
    Expr::call(Ty::Int, m, args)
}

/// A receiver supporting (something entailing) `p`: a variable in
/// scope, or a new value of an implementing type or a `dyn`.
pub fn receiver(g: &mut Gen, cx: &Ctx, p: Proto, d: u32) -> Option<Expr> {
    let vars: Vec<Var> = cx
        .vars_where(|t| supports(g, t).is_some_and(|q| q.entails(p)))
        .into_iter()
        .cloned()
        .collect();
    if g.rng.chance(50) {
        if let Some(v) = g.rng.pick(&vars) {
            return Some(Expr::var(&v.name, v.ty.clone()));
        }
    }
    let mut ts = impl_types(g, p, false);
    ts.push(Ty::Dyn(Proto::Rank, g.rng.chance(40)));
    if p == Proto::Score {
        ts.push(Ty::Dyn(Proto::Score, g.rng.chance(40)));
    }
    let t = g.rng.pick(&ts)?.clone();
    Some(g.expr(cx, &t, d.saturating_sub(1)))
}

/// `(defun gsN (x: a p0: T) [:where ((P a))] -> i64 (+ (m x ..) e))`: a
/// helper generic over a protocol-bounded `a`, the bound written or
/// inferred (types §3.6); with `Rank` written, `Score`'s methods are
/// still callable on `x` (§4.1 rule 2). Returns its index.
pub fn make_generic(g: &mut Gen, d: u32) -> usize {
    let name = g.fresh("gs");
    let p = if g.rng.chance(50) {
        Proto::Rank
    } else {
        Proto::Score
    };
    let mut params = vec![Param {
        name: "x".into(),
        ty: Ty::Gen(p, g.rng.chance(65)),
        inout: false,
    }];
    if g.rng.chance(50) {
        let t = g.any_type(&Ctx::top(Vec::new(), false));
        params.push(Param {
            name: "p0".into(),
            ty: t,
            inout: false,
        });
    }
    let cx = g.body_ctx(&params);
    let x = Expr::var("x", params[0].ty.clone());
    let hd = d.saturating_sub(1).clamp(1, 3);
    let first = call_method(g, &cx, x, p, hd);
    let rest = g.expr(&cx, &Ty::Int, hd);
    let body = Expr::call(Ty::Int, "+", vec![first, rest]);
    g.funs.push(FunDef {
        name,
        params,
        ret: Ty::Int,
        body,
    });
    g.funs.len() - 1
}
