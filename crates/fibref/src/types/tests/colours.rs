//! Colour parameters on structs and enums (types §1.3; owner's
//! decision of 2026-09-28).

use crate::types::ErrorKind as K;

use super::{binding_type, fails, ok};

const HANDLER: &str = "(defstruct (Handler k :colour) (f: (fn k (i64) i64)))";

#[test]
fn a_constructions_colour_argument_follows_its_closure() {
    let p = ok(&format!(
        "{HANDLER}
         (defun main () -> i64
           (let ((c (cell 1)) (loc (Handler (fn (x) (+ x @c)))) (snd (Handler (fn (x) x))))
             (+ ((. loc f) 1) (join (spawn (fn () ((. snd f) 2)))))))"
    ));
    assert_eq!(binding_type(&p, "loc"), "(Handler :local)");
    assert_eq!(binding_type(&p, "snd"), "(Handler :send)");
}

#[test]
fn a_generic_constructor_call_generalises_the_colour_flow() {
    let p = ok(&format!(
        "{HANDLER} (defun mk (g) (Handler g)) (defun main () -> i64 0)"
    ));
    let s = p.scheme("mk").expect("a scheme");
    assert_eq!(
        p.show_scheme(s),
        "∀ς0 ς1. ς0 ⊑ ς1 ⇒ (fn :send ((fn ς0 (i64) i64)) (Handler ς1))"
    );
}

#[test]
fn colour_parameters_and_type_parameters_do_not_mix() {
    fails(
        "(defstruct (H k :colour) (f: (fn k () i64) g: k)) (defun main () -> i64 0)",
        K::Resolve,
        "k is a colour parameter of H, not a type",
    );
    fails(
        "(defstruct (H a) (f: (fn a () i64))) (defun main () -> i64 0)",
        K::Resolve,
        "a is not a colour parameter of H",
    );
    fails(
        &format!("{HANDLER} (defun f (h: (Handler i64)) -> i64 1) (defun main () -> i64 0)"),
        K::Resolve,
        "i64 is not a colour: Handler takes :send, :local or a colour variable there",
    );
}

#[test]
fn a_named_colour_variable_ties_annotations_together() {
    ok(&format!(
        "{HANDLER}
         (defun same (a: (Handler k) b: (Handler k)) -> i64 (+ ((. a f) 1) ((. b f) 2)))
         (defun main () -> i64
           (let ((c (cell 1))) (same (Handler (fn (x) x)) (Handler (fn (x) (+ x @c))))))"
    ));
}

#[test]
fn an_impl_over_a_colour_parameterised_type_names_the_colour() {
    ok(&format!(
        "{HANDLER}
         (impl Show (Handler k) (show (self) (show ((. self f) 1))))
         (defun main () -> i64 (str-len (show (Handler (fn (x) (* x 10))))))"
    ));
}

#[test]
fn a_colour_parameterised_enum_matches_with_the_scrutinees_colour() {
    ok("(defenum (Job k :colour) (Idle) (Ready run: (fn k () i64)))
        (defun go (j) (match j ((Idle) 0) ((Ready r) (join (spawn r)))))
        (defun main () -> i64 (go (Ready (fn () 4))))");
    fails(
        "(defenum (Job k :colour) (Idle) (Ready run: (fn k () i64)))
         (defun go (j) (match j ((Idle) 0) ((Ready r) (join (spawn r)))))
         (defun main () -> i64 (let ((c (cell 4))) (go (Ready (fn () @c)))))",
        K::CellNotSend,
        "cell cannot be shared between threads",
    );
}
