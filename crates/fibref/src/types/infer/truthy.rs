//! Truthiness (stdlib §7 L20): the test of an `if` or of the operands of
//! `and` and `or` is a `bool` or an `(Option T)`, truthy when `true` or
//! `(some _)`; the one-armed `if` and the default-less `cond` are unit for
//! unit bodies and `(Option T)` otherwise; `and` has the type of its last
//! operand and `or` is typed by it. The rules type the forms; `elab`
//! rewrites them into `if` and `match` once the types are known.

use crate::syntax::Pos;

use crate::types::ast::Expr;
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::{Con, Scalar, Ty};

use super::cx::Cx;

impl Cx<'_> {
    /// The payload of `t` when it is an `Option` (resolved so far).
    fn option_payload(&mut self, t: &Ty) -> Option<Ty> {
        match self.st.resolve(t) {
            Ty::Con(Con::Nominal(id), args) if id == self.g.option => args.first().cloned(),
            _ => None,
        }
    }

    /// The test of an `if`, a clause of a one-armed `if`, an operand of
    /// `and` or a non-last operand of `or`: `bool` or `(Option T)`. A
    /// test of a primitive type that is not `bool` can never be false,
    /// which is the mistake a Clojure habit makes (`(if n ..)`, `(when s
    /// ..)`), so the error says that instead of `cannot unify i64 with
    /// bool` (stdlib §7 D1). A type not known yet is `bool`; any other
    /// type is the usual mismatch.
    pub(super) fn condition(&mut self, t: &Ty, pos: &Pos) -> TResult<()> {
        if self.option_payload(t).is_some() {
            return Ok(());
        }
        let always_true = match self.st.resolve(t) {
            Ty::Con(Con::Scalar(s), _) => !matches!(s, Scalar::Bool | Scalar::Unit | Scalar::Ptr),
            Ty::Con(Con::Str, _) => true,
            _ => false,
        };
        if always_true {
            let msg = format!(
                "a value of type {} is always true; write the test",
                self.show(t)
            );
            return Err(TypeError::new(ErrorKind::Unify, pos, msg));
        }
        self.unify(t, &Ty::bool(), pos)
    }

    /// `(if t b)` and the chain of them: the bodies are all unit, or all
    /// of one type `T` and the form is `(Option T)`. A body whose type is
    /// not known (a `recur`) is unit unless another body says otherwise.
    pub(super) fn guarded(&mut self, clauses: &[(Expr, Expr)]) -> TResult<Ty> {
        let mut bodies = Vec::new();
        for (test, body) in clauses {
            let ct = self.infer(test)?;
            self.condition(&ct, &test.pos)?;
            bodies.push(self.infer(body)?);
        }
        let mut first = None;
        for b in &bodies {
            let r = self.st.resolve(b);
            if !matches!(r, Ty::Var(_)) {
                first = Some(r);
                break;
            }
        }
        let value = first.filter(|t| !matches!(t, Ty::Con(Con::Scalar(Scalar::Unit), _)));
        let target = value.clone().unwrap_or_else(Ty::unit);
        for ((_, body), b) in clauses.iter().zip(&bodies) {
            self.flow(b, &target, &body.pos)?;
        }
        Ok(match value {
            Some(t) => Ty::nominal(self.g.option, vec![t]),
            None => Ty::unit(),
        })
    }

    /// `(and a b ..)`: each operand is a test; the type is the last's.
    pub(super) fn and_expr(&mut self, ops: &[Expr]) -> TResult<Ty> {
        let mut last = Ty::bool();
        for o in ops {
            last = self.infer(o)?;
            self.condition(&last, &o.pos)?;
        }
        Ok(last)
    }

    /// `(or a b ..)`: typed by the last operand `L`. An earlier operand
    /// is a `bool` (and `L` is), an `(Option T)` when `L` is `bool`, an
    /// `(Option T)` equal to `L` when `L` is an `Option`, else an
    /// `(Option L)`: `(or (get m k) 7)` is the value or the default.
    pub(super) fn or_expr(&mut self, ops: &[Expr]) -> TResult<Ty> {
        let mut tys = Vec::new();
        for o in ops {
            tys.push(self.infer(o)?);
        }
        let Some(last) = tys.last().cloned() else {
            return Ok(Ty::bool());
        };
        for (o, t) in ops.iter().zip(&tys).take(ops.len() - 1) {
            if self.option_payload(t).is_none() {
                self.condition(t, &o.pos)?;
                self.unify(&last, &Ty::bool(), &o.pos)?;
            } else if self.option_payload(&last).is_some() {
                self.unify(t, &last, &o.pos)?;
            } else if !matches!(
                self.st.resolve(&last),
                Ty::Con(Con::Scalar(Scalar::Bool), _)
            ) {
                let opt = Ty::nominal(self.g.option, vec![last.clone()]);
                self.unify(t, &opt, &o.pos)?;
            }
        }
        Ok(last)
    }
}
