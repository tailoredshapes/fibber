//! Exhaustiveness and redundancy of `match` (spec/types.md §2.6), decided
//! after the unit's types are solved, on the resolved scrutinee type, by
//! the usefulness algorithm (Maranget). A scrutinee whose type is still
//! a variable admits only `_`/variable clauses as exhaustive.
//!
//! A guarded clause is checked for redundancy against the unguarded
//! clauses above it and then left out: it covers nothing. A column of
//! type `(Vec T)` with a vector pattern has one constructor per length
//! below `L` and one for every length from `L` up ([`matrix`]).
//!
//! A missing case is printed as a pattern in the spelling of syntax
//! §3.6: `nil`, `(some _)`, `(Leaf)`, `(Circle _)`, `true`, `[_ _ & _]`,
//! `_`.

mod matrix;
mod show;

use crate::types::ast::{Clause, Expr, ExprKind, Lit, PatKind, Pattern, Rest};
use crate::types::error::{ErrorKind, TResult, TypeError};
use crate::types::ty::Ty;

use super::cx::Cx;
use matrix::{Ctor, Matrix, P};

fn simplify(p: &Pattern) -> P {
    match &p.kind {
        PatKind::Wild | PatKind::Bind(_) | PatKind::Lit(Lit::Unit) => P::Wild,
        PatKind::As(inner, _) => simplify(inner),
        PatKind::Lit(Lit::Bool(b)) => P::Ctor(Ctor::Bool(*b), Vec::new()),
        PatKind::Lit(l) => P::Ctor(Ctor::Lit(format!("{l:?}")), Vec::new()),
        PatKind::Ctor(_, v, subs) => P::Ctor(
            Ctor::Variant(v.unwrap_or(0)),
            subs.iter().map(simplify).collect(),
        ),
        PatKind::Vec(subs, Rest::Exact) => {
            P::Ctor(Ctor::Len(subs.len()), subs.iter().map(simplify).collect())
        }
        PatKind::Vec(subs, _) => P::Slice(subs.iter().map(simplify).collect()),
    }
}

const MIXED: &str = "vector patterns cannot be mixed with patterns of Vec's variants";

impl Cx<'_> {
    /// Checks every `match` in `e`.
    pub fn check_matches(&mut self, e: &Expr) -> TResult<()> {
        if let ExprKind::Match(s, clauses) = &e.kind {
            let st = self.t.expr_types.get(&s.id).cloned().unwrap_or(Ty::unit());
            self.check_match(e, &st, clauses)?;
        }
        let mut result = Ok(());
        e.children(&mut |c| {
            if result.is_ok() {
                result = self.check_matches(c);
            }
        });
        result
    }

    fn check_match(&mut self, e: &Expr, st: &Ty, clauses: &[Clause]) -> TResult<()> {
        let g = self.g;
        let mut m = Matrix::new(g, self.st);
        let mut rows: Vec<Vec<P>> = Vec::new();
        let tys = std::slice::from_ref(st);
        for c in clauses {
            let row = vec![simplify(&c.pat)];
            let useful = m.useful(&rows, &row, tys).is_some();
            if m.mixed {
                return Err(TypeError::other(&c.pat.pos, MIXED));
            }
            if !useful {
                let msg = "redundant match clause";
                return Err(TypeError::new(ErrorKind::Redundant, &c.pat.pos, msg));
            }
            if c.guard.is_none() {
                rows.push(row);
            }
        }
        if let Some(w) = m.useful(&rows, &[P::Wild], tys) {
            let text = show::show(g, m.st, &w[0], st);
            let msg = format!("non-exhaustive match: missing {text}");
            return Err(TypeError::new(ErrorKind::NonExhaustive, &e.pos, msg));
        }
        Ok(())
    }
}
