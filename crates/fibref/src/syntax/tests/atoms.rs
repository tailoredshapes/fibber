//! Symbols, keywords and the literals `true`, `false`, `nil` (§1.1).

use super::{err, kind, show, sym};
use crate::syntax::{FormKind, ReadErrorKind};

#[test]
fn literals_are_never_symbols() {
    assert_eq!(kind("true"), FormKind::Bool(true));
    assert_eq!(kind("false"), FormKind::Bool(false));
    assert_eq!(kind("nil"), FormKind::Nil);
    // Only the exact tokens: these are symbols.
    assert_eq!(kind("nil?"), sym("nil?"));
    assert_eq!(kind("true:"), sym("true:"));
    assert_eq!(kind("Nil"), sym("Nil"));
}

#[test]
fn ordinary_symbols_of_section_1_1() {
    for s in [
        "x",
        "x:",
        "->",
        ".",
        "...",
        "_",
        "-",
        "+",
        "seq/first",
        "/",
        "set!",
        "nil?",
        "a&b",
        "a#b",
        "x:y",
        "*x*",
        "<=",
        "-x",
        "--5",
        ".5",
        "+5",
    ] {
        assert_eq!(kind(s), sym(s), "{s}");
    }
}

#[test]
fn annotation_marker_is_two_forms() {
    assert_eq!(show("(n: Node)"), "(n: Node)");
    let items = super::one("(n: Node)");
    let items = items.as_list().expect("a list");
    assert_eq!(items[0].kind, sym("n:"));
    assert_eq!(items[1].kind, sym("Node"));
    // Without the space it is one symbol (maximal run).
    assert_eq!(kind("n:Node"), sym("n:Node"));
}

#[test]
fn non_ascii_symbols() {
    for s in ["λ", "café", "→", "数", "x′", "😀"] {
        assert_eq!(kind(s), sym(s), "{s}");
    }
}

#[test]
fn symbols_stop_at_terminating_characters() {
    assert_eq!(show("a'b"), "a (quote b)");
    assert_eq!(show("a@b"), "a (deref b)");
    assert_eq!(show("a\"s\""), "a \"s\"");
    assert_eq!(show("a;c\nb"), "a b");
    assert_eq!(show("a(b)"), "a (b)");
}

#[test]
fn slash_rules() {
    for bad in ["a/b/c", "a/", "//", "a//b"] {
        assert_eq!(err(bad), ReadErrorKind::InvalidSymbol(bad.to_string()));
    }
    assert_eq!(err("/a"), ReadErrorKind::InvalidSymbol("/a".to_string()));
}

#[test]
fn keywords() {
    assert_eq!(kind(":borrow"), FormKind::Kw("borrow".to_string()));
    assert_eq!(kind(":ns/name"), FormKind::Kw("ns/name".to_string()));
    assert_eq!(kind(":where"), FormKind::Kw("where".to_string()));
    assert_eq!(kind(":i64"), FormKind::Kw("i64".to_string()));
    assert_eq!(kind(":/"), FormKind::Kw("/".to_string()));
    for bad in [":", "::a", ":a/", ":a/b/c"] {
        assert_eq!(err(bad), ReadErrorKind::InvalidKeyword(bad.to_string()));
    }
    assert_eq!(err(":(a)"), ReadErrorKind::InvalidKeyword(":".to_string()));
}

#[test]
fn ampersand_alone_is_the_reserved_symbol() {
    assert_eq!(kind("&"), sym("&"));
    assert_eq!(show("(& x)"), "(& x)");
    assert_eq!(show("(a &)"), "(a &)");
    assert_eq!(show("[& ,]"), "[&]");
}
