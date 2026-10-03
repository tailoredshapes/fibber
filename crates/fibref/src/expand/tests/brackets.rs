//! Bracket binding forms (syntax §3.3, §3.18, §3.2, §4.4): `let` and
//! `loop` take flat pairs, `fn` a bracket parameter list, `if-let`,
//! `when-let` and `dotimes` a bracket pair, each the same expansion as the
//! parenthesised spelling.

use super::{ex, ex_err, ex_pos};
use crate::expand::error::ExpandErrorKind as K;

#[test]
fn let_takes_flat_pairs() {
    assert_eq!(ex("(let [a 1 b 2] (+ a b))"), "(let ((a 1) (b 2)) (+ a b))");
    assert_eq!(ex("(let [] x)"), "(let () x)");
    // The initialiser of a binding is expanded like any expression.
    assert_eq!(ex("(let [a (when c 1)] a)"), "(let ((a (if c 1))) a)");
}

#[test]
fn a_name_that_ends_in_a_colon_takes_its_type_and_then_its_value() {
    assert_eq!(
        ex("(let [a: i64 1 b 2 c: (Vec i64) []] a)"),
        "(let ((a: i64 1) (b 2) (c: (Vec i64) (fib.prelude/vec-empty))) a)"
    );
}

#[test]
fn a_pattern_may_stand_in_a_bracket_binding() {
    assert_eq!(
        ex("(let [[a b] p (P x y) q] a)"),
        "(let (([a b] p) ((P x y) q)) a)"
    );
}

#[test]
fn loop_takes_flat_pairs() {
    assert_eq!(
        ex("(loop [i 0 acc 0] (recur (+ i 1) acc))"),
        "(loop ((i 0) (acc 0)) (recur (+ i 1) acc))"
    );
}

#[test]
fn the_parenthesised_spellings_are_untouched() {
    assert_eq!(ex("(let ((a 1) (b 2)) a)"), "(let ((a 1) (b 2)) a)");
    assert_eq!(ex("(let ((a: i64 1)) a)"), "(let ((a: i64 1)) a)");
    assert_eq!(ex("(loop ((i 0)) i)"), "(loop ((i 0)) i)");
}

#[test]
fn brackets_nest_and_mix_with_parentheses() {
    assert_eq!(
        ex("(let [a 1] (loop [i a] (let ((j i)) j)))"),
        "(let ((a 1)) (loop ((i a)) (let ((j i)) j)))"
    );
}

#[test]
fn a_binding_with_no_expression_is_malformed_at_the_binding() {
    for (src, head, col) in [
        ("(let [a 1 b] a)", "let", 11),
        ("(let [a: i64] a)", "let", 7),
        ("(let [a: i64 1 b: i64] a)", "let", 16),
        ("(loop [i 0 acc] i)", "loop", 12),
    ] {
        let e = ex_err(src);
        assert!(
            matches!(e.kind, K::Malformed { head: ref h, .. } if h == head),
            "{src}: {e}"
        );
        assert_eq!((e.pos.line, e.pos.col), (1, col), "{src}");
    }
    let e = ex_err("(let [a 1 b] a)");
    assert_eq!(
        e.to_string(),
        "t.fib:1:11: malformed let: a binding is a pattern and an expression, \
         or name: a type and an expression"
    );
}

#[test]
fn fn_takes_a_bracket_parameter_list() {
    assert_eq!(ex("(fn [x y] (+ x y))"), "(fn (x y) (+ x y))");
    assert_eq!(ex("(fn [] 1)"), "(fn () 1)");
    assert_eq!(ex("(fn [x: i64] -> i64 x)"), "(fn (x: i64) -> i64 x)");
    assert_eq!(ex("(fn go [n] (go n))"), "(fn go (n) (go n))");
    // A pattern parameter goes on to the desugar of stdlib §7 L7.
    assert_eq!(
        ex("(fn [acc [k v]] (+ k v))"),
        "(fn (acc #param.1) (let (([k v] #param.1)) (+ k v)))"
    );
}

#[test]
fn the_rewritten_forms_keep_their_positions() {
    // The pair is where its pattern is, the list where the vector was.
    assert_eq!(
        ex_pos("(let [a 1 b 2] a)"),
        "(let@1:2 ((a@1:7 1@1:9)@1:7 (b@1:11 2@1:13)@1:11)@1:6 a@1:16)@1:1"
    );
    assert_eq!(
        ex_pos("(fn [x y] x)"),
        "(fn@1:2 (x@1:6 y@1:8)@1:5 x@1:11)@1:1"
    );
}

#[test]
fn if_let_and_when_let_take_a_bracket_pair() {
    assert_eq!(
        ex("(if-let [x e] a b)"),
        "(match e ((fib.prelude/some x) a) (_ b))"
    );
    assert_eq!(ex("(if-let [x e] a b)"), ex("(if-let (x e) a b)"));
    assert_eq!(ex("(when-let [x e] a b)"), ex("(when-let (x e) a b)"));
    assert_eq!(
        ex("(if-let [[a b] o] a b)"),
        "(match o ((fib.prelude/some [a b]) a) (_ b))"
    );
    for src in ["(if-let [x] a b)", "(when-let [x e y] a)", "(if-let x a b)"] {
        let e = ex_err(src);
        assert!(matches!(e.kind, K::Malformed { .. }), "{src}: {e}");
    }
}

#[test]
fn dotimes_takes_a_bracket_pair() {
    assert_eq!(ex("(dotimes [i n] (f i))"), ex("(dotimes (i n) (f i))"));
    let e = ex_err("(dotimes [i] (f i))");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "dotimes"));
}

#[test]
fn reduce_and_for_each_read_a_bracket_fn() {
    assert_eq!(
        ex("(reduce (fn [acc x] (if (> acc 10) (reduced acc) (+ acc x))) 0 c)"),
        "(fib.seq/reduce-while (fn (acc x) (if (> acc 10) (fib.core/Done acc) \
         (fib.core/More (+ acc x)))) 0 c)"
    );
    assert_eq!(
        ex("(for-each (range 0 n) (fn [i] (f i)))"),
        ex("(for-each (range 0 n) (fn (i) (f i)))")
    );
}

#[test]
fn a_cond_in_a_reduce_tail_reads_the_flat_form() {
    assert_eq!(
        ex("(reduce (fn [a x] (cond (p a) (reduced 1) (q x) 2 :else (reduced a))) 0 c)"),
        "(fib.seq/reduce-while (fn (a x) (if (p a) (fib.core/Done 1) \
         (if (q x) (fib.core/More 2) (fib.core/Done a)))) 0 c)"
    );
    // A cond whose tails hold no `reduced` leaves the call alone.
    assert_eq!(
        ex("(reduce (fn [a x] (cond (p a) 1 :else 2)) 0 c)"),
        "(reduce (fn (a x) (if (p a) 1 2)) 0 c)"
    );
}
