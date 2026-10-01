//! The plan's operations (types §8.10): the value a site names, and
//! `retain`, `release` and the end of a stack object's scope.

use std::collections::HashSet;

use fibref::own::program::{BodyOwn, Op, OpKind, Site};
use fibref::types::ast::ExprId;

use super::{Cx, R};
use crate::compile::Unsupported;
use crate::ir::V;

/// The expressions whose value some operation names.
pub fn value_sites(own: &BodyOwn) -> HashSet<ExprId> {
    let mut out = HashSet::new();
    let mut add = |ops: &[Op]| {
        for op in ops {
            if let Site::Value(e) = op.site {
                out.insert(e);
            }
        }
    };
    own.exprs.values().for_each(|x| add(&x.after));
    own.calls.values().for_each(|c| add(&c.jump));
    own.recurs.values().for_each(|r| add(&r.jump));
    own.guard_fail.values().for_each(|ops| add(ops));
    out
}

impl<'a> Cx<'_, 'a> {
    /// The value a site names; `None` for a `def` (immortal: every
    /// operation on it is a no-op).
    pub fn site(&mut self, s: Site) -> R<Option<V>> {
        Ok(match s {
            Site::Bind(b) | Site::Capture(_, b) => Some(self.local(b)?),
            Site::Value(e) => Some(self.values.get(&e).cloned().ok_or_else(|| {
                Unsupported(format!(
                    "the plan names value {e:?}, which has no value here"
                ))
            })?),
            Site::Env(_) => self.env.clone(),
            Site::Global(_) => None,
        })
    }

    /// The operations after expression `e`.
    pub fn after(&mut self, e: ExprId) -> R<()> {
        let ops = match self.own.exprs.get(&e) {
            Some(x) if !x.after.is_empty() => x.after.clone(),
            _ => return Ok(()),
        };
        self.run_ops(&ops)
    }

    pub fn run_ops(&mut self, ops: &[Op]) -> R<()> {
        for op in ops {
            let Some(v) = self.site(op.site)? else {
                continue;
            };
            match op.kind {
                OpKind::Retain => self.retain(&v),
                OpKind::Release => self.release(&v),
                OpKind::EndStack => self.end_stack(&v)?,
            }
        }
        Ok(())
    }

    pub fn retain(&mut self, v: &V) {
        if let Some(w) = self.obj_word(v) {
            self.b.stmt(&format!("(call @fib.retain {w})"));
        }
    }

    pub fn release(&mut self, v: &V) {
        if let Some(w) = self.obj_word(v) {
            self.b.stmt(&format!("(call @fib.release {w})"));
        }
    }

    /// The end of a stack object's scope (§6.11): its inline drop.
    fn end_stack(&mut self, v: &V) -> R<()> {
        let w = self
            .obj_word(v)
            .ok_or_else(|| Unsupported("the plan ends a scalar as a stack object".into()))?;
        self.b.stmt(&format!("(call @fib.stack-end {w})"));
        if self.closure_slots.contains(&w) {
            // A stack closure holds only aliases: nothing to release.
            return Ok(());
        }
        self.b.stmt(&format!("(call @fib.drop-fields {w})"));
        Ok(())
    }
}
