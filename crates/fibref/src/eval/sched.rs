//! The executor's scheduler (spec/types.md §8.8, "The reference
//! interpreter's schedule"): which thread runs, which wait, and the
//! turn that hands the interpreter from one OS thread to the next.
//!
//! Every thread a program spawns runs on an OS thread of its own (it
//! needs a stack of its own to be suspended in the middle of an
//! evaluation), but only one of them runs at a time: the one holding
//! the [`Turn`]. The holder decides who runs next, by a fixed policy,
//! at the scheduling points listed in [`QUANTUM`]'s documentation; so a
//! run is one interleaving, the same on every run, and fair (a thread
//! that spins on an atom lets the others run). The data here is the
//! scheduler's state; the switching itself is in `threads`.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use crate::heap::ObjId;

use super::error::RunError;
use super::fx::FxMap;
use super::interp::Frame;

/// A thread of the program: 0 is the one that runs `main` (or the
/// `def`s, or a macro); spawned threads are numbered from 1 in the
/// order they are spawned.
pub type Tid = usize;

/// How many scheduling points a thread passes before it goes to the
/// back of the ready queue, if another thread is ready. The scheduling
/// points are: an atom read (`@a`, and the upgrade of a weak
/// reference), each attempt of a `swap!`, a `reset!`, `spawn`, `join`,
/// `await` and `block-on`, the back edge of a `loop` (`recur`) and a
/// tail call: so between two of them a thread runs only a bounded
/// computation.
/// Large enough that a thread whose work is short runs it in one turn
/// (the schedule of a program that does not spin is what a
/// run-to-completion executor gives), small enough that a spin-wait
/// costs little.
pub const QUANTUM: u32 = 1000;

/// How many times [`Turn::wait`] polls before it sleeps.
const POLLS: u32 = 200;

/// What a blocked thread waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The task to be done.
    Task(ObjId),
    /// Every other thread to finish (`main` returning, §6.8).
    Others,
}

/// Starts OS threads that may borrow for `'p`: a [`std::thread::Scope`].
pub trait Spawn<'p>: Sync {
    /// Runs `body` on a new OS thread with `stack` bytes of stack.
    fn start(&'p self, stack: usize, body: Box<dyn FnOnce() + Send + 'p>) -> std::io::Result<()>;
}

impl<'s> Spawn<'s> for std::thread::Scope<'s, '_> {
    fn start(&'s self, stack: usize, body: Box<dyn FnOnce() + Send + 's>) -> std::io::Result<()> {
        std::thread::Builder::new()
            .name("fibref-thread".into())
            .stack_size(stack)
            .spawn_scoped(self, body)
            .map(|_| ())
    }
}

/// Whose turn it is to run, shared by the OS threads of one run. A
/// thread touches the interpreter only while it holds the turn; the
/// mutex's release and acquire order everything one holder did before
/// everything the next does.
#[derive(Debug, Default)]
pub struct Turn {
    /// The holder, and whether the run is over (no turn will come).
    state: Mutex<(Tid, bool)>,
    changed: Condvar,
}

impl Turn {
    /// Gives the turn to `t`.
    pub fn give(&self, t: Tid) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.0 = t;
        self.changed.notify_all();
    }

    /// Waits until `t` holds the turn: `true`, or `false` if the run
    /// was closed first (the interpreter is gone; touch nothing). It
    /// polls a while, yielding the processor, before it sleeps: a turn
    /// usually comes back soon (a short spawned thread, a spin-wait's
    /// partner), and waking a sleeping thread costs far more.
    pub fn wait(&self, t: Tid) -> bool {
        for _ in 0..POLLS {
            let s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if s.0 == t || s.1 {
                return s.0 == t && !s.1;
            }
            drop(s);
            std::thread::yield_now();
        }
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        while s.0 != t && !s.1 {
            s = self.changed.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        s.0 == t && !s.1
    }

    /// Ends the run: every thread still waiting gives up.
    pub fn close(&self) {
        let mut s = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        s.1 = true;
        self.changed.notify_all();
    }
}

/// What a suspended thread had in the interpreter.
#[derive(Debug)]
pub struct Saved<'p> {
    /// Its activations.
    pub frames: Vec<Frame<'p>>,
    /// Where its stack began.
    pub stack_base: usize,
    /// How much stack it may use.
    pub stack_budget: usize,
}

/// The scheduler's state.
pub struct Sched<'p> {
    /// The turn.
    pub turn: Arc<Turn>,
    /// Where new OS threads come from; `None` runs no threads.
    pub spawner: Option<&'p dyn Spawn<'p>>,
    /// The thread holding the turn.
    pub current: Tid,
    /// The last thread number handed out.
    last: Tid,
    /// Threads ready to run, in the order they will.
    pub ready: VecDeque<Tid>,
    /// Blocked threads, in the order they blocked.
    pub blocked: Vec<(Tid, Wait)>,
    /// The state of every suspended thread.
    pub saved: FxMap<Tid, Saved<'p>>,
    /// Spawned threads not yet finished.
    pub live: usize,
    /// Scheduling points left in the current thread's quantum.
    pub left: u32,
    /// Why the run stopped, once a thread has stopped it (a trap or an
    /// error): every other thread then unwinds.
    pub abort: Option<RunError>,
}

impl std::fmt::Debug for Sched<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sched")
            .field("current", &self.current)
            .field("ready", &self.ready)
            .field("blocked", &self.blocked)
            .field("live", &self.live)
            .finish()
    }
}

impl Default for Sched<'_> {
    fn default() -> Self {
        Sched {
            turn: Arc::default(),
            spawner: None,
            current: 0,
            last: 0,
            ready: VecDeque::new(),
            blocked: Vec::new(),
            saved: FxMap::default(),
            live: 0,
            left: QUANTUM,
            abort: None,
        }
    }
}

impl Sched<'_> {
    /// A new thread's number.
    pub fn fresh(&mut self) -> Tid {
        self.last += 1;
        self.last
    }

    /// Uses one scheduling point: whether the quantum is used up (it
    /// starts again).
    #[inline]
    pub fn expired(&mut self) -> bool {
        self.left -= 1;
        if self.left > 0 {
            return false;
        }
        self.left = QUANTUM;
        true
    }

    /// Makes every thread blocked on `w` ready, in the order they blocked.
    pub fn wake(&mut self, w: Wait) {
        let mut i = 0;
        while i < self.blocked.len() {
            if self.blocked[i].1 == w {
                let (t, _) = self.blocked.remove(i);
                self.ready.push_back(t);
            } else {
                i += 1;
            }
        }
    }

    /// Records that the run stops with `e`, unless it already stopped.
    pub fn stop(&mut self, e: RunError) {
        if self.abort.is_none() {
            self.abort = Some(e);
        }
    }

    /// The thread to run next: the front of the ready queue. If none is
    /// ready, every thread is blocked on another, a deadlock: the run
    /// stops, and every blocked thread is made ready to unwind.
    pub fn pick(&mut self) -> Option<Tid> {
        if let Some(t) = self.ready.pop_front() {
            return Some(t);
        }
        if self.blocked.is_empty() {
            return None;
        }
        self.stop(RunError::unsupported(
            "deadlock: every thread waits for a task that no running thread will complete",
        ));
        let blocked = std::mem::take(&mut self.blocked);
        self.ready.extend(blocked.into_iter().map(|(t, _)| t));
        self.ready.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::{Heap, Kind};

    fn ids() -> (ObjId, ObjId) {
        let mut h = Heap::new();
        let mut obj = || h.alloc(Kind::Immutable, vec![]).expect("alloc");
        (obj(), obj())
    }

    #[test]
    fn the_quantum_expires_every_quantum_points() {
        let mut s = Sched::default();
        let hits = (0..3 * QUANTUM).filter(|_| s.expired()).count();
        assert_eq!(hits, 3);
    }

    #[test]
    fn wake_readies_only_the_waiters_of_that_task_in_order() {
        let (a, b) = ids();
        let mut s = Sched {
            blocked: vec![(3, Wait::Task(a)), (1, Wait::Task(b)), (2, Wait::Task(a))],
            ..Sched::default()
        };
        s.wake(Wait::Task(a));
        assert_eq!(s.ready, VecDeque::from([3, 2]));
        assert_eq!(s.blocked, vec![(1, Wait::Task(b))]);
    }

    #[test]
    fn pick_with_nobody_ready_is_a_deadlock_that_readies_everyone() {
        let mut s = Sched {
            blocked: vec![(0, Wait::Others), (1, Wait::Task(ids().0))],
            ..Sched::default()
        };
        assert_eq!(s.pick(), Some(0));
        assert_eq!(s.ready, VecDeque::from([1]));
        let e = s.abort.as_ref().map(|e| e.message.clone());
        assert!(e.is_some_and(|m| m.starts_with("deadlock")));
        s.ready.clear();
        assert_eq!(s.pick(), None);
    }

    #[test]
    fn the_first_stop_wins() {
        let mut s = Sched::default();
        s.stop(RunError::trap("first"));
        s.stop(RunError::trap("second"));
        assert_eq!(s.abort.map(|e| e.message), Some("first".into()));
    }

    #[test]
    fn a_closed_turn_releases_its_waiters() {
        let turn = Turn::default();
        turn.give(1);
        assert!(turn.wait(1));
        turn.close();
        assert!(!turn.wait(2));
        assert!(!turn.wait(1));
    }
}
