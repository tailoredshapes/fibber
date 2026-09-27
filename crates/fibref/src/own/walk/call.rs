//! Calls and `recur` (types §6.3 argument rows, §6.6 copy-in and
//! forwarding, §6.10 tail calls, admission and the jump).

use std::collections::HashSet;

use crate::types::ast::{Arg, Expr, ExprKind, GlobalRef};

use super::super::program::{
    Alloc, Because, CallOwn, Callee, Mode, OpKind, Pass, RecurOwn, Site, Tail, Why,
};
use super::callee::{allocates, builtin_stores, Param};
use super::{ArgAt, At, Class, FrameKind, HeadAt, ScopeKind, Val, Walker};

/// What the argument rules need to know about the call.
struct CallSite {
    call: crate::types::ast::ExprId,
    callee: Callee,
    /// A tail site (§6.10).
    site: bool,
    /// An admitted tail call.
    admitted: bool,
    /// In tail position.
    tail: bool,
}

impl Walker<'_> {
    /// A call (§6.2: its result is `Owned`).
    pub(super) fn call(&mut self, e: &Expr, head: &Expr, args: &[Arg], tail: bool) -> Mode {
        let callee = self.callee_of(head);
        self.push_scope(ScopeKind::Step);
        let head_m = (callee == Callee::Value).then(|| self.expr(head, false));
        let ms: Vec<Option<Mode>> = args
            .iter()
            .map(|a| match a {
                Arg::Expr(x) => Some(self.expr(x, false)),
                Arg::Amp(..) => None,
            })
            .collect();
        let params = self.params_of(callee, args.len());
        let tail_d = self.decide(e, callee, &params, args, &ms, tail);
        if tail {
            self.tails.insert(e.id, tail_d);
        }
        let cs = CallSite {
            call: e.id,
            callee,
            site: self.is_tail_site(e, tail_d),
            admitted: tail_d == Tail::TailCall,
            tail,
        };
        let mut moved = HashSet::new();
        let mut own = CallOwn {
            callee,
            tail: tail_d,
            head: self.head_pass(&cs, head, head_m, &mut moved),
            args: Vec::new(),
            write_backs: Vec::new(),
            jump: Vec::new(),
        };
        for (i, a) in args.iter().enumerate() {
            let pass = self.arg_pass(&cs, i, a, ms[i], params[i], &mut moved);
            if let (Pass::Acquire, Arg::Amp(b, _)) = (pass, a) {
                own.write_backs.push((i, *b));
            }
            own.args.push(pass);
        }
        self.finish_call(e, &cs, own, &moved);
        self.record_alloc(e, head);
        Mode::Owned { immortal: false }
    }

    /// The end of a call: the jump of a tail call (§6.10 step 3), or the
    /// release of the step's temporaries after an ordinary one.
    fn finish_call(&mut self, e: &Expr, cs: &CallSite, mut own: CallOwn, moved: &HashSet<Site>) {
        if cs.admitted {
            let keep_env = own.head == Pass::KeepEnv;
            let mut ops = self.release_scopes(0, moved, Why::Jump);
            ops.extend(self.release_params(moved, keep_env, Why::Jump));
            own.jump = ops;
            self.pop_scope();
        } else {
            let scope = self.pop_scope();
            self.exit_scope(scope, e.id, Mode::Owned { immortal: false });
        }
        self.out.calls.insert(e.id, own);
    }

    /// Records an allocation site as a heap one; the driver turns those
    /// that §6.11 finds scope-local into stack ones.
    fn record_alloc(&mut self, e: &Expr, head: &Expr) {
        let alloc = match head.kind {
            ExprKind::Global(GlobalRef::Ctor(t, _)) if t == self.cx.p.globals.option => {
                Alloc::Nothing
            }
            _ if allocates(self, e) => Alloc::Heap,
            _ => return,
        };
        self.out.allocs.insert(e.id, alloc);
    }

    /// Whether the call is a tail site (§6.10): in the first walk, one
    /// the rules admit with the initial facts; later, one that walk found.
    fn is_tail_site(&self, e: &Expr, d: Tail) -> bool {
        match self.cx.tail_sites {
            None => d == Tail::TailCall,
            Some(ts) => ts.get(&e.id) == Some(&Tail::TailCall),
        }
    }

    /// The decision of §6.10 for a call; `tail` is tail position.
    fn decide(
        &self,
        e: &Expr,
        c: Callee,
        ps: &[Param],
        args: &[Arg],
        ms: &[Option<Mode>],
        tail: bool,
    ) -> Tail {
        if !tail {
            return Tail::NotInTail;
        }
        match c {
            Callee::Ctor => return Tail::Ordinary(Because::Store),
            Callee::Builtin(b) if builtin_stores(b.0 as usize) => {
                return Tail::Ordinary(Because::Store)
            }
            Callee::Extern => return Tail::Ordinary(Because::Extern),
            _ => {}
        }
        if matches!(self.frame().kind, FrameKind::Async | FrameKind::Def) {
            return Tail::Ordinary(Because::AsyncBody);
        }
        let forwards = |a: &Arg| match a {
            Arg::Amp(b, _) => self.forwards(*b),
            Arg::Expr(_) => true,
        };
        if !args.iter().all(forwards) {
            return Tail::Ordinary(Because::AmpArgument);
        }
        if let Some(ts) = self.cx.tail_sites {
            match ts.get(&e.id) {
                Some(Tail::TailCall) if self.cx.admit => {}
                Some(d) => return *d,
                None => return Tail::NotInTail,
            }
        }
        self.rule_e(c, ps, ms)
    }

    /// Rule (e): a frame-owned argument at a borrowed position of a
    /// callee outside the current SCC makes the call ordinary.
    fn rule_e(&self, c: Callee, ps: &[Param], ms: &[Option<Mode>]) -> Tail {
        if c == Callee::Value || self.scc_callee(c).is_some() {
            return Tail::TailCall;
        }
        for (i, (p, m)) in ps.iter().zip(ms).enumerate() {
            if let Some(m) = m {
                if p.class == Class::Borrow && !p.amp && self.frame_owned(*m) {
                    return Tail::Ordinary(Because::FrameOwned { arg: i });
                }
            }
        }
        Tail::TailCall
    }

    /// Whether `&b` at a call in tail position forwards the private cell
    /// of an `&` parameter of the enclosing `defun` (§6.6).
    fn forwards(&self, b: crate::types::ast::BindingId) -> bool {
        let f = self.frame();
        f.kind == FrameKind::Defun && f.amp_params.contains(&b)
    }

    /// `consume` at an owned position (§6.3; at a tail call, step 2).
    fn consume(&self, m: Mode, admitted: bool, moved: &mut HashSet<Site>) -> Pass {
        match m {
            Mode::Scalar => Pass::Scalar,
            Mode::Owned { .. } => Pass::Move,
            Mode::Borrowed(s) if admitted && self.movable(s) && !moved.contains(&s) => {
                moved.insert(s);
                Pass::Move
            }
            Mode::Borrowed(_) | Mode::Derived(_) => Pass::Retain,
        }
    }

    fn head_pass(
        &mut self,
        cs: &CallSite,
        head: &Expr,
        m: Option<Mode>,
        moved: &mut HashSet<Site>,
    ) -> Pass {
        let Some(m) = m else { return Pass::Scalar };
        let (site, admitted) = (cs.site, cs.admitted);
        let self_name = self.frame().self_name;
        let self_tail = matches!(head.kind, ExprKind::Local(g) if Some(g) == self_name) && site;
        let at = At::Head(HeadAt {
            call: cs.call,
            tail_site: site,
            admitted,
            self_tail,
        });
        let val = self.val(head, m);
        self.note(val, at);
        if self_tail && admitted {
            return Pass::KeepEnv;
        }
        let pass = self.consume(m, admitted, moved);
        self.stack_literal_temp(head, pass);
        pass
    }

    /// A stack closure literal handed over at an owned position still
    /// ends at the end of the call's step (§6.11).
    fn stack_literal_temp(&mut self, x: &Expr, pass: Pass) {
        if matches!(x.kind, ExprKind::Fn(_))
            && pass == Pass::Move
            && !self.cx.facts.heap.contains_key(&x.id)
        {
            self.temp(x);
        }
    }

    /// How argument `i` is handed over (§6.3, §6.10 step 2), with the
    /// position it reaches recorded.
    fn arg_pass(
        &mut self,
        cs: &CallSite,
        i: usize,
        a: &Arg,
        m: Option<Mode>,
        p: Param,
        moved: &mut HashSet<Site>,
    ) -> Pass {
        let (x, m) = match (a, m) {
            (Arg::Expr(x), Some(m)) => (x, m),
            (Arg::Amp(b, _), _) => return self.amp_pass(*b, cs.callee, cs.tail),
            (Arg::Expr(_), None) => return Pass::Scalar,
        };
        if m == Mode::Scalar {
            return Pass::Scalar;
        }
        let (pass, val) = if p.class == Class::Borrow {
            let val = if matches!(m, Mode::Owned { .. }) {
                self.temp(x);
                Some(self.val(x, m).unwrap_or(Val::B(Site::Value(x.id))))
            } else {
                self.val(x, m)
            };
            (Pass::Borrow, val)
        } else {
            let pass = self.consume(m, cs.admitted, moved);
            self.stack_literal_temp(x, pass);
            (pass, self.val(x, m))
        };
        let at = self.arg_at(cs, i, p, m);
        self.note(val, at);
        if p.weak {
            self.note(val, At::Weak);
        }
        pass
    }

    /// The position argument `i` of mode `m` reaches.
    fn arg_at(&self, cs: &CallSite, i: usize, p: Param, m: Mode) -> At {
        match (p.class, p.raw_retained) {
            (_, true) => At::RawRetained,
            (Class::Store, _) => At::Store,
            (Class::Thread, _) => At::Thread,
            _ => At::Arg(ArgAt {
                call: cs.call,
                index: i,
                class: p.class,
                escapes: p.escapes,
                closure_value: cs.callee == Callee::Value,
                scc_callee: self.scc_callee(cs.callee),
                tail_site: cs.site,
                admitted: cs.admitted,
                frame_owned: self.frame_owned(m),
            }),
        }
    }

    /// `&b` handed to a callee (§6.6).
    fn amp_pass(&mut self, b: crate::types::ast::BindingId, c: Callee, tail: bool) -> Pass {
        if !super::super::syntactic::is_amp(&self.cx.p.globals, b) {
            self.note(Some(Val::B(Site::Bind(b))), At::Other);
        }
        if matches!(c, Callee::Builtin(_)) {
            Pass::OwnCell
        } else if tail && self.forwards(b) {
            Pass::Forward
        } else {
            Pass::Acquire
        }
    }

    /// `(recur ..)`: a tail call to the loop (§6.10).
    pub(super) fn recur(&mut self, args: &[Expr]) -> RecurOwn {
        // Lowering rejects a `recur` outside a loop (`recur outside loop`).
        let (scope, vars) = match self.frame().loops.last() {
            Some(l) => (l.scope, l.vars.clone()),
            None => {
                return RecurOwn {
                    args: Vec::new(),
                    jump: Vec::new(),
                }
            }
        };
        self.push_scope(ScopeKind::Step);
        let ms: Vec<Mode> = args.iter().map(|x| self.expr(x, false)).collect();
        let mut moved = HashSet::new();
        let mut passes = Vec::new();
        for (i, (x, m)) in args.iter().zip(&ms).enumerate() {
            let pass = match *m {
                Mode::Borrowed(s) if self.exited(scope, s) && !moved.contains(&s) => {
                    moved.insert(s);
                    Pass::Move
                }
                Mode::Borrowed(s) if self.exited(scope, s) => Pass::Retain,
                m => self.consume(m, false, &mut moved),
            };
            let val = self.val(x, *m);
            if let Some(v) = vars.get(i) {
                self.note(val, At::Recur { var: *v });
            }
            passes.push(pass);
        }
        let mut jump = self.release_scopes(scope + 1, &moved, Why::Jump);
        for v in vars.iter().rev() {
            let s = Site::Bind(*v);
            if self.binding_is_object(*v) && !moved.contains(&s) {
                jump.push(super::super::program::Op {
                    kind: OpKind::Release,
                    site: s,
                    why: Why::RecurOld,
                });
            }
        }
        self.pop_scope();
        RecurOwn { args: passes, jump }
    }

    /// Whether `s` is a binding that a `recur` of the loop whose scope is
    /// `scope` exits: one of the loop's variables or of a scope inside it.
    fn exited(&self, scope: usize, s: Site) -> bool {
        self.frame().scopes[scope..]
            .iter()
            .any(|sc| sc.sites.iter().any(|x| x.0 == s))
    }
}
