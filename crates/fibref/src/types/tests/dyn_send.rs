//! `(dyn P :send)` (types §2.15, §5.1; owner's decision of 2026-09-28).

use crate::types::ErrorKind as K;

use super::{binding_type, fails, ok};

const NAMED: &str = "(defstruct Named (s: str))
                     (impl Show Named (show (self) (. self s)))
                     (defstruct Counter (n: (Cell i64)))
                     (impl Show Counter (show (self) \"c\"))";

fn with(body: &str) -> String {
    format!("{NAMED}\n{body}")
}

#[test]
fn dyn_send_is_its_own_type_and_prints_so() {
    let p = ok(&with(
        "(defun main () -> i64 (let ((d (dyn Show :send (Named \"a\")))) (str-len (show d))))",
    ));
    assert_eq!(binding_type(&p, "d"), "(dyn Show :send)");
}

#[test]
fn dyn_send_is_written_in_annotations() {
    ok(&with(
        "(defun f (d: (dyn Show :send)) -> i64 (join (spawn (fn () (str-len (show d))))))
         (defun main () -> i64 (f (dyn Show :send (Named \"ab\"))))",
    ));
    fails(
        &with("(defun f (d: (dyn Show :local)) -> i64 1) (defun main () -> i64 0)"),
        K::Resolve,
        "malformed (dyn Show :local)",
    );
}

#[test]
fn dyn_send_needs_a_sendable_value_and_dyn_stays_local() {
    fails(
        &with("(defun main () -> i64 (do (dyn Show :send (Counter (cell 0))) 0))"),
        K::CellNotSend,
        "cell cannot be shared between threads: field n of Counter",
    );
    fails(
        &with(
            "(defun main () -> i64
               (let ((d (dyn Show (Named \"a\")))) (join (spawn (fn () (str-len (show d)))))))",
        ),
        K::ValueNotSend,
        "value of type (dyn Show) cannot be shared between threads",
    );
}

#[test]
fn a_generic_dyn_send_carries_the_send_bound() {
    let p = ok(&with(
        "(defun hide (x) (dyn Show :send x)) (defun main () -> i64 0)",
    ));
    let s = p.scheme("hide").expect("a scheme");
    let printer = crate::types::display::Printer::with_names(&p.globals, &s.var_names, &[]);
    let preds: Vec<String> = s.preds.iter().map(|q| printer.pred(q)).collect();
    assert!(preds.contains(&"(Send a)".to_string()), "{preds:?}");
    assert!(preds.contains(&"(Object a)".to_string()), "{preds:?}");
}

#[test]
fn dyn_send_and_dyn_do_not_unify_but_convert_explicitly() {
    fails(
        &with(
            "(defun main () -> i64
               (let ((s (dyn Show :send (Named \"a\")))) (vec-count [(dyn Show (Named \"b\")) s])))",
        ),
        K::Unify,
        "cannot unify",
    );
    ok(&with(
        "(defun main () -> i64
           (let ((s (dyn Show :send (Named \"a\")))) (vec-count [(dyn Show (Named \"b\")) (dyn Show s)])))",
    ));
}

#[test]
fn an_impl_body_needs_send_declared_to_make_a_dyn_send() {
    fails(
        "(defstruct (W a) (v: a))
         (impl Show (W a) :where ((Show a) (Object a)) (show (self) (show (dyn Show :send (. self v)))))
         (defun main () -> i64 0)",
        K::ImplContext,
        "no implementation of Send for a",
    );
    ok("(defstruct (W a) (v: a))
        (impl Show (W a) :where ((Show a) (Send a) (Object a))
          (show (self) (show (dyn Show :send (. self v)))))
        (defun main () -> i64 0)");
}
