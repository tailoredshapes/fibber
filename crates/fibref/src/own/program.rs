//! The output of the ownership checker: [`OwnedProgram`], what the
//! reference interpreter (and later the compiler) executes against
//! (spec/types.md §6, §8.9, §8.10, §9).
//!
//! # Reading an owned program
//!
//! The pass decides every count operation; the evaluator performs them
//! and decides nothing (§6.12: interpreter and compiler free the same
//! objects at the same points because both follow this output). An
//! owned program is a table of **bodies** ([`OwnedProgram::bodies`]),
//! one per `defun` ([`BodyKey::Fun`]), per all-owned body of a function
//! whose value is taken ([`BodyKey::AllOwned`], §8.4), per `impl`
//! method and per all-owned body of one a method value may run
//! ([`BodyKey::MethodOwned`]), per `defmacro` and per `def`
//! initialiser. A `fn` or `async` literal is part of the body it is
//! written in: its records are in
//! that body's tables, keyed by the literal's and its expressions'
//! [`ExprId`]s, so a closure value must remember the body whose tables
//! created it (the all-owned body of `f` has its own tables for the same
//! expressions).
//!
//! **Values and sites.** A [`Site`] names what holds a count or is
//! referred to by an operation: a binding, the value an expression
//! produced last in the current activation ([`Site::Value`], which is
//! also how implicit temporaries are named), a capture of a heap
//! closure or task, a closure's own environment, a `def`.
//!
//! **The evaluation protocol**, which the evaluator follows
//! mechanically. Evaluate expressions in the order of syntax §2. For
//! every expression `e`, after `e` has been evaluated (including
//! everything that [`BodyOwn::exprs`] lists for its sub-expressions),
//! run `exprs[e].after` in order; after a `match` guard `g` that is
//! false, run [`BodyOwn::guard_fail`]`[g]` too, then try the next
//! clause. Those lists hold every retain,
//! release and stack end that the pass decided: a join retain on a
//! branch, the scope exit of a `let` (after its body), of a `match`
//! (after each clause body) and of a `loop` (after its body, on the path
//! that does not `recur`), the release of a discarded `do` step or of a
//! step's temporaries, the E1 consume of a body and the release of its
//! owned parameters (after the body expression). A call has a
//! [`CallOwn`]: each argument is handed over as its [`Pass`] says, right
//! after it is evaluated, except a [`Pass::Acquire`], whose copy-in runs
//! at call entry, once every argument has been evaluated, in parameter
//! order (§6.6); an ordinary call then runs the callee, then
//! the write-backs in [`CallOwn::write_backs`], then `after`; a tail
//! call ([`Tail::TailCall`]) runs [`CallOwn::jump`], discards the frame
//! and enters the callee, whose result is the caller's, and nothing
//! of the caller runs afterwards. A `recur` has a [`RecurOwn`]. A `fn`
//! or `async` literal has a [`ClosureOwn`] saying how each capture is
//! taken and whether the object is on the stack.
//!
//! **What the primitives do by themselves** (not listed, part of their
//! definition in §6.6–§6.8, §8.6): `@c` acquires (+1; the value is
//! `Owned`); `set!`, `reset!` and `swap!` release the old content after
//! storing; a write-back moves the private cell's content into the
//! variable and releases the variable's old content; `(array n e)`
//! stores `e` `n` times, taking `n - 1` counts of its own; a `drop`
//! releases the object's counted children; binding a parameter,
//! a `let` variable or a loop variable takes over the count its value
//! carries, if any, and nothing else. A value that carries a count
//! (`Owned`) and is handed over with [`Pass::Move`] or bound is not
//! released by the caller: the count travels.
//!
//! **Stack objects.** [`BodyOwn::allocs`] gives every allocation site's
//! [`Alloc`]. A `Stack` object is allocated in the scope whose exit
//! lists its [`OpKind::EndStack`] (§6.11: the `let` or `match` exit of
//! its binding, the end of its step, the scope exits of a tail call or
//! a `recur`), receives no retain or release from this table, and a
//! count operation that reaches it through a callee (an owned
//! parameter's release) is a no-op (§8.2: "retain/release are
//! no-ops").
//!
//! **Plain counting.** The interpreter's own semantics is plain
//! counting with the exceptions of §6.12. The operations here are the
//! compiler's (§8.10): alias, derived and borrowed-parameter bindings
//! and stack-closure captures carry no count. An evaluator that adds a
//! retain/release pair around each of those intervals frees the same
//! objects at the same points (the argument of §6.12); one that does
//! not frees them at the same points by construction.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::types::ast::{BindingId, BuiltinId, DefId, ExprId, FunId};
use crate::types::ty::ProtoId;

/// Which body a record belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BodyKey {
    /// A `defun` (or `defmacro`) with its inferred kinds.
    Fun(FunId),
    /// The all-owned body `f.owned` of a function whose value is taken
    /// (§8.4): every object parameter owned; calls to members of its
    /// SCC go to their all-owned bodies.
    AllOwned(FunId),
    /// Method `method` of instance `instance` (indices into
    /// `Globals::instances` and its `methods`).
    Method(usize, usize),
    /// The all-owned body of that method's implementation, used as a
    /// value (§8.4: "a protocol method's implementation used as a value
    /// likewise"): every object parameter owned.
    MethodOwned(usize, usize),
    /// A `def` initialiser: no parameters, no tail calls; its value is
    /// immortalised (§8.2).
    Def(DefId),
}

/// What holds, or is named by, a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Site {
    /// A binding: parameter, `let` or pattern variable, loop variable.
    Bind(BindingId),
    /// The value expression `e` produced last in this activation: an
    /// implicit temporary (a step's, a `match` scrutinee's, a `let`
    /// pattern's initialiser, the operand of `.`) or the operand of an
    /// operation.
    Value(ExprId),
    /// Inside the body of the heap closure or task `lit`: its capture
    /// of `b`, an owning binding of the object, released by its drop.
    Capture(ExprId, BindingId),
    /// Inside the body of `fn` literal `lit`: the closure object, an
    /// owned parameter of the body (§8.4); a named `fn`'s self-name is
    /// an alias of it.
    Env(ExprId),
    /// A `def`: immortal, never counted.
    Global(DefId),
}

/// The mode of an expression (§6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    /// No object: a scalar value, or no value at all (`recur`).
    Scalar,
    /// The value carries a count of its own. `immortal`: a literal, a
    /// `Form` literal or a named function's closure, whose count
    /// operations are no-ops.
    Owned {
        /// Whether the object is immortal.
        immortal: bool,
    },
    /// The value is the value of the site, with no count of its own.
    Borrowed(Site),
    /// The value is a strict part of the site's value.
    Derived(Site),
}

/// A count operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpKind {
    /// `fib.retain`.
    Retain,
    /// `fib.release`.
    Release,
    /// The end of a scope-local object's scope: its drop runs inline,
    /// releasing its counted children; nothing is freed (§6.11).
    EndStack,
}

/// Why an operation is there (for `--explain` and tests).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// E1: the body's value leaves (retain of a borrowed or derived value).
    Return,
    /// E2: stored into an object or cell.
    Store,
    /// The retain of a join (§6.3) on a branch that is not `Owned`.
    Join,
    /// A scope exit (`let`, `match`, `loop`): an owning binding.
    ScopeExit,
    /// The end of a step: one of its temporaries.
    StepEnd,
    /// A `Derived(x)` result leaving `x`'s scope: retained first.
    DerivedExit,
    /// A non-final `do` step's `Owned` value.
    Discard,
    /// An owned parameter (or a closure's `env`) at the body's exit.
    ParamExit,
    /// Consumed into a loop variable by its initialiser.
    LoopInit,
    /// Released by a tail call or `recur` before its jump.
    Jump,
    /// The old value of a loop variable at a `recur`.
    RecurOld,
}

/// One operation of [`ExprOwn::after`], [`CallOwn::jump`] or
/// [`RecurOwn::jump`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Op {
    /// What to do.
    pub kind: OpKind,
    /// To which value.
    pub site: Site,
    /// Why.
    pub why: Why,
}

/// How an argument (or a callee value, or a capture) is handed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// A scalar: no count.
    Scalar,
    /// A borrowed position: nothing; an `Owned` argument is a temporary
    /// of the call step, released in the call's `after`.
    Borrow,
    /// The argument's own count travels into the parameter (an `Owned`
    /// value, or at a tail call an owning binding of the frame, which
    /// the jump then does not release).
    Move,
    /// Retained for the parameter (or the store, or the capture).
    Retain,
    /// `&x` at a call of a `defun`: copy-in acquires `@x` into a new
    /// private cell (a stack object of the call) at call entry, after
    /// every argument has been evaluated; a write-back follows.
    Acquire,
    /// `&v` forwarded at a call in tail position: the callee's parameter
    /// is the same private cell; no copy-in, no write-back (§6.6).
    Forward,
    /// `&x` given to a primitive (`array-set!`), or an `&` parameter
    /// captured by a closure: the variable's own cell, no count.
    OwnCell,
    /// A stack closure's capture: an alias, no count.
    Alias,
    /// The head of a named `fn`'s self tail call: `env` handed on
    /// unchanged and not released by the jump (§8.4).
    KeepEnv,
}

/// Why a call in tail position is an ordinary call (§6.10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Because {
    /// (b): an `&` argument that does not forward.
    AmpArgument,
    /// (e): argument `arg` (0-based) is frame-owned at a borrowed
    /// position of a callee outside the SCC.
    FrameOwned {
        /// The argument's index.
        arg: usize,
    },
    /// (f): in tail position of an `async` body (or a `def`
    /// initialiser, which is not a function body either).
    AsyncBody,
    /// A call to an `extern`.
    Extern,
    /// A constructor, or a primitive whose operand is stored (E2, E4):
    /// a store, not a call (proposed case 59).
    Store,
}

/// Whether a call is a tail call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    /// Not in tail position: an ordinary call.
    NotInTail,
    /// A tail call (§6.10): `jump`, then the frame is discarded.
    TailCall,
    /// In tail position, ordinary for the reason given.
    Ordinary(Because),
}

/// What a call calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callee {
    /// A `defun`, with its inferred kinds.
    Fun(FunId),
    /// The all-owned body of a `defun` (from an all-owned body of the
    /// same SCC).
    AllOwned(FunId),
    /// A protocol method, with its declared kinds.
    Method(ProtoId, usize),
    /// A builtin.
    Builtin(BuiltinId),
    /// A struct or variant constructor.
    Ctor,
    /// An `extern`.
    Extern,
    /// A closure value (the head is evaluated): every position owned.
    Value,
}

/// One call (`Call` expression).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallOwn {
    /// What it calls.
    pub callee: Callee,
    /// Tail call or not, and why.
    pub tail: Tail,
    /// How the head is handed over: [`Pass::Move`]/[`Pass::Retain`] for
    /// a closure value, [`Pass::KeepEnv`] for a self tail call,
    /// [`Pass::Scalar`] for a known callee.
    pub head: Pass,
    /// How each argument is handed over, in order.
    pub args: Vec<Pass>,
    /// The write-backs after an ordinary call: (argument index, the
    /// variable written), in parameter order.
    pub write_backs: Vec<(usize, BindingId)>,
    /// A tail call only: the releases before the jump — the scope exits
    /// of the enclosing scopes, the step's temporaries, then the owned
    /// parameters (and `env`) not moved (§6.10 step 3).
    pub jump: Vec<Op>,
}

/// One `recur`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecurOwn {
    /// How each argument is consumed into its loop variable:
    /// [`Pass::Move`] or [`Pass::Retain`].
    pub args: Vec<Pass>,
    /// The scope exits inside the loop body and the step's temporaries,
    /// then the old values of the loop variables not moved.
    pub jump: Vec<Op>,
}

/// Per expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExprOwn {
    /// Its mode (§6.2), after its own scope exits.
    pub mode: Mode,
    /// Operations to run right after it has been evaluated.
    pub after: Vec<Op>,
}

/// What kind of binding (§6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindKind {
    /// A scalar: nothing counted.
    Scalar,
    /// An owning binding (a `let` whose initialiser is `Owned`, a loop
    /// variable): released at its scope exit unless moved.
    Owns,
    /// An alias of the site: no count.
    AliasOf(Site),
    /// A part of the site's value: no count.
    DerivedOf(Site),
    /// A borrowed parameter: no count.
    BorrowedParam,
    /// An owned parameter: released at the body's exits unless moved.
    OwnedParam,
    /// An `&` parameter: a private cell, never a value.
    AmpParam,
}

/// Per binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindingOwn {
    /// Its kind.
    pub kind: BindKind,
    /// Scope-local (§6.11): its object is on the stack.
    pub scope_local: bool,
}

/// Why a parameter is owned (§6.4).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OwnedWhy {
    /// Rule 1, with what the position was (`returned`, `stored`, ...).
    Rule1(String),
    /// Rule 2: an argument of a tail site at this owned position.
    Rule2(String),
    /// Rule 3: a tail call from this function of the SCC passes a
    /// frame-owned argument.
    Rule3(String),
    /// Rule 1 through a loop variable.
    Loop(String),
    /// Not inferred: a closure parameter, an `:owned` method parameter,
    /// a parameter of an all-owned body.
    Declared,
}

/// A parameter's count kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamKind {
    /// No object.
    Scalar,
    /// An `&` parameter.
    Amp,
    /// Borrowed: no count crosses the call.
    Borrowed,
    /// Owned, with every reason found.
    Owned(Vec<OwnedWhy>),
}

/// Per parameter of a body or closure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamOwn {
    /// Its binding.
    pub binding: BindingId,
    /// Its count kind.
    pub kind: ParamKind,
    /// Its escape summary (§6.4); a declared `:borrow` is `false`.
    pub escapes: bool,
    /// Whether `:borrow` was written.
    pub declared_borrow: bool,
}

/// Why a closure is escaping (the first use of kind (d), §6.5).
pub type Escaping = Option<String>;

/// One capture of a closure or task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptureOwn {
    /// The captured binding (of the enclosing scope).
    pub binding: BindingId,
    /// How: [`Pass::Retain`] into a heap object (E3), [`Pass::Alias`]
    /// for a stack closure, [`Pass::OwnCell`] for an `&` parameter,
    /// [`Pass::Scalar`].
    pub pass: Pass,
}

/// A `fn` or `async` literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureOwn {
    /// Whether it is an `async` (always escaping and on the heap).
    pub is_async: bool,
    /// `Some(reason)` iff escaping (§6.5 use (d)).
    pub escaping: Escaping,
    /// `Some(reason)` iff on the heap: the escaping use, or
    /// `arg-of-tail-call`, `head-of-tail-call`, `arg-of-recur`,
    /// `arg-of-tail-site`, `head-of-tail-site`.
    pub heap: Option<String>,
    /// Its captures, in capture-set order.
    pub captures: Vec<CaptureOwn>,
    /// Its parameters (every object parameter owned).
    pub params: Vec<ParamOwn>,
}

/// Where an allocation site's object lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alloc {
    /// Counted, on the heap.
    Heap,
    /// Scope-local: `STACK` flag, count 0, ended at its scope's exit.
    Stack,
    /// Allocates nothing (`some`/`nil` of a non-`Option` object type,
    /// §8.1). A `some`/`nil` of a heap-enum `Option` (a scalar, `dyn` or
    /// `Option` payload) is `Heap`; one whose payload type is a
    /// quantified variable has no entry, and the evaluator decides by
    /// the payload it gets.
    Nothing,
}

/// Everything the pass decided for one body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BodyOwn {
    /// The parameters (none for a `def`).
    pub params: Vec<ParamOwn>,
    /// Every expression walked, including those of nested literals.
    pub exprs: HashMap<ExprId, ExprOwn>,
    /// Every binding bound in the body or its literals.
    pub bindings: HashMap<BindingId, BindingOwn>,
    /// Every call.
    pub calls: HashMap<ExprId, CallOwn>,
    /// Every `recur`.
    pub recurs: HashMap<ExprId, RecurOwn>,
    /// Every `fn` and `async` literal.
    pub closures: HashMap<ExprId, ClosureOwn>,
    /// Every allocation site: constructor calls, `cell`, `atom`, `fn`,
    /// `async`.
    pub allocs: HashMap<ExprId, Alloc>,
    /// Scope-local implicit temporaries (a step's, a `match`'s, a `let`
    /// pattern's), by the initialising expression.
    pub stack_temps: BTreeSet<ExprId>,
    /// Per guard of a `match` clause: the operations of its false edge,
    /// run when it is false and before the next clause is tried (the
    /// clause's rest vectors released in reverse order, §6.3).
    pub guard_fail: HashMap<ExprId, Vec<Op>>,
}

/// A `defun`'s interface for its callers: per parameter, its count kind
/// and escape summary (§6.4, exported with it, §3.9).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Per parameter: `(owned, escapes)`; `&` and scalar parameters are
    /// `(false, false)`.
    pub params: Vec<(bool, bool)>,
}

/// A checked program's ownership decisions. See the module
/// documentation.
#[derive(Clone, Debug, Default)]
pub struct OwnedProgram {
    /// Every body.
    pub bodies: BTreeMap<BodyKey, BodyOwn>,
    /// The bodies in the order they were decided (the units' order).
    pub order: Vec<BodyKey>,
    /// Every `defun`'s summary.
    pub summaries: HashMap<FunId, Summary>,
    /// The functions whose value is taken (they have an all-owned body).
    pub value_taken: BTreeSet<FunId>,
    /// The method implementations `(instance, method)` that a method
    /// value may run (they have a [`BodyKey::MethodOwned`] body).
    pub methods_taken: BTreeSet<(usize, usize)>,
}

impl OwnedProgram {
    /// The body of the `defun` `f`.
    pub fn fun(&self, f: FunId) -> Option<&BodyOwn> {
        self.bodies.get(&BodyKey::Fun(f))
    }
}
