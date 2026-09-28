//! Atomic instructions and fences (spec/lir.md §6.6).

use super::expr::{operand, type_arg};
use super::{arity, atom};
use crate::ast::{Expr, Kind, Ordering, RmwOp, Scope};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;

/// Parse `name` if it is an atomic instruction.
pub fn parse(name: &str, args: &[Sexp], pos: Pos) -> Option<Result<Expr>> {
    let r = match name {
        "atomic-load" | "atomic-store" | "fence" => load_store_fence(name, args, pos),
        "atomicrmw" => rmw(args, pos),
        "cmpxchg" => cmpxchg(args, pos),
        _ => return None,
    };
    Some(r)
}

pub fn ordering_word(s: &str) -> Option<Ordering> {
    use Ordering::*;
    Some(match s {
        "unordered" => Unordered,
        "monotonic" => Monotonic,
        "acquire" => Acquire,
        "release" => Release,
        "acq_rel" => AcqRel,
        "seq_cst" => SeqCst,
        _ => return None,
    })
}

fn ordering(s: &Sexp) -> Result<Ordering> {
    let a = atom(s, "an ordering")?;
    ordering_word(a).map_or_else(|| err(s.pos(), format!("unknown ordering {a}")), Ok)
}

/// An optional leading `word`: whether it was there and the rest.
fn flag<'a>(args: &'a [Sexp], word: &str) -> (bool, &'a [Sexp]) {
    match args.first().and_then(Sexp::atom) {
        Some(w) if w == word => (true, &args[1..]),
        _ => (false, args),
    }
}

fn scope(args: &[Sexp]) -> (Scope, &[Sexp]) {
    match flag(args, "singlethread") {
        (true, rest) => (Scope::SingleThread, rest),
        (false, rest) => (Scope::System, rest),
    }
}

fn load_store_fence(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let (sc, rest) = scope(args);
    let n = if name == "fence" { 1 } else { 3 };
    arity(name, rest, n, pos)?;
    let ord = ordering(&rest[0])?;
    let kind = match name {
        "fence" => Kind::Fence(sc, ord),
        "atomic-load" => Kind::AtomicLoad(sc, ord, type_arg(&rest[1])?, operand(&rest[2])?),
        _ => Kind::AtomicStore(sc, ord, operand(&rest[1])?, operand(&rest[2])?),
    };
    Ok(Expr::new(kind, pos))
}

fn rmw_op(s: &Sexp) -> Result<RmwOp> {
    use RmwOp::*;
    let a = atom(s, "an atomicrmw operation")?;
    Ok(match a {
        "xchg" => Xchg,
        "add" => Add,
        "sub" => Sub,
        "and" => And,
        "nand" => Nand,
        "or" => Or,
        "xor" => Xor,
        "max" => Max,
        "min" => Min,
        "umax" => UMax,
        "umin" => UMin,
        "fadd" => FAdd,
        "fsub" => FSub,
        "fmax" => FMax,
        "fmin" => FMin,
        _ => return err(s.pos(), format!("unknown atomicrmw operation {a}")),
    })
}

fn rmw(args: &[Sexp], pos: Pos) -> Result<Expr> {
    let Some(first) = args.first() else {
        return err(
            pos,
            "atomicrmw expects an operation, an ordering and 2 operands",
        );
    };
    let op = rmw_op(first)?;
    let (sc, rest) = scope(&args[1..]);
    arity("atomicrmw", rest, 3, pos)?;
    let ord = ordering(&rest[0])?;
    let kind = Kind::AtomicRmw(op, sc, ord, operand(&rest[1])?, operand(&rest[2])?);
    Ok(Expr::new(kind, pos))
}

/// The failure ordering LLVM derives from a success ordering.
pub fn default_failure(success: Ordering) -> Ordering {
    match success {
        Ordering::AcqRel => Ordering::Acquire,
        Ordering::Release => Ordering::Monotonic,
        o => o,
    }
}

fn cmpxchg(args: &[Sexp], pos: Pos) -> Result<Expr> {
    let (weak, rest) = flag(args, "weak");
    let (sc, rest) = scope(rest);
    let Some(first) = rest.first() else {
        return err(pos, "cmpxchg expects an ordering and 3 operands");
    };
    let success = ordering(first)?;
    let rest = &rest[1..];
    let (failure, rest) = match rest.first().and_then(Sexp::atom).and_then(ordering_word) {
        Some(f) => (f, &rest[1..]),
        None => (default_failure(success), rest),
    };
    arity("cmpxchg", rest, 3, pos)?;
    Ok(Expr::new(
        Kind::CmpXchg {
            weak,
            scope: sc,
            success,
            failure,
            ptr: operand(&rest[0])?,
            expected: operand(&rest[1])?,
            new: operand(&rest[2])?,
        },
        pos,
    ))
}
