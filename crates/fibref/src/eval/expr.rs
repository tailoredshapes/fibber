//! Evaluation of each core form (syntax §2, §3), following the plan:
//! after every expression that produced a value, its `after` list runs
//! (own/program.rs). A tail call or a `recur` is a [`Flow`] that jumps:
//! nothing of the forms it passes through runs afterwards, the plan
//! having put their releases into the jump.

use crate::syntax::{FltWidth, IntWidth};
use crate::types::ast::{Clause, Expr, ExprKind, GlobalRef, Lit, Pattern};
use crate::types::ty::{Con, Scalar, Ty};

use super::call::Jump;
use super::error::{RunError, R};
use super::interp::Interp;
use super::value::Val;

/// What evaluating an expression did.
#[derive(Debug)]
pub enum Flow {
    /// It produced a value.
    Val(Val),
    /// A tail call: the frame's jump has run; the callee's result is
    /// the frame's.
    Tail(Box<Jump>),
    /// A `recur` with the new values of the loop variables; its jump has
    /// run.
    Recur(Vec<Val>),
}

impl<'p> Interp<'p> {
    /// Evaluates `e` where its value is needed (never a jump).
    pub fn val(&mut self, e: &'p Expr) -> R<Val> {
        match self.flow(e)? {
            Flow::Val(v) => Ok(v),
            _ => Err(RunError::internal("a jump where a value is needed").at(&e.pos)),
        }
    }

    /// Evaluates `e`; if it produced a value, records it and runs its
    /// `after`.
    pub fn flow(&mut self, e: &'p Expr) -> R<Flow> {
        self.enter().map_err(|err| err.at(&e.pos))?;
        let f = self.form(e).map_err(|err| err.at(&e.pos))?;
        if let Flow::Val(v) = &f {
            self.record(e.id, v)?;
            self.after(e.id).map_err(|err| err.at(&e.pos))?;
        }
        Ok(f)
    }

    fn form(&mut self, e: &'p Expr) -> R<Flow> {
        let v = match &e.kind {
            ExprKind::Lit(l) => self.literal(e, l)?,
            ExprKind::Local(b) => self.local(*b)?,
            ExprKind::Global(g) => self.global(e, *g)?,
            ExprKind::Call(h, args) => return self.call(e, h, args),
            ExprKind::Fn(_) => self.make_fn(e)?,
            ExprKind::Let(bs, body) => return self.let_form(bs, body),
            ExprKind::If(c, t, f) => {
                let branch = if self.val(c)?.as_bool()? { t } else { f };
                return self.flow(branch);
            }
            ExprKind::Do(es) => return self.do_form(es),
            ExprKind::Match(s, cls) => return self.match_form(s, cls),
            ExprKind::Loop(vs, body) => return self.loop_form(vs, body),
            ExprKind::Recur(args) => return self.recur(e, args),
            ExprKind::Field(x, name, _) => self.field_of(x, name)?,
            ExprKind::Deref(p, _) => self.deref_place(e, p)?,
            ExprKind::Set(p, v) => self.set_place(p, v)?,
            ExprKind::SetField(b, f, v) => self.set_field(*b, f, v)?,
            ExprKind::Async(..) => self.make_async(e)?,
            ExprKind::Await(x) => self.await_task(x)?,
            ExprKind::Unsafe(b) | ExprKind::Dyn(_, _, _, b) => self.val(b)?,
            ExprKind::Quote(f) => self.quote(e, f)?,
            ExprKind::Convert(op, t, x) => {
                let v = self.val(x)?;
                super::arith::convert(*op, *t, &v)?
            }
            ExprKind::Concat(es) => self.concat(es)?,
            ExprKind::Guarded(_) | ExprKind::And(_) | ExprKind::Or(_) | ExprKind::Elided => {
                return Err(RunError::internal("an unelaborated form".to_string()))
            }
        };
        Ok(Flow::Val(v))
    }

    fn literal(&mut self, e: &Expr, l: &Lit) -> R<Val> {
        Ok(match l {
            Lit::Int(n, w) => match self.p.expr_types.get(&e.id) {
                // An integer literal that took a float type (stdlib §7 L19).
                Some(Ty::Con(Con::Scalar(s @ (Scalar::F32 | Scalar::F64)), _)) => {
                    Val::Float(*n as f64, *s)
                }
                _ => Val::Int(*n, int_scalar(*w)),
            },
            Lit::Float(x, FltWidth::F32) => Val::Float(*x, Scalar::F32),
            Lit::Float(x, FltWidth::F64) => Val::Float(*x, Scalar::F64),
            Lit::Str(s) => self.string_literal(e.id, s)?,
            Lit::Char(c) => Val::Char(*c),
            Lit::Bool(b) => Val::Bool(*b),
            Lit::Keyword(k) => Val::Kw(self.keyword(k)),
            Lit::Unit => Val::Unit,
        })
    }

    fn global(&mut self, e: &Expr, g: GlobalRef) -> R<Val> {
        match g {
            GlobalRef::Def(d) => self
                .defs
                .get(d.0 as usize)
                .cloned()
                .flatten()
                .ok_or_else(|| {
                    RunError::internal(format!("def {} read before it was evaluated", d.0))
                }),
            GlobalRef::Ctor(t, v) => self.ctor_value(e, t, v),
            GlobalRef::Method(p, i) => self.method_value(e.id, p, i),
            other => self.function_value(other),
        }
    }

    fn let_form(&mut self, bs: &'p [(Pattern, Expr)], body: &'p Expr) -> R<Flow> {
        for (pat, init) in bs {
            let v = self.val(init)?;
            if !self.bind_pattern(pat, &v)? {
                return Err(RunError::internal("a let pattern did not match").at(&pat.pos));
            }
        }
        self.flow(body)
    }

    fn do_form(&mut self, es: &'p [Expr]) -> R<Flow> {
        let Some((last, steps)) = es.split_last() else {
            return Ok(Flow::Val(Val::Unit));
        };
        for s in steps {
            self.val(s)?;
        }
        self.flow(last)
    }

    /// `match` (syntax §3.6): each clause's pattern, then its guard; a
    /// false guard runs the plan's `guard_fail` for it (the clause's rest
    /// vectors released) before the next clause is tried.
    fn match_form(&mut self, s: &'p Expr, cls: &'p [Clause]) -> R<Flow> {
        let v = self.val(s)?;
        for c in cls {
            if !self.bind_pattern(&c.pat, &v)? {
                continue;
            }
            if let Some(g) = &c.guard {
                if !self.val(g)?.as_bool()? {
                    let ops: &'p [crate::own::program::Op] = self
                        .plan()?
                        .own
                        .guard_fail
                        .get(&g.id)
                        .map_or(&[], Vec::as_slice);
                    self.run_ops(ops)?;
                    continue;
                }
            }
            return self.flow(&c.body);
        }
        Err(RunError::trap("no match clause matched"))
    }

    fn loop_form(
        &mut self,
        vs: &'p [(crate::types::ast::BindingId, Expr)],
        body: &'p Expr,
    ) -> R<Flow> {
        for (b, init) in vs {
            let v = self.val(init)?;
            self.bind(*b, v)?;
        }
        loop {
            match self.flow(body)? {
                Flow::Recur(vals) => {
                    for ((b, _), v) in vs.iter().zip(vals) {
                        self.bind(*b, v)?;
                    }
                    self.tick()?;
                }
                other => return Ok(other),
            }
        }
    }

    fn recur(&mut self, e: &Expr, args: &'p [Expr]) -> R<Flow> {
        let own: &'p crate::own::program::RecurOwn = self
            .plan()?
            .recurs
            .get(&e.id)
            .copied()
            .ok_or_else(|| RunError::gap("no plan for a recur"))?;
        let mut vals = Vec::with_capacity(args.len());
        for (i, x) in args.iter().enumerate() {
            let v = self.val(x)?;
            if own.args.get(i) == Some(&crate::own::program::Pass::Retain) {
                self.retain(&v)?;
            }
            vals.push(v);
        }
        self.run_ops(&own.jump)?;
        Ok(Flow::Recur(vals))
    }

    fn field_of(&mut self, x: &'p Expr, name: &str) -> R<Val> {
        let v = self.val(x)?;
        let id = v.expect_obj("the operand of .")?;
        let i = self.field_index(id, name)?;
        self.field(id, i)
    }

    /// The index of the field `name` of the struct `id`.
    pub fn field_index(&self, id: crate::heap::ObjId, name: &str) -> R<usize> {
        let ty = match self.objs.get(&self.heap, id)? {
            super::object::Obj::Struct { ty, .. } => *ty,
            o => {
                return Err(RunError::internal(format!(
                    "field {name} of a non-struct {o:?}"
                )))
            }
        };
        match &self.p.globals.ty(ty).shape {
            crate::types::decls::Shape::Struct(fs) => fs
                .iter()
                .position(|f| f.name == name)
                .ok_or_else(|| RunError::internal(format!("no field {name}"))),
            _ => Err(RunError::internal(format!("field {name} of an enum"))),
        }
    }

    /// The interned id of keyword `k`.
    pub fn keyword(&mut self, k: &str) -> u32 {
        if let Some(id) = self.statics.keyword_ids.get(k) {
            return *id;
        }
        let id = self.statics.keywords.len() as u32;
        self.statics.keywords.push(k.to_string());
        self.statics.keyword_ids.insert(k.to_string(), id);
        id
    }

    /// The name of the keyword `k`.
    pub fn keyword_name(&self, k: u32) -> R<&str> {
        self.statics
            .keywords
            .get(k as usize)
            .map(String::as_str)
            .ok_or_else(|| RunError::internal(format!("no keyword {k}")))
    }
}

/// The scalar type of an integer literal's width.
pub fn int_scalar(w: IntWidth) -> Scalar {
    match w {
        IntWidth::I8 => Scalar::I8,
        IntWidth::I16 => Scalar::I16,
        IntWidth::I32 => Scalar::I32,
        IntWidth::I64 => Scalar::I64,
    }
}
