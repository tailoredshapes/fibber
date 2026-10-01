//! The methods of the built-in instances (types §2.9, §2.12): `Num`,
//! `Bits`, `Eq`, `Ord`, `Hash`, `Show` on scalars and `str` (a
//! field-less enum by variant index), `Deref` on cells, atoms and weak
//! references; and the conversions. Integer arithmetic has Rust's
//! semantics (§2.12, Decided): division and remainder by zero and
//! signed overflow trap, shift amounts are masked to the width, and
//! float-to-integer conversion saturates.

use crate::types::ast::{ConvOp, IntConv};
use crate::types::ty::Scalar;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::{bits, wrap, Val};

pub use super::floattext::float_text;

impl Interp<'_> {
    /// Method `m` of the built-in instance `inst` on `args`.
    pub fn native_method(&mut self, inst: usize, m: usize, args: &[Val]) -> R<Val> {
        let p = self.p;
        let g = &p.globals;
        let proto = g.proto(g.instances[inst].proto);
        let name = proto.methods[m].name.as_str();
        let x = args
            .first()
            .ok_or_else(|| RunError::internal(format!("{name} without a receiver")))?;
        match proto.name.as_str() {
            "Deref" => self.deref_val(x, Placement::Undecided),
            "Show" => {
                let text = self.show(x)?;
                self.new_str(text, Placement::Heap)
            }
            "Hash" => Ok(Val::Int(self.hash(x)?, Scalar::I64)),
            _ => {
                let y = args.get(1);
                self.scalar_method(name, x, y)
            }
        }
    }

    fn scalar_method(&self, name: &str, x: &Val, y: Option<&Val>) -> R<Val> {
        if let (Val::Obj(_), Some(y)) = (x, y) {
            let (a, b) = (self.string(x)?, self.string(y)?);
            return compare(name, a.cmp(b));
        }
        match (x, y) {
            (Val::Int(a, w), Some(Val::Int(b, _))) => int_binary(name, *a, *b, *w),
            (Val::Int(a, w), None) => int_unary(name, *a, *w),
            (Val::Float(a, w), Some(Val::Float(b, _))) => float_binary(name, *a, *b, *w),
            (Val::Float(a, w), None) if name == "neg" => Ok(Val::Float(round(-a, *w), *w)),
            (x, Some(y)) => compare(name, self.order(x, y)?),
            _ => Err(RunError::internal(format!("{name} on {x:?}"))),
        }
    }

    /// The order of two scalars of one type (declaration order for a
    /// field-less enum).
    fn order(&self, x: &Val, y: &Val) -> R<std::cmp::Ordering> {
        Ok(match (x, y) {
            (Val::Bool(a), Val::Bool(b)) => a.cmp(b),
            (Val::Char(a), Val::Char(b)) => a.cmp(b),
            (Val::Kw(a), Val::Kw(b)) => self.keyword_name(*a)?.cmp(self.keyword_name(*b)?),
            (Val::Unit, Val::Unit) => std::cmp::Ordering::Equal,
            (Val::Ptr(a), Val::Ptr(b)) => a.cmp(b),
            (Val::Tag(_, a), Val::Tag(_, b)) => a.cmp(b),
            _ => return Err(RunError::internal(format!("comparing {x:?} with {y:?}"))),
        })
    }

    fn hash(&self, x: &Val) -> R<i64> {
        Ok(match x {
            Val::Obj(_) => fnv(self.string(x)?.as_bytes()),
            Val::Int(n, _) => *n,
            Val::Float(f, _) => float_hash(*f),
            Val::Bool(b) => i64::from(*b),
            Val::Char(c) => i64::from(u32::from(*c)),
            Val::Kw(k) => fnv(self.keyword_name(*k)?.as_bytes()),
            Val::Tag(_, i) => i64::from(*i),
            Val::Ptr(p) => *p as i64,
            _ => 0,
        })
    }

    /// The text of `show` (types §2.12).
    fn show(&self, x: &Val) -> R<String> {
        Ok(match x {
            Val::Obj(_) => self.string(x)?.to_string(),
            Val::Int(n, _) => n.to_string(),
            Val::Float(f, w) => float_text(*f, *w),
            Val::Bool(b) => b.to_string(),
            Val::Char(c) => c.to_string(),
            Val::Kw(k) => format!(":{}", self.keyword_name(*k)?),
            Val::Unit => "()".to_string(),
            Val::Tag(t, i) => match &self.p.globals.ty(*t).shape {
                crate::types::decls::Shape::Enum(vs) => vs
                    .get(*i as usize)
                    .map_or_else(|| format!("{i}"), |v| v.name.clone()),
                _ => format!("{i}"),
            },
            Val::Ptr(p) => format!("#ptr{p:x}"),
            v => return Err(RunError::internal(format!("show of {v:?}"))),
        })
    }
}

/// The bits of the quiet NaN that every NaN hashes as (`hash` of a
/// float, types §2.12); `fib.double-hash` in `rt/core.lir` has the same.
const NAN_HASH: i64 = 0x7ff8_0000_0000_0000;

/// `hash` of a float (types §2.12): the bits of its value as an `f64`,
/// except that `-0.0` hashes as `0.0` and every NaN as [`NAN_HASH`], so
/// that `(= x y)` implies `(= (hash x) (hash y))` for floats too: a
/// `Map` finds the key `0.0` by `-0.0`.
pub fn float_hash(f: f64) -> i64 {
    if f.is_nan() {
        NAN_HASH
    } else if f == 0.0 {
        0
    } else {
        f.to_bits() as i64
    }
}

fn compare(name: &str, o: std::cmp::Ordering) -> R<Val> {
    use std::cmp::Ordering::*;
    let b = match name {
        "=" => o == Equal,
        "!=" => o != Equal,
        "<" => o == Less,
        "<=" => o != Greater,
        ">" => o == Greater,
        ">=" => o != Less,
        _ => return Err(RunError::internal(format!("{name} is not a comparison"))),
    };
    Ok(Val::Bool(b))
}

/// An integer `Num` or `Bits` method, or a comparison, at width `w`
/// (§2.12, Decided: Rust's semantics). Division and remainder by zero
/// trap; a signed result that does not fit `w` traps (`+ - * /`, and
/// `rem` of the minimum by -1, as Rust's does); the shift amount is
/// taken modulo the width.
fn int_binary(name: &str, a: i64, b: i64, w: Scalar) -> R<Val> {
    let mask = u64::MAX >> (64 - bits(w));
    let shift = (b as u32) & (bits(w) - 1);
    let n = match name {
        "+" => checked(name, i128::from(a) + i128::from(b), w)?,
        "-" => checked(name, i128::from(a) - i128::from(b), w)?,
        "*" => checked(name, i128::from(a) * i128::from(b), w)?,
        "/" | "rem" if b == 0 => return Err(RunError::trap(format!("integer {name} by zero"))),
        "/" => checked(name, i128::from(a) / i128::from(b), w)?,
        "rem" if b == -1 && a == i64::MIN >> (64 - bits(w)) => return Err(overflow(name, w)),
        "rem" => a % b,
        "bit-and" => a & b,
        "bit-or" => a | b,
        "bit-xor" => a ^ b,
        "shl" => a.wrapping_shl(shift),
        "shr" => (((a as u64) & mask) >> shift) as i64,
        "sar" => a >> shift,
        _ => return compare(name, a.cmp(&b)),
    };
    Ok(Val::Int(wrap(n, w), w))
}

/// `n` if it fits the signed width `w`, else the overflow trap.
fn checked(name: &str, n: i128, w: Scalar) -> R<i64> {
    let half = 1i128 << (bits(w) - 1);
    if (-half..half).contains(&n) {
        // In range of `w`, which is at most 64 bits wide.
        Ok(n as i64)
    } else {
        Err(overflow(name, w))
    }
}

fn overflow(name: &str, w: Scalar) -> RunError {
    RunError::trap(format!("integer overflow in {name} at {}", w.name()))
}

fn int_unary(name: &str, a: i64, w: Scalar) -> R<Val> {
    let mask = u64::MAX >> (64 - bits(w));
    let n = match name {
        "neg" => checked(name, -i128::from(a), w)?,
        "bit-not" => !a,
        "popcount" => i64::from(((a as u64) & mask).count_ones() as u8),
        _ => return Err(RunError::internal(format!("{name} on an integer"))),
    };
    Ok(Val::Int(wrap(n, w), w))
}

fn float_binary(name: &str, a: f64, b: f64, w: Scalar) -> R<Val> {
    let x = match name {
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "rem" => a % b,
        _ => {
            return Ok(Val::Bool(match name {
                "=" => a == b,
                "!=" => a != b,
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                ">=" => a >= b,
                _ => return Err(RunError::internal(format!("{name} on a float"))),
            }))
        }
    };
    Ok(Val::Float(round(x, w), w))
}

/// Rounds to `f32` for an `F32` value.
fn round(x: f64, w: Scalar) -> f64 {
    if w == Scalar::F32 {
        f64::from(x as f32)
    } else {
        x
    }
}

/// FNV-1a, 64 bits.
fn fnv(bytes: &[u8]) -> i64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h as i64
}

/// A conversion primitive (§2.12).
pub fn convert(op: ConvOp, t: Scalar, v: &Val) -> R<Val> {
    Ok(match (op, v) {
        (ConvOp::IntToInt(k), Val::Int(n, w)) => {
            let n = match k {
                IntConv::Zext => ((*n as u64) & (u64::MAX >> (64 - bits(*w)))) as i64,
                IntConv::Sext | IntConv::Trunc => *n,
            };
            Val::Int(wrap(n, t), t)
        }
        (ConvOp::FloatToFloat, Val::Float(x, _)) => Val::Float(round(*x, t), t),
        (ConvOp::FloatToInt { signed }, Val::Float(x, _)) => Val::Int(saturate(*x, signed, t), t),
        (ConvOp::IntToFloat { signed: true }, Val::Int(n, _)) => Val::Float(round(*n as f64, t), t),
        (ConvOp::IntToFloat { signed: false }, Val::Int(n, w)) => {
            let u = (*n as u64) & (u64::MAX >> (64 - bits(*w)));
            Val::Float(round(u as f64, t), t)
        }
        _ => return Err(RunError::internal(format!("conversion {op:?} of {v:?}"))),
    })
}

/// `fptosi`/`fptoui` to the width `t` (§2.12, Decided: Rust's `as`):
/// NaN is 0, and a value outside the target's range, infinities
/// included, gives the nearest end of it. An unsigned result is kept
/// as its bit pattern at the width, as every integer value is.
fn saturate(x: f64, signed: bool, t: Scalar) -> i64 {
    let b = bits(t);
    if signed {
        // `as` saturates to the i64 range and maps NaN to 0.
        (x as i64).clamp(i64::MIN >> (64 - b), i64::MAX >> (64 - b))
    } else {
        let max = u64::MAX >> (64 - b);
        wrap((x as u64).min(max) as i64, t)
    }
}

/// `char->i32`, `i32->char` (the latter traps on a non-scalar value).
pub fn char_conv(name: &str, v: &Val) -> R<Val> {
    match (name, v) {
        ("char->i32", Val::Char(c)) => Ok(Val::Int(i64::from(u32::from(*c)), Scalar::I32)),
        ("i32->char", Val::Int(n, _)) => u32::try_from(*n)
            .ok()
            .and_then(char::from_u32)
            .map(Val::Char)
            .ok_or_else(|| RunError::trap(format!("i32->char of {n}, not a Unicode scalar value"))),
        _ => Err(RunError::internal(format!("{name} of {v:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64, w: Scalar) -> R<Val> {
        Ok(Val::Int(n, w))
    }

    fn traps(r: R<Val>, text: &str) {
        let e = r.expect_err(text);
        assert!(e.to_string().contains(text), "{e}");
    }

    #[test]
    fn signed_overflow_and_division_by_zero_trap() {
        use Scalar::{I64, I8};
        traps(int_binary("+", 127, 1, I8), "integer overflow in + at i8");
        assert_eq!(int_binary("+", 126, 1, I8), int(127, I8));
        traps(int_binary("-", -128, 1, I8), "integer overflow in -");
        traps(
            int_binary("*", 1 << 32, 1 << 31, I64),
            "integer overflow in *",
        );
        traps(int_binary("/", i64::MIN, -1, I64), "integer overflow in /");
        traps(int_binary("/", -128, -1, I8), "integer overflow in /");
        traps(
            int_binary("rem", i64::MIN, -1, I64),
            "integer overflow in rem",
        );
        assert_eq!(int_binary("rem", -127, -1, I8), int(0, I8));
        assert_eq!(int_binary("/", -7, 2, I64), int(-3, I64));
        assert_eq!(int_binary("rem", -7, 2, I64), int(-1, I64));
        traps(int_unary("neg", i64::MIN, I64), "integer overflow in neg");
        assert_eq!(int_unary("neg", -127, I8), int(127, I8));
        traps(int_binary("/", 1, 0, I64), "integer / by zero");
        traps(int_binary("rem", 1, 0, I8), "integer rem by zero");
    }

    #[test]
    fn shift_amounts_are_masked_to_the_width() {
        use Scalar::{I64, I8};
        assert_eq!(int_binary("shl", 1, 64, I64), int(1, I64));
        assert_eq!(int_binary("shl", 1, 9, I8), int(2, I8));
        assert_eq!(int_binary("sar", -1, 65, I64), int(-1, I64));
        assert_eq!(int_binary("shr", -1, 127, I64), int(1, I64));
        assert_eq!(int_binary("shl", 1, -1, I64), int(i64::MIN, I64));
        assert_eq!(int_binary("shl", 1, 7, I8), int(-128, I8));
    }

    #[test]
    fn float_to_integer_saturates_and_nan_is_zero() {
        use Scalar::{I64, I8};
        let f = |x: f64| Val::Float(x, Scalar::F64);
        let si = |x, t| convert(ConvOp::FloatToInt { signed: true }, t, &f(x));
        let ui = |x, t| convert(ConvOp::FloatToInt { signed: false }, t, &f(x));
        assert_eq!(si(f64::NAN, I64), int(0, I64));
        assert_eq!(si(f64::INFINITY, I64), int(i64::MAX, I64));
        assert_eq!(si(-1e300, I64), int(i64::MIN, I64));
        assert_eq!(si(300.0, I8), int(127, I8));
        assert_eq!(si(-300.0, I8), int(-128, I8));
        assert_eq!(si(-2.7, I8), int(-2, I8));
        assert_eq!(ui(-5.0, I8), int(0, I8));
        assert_eq!(ui(300.0, I8), int(-1, I8)); // 255, as the bits of an i8
        assert_eq!(ui(200.0, I8), int(-56, I8));
        assert_eq!(ui(f64::NAN, I64), int(0, I64));
        assert_eq!(ui(1e30, I64), int(-1, I64)); // u64::MAX
    }

    #[test]
    fn comparisons_and_logical_shift_right() {
        assert_eq!(
            int_binary("shr", -1, 60, Scalar::I64),
            Ok(Val::Int(15, Scalar::I64))
        );
        assert_eq!(int_binary("<", 1, 2, Scalar::I64), Ok(Val::Bool(true)));
    }

    #[test]
    fn float_rem_is_fmod_with_the_sign_of_the_dividend() {
        let rem = |a, b, w| match float_binary("rem", a, b, w) {
            Ok(Val::Float(x, _)) => x,
            other => panic!("{other:?}"),
        };
        use Scalar::{F32, F64};
        assert_eq!(rem(-7.5, 2.0, F64), -1.5);
        assert_eq!(rem(7.5, -2.0, F64), 1.5);
        assert_eq!(rem(-7.5, -2.0, F32), -1.5);
        assert_eq!(rem(5.0, f64::INFINITY, F64), 5.0);
        assert!(rem(5.0, 0.0, F64).is_nan());
        assert!(rem(f64::INFINITY, 2.0, F64).is_nan());
        assert!(rem(-4.0, 2.0, F64).is_sign_negative()); // -0.0
    }

    #[test]
    fn zext_and_sext_differ_on_negative_operands() {
        let v = Val::Int(-1, Scalar::I8);
        let z = convert(ConvOp::IntToInt(IntConv::Zext), Scalar::I64, &v);
        let s = convert(ConvOp::IntToInt(IntConv::Sext), Scalar::I64, &v);
        assert_eq!(z, Ok(Val::Int(255, Scalar::I64)));
        assert_eq!(s, Ok(Val::Int(-1, Scalar::I64)));
    }
}
