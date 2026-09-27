//! Protocol methods used as function values (types §8.4): each runs
//! its implementation's all-owned body, which releases every object
//! argument at its own exit, so a leak or a double release shows in
//! the audit. The instance is the one resolved at the use (§4.2), or
//! the receiver's at each call where the use is generic or `dyn`
//! (§4.3, §4.5).

use fibref::cases::{AuditSummary, Evaluator, Outcome, Value};
use fibref::eval::Interpreter;

/// The result and audit of a program that must run.
fn ran(src: &str) -> (i64, AuditSummary) {
    match Interpreter.run(src) {
        Outcome::Compiled {
            result: Value::Int(n),
            audit,
        } => (n, audit),
        other => panic!("expected a run, got {other:?}"),
    }
}

fn clean(src: &str, n: i64) {
    let (got, audit) = ran(src);
    assert_eq!(got, n);
    assert!(audit.clean, "{audit}");
}

const PT: &str = "(defstruct Pt (x: i64 y: i64))
     (impl Show Pt (show (self) (str-concat \"p\" (show (. self x)))))";

const TAG: &str = "(defstruct Tag (n: i64))
     (impl Show Tag (show (self) (str-concat \"tag\" (show (. self n)))))";

/// `(map show v)` over a user instance ("p1", "p30") and over a
/// built-in one ("7", "123").
#[test]
fn map_show_over_a_vector() {
    clean(
        &format!(
            "{PT}
             (defun main () -> i64
               (let ((v (conj (conj (vec-empty) (Pt 1 2)) (Pt 30 4)))
                     (w (map show v)))
                 (+ (str-len (nth w 0)) (str-len (nth w 1)))))"
        ),
        5,
    );
    clean(
        "(defun main () -> i64
           (let ((w (map show (conj (conj (vec-empty) 7) 123))))
             (+ (str-len (nth w 0)) (str-len (nth w 1)))))",
        4,
    );
}

/// A method value stored in a struct field, called after the struct
/// was built and passed around.
#[test]
fn method_value_stored_in_a_struct_and_called_later() {
    clean(
        &format!(
            "{PT}
             (defstruct Holder (f: (fn (Pt) str)))
             (defun use-it (h: Holder p: Pt) -> i64 (str-len ((. h f) p)))
             (defun main () -> i64
               (let ((h (Holder show)))
                 (+ (use-it h (Pt 12 3)) (use-it h (Pt 4567 0)))))"
        ),
        8,
    );
}

/// A method value handed along 100 000 self tail calls, then called in
/// tail position of `go` (`(f p)`, a tail call through a value that
/// moves `p` into the all-owned body, which releases it).
#[test]
fn method_value_passed_through_tail_calls() {
    clean(
        &format!(
            "{PT}
             (defun apply-n (f p n acc)
               (if (= n 0) acc (apply-n f p (- n 1) (+ acc (str-len (f p))))))
             (defun go (f p) (f p))
             (defun main () -> i64
               (+ (apply-n show (Pt 5 6) 100000 0) (str-len (go show (Pt 42 0)))))"
        ),
        200_003,
    );
}

/// A method value in a generic caller: its instance is the receiver's
/// (`lens` runs `show` for `Pt` and for `Tag`).
#[test]
fn method_value_whose_instance_a_generic_caller_chooses() {
    clean(
        &format!(
            "{PT} {TAG}
             (defun lens (v) (let ((w (map show v))) (str-len (nth w 0))))
             (defun main () -> i64
               (+ (lens (conj (vec-empty) (Pt 100 0)))
                  (* 10 (lens (conj (vec-empty) (Tag 7))))))"
        ),
        44,
    );
}

/// A method value over `(dyn Show)` elements: each call runs the
/// element's instance.
#[test]
fn method_value_over_dyn_elements() {
    clean(
        &format!(
            "{PT} {TAG}
             (defun main () -> i64
               (let ((v (conj (conj (vec-empty) (dyn Show (Pt 1 0))) (dyn Show (Tag 22))))
                     (w (map show v)))
                 (+ (str-len (nth w 0)) (* 10 (str-len (nth w 1))))))"
        ),
        52,
    );
}
