//! The prelude macros of §4.4, each against the expansion the table
//! gives (after the result is itself expanded).

use super::{ex, ex_err, v};
use crate::expand::error::ExpandErrorKind as K;

#[test]
fn when_and_unless() {
    assert_eq!(ex("(when c x)"), "(if c x)");
    assert_eq!(ex("(when c x y)"), "(if c (do x y))");
    assert_eq!(ex("(when c)"), "(if c (do))");
    assert_eq!(ex("(unless c x y)"), "(if (fib.prelude/not c) (do x y))");
}

#[test]
fn and_or() {
    // Core forms (stdlib §7 L20): the expander passes them through.
    assert_eq!(ex("(and)"), "(and)");
    assert_eq!(ex("(and a b c)"), "(and a b c)");
    assert_eq!(ex("(or a (and b c))"), "(or a (and b c))");
}

#[test]
fn cond_is_flat_nests_ifs_and_traps_when_nothing_matches() {
    assert_eq!(
        ex("(cond a 1 b (do 2 3) :else 4)"),
        "(if a 1 (if b (do 2 3) 4))"
    );
    // Any keyword is a test that is always true.
    assert_eq!(ex("(cond a 1 :default 4)"), "(if a 1 4)");
    // Without a default the last `if` is one-armed (stdlib §7 L20); a `true` test is a default.
    assert_eq!(ex("(cond a 1)"), "(if a 1)");
    assert_eq!(ex("(cond a 1 b 2)"), "(if a 1 (if b 2))");
    assert_eq!(ex("(cond a 1 true 2)"), "(if a 1 2)");
    assert_eq!(ex("(cond)"), "()");
}

#[test]
fn cond_has_no_special_symbol_else() {
    // `else` is a variable like any other: a test, not the default.
    assert_eq!(ex("(cond a 1 else 4)"), "(if a 1 (if else 4))");
}

#[test]
fn cond_rejects_a_test_with_no_expression_and_a_keyword_before_the_end() {
    let e = ex_err("(cond a 1 b)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "cond"));
    assert_eq!(
        e.to_string(),
        "t.fib:1:11: malformed cond: a test with no expression"
    );
    assert_eq!((e.pos.line, e.pos.col), (1, 11));
    // The old paired clause is one form, so an odd count says so.
    let e = ex_err("(cond (a 1))");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "cond"));
    assert_eq!((e.pos.line, e.pos.col), (1, 7));
    let e = ex_err("(cond :else 1 a 2)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "cond"));
    assert_eq!((e.pos.line, e.pos.col), (1, 7));
}

#[test]
fn if_let_and_when_let_are_match() {
    // The else is a wildcard, so a refutable pattern falls to it (E13).
    assert_eq!(
        ex("(if-let (x e) a b)"),
        "(match e ((fib.prelude/some x) a) (_ b))"
    );
    assert_eq!(
        ex("(when-let (x e) a b)"),
        "(match e ((fib.prelude/some x) (if true (do a b))) (_ (fib.prelude/elide)))"
    );
    let e = ex_err("(if-let x a b)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "if-let"));
}

#[test]
fn if_let_takes_any_pattern_and_has_an_optional_else() {
    // E13: `[a b]` is refutable; the expansion puts it under `some` and
    // leaves the mismatch to the wildcard clause.
    assert_eq!(
        ex("(if-let ([a b] o) a b)"),
        "(match o ((fib.prelude/some [a b]) a) (_ b))"
    );
    assert_eq!(
        ex("(if-let ((P a b) o) a b)"),
        "(match o ((fib.prelude/some (P a b)) a) (_ b))"
    );
    assert_eq!(
        ex("(when-let ([a & r] o) a)"),
        "(match o ((fib.prelude/some [a & r]) (if true a)) (_ (fib.prelude/elide)))"
    );
    // The else may be left out: it is `()`.
    assert_eq!(
        ex("(if-let (x e) a)"),
        "(match e ((fib.prelude/some x) a) (_ ()))"
    );
}

#[test]
fn if_let_arity_is_two_or_three() {
    for (src, found) in [("(if-let)", 0), ("(if-let (x e) a b c)", 4)] {
        let e = ex_err(src);
        let expected = "2 to 3".to_string();
        assert_eq!(
            e.kind,
            K::MacroArity {
                name: "if-let".into(),
                expected,
                found
            },
            "{src}"
        );
    }
}

#[test]
fn list_is_cons_cells() {
    assert_eq!(
        ex("(list a b)"),
        "(fib.prelude/Cons a (fib.prelude/Cons b fib.prelude/Empty))"
    );
    assert_eq!(ex("(list)"), "fib.prelude/Empty");
}

#[test]
fn plet_spawns_then_joins() {
    assert_eq!(
        ex("(plet ((a e1) (b e2)) (f a b))"),
        "(let ((#a.1 (fib.prelude/spawn (fn () e1))) (#b.2 (fib.prelude/spawn (fn () e2)))) \
         (let ((a (fib.prelude/join #a.1)) (b (fib.prelude/join #b.2))) (f a b)))"
    );
    let e = ex_err("(plet ((1 e)) x)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "plet"));
    let e = ex_err("(plet () x)");
    assert!(matches!(e.kind, K::Malformed { .. }));
}

#[test]
fn while_is_the_table_loop() {
    assert_eq!(ex("(while c x y)"), "(loop () (if c (do x y (recur)) ()))");
}

#[test]
fn dotimes_is_the_table_loop() {
    assert_eq!(
        ex("(dotimes (i n) (f i))"),
        "(let ((#m.1 n)) (loop ((i 0)) (if (fib.prelude/< i #m.1) (do (f i) (recur (fib.prelude/+ i 1))) ())))"
    );
    let e = ex_err("(dotimes i (f i))");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "dotimes"));
}

#[test]
fn for_each_over_a_literal_range_is_a_loop() {
    assert_eq!(
        ex("(for-each (range a b) (fn (i) (f i) (g i)))"),
        "(let ((#s.1 a) (#m.2 b)) (loop ((i #s.1)) (if (fib.prelude/< i #m.2) (do (f i) (g i) (recur (fib.prelude/+ i 1))) ())))"
    );
    // (range n) is (range 0 n) (Decided, owner, 2026-09-27; case 86).
    assert_eq!(
        ex("(for-each (range n) (fn (i) i))"),
        "(let ((#s.1 0) (#m.2 n)) (loop ((i #s.1)) (if (fib.prelude/< i #m.2) (do i (recur (fib.prelude/+ i 1))) ())))"
    );
}

#[test]
fn range_takes_one_or_two_arguments_and_declines_three() {
    // (range a b) is the rewrite to the library's range-by with step 1;
    // (range n) stays the library function, which is also range's value.
    assert_eq!(ex("(range a b)"), "(fib.seq/range-by a b 1)");
    assert_eq!(ex("(range n)"), "(range n)");
    assert_eq!(ex("(map range xs)"), "(map range xs)");
    // (range a b step) is the library's range-by: declined, so the
    // checker's arity error of the library function names it (D1).
    assert_eq!(ex("(range a b c)"), "(range a b c)");
    let e = ex_err("(range)");
    assert!(matches!(e.kind, K::MacroArity { ref name, .. } if name == "range"));
    let e = ex_err("(range a b c d)");
    assert!(matches!(e.kind, K::MacroArity { ref name, .. } if name == "range"));
}

#[test]
fn any_other_for_each_is_run_with_the_function_first() {
    assert_eq!(
        ex("(for-each xs (fn (i) (f i)))"),
        "(fib.seq/run! (fn (i) (f i)) xs)"
    );
    assert_eq!(ex("(for-each (range n) f)"), "(fib.seq/run! f (range n))");
    assert_eq!(
        ex("(for-each (range a b) f)"),
        "(fib.seq/run! f (fib.seq/range-by a b 1))"
    );
    assert_eq!(
        ex("(for-each (range a b) (fn g (i) i))"),
        "(fib.seq/run! (fn g (i) i) (fib.seq/range-by a b 1))"
    );
    // An annotated parameter: loop variables take no annotation, so the
    // library function runs the fn and checks it (case 86).
    assert_eq!(
        ex("(for-each (range a b) (fn (i: i64) i))"),
        "(fib.seq/run! (fn (i: i64) i) (fib.seq/range-by a b 1))"
    );
    assert_eq!(
        ex("(for-each (range a b) (fn (i) -> i64 i))"),
        "(fib.seq/run! (fn (i) -> i64 i) (fib.seq/range-by a b 1))"
    );
    for src in ["(for-each xs)", "(for-each)", "(for-each xs f g)"] {
        let e = ex_err(src);
        assert!(
            matches!(e.kind, K::MacroArity { ref name, .. } if name == "for-each"),
            "{src}"
        );
    }
}

#[test]
fn threading() {
    assert_eq!(ex("(-> x (f a) g (h b c))"), "(h (g (f x a)) b c)");
    assert_eq!(ex("(->> x (f a) g (h b c))"), "(h b c (g (f a x)))");
    assert_eq!(ex("(-> x)"), "x");
    assert_eq!(ex("(-> &v (push! 1))"), "(push! (& v) 1)");
}

#[test]
fn threading_steps_must_be_calls() {
    for src in ["(-> x 5)", "(-> x ())", "(->> x [f])", "(-> x :k)"] {
        assert_eq!(ex_err(src).kind, K::ThreadStep, "{src}");
    }
    let e = ex_err("(-> x f 5)");
    assert_eq!((e.pos.line, e.pos.col), (1, 9));
    let e = ex_err("(->)");
    assert!(matches!(e.kind, K::MacroArity { .. }));
}

#[test]
fn doto_binds_once() {
    assert_eq!(
        ex("(doto (mk) (f a) g)"),
        "(let ((#doto.1 (mk))) (f #doto.1 a) (g #doto.1) #doto.1)"
    );
}

#[test]
fn assert_is_if_and_trap() {
    assert_eq!(
        ex("(assert c \"no\")"),
        "(if c () (fib.prelude/trap \"no\"))"
    );
    assert_eq!(
        ex("(assert (< a b))"),
        "(if (< a b) () (fib.prelude/trap \"assert failed at t.fib:1:1: (< a b)\"))"
    );
    assert!(matches!(ex_err("(assert)").kind, K::MacroArity { .. }));
    assert!(matches!(
        ex_err("(assert a b c)").kind,
        K::MacroArity { .. }
    ));
}

#[test]
fn dbg_evaluates_once_prints_with_show_and_returns_the_value() {
    assert_eq!(
        ex("(dbg (f x))"),
        "(let ((#dbg.1 (f x))) (fib.prelude/eprintln (fib.prelude/str-concat \"dbg t.fib:1:1: (f x) = \" (fib.prelude/show #dbg.1))) #dbg.1)"
    );
    assert!(matches!(ex_err("(dbg)").kind, K::MacroArity { .. }));
    assert!(matches!(ex_err("(dbg a b)").kind, K::MacroArity { .. }));
}

#[test]
fn expansions_are_expanded_again() {
    assert_eq!(
        ex("(when (and a b) [x])"),
        format!("(if (and a b) {})", v(&["x"]))
    );
}

/// `src` expanded and printed with the position of every node,
/// `node@line:col`, a list as `(items..)@line:col`.
fn ex_at(src: &str) -> String {
    use crate::expand::{expand_expr, ExpandCtx, NoRunner};
    fn shown(f: &crate::syntax::Form) -> String {
        let at = format!("@{}:{}", f.pos.line, f.pos.col);
        match f.as_list() {
            Some(items) => {
                let inner: Vec<String> = items.iter().map(shown).collect();
                format!("({}){at}", inner.join(" "))
            }
            None => format!("{f}{at}"),
        }
    }
    let form = expand_expr(super::one(src), &mut ExpandCtx::new(), &mut NoRunner)
        .unwrap_or_else(|e| panic!("{src:?}: {e}"));
    shown(&form)
}

#[test]
fn if_let_builds_every_part_at_the_call_and_keeps_the_input_forms_where_they_were() {
    // §1.3: the `match`, the `some` pattern, the wildcard clause and the
    // unit else take the call's position (2:3); `e`, `x`, `a` and `b` keep
    // their own.
    assert_eq!(
        ex_at("(g\n  (if-let (x e) a b))"),
        "(g@1:2 (match@2:3 e@2:14 ((fib.prelude/some@2:3 x@2:12)@2:3 a@2:17)@2:3 (_@2:3 b@2:19)@2:3)@2:3)@1:1"
    );
    assert_eq!(
        ex_at("(g\n  (if-let (x e) a))"),
        "(g@1:2 (match@2:3 e@2:14 ((fib.prelude/some@2:3 x@2:12)@2:3 a@2:17)@2:3 (_@2:3 ()@2:3)@2:3)@2:3)@1:1"
    );
}

#[test]
fn when_let_builds_every_part_at_the_call_and_keeps_the_input_forms_where_they_were() {
    assert_eq!(
        ex_at("(g\n  (when-let (x e) a b))"),
        "(g@1:2 (match@2:3 e@2:16 ((fib.prelude/some@2:3 x@2:14)@2:3 (if@2:3 true@2:3 (do@2:3 a@2:19 b@2:21)@2:3)@2:3)@2:3 (_@2:3 (fib.prelude/elide@2:3)@2:3)@2:3)@2:3)@1:1"
    );
}

#[test]
fn the_binding_of_if_let_and_when_let_is_exactly_a_pattern_and_an_expression() {
    for (src, head) in [
        ("(if-let (x e extra) a b)", "if-let"),
        ("(if-let (x) a b)", "if-let"),
        ("(if-let () a b)", "if-let"),
        ("(when-let (x e extra) a)", "when-let"),
        ("(when-let (x) a)", "when-let"),
        ("(when-let x a)", "when-let"),
    ] {
        let e = ex_err(src);
        assert!(
            matches!(e.kind, K::Malformed { head: ref h, .. } if h == head),
            "{src}: {:?}",
            e.kind
        );
    }
}

#[test]
fn when_let_needs_a_binding_and_no_body_and_if_let_needs_a_then() {
    assert_eq!(
        ex("(when-let (x e))"),
        "(match e ((fib.prelude/some x) (if true (do))) (_ (fib.prelude/elide)))"
    );
    assert_eq!(
        ex_err("(when-let)").kind,
        K::MacroArity {
            name: "when-let".into(),
            expected: "at least 1".into(),
            found: 0
        }
    );
    assert_eq!(
        ex_err("(if-let (x e))").kind,
        K::MacroArity {
            name: "if-let".into(),
            expected: "2 to 3".into(),
            found: 1
        }
    );
}

#[test]
fn range_serves_one_and_two_arguments_declines_three_and_refuses_four() {
    // Three arguments are Clojure's `(range a b step)`: the macro leaves
    // the call to the checker, whose arity error says `use range-by`.
    assert_eq!(ex("(range a b c)"), "(range a b c)");
    assert_eq!(ex("(range a b)"), "(fib.seq/range-by a b 1)");
    assert_eq!(
        ex_err("(range a b c d)").kind,
        K::MacroArity {
            name: "range".into(),
            expected: "1 to 2".into(),
            found: 4
        }
    );
}
