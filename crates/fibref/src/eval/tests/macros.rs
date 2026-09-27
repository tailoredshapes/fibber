//! User macros at expansion time (syntax §3.16): reflection answers at
//! their declared types, and the audit of a macro's run.

use super::{clean, run};
use crate::cases::Outcome;

#[test]
fn struct_reflection_in_a_user_macro() {
    // Adversary 011: struct-* are not string builtins.
    clean(
        "(defstruct P (a: i64 b: str))
         (defmacro nfields (name) (Int (count (struct-fields name)) :i64))
         (defmacro isstruct (name) (if (struct? name) '100 '0))
         (defun main () -> i64 (+ (nfields P) (isstruct P)))",
        102,
    );
}

#[test]
fn enum_reflection_returns_a_vec_and_a_bool() {
    // Adversary 012: (Vec Form) and bool, not a Form.
    clean(
        "(defmacro nvariants (name) (Int (count (enum-variants name)) :i64))
         (defmacro isenum (name) (if (enum? name) (quote 10) (quote 0)))
         (defun main () -> i64 (+ (nvariants Option) (isenum Option)))",
        12,
    );
}

#[test]
fn reflection_at_run_time_is_unsupported_not_a_string_builtin() {
    let src = "(defun main () -> i64 (count (struct-fields 'P)))";
    match run(src) {
        Outcome::Failed { message } => {
            assert!(message.contains("only at expansion time"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_macro_run_that_leaks_a_cell_cycle_expands() {
    // Adversary 003: the permitted leak of ownership.md §6 does not fail
    // the expansion, and the program's own run is clean.
    clean(
        "(defmacro knot (x)
           (let ((c (cell [])))
             (do (set! c (conj @c (fn () (count @c)))) x)))
         (defun main () -> i64 (knot 5))",
        5,
    );
}

#[test]
fn a_macro_built_literal_out_of_its_width_is_rejected() {
    // Adversary 013.
    let src = "(defmacro m () (List [(Sym \"sext\") (Sym \"i64\") (Int 300 :i8)]))
               (defun main () -> i64 (m))";
    match run(src) {
        Outcome::Rejected { message } => assert!(message.contains("does not fit i8"), "{message}"),
        other => panic!("{other:?}"),
    }
    let src = "(defmacro m () (List [(Sym \"fptosi\") (Sym \"i64\") (Flt 2.5 :f16)]))
               (defun main () -> i64 (m))";
    match run(src) {
        Outcome::Rejected { message } => assert!(message.contains("width :f16"), "{message}"),
        other => panic!("{other:?}"),
    }
}
