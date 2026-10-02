//! One test per type error of the catalogue of types §6.14, continued:
//! `match`, `await`, `weak`, constants, `recur`, `impl` contexts, `def`.

use crate::types::ErrorKind as K;

use super::{fails, ok};

#[test]
fn non_exhaustive_match() {
    fails(
        "(defun main () -> i64 (match (some 1) ((some x) x)))",
        K::NonExhaustive,
        "non-exhaustive match: missing nil",
    );
    fails(
        "(defenum Shape (Circle r: i64) (Sq s: i64))
         (defun main () -> i64 (match (Circle 1) ((Circle r) r)))",
        K::NonExhaustive,
        "non-exhaustive match: missing (Sq _)",
    );
    fails(
        "(defun main () -> i64 (match true (true 1)))",
        K::NonExhaustive,
        "non-exhaustive match: missing false",
    );
    fails(
        "(defun main () -> i64 (match 3 (1 1) (2 2)))",
        K::NonExhaustive,
        "non-exhaustive match: missing _",
    );
    fails(
        "(defun main () -> i64 (match (some (some 1)) ((some (some x)) x) (nil 0)))",
        K::NonExhaustive,
        "non-exhaustive match: missing (some nil)",
    );
    ok("(defun main () -> i64 (match (some 1) ((some x) x) (nil 0)))");
}

#[test]
fn redundant_match_clause() {
    fails(
        "(defun main () -> i64 (match true (true 1) (false 2) (_ 3)))",
        K::Redundant,
        "redundant match clause",
    );
    fails(
        "(defun main () -> i64 (match 1 (x x) (2 2)))",
        K::Redundant,
        "redundant match clause",
    );
}

#[test]
fn await_outside_async() {
    fails(
        "(defun main () -> i64 (await (async 1)))",
        K::AwaitOutsideAsync,
        "await outside async",
    );
    // Inside a fn nested in an async.
    fails(
        "(defun main () -> i64 (block-on (async ((fn () (await (async 1)))))))",
        K::AwaitOutsideAsync,
        "await outside async",
    );
    // A loop body inside the async is part of it (proposed case 32).
    ok("(defun main () -> i64 (block-on (async (dotimes (i 3) (await (yield))) 3)))");
}

#[test]
fn weak_requires_an_object_type() {
    fails(
        "(defun main () -> i64 (do (weak 1) 0))",
        K::WeakScalar,
        "weak requires an object type",
    );
    fails(
        "(defenum Colour Red Green) (defun main () -> i64 (do (weak Red) 0))",
        K::WeakScalar,
        "weak requires an object type",
    );
}

#[test]
fn weak_of_an_option_is_not_allowed() {
    // Decided (owner, 2026-09-27; case 82): an Option has no object of
    // its own for a weak reference to observe.
    let text = "weak of an Option is not allowed";
    fails(
        "(defun main () -> i64 (do (weak (some \"a\")) 0))",
        K::WeakOption,
        text,
    );
    fails(
        "(defun main () -> i64 (do (weak nil) 0))",
        K::WeakOption,
        text,
    );
    // Through a generic function: the bound travels in its scheme.
    fails(
        "(defun w (x) (weak x)) (defun main () -> i64 (do (w (some 5)) 0))",
        K::WeakOption,
        text,
    );
    // The payload itself is fine.
    ok("(defun main () -> i64 (do (weak \"a\") 0))");
}

#[test]
fn dyn_requires_an_object_type() {
    // Decided (owner, 2026-09-27; case 90): a scalar has no object word
    // for the dyn value's count operations to act on.
    let decls = "(defprotocol Q (q (self) -> i64)) (impl Q i64 (q (self) self)) \
                 (defenum E A B) (impl Q E (q (self) 1)) (impl Q unit (q (self) 2)) \
                 (impl Q (Option a) (q (self) 3))";
    for arg in ["7", "A", "()"] {
        let src = format!("{decls} (defun main () -> i64 (q (dyn Q {arg})))");
        fails(&src, K::NotObject, "dyn requires an object type");
    }
    // Through a generic function the bound is (Object a) in its scheme.
    let src = format!("{decls} (defun wrap (x) (dyn Q x)) (defun main () -> i64 (q (wrap 7)))");
    fails(&src, K::NotObject, "wrap requires an object type");
    // An Option is an object type: dyn of it is fine.
    ok(&format!(
        "{decls} (defun main () -> i64 (q (dyn Q (some 1))))"
    ));
}

#[test]
fn constant_is_not_a_function() {
    fails(
        "(defun main () -> i64 (vec-count (Empty)))",
        K::ConstantCalled,
        "Empty is a constant, not a function; write Empty",
    );
}

#[test]
fn recur_outside_loop_and_not_in_tail_position() {
    fails(
        "(defun main () -> i64 (recur 1))",
        K::RecurOutsideLoop,
        "recur outside loop",
    );
    fails(
        "(defun main () -> i64 (loop ((i 0)) ((fn () (recur 1)))))",
        K::RecurOutsideLoop,
        "recur outside loop",
    );
    fails(
        "(defun main () -> i64 (loop ((i 0)) (+ 1 (recur (+ i 1)))))",
        K::RecurNotTail,
        "recur not in tail position",
    );
}

#[test]
fn impl_body_needs_a_where_bound() {
    fails(
        "(defprotocol Describe (describe (self) -> str))
         (defstruct (Wrap a) (v: a))
         (impl Describe (Wrap a) (describe (self) (describe (. self v))))
         (defun main () -> i64 0)",
        K::ImplContext,
        "no implementation of Describe for a; add (Describe a) to the :where of the impl",
    );
    ok("(defprotocol Describe (describe (self) -> str))
        (defstruct (Wrap a) (v: a))
        (impl Describe i64 (describe (self) \"n\"))
        (impl Describe (Wrap a) :where ((Describe a)) (describe (self) (describe (. self v))))
        (defun main () -> i64 (str-len (describe (Wrap 1))))");
}

#[test]
fn def_has_an_unresolved_type() {
    fails(
        "(def e []) (defun main () -> i64 0)",
        K::DefUnresolved,
        "def e has an unresolved type; annotate it",
    );
    fails(
        "(defun double (x) (+ x x)) (def twice-fn double) (defun main () -> i64 0)",
        K::DefUnresolved,
        "def twice-fn has an unresolved type; annotate it",
    );
    let p = ok("(defun double (x) (+ x x)) (def twice-fn: (fn (i64) i64) double) (defun main () -> i64 (twice-fn 21))");
    let t = p.def_type("twice-fn").expect("typed");
    assert_eq!(
        crate::types::display::Printer::new(&p.globals).ty(t),
        "(fn :send (i64) i64)"
    );
    ok("(def e: (Vec i64) []) (defun main () -> i64 (vec-count e))");
}

#[test]
fn def_initialiser_is_not_a_constant_expression() {
    fails(
        "(def c (cell 0)) (defun main () -> i64 0)",
        K::DefNotConstant,
        "def c: initialiser is not a constant expression",
    );
    fails(
        "(def f (fn () 1)) (defun main () -> i64 0)",
        K::DefNotConstant,
        "def f: initialiser is not a constant expression",
    );
    ok("(def primes [2 3 5]) (defun main () -> i64 (vec-nth primes 0))");
}

#[test]
fn def_and_defun_depend_on_each_other() {
    fails(
        "(def t [f]) (defun f () (vec-count t)) (defun main () -> i64 (f))",
        K::DefCycle,
        "def t and defun f depend on each other",
    );
}
