//! Memory forms, control flow, calls, phi and let.

use super::expr::{exprs, operand, parse_expr, type_arg};
use super::ty::parse_fn_type;
use super::{arity, atom, label, valid_name};
use crate::ast::{Binding, Callee, Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;

/// Parse `name` if it is a control-flow, call, phi or let form.
pub fn parse(name: &str, args: &[Sexp], pos: Pos) -> Option<Result<Expr>> {
    let r = match name {
        "ret" => ret(args, pos),
        "br" => br(args, pos),
        "switch" => switch(args, pos),
        "unreachable" => arity(name, args, 0, pos).map(|_| Expr::new(Kind::Unreachable, pos)),
        "phi" => phi(args, pos),
        "let" => let_form(args, pos),
        "call" | "tailcall" => direct_call(name, args, pos),
        "indirect-call" | "indirect-tailcall" => indirect_call(name, args, pos),
        _ => return None,
    };
    Some(r)
}

/// `alloca`, `load`, `store`, `getelementptr`.
pub fn memory(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let kind = match name {
        "alloca" => {
            if args.is_empty() || args.len() > 2 {
                return err(
                    pos,
                    format!("alloca expects 1 or 2 operands, found {}", args.len()),
                );
            }
            let count = args.get(1).map(operand).transpose()?;
            Kind::Alloca(type_arg(&args[0])?, count)
        }
        "load" => {
            arity(name, args, 2, pos)?;
            Kind::Load(type_arg(&args[0])?, operand(&args[1])?)
        }
        "store" => {
            arity(name, args, 2, pos)?;
            Kind::Store(operand(&args[0])?, operand(&args[1])?)
        }
        _ => return gep(args, pos),
    };
    Ok(Expr::new(kind, pos))
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

fn ret(args: &[Sexp], pos: Pos) -> Result<Expr> {
    match args {
        [] => Ok(Expr::new(Kind::Ret(None), pos)),
        [v] => Ok(Expr::new(Kind::Ret(Some(operand(v)?)), pos)),
        _ => err(
            pos,
            format!("ret expects at most 1 operand, found {}", args.len()),
        ),
    }
}

fn br(args: &[Sexp], pos: Pos) -> Result<Expr> {
    match args {
        [l] => Ok(Expr::new(Kind::Br(label(l)?), pos)),
        [c, t, f] => Ok(Expr::new(
            Kind::CondBr(operand(c)?, label(t)?, label(f)?),
            pos,
        )),
        _ => err(
            pos,
            format!("br expects 1 or 3 operands, found {}", args.len()),
        ),
    }
}

fn switch(args: &[Sexp], pos: Pos) -> Result<Expr> {
    if args.len() < 2 {
        return err(pos, "switch expects a value, a default label and cases");
    }
    let mut cases = Vec::new();
    for c in &args[2..] {
        match c {
            Sexp::List(kv, _) if kv.len() == 2 => cases.push((parse_expr(&kv[0])?, label(&kv[1])?)),
            other => return err(other.pos(), "a switch case must be ((iK c) LABEL)"),
        }
    }
    Ok(Expr::new(
        Kind::Switch(operand(&args[0])?, label(&args[1])?, cases),
        pos,
    ))
}

fn phi(args: &[Sexp], pos: Pos) -> Result<Expr> {
    if args.len() < 2 {
        return err(pos, "phi needs a type and at least one incoming value");
    }
    let mut incoming = Vec::new();
    for s in &args[1..] {
        match s {
            Sexp::List(lv, p) if lv.len() == 2 => incoming.push(Binding {
                name: label(&lv[0])?,
                value: parse_expr(&lv[1])?,
                pos: *p,
            }),
            other => return err(other.pos(), "a phi entry must be (LABEL value)"),
        }
    }
    Ok(Expr::new(Kind::Phi(type_arg(&args[0])?, incoming), pos))
}

fn let_form(args: &[Sexp], pos: Pos) -> Result<Expr> {
    let Some(Sexp::List(binds, _)) = args.first() else {
        return err(pos, "let binding must be (NAME value)");
    };
    let mut out = Vec::new();
    for b in binds {
        match b {
            Sexp::List(nv, p) if nv.len() == 2 && nv[0].atom().is_some() => out.push(Binding {
                name: valid_name(atom(&nv[0], "a name")?, *p)?,
                value: parse_expr(&nv[1])?,
                pos: *p,
            }),
            other => return err(other.pos(), "let binding must be (NAME value)"),
        }
    }
    if args.len() < 2 {
        return err(pos, "let without a body");
    }
    Ok(Expr::new(Kind::Let(out, exprs(&args[1..])?), pos))
}

fn direct_call(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let Some(head) = args.first() else {
        return err(pos, format!("{name} needs a function name @f"));
    };
    let f = match head.atom().and_then(|a| a.strip_prefix('@')) {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => {
            return err(
                head.pos(),
                format!("{name} needs a function name @f, found {}", head.describe()),
            )
        }
    };
    Ok(Expr::new(
        Kind::Call {
            callee: Callee::Direct(f),
            args: exprs(&args[1..])?,
            tail: name == "tailcall",
        },
        pos,
    ))
}

fn indirect_call(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    if args.len() < 2 {
        return err(pos, format!("{name} needs a callee and a function type"));
    }
    let ty = parse_fn_type(&args[1], name)?;
    Ok(Expr::new(
        Kind::Call {
            callee: Callee::Indirect(operand(&args[0])?, ty),
            args: exprs(&args[2..])?,
            tail: name == "indirect-tailcall",
        },
        pos,
    ))
}
