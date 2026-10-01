//! The float bit casts `f64->bits`, `bits->f64`, `f32->bits` and
//! `bits->f32` (syntax §4.3, types §2.12).

use crate::types::ty::Scalar;

use super::error::{RunError, R};
use super::value::Val;

/// `f64->bits`, `bits->f64`, `f32->bits`, `bits->f32` (syntax §4.3): a
/// float's IEEE 754 bit pattern and back, every bit kept, a NaN's
/// payload and a zero's sign included. An `f32` is held widened to
/// `f64` (`Val::Float`), and the hardware conversion between the two
/// quiets a signalling NaN, so a NaN is widened and narrowed by hand: its
/// sign, its payload shifted to and from the top of the wider mantissa,
/// the exponent all ones. A compiled program's `bitcast` keeps those bits
/// too, so the two ways agree on every input.
pub fn float_bits(name: &str, v: &Val) -> R<Val> {
    match (name, v) {
        ("f64->bits", Val::Float(x, _)) => Ok(Val::Int(x.to_bits() as i64, Scalar::I64)),
        ("bits->f64", Val::Int(n, _)) => Ok(Val::Float(f64::from_bits(*n as u64), Scalar::F64)),
        ("f32->bits", Val::Float(x, _)) => {
            Ok(Val::Int(i64::from(narrow_bits(*x) as i32), Scalar::I32))
        }
        ("bits->f32", Val::Int(n, _)) => Ok(Val::Float(widen_bits(*n as u32), Scalar::F32)),
        _ => Err(RunError::internal(format!("{name} of {v:?}"))),
    }
}

/// The bits of the `f32` that `x` holds widened.
fn narrow_bits(x: f64) -> u32 {
    if x.is_nan() {
        let b = x.to_bits();
        (((b >> 63) as u32) << 31) | 0x7f80_0000 | ((b >> 29) as u32 & 0x007f_ffff)
    } else {
        (x as f32).to_bits()
    }
}

/// The `f64` that holds the `f32` of these bits widened.
fn widen_bits(n: u32) -> f64 {
    let f = f32::from_bits(n);
    if f.is_nan() {
        let sign = u64::from(n >> 31) << 63;
        f64::from_bits(sign | 0x7ff0_0000_0000_0000 | (u64::from(n & 0x007f_ffff) << 29))
    } else {
        f64::from(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flt(x: f64, w: Scalar) -> Val {
        Val::Float(x, w)
    }

    /// The bits `float_bits` reads back from the value it made from `n`.
    fn f64_round_trip(n: i64) -> i64 {
        let x = float_bits("bits->f64", &Val::Int(n, Scalar::I64)).expect("bits->f64");
        match float_bits("f64->bits", &x) {
            Ok(Val::Int(m, Scalar::I64)) => m,
            other => panic!("{other:?}"),
        }
    }

    fn f32_round_trip(n: i64) -> i64 {
        let x = float_bits("bits->f32", &Val::Int(n, Scalar::I32)).expect("bits->f32");
        match float_bits("f32->bits", &x) {
            Ok(Val::Int(m, Scalar::I32)) => m,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn f64_bits_are_the_ieee_pattern() {
        let bits = |x: f64| float_bits("f64->bits", &flt(x, Scalar::F64));
        let int = |n: i64| Ok(Val::Int(n, Scalar::I64));
        assert_eq!(bits(1.0), int(0x3FF0_0000_0000_0000));
        assert_eq!(bits(-0.0), int(i64::MIN));
        assert_eq!(bits(0.0), int(0));
        assert_eq!(bits(5e-324), int(1));
        assert_eq!(bits(f64::MAX), int(0x7FEF_FFFF_FFFF_FFFF));
        assert_eq!(bits(f64::INFINITY), int(0x7FF0_0000_0000_0000));
        assert_eq!(
            bits(f64::NEG_INFINITY),
            int(0xFFF0_0000_0000_0000_u64 as i64)
        );
        match float_bits("bits->f64", &Val::Int(i64::MIN, Scalar::I64)) {
            Ok(Val::Float(x, Scalar::F64)) => assert!(x == 0.0 && x.is_sign_negative()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn f64_round_trips_every_pattern_including_nan_payloads() {
        for n in [
            0x7FF8_0000_0000_0001_u64, // a quiet NaN with a payload
            0x7FF0_0000_0000_0001,     // a signalling NaN
            0xFFFF_FFFF_FFFF_FFFF,     // negative, every payload bit
            0x7FF8_DEAD_BEEF_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0000,
        ] {
            assert_eq!(f64_round_trip(n as i64), n as i64, "{n:#x}");
        }
    }

    #[test]
    fn f32_bits_are_the_ieee_pattern_and_the_result_is_an_i32() {
        let bits = |x: f64| float_bits("f32->bits", &flt(x, Scalar::F32));
        let int = |n: i64| Ok(Val::Int(n, Scalar::I32));
        assert_eq!(bits(1.0), int(0x3F80_0000));
        assert_eq!(bits(-0.0), int(i64::from(i32::MIN)));
        assert_eq!(bits(f64::from(f32::MAX)), int(0x7F7F_FFFF));
        assert_eq!(bits(f64::from(f32::MIN_POSITIVE) / 2.0), int(0x0040_0000));
        assert_eq!(
            bits(f64::from(f32::NEG_INFINITY)),
            int(i64::from(0xFF80_0000_u32 as i32))
        );
        // A negative pattern is sign-extended, as every i32 value is held.
        assert_eq!(f32_round_trip(-1), -1);
        assert_eq!(f32_round_trip(i64::from(i32::MIN)), i64::from(i32::MIN));
    }

    #[test]
    fn f32_round_trips_every_pattern_including_signalling_nans() {
        for n in [
            0x7FC0_0001_u32, // a quiet NaN with a payload
            0x7F80_0001,     // a signalling NaN: widening by hardware would quiet it
            0xFFFF_FFFF,
            0xFFA0_0000,
            0x0000_0001,
            0x8000_0000,
            0x7F80_0000,
        ] {
            assert_eq!(
                f32_round_trip(i64::from(n as i32)),
                i64::from(n as i32),
                "{n:#x}"
            );
        }
    }

    #[test]
    fn bits_of_a_wrong_kind_of_value_is_an_internal_error() {
        let e = float_bits("f64->bits", &Val::Int(1, Scalar::I64)).expect_err("not a float");
        assert!(e.to_string().contains("f64->bits"), "{e}");
    }
}
