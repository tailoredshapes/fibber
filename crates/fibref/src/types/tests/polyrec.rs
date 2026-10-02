//! Polymorphic recursion that would never end (types §3.6, stdlib design
//! §7 B4): a function that calls itself, or a member of its SCC, at a
//! type built from its own variable is an error; recursion at other types
//! that stay finite is not.

use super::{fails, ok, ErrorKind as K};

/// `len` over anything with a size, recursing at a wrapper of its own
/// variable: the stdlib design's `(len (drop 1 xs))`.
const DROPPED: &str = "(defstruct (Dropped c) (inner: c n: i64))
    (defprotocol Sz (size (self) -> i64))
    (impl Sz (Vec a) (size (self) (vec-count self)))
    (impl Sz (Dropped c) :where ((Sz c)) (size (self) (- (size (. self inner)) (. self n))))";

#[test]
fn a_call_at_a_wrapper_of_its_own_variable_is_rejected_naming_the_function_and_the_type() {
    let e = fails(
        &format!(
            "{DROPPED}
             (defun len (xs: c) :where ((Sz c)) -> i64
               (if (= (size xs) 0) 0 (+ 1 (len (Dropped xs 1)))))
             (defun main () -> i64 (len [1 2 3]))"
        ),
        K::Other,
        "len recurses at (Dropped c): polymorphic recursion is not supported; use loop or a List",
    );
    assert_eq!((e.pos.line, e.pos.col), (6, 44));
}

#[test]
fn the_rule_does_not_need_a_bound() {
    fails(
        "(defun f (x: a n: i64) -> i64 (if (= n 0) 0 (f [x] (- n 1))))
         (defun main () -> i64 (f 1 3))",
        K::Other,
        "f recurses at (Vec a): polymorphic recursion is not supported",
    );
}

#[test]
fn growth_through_a_second_member_is_rejected_and_names_both() {
    fails(
        "(defun f (x: a n: i64) -> i64 (if (= n 0) 0 (g x n)))
         (defun g (y: b n: i64) -> i64 (f [y] (- n 1)))
         (defun main () -> i64 (f 1 3))",
        K::Other,
        "g recurses through f at (Vec b): polymorphic recursion is not supported",
    );
}

#[test]
fn recursion_at_the_same_types_or_a_permutation_of_them_is_finite_and_accepted() {
    ok("(defun f (x: a n: i64) -> i64 (if (= n 0) 0 (f x (- n 1))))
        (defun main () -> i64 (f 1 3))");
    ok(
        "(defun f (x: a y: b n: i64) -> i64 (if (= n 0) 0 (f y x (- n 1))))
        (defun main () -> i64 (f 1 \"s\" 3))",
    );
}

#[test]
fn recursion_at_a_type_no_variable_of_the_function_is_in_is_accepted() {
    ok("(defun f (x: a n: i64) :where ((Ord a)) -> bool
          (if (= n 0) (= x x) (f 1 (- n 1))))
        (defun main () -> i64 (if (f true 3) 1 0))");
}

#[test]
fn a_growing_edge_on_no_cycle_is_finite_and_accepted() {
    // a is called at (Vec b) and b at b: the instances are (a, b), then
    // ((Vec b), b) again, which is the second one forever.
    ok(
        "(defun f (x: a y: b n: i64) -> i64 (if (= n 0) 0 (f [y] y (- n 1))))
        (defun main () -> i64 (f 1 2 3))",
    );
}

#[test]
fn a_function_that_is_not_fully_annotated_cannot_recurse_polymorphically_at_all() {
    // The unannotated parameter makes f monomorphic inside its own body,
    // so the occurrence at (Vec a) is a unification failure, as before.
    fails(
        "(defun f (x n: i64) -> i64 (if (= n 0) 0 (f [x] (- n 1))))
         (defun main () -> i64 (f 1 3))",
        K::Infinite,
        "cannot construct the infinite type",
    );
}
