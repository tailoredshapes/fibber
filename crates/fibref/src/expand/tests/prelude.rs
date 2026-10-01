//! The prelude macros of §4.4, each against the expansion the table
//! gives (after the result is itself expanded).

use super::{ex, ex_err, v};
use crate::expand::error::ExpandErrorKind as K;

#[test]
fn when_and_unless() {
    assert_eq!(ex("(when c x)"), "(if c x ())");
    assert_eq!(ex("(when c x y)"), "(if c (do x y) ())");
    assert_eq!(ex("(when c)"), "(if c (do) ())");
    assert_eq!(ex("(unless c x y)"), "(if c () (do x y))");
}

#[test]
fn and_or() {
    assert_eq!(ex("(and)"), "true");
    assert_eq!(ex("(and a)"), "a");
    assert_eq!(ex("(and a b c)"), "(if a (if b c false) false)");
    assert_eq!(ex("(or)"), "false");
    assert_eq!(ex("(or a)"), "a");
    assert_eq!(ex("(or a b c)"), "(if a true (if b true c))");
}

#[test]
fn cond_nests_ifs_and_traps_when_nothing_matches() {
    assert_eq!(
        ex("(cond (a 1) (b 2 3) (:else 4))"),
        "(if a 1 (if b (do 2 3) 4))"
    );
    assert_eq!(ex("(cond (a 1) (else 4))"), "(if a 1 4)");
    assert_eq!(
        ex("(cond (a 1))"),
        "(if a 1 (fib.prelude/trap \"cond: no clause matched at t.fib:1:1\"))"
    );
    assert_eq!(
        ex("(cond)"),
        "(fib.prelude/trap \"cond: no clause matched at t.fib:1:1\")"
    );
}

#[test]
fn cond_rejects_bad_clauses() {
    let e = ex_err("(cond a 1)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "cond"));
    assert_eq!((e.pos.line, e.pos.col), (1, 7));
    let e = ex_err("(cond (else 1) (a 2))");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "cond"));
    let e = ex_err("(cond (a))");
    assert!(matches!(e.kind, K::Malformed { .. }));
}

#[test]
fn if_let_and_when_let_are_match() {
    assert_eq!(
        ex("(if-let (x e) a b)"),
        "(match e ((fib.prelude/some x) a) (nil b))"
    );
    assert_eq!(
        ex("(when-let (x e) a b)"),
        "(match e ((fib.prelude/some x) (do a b)) (nil ()))"
    );
    let e = ex_err("(if-let (x e) a)");
    let expected = "3".to_string();
    assert_eq!(
        e.kind,
        K::MacroArity {
            name: "if-let".into(),
            expected,
            found: 2
        }
    );
    let e = ex_err("(if-let x a b)");
    assert!(matches!(e.kind, K::Malformed { ref head, .. } if head == "if-let"));
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
fn range_takes_one_or_two_arguments() {
    // (range a b) is the rewrite to the library's range-between; (range
    // n) stays the library function, which is also range's value.
    assert_eq!(ex("(range a b)"), "(fib.prelude/range-between a b)");
    assert_eq!(ex("(range n)"), "(range n)");
    assert_eq!(ex("(map range xs)"), "(map range xs)");
    let e = ex_err("(range a b c)");
    assert!(matches!(e.kind, K::MacroArity { ref name, .. } if name == "range"));
    let e = ex_err("(range)");
    assert!(matches!(e.kind, K::MacroArity { ref name, .. } if name == "range"));
}

#[test]
fn any_other_for_each_is_the_library_call() {
    assert_eq!(
        ex("(for-each xs (fn (i) (f i)))"),
        "(for-each xs (fn (i) (f i)))"
    );
    assert_eq!(ex("(for-each (range n) f)"), "(for-each (range n) f)");
    assert_eq!(
        ex("(for-each (range a b) f)"),
        "(for-each (fib.prelude/range-between a b) f)"
    );
    assert_eq!(
        ex("(for-each (range a b) (fn g (i) i))"),
        "(for-each (fib.prelude/range-between a b) (fn g (i) i))"
    );
    // An annotated parameter: loop variables take no annotation, so the
    // library function runs the fn and checks it (case 86).
    assert_eq!(
        ex("(for-each (range a b) (fn (i: i64) i))"),
        "(for-each (fib.prelude/range-between a b) (fn (i: i64) i))"
    );
    assert_eq!(
        ex("(for-each (range a b) (fn (i) -> i64 i))"),
        "(for-each (fib.prelude/range-between a b) (fn (i) -> i64 i))"
    );
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
        format!("(if (if a b false) {} ())", v(&["x"]))
    );
}
