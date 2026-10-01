//! The externs the interpreter provides (syntax §3.15): `strtod` and
//! `strtof` end to end through a program, and what they refuse.

use super::{clean, failed, run};
use crate::cases::Outcome;

#[test]
fn the_end_pointer_is_stored_through_the_second_argument() {
    let src = "(extern strtod :private (ptr ptr) -> f64)
        (defun main () -> i64
          (unsafe
            (let ((p (alloc 16)) (e (alloc 8)))
              (do (store-i8 (ptr+ p 0) 49i8) (store-i8 (ptr+ p 1) 50i8)
                  (store-i8 (ptr+ p 2) 120i8) (store-i8 (ptr+ p 3) 0i8)
                  (let ((x (strtod p e))
                        (q (load-ptr e)))
                    (+ (fptosi i64 x) (if (= q (ptr+ p 2)) 1000 0)))))))";
    clean(src, 1012);
}

#[test]
fn a_null_end_pointer_is_allowed() {
    let src = "(extern strtod :private (ptr ptr) -> f64)
        (defun main () -> i64
          (unsafe
            (let ((p (alloc 16)) (c (alloc 8)))
              (do (store-i8 (ptr+ p 0) 55i8) (store-i8 (ptr+ p 1) 0i8)
                  (store-i64 c 0)
                  (fptosi i64 (strtod p (load-ptr c)))))))";
    clean(src, 7);
}

#[test]
fn a_hex_float_is_unsupported_rather_than_read_as_its_leading_zero() {
    let src = "(extern strtod :private (ptr ptr) -> f64)
        (defun main () -> i64
          (unsafe
            (let ((p (alloc 16)) (e (alloc 8)))
              (do (store-i8 (ptr+ p 0) 48i8) (store-i8 (ptr+ p 1) 120i8)
                  (store-i8 (ptr+ p 2) 49i8) (store-i8 (ptr+ p 3) 0i8)
                  (fptosi i64 (strtod p e))))))";
    let message = failed(src);
    assert!(message.contains("unsupported: hex float"), "{message}");
}

#[test]
fn a_string_without_its_nul_is_a_trap() {
    let src = "(extern strtod :private (ptr ptr) -> f64)
        (defun main () -> i64
          (unsafe
            (let ((p (alloc 2)) (e (alloc 8)))
              (do (store-i8 (ptr+ p 0) 49i8) (store-i8 (ptr+ p 1) 50i8)
                  (fptosi i64 (strtod p e))))))";
    match run(src) {
        Outcome::Trapped { message, .. } => assert!(message.contains("no NUL"), "{message}"),
        other => panic!("expected a trap, got {other:?}"),
    }
}

#[test]
fn a_declaration_other_than_the_libc_signature_is_unsupported() {
    let src = "(extern strtod :private (ptr ptr) -> f32)
        (defun main () -> i64
          (unsafe
            (let ((p (alloc 2)) (e (alloc 8)))
              (do (store-i8 (ptr+ p 0) 49i8) (store-i8 (ptr+ p 1) 0i8)
                  (fptosi i64 (fpext f64 (strtod p e)))))))";
    let message = failed(src);
    assert!(
        message.contains("unsupported: extern strtod declared other than"),
        "{message}"
    );
}
