//! The mailbox of the C interface used out of order (spec/compiler.md
//! §9, `src/capi/exchange.rs`): each misuse does what the header says,
//! reports it once, and leaves the call that was running as it was.

mod common;

use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use common::*;
use lair::capi::*;

#[test]
fn a_reply_with_nothing_pending_is_a_fault_and_the_mailbox_recovers() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        lair_call_hook_reply(c, 5);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_hook_reply: no hook call is waiting")
        );
        assert_eq!(lair_call_wait(c), -1, "the next wait reports it");
        assert_eq!(
            lair_call_wait(c),
            -1,
            "and nothing was started, which is its own fault"
        );
        // A start that is accepted is a clean slate.
        h.install(c);
        h.start(c, "one", &[1]);
        assert_eq!(fault(c), None);
        assert_eq!(serve(c, |a| a, |_, _| unreachable!()).1, 1_000_001);
        // A reply to a call that is done, and a second reply to one parked.
        lair_call_hook_reply(c, 5);
        assert!(fault(c).is_some());
        assert_eq!(lair_call_wait(c), -1);
        assert_eq!(
            lair_call_wait(c),
            0,
            "once reported, the finished call is still finished"
        );
        lair_call_free(c);
        let c = parked(&h, "one", 3);
        lair_call_hook_reply(c, 4);
        lair_call_hook_reply(c, 5);
        assert_eq!(lair_call_wait(c), -1, "the second reply was a fault");
        assert_eq!(
            lair_call_wait(c),
            0,
            "the call went on with the first reply"
        );
        assert_eq!(lair_call_result(c), 1_000_004);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn start_while_a_call_is_running_is_refused_and_the_call_is_untouched() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = parked(&h, "one", 7);
        h.start(c, "one", &[99]);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_start: a call is still running")
        );
        assert_eq!(lair_call_wait(c), -1, "reported once");
        assert_eq!(
            lair_call_wait(c),
            1,
            "the first call is still parked at its hook"
        );
        assert_eq!(
            lair_call_hook_arg(c, 0),
            7,
            "and still asks its own question"
        );
        lair_call_hook_reply(c, 70);
        assert_eq!(lair_call_wait(c), 0);
        assert_eq!(lair_call_result(c), 1_000_070);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn a_start_that_cannot_run_is_refused_and_a_finished_result_survives_it() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let seven = h.at("seven");
        let c = lair_call_new();
        lair_call_start(c, seven, ptr::null(), 0);
        assert_eq!((lair_call_wait(c), lair_call_result(c)), (0, 7));
        lair_call_start(c, 0, ptr::null(), 0);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_start: the address is 0")
        );
        assert_eq!(lair_call_wait(c), -1);
        assert_eq!(
            lair_call_wait(c),
            0,
            "reported once; the old call is still done"
        );
        assert_eq!(lair_call_result(c), 7, "its result is not lost");
        // The first fault's text stays; every misuse is reported once.
        lair_call_start(c, seven, ptr::null(), 1);
        lair_call_start(c, seven, [0i64; 9].as_ptr(), 9);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_start: the address is 0")
        );
        assert_eq!(lair_call_wait(c), -1);
        // An accepted start is a clean slate.
        lair_call_start(c, seven, ptr::null(), 0);
        assert_eq!(fault(c), None);
        assert_eq!((lair_call_wait(c), lair_call_result(c)), (0, 7));
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn a_start_with_bad_arguments_says_which() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let seven = h.at("seven");
        let c = lair_call_new();
        lair_call_start(c, seven, ptr::null(), 1);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_start: args is null but n is not 0")
        );
        assert_eq!(lair_call_wait(c), -1);
        assert_eq!(lair_call_wait(c), -1, "nothing was ever started");
        lair_call_free(c);
        let c = lair_call_new();
        lair_call_start(c, seven, [0i64; 9].as_ptr(), 9);
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_start: more than 8 arguments")
        );
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn arguments_and_results_out_of_order_are_defined() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let idle = lair_call_new();
        assert_eq!(lair_call_hook_arg(idle, 0), 0);
        assert_eq!(
            fault(idle).as_deref(),
            Some("lair_call_hook_arg: no hook call is waiting")
        );
        assert_eq!(lair_call_result(idle), 0);
        assert_eq!(lair_call_wait(idle), -1);
        lair_call_free(idle);

        let c = parked(&h, "one", 3);
        assert_eq!(
            lair_call_hook_arg(c, 1),
            0,
            "a hook of arity 1 has no argument 1"
        );
        assert_eq!(
            fault(c).as_deref(),
            Some("lair_call_hook_arg: the index is not below the arity")
        );
        assert_eq!(lair_call_result(c), 0, "the call has not finished");
        assert_eq!(lair_call_wait(c), -1);
        assert_eq!(
            lair_call_wait(c),
            1,
            "still parked, with its arguments intact"
        );
        assert_eq!(lair_call_hook_arg(c, 0), 3);
        lair_call_hook_reply(c, 1);
        assert_eq!(
            lair_call_wait(c),
            0,
            "two misuses, one report: it was made before"
        );
        assert_eq!(lair_call_result(c), 1_000_001);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn a_hook_called_off_its_worker_returns_zero_instead_of_waiting() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        h.install(c);
        // Idle: calling a module function that hooks, on this thread.
        assert_eq!(call(h.at("ask1"), &[5]), 0);
        assert_eq!(
            fault(c).as_deref(),
            Some("a hook was called outside its call's worker thread")
        );
        assert_eq!(lair_call_wait(c), -1);
        // While a call is parked, and while it runs, the same from this thread.
        let w = parked(&h, "one", 1);
        assert_eq!(
            call(h.at("ask2"), &[1, 2]),
            0,
            "this thread is not the worker"
        );
        assert_eq!(lair_call_wait(w), -1);
        assert_eq!(lair_call_wait(w), 1, "the worker is still parked");
        lair_call_hook_reply(w, 5);
        assert_eq!(lair_call_wait(w), 0);
        h.start(w, "spin_then_mark", &[]);
        assert_eq!(
            call(h.at("ask1"), &[1]),
            0,
            "running, but not on this thread"
        );
        assert_eq!(
            fault(w).as_deref(),
            Some("a hook was called outside its call's worker thread")
        );
        call(h.at("set_flag"), &[]);
        assert_eq!(lair_call_wait(w), -1);
        assert_eq!(lair_call_wait(w), 0);
        assert_eq!(lair_call_result(w), 99);
        lair_call_free(w);
        // A null context: the hook has nowhere to wait.
        call(
            h.install,
            &[lair_hook1_address() as i64, lair_hook2_address() as i64, 0],
        );
        assert_eq!(call(h.at("ask1"), &[5]), 0);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}

#[test]
fn null_mailboxes_do_nothing_and_say_so() {
    let _w = watchdog(60);
    unsafe {
        lair_call_free(ptr::null_mut());
        lair_call_start(ptr::null_mut(), 1, ptr::null(), 0);
        lair_call_hook_reply(ptr::null_mut(), 1);
        assert_eq!(lair_call_wait(ptr::null_mut()), -1);
        assert_eq!(lair_call_hook_arg(ptr::null_mut(), 0), 0);
        assert_eq!(lair_call_result(ptr::null_mut()), 0);
        let mut len = 0usize;
        let p = lair_call_fault(ptr::null_mut(), &mut len);
        assert_eq!(
            std::slice::from_raw_parts(p.cast::<u8>(), len),
            b"the mailbox is null"
        );
    }
}

#[test]
fn freeing_a_parked_call_detaches_it_and_it_never_resumes() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = parked(&h, "mark_after", 5);
        lair_call_free(c);
        // The worker was waiting for an answer that cannot come now: it
        // does not run on (the marker it would set stays 0), and the
        // library is intact for the next mailbox.
        thread::sleep(Duration::from_millis(200));
        assert_eq!(call(h.at("get_marker"), &[]), 0);
        let c2 = lair_call_new();
        h.install(c2);
        h.start(c2, "one", &[2]);
        assert_eq!(serve(c2, |a| a + 40, |_, _| unreachable!()).1, 1_000_042);
        lair_call_free(c2);
        // The session is not freed: the detached worker's frames are in its
        // code, and a call in progress is a session that must stay.
    }
}

#[test]
fn freeing_a_running_call_lets_it_finish_unobserved() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        let c = lair_call_new();
        h.start(c, "spin_then_mark", &[]);
        thread::sleep(Duration::from_millis(50));
        lair_call_free(c);
        assert_eq!(call(h.at("get_marker"), &[]), 0, "still spinning");
        call(h.at("set_flag"), &[]);
        let deadline = Instant::now() + Duration::from_secs(10);
        while call(h.at("get_marker"), &[]) == 0 {
            assert!(
                Instant::now() < deadline,
                "the detached worker never finished"
            );
            thread::sleep(Duration::from_millis(10));
        }
        // Not freeing the session: the worker set the marker a moment
        // before it returned out of the session's code, and a freed
        // session under a running call is undefined. Nobody can tell
        // when a detached worker is done, which is why free says "finish
        // the call first".
    }
}

#[test]
fn a_finished_mailbox_is_freed_and_made_again_any_number_of_times() {
    let _w = watchdog(60);
    unsafe {
        let h = Hooked::new(0);
        for _ in 0..50 {
            let c = lair_call_new();
            h.install(c);
            h.start(c, "one", &[1]);
            assert_eq!(serve(c, |a| a, |_, _| unreachable!()).1, 1_000_001);
            lair_call_free(c);
        }
        // Never started; and finished with its result never read.
        lair_call_free(lair_call_new());
        let c = lair_call_new();
        h.start(c, "get_marker", &[]);
        assert_eq!(lair_call_wait(c), 0);
        lair_call_free(c);
        lair_jit_free(h.jit);
    }
}
