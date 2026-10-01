//! Which positions of core forms are expanded, top-level splicing, the
//! §1.4 rewrite, `nil`, `&` and the primitive forms.

use super::{ex, prog, v};

#[test]
fn literal_collections_become_library_calls() {
    assert_eq!(ex("[]"), "(fib.prelude/vec-empty)");
    assert_eq!(ex("[1 2 3]"), v(&["1", "2", "3"]));
    assert_eq!(ex("{}"), "(fib.prelude/map-empty)");
    assert_eq!(
        ex("{:a 1 :b 2}"),
        "(fib.prelude/assoc (fib.prelude/assoc (fib.prelude/map-empty) :a 1) :b 2)"
    );
    assert_eq!(ex("[[a]]"), v(&[&v(&["a"])]));
}

#[test]
fn nil_symbol_is_the_nil_form() {
    let form = crate::expand::expand_expr(
        crate::syntax::Form::new(
            crate::syntax::FormKind::Sym("nil".into()),
            super::one("x").pos,
        ),
        &mut crate::expand::ExpandCtx::new(),
        &mut crate::expand::NoRunner,
    );
    let form = form.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(form.kind, crate::syntax::FormKind::Nil);
}

#[test]
fn let_fn_match_loop_positions() {
    assert_eq!(
        ex("(let ((x (when a b)) (y [1])) (fn f (p) -> i64 (and p x)))"),
        format!(
            "(let ((x (if a b ())) (y {})) (fn f (p) -> i64 (if p x false)))",
            v(&["1"])
        )
    );
    assert_eq!(
        ex("(match (or a b) ((some (list x)) (list x)) (nil []))"),
        format!(
            "(match (if a true b) ((some (list x)) (cons x empty)) (nil {}))",
            v(&[])
        )
    );
    assert_eq!(
        ex("(loop ((i 0)) (if (< i 3) (recur (+ i 1)) (list i)))"),
        "(loop ((i 0)) (if (< i 3) (recur (+ i 1)) (cons i empty)))"
    );
    assert_eq!(ex("(. (-> x f) (list a))"), "(. (f x) (list a))");
    assert_eq!(
        ex("(async (await (list)) [])"),
        format!("(async (await empty) {})", v(&[]))
    );
    assert_eq!(ex("(unsafe (list))"), "(unsafe empty)");
}

#[test]
fn primitive_operands_are_left_alone() {
    assert_eq!(
        ex("(set-field! &c x (list x))"),
        "(set-field! (& c) x (cons x empty))"
    );
    assert_eq!(ex("(set-field! &c list 1)"), "(set-field! (& c) list 1)");
    assert_eq!(ex("(dyn Show [1])"), format!("(dyn Show {})", v(&["1"])));
    assert_eq!(ex("(trunc i8 (list))"), "(trunc i8 empty)");
    assert_eq!(ex("(sitofp (list) x)"), "(sitofp (list) x)");
}

#[test]
fn in_out_arguments_stay() {
    assert_eq!(ex("(f &x @x)"), "(f (& x) (deref x))");
    assert_eq!(ex("((g) &x)"), "((g) (& x))");
}

#[test]
fn nil_in_patterns_is_the_nil_form() {
    // A macro can only build (Sym "nil"); a pattern (nil) reads as
    // (List [(Nil)]) and stays so.
    assert_eq!(
        ex("(match o ((nil) 0) ((some x) x))"),
        "(match o ((nil) 0) ((some x) x))"
    );
}

#[test]
fn definitions_expand_their_bodies() {
    let out = prog(
        "(ns demo (:require [lib :as l]))
         (defstruct P (a: i64 b))
         (defenum E (A) (B x: i64))
         (defprotocol Q (q (self) -> i64))
         (extern puts (ptr) -> i32)
         (def t [1])
         (def u: i64 (-> 1 inc))
         (defun f (x &y: i64) :where ((Eq a)) -> i64 (when x [1]) (list))
         (impl Q P (q (self) -> i64 (list)) (r (self) (and a b)))",
    );
    assert_eq!(out.len(), 9);
    assert_eq!(out[0], "(ns demo (:require [lib :as l]))");
    assert_eq!(out[5], format!("(def t {})", v(&["1"])));
    assert_eq!(out[6], "(def u: i64 (inc 1))");
    assert_eq!(
        out[7],
        format!(
            "(defun f (x (& y:) i64) :where ((Eq a)) -> i64 (if x {} ()) empty)",
            v(&["1"])
        )
    );
    assert_eq!(
        out[8],
        "(impl Q P (q (self) -> i64 empty) (r (self) (if a b false)))"
    );
}

#[test]
fn top_level_do_splices_in_order() {
    let out = prog("(do (defun a () 1) (do) (do (defun b () 2) (defun c () 3)))");
    assert_eq!(out, ["(defun a () 1)", "(defun b () 2)", "(defun c () 3)"]);
}

#[test]
fn defmacro_stays_with_its_body_expanded() {
    let out = prog("(defmacro m (a ... r) `(f ,a ,@r))");
    let body = format!(
        "(fib.prelude/List (fib.prelude/concat {} {} r))",
        v(&["(quote f)"]),
        v(&["a"])
    );
    assert_eq!(out, [format!("(defmacro m (a ... r) {body})")]);
}
