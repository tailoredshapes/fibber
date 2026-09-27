//! Integer and float literals (spec/syntax.md §1.1).
//!
//! Grammar accepted, with `D` a run of digits of the radix in which `_`
//! may only stand between two digits:
//!
//! ```text
//! number  ::= -? (0x HEX | 0b BIN) isuffix?
//!           | -? DEC isuffix?
//!           | -? DEC (. DEC)? ([eE] [+-]? DEC)? fsuffix?   ; `.` or exponent present
//! isuffix ::= i8 | i16 | i32 | i64
//! fsuffix ::= f32 | f64
//! ```

use super::error::ReadErrorKind;
use super::form::{FltWidth, FormKind, IntWidth};

fn invalid(text: &str, reason: &'static str) -> ReadErrorKind {
    ReadErrorKind::InvalidNumber {
        text: text.to_string(),
        reason,
    }
}

/// Parses a token that starts with a digit or with `-` and a digit.
pub(crate) fn parse_number(text: &str) -> Result<FormKind, ReadErrorKind> {
    let (neg, body) = match text.strip_prefix('-') {
        Some(body) => (true, body),
        None => (false, text),
    };
    if let Some(rest) = body.strip_prefix("0x") {
        return parse_radix(text, neg, rest, 16);
    }
    if let Some(rest) = body.strip_prefix("0b") {
        return parse_radix(text, neg, rest, 2);
    }
    parse_decimal(text, neg, body)
}

/// Splits `s` after its leading run of digits of `radix` and `_`.
fn split_digits(s: &str, radix: u32) -> (&str, &str) {
    let end = s
        .find(|c: char| !(c == '_' || c.is_digit(radix)))
        .unwrap_or(s.len());
    s.split_at(end)
}

fn check_digits(text: &str, run: &str) -> Result<(), ReadErrorKind> {
    if run.is_empty() || run.starts_with('_') {
        return Err(invalid(text, "expected a digit"));
    }
    if run.ends_with('_') || run.contains("__") {
        return Err(invalid(text, "`_` may only stand between two digits"));
    }
    Ok(())
}

fn parse_radix(text: &str, neg: bool, rest: &str, radix: u32) -> Result<FormKind, ReadErrorKind> {
    let (digits, suffix) = split_digits(rest, radix);
    check_digits(text, digits)?;
    let width = int_suffix(text, suffix)?;
    make_int(text, neg, digits, radix, width)
}

fn int_suffix(text: &str, suffix: &str) -> Result<IntWidth, ReadErrorKind> {
    if suffix.is_empty() {
        return Ok(IntWidth::I64);
    }
    IntWidth::from_suffix(suffix).ok_or_else(|| {
        let reason = if suffix.starts_with('u') {
            "there are no unsigned types; an integer suffix is i8, i16, i32 or i64"
        } else if FltWidth::from_suffix(suffix).is_some() {
            "a float suffix needs a `.` or an exponent (write 1.0f32)"
        } else {
            "trailing characters are not a width suffix (i8 i16 i32 i64)"
        };
        invalid(text, reason)
    })
}

fn float_suffix(text: &str, suffix: &str) -> Result<FltWidth, ReadErrorKind> {
    if suffix.is_empty() {
        return Ok(FltWidth::F64);
    }
    FltWidth::from_suffix(suffix).ok_or_else(|| {
        let reason = if IntWidth::from_suffix(suffix).is_some() {
            "an integer suffix cannot go on a float literal"
        } else {
            "trailing characters are not a width suffix (f32 f64)"
        };
        invalid(text, reason)
    })
}

fn make_int(
    text: &str,
    neg: bool,
    digits: &str,
    radix: u32,
    width: IntWidth,
) -> Result<FormKind, ReadErrorKind> {
    let out_of_range = || ReadErrorKind::IntegerOutOfRange {
        text: text.to_string(),
        width,
    };
    let mut mag: u128 = 0;
    for c in digits.chars().filter(|c| *c != '_') {
        let d = c
            .to_digit(radix)
            .ok_or_else(|| invalid(text, "expected a digit"))?;
        mag = mag
            .checked_mul(u128::from(radix))
            .and_then(|m| m.checked_add(u128::from(d)))
            .ok_or_else(out_of_range)?;
    }
    let mag = i128::try_from(mag).map_err(|_| out_of_range())?;
    let value = if neg { -mag } else { mag };
    let (lo, hi) = width.range();
    if value < lo || value > hi {
        return Err(out_of_range());
    }
    let v = i64::try_from(value).map_err(|_| out_of_range())?;
    Ok(FormKind::Int { v, width })
}

fn parse_decimal(text: &str, neg: bool, body: &str) -> Result<FormKind, ReadErrorKind> {
    let (int_part, rest) = split_digits(body, 10);
    check_digits(text, int_part)?;
    let (frac, rest) = match rest.strip_prefix('.') {
        Some(after_dot) => {
            let (frac, rest) = split_digits(after_dot, 10);
            check_digits(text, frac)?;
            (Some(frac), rest)
        }
        None => (None, rest),
    };
    let (exp, suffix) = split_exponent(text, rest)?;
    if frac.is_none() && exp.is_none() {
        let width = int_suffix(text, suffix)?;
        return make_int(text, neg, int_part, 10, width);
    }
    let width = float_suffix(text, suffix)?;
    let clean = format!(
        "{}{}.{}{}",
        if neg { "-" } else { "" },
        int_part.replace('_', ""),
        frac.unwrap_or("0").replace('_', ""),
        exp.unwrap_or_default()
    );
    make_float(text, &clean, width)
}

/// Splits an exponent (`e`, optional sign, digits) off the front of
/// `rest`, returned as `e<sign><digits>` without underscores.
fn split_exponent<'a>(
    text: &str,
    rest: &'a str,
) -> Result<(Option<String>, &'a str), ReadErrorKind> {
    let Some(after_e) = rest.strip_prefix(['e', 'E']) else {
        return Ok((None, rest));
    };
    let (sign, after_sign) = match after_e.strip_prefix(['+', '-']) {
        // Cannot panic: the sign is the one-byte `+` or `-`.
        Some(after) => (&after_e[..1], after),
        None => ("", after_e),
    };
    let (digits, suffix) = split_digits(after_sign, 10);
    check_digits(text, digits)?;
    Ok((Some(format!("e{sign}{}", digits.replace('_', ""))), suffix))
}

fn make_float(text: &str, clean: &str, width: FltWidth) -> Result<FormKind, ReadErrorKind> {
    let parsed = match width {
        FltWidth::F64 => clean.parse::<f64>().ok(),
        FltWidth::F32 => clean.parse::<f32>().ok().map(f64::from),
    };
    let v = parsed.ok_or_else(|| invalid(text, "not a float"))?;
    if !v.is_finite() {
        return Err(ReadErrorKind::FloatOutOfRange {
            text: text.to_string(),
            width,
        });
    }
    Ok(FormKind::Flt { v, width })
}

/// Checks a literal the reader did not read (a `Form` a macro built,
/// §3.16) as the reader checks one it reads (§1.1): an `Int` must fit
/// its width and a `Flt` must be a finite number, an `f32` one becoming
/// the `f32` nearest its value, so the invariants [`FormKind`] states
/// hold. Every other form passes unchanged.
pub fn check_literal(kind: &mut FormKind) -> Result<(), ReadErrorKind> {
    match kind {
        FormKind::Int { v, width } => {
            let (lo, hi) = width.range();
            let n = i128::from(*v);
            if n < lo || n > hi {
                return Err(ReadErrorKind::IntegerOutOfRange {
                    text: format!("{v}{}", width.suffix()),
                    width: *width,
                });
            }
        }
        FormKind::Flt { v, width } => {
            let text = format!("{v:?}{}", width.suffix());
            if v.is_nan() {
                return Err(invalid(&text, "NaN is not a literal"));
            }
            let near = match width {
                // A saturating conversion: out of range becomes infinite.
                FltWidth::F32 => f64::from(*v as f32),
                FltWidth::F64 => *v,
            };
            if !near.is_finite() {
                return Err(ReadErrorKind::FloatOutOfRange {
                    text,
                    width: *width,
                });
            }
            *v = near;
        }
        _ => {}
    }
    Ok(())
}
