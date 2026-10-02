//! `reduce`, the macro that declines by default (stdlib design §2.1 rule
//! 3, tranche 1 R6b, PC-6): the two-argument form, the identity heads and
//! the rewrite of `reduced` in a literal `fn`, each as printed, and every
//! call it leaves to the library function.

use super::{ex, program};

/// Expands `call` as the last form of a module whose first form is `defs`
/// (a program: the module's own definitions are what the macros yield to).
fn own(defs: &str, call: &str) -> String {
    let src = format!("{defs} (defun m () {call})");
    let forms = program(&src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    let body = forms
        .last()
        .and_then(|f| f.as_list())
        .and_then(|l| l.last());
    body.map(|f| f.to_string()).unwrap_or_default()
}

#[test]
fn two_arguments_is_reduce_nonempty() {
    assert_eq!(ex("(reduce f c)"), "(fib.seq/reduce-nonempty f c)");
    assert_eq!(
        ex("(reduce (fn (a x) (max a x)) [1 2])"),
        "(fib.seq/reduce-nonempty (fn (a x) (max a x)) \
         (fib.prelude/vec-conj (fib.prelude/vec-conj (fib.prelude/vec-empty) 1) 2))"
    );
}

#[test]
fn the_two_argument_form_does_not_read_reduced() {
    // there is no reduce-while without an initial value (the plan's rule a)
    assert_eq!(
        ex("(reduce (fn (a x) (reduced a)) c)"),
        "(fib.seq/reduce-nonempty (fn (a x) (reduced a)) c)"
    );
}

#[test]
fn the_literal_heads_start_from_their_identity() {
    assert_eq!(ex("(reduce + c)"), "(fib.seq/reduce + 0 c)");
    assert_eq!(ex("(reduce * c)"), "(fib.seq/reduce * 1 c)");
    assert_eq!(
        ex("(reduce str c)"),
        "(fib.seq/reduce (fn (#r.1 #r.2) (fib.prelude/str-concat (fib.core/to-str #r.1) \
         (fib.core/to-str #r.2))) \"\" c)"
    );
    assert_eq!(
        ex("(reduce conj c)"),
        "(fib.seq/reduce conj (fib.prelude/vec-empty) c)"
    );
    assert_eq!(
        ex("(reduce concat c)"),
        "(fib.seq/reduce concat (fib.seq/lazy-node (fn () fib.seq/LNil)) c)"
    );
    assert_eq!(
        ex("(reduce merge c)"),
        "(fib.seq/reduce merge (fib.prelude/map-empty) c)"
    );
    // any other symbol, a qualified one and a list head are not in the table
    assert_eq!(ex("(reduce - c)"), "(fib.seq/reduce-nonempty - c)");
    // a list head that no macro rewrites (`(+)` is the fold macro's 0)
    assert_eq!(
        ex("(reduce (comp f g) c)"),
        "(fib.seq/reduce-nonempty (comp f g) c)"
    );
}

#[test]
fn a_literal_str_is_folded_with_the_macro_in_both_forms() {
    // the value `str` is the one-argument function: the step is a `fn` over gensyms
    let step = "(fn (#r.1 #r.2) (fib.prelude/str-concat (fib.core/to-str #r.1) \
                (fib.core/to-str #r.2)))";
    assert_eq!(
        ex("(reduce str \"x\" c)"),
        format!("(fib.seq/reduce {step} \"x\" c)")
    );
    // the gensyms are fresh for each use, the same two names never twice
    assert_eq!(
        ex("(do (reduce str c) (reduce str c))"),
        "(do (fib.seq/reduce (fn (#r.1 #r.2) (fib.prelude/str-concat (fib.core/to-str #r.1) \
         (fib.core/to-str #r.2))) \"\" c) (fib.seq/reduce (fn (#r.3 #r.4) \
         (fib.prelude/str-concat (fib.core/to-str #r.3) (fib.core/to-str #r.4))) \"\" c))"
    );
    // a `str` that is the module's own is that function, in both forms
    assert_eq!(
        own("(defun str (a x) a)", "(reduce str c)"),
        "(fib.seq/reduce-nonempty str c)"
    );
    assert_eq!(
        own("(defun str (a x) a)", "(reduce str \"x\" c)"),
        "(reduce str \"x\" c)"
    );
}

#[test]
fn a_head_the_module_defines_has_no_identity() {
    for head in ["+", "*", "conj", "concat", "merge"] {
        let defs = format!("(defun {head} (a x) a)");
        assert_eq!(
            own(&defs, &format!("(reduce {head} c)")),
            format!("(fib.seq/reduce-nonempty {head} c)"),
            "{head}"
        );
    }
}

#[test]
fn reduced_in_a_literal_fn_is_reduce_while() {
    // the example of §2.1 rule 3: 11
    assert_eq!(
        ex("(reduce (fn (acc x) (if (> acc 10) (reduced acc) (+ acc x))) 0 c)"),
        "(fib.seq/reduce-while (fn (acc x) (if (> acc 10) (fib.core/Done acc) \
         (fib.core/More (+ acc x)))) 0 c)"
    );
}

#[test]
fn every_tail_is_found_through_the_forms_a_body_nests() {
    assert_eq!(
        ex("(reduce (fn (a x) (let ((y (f x))) (do (g) (match y (0 (reduced a)) (_ (cond (p 1) (q (reduced 2)) (else 3))))))) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (let ((y (f x))) (do (g) (match y \
         (0 (fib.core/Done a)) (_ (if p (fib.core/More 1) (if q (fib.core/Done 2) \
         (fib.core/More 3)))))))) 0 c)"
    );
    assert_eq!(
        ex("(reduce (fn (a x) (when-let (v (f x)) (if-let (w v) (reduced w) a))) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (match (f x) ((fib.prelude/some v) \
         (match v ((fib.prelude/some w) (fib.core/Done w)) (_ (fib.core/More a)))) \
         (_ ()))) 0 c)"
    );
    assert_eq!(
        ex("(reduce (fn (a x) (g) (unless p (reduced a))) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (g) (if p () (fib.core/Done a))) 0 c)"
    );
}

#[test]
fn a_recur_tail_is_not_a_value() {
    assert_eq!(
        ex("(reduce (fn (a x) (loop ((i 0)) (if (< i x) (recur (+ i 1)) (reduced i)))) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (loop ((i 0)) (if (< i x) (recur (+ i 1)) \
         (fib.core/Done i)))) 0 c)"
    );
}

#[test]
fn a_reduced_that_is_not_a_tail_does_not_make_a_reduce_while() {
    for src in [
        "(reduce (fn (a x) (+ a (reduced x))) 0 c)",
        "(reduce (fn (a x) (g (reduced a)) a) 0 c)",
        "(reduce (fn (a x) (let ((r (reduced a))) a)) 0 c)",
        "(reduce (fn (a x) (fn () (reduced a))) 0 c)",
    ] {
        assert_eq!(ex(src), src, "{src}");
    }
}

#[test]
fn a_reduced_with_the_wrong_operands_is_left_for_the_checker() {
    assert_eq!(
        ex("(reduce (fn (a x) (if p (reduced) a)) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (if p (reduced) (fib.core/More a))) 0 c)"
    );
    assert_eq!(
        ex("(reduce (fn (a x) (if p (reduced a x) a)) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (if p (reduced a x) (fib.core/More a))) 0 c)"
    );
}

#[test]
fn what_is_not_the_shape_is_declined() {
    // a function value, a named fn, an annotated fn, three parameters
    for src in [
        "(reduce f 0 c)",
        "(reduce (fn go (a x) (reduced a)) 0 c)",
        "(reduce (fn (a x) -> i64 (reduced a)) 0 c)",
        "(reduce (fn (a x) :where ((Eq a)) (reduced a)) 0 c)",
        "(reduce (fn (a x y) (reduced a)) 0 c)",
        "(reduce (fn (a) (reduced a)) 0 c)",
        "(reduce)",
        "(reduce f)",
        "(reduce f 0 c d)",
    ] {
        assert_eq!(ex(src), src, "{src}");
    }
}

#[test]
fn a_declined_call_still_has_its_arguments_expanded() {
    assert_eq!(
        ex("(reduce f (when a b) (and c d))"),
        "(reduce f (if a b ()) (if c d false))"
    );
}

#[test]
fn the_wrappers_take_the_call_position_and_the_operands_their_own() {
    let form = super::one("(h\n  (reduce (fn (a x)\n   (reduced a)) 0 c))");
    let expanded = crate::expand::expand_expr(
        form,
        &mut crate::expand::ExpandCtx::new(),
        &mut crate::expand::NoRunner,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let call = &expanded.as_list().unwrap_or(&[])[1];
    let lc = |f: &crate::syntax::Form| (f.pos.line, f.pos.col);
    assert_eq!(lc(call), (2, 3), "the built call");
    let f = &call.as_list().unwrap_or(&[])[1];
    assert_eq!(lc(f), (2, 11), "the fn keeps its own");
    let done = &f.as_list().unwrap_or(&[])[2];
    assert_eq!(lc(done), (2, 3), "the built Done");
    assert_eq!(
        lc(&done.as_list().unwrap_or(&[])[1]),
        (3, 13),
        "its operand"
    );
}
