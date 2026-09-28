//! Integers of every width and floats (types §2.12), folded into an
//! `i64` by a conversion. Most arithmetic cannot trap by construction:
//! its operands are masked so the exact result fits (`(+ (bit-and a M)
//! (bit-and b M))`), its divisors are odd (`(bit-or b 1)`). Now and then
//! an operation is left unmasked; if the model then predicts a trap,
//! the interpreter must trap with the same message (run.rs). Float
//! arithmetic never traps, so floats are divided and `rem`-ed by any
//! value, and NaN and the infinities arise, from a computed zero divisor
//! or from the literal forms `(/ 0.0 0.0)` and `(/ ±1.0 0.0)`; the
//! built-in comparisons on them are IEEE's (types §2.12, Decided,
//! owner, 2026-09-28).

use crate::ast::{Expr, Kind};
use crate::ty::{NumTy, Ty};

use super::{Ctx, Gen};

/// An integer width: `None` is `i64`.
type IntW = Option<NumTy>;

fn ty_of(t: IntW) -> Ty {
    t.map_or(Ty::Int, Ty::Num)
}

fn bits(t: IntW) -> u32 {
    t.map_or(64, |t| t.bits())
}

fn ilit(v: i64, t: IntW) -> Expr {
    match t {
        None => Expr::int(v),
        Some(n) => Expr::new(Ty::Num(n), Kind::IntW(v, n)),
    }
}

fn conv(op: &str, to: Option<NumTy>, e: Expr) -> Expr {
    let ty = to.map_or(Ty::Int, Ty::Num);
    Expr::new(ty, Kind::Conv(op.into(), to, Box::new(e)))
}

/// `(bit-and e m)`: `e` at `t` made to lie in `0..=m`.
fn masked(g: &mut Gen, cx: &Ctx, t: IntW, d: u32, m: i64) -> Expr {
    let e = int_expr(g, cx, t, d);
    Expr::call(ty_of(t), "bit-and", vec![e, ilit(m, t)])
}

/// A literal, a program value truncated to the width, or a float
/// converted (saturating) to it.
fn int_leaf(g: &mut Gen, cx: &Ctx, t: IntW, d: u32) -> Expr {
    let max = i64::MAX >> (64 - bits(t));
    match g.rng.below(5) {
        0 if g.rng.chance(30) => ilit(if g.rng.chance(50) { max } else { -max - 1 }, t),
        0..=2 => ilit(g.rng.range(-20, 20), t),
        3 => {
            let e = g.expr(cx, &Ty::Int, d);
            if t.is_none() {
                e
            } else {
                conv("trunc", t, e)
            }
        }
        _ => {
            let f = float_expr(g, cx, NumTy::F64, d);
            let op = if g.rng.chance(60) { "fptosi" } else { "fptoui" };
            conv(op, t, f)
        }
    }
}

/// An expression of integer type `t`.
pub fn int_expr(g: &mut Gen, cx: &Ctx, t: IntW, d: u32) -> Expr {
    if d == 0 || g.rng.chance(20) {
        return int_leaf(g, cx, t, d.saturating_sub(1));
    }
    let (ty, w, sub) = (ty_of(t), bits(t), d - 1);
    let half = (1i64 << (w - 2)) - 1;
    let bin = |h: &str, a: Expr, b: Expr| Expr::call(ty.clone(), h, vec![a, b]);
    match g.rng.below(10) {
        0 | 1 => {
            let h = if g.rng.chance(50) { "+" } else { "-" };
            bin(h, masked(g, cx, t, sub, half), masked(g, cx, t, sub, half))
        }
        2 => {
            let m = (1i64 << ((w - 2) / 2)) - 1;
            bin("*", masked(g, cx, t, sub, m), masked(g, cx, t, sub, m))
        }
        3 => {
            let h = if g.rng.chance(50) { "/" } else { "rem" };
            let odd = Expr::call(
                ty.clone(),
                "bit-or",
                vec![int_expr(g, cx, t, sub), ilit(1, t)],
            );
            bin(h, masked(g, cx, t, sub, half), odd)
        }
        4 => {
            let h = ["+", "-", "*", "/", "rem"][g.rng.below(5)];
            bin(h, int_expr(g, cx, t, sub), int_expr(g, cx, t, sub))
        }
        5 | 6 => bits_op(g, cx, t, sub),
        7 => {
            let n = masked(g, cx, t, sub, half);
            Expr::call(ty.clone(), "neg", vec![n])
        }
        _ => resize(g, cx, t, sub),
    }
}

/// `bit-and`, `bit-or`, `bit-xor`, `bit-not`, `popcount` and the shifts,
/// whose amount is any value (taken modulo the width).
fn bits_op(g: &mut Gen, cx: &Ctx, t: IntW, d: u32) -> Expr {
    let ty = ty_of(t);
    let h = [
        "bit-and", "bit-or", "bit-xor", "bit-not", "popcount", "shl", "shr", "sar",
    ][g.rng.below(8)];
    let a = int_expr(g, cx, t, d);
    if matches!(h, "bit-not" | "popcount") {
        return Expr::call(ty, h, vec![a]);
    }
    let b = int_expr(g, cx, t, d);
    Expr::call(ty, h, vec![a, b])
}

/// A value of another integer width converted to `t`: `trunc` from a
/// wider one, `sext` or `zext` from a narrower one.
fn resize(g: &mut Gen, cx: &Ctx, t: IntW, d: u32) -> Expr {
    let all = [Some(NumTy::I8), Some(NumTy::I16), Some(NumTy::I32), None];
    let others: Vec<IntW> = all.into_iter().filter(|o| *o != t).collect();
    let from = others[g.rng.below(others.len())];
    let e = int_expr(g, cx, from, d);
    let op = if bits(from) > bits(t) {
        "trunc"
    } else if g.rng.chance(50) {
        "sext"
    } else {
        "zext"
    };
    conv(op, t, e)
}

fn flit(x: f64, t: NumTy) -> Expr {
    Expr::new(Ty::Num(t), Kind::Flt(x, t))
}

/// A NaN, `(/ 0.0 0.0)`, or an infinity, `(/ 1.0 0.0)` or `(/ -1.0
/// 0.0)`: there is no literal for either (types §2.12).
fn special(g: &mut Gen, t: NumTy) -> Expr {
    let num = [0.0, 1.0, -1.0][g.rng.below(3)];
    Expr::call(Ty::Num(t), "/", vec![flit(num, t), flit(0.0, t)])
}

/// An expression of float type `t`.
pub fn float_expr(g: &mut Gen, cx: &Ctx, t: NumTy, d: u32) -> Expr {
    let ty = Ty::Num(t);
    if d == 0 || g.rng.chance(25) {
        return match g.rng.below(4) {
            0 => flit(g.rng.range(-40, 40) as f64 / 4.0, t),
            3 => special(g, t),
            1 => conv(
                "sitofp",
                Some(t),
                int_expr(g, cx, None, d.saturating_sub(1)),
            ),
            _ => conv(
                "uitofp",
                Some(t),
                int_expr(g, cx, Some(NumTy::I8), d.saturating_sub(1)),
            ),
        };
    }
    let sub = d - 1;
    match g.rng.below(8) {
        7 => {
            let h = if g.rng.chance(50) { "/" } else { "rem" };
            let (a, b) = (float_expr(g, cx, t, sub), float_expr(g, cx, t, sub));
            Expr::call(ty, h, vec![a, b])
        }
        0..=2 => {
            let h = ["+", "-", "*"][g.rng.below(3)];
            let (a, b) = (float_expr(g, cx, t, sub), float_expr(g, cx, t, sub));
            Expr::call(ty, h, vec![a, b])
        }
        3 => {
            let k = [-8, -4, -2, -1, 1, 2, 4, 8][g.rng.below(8)] as f64 / 2.0;
            Expr::call(ty, "/", vec![float_expr(g, cx, t, sub), flit(k, t)])
        }
        4 => Expr::call(ty, "neg", vec![float_expr(g, cx, t, sub)]),
        _ if t == NumTy::F64 => conv("fpext", Some(t), float_expr(g, cx, NumTy::F32, sub)),
        _ => conv("fptrunc", Some(t), float_expr(g, cx, NumTy::F64, sub)),
    }
}

/// `e` of a number type as an `i64`: extended, or converted (saturating).
pub fn to_i64(g: &mut Gen, e: Expr) -> Expr {
    match &e.ty {
        Ty::Num(n) if n.is_float() => {
            let op = if g.rng.chance(70) { "fptosi" } else { "fptoui" };
            conv(op, None, e)
        }
        Ty::Num(_) => {
            let op = if g.rng.chance(60) { "sext" } else { "zext" };
            conv(op, None, e)
        }
        _ => e,
    }
}

/// An `i64` from arithmetic at some width: a value converted, or a
/// comparison of two values of the width.
pub fn num_int(g: &mut Gen, cx: &Ctx, d: u32) -> Expr {
    let d = d.min(3);
    let ts = [
        Some(NumTy::I8),
        Some(NumTy::I16),
        Some(NumTy::I32),
        None,
        Some(NumTy::F32),
        Some(NumTy::F64),
    ];
    let t = ts[g.rng.below(ts.len())];
    let mk = |g: &mut Gen| match t {
        Some(f) if f.is_float() => float_expr(g, cx, f, d),
        _ => int_expr(g, cx, t, d),
    };
    let a = mk(g);
    if g.rng.chance(25) {
        let b = mk(g);
        let op = ["=", "!=", "<", "<=", ">", ">="][g.rng.below(6)];
        let c = Expr::call(Ty::Bool, op, vec![a, b]);
        let (x, y) = (Expr::int(g.small()), Expr::int(g.small()));
        return Expr::new(Ty::Int, Kind::If(Box::new(c), Box::new(x), Box::new(y)));
    }
    to_i64(g, a)
}
