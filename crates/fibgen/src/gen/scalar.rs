//! Productions for `i64`, `bool` and `str`.

use crate::ast::{Expr, Kind};
use crate::ty::Ty;

use super::{
    effects, funcs, gadgets, gadgets2, hooks, objects, observe, protos, tasks, vpat, Ctx, Gen,
};

/// The text of a string literal.
pub fn literal_text(g: &mut Gen) -> String {
    let words = ["", "a", "bc", "def", "xyz", "hello"];
    words[g.rng.below(words.len())].to_string()
}

/// An `i64` expression: the fold of the program's observable state.
pub fn int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let weights = [
        2, // literal
        4, // arithmetic
        6, // observe a value of another type
        2, // call a closure
        2, // atom update or read
        2, // weak reference
        1, // plet
        1, // pmap
        1, // immediately applied closure
        2, // & helper through fresh cells
        2, // extract from a container
        1, // for-each accumulating in a cell
        4, // an ownership template (gadgets)
        3, // more ownership templates (gadgets2)
        8, // protocols: method calls, generic helpers, dyn (protos)
        2, // a vector match whose guards count themselves in a cell
        1, // a (Hook :send) crossing a thread
    ];
    let e = match g.rng.weighted(&weights) {
        Some(1) => Some(arith(g, cx, d)),
        Some(2) => {
            let t = g.any_type(cx);
            let e = g.expr(cx, &t, d - 1);
            Some(observe::observe(g, cx, e, d))
        }
        Some(3) => Some(funcs::apply(g, cx, d)),
        Some(4) => effects::atom_int(g, cx, d),
        Some(5) => Some(effects::weak_int(g, cx, d)),
        Some(6) => Some(tasks::plet(g, cx, &Ty::Int, d)),
        Some(7) => Some(tasks::pmap_sum(g, cx, d)),
        Some(8) => Some(funcs::immediate(g, cx, d)),
        Some(9) => funcs::inout_int(g, cx, d),
        Some(10) => objects::extract(g, cx, &Ty::Int, d),
        Some(11) => Some(effects::for_each_sum(g, cx, d)),
        Some(12) => gadgets::gadget(g, cx, d),
        Some(13) => gadgets2::gadget(g, cx, d),
        Some(14) => protos::int_form(g, cx, d),
        Some(15) => Some(vpat::counted_match(g, cx, d)),
        Some(16) => Some(hooks::cross(g, cx, d)),
        _ => None,
    };
    e.unwrap_or_else(|| Expr::int(g.small()))
}

/// Arithmetic that cannot trap (types §2.12: overflow traps): `rem` by
/// a positive literal, `*` by a small one, so that a product repeated
/// through a loop or a recursion (`(* acc acc)` squared an accumulator
/// past `i64`) grows no faster than the sums do.
fn arith(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let op = ["+", "-", "*", "+", "rem"][g.rng.below(5)];
    let a = g.expr(cx, &Ty::Int, d - 1);
    let b = match op {
        "rem" => Expr::int(g.rng.range(1, 7)),
        "*" => Expr::int(g.rng.range(-2, 3)),
        _ => g.expr(cx, &Ty::Int, d - 1),
    };
    Expr::call(Ty::Int, op, vec![a, b])
}

/// A `bool` expression.
pub fn boolean(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    match g.rng.below(6) {
        0 => Expr::new(Ty::Bool, Kind::Bool(g.rng.chance(50))),
        1 | 2 => {
            let op = ["<", "<=", "=", "!=", ">", ">="][g.rng.below(6)];
            let a = g.expr(cx, &Ty::Int, d - 1);
            let b = g.expr(cx, &Ty::Int, d - 1);
            Expr::call(Ty::Bool, op, vec![a, b])
        }
        3 => {
            let a = g.expr(cx, &Ty::Str, d - 1);
            let b = g.expr(cx, &Ty::Str, d - 1);
            Expr::call(Ty::Bool, "=", vec![a, b])
        }
        4 => {
            let inner = [Ty::Int, Ty::Str, Ty::Wrap][g.rng.below(3)].clone();
            let o = g.expr(cx, &Ty::opt(inner), d - 1);
            let f = if g.rng.chance(50) { "nil?" } else { "some?" };
            Expr::call(Ty::Bool, f, vec![o])
        }
        _ => {
            let b = g.expr(cx, &Ty::Bool, d - 1);
            Expr::call(Ty::Bool, "not", vec![b])
        }
    }
}

/// A `str` expression.
pub fn string(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    match g.rng.below(5) {
        0 => Expr::new(Ty::Str, Kind::Str(literal_text(g))),
        1 | 2 => {
            let a = g.expr(cx, &Ty::Str, d - 1);
            let b = g.expr(cx, &Ty::Str, d - 1);
            Expr::call(Ty::Str, "str-concat", vec![a, b])
        }
        _ => objects::extract(g, cx, &Ty::Str, d)
            .unwrap_or_else(|| Expr::new(Ty::Str, Kind::Str(literal_text(g)))),
    }
}
