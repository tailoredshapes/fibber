//! One test per expansion error, each checking the kind and position.

use super::{ex, ex_err, prog_err, program};
use crate::expand::error::ExpandErrorKind as K;

/// The line and column of the error of the program `src`.
fn at(src: &str) -> (usize, usize) {
    match program(src) {
        Ok(_) => panic!("{src:?} expanded"),
        Err(e) => (e.pos.line, e.pos.col),
    }
}

#[test]
fn user_macro_without_evaluator_is_pending() {
    let src = "(defmacro m (x) x)\n(defun f () (m 1))";
    assert_eq!(prog_err(src), K::MacroNeedsEvaluator { name: "m".into() });
    assert_eq!(at(src), (2, 13));
}

#[test]
fn user_macro_arity_is_checked_before_running() {
    let src = "(defmacro m (x) x) (defun f () (m))";
    let expected = "1".to_string();
    assert_eq!(
        prog_err(src),
        K::MacroArity {
            name: "m".into(),
            expected,
            found: 0
        }
    );
    let src = "(defmacro m (x ... r) x) (defun f () (m))";
    let expected = "at least 1".to_string();
    assert_eq!(
        prog_err(src),
        K::MacroArity {
            name: "m".into(),
            expected,
            found: 0
        }
    );
}

#[test]
fn prelude_macro_arity() {
    let e = ex_err("(derive Eq)");
    assert!(matches!(e.kind, K::MacroArity { ref name, found: 1, .. } if name == "derive"));
    let e = ex_err("(while)");
    assert!(matches!(e.kind, K::MacroArity { ref name, found: 0, .. } if name == "while"));
}

#[test]
fn malformed_core_forms() {
    for src in [
        "(defun f () (if a))",
        "(defun f () (let (x 1) x))",
        "(defun f () (let ((x)) x))",
        "(defun f () (let ((x 1))))",
        "(defun f () (fn x))",
        "(defun f () (match x))",
        "(defun f () (match x y))",
        "(defun f () (quote))",
        "(defun f () (. a))",
        "(defun f () (await))",
        "(defun f ())",
        "(defun (f) () 1)",
        "(def x)",
        // `(impl Eq T)` is well formed since methods may have defaults
        // (types §4.1); the checker says which one is missing.
        "(impl Eq)",
        "(impl Eq T (m))",
        "(defstruct S ())",
        "(defstruct S (x: ))",
        "(defstruct S (1))",
        "(defenum E)",
        "(defenum E 1)",
        "(defmacro m x 1)",
        "(defmacro m (... ) 1)",
        "(defmacro m (... a b) 1)",
        "(defmacro 1 () 1)",
    ] {
        assert!(matches!(prog_err(src), K::Malformed { .. }), "{src}");
    }
    assert_eq!(at("(defun f ()\n  (if a))"), (2, 3));
}

#[test]
fn defmacro_cannot_take_a_core_name() {
    for name in ["if", "let", "defun", "quasiquote", "unquote", "&", "."] {
        let src = format!("(defmacro {name} (a) a)");
        let kind = prog_err(&src);
        assert!(
            matches!(kind, K::MacroNamesCoreForm { .. }),
            "{src}: {kind:?}"
        );
    }
}

#[test]
fn in_out_outside_an_argument() {
    for src in [
        "(let ((y &x)) y)",
        "(&f 1)",
        "(if &c a b)",
        "[&x]",
        "(& 1)",
        "(& x y)",
    ] {
        assert_eq!(ex_err(src).kind, K::InOutOutsideArgument, "{src}");
    }
    let e = ex_err("(let ((y &x)) y)");
    assert_eq!((e.pos.line, e.pos.col), (1, 10));
}

#[test]
fn nil_called() {
    assert_eq!(ex_err("(nil)").kind, K::NilCalled);
    assert_eq!(ex_err("(f (nil 1))").kind, K::NilCalled);
}

#[test]
fn braces_in_patterns() {
    assert_eq!(ex_err("(match v ({a b} 1))").kind, K::BraceInPattern);
    assert_eq!(ex_err("(let (({a 1} m)) a)").kind, K::BraceInPattern);
    let e = ex_err("(match v ((some [{a 1}]) 1))");
    assert_eq!((e.kind, e.pos.col), (K::BraceInPattern, 18));
}

#[test]
fn brackets_in_patterns_are_vector_patterns_walked_as_patterns() {
    // Not the §1.4 rewrite to conj calls: the items stay, `nil` in one
    // is normalised to (Nil), and `&` is left for lowering (§3.6).
    assert_eq!(
        ex("(match v ([a nil & r] 1) (_ 2))"),
        "(match v ([a nil & r] 1) (_ 2))"
    );
    assert_eq!(ex("(let (([& r] v)) r)"), "(let (([& r] v)) r)");
}

#[test]
fn expression_at_top_level() {
    assert_eq!(prog_err("(+ 1 2)"), K::ExpressionAtTopLevel);
    assert_eq!(prog_err("x"), K::ExpressionAtTopLevel);
    assert_eq!(prog_err("()"), K::ExpressionAtTopLevel);
    assert_eq!(prog_err("(when c (defun f () 1))"), K::ExpressionAtTopLevel);
    assert_eq!(at("(defun f () 1)\n(do (defun g () 2) 3)"), (2, 20));
}

#[test]
fn definition_in_expression() {
    let kind = prog_err("(defun f () (defun g () 1))");
    assert_eq!(
        kind,
        K::DefinitionInExpression {
            head: "defun".into()
        }
    );
    let kind = prog_err("(defstruct P (a)) (defun f () (derive Eq P))");
    assert_eq!(
        kind,
        K::DefinitionInExpression {
            head: "impl".into()
        }
    );
}

#[test]
fn ns_must_be_first() {
    assert_eq!(prog_err("(defun f () 1) (ns late)"), K::NsNotFirst);
}

#[test]
fn odd_map_built_by_hand_is_malformed() {
    use crate::syntax::{Form, FormKind};
    let pos = super::one("x").pos;
    let odd = Form::new(FormKind::Map(vec![super::one("k")]), pos);
    let r = crate::expand::expand_expr(
        odd,
        &mut crate::expand::ExpandCtx::new(),
        &mut crate::expand::NoRunner,
    );
    assert!(matches!(r, Err(ref e) if matches!(e.kind, K::Malformed { .. })));
}
