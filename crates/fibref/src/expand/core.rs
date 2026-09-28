//! Which operands of the core forms of §3 are expressions (expanded),
//! which are patterns (normalised, not expanded), and which are names,
//! parameter lists or types (left alone), as a [`Role`] per operand.
//!
//! Planning is separate from walking so that the recursive walk in
//! `expr` stays two small stack frames per level of nesting: the shape
//! checks and the choice of roles happen here, in frames that have
//! returned before the walk descends.

use crate::syntax::{Form, FormKind, Pos};

use super::build::malformed;
use super::error::ExpandError;

/// What the walk does with a form.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Role {
    /// Leave it alone.
    Keep,
    /// Expand it in expression position.
    Expr,
    /// Expand it in argument position of a call, where `(& x)` may stand.
    Arg,
    /// Normalise it as a pattern.
    Pattern,
    /// Walk its items: item `i` by the `i`th role, the rest by the last.
    Items(Vec<Role>, Box<Role>),
}

impl Role {
    /// `n` kept items, then every other item with `rest`.
    pub(crate) fn after(n: usize, rest: Role) -> Role {
        Role::Items(vec![Role::Keep; n], Box::new(rest))
    }

    /// The roles `first`, then every other item with `rest`.
    pub(crate) fn items(first: Vec<Role>, rest: Role) -> Role {
        Role::Items(first, Box::new(rest))
    }
}

/// The role of the core expression form `name` (not a definition),
/// after checking its shape.
pub(crate) fn expr_plan(name: &str, items: &[Form], pos: &Pos) -> Result<Role, ExpandError> {
    use Role::{Expr, Keep};
    let exact = |len: usize| {
        if items.len() == len {
            Ok(())
        } else {
            Err(malformed(name, "wrong number of operands", pos))
        }
    };
    match name {
        "quote" | "var" => exact(2).map(|_| Keep),
        "if" => exact(4).map(|_| Role::after(1, Expr)),
        "await" => exact(2).map(|_| Role::after(1, Expr)),
        "." => exact(3).map(|_| Role::items(vec![Keep, Expr], Keep)),
        "fn" => {
            let named = items.get(1).is_some_and(|f| f.as_sym().is_some());
            let i = if named { 2 } else { 1 };
            body_start(name, items, i, pos).map(|start| Role::after(start, Expr))
        }
        "let" | "loop" => let_plan(name, items, pos),
        "match" => match_plan(items, pos),
        // do, async, unsafe, recur: every operand is an expression.
        _ => Ok(Role::after(1, Expr)),
    }
}

/// Skips `:where C` and `-> T` annotations starting at `i`.
pub(crate) fn skip_annotations(items: &[Form], mut i: usize) -> usize {
    while i < items.len() {
        let annotation = match &items[i].kind {
            FormKind::Kw(k) => k == "where",
            FormKind::Sym(s) => s == "->",
            _ => false,
        };
        if !annotation {
            break;
        }
        i += 2;
    }
    i.min(items.len())
}

/// Where the body starts in `params ret? body+`, `items[i]` being the
/// parameter list; the body must not be empty.
pub(crate) fn body_start(
    head: &str,
    items: &[Form],
    i: usize,
    pos: &Pos,
) -> Result<usize, ExpandError> {
    if items.get(i).is_none_or(|p| p.as_list().is_none()) {
        return Err(malformed(head, "expected a parameter list", pos));
    }
    let start = skip_annotations(items, i + 1);
    if start >= items.len() {
        return Err(malformed(head, "missing body", pos));
    }
    Ok(start)
}

/// `(let (binding*) body)` (§3.3) and `(loop (binding*) body)` (§3.18),
/// `binding ::= (pat expr) | (sym: type expr)` (§1.5).
fn let_plan(head: &str, items: &[Form], pos: &Pos) -> Result<Role, ExpandError> {
    if items.len() < 3 {
        return Err(malformed(head, "expected bindings and a body", pos));
    }
    let Some(pairs) = items[1].as_list() else {
        return Err(malformed(head, "bindings must be a list", &items[1].pos));
    };
    let mut roles = Vec::new();
    for pair in pairs {
        let role = match pair.as_list() {
            Some([_, _]) => Role::items(vec![Role::Pattern, Role::Expr], Role::Keep),
            Some([name, _, _]) if annotated_name(name) => {
                Role::items(vec![Role::Keep, Role::Keep, Role::Expr], Role::Keep)
            }
            _ => {
                let reason = "a binding is (pattern expression) or (name: type expression)";
                return Err(malformed(head, reason, &pair.pos));
            }
        };
        roles.push(role);
    }
    let bindings = Role::items(roles, Role::Keep);
    Ok(Role::items(vec![Role::Keep, bindings], Role::Expr))
}

/// Whether `form` is `x:`, the name of an annotated binding (§1.5).
pub(crate) fn annotated_name(form: &Form) -> bool {
    form.as_sym()
        .and_then(|s| s.strip_suffix(':'))
        .is_some_and(|n| !n.is_empty())
}

/// `(match expr clause+)`, `clause ::= (pat body)` (§3.6).
fn match_plan(items: &[Form], pos: &Pos) -> Result<Role, ExpandError> {
    if items.len() < 3 {
        return Err(malformed("match", "expected a scrutinee and clauses", pos));
    }
    for clause in &items[2..] {
        if clause.as_list().is_none_or(|c| c.len() < 2) {
            return Err(malformed(
                "match",
                "a clause is (pattern body+)",
                &clause.pos,
            ));
        }
    }
    let clause = Role::items(vec![Role::Pattern], Role::Expr);
    Ok(Role::items(vec![Role::Keep, Role::Expr], clause))
}
