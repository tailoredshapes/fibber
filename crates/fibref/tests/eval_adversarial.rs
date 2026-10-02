//! Programs chosen to break the evaluator, or to make it disagree with
//! the audit: deep non-tail recursion, a million mutual tail calls with
//! an owned accumulator, a stack object at an owned parameter, a cycle
//! through a cell holding a closure, weak references dying inside a
//! loop, `swap!` reading its own atom, a trap inside a spawned thread,
//! and a macro calling a function of its own module.

use fibref::cases::{AuditSummary, Evaluator, Outcome, Value};
use fibref::eval::Interpreter;

fn run(src: &str) -> Outcome {
    Interpreter.run(src)
}

/// The result and audit of a program that must run.
fn ran(src: &str) -> (i64, AuditSummary) {
    match run(src) {
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

fn failed(src: &str) -> String {
    match run(src) {
        Outcome::Failed { message } | Outcome::Trapped { message, .. } => message,
        other => panic!("expected a failed run, got {other:?}"),
    }
}

/// Deep non-tail recursion stops with a clear error, not a Rust stack
/// overflow. The limit is the stack budget (`STACK_BUDGET`, 896 MiB of
/// the evaluator thread's 1 GiB): measured on this recursion, between
/// 100 000 and 150 000 levels in the dev profile (opt-level 1), between
/// 200 000 and 240 000 in release, and about 20 000 at opt-level 0; a
/// million is past it in every profile.
#[test]
fn deep_non_tail_recursion_is_an_error_not_a_crash() {
    let src = "(defun deep (n) (if (= n 0) 0 (+ 1 (deep (- n 1)))))
               (defun main () -> i64 (deep 1000000))";
    let msg = failed(src);
    assert!(msg.contains("too deep"), "{msg}");
    assert!(msg.contains("non-tail recursion"), "{msg}");
    clean(
        "(defun deep (n) (if (= n 0) 0 (+ 1 (deep (- n 1)))))
         (defun main () -> i64 (deep 5000))",
        5000,
    );
}

/// A million mutual tail calls, each passing an owned accumulator (a
/// fresh struct sharing one string with the previous one): constant
/// Rust stack, each accumulator freed at the jump that replaces it.
#[test]
fn a_million_mutual_tail_calls_with_an_owned_accumulator() {
    let src = "(defstruct Acc (n: i64 s: str))
               (defun ping (a k) (if (= k 0) (+ (. a n) (str-len (. a s)))
                                     (pong (Acc (+ (. a n) 1) (. a s)) (- k 1))))
               (defun pong (a k) (if (= k 0) (+ (. a n) (str-len (. a s)))
                                     (ping (Acc (+ (. a n) 1) (. a s)) (- k 1))))
               (defun main () -> i64 (ping (Acc 0 (str-concat \"ab\" \"c\")) 1000000))";
    clean(src, 1_000_003);
}

/// A scope-local object passed to an owned parameter of an ordinary
/// call (`fibref explain`: `x owns scope-local`, `arg 1 x: retain`,
/// `b owned (rule 3)`, escapes=no): the retain and the callee's release
/// before its jump are no-ops on the stack object, whose drop runs at
/// the caller's scope exit (the unit test
/// `eval::tests::stack_object_at_owned_parameter_is_never_counted`
/// checks the trace).
#[test]
fn a_stack_object_at_an_owned_parameter() {
    let src = "(defun walk (b n) (if (= n 0) (unbox b) (walk (Box (+ (unbox b) 1)) (- n 1))))
               (defun main () -> i64 (let ((x (Box 31))) (+ 1 (walk x 10))))";
    clean(src, 42);
}

/// A closure stored in a cell that it captures: a cycle through the
/// cell, reported as the permitted leak and nothing else.
#[test]
fn a_closure_in_a_cell_capturing_the_cell_is_a_cycle_leak() {
    let src = "(defun main () -> i64
                 (let ((c (cell (fn () 0))))
                   (set! c (fn () (+ 1 (@c))))
                   7))";
    let (n, audit) = ran(src);
    assert_eq!(n, 7);
    assert!(!audit.clean && audit.leak_cycles > 0, "{audit}");
    assert_eq!(audit.leaks, 0, "{audit}");
    assert!(audit.errors.is_empty(), "{audit}");
}

/// A weak reference whose target dies in each iteration of a loop:
/// every `@w` after the death is `nil`, and nothing is left.
#[test]
fn weak_deref_after_the_target_dies_inside_a_loop() {
    let src = "(defun main () -> i64
                 (loop ((i 0) (dead 0))
                   (if (< i 100)
                       (let ((w (let ((v (conj [] i))) (weak v))))
                         (recur (+ i 1) (if (nil? @w) (+ dead 1) dead)))
                       dead)))";
    clean(src, 100);
}

/// `swap!` whose function reads the atom it is swapping: the snapshot
/// and the read both hold counts; the old vector dies once both are
/// released.
#[test]
fn swap_whose_function_reads_the_atom() {
    let src = "(defun main () -> i64
                 (let ((a (atom [1 2])))
                   (swap! a (fn (v) (conj v (count @a))))
                   (swap! a (fn (v) (conj v (count @a))))
                   (+ (count @a) (nth @a 3))))";
    clean(src, 7);
}

/// A trap inside a spawned closure stops the run with the trap's
/// message and position, not a panic.
#[test]
fn a_trap_inside_a_spawned_closure() {
    let src = "(defun main () -> i64
                 (join (spawn (fn () (+ 1 (trap \"boom in thread\"))))))";
    let msg = failed(src);
    assert!(msg.contains("trap: boom in thread"), "{msg}");
    assert!(msg.contains(":2:"), "no position in {msg}");
}

/// Phase separation (syntax §3.16): a macro that calls a function of
/// its own module is an expansion error.
#[test]
fn a_macro_calling_a_function_of_its_module_is_a_phase_error() {
    let src = "(defun helper (f: Form) -> Form f)
               (defmacro m (x) (helper x))
               (defun main () -> i64 (m 1))";
    match run(src) {
        Outcome::Rejected { message } => assert!(
            message.contains(
                "macro m calls helper, which is not available at expansion time; move helper to a required module"
            ),
            "{message}"
        ),
        other => panic!("expected the phase error, got {other:?}"),
    }
}

/// A macro that calls a prelude function (a required module) expands.
#[test]
fn a_macro_calling_a_prelude_function_expands() {
    let src = "(defmacro twice (x) `(+ ,x ,(vec-nth [x] 0)))
               (defun main () -> i64 (twice 21))";
    clean(src, 42);
}
