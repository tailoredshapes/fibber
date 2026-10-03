//! Pattern parameters (stdlib spec §7 L7): a parameter that destructures
//! becomes a plain one and a `let` around the body.

use super::{ex, prog, prog_err, program};
use crate::expand::error::ExpandErrorKind as K;

#[test]
fn a_bare_vector_in_a_fn_is_a_fresh_parameter_and_a_let() {
    assert_eq!(
        ex("(fn (acc [k v]) (+ k v))"),
        "(fn (acc #param.1) (let (([k v] #param.1)) (+ k v)))"
    );
    // Two patterns share one let, in parameter order, each with its own name.
    assert_eq!(
        ex("(fn ([a b] [c d]) a)"),
        "(fn (#param.1 #param.2) (let (([a b] #param.1) ([c d] #param.2)) a))"
    );
}

#[test]
fn as_names_the_parameter_and_may_give_its_type() {
    assert_eq!(
        ex("(fn (a ([k v] :as p)) (+ k v))"),
        "(fn (a p) (let (([k v] p)) (+ k v)))"
    );
    assert_eq!(
        ex("(fn (a ([k v] :as p: (Vec i64))) (+ k v))"),
        "(fn (a p: (Vec i64)) (let (([k v] p)) (+ k v)))"
    );
}

#[test]
fn the_return_type_stays_in_front_of_the_let_and_every_body_form_goes_in() {
    assert_eq!(
        ex("(fn (x [a b]) -> i64 (+ a b) x)"),
        "(fn (x #param.1) -> i64 (let (([a b] #param.1)) (+ a b) x))"
    );
}

#[test]
fn a_type_is_not_a_pattern_and_a_fn_without_one_is_left_alone() {
    assert_eq!(
        ex("(fn (x: (Vec i64) y: (Pair i64 i64)) x)"),
        "(fn (x: (Vec i64) y: (Pair i64 i64)) x)"
    );
}

#[test]
fn a_defun_takes_a_typed_pattern_parameter() {
    assert_eq!(
        prog("(defun f (n: i64 ([a b] :as v: (Vec i64))) -> i64 (+ n a))"),
        ["(defun f (n: i64 v: (Vec i64)) -> i64 (let (([a b] v)) (+ n a)))"]
    );
}

#[test]
fn a_defun_pattern_parameter_without_a_type_is_refused() {
    for src in [
        "(defun f ([a b]) -> i64 a)",
        "(defun f (([a b] :as v)) -> i64 a)",
    ] {
        assert_eq!(
            prog_err(src),
            K::Malformed {
                head: "defun".to_string(),
                reason: "a pattern parameter is (pattern :as name: type)",
            },
            "{src}"
        );
    }
    assert!(program("(defun f (([a b] :as v: (Vec i64))) -> i64 1)").is_ok());
}

#[test]
fn a_borrow_qualifier_after_the_type_is_part_of_its_parameter() {
    use super::read;
    use crate::expand::params::arity;
    let ps = |s: &str| read(s).remove(0).as_list().expect("list").to_vec();
    assert_eq!(arity(&ps("(f: (fn (a) b) :borrow x: a)")), 2);
    assert_eq!(arity(&ps("(f: (fn (a) b) :borrow x: a y: a)")), 3);
}
