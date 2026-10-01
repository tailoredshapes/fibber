//! The variadic collection functions (stdlib design §6.3, §4.5; tranche 1
//! R6a): `conj`, `assoc`, `dissoc` and `merge`. Each is a method or a
//! function of `fib.coll` of the fewest arguments (`conj c x`, `assoc m k
//! v`, `dissoc m k`, `merge a b`), so the macro answers the calls with more
//! and **declines** the call the function serves.
//!
//! Decided here (the plan's R6a text; the page where it is silent):
//! - `(conj c)` is `c`; `(conj c x y ..)` is the left fold of
//!   `fib.coll/conj`, one element at a time.
//! - `(assoc m k v k2 v2 ..)` is the left fold of `fib.coll/assoc`, one
//!   pair at a time; a key without a value, `(assoc m k)` or `(assoc m k v
//!   k2)`, is the error `malformed assoc: a key without a value`. `(assoc
//!   m)` and the three-argument call are declined (the function, or the
//!   checker's arity error).
//! - `(dissoc m)` is `m`; `(dissoc m k1 k2 ..)` is the left fold of
//!   `fib.coll/dissoc`.
//! - `(merge)` is the error `malformed merge: needs at least one
//!   argument`. A literal `nil` operand is skipped (§2.4: Clojure's
//!   `(merge a nil b)`); what is left is `a` for one operand, the left fold
//!   of `fib.coll/merge` for more, and `nil` when every operand was one.
//!   A call of two operands none of which is a literal `nil` is declined.
//!   (The page's other §2.4 rules for a literal `nil` first argument of
//!   `conj` and `assoc`, `(list)` and `{}`, are not done here.)
//!
//! The heads are `fib.coll/NAME`, which resolve in a module that has
//! `fib.coll` in scope (a `:use`, or the implicit list once it is
//! filled), so a binding of `conj` or `merge` in scope is not what the
//! expansion calls. Generated forms carry the call's position, the
//! arguments their own.

use crate::syntax::{Form, FormKind, Pos};

use super::fold::{declined, fold, left_fold, Alone};
use super::Outcome;
use crate::expand::build::{call, malformed, unit};
use crate::expand::error::ExpandError;

const CONJ: &str = "fib.coll/conj";
const ASSOC: &str = "fib.coll/assoc";
const DISSOC: &str = "fib.coll/dissoc";
const MERGE: &str = "fib.coll/merge";

/// `conj`: `c` for one argument, a fold from three.
pub(super) fn conj(items: Vec<Form>, pos: &Pos) -> Outcome {
    fold(items, pos, CONJ, Alone::Itself)
}

/// `dissoc`: `m` for one argument, a fold from three.
pub(super) fn dissoc(items: Vec<Form>, pos: &Pos) -> Outcome {
    fold(items, pos, DISSOC, Alone::Itself)
}

/// `(assoc m k v k2 v2 ..)`: the pairs one at a time.
pub(super) fn assoc(items: Vec<Form>, pos: &Pos) -> Result<Outcome, ExpandError> {
    let n = items.len().saturating_sub(1);
    if n < 2 || n == 3 {
        return Ok(declined(items, pos));
    }
    if n.is_multiple_of(2) {
        return Err(malformed("assoc", "a key without a value", pos));
    }
    // At least five items: the collection is there; `unit` is for totality.
    let mut it = items.into_iter().skip(1);
    let mut acc = it.next().unwrap_or_else(|| unit(pos));
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        acc = call(ASSOC, vec![acc, k, v], pos);
    }
    Ok(Outcome::Expanded(acc))
}

/// `(merge a nil b)`: the operands that are not a literal `nil`, folded.
pub(super) fn merge(items: Vec<Form>, pos: &Pos) -> Result<Outcome, ExpandError> {
    let n = items.len().saturating_sub(1);
    if n == 0 {
        return Err(malformed("merge", "needs at least one argument", pos));
    }
    let is_nil = |f: &Form| matches!(f.kind, FormKind::Nil);
    if n == 2 && !items.iter().any(is_nil) {
        return Ok(declined(items, pos));
    }
    let operands: Vec<Form> = items.into_iter().skip(1).filter(|f| !is_nil(f)).collect();
    if operands.is_empty() {
        return Ok(Outcome::Expanded(Form::new(FormKind::Nil, pos.clone())));
    }
    Ok(Outcome::Expanded(left_fold(MERGE, operands, pos)))
}
