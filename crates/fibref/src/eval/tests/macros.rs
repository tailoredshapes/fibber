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

#[test]
fn reflection_does_not_see_a_private_type_of_the_prelude() {
    // Syntax §5: VNode is private to fib.prelude; Vec is not. A program
    // type of the same name is its own.
    let src = "(defmacro seen () (if (enum? 'VNode) '1 (if (enum? 'Vec) '2 '3)))
               (defun main () -> i64 (seen))";
    clean(src, 2);
    let src = "(defenum VNode (A x: i64))
               (defmacro seen () (if (enum? 'VNode) '5 '6))
               (defun main () -> i64 (seen))";
    clean(src, 5);
}

#[test]
fn two_modules_macros_of_one_name_are_told_apart_by_module() {
    // The run of a macro is checked once; that check is kept per macro,
    // and the macro is the one of its module (syntax §5), not the first
    // of its name: this read 200 (both `same`s the used module's `*`) and
    // the JIT's runner read 102.
    let dir = std::env::temp_dir().join(format!("fibref-same-macro-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).expect("writable");
    write(
        "util.fib",
        "(ns util)\n(defmacro same (x) (List [(Sym \"+\") x (Int 1 :i64)]))\n",
    );
    write(
        "more.fib",
        "(ns more)\n(defmacro same (x) (List [(Sym \"*\") x (Int 100 :i64)]))\n",
    );
    let main = "(ns main (:require [more :as m]) (:use util))\n\
                (defun main () -> i64 (+ (m/same 1) (same 1)))\n";
    let file = dir.join("main.fib").to_string_lossy().into_owned();
    let outcome = crate::eval::run_source(main, &file);
    let _ = std::fs::remove_dir_all(&dir);
    match outcome {
        Outcome::Compiled { result, .. } => assert_eq!(result, crate::cases::Value::Int(102)),
        other => panic!("{other:?}"),
    }
}
