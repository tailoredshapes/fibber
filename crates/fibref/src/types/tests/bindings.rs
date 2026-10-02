//! Annotated `let`, `loop` and `plet` bindings (syntax §1.5, §3.3,
//! §3.12, §3.18): `(x: T expr)` types `x` as `T`, the initialiser's type
//! flowing into it as an argument's flows into an annotated parameter.

use crate::types::ErrorKind as K;

use super::{binding_type, fails, ok};

#[test]
fn an_annotated_let_binding_has_its_annotation() {
    let p = ok("(defun main () -> i64 (let ((v: (Vec str) []) (n: i64 (vec-count v))) n))");
    assert_eq!(binding_type(&p, "v"), "(Vec str)");
    assert_eq!(binding_type(&p, "n"), "i64");
    fails(
        "(defun main () -> i64 (let ((x: i64 \"s\")) x))",
        K::Unify,
        "cannot unify",
    );
}

#[test]
fn an_annotated_loop_variable_has_its_annotation() {
    let p = ok("(defun main () -> i64
                  (loop ((i: i64 0) (acc: (Vec i8) []))
                    (if (< i 2) (recur (+ i 1) acc) (vec-count acc))))");
    assert_eq!(binding_type(&p, "acc"), "(Vec i8)");
    fails(
        "(defun main () -> i64 (loop ((i: str 0)) 0))",
        K::Unify,
        "cannot unify",
    );
}

#[test]
fn an_annotated_plet_binding_has_its_annotation() {
    let p = ok("(defun main () -> i64 (plet ((a: (Vec i16) []) (b 1)) (+ b (vec-count a))))");
    assert_eq!(binding_type(&p, "a"), "(Vec i16)");
}

#[test]
fn only_a_variable_is_annotated() {
    fails(
        "(defun main () -> i64 (let ((_: i64 3)) 0))",
        K::Resolve,
        "only a variable can be annotated in a binding",
    );
}

#[test]
fn a_binding_of_three_items_needs_an_annotated_name() {
    let src = "(defun main () -> i64 (let ((x i64 3)) 0))";
    let e = crate::types::check_source(src, "t.fib").expect_err("malformed");
    let text = format!("{e:?}");
    assert!(
        text.contains("a binding is (pattern expression) or (name: type expression)"),
        "{text}"
    );
}
