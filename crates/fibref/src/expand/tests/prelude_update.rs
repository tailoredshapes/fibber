//! `update` with extra arguments and the `fnil` routing (stdlib design
//! §2.4, tranche 1 R6b; Appendix A11 t24): each expansion as printed, and
//! each call the macro declines.

use super::{ex, ex_pos, one};
use crate::expand::error::ExpandErrorKind as K;
use crate::expand::{expand_expr, ExpandCtx, Limits, NoRunner};

#[test]
fn extra_arguments_go_into_a_closure_over_the_value() {
    assert_eq!(
        ex("(update m \"a\" + 10)"),
        "(fib.coll/update m \"a\" (fn (#v.1) (+ #v.1 10)))"
    );
    assert_eq!(
        ex("(update m k f x y)"),
        "(fib.coll/update m k (fn (#v.1) (f #v.1 x y)))"
    );
}

#[test]
fn a_literal_fnil_is_update_or() {
    assert_eq!(
        ex("(update m \"w\" (fnil g 100))"),
        "(fib.coll/update-or m \"w\" g 100)"
    );
    // the call of Appendix A11 t24: 107
    assert_eq!(
        ex("(update m \"w\" (fnil + 100) 7)"),
        "(fib.coll/update-or m \"w\" (fn (#v.1) (+ #v.1 7)) 100)"
    );
}

#[test]
fn a_call_the_library_function_serves_is_declined() {
    assert_eq!(ex("(update m k f)"), "(update m k f)");
    assert_eq!(ex("(update m k (fnil g))"), "(update m k (fnil g))");
    assert_eq!(ex("(update m k (fnil g d e))"), "(update m k (fnil g d e))");
    assert_eq!(ex("(update m k)"), "(update m k)");
    assert_eq!(ex("(update m)"), "(update m)");
    assert_eq!(ex("(update)"), "(update)");
}

#[test]
fn a_declined_call_still_has_its_arguments_expanded() {
    assert_eq!(
        ex("(update (when a b) k (and c d))"),
        "(update (if a b) k (and c d))"
    );
}

#[test]
fn only_a_fnil_with_one_function_and_one_default_is_routed() {
    // four extra arguments and a computed f: an ordinary closure
    assert_eq!(
        ex("(update m k (comp a b) x)"),
        "(fib.coll/update m k (fn (#v.1) ((comp a b) #v.1 x)))"
    );
    assert_eq!(
        ex("(update m k (fnil g d e) x)"),
        "(fib.coll/update m k (fn (#v.1) ((fnil g d e) #v.1 x)))"
    );
}

#[test]
fn the_arguments_are_expanded_inside_the_closure() {
    assert_eq!(
        ex("(update m k f (when a b))"),
        "(fib.coll/update m k (fn (#v.1) (f #v.1 (if a b))))"
    );
}

#[test]
fn two_updates_in_one_form_get_two_gensyms() {
    assert_eq!(
        ex("(update (update m k f x) k g y)"),
        "(fib.coll/update (fib.coll/update m k (fn (#v.2) (f #v.2 x))) k (fn (#v.1) (g #v.1 y)))"
    );
}

#[test]
fn no_argument_count_is_an_error_of_the_macro() {
    // the library function reports its own arity
    for src in ["(update)", "(update a)", "(update a b)"] {
        assert_eq!(ex(src), src);
    }
}

#[test]
fn the_built_forms_take_the_call_position_and_the_operands_their_own() {
    assert_eq!(
        ex_pos("(h\n  (update m k f x))"),
        "(h@1:2 (fib.coll/update@2:3 m@2:11 k@2:13 (fn@2:3 (#v.1@2:3)@2:3 \
         (f@2:15 #v.1@2:3 x@2:17)@2:3)@2:3)@2:3)@1:1"
    );
    assert_eq!(
        ex_pos("(h\n  (update m k (fnil g d)))"),
        "(h@1:2 (fib.coll/update-or@2:3 m@2:11 k@2:13 g@2:21 d@2:23)@2:3)@1:1"
    );
    assert_eq!(
        ex_pos("(h\n  (update m k (fnil g d) x))"),
        "(h@1:2 (fib.coll/update-or@2:3 m@2:11 k@2:13 (fn@2:3 (#v.1@2:3)@2:3 \
         (g@2:21 #v.1@2:3 x@2:26)@2:3)@2:3 d@2:23)@2:3)@1:1"
    );
}

#[test]
fn a_declined_call_keeps_its_own_position() {
    assert_eq!(
        ex_pos("(h\n  (update m k f))"),
        "(h@1:2 (update@2:4 m@2:11 k@2:13 f@2:15)@2:3)@1:1"
    );
}

#[test]
fn an_expansion_is_admitted_like_any_macro_result() {
    // twelve forms: the call and its head, `m` and `k`, the closure with its
    // parameter list, its body, `f`, the value and `x`
    let at = |max_forms: usize| {
        let mut ctx = ExpandCtx::new();
        ctx.limits = Limits {
            max_forms,
            ..Limits::default()
        };
        expand_expr(one("(update m k f x)"), &mut ctx, &mut NoRunner).map_err(|e| e.kind)
    };
    assert_eq!(at(11), Err(K::TooLarge { limit: 11 }));
    assert!(at(12).is_ok(), "{:?}", at(12));
}
