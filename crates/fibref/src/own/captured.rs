//! Whether an `&` parameter is **captured by an argument** of a call
//! (types §6.6; **Decided**, owner, 2026-09-28): an argument other than
//! the `&v` itself mentions `v` or a **carrier** of `v`, a `let` binding
//! whose initialiser is directly a `fn` literal that mentions `v` or a
//! carrier. Such an `&v` is not forwarded at a call in tail position;
//! it is copied in and written back, and the call is ordinary (§6.10
//! rule (b)). The test is syntactic.

use std::collections::HashSet;

use crate::types::ast::{Arg, BindingId, Expr, ExprKind, PatKind, Place};

use super::syntactic::{children_of, visit};

/// The carriers of the `&` parameter `v` in the body `body`.
pub(super) fn carriers(body: &Expr, v: BindingId) -> HashSet<BindingId> {
    let mut lets = Vec::new();
    closure_lets(body, &mut lets);
    let mut out = HashSet::new();
    loop {
        let before = out.len();
        for (b, init) in &lets {
            if !out.contains(b) && mentions(init, v, &out) {
                out.insert(*b);
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// Every `let` binding in `e` whose initialiser is directly a `fn`
/// literal, with that literal.
fn closure_lets<'e>(e: &'e Expr, out: &mut Vec<(BindingId, &'e Expr)>) {
    if let ExprKind::Let(bs, _) = &e.kind {
        for (p, init) in bs {
            if let (PatKind::Bind(b), ExprKind::Fn(_)) = (&p.kind, &init.kind) {
                out.push((*b, init));
            }
        }
    }
    for c in children_of(e) {
        closure_lets(c, out);
    }
}

/// Whether `e` mentions the `&` parameter `v` (`@v`, `&v`, the target
/// of `set!` or `set-field!`) or one of the bindings `carriers`, at any
/// depth, inside `fn` and `async` literals too.
pub(super) fn mentions(e: &Expr, v: BindingId, carriers: &HashSet<BindingId>) -> bool {
    let mut hit = false;
    visit(e, &mut |x| {
        hit |= match &x.kind {
            ExprKind::Deref(Place::Amp(b), _) | ExprKind::Set(Place::Amp(b), _) => *b == v,
            ExprKind::SetField(b, _, _) => *b == v,
            ExprKind::Local(b) => carriers.contains(b),
            ExprKind::Call(_, args) => args.iter().any(|a| matches!(a, Arg::Amp(b, _) if *b == v)),
            _ => false,
        };
    });
    hit
}

/// Whether argument `i`, an `&v`, is captured by another argument of
/// the call whose arguments are `args`.
pub(super) fn captured_at(
    args: &[Arg],
    i: usize,
    v: BindingId,
    carriers: &HashSet<BindingId>,
) -> bool {
    args.iter().enumerate().any(|(j, a)| {
        j != i
            && match a {
                Arg::Expr(x) => mentions(x, v, carriers),
                Arg::Amp(b, _) => *b == v,
            }
    })
}
