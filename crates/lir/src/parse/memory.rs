//! Memory forms: `alloca`, `load`, `store` with their `volatile` and
//! `(align N)` options, and `getelementptr` (spec/lir.md §6.5).

use super::expr::{exprs, operand, type_arg};
use super::literal::int_token;
use crate::ast::{Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;

/// The largest alignment lIR accepts, 2^30.
const MAX_ALIGN: i128 = 1 << 30;

/// `alloca`, `load`, `store`, `getelementptr`.
pub fn memory(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let kind = match name {
        "alloca" => {
            let (align, rest) = align_option(args)?;
            if rest.is_empty() || rest.len() > 2 {
                return err(
                    pos,
                    format!("alloca expects 1 or 2 operands, found {}", rest.len()),
                );
            }
            Kind::Alloca {
                ty: type_arg(&rest[0])?,
                count: rest.get(1).map(operand).transpose()?,
                align,
            }
        }
        "load" => {
            let (volatile, align, rest) = access_options(args)?;
            super::arity(name, rest, 2, pos)?;
            Kind::Load {
                ty: type_arg(&rest[0])?,
                ptr: operand(&rest[1])?,
                volatile,
                align,
            }
        }
        "store" => {
            let (volatile, align, rest) = access_options(args)?;
            super::arity(name, rest, 2, pos)?;
            Kind::Store {
                value: operand(&rest[0])?,
                ptr: operand(&rest[1])?,
                volatile,
                align,
            }
        }
        _ => return gep(args, pos),
    };
    Ok(Expr::new(kind, pos))
}

/// A leading `volatile`, then an optional `(align N)`.
fn access_options(args: &[Sexp]) -> Result<(bool, Option<u32>, &[Sexp])> {
    let volatile = args.first().and_then(Sexp::atom) == Some("volatile");
    let rest = if volatile { &args[1..] } else { args };
    let (align, rest) = align_option(rest)?;
    Ok((volatile, align, rest))
}

/// An optional leading `(align N)`: `N` a power of two up to 2^30.
fn align_option(args: &[Sexp]) -> Result<(Option<u32>, &[Sexp])> {
    let Some(Sexp::List(items, p)) = args.first() else {
        return Ok((None, args));
    };
    if items.first().and_then(Sexp::atom) != Some("align") {
        return Ok((None, args));
    }
    let [_, n] = items.as_slice() else {
        return err(*p, "align must be (align N)");
    };
    let value = n.atom().and_then(int_token);
    match value {
        Some(v) if (1..=MAX_ALIGN).contains(&v) && (v & (v - 1)) == 0 => {
            Ok((Some(v as u32), &args[1..]))
        }
        Some(v) => err(
            *p,
            format!("align must be a power of two from 1 to 2^30, found {v}"),
        ),
        None => err(*p, "align must be (align N)"),
    }
}

fn gep(args: &[Sexp], pos: Pos) -> Result<Expr> {
    let inbounds = args.first().and_then(Sexp::atom) == Some("inbounds");
    let rest = if inbounds { &args[1..] } else { args };
    if rest.len() < 3 {
        return err(
            pos,
            "getelementptr needs a type, a pointer and at least one index",
        );
    }
    Ok(Expr::new(
        Kind::Gep {
            inbounds,
            ty: type_arg(&rest[0])?,
            ptr: operand(&rest[1])?,
            indices: exprs(&rest[2..])?,
        },
        pos,
    ))
}
