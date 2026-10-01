//! `update` with extra arguments and the `fnil` routing (stdlib design
//! §2.4, tranche 1 R6b; Appendix A11 t24): each expansion as printed, and
//! each call the macro declines.

use super::ex;

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
        "(update (if a b ()) k (if c d false))"
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
        "(fib.coll/update m k (fn (#v.1) (f #v.1 (if a b ()))))"
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
