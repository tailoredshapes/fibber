//! Arithmetic, comparisons and conversions (types §8.12): the native
//! instances of `Num`, `Eq`, `Ord` and `Bits` on scalars and strings,
//! with every check §8.12 requires, and the conversion primitives.

use fibref::types::ast::{ConvOp, IntConv};
use fibref::types::ty::{Con, Scalar, Ty};

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::{LirTy, V};

impl<'a> Cx<'_, 'a> {
    /// A method of a native instance.
    pub fn native_method(
        &mut self,
        proto: &str,
        method: &str,
        a: &[V],
        tys: &[Ty],
        rt: &Ty,
    ) -> R<V> {
        let x = a
            .first()
            .ok_or_else(|| Unsupported(format!("{method} without a receiver")))?
            .clone();
        let y = a.get(1).cloned();
        match proto {
            "Deref" => self.deref_val(&x, &tys[0]),
            "Show" => self.native_show(&x, &tys[0]),
            "Hash" => self.native_hash(&x, &tys[0]),
            _ => {
                if matches!(tys[0], Ty::Con(Con::Str, _)) {
                    return self.str_compare(method, &x, y.as_ref());
                }
                let t = x
                    .ty()
                    .ok_or_else(|| Unsupported(format!("{method} on unit")))?;
                let _ = rt;
                match (t, y) {
                    (LirTy::Float | LirTy::Double, Some(y)) => {
                        Ok(self.float_binary(method, &x, &y))
                    }
                    (LirTy::Float | LirTy::Double, None) => {
                        Ok(self.b.val(&format!("(fneg {})", x.text()), t))
                    }
                    (_, Some(y)) => self.int_binary(method, &x, &y),
                    (_, None) => self.int_unary(method, &x),
                }
            }
        }
    }

    fn str_compare(&mut self, method: &str, x: &V, y: Option<&V>) -> R<V> {
        let y =
            y.ok_or_else(|| Unsupported("a string comparison without its second operand".into()))?;
        let c = self.b.val(
            &format!("(call @fib.str-cmp {} {})", x.text(), y.text()),
            LirTy::I32,
        );
        let pred = compare_pred(method)?;
        Ok(self
            .b
            .val(&format!("(icmp {pred} {} (i32 0))", c.text()), LirTy::I1))
    }

    fn float_binary(&mut self, method: &str, x: &V, y: &V) -> V {
        let t = x.ty().unwrap_or(LirTy::Double);
        let (a, b) = (x.text(), y.text());
        let (instr, rt) = match method {
            "+" => (format!("(fadd {a} {b})"), t),
            "-" => (format!("(fsub {a} {b})"), t),
            "*" => (format!("(fmul {a} {b})"), t),
            "/" => (format!("(fdiv {a} {b})"), t),
            "rem" => (format!("(frem {a} {b})"), t),
            "=" => (format!("(fcmp oeq {a} {b})"), LirTy::I1),
            "!=" => (format!("(fcmp une {a} {b})"), LirTy::I1),
            "<" => (format!("(fcmp olt {a} {b})"), LirTy::I1),
            "<=" => (format!("(fcmp ole {a} {b})"), LirTy::I1),
            ">" => (format!("(fcmp ogt {a} {b})"), LirTy::I1),
            _ => (format!("(fcmp oge {a} {b})"), LirTy::I1),
        };
        self.b.val(&instr, rt)
    }

    fn int_binary(&mut self, method: &str, x: &V, y: &V) -> R<V> {
        let t = x.ty().unwrap_or(LirTy::I64);
        let (a, b) = (x.text().to_string(), y.text().to_string());
        let w = t.text();
        Ok(match method {
            "+" | "-" | "*" => {
                let op = match method {
                    "+" => "sadd-overflow",
                    "-" => "ssub-overflow",
                    _ => "smul-overflow",
                };
                let r = self.b.val(&format!("({op} {a} {b})"), t);
                let ovf = self
                    .b
                    .val(&format!("(extractvalue {} 1)", r.text()), LirTy::I1);
                self.trap_if(&ovf, &format!("integer overflow in {method} at {w}"));
                self.b.val(&format!("(extractvalue {} 0)", r.text()), t)
            }
            "/" | "rem" => {
                let zero = self.b.val(&format!("(icmp eq {b} ({w} 0))",), LirTy::I1);
                self.trap_if(&zero, &format!("integer {method} by zero"));
                let min = -(1i128 << (t.bits() - 1));
                let amin = self.b.val(&format!("(icmp eq {a} ({w} {min}))"), LirTy::I1);
                let bm1 = self.b.val(&format!("(icmp eq {b} ({w} -1))"), LirTy::I1);
                let both = self
                    .b
                    .val(&format!("(and {} {})", amin.text(), bm1.text()), LirTy::I1);
                self.trap_if(&both, &format!("integer overflow in {method} at {w}"));
                let op = if method == "/" { "sdiv" } else { "srem" };
                self.b.val(&format!("({op} {a} {b})"), t)
            }
            "bit-and" => self.b.val(&format!("(and {a} {b})"), t),
            "bit-or" => self.b.val(&format!("(or {a} {b})"), t),
            "bit-xor" => self.b.val(&format!("(xor {a} {b})"), t),
            "shl" | "shr" | "sar" => {
                let m = self.b.val(&format!("(and {b} ({w} {}))", t.bits() - 1), t);
                let op = match method {
                    "shl" => "shl",
                    "shr" => "lshr",
                    _ => "ashr",
                };
                self.b.val(&format!("({op} {a} {})", m.text()), t)
            }
            other => {
                let pred = compare_pred(other)?;
                self.b.val(&format!("(icmp {pred} {a} {b})"), LirTy::I1)
            }
        })
    }

    fn int_unary(&mut self, method: &str, x: &V) -> R<V> {
        let t = x.ty().unwrap_or(LirTy::I64);
        let (a, w) = (x.text().to_string(), t.text());
        Ok(match method {
            "neg" => {
                let min = -(1i128 << (t.bits() - 1));
                let amin = self.b.val(&format!("(icmp eq {a} ({w} {min}))"), LirTy::I1);
                self.trap_if(&amin, &format!("integer overflow in neg at {w}"));
                self.b.val(&format!("(sub ({w} 0) {a})"), t)
            }
            "bit-not" => self.b.val(&format!("(xor {a} ({w} -1))"), t),
            "popcount" => self.b.val(&format!("(ctpop {a})"), t),
            other => return Err(Unsupported(format!("unary {other}"))),
        })
    }

    /// Traps with `msg` when `c` holds; continues in a fresh block.
    pub fn trap_if(&mut self, c: &V, msg: &str) {
        let (lt, lk) = (self.b.label("trap"), self.b.label("ok"));
        self.b.term(&format!("(br {} {lt} {lk})", c.text()));
        self.b.open(&lt);
        self.trap_c(msg);
        self.b.open(&lk);
    }

    /// A conversion primitive (§8.12).
    pub fn convert(&mut self, op: ConvOp, target: Scalar, v: &V) -> R<V> {
        let g = self.p.g();
        let to = crate::layout::lir_ty(g, &Ty::scalar(target))?
            .ok_or_else(|| Unsupported("conversion to unit".into()))?;
        let from = v
            .ty()
            .ok_or_else(|| Unsupported("conversion of unit".into()))?;
        let x = v.text();
        let instr = match op {
            ConvOp::IntToInt(IntConv::Trunc) => format!("(trunc {} {x})", to.text()),
            ConvOp::IntToInt(IntConv::Zext) => format!("(zext {} {x})", to.text()),
            ConvOp::IntToInt(IntConv::Sext) => format!("(sext {} {x})", to.text()),
            ConvOp::FloatToFloat if to == LirTy::Float => format!("(fptrunc float {x})"),
            ConvOp::FloatToFloat => format!("(fpext double {x})"),
            ConvOp::FloatToInt { signed: true } => format!("(fptosi-sat {} {x})", to.text()),
            ConvOp::FloatToInt { signed: false } => format!("(fptoui-sat {} {x})", to.text()),
            ConvOp::IntToFloat { signed: true } => format!("(sitofp {} {x})", to.text()),
            ConvOp::IntToFloat { signed: false } => format!("(uitofp {} {x})", to.text()),
        };
        if from == to && matches!(op, ConvOp::IntToInt(_) | ConvOp::FloatToFloat) {
            return Ok(v.clone());
        }
        Ok(self.b.val(&instr, to))
    }
}

fn compare_pred(method: &str) -> R<&'static str> {
    Ok(match method {
        "=" => "eq",
        "!=" => "ne",
        "<" => "slt",
        "<=" => "sle",
        ">" => "sgt",
        ">=" => "sge",
        other => return Err(Unsupported(format!("native method {other}"))),
    })
}
