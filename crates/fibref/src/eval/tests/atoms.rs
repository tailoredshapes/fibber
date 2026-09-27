//! `swap!`: the counts it hands `f` (types §8.4, §8.6) and its compare
//! and retry.

use super::clean;

#[test]
fn swap_retains_a_heap_closure_it_calls() {
    // The closure is stored (a heap closure, §6.5); its body releases
    // `env` at its exit, so swap! must hand it a count (adversary 008).
    clean(
        "(defstruct H (f: (fn (i64) i64)))
         (defun main () -> i64
           (let ((a (atom 1)) (h (H (fn (x) (+ x 1))))) (swap! a (. h f))))",
        2,
    );
    clean(
        "(defun main () -> i64
           (let ((a (atom 1)) (g (fn (x) (+ x 1))) (c (cell g))) (swap! a g)))",
        2,
    );
}

#[test]
fn swap_as_a_function_value_releases_each_count_once() {
    // Adversary 005: through a value swap! owns the closure too.
    clean(
        "(defun apply2 (f x y) (f x y))
         (defun main () -> i64 (let ((a (atom 1))) (apply2 swap! a (fn (x) (+ x 1)))))",
        2,
    );
}

#[test]
fn swap_retries_when_f_changed_the_atom() {
    // f's first run adds 10 through a nested swap!; the compare fails,
    // f runs again on 11 and 12 is stored (adversary 001, no thread).
    clean(
        "(defun main () -> i64
           (let ((a (atom 1)) (once (cell false)))
             (swap! a (fn (x) (do (if @once () (do (set! once true)
                (swap! a (fn (y) (+ y 10))) ())) (+ x 1))))))",
        12,
    );
}

#[test]
fn a_failed_attempt_releases_its_result_and_snapshot() {
    clean(
        "(defstruct B (n: i64))
         (defun main () -> i64
           (let ((a (atom (B 1))) (once (cell false)))
             (. (swap! a (fn (x) (do (if @once () (do (set! once true)
                (swap! a (fn (y) (B (+ (. y n) 10)))) ())) (B (+ (. x n) 1))))) n)))",
        12,
    );
}
