//! Java's `Double.toString` text of a float (types §2.12, Decided,
//! owner, 2026-10-01), written down twice and independently of both
//! runtimes: as a table of hand-written values whose texts were taken
//! from Java 27's `Double.toString` and `Float.toString` (not from
//! anything of fibber's), and as a model that follows the definition
//! from the exact decimal expansion of the value, so that every value
//! of the 64,000 of `floats.rs` has a third opinion beside the two tools'.
//!
//! The one place the model is not Java's is where Java is not Rust's:
//! an exact tie between two decimals of the same length goes to the even
//! digit in Java and to the larger magnitude here (types §2.12), and the
//! model takes the larger. The table holds no such value.

use super::Width;

/// `(a value as Rust parses it, Java's `Double.toString` of it)`: the
/// powers of ten from 1e-10 to 1e22, the neighbours of the two thresholds
/// 1e-3 and 1e7, the smallest and largest doubles, subnormals that
/// one digit spells, and signs.
pub const F64_TEXTS: &[(&str, &str)] = &[
    ("1e-10", "1.0E-10"),
    ("1e-9", "1.0E-9"),
    ("1e-8", "1.0E-8"),
    ("1e-7", "1.0E-7"),
    ("1e-6", "1.0E-6"),
    ("1e-5", "1.0E-5"),
    ("1e-4", "1.0E-4"),
    ("1e-3", "0.001"),
    ("1e-2", "0.01"),
    ("1e-1", "0.1"),
    ("1e0", "1.0"),
    ("1e1", "10.0"),
    ("1e2", "100.0"),
    ("1e3", "1000.0"),
    ("1e4", "10000.0"),
    ("1e5", "100000.0"),
    ("1e6", "1000000.0"),
    ("1e7", "1.0E7"),
    ("1e8", "1.0E8"),
    ("1e9", "1.0E9"),
    ("1e10", "1.0E10"),
    ("1e11", "1.0E11"),
    ("1e12", "1.0E12"),
    ("1e13", "1.0E13"),
    ("1e14", "1.0E14"),
    ("1e15", "1.0E15"),
    ("1e16", "1.0E16"),
    ("1e17", "1.0E17"),
    ("1e18", "1.0E18"),
    ("1e19", "1.0E19"),
    ("1e20", "1.0E20"),
    ("1e21", "1.0E21"),
    ("1e22", "1.0E22"),
    ("0.001", "0.001"),
    ("9.999999999999998e-4", "9.999999999999998E-4"),
    ("0.0010000000000000002", "0.0010000000000000002"),
    ("9.999999e6", "9999999.0"),
    ("9999999.0", "9999999.0"),
    ("9999999.999999998", "9999999.999999998"),
    ("1e7", "1.0E7"),
    ("10000000.000000002", "1.0000000000000002E7"),
    ("123456789.0", "1.23456789E8"),
    ("1234567.0", "1234567.0"),
    ("100.0", "100.0"),
    ("0.5", "0.5"),
    ("1.5", "1.5"),
    ("123.456", "123.456"),
    ("0.1", "0.1"),
    ("1e23", "1.0E23"),
    ("2.5e-5", "2.5E-5"),
    ("12345e6", "1.2345E10"),
    ("1.2345e10", "1.2345E10"),
    ("4.9e-324", "4.9E-324"),
    ("9.9e-324", "9.9E-324"),
    ("1.5e-323", "1.5E-323"),
    ("2.2250738585072014e-308", "2.2250738585072014E-308"),
    ("1.7976931348623157e308", "1.7976931348623157E308"),
    ("0.30000000000000004", "0.30000000000000004"),
    ("-1.5e-7", "-1.5E-7"),
    ("-0.001", "-0.001"),
    ("-1234567.0", "-1234567.0"),
    ("-1e7", "-1.0E7"),
    ("-100.0", "-100.0"),
];

/// The same at `f32`, with `Float.toString`.
pub const F32_TEXTS: &[(&str, &str)] = &[
    ("0.1", "0.1"),
    ("1.0", "1.0"),
    ("100.0", "100.0"),
    ("0.001", "0.001"),
    ("9999999.0", "9999999.0"),
    ("1e7", "1.0E7"),
    ("1e-4", "1.0E-4"),
    ("1e10", "1.0E10"),
    ("3.4028235e38", "3.4028235E38"),
    ("1.4e-45", "1.4E-45"),
    ("2.8e-45", "2.8E-45"),
    ("1.17549435e-38", "1.1754944E-38"),
    ("16777216.0", "1.6777216E7"),
    ("1.5", "1.5"),
    ("123456.79", "123456.79"),
    ("0.3", "0.3"),
    ("-2.5e-8", "-2.5E-8"),
    ("3.14159274", "3.1415927"),
];

/// The exact decimal expansion of `a` (finite, positive, nonzero) at
/// width `w` as its digits, and the exponent of the first: `a` is
/// `d.ddd x 10^exp`.
fn expansion(w: Width, a: f64) -> (Vec<u8>, i32) {
    let text = match w {
        Width::F64 => format!("{a:.800e}"),
        Width::F32 => format!("{:.800e}", a as f32),
    };
    let (mantissa, exp) = text.split_once('e').expect("LowerExp writes an e");
    let digits = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    (digits, exp.parse().expect("LowerExp writes an exponent"))
}

/// Whether the decimal `digits` x 10^(exp - len + 1) reads back to `a`.
fn reads_back(w: Width, digits: &[u8], exp: i32, a: f64) -> bool {
    let text = format!(
        "{}e{}",
        String::from_utf8_lossy(digits),
        exp - (digits.len() as i32 - 1)
    );
    match w {
        Width::F64 => text.parse::<f64>() == Ok(a),
        Width::F32 => text.parse::<f32>() == Ok(a as f32),
    }
}

/// `digits` plus one in the last place, and the exponent it then has.
fn bumped(digits: &[u8], exp: i32) -> (Vec<u8>, i32) {
    let mut up = digits.to_vec();
    for d in up.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            return (up, exp);
        }
    }
    up.insert(0, b'1');
    up.pop();
    (up, exp + 1)
}

/// Java's digits of `a` (finite, positive, nonzero) and the exponent of
/// the first, by the definition: of the decimals that read back to `a`,
/// the shortest, but of two digits where one would do, and of those the
/// nearest to `a`, a tie going up. For each length n from 2 the only
/// candidates are the two decimals of n digits that bracket `a`, `D`
/// (the first n digits of the expansion) and `D` plus a unit; `D` plus a
/// unit is the nearer when the next digit of the expansion is 5 or more.
fn digits(w: Width, a: f64) -> (String, i32) {
    let (ds, exp) = expansion(w, a);
    for n in 2..=17 {
        let (down, up) = (&ds[..n], bumped(&ds[..n], exp));
        let (down_ok, up_ok) = (reads_back(w, down, exp, a), reads_back(w, &up.0, up.1, a));
        let (picked, e) = if up_ok && (ds[n] >= b'5' || !down_ok) {
            up
        } else if down_ok {
            (down.to_vec(), exp)
        } else {
            continue;
        };
        let text = String::from_utf8_lossy(&picked)
            .trim_end_matches('0')
            .to_string();
        return (if text.is_empty() { "0".into() } else { text }, e);
    }
    unreachable!("a float has a shortest spelling of at most 17 digits")
}

/// Java's text of the value of the bit pattern `bits` at width `w`.
pub fn text(w: Width, bits: u64) -> String {
    let x = w.value(bits);
    if x.is_nan() {
        return "NaN".into();
    }
    let sign = if bits & w.sign() != 0 { "-" } else { "" };
    if x.is_infinite() {
        return format!("{sign}Infinity");
    }
    let a = x.abs();
    if a == 0.0 {
        return format!("{sign}0.0");
    }
    let (ds, exp) = digits(w, a);
    let n = ds.len() as i32;
    if (1e-3..1e7).contains(&a) {
        // The decimal point stands after the first `point` digits.
        let point = exp + 1;
        let body = if point <= 0 {
            format!("0.{}{ds}", "0".repeat((-point) as usize))
        } else if point >= n {
            format!("{ds}{}.0", "0".repeat((point - n) as usize))
        } else {
            format!("{}.{}", &ds[..point as usize], &ds[point as usize..])
        };
        return format!("{sign}{body}");
    }
    let rest = if n > 1 { &ds[1..] } else { "0" };
    format!("{sign}{}.{rest}E{exp}", &ds[..1])
}
