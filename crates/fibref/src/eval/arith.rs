//! The methods of the built-in instances (types §2.9, §2.12): `Num`,
//! `Bits`, `Eq`, `Ord`, `Hash`, `Show` on scalars and `str` (a
//! field-less enum by variant index), `Deref` on cells, atoms and weak
//! references; and the conversions. Signed overflow wraps; integer
//! division and remainder trap on zero (§2.12).

use crate::types::ast::{ConvOp, IntConv};
use crate::types::ty::Scalar;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::{bits, wrap, Val};

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
            "Deref" => self.deref_val(x),
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
            Val::Float(f, _) => f.to_bits() as i64,
            Val::Bool(b) => i64::from(*b),
            Val::Char(c) => i64::from(u32::from(*c)),
            Val::Kw(k) => fnv(self.keyword_name(*k)?.as_bytes()),
            Val::Tag(_, i) => i64::from(*i),
            Val::Ptr(p) => *p as i64,
            _ => 0,
        })
    }

    fn show(&self, x: &Val) -> R<String> {
        Ok(match x {
            Val::Obj(_) => format!("{:?}", self.string(x)?),
            Val::Int(n, _) => n.to_string(),
            Val::Float(f, _) => format!("{f:?}"),
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

fn int_binary(name: &str, a: i64, b: i64, w: Scalar) -> R<Val> {
    let mask = u64::MAX >> (64 - bits(w));
    let shift = (b as u32) & (bits(w) - 1);
    let n = match name {
        "+" => a.wrapping_add(b),
        "-" => a.wrapping_sub(b),
        "*" => a.wrapping_mul(b),
        "/" | "rem" if b == 0 => return Err(RunError::trap(format!("integer {name} by zero"))),
        "/" => a.wrapping_div(b),
        "rem" => a.wrapping_rem(b),
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

fn int_unary(name: &str, a: i64, w: Scalar) -> R<Val> {
    let mask = u64::MAX >> (64 - bits(w));
    let n = match name {
        "neg" => a.wrapping_neg(),
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
        (ConvOp::FloatToInt { signed: true }, Val::Float(x, _)) => Val::Int(wrap(*x as i64, t), t),
        (ConvOp::FloatToInt { signed: false }, Val::Float(x, _)) => {
            Val::Int(wrap(*x as u64 as i64, t), t)
        }
        (ConvOp::IntToFloat { signed: true }, Val::Int(n, _)) => Val::Float(round(*n as f64, t), t),
        (ConvOp::IntToFloat { signed: false }, Val::Int(n, w)) => {
            let u = (*n as u64) & (u64::MAX >> (64 - bits(*w)));
            Val::Float(round(u as f64, t), t)
        }
        _ => return Err(RunError::internal(format!("conversion {op:?} of {v:?}"))),
    })
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

    #[test]
    fn integer_arithmetic_wraps_and_division_by_zero_traps() {
        let r = int_binary("+", 127, 1, Scalar::I8).expect("adds");
        assert_eq!(r, Val::Int(-128, Scalar::I8));
        assert!(int_binary("/", 1, 0, Scalar::I64).is_err());
        assert_eq!(
            int_binary("shr", -1, 60, Scalar::I64),
            Ok(Val::Int(15, Scalar::I64))
        );
        assert_eq!(int_binary("<", 1, 2, Scalar::I64), Ok(Val::Bool(true)));
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
