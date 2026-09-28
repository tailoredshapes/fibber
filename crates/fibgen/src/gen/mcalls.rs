//! Calls of the preamble's macros (syntax §3.16): arguments that the
//! macro takes apart by a vector pattern on their form, or that fall
//! through; a rest parameter spliced and recursed over; arguments run
//! twice, bound to a gensym, or never run at all.

use crate::ast::{Expr, Kind};
use crate::macros::Mac;
use crate::ty::Ty;

use super::{effects, Ctx, Gen};

/// An `i64` from a macro call.
pub fn macro_int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let sub = d - 1;
    let int = |g: &mut Gen| g.expr(cx, &Ty::Int, sub);
    let (m, args) = match g.rng.below(7) {
        0 => (Mac::Twice, vec![int(g)]),
        1 => (Mac::Once2, vec![int(g)]),
        2 if g.rng.chance(60) => {
            let sub_form = Expr::call(Ty::Int, "-", vec![int(g), int(g)]);
            (Mac::SwapSub, vec![sub_form])
        }
        2 => (Mac::SwapSub, vec![int(g)]),
        3 if g.rng.chance(60) => {
            let c = g.expr(cx, &Ty::Bool, sub);
            let (t, e) = (int(g), int(g));
            let form = Expr::new(Ty::Int, Kind::If(Box::new(c), Box::new(t), Box::new(e)));
            (Mac::FlipIf, vec![form])
        }
        3 => (Mac::FlipIf, vec![int(g)]),
        4 => {
            let n = g.rng.range(0, 4) as usize;
            (Mac::SumAll, (0..n).map(|_| int(g)).collect())
        }
        5 => {
            let n = g.rng.range(1, 3) as usize;
            let args = (0..n)
                .map(|_| {
                    let t = g.any_type(cx);
                    g.expr(cx, &t, sub)
                })
                .collect();
            (Mac::Nargs, args)
        }
        _ => return seq(g, cx, &Ty::Int, sub),
    };
    Expr::new(Ty::Int, Kind::Macro(m, args))
}

/// `(seq step .. last)` of type `ty`: statements, then a value.
pub fn seq(g: &mut Gen, cx: &Ctx, ty: &Ty, d: u32) -> Expr {
    let mut steps: Vec<Expr> = (0..g.rng.below(3))
        .map(|_| effects::stmt(g, cx, d))
        .collect();
    steps.push(g.expr(cx, ty, d));
    Expr::new(ty.clone(), Kind::Macro(Mac::Seq, steps))
}
