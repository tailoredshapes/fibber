//! Lists, vectors, maps, comments, commas and the six prefix reader
//! macros (§1.1, §1.2).

use super::{err, kind, one, show, sym};
use crate::syntax::{read_all, FormKind, ReadErrorKind};

#[test]
fn collections() {
    assert_eq!(kind("()"), FormKind::List(vec![]));
    assert_eq!(kind("[]"), FormKind::Vec(vec![]));
    assert_eq!(kind("{}"), FormKind::Map(vec![]));
    let FormKind::Map(items) = kind("{:a 1 :b [2 3]}") else {
        panic!("not a map")
    };
    assert_eq!(items.len(), 4);
    assert!(matches!(items[3].kind, FormKind::Vec(ref v) if v.len() == 2));
    assert_eq!(show("( a  (b [c {d e}]) )"), "(a (b [c {d e}]))");
}

#[test]
fn odd_map_is_an_error() {
    assert_eq!(err("{1}"), ReadErrorKind::OddMapEntries { count: 1 });
    assert_eq!(err("{1 2 3}"), ReadErrorKind::OddMapEntries { count: 3 });
    // A form comment does not count as an entry.
    assert_eq!(show("{1 #_2 3}"), "{1 3}");
}

#[test]
fn the_six_prefix_macros() {
    assert_eq!(show("'x"), "(quote x)");
    assert_eq!(show("`x"), "(quasiquote x)");
    assert_eq!(
        show("`(a ,b ,@cs d)"),
        "(quasiquote (a (unquote b) (unquote-splicing cs) d))"
    );
    assert_eq!(show("@x"), "(deref x)");
    assert_eq!(show("&x"), "(& x)");
    assert_eq!(show("@(. p children)"), "(deref (. p children))");
    assert_eq!(show("'(1 2)"), "(quote (1 2))");
    assert_eq!(show("'[a]"), "(quote [a])");
    assert_eq!(show("&v:"), "(& v:)");
    assert_eq!(show("@@x"), "(deref (deref x))");
    assert_eq!(show(",@@x"), "(unquote-splicing (deref x))");
    assert_eq!(show("''x"), "(quote (quote x))");
}

#[test]
fn quote_and_quasiquote_allow_space_and_comments() {
    assert_eq!(show("' x"), "(quote x)");
    assert_eq!(show("` ;c\n x"), "(quasiquote x)");
    assert_eq!(show("'#_a b"), "(quote b)");
}

#[test]
fn prefix_lists_are_ordinary_lists() {
    let f = one("'x");
    let items = f.as_list().expect("list");
    assert_eq!(items[0].kind, sym("quote"));
    assert_eq!(items[1].kind, sym("x"));
    assert_eq!(kind("(quote x)"), f.kind);
}

#[test]
fn commas() {
    assert_eq!(show("[1, 2]"), "[1 2]");
    assert_eq!(show("[1 ,2]"), "[1 (unquote 2)]");
    assert_eq!(show("[1,2]"), "[1 (unquote 2)]");
    assert_eq!(show("(a ,)"), "(a)");
    assert_eq!(show("a,"), "a");
    assert_eq!(show(",,x"), "(unquote (unquote x))");
    assert_eq!(show("(a ,, )"), "(a)");
    assert_eq!(show(", x"), "x");
    assert_eq!(show("{:a 1, :b 2}"), "{:a 1 :b 2}");
    assert_eq!(show(",;c\nx"), "x");
}

#[test]
fn line_comments() {
    assert_eq!(show(";; header\n(a) ; trailing\n; last"), "(a)");
    assert_eq!(show("(a ; inside\n b)"), "(a b)");
    assert_eq!(show("; only a comment"), "");
    assert_eq!(show("a;no-newline-at-end"), "a");
    assert_eq!(show(";\"(\n x"), "x");
}

#[test]
fn form_comments() {
    assert_eq!(show("#_x y"), "y");
    assert_eq!(show("(a #_(b c) d)"), "(a d)");
    assert_eq!(show("(a #_ b c)"), "(a c)");
    assert_eq!(show("#_#_a b c"), "c");
    assert_eq!(show("(#_'x y)"), "(y)");
    assert_eq!(show("#_@x y"), "y");
    assert_eq!(show("[#_ ;c\n x y]"), "[y]");
    assert!(read_all("#_x", "t").expect("reads").is_empty());
}

#[test]
fn quasiquote_nesting_reads_structurally() {
    assert_eq!(
        show("``(a ,,b ,@,c)"),
        "(quasiquote (quasiquote (a (unquote (unquote b)) (unquote-splicing (unquote c)))))"
    );
    assert_eq!(show(",x"), "(unquote x)");
    assert_eq!(
        show("`(do (defstruct ,name ,fields) (derive Eq ,name))"),
        "(quasiquote (do (defstruct (unquote name) (unquote fields)) (derive Eq (unquote name))))"
    );
    assert_eq!(
        show("`[,a {,k ,@v}]"),
        "(quasiquote [(unquote a) {(unquote k) (unquote-splicing v)}])"
    );
}

#[test]
fn top_level_sequence() {
    let forms = read_all("(a) b 1 \"s\" [c]", "t").expect("reads");
    assert_eq!(forms.len(), 5);
    assert!(read_all("", "t").expect("reads").is_empty());
    assert!(read_all("  \n\t ,, ", "t").expect("reads").is_empty());
}
