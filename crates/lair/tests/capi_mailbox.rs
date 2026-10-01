//! The mailbox of the C interface serving a macro-time module's hooks
//! (spec/compiler.md §9): every request answered, the replies reaching
//! the module's result; many calls, many mailboxes, many threads.
//! `capi_misuse.rs` has the mailbox used out of order.

mod common;

use std::sync::{Arc, Barrier};
use std::thread;

use common::*;
use lair::capi::*;

#[test]
fn a_hook_of_arity_1_is_served_and_its_reply_is_the_result() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        assert!(!c.is_null());
        h.install(c);
        h.start(c, "one", &[20]);
        let (seen, result) = serve(c, |a| a * 3 + 1, |_, _| unreachable!());
        assert_eq!(seen, vec![vec![20]]);
        assert_eq!(result, 61 + 1_000_000);
        // The worker runs on a 64 MiB stack of its own, not the test's.
        assert_eq!(
            lair_call_wait(c),
            0,
            "waiting on a finished call is 0 again"
        );
        assert_eq!(lair_call_result(c), 1_000_061);
        assert_eq!(fault(c), None);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn a_hook_of_arity_2_is_served_and_replies_may_be_any_word() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(2);
        let c = lair_call_new();
        h.install(c);
        h.start(c, "two", &[5, 6]);
        let (seen, result) = serve(c, |_| unreachable!(), |a, b| a * 100 + b);
        assert_eq!((seen, result), (vec![vec![5, 6]], 506 + 2_000_000));
        // Negative and full-width words survive the trip both ways.
        h.start(c, "two", &[-1, i64::MIN]);
        let (seen, result) = serve(c, |_| unreachable!(), |a, b| a.wrapping_add(b));
        assert_eq!(seen, vec![vec![-1, i64::MIN]]);
        assert_eq!(
            result,
            (-1i64).wrapping_add(i64::MIN).wrapping_add(2_000_000)
        );
        // Both arities in one call, in order, and the same mailbox again.
        h.start(c, "both", &[4]);
        let (seen, result) = serve(c, |a| a * 3 + 1, |a, b| a * 100 + b);
        assert_eq!(seen, vec![vec![4], vec![4, 7]]);
        assert_eq!(result, 13 + 407);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn a_thousand_hook_calls_in_one_call() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        h.install(c);
        h.start(c, "many", &[1000]);
        let (seen, result) = serve(c, |a| 3 * a + 1, |_, _| unreachable!());
        assert_eq!(seen.len(), 1000);
        assert!(
            seen.iter().enumerate().all(|(i, r)| r == &vec![i as i64]),
            "in order"
        );
        assert_eq!(result, 3 * (999 * 1000 / 2) + 1000);
        // Zero calls: the call returns with no request at all.
        h.start(c, "many", &[0]);
        assert_eq!(
            serve(c, |_| unreachable!(), |_, _| unreachable!()),
            (vec![], 0)
        );
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn two_calls_run_at_once_each_with_its_own_mailbox() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let (a, b) = (lair_call_new(), lair_call_new());
        let hook = lair_hook1_address() as i64;
        let ask = h.at("ask1cx");
        // Both workers park in their hooks before either is answered.
        lair_call_start(a, ask, [hook, a as i64, 10].as_ptr(), 3);
        lair_call_start(b, ask, [hook, b as i64, 20].as_ptr(), 3);
        assert_eq!((lair_call_wait(a), lair_call_wait(b)), (1, 1));
        assert_eq!(
            (lair_call_hook_arg(a, 0), lair_call_hook_arg(b, 0)),
            (10, 20)
        );
        lair_call_hook_reply(b, 2000);
        assert_eq!(lair_call_wait(b), 0);
        assert_eq!(lair_call_wait(a), 1, "a is still parked while b finished");
        lair_call_hook_reply(a, 1000);
        assert_eq!(lair_call_wait(a), 0);
        assert_eq!((lair_call_result(a), lair_call_result(b)), (1000, 2000));
        lair_call_free(a);
        lair_call_free(b);
        lair_jit_free(h.jit);
    }
}

#[test]
fn mailboxes_in_two_threads_with_a_session_each_at_once() {
    let _w = watchdog(60);
    let start = Arc::new(Barrier::new(2));
    let threads: Vec<_> = (0..2i64)
        .map(|t| {
            let start = Arc::clone(&start);
            thread::spawn(move || unsafe {
                let h = Hooked::new(t as i32);
                let c = lair_call_new();
                h.install(c);
                start.wait();
                for round in 0..3 {
                    h.start(c, "many", &[500]);
                    let (seen, result) = serve(c, |a| a * (t + 2) + round, |_, _| unreachable!());
                    assert_eq!(seen.len(), 500);
                    assert_eq!(result, (t + 2) * (499 * 500 / 2) + 500 * round);
                }
                lair_call_free(c);
                lair_jit_free(h.jit);
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
}

#[test]
fn a_handle_made_on_one_thread_is_used_from_another() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        h.install(c);
        let (jit, cm) = (h.jit as usize, c as usize);
        let result = thread::spawn(move || {
            let (jit, c) = (jit as *mut LairJit, cm as *mut Call);
            let one = address(jit, "one").unwrap();
            lair_call_start(c, one, [8].as_ptr(), 1);
            serve(c, |a| a + 1, |_, _| unreachable!()).1
        })
        .join()
        .unwrap();
        assert_eq!(result, 9 + 1_000_000);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

/// The worker's stack is 64 MiB (spec/compiler.md §9): a macro may
/// recurse where a 2 MiB test thread, or the 8 MiB of a main thread,
/// would overflow. At `-O0` a frame of `deep` is tens of bytes, so
/// 400 000 levels need well over 8 MiB.
#[test]
fn the_worker_has_a_stack_for_deep_recursion_even_parked_at_the_bottom() {
    let _w = watchdog(60);
    const LEVELS: i64 = 400_000;
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        h.install(c);
        h.start(c, "deep", &[LEVELS]);
        assert_eq!(
            serve(c, |_| unreachable!(), |_, _| unreachable!()),
            (vec![], LEVELS)
        );
        h.start(c, "deep_ask", &[LEVELS]);
        let (seen, result) = serve(c, |a| 3 * a + 1, |_, _| unreachable!());
        assert_eq!((seen, result), (vec![vec![7]], 22 + LEVELS));
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}
