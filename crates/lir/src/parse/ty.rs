//! Types, result types and function types as written.

use super::arity;
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;
use crate::types::{scalar_keyword, Cc, FnType, Type};

/// A first-class type (spec/lir.md §2).
pub fn parse_type(s: &Sexp) -> Result<Type> {
    match s {
        Sexp::Atom(a, p) => {
            if let Some(t) = scalar_keyword(a) {
                return Ok(t);
            }
            if a == "void" {
                return err(*p, "void is only a function result type");
            }
            match a.strip_prefix("%struct.") {
                Some(n) if !n.is_empty() => Ok(Type::Named(n.to_string())),
                _ => err(*p, format!("unknown type {a}")),
            }
        }
        Sexp::VecType(n, e, p) => {
            let lanes = match n.parse::<u32>() {
                Ok(k) if (1..=1024).contains(&k) => k,
                _ => return err(*p, "vector length must be 1 to 1024"),
            };
            match scalar_keyword(e) {
                Some(t) => Ok(Type::Vector(lanes, Box::new(t))),
                None => err(*p, format!("unknown vector element type {e}")),
            }
        }
        Sexp::Brace(items, _) => Ok(Type::Anon(
            items.iter().map(parse_type).collect::<Result<_>>()?,
        )),
        Sexp::Bracket(items, p) => array_type(items, *p),
        other => err(
            other.pos(),
            format!("expected a type, found {}", other.describe()),
        ),
    }
}

/// `[N x T]` (spec/lir.md §2.1).
fn array_type(items: &[Sexp], p: Pos) -> Result<Type> {
    let [n, x, t] = items else {
        return err(p, "array type must be [N x T]");
    };
    if x.atom() != Some("x") {
        return err(p, "array type must be [N x T]");
    }
    let digits = n.atom().unwrap_or("");
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return err(
            n.pos(),
            format!(
                "array length must be a non-negative integer, found {}",
                n.describe()
            ),
        );
    }
    let len = match digits.parse::<u64>() {
        Ok(k) if k <= u64::from(u32::MAX) => k,
        _ => return err(n.pos(), format!("array length {digits} is too large")),
    };
    Ok(Type::Array(len, Box::new(parse_type(t)?)))
}

/// A result type: a type or `void`.
pub fn parse_ret(s: &Sexp) -> Result<Option<Type>> {
    match s {
        Sexp::Atom(a, _) if a == "void" => Ok(None),
        _ => parse_type(s).map(Some),
    }
}

/// A parameter type list `(T* ...?)`.
pub fn parse_params(s: &Sexp) -> Result<(Vec<Type>, bool)> {
    let Sexp::List(items, _) = s else {
        return err(
            s.pos(),
            format!("expected a parameter type list, found {}", s.describe()),
        );
    };
    let mut out = Vec::new();
    for (i, t) in items.iter().enumerate() {
        if t.atom() == Some("...") {
            if i + 1 != items.len() {
                return err(t.pos(), "... must be the last parameter type");
            }
            return Ok((out, true));
        }
        out.push(parse_type(t)?);
    }
    Ok((out, false))
}

/// An optional calling convention keyword at `items[0]`: returns the
/// convention and how many items it took.
pub fn parse_cc(items: &[Sexp]) -> (Cc, usize) {
    match items.first().and_then(Sexp::atom) {
        Some("tailcc") => (Cc::Tail, 1),
        Some("ccc") => (Cc::C, 1),
        _ => (Cc::C, 0),
    }
}

/// `(fn cc? R (T* ...?))`, or the error liar's bare result type gets.
pub fn parse_fn_type(s: &Sexp, op: &str) -> Result<FnType> {
    let found = || {
        err(
            s.pos(),
            format!(
                "{op} needs a function type (fn R (T..)), found {}",
                s.describe()
            ),
        )
    };
    let Sexp::List(items, pos) = s else {
        return found();
    };
    if items.first().and_then(Sexp::atom) != Some("fn") {
        return found();
    }
    let rest = &items[1..];
    let (cc, k) = parse_cc(rest);
    let rest = &rest[k..];
    arity("fn", rest, 2, *pos)?;
    let ret = parse_ret(&rest[0])?;
    let (params, varargs) = parse_params(&rest[1])?;
    Ok(FnType {
        cc,
        ret,
        params,
        varargs,
    })
}
