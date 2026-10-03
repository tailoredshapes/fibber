use super::*;

/// Completes at the `|` in `src` (the marker is removed).
fn at(src: &str) -> Vec<Item> {
    let off = src.find('|').expect("a cursor");
    let text = src.replacen('|', "", 1);
    let (line, col) = super::super::text::line_col(&text, off);
    complete(&text, "/nonexistent/t.fib", line, col, &Roots::default())
}

fn find<'a>(items: &'a [Item], label: &str) -> Option<&'a Item> {
    items.iter().find(|i| i.label == label)
}

#[test]
fn a_cursor_in_a_let_body_sees_the_parameters_and_bindings_and_the_library() {
    let items = at("(defun f (a: i64) -> i64 (let ((x 1)) (+ |");
    assert_eq!(find(&items, "a").map(|i| i.kind), Some("variable"));
    assert_eq!(find(&items, "x").map(|i| i.kind), Some("variable"));
    let map = find(&items, "map").expect("the library's map");
    assert_eq!(map.kind, "function");
    assert!(
        map.detail.starts_with("∀") || map.detail.contains("fn"),
        "{}",
        map.detail
    );
    assert!(find(&items, "let").is_some_and(|i| i.kind == "macro"));
}

#[test]
fn a_buffer_that_does_not_read_still_completes_its_locals_and_the_library() {
    let items = at("(defun g (x: i64) -> i64 (let ((z 1)) (foo |");
    assert!(find(&items, "x").is_some() && find(&items, "z").is_some());
    assert!(find(&items, "map").is_some());
    let in_string = at("(defun g (x: i64) -> i64 \"unterminated |");
    assert!(find(&in_string, "map").is_some());
}

#[test]
fn fields_follow_a_dot_form_by_the_annotation_of_its_value() {
    let items = at("(defstruct P (x: i64 name: str))\n(defun f (p: P) -> i64 (. p |");
    let fields: Vec<_> = items
        .iter()
        .map(|i| (i.label.as_str(), i.kind, i.detail.as_str()))
        .collect();
    assert_eq!(fields, [("x", "field", "i64"), ("name", "field", "str")]);
}

#[test]
fn types_follow_a_name_with_a_colon_and_an_arrow() {
    let items = at("(defstruct P (x: i64))\n(defun f (p: |");
    assert!(find(&items, "P").is_some_and(|i| i.kind == "struct"));
    assert!(find(&items, "i64").is_some_and(|i| i.kind == "type"));
    assert!(find(&items, "map").is_none());
    assert!(find(&at("(defun f () -> |"), "str").is_some());
}

#[test]
fn an_alias_prefix_lists_that_modules_exports() {
    let items = at("(ns t (:require [fib.seq :as seq]))\n(defun f () -> i64 (seq/|");
    assert!(find(&items, "map").is_some_and(|i| i.kind == "function"));
    assert!(find(&items, "let").is_none());
}

#[test]
fn keywords_come_from_the_file_but_not_the_word_being_typed() {
    let items = at("(f :alpha :beta)\n(g :al|");
    let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, [":alpha", ":beta"]);
}

#[test]
fn module_definitions_of_a_buffer_that_does_not_check_are_listed() {
    let items = at("(defun helper (a: i64) -> i64 a)\n(defun broken () -> i64 (+ 1 \"s\"))\n(defun h () -> i64 (|");
    let h = find(&items, "helper").expect("helper");
    assert_eq!((h.kind, h.detail.as_str()), ("function", "(a: i64) -> i64"));
}

#[test]
fn hover_shows_the_scheme_of_a_global_and_the_type_of_a_local() {
    let text = "(defun f (n: i64) -> i64 (inc n))\n(defun inc (k: i64) -> i64 (+ k 1))\n";
    let roots = Roots::default();
    let at = |line, col| hover(text, "/nonexistent/t.fib", line, col, &roots);
    assert!(
        at(1, 27).is_some_and(|h| h.starts_with("inc : (fn") && h.contains("i64")),
        "{:?}",
        at(1, 27)
    );
    assert!(at(1, 30).is_some_and(|h| h == "n : i64"), "{:?}", at(1, 30));
    assert_eq!(at(1, 0), None);
}
