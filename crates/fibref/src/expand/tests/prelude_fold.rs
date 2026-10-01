//! The variadic folds of the operators (stdlib design §4.1, §4.3, §6.3,
//! tranche 1 R6a): `+ - * < > <= >= =`, `max min`, `bit-and bit-or
//! bit-xor`. Each expansion as printed, each call the macro declines (the
//! binary call, which the builtin or the library function serves), each
//! error, and the positions of what the macro builds.

use super::{ex, ex_err, one};
use crate::expand::{expand_expr, ExpandCtx, ExpandErrorKind, NoRunner};
use crate::syntax::Form;

#[test]
fn plus_and_times_of_nothing_are_their_identities() {
    assert_eq!(ex("(+)"), "0");
    assert_eq!(ex("(*)"), "1");
}

#[test]
fn plus_and_times_of_one_argument_are_the_argument() {
    assert_eq!(ex("(+ a)"), "a");
    assert_eq!(ex("(* (f a))"), "(f a)");
}

#[test]
fn the_binary_call_is_declined() {
    for src in [
        "(+ a b)",
        "(- a b)",
        "(* a b)",
        "(< a b)",
        "(> a b)",
        "(<= a b)",
        "(>= a b)",
        "(= a b)",
        "(max a b)",
        "(min a b)",
        "(bit-and a b)",
        "(bit-or a b)",
        "(bit-xor a b)",
    ] {
        assert_eq!(ex(src), src, "{src}");
    }
}

#[test]
fn plus_and_times_fold_from_the_left() {
    assert_eq!(ex("(+ a b c)"), "(fib.prelude/+ (fib.prelude/+ a b) c)");
    assert_eq!(
        ex("(* a b c d)"),
        "(fib.prelude/* (fib.prelude/* (fib.prelude/* a b) c) d)"
    );
}

#[test]
fn minus_of_one_argument_is_neg_and_more_fold_from_the_left() {
    assert_eq!(ex("(- a)"), "(fib.prelude/neg a)");
    assert_eq!(ex("(- a b c)"), "(fib.prelude/- (fib.prelude/- a b) c)");
}

#[test]
fn minus_of_nothing_is_an_arity_error() {
    let e = ex_err("(-)");
    assert_eq!(
        e.kind,
        ExpandErrorKind::MacroArity {
            name: "-".to_string(),
            expected: "at least 1".to_string(),
            found: 0,
        }
    );
    assert_eq!(
        e.kind.to_string(),
        "macro - takes at least 1 argument(s), got 0"
    );
}

#[test]
fn the_error_is_at_the_position_of_the_call() {
    let e = ex_err("(g\n  (-))");
    assert_eq!((e.pos.line, e.pos.col), (2, 3));
}

#[test]
fn a_comparison_of_one_argument_evaluates_it_and_is_true() {
    assert_eq!(ex("(< a)"), "(let ((#cmp.1 a)) true)");
    assert_eq!(ex("(= (f a))"), "(let ((#cmp.1 (f a))) true)");
}

#[test]
fn a_comparison_of_nothing_is_declined() {
    assert_eq!(ex("(<)"), "(<)");
    assert_eq!(ex("(=)"), "(=)");
}

#[test]
fn a_chain_of_simple_operands_is_an_and_of_the_pairs() {
    assert_eq!(
        ex("(< a b c)"),
        "(if (fib.prelude/< a b) (fib.prelude/< b c) false)"
    );
    assert_eq!(
        ex("(<= 1 x 10 y)"),
        "(if (fib.prelude/<= 1 x) (if (fib.prelude/<= x 10) (fib.prelude/<= 10 y) false) false)"
    );
    for op in [">", ">=", "="] {
        assert_eq!(
            ex(&format!("({op} a b c)")),
            format!("(if (fib.prelude/{op} a b) (fib.prelude/{op} b c) false)")
        );
    }
}

#[test]
fn a_field_path_is_simple_and_a_call_is_bound_once_in_order() {
    assert_eq!(
        ex("(= (. p n) (. q n) r)"),
        "(if (fib.prelude/= (. p n) (. q n)) (fib.prelude/= (. q n) r) false)"
    );
    assert_eq!(
        ex("(< a (f x) c (g y))"),
        "(let ((#cmp.1 (f x)) (#cmp.2 (g y))) (if (fib.prelude/< a #cmp.1) \
         (if (fib.prelude/< #cmp.1 c) (fib.prelude/< c #cmp.2) false) false))"
    );
}

#[test]
fn a_literal_collection_and_a_call_inside_a_path_are_not_simple() {
    assert_eq!(
        ex("(= [1] a b)"),
        "(let ((#cmp.1 (fib.prelude/vec-conj (fib.prelude/vec-empty) 1))) \
         (if (fib.prelude/= #cmp.1 a) (fib.prelude/= a b) false))"
    );
    assert_eq!(
        ex("(< (. (f x) n) a b)"),
        "(let ((#cmp.1 (. (f x) n))) (if (fib.prelude/< #cmp.1 a) (fib.prelude/< a b) false))"
    );
}

#[test]
fn the_operands_of_every_form_are_expanded() {
    assert_eq!(
        ex("(+ (+ a b c) (when p q) 1)"),
        "(fib.prelude/+ (fib.prelude/+ (fib.prelude/+ (fib.prelude/+ a b) c) (if p q ())) 1)"
    );
    assert_eq!(
        ex("(< (+ a b c) x y)"),
        "(let ((#cmp.1 (fib.prelude/+ (fib.prelude/+ a b) c))) \
         (if (fib.prelude/< #cmp.1 x) (fib.prelude/< x y) false))"
    );
}

#[test]
fn max_min_and_the_bit_operations_fold_from_three_arguments() {
    assert_eq!(ex("(max a b c)"), "(fib.core/max (fib.core/max a b) c)");
    assert_eq!(
        ex("(min a b c d)"),
        "(fib.core/min (fib.core/min (fib.core/min a b) c) d)"
    );
    assert_eq!(
        ex("(bit-and a b c)"),
        "(fib.prelude/bit-and (fib.prelude/bit-and a b) c)"
    );
    assert_eq!(
        ex("(bit-or a b c)"),
        "(fib.prelude/bit-or (fib.prelude/bit-or a b) c)"
    );
    assert_eq!(
        ex("(bit-xor a b c)"),
        "(fib.prelude/bit-xor (fib.prelude/bit-xor a b) c)"
    );
}

#[test]
fn max_min_and_the_bit_operations_of_fewer_than_two_are_declined() {
    for src in [
        "(max)",
        "(max a)",
        "(min a)",
        "(bit-and)",
        "(bit-or a)",
        "(bit-xor)",
    ] {
        assert_eq!(ex(src), src, "{src}");
    }
}

#[test]
fn the_names_are_values_when_they_are_not_in_head_position() {
    assert_eq!(ex("(reduce + 0 xs)"), "(reduce + 0 xs)");
    assert_eq!(ex("(f < max =)"), "(f < max =)");
}

fn expand(src: &str) -> Form {
    expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner).unwrap_or_else(|e| panic!("{e}"))
}

fn lc(f: &Form) -> (usize, usize) {
    (f.pos.line, f.pos.col)
}

#[test]
fn built_forms_take_the_call_position_and_arguments_keep_their_own() {
    // (g\n  (< a (f x)\n    c)) : the chain is at 2:3
    let f = expand("(g\n  (< a (f x)\n    c))");
    let chain = &f.as_list().unwrap_or(&[])[1];
    assert_eq!(lc(chain), (2, 3), "the built let");
    let parts = chain.as_list().unwrap_or(&[]);
    let binding = parts[1].as_list().unwrap_or(&[])[0]
        .as_list()
        .unwrap_or(&[]);
    assert_eq!(lc(&parts[1]), (2, 3), "the built binding list");
    assert_eq!(lc(&binding[0]), (2, 3), "the gensym");
    assert_eq!(lc(&binding[1]), (2, 8), "the argument keeps its own");
    let test = parts[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&parts[2]), (2, 3), "the built if");
    let first = test[1].as_list().unwrap_or(&[]);
    assert_eq!(lc(&first[0]), (2, 3), "the built fib.prelude/<");
    assert_eq!(lc(&first[1]), (2, 6), "a keeps its own");
    let rest = test[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&rest[2]), (3, 5), "c keeps its own");
}

#[test]
fn a_fold_takes_the_call_position() {
    let f = expand("(g\n  (max a\n    b c))");
    let fold = f.as_list().unwrap_or(&[])[1].as_list().unwrap_or(&[]);
    assert_eq!(lc(&fold[0]), (2, 3));
    let inner = fold[1].as_list().unwrap_or(&[]);
    assert_eq!(lc(&fold[1]), (2, 3));
    assert_eq!(lc(&inner[1]), (2, 8));
    assert_eq!(lc(&fold[2]), (3, 7));
}
