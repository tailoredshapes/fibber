//! Colour parameters in `impl` heads (types §1.3, §4.1, §5.4; owner's
//! decision of 2026-09-28): a head's colour variable is rigid in the
//! bodies, and a head may give a colour instead.

use crate::types::ErrorKind as K;

use super::{fails, ok};

const HOOK: &str = "(defstruct (Hook k :colour) (f: (fn k (i64) i64) tag: i64))
                    (defstruct (Slot k :colour) (c: (Cell (fn k () i64))))
                    (defun inc1 (x: i64) -> i64 (+ x 1))
                    (defun usel (h: (Hook :local)) -> i64 (. h tag))
                    (defprotocol Score (score (self) -> i64))";

fn with(body: &str) -> String {
    format!("{HOOK} {body} (defun main () -> i64 0)")
}

#[test]
fn a_local_closure_may_not_flow_into_a_rigid_colour() {
    fails(
        &with(
            "(defprotocol P (poison (self d: (Cell i64)) -> unit))
               (impl P (Slot k) (poison (self d) (set! (. self c) (fn () @d))))",
        ),
        K::RigidColour,
        "local closure where colour k is required: closure capture d has type (Cell i64)",
    );
}

#[test]
fn a_send_closure_flows_into_a_rigid_colour_and_joins_self() {
    ok(&with(
        "(defprotocol P (reset (self) -> unit) (same (self) -> Self))
         (impl P (Slot k)
           (reset (self) (set! (. self c) (fn () 42)))
           (same (self) (if true self (Slot (cell (fn () 1))))))",
    ));
    ok(&with(
        "(impl Score (Hook k) (score (self) (. (if true self (Hook inc1 0)) tag)))",
    ));
}

#[test]
fn a_rigid_colour_equals_only_itself() {
    fails(
        &with("(impl Score (Hook k) (score (self) (usel self)))"),
        K::Unify,
        "cannot unify (Hook k) with (Hook :local)",
    );
    fails(
        &with(
            "(defprotocol P (as-send (self) -> (Hook :send)))
               (impl P (Hook k) (as-send (self) self))",
        ),
        K::Unify,
        "cannot unify (Hook k) with (Hook :send)",
    );
}

#[test]
fn a_rigid_colour_is_not_send() {
    fails(
        &with("(defprotocol R (run (self) -> i64))
               (impl R (Hook k) (run (self) (join (spawn (fn () ((. self f) 1))))))"),
        K::RigidColour,
        "closure of colour k cannot be shared between threads: closure capture self, field f of Hook",
    );
}

#[test]
fn two_rigid_colours_are_unordered() {
    fails(
        "(defstruct (Two j :colour k :colour) (a: (fn j () i64) b: (Cell (fn k () i64))))
         (defprotocol P (mix (self) -> unit))
         (impl P (Two j k) (mix (self) (set! (. self b) (. self a))))
         (defun main () -> i64 0)",
        K::RigidColour,
        "closure of colour j where colour k is required",
    );
}

#[test]
fn a_head_that_gives_a_colour_covers_only_that_colour() {
    ok(&with(
        "(impl Score (Hook :local) (score (self) (usel self)))
         (defun use-it () -> i64 (score (Hook inc1 1)))",
    ));
    fails(
        &with(
            "(impl Score (Hook :local) (score (self) (usel self)))
             (defun use-it (h: (Hook :send)) -> i64 (score h))",
        ),
        K::NoInstance,
        "no implementation of Score for (Hook :send)",
    );
    ok(&with(
        "(defprotocol R (run (self) -> i64))
         (impl R (Hook :send) (run (self) (join (spawn (fn () ((. self f) 1))))))",
    ));
}

#[test]
fn one_instance_per_constructor_whatever_the_colour() {
    fails(
        &with(
            "(impl Score (Hook :local) (score (self) 1))
             (impl Score (Hook k) (score (self) 2))",
        ),
        K::Other,
        "overlapping instances: Score for (Hook k)",
    );
}

#[test]
fn supertraits_must_cover_the_colours_of_the_impl() {
    let src = "(defprotocol Base (base (self) -> i64))
               (defprotocol Top :requires (Base) (top (self) -> i64))
               (impl Base (Hook :local) (base (self) 1))";
    fails(
        &with(&format!("{src} (impl Top (Hook k) (top (self) 2))")),
        K::Other,
        "impl Top for (Hook k) requires an impl of Base for (Hook k); \
         impl Base for (Hook :local) covers only (Hook :local)",
    );
    ok(&with(&format!(
        "{src} (impl Top (Hook :local) (top (self) 2))"
    )));
}

#[test]
fn a_determined_argument_carries_the_rigid_colour() {
    ok(&with(
        "(defprotocol (Twin s t) (twin (self) -> t))
         (impl (Twin (Hook k)) (Hook k) (twin (self) (Hook (. self f) 9)))",
    ));
    // A k closure may be viewed at :local, never at :send.
    ok(&with(
        "(defprotocol (Twin s t) (twin (self) -> t))
         (impl (Twin (Hook :local)) (Hook k) (twin (self) (Hook (. self f) 9)))",
    ));
    fails(
        &with(
            "(defprotocol (Twin s t) (twin (self) -> t))
             (impl (Twin (Hook :send)) (Hook k) (twin (self) (Hook (. self f) 9)))",
        ),
        K::RigidColour,
        "closure of colour k cannot be shared between threads",
    );
    fails(
        &with(
            "(defprotocol (Box s t) (unbox (self) -> t)) (impl (Box k) (Hook k) (unbox (self) 1))",
        ),
        K::Resolve,
        "k is a colour parameter of the impl head, not a type",
    );
}
