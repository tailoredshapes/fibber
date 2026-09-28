//! Lowering `match` (syntax §3.6): the scrutinee, then each clause
//! `(pat body+)` or `(pat :when guard body+)`. A clause's pattern
//! variables are in scope in its guard and its body; the guard is never
//! in tail position, the body is when the `match` is.

use crate::syntax::{Form, FormKind};
use crate::types::ast::{BindingKind, Clause, Expr, ExprKind};
use crate::types::error::{TResult, TypeError};

use super::scope::Lowerer;

/// Whether `f` is the keyword `:when`.
fn is_when(f: &Form) -> bool {
    matches!(&f.kind, FormKind::Kw(k) if k == "when")
}

impl Lowerer<'_> {
    pub(super) fn match_form(&mut self, items: &[Form], form: &Form, tail: bool) -> TResult<Expr> {
        if items.len() < 3 {
            return Err(TypeError::resolve(
                &form.pos,
                "match needs a scrutinee and clauses",
            ));
        }
        let scrut = self.expr(&items[1], false)?;
        let mut clauses = Vec::new();
        for clause in &items[2..] {
            let mark = self.mark();
            let lowered = self.clause(clause, tail);
            self.reset(mark);
            clauses.push(lowered?);
        }
        Ok(self.mk(&form.pos, ExprKind::Match(Box::new(scrut), clauses)))
    }

    fn clause(&mut self, clause: &Form, tail: bool) -> TResult<Clause> {
        let parts = clause.as_list().unwrap_or(&[]);
        if parts.len() < 2 {
            return Err(TypeError::resolve(
                &clause.pos,
                "a clause is (pattern body+)",
            ));
        }
        let guarded = is_when(&parts[1]);
        if guarded && parts.len() < 4 {
            let msg = "a guarded clause is (pattern :when guard body+)";
            return Err(TypeError::resolve(&clause.pos, msg));
        }
        let pat = super::pattern::pattern(self, &parts[0], BindingKind::Pattern, &mut Vec::new())?;
        let (guard, body) = if guarded {
            (Some(self.expr(&parts[2], false)?), &parts[3..])
        } else {
            (None, &parts[1..])
        };
        let body = self.body(body, &clause.pos, tail)?;
        Ok(Clause { pat, guard, body })
    }
}
