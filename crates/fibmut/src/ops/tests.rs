use super::*;

/// Every mutant of `src` made by `op`, as the whole mutated source, in the
/// module's order: by position, and by replacement text at one position.
fn made(op: &str, src: &str) -> Vec<String> {
    mutants(src)
        .unwrap()
        .iter()
        .filter(|m| m.op == op)
        .map(|m| m.apply(src))
        .collect()
}

/// The code of a function body, wrapped in a `defun`, so that it is code.
fn body(code: &str) -> String {
    format!("(defun f (a: i64 b: i64) -> i64 {code})")
}

#[test]
fn cmp_flips_each_comparison() {
    let src = body("(if (< a b) (if (>= a b) (if (= a b) 1 2) 3) 4)");
    assert_eq!(
        made("cmp", &src),
        vec![
            body("(if (<= a b) (if (>= a b) (if (= a b) 1 2) 3) 4)"),
            body("(if (< a b) (if (> a b) (if (= a b) 1 2) 3) 4)"),
            body("(if (< a b) (if (>= a b) (if (!= a b) 1 2) 3) 4)"),
        ]
    );
}

#[test]
fn arith_changes_plus_minus_and_times() {
    let src = body("(+ (- a b) (* a b))");
    assert_eq!(
        made("arith", &src),
        vec![
            body("(- (- a b) (* a b))"),
            body("(+ (+ a b) (* a b))"),
            body("(+ (- a b) (+ a b))"),
        ]
    );
}

#[test]
fn bool_swaps_literals_connectives_and_drops_a_not() {
    let src = body("(if (and true (not false)) 1 2)");
    let got = made("bool", &src);
    assert_eq!(
        got,
        vec![
            body("(if (or true (not false)) 1 2)"),
            body("(if (and false (not false)) 1 2)"),
            body("(if (and true false) 1 2)"),
            body("(if (and true (not true)) 1 2)"),
        ]
    );
    assert_eq!(made("bool", &body("(or a b)")), vec![body("(and a b)")]);
}

#[test]
fn const_makes_three_neighbours_of_an_integer() {
    assert_eq!(
        made("const", &body("(+ a 5)")),
        vec![body("(+ a 0)"), body("(+ a 4)"), body("(+ a 6)")]
    );
    // zero has two neighbours, and so has one (its predecessor is zero); a
    // width and a sign survive; a value out of its width is not offered
    assert_eq!(made("const", &body("0")), vec![body("-1"), body("1")]);
    assert_eq!(made("const", &body("1")), vec![body("0"), body("2")]);
    assert_eq!(
        made("const", &body("-3i8")),
        vec![body("-2i8"), body("-4i8"), body("0i8")]
    );
    assert_eq!(
        made("const", &body("127i8")),
        vec![body("0i8"), body("126i8")]
    );
    assert_eq!(
        made("const", &body("0x1F")),
        vec![body("0"), body("30"), body("32")]
    );
    assert_eq!(
        made("const", &body("1_000")),
        vec![body("0"), body("1001"), body("999")]
    );
}

#[test]
fn const_leaves_floats_symbols_and_types_alone() {
    for code in ["2.5", "1e9", "x1", "+5", "_1", "-", "(- a 1.5)"] {
        let ints = made("const", &body(code));
        assert!(ints.is_empty(), "{code}: {ints:?}");
    }
    // an integer in a type or a signature is not code
    let src = "(defun f (a: (Foo 3)) -> (Foo 4) :where ((Bar 5)) (let ((x: (Foo 6) 7)) x))";
    let made_ = made("const", src);
    assert_eq!(made_.len(), 3);
    assert!(made_.iter().all(|m| m.contains("(Foo 3)")
        && m.contains("(Foo 4)")
        && m.contains("(Bar 5)")
        && m.contains("(Foo 6)")));
    assert!(
        made_.iter().any(|m| m.contains("(let ((x: (Foo 6) 8)) x)")),
        "{made_:?}"
    );
}

#[test]
fn branch_swaps_the_arms_and_negates_the_test() {
    let src = body("(if (< a b) a b)");
    // the negation edits the test, which comes before the arms
    assert_eq!(
        made("branch", &src),
        vec![body("(if (not (< a b)) a b)"), body("(if (< a b) b a)")]
    );
    // an if with no else is not a site
    assert!(made("branch", &body("(if (< a b) a)")).is_empty());
    // equal arms: swapping them changes nothing, so only the negation is offered
    assert_eq!(
        made("branch", &body("(if (< a b) a a)")),
        vec![body("(if (not (< a b)) a a)")]
    );
}

#[test]
fn clause_deletes_one_clause_of_a_match_or_a_cond() {
    let src = body("(match a (0 1) (1 2) (_ 3))");
    assert_eq!(
        made("clause", &src),
        vec![
            body("(match a  (1 2) (_ 3))"),
            body("(match a (0 1)  (_ 3))"),
            body("(match a (0 1) (1 2) )"),
        ]
    );
    let cond = body("(cond ((< a b) 1) (true 2))");
    assert_eq!(made("clause", &cond).len(), 2);
    assert!(made("clause", &body("(match a (_ 1))")).is_empty());
}

#[test]
fn swap_exchanges_the_arguments_of_a_call_of_two_variables() {
    assert_eq!(made("swap", &body("(g a b)")), vec![body("(g b a)")]);
    for code in [
        "(g a a)",
        "(g a 1)",
        "(g 1 2)",
        "(g a b c)",
        "(g a)",
        "(. a b)",
        "(if a b)",
        "(g a :k)",
    ] {
        assert!(made("swap", &body(code)).is_empty(), "{code}");
    }
    // `-` and `<` care about the order, `+` and `=` do not
    assert_eq!(made("swap", &body("(- a b)")), vec![body("(- b a)")]);
    assert_eq!(made("swap", &body("(< a b)")), vec![body("(< b a)")]);
    for code in ["(+ a b)", "(* a b)", "(= a b)", "(!= a b)", "(bit-xor a b)"] {
        assert!(made("swap", &body(code)).is_empty(), "{code}");
    }
    // recur is a call; a pattern is not code
    assert_eq!(
        made("swap", &body("(recur a b)")),
        vec![body("(recur b a)")]
    );
    let m = body("(match a ((Cons h t) (g h t)) (_ 0))");
    assert_eq!(
        made("swap", &m),
        vec![body("(match a ((Cons h t) (g t h)) (_ 0))")]
    );
}

#[test]
fn stmt_deletes_a_set_and_a_form_of_a_do_before_the_last() {
    let src = body("(do (set! a 1) (g a) b)");
    assert_eq!(
        made("stmt", &src),
        vec![body("(do () (g a) b)"), body("(do (set! a 1) () b)")]
    );
    // a set! that is not a statement of a do is deleted too
    assert_eq!(
        made("stmt", &body("(if a (set! b 2) ())")),
        vec![body("(if a () ())")]
    );
    // the last form of a do is its value: not a site
    assert!(made("stmt", &body("(do b)")).is_empty());
}

#[test]
fn exit_flips_the_tails_of_an_each_while_callback() {
    let src = body("(each-while c (fn (x) (if (< x 3) (do (set! n 1) true) false)))");
    assert_eq!(
        made("exit", &src),
        vec![
            body("(each-while c (fn (x) (if (< x 3) (do (set! n 1) false) false)))"),
            body("(each-while c (fn (x) (if (< x 3) (do (set! n 1) true) true)))"),
        ]
    );
    // another call's callback, and a true that is not in a tail, are not sites
    assert!(made("exit", &body("(map c (fn (x) true))")).is_empty());
    assert!(made("exit", &body("(each-while c (fn (x) (do (g true) (k x))))")).is_empty());
}

#[test]
fn exit_wins_the_label_over_bool_for_the_same_edit() {
    let src = body("(each-while c (fn (x) true))");
    let ms = mutants(&src).unwrap();
    assert_eq!(ms.iter().filter(|m| m.op == "exit").count(), 1);
    assert_eq!(ms.iter().filter(|m| m.op == "bool").count(), 0);
}

#[test]
fn only_code_is_mutated() {
    let src = "(ns m (:use x))\n\
               ;; (+ 1 2) in a comment\n\
               (defstruct (P a) (x: a y: i64))\n\
               (defenum E (A n: i64) B)\n\
               (defun f (a: i64) -> i64 :where ((C 1)) \"(+ 1 2)\" 'q (quote (+ 3 4)) a)";
    assert!(
        mutants(src).unwrap().is_empty(),
        "{:?}",
        mutants(src).unwrap()
    );
}

#[test]
fn methods_defaults_and_defs_are_code() {
    let impl_ = "(impl (P a) (Q a) (m (self k: (fn (e) bool) :borrow) -> i64 (+ 1 2)))";
    assert_eq!(made("arith", impl_).len(), 1);
    let proto = "(defprotocol (P s e) (m (self) -> i64 (+ 1 2)) (n (self) -> i64))";
    assert_eq!(made("arith", proto).len(), 1);
    assert_eq!(made("const", "(def k: i64 10)").len(), 3);
    assert_eq!(made("arith", "(defmacro m (x) (+ 1 x))").len(), 1);
    assert_eq!(made("arith", "(defn f [x: i64] -> i64 (+ x 1))").len(), 1);
}

#[test]
fn bindings_and_patterns() {
    // let and loop: the init is code, the name and its type are not
    let src = body("(let ((n 0) (m: i64 7)) (loop ((i 1)) (match i (3 n) (_ (recur 2)))))");
    let ints: Vec<String> = made("const", &src);
    // n+1, n-1 and 0 without repeats: 0 gives 2, 7 gives 3, the loop's 1
    // gives 2 (its predecessor is zero), the literal pattern 3 gives 3, and
    // the 2 in recur gives 3
    assert_eq!(ints.len(), 2 + 3 + 2 + 3 + 3, "{ints:?}");
    assert!(ints
        .iter()
        .all(|m| m.contains("(m: i64 ") && m.contains("(_ (recur ")));
    // a structured pattern is not code: the 1 inside `(Some 1)` is never edited
    let p = body("(match a ((Some 1) 0) (_ 9))");
    let in_bodies = made("const", &p);
    assert_eq!(in_bodies.len(), 2 + 3, "{in_bodies:?}");
    assert!(in_bodies.iter().all(|m| m.contains("((Some 1) ")));
}

#[test]
fn a_discarded_form_and_a_comment_are_not_sites() {
    let src = body("(+ a #_(+ 1 2) b ; (+ 3 4)\n)");
    assert_eq!(made("arith", &src).len(), 1);
}

#[test]
fn the_sites_come_in_source_order_without_repeats() {
    let src = body("(if (< a 1) (+ a 2) (- b 3))");
    let ms = mutants(&src).unwrap();
    let starts: Vec<usize> = ms.iter().map(|m| m.start).collect();
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    for (i, m) in ms.iter().enumerate() {
        assert!(ms[i + 1..].iter().all(|n| n != m), "repeated {m:?}");
        assert_ne!(m.apply(&src), src);
    }
    // cmp 1; const 2 + 3 + 3 (n+1, n-1 and 0 of 1, 2 and 3, the predecessor
    // of 1 being 0); arith 2; branch 2; no swap, there is no variable pair
    assert_eq!(ms.len(), 1 + 8 + 2 + 2, "{ms:#?}");
}

#[test]
fn an_unsplittable_module_is_an_error() {
    assert!(mutants("(defun f (").is_err());
}

#[test]
fn line_and_apply() {
    let src = "(ns m)\n(defun f () -> i64 (+ 1\n 2))";
    let ms = mutants(src).unwrap();
    let plus = ms.iter().find(|m| m.op == "arith").unwrap();
    assert_eq!(plus.line(src), 2);
    assert_eq!(plus.apply(src), "(ns m)\n(defun f () -> i64 (- 1\n 2))");
    let two = ms.iter().find(|m| m.text == "3").unwrap();
    assert_eq!(two.line(src), 3);
}
