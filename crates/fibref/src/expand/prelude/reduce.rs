//! `reduce` (stdlib design §2.1 rule 3 and the rows `reduce`, `reduced`,
//! tranche 1 R6b, PC-6): the 33rd Rust macro, one that declines by
//! default. The library function `fib.seq/reduce` is the three-argument
//! fold `(reduce f init c)` with `f: (fn (a e) a)`; the macro answers the
//! two calls the function cannot:
//!
//! ```text
//! (reduce f c)        ⟹ (fib.seq/reduce-nonempty f c)
//! (reduce + c)        ⟹ (fib.seq/reduce + 0 c)        ; * 1, conj [], merge {}
//! (reduce str c)      ⟹ (fib.seq/reduce (fn (a x) (fib.prelude/str a x)) "" c)
//! (reduce concat c)   ⟹ (fib.seq/reduce concat (fib.seq/lazy-node (fn () fib.seq/LNil)) c)
//! (reduce (fn (a x) .. (reduced e) ..) init c)
//!                     ⟹ (fib.seq/reduce-while (fn (a x) ..) init c)
//! ```
//!
//! The value `str` is the library's one-argument function, which is not
//! the accumulator's two-argument step, so a literal `str` is the `fn` that
//! calls the macro (gensym parameters `#r.N`), whose pieces take their
//! `to-str` as `(str a b)` does, in the two-argument form and in
//! `(reduce str init c)` alike; `concat` is the two-argument function, but
//! returns an `LSeq`, so its start is the empty one. A literal head the
//! module defines or sees exported by a program module (`hides_macro`) is
//! that function and has no identity: it goes through `reduce-nonempty`.
//!
//! In the last form every tail position of the literal `fn`'s body is
//! rewritten, through `if`, `let`, `do`, `match`, flat `cond`, `when`,
//! `unless`, `if-let`, `when-let` and `loop`: a tail `(reduced e)` becomes
//! `(fib.core/Done e)`, any other tail `v` becomes `(fib.core/More v)`,
//! and a tail `(recur ..)` is left alone (it is not a value). A call is
//! declined, and stays a call of the library function, when it has any
//! other number of arguments than 2 or 3, or when it is the three-argument
//! form and `f` is not a literal unnamed `(fn (a x) body+)` of two
//! parameters with no result annotation or `:where`, or has no `(reduced
//! e)` in a tail position. The tails are found with an explicit stack, so
//! the depth of a body costs no native stack.

use crate::syntax::{Form, FormKind, IntWidth, Pos};

use super::Outcome;
use crate::expand::brackets::seq_items;
use crate::expand::build::{call, list, string, sym};
use crate::expand::collections::prelude_name;
use crate::expand::ctx::ExpandCtx;
use crate::expand::error::ExpandError;

const REDUCE: &str = "fib.seq/reduce";
const REDUCE_NONEMPTY: &str = "fib.seq/reduce-nonempty";
const REDUCE_WHILE: &str = "fib.seq/reduce-while";
const MORE: &str = "fib.core/More";
const DONE: &str = "fib.core/Done";
const LAZY_NODE: &str = "fib.seq/lazy-node";
const LNIL: &str = "fib.seq/LNil";

/// The function a literal `str` is folded with: `(fn (a x) (str a x))`
/// over gensyms, the call written `fib.prelude/str`, which no definition
/// of the module's own `str` captures.
fn str_step(ctx: &ExpandCtx, pos: &Pos) -> Form {
    let (acc, elem) = (ctx.gensym("r", pos), ctx.gensym("r", pos));
    let body = call(&prelude_name("str"), vec![acc.clone(), elem.clone()], pos);
    call("fn", vec![list(vec![acc, elem], pos), body], pos)
}

/// Whether `f` is the literal `str` that the prelude macro `str` is the
/// meaning of in this module.
fn is_prelude_str(ctx: &ExpandCtx, f: &Form) -> bool {
    f.as_sym() == Some("str") && !ctx.hides_macro("str")
}

/// What `(reduce f c)` folds with and starts from, for the literal heads
/// whose `(f)` Clojure's reduce calls (`(+)` is 0): the function and the
/// value; `None` for any other head, and for one the module defines.
fn start(ctx: &ExpandCtx, f: &Form, pos: &Pos) -> Option<(Form, Form)> {
    let head = f.as_sym().filter(|h| !ctx.hides_macro(h))?;
    let literal = |kind| Form::new(kind, pos.clone());
    let init = match head {
        "+" => literal(FormKind::Int {
            v: 0,
            width: IntWidth::I64,
        }),
        "*" => literal(FormKind::Int {
            v: 1,
            width: IntWidth::I64,
        }),
        "str" => return Some((str_step(ctx, pos), string("", pos))),
        "conj" => literal(FormKind::Vec(Vec::new())),
        "concat" => return Some((f.clone(), empty_lseq(pos))),
        "merge" => literal(FormKind::Map(Vec::new())),
        _ => return None,
    };
    Some((f.clone(), init))
}

/// The empty lazy seq: `(fib.seq/lazy-node (fn () fib.seq/LNil))`.
fn empty_lseq(pos: &Pos) -> Form {
    let nil = call("fn", vec![list(Vec::new(), pos), sym(LNIL, pos)], pos);
    call(LAZY_NODE, vec![nil], pos)
}

/// Where the tail sub-forms of a form are.
enum Shape {
    /// None: the form is a leaf tail (a call, a literal, a symbol).
    Leaf,
    /// Every item from this index on (the branches of an `if`).
    From(usize),
    /// The last item (the body of `let`, `do`, `loop`, `when`..).
    Last,
    /// The last item of every clause from this index on (`match`).
    Clauses(usize),
    /// Every second item after this index: the expressions of the pairs of
    /// a test and an expression that start at it (`cond`).
    Flat(usize),
}

fn shape(form: &Form) -> Shape {
    let Some(items) = form.as_list() else {
        return Shape::Leaf;
    };
    let head = items.first().and_then(Form::as_sym).unwrap_or("");
    match (head, items.len()) {
        ("if" | "if-let", 4) => Shape::From(2),
        ("do", 2..) | ("let" | "loop" | "when" | "unless" | "when-let", 3..) => Shape::Last,
        ("match", 3..) => Shape::Clauses(2),
        ("cond", 2..) => Shape::Flat(1),
        _ => Shape::Leaf,
    }
}

/// The tail sub-forms of `form`, which has the shape `shape`.
fn tail_slots(form: &mut Form, shape: Shape) -> Vec<&mut Form> {
    let FormKind::List(items) = &mut form.kind else {
        return Vec::new();
    };
    match shape {
        Shape::Leaf => Vec::new(),
        Shape::From(n) => items.iter_mut().skip(n).collect(),
        Shape::Last => items.last_mut().into_iter().collect(),
        Shape::Flat(n) => items.iter_mut().skip(n + 1).step_by(2).collect(),
        Shape::Clauses(n) => items
            .iter_mut()
            .skip(n)
            .filter_map(|clause| match &mut clause.kind {
                FormKind::List(parts) if parts.len() >= 2 => parts.last_mut(),
                _ => None,
            })
            .collect(),
    }
}

/// What a leaf tail is.
enum Leaf {
    /// `(reduced e)` with one operand.
    Done,
    /// `(reduced)` or `(reduced a b)`: left alone, the checker reports it.
    Misused,
    /// `(recur ..)`: not a value.
    Recur,
    /// Any other value.
    Value,
}

fn leaf_of(form: &Form) -> Leaf {
    match form.as_list() {
        Some([head, _]) if head.as_sym() == Some("reduced") => Leaf::Done,
        Some([head, ..]) if head.as_sym() == Some("reduced") => Leaf::Misused,
        Some([head, ..]) if head.as_sym() == Some("recur") => Leaf::Recur,
        _ => Leaf::Value,
    }
}

/// Rewrites every tail of `body` as the module comment says and returns
/// whether one of them was a `reduced`.
fn rewrite_tails(body: &mut Form, pos: &Pos) -> bool {
    let mut found = false;
    let mut stack = vec![body];
    while let Some(form) = stack.pop() {
        let shape = shape(form);
        if !matches!(shape, Shape::Leaf) {
            stack.extend(tail_slots(form, shape));
            continue;
        }
        let old = std::mem::replace(form, Form::new(FormKind::Nil, pos.clone()));
        *form = match leaf_of(&old) {
            Leaf::Done => {
                found = true;
                let operand = old.as_list().and_then(|i| i.get(1)).cloned();
                call(DONE, operand.into_iter().collect(), pos)
            }
            Leaf::Misused => {
                found = true;
                old
            }
            Leaf::Recur => old,
            Leaf::Value => call(MORE, vec![old], pos),
        };
    }
    found
}

/// The parts of a literal `(fn (acc x) body+)`: no name, two parameters,
/// no `->` or `:where` before the body.
fn literal_fn(form: &Form) -> Option<&[Form]> {
    let items = form.as_list()?;
    if items.len() < 3 || items[0].as_sym() != Some("fn") || seq_items(&items[1])?.len() != 2 {
        return None;
    }
    let annotated = match &items[2].kind {
        FormKind::Sym(s) => s == "->",
        FormKind::Kw(k) => k == "where",
        _ => false,
    };
    (!annotated).then_some(items)
}

/// `f` with its tails rewritten, when it is a literal `fn` that has a
/// `reduced` in a tail position.
fn with_reduced_tails(f: &Form, pos: &Pos) -> Option<Form> {
    literal_fn(f)?;
    let mut rewritten = f.clone();
    let FormKind::List(items) = &mut rewritten.kind else {
        return None;
    };
    let found = rewrite_tails(items.last_mut()?, pos);
    found.then_some(rewritten)
}

/// The three-argument call: the `fn` with its tails rewritten, or a literal
/// `str` as the step.
fn three_arguments(ctx: &ExpandCtx, items: &[Form], pos: &Pos) -> Option<Form> {
    let [_, f, init, c] = items else { return None };
    let f = match with_reduced_tails(f, pos) {
        Some(f) => return Some(call(REDUCE_WHILE, vec![f, init.clone(), c.clone()], pos)),
        None if is_prelude_str(ctx, f) => str_step(ctx, pos),
        None => return None,
    };
    Some(call(REDUCE, vec![f, init.clone(), c.clone()], pos))
}

/// One call of `reduce`: the rewrite, or the call itself, declined.
pub(super) fn reduce(ctx: &ExpandCtx, items: Vec<Form>, pos: Pos) -> Result<Outcome, ExpandError> {
    let expansion = match items.as_slice() {
        [_, f, c] => Some(match start(ctx, f, &pos) {
            Some((f, init)) => call(REDUCE, vec![f, init, c.clone()], &pos),
            None => call(REDUCE_NONEMPTY, vec![f.clone(), c.clone()], &pos),
        }),
        [_, _, _, _] => three_arguments(ctx, &items, &pos),
        _ => None,
    };
    Ok(match expansion {
        Some(form) => Outcome::Expanded(form),
        None => Outcome::Declined(list(items, &pos)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::read_all;

    fn one(src: &str) -> Form {
        let mut forms = read_all(src, "t.fib").unwrap_or_else(|e| panic!("{src:?}: {e}"));
        forms.remove(0)
    }

    fn tails(src: &str) -> (String, bool) {
        let mut form = one(src);
        let pos = form.pos.clone();
        let found = rewrite_tails(&mut form, &pos);
        (form.to_string(), found)
    }

    #[test]
    fn every_tail_through_every_form_is_rewritten() {
        let (text, found) = tails("(if a (reduced b) (let ((x 1)) (do p (match x (1 u) (_ w)))))");
        assert!(found);
        assert_eq!(
            text,
            "(if a (fib.core/Done b) (let ((x 1)) (do p (match x (1 (fib.core/More u)) \
             (_ (fib.core/More w))))))"
        );
    }

    #[test]
    fn a_recur_tail_and_a_non_tail_reduced_are_left_alone() {
        let (text, found) = tails("(loop ((i 0)) (if a (recur (f (reduced i))) b))");
        assert!(!found);
        assert_eq!(
            text,
            "(loop ((i 0)) (if a (recur (f (reduced i))) (fib.core/More b)))"
        );
    }
}
