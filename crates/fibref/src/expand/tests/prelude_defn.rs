//! `defn` and `defn-` (stdlib design §4.2, tranche 1 R6b): each against
//! the `defun` it expands to, and each error with its position.

use super::{prog, prog_err, program};
use crate::expand::error::ExpandErrorKind as K;

fn malformed(head: &str, reason: &'static str) -> K {
    K::Malformed {
        head: head.to_string(),
        reason,
    }
}

#[test]
fn defn_is_defun_with_a_list_of_parameters() {
    assert_eq!(
        prog("(defn f [x: i64 y: i64] -> i64 (+ x y))"),
        ["(defun f (x: i64 y: i64) -> i64 (+ x y))"]
    );
    assert_eq!(prog("(defn f [] 1)"), ["(defun f () 1)"]);
    assert_eq!(prog("(defn f [x] x y)"), ["(defun f (x) x y)"]);
}

#[test]
fn the_docstring_is_dropped_and_what_follows_the_vector_is_passed_on() {
    assert_eq!(prog("(defn f \"adds\" [x] x)"), ["(defun f (x) x)"]);
    assert_eq!(
        prog("(defn f [x: a] :where ((Eq a)) -> bool (= x x))"),
        ["(defun f (x: a) :where ((Eq a)) -> bool (= x x))"]
    );
    // a string that is the body is not a docstring when a vector comes first
    assert_eq!(prog("(defn f [x] \"s\")"), ["(defun f (x) \"s\")"]);
}

#[test]
fn defn_dash_puts_private_after_the_name() {
    assert_eq!(
        prog("(defn- f \"doc\" [x: i64] -> i64 x)"),
        ["(defun f :private (x: i64) -> i64 x)"]
    );
}

#[test]
fn the_body_is_expanded_as_a_defun_body() {
    assert_eq!(
        prog("(defn f [x] (when x 1))"),
        ["(defun f (x) (if x 1 ()))"]
    );
}

#[test]
fn rest_parameters_and_several_arities_are_errors_naming_the_item() {
    let rest = "rest parameters need L2";
    assert_eq!(prog_err("(defn f [x & r] x)"), malformed("defn", rest));
    assert_eq!(prog_err("(defn- f [& r] x)"), malformed("defn-", rest));
    let arities = "several arities need L1";
    let src = "(defn f ([x] 1) ([x y] 2))";
    assert_eq!(prog_err(src), malformed("defn", arities));
}

#[test]
fn a_call_that_is_not_a_defn_is_an_error() {
    let vector = "expected a parameter vector";
    assert_eq!(prog_err("(defn f (x) 1)"), malformed("defn", vector));
    assert_eq!(prog_err("(defn f \"doc\")"), malformed("defn", vector));
    assert_eq!(
        prog_err("(defn 1 [x] x)"),
        malformed("defn", "the name must be a symbol")
    );
    let arity = K::MacroArity {
        name: "defn".into(),
        expected: "at least 2".into(),
        found: 0,
    };
    assert_eq!(prog_err("(defn)"), arity);
    let one = K::MacroArity {
        name: "defn".into(),
        expected: "at least 2".into(),
        found: 1,
    };
    assert_eq!(prog_err("(defn f)"), one);
    assert_eq!(prog_err("(defn f [x])"), malformed("defun", "missing body"));
}

#[test]
fn the_defun_takes_the_call_position_and_the_parameters_the_vectors() {
    let forms = program("\n  (defn f [x]\n   x)").unwrap_or_else(|e| panic!("{e}"));
    let items = forms[0].as_list().unwrap_or(&[]);
    let lc = |f: &crate::syntax::Form| (f.pos.line, f.pos.col);
    assert_eq!(lc(&forms[0]), (2, 3), "the built defun");
    assert_eq!(lc(&items[0]), (2, 3));
    assert_eq!(lc(&items[1]), (2, 9), "the name keeps its own");
    assert_eq!(lc(&items[2]), (2, 11), "the vector's");
    assert_eq!(lc(&items[3]), (3, 4), "the body keeps its own");
}

#[test]
fn the_private_keyword_takes_the_call_position() {
    let forms = program("\n  (defn- f [x]\n   x)").unwrap_or_else(|e| panic!("{e}"));
    let items = forms[0].as_list().unwrap_or(&[]);
    assert_eq!(items[2].to_string(), ":private");
    let at = (items[2].pos.line, items[2].pos.col);
    assert_eq!(at, (2, 3), "the generated keyword is the call's");
}

#[test]
fn error_positions_are_the_offending_form() {
    let at = |src: &str| match program(src) {
        Ok(f) => panic!("{src:?} expanded to {f:?}"),
        Err(e) => (e.pos.line, e.pos.col),
    };
    assert_eq!(at("(defn f [x & r] x)"), (1, 9), "the vector");
    assert_eq!(at("(defn f\n ([x] 1))"), (2, 2), "the clause list");
    assert_eq!(at("(defn 1 [x] x)"), (1, 7), "the name");
    assert_eq!(at("(defn f)"), (1, 1), "the call");
    assert_eq!(
        at("(defn f \"doc\")"),
        (1, 1),
        "the call, nothing after the docstring"
    );
    assert_eq!(
        at("(defn f\n  (x) 1)"),
        (2, 3),
        "the list that is not a vector"
    );
}
