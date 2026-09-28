//! Numbers of other widths and floats, as the model evaluates them
//! (types §2.12, Rust's semantics): integer `+ - * / rem neg` trap when
//! the exact result does not fit the width or the divisor is zero;
//! shifts take the amount modulo the width; float arithmetic is IEEE at
//! its width; float-to-integer conversions truncate and saturate.

use crate::ty::NumTy;

use super::eval::{trap, unsupported, Res};
use super::value::V;

/// `v` at `bits` bits: its low bits, sign-extended.
fn wrap(v: i128, bits: u32) -> i64 {
    let shift = 128 - bits;
    ((v << shift) >> shift) as i64
}

/// The low `bits` bits of `v`, read as unsigned.
fn unsigned(v: i64, bits: u32) -> u64 {
    if bits == 64 {
        v as u64
    } else {
        (v as u64) & ((1u64 << bits) - 1)
    }
}

/// An integer value at `bits` (64 is `i64`, a `V::Int`).
fn int(v: i64, bits: u32) -> V {
    match bits {
        8 => V::IntW(v, NumTy::I8),
        16 => V::IntW(v, NumTy::I16),
        32 => V::IntW(v, NumTy::I32),
        _ => V::Int(v),
    }
}

/// `(v, bits)` of an integer value.
fn as_int(v: &V) -> Option<(i64, u32)> {
    match v {
        V::Int(n) => Some((*n, 64)),
        V::IntW(n, t) => Some((*n, t.bits())),
        _ => None,
    }
}

fn width_name(bits: u32) -> &'static str {
    match bits {
        8 => "i8",
        16 => "i16",
        32 => "i32",
        _ => "i64",
    }
}

/// `v` exactly, or the overflow trap of `h` at `bits`.
fn checked(h: &str, v: i128, bits: u32) -> Res {
    if i128::from(wrap(v, bits)) == v {
        return Ok(int(v as i64, bits));
    }
    Err(trap(&format!(
        "integer overflow in {h} at {}",
        width_name(bits)
    )))
}

/// A binary integer operation at `bits`.
fn int_binary(h: &str, a: i64, b: i64, bits: u32) -> Res {
    let (x, y) = (i128::from(a), i128::from(b));
    let s = (b.rem_euclid(i64::from(bits))) as u32;
    match h {
        "+" => checked(h, x + y, bits),
        "-" => checked(h, x - y, bits),
        "*" => checked(h, x * y, bits),
        "/" | "rem" if b == 0 => Err(trap(&format!("integer {h} by zero"))),
        // The exact quotient of the minimum by -1 does not fit; Rust's
        // `%` traps there too, although its exact result, 0, fits.
        "/" | "rem" if b == -1 && a == wrap(1i128 << (bits - 1), bits) => Err(trap(&format!(
            "integer overflow in {h} at {}",
            width_name(bits)
        ))),
        "/" => checked(h, x / y, bits),
        "rem" => checked(h, x % y, bits),
        "bit-and" => Ok(int(a & b, bits)),
        "bit-or" => Ok(int(a | b, bits)),
        "bit-xor" => Ok(int(a ^ b, bits)),
        "shl" => Ok(int(wrap(i128::from(unsigned(a, bits)) << s, bits), bits)),
        "shr" => Ok(int(wrap(i128::from(unsigned(a, bits) >> s), bits), bits)),
        "sar" => Ok(int(a >> s, bits)),
        _ => Err(unsupported(format!("{h} on integers"))),
    }
}

fn int_unary(h: &str, a: i64, bits: u32) -> Res {
    match h {
        "neg" => checked(h, -i128::from(a), bits),
        "bit-not" => Ok(int(!a, bits)),
        "popcount" => Ok(int(i64::from(unsigned(a, bits).count_ones() as u8), bits)),
        _ => Err(unsupported(format!("{h} on an integer"))),
    }
}

/// `x` rounded to the precision of `t`.
fn round(x: f64, t: NumTy) -> f64 {
    if t == NumTy::F32 {
        f64::from(x as f32)
    } else {
        x
    }
}

fn float_op(h: &str, a: f64, b: Option<f64>, t: NumTy) -> Res {
    let r = match (h, b) {
        ("+", Some(b)) => a + b,
        ("-", Some(b)) => a - b,
        ("*", Some(b)) => a * b,
        ("/", Some(b)) => a / b,
        ("neg", None) => -a,
        _ => return Err(unsupported(format!("{h} on floats"))),
    };
    Ok(V::Flt(round(r, t), t))
}

/// A comparison of two numbers of one type.
fn compare(h: &str, a: &V, b: &V) -> Option<bool> {
    let (x, y) = match (a, b) {
        (V::Flt(x, _), V::Flt(y, _)) => (*x, *y),
        _ => {
            let ((x, _), (y, _)) = (as_int(a)?, as_int(b)?);
            return Some(match h {
                "=" => x == y,
                "!=" => x != y,
                "<" => x < y,
                "<=" => x <= y,
                ">" => x > y,
                _ => x >= y,
            });
        }
    };
    Some(match h {
        "=" => x == y,
        "!=" => x != y,
        "<" => x < y,
        "<=" => x <= y,
        ">" => x > y,
        _ => x >= y,
    })
}

/// The operation `h` on numbers other than the `i64` `+ - * rem` and
/// comparisons the builtins already do; `None` when it is not one.
pub fn op(h: &str, a: &[V]) -> Option<Res> {
    let wide = a.iter().any(|v| matches!(v, V::IntW(..) | V::Flt(..)));
    let new_op = matches!(
        h,
        "/" | "neg"
            | "bit-and"
            | "bit-or"
            | "bit-xor"
            | "bit-not"
            | "shl"
            | "shr"
            | "sar"
            | "popcount"
    );
    if !wide && !new_op {
        return None;
    }
    if matches!(h, "=" | "!=" | "<" | "<=" | ">" | ">=") {
        return Some(
            compare(h, a.first()?, a.get(1)?)
                .map(V::Bool)
                .ok_or_else(|| unsupported(format!("compare {a:?}"))),
        );
    }
    Some(match a {
        [V::Flt(x, t), V::Flt(y, _)] => float_op(h, *x, Some(*y), *t),
        [V::Flt(x, t)] => float_op(h, *x, None, *t),
        [x, y] => match (as_int(x), as_int(y)) {
            (Some((x, bits)), Some((y, _))) => int_binary(h, x, y, bits),
            _ => Err(unsupported(format!("{h} on {a:?}"))),
        },
        [x] => match as_int(x) {
            Some((x, bits)) => int_unary(h, x, bits),
            None => Err(unsupported(format!("{h} on {x:?}"))),
        },
        _ => Err(unsupported(format!("{h} on {a:?}"))),
    })
}

/// A float truncated toward zero and saturated into `bits` bits, signed
/// or unsigned; NaN is 0.
fn saturate(x: f64, bits: u32, signed: bool) -> i128 {
    if x.is_nan() {
        return 0;
    }
    let (lo, hi) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    let t = x.trunc();
    if t <= lo as f64 {
        lo
    } else if t >= hi as f64 {
        hi
    } else {
        t as i128
    }
}

/// `(op T v)`; `to` is `None` for `i64`.
pub fn convert(op: &str, to: Option<NumTy>, v: &V) -> Res {
    let bits = to.map_or(64, |t| t.bits());
    match (op, v) {
        ("trunc" | "sext", _) => {
            let (x, _) = as_int(v).ok_or_else(|| unsupported(format!("{op} {v:?}")))?;
            Ok(int(wrap(i128::from(x), bits), bits))
        }
        ("zext", _) => {
            let (x, from) = as_int(v).ok_or_else(|| unsupported(format!("zext {v:?}")))?;
            Ok(int(unsigned(x, from) as i64, bits))
        }
        ("fptosi", V::Flt(x, _)) => Ok(int(wrap(saturate(*x, bits, true), bits), bits)),
        ("fptoui", V::Flt(x, _)) => Ok(int(wrap(saturate(*x, bits, false), bits), bits)),
        ("sitofp" | "uitofp", _) => {
            let (x, from) = as_int(v).ok_or_else(|| unsupported(format!("{op} {v:?}")))?;
            let t = to.unwrap_or(NumTy::F64);
            // One rounding, at the target width.
            let f = match (op, t) {
                ("sitofp", NumTy::F32) => f64::from(x as f32),
                ("sitofp", _) => x as f64,
                (_, NumTy::F32) => f64::from(unsigned(x, from) as f32),
                _ => unsigned(x, from) as f64,
            };
            Ok(V::Flt(f, t))
        }
        ("fptrunc" | "fpext", V::Flt(x, _)) => {
            let t = to.unwrap_or(NumTy::F64);
            Ok(V::Flt(round(*x, t), t))
        }
        _ => Err(unsupported(format!("{op} of {v:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i8v(n: i64) -> V {
        V::IntW(n, NumTy::I8)
    }

    #[test]
    fn narrow_arithmetic_traps_on_overflow_and_masks_shifts() {
        assert!(matches!(
            op("+", &[i8v(100), i8v(27)]),
            Some(Ok(V::IntW(127, NumTy::I8)))
        ));
        assert!(op("+", &[i8v(100), i8v(28)]).is_some_and(|r| r.is_err()));
        assert!(op("rem", &[i8v(-128), i8v(-1)]).is_some_and(|r| r.is_err()));
        assert!(matches!(
            op("shl", &[V::Int(1), V::Int(64)]),
            Some(Ok(V::Int(1)))
        ));
        let shr = op("shr", &[V::IntW(-1, NumTy::I16), V::IntW(3, NumTy::I16)]);
        assert!(matches!(shr, Some(Ok(V::IntW(8191, NumTy::I16)))));
        assert!(op("+", &[V::Int(1), V::Int(2)]).is_none());
    }

    #[test]
    fn conversions_saturate_and_keep_bits() {
        let f = V::Flt(300.0, NumTy::F64);
        assert!(matches!(
            convert("fptosi", Some(NumTy::I8), &f),
            Ok(V::IntW(127, _))
        ));
        assert!(matches!(
            convert("fptoui", Some(NumTy::I8), &f),
            Ok(V::IntW(-1, _))
        ));
        assert!(matches!(convert("zext", None, &i8v(-1)), Ok(V::Int(255))));
        assert!(matches!(
            convert("trunc", Some(NumTy::I8), &V::Int(300)),
            Ok(V::IntW(44, _))
        ));
    }
}
