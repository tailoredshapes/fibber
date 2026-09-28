//! Switching between the program's threads (see [`super::sched`] for
//! the policy, spec/types.md §8.8 for its specification).
//!
//! A spawned thread runs on an OS thread of its own, started from the
//! run's [`std::thread::Scope`] with a pointer to the one interpreter
//! all threads share. Only the thread holding the [`Turn`] touches the
//! interpreter: a thread gives the turn away only in [`Interp::switch`]
//! and at its end, after its last access, and after giving it touches
//! nothing until the turn comes back; the turn's mutex orders the
//! accesses of successive holders. So the interpreter is never used by
//! two threads at once, which is what the `unsafe` below relies on. (A
//! suspended thread keeps a `&mut Interp` on its stack, unused while it
//! waits; the aliasing model of the Rust reference does not bless that
//! pattern, but no code can observe it: every suspension is inside a
//! call that is given that same `&mut`, so nothing is cached across
//! it.) The run's scope joins every OS thread before the interpreter is
//! dropped, and `drain` has finished every thread before that; a run
//! that ends another way (a panic) closes the turn, and a thread
//! waiting for it then unwinds without touching the interpreter.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use crate::heap::ObjId;
use crate::syntax::Pos;

use super::alloc::Placement;
use super::call::Jump;
use super::error::{RunError, R};
use super::interp::{stack_here, Interp, STACK_BUDGET};
use super::object::TaskState;
use super::pipeline::STACK_BYTES;
use super::sched::{Saved, Tid, Turn, Wait, QUANTUM};
use super::value::Val;

/// The interpreter, as handed to a spawned thread's OS thread.
struct Shared<'p>(*mut Interp<'p>);

// SAFETY: the pointer is dereferenced only by the holder of the turn
// (module documentation), so the interpreter is never accessed from two
// OS threads at once, and the turn's mutex orders the accesses.
unsafe impl Send for Shared<'_> {}

impl<'p> Shared<'p> {
    /// The pointer (a method, so that a closure captures the whole
    /// `Shared` and not its non-`Send` field).
    fn get(&self) -> *mut Interp<'p> {
        self.0
    }
}

/// The unwind payload of a thread whose run was closed while it waited.
struct Closed;

/// Closes the turn when the run ends, however it ends.
pub struct CloseOnDrop(pub Arc<Turn>);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl<'p> Interp<'p> {
    /// A scheduling point: when the quantum is used up and another
    /// thread is ready, this thread goes to the back of the queue.
    #[inline]
    pub fn tick(&mut self) -> R<()> {
        if !self.sched.expired() || self.sched.ready.is_empty() {
            return Ok(());
        }
        let me = self.sched.current;
        self.sched.ready.push_back(me);
        self.switch()
    }

    /// Blocks the current thread until `task` is done.
    pub fn wait_task(&mut self, task: ObjId) -> R<()> {
        while self.task_state(task)? != TaskState::Done {
            let me = self.sched.current;
            self.sched.blocked.push((me, Wait::Task(task)));
            self.switch()?;
        }
        Ok(())
    }

    /// Runs the thread the scheduler picks next, the current one having
    /// been queued or blocked, and returns when this one runs again: an
    /// error if the run was stopped meanwhile.
    fn switch(&mut self) -> R<()> {
        let me = self.sched.current;
        let next = self
            .sched
            .pick()
            .ok_or_else(|| RunError::internal("no thread to run"))?;
        if next != me {
            self.save(me);
            let turn = self.hand_over(next);
            if !turn.wait(me) {
                resume_unwind(Box::new(Closed));
            }
            self.restore(me)?;
        }
        match &self.sched.abort {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Gives the turn to `next`, with a fresh quantum. The caller must
    /// not touch the interpreter afterwards until it holds the turn.
    fn hand_over(&mut self, next: Tid) -> Arc<Turn> {
        self.sched.left = QUANTUM;
        self.sched.current = next;
        let turn = Arc::clone(&self.sched.turn);
        turn.give(next);
        turn
    }

    fn save(&mut self, t: Tid) {
        let s = Saved {
            frames: std::mem::take(&mut self.frames),
            stack_base: self.stack_base,
            stack_budget: self.stack_budget,
        };
        self.sched.saved.insert(t, s);
    }

    fn restore(&mut self, t: Tid) -> R<()> {
        let s =
            self.sched.saved.remove(&t).ok_or_else(|| {
                RunError::internal(format!("thread {t} resumed with no saved state"))
            })?;
        self.frames = s.frames;
        self.stack_base = s.stack_base;
        self.stack_budget = s.stack_budget;
        Ok(())
    }

    /// Starts thread `tid` running `f` (consumed) for `task`, whose
    /// thread count it releases at its end. The new thread runs at once;
    /// the spawning one goes to the back of the ready queue.
    pub fn start_thread(&mut self, tid: Tid, f: Val, task: ObjId, pos: Pos) -> R<()> {
        let spawner = self
            .sched
            .spawner
            .ok_or_else(|| RunError::unsupported("spawn in a run that cannot start threads"))?;
        let turn = Arc::clone(&self.sched.turn);
        let shared = Shared(self as *mut Interp<'p>);
        let body = Box::new(move || thread_main(shared, turn, tid, f, task, pos));
        spawner
            .start(STACK_BYTES, body)
            .map_err(|e| RunError::unsupported(format!("cannot start a thread: {e}")))?;
        self.sched.live += 1;
        self.sched.ready.push_front(tid);
        let me = self.sched.current;
        self.sched.ready.push_back(me);
        self.switch()
    }

    /// A spawned thread's life, on its own stack: `f` called, its result
    /// stored in the task, the thread's count on the task released.
    fn run_thread(&mut self, f: &Val, task: ObjId, pos: &Pos) -> R<()> {
        self.stack_base = stack_here();
        self.stack_budget = STACK_BUDGET;
        if let Some(e) = &self.sched.abort {
            return Err(e.clone());
        }
        let target = self.value_target(f, &[])?;
        let v = self.invoke(Jump {
            target,
            args: Vec::new(),
            pos: pos.clone(),
            placement: Placement::Heap,
        })?;
        self.complete(task, v)?;
        self.heap.release(task)?;
        Ok(())
    }

    /// A spawned thread has finished (or unwound): the next one runs.
    fn finish_thread(&mut self) {
        self.frames.clear();
        self.sched.live -= 1;
        if self.sched.live == 0 {
            self.sched.wake(Wait::Others);
        }
        let next = self.sched.pick().unwrap_or(0);
        self.hand_over(next);
    }

    /// `main` returning (§6.8): waits for every spawned thread to
    /// finish. An error stops the run; it is in `sched.abort`.
    pub fn drain(&mut self) {
        while self.sched.live > 0 {
            let me = self.sched.current;
            self.sched.blocked.push((me, Wait::Others));
            let _ = self.switch();
        }
    }
}

/// The body of a spawned thread's OS thread.
fn thread_main(shared: Shared<'_>, turn: Arc<Turn>, tid: Tid, f: Val, task: ObjId, pos: Pos) {
    if !turn.wait(tid) {
        return;
    }
    // SAFETY: this thread holds the turn (module documentation).
    let it = unsafe { &mut *shared.get() };
    match catch_unwind(AssertUnwindSafe(|| it.run_thread(&f, task, &pos))) {
        Ok(Ok(())) => {}
        Ok(Err(e)) => it.sched.stop(e),
        Err(p) if p.is::<Closed>() => return,
        Err(_) => it
            .sched
            .stop(RunError::internal("a thread of the evaluator panicked")),
    }
    it.finish_thread();
}
