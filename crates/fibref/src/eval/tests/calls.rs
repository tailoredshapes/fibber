//! Calls, tail calls, loops, closures, `&` parameters, macros.

use super::{clean, run};
use crate::cases::Outcome;
use crate::heap::{Event, Kind};

#[test]
fn self_tail_calls_run_in_constant_stack() {
    clean(
        "(defun count-down (n) (if (= n 0) 7 (count-down (- n 1))))
         (defun main () -> i64 (count-down 300000))",
        7,
    );
}

#[test]
fn a_closure_tail_call_through_a_named_function_value() {
    clean(
        "(defstruct K (f: (fn (i64 K) i64)))
         (defun hop (n: i64 k: K) -> i64 (if (= n 0) 5 ((. k f) (- n 1) k)))
         (defun main () -> i64 (hop 100000 (K hop)))",
        5,
    );
}

#[test]
fn loop_and_recur_rebind_their_variables() {
    clean(
        "(defun main () -> i64
           (loop ((i 0) (acc (vec-empty)))
             (if (< i 40) (recur (+ i 1) (conj acc i)) (count acc))))",
        40,
    );
}

#[test]
fn an_amp_argument_copies_in_and_writes_back() {
    clean(
        "(defun add2 (&v) (do (push! &v 1) (push! &v 2)))
         (defun main () -> i64 (let ((c (cell [9]))) (do (add2 &c) (count @c))))",
        3,
    );
}

#[test]
fn a_forwarded_amp_parameter_keeps_its_private_cell() {
    clean(
        "(defun fill (&v n) (if (= n 0) () (do (append &v n) (fill &v (- n 1)))))
         (defun main () -> i64 (let ((c (cell []))) (do (fill &c 50) (count @c))))",
        50,
    );
}

/// Types §6.6 (owner, 2026-09-28): the copy-in happens at call entry,
/// so a later argument's write to the variable is seen by the callee
/// (6 with a copy-in at the argument's position).
#[test]
fn a_copy_in_sees_a_later_arguments_write() {
    clean(
        "(defun f (&v: i64 n: i64) -> i64 (+ @v n))
         (defun main () -> i64 (let ((c (cell 1))) (f &c (do (set! c 10) 5))))",
        15,
    );
}

/// A forwarded cell and a copied-in one see the same writes by the
/// arguments (cases 150 and 151).
#[test]
fn forwarding_and_copy_in_agree_on_the_arguments_writes() {
    let callee = "(defun put (&v: (Vec i64) n: i64) -> unit (push! &v n))";
    clean(
        &format!(
            "{callee}
             (defun fwd (&v: (Vec i64)) -> unit (push! &v (do (put &v 4) 0)))
             (defun main () -> i64 (let ((c (cell []))) (do (fwd &c) (count @c))))"
        ),
        2,
    );
    clean(
        &format!(
            "{callee}
             (defun main () -> i64
               (let ((c (cell []))) (do (push! &c (do (put &c 4) 0)) (count @c))))"
        ),
        2,
    );
}

/// Two `&` arguments: a later argument writes the first one's variable
/// before either copy-in; the write-backs keep parameter order.
#[test]
fn every_copy_in_follows_the_last_argument() {
    clean(
        "(defun g (&a: i64 &b: i64 n: i64) -> i64
           (do (set! b (+ @a n)) (set! a 0) @b))
         (defun main () -> i64
           (let ((x (cell 1)) (y (cell 0)))
             (let ((r (g &x &y (do (set! x 7) 100))))
               (+ (* 1000 r) (+ (* 10 @x) @y)))))",
        107_107,
    );
}

/// The private cell is made after the arguments: every heap allocation
/// of the call (here `conj`'s result, an argument) precedes it.
#[test]
fn the_private_cell_is_made_after_the_arguments() {
    let (n, report) = super::traced(
        "(defun f (&v: i64 s: (Vec i64)) -> i64 (+ @v (count s)))
         (defun main () -> i64 (let ((c (cell 1))) (f &c (conj (vec-empty) 3))))",
    );
    assert_eq!(n, 2);
    assert!(report.is_clean());
    let private = report
        .trace
        .iter()
        .rposition(|e| {
            matches!(
                e,
                Event::AllocStack {
                    kind: Kind::Cell,
                    ..
                }
            )
        })
        .expect("a private cell");
    let heap: Vec<usize> = report
        .trace
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Event::Alloc { .. }))
        .map(|(i, _)| i)
        .collect();
    assert!(!heap.is_empty(), "{:?}", report.trace);
    assert!(heap.iter().all(|i| *i < private), "{:?}", report.trace);
}

#[test]
fn closures_capture_and_share_a_cell() {
    clean(
        "(defun main () -> i64
           (let ((n (cell 0))
                 (f (fn () (set! n (+ @n 2)))))
             (do (f) (f) @n)))",
        4,
    );
}

#[test]
fn a_user_macro_with_a_rest_parameter_expands() {
    clean(
        "(defmacro sum (... xs) `(+ 0 (+ ,@xs)))
         (defun main () -> i64 (sum 1 2))",
        3,
    );
}

#[test]
fn a_macro_that_traps_is_an_expansion_error() {
    match run("(defmacro bad (x) (trap \"no\")) (defun main () -> i64 (bad 1))") {
        Outcome::Rejected { message } => {
            assert!(message.contains("macro bad failed"), "{message}");
            assert!(message.contains("trap: no"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_macro_argument_keeps_its_own_position_in_the_expansion() {
    // `car` is unbound: the error names the symbol's own position in
    // the argument (line 2, column 28), not the macro call's (column 23).
    match run("(defmacro id (x) x)\n(defun main () -> i64 (id (car)))") {
        Outcome::Rejected { message } => assert!(message.contains(":2:28:"), "{message}"),
        other => panic!("{other:?}"),
    }
}
