//! Hygiene of the prelude macros (stdlib design C-4, tranche 1 R2): the
//! heads an expansion emits are core forms, names the call's own text
//! uses, gensyms, or `fib.prelude/NAME`: never a bare `cons`, `trap`,
//! `show`, `+`, `<`, that a program's or a library's own binding of the
//! name would capture.
//!
//! One sample call per prelude macro is expanded and its output walked.
//! A macro added to the registry without a sample here fails
//! `every_prelude_macro_has_a_sample`, so the next package cannot add a
//! macro that this check does not see.

use std::collections::BTreeSet;

use super::{program, read};
use crate::expand::{is_core, PRELUDE_MACROS};
use crate::syntax::{Form, FormKind};

/// The program that uses each macro. It writes none of the names the
/// expansions call (`cons trap show + < = not hash str-concat spawn join
/// eprintln some`), so a bare one in the output is the
/// macro's.
const SAMPLES: &[(&str, &str)] = &[
    ("when", "(defun m () (when a b c))"),
    ("unless", "(defun m () (unless a b c))"),
    ("cond", "(defun m () (cond (a b) (c d)))"),
    ("and", "(defun m () (and a b c))"),
    ("or", "(defun m () (or a b c))"),
    ("if-let", "(defun m () (if-let (x a) b c))"),
    ("when-let", "(defun m () (when-let (x a) b c))"),
    ("list", "(defun m () (list a b))"),
    ("plet", "(defun m () (plet ((s a) (t b)) (f s t)))"),
    ("while", "(defun m () (while a b c))"),
    ("dotimes", "(defun m () (dotimes (i n) (f i)))"),
    (
        "for-each",
        "(defun m () (for-each (range a b) (fn (i) (f i))))",
    ),
    ("range", "(defun m () (range a b))"),
    ("->", "(defun m () (-> a (f b) g))"),
    ("->>", "(defun m () (->> a (f b) g))"),
    ("doto", "(defun m () (doto (mk) (f a) g))"),
    ("assert", "(defun m () (assert (f a) \"no\"))"),
    ("dbg", "(defun m () (dbg (f a)))"),
    (
        "derive",
        "(defstruct (P t) (a: i64 b: t)) (derive Eq P) (derive Ord P) (derive Hash P) \
         (derive Show P) (defenum T (K) (L x: i64 y: str)) (derive Eq T) (derive Ord T) \
         (derive Hash T) (derive Show T) (derive Debug P) (derive ToStr P) (derive Debug T) \
         (derive ToStr T)",
    ),
    ("defn", "(defn m \"doc\" [x: i64] -> i64 (f x))"),
    ("defn-", "(defn- m [x] (f x))"),
    (
        "update",
        "(defun m () (do (update a b f x) (update a b (fnil g d)) (update a b (fnil g d) x)))",
    ),
    (
        "reduce",
        "(defun m () (do (reduce f c) (reduce + c) (reduce (fn (a x) (if p (reduced a) (g a x))) 0 c)))",
    ),
    ("str", "(defun m () (str a \"s\" nil (f b)))"),
    ("println", "(defun m () (println a \"s\" nil (f b)))"),
    ("print", "(defun m () (print a \"s\" nil (f b)))"),
    ("prn", "(defun m () (prn a \"s\" nil (f b)))"),
    ("pr", "(defun m () (pr a \"s\" nil (f b)))"),
    ("+", "(defun m () (+ a b c))"),
    ("-", "(defun m () (- a b c))"),
    ("*", "(defun m () (* a b c))"),
    ("<", "(defun m () (< a (f b) c))"),
    (">", "(defun m () (> a (f b) c))"),
    ("<=", "(defun m () (<= a (f b) c))"),
    (">=", "(defun m () (>= a (f b) c))"),
    ("=", "(defun m () (= a (f b) c))"),
    ("max", "(defun m () (max a b c))"),
    ("min", "(defun m () (min a b c))"),
    ("bit-and", "(defun m () (bit-and a b c))"),
    ("bit-or", "(defun m () (bit-or a b c))"),
    ("bit-xor", "(defun m () (bit-xor a b c))"),
    ("conj", "(defun m () (conj c x y))"),
    ("assoc", "(defun m () (assoc m a 1 b 2))"),
    ("dissoc", "(defun m () (dissoc m a b))"),
    ("merge", "(defun m () (merge a nil b c))"),
    ("swap!", "(defun m () (swap! a f x))"),
    // tranche 2, X3
    ("if-not", "(defun m () (if-not a b c))"),
    ("when-not", "(defun m () (when-not a b c))"),
    ("some", "(defun m () (some a b))"),
    ("declare", "(defun m () (declare a b))"),
    ("if-some", "(defun m () (if-some [x a] b c))"),
    ("when-some", "(defun m () (when-some [x a] b c))"),
    ("when-first", "(defun m () (when-first [x a] b c))"),
    ("try-let", "(defun m () (try-let ((x a) (y b)) (f x y)))"),
    ("for", "(defun m () (for [x a :when (f x) y b :let [z x] :when (g z)] (h z y)))"),
    ("doseq", "(defun m () (doseq [x a :let [y (f x)] :when (g y)] (h x y)))"),
    ("vector", "(defun m () (vector a b))"),
    ("hash-set", "(defun m () (hash-set a b))"),
    ("array-map", "(defun m () (array-map a b c d))"),
    ("hash-map", "(defun m () (hash-map a b c d))"),
    ("max-key", "(defun m () (max-key k a b c))"),
    ("min-key", "(defun m () (min-key k a b c))"),
    ("vswap!", "(defun m () (vswap! v f x))"),
    ("print-str", "(defun m () (print-str a b))"),
    ("pr-str", "(defun m () (pr-str a b))"),
    ("println-str", "(defun m () (println-str a b))"),
    ("prn-str", "(defun m () (prn-str a b))"),
];

/// Every symbol the text uses, anywhere.
fn text_symbols(forms: &[Form], out: &mut BTreeSet<String>) {
    for f in forms {
        match &f.kind {
            FormKind::Sym(s) => {
                out.insert(s.clone());
            }
            FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => {
                text_symbols(items, out)
            }
            _ => {}
        }
    }
}

/// Whether `f` is an impl's method clause `(name (params) body)`, whose
/// head is a definition, not a reference.
fn is_method_clause(f: &Form) -> bool {
    match f.as_list() {
        Some([name, params, _, ..]) => name.as_sym().is_some() && params.as_list().is_some(),
        _ => false,
    }
}

/// The symbol heads of every call in `f`, in order (method names of an
/// `impl` are definitions and are skipped).
fn heads(f: &Form, out: &mut Vec<String>) {
    let items = match &f.kind {
        FormKind::List(items) | FormKind::Vec(items) | FormKind::Map(items) => items,
        _ => return,
    };
    let is_impl = f.as_list().and_then(|l| l.first()).and_then(Form::as_sym) == Some("impl");
    if let (FormKind::List(_), Some(h)) = (&f.kind, items.first().and_then(Form::as_sym)) {
        out.push(h.to_string());
    }
    for item in items {
        if is_impl && is_method_clause(item) {
            let body = item.as_list().unwrap_or(&[]);
            body[1..].iter().for_each(|b| heads(b, out));
        } else {
            heads(item, out);
        }
    }
}

/// The heads of `out` that are not core forms, gensyms, `self` and `y`
/// (the derived methods' parameters), `_` (a wildcard clause), qualified
/// into the prelude or a library facade (`fib.core`: R5's `to-str` and
/// `debug`, R6b's `Done` and `More`; `fib.seq` and `fib.coll`: R6b's
/// `reduce-while`, `update-or`), or names
/// the sample's own text uses.
fn bare_heads(text: &BTreeSet<String>, out: &[Form]) -> Vec<String> {
    let mut all = Vec::new();
    out.iter().for_each(|f| heads(f, &mut all));
    all.retain(|h| {
        let own = text.contains(h) || ["self", "y", "_"].contains(&h.as_str());
        let qualified = ["fib.prelude/", "fib.core/", "fib.seq/", "fib.coll/"]
            .iter()
            .any(|ns| h.starts_with(ns));
        !(is_core(h) || h.starts_with('#') || qualified || own)
    });
    all
}

#[test]
fn every_prelude_macro_has_a_sample() {
    let sampled: Vec<&str> = SAMPLES.iter().map(|(n, _)| *n).collect();
    assert_eq!(sampled, PRELUDE_MACROS, "one sample per macro, in order");
}

#[test]
fn every_head_a_prelude_macro_emits_is_qualified() {
    for (name, src) in SAMPLES {
        let mut text = BTreeSet::new();
        text_symbols(&read(src), &mut text);
        let out = program(src).unwrap_or_else(|e| panic!("{name}: {src}: {e}"));
        let bare = bare_heads(&text, &out);
        let shown: Vec<String> = out.iter().map(|f| f.to_string()).collect();
        assert!(
            bare.is_empty(),
            "{name} emitted bare heads {bare:?}: {shown:?}"
        );
        assert!(!out.is_empty(), "{name} expanded to nothing");
    }
}

#[test]
fn the_walk_flags_a_bare_head() {
    // The check can fail: the old expansions, written by hand, are caught.
    let old = read("(defun m () (cons a (cons b empty)))");
    let mut text = BTreeSet::new();
    text_symbols(&read("(defun m () (list a b))"), &mut text);
    assert_eq!(bare_heads(&text, &old), ["cons", "cons"]);
    let derived =
        read("(impl Eq P (= (self y) (= (. self a) (. y a))) (!= (self y) (not (= self y))))");
    let mut text = BTreeSet::new();
    text_symbols(&read("(derive Eq P)"), &mut text);
    assert_eq!(bare_heads(&text, &derived), ["=", "not", "="]);
}

#[test]
fn an_operator_macro_emits_none_of_its_own_bare_name() {
    // The sample's text must write the operator, so the check above would
    // let a bare `+` through (R6a): these samples call it with arguments
    // that are not calls of it, so every head of the output is the
    // expansion's.
    for name in [
        "+", "-", "*", "<", ">", "<=", ">=", "=", "max", "min", "bit-and", "bit-or", "bit-xor",
        "conj", "assoc", "dissoc", "merge", "swap!",
    ] {
        let src = SAMPLES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s);
        let out = program(src.unwrap_or("")).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut all = Vec::new();
        out.iter().for_each(|f| heads(f, &mut all));
        assert!(!all.iter().any(|h| h == name), "{name}: heads {all:?}");
    }
}
