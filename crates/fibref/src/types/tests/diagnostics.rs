//! The two diagnostics of stdlib §7 D1 that are the checker's: a test that
//! can never be false, and the stand-in a library function has for the
//! arity a call wrote.

use crate::types::ErrorKind as K;

use super::{fails, fails_lib, ok};

/// The message of a program that must fail with a mismatch.
fn unify(src: &str, text: &str) {
    fails(src, K::Unify, text);
}

#[test]
fn a_primitive_that_is_not_a_bool_is_never_false() {
    for (ty, value) in [
        ("i64", "1"),
        ("i32", "1i32"),
        ("i8", "1i8"),
        ("f64", "1.5"),
        ("f32", "1.5f32"),
        ("char", "\\a"),
        ("str", "\"x\""),
    ] {
        let src = format!("(defun main () -> i64 (if {value} 2 3))");
        let text = format!("a value of type {ty} is always true; write the test");
        let e = fails(&src, K::Unify, &text);
        assert_eq!(e.message, text, "{src}");
        assert_eq!(
            (e.pos.line, e.pos.col),
            (1, 27),
            "the position of the test: {src}"
        );
    }
}

#[test]
fn the_forms_that_expand_to_if_say_it_too() {
    let always = "a value of type i64 is always true; write the test";
    for body in [
        "(when 1 2)",
        "(unless 1 2)",
        "(do (while 1 (set! c 1)) 0)",
        "(cond 1 2 :else 3)",
        "(and 1 true)",
        "(or 1 true)",
        "(if (if true 1 2) 3 4)",
    ] {
        let src = format!("(defun main () -> i64 (let ((c (cell 0))) (do {body} 0)))");
        let e = fails(&src, K::Unify, always);
        assert_eq!(e.message, always, "{src}");
    }
}

#[test]
fn a_test_of_any_other_type_is_the_plain_mismatch() {
    // `unit` has no use as a test and no Clojure habit behind it;
    // an Option is the case stdlib §7 L20 is about, and says what it did.
    unify(
        "(defun main () -> i64 (if (do) 2 3))",
        "cannot unify unit with bool",
    );
    // (an Option is a test since L20: case 900)
    unify(
        "(defun main () -> i64 (if [1] 2 3))",
        "cannot unify (Vec i64) with bool",
    );
    // a raw pointer is a primitive that is not a bool too, but the
    // Clojure habit is not to test one: the plain mismatch.
    unify(
        "(defun main () -> i64 (unsafe (if (alloc 8) 2 3)))",
        "cannot unify ptr with bool",
    );
}

#[test]
fn a_bool_test_and_a_test_that_becomes_a_bool_are_accepted() {
    ok("(defun main () -> i64 (if (< 1 2) 3 4))");
    ok("(defun f (x) (if x 1 2)) (defun main () -> i64 (f true))");
    ok("(defun main () -> i64 (do (when true (do)) 0))");
}

#[test]
fn a_library_functions_arity_error_has_no_hint() {
    // D1's hints went with the stand-ins (L1).
    for (src, text) in [
        (
            "(defun main () -> i64 (get (map-empty) 1 0))",
            "get takes 2 argument(s), got 3",
        ),
        (
            "(defun main () -> i64 (nth [1 2] 1 0))",
            "nth takes 2 argument(s), got 3",
        ),
        (
            "(defun main () -> i64 (range 0 10 2))",
            "range takes 1 argument(s), got 3",
        ),
    ] {
        let e = fails_lib(src, K::Other, text);
        assert_eq!(e.message, text, "{src}");
    }
}

#[test]
fn a_programs_own_function_gets_the_plain_arity_error() {
    let e = fails(
        "(defun get (a: i64 b: i64) -> i64 (+ a b))
         (defun main () -> i64 (get 1 2 3))",
        K::Other,
        "get takes 2 argument(s), got 3",
    );
    assert_eq!(e.message, "get takes 2 argument(s), got 3");
}

#[test]
fn a_programs_own_constructor_gets_the_plain_arity_error() {
    for src in [
        "(defstruct nth (a: i64)) (defun main () -> i64 (do (nth 1 2 3) 0))",
        "(defenum E (nth a: i64)) (defun main () -> i64 (do (nth 1 2 3) 0))",
    ] {
        let e = fails(src, K::Other, "nth takes 1 argument(s), got 3");
        assert_eq!(e.message, "nth takes 1 argument(s), got 3", "{src}");
    }
}
