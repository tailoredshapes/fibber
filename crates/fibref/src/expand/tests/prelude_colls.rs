//! The variadic collection functions and `swap!` (stdlib design §4.1,
//! §4.5, §6.3, tranche 1 R6a): `conj assoc dissoc merge` and `swap!`.
//! Each expansion as printed, each call the macro declines, each error.

use super::{ex, ex_err};
use crate::expand::ExpandErrorKind;

#[test]
fn conj_of_one_argument_is_the_collection_and_more_fold_from_the_left() {
    assert_eq!(ex("(conj c)"), "c");
    assert_eq!(ex("(conj c x y)"), "(fib.coll/conj (fib.coll/conj c x) y)");
    assert_eq!(
        ex("(conj c x y z)"),
        "(fib.coll/conj (fib.coll/conj (fib.coll/conj c x) y) z)"
    );
}

#[test]
fn conj_of_two_arguments_or_none_is_declined() {
    assert_eq!(ex("(conj c x)"), "(conj c x)");
    assert_eq!(ex("(conj)"), "(conj)");
}

#[test]
fn assoc_folds_the_pairs_from_the_left() {
    assert_eq!(
        ex("(assoc m a 1 b 2)"),
        "(fib.coll/assoc (fib.coll/assoc m a 1) b 2)"
    );
    assert_eq!(
        ex("(assoc m a 1 b 2 c 3)"),
        "(fib.coll/assoc (fib.coll/assoc (fib.coll/assoc m a 1) b 2) c 3)"
    );
}

#[test]
fn assoc_of_one_pair_or_no_pair_is_declined() {
    assert_eq!(ex("(assoc m k v)"), "(assoc m k v)");
    assert_eq!(ex("(assoc m)"), "(assoc m)");
    assert_eq!(ex("(assoc)"), "(assoc)");
}

#[test]
fn assoc_with_a_key_without_a_value_is_an_error() {
    for src in ["(assoc m k)", "(assoc m a 1 b)", "(assoc m a 1 b 2 c)"] {
        let e = ex_err(src);
        assert_eq!(
            e.kind,
            ExpandErrorKind::Malformed {
                head: "assoc".to_string(),
                reason: "a key without a value",
            },
            "{src}"
        );
    }
    assert_eq!(
        ex_err("(assoc m k)").kind.to_string(),
        "malformed assoc: a key without a value"
    );
}

#[test]
fn the_errors_are_at_the_position_of_the_call() {
    for src in ["(g\n  (assoc m k))", "(g\n  (merge))"] {
        let e = ex_err(src);
        assert_eq!((e.pos.line, e.pos.col), (2, 3), "{src}");
    }
}

#[test]
fn dissoc_of_one_argument_is_the_map_and_more_fold_from_the_left() {
    assert_eq!(ex("(dissoc m)"), "m");
    assert_eq!(
        ex("(dissoc m a b)"),
        "(fib.coll/dissoc (fib.coll/dissoc m a) b)"
    );
    assert_eq!(ex("(dissoc m a)"), "(dissoc m a)");
    assert_eq!(ex("(dissoc)"), "(dissoc)");
}

#[test]
fn merge_of_nothing_is_an_error() {
    let e = ex_err("(merge)");
    assert_eq!(
        e.kind,
        ExpandErrorKind::Malformed {
            head: "merge".to_string(),
            reason: "needs at least one argument",
        }
    );
    assert_eq!(
        e.kind.to_string(),
        "malformed merge: needs at least one argument"
    );
}

#[test]
fn merge_of_one_argument_is_the_argument_and_two_are_declined() {
    assert_eq!(ex("(merge a)"), "a");
    assert_eq!(ex("(merge a b)"), "(merge a b)");
}

#[test]
fn merge_folds_three_or_more_from_the_left() {
    assert_eq!(
        ex("(merge a b c)"),
        "(fib.coll/merge (fib.coll/merge a b) c)"
    );
    assert_eq!(
        ex("(merge a b c d)"),
        "(fib.coll/merge (fib.coll/merge (fib.coll/merge a b) c) d)"
    );
}

#[test]
fn merge_skips_a_literal_nil_operand() {
    assert_eq!(ex("(merge a nil b)"), "(fib.coll/merge a b)");
    assert_eq!(ex("(merge nil a)"), "a");
    assert_eq!(ex("(merge a nil)"), "a");
    assert_eq!(ex("(merge nil a nil b nil)"), "(fib.coll/merge a b)");
    assert_eq!(ex("(merge nil)"), "nil");
    assert_eq!(ex("(merge nil nil)"), "nil");
}

#[test]
fn the_operands_are_expanded() {
    assert_eq!(
        ex("(conj (when a b) (and c d) e)"),
        "(fib.coll/conj (fib.coll/conj (if a b) (and c d)) e)"
    );
}

#[test]
fn swap_with_extra_arguments_goes_through_a_closure_over_the_value() {
    assert_eq!(
        ex("(swap! a + 5)"),
        "(fib.prelude/swap! a (fn (#v.1) (+ #v.1 5)))"
    );
    assert_eq!(
        ex("(swap! a f x y)"),
        "(fib.prelude/swap! a (fn (#v.1) (f #v.1 x y)))"
    );
}

#[test]
fn swap_with_a_computed_function_calls_it() {
    assert_eq!(
        ex("(swap! a (comp f g) x)"),
        "(fib.prelude/swap! a (fn (#v.1) ((comp f g) #v.1 x)))"
    );
    assert_eq!(
        ex("(swap! a (fn (o) (g o)) x)"),
        "(fib.prelude/swap! a (fn (#v.1) ((fn (o) (g o)) #v.1 x)))"
    );
}

#[test]
fn swap_of_two_arguments_or_fewer_is_declined() {
    assert_eq!(ex("(swap! a f)"), "(swap! a f)");
    assert_eq!(ex("(swap! a)"), "(swap! a)");
    assert_eq!(ex("(swap!)"), "(swap!)");
}

#[test]
fn the_extra_arguments_are_expanded_inside_the_closure() {
    assert_eq!(
        ex("(swap! a f (when p q))"),
        "(fib.prelude/swap! a (fn (#v.1) (f #v.1 (if p q))))"
    );
}
