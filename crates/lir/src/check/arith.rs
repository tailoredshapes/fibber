//! Arithmetic, comparisons, conversions and select (spec/lir.md §6.1 to
//! §6.3).

use super::fcx::Fcx;
use crate::ast::{BinOp, CastOp, Expr, Kind, UnOp};
use crate::diag::{err, Pos, Result};
use crate::types::{Type, I1};

fn bin_name(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "add",
        Sub => "sub",
        Mul => "mul",
        SDiv => "sdiv",
        UDiv => "udiv",
        SRem => "srem",
        URem => "urem",
        FAdd => "fadd",
        FSub => "fsub",
        FMul => "fmul",
        FDiv => "fdiv",
        FRem => "frem",
        And => "and",
        Or => "or",
        Xor => "xor",
        Shl => "shl",
        LShr => "lshr",
        AShr => "ashr",
    }
}

/// Every literal lane of `e` as an unsigned value of its width.
fn literal_lanes(e: &Expr) -> Vec<u128> {
    let lane = |x: &Expr| match (&x.kind, x.int_literal()) {
        (Kind::Int(Type::Int(b), _), Some(v)) => Some((v as u128) & mask(*b)),
        _ => None,
    };
    match &e.kind {
        Kind::Vector(_, es) => es.iter().filter_map(lane).collect(),
        _ => lane(e).into_iter().collect(),
    }
}

pub fn mask(bits: u32) -> u128 {
    if bits >= 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    }
}

impl<'a> Fcx<'a> {
    pub fn bin(&mut self, op: BinOp, a: &'a Expr, b: &'a Expr, p: Pos) -> Result<Type> {
        use BinOp::*;
        let name = bin_name(op);
        let ta = self.val(a)?;
        let float = matches!(op, FAdd | FSub | FMul | FDiv | FRem);
        if float && !ta.is_float_like() {
            return err(p, format!("{name} needs float operands, found {ta}"));
        }
        if !float && !ta.is_int_like() {
            return err(p, format!("{name} needs integer operands, found {ta}"));
        }
        let tb = self.val(b)?;
        self.same(name, 2, &tb, &ta, p)?;
        let lanes = literal_lanes(b);
        if matches!(op, SDiv | UDiv | SRem | URem) && lanes.contains(&0) {
            return err(p, "division by constant zero");
        }
        let width = u128::from(ta.scalar().bits().unwrap_or(0));
        if matches!(op, Shl | LShr | AShr) {
            if let Some(n) = lanes.iter().find(|&&n| n >= width) {
                return err(
                    p,
                    format!("shift amount {n} is not less than the width {width}"),
                );
            }
        }
        Ok(ta)
    }

    pub fn un(&mut self, op: UnOp, a: &'a Expr, p: Pos) -> Result<Type> {
        let t = self.val(a)?;
        match op {
            UnOp::FNeg if !t.is_float_like() => {
                err(p, format!("fneg needs float operands, found {t}"))
            }
            UnOp::Ctpop if !t.is_int_like() => {
                err(p, format!("ctpop needs integer operands, found {t}"))
            }
            _ => Ok(t),
        }
    }

    pub fn cmp(&mut self, name: &str, a: &'a Expr, b: &'a Expr, p: Pos) -> Result<Type> {
        let ta = self.val(a)?;
        let ok = if name == "icmp" {
            ta.is_int_like() || *ta.scalar() == Type::Ptr
        } else {
            ta.is_float_like()
        };
        if !ok {
            let what = if name == "icmp" {
                "integer or ptr"
            } else {
                "float"
            };
            return err(p, format!("{name} needs {what} operands, found {ta}"));
        }
        let tb = self.val(b)?;
        self.same(name, 2, &tb, &ta, p)?;
        Ok(ta.bool_shape())
    }

    pub fn select(&mut self, c: &'a Expr, a: &'a Expr, b: &'a Expr, p: Pos) -> Result<Type> {
        let tc = self.val(c)?;
        let ta = self.val(a)?;
        let tb = self.val(b)?;
        match (&tc, ta.lanes()) {
            (t, _) if *t == I1 => {}
            (Type::Vector(n, e), Some(m)) if **e == I1 && *n == m => {}
            (Type::Vector(_, e), _) if **e == I1 => {
                return err(p, format!("select condition {tc} does not match {ta}"))
            }
            _ => return err(p, format!("select condition must be i1, found {tc}")),
        }
        self.same("select", 3, &tb, &ta, p)?;
        Ok(ta)
    }

    pub fn cast(&mut self, op: CastOp, t: &Type, v: &'a Expr, p: Pos) -> Result<Type> {
        self.valid(t, p)?;
        let tv = self.val(v)?;
        cast_rule(op, t, &tv, p)?;
        Ok(t.clone())
    }
}

fn cast_name(op: CastOp) -> &'static str {
    use CastOp::*;
    match op {
        Trunc => "trunc",
        ZExt => "zext",
        SExt => "sext",
        FpTrunc => "fptrunc",
        FpExt => "fpext",
        FpToSi => "fptosi",
        FpToUi => "fptoui",
        SiToFp => "sitofp",
        UiToFp => "uitofp",
        PtrToInt => "ptrtoint",
        IntToPtr => "inttoptr",
        Bitcast => "bitcast",
    }
}

fn need(ok: bool, p: Pos, msg: String) -> Result<()> {
    if ok {
        Ok(())
    } else {
        err(p, msg)
    }
}

fn cast_rule(op: CastOp, t: &Type, v: &Type, p: Pos) -> Result<()> {
    use CastOp::*;
    let n = cast_name(op);
    let bits = |x: &Type| x.bits().unwrap_or(0);
    match op {
        Trunc | ZExt | SExt => {
            let bad = if !t.is_int() { t } else { v };
            need(
                t.is_int() && v.is_int(),
                p,
                format!("{n} needs integer types, found {bad}"),
            )?;
            if op == Trunc {
                need(
                    bits(t) < bits(v),
                    p,
                    format!("{n}: {t} is not narrower than {v}"),
                )
            } else {
                need(
                    bits(t) > bits(v),
                    p,
                    format!("{n}: {t} is not wider than {v}"),
                )
            }
        }
        FpTrunc | FpExt => {
            let bad = if !t.is_float() { t } else { v };
            need(
                t.is_float() && v.is_float(),
                p,
                format!("{n} needs float types, found {bad}"),
            )?;
            if op == FpTrunc {
                need(
                    bits(t) < bits(v),
                    p,
                    format!("{n}: {t} is not narrower than {v}"),
                )
            } else {
                need(
                    bits(t) > bits(v),
                    p,
                    format!("{n}: {t} is not wider than {v}"),
                )
            }
        }
        FpToSi | FpToUi => {
            need(
                t.is_int(),
                p,
                format!("{n} needs an integer result type, found {t}"),
            )?;
            need(
                v.is_float(),
                p,
                format!("{n} needs a float operand, found {v}"),
            )
        }
        SiToFp | UiToFp => {
            need(
                t.is_float(),
                p,
                format!("{n} needs a float result type, found {t}"),
            )?;
            need(
                v.is_int(),
                p,
                format!("{n} needs an integer operand, found {v}"),
            )
        }
        PtrToInt => {
            need(
                t.is_int(),
                p,
                format!("{n} needs an integer result type, found {t}"),
            )?;
            need(
                *v == Type::Ptr,
                p,
                format!("{n} needs a ptr operand, found {v}"),
            )
        }
        IntToPtr => {
            need(
                *t == Type::Ptr,
                p,
                format!("{n} needs a ptr result type, found {t}"),
            )?;
            need(
                v.is_int(),
                p,
                format!("{n} needs an integer operand, found {v}"),
            )
        }
        Bitcast => bitcast_rule(t, v, p),
    }
}

fn bitcast_rule(t: &Type, v: &Type, p: Pos) -> Result<()> {
    for x in [t, v] {
        if *x.scalar() == Type::Ptr {
            return err(p, "bitcast cannot convert ptr");
        }
        if x.is_aggregate() {
            return err(p, format!("bitcast cannot convert {x}"));
        }
    }
    need(
        t.bits() == v.bits(),
        p,
        format!("bitcast: {v} and {t} differ in size"),
    )
}
