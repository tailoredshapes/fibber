//! Vector patterns and guards in the ownership pass (types §6.1, §6.3
//! the `match` clause row, §6.10, §6.11).

use super::super::program::{BindKind, Site};
use super::{all, binding, fun, ok, op_text, ops, param};

/// The false-edge operations of every guard of `fun_name`, as text.
fn guard_fail(c: &crate::own::Checked, fun_name: &str) -> Vec<Vec<String>> {
    let (b, body) = fun(c, fun_name);
    all(body)
        .into_iter()
        .filter_map(|e| b.guard_fail.get(&e.id))
        .map(|ops| ops.iter().map(|o| op_text(c, body, o)).collect())
        .collect()
}

#[test]
fn elements_are_derived_and_a_rest_owns_a_heap_vector() {
    let c = ok("(defun f (v: (Vec (Vec i64))) -> i64
                  (match v ([a & r] (if (> (count a) (count r)) 1 2)) ([] 0)))
                (defun main () -> i64 (f [[1]]))");
    let a = binding(&c, "f", "a");
    let v = param(&c, "f", "v").binding;
    assert_eq!(a.kind, BindKind::DerivedOf(Site::Bind(v)));
    let r = binding(&c, "f", "r");
    assert_eq!(r.kind, BindKind::Owns);
    assert!(!r.scope_local, "a rest is a call result (§6.11)");
    assert!(ops(&c, "f").contains(&"release r (exit)".to_string()));
    assert!(!param(&c, "f", "v").escapes, "a rest is not the scrutinee");
}

#[test]
fn a_false_guard_releases_the_clause_rests_in_reverse_order() {
    let c = ok("(defun f (v: (Vec (Vec i64))) -> i64
                  (match v
                    ([[x & p] & q] :when (> x 0) (count q))
                    ([_ & q] :when false 1)
                    (_ 0)))
                (defun main () -> i64 (f []))");
    assert_eq!(
        guard_fail(&c, "f"),
        vec![
            vec![
                "release q (exit)".to_string(),
                "release p (exit)".to_string()
            ],
            vec!["release q (exit)".to_string()],
        ]
    );
}

#[test]
fn a_guard_without_rests_releases_nothing_and_its_temporaries_die_at_its_end() {
    let c = ok("(defun f (v: (Vec i64)) -> i64
                  (match v (x :when (> (count (conj x 1)) 2) 1) (_ 0)))
                (defun main () -> i64 (f []))");
    assert_eq!(guard_fail(&c, "f"), vec![Vec::<String>::new()]);
    assert!(ops(&c, "f").contains(&"release (conj) (step)".to_string()));
}

#[test]
fn a_returned_rest_is_moved_out_and_a_let_rest_is_owned_by_the_let() {
    let c = ok(
        "(defun f (v: (Vec i64)) -> (Vec i64) (match v ([_ & r] r) ([] v)))
                (defun g (v: (Vec i64)) -> (Vec i64) (let (([& all] v)) all))
                (defun main () -> i64 (+ (count (f [1])) (count (g [1]))))",
    );
    assert!(!ops(&c, "f").contains(&"release r (exit)".to_string()));
    assert_eq!(binding(&c, "g", "all").kind, BindKind::Owns);
    assert!(!ops(&c, "g").contains(&"release all (exit)".to_string()));
}

#[test]
fn a_rest_passed_at_a_tail_call_from_a_guarded_body_is_moved() {
    let c = ok("(defun s (v: (Vec i64) acc: i64) -> i64
                  (match v ([x & r] :when (> x 0) (s r (+ acc x))) ([_ & r] (s r acc)) ([] acc)))
                (defun main () -> i64 (s [1] 0))");
    let (b, body) = fun(&c, "s");
    let tails: Vec<_> = all(body)
        .into_iter()
        .filter_map(|e| b.calls.get(&e.id))
        .filter(|call| call.tail == super::super::program::Tail::TailCall)
        .collect();
    assert_eq!(tails.len(), 2);
    for t in tails {
        assert_eq!(t.args[0], super::super::program::Pass::Move);
        let jump: Vec<String> = t.jump.iter().map(|o| op_text(&c, body, o)).collect();
        assert!(!jump.contains(&"release r (jump)".to_string()), "{jump:?}");
    }
}
