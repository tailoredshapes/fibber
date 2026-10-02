//! `max-key` and `min-key` (stdlib §4.6; tranche 2 X3): Clojure's text, a
//! macro that folds its arguments.
//!
//! ```text
//! (max-key k x)       ⟹ x
//! (max-key k x y z)   ⟹ (let ((g k)) (fib.seq/max-key-pair g (fib.seq/max-key-pair g x y) z))
//! ```
//!
//! `g` is a gensym, so `k` is evaluated once however many arguments there
//! are. The pair function returns the second of two equal keys, so a tie
//! goes to the last argument, as Clojure's; `min-key` is the same over
//! `min-key-pair`. `(max-key k x)` does not evaluate `k`. The collection
//! form `(apply max-key k c)` is the library's.

use crate::syntax::{Form, Pos};

use crate::expand::build::{call, check_arity, list};
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

/// `(max-key k x y ..)` over the pair function `pair`, named `name` in
/// the arity error.
pub(super) fn key_extremum(
    ctx: &ExpandCtx,
    items: Vec<Form>,
    pos: &Pos,
    name: &str,
    pair: &str,
) -> Result<Form, ExpandError> {
    check_arity(name, &items, 2, None, pos)?;
    let mut it = items.into_iter().skip(1);
    let key = it.next().unwrap_or_else(|| list(Vec::new(), pos));
    let mut args = it;
    let first = args.next().unwrap_or_else(|| list(Vec::new(), pos));
    let rest: Vec<Form> = args.collect();
    if rest.is_empty() {
        return Ok(first);
    }
    let g = ctx.gensym("k", pos);
    let fold = rest
        .into_iter()
        .fold(first, |acc, x| call(pair, vec![g.clone(), acc, x], pos));
    let binding = list(vec![list(vec![g, key], pos)], pos);
    Ok(call("let", vec![binding, fold], pos))
}
