//! Switching between the program's threads (see [`super::sched`] for
//! the policy, spec/types.md §8.8 for its specification).
//!
//! A spawned thread runs on an OS thread of its own, a [`worker`]
//! started from the run's [`std::thread::Scope`]; when the thread
//! finishes, the worker waits, idle, to run a later one. Each OS thread
//! has an [`Interp`] of its own: its frames and its stack, which no
//! other thread sees. What the threads share (the heap, the objects,
//! the atoms and tasks, the `def`s, the scheduler) is the
//! [`World`](super::world::World), and exactly one thread owns it: the
//! one holding the [`Turn`]. At a switch ([`Interp::switch`]) and at
//! its end, a thread moves its world out of its `Interp` and into the
//! turn, by value, and the next holder moves it into its own; a thread
//! without the world blocks until the turn hands it one.
//!
//! This is safe Rust, and the compiler checks the argument, not a
//! comment: the world is a `Box` owned by one `Interp` at a time,
//! moved between OS threads through the turn's mutex, which needs only
//! that it is `Send` (it is, by its fields); every access to it
//! borrows the `Interp` it is in, and a switch takes that `Interp` by
//! `&mut`, so no reference into the world can live across a switch.
//! (The crate forbids `unsafe` code.)
//!
//! When the run ends, `drain` has finished every thread, so every
//! worker is idle, waiting for a turn, and the world is back with the
//! thread that runs `main`; the run then closes the turn and each
//! worker returns. A run that ends another way (a panic) closes the
//! turn too, and a thread suspended in the middle of its work, which
//! holds no world, unwinds.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::Arc;

use crate::heap::ObjId;

use super::alloc::Placement;
use super::call::Jump;
use super::error::{RunError, R};
use super::interp::{stack_here, Interp, STACK_BUDGET};
use super::object::TaskState;
use super::pipeline::STACK_BYTES;
use super::sched::{Job, Tid, Turn, Wait, QUANTUM};
use super::world::World;

/// The unwind payload of a thread whose run was closed while it waited.
struct Closed;

/// Closes the turn when the run ends, however it ends.
pub struct CloseOnDrop<W>(pub Arc<Turn<W>>);

impl<W> Drop for CloseOnDrop<W> {
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
            self.hand_over(next);
            match self.turn.wait(me) {
                Some(w) => self.put_world(w),
                None => resume_unwind(Box::new(Closed)),
            }
        }
        match &self.sched.abort {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }

    /// Gives the turn to `next`, with a fresh quantum, and the world
    /// with it: this thread holds none until it is handed one back.
    fn hand_over(&mut self, next: Tid) {
        self.sched.left = QUANTUM;
        self.sched.current = next;
        if let Some(w) = self.take_world() {
            self.turn.give(next, w);
        }
    }

    /// Starts thread `tid` running `job`, on the idle OS thread waiting
    /// under `tid` if `pooled`, else on a new one. The new thread runs
    /// at once; the spawning one goes to the back of the ready queue.
    pub fn start_thread(&mut self, tid: Tid, pooled: bool, job: Job) -> R<()> {
        if !pooled {
            self.start_worker(tid)?;
        }
        self.sched.jobs.insert(tid, job);
        self.sched.live += 1;
        self.sched.ready.push_front(tid);
        let me = self.sched.current;
        self.sched.ready.push_back(me);
        self.switch()
    }

    /// A new OS thread, which waits for the turn of thread `tid`.
    fn start_worker(&mut self, tid: Tid) -> R<()> {
        let spawner = self
            .sched
            .spawner
            .ok_or_else(|| RunError::unsupported("spawn in a run that cannot start threads"))?;
        let turn = Arc::clone(&self.turn);
        spawner
            .start(STACK_BYTES, Box::new(move || worker(turn, tid)))
            .map_err(|e| RunError::unsupported(format!("cannot start a thread: {e}")))
    }

    /// Runs thread `tid`'s job on the calling OS thread, which then
    /// waits idle under the number it returns; `None` if the run was
    /// closed under it (touch nothing more).
    fn run_job(&mut self, tid: Tid) -> Option<Tid> {
        let run = match self.sched.jobs.remove(&tid) {
            Some(job) => catch_unwind(AssertUnwindSafe(|| self.run_thread(&job))),
            None => Ok(Err(RunError::internal(format!("thread {tid} has no job")))),
        };
        match run {
            Ok(Ok(())) => {}
            Ok(Err(e)) => self.sched.stop(e),
            Err(p) if p.is::<Closed>() => return None,
            Err(_) => self
                .sched
                .stop(RunError::internal("a thread of the evaluator panicked")),
        }
        Some(self.finish_thread())
    }

    /// A spawned thread's life, on its own stack: `f` called, its result
    /// stored in the task, the thread's count on the task released.
    fn run_thread(&mut self, job: &Job) -> R<()> {
        self.stack_base = stack_here();
        self.stack_budget = STACK_BUDGET;
        if let Some(e) = &self.sched.abort {
            return Err(e.clone());
        }
        let target = self.value_target(&job.f, &[])?;
        let v = self.invoke(Jump {
            target,
            args: Vec::new(),
            pos: job.pos.clone(),
            placement: Placement::Heap,
        })?;
        self.complete(job.task, v)?;
        self.heap.release(job.task)?;
        Ok(())
    }

    /// A spawned thread has finished (or unwound): its OS thread goes
    /// idle under a new number, which is returned, and the next thread
    /// runs.
    fn finish_thread(&mut self) -> Tid {
        self.frames.clear();
        self.sched.live -= 1;
        if self.sched.live == 0 {
            self.sched.wake(Wait::Others);
        }
        let ticket = self.sched.fresh();
        self.sched.idle.push(ticket);
        let next = self.sched.pick().unwrap_or(0);
        self.hand_over(next);
        ticket
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

/// The body of an OS thread that runs spawned threads: the one it was
/// started for, then, idle, whichever the scheduler hands it, until the
/// run is closed. The thread has an [`Interp`] of its own, which holds
/// the world only between the turn handing it over and the thread
/// handing it on.
fn worker<'p>(turn: Arc<Turn<Box<World<'p>>>>, mut tid: Tid) {
    let mut it = Interp::thread(turn);
    while let Some(w) = it.turn.wait(tid) {
        it.put_world(w);
        match it.run_job(tid) {
            Some(next) => tid = next,
            None => return,
        }
    }
}
