//! An `&` parameter captured by an argument of a call is not forwarded
//! (types §6.6, §6.10 rule (b); owner's decision of 2026-09-28).

use super::super::program::{Because, Pass, Tail};
use super::*;

const G: &str = "(defun g (&w: i64 k: (fn () unit) :borrow) -> i64 (do (set! w 1) (k) @w))
                 (defun h (&w: i64 n: i64) -> i64 (do (set! w n) @w))";

fn with(f: &str) -> Checked {
    ok(&format!(
        "{G} {f} (defun main () -> i64 (let ((c (cell 0))) (f &c)))"
    ))
}

fn captured(c: &Checked, callee: &str) -> bool {
    let (_, g) = call(c, "f", callee);
    match g.tail {
        Tail::Ordinary(Because::AmpCaptured { param }) => {
            assert_eq!(c.typed.globals.binding(param).name, "v");
            assert_eq!(g.args[0], Pass::Acquire);
            assert_eq!(g.write_backs.len(), 1);
            true
        }
        _ => false,
    }
}

#[test]
fn a_closure_argument_capturing_the_parameter_makes_it_copy_in() {
    assert!(captured(
        &with("(defun f (&v: i64) -> i64 (g &v (fn () (set! v 7))))"),
        "g"
    ));
}

#[test]
fn a_let_bound_closure_named_by_an_argument_makes_it_copy_in() {
    let c = with("(defun f (&v: i64) -> i64 (let ((k (fn () (set! v 7)))) (g &v k)))");
    assert!(captured(&c, "g"));
}

#[test]
fn any_mention_in_another_argument_counts_even_a_read() {
    assert!(captured(&with("(defun f (&v: i64) -> i64 (h &v @v))"), "h"));
    assert!(captured(
        &with("(defun f (&v: i64) -> i64 (h &v (do (set! v 2) 3)))"),
        "h"
    ));
}

#[test]
fn a_closure_that_does_not_reach_the_call_leaves_it_forwarded() {
    let c = with(
        "(defun f (&v: i64) -> i64
           (let ((k (fn () (set! v 7))))
             (do (k) ((fn () (set! v 8))) (h &v 3))))",
    );
    let (_, h) = call(&c, "f", "h");
    assert_eq!(h.tail, Tail::TailCall);
    assert_eq!(h.args[0], Pass::Forward);
    assert!(h.write_backs.is_empty());
}

#[test]
fn explain_names_the_captured_parameter() {
    let c = with("(defun f (&v: i64) -> i64 (g &v (fn () (set! v 7))))");
    let text = crate::own::explain::explain(&c.typed, &c.owned);
    assert!(
        text.contains("call (b: &v captured by an argument)   &v: acquire"),
        "{text}"
    );
}
