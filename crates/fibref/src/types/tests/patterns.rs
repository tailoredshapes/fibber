//! Vector patterns and guards (syntax §3.6; types §2.6): typing,
//! lowering errors, and the exhaustiveness matrix with lengths and
//! guarded clauses.

use crate::types::ErrorKind as K;

use super::{binding_type, check, fails, ok};

fn f(clauses: &str) -> String {
    format!("(defun f (v: (Vec i64)) -> i64 (match v {clauses})) (defun main () -> i64 (f []))")
}

#[test]
fn elements_have_the_element_type_and_a_rest_the_vector_type() {
    let p = ok(
        "(defun main () -> i64 (match [\"a\"] ([s & rest] (+ (str-len s) (vec-count rest))) (_ 0)))",
    );
    assert_eq!(binding_type(&p, "s"), "str");
    assert_eq!(binding_type(&p, "rest"), "(Vec str)");
}

#[test]
fn exhaustive_sets_of_lengths() {
    ok(&f("([] 0) ([x & r] x)"));
    ok(&f("([] 0) ([x] x) ([x y & _] y)"));
    ok(&f("([& r] 0)"));
    ok(
        "(defun g (v: (Vec bool)) -> i64 (match v ([] 0) ([true & _] 1) ([false & _] 2)))
        (defun main () -> i64 (g []))",
    );
    ok("(defun g (o: (Option (Vec i64))) -> i64 (match o (nil 0) ((some []) 1) ((some [_ & _]) 2)))
        (defun main () -> i64 (g nil))");
}

#[test]
fn missing_lengths_are_printed_as_vector_patterns() {
    fails(
        &f("([x] x)"),
        K::NonExhaustive,
        "non-exhaustive match: missing []",
    );
    fails(
        &f("([] 0) ([x y] x)"),
        K::NonExhaustive,
        "non-exhaustive match: missing [_]",
    );
    fails(
        &f("([] 0) ([x] x)"),
        K::NonExhaustive,
        "non-exhaustive match: missing [_ _ & _]",
    );
    fails(
        "(defun g (v: (Vec bool)) -> i64 (match v ([] 0) ([true & _] 1)))
         (defun main () -> i64 (g []))",
        K::NonExhaustive,
        "non-exhaustive match: missing [false & _]",
    );
    // With every vector clause guarded the column has no vector pattern
    // left; Vec's own variants are still printed as lengths.
    fails(
        &f("([& _] :when true 0)"),
        K::NonExhaustive,
        "non-exhaustive match: missing []",
    );
}

#[test]
fn guarded_clauses_cover_nothing_and_make_nothing_redundant() {
    fails(
        &f("([] 0) ([x & _] :when (> x 0) 1)"),
        K::NonExhaustive,
        "non-exhaustive match: missing [_ & _]",
    );
    ok(&f("([x & _] :when (> x 0) 1) ([x & _] 2) ([] 0)"));
    fails(
        &f("(_ 0) (x :when true 1)"),
        K::Redundant,
        "redundant match clause",
    );
    fails(
        &f("([& r] 0) ([x] x)"),
        K::Redundant,
        "redundant match clause",
    );
}

#[test]
fn a_guard_must_be_a_bool() {
    fails(&f("(x :when 1 0) (_ 1)"), K::Unify, "cannot unify");
}

#[test]
fn vector_patterns_need_a_vec_and_do_not_mix_with_its_variants() {
    fails(
        "(defun main () -> i64 (match 'x ([a] 1) (_ 0)))",
        K::Unify,
        "cannot unify",
    );
    let es = check(&f("([] 0) ((VecOf n _ _ _) n)")).expect_err("mixed");
    assert!(
        es[0]
            .message
            .contains("vector patterns cannot be mixed with patterns of Vec's variants"),
        "{}",
        es[0]
    );
}

#[test]
fn malformed_vector_patterns_and_guarded_clauses() {
    for (clauses, text) in [
        (
            "([x & ] 0) (_ 1)",
            "& in a vector pattern is followed by one symbol or _",
        ),
        (
            "([x & r s] 0) (_ 1)",
            "& in a vector pattern is followed by one symbol or _",
        ),
        (
            "([x & [y]] 0) (_ 1)",
            "& in a vector pattern is followed by one symbol or _",
        ),
        (
            "([& & r] 0) (_ 1)",
            "& in a vector pattern is followed by one symbol or _",
        ),
        ("([x &r] 0) (_ 1)", "&r reads as (& r)"),
        ("([x & x] 0) (_ 1)", "x is bound twice in one pattern"),
        (
            "(x :when true) (_ 1)",
            "a guarded clause is (pattern :when guard body+)",
        ),
    ] {
        let es = check(&f(clauses)).expect_err(clauses);
        assert!(es[0].message.starts_with(text), "{clauses}: {}", es[0]);
    }
}

const POINT: &str = "(defstruct P (x: i64 y: i64))";

#[test]
fn a_refutable_let_pattern_is_a_match_whose_last_clause_traps() {
    // Was `a let pattern must be irrefutable` (stdlib §7 L8).
    ok("(defun main () -> i64 (let (([& r] [1 2]) ([a & _] r)) a))");
    ok("(defun main () -> i64 (let (([a b] [1 2 3]) ((some c) (some 4))) (+ a (+ b c))))");
    ok("(defun main () -> i64 (let ((a 1) ([b] [2]) (c 3)) (+ a (+ b c))))");
    // An irrefutable struct pattern stays a let: it has no trap clause for
    // the redundancy check to meet...
    ok(&format!(
        "{POINT} (defun main () -> i64 (let (((P x y) (P 1 2))) (+ x y)))"
    ));
    // ...which is the checker's own: one the program writes is redundant.
    fails(
        &format!("{POINT} (defun main () -> i64 (match (P 1 2) ((P x _) x) (_ 0)))"),
        K::Redundant,
        "redundant match clause",
    );
}

#[test]
fn a_vector_pattern_of_a_pair_or_a_triple_takes_its_fields_exactly() {
    let p = ok("(defun main () -> i64 (match (Pair 1 \"s\") ([a b] (+ a (str-len b)))))");
    assert_eq!(binding_type(&p, "b"), "str");
    ok("(defun main () -> i64 (match (Triple 1 2 3) ([a b c] (+ a (+ b c)))))");
    ok("(defun main () -> i64 (let (([a b] (Pair 1 2))) (+ a b)))");
    let text = "a vector pattern of a Pair takes exactly its 2 field(s) and no rest";
    for pat in ["[a]", "[a b c]", "[a & r]"] {
        let src = format!("(defun main () -> i64 (match (Pair 1 2) ({pat} 1)))");
        fails(&src, K::Other, text);
    }
    fails(
        "(defun main () -> i64 (match (Pair 1 2) ([1 b] b)))",
        K::NonExhaustive,
        "non-exhaustive match: missing (Pair _ _)",
    );
}

#[test]
fn a_vector_pattern_on_a_type_not_known_yet_waits_for_it() {
    // The literal is checked before its type is known from the call.
    let p = ok(
        "(defun fold (f: (fn (i64 (Pair i64 str)) i64) p: (Pair i64 str)) -> i64 (f 0 p))
        (defun main () -> i64 (fold (fn (acc [k v]) (+ acc (+ k (str-len v)))) (Pair 3 \"ab\")))",
    );
    assert_eq!(binding_type(&p, "v"), "str");
    // Never known: a vector's pattern, as before.
    let p = ok("(defun main () -> i64 (let ((f (fn (xs) (match xs ([a b] a) (_ 0))))) (f [1 2])))");
    assert_eq!(binding_type(&p, "a"), "i64");
}
