//! `derive` (§3.16, §4.4): the spec's own examples exactly, then the
//! other protocols, generic and recursive types, and the prelude.

use super::{one, prog, prog_err, read};
use crate::expand::derive::derive;
use crate::expand::error::ExpandErrorKind as K;
use crate::expand::{expand_prelude, expand_program, ExpandCtx, NoRunner};
use crate::syntax::FormKind;

/// Registers `decls` in a fresh context, then runs the `derive` rewrite
/// once on `call` and prints the result (not expanded further).
fn derive_once(decls: &str, call: &str) -> String {
    let mut ctx = ExpandCtx::new();
    expand_program(read(decls), &mut ctx, &mut NoRunner).unwrap_or_else(|e| panic!("{e}"));
    let form = one(call);
    let pos = form.pos.clone();
    let FormKind::List(items) = form.kind else {
        panic!("not a list")
    };
    match derive(&ctx, items, &pos) {
        Ok(f) => f.to_string(),
        Err(e) => panic!("{call}: {e}"),
    }
}

#[test]
fn eq_on_the_generic_struct_is_the_spec_text() {
    let got = derive_once("(defstruct Pair (a b))", "(derive Eq Pair)");
    let spec = "(impl Eq (Pair a b) :where ((Eq a) (Eq b)) \
                (= (self y) (and (fib.prelude/= (. self a) (. y a)) (fib.prelude/= (. self b) (. y b)))) \
                (!= (self y) (fib.prelude/not (fib.prelude/= self y))))";
    assert_eq!(one(&got), one(spec));
}

#[test]
fn eq_on_the_enum_is_the_spec_text_with_gensyms() {
    let got = derive_once(
        "(defenum Shape (Circle r: f64) (Rect w: f64 h: f64))",
        "(derive Eq Shape)",
    );
    let spec = "(impl Eq Shape \
       (= (self y) \
         (match self \
           ((Circle #r.1) (match y ((Circle #r2.2) (fib.prelude/= #r.1 #r2.2)) (_ false))) \
           ((Rect #w.3 #h.4) (match y ((Rect #w2.5 #h2.6) (and (fib.prelude/= #w.3 #w2.5) (fib.prelude/= #h.4 #h2.6))) (_ false))))) \
       (!= (self y) (fib.prelude/not (fib.prelude/= self y))))";
    assert_eq!(
        got,
        spec.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace("( ", "(")
    );
}

#[test]
fn eq_on_a_one_variant_enum_has_no_wildcard() {
    let got = derive_once("(defenum W (Wrap x: i64))", "(derive Eq W)");
    assert_eq!(
        got,
        "(impl Eq W (= (self y) (match self ((Wrap #x.1) (match y ((Wrap #x2.2) (fib.prelude/= #x.1 #x2.2)))))) \
         (!= (self y) (fib.prelude/not (fib.prelude/= self y))))"
    );
}

#[test]
fn ord_on_a_struct_is_lexicographic_and_lists_only_ord() {
    let got = derive_once("(defstruct (P t) (a: i64 b: t c: t))", "(derive Ord P)");
    let less = "(or (fib.prelude/< (. self a) (. y a)) (and (fib.prelude/= (. self a) (. y a)) \
                (or (fib.prelude/< (. self b) (. y b)) (and (fib.prelude/= (. self b) (. y b)) (fib.prelude/< (. self c) (. y c))))))";
    let expected = format!(
        "(impl Ord (P t) :where ((Ord t)) (< (self y) {less}) \
         (<= (self y) (fib.prelude/not (fib.prelude/< y self))) (> (self y) (fib.prelude/< y self)) (>= (self y) (fib.prelude/not (fib.prelude/< self y))))"
    );
    assert_eq!(one(&got), one(&expected));
}

#[test]
fn ord_on_an_enum_orders_variants_then_fields() {
    let got = derive_once("(defenum T (A) (B x: i64))", "(derive Ord T)");
    let expected = "(impl Ord T (< (self y) (match self \
        ((A) (match y ((A) false) ((B _) true))) \
        ((B #x.1) (match y ((A) false) ((B #x2.2) (fib.prelude/< #x.1 #x2.2)))))) \
        (<= (self y) (fib.prelude/not (fib.prelude/< y self))) (> (self y) (fib.prelude/< y self)) (>= (self y) (fib.prelude/not (fib.prelude/< self y))))";
    assert_eq!(
        got,
        expected.split_whitespace().collect::<Vec<_>>().join(" ")
    );
}

#[test]
fn hash_and_show_on_a_struct() {
    let got = derive_once("(defstruct S (a: i64 b: str))", "(derive Hash S)");
    assert_eq!(
        one(&got),
        one("(impl Hash S (hash (self) (fib.prelude/hash-combine \
             (fib.prelude/hash-combine 0 (fib.prelude/hash (. self a))) (fib.prelude/hash (. self b)))))")
    );
    let got = derive_once("(defstruct S (a: i64 b: str))", "(derive Show S)");
    assert_eq!(
        one(&got),
        one(
            "(impl Show S (show (self) (fib.prelude/str-concat \"(S \" (fib.prelude/str-concat (fib.prelude/show (. self a)) \
             (fib.prelude/str-concat \" \" (fib.prelude/str-concat (fib.prelude/show (. self b)) \")\"))))))"
        )
    );
}

#[test]
fn hash_and_show_on_an_enum() {
    let got = derive_once("(defenum T (A) (B x: i64))", "(derive Hash T)");
    assert_eq!(
        got,
        "(impl Hash T (hash (self) (match self ((A) 0) \
         ((B #x.1) (fib.prelude/hash-combine 1 (fib.prelude/hash #x.1))))))"
    );
    let got = derive_once("(defenum T (A) (B x: i64))", "(derive Show T)");
    assert_eq!(
        got,
        "(impl Show T (show (self) (match self ((A) \"A\") \
         ((B #x.1) (fib.prelude/str-concat \"(B \" (fib.prelude/str-concat (fib.prelude/show #x.1) \")\"))))))"
    );
}

#[test]
fn generic_recursive_enum_lists_only_used_parameters() {
    let got = derive_once(
        "(defenum (Tree a b) (leaf) (node v: a l: (Tree a b) r: (Tree a b)))",
        "(derive Eq Tree)",
    );
    assert!(
        got.starts_with("(impl Eq (Tree a b) :where ((Eq a) (Eq b)) "),
        "{got}"
    );
    let got = derive_once("(defenum (Ph a u) (only x: a))", "(derive Hash Ph)");
    assert!(
        got.starts_with("(impl Hash (Ph a u) :where ((Hash a)) "),
        "{got}"
    );
}

#[test]
fn field_less_enum_derives_nothing() {
    assert_eq!(derive_once("(defenum C (R) G B)", "(derive Eq C)"), "(do)");
    assert_eq!(
        prog("(defenum C (R) G B) (derive Show C)"),
        ["(defenum C (R) G B)"]
    );
}

#[test]
fn derive_expands_fully_in_a_program() {
    let out = prog("(defstruct Pair (a b)) (derive Eq Pair)");
    assert_eq!(
        out[1],
        "(impl Eq (Pair a b) :where ((Eq a) (Eq b)) \
         (= (self y) (if (fib.prelude/= (. self a) (. y a)) (fib.prelude/= (. self b) (. y b)) false)) \
         (!= (self y) (fib.prelude/not (fib.prelude/= self y))))"
    );
}

#[test]
fn bad_protocol_and_target() {
    assert_eq!(
        prog_err("(defstruct P (a)) (derive Foo P)"),
        K::DeriveProtocol { name: "Foo".into() }
    );
    assert_eq!(
        prog_err("(derive Eq Nope)"),
        K::DeriveTarget {
            name: "Nope".into()
        }
    );
    assert_eq!(
        prog_err("(derive Eq (P))"),
        K::DeriveTarget { name: "(P)".into() }
    );
    assert!(matches!(prog_err("(derive Eq)"), K::MacroArity { .. }));
}

#[test]
fn derive_sees_only_earlier_definitions() {
    assert_eq!(
        prog_err("(derive Eq Later) (defstruct Later (a))"),
        K::DeriveTarget {
            name: "Later".into()
        }
    );
}

#[test]
fn prelude_expands() {
    let mut ctx = ExpandCtx::new();
    let out = expand_prelude(&mut ctx).unwrap_or_else(|e| panic!("{e}"));
    // the defenum and six impls: Eq, Ord, Hash of Option and of List;
    // Show of both is written by hand in lib/prelude.fib.
    assert_eq!(out.len(), 7);
    assert_eq!(
        out[0].to_string(),
        "(defenum (List a) (Empty) (Cons head: a tail: (List a)))"
    );
    let info = ctx.enum_info("List").unwrap_or_else(|| panic!("no List"));
    let names: Vec<&str> = info.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["Empty", "Cons"]);
    let eq_option = out[1].to_string();
    assert!(
        eq_option.starts_with(
            "(impl Eq (Option a) :where ((Eq a)) (= (self y) (match self ((nil) (match y ((nil) true) (_ false)))"
        ),
        "{eq_option}"
    );
    let eq_list = out[4].to_string();
    assert!(
        eq_list.starts_with(
            "(impl Eq (List a) :where ((Eq a)) (= (self y) (match self ((Empty) (match y ((Empty) true) (_ false)))"
        ),
        "{eq_list}"
    );
    let all: String = out.iter().map(|f| f.to_string()).collect();
    assert!(!all.contains("(cons ") && !all.contains("(empty)"), "{all}");
}
