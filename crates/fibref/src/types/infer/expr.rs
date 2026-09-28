//! Constraint generation for expressions (spec/types.md §2), unifying
//! eagerly (§3.5 step b). Calls and global names are in `call`.

use crate::syntax::Pos;

use crate::types::ast::{ConvOp, Expr, ExprKind, FnLit, Lit, Pattern, Place};
use crate::types::error::{TResult, TypeError};
use crate::types::ty::{Con, Scalar, Ty};

use super::cx::{CapsCon, Cx, DKind};

/// The type of a literal.
pub fn lit_type(l: &Lit) -> Ty {
    use crate::syntax::{FltWidth, IntWidth};
    let s = match l {
        Lit::Int(_, IntWidth::I8) => Scalar::I8,
        Lit::Int(_, IntWidth::I16) => Scalar::I16,
        Lit::Int(_, IntWidth::I32) => Scalar::I32,
        Lit::Int(_, IntWidth::I64) => Scalar::I64,
        Lit::Float(_, FltWidth::F32) => Scalar::F32,
        Lit::Float(_, FltWidth::F64) => Scalar::F64,
        Lit::Str(_) => return Ty::str(),
        Lit::Char(_) => Scalar::Char,
        Lit::Bool(_) => Scalar::Bool,
        Lit::Keyword(_) => Scalar::Keyword,
        Lit::Unit => Scalar::Unit,
    };
    Ty::scalar(s)
}

impl Cx<'_> {
    /// Infers the type of `e`, recording it.
    ///
    /// The worklist is retried after every call, field access and `@`
    /// (the forms that emit deferred constraints), so a constraint that
    /// cannot hold is reported at its form in evaluation order (§3.5)
    /// rather than at the end of the unit; acceptance does not depend on
    /// when it is retried.
    pub fn infer(&mut self, e: &Expr) -> TResult<Ty> {
        let ty = self.infer_kind(e)?;
        self.record(e.id, &ty);
        let emits = matches!(
            e.kind,
            ExprKind::Call(..) | ExprKind::Field(..) | ExprKind::Deref(..)
        );
        if emits && !self.u.deferred.is_empty() {
            self.solve_all()?;
        }
        Ok(ty)
    }

    fn infer_kind(&mut self, e: &Expr) -> TResult<Ty> {
        let pos = &e.pos;
        match &e.kind {
            ExprKind::Lit(l) => Ok(lit_type(l)),
            ExprKind::Local(b) => self.binding(*b, pos),
            ExprKind::Global(r) => self.global_value(e.id, *r, pos),
            ExprKind::Call(head, args) => self.call(e, head, args),
            ExprKind::Fn(lit) => self.fn_lit(e, lit),
            ExprKind::Let(bs, body) => self.let_expr(bs, body),
            ExprKind::If(c, t, f) => {
                let ct = self.infer(c)?;
                self.unify(&ct, &Ty::bool(), &c.pos)?;
                self.join(&[t, f], pos)
            }
            ExprKind::Do(steps) => self.do_expr(steps),
            ExprKind::Match(s, clauses) => self.match_expr(s, clauses, pos),
            ExprKind::Loop(vars, body) => self.loop_expr(vars, body),
            ExprKind::Recur(args) => self.recur(args, pos),
            ExprKind::Field(x, f, text) => self.field(x, f, text, pos),
            ExprKind::Deref(place, text) => self.deref(place, text, pos),
            ExprKind::Set(place, v) => self.set(place, v, pos),
            ExprKind::SetField(b, f, v) => self.set_field(*b, f, v, pos),
            ExprKind::Async(body, caps) => self.async_expr(body, caps, pos),
            ExprKind::Await(x) => self.await_expr(x, pos),
            ExprKind::Unsafe(b) => self.infer(b),
            ExprKind::Quote(_) => self.form_type(pos),
            ExprKind::Dyn(p, dets, x) => self.dyn_expr(*p, dets, x, pos),
            ExprKind::Convert(op, target, x) => self.convert(*op, *target, x),
            ExprKind::Concat(parts) => self.concat(parts, pos),
        }
    }

    fn let_expr(&mut self, bs: &[(Pattern, Expr)], body: &Expr) -> TResult<Ty> {
        for (pat, init) in bs {
            let t = self.infer(init)?;
            let t = self.let_binding_type(pat, t, &init.pos)?;
            self.check_pattern(pat, &t)?;
        }
        self.infer(body)
    }

    fn do_expr(&mut self, steps: &[Expr]) -> TResult<Ty> {
        let mut last = Ty::unit();
        for s in steps {
            last = self.infer(s)?;
        }
        Ok(last)
    }

    /// `(. e f)`: `HasField(T, f, R)` (§2.5).
    fn field(&mut self, x: &Expr, f: &str, text: &str, pos: &Pos) -> TResult<Ty> {
        let t = self.infer(x)?;
        let r = self.fresh();
        let kind = DKind::Field(t, f.to_string(), r.clone(), text.to_string());
        self.defer(kind, pos, None);
        Ok(r)
    }

    fn await_expr(&mut self, x: &Expr, pos: &Pos) -> TResult<Ty> {
        let t = self.infer(x)?;
        let r = self.fresh();
        self.unify(&t, &Ty::task(r.clone()), pos)?;
        Ok(r)
    }

    /// Branches joined: each flows into the result (a flow site, §3.2).
    fn join(&mut self, branches: &[&Expr], _pos: &Pos) -> TResult<Ty> {
        let r = self.fresh();
        for b in branches {
            let t = self.infer(b)?;
            self.flow(&t, &r, &b.pos)?;
        }
        Ok(r)
    }

    fn match_expr(&mut self, s: &Expr, clauses: &[(Pattern, Expr)], _pos: &Pos) -> TResult<Ty> {
        let st = self.infer(s)?;
        let r = self.fresh();
        for (pat, body) in clauses {
            self.check_pattern(pat, &st)?;
            let t = self.infer(body)?;
            self.flow(&t, &r, &body.pos)?;
        }
        Ok(r)
    }

    fn loop_expr(
        &mut self,
        vars: &[(crate::types::ast::BindingId, Expr)],
        body: &Expr,
    ) -> TResult<Ty> {
        let mut tys = Vec::new();
        for (b, init) in vars {
            let t = self.infer(init)?;
            let v = match self.binding_ann(*b, &init.pos)? {
                Some(a) => a,
                None => self.fresh(),
            };
            self.flow(&t, &v, &init.pos)?;
            self.bind(*b, &v);
            tys.push(v);
        }
        self.u.loops.push(tys);
        let r = self.infer(body);
        self.u.loops.pop();
        r
    }

    fn recur(&mut self, args: &[Expr], pos: &Pos) -> TResult<Ty> {
        let Some(vars) = self.u.loops.last().cloned() else {
            return Err(TypeError::other(pos, "recur outside loop"));
        };
        if vars.len() != args.len() {
            let msg = format!("recur takes {} argument(s), got {}", vars.len(), args.len());
            return Err(TypeError::other(pos, msg));
        }
        for (a, v) in args.iter().zip(&vars) {
            let t = self.infer(a)?;
            self.flow(&t, v, &a.pos)?;
        }
        Ok(self.fresh())
    }

    fn fn_lit(&mut self, e: &Expr, lit: &FnLit) -> TResult<Ty> {
        let colour = self.st.fresh_colour();
        let mut params = Vec::new();
        for (b, ann) in &lit.params {
            let t = match ann {
                Some(a) => self.ann(a, &e.pos)?,
                None => self.fresh(),
            };
            self.bind(*b, &t);
            params.push(t);
        }
        let ret = match &lit.ret {
            Some(a) => self.ann(a, &e.pos)?,
            None => self.fresh(),
        };
        let ty = Ty::Fn(colour, params, Box::new(ret.clone()));
        if let Some(name) = lit.name {
            self.bind(name, &ty);
        }
        let body = self.infer(&lit.body)?;
        self.flow(&body, &ret, &lit.body.pos)?;
        for c in &lit.captures {
            let ct = self.binding(*c, &e.pos)?;
            let label = format!("closure capture {}", self.g.binding(*c).name);
            self.u.caps.push(CapsCon {
                colour,
                ty: ct,
                label,
                pos: e.pos.clone(),
            });
        }
        self.u.fn_lits.push((e.id, colour));
        Ok(ty)
    }

    fn deref(&mut self, place: &Place, text: &str, pos: &Pos) -> TResult<Ty> {
        match place {
            Place::Amp(b) => {
                let t = self.binding(*b, pos)?;
                let a = self.fresh();
                self.unify(&t, &Ty::cell(a.clone()), pos)?;
                Ok(a)
            }
            Place::Expr(x) => {
                let t = self.infer(x)?;
                let r = self.fresh();
                self.defer(DKind::Deref(t, r.clone(), text.to_string()), pos, None);
                Ok(r)
            }
        }
    }

    fn set(&mut self, place: &Place, v: &Expr, pos: &Pos) -> TResult<Ty> {
        let target = match place {
            Place::Amp(b) => self.binding(*b, pos)?,
            Place::Expr(x) => self.infer(x)?,
        };
        let a = self.fresh();
        self.unify(&target, &Ty::cell(a.clone()), pos)?;
        let vt = self.infer(v)?;
        self.flow(&vt, &a, &v.pos)?;
        Ok(Ty::unit())
    }

    fn set_field(
        &mut self,
        b: crate::types::ast::BindingId,
        f: &str,
        v: &Expr,
        pos: &Pos,
    ) -> TResult<Ty> {
        let t = self.binding(b, pos)?;
        let s = self.fresh();
        if self.unify(&t, &Ty::cell(s.clone()), pos).is_err() {
            return Err(amp_not_cell(pos));
        }
        let ft = self.fresh();
        let text = format!("@{}", self.g.binding(b).name);
        self.defer(DKind::Field(s, f.to_string(), ft.clone(), text), pos, None);
        let vt = self.infer(v)?;
        self.flow(&vt, &ft, &v.pos)?;
        Ok(Ty::unit())
    }

    fn async_expr(
        &mut self,
        body: &Expr,
        caps: &[crate::types::ast::BindingId],
        pos: &Pos,
    ) -> TResult<Ty> {
        let saved = std::mem::take(&mut self.u.loops);
        let t = self.infer(body);
        self.u.loops = saved;
        let t = t?;
        for c in caps {
            let ct = self.binding(*c, pos)?;
            let path = vec![format!("async capture {}", self.g.binding(*c).name)];
            self.defer(DKind::Send(ct, path), pos, None);
        }
        Ok(Ty::task(t))
    }

    fn form_type(&mut self, pos: &Pos) -> TResult<Ty> {
        match self.g.form {
            Some(f) => Ok(Ty::nominal(f, Vec::new())),
            None => Err(TypeError::other(pos, "Form is not declared")),
        }
    }

    fn dyn_expr(
        &mut self,
        p: crate::types::ty::ProtoId,
        dets: &[crate::types::ast::TypeAnn],
        x: &Expr,
        pos: &Pos,
    ) -> TResult<Ty> {
        let t = self.infer(x)?;
        self.defer(DKind::Object(t.clone(), "dyn".to_string()), pos, None);
        let mut args = vec![t];
        let mut dtys = Vec::new();
        for d in dets {
            let dt = self.ann(d, pos)?;
            dtys.push(dt.clone());
            args.push(dt);
        }
        self.defer(DKind::Proto(p, args, None), pos, None);
        Ok(Ty::Con(Con::Dyn(p), dtys))
    }

    fn convert(&mut self, op: ConvOp, target: Scalar, x: &Expr) -> TResult<Ty> {
        let t = self.infer(x)?;
        match op {
            ConvOp::IntToInt(_) | ConvOp::IntToFloat { .. } => {
                let Some(bits) = self
                    .g
                    .proto_name(crate::types::decls::ModuleId::Builtin, "Bits")
                else {
                    return Err(TypeError::other(&x.pos, "no Bits protocol"));
                };
                self.defer(DKind::Proto(bits, vec![t], None), &x.pos, None);
            }
            ConvOp::FloatToFloat | ConvOp::FloatToInt { .. } => {
                self.defer(DKind::Float(t), &x.pos, None)
            }
        }
        Ok(Ty::scalar(target))
    }

    fn concat(&mut self, parts: &[Expr], pos: &Pos) -> TResult<Ty> {
        let Some(vec) = self.g.vec else {
            return Err(TypeError::other(pos, "concat needs the prelude's Vec"));
        };
        let a = self.fresh();
        let vt = Ty::nominal(vec, vec![a]);
        for p in parts {
            let t = self.infer(p)?;
            self.unify(&t, &vt, &p.pos)?;
        }
        Ok(vt)
    }
}

/// `& argument must be a cell variable` (§2.14).
pub fn amp_not_cell(pos: &Pos) -> TypeError {
    TypeError::new(
        crate::types::error::ErrorKind::AmpArgument,
        pos,
        "& argument must be a cell variable",
    )
}
