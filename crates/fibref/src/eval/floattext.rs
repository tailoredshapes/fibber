//! The text of `show` on a float (types §2.12, **Decided**, owner,
//! 2026-10-01; stdlib design §7 C12): Clojure's, which is Java's
//! `Double.toString` (`Float.toString` at `f32`).
//!
//! - The digits are the shortest decimal that reads back to the value at
//!   its width, the nearest to the value among those of that length, an
//!   exact tie going up (what Rust's `{:e}` writes). One rule is Java's
//!   own: when one digit would do, the nearest decimal of *two* digits
//!   is written, so the smallest `f64` is `4.9E-324` and not `5E-324`,
//!   and the smallest `f32` `1.4E-45`. It differs from the one digit only
//!   for subnormals, whose neighbours are far apart; every other value
//!   has the digit and a zero. Java breaks an exact tie to the even digit
//!   where Rust, and types §2.12, take the larger magnitude: the two
//!   differ on `2^-25` (`2.9802322387695312E-8` in Java): types §2.12 and
//!   the owner's decision of 2026-10-01 do not say, so it is left as it
//!   was, and `crates/fibc/tests/floats.rs` counts on it.
//! - The layout is positional, with at least one digit after the point,
//!   when `1e-3 <= |x| < 1e7` (`1.0`, `100.0`, `0.001`, `1234567.0`) and
//!   otherwise a digit, a point, at least one more digit, `E` and the
//!   exponent, with `-` when it is negative and no `+` (`1.0E7`,
//!   `1.0E-4`, `4.9E-324`).
//! - `NaN`, `Infinity`, `-Infinity`, and `-0.0`.
//!
//! The compiled runtime has the same text (`fib.show-fp` in
//! `crates/fibc/rt/str.lir`); `crates/fibc/tests/floats.rs` compares the
//! two over tens of thousands of values and against Java's text of
//! hand-written ones.

use crate::types::ty::Scalar;

/// A float's `show` text at the width `w`.
pub fn float_text(f: f64, w: Scalar) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let (digits, exp) = digits_of(f.abs(), w);
    let sign = if f.is_sign_negative() { "-" } else { "" };
    format!("{sign}{}", layout(&digits, exp))
}

/// The significant digits of a finite `a >= 0` (no trailing zero, but
/// one digit for zero) and the decimal exponent of the first: `a` is
/// `d.ddd x 10^exp`.
fn digits_of(a: f64, w: Scalar) -> (String, i32) {
    let shortest = e_text(a, w, None);
    if a == 0.0 || shortest.0.len() > 1 {
        return shortest;
    }
    // One digit would do: Java writes the nearest decimal of two.
    let (two, exp) = e_text(a, w, Some(1));
    let kept = two.trim_end_matches('0');
    (if kept.is_empty() { "0" } else { kept }.into(), exp)
}

/// Rust's `{:e}` of `a` at width `w`, or `{:.Pe}` for `Some(P)`, as the
/// digits without the point and the exponent.
fn e_text(a: f64, w: Scalar, precision: Option<usize>) -> (String, i32) {
    let text = match (w, precision) {
        (Scalar::F32, None) => format!("{:e}", a as f32),
        (Scalar::F32, Some(p)) => format!("{:.p$e}", a as f32),
        (_, None) => format!("{a:e}"),
        (_, Some(p)) => format!("{a:.p$e}"),
    };
    // LowerExp always writes `d[.ddd]e[-]N`; a text without the `e` could
    // only be a bug there, and reads as zero digits and exponent 0.
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    (mantissa.replace('.', ""), exp.parse().unwrap_or(0))
}

/// The layout of the digits `digits` whose first has exponent `exp`.
fn layout(digits: &str, exp: i32) -> String {
    if !(-3..7).contains(&exp) {
        let (head, tail) = digits.split_at(1);
        let fraction = if tail.is_empty() { "0" } else { tail };
        return format!("{head}.{fraction}E{exp}");
    }
    if exp < 0 {
        return format!("0.{}{digits}", "0".repeat((-exp - 1) as usize));
    }
    let whole = exp as usize + 1;
    if digits.len() <= whole {
        format!("{digits}{}.0", "0".repeat(whole - digits.len()))
    } else {
        format!("{}.{}", &digits[..whole], &digits[whole..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(x: f64) -> String {
        float_text(x, Scalar::F64)
    }

    fn s(x: f32) -> String {
        float_text(f64::from(x), Scalar::F32)
    }

    #[test]
    fn positional_from_a_thousandth_below_ten_million() {
        let texts: Vec<String> = [1.0, 100.0, 0.001, 1234567.0, 9999999.0, 0.5, 123.456]
            .iter()
            .map(|&x| d(x))
            .collect();
        assert_eq!(
            texts,
            [
                "1.0",
                "100.0",
                "0.001",
                "1234567.0",
                "9999999.0",
                "0.5",
                "123.456"
            ]
        );
    }

    #[test]
    fn scientific_outside_it() {
        let texts: Vec<String> = [1e7, 1e-4, 12345e6, 1e21, 1.5e-7, 1e100, -1.0e-10]
            .iter()
            .map(|&x| d(x))
            .collect();
        assert_eq!(
            texts,
            [
                "1.0E7",
                "1.0E-4",
                "1.2345E10",
                "1.0E21",
                "1.5E-7",
                "1.0E100",
                "-1.0E-10"
            ]
        );
    }

    #[test]
    fn the_special_values() {
        assert_eq!(d(f64::NAN), "NaN");
        assert_eq!(d(f64::INFINITY), "Infinity");
        assert_eq!(d(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(d(0.0), "0.0");
        assert_eq!(d(-0.0), "-0.0");
        assert_eq!(s(-0.0), "-0.0");
    }

    #[test]
    fn two_digits_where_one_would_do() {
        assert_eq!(d(f64::from_bits(1)), "4.9E-324");
        assert_eq!(d(f64::from_bits(2)), "9.9E-324");
        assert_eq!(d(f64::from_bits(3)), "1.5E-323");
        assert_eq!(s(f32::from_bits(1)), "1.4E-45");
        assert_eq!(s(f32::from_bits(2)), "2.8E-45");
        assert_eq!(d(1e23), "1.0E23");
    }

    #[test]
    fn f32_is_shortest_at_its_width() {
        assert_eq!(s(0.1), "0.1");
        assert_eq!(s(1e10), "1.0E10");
        assert_eq!(s(f32::MAX), "3.4028235E38");
        assert_eq!(s(16777217.0), "1.6777216E7");
    }
}
