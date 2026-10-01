//! One test per type error of the catalogue of types §6.14 (the
//! ownership errors are the next pass's): threads, `&`, fields, `deref`,
//! instances, ambiguity, unification.

use crate::types::ErrorKind as K;

use super::{fails, ok};

#[test]
fn cell_cannot_be_shared_between_threads() {
    let e = fails(
        "(defun main () -> i64 (let ((n (cell 0))) (join (spawn (fn () @n)))))",
        K::CellNotSend,
        "cell cannot be shared between threads",
    );
    assert_eq!(
        e.message,
        "cell cannot be shared between threads: closure capture n has type (Cell i64)"
    );
}

#[test]
fn value_cannot_be_shared_between_threads() {
    // A closure read from a struct field is `local` (§1.4: an omitted
    // colour on a field); capturing it makes the thunk local too.
    let e = fails(
        "(defstruct H (f: (fn () i64)))
         (defun main () -> i64
           (let ((h (H (fn () 1))) (g (. h f)))
             (join (spawn (fn () (g))))))",
        K::ValueNotSend,
        "value of type (fn :local () i64) cannot be shared between threads",
    );
    assert_eq!(
        e.message,
        "value of type (fn :local () i64) cannot be shared between threads: closure capture g"
    );
    fails(
        "(defun main () -> i64
           (unsafe (let ((p (alloc 8))) (join (spawn (fn () (do (free p) 0)))))))",
        K::ValueNotSend,
        "value of type ptr cannot be shared between threads: closure capture p",
    );
}

#[test]
fn amp_argument_must_be_a_cell_variable() {
    let head = "(defun bump (&v) (set! v (+ @v 1)))";
    fails(
        &format!("{head} (defun main () -> i64 (let ((x 5)) (do (bump &x) x)))"),
        K::AmpArgument,
        "& argument must be a cell variable",
    );
    fails(
        &format!("{head} (defun main () -> i64 (do (bump &main) 0))"),
        K::AmpArgument,
        "& argument must be a cell variable",
    );
}

#[test]
fn amp_parameter_used_as_a_value() {
    fails(
        "(defun leak (&v) v) (defun main () -> i64 0)",
        K::AmpParamValue,
        "& parameter v used as a value in leak",
    );
    fails(
        "(defun leak (&v) (fn () (cell v))) (defun main () -> i64 0)",
        K::AmpParamValue,
        "& parameter v used as a value in leak",
    );
}

#[test]
fn parameter_is_amp_pass_amp_x() {
    fails(
        "(defun bump (&v) (set! v (+ @v 1)))
         (defun main () -> i64 (let ((c (cell 0))) (do (bump c) @c)))",
        K::AmpPosition,
        "parameter v of bump is &; pass &x",
    );
}

#[test]
fn function_with_amp_parameters_is_not_a_value() {
    fails(
        "(defun bump (&v) (set! v (+ @v 1)))
         (defun main () -> i64 (let ((f bump)) 0))",
        K::AmpFunctionValue,
        "function with & parameters is not a value",
    );
    fails(
        "(defun main () -> i64 (let ((f array-set!)) 0))",
        K::AmpFunctionValue,
        "function with & parameters is not a value",
    );
    fails(
        "(defun main () -> i64 (let ((f set-field!)) 0))",
        K::AmpFunctionValue,
        "function with & parameters is not a value",
    );
}

#[test]
fn type_has_no_field() {
    fails(
        "(defstruct P (x: i64 y: i64)) (defun main () -> i64 (. (P 1 2) z))",
        K::NoField,
        "P has no field z",
    );
    fails(
        "(defstruct P (x: i64 y: i64))
         (defun main () -> i64 (let ((c (cell (P 1 2)))) (do (set-field! &c z 3) 0)))",
        K::NoField,
        "P has no field z",
    );
    fails(
        "(defun main () -> i64 (. 5 x))",
        K::NoField,
        "i64 has no field x",
    );
}

#[test]
fn cannot_infer_the_struct_type() {
    fails(
        "(defun getx (p) (. p x)) (defun main () -> i64 0)",
        K::FieldUnresolved,
        "cannot infer the struct type of p for field x; annotate it",
    );
    fails(
        "(defun setx (&c) (set-field! &c x 1)) (defun main () -> i64 0)",
        K::FieldUnresolved,
        "cannot infer the struct type of @c for field x; annotate it",
    );
}

#[test]
fn cannot_infer_cell_atom_weak_or_task() {
    fails(
        "(defun get (c) @c) (defun main () -> i64 0)",
        K::DerefUnresolved,
        "cannot infer whether c is a cell, an atom, a weak reference or a task",
    );
    // Any later use that fixes the head resolves it (§3.4).
    ok("(defun get (c) (do (set! c 1) @c)) (defun main () -> i64 (get (cell 2)))");
}

#[test]
fn no_implementation() {
    fails(
        "(defun main () -> i64 (str-len (+ \"a\" \"b\")))",
        K::NoInstance,
        "no implementation of Num for str",
    );
    fails(
        "(defprotocol Describe (describe (self) -> str))
         (defun main () -> i64 (str-len (describe 1)))",
        K::NoInstance,
        "no implementation of Describe for i64",
    );
    fails(
        "(defun main () -> i64 @5)",
        K::NoInstance,
        "no implementation of Deref for i64",
    );
}

#[test]
fn ambiguous_constraint() {
    fails(
        "(defun f (x) (do (show (trap \"no\")) x)) (defun main () -> i64 (f 1))",
        K::Ambiguous,
        "ambiguous constraint Show a in f; add an annotation",
    );
}

#[test]
fn cannot_construct_the_infinite_type() {
    // Case 15's original spelling (syntax Appendix A).
    fails(
        "(defun main () -> i64 (let ((c (cell []))) (do (set! c [c]) 0)))",
        K::Infinite,
        "cannot construct the infinite type",
    );
}

#[test]
fn cannot_unify() {
    // §2.12: no promotion.
    fails(
        "(defun main () -> i64 (+ 1i32 2))",
        K::Unify,
        "cannot unify i64 with i32",
    );
    // A test that is not a bool: a primitive says why (D1, `conditions.rs`),
    // anything else is the plain mismatch.
    fails(
        "(defun main () -> i64 (if (some 1) 2 3))",
        K::Unify,
        "cannot unify (Option i64) with bool",
    );
    // §2.10: set! on an atom.
    fails(
        "(defun main () -> i64 (let ((a (atom 0))) (do (set! a 1) 0)))",
        K::Unify,
        "cannot unify (Atom i64) with (Cell",
    );
}
