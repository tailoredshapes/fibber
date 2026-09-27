//! Pattern variables (§6.1, §6.3 the `match` scrutinee row): a variable
//! that binds the whole scrutinee takes the scrutinee's mode; one bound
//! inside a variant or struct pattern is derived of the scrutinee's
//! binding.

use crate::types::ast::{PatKind, Pattern};

use super::super::program::{BindKind, Mode, Site};
use super::Walker;

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
        }
    }

    fn bind_var(&mut self, b: crate::types::ast::BindingId, m: Mode) {
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
