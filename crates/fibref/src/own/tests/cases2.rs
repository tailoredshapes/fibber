//! Cases 11–20 of `cases/ownership`: the rejections of 12, 13, 14 and 18
//! with their texts, and the decisions types §7 states for the others,
//! with the traces of §6.6 (case 17) and §6.7 (cases 15, 19, 20).

use super::super::error::OwnErrorKind;
use super::super::program::{Because, Pass, Tail};
use super::*;
use crate::types::ErrorKind;

fn has(ops: &[String], op: &str) -> bool {
    ops.iter().any(|o| o == op)
}

fn own_errors(name: &str) -> Vec<(OwnErrorKind, String)> {
    match rejected(&case_source(name)) {
        CheckError::Own(es) => es.into_iter().map(|e| (e.kind, e.message)).collect(),
        other => panic!("case {name}: expected an ownership error, got {other}"),
    }
}

/// `async` is E3 for `s` (retained at creation); the task owns it; `s`'s
/// `let` releases; the call in the body's tail position is ordinary (f).
#[test]
fn case_11_task_retains_its_capture() {
    let c = case("11");
    let task = &closures(&c, "measure")[0];
    assert!(task.is_async);
    assert_eq!(task.captures[0].pass, Pass::Retain);
    assert!(param(&c, "measure", "s").escapes);
    assert_eq!(call(&c, "main", "measure").1.args, vec![Pass::Retain]);
    assert!(has(&ops(&c, "main"), "release s (exit)"));
    assert_eq!(
        call(&c, "measure", "length").1.tail,
        Tail::Ordinary(Because::AsyncBody)
    );
}

#[test]
fn case_12_is_rejected_before_typing_with_the_canonical_text() {
    assert_eq!(
        own_errors("12"),
        vec![(
            OwnErrorKind::AmpTwice,
            "variable x passed to more than one & parameter in call to bar".to_string()
        )]
    );
}

#[test]
fn case_13_still_fails_in_typing() {
    match rejected(&case_source("13")) {
        CheckError::Type(es) => {
            assert_eq!(es[0].kind, ErrorKind::CellNotSend);
            assert_eq!(
                es[0].message,
                "cell cannot be shared between threads: closure capture n has type (Cell i64)"
            );
        }
        other => panic!("expected a type error, got {other}"),
    }
}

/// The async-function check runs before typing, so case 14 reports its
/// own text and not the `Send` error typing would give.
#[test]
fn case_14_is_rejected_as_an_async_function() {
    assert_eq!(
        own_errors("14"),
        vec![(
            OwnErrorKind::AmpInAsync,
            "& parameter in async function: buf in fill".to_string()
        )]
    );
}

/// `[k]` stores `k` inside `conj` (the library's E2); the `set!` stores
/// the fresh vector (moved: no retain); the `let` releases one count.
#[test]
fn case_15_let_releases_its_count_of_k() {
    let c = case("15");
    let k = binding(&c, "main", "k");
    assert!(!k.scope_local, "k is passed to conj's escaping x");
    let ops = ops(&c, "main");
    assert!(has(&ops, "release k (exit)"), "{ops:?}");
    assert!(!ops.iter().any(|o| o.ends_with("(store)")), "{ops:?}");
}

/// One `swap!` with a stack closure: the call is ordinary by rule (e)
/// (a fresh closure at a borrowed position), so the literal is no tail
/// site and stays on the stack.
#[test]
fn case_16_swap_closure_stays_on_the_stack() {
    let c = case("16");
    let (_, s) = call(&c, "transfer", "swap!");
    assert_eq!(s.tail, Tail::Ordinary(Because::FrameOwned { arg: 1 }));
    assert!(closures(&c, "transfer")[0].heap.is_none());
    let steps: Vec<String> = ops(&c, "transfer")
        .into_iter()
        .filter(|o| o.ends_with("(step)"))
        .collect();
    assert_eq!(steps, vec!["end-stack fn (step)"]);
}

/// §6.6's trace of case 17: the copy-in acquires, `@v` acquires as a
/// borrowed argument, the write-back follows the call and the temporary
/// is released after it; inside, `append` gets the private cell
/// forwarded at a tail call.
#[test]
fn case_17_acquire_copy_in_and_temporary_released_after_the_call() {
    let c = case("17");
    let (_, pc) = call(&c, "main", "push-count");
    assert_eq!(pc.args, vec![Pass::Acquire, Pass::Borrow]);
    assert_eq!(pc.write_backs.len(), 1);
    let (e, _) = call(&c, "main", "push-count");
    let (b, body) = fun(&c, "main");
    let after: Vec<String> = b.exprs[&e.id]
        .after
        .iter()
        .map(|o| op_text(&c, body, o))
        .collect();
    assert_eq!(after, vec!["release @ (step)"]);
    let (_, app) = call(&c, "push-count", "append");
    assert_eq!(
        (app.tail, app.args.clone()),
        (Tail::TailCall, vec![Pass::Forward, Pass::Scalar])
    );
    assert!(binding(&c, "main", "v").scope_local);
}

#[test]
fn case_18_is_rejected_for_the_escaping_capture() {
    assert_eq!(
        own_errors("18"),
        vec![(
            OwnErrorKind::AmpCaptured,
            "& parameter captured by escaping closure: v in make-pusher".to_string()
        )]
    );
}

/// §6.7: `(weak parent)` is no count operation (and makes `parent`
/// escape); `main`'s `let`s release `b`, `a`, `root` in reverse order.
#[test]
fn case_19_lets_release_in_reverse_order() {
    let c = case("19");
    let parent = param(&c, "add-child", "parent");
    assert!(parent.escapes);
    assert_eq!(call(&c, "add-child", "weak").1.args, vec![Pass::Borrow]);
    assert!(
        !has(&ops(&c, "add-child"), "release c (exit)"),
        "c is moved out"
    );
    let ops = ops(&c, "main");
    let order: Vec<&String> = ops.iter().filter(|o| o.ends_with("(exit)")).collect();
    assert_eq!(
        order,
        [
            "release b (exit)",
            "release a (exit)",
            "release root (exit)"
        ]
    );
}

/// §6.7: the inner `let` releases the only count of `v` before `@w`.
#[test]
fn case_20_v_is_released_before_the_weak_read() {
    let c = case("20");
    let ops = ops(&c, "main");
    let v = ops
        .iter()
        .position(|o| o == "release v (exit)")
        .expect("v released");
    let w = ops
        .iter()
        .position(|o| o == "release @ (step)")
        .expect("@w released");
    assert!(v < w, "{ops:?}");
    assert_eq!(call(&c, "main", "weak").1.args, vec![Pass::Borrow]);
}

/// Every accept case passes the whole front end; the four rejects fail.
#[test]
fn every_case_has_its_verdict() {
    for n in 1..=20 {
        let name = format!("{n:02}");
        let r = check_source(&case_source(&name), "case.fib");
        match n {
            12 | 13 | 14 | 18 => assert!(r.is_err(), "case {name} accepted"),
            _ => assert!(
                r.is_ok(),
                "case {name}: {}",
                r.err().map(|e| e.to_string()).unwrap_or_default()
            ),
        }
    }
}
