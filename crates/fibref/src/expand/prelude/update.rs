//! `update` with Clojure's extra arguments and the `fnil` routing
//! (stdlib design §2.4 and the row `update`, tranche 1 R6b; Appendix A11
//! t24). The library function `fib.coll/update` is the three-argument
//! form: `f` sees the value, a missing key traps `update: no key`. This
//! macro answers the calls the function cannot:
//!
//! ```text
//! (update m k f x ..)        ⟹ (fib.coll/update m k (fn (v) (f v x ..)))
//! (update m k (fnil g d))    ⟹ (fib.coll/update-or m k g d)
//! (update m k (fnil g d) x ..) ⟹ (fib.coll/update-or m k (fn (v) (g v x ..)) d)
//! ```
//!
//! with `v` a gensym. A call with fewer than three arguments, and a
//! three-argument call whose `f` is not a literal `(fnil g d)`, is
//! declined: it stays a call of the library function `update`. The extra
//! arguments are evaluated inside the closure, each time the library calls
//! it (once, or never when the key is missing and the function traps).
//! Only the literal `(fnil g d)` with exactly one function and one default
//! is routed; any other `f` is an ordinary expression.

use crate::syntax::{Form, FormKind, Pos};

use super::Outcome;
use crate::expand::build::{call, list, sym};
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// The library's functions, written through the facade `fib.coll` so that
/// a binding of the program's own `update` cannot capture them.
const UPDATE: &str = "fib.coll/update";
const UPDATE_OR: &str = "fib.coll/update-or";

/// The `g` and `d` of a literal `(fnil g d)`.
fn fnil_literal(form: &Form) -> Option<(&Form, &Form)> {
    match form.as_list()? {
        [head, g, d] if head.as_sym() == Some("fnil") => Some((g, d)),
        _ => None,
    }
}

/// `(fn (v) (f v x ..))` with `v` a gensym: `f` called with the value and
/// the extra arguments.
fn closure(ctx: &ExpandCtx, f: Form, extra: &[Form], pos: &Pos) -> Form {
    let v = ctx.gensym("v", pos);
    let mut body = vec![f, v.clone()];
    body.extend(extra.iter().cloned());
    let params = list(vec![v], pos);
    list(vec![sym("fn", pos), params, list(body, pos)], pos)
}

/// One call of `update`: the rewrite, or the call itself, declined.
pub(super) fn update(ctx: &ExpandCtx, items: Vec<Form>, pos: Pos) -> Result<Outcome, ExpandError> {
    if items.len() < 4 {
        return Ok(Outcome::Declined(Form::new(FormKind::List(items), pos)));
    }
    let (m, k, f) = (items[1].clone(), items[2].clone(), items[3].clone());
    let extra = &items[4..];
    let routed = fnil_literal(&f).map(|(g, d)| (g.clone(), d.clone()));
    let expansion = match routed {
        Some((g, d)) if extra.is_empty() => call(UPDATE_OR, vec![m, k, g, d], &pos),
        Some((g, d)) => {
            let f = closure(ctx, g, extra, &pos);
            call(UPDATE_OR, vec![m, k, f, d], &pos)
        }
        None if extra.is_empty() => {
            return Ok(Outcome::Declined(Form::new(FormKind::List(items), pos)))
        }
        None => {
            let f = closure(ctx, f, extra, &pos);
            call(UPDATE, vec![m, k, f], &pos)
        }
    };
    Ok(Outcome::Expanded(expansion))
}
