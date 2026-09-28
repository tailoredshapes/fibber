//! The interpreter's state: the audited heap, the object table, the
//! activation frames, and the operations of the ownership plan run
//! against them (own/program.rs, "the evaluation protocol").

use std::collections::HashMap;

use crate::expand::ExpandCtx;
use crate::heap::{Heap, ObjId, ScopeId};
use crate::own::program::{BodyKey, Op, OpKind, OwnedProgram, Site};
use crate::types::ast::{BindingId, Expr, ExprId};
use crate::types::TypedProgram;

use super::error::{RunError, R};
use super::fx::{FxMap, FxSet};
use super::object::Objects;
use super::plan::{literals, value_sites, Plan, Plans};
use super::raw::RawMemory;
use super::value::Val;

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
fn stack_here() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// One activation: a body run with its plan.
#[derive(Debug)]
pub struct Frame<'p> {
    /// The tables of the body being run (§6: a closure's are those of
    /// the body that created it), an index into [`Interp::plans`].
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

/// Values cached per program: immortal literals and function values.
#[derive(Debug, Default)]
pub struct Statics {
    /// String and `Form` literals by expression.
    pub lits: FxMap<ExprId, ObjId>,
    /// Immortal closures of named functions, builtins, methods, ctors.
    pub closures: HashMap<String, ObjId>,
    /// Interned keywords.
    pub keywords: Vec<String>,
    /// Keyword ids by name.
    pub keyword_ids: HashMap<String, u32>,
}

/// The interpreter over one checked program.
pub struct Interp<'p> {
    /// The typed program.
    pub p: &'p TypedProgram,
    /// Its ownership plan.
    pub o: &'p OwnedProgram,
    /// The audited heap.
    pub heap: Heap,
    /// What each object is.
    pub objs: Objects,
    /// The activations, innermost last.
    pub frames: Vec<Frame<'p>>,
    /// Every body's plan, indexed.
    pub plans: Plans<'p>,
    /// Every `fn` and `async` literal, by id.
    pub lits: FxMap<ExprId, &'p Expr>,
    /// The instance of every method use the checker resolved to one.
    pub instances: FxMap<ExprId, usize>,
    /// The expressions whose values some operation names.
    pub value_sites: FxSet<ExprId>,
    /// The scope of each live stack object.
    pub stack_scopes: FxMap<ObjId, ScopeId>,
    /// Immortal literals and function values.
    pub statics: Statics,
    /// The value of every `def`, once evaluated.
    pub defs: Vec<Option<Val>>,
    /// Where the stack was when the interpreter was made.
    pub stack_base: usize,
    /// How much stack below `stack_base` evaluation may use.
    pub stack_budget: usize,
    /// The `unsafe` byte arena.
    pub raw: RawMemory,
    /// The expansion context, when running a macro (gensym, reflection).
    pub ctx: Option<&'p ExpandCtx>,
    /// Gensyms handed out outside a macro run.
    pub gensyms: u64,
    /// The expansion error a reflection call raised, for the runner.
    pub expand_error: Option<crate::expand::ExpandError>,
    /// The input form behind each `Form` object built from a macro's
    /// arguments, so that the macro's result keeps their positions
    /// (syntax §1.3); filled while `recording_inputs`.
    pub input_forms: FxMap<ObjId, crate::syntax::Form>,
    /// Whether `Form` objects being built are a macro's input.
    pub recording_inputs: bool,
    /// The weak box of each object that has one (§8.7: the first `weak`
    /// of an object allocates its box, later ones find it).
    pub weak_boxes: FxMap<ObjId, ObjId>,
}

impl<'p> Interp<'p> {
    /// An interpreter over `p` and its plan `o`, with an empty heap.
    pub fn new(p: &'p TypedProgram, o: &'p OwnedProgram) -> Self {
        Interp {
            p,
            o,
            heap: Heap::new(),
            objs: Objects::default(),
            frames: Vec::new(),
            plans: Plans::new(o),
            lits: literals(p),
            instances: super::plan::instances(p),
            value_sites: value_sites(o),
            stack_scopes: FxMap::default(),
            statics: Statics::default(),
            defs: vec![None; p.globals.defs.len()],
            stack_base: stack_here(),
            stack_budget: STACK_BUDGET,
            raw: RawMemory::default(),
            ctx: None,
            gensyms: 0,
            expand_error: None,
            input_forms: FxMap::default(),
            recording_inputs: false,
            weak_boxes: FxMap::default(),
        }
    }

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
