//! Threads and tasks (syntax §3.12, §3.14; types §6.8, §6.9, §8.8),
//! run by a deterministic, fair executor (types §8.8, "The reference
//! interpreter's schedule"; the switching is in `threads`, the policy
//! in `sched`):
//!
//! - `(spawn f)` starts a thread that runs `f` on a stack of its own;
//!   it runs at once, and the spawning thread goes to the back of the
//!   ready queue;
//! - an `async` task runs when it is first joined or awaited, driven by
//!   whoever does so, from start to finish: an `await` inside it drives
//!   the awaited task the same way, nested;
//! - `join` (and `await`, `block-on`) of a task another thread is
//!   running blocks until it is done; of a task the joining thread is
//!   itself driving, further down its stack, is a deadlock, reported.
//!
//! Each run is one interleaving of the threads the program starts, the
//! same on every run.
//!
//! Counts follow §6.8 and §8.8: a spawned task is created with count 2,
//! the thread's count released after the result is stored; an `async`
//! task retains its captures at creation (share-marked, since any
//! worker may run it), and the result is share-marked before it is
//! stored. The result lives in an atom the task holds (the heap has no
//! object with both fixed and writable fields), freed with the task.

use crate::heap::{Kind, ObjId, Value};
use crate::syntax::Pos;
use crate::types::ast::{Expr, ExprKind};

use super::alloc::Placement;
use super::error::{RunError, R};
use super::expr::Flow;
use super::interp::{Frame, Interp};
use super::object::{AsyncBody, Obj, TaskData, TaskState};
use super::sched::{Job, Wait};
use super::value::Val;

impl<'p> Interp<'p> {
    /// A task object with `fields` (its captures) and a result atom.
    fn new_task(&mut self, mut fields: Vec<Value>, data: TaskData) -> R<ObjId> {
        let result = data.result;
        fields.push(Value::Ref(result));
        let id = self.alloc(
            Kind::Immutable,
            fields,
            Obj::Task(Box::new(data)),
            Placement::Heap,
        )?;
        self.heap.release(result)?;
        Ok(id)
    }

    fn result_atom(&mut self) -> R<ObjId> {
        self.new_slot(Kind::Atom, Val::Unit, Placement::Heap)?
            .expect_obj("a result atom")
    }

    /// `(spawn f)`: `f` arrives consumed; it is share-marked and handed
    /// to a new thread, which holds the task's second count.
    pub fn spawn(&mut self, f: &Val, pos: &Pos) -> R<Val> {
        let fid = f.expect_obj("the thunk of spawn")?;
        self.heap.mark_shared(fid)?;
        let result = self.result_atom()?;
        let (tid, pooled) = self.sched.slot();
        let data = TaskData {
            body: None,
            state: TaskState::Running,
            driver: tid,
            result,
        };
        let task = self.new_task(Vec::new(), data)?;
        self.heap.retain(task)?;
        let job = Job {
            f: f.clone(),
            task,
            pos: pos.clone(),
        };
        self.start_thread(tid, pooled, job)?;
        Ok(Val::Obj(task))
    }

    /// Stores a finished task's result: share-marked, moved into the
    /// result atom; the task is done and its joiners ready.
    pub fn complete(&mut self, task: ObjId, v: Val) -> R<()> {
        if let Some(id) = v.obj() {
            self.heap.mark_shared(id)?;
        }
        let atom = self.task(task)?.result;
        self.write_slot(atom, v.clone())?;
        self.release(&v)?;
        self.task_mut(task)?.state = TaskState::Done;
        self.sched.wake(Wait::Task(task));
        Ok(())
    }

    /// Where the task `id` is in its life.
    pub fn task_state(&self, id: ObjId) -> R<TaskState> {
        Ok(self.task(id)?.state)
    }

    fn task(&self, id: ObjId) -> R<&TaskData> {
        match self.objs.get(&self.heap, id)? {
            Obj::Task(t) => Ok(t),
            o => Err(RunError::internal(format!("not a task: {o:?}"))),
        }
    }

    fn task_mut(&mut self, id: ObjId) -> R<&mut TaskData> {
        match self.objs.get_mut(&self.heap, id)? {
            Obj::Task(t) => Ok(t),
            o => Err(RunError::internal(format!("not a task: {o:?}"))),
        }
    }

    /// `(join t)` and the result of `(await t)`, a scheduling point:
    /// drives `t` if nobody has, or waits for the thread that runs it,
    /// then its result, retained for the caller.
    pub fn join(&mut self, t: &Val) -> R<Val> {
        let id = t.expect_obj("a task")?;
        self.tick()?;
        let (state, driver) = {
            let d = self.task(id)?;
            (d.state, d.driver)
        };
        match state {
            TaskState::Pending => self.drive(id)?,
            TaskState::Running if driver == self.sched.current => {
                return Err(RunError::unsupported(
                    "deadlock: a task waits for a task that its own thread is driving",
                ))
            }
            TaskState::Running => self.wait_task(id)?,
            TaskState::Done => {}
        }
        let atom = self.task(id)?.result;
        let v = self.slot(atom)?;
        self.retain(&v)?;
        Ok(v)
    }

    /// Runs an `async` task's body to completion.
    fn drive(&mut self, id: ObjId) -> R<()> {
        let AsyncBody {
            lit,
            body: key,
            caps,
        } = self
            .task(id)?
            .body
            .clone()
            .ok_or_else(|| RunError::internal("a pending task with no body"))?;
        let me = self.sched.current;
        let t = self.task_mut(id)?;
        t.state = TaskState::Running;
        t.driver = me;
        let e = self.literal_expr(lit)?;
        let ExprKind::Async(body, _) = &e.kind else {
            return Err(RunError::internal("a task whose literal is not async"));
        };
        let mut frame = Frame::new(self.body(key)?, key);
        frame.locals.extend(caps);
        let v = match self.run_frame(frame, body)? {
            Flow::Val(v) => v,
            _ => return Err(RunError::internal("an async body jumped")),
        };
        self.complete(id, v)
    }

    /// An `async` literal (§6.9): a task retaining its captures (E3),
    /// which are share-marked; the body does not run yet.
    pub fn make_async(&mut self, e: &Expr) -> R<Val> {
        let own = self
            .plan()?
            .closures
            .get(&e.id)
            .ok_or_else(|| RunError::gap("no plan for an async literal"))?;
        let (caps, fields) = self.captures(&own.captures)?;
        for (_, v) in &caps {
            if let Some(id) = v.obj() {
                self.heap.mark_shared(id)?;
            }
        }
        let result = self.result_atom()?;
        let key = self.frame()?.key;
        let data = TaskData {
            body: Some(AsyncBody {
                lit: e.id,
                body: key,
                caps,
            }),
            state: TaskState::Pending,
            driver: 0,
            result,
        };
        Ok(Val::Obj(self.new_task(fields, data)?))
    }

    /// `(await x)`: the task is an `Owned` temporary of the step (the
    /// plan releases it); its result is owned.
    pub fn await_task(&mut self, x: &'p Expr) -> R<Val> {
        let t = self.val(x)?;
        self.join(&t)
    }
}
