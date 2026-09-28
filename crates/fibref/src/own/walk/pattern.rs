//! `match` and pattern variables (§6.1, §6.3 the `match` scrutinee and
//! clause rows): a variable that binds the whole scrutinee takes the
//! scrutinee's mode; one bound inside a variant, struct or vector
//! pattern is derived of the scrutinee's binding; a rest variable owns
//! its new vector in its clause's scope, which a false guard exits.

use crate::types::ast::{BindingId, Clause, Expr, PatKind, Pattern, Rest};

use super::super::program::{BindKind, Mode, Op, Site, Why};
use super::{ScopeKind, Walker};

/// The modes of a whole-scrutinee variable and of a variable inside a
/// constructor pattern, for a scrutinee of mode `m` whose implicit
/// owning binding (when `m` is `Owned`) is `temp`.
pub(super) fn pattern_modes(m: Mode, temp: Site) -> (Mode, Mode) {
    match m {
        Mode::Owned { .. } => (Mode::Borrowed(temp), Mode::Derived(temp)),
        Mode::Borrowed(s) => (Mode::Borrowed(s), Mode::Derived(s)),
        Mode::Derived(s) => (Mode::Derived(s), Mode::Derived(s)),
        Mode::Scalar => (Mode::Scalar, Mode::Scalar),
    }
}

/// The kind of a binding that reads as `m`.
pub(super) fn kind_of(m: Mode) -> BindKind {
    match m {
        Mode::Borrowed(s) => BindKind::AliasOf(s),
        Mode::Derived(s) => BindKind::DerivedOf(s),
        _ => BindKind::Scalar,
    }
}

impl Walker<'_> {
    /// Binds the variables of `pat`; `top` while no constructor pattern
    /// encloses it.
    pub(super) fn bind_pattern(&mut self, pat: &Pattern, whole: Mode, inner: Mode, top: bool) {
        let here = if top { whole } else { inner };
        match &pat.kind {
            PatKind::Wild | PatKind::Lit(_) => {}
            PatKind::Bind(b) => self.bind_var(*b, here),
            PatKind::Ctor(_, _, subs) => {
                for s in subs {
                    self.bind_pattern(s, whole, inner, false);
                }
            }
            PatKind::As(p, b) => {
                self.bind_var(*b, here);
                self.bind_pattern(p, whole, inner, top);
            }
            PatKind::Vec(subs, rest) => {
                for s in subs {
                    self.bind_pattern(s, whole, inner, false);
                }
                if let Rest::Bind(r) = rest {
                    self.bind_rest(*r);
                }
            }
        }
    }

    /// A rest variable: an owning binding of the innermost scope (the
    /// clause's, or the `let`'s), never scope-local (§6.11).
    fn bind_rest(&mut self, r: BindingId) {
        self.own_site(Site::Bind(r), false);
        self.bind(r, Walker::owning(r), BindKind::Owns);
    }

    /// `(match s clause..)`: the scrutinee's scope, each clause in a
    /// scope of its own, the join, the scrutinee's scope exit on every
    /// clause's path.
    pub(super) fn match_form(&mut self, e: &Expr, s: &Expr, cls: &[Clause], tail: bool) -> Mode {
        self.push_scope(ScopeKind::Bind);
        let ms = self.expr(s, false);
        let (whole, inner) = self.scrutinee(s, ms);
        let mut branches = Vec::new();
        for c in cls {
            let m = self.clause(c, whole, inner, tail);
            branches.push((&c.body, m));
        }
        let m = self.join(e, &branches);
        let scope = self.pop_scope();
        let mut out = m;
        for (i, (body, _)) in branches.iter().enumerate() {
            out = self.exit_scope_copy(&scope, body.id, m, i == 0);
        }
        out
    }

    /// One clause (§6.3): its rest variables are the owning bindings of
    /// its scope; the guard is a step, not in tail position, whose false
    /// edge exits the scope with no value; then the body.
    fn clause(&mut self, c: &Clause, whole: Mode, inner: Mode, tail: bool) -> Mode {
        self.push_scope(ScopeKind::Bind);
        self.bind_pattern(&c.pat, whole, inner, true);
        if let Some(g) = &c.guard {
            self.step(g, false);
            let ops = self.guard_exit();
            self.out.guard_fail.insert(g.id, ops);
        }
        let m = self.step(&c.body, tail);
        let scope = self.pop_scope();
        self.exit_scope(scope, c.body.id, m)
    }

    /// The releases of a false guard: the clause's owning bindings (the
    /// innermost scope's sites) in reverse order.
    fn guard_exit(&self) -> Vec<Op> {
        let Some(scope) = self.frame().scopes.last() else {
            return Vec::new();
        };
        scope
            .sites
            .iter()
            .rev()
            .map(|(s, _)| self.end_op(*s, Why::ScopeExit))
            .collect()
    }

    fn bind_var(&mut self, b: BindingId, m: Mode) {
        let m = if self.binding_is_object(b) {
            m
        } else {
            Mode::Scalar
        };
        self.bind(b, m, kind_of(m));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ast::ExprId;

    #[test]
    fn owned_scrutinee_binds_aliases_and_parts_of_its_temporary() {
        let t = Site::Value(ExprId(7));
        assert_eq!(
            pattern_modes(Mode::Owned { immortal: false }, t),
            (Mode::Borrowed(t), Mode::Derived(t))
        );
        let b = Site::Bind(crate::types::ast::BindingId(1));
        assert_eq!(
            pattern_modes(Mode::Derived(b), t),
            (Mode::Derived(b), Mode::Derived(b))
        );
        assert_eq!(kind_of(Mode::Borrowed(b)), BindKind::AliasOf(b));
    }
}
