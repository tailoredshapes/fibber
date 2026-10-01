//! `reduce` (stdlib design §2.1 rule 3 and the rows `reduce`, `reduced`,
//! tranche 1 R6b, PC-6): the 33rd Rust macro, one that declines by
//! default. The library function `fib.seq/reduce` is the three-argument
//! fold `(reduce f init c)` with `f: (fn (a e) a)`; the macro answers the
//! two calls the function cannot:
//!
//! ```text
//! (reduce f c)        ⟹ (fib.seq/reduce-nonempty f c)
//! (reduce + c)        ⟹ (fib.seq/reduce + 0 c)        ; * 1, str "", conj [], merge {}, concat []
//! (reduce (fn (a x) .. (reduced e) ..) init c)
//!                     ⟹ (fib.seq/reduce-while (fn (a x) ..) init c)
//! ```
//!
//! In the last form every tail position of the literal `fn`'s body is
//! rewritten, through `if`, `let`, `do`, `match`, `cond`, `when`,
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
use crate::expand::build::{call, list, string};
use crate::expand::error::ExpandError;

const REDUCE: &str = "fib.seq/reduce";
const REDUCE_NONEMPTY: &str = "fib.seq/reduce-nonempty";
const REDUCE_WHILE: &str = "fib.seq/reduce-while";
const MORE: &str = "fib.core/More";
const DONE: &str = "fib.core/Done";

/// The identity of the literal heads whose `(f)` Clojure's reduce calls
/// (`(+)` is 0): the value `(reduce f c)` starts from.
fn identity(f: &Form, pos: &Pos) -> Option<Form> {
    let kind = match f.as_sym()? {
        "+" => FormKind::Int {
            v: 0,
            width: IntWidth::I64,
        },
        "*" => FormKind::Int {
            v: 1,
            width: IntWidth::I64,
        },
        "str" => return Some(string("", pos)),
        "conj" | "concat" => FormKind::Vec(Vec::new()),
        "merge" => FormKind::Map(Vec::new()),
        _ => return None,
    };
    Some(Form::new(kind, pos.clone()))
}

/// Where the tail sub-forms of a form are.
enum Shape {
    /// None: the form is a leaf tail (a call, a literal, a symbol).
    Leaf,
    /// Every item from this index on (the branches of an `if`).
    From(usize),
    /// The last item (the body of `let`, `do`, `loop`, `when`..).
    Last,
    /// The last item of every clause from this index on (`match`, `cond`).
    Clauses(usize),
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
        ("cond", 2..) => Shape::Clauses(1),
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
    if items.len() < 3 || items[0].as_sym() != Some("fn") || items[1].as_list()?.len() != 2 {
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

/// One call of `reduce`: the rewrite, or the call itself, declined.
pub(super) fn reduce(items: Vec<Form>, pos: Pos) -> Result<Outcome, ExpandError> {
    let expansion = match items.as_slice() {
        [_, f, c] => Some(match identity(f, &pos) {
            Some(init) => call(REDUCE, vec![f.clone(), init, c.clone()], &pos),
            None => call(REDUCE_NONEMPTY, vec![f.clone(), c.clone()], &pos),
        }),
        [_, f, init, c] => with_reduced_tails(f, &pos)
            .map(|f| call(REDUCE_WHILE, vec![f, init.clone(), c.clone()], &pos)),
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
