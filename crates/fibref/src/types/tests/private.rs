//! `:private` and `var` (syntax §5, §3.20; owner's decision of
//! 2026-09-28).

use crate::types::ErrorKind as K;

use super::{fails, ok};

#[test]
fn a_private_prelude_name_is_refused_unqualified_and_qualified() {
    fails(
        "(defun main () -> i64 (array-len (array-push (array 1 1) 2)))",
        K::Resolve,
        "array-push is private to fib.prelude; it is not exported",
    );
    fails(
        "(defun main () -> i64 (array-len (fib.prelude/array-push (array 1 1) 2)))",
        K::Resolve,
        "fib.prelude/array-push is private to fib.prelude",
    );
}

#[test]
fn a_private_type_and_its_variants_are_refused() {
    fails(
        "(defun f (n: (VNode i64)) -> i64 1) (defun main () -> i64 0)",
        K::Resolve,
        "VNode is private to fib.prelude",
    );
    fails(
        "(defun main () -> i64 (do (VLeaf (array 1 1)) 0))",
        K::Resolve,
        "VLeaf is private to fib.prelude",
    );
}

#[test]
fn a_private_name_does_not_clash_with_the_programs_own() {
    ok("(defenum VNode (A) (B))
        (defun vnode-get (x: i64) -> i64 x)
        (defun main () -> i64 (match A ((A) (vnode-get 1)) ((B) 2)))");
}

#[test]
fn the_programs_own_private_definitions_are_visible_to_it() {
    ok("(defprotocol Area :private (area (self) -> i64))
        (defstruct Sq :private (s: i64))
        (impl Area Sq (area (self) (* (. self s) (. self s))))
        (def unit :private (Sq 1))
        (defun twice :private (x: i64) -> i64 (* 2 x))
        (defun main () -> i64 (twice (+ (area unit) (area (Sq 3)))))");
}

#[test]
fn var_reaches_a_private_definition_and_ignores_locals() {
    ok("(defun main () -> i64
          (let ((array-push 1))
            (array-len ((var fib.prelude/array-push) (array array-push 0) 5))))");
    ok("(defun main () -> i64 (array-len ((var array-push) (array 1 0) 5)))");
    fails(
        "(defun main () -> i64 (let ((x 1)) (var x)))",
        K::Resolve,
        "var: no definition named x",
    );
}

#[test]
fn a_private_struct_is_parsed_derived_and_used_in_its_module() {
    ok("(defstruct P :private (x: i64))
        (derive Eq P)
        (defun main () -> i64 (if (= (P 1) (P 1)) 1 0))");
}
