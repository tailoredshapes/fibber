//! §1.3: built forms carry the macro call's position, input forms keep
//! their own.

use super::one;
use crate::expand::{expand_expr, ExpandCtx, NoRunner};
use crate::syntax::Form;

fn expand(src: &str) -> Form {
    expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner).unwrap_or_else(|e| panic!("{e}"))
}

fn lc(f: &Form) -> (usize, usize) {
    (f.pos.line, f.pos.col)
}

#[test]
fn macro_built_forms_take_the_call_position() {
    let f = expand("(g\n  (when c\n    x))");
    let when = &f.as_list().unwrap_or(&[])[1];
    let items = when.as_list().unwrap_or(&[]);
    assert_eq!(lc(when), (2, 3));
    assert_eq!(lc(&items[0]), (2, 3), "the built `if`");
    assert_eq!(lc(&items[1]), (2, 9), "the test keeps its own");
    assert_eq!(lc(&items[2]), (3, 5), "the body keeps its own");
    assert_eq!(lc(&items[3]), (2, 3), "the built ()");
}

#[test]
fn nested_expansions_keep_the_inner_call_position() {
    // (and a b c) -> (if a (and b c) false): the inner `and` is built at
    // the outer call, and its own expansion takes that position.
    let f = expand("(and a\n b c)");
    let items = f.as_list().unwrap_or(&[]);
    assert_eq!(lc(&items[1]), (1, 6));
    assert_eq!(lc(&items[2]), (1, 1));
    let inner = items[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&inner[1]), (2, 2));
}

#[test]
fn collection_rewrite_takes_the_literal_position() {
    let f = expand("(f\n  [x])");
    let conj = &f.as_list().unwrap_or(&[])[1];
    let items = conj.as_list().unwrap_or(&[]);
    assert_eq!(lc(conj), (2, 3));
    assert_eq!(lc(&items[1]), (2, 3), "(vec-empty)");
    assert_eq!(lc(&items[2]), (2, 4), "the element keeps its own");
}

#[test]
fn threading_keeps_the_step_position() {
    let f = expand("(-> x\n  (f a))");
    assert_eq!(lc(&f), (2, 3));
    assert_eq!(lc(&f.as_list().unwrap_or(&[])[1]), (1, 5));
}
