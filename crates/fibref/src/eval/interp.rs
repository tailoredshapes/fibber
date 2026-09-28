//! A thread of the interpreter: its activation frames and stack, the
//! shared [`World`] (the audited heap, the object table, ...) reached
//! through it, and the operations of the ownership plan run against
//! them (own/program.rs, "the evaluation protocol").

use std::sync::Arc;

use crate::heap::ObjId;
use crate::own::program::{BodyKey, Op, OpKind, Site};
use crate::types::ast::{BindingId, ExprId};

use super::error::{RunError, R};
use super::fx::FxMap;
use super::plan::Plan;
use super::sched::Turn;
use super::value::Val;
use super::world::World;

/// How much Rust stack evaluation may use below the point where the
/// interpreter was created, before a run stops with a
/// [`RunErrorKind::Depth`](super::RunErrorKind) error instead of
/// overflowing: non-tail recursion nests Rust calls (tail calls and
/// `recur` do not). The evaluator's thread has
/// [`STACK_BYTES`](super::STACK_BYTES); the rest is headroom for the
/// primitives, which do not recurse on data (the heap's cascades are
/// iterative) except over `Form` nesting, which expansion bounds.
pub const STACK_BUDGET: usize = super::pipeline::STACK_BYTES - 128 * 1024 * 1024;

/// The stack budget of a run inside a macro expansion, which runs on
/// whatever thread the expander does (`run_source` gives it the
/// evaluator's; see [`MacroEvaluator`](super::MacroEvaluator)).
pub const MACRO_STACK_BUDGET: usize = 64 * 1024 * 1024;

/// The address of a local of the caller: where the stack is now.
#[inline(never)]
pub fn stack_here() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// One activation: a body run with its plan.
#[derive(Debug)]
pub struct Frame<'p> {
    /// The tables of the body being run (§6: a closure's are those of
    /// the body that created it), an index into
    /// [`World::plans`].
    pub plan: usize,
    /// Which body that is (a closure made here remembers it).
    pub key: BodyKey,
    /// Parameters, `let`/pattern/loop variables, captures, self-name.
    pub locals: FxMap<BindingId, Val>,
    /// The value each expression produced last in this activation
    /// (`Site::Value`), for the expressions a plan operation names.
    pub values: FxMap<ExprId, Val>,
    /// The closure object of a closure body (`Site::Env`).
    pub env: Option<Val>,
    /// Keeps the lifetime of the plans the frame indexes.
    marker: std::marker::PhantomData<&'p ()>,
}

impl Frame<'_> {
    /// A fresh frame running the plan `plan`, the plan of `key`.
    pub fn new(plan: usize, key: BodyKey) -> Self {
        Frame {
            plan,
            key,
            locals: FxMap::default(),
            values: FxMap::default(),
            env: None,
            marker: std::marker::PhantomData,
        }
    }
}

/// A thread of the interpreter: its own evaluation state, and the
/// shared [`World`] while it holds the turn (see `threads` for why
/// this is sound without `unsafe`).
///
/// Every access to the world goes through `Deref`/`DerefMut` (`world`) (so
/// `self.heap` is `self.world.heap`), a borrow of `self` that ends
/// before the next statement that needs `&mut self`; a switch between
/// threads takes `&mut self`, so the borrow checker proves no
/// reference into the world lives across one.
pub struct Interp<'p> {
    /// The shared state; `None` only inside a switch, while another
    /// thread holds the turn.
    pub(super) world: Option<Box<World<'p>>>,
    /// The activations, innermost last.
    pub frames: Vec<Frame<'p>>,
    /// Where this thread's stack began.
    pub stack_base: usize,
    /// How much stack below `stack_base` evaluation may use.
    pub stack_budget: usize,
    /// The turn, through which the world passes between threads.
    pub turn: Arc<Turn<Box<World<'p>>>>,
}

impl<'p> Interp<'p> {
    /// The index of the plan of the body `key`.
    pub fn body(&self, key: BodyKey) -> R<usize> {
        self.plans
            .index
            .get(&key)
            .copied()
            .ok_or_else(|| RunError::gap(format!("no ownership plan for body {key:?}")))
    }

    /// The innermost frame.
    pub fn frame(&self) -> R<&Frame<'p>> {
        self.frames
            .last()
            .ok_or_else(|| RunError::internal("no frame"))
    }

    /// The innermost frame, to change.
    pub fn frame_mut(&mut self) -> R<&mut Frame<'p>> {
        self.frames
            .last_mut()
            .ok_or_else(|| RunError::internal("no frame"))
    }

    /// The plan of the innermost frame.
    pub fn plan(&self) -> R<&Plan<'p>> {
        let i = self.frame()?.plan;
        Ok(&self.plans.list[i])
    }

    /// Binds `b` in the innermost frame.
    pub fn bind(&mut self, b: BindingId, v: Val) -> R<()> {
        self.frame_mut()?.locals.insert(b, v);
        Ok(())
    }

    /// The value of the local `b`.
    pub fn local(&self, b: BindingId) -> R<Val> {
        self.frame()?.locals.get(&b).cloned().ok_or_else(|| {
            let name = &self.p.globals.binding(b).name;
            RunError::internal(format!("unbound local {name} ({b:?})"))
        })
    }

    /// Records the value of `e` if an operation may name it.
    pub fn record(&mut self, e: ExprId, v: &Val) -> R<()> {
        if self.value_sites.contains(&e) {
            self.frame_mut()?.values.insert(e, v.clone());
        }
        Ok(())
    }

    /// The value a site names in the innermost frame.
    pub fn site(&self, s: Site) -> R<Val> {
        let f = self.frame()?;
        let v = match s {
            Site::Bind(b) | Site::Capture(_, b) => f.locals.get(&b).cloned(),
            Site::Value(e) => f.values.get(&e).cloned(),
            Site::Env(_) => f.env.clone(),
            Site::Global(d) => self.defs.get(d.0 as usize).cloned().flatten(),
        };
        v.ok_or_else(|| RunError::gap(format!("the plan names {s:?}, which has no value here")))
    }

    /// Runs `exprs[e].after`.
    pub fn after(&mut self, e: ExprId) -> R<()> {
        if let Some(ops) = self.plan()?.after.get(&e).copied() {
            self.run_ops(ops)?;
        }
        Ok(())
    }

    /// Runs plan operations in order.
    pub fn run_ops(&mut self, ops: &[Op]) -> R<()> {
        for op in ops {
            let v = self.site(op.site)?;
            match op.kind {
                OpKind::Retain => self.retain(&v)?,
                OpKind::Release => self.release(&v)?,
                OpKind::EndStack => self.end_stack(&v)?,
            }
        }
        Ok(())
    }

    /// `fib.retain` on the value's object, if it carries a count.
    pub fn retain(&mut self, v: &Val) -> R<()> {
        if let Some(id) = v.obj() {
            self.heap.retain(id)?;
        }
        Ok(())
    }

    /// `fib.release` on the value's object, if it carries a count.
    pub fn release(&mut self, v: &Val) -> R<()> {
        if let Some(id) = v.obj() {
            self.heap.release(id)?;
        }
        Ok(())
    }

    /// Ends the scope of the stack object `v` (§6.11): its drop runs.
    /// Stack lifetimes need not nest (types §8.2: each site has its own
    /// slot in the entry block and its end is an inline drop), so a
    /// temporary of a step ends at the step's end even while an object
    /// made after it, bound by a `let`, lives on.
    pub fn end_stack(&mut self, v: &Val) -> R<()> {
        let id = v.expect_obj("end of a stack object")?;
        let scope = self.stack_scopes.remove(&id).ok_or_else(|| {
            RunError::gap(format!(
                "the plan ends {id} as a stack object; it is not one"
            ))
        })?;
        self.heap.end_scope(scope)?;
        Ok(())
    }

    /// Ends the scope of a private cell at its write-back (§6.6).
    pub fn end_private(&mut self, id: ObjId) -> R<()> {
        let scope = self
            .stack_scopes
            .remove(&id)
            .ok_or_else(|| RunError::internal(format!("private cell {id} has no scope")))?;
        self.heap.end_scope(scope)?;
        Ok(())
    }

    /// Fails with the depth error once evaluation has used more than
    /// the stack budget: called on entry to every expression.
    pub fn enter(&self) -> R<()> {
        if stack_here().abs_diff(self.stack_base) > self.stack_budget {
            return Err(RunError::depth(self.stack_budget));
        }
        Ok(())
    }
}
