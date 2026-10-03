//! Adversarial tests for the expander: inputs written to break it.
//!
//! User macros need an evaluator, which does not exist yet, so these
//! tests bring a tiny one ([`Mini`]) that implements `MacroRunner` for
//! the subset of fibber that expanded macro bodies here use: parameters,
//! `quote`, `let`, `if`, `do`, the `Form` constructors `List`/`Vec`/`Sym`,
//! `concat`, the §1.4 prelude calls, `gensym`, `=` on forms and the
//! reflection calls. It exercises the trait as the real evaluator will.

use fibref::expand::{
    expand_expr, expand_program, ExpandCtx, ExpandError, ExpandErrorKind as K, Limits, NoRunner,
    MAX_EXPAND_DEPTH, MAX_STEPS,
};
use fibref::syntax::{read_all, Form, FormKind};

#[path = "expand_adversarial/mini.rs"]
mod mini;

use mini::Mini;

fn read(src: &str) -> Vec<Form> {
    read_all(src, "adv.fib").unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// Expands `src` with the mini evaluator and prints each form.
fn expand(src: &str) -> Result<Vec<String>, ExpandError> {
    let out = expand_program(read(src), &mut ExpandCtx::new(), &mut Mini)?;
    Ok(out.iter().map(|f| f.to_string()).collect())
}

fn ok(src: &str) -> Vec<String> {
    expand(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn err(src: &str) -> ExpandError {
    match expand(src) {
        Ok(out) => panic!("{src} expanded to {out:?}"),
        Err(e) => e,
    }
}

/// The last expanded form of `src`.
fn last(src: &str) -> String {
    ok(src).pop().unwrap_or_default()
}

// ---- hygiene: what gensym must survive -------------------------------

const SWAP_ADD: &str =
    "(defmacro add-first (a b) (let ((t (gensym \"t\"))) `(let ((~t ~a)) (+ ~t ~b))))\n";

#[test]
fn gensym_binding_does_not_capture_a_user_variable_of_the_same_name() {
    let out = last(&format!("{SWAP_ADD}(defun f (t) (add-first 10 t))"));
    assert_eq!(out, "(defun f (t) (let ((#t.1 10)) (+ #t.1 t)))");
}

#[test]
fn the_user_cannot_write_a_gensym() {
    for text in ["#t.1", "(f #t.1)", "#t"] {
        assert!(read_all(text, "adv.fib").is_err(), "{text} must not read");
    }
    // Every spelling the reader does accept is a different symbol.
    let out = last(&format!(
        "{SWAP_ADD}(defun f (t.1 t1 t#1) (add-first t.1 t#1))"
    ));
    assert_eq!(
        out,
        "(defun f (t.1 t1 t#1) (let ((#t.1 t.1)) (+ #t.1 t#1)))"
    );
}

#[test]
fn nested_uses_get_distinct_gensyms() {
    let out = last(&format!(
        "{SWAP_ADD}(defun f (x) (add-first (add-first x 1) x))"
    ));
    assert_eq!(
        out,
        "(defun f (x) (let ((#t.1 (let ((#t.2 x)) (+ #t.2 1)))) (+ #t.1 x)))"
    );
}

#[test]
fn prelude_gensyms_do_not_capture_user_names() {
    let out =
        last("(defun f (m doto) (dotimes (i m) (g m doto)) (doto m (h doto)) (plet ((a m)) a))");
    assert_eq!(
        out,
        "(defun f (m doto) \
         (let ((#m.1 m)) (loop ((i 0)) (if (fib.prelude/< i #m.1) (do (g m doto) (recur (fib.prelude/+ i 1))) ()))) \
         (let ((#doto.2 m)) (h #doto.2 doto) #doto.2) \
         (let ((#a.3 (fib.prelude/spawn (fn () m)))) (let ((a (fib.prelude/join #a.3))) a)))"
    );
}

#[test]
fn an_unhygienic_macro_does_capture_as_the_spec_decides() {
    // §3.16: "There is no automatic hygiene"; a macro that binds a plain
    // name captures the caller's use of it.
    let src = "(defmacro bad (e) `(let ((t 1)) (+ t ~e)))\n(defun f (t) (bad t))";
    assert_eq!(last(src), "(defun f (t) (let ((t 1)) (+ t t)))");
}

// ---- nested quasiquote with splicing ---------------------------------

#[test]
fn a_macro_defining_macro_with_nested_quasiquote_and_splicing() {
    let src = "(defmacro defalias (new old) `(defmacro ~new (... args) `(~'~old ~@args)))\n\
               (defalias plus g)\n\
               (defun f () (plus 1 2 3))";
    let out = ok(src);
    assert_eq!(out.len(), 3);
    assert!(
        out[1].starts_with("(defmacro plus (... args) "),
        "{}",
        out[1]
    );
    // `g` and not `+`: since R6a a call of `+` with three operands is the
    // prelude macro's and is expanded again (`(fib.prelude/+ (fib.prelude/+
    // 1 2) 3)`), which is not what this test is about.
    assert_eq!(out[2], "(defun f () (g 1 2 3))");
}

#[test]
fn splicing_at_several_levels() {
    let src =
        "(defmacro wrap (... xs) `(f [~@xs] (g ~@xs ~@xs) '(h ~@xs)))\n(defun k () (wrap a b))";
    let out = last(src);
    let v = "(fib.prelude/vec-conj (fib.prelude/vec-conj (fib.prelude/vec-empty) a) b)";
    assert_eq!(
        out,
        format!("(defun k () (f {v} (g a b a b) (quote (h a b))))")
    );
}

#[test]
fn splice_outside_a_list_is_an_error_at_definition() {
    let e = err("(defmacro m (xs) `~@xs)");
    assert_eq!(e.kind, K::SpliceOutsideList);
    assert_eq!((e.pos.line, e.pos.col), (1, 19));
}

#[test]
fn a_macro_returning_an_unquote_is_an_error() {
    let e = err("(defmacro m () '(unquote x))\n(defun f () (m))");
    assert_eq!(e.kind, K::UnquoteOutsideQuasiquote { head: "unquote" });
    assert_eq!(e.pos.line, 2);
}

// ---- macros producing top-level forms --------------------------------

#[test]
fn a_macro_producing_a_top_level_do_of_several_defs() {
    // §3.16's defrecord (proposed case 41): the spliced defstruct is
    // registered before the spliced derive is expanded.
    let src =
        "(defmacro defrecord (name fields) `(do (defstruct ~name ~fields) (derive Eq ~name)))\n\
               (defrecord Pt (x y))\n\
               (defun main () (= (Pt 1 2) (Pt 1 2)))";
    let out = ok(src);
    assert_eq!(out.len(), 4);
    assert_eq!(out[1], "(defstruct Pt (x y))");
    assert!(
        out[2].starts_with("(impl Eq (Pt x y) :where ((Eq x) (Eq y)) "),
        "{}",
        out[2]
    );
}

#[test]
fn a_macro_producing_nested_dos_of_defuns() {
    let src = "(defmacro two (a b) `(do (defun ~a () 1) (do (do) (defun ~b () 2))))\n(two f g)";
    assert_eq!(ok(src)[1..], ["(defun f () 1)", "(defun g () 2)"]);
}

#[test]
fn a_macro_producing_definitions_in_an_expression_is_an_error() {
    let src = "(defmacro two (a) `(do (defun ~a () 1)))\n(defun h () (two f))";
    assert_eq!(
        err(src).kind,
        K::DefinitionInExpression {
            head: "defun".into()
        }
    );
}

#[test]
fn reflection_from_a_macro() {
    let src = "(defstruct (P a) (x: a y: i64))\n\
               (defmacro fields (n) `(list ~@(struct-fields n)))\n\
               (defmacro no (n) (struct-fields n))\n\
               (defun f (p) (fields P))";
    assert_eq!(
        last(src),
        "(defun f (p) (fib.prelude/Cons x (fib.prelude/Cons y fib.prelude/Empty)))"
    );
    let e = err("(defenum E (A x: i64))\n(defmacro no (n) (struct-fields n))\n(defun f () (no E))");
    assert_eq!(
        e.kind,
        K::NotAStruct {
            op: "struct-fields",
            name: "E".into()
        }
    );
    assert_eq!(e.pos.line, 3);
}

// ---- prelude edge cases ------------------------------------------------

fn expr(src: &str) -> Result<String, ExpandError> {
    let form = read(src).remove(0);
    expand_expr(form, &mut ExpandCtx::new(), &mut NoRunner).map(|f| f.to_string())
}

#[test]
fn cond_with_no_clauses_is_unit() {
    // stdlib §7 L20: without a default a cond falls to nil, or `()` for unit bodies; no trap.
    assert_eq!(expr("(cond)").ok(), Some("()".into()));
    assert_eq!(expr("(cond a 1)").ok(), Some("(if a 1)".into()));
}

#[test]
fn threading_with_non_list_steps() {
    assert_eq!(expr("(-> x f g)").ok(), Some("(g (f x))".into()));
    assert_eq!(expr("(->> x f (g a))").ok(), Some("(g a (f x))".into()));
    for src in [
        "(-> x 1)",
        "(-> x \"f\")",
        "(->> x ())",
        "(-> x [f])",
        "(doto x 1)",
    ] {
        assert_eq!(expr(src).map_err(|e| e.kind), Err(K::ThreadStep), "{src}");
    }
    // A step that is itself a macro call is expanded after threading.
    assert_eq!(expr("(-> a (when b))").ok(), Some("(if a b)".into()));
}

#[test]
fn derive_on_a_generic_enum_with_a_recursive_field() {
    let src = "(defenum (Tree a) (leaf) (node v: a l: (Tree a) r: (Tree a)))\n\
               (derive Eq Tree) (derive Ord Tree) (derive Hash Tree) (derive Show Tree)";
    let out = ok(src);
    assert_eq!(out.len(), 5);
    for (form, p) in out[1..].iter().zip(["Eq", "Ord", "Hash", "Show"]) {
        // `(Ord a)` alone: it entails `(Eq a)` (types §4.1).
        let ctx = format!("(({p} a))");
        let head = format!("(impl {p} (Tree a) :where {ctx} ");
        assert!(form.starts_with(&head), "{form}");
    }
    assert!(out[1].contains("((node #v.1 #l.2 #r.3) (match y ((node #v2.4 #l2.5 #r2.6) (and (fib.prelude/= #v.1 #v2.4) (fib.prelude/= #l.2 #l2.5) (fib.prelude/= #r.3 #r2.6))) (_ false)))"), "{}", out[1]);
}

#[test]
fn a_macro_that_expands_to_itself_hits_the_step_limit() {
    let e = err("(defmacro forever () '(forever))\n(defun f () (forever))");
    assert_eq!(e.kind, K::TooManySteps { limit: MAX_STEPS });
    assert_eq!((e.pos.line, e.pos.col), (2, 13));
    let e = err("(defmacro forever () '(forever))\n(forever)");
    assert_eq!(e.kind, K::TooManySteps { limit: MAX_STEPS });
}

#[test]
fn a_macro_that_grows_hits_the_depth_limit() {
    let e = err("(defmacro grow (x) `(f (grow ~x)))\n(defun f () (grow 1))");
    assert_eq!(
        e.kind,
        K::TooDeep {
            limit: MAX_EXPAND_DEPTH
        }
    );
    let e = err("(defmacro grow () '(do (grow) (grow)))\n(grow)");
    assert_eq!(e.kind, K::TooManySteps { limit: MAX_STEPS });
}

#[test]
fn limits_are_per_context() {
    let mut ctx = ExpandCtx::new();
    ctx.limits = Limits {
        max_steps: 5,
        max_depth: 50,
        ..Limits::default()
    };
    let forms = read("(defmacro forever () '(forever))\n(defun f () (forever))");
    let e = expand_program(forms, &mut ctx, &mut Mini)
        .map(|_| ())
        .map_err(|e| e.kind);
    assert_eq!(e, Err(K::TooManySteps { limit: 5 }));
}

#[test]
fn loop_bodies_with_recur_looking_user_symbols() {
    let out = expr("(dotimes (i n) (recur-count i) (let ((recur 1) (m 2)) (+ recur m)))");
    assert_eq!(
        out.ok(),
        Some(
            "(let ((#m.1 n)) (loop ((i 0)) (if (fib.prelude/< i #m.1) (do (recur-count i) \
             (let ((recur 1) (m 2)) (+ recur m)) (recur (fib.prelude/+ i 1))) ())))"
                .into()
        )
    );
    // A user `(recur ..)` in a while body is kept where it is, before
    // the loop's own `(recur)`, for the checker to reject as not in tail
    // position (§3.18).
    let out = expr("(while (go?) (recur! x) (recur x))");
    assert_eq!(
        out.ok(),
        Some("(loop () (if (go?) (do (recur! x) (recur x) (recur)) ()))".into())
    );
    let out = expr("(for-each (range 0 k) (fn (recur) (f recur)))");
    assert_eq!(
        out.ok(),
        Some(
            "(let ((#s.1 0) (#m.2 k)) (loop ((recur #s.1)) \
             (if (fib.prelude/< recur #m.2) (do (f recur) (recur (fib.prelude/+ recur 1))) ())))"
                .into()
        )
    );
}

#[test]
fn positions_through_a_user_macro() {
    let src = "(defmacro call-g (x) `(g ~x))\n(defun f ()\n  (call-g\n    arg))";
    let out = expand_program(read(src), &mut ExpandCtx::new(), &mut Mini)
        .unwrap_or_else(|e| panic!("{e}"));
    let body = &out[1].as_list().unwrap_or(&[])[3];
    assert_eq!((body.pos.line, body.pos.col), (3, 3), "built at the call");
    let arg = &body.as_list().unwrap_or(&[])[1];
    assert_eq!(
        (arg.pos.line, arg.pos.col),
        (4, 5),
        "argument keeps its own"
    );
}

#[test]
fn user_macro_shadows_a_prelude_macro() {
    let src = "(defmacro when (c x) `(if ~c ~x 0))\n(defun f (c) (when c 1))";
    assert_eq!(last(src), "(defun f (c) (if c 1 0))");
}

#[test]
fn no_runner_reports_pending_with_the_call_position() {
    let forms = read("(defmacro m () 1)\n(defun f ()\n  (m))");
    let e = expand_program(forms, &mut ExpandCtx::new(), &mut NoRunner).err();
    let e = e.unwrap_or_else(|| panic!("expected pending"));
    assert_eq!(e.kind, K::MacroNeedsEvaluator { name: "m".into() });
    assert_eq!((e.pos.line, e.pos.col), (3, 3));
}

// ---- the proposed cases that need an evaluator -------------------------

#[test]
fn proposed_case_41_expands_with_an_evaluator() {
    let src = "(defmacro defrecord (name fields)\n  `(do (defstruct ~name ~fields)\n       (derive Eq ~name)))\n\
               (defrecord Point (x: i64 y: i64))\n\
               (defun main () -> i64 (if (= (Point 1 2) (Point 1 2)) 1 0))";
    let out = ok(src);
    assert_eq!(out[1], "(defstruct Point (x: i64 y: i64))");
    assert_eq!(
        out[2],
        "(impl Eq Point (= (self y) (and (fib.prelude/= (. self x) (. y x)) (fib.prelude/= (. self y) (. y y)))) \
         (!= (self y) (fib.prelude/not (fib.prelude/= self y))))"
    );
    assert_eq!(
        out[3],
        "(defun main () -> i64 (if (= (Point 1 2) (Point 1 2)) 1 0))"
    );
}

#[test]
fn proposed_case_51_and_its_companions() {
    let src = "(defmacro or-zero (e)\n  `(match ~e ((some x) x) (nil 0)))\n\
               (defun main () -> i64 (+ (or-zero (pick true)) (or-zero (pick false))))";
    assert_eq!(
        last(src),
        "(defun main () -> i64 (+ (match (pick true) ((some x) x) (nil 0)) \
         (match (pick false) ((some x) x) (nil 0))))"
    );
    // (defmacro none () 'nil) as an expression is the constant.
    let forms = expand_program(
        read("(defmacro none () 'nil)\n(def z: (Option i64) (none))"),
        &mut ExpandCtx::new(),
        &mut Mini,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let value = &forms[1].as_list().unwrap_or(&[])[3];
    assert_eq!(value.kind, FormKind::Nil);
    // A macro-built (Sym "nil") is the constant in an expression and the
    // empty-variant pattern in a pattern; (nil) built from enum-variants
    // is its (Variant) spelling.
    let src = "(defmacro m (e) (List [(quote match) e (List [(Sym \"nil\") (Sym \"nil\")]) \
               (List [(List [(Sym \"nil\")]) 0])]))\n(defun f (o) (m o))";
    let forms = expand_program(read(src), &mut ExpandCtx::new(), &mut Mini)
        .unwrap_or_else(|e| panic!("{e}"));
    let m = forms[1].as_list().unwrap_or(&[])[3]
        .as_list()
        .unwrap_or(&[])
        .to_vec();
    let clause = m[2].as_list().unwrap_or(&[]);
    assert_eq!(
        (&clause[0].kind, &clause[1].kind),
        (&FormKind::Nil, &FormKind::Nil)
    );
    assert_eq!(
        forms[1].to_string(),
        "(defun f (o) (match o (nil nil) ((nil) 0)))"
    );
}

#[test]
fn loop_bounds_are_evaluated_before_the_loop_variable_exists() {
    // §4.4 (owner decision): the bound is bound to a gensym by a `let`
    // outside the loop, so a bound that mentions a variable named like
    // the loop variable sees the outer one. The earlier table put the
    // bound inside the loop's own sequential bindings, where `(dotimes
    // (i i) ..)` inside `(let ((i 5)) ..)` ran zero times.
    let out = expr("(let ((i 5)) (dotimes (i i) (f i)))");
    assert_eq!(
        out.ok(),
        Some(
            "(let ((i 5)) (let ((#m.1 i)) (loop ((i 0)) \
             (if (fib.prelude/< i #m.1) (do (f i) (recur (fib.prelude/+ i 1))) ()))))"
                .into()
        )
    );
    let out = expr("(for-each (range 0 i) (fn (i) (f i)))");
    assert_eq!(
        out.ok(),
        Some(
            "(let ((#s.1 0) (#m.2 i)) (loop ((i #s.1)) \
             (if (fib.prelude/< i #m.2) (do (f i) (recur (fib.prelude/+ i 1))) ())))"
                .into()
        )
    );
}
