//! The executor (types §8.8, the reference interpreter's schedule):
//! threads switch at scheduling points, a thread's trap stops every
//! thread, `main` waits for every thread, and a deadlock is reported.

use super::{clean, failed};

#[test]
fn a_spawned_thread_spinning_on_main_lets_main_run() {
    clean(
        "(defun main () -> i64
           (let ((a (atom 0)) (t (spawn (fn () (loop () (if (= @a 0) (recur) @a))))))
             (do (reset! a 3) (join t))))",
        3,
    );
}

#[test]
fn main_spinning_on_a_thread_lets_it_run() {
    clean(
        "(defun main () -> i64
           (let ((a (atom 0)) (t (spawn (fn () (do (reset! a 5) 1)))))
             (loop () (if (= @a 0) (recur) (+ @a (join t))))))",
        6,
    );
}

#[test]
fn a_trap_in_a_thread_stops_main_spinning_for_ever() {
    let m = failed(
        "(defun main () -> i64
           (let ((a (atom 0)))
             (do (spawn (fn () (loop ((i 0)) (if (< i 5000) (recur (+ i 1)) (trap \"stop\")))))
                 (loop () (if (= @a 0) (recur) 1)))))",
    );
    assert!(m.contains("trap: stop"), "{m}");
}

#[test]
fn a_trap_after_main_returned_is_the_programs() {
    let m = failed(
        "(defun main () -> i64
           (do (spawn (fn () (loop ((i 0)) (if (< i 5000) (recur (+ i 1)) (trap \"late\")))))
               0))",
    );
    assert!(m.contains("trap: late"), "{m}");
}

#[test]
fn threads_joining_each_other_are_a_reported_deadlock() {
    let m = failed(
        "(defun wait-join (s: (Atom (Option (Task i64)))) -> i64
           (loop () (match @s ((some x) (join x)) (nil (recur)))))
         (defun main () -> i64
           (let ((s1 (atom nil)) (s2 (atom nil))
                 (t (spawn (fn () (wait-join s2))))
                 (u (spawn (fn () (wait-join s1)))))
             (do (reset! s1 (some t)) (reset! s2 (some u)) (join t))))",
    );
    assert!(m.contains("deadlock: every thread waits"), "{m}");
}

#[test]
fn a_thread_joining_its_own_task_is_a_reported_deadlock() {
    let m = failed(
        "(defun wait-join (s: (Atom (Option (Task i64)))) -> i64
           (loop () (match @s ((some x) (join x)) (nil (recur)))))
         (defun main () -> i64
           (let ((s1 (atom nil)) (t (spawn (fn () (wait-join s1)))))
             (do (reset! s1 (some t)) (join t))))",
    );
    assert!(m.contains("its own thread is driving"), "{m}");
}

#[test]
fn a_task_driven_by_one_thread_is_waited_for_by_another() {
    // main drives the async task k, which spins until u sets the atom;
    // t joins k meanwhile, so it waits for main's drive to finish
    // rather than drive k itself.
    clean(
        "(defun main () -> i64
           (let ((a (atom 0)) (started (atom 0))
                 (k (async (do (reset! started 1) (loop () (if (= @a 0) (recur) 10)))))
                 (s (atom (some k)))
                 (t (spawn (fn () (loop () (if (= @started 0) (recur)
                                             (match @s ((some x) (+ (join x) 1)) (nil 0)))))))
                 (u (spawn (fn () (loop () (if (= @started 0) (recur)
                                             (do (dotimes (i 5000) ()) (reset! a 1) 0)))))))
             (+ (join k) (+ (join t) (join u)))))",
        21,
    );
}

#[test]
fn a_macro_body_runs_threads_on_the_same_executor() {
    clean(
        "(defmacro spun (x)
           (let ((a (atom 0)) (t (spawn (fn () (loop () (if (= @a 0) (recur) @a))))))
             (do (reset! a 5) (join t) x)))
         (defun main () -> i64 (+ (spun 2) 1))",
        3,
    );
}

#[test]
fn deref_of_a_task_is_join() {
    // types §2.9: `@t` joins, retaining the result for the caller each
    // time, so two reads of a Vec result are two counts on one object.
    clean(
        "(defun main () -> i64
           (let ((t (spawn (fn () [1 2 3]))))
             (let ((a @t) (b (join t)))
               (+ (count a) (+ (count b) (count @t))))))",
        9,
    );
    clean(
        "(defun main () -> i64 (+ @(spawn (fn () 40)) @(async 2)))",
        42,
    );
}

#[test]
fn deref_of_a_trapping_task_is_its_trap() {
    let m = failed("(defun main () -> i64 @(spawn (fn () (+ 1 (trap \"in the task\")))))");
    assert!(m.contains("trap: in the task"), "{m}");
}

#[test]
fn a_thread_dereferencing_its_own_task_is_a_reported_deadlock() {
    let m = failed(
        "(defun wait-deref (s: (Atom (Option (Task i64)))) -> i64
           (loop () (match @s ((some x) @x) (nil (recur)))))
         (defun main () -> i64
           (let ((s1 (atom nil)) (t (spawn (fn () (wait-deref s1)))))
             (do (reset! s1 (some t)) (join t))))",
    );
    assert!(m.contains("its own thread is driving"), "{m}");
}
