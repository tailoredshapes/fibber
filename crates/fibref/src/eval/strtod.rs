//! `strtod` and `strtof` for the interpreter's `extern` table (syntax
//! §3.15): libc's conversion of the start of a NUL-terminated string in
//! raw memory to a float, so that a program that declares them (the
//! reader's `compiler/syntax/number.fib`) runs here as it does compiled.
//!
//! The C rules, in the C locale: skip white space (`isspace`), an
//! optional sign, then `inf`/`infinity`, `nan`/`nan(chars)` (any case),
//! or decimal digits with an optional `.` (at least one digit around
//! it) and an optional exponent that counts only when it has digits.
//! The value is the correctly rounded one for the result's own width
//! (`str::parse::<f32>` is not `f64` rounded again), overflow is
//! infinity, and with no conversion the result is 0 and the end pointer
//! is the start of the string. The end pointer is stored through the
//! second argument unless that is null.
//!
//! A hexadecimal float (`0x1.8p3`) is the one thing C converts that is
//! not done here: it is `unsupported`, never converted as the `0` before
//! the `x`, which is what would silently disagree with the compiled
//! program.

use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;
use crate::types::ty::{Scalar, Ty};

/// What `strtod` found at the start of a string.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    /// No conversion; the end is the start.
    Nothing,
    /// A hexadecimal float.
    Hex,
    /// `inf` or `infinity`, negative when the flag is set.
    Inf(bool),
    /// `nan`, negative when the flag is set.
    Nan(bool),
    /// A decimal number, written so that Rust's parser reads it.
    Decimal(String),
}

/// The literal at the start of `s` (up to its NUL) and the index just
/// after it, which is 0 when there is none.
pub fn scan(s: &[u8]) -> (Literal, usize) {
    let mut i = s.iter().take_while(|b| is_space(**b)).count();
    let neg = matches!(s.get(i), Some(b'-'));
    let signed = usize::from(matches!(s.get(i), Some(b'-' | b'+')));
    let rest = &s[i + signed..];
    if let Some(n) = word(rest, "infinity").or_else(|| word(rest, "inf")) {
        return (Literal::Inf(neg), i + signed + n);
    }
    if let Some(n) = nan_len(rest) {
        return (Literal::Nan(neg), i + signed + n);
    }
    if hex_start(rest) {
        return (Literal::Hex, 0);
    }
    match decimal(rest) {
        Some((text, n)) => {
            i += signed + n;
            (
                Literal::Decimal(format!("{}{text}", if neg { "-" } else { "" })),
                i,
            )
        }
        None => (Literal::Nothing, 0),
    }
}

/// C's `isspace` in the C locale.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t'..=b'\r')
}

/// The length of `w` when `s` starts with it, in any case.
fn word(s: &[u8], w: &str) -> Option<usize> {
    s.get(..w.len())
        .filter(|head| head.eq_ignore_ascii_case(w.as_bytes()))
        .map(|_| w.len())
}

/// `nan`, then `(` alphanumerics and `_` `)` when that is whole.
fn nan_len(s: &[u8]) -> Option<usize> {
    let n = word(s, "nan")?;
    if s.get(n) != Some(&b'(') {
        return Some(n);
    }
    let body = s[n + 1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
        .count();
    Some(if s.get(n + 1 + body) == Some(&b')') {
        n + body + 2
    } else {
        n
    })
}

/// `0x` followed by a hex digit, or by `.` and a hex digit.
fn hex_start(s: &[u8]) -> bool {
    let hex = |i: usize| s.get(i).is_some_and(u8::is_ascii_hexdigit);
    s.first() == Some(&b'0')
        && matches!(s.get(1), Some(b'x' | b'X'))
        && (hex(2) || (s.get(2) == Some(&b'.') && hex(3)))
}

/// A decimal float at the start of `s`: its text without the sign, and
/// its length. `5.` and `.5` are numbers, `.` is not; `e` counts when
/// digits follow it, after an optional sign.
fn decimal(s: &[u8]) -> Option<(String, usize)> {
    let digits = |from: usize| {
        s[from.min(s.len())..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let int = digits(0);
    let point = s.get(int) == Some(&b'.');
    let frac = if point { digits(int + 1) } else { 0 };
    if int + frac == 0 {
        return None;
    }
    let mut end = int + if point { 1 + frac } else { 0 };
    let mut text = String::from_utf8_lossy(&s[..int]).into_owned();
    if int == 0 {
        text.push('0');
    }
    if frac > 0 {
        text.push_str(&String::from_utf8_lossy(&s[int..int + 1 + frac]));
    }
    if matches!(s.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(s.get(end + 1), Some(b'-' | b'+')));
        let exp = digits(end + 1 + sign);
        if exp > 0 {
            text.push_str(&String::from_utf8_lossy(&s[end..end + 1 + sign + exp]).to_lowercase());
            end += 1 + sign + exp;
        }
    }
    Some((text, end))
}

/// The value of a literal at width `f32` or `f64`, as the `f64` that
/// holds it (an `f32` is held exactly).
fn value(lit: &Literal, width: Scalar) -> R<f64> {
    let single = width == Scalar::F32;
    Ok(match lit {
        Literal::Nothing => 0.0,
        Literal::Hex => return Err(RunError::unsupported("hex float")),
        Literal::Inf(neg) => signed(f64::INFINITY, *neg),
        Literal::Nan(neg) => signed(f64::NAN, *neg),
        Literal::Decimal(t) if single => t.parse::<f32>().map(f64::from).map_err(bad(t))?,
        Literal::Decimal(t) => t.parse::<f64>().map_err(bad(t))?,
    })
}

fn signed(x: f64, neg: bool) -> f64 {
    if neg {
        -x
    } else {
        x
    }
}

fn bad<E: std::fmt::Display>(text: &str) -> impl Fn(E) -> RunError + '_ {
    move |e| RunError::internal(format!("strtod: {text} is not a float: {e}"))
}

/// Whether the extern was declared `(ptr ptr) -> width`.
fn declared(ty: &Ty, width: Scalar) -> bool {
    use crate::types::ty::Con;
    let scalar = |t: &Ty| match t {
        Ty::Con(Con::Scalar(s), args) if args.is_empty() => Some(*s),
        _ => None,
    };
    match ty {
        Ty::Fn(_, params, ret) => {
            params.iter().map(scalar).collect::<Vec<_>>() == [Some(Scalar::Ptr); 2]
                && scalar(ret) == Some(width)
        }
        _ => false,
    }
}

impl Interp<'_> {
    /// `(strtod s end)` and `(strtof s end)`, declared as the extern
    /// `name` of the type `ty`: the float at the start of the string
    /// at `s`, and the address after it stored through `end` unless
    /// that is null.
    pub fn strto(&mut self, name: &str, ty: &Ty, a: &[Val]) -> R<Val> {
        let width = if name == "strtof" {
            Scalar::F32
        } else {
            Scalar::F64
        };
        let (s, end) = match a {
            [Val::Ptr(s), Val::Ptr(end)] if declared(ty, width) => (*s, *end),
            _ => {
                return Err(RunError::unsupported(format!(
                    "extern {name} declared other than (ptr ptr) -> {}",
                    if width == Scalar::F32 { "f32" } else { "f64" }
                )))
            }
        };
        let text = self.raw.c_string(s)?;
        let (lit, used) = scan(&text);
        let x = value(&lit, width)?;
        if end != 0 {
            self.raw.store(end, 8, s.wrapping_add(used as u64) as i64)?;
        }
        Ok(Val::Float(x, width))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::error::RunErrorKind;

    fn parse(s: &str, width: Scalar) -> (f64, usize) {
        let (lit, used) = scan(s.as_bytes());
        (value(&lit, width).expect("converts"), used)
    }

    #[test]
    fn decimals_stop_where_c_stops() {
        for (s, x, used) in [
            ("2.5e-3", 0.0025, 6),
            ("  \t\n-7.25abc", -7.25, 9),
            ("5.", 5.0, 2),
            (".5", 0.5, 2),
            ("5.e3", 5000.0, 4),
            ("1e5x", 1e5, 3),
            ("1e+", 1.0, 1),
            ("1E-2", 0.01, 4),
            ("+3", 3.0, 2),
            ("007", 7.0, 3),
            ("0x", 0.0, 1),
            ("0xg", 0.0, 1),
            ("1_000", 1.0, 1),
        ] {
            assert_eq!(parse(s, Scalar::F64), (x, used), "{s:?}");
        }
    }

    #[test]
    fn no_conversion_leaves_the_end_at_the_start() {
        for s in [
            "", " ", ".", "-", "+.e5", "e5", "x1", "--1", "- 1", "i", "na", "n",
        ] {
            assert_eq!(parse(s, Scalar::F64), (0.0, 0), "{s:?}");
        }
    }

    #[test]
    fn words_in_any_case() {
        assert_eq!(parse("INF", Scalar::F64), (f64::INFINITY, 3));
        assert_eq!(parse("-Infinity!", Scalar::F64), (f64::NEG_INFINITY, 9));
        assert_eq!(parse("infinit", Scalar::F64), (f64::INFINITY, 3));
        let (nan, used) = parse("-NaN(a_1)x", Scalar::F64);
        assert!(nan.is_nan() && nan.is_sign_negative());
        assert_eq!(used, 9);
        assert_eq!(parse("nan(a-)", Scalar::F64).1, 3);
        assert_eq!(parse("nan(a", Scalar::F64).1, 3);
    }

    #[test]
    fn overflow_is_infinity_and_underflow_is_zero() {
        assert_eq!(parse("1e999", Scalar::F64), (f64::INFINITY, 5));
        assert_eq!(
            parse("-1e99999999999999999999", Scalar::F64).0,
            f64::NEG_INFINITY
        );
        assert_eq!(parse("1e-999", Scalar::F64), (0.0, 6));
        assert_eq!(parse("1e39", Scalar::F32).0, f64::INFINITY);
        assert_eq!(parse("0e99999999999999999999", Scalar::F64).0, 0.0);
    }

    #[test]
    fn each_width_is_rounded_once() {
        assert_eq!(parse("0.1", Scalar::F64).0, 0.1f64);
        assert_eq!(parse("0.1", Scalar::F32).0, f64::from(0.1f32));
        assert_ne!(parse("0.1", Scalar::F32).0, 0.1f64);
        // Halfway between two f32s, and a hair over: rounding through
        // f64 first would round the second one down to the first.
        assert_eq!(parse("16777217", Scalar::F32).0, 16777216.0);
        assert_eq!(parse("16777217.000000001", Scalar::F32).0, 16777218.0);
    }

    #[test]
    fn a_hex_float_is_not_converted_here() {
        for s in ["0x1p3", " -0X.8", "0x1.8p3"] {
            let (lit, _) = scan(s.as_bytes());
            assert_eq!(lit, Literal::Hex, "{s:?}");
            let err = value(&lit, Scalar::F64).expect_err("unsupported");
            assert_eq!(err.kind, RunErrorKind::Unsupported);
            assert_eq!(err.message, "hex float");
        }
    }
}
