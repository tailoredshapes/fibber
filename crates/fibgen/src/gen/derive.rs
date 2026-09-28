//! Values of the preamble's `Ver` (a struct) and `Lvl` (an enum with a
//! field-less variant), whose `Eq` and `Ord` are made by `derive`
//! (syntax §3.16, types §2.12), compared.

use crate::ast::{Expr, Kind};
use crate::ty::Ty;

use super::{Ctx, Gen};

/// A value of `Ver` or `Lvl`.
pub fn value(g: &mut Gen, cx: &Ctx, name: &'static str, d: u32) -> Expr {
    let ty = Ty::Derived(name);
    let sub = d.saturating_sub(1);
    let int = |g: &mut Gen| {
        if g.rng.chance(60) {
            Expr::int(g.rng.range(0, 2))
        } else {
            g.expr(cx, &Ty::Int, sub)
        }
    };
    if name == "Ver" {
        let (a, b) = (int(g), int(g));
        return Expr::call(ty, "Ver", vec![a, b]);
    }
    match g.rng.below(3) {
        0 => Expr::new(ty, Kind::Var("Low".into())),
        1 => {
            let n = int(g);
            Expr::call(ty, "Mid", vec![n])
        }
        _ => {
            let a = int(g);
            let s = if g.rng.chance(50) {
                Expr::new(Ty::Str, Kind::Str(["a", "b", ""][g.rng.below(3)].into()))
            } else {
                g.expr(cx, &Ty::Str, sub)
            };
            Expr::call(ty, "High", vec![a, s])
        }
    }
}

/// `(if (op x y) k1 k2)` over two values of one derived type.
pub fn compare_int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let name = if g.rng.chance(50) { "Ver" } else { "Lvl" };
    let (x, y) = (value(g, cx, name, d), value(g, cx, name, d));
    let op = ["=", "!=", "<", "<=", ">", ">="][g.rng.below(6)];
    let c = Expr::call(Ty::Bool, op, vec![x, y]);
    let (a, b) = (Expr::int(g.small()), Expr::int(g.small()));
    Expr::new(Ty::Int, Kind::If(Box::new(c), Box::new(a), Box::new(b)))
}

/// A value of a derived type folded: compared with a fixed one.
pub fn observe(g: &mut Gen, e: Expr) -> Expr {
    let Ty::Derived(name) = e.ty else {
        return Expr::int(0);
    };
    let fixed = if name == "Ver" {
        Expr::call(e.ty.clone(), "Ver", vec![Expr::int(1), Expr::int(1)])
    } else {
        Expr::call(e.ty.clone(), "Mid", vec![Expr::int(g.small())])
    };
    let c = Expr::call(Ty::Bool, "<", vec![e, fixed]);
    Expr::new(
        Ty::Int,
        Kind::If(Box::new(c), Box::new(Expr::int(1)), Box::new(Expr::int(2))),
    )
}
