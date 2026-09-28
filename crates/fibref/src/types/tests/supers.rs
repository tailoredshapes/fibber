//! Supertraits and default methods (types §4.1; owner's decision of
//! 2026-09-28).

use crate::types::ErrorKind as K;

use super::{fails, ok};

#[test]
fn a_protocol_that_requires_itself_is_refused() {
    fails(
        "(defprotocol A :requires (B) (a (self) -> i64))
         (defprotocol B :requires (A) (b (self) -> i64))
         (defun main () -> i64 0)",
        K::Resolve,
        "protocol B requires itself",
    );
}

#[test]
fn a_supertrait_names_the_dispatch_parameter_first() {
    fails(
        "(defprotocol (Coll s e) :requires ((Indexable e s)) (c (self) -> i64))
         (defun main () -> i64 0)",
        K::Resolve,
        "a supertrait is Indexable or (Indexable s ..)",
    );
    fails(
        "(defprotocol (Coll s e) :requires (Indexable) (c (self) -> i64))
         (defun main () -> i64 0)",
        K::Resolve,
        "a supertrait is Indexable or (Indexable s ..)",
    );
}

#[test]
fn a_determined_supertrait_must_agree_with_the_impl() {
    let proto = "(defprotocol (Coll s e) :requires ((Indexable s e)) (first-of (self) -> e))";
    ok(&format!(
        "{proto}
         (impl (Coll a) (Vec a) (first-of (self) (nth self 0)))
         (defun main () -> i64 (first-of [5 6]))"
    ));
    fails(
        &format!(
            "{proto}
             (defstruct (W a) (v: a))
             (impl (Indexable a) (W a) (nth (self i) (. self v)))
             (impl (Coll i64) (W a) (first-of (self) 1))
             (defun main () -> i64 0)"
        ),
        K::Other,
        "impl Coll for (W a) requires (Indexable (W a) i64), but impl Indexable for (W a) determines (Indexable (W a) a)",
    );
}

#[test]
fn a_bound_entails_its_supertraits_in_a_polymorphically_recursive_defun() {
    ok("(defun f (x: a n: i64) :where ((Ord a)) -> bool
          (if (= n 0) (= x x) (f (some x) (- n 1))))
        (defun main () -> i64 (if (f 1 3) 1 0))");
}

#[test]
fn a_dyn_upcasts_only_to_a_supertrait() {
    let src = "(defprotocol Named :requires (Show) (name (self) -> str))
               (defstruct Dog (n: str))
               (impl Show Dog (show (self) (. self n)))
               (impl Hash Dog (hash (self) 1))
               (impl Named Dog (name (self) (. self n)))";
    ok(&format!(
        "{src} (defun main () -> i64 (str-len (show (dyn Show (dyn Named (Dog \"a\"))))))"
    ));
    fails(
        &format!("{src} (defun main () -> i64 (hash (dyn Hash (dyn Named (Dog \"a\")))))"),
        K::NoInstance,
        "no implementation of Hash for (dyn Named)",
    );
}

#[test]
fn an_impl_may_give_a_default_method_itself_or_omit_all_defaults() {
    ok(
        "(defprotocol P (a (self) -> i64) (b (self) -> i64 (+ (a self) 1)))
        (defstruct S (n: i64))
        (defstruct T (n: i64))
        (impl P S (a (self) (. self n)))
        (impl P T (b (self) 100) (a (self) (. self n)))
        (defun main () -> i64 (+ (b (S 1)) (b (T 1))))",
    );
    fails(
        "(defprotocol P (a (self) -> i64) (b (self) -> i64 1))
         (defstruct S (n: i64))
         (impl P S)
         (defun main () -> i64 0)",
        K::Other,
        "impl P for S is missing the method a",
    );
}
