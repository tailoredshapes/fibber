//! Expressions: names, literals and the dispatch to instruction forms.

use super::literal::{scalar_literal, vector_literal};
use super::ty::parse_type;
use super::{atomics, control, instr};
use crate::ast::{Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;
use crate::types::{scalar_keyword, Type};

/// Parse one expression (spec/lir.md §3, §6).
pub fn parse_expr(s: &Sexp) -> Result<Expr> {
    match s {
        Sexp::Atom(a, p) => atom_expr(a, *p),
        Sexp::Str(_, p) => err(*p, "a string must be written (string \"…\")"),
        Sexp::Brace(items, p) => {
            let fields = items.iter().map(parse_expr).collect::<Result<_>>()?;
            Ok(Expr::new(Kind::Struct(None, fields), *p))
        }
        Sexp::VecType(..) => err(s.pos(), format!("expected a value, found {}", s.describe())),
        Sexp::List(items, p) => list_expr(items, *p),
    }
}

fn atom_expr(a: &str, p: Pos) -> Result<Expr> {
    if let Some(g) = a.strip_prefix('@') {
        if g.is_empty() {
            return err(p, "invalid name @");
        }
        return Ok(Expr::new(Kind::Global(g.to_string()), p));
    }
    let body = a.strip_prefix('-').unwrap_or(a);
    if body.starts_with(|c: char| c.is_ascii_digit()) {
        return err(
            p,
            format!("a number must be written with its type, as (i32 {a})"),
        );
    }
    Ok(Expr::new(Kind::Local(a.to_string()), p))
}

fn list_expr(items: &[Sexp], pos: Pos) -> Result<Expr> {
    let Some(head) = items.first() else {
        return err(pos, "empty form ()");
    };
    let args = &items[1..];
    match head {
        Sexp::VecType(..) => vector_literal(parse_type(head)?, args, pos),
        Sexp::Atom(name, _) => named_form(name, args, pos),
        other => err(
            other.pos(),
            format!("expected an instruction name, found {}", other.describe()),
        ),
    }
}

fn named_form(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    if let Some(ty) = scalar_keyword(name) {
        return scalar_literal(ty, args, pos);
    }
    if let Some(s) = name.strip_prefix("%struct.") {
        let fields = args.iter().map(parse_expr).collect::<Result<_>>()?;
        return Ok(Expr::new(Kind::Struct(Some(s.to_string()), fields), pos));
    }
    if name == "string" {
        return string_literal(args, pos);
    }
    if let Some(r) = instr::parse(name, args, pos) {
        return r;
    }
    if let Some(r) = atomics::parse(name, args, pos) {
        return r;
    }
    if let Some(r) = control::parse(name, args, pos) {
        return r;
    }
    err(pos, format!("unknown instruction {name}"))
}

fn string_literal(args: &[Sexp], pos: Pos) -> Result<Expr> {
    match args {
        [Sexp::Str(bytes, _)] => Ok(Expr::new(Kind::Str(bytes.clone()), pos)),
        _ => err(pos, "string expects one string operand"),
    }
}

/// Parse each operand.
pub fn exprs(args: &[Sexp]) -> Result<Vec<Expr>> {
    args.iter().map(parse_expr).collect()
}

/// Parse a type operand, the `T` of `(load T p)` and friends.
pub fn type_arg(s: &Sexp) -> Result<Type> {
    parse_type(s)
}

/// Box of a parsed operand.
pub fn operand(s: &Sexp) -> Result<Box<Expr>> {
    parse_expr(s).map(Box::new)
}
