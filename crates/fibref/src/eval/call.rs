//! Calls (syntax §2, types §6.3, §6.6, §6.10, §8.9): the head and the
//! arguments left to right, each handed over as its [`Pass`] says right
//! after it is evaluated; then either an ordinary call (the callee runs,
//! then the write-backs, then the call's `after`) or a tail call (the
//! jump runs, the frame is discarded, the callee's result is the
//! caller's). [`Interp::invoke`] is the trampoline that makes a chain of
//! tail calls run in constant Rust stack.

use crate::heap::{Kind, ObjId};
use crate::own::program::{Alloc, CallOwn, Callee, Pass, Tail};
use crate::syntax::Pos;
use crate::types::ast::{Arg, BindingId, BuiltinId, Expr, ExprId, ExprKind, ExternId, GlobalRef};
use crate::types::ty::TypeId;

use super::alloc::Placement;
use super::error::{RunError, R};
use super::expr::Flow;
use super::interp::Interp;
use super::object::Obj;
use super::value::Val;

/// What a call enters.
#[derive(Clone, Debug)]
pub enum Target {
    /// A body with a plan: a `defun`, its all-owned body, an `impl`
    /// method.
    Body(crate::own::program::BodyKey),
    /// A `fn` closure object.
    Lambda(ObjId),
    /// A builtin; `owned`: reached through a function value, so every
    /// object argument is the callee's (§8.4).
    Builtin(BuiltinId, bool),
    /// A method of a built-in instance (instance index, method index);
    /// `owned`: reached through a method value, so every object
    /// argument is the callee's (§8.4).
    Native(usize, usize, bool),
    /// A constructor.
    Ctor(TypeId, Option<usize>),
    /// An `extern`.
    Extern(ExternId),
}

/// A call about to be entered: its target, its arguments, and the call
/// site (for traps and the placement of what it allocates).
#[derive(Clone, Debug)]
pub struct Jump {
    /// What is called.
    pub target: Target,
    /// The argument values (an `&` position: the private cell).
    pub args: Vec<Val>,
    /// Where the call is.
    pub pos: Pos,
    /// Where the object the call allocates lives, as the plan decided.
    pub placement: Placement,
}

impl<'p> Interp<'p> {
    /// A call expression.
    pub fn call(&mut self, e: &'p Expr, head: &'p Expr, args: &'p [Arg]) -> R<Flow> {
        let own: &'p CallOwn = self
            .plan()?
            .calls
            .get(&e.id)
            .copied()
            .ok_or_else(|| RunError::gap("no plan for a call"))?;
        let head_val = match own.callee {
            Callee::Value => Some(self.pass_head(head, own.head)?),
            _ => None,
        };
        let mut vals = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            let pass = *own
                .args
                .get(i)
                .ok_or_else(|| RunError::gap("no pass for an argument"))?;
            vals.push(self.argument(a, pass)?);
        }
        let target = self.target(head, own.callee, head_val.as_ref(), &vals)?;
        let placement = Placement::of(self.plan()?.allocs.get(&e.id).copied());
        let jump = Jump {
            target,
            args: vals,
            pos: e.pos.clone(),
            placement,
        };
        if own.tail == Tail::TailCall {
            self.run_ops(&own.jump)?;
            return Ok(Flow::Tail(Box::new(jump)));
        }
        Ok(Flow::Val(self.ordinary(own, jump)?))
    }

    /// An ordinary call: the callee runs, then the write-backs (§6.6).
    fn ordinary(&mut self, own: &CallOwn, jump: Jump) -> R<Val> {
        if !matches!(jump.target, Target::Body(_) | Target::Lambda(_)) {
            return self.native_call(&jump.target, &jump.args, &jump.pos, jump.placement);
        }
        let cells: Vec<Val> = own
            .write_backs
            .iter()
            .map(|(i, _)| jump.args.get(*i).cloned().unwrap_or(Val::Unit))
            .collect();
        let result = self.invoke(jump)?;
        for ((_, var), cell) in own.write_backs.iter().zip(cells) {
            self.write_back(*var, &cell)?;
        }
        Ok(result)
    }

    fn pass_head(&mut self, head: &'p Expr, pass: Pass) -> R<Val> {
        let v = self.val(head)?;
        if pass == Pass::Retain {
            self.retain(&v)?;
        }
        Ok(v)
    }

    /// One argument, handed over as `pass` says.
    fn argument(&mut self, a: &'p Arg, pass: Pass) -> R<Val> {
        match a {
            Arg::Expr(x) => {
                let v = self.val(x)?;
                if pass == Pass::Retain {
                    self.retain(&v)?;
                }
                Ok(v)
            }
            Arg::Amp(b, pos) => self.amp_argument(*b, pass).map_err(|e| e.at(pos)),
        }
    }

    /// `&b` (§6.6): a copy-in into a new private cell, the forwarded
    /// private cell, or (for a primitive) the variable's own cell.
    fn amp_argument(&mut self, b: BindingId, pass: Pass) -> R<Val> {
        let cell = self.local(b)?;
        match pass {
            Pass::Forward | Pass::OwnCell => Ok(cell),
            Pass::Acquire => {
                let content = self.slot(cell.expect_obj("an & argument")?)?;
                // The stack cell's store of the content is the acquire (+1).
                self.new_slot(Kind::Cell, content, Placement::Stack)
            }
            other => Err(RunError::gap(format!("an & argument passed as {other:?}"))),
        }
    }

    /// The write-back of the private cell `cell` into the variable `var`
    /// (§6.6): `var`'s content := the private cell's (its old content
    /// released), then the private cell ends.
    fn write_back(&mut self, var: BindingId, cell: &Val) -> R<()> {
        let private = cell.expect_obj("a private cell")?;
        let content = self.slot(private)?;
        let target = self.local(var)?.expect_obj("an & variable")?;
        self.write_slot(target, content)?;
        self.end_private(private)
    }

    /// What `head` calls.
    fn target(&mut self, head: &Expr, c: Callee, hv: Option<&Val>, args: &[Val]) -> R<Target> {
        use crate::own::program::BodyKey;
        Ok(match (c, &head.kind) {
            (Callee::Fun(f), _) => Target::Body(BodyKey::Fun(f)),
            (Callee::AllOwned(f), _) => Target::Body(BodyKey::AllOwned(f)),
            (Callee::Method(p, i), _) => {
                let recv = args
                    .first()
                    .ok_or_else(|| RunError::internal("a method call without a receiver"))?;
                self.dispatch(Some(head.id), p, i, recv)?
            }
            (Callee::Builtin(b), _) => Target::Builtin(b, false),
            (Callee::Ctor, ExprKind::Global(GlobalRef::Ctor(t, v))) => Target::Ctor(*t, *v),
            (Callee::Extern, ExprKind::Global(GlobalRef::Extern(x))) => Target::Extern(*x),
            (Callee::Value, _) => {
                let v = hv.ok_or_else(|| RunError::internal("no head value"))?;
                self.value_target(v, args)?
            }
            (c, _) => {
                return Err(RunError::internal(format!(
                    "callee {c:?} with head {head:?}"
                )))
            }
        })
    }

    /// The target of a call through the function value `v` with the
    /// arguments `args` (§8.4).
    pub fn value_target(&mut self, v: &Val, args: &[Val]) -> R<Target> {
        use crate::own::program::BodyKey;
        let id = v.expect_obj("a function value")?;
        let clo = match self.objs.get(&self.heap, id)? {
            Obj::Closure(c) => c.clone(),
            o => return Err(RunError::internal(format!("calling a non-closure {o:?}"))),
        };
        Ok(match clo {
            super::object::Clo::Lambda { .. } => Target::Lambda(id),
            super::object::Clo::Fun(f) => Target::Body(BodyKey::AllOwned(f)),
            super::object::Clo::Builtin(b) => Target::Builtin(b, true),
            super::object::Clo::Ctor(t, i) => Target::Ctor(t, i),
            super::object::Clo::Impl(inst, i) => self.method_value_target(inst, i)?,
            super::object::Clo::Method(p, i) => {
                let recv = args.first().ok_or_else(|| {
                    RunError::internal("a method value called without a receiver")
                })?;
                let inst = self.instance_of(p, recv)?;
                self.method_value_target(inst, i)?
            }
        })
    }

    /// Runs a call and every tail call it makes, in constant Rust stack.
    pub fn invoke(&mut self, mut jump: Jump) -> R<Val> {
        loop {
            let pos = jump.pos.clone();
            let flow = match jump.target {
                Target::Body(key) => self.run_body(key, jump.args),
                Target::Lambda(clo) => self.run_lambda(clo, jump.args),
                _ => {
                    return self
                        .native_call(&jump.target, &jump.args, &pos, jump.placement)
                        .map_err(|e| e.at(&pos))
                }
            };
            match flow.map_err(|e| e.at(&pos))? {
                Flow::Val(v) => return Ok(v),
                Flow::Tail(next) => jump = *next,
                Flow::Recur(_) => return Err(RunError::internal("recur outside its loop")),
            }
        }
    }

    /// Enters a body with a plan: binds the parameters and evaluates the
    /// body in a new frame.
    fn run_body(&mut self, key: crate::own::program::BodyKey, args: Vec<Val>) -> R<Flow> {
        use crate::own::program::BodyKey;
        let plan = self.body(key)?;
        let g = &self.p.globals;
        let (params, body): (Vec<BindingId>, &'p Expr) = match key {
            BodyKey::Fun(f) | BodyKey::AllOwned(f) => {
                let d = g.fun(f);
                (d.params.iter().map(|p| p.binding).collect(), &d.body)
            }
            BodyKey::Method(i, m) | BodyKey::MethodOwned(i, m) => {
                let im = &g.instances[i].methods[m];
                (im.params.clone(), &im.body)
            }
            BodyKey::Def(d) => (Vec::new(), &g.def(d).init),
        };
        let mut frame = super::interp::Frame::new(plan, key);
        for (b, v) in params.into_iter().zip(args) {
            frame.locals.insert(b, v);
        }
        self.run_frame(frame, body)
    }

    /// Evaluates `body` in `frame`, popping the frame afterwards.
    pub fn run_frame(&mut self, frame: super::interp::Frame<'p>, body: &'p Expr) -> R<Flow> {
        self.frames.push(frame);
        let r = self.flow(body);
        self.frames.pop();
        r
    }

    /// The placement of what the call expression `e` allocates.
    pub fn placement_of(&self, e: ExprId) -> R<Placement> {
        let alloc: Option<Alloc> = self.plan()?.allocs.get(&e).copied();
        Ok(Placement::of(alloc))
    }
}
