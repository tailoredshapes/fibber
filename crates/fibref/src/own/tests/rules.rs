//! Rules of §6 one at a time, each on the smallest program that shows it.

use super::super::error::OwnErrorKind;
use super::super::program::{Alloc, Because, BodyKey, OwnedWhy, ParamKind, Pass, Tail};
use super::*;

fn own_error(src: &str) -> (OwnErrorKind, String) {
    match rejected(src) {
        CheckError::Own(es) => (es[0].kind, es[0].message.clone()),
        other => panic!("expected an ownership error, got {other}"),
    }
}

#[test]
fn declared_borrow_that_escapes_is_an_error() {
    let (k, m) = own_error(
        "(defun keep (xs: (Vec i64) :borrow) -> (Vec i64) xs)
         (defun main () -> i64 (count (keep [1 2])))",
    );
    assert_eq!(k, OwnErrorKind::BorrowEscapes);
    assert_eq!(m, "parameter xs of keep is declared :borrow but escapes");
}

#[test]
fn an_impl_that_breaks_a_declared_borrow_is_an_error() {
    let (k, m) = own_error(
        "(defstruct Keep (v: (Vec i64)))
         (defprotocol Holder (hold (self x: (Vec i64) :borrow) -> Keep))
         (impl Holder i64 (hold (self x) (Keep x)))
         (defun main () -> i64 (count (. (hold 1 [2]) v)))",
    );
    assert_eq!(k, OwnErrorKind::ImplEscapes);
    assert_eq!(
        m,
        "implementation of Holder/hold for i64 makes parameter x escape; the protocol declares it :borrow"
    );
}

/// §6.1/§6.4: a whole-scrutinee pattern variable is `Borrowed(p)`, so
/// returning it makes `p` escape and owned (proposed case 22).
#[test]
fn a_whole_scrutinee_pattern_variable_is_the_parameter() {
    let c = ok("(defun same (p) (match p (w w)))
                (defun main () -> i64 (str-len (same (str-concat \"a\" \"b\"))))");
    let p = param(&c, "same", "p");
    assert!(p.escapes);
    assert_eq!(
        p.kind,
        ParamKind::Owned(vec![OwnedWhy::Rule1("returned".into())])
    );
}

/// §6.4 loop variables: a parameter returned through a loop variable
/// escapes and is owned (proposed case 75).
#[test]
fn a_parameter_returned_through_a_loop_variable_is_owned() {
    let c = ok("(defun last-or (default xs)
                  (loop ((best default) (i 0))
                    (if (< i (count xs)) (recur (nth xs i) (+ i 1)) best)))
                (defun main () -> i64 (str-len (last-or \"ab\" [\"x\"])))");
    let d = param(&c, "last-or", "default");
    assert!(d.escapes);
    assert!(matches!(&d.kind, ParamKind::Owned(ws) if matches!(ws[0], OwnedWhy::Loop(_))));
}

/// §6.10: a `recur` retains a binding from outside the loop and moves
/// one it exits (proposed case 63).
#[test]
fn recur_retains_outside_bindings_and_moves_exited_ones() {
    let c = ok("(defun main () -> i64
                  (let ((y (Box (str-concat \"c\" \"d\"))))
                    (loop ((b (Box (str-concat \"a\" \"b\"))) (i 0))
                      (if (< i 2)
                          (let ((z (Box (str-concat \"e\" \"f\")))) (if (< i 1) (recur y (+ i 1)) (recur z (+ i 1))))
                          (str-len (unbox b))))))");
    let (b, _) = fun(&c, "main");
    let mut recurs: Vec<_> = b.recurs.values().map(|r| r.args[0]).collect();
    recurs.sort_by_key(|p| format!("{p:?}"));
    assert_eq!(recurs, vec![Pass::Move, Pass::Retain]);
}

/// §6.4, §6.10 rule (e): a capture of a heap closure is frame-owned, so
/// a call that borrows it at the end of the closure's body is ordinary
/// (proposed case 57).
#[test]
fn a_closure_body_call_borrowing_a_capture_is_ordinary() {
    let c = ok("(defun mk () (let ((v [1 2 3])) (fn () (count v))))
                (defun main () -> i64 (+ ((mk)) 0))");
    let (_, count) = call(&c, "mk", "count");
    assert_eq!(count.tail, Tail::Ordinary(Because::FrameOwned { arg: 0 }));
}

/// §6.10, tail sites before kinds (proposed case 74): the literal is
/// heap as an argument of a tail site, its capture makes `p` owned, and
/// the call is then ordinary through `p` at a borrowed position.
#[test]
fn tail_sites_are_fixed_before_the_kinds() {
    let c = ok(
        "(defun hold (k :borrow s n) (if (= n 0) (+ (k) (str-len s)) (hold (fn () 0) s (- n 1))))
                (defun f (p) (hold (fn () (str-len p)) p 3))
                (defun main () -> i64 (f (str-concat \"a\" \"b\")))",
    );
    let cl = &closures(&c, "f")[0];
    assert_eq!(
        (cl.escaping.clone(), cl.heap.as_deref()),
        (None, Some("arg-of-tail-site"))
    );
    let p = param(&c, "f", "p");
    assert_eq!(
        p.kind,
        ParamKind::Owned(vec![OwnedWhy::Rule1("captured by a heap closure".into())])
    );
    assert_eq!(
        call(&c, "f", "hold").1.tail,
        Tail::Ordinary(Because::FrameOwned { arg: 1 })
    );
}

/// §8.4: a function whose value is taken has an all-owned body, whose
/// parameters are all owned and whose borrowed call is then ordinary
/// (proposed case 52).
#[test]
fn a_function_used_as_a_value_has_an_all_owned_body() {
    let c = ok("(defstruct K (f: (fn ((Vec i64) i64 K) i64)))
                (defun hop (s: (Vec i64) n: i64 k: K) -> i64 (if (= n 0) (count s) ((. k f) (conj [] 1) (- n 1) k)))
                (defun main () -> i64 (hop (conj [] 1) 3 (K hop)))");
    let f = c.typed.fun("hop").expect("hop");
    assert!(c.owned.value_taken.contains(&f));
    let owned = &c.owned.bodies[&BodyKey::AllOwned(f)];
    assert!(owned
        .params
        .iter()
        .all(|p| matches!(p.kind, ParamKind::Owned(_) | ParamKind::Scalar)));
    assert_eq!(param(&c, "hop", "s").kind, ParamKind::Borrowed);
    assert_eq!(call(&c, "hop", "count").1.tail, Tail::TailCall);
}

/// §6.11: a constructor whose binding reaches only a non-escaping
/// borrowed parameter is on the stack; `weak` keeps another off it.
#[test]
fn scope_local_is_decided_on_the_binding() {
    let c = ok("(defstruct B (v: i64))
                (defun get (b: B) -> i64 (. b v))
                (defun main () -> i64
                  (let ((s (B 1)) (h (B 2)))
                    (do (weak h) (+ (get s) (get h)))))");
    assert!(binding(&c, "main", "s").scope_local);
    assert!(!binding(&c, "main", "h").scope_local);
    let (b, _) = fun(&c, "main");
    let mut allocs: Vec<Alloc> = calls(&c, "main", "B")
        .iter()
        .map(|(e, _)| b.allocs[&e.id])
        .collect();
    allocs.sort_by_key(|a| format!("{a:?}"));
    assert_eq!(allocs, vec![Alloc::Heap, Alloc::Stack]);
    assert!(ops(&c, "main").contains(&"end-stack s (exit)".to_string()));
}

/// §6.5 (c): a `let`-bound closure called directly is a stack closure;
/// its binding is scope-local and ends at the `let`'s exit.
#[test]
fn a_let_bound_closure_called_directly_is_on_the_stack() {
    let c = ok("(defun main () -> i64
                  (let ((s (str-concat \"a\" \"b\")) (f (fn () (str-len s))))
                    (+ (f) 1)))");
    let cl = &closures(&c, "main")[0];
    assert_eq!((cl.escaping.clone(), cl.heap.clone()), (None, None));
    assert_eq!(cl.captures[0].pass, Pass::Alias);
    assert!(binding(&c, "main", "f").scope_local);
    // The body (+ ..) is a tail call; f's scope ends before its jump.
    let (_, plus) = call(&c, "main", "+");
    assert!(jump(&c, "main", plus).contains(&"end-stack f (jump)".to_string()));
}
