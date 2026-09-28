//! `Option` representations (types §8.1): a heap enum object for a
//! scalar, `dyn` or `Option` payload, nothing for another object.

use super::{clean, traced};

/// The counted objects a run allocated, after checking that it freed
/// them all.
fn allocated(src: &str) -> usize {
    let (_, report) = traced(src);
    assert!(report.is_clean(), "{src}: {:?}", report.leaks);
    assert_eq!(report.allocated, report.freed, "{src}");
    report.allocated
}

#[test]
fn a_some_of_a_scalar_allocates_a_heap_object() {
    let src = "(defun main () -> i64 (let ((o (some 5))) (match o ((some v) v) (nil 0))))";
    assert_eq!(allocated(src), 1);
    // nil of (Option i64) too: an ordinary heap enum.
    let src = "(defun main () -> i64
                 (let ((o (if (= 1 2) (some 5) nil))) (match o ((some v) v) (nil 7))))";
    assert_eq!(allocated(src), 1);
}

#[test]
fn a_some_of_an_object_allocates_nothing() {
    let src = "(defun main () -> i64
                 (let ((o (some (str-concat \"a\" \"b\")))) (match o ((some s) (str-len s)) (nil 0))))";
    assert_eq!(allocated(src), 1, "the string only");
}

#[test]
fn nested_options_are_boxed_and_distinct() {
    let src = "(defun main () -> i64
                 (let ((n (some nil)) (m (some (some \"x\"))))
                   (+ (match n ((some nil) 1) ((some (some _)) 10) (nil 100))
                      (match m ((some (some s)) (str-len s)) (_ 0)))))";
    assert_eq!(
        allocated(src),
        3,
        "(some nil), nil and (some (some ..)); \"x\" is immortal"
    );
    clean(src, 2);
}

#[test]
fn an_upgrade_of_a_weak_dyn_is_a_heap_enum() {
    // types §8.7: `@w` on a `(Weak (dyn P))` is `(some { t, vt })` as a
    // heap enum, or the heap enum of tag `nil`; on any other `Weak` it
    // allocates nothing.
    let live = "(defstruct A (n: i64))
                (impl Show A (show (self) \"A\"))
                (defun main () -> i64
                  (let ((b (A 3)) (w (weak (dyn Show b))))
                    (match @w ((some d) (str-len (show d))) (nil 0))))";
    assert_eq!(allocated(live), 3, "A, its weak box and the some");
    clean(live, 1);
    let dead = "(defstruct A (n: i64))
                (impl Show A (show (self) \"A\"))
                (defun main () -> i64
                  (let ((w (let ((b (A 3))) (weak (dyn Show b)))))
                    (match @w ((some d) (str-len (show d))) (nil 7))))";
    assert_eq!(allocated(dead), 3, "A, its weak box and the nil");
    clean(dead, 7);
    let plain = "(defstruct A (n: i64))
                 (defun main () -> i64
                   (let ((b (A 3)) (w (weak b)))
                     (match @w ((some a) (. a n)) (nil 0))))";
    assert_eq!(allocated(plain), 2, "A and its weak box only");
}

#[test]
fn a_generic_some_is_boxed_by_its_payload() {
    let src = "(defun wrap (x) (some x))
               (defun main () -> i64
                 (+ (match (wrap 5) ((some v) v) (nil 0))
                    (match (wrap (str-concat \"a\" \"b\")) ((some s) (str-len s)) (nil 0))))";
    assert_eq!(allocated(src), 2, "the box of 5 and the string");
    clean(src, 7);
}
