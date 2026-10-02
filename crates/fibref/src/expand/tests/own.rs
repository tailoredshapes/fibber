//! A module's own definitions beat the prelude macros, and
//! `fib.prelude/NAME` is always the prelude's (tranche 1 R14, the owner's
//! rule of 2026-10-01; syntax §4.4 "Declining" and "Hygiene").

use std::collections::HashMap;

use super::read;
use crate::expand::{expand_program, ExpandCtx, ExpandError, ExpandErrorKind as K, NoRunner};
use crate::syntax::Form;

/// Expands the program `src` as the module `main` (a user module: not the
/// prelude's own namespace, which a context starts in).
fn program(src: &str) -> Result<Vec<Form>, ExpandError> {
    let mut ctx = ExpandCtx::new();
    ctx.begin_module("main", (&[], &[]), HashMap::new());
    expand_program(read(src), &mut ctx, &mut NoRunner)
}

/// The last form of `defs` followed by `(defun m () CALL)`, expanded and
/// printed: the body of `m`.
fn body(defs: &str, call: &str) -> String {
    let src = format!("{defs} (defun m () {call})");
    let forms = program(&src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let last = forms
        .last()
        .and_then(|f| f.as_list())
        .and_then(|l| l.last());
    last.map(|f| f.to_string()).unwrap_or_default()
}

#[test]
fn a_defun_of_the_same_name_makes_the_call_an_ordinary_call() {
    // each macro that did not decline before: the module's function wins
    for (def, call) in [
        ("(defun str (a b) a)", "(str x y)"),
        ("(defun update (m k f x) m)", "(update m k f x)"),
        ("(defun reduce (f c) c)", "(reduce + c)"),
        ("(defun print (a b) a)", "(print x y)"),
        ("(defun println (a b) a)", "(println x y)"),
        ("(defun prn (a b) a)", "(prn x y)"),
        ("(defun pr (a b) a)", "(pr x y)"),
        ("(defun when (a b) a)", "(when x y)"),
        ("(defun unless (a b) a)", "(unless x y)"),
        ("(defun cond (a b) a)", "(cond x y)"),
        ("(defun and (a b) a)", "(and x y)"),
        ("(defun or (a b) a)", "(or x y)"),
        ("(defun list (a b) a)", "(list x y)"),
        ("(defun assert (a b) a)", "(assert x y)"),
        ("(defun doto (a b) a)", "(doto x y)"),
        ("(defun dbg (a) a)", "(dbg x)"),
        ("(defun max (a b c) a)", "(max x y z)"),
        ("(defun + (a b c) a)", "(+ x y z)"),
        ("(defun < (a b c) a)", "(< x y z)"),
        ("(defun conj (a b c) a)", "(conj x y z)"),
        ("(defun assoc (a b c d e) a)", "(assoc x y z u v)"),
        ("(defun merge (a b c) a)", "(merge x y z)"),
        ("(defun swap! (a b c) a)", "(swap! x y z)"),
        ("(defun range (a b c) a)", "(range x y z)"),
    ] {
        assert_eq!(body(def, call), call, "{def} then {call}");
    }
}

#[test]
fn without_the_definition_the_macro_still_expands() {
    assert_eq!(body("(defun other (a) a)", "(when x y)"), "(if x y ())");
    assert_eq!(
        body("", "(update m k f x)"),
        "(fib.coll/update m k (fn (#v.1) (f #v.1 x)))"
    );
}

#[test]
fn every_kind_of_definition_hides_the_macro() {
    let call = "(when x y)";
    for def in [
        "(defun when (a b) a)",
        "(defun when :private (a b) a)",
        "(defn when [a b] a)",
        "(defn- when [a b] a)",
        "(def when: i64 1)",
        "(extern when (i64) -> i64)",
        "(defstruct when (a: i64))",
        "(defenum E (when a: i64))",
        "(defenum E when)",
        "(defprotocol P (when (self) -> i64))",
        "(do (do (defun when (a b) a)))",
    ] {
        assert_eq!(body(def, call), call, "{def}");
    }
}

#[test]
fn a_definition_below_the_use_hides_the_macro_there_too() {
    let forms = program("(defun a () (when x y)) (defun when (p q) p)").expect("expands");
    assert_eq!(forms[0].to_string(), "(defun a () (when x y))");
}

#[test]
fn only_a_definition_of_the_module_counts_not_a_binding() {
    assert_eq!(
        body("", "(let ((when 1)) (when x y))"),
        "(let ((when 1)) (if x y ()))"
    );
    assert_eq!(body("(defun f (when) when)", "(when x y)"), "(if x y ())");
}

#[test]
fn the_names_a_program_module_exports_hide_the_macro_in_the_module_that_uses_it() {
    let mut ctx = ExpandCtx::new();
    ctx.begin_module("util", (&[], &[]), HashMap::new());
    let util = read("(ns util) (defun str (a b) a) (defun hidden :private (a) a)");
    expand_program(util, &mut ctx, &mut NoRunner).expect("util expands");
    ctx.end_module();
    let uses = ["util".to_string()];
    ctx.begin_module("main", (&uses, &[]), HashMap::new());
    let main = read("(defun m () (str x y)) (defun n () (when x y))");
    let out = expand_program(main, &mut ctx, &mut NoRunner).expect("main expands");
    assert_eq!(out[0].to_string(), "(defun m () (str x y))");
    assert_eq!(out[1].to_string(), "(defun n () (if x y ()))");
    ctx.end_module();
    // a module that does not use util has the macro, and so has the next one
    ctx.begin_module("other", (&[], &[]), HashMap::new());
    let other = read("(defun m () (str x))");
    let out = expand_program(other, &mut ctx, &mut NoRunner).expect("other expands");
    assert_eq!(out[0].to_string(), "(defun m () (fib.core/to-str x))");
}

#[test]
fn the_implicit_library_does_not_hide_the_macros() {
    let mut ctx = ExpandCtx::new();
    let implicit = ["fib.coll".to_string()];
    ctx.begin_module("main", (&[], &implicit), HashMap::new());
    let main = read("(defun m () (update a b f x))");
    let out = expand_program(main, &mut ctx, &mut NoRunner).expect("expands");
    assert_eq!(
        out[0].to_string(),
        "(defun m () (fib.coll/update a b (fn (#v.1) (f #v.1 x))))"
    );
}

#[test]
fn a_macro_of_an_implicit_module_beats_a_macro_of_the_prelude_and_a_used_one_beats_both() {
    // syntax §5: a name is the module's own, then what its `:use`s export, then the
    // implicit library, then the prelude. A context starts in the prelude's namespace.
    let mut ctx = ExpandCtx::new();
    let define = |ctx: &mut ExpandCtx, ns: &str, body: &str| {
        ctx.begin_module(ns, (&[], &[]), HashMap::new());
        let src = format!("(defmacro dup (e) {body})");
        expand_program(read(&src), ctx, &mut NoRunner).expect("defines");
        ctx.end_module();
    };
    ctx.begin_module("fib.prelude", (&[], &[]), HashMap::new());
    expand_program(read("(defmacro dup (e) e)"), &mut ctx, &mut NoRunner).expect("defines");
    ctx.end_module();
    define(&mut ctx, "fib.x", "e");
    define(&mut ctx, "mine", "e");
    let owner = |ctx: &ExpandCtx| ctx.macro_def("dup").map(|d| d.ns.clone());
    ctx.begin_module("a", (&[], &[]), HashMap::new());
    assert_eq!(owner(&ctx).as_deref(), Some("fib.prelude"));
    let (uses, implicit) = (["mine".to_string()], ["fib.x".to_string()]);
    ctx.begin_module("b", (&[], &implicit), HashMap::new());
    assert_eq!(owner(&ctx).as_deref(), Some("fib.x"));
    ctx.begin_module("c", (&uses, &implicit), HashMap::new());
    assert_eq!(owner(&ctx).as_deref(), Some("mine"));
}

#[test]
fn a_qualified_prelude_head_is_the_prelude_macro_whatever_the_module_defines() {
    let defs = "(defun when (a b) a) (defun and (a b) a) (defun str (a) a)";
    assert_eq!(body(defs, "(fib.prelude/when x y)"), "(if x y ())");
    assert_eq!(
        body(defs, "(fib.prelude/and x y z)"),
        "(if x (if y z false) false)"
    );
    assert_eq!(body(defs, "(fib.prelude/or x y)"), "(if x true y)");
    assert_eq!(body(defs, "(fib.prelude/str x)"), "(fib.core/to-str x)");
    assert_eq!(body(defs, "(str x)"), "(str x)");
}

#[test]
fn a_qualified_prelude_head_reaches_the_fold_of_a_name_the_module_defines() {
    let defs = "(defun + (a b) a) (defun max (a b) a) (defun conj (a b) a)";
    assert_eq!(body(defs, "(+ x y z)"), "(+ x y z)");
    assert_eq!(
        body(defs, "(fib.prelude/+ x y z)"),
        "(fib.prelude/+ (fib.prelude/+ x y) z)"
    );
    assert_eq!(
        body(defs, "(fib.prelude/max x y z)"),
        "(fib.core/max (fib.core/max x y) z)"
    );
    assert_eq!(
        body(defs, "(fib.prelude/conj x y z)"),
        "(fib.coll/conj (fib.coll/conj x y) z)"
    );
}

#[test]
fn what_a_qualified_head_does_not_expand_stays_the_call() {
    // the macro declines the binary builtin and the library functions, and
    // `fib.prelude/println` is the function the printing macros call (the
    // macro of that name would expand its own expansion for ever)
    for call in [
        "(fib.prelude/println x)",
        "(fib.prelude/+ x y)",
        "(fib.prelude/< x y)",
        "(fib.prelude/swap! a f)",
        "(fib.prelude/conj x y)",
        "(fib.prelude/range x)",
        "(fib.prelude/push! x f)",
        "(fib.prelude/trap x)",
        "(fib.prelude/nonesuch x)",
    ] {
        assert_eq!(body("", call), call, "{call}");
    }
}

#[test]
fn a_qualified_call_that_is_declined_is_not_a_step() {
    let mut ctx = ExpandCtx::new();
    ctx.begin_module("main", (&[], &[]), HashMap::new());
    let src = read("(defun m () (fib.prelude/+ a b))");
    expand_program(src, &mut ctx, &mut NoRunner).expect("expands");
    assert_eq!(ctx.counters().0, 0);
    let src = read("(defun n () (fib.prelude/+ a b c))");
    expand_program(src, &mut ctx, &mut NoRunner).expect("expands");
    assert_eq!(ctx.counters().0, 1);
}

#[test]
fn a_user_macro_named_like_a_prelude_one_does_not_capture_the_preludes_own_heads() {
    let defs = "(defmacro and (a b) a) (defmacro or (a b) b)";
    // the folds, `and` and `or` of the prelude and `derive` write fib.prelude/and
    assert_eq!(
        body(defs, "(< a b c)"),
        "(if (fib.prelude/< a b) (fib.prelude/< b c) false)"
    );
    assert_eq!(
        body(defs, "(fib.prelude/and x y z)"),
        "(if x (if y z false) false)"
    );
    // the user's own `and` is still the user's macro in the user's module
    let src = format!("{defs} (defun m () (and x y))");
    let err = program(&src).expect_err("the user's macro needs the evaluator");
    assert!(matches!(err.kind, K::MacroNeedsEvaluator { .. }), "{err:?}");
}
