//! The quasiquote rewrite (§3.16), before and after the §1.4 rewrite.

use super::{ex, ex_err, one, v};
use crate::expand::error::ExpandErrorKind as K;
use crate::expand::quasi::rewrite;

/// The rewrite alone, printed (vector literals not yet rewritten).
fn qq(src: &str) -> String {
    match rewrite(one(src)) {
        Ok(f) => f.to_string(),
        Err(e) => panic!("{src:?}: {e}"),
    }
}

#[test]
fn atom_is_quoted() {
    assert_eq!(qq("`a"), "(quote a)");
    assert_eq!(qq("`1"), "(quote 1)");
    assert_eq!(qq("`nil"), "(quote nil)");
    assert_eq!(qq("`:k"), "(quote :k)");
}

#[test]
fn the_spec_example() {
    assert_eq!(
        qq("`(a ,b ,@cs d)"),
        "(List (concat [(quote a)] [b] cs [(quote d)]))"
    );
}

#[test]
fn vectors_and_maps_use_their_constructors() {
    assert_eq!(qq("`[a ,b]"), "(Vec (concat [(quote a)] [b]))");
    assert_eq!(qq("`{k ,v}"), "(Map (concat [(quote k)] [v]))");
    assert_eq!(qq("`[,@xs]"), "(Vec (concat xs))");
    assert_eq!(qq("`()"), "(List (concat))");
}

#[test]
fn unquote_at_top_is_the_operand() {
    assert_eq!(qq("`,x"), "x");
    assert_eq!(qq("`,(f x)"), "(f x)");
}

#[test]
fn nested_lists_are_rebuilt() {
    assert_eq!(
        qq("`(a (b ,c))"),
        "(List (concat [(quote a)] [(List (concat [(quote b)] [c]))]))"
    );
}

#[test]
fn nested_quasiquote_evaluates_only_level_one() {
    // `(a `(b ,(c ,d))): the inner , belongs to the inner quasiquote and
    // is kept as data, but its ,d is at level one and is evaluated.
    let inner_c = "(List (concat [(quote c)] [d]))";
    let unq = format!("(List (concat [(quote unquote)] [{inner_c}]))");
    let b = format!("(List (concat [(quote b)] [{unq}]))");
    let qq_b = format!("(List (concat [(quote quasiquote)] [{b}]))");
    let expected = format!("(List (concat [(quote a)] [{qq_b}]))");
    assert_eq!(qq("`(a `(b ,(c ,d)))"), expected);
}

#[test]
fn nested_quasiquote_keeps_deep_unquotes_as_data() {
    // `(a `(b ,c)): c is at level two, so the result holds (unquote c).
    let unq = "(List (concat [(quote unquote)] [(quote c)]))";
    let b = format!("(List (concat [(quote b)] [{unq}]))");
    let qq_b = format!("(List (concat [(quote quasiquote)] [{b}]))");
    assert_eq!(
        qq("`(a `(b ,c))"),
        format!("(List (concat [(quote a)] [{qq_b}]))")
    );
}

#[test]
fn nested_splice_at_level_one() {
    // `(x `(y ,@(z ,@w))): w is spliced now, into the list (z ...).
    let z = "(List (concat [(quote z)] w))";
    let uqs = format!("(List (concat [(quote unquote-splicing)] [{z}]))");
    let y = format!("(List (concat [(quote y)] [{uqs}]))");
    let qq_y = format!("(List (concat [(quote quasiquote)] [{y}]))");
    assert_eq!(
        qq("`(x `(y ,@(z ,@w)))"),
        format!("(List (concat [(quote x)] [{qq_y}]))")
    );
}

#[test]
fn splice_as_the_whole_template_is_an_error() {
    let e = ex_err("`,@xs");
    assert_eq!(e.kind, K::SpliceOutsideList);
    assert_eq!((e.pos.line, e.pos.col), (1, 2));
}

#[test]
fn bad_unquote_arity_is_malformed() {
    let e = ex_err("`(a (unquote b c))");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "unquote"));
    let e = ex_err("(quasiquote a b)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "quasiquote"));
}

#[test]
fn after_expansion_vectors_are_library_calls() {
    let a = v(&["(quote a)"]);
    let b = v(&["b"]);
    assert_eq!(ex("`(a ,b)"), format!("(List (concat {a} {b}))"));
}

#[test]
fn unquote_operands_are_expanded() {
    let x = v(&["(if p q false)"]);
    assert_eq!(ex("`(,(and p q))"), format!("(List (concat {x}))"));
}

#[test]
fn quote_keeps_brackets_and_unquotes_as_data() {
    assert_eq!(ex("'(a ,b [c] {d e})"), "(quote (a (unquote b) [c] {d e}))");
}

#[test]
fn unquote_outside_quasiquote_is_an_error() {
    let e = ex_err("(f ,x)");
    assert_eq!(e.kind, K::UnquoteOutsideQuasiquote { head: "unquote" });
    assert_eq!((e.pos.line, e.pos.col), (1, 4));
    let e = ex_err("(f ,@x)");
    assert_eq!(
        e.kind,
        K::UnquoteOutsideQuasiquote {
            head: "unquote-splicing"
        }
    );
}
