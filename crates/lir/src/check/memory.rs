//! Memory and atomic instructions (spec/lir.md §6.5, §6.6).

use super::env::Symbol;
use super::fcx::Fcx;
use crate::ast::{Expr, Kind, Ordering, RmwOp};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

fn ord_name(o: Ordering) -> &'static str {
    match o {
        Ordering::Unordered => "unordered",
        Ordering::Monotonic => "monotonic",
        Ordering::Acquire => "acquire",
        Ordering::Release => "release",
        Ordering::AcqRel => "acq_rel",
        Ordering::SeqCst => "seq_cst",
    }
}

fn atomic_type(t: &Type, p: Pos) -> Result<()> {
    match t {
        Type::Int(8 | 16 | 32 | 64) | Type::Ptr | Type::Float | Type::Double => Ok(()),
        _ => err(p, format!("atomic operation on {t}")),
    }
}

fn forbid(op: &str, o: Ordering, bad: &[Ordering], p: Pos) -> Result<()> {
    if bad.contains(&o) {
        return err(p, format!("{op} cannot have ordering {}", ord_name(o)));
    }
    Ok(())
}

impl<'a> Fcx<'a> {
    fn pointer(&mut self, op: &str, e: &'a Expr, p: Pos) -> Result<()> {
        let t = self.val(e)?;
        if t != Type::Ptr {
            return err(p, format!("{op} needs a ptr operand, found {t}"));
        }
        Ok(())
    }

    /// The typed direct-access rule (spec/lir.md §6.5).
    fn direct(&self, op: &str, ty: &Type, ptr: &Expr, p: Pos) -> Result<()> {
        match &ptr.kind {
            Kind::Global(g) => match self.env.symbols.get(g) {
                Some(Symbol::Var { ty: gt, .. }) if gt != ty => err(
                    p,
                    format!("{op} of {ty} through @{g}, a global of type {gt}"),
                ),
                _ => Ok(()),
            },
            Kind::Local(n) => match self.allocas.get(n) {
                Some(at) if at != ty => err(
                    p,
                    format!("{op} of {ty} through {n}, which is (alloca {at})"),
                ),
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    pub fn memory(&mut self, e: &'a Expr) -> Result<Option<Type>> {
        let p = e.pos;
        match &e.kind {
            Kind::Alloca(t, count) => {
                self.valid(t, p)?;
                if let Some(c) = count {
                    let tc = self.val(c)?;
                    if !tc.is_int() {
                        return err(p, format!("alloca count must be an integer, found {tc}"));
                    }
                }
                Ok(Some(Type::Ptr))
            }
            Kind::Load(t, ptr) => {
                self.valid(t, p)?;
                self.pointer("load", ptr, p)?;
                self.direct("load", t, ptr, p)?;
                Ok(Some(t.clone()))
            }
            Kind::Store(v, ptr) => self.store(v, ptr, p).map(|_| None),
            Kind::Gep {
                ty, ptr, indices, ..
            } => self.gep(ty, ptr, indices, p).map(Some),
            _ => self.atomic(e),
        }
    }

    fn store(&mut self, v: &'a Expr, ptr: &'a Expr, p: Pos) -> Result<()> {
        let tv = self.val(v)?;
        self.pointer("store", ptr, p)?;
        if let Kind::Global(g) = &ptr.kind {
            if let Some(Symbol::Var { constant: true, .. }) = self.env.symbols.get(g) {
                return err(p, format!("store to constant @{g}"));
            }
        }
        self.direct("store", &tv, ptr, p)
    }

    fn gep(&mut self, ty: &Type, ptr: &'a Expr, idx: &'a [Expr], p: Pos) -> Result<Type> {
        self.valid(ty, p)?;
        self.pointer("getelementptr", ptr, p)?;
        let mut cur = ty.clone();
        for (k, i) in idx.iter().enumerate() {
            let ti = self.val(i)?;
            if !ti.is_int() {
                return err(
                    p,
                    format!("getelementptr index must be an integer, found {ti}"),
                );
            }
            if k == 0 {
                continue;
            }
            let Some(fields) = self.env.fields(&cur) else {
                return err(p, format!("getelementptr cannot index into {cur}"));
            };
            let f = match (&i.kind, &ti) {
                (Kind::Int(..), Type::Int(32)) => i.int_literal().unwrap_or(-1),
                _ => {
                    return err(
                        p,
                        "getelementptr: struct field index must be a constant i32",
                    )
                }
            };
            if f < 0 || f as usize >= fields.len() {
                let f = (f as u128) & 0xffff_ffff;
                return err(
                    p,
                    format!("getelementptr: field index {f} out of range for {cur}"),
                );
            }
            cur = fields[f as usize].clone();
        }
        Ok(Type::Ptr)
    }

    fn atomic(&mut self, e: &'a Expr) -> Result<Option<Type>> {
        use Ordering::*;
        let p = e.pos;
        match &e.kind {
            Kind::AtomicLoad(_, o, t, ptr) => {
                forbid("atomic-load", *o, &[Release, AcqRel], p)?;
                self.valid(t, p)?;
                atomic_type(t, p)?;
                self.pointer("atomic-load", ptr, p)?;
                self.direct("atomic-load", t, ptr, p)?;
                Ok(Some(t.clone()))
            }
            Kind::AtomicStore(_, o, v, ptr) => {
                forbid("atomic-store", *o, &[Acquire, AcqRel], p)?;
                let tv = self.val(v)?;
                atomic_type(&tv, p)?;
                self.pointer("atomic-store", ptr, p)?;
                self.direct("atomic-store", &tv, ptr, p)?;
                Ok(None)
            }
            Kind::AtomicRmw(op, _, o, ptr, v) => self.rmw(*op, *o, ptr, v, p).map(Some),
            Kind::CmpXchg { .. } => self.cmpxchg(e).map(Some),
            Kind::Fence(_, o) => match o {
                Acquire | Release | AcqRel | SeqCst => Ok(None),
                _ => err(
                    p,
                    format!(
                        "fence ordering must be acquire, release, acq_rel or seq_cst, found {}",
                        ord_name(*o)
                    ),
                ),
            },
            _ => err(p, "unexpected form"),
        }
    }

    fn rmw(&mut self, op: RmwOp, o: Ordering, ptr: &'a Expr, v: &'a Expr, p: Pos) -> Result<Type> {
        forbid("atomicrmw", o, &[Ordering::Unordered], p)?;
        self.pointer("atomicrmw", ptr, p)?;
        let tv = self.val(v)?;
        let name = format!("{op:?}").to_lowercase();
        match op {
            RmwOp::Xchg => atomic_type(&tv, p)?,
            RmwOp::FAdd | RmwOp::FSub | RmwOp::FMax | RmwOp::FMin => {
                if !tv.is_float() {
                    return err(
                        p,
                        format!("atomicrmw {name} needs a float operand, found {tv}"),
                    );
                }
            }
            _ => {
                if !tv.is_int() {
                    return err(
                        p,
                        format!("atomicrmw {name} needs an integer operand, found {tv}"),
                    );
                }
                atomic_type(&tv, p)?;
            }
        }
        self.direct("atomicrmw", &tv, ptr, p)?;
        Ok(tv)
    }

    fn cmpxchg(&mut self, e: &'a Expr) -> Result<Type> {
        let p = e.pos;
        let Kind::CmpXchg {
            success,
            failure,
            ptr,
            expected,
            new,
            ..
        } = &e.kind
        else {
            return err(p, "unexpected form");
        };
        forbid("cmpxchg", *success, &[Ordering::Unordered], p)?;
        forbid("cmpxchg", *failure, &[Ordering::Unordered], p)?;
        if matches!(failure, Ordering::Release | Ordering::AcqRel) {
            return err(
                p,
                format!("cmpxchg failure ordering cannot be {}", ord_name(*failure)),
            );
        }
        self.pointer("cmpxchg", ptr, p)?;
        let t = self.val(expected)?;
        if !(t.is_int() || t == Type::Ptr) {
            return err(
                p,
                format!("cmpxchg needs an integer or ptr operand, found {t}"),
            );
        }
        atomic_type(&t, p)?;
        let tn = self.val(new)?;
        self.same("cmpxchg", 3, &tn, &t, p)?;
        self.direct("cmpxchg", &t, ptr, p)?;
        Ok(Type::Anon(vec![t, Type::Int(1)]))
    }
}
