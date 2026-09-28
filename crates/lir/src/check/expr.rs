//! Typing expressions: the dispatch over instruction groups.

use super::fcx::Fcx;
use crate::ast::{Expr, Kind};
use crate::diag::{err, Pos, Result};
use crate::types::Type;

/// Whether evaluating `e` emits an instruction of its own (a phi must
/// come before any, spec/lir.md §5.4).
fn emits(e: &Expr) -> bool {
    match &e.kind {
        Kind::Local(_)
        | Kind::Global(_)
        | Kind::Int(..)
        | Kind::Float(..)
        | Kind::Null
        | Kind::Vector(..)
        | Kind::Str(_)
        | Kind::Zero(_)
        | Kind::Let(..)
        | Kind::Phi(..) => false,
        Kind::Struct(_, fields) | Kind::Array(_, fields) => !fields.iter().all(is_constant),
        _ => true,
    }
}

/// A constant written in place: literals, `@name`, strings and structs
/// of constants (spec/lir.md §6.11, §9).
pub fn is_constant(e: &Expr) -> bool {
    match &e.kind {
        Kind::Int(..) | Kind::Float(..) | Kind::Null | Kind::Vector(..) | Kind::Str(_) => true,
        Kind::Global(_) | Kind::Zero(_) => true,
        Kind::Struct(_, fs) | Kind::Array(_, fs) => fs.iter().all(is_constant),
        _ => false,
    }
}

impl<'a> Fcx<'a> {
    /// The type of `e` (`None` for void).
    pub fn ty(&mut self, e: &'a Expr) -> Result<Option<Type>> {
        let t = self.ty_inner(e)?;
        if emits(e) {
            self.emitted = true;
        }
        Ok(t)
    }

    fn ty_inner(&mut self, e: &'a Expr) -> Result<Option<Type>> {
        let p = e.pos;
        let some = |t: Type| Ok(Some(t));
        match &e.kind {
            Kind::Local(n) => some(self.lookup(n, p)?),
            Kind::Global(g) => some(self.global_ref(g, p)?),
            Kind::Int(t, _) | Kind::Float(t, _) => some(t.clone()),
            Kind::Null | Kind::Str(_) => some(Type::Ptr),
            Kind::Vector(t, _) => some(t.clone()),
            Kind::Struct(name, fields) => some(self.struct_literal(name, fields, p)?),
            Kind::Array(t, elems) => some(self.array_literal(t, elems, p)?),
            Kind::Zero(t) => {
                self.valid(t, p)?;
                some(t.clone())
            }
            Kind::Bin(op, a, b) => some(self.bin(*op, a, b, p)?),
            Kind::Overflow(op, a, b) => some(self.overflow(*op, a, b, p)?),
            Kind::Trap => Ok(None),
            Kind::Un(op, a) => some(self.un(*op, a, p)?),
            Kind::ICmp(_, a, b) => some(self.cmp("icmp", a, b, p)?),
            Kind::FCmp(_, a, b) => some(self.cmp("fcmp", a, b, p)?),
            Kind::Cast(op, t, v) => some(self.cast(*op, t, v, p)?),
            Kind::Select(c, a, b) => some(self.select(c, a, b, p)?),
            Kind::ExtractElement(..) | Kind::InsertElement(..) | Kind::Shuffle(..) => {
                some(self.vector_op(e)?)
            }
            Kind::ExtractValue(a, idx) => some(self.extract_value(a, idx, p)?),
            Kind::InsertValue(a, v, idx) => some(self.insert_value(a, v, idx, p)?),
            Kind::Phi(t, inc) => some(self.phi(t, inc, p)?),
            Kind::Let(binds, body) => self.let_form(binds, body, true),
            Kind::Call { tail: false, .. } => self.call(e),
            k if k.is_terminator() => err(p, "terminator used as a value"),
            _ => self.memory(e),
        }
    }

    fn global_ref(&self, g: &str, p: Pos) -> Result<Type> {
        match self.env.symbols.get(g) {
            Some(_) => Ok(Type::Ptr),
            None => err(p, format!("undefined global @{g}")),
        }
    }

    /// `op: operand k has type X, expected Y` unless `got == want`.
    pub fn same(&self, op: &str, k: usize, got: &Type, want: &Type, p: Pos) -> Result<()> {
        if got == want {
            Ok(())
        } else {
            err(
                p,
                format!("{op}: operand {k} has type {got}, expected {want}"),
            )
        }
    }
}
