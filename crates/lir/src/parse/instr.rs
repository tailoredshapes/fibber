//! Arithmetic, comparisons, conversions, vectors, aggregates, memory.

use super::expr::{exprs, operand, type_arg};
use super::literal::int_token;
use super::{arity, atom};
use crate::ast::{BinOp, CastOp, Expr, FPred, IPred, Kind, OvfOp, UnOp};
use crate::diag::{err, Pos, Result};
use crate::sexp::Sexp;

/// Parse `name` if it is one of this module's instructions.
pub fn parse(name: &str, args: &[Sexp], pos: Pos) -> Option<Result<Expr>> {
    if let Some(op) = binop(name) {
        return Some(bin(name, op, args, pos));
    }
    if let Some(op) = castop(name) {
        return Some(cast(name, op, args, pos));
    }
    let r = match name {
        "fneg" => un(name, UnOp::FNeg, args, pos),
        "ctpop" => un(name, UnOp::Ctpop, args, pos),
        "sadd-overflow" => overflow(name, OvfOp::SAdd, args, pos),
        "ssub-overflow" => overflow(name, OvfOp::SSub, args, pos),
        "smul-overflow" => overflow(name, OvfOp::SMul, args, pos),
        "icmp" => icmp(args, pos),
        "fcmp" => fcmp(args, pos),
        "select" | "insertelement" | "shufflevector" | "extractelement" => {
            vector_op(name, args, pos)
        }
        "extractvalue" | "insertvalue" => aggregate(name, args, pos),
        "alloca" | "load" | "store" | "getelementptr" => super::memory::memory(name, args, pos),
        _ => return None,
    };
    Some(r)
}

fn binop(name: &str) -> Option<BinOp> {
    use BinOp::*;
    Some(match name {
        "add" => Add,
        "sub" => Sub,
        "mul" => Mul,
        "sdiv" => SDiv,
        "udiv" => UDiv,
        "srem" => SRem,
        "urem" => URem,
        "fadd" => FAdd,
        "fsub" => FSub,
        "fmul" => FMul,
        "fdiv" => FDiv,
        "frem" => FRem,
        "and" => And,
        "or" => Or,
        "xor" => Xor,
        "shl" => Shl,
        "lshr" => LShr,
        "ashr" => AShr,
        _ => return None,
    })
}

fn castop(name: &str) -> Option<CastOp> {
    use CastOp::*;
    Some(match name {
        "trunc" => Trunc,
        "zext" => ZExt,
        "sext" => SExt,
        "fptrunc" => FpTrunc,
        "fpext" => FpExt,
        "fptosi" => FpToSi,
        "fptoui" => FpToUi,
        "sitofp" => SiToFp,
        "uitofp" => UiToFp,
        "ptrtoint" => PtrToInt,
        "inttoptr" => IntToPtr,
        "bitcast" => Bitcast,
        "fptosi-sat" => FpToSiSat,
        "fptoui-sat" => FpToUiSat,
        _ => return None,
    })
}

fn bin(name: &str, op: BinOp, args: &[Sexp], pos: Pos) -> Result<Expr> {
    arity(name, args, 2, pos)?;
    Ok(Expr::new(
        Kind::Bin(op, operand(&args[0])?, operand(&args[1])?),
        pos,
    ))
}

fn overflow(name: &str, op: OvfOp, args: &[Sexp], pos: Pos) -> Result<Expr> {
    arity(name, args, 2, pos)?;
    Ok(Expr::new(
        Kind::Overflow(op, operand(&args[0])?, operand(&args[1])?),
        pos,
    ))
}

fn un(name: &str, op: UnOp, args: &[Sexp], pos: Pos) -> Result<Expr> {
    arity(name, args, 1, pos)?;
    Ok(Expr::new(Kind::Un(op, operand(&args[0])?), pos))
}

fn cast(name: &str, op: CastOp, args: &[Sexp], pos: Pos) -> Result<Expr> {
    arity(name, args, 2, pos)?;
    Ok(Expr::new(
        Kind::Cast(op, type_arg(&args[0])?, operand(&args[1])?),
        pos,
    ))
}

fn icmp(args: &[Sexp], pos: Pos) -> Result<Expr> {
    use IPred::*;
    arity("icmp", args, 3, pos)?;
    let p = atom(&args[0], "a predicate")?;
    let pred = match p {
        "eq" => Eq,
        "ne" => Ne,
        "slt" => Slt,
        "sle" => Sle,
        "sgt" => Sgt,
        "sge" => Sge,
        "ult" => Ult,
        "ule" => Ule,
        "ugt" => Ugt,
        "uge" => Uge,
        _ => return err(args[0].pos(), format!("unknown icmp predicate {p}")),
    };
    Ok(Expr::new(
        Kind::ICmp(pred, operand(&args[1])?, operand(&args[2])?),
        pos,
    ))
}

fn fcmp(args: &[Sexp], pos: Pos) -> Result<Expr> {
    use FPred::*;
    arity("fcmp", args, 3, pos)?;
    let p = atom(&args[0], "a predicate")?;
    let pred = match p {
        "oeq" => Oeq,
        "one" => One,
        "olt" => Olt,
        "ole" => Ole,
        "ogt" => Ogt,
        "oge" => Oge,
        "ord" => Ord,
        "ueq" => Ueq,
        "une" => Une,
        "ult" => Ult,
        "ule" => Ule,
        "ugt" => Ugt,
        "uge" => Uge,
        "uno" => Uno,
        _ => return err(args[0].pos(), format!("unknown fcmp predicate {p}")),
    };
    Ok(Expr::new(
        Kind::FCmp(pred, operand(&args[1])?, operand(&args[2])?),
        pos,
    ))
}

fn vector_op(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let n = if name == "extractelement" { 2 } else { 3 };
    arity(name, args, n, pos)?;
    let mut ops = exprs(args)?.into_iter().map(Box::new);
    // Cannot fail: the arity check above guarantees `n` operands.
    let mut next = || ops.next().expect("arity checked above");
    let kind = match name {
        "select" => Kind::Select(next(), next(), next()),
        "insertelement" => Kind::InsertElement(next(), next(), next()),
        "shufflevector" => Kind::Shuffle(next(), next(), next()),
        _ => Kind::ExtractElement(next(), next()),
    };
    Ok(Expr::new(kind, pos))
}

fn indices(args: &[Sexp]) -> Result<Vec<i128>> {
    args.iter()
        .map(|a| match a.atom().and_then(int_token) {
            Some(v) => Ok(v),
            None => err(a.pos(), "field index must be a number"),
        })
        .collect()
}

fn aggregate(name: &str, args: &[Sexp], pos: Pos) -> Result<Expr> {
    let fixed = if name == "extractvalue" { 1 } else { 2 };
    if args.len() <= fixed {
        return err(pos, format!("{name} needs at least one field index"));
    }
    let agg = operand(&args[0])?;
    let kind = if fixed == 1 {
        Kind::ExtractValue(agg, indices(&args[1..])?)
    } else {
        Kind::InsertValue(agg, operand(&args[1])?, indices(&args[2..])?)
    };
    Ok(Expr::new(kind, pos))
}
