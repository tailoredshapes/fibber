//! Integer and float literals (§1.1): spellings, suffixes, ranges.

use super::{err, kind};
use crate::syntax::{FltWidth, FormKind, IntWidth, ReadErrorKind};

fn int(v: i64, width: IntWidth) -> FormKind {
    FormKind::Int { v, width }
}

fn flt(v: f64, width: FltWidth) -> FormKind {
    FormKind::Flt { v, width }
}

fn out_of_range(text: &str, width: IntWidth) -> ReadErrorKind {
    ReadErrorKind::IntegerOutOfRange {
        text: text.to_string(),
        width,
    }
}

fn invalid_reason(src: &str) -> &'static str {
    match err(src) {
        ReadErrorKind::InvalidNumber { text, reason } => {
            assert_eq!(text, src);
            reason
        }
        other => panic!("{src}: expected InvalidNumber, got {other:?}"),
    }
}

#[test]
fn the_spelled_examples_of_section_1_1() {
    assert_eq!(kind("42"), int(42, IntWidth::I64));
    assert_eq!(kind("-7"), int(-7, IntWidth::I64));
    assert_eq!(kind("0x1F"), int(31, IntWidth::I64));
    assert_eq!(kind("0b1010"), int(10, IntWidth::I64));
    assert_eq!(kind("1_000_000"), int(1_000_000, IntWidth::I64));
    assert_eq!(kind("1i32"), int(1, IntWidth::I32));
    let pi_ish: f64 = "3.14".parse().expect("a float");
    assert_eq!(kind("3.14"), flt(pi_ish, FltWidth::F64));
    assert_eq!(kind("-0.5"), flt(-0.5, FltWidth::F64));
    assert_eq!(kind("1e9"), flt(1e9, FltWidth::F64));
    assert_eq!(kind("2.5e-3"), flt(2.5e-3, FltWidth::F64));
}

#[test]
fn every_suffix() {
    assert_eq!(kind("5i8"), int(5, IntWidth::I8));
    assert_eq!(kind("5i16"), int(5, IntWidth::I16));
    assert_eq!(kind("5i32"), int(5, IntWidth::I32));
    assert_eq!(kind("5i64"), int(5, IntWidth::I64));
    assert_eq!(kind("0x7Fi8"), int(127, IntWidth::I8));
    assert_eq!(kind("-0x80i8"), int(-128, IntWidth::I8));
    assert_eq!(kind("0.1f32"), flt(f64::from(0.1f32), FltWidth::F32));
    assert_eq!(kind("0.1f64"), flt(0.1, FltWidth::F64));
    assert_eq!(kind("1e3f32"), flt(1000.0, FltWidth::F32));
    // Hex digits include f: this is 0x1FF32, not 0x1F with a suffix.
    assert_eq!(kind("0x1Ff32"), int(0x1FF32, IntWidth::I64));
    assert_eq!(kind("1E3"), flt(1000.0, FltWidth::F64));
    assert_eq!(kind("1e+3"), flt(1000.0, FltWidth::F64));
}

#[test]
fn range_boundaries_of_every_width() {
    for (w, s) in [
        (IntWidth::I8, "i8"),
        (IntWidth::I16, "i16"),
        (IntWidth::I32, "i32"),
        (IntWidth::I64, "i64"),
    ] {
        let (lo, hi) = w.range();
        let lo64 = i64::try_from(lo).expect("fits");
        let hi64 = i64::try_from(hi).expect("fits");
        assert_eq!(kind(&format!("{lo}{s}")), int(lo64, w));
        assert_eq!(kind(&format!("{hi}{s}")), int(hi64, w));
        let below = format!("{}{s}", lo - 1);
        let above = format!("{}{s}", hi + 1);
        assert_eq!(err(&below), out_of_range(&below, w));
        assert_eq!(err(&above), out_of_range(&above, w));
    }
}

#[test]
fn the_out_of_range_examples_never_wrap() {
    assert_eq!(err("300i8"), out_of_range("300i8", IntWidth::I8));
    let big = "9223372036854775808";
    assert_eq!(err(big), out_of_range(big, IntWidth::I64));
    assert_eq!(kind("-9223372036854775808"), int(i64::MIN, IntWidth::I64));
    // Hex is a value, not a bit pattern: 0xFF does not fit i8.
    assert_eq!(err("0xFFi8"), out_of_range("0xFFi8", IntWidth::I8));
    assert_eq!(kind("0x7FFFFFFFFFFFFFFF"), int(i64::MAX, IntWidth::I64));
    let hex = "0x8000000000000000";
    assert_eq!(err(hex), out_of_range(hex, IntWidth::I64));
}

#[test]
fn floats_out_of_range() {
    for (src, w) in [("1e400", FltWidth::F64), ("1e39f32", FltWidth::F32)] {
        assert_eq!(
            err(src),
            ReadErrorKind::FloatOutOfRange {
                text: src.to_string(),
                width: w
            }
        );
    }
    // Underflow rounds; it is not an error.
    assert_eq!(kind("1e-400"), flt(0.0, FltWidth::F64));
}

#[test]
fn invalid_numbers_say_why() {
    assert!(invalid_reason("0xFFu8").contains("no unsigned types"));
    assert!(invalid_reason("1u32").contains("no unsigned types"));
    assert!(invalid_reason("1f32").contains("float suffix"));
    assert!(invalid_reason("1.5i32").contains("integer suffix"));
    assert!(invalid_reason("1_").contains("between two digits"));
    assert!(invalid_reason("1__0").contains("between two digits"));
    assert!(invalid_reason("0x_1").contains("expected a digit"));
    for bad in [
        "1abc", "1.", "1.e5", "1e", "1e+", "0x", "0b102", "1.5.6", "1/2.5", "1i128", "0X1F", "-0x",
        "1-2", "0x1.5",
    ] {
        invalid_reason(bad);
    }
}

#[test]
fn a_float_suffix_that_names_no_width_is_a_read_error() {
    // Syntax §1.1: only f32 and f64 (case 105 is the macro-built form).
    for bad in ["2.5f16", "2.5f128", "1e3f8"] {
        assert!(
            invalid_reason(bad).contains("not a width suffix (f32 f64)"),
            "{bad}"
        );
    }
}

#[test]
fn negative_zero_and_leading_zeros() {
    assert_eq!(kind("-0"), int(0, IntWidth::I64));
    assert_eq!(kind("007"), int(7, IntWidth::I64));
    let FormKind::Flt { v, .. } = kind("-0.0") else {
        panic!("not a float")
    };
    assert!(v == 0.0 && v.is_sign_negative());
    assert_ne!(kind("-0.0"), kind("0.0"));
}

#[test]
fn a_built_literal_is_checked_as_a_read_one() {
    use crate::syntax::check_literal;
    let mut k = int(300, IntWidth::I8);
    assert_eq!(
        check_literal(&mut k),
        Err(out_of_range("300i8", IntWidth::I8))
    );
    let msg = out_of_range("300i8", IntWidth::I8).to_string();
    assert!(msg.contains("does not fit i8"), "{msg}");
    let mut k = int(-128, IntWidth::I8);
    assert_eq!(check_literal(&mut k), Ok(()));
    assert_eq!(k, int(-128, IntWidth::I8));
    let mut k = int(i64::MIN, IntWidth::I64);
    assert_eq!(check_literal(&mut k), Ok(()));
}

#[test]
fn a_built_float_is_finite_and_f32_rounded() {
    use crate::syntax::check_literal;
    let mut k = flt(0.1, FltWidth::F32);
    assert_eq!(check_literal(&mut k), Ok(()));
    assert_eq!(k, kind("0.1f32"));
    let mut k = flt(1e300, FltWidth::F32);
    let r = check_literal(&mut k);
    assert!(
        matches!(r, Err(ReadErrorKind::FloatOutOfRange { .. })),
        "{r:?}"
    );
    let mut k = flt(f64::NAN, FltWidth::F64);
    assert!(matches!(
        check_literal(&mut k),
        Err(ReadErrorKind::InvalidNumber { .. })
    ));
    let mut k = flt(f64::INFINITY, FltWidth::F64);
    assert!(check_literal(&mut k).is_err());
}
