//! The mailbox's state machine: a call on a worker thread of its own
//! that parks at each hook call until the compiler's thread answers
//! (spec/compiler.md §9). Pure Rust, tested without C.
//!
//! One mutex guards the state; the worker and the compiler's thread
//! each sleep on a condition variable of their own and never run at
//! once, so each hands the other everything it wrote through the lock.
//!
//! ```text
//!   Idle or Done(v)  --start-->            Running   (a new worker thread)
//!   Running          --hook(a, b)-->       Hook      (the worker parks)
//!   Hook             --reply(v)-->         Replied
//!   Replied          --worker resumes-->   Running
//!   Running          --the call returns--> Done(result)
//! ```
//!
//! Misuse is a *fault*: the call that was out of order does nothing and
//! returns its failure value (it cannot, if it is `void`), and the
//! mailbox records the first fault's text until the next accepted
//! `start` and makes the next `wait` return -1, once, so that the
//! compiler's thread finds out. The call that was running is left as it
//! was and can still be collected by waiting again. Each function says
//! what it does out of order:
//!
//! - `start` while a call is Running, in a Hook or Replied: refused.
//! - `start` with 0 as address, more than 8 arguments, or a null
//!   argument array for `n > 0`: refused.
//! - `wait` with no call ever started: a fault, -1. `wait` on a Done
//!   call returns 0 again; on a Hook, its arity again.
//! - `hook_arg` with no hook waiting, or an index not below its arity:
//!   a fault, 0. `reply` with no hook waiting (nothing pending, or
//!   already answered): a fault, ignored. `result` before the call is
//!   Done: a fault, 0.
//! - A hook called by a thread that is not the call's worker, or when
//!   no call is running: a fault; the hook returns 0 without waiting
//!   (waiting would deadlock the compiler's own thread).
//! - `free` with the call Running, in a Hook or Replied: the mailbox is
//!   detached. The worker keeps its own reference to the shared
//!   state, so nothing it touches is freed; it runs on and its result
//!   is dropped, but it parks forever at its next hook call, as nobody
//!   will answer. A worker already parked stays parked. Nothing is
//!   unsafe: what is leaked is that thread (64 MiB of address space,
//!   little memory) and the few words of state, until the process ends.
//!   Free after the call is Done joins the worker: nothing is left.

use std::ffi::c_int;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle, ThreadId};

use super::call::{invoke, MAX_ARGS};
use super::error::Message;

/// The worker's stack: a macro may recurse deeply.
const WORKER_STACK: usize = 64 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Nothing started (or a start that could not spawn).
    Idle,
    /// The worker is running module code.
    Running,
    /// The worker is parked at a hook call of `arity` 1 or 2, waiting.
    Hook { arity: u8, a: i64, b: i64 },
    /// The answer is in; the worker has not resumed yet.
    Replied,
    /// The call returned this.
    Done(i64),
}

struct Inner {
    state: State,
    /// The answer to a hook, while `Replied`.
    reply: i64,
    worker: Option<JoinHandle<()>>,
    worker_id: Option<ThreadId>,
    /// The first misuse since the last accepted `start`.
    fault: Option<Message>,
    /// A misuse `wait` has not yet reported.
    unreported: bool,
}

/// Sleep on `cv` until woken, giving the lock back meanwhile.
fn sleep<'a>(cv: &Condvar, g: MutexGuard<'a, Inner>) -> MutexGuard<'a, Inner> {
    cv.wait(g).unwrap_or_else(PoisonError::into_inner)
}

/// A mailbox. It lives behind an `Arc`: the handle is one count and the
/// running worker another, so a freed handle cannot leave the worker
/// with a dangling mailbox.
pub(super) struct Mailbox {
    inner: Mutex<Inner>,
    /// The compiler's thread sleeps here: for a hook or the return.
    to_main: Condvar,
    /// The worker sleeps here: for an answer.
    to_worker: Condvar,
}

impl Mailbox {
    pub(super) fn new() -> Mailbox {
        Mailbox {
            inner: Mutex::new(Inner {
                state: State::Idle,
                reply: 0,
                worker: None,
                worker_id: None,
                fault: None,
                unreported: false,
            }),
            to_main: Condvar::new(),
            to_worker: Condvar::new(),
        }
    }

    /// The state. A poisoned lock still holds a coherent state: nothing
    /// panics between two writes to it.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record a misuse (the first text stays), have the next `wait`
    /// report it, and wake the compiler's thread, which may be waiting
    /// on something that will not come.
    fn fault(&self, g: &mut Inner, what: &str) {
        if g.fault.is_none() {
            g.fault = Some(Message::new(what));
        }
        g.unreported = true;
        self.to_main.notify_all();
    }

    /// Run `addr(args..)` on a new worker thread.
    pub(super) fn start(self: &Arc<Self>, addr: usize, args: &[i64]) {
        let old = {
            let mut g = self.lock();
            if !matches!(g.state, State::Idle | State::Done(_)) {
                return self.fault(&mut g, "lair_call_start: a call is still running");
            }
            if addr == 0 {
                return self.fault(&mut g, "lair_call_start: the address is 0");
            }
            if args.len() > MAX_ARGS {
                return self.fault(&mut g, "lair_call_start: more than 8 arguments");
            }
            g.fault = None;
            g.unreported = false;
            g.state = State::Running;
            g.worker_id = None;
            g.worker.take()
        };
        if let Some(h) = old {
            let _ = h.join();
        }
        let mut packed = [0i64; MAX_ARGS];
        packed[..args.len()].copy_from_slice(args);
        let (me, n) = (Arc::clone(self), args.len());
        let spawned = thread::Builder::new()
            .name("lair-call".to_string())
            .stack_size(WORKER_STACK)
            .spawn(move || me.work(addr, &packed[..n]));
        let mut g = self.lock();
        match spawned {
            Ok(h) => g.worker = Some(h),
            Err(e) => {
                g.state = State::Idle;
                self.fault(&mut g, &format!("lair_call_start: no worker thread: {e}"));
            }
        }
    }

    /// The worker's body: the call, then the result.
    fn work(&self, addr: usize, args: &[i64]) {
        self.lock().worker_id = Some(thread::current().id());
        // SAFETY: `start`'s caller promised `addr` is a ccc function of
        // `args.len()` integer parameters that lives as long as the call.
        let r = unsafe { invoke(addr, args) };
        self.lock().state = State::Done(r);
        self.to_main.notify_all();
    }

    /// Block until the call returned (0) or a hook waits (its arity);
    /// -1, once, for a misuse not yet reported.
    pub(super) fn wait(&self) -> c_int {
        let mut g = self.lock();
        loop {
            if g.unreported {
                g.unreported = false;
                return -1;
            }
            match g.state {
                State::Idle => self.fault(&mut g, "lair_call_wait: no call was started"),
                State::Hook { arity, .. } => return c_int::from(arity),
                State::Done(_) => return 0,
                State::Running | State::Replied => g = sleep(&self.to_main, g),
            }
        }
    }

    /// Argument `i` of the waiting hook call; 0 and a fault if none.
    pub(super) fn hook_arg(&self, i: usize) -> i64 {
        let mut g = self.lock();
        match g.state {
            State::Hook { arity, a, b } if i < usize::from(arity) => [a, b][i],
            State::Hook { .. } => {
                self.fault(
                    &mut g,
                    "lair_call_hook_arg: the index is not below the arity",
                );
                0
            }
            _ => {
                self.fault(&mut g, "lair_call_hook_arg: no hook call is waiting");
                0
            }
        }
    }

    /// Answer the waiting hook call and let the worker resume.
    pub(super) fn answer(&self, v: i64) {
        let mut g = self.lock();
        if matches!(g.state, State::Hook { .. }) {
            g.reply = v;
            g.state = State::Replied;
            self.to_worker.notify_all();
        } else {
            self.fault(&mut g, "lair_call_hook_reply: no hook call is waiting");
        }
    }

    /// The returned value of the finished call; 0 and a fault if none.
    pub(super) fn result(&self) -> i64 {
        let mut g = self.lock();
        match g.state {
            State::Done(v) => v,
            _ => {
                self.fault(&mut g, "lair_call_result: no call has finished");
                0
            }
        }
    }

    /// A misuse the C wrapper found before it reached the state
    /// machine (a null argument array): a fault like any other.
    pub(super) fn misuse(&self, what: &str) {
        let mut g = self.lock();
        self.fault(&mut g, what);
    }

    /// The first fault's text and its length without the NUL after it;
    /// a null pointer and 0 if there is none. The text lives until the
    /// next accepted `start` or the free.
    pub(super) fn fault_text(&self) -> (*const u8, usize) {
        match &self.lock().fault {
            Some(m) => (m.with_nul().as_ptr(), m.with_nul().len() - 1),
            None => (std::ptr::null(), 0),
        }
    }

    /// The worker's side of a hook call: park until answered. 0 and a
    /// fault when not called by the worker of a running call.
    pub(super) fn hook(&self, arity: u8, a: i64, b: i64) -> i64 {
        let mut g = self.lock();
        if g.state != State::Running || g.worker_id != Some(thread::current().id()) {
            self.fault(&mut g, "a hook was called outside its call's worker thread");
            return 0;
        }
        g.state = State::Hook { arity, a, b };
        self.to_main.notify_all();
        while g.state != State::Replied {
            g = sleep(&self.to_worker, g);
        }
        g.state = State::Running;
        g.reply
    }

    /// End the handle's reference: see the module comment for a call
    /// that has not finished.
    pub(super) fn release(self: Arc<Self>) {
        let joinable = {
            let mut g = self.lock();
            match g.state {
                State::Idle | State::Done(_) => g.worker.take(),
                _ => None,
            }
        };
        if let Some(h) = joinable {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn plus_one(x: i64) -> i64 {
        x + 1
    }

    /// A call that asks its mailbox (`mb`, an `Arc<Mailbox>`'s raw
    /// pointer) a question, as a macro-time module does through a hook.
    extern "C" fn ask(mb: i64, x: i64) -> i64 {
        // SAFETY: the tests pass the address of a mailbox that outlives the call.
        let m = unsafe { &*(mb as *const Mailbox) };
        m.hook(1, x, 0) + 1
    }

    /// Each level holds 1 MiB of stack: 41 levels need far more than
    /// the 2 MiB of a test thread or the 8 MiB of a main thread.
    extern "C" fn deep(n: i64) -> i64 {
        let mut pad = [0u8; 1 << 20];
        let at = n as usize % pad.len();
        pad[at] = 1;
        let below = if n > 0 { deep(n - 1) } else { 0 };
        below + i64::from(std::hint::black_box(&pad)[at])
    }

    fn addr(f: extern "C" fn(i64) -> i64) -> usize {
        f as usize
    }

    fn ask_addr() -> usize {
        ask as extern "C" fn(i64, i64) -> i64 as usize
    }

    fn raw(m: &Arc<Mailbox>) -> i64 {
        Arc::as_ptr(m) as i64
    }

    #[test]
    fn a_call_without_a_hook_returns_its_value() {
        let m = Arc::new(Mailbox::new());
        m.start(addr(plus_one), &[41]);
        assert_eq!((m.wait(), m.result()), (0, 42));
        assert_eq!(m.wait(), 0, "a done call stays done");
        m.release();
    }

    #[test]
    fn a_hook_parks_the_worker_until_it_is_answered() {
        let m = Arc::new(Mailbox::new());
        m.start(ask_addr(), &[raw(&m), 5]);
        assert_eq!((m.wait(), m.hook_arg(0)), (1, 5));
        assert_eq!(m.wait(), 1, "still parked: no answer yet");
        m.answer(100);
        assert_eq!((m.wait(), m.result()), (0, 101));
        m.release();
    }

    #[test]
    fn the_worker_has_a_64_mib_stack() {
        let m = Arc::new(Mailbox::new());
        m.start(addr(deep), &[40]);
        assert_eq!((m.wait(), m.result()), (0, 41));
        m.release();
    }

    #[test]
    fn a_misuse_is_one_report_and_leaves_the_call_alone() {
        let m = Arc::new(Mailbox::new());
        assert_eq!(m.wait(), -1, "no call was started");
        m.start(ask_addr(), &[raw(&m), 1]);
        assert_eq!(m.fault_text().1, 0, "a start clears the fault");
        assert_eq!(m.wait(), 1);
        m.start(addr(plus_one), &[1]);
        assert_eq!(m.wait(), -1, "the refused start is reported once");
        assert_eq!(m.wait(), 1, "and the parked call is as it was");
        let (p, n) = m.fault_text();
        // SAFETY: the text lives as long as the mailbox, which this test holds.
        let text = unsafe { std::slice::from_raw_parts(p, n) };
        assert_eq!(text, b"lair_call_start: a call is still running");
        m.answer(0);
        assert_eq!((m.wait(), m.result()), (0, 1));
        m.release();
    }

    #[test]
    fn a_hook_called_on_the_wrong_thread_returns_zero_at_once() {
        let m = Arc::new(Mailbox::new());
        assert_eq!(m.hook(1, 7, 0), 0, "no call is running");
        assert_eq!(m.wait(), -1);
    }
}
