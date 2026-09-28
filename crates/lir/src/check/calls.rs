//! Calls, tail calls and the other terminators (spec/lir.md §6.7, §7).

use std::collections::HashSet;

use super::arith::mask;
use super::fcx::Fcx;
use crate::ast::{Callee, Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::{show_ret, Cc, FnType, Type};

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// The C promotion a variadic argument of type `t` needs, if any.
fn promoted(t: &Type) -> Option<&'static str> {
    match t {
        Type::Int(1 | 8 | 16) => Some("i32"),
        Type::Float => Some("double"),
        _ => None,
    }
}

impl<'a> Fcx<'a> {
    /// The callee's type and a name for messages.
    fn callee(&mut self, callee: &'a Callee, tail: bool, p: Pos) -> Result<(FnType, String)> {
        match callee {
            Callee::Direct(f) => {
                let ty = self.env.function(f, p)?.clone();
                let what = if tail { "tailcall to" } else { "call to" };
                Ok((ty, format!("{what} @{f}")))
            }
            Callee::Indirect(ptr, ty) => {
                let op = if tail {
                    "indirect-tailcall"
                } else {
                    "indirect-call"
                };
                let tp = self.val(ptr)?;
                if tp != Type::Ptr {
                    return err(p, format!("{op} needs a ptr callee, found {tp}"));
                }
                self.env.valid_fn(ty, p)?;
                Ok((ty.clone(), op.to_string()))
            }
        }
    }

    fn args(&mut self, ty: &FnType, what: &str, var: &str, args: &'a [Expr], p: Pos) -> Result<()> {
        let n = ty.params.len();
        let bad = if ty.varargs {
            args.len() < n
        } else {
            args.len() != n
        };
        if bad {
            let least = if ty.varargs { "at least " } else { "" };
            return err(
                p,
                format!(
                    "{what}: expected {least}{n} argument{}, found {}",
                    plural(n),
                    args.len()
                ),
            );
        }
        for (i, a) in args.iter().enumerate() {
            let t = self.val(a)?;
            match ty.params.get(i) {
                Some(want) if *want != t => {
                    return err(
                        a.pos,
                        format!("{what}: argument {} has type {t}, expected {want}", i + 1),
                    )
                }
                Some(_) => {}
                None => {
                    if let Some(to) = promoted(&t) {
                        return err(a.pos, format!(
                            "variadic argument {} of {var} has type {t}, which C promotes; pass {to}",
                            i + 1
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// A call that is not a tail call.
    pub fn call(&mut self, e: &'a Expr) -> Result<Option<Type>> {
        let Kind::Call { callee, args, tail } = &e.kind else {
            return err(e.pos, "unexpected form");
        };
        let (ty, what) = self.callee(callee, *tail, e.pos)?;
        let var = match callee {
            Callee::Direct(f) => format!("@{f}"),
            Callee::Indirect(..) => what.clone(),
        };
        self.args(&ty, &what, &var, args, e.pos)?;
        Ok(ty.ret)
    }

    /// A terminator in statement position.
    pub fn terminator(&mut self, e: &'a Expr) -> Result<()> {
        let p = e.pos;
        match &e.kind {
            Kind::Ret(v) => self.ret(v.as_deref(), p),
            Kind::Br(_) | Kind::Unreachable => Ok(()),
            Kind::CondBr(c, _, _) => match self.val(c)? {
                Type::Int(1) => Ok(()),
                t => err(p, format!("br condition must be i1, found {t}")),
            },
            Kind::Switch(v, _, cases) => self.switch(v, cases, p),
            Kind::Call { callee, .. } => {
                self.call(e)?;
                self.tail_rules(callee, p)
            }
            _ => err(p, "not a terminator"),
        }
    }

    fn ret(&mut self, v: Option<&'a Expr>, p: Pos) -> Result<()> {
        let f = self.f;
        match (v, &f.ty.ret) {
            (None, None) => Ok(()),
            (None, Some(r)) => err(
                p,
                format!("ret without a value in @{}, which returns {r}", f.name),
            ),
            (Some(v), want) => {
                let t = self.val(v)?;
                if Some(&t) != want.as_ref() {
                    return err(
                        p,
                        format!(
                            "ret type {t} does not match @{}'s result {}",
                            f.name,
                            show_ret(want)
                        ),
                    );
                }
                Ok(())
            }
        }
    }

    fn switch(&mut self, v: &'a Expr, cases: &'a [(Expr, String)], p: Pos) -> Result<()> {
        let t = self.val(v)?;
        let Type::Int(bits) = t else {
            return err(p, format!("switch needs an integer scalar, found {t}"));
        };
        let mut seen = HashSet::new();
        for (c, _) in cases {
            let (ct, val) = match &c.kind {
                Kind::Int(ct, val) => (ct, *val),
                _ => return err(c.pos, "switch case must be an integer literal"),
            };
            if *ct != t {
                return err(c.pos, format!("switch: case has type {ct}, expected {t}"));
            }
            if !seen.insert((val as u128) & mask(bits)) {
                return err(c.pos, format!("switch: duplicate case {val}"));
            }
        }
        Ok(())
    }

    /// The musttail rules of spec/lir.md §7.3.
    fn tail_rules(&mut self, callee: &'a Callee, p: Pos) -> Result<()> {
        let me = &self.f.ty;
        let fname = format!("@{}", self.f.name);
        let (ty, op, them) = match callee {
            Callee::Direct(g) => (
                self.env.function(g, p)?.clone(),
                "tailcall",
                format!("@{g}"),
            ),
            Callee::Indirect(_, t) => (t.clone(), "indirect-tailcall", "the callee".to_string()),
        };
        if me.cc != ty.cc {
            return err(
                p,
                format!("{op}: {fname} is {} and {them} is {}", me.cc, ty.cc),
            );
        }
        if me.ret != ty.ret {
            return err(
                p,
                format!(
                    "{op}: {them} returns {}, {fname} returns {}",
                    show_ret(&ty.ret),
                    show_ret(&me.ret)
                ),
            );
        }
        if ty.varargs {
            return err(p, format!("{op} to a variadic function"));
        }
        if me.cc == Cc::C && me.params != ty.params {
            let them_has = if them == "the callee" {
                "the callee has".into()
            } else {
                format!("{them} has")
            };
            return err(p, format!(
                "{op} under ccc needs identical parameter types ({fname} has {}, {them_has} {}); use tailcc",
                me.show_params(),
                ty.show_params()
            ));
        }
        self.result_in_registers(ty.ret.as_ref(), op, p)
    }

    /// Rule 5 of spec/lir.md §7.3: a result the target may return in
    /// memory (through a hidden pointer) cannot be tail-called, since
    /// the callee would write the caller's temporary; LLVM aborts on
    /// such a `musttail` ("failed to perform tail call elimination").
    /// Portably: at most two leaves, each a scalar or a vector of at
    /// most 512 bits.
    fn result_in_registers(&self, ret: Option<&Type>, op: &str, p: Pos) -> Result<()> {
        let Some(ret) = ret else {
            return Ok(());
        };
        let (leaves, widest) = self.leaves(ret);
        if leaves > 2 || widest > 512 {
            return err(p, format!(
                "{op}: a result of type {ret} may be returned in memory, which no tail call can: at most 2 scalar or vector leaves of at most 512 bits (found {leaves} leaves, the widest {widest} bits)"
            ));
        }
        Ok(())
    }

    /// How many scalars and vectors `t` holds, structs and arrays
    /// flattened, and the widest of them in bits (a pointer counts 64).
    fn leaves(&self, t: &Type) -> (u64, u64) {
        match t {
            Type::Array(n, e) => {
                let (count, widest) = self.leaves(e);
                (count.saturating_mul(*n), widest)
            }
            Type::Named(_) | Type::Anon(_) => self
                .env
                .fields(t)
                .unwrap_or_default()
                .iter()
                .map(|f| self.leaves(f))
                .fold((0, 0), |(c, w), (fc, fw)| (c.saturating_add(fc), w.max(fw))),
            leaf => (1, leaf.bits().unwrap_or(64)),
        }
    }
}
