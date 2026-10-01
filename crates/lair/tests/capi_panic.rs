//! "No panic crosses the boundary" (`include/lair.h`, spec/compiler.md
//! §9) at real exported entries. Every export goes through `guard` (an
//! error comes back) or `shield` (a fallback value comes back), and
//! `src/capi/error.rs` tests those two with a closure; nothing showed a
//! panic arriving at an entry itself. With the feature `test-panic`,
//! which the dev-dependency of this crate turns on for `cargo test` and
//! no build of the library has, `lair_check_source` panics on one fixed
//! text and `lair_call_i64` and `lair_call_f64` at one fixed address.
//!
//! The entries are `extern "C"` functions, and since Rust 1.81 a panic
//! that leaves one aborts the process, so a panic that was not caught
//! inside the entry ends the test binary, and a test that returns shows
//! that none crossed. The panic hook counts the panics that happened, so
//! a pass is not a call that never panicked.

mod common;

use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;
use std::thread;

use common::*;
use lair::capi::*;

static PANICS: AtomicUsize = AtomicUsize::new(0);

/// Counts the panics from here on and says nothing of the injected ones,
/// which the default hook would write to standard error each; any other
/// panic (a failed assertion) is reported as it always is.
fn count_panics() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let reported = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            PANICS.fetch_add(1, Ordering::SeqCst);
            let said = info.payload().downcast_ref::<&str>().copied();
            if said != Some("injected by the test-panic feature") {
                reported(info);
            }
        }));
    });
}

fn panics() -> usize {
    PANICS.load(Ordering::SeqCst)
}

const SQUARE: &str = "(define (square i64) ((i64 x)) (block entry (ret (mul x x))))";

/// The library is whole after a panic: a source that checks, one that does
/// not, a session that compiles a function and calls it.
unsafe fn the_library_still_works() {
    let (ok, ok_len) = s(SQUARE);
    assert_eq!(take(lair_check_source(ok, ok_len)), None);
    let (bad, bad_len) = s("(define (h i32) () (block entry (ret (i64 1))))");
    let message = take(lair_check_source(bad, bad_len)).expect("an error");
    assert!(message.contains("does not match"), "{message}");
    let j = new_jit(0);
    assert_eq!(add(j, "square", SQUARE), None);
    let f = address(j, "square").expect("the function has an address");
    assert_eq!(call(f, &[12]), 144);
    lair_jit_free(j);
}

#[test]
fn a_panic_in_an_entry_that_reports_errors_is_an_error_that_says_so() {
    let _w = watchdog(120);
    count_panics();
    let before = panics();
    unsafe {
        let (src, len) = s(TEST_PANIC_SOURCE);
        let e = lair_check_source(src, len);
        assert!(!e.is_null(), "a panic is not a success");
        let message = take(e).expect("an error");
        assert!(message.starts_with("internal error: "), "{message}");
        assert!(
            message.contains("injected by the test-panic feature"),
            "{message}"
        );
        the_library_still_works();
    }
    assert!(
        panics() > before,
        "no panic was raised: the test proves nothing"
    );
}

#[test]
fn a_panic_in_an_entry_that_returns_a_value_is_its_fallback() {
    let _w = watchdog(120);
    count_panics();
    let before = panics();
    unsafe {
        assert_eq!(lair_call_i64(TEST_PANIC_ADDRESS, ptr::null(), 0), 0);
        assert_eq!(lair_call_f64(TEST_PANIC_ADDRESS, ptr::null(), 0), 0.0);
        the_library_still_works();
    }
    assert!(
        panics() >= before + 2,
        "no panic was raised: the test proves nothing"
    );
}

/// The same from several threads at once, each panicking and recovering:
/// a panic leaves no lock or flag of the library behind.
#[test]
fn panics_on_several_threads_at_once_leave_the_library_usable() {
    let _w = watchdog(120);
    count_panics();
    let before = panics();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| unsafe {
                for _ in 0..25 {
                    let (src, len) = s(TEST_PANIC_SOURCE);
                    let message = take(lair_check_source(src, len)).expect("an error");
                    assert!(message.starts_with("internal error: "), "{message}");
                    assert_eq!(lair_call_i64(TEST_PANIC_ADDRESS, ptr::null(), 0), 0);
                }
                the_library_still_works();
            })
        })
        .collect();
    for t in threads {
        t.join()
            .expect("the thread ends without a panic that escaped");
    }
    assert!(panics() >= before + 8 * 25 * 2);
}
