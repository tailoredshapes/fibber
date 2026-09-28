//! Literals (spec/lir.md §3).

use super::arity;
use crate::ast::{Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;
use crate::types::Type;

/// Parse an integer token: decimal, `0x`, `0b`, optional `-`.
pub fn int_token(s: &str) -> Option<i128> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let (radix, digits) = if let Some(h) = body.strip_prefix("0x") {
        (16, h)
    } else if let Some(b) = body.strip_prefix("0b") {
        (2, b)
    } else {
        (10, body)
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let v = i128::from_str_radix(digits, radix).ok()?;
    Some(if neg { -v } else { v })
}

fn is_int_text(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    !body.is_empty()
        && body.chars().all(|c| c.is_ascii_alphanumeric())
        && body.starts_with(|c: char| c.is_ascii_digit())
}

/// `v` fits `iK`: `-2^(K-1) ≤ v ≤ 2^K - 1`.
pub fn int_fits(bits: u32, v: i128) -> bool {
    let lo = -(1i128 << (bits - 1));
    let hi = (1i128 << bits) - 1;
    (lo..=hi).contains(&v)
}

/// An integer literal of type `iK` from its token.
pub fn int_value(bits: u32, tok: &str, pos: Pos) -> Result<i128> {
    let out_of_range = || {
        err(
            pos,
            format!("integer literal {tok} out of range for i{bits}"),
        )
    };
    match int_token(tok) {
        Some(v) if int_fits(bits, v) => Ok(v),
        Some(_) => out_of_range(),
        None if is_int_text(tok) => out_of_range(),
        None => err(pos, format!("invalid integer literal {tok}")),
    }
}

fn float_syntax(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let mut seen_digit = false;
    let mut chars = body.chars().peekable();
    while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
        chars.next();
        seen_digit = true;
    }
    if chars.peek() == Some(&'.') {
        chars.next();
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            chars.next();
            seen_digit = true;
        }
    }
    if matches!(chars.peek(), Some('e') | Some('E')) {
        chars.next();
        if matches!(chars.peek(), Some('+') | Some('-')) {
            chars.next();
        }
        let mut exp = false;
        while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
            chars.next();
            exp = true;
        }
        seen_digit &= exp;
    }
    seen_digit && chars.next().is_none()
}

/// A float literal of type `float` or `double` from its token.
pub fn float_value(ty: &Type, tok: &str, pos: Pos) -> Result<f64> {
    let v = match tok {
        "inf" => f64::INFINITY,
        "-inf" => f64::NEG_INFINITY,
        "nan" => f64::NAN,
        _ if float_syntax(tok) => tok
            .parse::<f64>()
            .or_else(|_| err(pos, format!("invalid float literal {tok}")))?,
        _ => return err(pos, format!("invalid float literal {tok}")),
    };
    let finite_token = !matches!(tok, "inf" | "-inf" | "nan");
    let overflow = match ty {
        Type::Float => (v as f32).is_infinite(),
        _ => v.is_infinite(),
    };
    if finite_token && overflow {
        return err(pos, format!("float literal out of range for {ty}"));
    }
    Ok(v)
}

/// `(iK n)`, `(float x)`, `(double x)`, `(ptr null)`.
pub fn scalar_literal(ty: Type, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let name = ty.to_string();
    arity(&name, args, 1, pos)?;
    let tok = match &args[0] {
        Sexp::Atom(a, _) => a.as_str(),
        other => return err(other.pos(), format!("{name} literal needs a number")),
    };
    let kind = match ty {
        Type::Int(b) => Kind::Int(ty, int_value(b, tok, args[0].pos())?),
        Type::Ptr if tok == "null" => Kind::Null,
        Type::Ptr => return err(pos, "the only ptr literal is (ptr null)"),
        _ => Kind::Float(ty.clone(), float_value(&ty, tok, args[0].pos())?),
    };
    Ok(Expr::new(kind, pos))
}

/// `(<N x E> v₁ .. v_N)`.
pub fn vector_literal(ty: Type, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let (n, elem) = match &ty {
        Type::Vector(n, e) => (*n as usize, (**e).clone()),
        _ => return err(pos, "not a vector type"),
    };
    if args.len() != n {
        return err(
            pos,
            format!(
                "vector literal {ty} needs {n} elements, found {}",
                args.len()
            ),
        );
    }
    let elems = args
        .iter()
        .map(|a| vector_element(&elem, a))
        .collect::<Result<_>>()?;
    Ok(Expr::new(Kind::Vector(ty, elems), pos))
}

/// `([N x T] v₁ .. v_N)`: any values; the checker types them.
pub fn array_literal(ty: Type, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let Type::Array(n, _) = &ty else {
        return err(pos, "not an array type");
    };
    if args.len() as u64 != *n {
        return err(
            pos,
            format!("{ty} literal: {n} elements expected, found {}", args.len()),
        );
    }
    let elems = args
        .iter()
        .map(super::expr::parse_expr)
        .collect::<Result<_>>()?;
    Ok(Expr::new(Kind::Array(ty, elems), pos))
}

fn vector_element(elem: &Type, a: &Sexp) -> Result<Expr> {
    let pos = a.pos();
    let Some(tok) = a.atom() else {
        return err(pos, "vector literal element must be a number");
    };
    let kind = match elem {
        Type::Int(b) if int_token(tok).is_some() || is_int_text(tok) => {
            Kind::Int(elem.clone(), int_value(*b, tok, pos)?)
        }
        Type::Ptr if tok == "null" => Kind::Null,
        Type::Float | Type::Double => {
            let v = float_value(elem, tok, pos).map_err(|mut d| {
                if d.message.starts_with("invalid") {
                    d.message = "vector literal element must be a number".into();
                }
                d
            })?;
            Kind::Float(elem.clone(), v)
        }
        _ => return err(pos, "vector literal element must be a number"),
    };
    Ok(Expr::new(kind, pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_ranges() {
        let p = Pos::default();
        assert_eq!(int_value(8, "255", p).unwrap(), 255);
        assert_eq!(int_value(8, "-128", p).unwrap(), -128);
        assert!(int_value(8, "300", p).is_err());
        assert!(int_value(8, "-129", p).is_err());
        assert_eq!(int_value(1, "-1", p).unwrap(), -1);
        assert!(int_value(1, "2", p).is_err());
        assert_eq!(
            int_value(64, "18446744073709551615", p).unwrap(),
            u64::MAX as i128
        );
        assert_eq!(int_value(8, "0b1010", p).unwrap(), 10);
        assert_eq!(int_value(16, "0xff", p).unwrap(), 255);
        let e = int_value(64, "99999999999999999999999999999999999999999", p).unwrap_err();
        assert!(e.message.contains("out of range"), "{}", e.message);
        assert!(int_value(32, "--5", p).is_err());
    }

    #[test]
    fn float_ranges() {
        let p = Pos::default();
        assert!(float_value(&Type::Double, "1e999", p).is_err());
        assert!(float_value(&Type::Float, "1e39", p).is_err());
        assert!(float_value(&Type::Double, "1e39", p).is_ok());
        assert!(float_value(&Type::Double, "inf", p).unwrap().is_infinite());
        assert!(float_value(&Type::Double, "--1.0", p).is_err());
        assert!(float_value(&Type::Double, "infinity", p).is_err());
        assert_eq!(float_value(&Type::Double, "-0.5e1", p).unwrap(), -5.0);
    }
}
