//! The printing macros of stdlib §2.7, §2.10 and §4.14 (tranche 1 R5):
//! `str`, `println`, `print`, `prn`, `pr`, each against the text of its
//! expansion in the dump format.

use super::{ex, one};
use crate::expand::{expand_expr, ExpandCtx, NoRunner};
use crate::syntax::Form;

const CONCAT: &str = "fib.prelude/str-concat";

#[test]
fn str_with_no_argument_is_the_empty_text() {
    assert_eq!(ex("(str)"), "\"\"");
}

#[test]
fn str_of_one_argument_is_its_to_str() {
    assert_eq!(ex("(str a)"), "(fib.core/to-str a)");
    assert_eq!(ex("(str (f a 1))"), "(fib.core/to-str (f a 1))");
}

#[test]
fn str_of_several_arguments_is_a_right_fold_of_concat() {
    assert_eq!(
        ex("(str a b)"),
        format!("({CONCAT} (fib.core/to-str a) (fib.core/to-str b))")
    );
    assert_eq!(
        ex("(str a b c)"),
        format!(
            "({CONCAT} (fib.core/to-str a) ({CONCAT} (fib.core/to-str b) (fib.core/to-str c)))"
        )
    );
}

#[test]
fn str_takes_a_string_literal_whole_and_a_literal_nil_as_nothing() {
    assert_eq!(
        ex("(str \"x=\" a \"!\")"),
        format!("({CONCAT} \"x=\" ({CONCAT} (fib.core/to-str a) \"!\"))")
    );
    assert_eq!(ex("(str \"x\")"), "\"x\"");
    assert_eq!(ex("(str nil)"), "\"\"");
    assert_eq!(
        ex("(str a nil)"),
        format!("({CONCAT} (fib.core/to-str a) \"\")")
    );
}

#[test]
fn str_expands_the_macros_among_its_arguments_and_asks_their_text_to_str() {
    assert_eq!(
        ex("(str (str a b) c)"),
        format!(
            "({CONCAT} (fib.core/to-str ({CONCAT} (fib.core/to-str a) (fib.core/to-str b))) \
             (fib.core/to-str c))"
        )
    );
}

#[test]
fn println_shows_each_argument_and_joins_with_a_space() {
    assert_eq!(ex("(println)"), "(fib.prelude/println \"\")");
    assert_eq!(
        ex("(println a)"),
        "(fib.prelude/println (fib.prelude/show a))"
    );
    assert_eq!(
        ex("(println a b c)"),
        format!(
            "(fib.prelude/println ({CONCAT} (fib.prelude/show a) ({CONCAT} \" \" \
             ({CONCAT} (fib.prelude/show b) ({CONCAT} \" \" (fib.prelude/show c))))))"
        )
    );
}

#[test]
fn a_string_literal_is_shown_like_any_argument_and_a_literal_nil_is_the_word_nil() {
    assert_eq!(
        ex("(println \"hi\")"),
        "(fib.prelude/println (fib.prelude/show \"hi\"))"
    );
    assert_eq!(ex("(println nil)"), "(fib.prelude/println \"nil\")");
    assert_eq!(
        ex("(println \"n =\" n)"),
        format!(
            "(fib.prelude/println ({CONCAT} (fib.prelude/show \"n =\") \
             ({CONCAT} \" \" (fib.prelude/show n))))"
        )
    );
}

#[test]
fn print_is_println_without_the_newline() {
    assert_eq!(ex("(print)"), "(fib.prelude/print-str \"\")");
    assert_eq!(
        ex("(print a)"),
        "(fib.prelude/print-str (fib.prelude/show a))"
    );
    assert_eq!(
        ex("(print a \"b\")"),
        format!(
            "(fib.prelude/print-str ({CONCAT} (fib.prelude/show a) \
             ({CONCAT} \" \" (fib.prelude/show \"b\"))))"
        )
    );
}

#[test]
fn prn_and_pr_are_the_same_over_debug() {
    assert_eq!(ex("(prn)"), "(fib.prelude/println \"\")");
    assert_eq!(ex("(prn a)"), "(fib.prelude/println (fib.core/debug a))");
    assert_eq!(
        ex("(prn \"s\" nil)"),
        format!("(fib.prelude/println ({CONCAT} (fib.core/debug \"s\") ({CONCAT} \" \" \"nil\")))")
    );
    assert_eq!(ex("(pr)"), "(fib.prelude/print-str \"\")");
    assert_eq!(ex("(pr a)"), "(fib.prelude/print-str (fib.core/debug a))");
    assert_eq!(
        ex("(pr a b)"),
        format!(
            "(fib.prelude/print-str ({CONCAT} (fib.core/debug a) ({CONCAT} \" \" (fib.core/debug b))))"
        )
    );
}

#[test]
fn the_names_are_values_when_they_are_not_in_head_position() {
    // `(map str xs)` is the function twin's call: the symbol is left alone.
    assert_eq!(ex("(map str xs)"), "(map str xs)");
    assert_eq!(ex("(run! println xs)"), "(run! println xs)");
}

fn expand(src: &str) -> Form {
    expand_expr(one(src), &mut ExpandCtx::new(), &mut NoRunner).unwrap_or_else(|e| panic!("{e}"))
}

fn lc(f: &Form) -> (usize, usize) {
    (f.pos.line, f.pos.col)
}

#[test]
fn built_forms_take_the_call_position_and_arguments_keep_their_own() {
    // (g\n  (println a\n    "s")) : the call is at 2:3
    let f = expand("(g\n  (println a\n    \"s\"))");
    let call = &f.as_list().unwrap_or(&[])[1];
    let items = call.as_list().unwrap_or(&[]);
    assert_eq!(lc(call), (2, 3));
    assert_eq!(lc(&items[0]), (2, 3), "the built fib.prelude/println");
    let concat = items[1].as_list().unwrap_or(&[]);
    assert_eq!(lc(&items[1]), (2, 3), "the built concat");
    let shown = concat[1].as_list().unwrap_or(&[]);
    assert_eq!(lc(&concat[1]), (2, 3), "the built show");
    assert_eq!(lc(&shown[1]), (2, 12), "the argument keeps its own");
    let rest = concat[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&rest[1]), (2, 3), "the built separator");
    let second = rest[2].as_list().unwrap_or(&[]);
    assert_eq!(lc(&rest[2]), (2, 3), "the built show");
    assert_eq!(lc(&second[1]), (3, 5), "the literal keeps its own");
}
