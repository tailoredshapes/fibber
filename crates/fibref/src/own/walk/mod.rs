//! One walk over the bodies of a unit (types §6): the mode of every
//! expression (§6.1, §6.2), the positions every value reaches (the
//! events that the fixpoint of §6.4 and the stack rule of §6.11 read),
//! and the operations of §6.3 and §6.10.
//!
//! A walk takes the unit's current facts as input and changes none of
//! them; the driver ([`super::unit`]) runs it to find the tail sites,
//! then to a fixpoint, then to admit tail calls and find the
//! scope-local bindings, then once more to emit. Every walk computes
//! everything; each run reads what it needs.
//!
//! State: a stack of **frames** (a body: a `defun`'s, a method's, a
//! `fn` literal's, an `async` literal's, a `def`'s), each with the
//! modes its bindings read as, a stack of **scopes** (a `let`, a
//! `match`, a `loop`, a step) holding the owning sites that the scope
//! releases at its exit, and the loops it is inside. A site is owned by
//! the frame whose depth [`Walker::site_frame`] records.

mod call;
mod callee;
mod expr;
mod lit;
mod pattern;

use std::collections::{HashMap, HashSet};

use crate::types::ast::{BindingId, Expr, ExprId, FunId};
use crate::types::TypedProgram;

use super::facts::Facts;
use super::program::{
    BindKind, BindingOwn, BodyOwn, ExprOwn, Mode, Op, OpKind, Site, Summary, Tail, Why,
};

pub(super) use callee::SELF_ESCAPES_BY_DEFAULT;
pub(super) use lit::{BodySpec, ParamIn};

/// A value as the events see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Val {
    /// `Borrowed(site)`; an `Owned` temporary is `Borrowed(Value(e))`.
    B(Site),
    /// `Derived(site)`.
    D(Site),
    /// A `fn` literal itself.
    Lit(ExprId),
}

/// The class of a parameter position (§6.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Class {
    /// A borrowed position (or `weak`, `raw`: no count).
    Borrow,
    /// An owned position of a call: consumed, not an escape by itself.
    Owned,
    /// E2: stored (a constructor field, a storing primitive).
    Store,
    /// E4: `spawn`.
    Thread,
}

/// A position a value reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum At {
    /// E1 of the current frame's body.
    Return,
    /// E2.
    Store,
    /// E4.
    Thread,
    /// A join retain.
    Join,
    /// The operand of `raw-retained`.
    RawRetained,
    /// The operand of `weak`.
    Weak,
    /// Moved out by a scope exit.
    MoveOut,
    /// Captured by literal `lit`, heap or not.
    Capture { lit: ExprId, heap: bool },
    /// Consumed into loop variable `var` by its initialiser.
    LoopInit { var: BindingId },
    /// Consumed into loop variable `var` by a `recur`.
    Recur { var: BindingId },
    /// An argument of a call.
    Arg(ArgAt),
    /// The head of a call through a closure value.
    Head(HeadAt),
    /// The direct initialiser of the `let` binding.
    LetInit { binding: BindingId },
    /// Anything else (a read, a discarded step, a scrutinee, ...).
    Other,
}

/// An argument position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct ArgAt {
    pub call: ExprId,
    pub index: usize,
    pub class: Class,
    /// The parameter escapes (or the callee is a closure value).
    pub escapes: bool,
    pub closure_value: bool,
    /// The callee is a `defun` of the current SCC (then its `FunId`).
    pub scc_callee: Option<FunId>,
    pub tail_site: bool,
    pub admitted: bool,
    /// The argument is frame-owned (§6.4) in the current facts.
    pub frame_owned: bool,
}

/// A head position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct HeadAt {
    pub call: ExprId,
    pub tail_site: bool,
    pub admitted: bool,
    /// The self-name at its own self tail call.
    pub self_tail: bool,
}

/// One recorded position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Event {
    pub val: Val,
    pub at: At,
}

/// What a walk needs and does not change.
pub(super) struct Ctx<'a> {
    pub p: &'a TypedProgram,
    /// Summaries of the `defun`s of earlier units.
    pub summaries: &'a HashMap<FunId, Summary>,
    /// The unit's facts so far.
    pub facts: &'a Facts,
    /// The unit's `defun`s (empty for a method or a `def`).
    pub scc: &'a [FunId],
    /// Whether the bodies walked are all-owned bodies (§8.4).
    pub all_owned: bool,
    /// The tail sites, once known (`None` while finding them).
    pub tail_sites: Option<&'a HashMap<ExprId, Tail>>,
    /// Whether tail sites are admitted by rule (e) with these facts.
    pub admit: bool,
    /// The scope-local sites, once known.
    pub scope_local: &'a HashSet<Site>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameKind {
    Defun,
    Method,
    Fn,
    Async,
    Def,
}

/// What opened a scope: a step (its temporaries) or a binding form
/// (`let`, `match`, `loop`). Both release their sites at their exit;
/// the kind documents the walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Step,
    Bind,
}

struct Scope {
    /// Owning sites in creation order, each with whether it is a temporary.
    sites: Vec<(Site, bool)>,
}

struct LoopCtx {
    /// The index of the loop's scope (its variables) in the frame.
    scope: usize,
    vars: Vec<BindingId>,
}

struct Frame {
    kind: FrameKind,
    env: HashMap<BindingId, Mode>,
    scopes: Vec<Scope>,
    loops: Vec<LoopCtx>,
    /// The owned parameters (and `env`) released at the exits, in order.
    owned_params: Vec<Site>,
    /// The `&` parameters (a `defun` frame only).
    amp_params: Vec<BindingId>,
    /// A named `fn`'s self-name.
    self_name: Option<BindingId>,
}

/// A walk in progress.
pub(super) struct Walker<'a> {
    pub cx: Ctx<'a>,
    frames: Vec<Frame>,
    /// The frame depth that owns each owning site.
    site_frame: HashMap<Site, usize>,
    /// The tables being filled.
    pub out: BodyOwn,
    /// Every position reached.
    pub events: Vec<Event>,
    /// The decision at every call in tail position.
    pub tails: HashMap<ExprId, Tail>,
    /// Allocation sites that initialise an owning site: the §6.11 candidates.
    pub candidates: HashMap<Site, ExprId>,
}

impl<'a> Walker<'a> {
    /// A walker with nothing walked.
    pub fn new(cx: Ctx<'a>) -> Self {
        Walker {
            cx,
            frames: Vec::new(),
            site_frame: HashMap::new(),
            out: BodyOwn::default(),
            events: Vec::new(),
            tails: HashMap::new(),
            candidates: HashMap::new(),
        }
    }

    fn frame(&self) -> &Frame {
        // The walk pushes a frame before walking any expression.
        self.frames.last().expect("a frame is open while walking")
    }

    fn frame_mut(&mut self) -> &mut Frame {
        // As in `frame`.
        self.frames
            .last_mut()
            .expect("a frame is open while walking")
    }

    fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Whether `e` has an object type.
    fn is_object(&self, e: &Expr) -> bool {
        match self.cx.p.expr_types.get(&e.id) {
            Some(t) => super::objects::is_object(&self.cx.p.globals, t),
            None => true,
        }
    }

    /// Whether the binding `b` has an object type.
    fn binding_is_object(&self, b: BindingId) -> bool {
        match self.cx.p.binding_types.get(&b) {
            Some(t) => super::objects::is_object(&self.cx.p.globals, t),
            None => true,
        }
    }

    fn note(&mut self, val: Option<Val>, at: At) {
        if let Some(val) = val {
            self.events.push(Event { val, at });
        }
    }

    fn set_mode(&mut self, e: ExprId, mode: Mode) {
        self.expr_own(e).mode = mode;
    }

    fn expr_own(&mut self, e: ExprId) -> &mut ExprOwn {
        self.out.exprs.entry(e).or_insert(ExprOwn {
            mode: Mode::Scalar,
            after: Vec::new(),
        })
    }

    fn push_op(&mut self, e: ExprId, kind: OpKind, site: Site, why: Why) {
        self.add_after(e, Op { kind, site, why });
    }

    /// Appends to `e`'s `after`, unless `e` jumps (an admitted tail
    /// call or a `recur`), after which nothing of the frame runs.
    fn add_after(&mut self, e: ExprId, op: Op) {
        let jumps = self.out.recurs.contains_key(&e)
            || self
                .out
                .calls
                .get(&e)
                .is_some_and(|c| c.tail == Tail::TailCall);
        if !jumps {
            self.expr_own(e).after.push(op);
        }
    }

    /// The operation that ends `site` at a scope exit or a jump.
    fn end_op(&self, site: Site, why: Why) -> Op {
        let kind = if self.cx.scope_local.contains(&site) {
            OpKind::EndStack
        } else {
            OpKind::Release
        };
        Op { kind, site, why }
    }

    fn bind(&mut self, b: BindingId, mode: Mode, kind: BindKind) {
        self.frame_mut().env.insert(b, mode);
        self.out.bindings.insert(
            b,
            BindingOwn {
                kind,
                scope_local: self.cx.scope_local.contains(&Site::Bind(b)),
            },
        );
    }

    /// The mode a read of `b` has in the current frame.
    fn read(&self, b: BindingId) -> Mode {
        self.frame()
            .env
            .get(&b)
            .copied()
            .unwrap_or(Mode::Borrowed(Site::Bind(b)))
    }

    fn push_scope(&mut self, _kind: ScopeKind) {
        self.frame_mut().scopes.push(Scope { sites: Vec::new() });
    }

    fn pop_scope(&mut self) -> Scope {
        // Every push is paired with a pop in the same function.
        self.frame_mut().scopes.pop().expect("scope pushed")
    }

    /// Adds an owning site to the innermost scope of the frame.
    fn own_site(&mut self, site: Site, temp: bool) {
        let d = self.depth();
        self.site_frame.insert(site, d);
        if let Some(s) = self.frame_mut().scopes.last_mut() {
            s.sites.push((site, temp));
        }
    }

    /// Whether `m` is frame-owned in the current frame (§6.4).
    fn frame_owned(&self, m: Mode) -> bool {
        match m {
            Mode::Owned { immortal } => !immortal,
            Mode::Borrowed(s) | Mode::Derived(s) => self.site_frame.get(&s) == Some(&self.depth()),
            Mode::Scalar => false,
        }
    }

    /// Whether `s` is an owning binding of the frame that a tail call
    /// may move (a scope's site or an owned parameter; not a capture).
    fn movable(&self, s: Site) -> bool {
        let f = self.frame();
        !matches!(s, Site::Capture(..) | Site::Global(_))
            && self.site_frame.get(&s) == Some(&self.depth())
            && (f.owned_params.contains(&s)
                || f.scopes.iter().any(|sc| sc.sites.iter().any(|x| x.0 == s)))
    }

    /// The scope exit of `scope` for a value `m` produced by `e`
    /// (§6.3): ops appended to `e`'s `after`; the adjusted mode.
    fn exit_scope(&mut self, scope: Scope, e: ExprId, m: Mode) -> Mode {
        self.exit_scope_copy(&scope, e, m, true)
    }

    /// As [`Walker::exit_scope`], for one of several paths out of the
    /// same scope (the clauses of a `match`); `note` on the first only.
    fn exit_scope_copy(&mut self, scope: &Scope, e: ExprId, m: Mode, note: bool) -> Mode {
        let owns = |s: &Site| scope.sites.iter().any(|x| x.0 == *s);
        let mut moved = None;
        let mut out = m;
        match m {
            Mode::Borrowed(s) if owns(&s) => {
                moved = Some(s);
                out = Mode::Owned { immortal: false };
                if note {
                    self.note(Some(Val::B(s)), At::MoveOut);
                }
            }
            Mode::Derived(s) if owns(&s) => {
                self.push_op(e, OpKind::Retain, Site::Value(e), Why::DerivedExit);
                out = Mode::Owned { immortal: false };
            }
            _ => {}
        }
        for (s, temp) in scope.sites.iter().rev() {
            if Some(*s) != moved {
                let op = self.end_op(*s, if *temp { Why::StepEnd } else { Why::ScopeExit });
                self.add_after(e, op);
            }
        }
        out
    }

    /// The releases of the scopes `from..` of the frame, bindings
    /// first (innermost scope first, reverse order), then temporaries,
    /// skipping `moved` (§6.10 step 3).
    fn release_scopes(&self, from: usize, moved: &HashSet<Site>, why: Why) -> Vec<Op> {
        let f = self.frame();
        let mut ops = Vec::new();
        for temps in [false, true] {
            for sc in f.scopes[from..].iter().rev() {
                for (s, t) in sc.sites.iter().rev() {
                    if *t == temps && !moved.contains(s) {
                        ops.push(self.end_op(*s, why));
                    }
                }
            }
        }
        ops
    }

    /// The owned parameters of the frame not in `moved`.
    fn release_params(&self, moved: &HashSet<Site>, keep_env: bool, why: Why) -> Vec<Op> {
        self.frame()
            .owned_params
            .iter()
            .filter(|s| !(moved.contains(s) || keep_env && matches!(s, Site::Env(_))))
            .map(|s| Op {
                kind: OpKind::Release,
                site: *s,
                why,
            })
            .collect()
    }

    /// The mode of the kind of a new owning binding.
    fn owning(b: BindingId) -> Mode {
        Mode::Borrowed(Site::Bind(b))
    }

    /// The `defun` summary of `f` in the current facts or earlier units.
    fn summary_of(&self, f: FunId) -> Option<Summary> {
        if self.cx.scc.contains(&f) {
            return Some(self.cx.facts.summary(self.cx.p, f));
        }
        self.cx.summaries.get(&f).cloned()
    }
}

/// The body-level kinds: whether a binding id is an object parameter.
pub(super) fn kind_of_param(owned: bool) -> BindKind {
    if owned {
        BindKind::OwnedParam
    } else {
        BindKind::BorrowedParam
    }
}
