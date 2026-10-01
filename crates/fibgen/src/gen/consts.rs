//! Top-level constants (`def`, syntax §3.19): immortal values that any
//! function, closure, thread or task may read without a count.

use crate::ast::{Def, Expr, Kind};
use crate::ty::Ty;

use super::{scalar, Gen, Var, VarKind};

/// The types a `def` may have: constant expressions build only
/// immutable objects and named functions.
fn def_types() -> Vec<Ty> {
    vec![
        Ty::Int,
        Ty::Str,
        Ty::Pt,
        Ty::Wrap,
        Ty::Shape,
        Ty::vec(Ty::Int),
        Ty::vec(Ty::Str),
        Ty::vec(Ty::Wrap),
        Ty::opt(Ty::Wrap),
        Ty::boxed(Ty::Str),
        Ty::List,
        Ty::fn_i(),
    ]
}

/// A constant expression of type `ty` (the grammar of syntax §3.19).
pub fn constant(g: &mut Gen, ty: &Ty, d: u32) -> Expr {
    let sub = d.saturating_sub(1);
    let call = |h: &str, args: Vec<Expr>| Expr::call(ty.clone(), h, args);
    match ty {
        Ty::Int => Expr::int(g.small()),
        Ty::Str => Expr::new(Ty::Str, Kind::Str(scalar::literal_text(g))),
        Ty::Pt => call("Pt", vec![Expr::int(g.small()), Expr::int(g.small())]),
        Ty::Wrap => {
            let s = constant(g, &Ty::Str, sub);
            let v = constant(g, &Ty::vec(Ty::Int), sub);
            call("Wrap", vec![s, v])
        }
        Ty::Shape if g.rng.chance(50) => call("Circle", vec![Expr::int(g.small())]),
        Ty::Shape => {
            let s = constant(g, &Ty::Str, sub);
            let w = constant(g, &Ty::Wrap, sub);
            call("Named", vec![s, w])
        }
        Ty::Vec(t) => {
            let n = if d == 0 { 0 } else { g.rng.below(4) };
            let items = (0..n).map(|_| constant(g, t, sub)).collect();
            Expr::new(ty.clone(), Kind::VecLit(items))
        }
        Ty::Opt(t) if d > 0 && g.rng.chance(70) => call("some", vec![constant(g, t, sub)]),
        Ty::Opt(_) => Expr::new(ty.clone(), Kind::Nil),
        Ty::Boxed(t) => call("Box", vec![constant(g, t, sub)]),
        Ty::List if d > 0 && g.rng.chance(70) => call(
            "Cons",
            vec![Expr::int(g.small()), constant(g, &Ty::List, sub)],
        ),
        Ty::List => Expr::new(Ty::List, Kind::Empty),
        _ => Expr::new(Ty::fn_i(), Kind::Global("inc1".into())),
    }
}

/// Up to `max` constants, as definitions and as the variables that name them.
pub fn defs(g: &mut Gen, max: usize) -> (Vec<Def>, Vec<Var>) {
    let types = def_types();
    let mut defs = Vec::new();
    let mut vars = Vec::new();
    for _ in 0..g.rng.below(max + 1) {
        let ty = types[g.rng.below(types.len())].clone();
        let name = g.fresh("tbl");
        let init = constant(g, &ty, 3);
        vars.push(Var {
            name: name.clone(),
            ty: ty.clone(),
            kind: VarKind::Global,
            send_fn: ty.is_fn(),
        });
        defs.push(Def { name, ty, init });
    }
    (defs, vars)
}
